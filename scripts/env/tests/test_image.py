"""image.sh: the docker build line it composes, the tags it gives, and the pushes it refuses.

The docker CLI is a fake on PATH that records its argv and answers `image inspect` from a map,
so nothing here builds, pulls or pushes.
"""
import hashlib
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
SCRIPT = REPO / "scripts" / "env" / "image.sh"
REGISTRY = "ghcr.io/asymptote-tech"
SUBMODULE_SHA = "43505bb975f46ce4e140fdaf55192611d6e231ac"

FAKE_DOCKER = r'''#!/usr/bin/env python3
import json, os, sys
log = os.environ["FAKE_DOCKER_LOG"]
with open(log, "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
if "FAKE_DOCKER_ENV_LOG" in os.environ:
    with open(os.environ["FAKE_DOCKER_ENV_LOG"], "a") as f:
        f.write(json.dumps({k: v for k, v in os.environ.items() if k.startswith("AWS_")}) + "\n")
images = json.loads(os.environ.get("FAKE_DOCKER_IMAGES", "{}"))
a = sys.argv[1:]
if a[:2] == ["image", "inspect"]:
    tag = a[-1]
    if tag not in images:
        print(f"Error: No such image: {tag}", file=sys.stderr); sys.exit(1)
    print(images[tag]); sys.exit(0)
sys.exit(0)
'''


def read_pins():
    pairs = (line.partition("=") for line in (REPO / "docker" / "pins.env").read_text().splitlines()
             if line and not line.startswith("#"))
    return {name: value for name, _, value in pairs}


class Run:
    def __init__(self, proc, calls):
        self.returncode, self.stdout, self.stderr, self.calls = proc.returncode, proc.stdout, proc.stderr, calls

    def commands(self):
        """The dry run's printed commands, each split as the shell would split it."""
        return [shlex.split(line) for line in self.stdout.splitlines() if line.startswith("docker ")]


def run(*args, script=SCRIPT, images=None, env=None):
    """`env` replaces variables of the caller's environment; a None value removes one."""
    with tempfile.TemporaryDirectory() as tmp:
        bin_dir = pathlib.Path(tmp) / "bin"
        bin_dir.mkdir()
        docker = bin_dir / "docker"
        docker.write_text(FAKE_DOCKER)
        docker.chmod(0o755)
        log = pathlib.Path(tmp) / "docker.log"
        log.touch()
        env_log = pathlib.Path(tmp) / "docker-env.log"
        env_log.touch()
        full = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "FAKE_DOCKER_LOG": str(log),
                "FAKE_DOCKER_IMAGES": json.dumps(images or {}), "FAKE_DOCKER_ENV_LOG": str(env_log)}
        for name, value in (env or {}).items():
            if value is None:
                full.pop(name, None)
            else:
                full[name] = value
        proc = subprocess.run([str(script), *args], capture_output=True, text=True, env=full,
                              stdin=subprocess.DEVNULL, timeout=60)
        calls = [json.loads(line) for line in log.read_text().splitlines()]
        result = Run(proc, calls)
        result.docker_env = [json.loads(line) for line in env_log.read_text().splitlines()]
    return result


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout.strip()


class Repo:
    """A throwaway git repo holding image.sh and its inputs, so clean and dirty are the test's to choose."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name) / "repo"
        for rel in ["scripts/env/image.sh", "scripts/lib/image-inputs.sh", "scripts/lib/peacock-config.sh",
                    "scripts/lib/calibration-bucket.sh", "scripts/lib/s3.sh", "cpp/CMakeLists.txt"]:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.root / rel)
        shutil.copytree(REPO / "docker", self.root / "docker")
        (self.root / "third_party" / "cudf").mkdir(parents=True)  # an uninitialised submodule
        git(self.root, "init", "-q")
        git(self.root, "add", "-A")
        git(self.root, "update-index", "--add", "--cacheinfo", f"160000,{SUBMODULE_SHA},third_party/cudf")
        git(self.root, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "inputs")
        self.sha = git(self.root, "rev-parse", "--short=12", "HEAD")
        self.script = self.root / "scripts" / "env" / "image.sh"
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()


def build_line(result):
    lines = [c for c in result.commands() if c[:2] == ["docker", "build"]]
    assert len(lines) == 1, f"want one docker build line, got {result.stdout!r}"
    return lines[0]


def flag_values(line, flag):
    return [line[i + 1] for i, token in enumerate(line[:-1]) if token == flag]


def test_build_args():
    r = run("build", "build", "25.02", "--dry-run")
    assert r.returncode == 0, r.stderr
    line = build_line(r)
    args = flag_values(line, "--build-arg")
    assert "CUDF=25.02" in args, args
    missing = [f"{k}={v}" for k, v in read_pins().items() if f"{k}={v}" not in args]
    assert not missing, f"pins.env values not passed as build args: {missing}"
    assert f"CUDF_SUBMODULE={SUBMODULE_SHA}" in args, args
    assert f"{REGISTRY}/peacockdb-build:cudf-25.02" in flag_values(line, "-t"), line
    assert flag_values(line, "-f") == [str(REPO / "docker" / "build.Dockerfile")], line
    assert "org.opencontainers.image.version=cudf-25.02" in flag_values(line, "--label"), line
    assert line[-1] == str(REPO), line
    assert r.calls == [], f"a dry run called docker: {r.calls}"


def test_build_refuses_an_unknown_cudf():
    r = run("build", "build", "25.06", "--dry-run")
    assert r.returncode == 2, (r.returncode, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1 and "25.06" in r.stderr, r.stderr
    assert "25.02" in r.stderr and "25.10" in r.stderr and "26.02" in r.stderr, r.stderr
    assert r.stdout == "" and r.calls == []


def test_build_25_10():
    line = build_line(run("build", "build", "25.10", "--dry-run"))
    assert "CUDF=25.10" in flag_values(line, "--build-arg")
    assert f"{REGISTRY}/peacockdb-build:cudf-25.10" in flag_values(line, "-t")


def test_run_refuses_a_build_only_cudf():
    """25.10 has a spec and no driver minimum: a build image for CI, and no run image."""
    for verb in ("build", "push"):
        r = run(verb, "run", "25.10", "--dry-run")
        assert r.returncode == 2, (verb, r.returncode, r.stderr)
        assert len(r.stderr.strip().splitlines()) == 1 and "DRIVER_MINIMUM_25_10" in r.stderr, (verb, r.stderr)
        assert r.stdout == "" and r.calls == [], (verb, r.stdout, r.calls)


def test_build_26_02():
    line = build_line(run("build", "build", "26.02", "--dry-run"))
    assert "CUDF=26.02" in flag_values(line, "--build-arg")
    assert f"{REGISTRY}/peacockdb-build:cudf-26.02" in flag_values(line, "-t")


def inputs_tag(tree, kind, cudf):
    return subprocess.run(["bash", "-c", f'. "{tree}/scripts/lib/image-inputs.sh"; image_inputs_tag "{tree}" {kind} {cudf}'],
                          check=True, capture_output=True, text=True).stdout.strip()


def test_the_inputs_tag_is_the_hash_of_the_inputs():
    out = subprocess.run(["bash", "-c", f'. "{REPO}/scripts/lib/image-inputs.sh"; image_inputs "{REPO}" build 25.02'],
                         check=True, capture_output=True, text=True).stdout
    assert out.endswith("\n") and not out.endswith("\n\n"), out
    assert inputs_tag(REPO, "build", "25.02") == f"cudf-25.02-in-{hashlib.sha256(out.encode()).hexdigest()[:12]}"
    assert inputs_tag(REPO, "run", "26.02").startswith("cudf-26.02-in-")


def test_build_adds_the_inputs_tag():
    with Repo() as repo:
        name = f"{REGISTRY}/peacockdb-build"
        tag = inputs_tag(repo.root, "build", "25.02")
        assert re.fullmatch(r"cudf-25\.02-in-[0-9a-f]{12}", tag), tag
        clean = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert f"{name}:{tag}" in flag_values(clean, "-t"), clean
        (repo.root / "untracked.txt").write_text("x")
        dirty = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert f"{name}:{tag}" in flag_values(dirty, "-t"), dirty


def test_a_clean_tree_adds_the_commit_tag():
    with Repo() as repo:
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        name = f"{REGISTRY}/peacockdb-build"
        assert flag_values(line, "-t") == [
            f"{name}:cudf-25.02", f"{name}:{inputs_tag(repo.root, 'build', '25.02')}", f"{name}:cudf-25.02-{repo.sha}"
        ], line
        assert f"org.opencontainers.image.revision={repo.sha}" in flag_values(line, "--label"), line


def test_a_dirty_tree_gets_no_commit_tag():
    with Repo() as repo:
        tag = inputs_tag(repo.root, "build", "25.02")
        (repo.root / "untracked.txt").write_text("x")
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        name = f"{REGISTRY}/peacockdb-build"
        assert flag_values(line, "-t") == [f"{name}:cudf-25.02", f"{name}:{tag}"], line
        assert f"org.opencontainers.image.revision={repo.sha}-dirty" in flag_values(line, "--label"), line


def test_an_untracked_file_is_dirty_whatever_the_git_config_hides():
    with Repo() as repo:
        git(repo.root, "config", "status.showUntrackedFiles", "no")
        (repo.root / "untracked.txt").write_text("x")
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert not [t for t in flag_values(line, "-t") if t.endswith(repo.sha)], line


def test_build_runs_the_line_it_prints():
    with Repo() as repo:
        printed = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        r = run("build", "build", "25.02", script=repo.script)
        assert r.returncode == 0, r.stderr
        assert r.calls == [printed[1:]], r.calls


def test_a_tree_without_the_cudf_submodule_is_refused_by_name():
    with Repo() as repo:
        git(repo.root, "rm", "-q", "--cached", "third_party/cudf")
        r = run("build", "build", "25.02", "--dry-run", script=repo.script)
        assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (r.returncode, r.stderr)
        assert "third_party/cudf" in r.stderr and r.stdout == "", r


def built(repo, kind="build", cudf="25.02", **ids):
    """The local images a clean build of <repo> leaves: version, inputs and commit tags, one image unless
    `ids` says otherwise (keys: version, inputs, commit)."""
    name = f"{REGISTRY}/peacockdb-{kind}"
    tags = {"version": f"{name}:cudf-{cudf}", "inputs": f"{name}:{inputs_tag(repo.root, kind, cudf)}",
            "commit": f"{name}:cudf-{cudf}-{repo.sha}"}
    return tags, {tags[k]: ids.get(k, "sha256:aaa") for k in tags if ids.get(k, "sha256:aaa")}


def pushes(result):
    return [c[1:] for c in result.calls if c[0] == "push"]


def test_push_refuses_without_the_commit_tag():
    with Repo() as repo:
        r = run("push", "build", "25.02", script=repo.script)
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert f"peacockdb-build:cudf-25.02-{repo.sha}" in r.stderr and len(r.stderr.strip().splitlines()) == 1
        assert not [c for c in r.calls if c[0] == "push"], r.calls


def test_push_refuses_when_the_version_tag_moved():
    with Repo() as repo:
        name = f"{REGISTRY}/peacockdb-build:cudf-25.02"
        images = {name: "sha256:bbb", f"{name}-{repo.sha}": "sha256:aaa"}
        r = run("push", "build", "25.02", script=repo.script, images=images)
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert name in r.stderr and len(r.stderr.strip().splitlines()) == 1, r.stderr
        assert not [c for c in r.calls if c[0] == "push"], r.calls


def test_push_refuses_when_the_inputs_tag_is_missing_or_another_image():
    with Repo() as repo:
        for inputs_id in (None, "sha256:bbb"):
            tags, images = built(repo, inputs=inputs_id)
            r = run("push", "build", "25.02", script=repo.script, images=images)
            assert r.returncode == 2, (inputs_id, r.returncode, r.stderr)
            assert tags["inputs"] in r.stderr and len(r.stderr.strip().splitlines()) == 1, (inputs_id, r.stderr)
            assert pushes(r) == [], r.calls


def test_push_pushes_the_inputs_tag():
    with Repo() as repo:
        tags, images = built(repo)
        r = run("push", "build", "25.02", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert [tags["inputs"]] in pushes(r), r.calls


def test_push_leaves_the_moving_tag_without_moving():
    with Repo() as repo:
        tags, images = built(repo)
        r = run("push", "build", "25.02", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert pushes(r) == [[tags["inputs"]], [tags["commit"]]], r.calls
        r = run("push", "build", "25.02", "--moving", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert pushes(r) == [[tags["inputs"]], [tags["commit"]], [tags["version"]]], r.calls


def test_push_dry_run_pushes_nothing():
    with Repo() as repo:
        tags, images = built(repo)
        r = run("push", "build", "25.02", "--dry-run", "--moving", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert r.commands() == [["docker", "push", tags[k]] for k in ("inputs", "commit", "version")], r.stdout
        assert pushes(r) == [], r.calls


def test_run_build_args():
    for cudf in ["25.02", "26.02"]:
        r = run("build", "run", cudf, "--dry-run")
        assert r.returncode == 0, r.stderr
        line = build_line(r)
        args = flag_values(line, "--build-arg")
        assert f"CUDF={cudf}" in args, args
        missing = [f"{k}={v}" for k, v in read_pins().items() if f"{k}={v}" not in args]
        assert not missing, f"pins.env values not passed as build args: {missing}"
        assert f"{REGISTRY}/peacockdb-run:cudf-{cudf}" in flag_values(line, "-t"), line
        assert flag_values(line, "-f") == [str(REPO / "docker" / "run.Dockerfile")], line
        assert f"org.opencontainers.image.version=cudf-{cudf}" in flag_values(line, "--label"), line
        assert line[-1] == str(REPO), line
        assert r.calls == [], f"a dry run called docker: {r.calls}"


def test_run_push_pushes_the_run_tags():
    with Repo() as repo:
        tags, images = built(repo, kind="run", cudf="26.02")
        r = run("push", "run", "26.02", "--moving", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert pushes(r) == [[tags["inputs"]], [tags["commit"]], [tags["version"]]], r.calls


def test_bad_arguments_fail_in_one_line():
    for args in [(), ("build",), ("build", "build"), ("deploy", "build", "25.02"), ("build", "kit", "25.02"),
                 ("build", "build", "25.02", "--push"), ("build", "build", "25.02", "extra"),
                 ("build", "build", "25.02", "--moving")]:
        r = run(*args)
        assert r.returncode == 2, (args, r.returncode, r.stderr)
        assert len(r.stderr.strip().splitlines()) == 1 and r.stderr.startswith("image.sh: "), (args, r.stderr)
        assert r.calls == [], (args, r.calls)


def test_help():
    r = run("--help")
    assert r.returncode == 0 and "image.sh build" in r.stdout and "image.sh push" in r.stdout, r


# Pins of what is installed on the machine, not in the image (Task 6's bootstrap-vm.sh): Claude
# Code is proprietary and the images are public.
MACHINE_PINS = {"CLAUDE_CODE_VERSION", "SUPERPOWERS_VERSION", "SUPERPOWERS_SHA"}


def test_the_dockerfiles_declare_every_pin_but_the_machine_s():
    """image.sh passes every pin to both Dockerfiles and BuildKit is silent about an unused one, so
    a pin neither declares is a pin nothing reads."""
    declared = {kind: set(re.findall(r"^ARG (\w+)", (REPO / "docker" / f"{kind}.Dockerfile").read_text(), re.M))
                for kind in ["build", "run", "sirius"]}
    assert MACHINE_PINS <= set(read_pins()), "pins.env lost a machine pin"
    missing = sorted((set(read_pins()) - MACHINE_PINS | {"CUDF", "CUDF_SUBMODULE"}) - set().union(*declared.values()))
    assert not missing, f"no Dockerfile declares: {missing}"
    assert "CUDF" in declared["run"], "run.Dockerfile never declares CUDF"
    for kind, names in declared.items():
        assert not names & MACHINE_PINS, f"{kind}.Dockerfile declares machine pins: {names & MACHINE_PINS}"


def test_no_claude_code_is_installed_in_the_image():
    for kind in ["build", "run", "sirius"]:
        text = (REPO / "docker" / f"{kind}.Dockerfile").read_text()
        found = re.findall(r"claude\.ai/install\.sh|claude plugin|@anthropic-ai/claude-code", text)
        assert not found, f"{kind}.Dockerfile installs Claude Code: {found}"


def label(line):
    """The peacockdb.inputs label of a build line, as `{input: value}`."""
    [value] = [v for v in flag_values(line, "--label") if v.startswith("peacockdb.inputs=")]
    return dict(entry.split("=", 1) for entry in value[len("peacockdb.inputs="):].split(" "))


def image_inputs(tree, kind, cudf):
    out = subprocess.run(["bash", "-c", f'. "{tree}/scripts/lib/image-inputs.sh"; image_inputs "{tree}" {kind} {cudf}'],
                         check=True, capture_output=True, text=True).stdout
    return dict(line.split("=", 1) for line in out.splitlines())


def test_image_labels_its_inputs():
    with Repo() as repo:
        before = {kind: label(build_line(run("build", kind, "25.02", "--dry-run", script=repo.script)))
                  for kind in ("build", "run")}
        for kind in ("build", "run"):
            assert before[kind] == image_inputs(repo.root, kind, "25.02"), kind
        assert before["build"]["third_party/cudf"] == SUBMODULE_SHA, before["build"]
        assert before["build"]["cpp/CMakeLists.txt:flatbuffers"] == "v25.12.19", before["build"]
        assert "docker/venv.lock" in before["build"] and "docker/venv.lock" not in before["run"]
        (repo.root / "docker" / "venv.lock").write_text("changed==1\n")
        after = {kind: label(build_line(run("build", kind, "25.02", "--dry-run", script=repo.script)))
                 for kind in ("build", "run")}
        changed = sorted(k for k in before["build"] if before["build"][k] != after["build"][k])
        assert changed == ["docker/venv.lock"], changed
        assert after["run"] == before["run"]


def files_read(kind, cudf):
    """The docker/ files <kind>.Dockerfile reads for <cudf>: each file it copies, and each file under a
    copied directory that its instructions name by the path it was copied to."""
    text = (REPO / "docker" / f"{kind}.Dockerfile").read_text()
    files = set()
    for src, dst in re.findall(r"^COPY (?!--from)(docker/\S*) (\S+)", text, re.M):
        if not src.endswith("/"):
            files.add(src)
            continue
        for rest in re.findall(re.escape(dst) + r"([^\s\"';)/]+)", text):
            files.add(src + rest.replace("${CUDF}", cudf))
    return files


def test_the_label_holds_every_docker_file_its_dockerfile_reads():
    for kind, cudf in IMAGES:
        read = files_read(kind, cudf)
        assert f"docker/conda/cudf-{cudf}.spec" in read, (kind, cudf, read)
        missing = sorted(read - set(image_inputs(REPO, kind, cudf)))
        assert not missing, f"the {kind} image's {cudf} label misses {missing}"
    for cudf in ("25.02", "25.10"):
        assert "docker/conda/cudf-26.02.spec" in files_read("build", cudf), f"{cudf}: the nvcc spec is no longer read"


# Every image there is: 25.10 is build-only.
IMAGES = [("build", "25.02"), ("build", "25.10"), ("build", "26.02"), ("run", "25.02"), ("run", "26.02")]


def every_label(repo):
    return {image: label(build_line(run("build", *image, "--dry-run", script=repo.script))) for image in IMAGES}


def bump(repo, name):
    pins = repo.root / "docker" / "pins.env"
    pins.write_text("".join(line + ("9" if line.startswith(f"{name}=") else "") + "\n"
                            for line in pins.read_text().splitlines()))


def labels(repo):
    return {kind: label(build_line(run("build", kind, "25.02", "--dry-run", script=repo.script)))
            for kind in ("build", "run")}


def test_a_pin_no_dockerfile_reads_changes_no_label():
    with Repo() as repo:
        before = labels(repo)
        for name in ("CLAUDE_CODE_VERSION", "SUPERPOWERS_VERSION", "SUPERPOWERS_SHA"):
            bump(repo, name)
        assert labels(repo) == before


def test_a_pin_changes_the_label_of_the_image_that_reads_it():
    for name, changed in (("GH_VERSION", "build"), ("DRIVER_MINIMUM_25_02", "run"), ("MICROMAMBA_VERSION", "both")):
        with Repo() as repo:
            before = labels(repo)
            bump(repo, name)
            after = labels(repo)
            for kind in ("build", "run"):
                moved = sorted(k for k in before[kind] if before[kind][k] != after[kind][k])
                want = ["docker/pins.env"] if changed in (kind, "both") else []
                assert moved == want, (name, kind, moved)


def sorted_in(locale, lines):
    return subprocess.run(["sort"], input="".join(f"{ln}\n" for ln in lines), capture_output=True, text=True,
                          check=True, env={**os.environ, "LC_ALL": locale}).stdout


def a_locale_sorting_unlike_c():
    """An installed locale whose `sort` orders pins.env's names otherwise than C does, or None."""
    names = ["GCC_25_02=1", "GH_VERSION=1", "G_A=1"]
    installed = subprocess.run(["locale", "-a"], capture_output=True, text=True).stdout.split()
    return next((loc for loc in installed if sorted_in(loc, names) != sorted_in("C", names)), None)


def test_the_inputs_are_the_same_in_every_locale():
    """The label is made on one host and checked on another, each in its own locale."""
    locale = a_locale_sorting_unlike_c()
    # Fails rather than skips: a skip reads as a pass. The build image has none (#309), so CI runs
    # this file on the runner.
    assert locale is not None, "no installed locale sorts unlike C, such as en_US.UTF-8: run this file outside the image"

    def inputs(loc):
        call = f'. "{REPO}/scripts/lib/image-inputs.sh"; image_inputs "{REPO}" build 25.02'
        return subprocess.run(["bash", "-c", call], check=True, capture_output=True, text=True,
                              env={**os.environ, "LC_ALL": loc, "LANG": loc}).stdout
    assert inputs("C") == inputs(locale), locale


def test_a_version_s_pin_changes_only_that_version_s_labels():
    for pin, image in (("GCC_26_02", ("build", "26.02")), ("GCC_25_10", ("build", "25.10"))):
        with Repo() as repo:
            before = every_label(repo)
            bump(repo, pin)
            after = every_label(repo)
            moved = sorted(k for k in before if before[k] != after[k])
            assert moved == [image], (pin, moved)


def declare(repo, kind, after, arg, value="1"):
    """Declares ARG <arg> in <kind>.Dockerfile on the line after <after>, and gives it a pin."""
    dockerfile = repo.root / "docker" / f"{kind}.Dockerfile"
    text = dockerfile.read_text()
    assert text.count(after + "\n") == 1, after
    dockerfile.write_text(text.replace(after + "\n", f"{after}\nARG {arg}\n"))
    pins = repo.root / "docker" / "pins.env"
    if f"\n{arg}=" not in pins.read_text():
        pins.write_text(pins.read_text() + f"{arg}={value}\n")




def test_a_suffix_that_is_no_cudf_version_stays_in_every_label():
    with Repo() as repo:
        declare(repo, "build", "FROM cudf-${CUDF} AS toolchain", "ARROW_19_0")
        declare(repo, "run", "FROM ubuntu:24.04 AS prefix", "ARROW_19_0")
        before = every_label(repo)
        bump(repo, "ARROW_19_0")
        after = every_label(repo)
        moved = sorted(k for k in before if before[k] != after[k])
        assert moved == sorted(before), moved


def test_a_version_s_pin_read_by_another_version_s_stage_stays_in_its_label():
    with Repo() as repo:
        declare(repo, "build", "FROM cudf-${CUDF} AS toolchain", "GCC_26_02")
        before = every_label(repo)
        bump(repo, "GCC_26_02")
        after = every_label(repo)
        moved = sorted(k for k in before if before[k] != after[k])
        assert moved == [image for image in IMAGES if image[0] == "build"], moved


def test_the_inputs_refuse_a_flatbuffers_tag_named_twice():
    with Repo() as repo:
        cmake = repo.root / "cpp" / "CMakeLists.txt"
        cmake.write_text(cmake.read_text().replace("GIT_TAG        v25.12.19", "GIT_TAG v1\n  GIT_TAG v2"))
        r = run("build", "build", "25.02", "--dry-run", script=repo.script)
        assert r.returncode == 2 and "flatbuffers" in r.stderr, (r.returncode, r.stderr)
        assert r.stdout == "", r.stdout


# --- the Sirius image ---------------------------------------------------------------------------

# Values no input, label, argv or output may hold: the storage keys a Sirius build fetches with.
KEY_ID, SECRET = "AKIA-sentinel-key-id", "sentinel-secret-access-key"
CI_KEYS = {"AWS_ACCESS_KEY_ID": KEY_ID, "AWS_SECRET_ACCESS_KEY": SECRET, "AWS_ENDPOINT_URL": None,
           "AWS_REGION": None}
SECRETS = ["id=aws_access_key_id,env=AWS_ACCESS_KEY_ID", "id=aws_secret_access_key,env=AWS_SECRET_ACCESS_KEY",
           "id=aws_endpoint_url,env=AWS_ENDPOINT_URL", "id=aws_region,env=AWS_REGION"]


def keys_nowhere(result):
    seen = [text for text in (result.stdout, result.stderr, json.dumps(result.calls)) if KEY_ID in text or SECRET in text]
    assert not seen, f"a storage key reached the output or argv: {seen}"


def test_build_sirius_tags_its_three_names():
    with Repo() as repo:
        r = run("build", "sirius", "26.08", "--dry-run", script=repo.script, env=CI_KEYS)
        assert r.returncode == 0, r.stderr
        line = build_line(r)
        name = f"{REGISTRY}/peacockdb-sirius"
        assert flag_values(line, "-t") == [f"{name}:cudf-26.08", f"{name}:{inputs_tag(repo.root, 'sirius', '26.08')}",
                                           f"{name}:cudf-26.08-{repo.sha}"], line
        assert flag_values(line, "-f") == [str(repo.root / "docker" / "sirius.Dockerfile")], line
        assert "org.opencontainers.image.version=cudf-26.08" in flag_values(line, "--label"), line
        assert r.calls == [], f"a dry run called docker: {r.calls}"


def test_sirius_is_26_08_alone_and_26_08_is_sirius_s_alone():
    for args in [("build", "sirius", "25.02"), ("build", "run", "26.08"), ("build", "build", "26.08")]:
        r = run(*args, "--dry-run", env=CI_KEYS)
        assert r.returncode == 2, (args, r.returncode, r.stderr)
        assert len(r.stderr.strip().splitlines()) == 1 and r.stderr.startswith("image.sh: "), (args, r.stderr)
        assert "26.08" in r.stderr and "sirius" in r.stderr, (args, r.stderr)
        assert r.stdout == "" and r.calls == [], args


def test_the_sirius_label_names_its_files_and_pins_and_no_cudf_spec():
    with Repo() as repo:
        got = label(build_line(run("build", "sirius", "26.08", "--dry-run", script=repo.script, env=CI_KEYS)))
        assert got == image_inputs(repo.root, "sirius", "26.08"), got
        assert set(got) == files_read("sirius", "26.08") | {"docker/sirius.Dockerfile",
                                                             "docker/sirius.Dockerfile.dockerignore",
                                                             "docker/pins.env"}, got
        assert not [k for k in got if k.startswith("docker/conda/") or k.startswith("third_party/")], got
        every = IMAGES + [("sirius", "26.08")]

        def all_labels():
            return {image: label(build_line(run("build", *image, "--dry-run", script=repo.script, env=CI_KEYS)))
                    for image in every}
        before = all_labels()
        bump(repo, "SIRIUS_EXTENSION_SHA256")
        after = all_labels()
        moved = sorted(image for image in every if before[image] != after[image])
        assert moved == [("sirius", "26.08")], moved


def test_the_sirius_build_takes_the_keys_as_secrets_and_holds_them_nowhere_else():
    """CI's keys are in the environment: docker gets them by name, as BuildKit secrets, with the
    calibration bucket's storage, and no argv, output, label or input holds a value."""
    with Repo() as repo:
        r = run("build", "sirius", "26.08", script=repo.script, env=CI_KEYS)
        assert r.returncode == 0, r.stderr
        [line] = [["docker", *c] for c in r.calls if c[:1] == ["build"]]
        assert flag_values(line, "--secret") == SECRETS, line
        assert r.docker_env == [{"AWS_ACCESS_KEY_ID": KEY_ID, "AWS_SECRET_ACCESS_KEY": SECRET,
                                 "AWS_ENDPOINT_URL": "https://storage.eu-north1.nebius.cloud:443",
                                 "AWS_REGION": "eu-north-1"}], r.docker_env
        keys_nowhere(r)
        inputs = json.dumps(image_inputs(repo.root, "sirius", "26.08")) + inputs_tag(repo.root, "sirius", "26.08")
        assert KEY_ID not in inputs and SECRET not in inputs


def test_a_local_sirius_build_reads_the_keys_and_storage_from_peacock_env():
    with Repo() as repo, tempfile.TemporaryDirectory() as home:
        config = pathlib.Path(home) / ".config" / "peacock"
        config.mkdir(parents=True)
        (config / "peacock.env").write_text(f"AWS_ACCESS_KEY_ID={KEY_ID}\nAWS_SECRET_ACCESS_KEY={SECRET}\n"
                                            "AWS_ENDPOINT_URL=https://s3.example:443\nAWS_REGION=r-1\n")
        env = {"HOME": home, "AWS_ACCESS_KEY_ID": None, "AWS_SECRET_ACCESS_KEY": None, "AWS_ENDPOINT_URL": None,
               "AWS_REGION": None}
        r = run("build", "sirius", "26.08", script=repo.script, env=env)
        assert r.returncode == 0, r.stderr
        assert r.docker_env == [{"AWS_ACCESS_KEY_ID": KEY_ID, "AWS_SECRET_ACCESS_KEY": SECRET,
                                 "AWS_ENDPOINT_URL": "https://s3.example:443", "AWS_REGION": "r-1"}], r.docker_env
        keys_nowhere(r)


def test_a_sirius_build_without_keys_is_refused_before_docker():
    with tempfile.TemporaryDirectory() as home:
        for env in [{"HOME": home, "AWS_ACCESS_KEY_ID": None, "AWS_SECRET_ACCESS_KEY": None},
                    {"HOME": home, "AWS_ACCESS_KEY_ID": KEY_ID, "AWS_SECRET_ACCESS_KEY": None}]:
            r = run("build", "sirius", "26.08", env=env)
            assert r.returncode == 2, (env, r.returncode, r.stderr)
            assert len(r.stderr.strip().splitlines()) == 1 and "AWS_SECRET_ACCESS_KEY" in r.stderr, r.stderr
            assert r.calls == [], r.calls
            keys_nowhere(r)


def test_only_the_sirius_build_gets_keys():
    r = run("build", "run", "26.02", env=CI_KEYS)
    assert r.returncode == 0, r.stderr
    [line] = [c for c in r.calls if c[:1] == ["build"]]
    assert "--secret" not in line, line


def instructions(stage):
    """A Dockerfile stage's instructions, continuation lines joined and comments dropped."""
    joined = re.sub(r"\\\n\s*", " ", "\n".join(l for l in stage.splitlines() if not l.lstrip().startswith("#")))
    return [line.strip() for line in joined.splitlines() if line.strip()]


def test_the_sirius_keys_are_read_under_a_secret_mount_in_the_fetch_stage_alone():
    """No ARG or ENV can carry a key into a layer or the inputs: the keys exist only as BuildKit
    secrets, mounted for the one RUN that fetches, in a stage the image takes one file from."""
    text = (REPO / "docker" / "sirius.Dockerfile").read_text()
    stages = re.split(r"^FROM ", text, flags=re.M)[1:]
    named = {stage.split("\n", 1)[0].split()[-1]: instructions(stage) for stage in stages[:-1]}
    final = instructions(stages[-1])
    every = [line for stage in [*named.values(), final] for line in stage]
    storage = r"\bAWS_(ACCESS_KEY_ID|SECRET_ACCESS_KEY|ENDPOINT_URL|REGION)\b"
    assert not [line for line in every if re.match(r"(ARG|ENV)\b", line) and re.search(storage, line)], \
        "an ARG or ENV names a storage variable"
    secret = [line for line in every if "secret" in line]
    assert len(secret) == 1 and secret[0] in named["fetch"], secret
    assert secret[0].startswith("RUN --mount=type=secret,id=aws_access_key_id,required=true "), secret
    for name in ("aws_secret_access_key", "aws_endpoint_url", "aws_region"):
        assert f"--mount=type=secret,id={name},required=true " in secret[0], (name, secret)
    assert [line for line in final if line.startswith("COPY --from=fetch ")] == [
        "COPY --from=fetch /out/sirius.duckdb_extension /opt/sirius/sirius.duckdb_extension"], final


def test_the_sirius_image_ships_sirius_s_licence_at_its_pinned_commit():
    """The extension is Apache-2.0, and the image redistributes it: Sirius's LICENSE at SIRIUS_COMMIT
    is committed as docker/sirius-LICENSE, pinned by its sha256, checked by the build against the pin,
    and installed beside the extension, where the self-test holds it to the PIN."""
    licence = (REPO / "docker" / "sirius-LICENSE").read_bytes()
    assert hashlib.sha256(licence).hexdigest() == read_pins()["SIRIUS_LICENSE_SHA256"]
    assert licence.decode().split()[:4] == ["Apache", "License", "Version", "2.0,"], licence[:80]
    final = instructions(re.split(r"^FROM ", (REPO / "docker" / "sirius.Dockerfile").read_text(), flags=re.M)[-1])
    assert "COPY docker/sirius-LICENSE /opt/sirius/LICENSE" in final, final
    [pin] = [line for line in final if "/opt/sirius/PIN" in line and line.startswith("RUN ")]
    assert '"license_sha256=${SIRIUS_LICENSE_SHA256}"' in pin, pin
    assert 'echo "${SIRIUS_LICENSE_SHA256}  /opt/sirius/LICENSE" | sha256sum -c' in pin, pin
    ignore = (REPO / "docker" / "sirius.Dockerfile.dockerignore").read_text().splitlines()
    assert "!docker/sirius-LICENSE" in ignore, ignore


FETCH = REPO / "docker" / "sirius-fetch.sh"

FAKE_AWS = r'''#!/usr/bin/env python3
import json, os, shutil, sys
with open(os.environ["FAKE_AWS_LOG"], "a") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "env": {k: v for k, v in os.environ.items() if k.startswith("AWS_")}}) + "\n")
shutil.copyfile(os.environ["FAKE_AWS_OBJECT"], sys.argv[-1])
'''


def fetch(payload, secrets=("aws_access_key_id", "aws_secret_access_key", "aws_endpoint_url", "aws_region"),
          **pins):
    """sirius-fetch.sh over a fake aws that serves the gzip of `payload`: (proc, out file or None,
    aws calls)."""
    import gzip
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        gz = gzip.compress(payload, mtime=0)
        (tmp / "object.gz").write_bytes(gz)
        (tmp / "bin").mkdir()
        (tmp / "bin" / "aws").write_text(FAKE_AWS)
        (tmp / "bin" / "aws").chmod(0o755)
        values = {"aws_access_key_id": KEY_ID, "aws_secret_access_key": SECRET,
                  "aws_endpoint_url": "https://s3.example:443", "aws_region": "r-1"}
        (tmp / "secrets").mkdir()
        for name in secrets:
            (tmp / "secrets" / name).write_text(values[name])
        env = {"PATH": f"{tmp / 'bin'}:{os.environ['PATH']}", "FAKE_AWS_LOG": str(tmp / "aws.log"),
               "FAKE_AWS_OBJECT": str(tmp / "object.gz"),
               "SIRIUS_EXTENSION_S3": "s3://calibration/sirius/x.duckdb_extension.gz",
               "SIRIUS_EXTENSION_SHA256": hashlib.sha256(gz).hexdigest(),
               "SIRIUS_EXTENSION_BYTES": str(len(gz)),
               "SIRIUS_EXTENSION_UNPACKED_SHA256": hashlib.sha256(payload).hexdigest(), **pins}
        out = tmp / "out" / "sirius.duckdb_extension"
        proc = subprocess.run(["bash", str(FETCH), str(tmp / "secrets"), str(out)], env=env, capture_output=True,
                              text=True, timeout=60)
        log = tmp / "aws.log"
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return proc, out.read_bytes() if out.exists() else None, calls


def test_the_fetch_installs_the_extension_it_was_pinned():
    proc, out, calls = fetch(b"extension bytes")
    assert proc.returncode == 0, proc.stderr
    assert out == b"extension bytes"
    [call] = calls
    assert call["argv"][-2:][0] == "s3://calibration/sirius/x.duckdb_extension.gz", call
    assert call["env"] == {"AWS_ACCESS_KEY_ID": KEY_ID, "AWS_SECRET_ACCESS_KEY": SECRET,
                           "AWS_ENDPOINT_URL": "https://s3.example:443", "AWS_REGION": "r-1"}, call
    assert KEY_ID not in json.dumps(call["argv"]) and SECRET not in json.dumps(call["argv"]), call
    assert KEY_ID not in proc.stdout + proc.stderr and SECRET not in proc.stdout + proc.stderr


def test_a_wrong_sha256_or_byte_count_fails_the_fetch_and_installs_nothing():
    for pins in [{"SIRIUS_EXTENSION_SHA256": "0" * 64}, {"SIRIUS_EXTENSION_BYTES": "921613272"},
                 {"SIRIUS_EXTENSION_UNPACKED_SHA256": "0" * 64}]:
        proc, out, _ = fetch(b"extension bytes", **pins)
        assert proc.returncode != 0, (pins, proc.stdout, proc.stderr)
        assert out is None, pins
        assert next(iter(pins)) in proc.stderr, (pins, proc.stderr)


def test_the_fetch_without_its_secrets_calls_nothing():
    proc, out, calls = fetch(b"x", secrets=("aws_access_key_id",))
    assert proc.returncode != 0 and out is None and calls == [], (proc.stderr, calls)
    assert "aws_secret_access_key" in proc.stderr, proc.stderr


if __name__ == "__main__":
    raise SystemExit(main(globals()))
