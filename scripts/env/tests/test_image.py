"""image.sh: the docker build line it composes, the tags it gives, and the pushes it refuses.

The docker CLI is a fake on PATH that records its argv and answers `image inspect` from a map,
so nothing here builds, pulls or pushes.
"""
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


def run(*args, script=SCRIPT, images=None):
    with tempfile.TemporaryDirectory() as tmp:
        bin_dir = pathlib.Path(tmp) / "bin"
        bin_dir.mkdir()
        docker = bin_dir / "docker"
        docker.write_text(FAKE_DOCKER)
        docker.chmod(0o755)
        log = pathlib.Path(tmp) / "docker.log"
        log.touch()
        env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "FAKE_DOCKER_LOG": str(log),
               "FAKE_DOCKER_IMAGES": json.dumps(images or {})}
        proc = subprocess.run([str(script), *args], capture_output=True, text=True, env=env,
                              stdin=subprocess.DEVNULL, timeout=60)
        calls = [json.loads(line) for line in log.read_text().splitlines()]
    return Run(proc, calls)


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout.strip()


class Repo:
    """A throwaway git repo holding image.sh and its inputs, so clean and dirty are the test's to choose."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name) / "repo"
        for rel in ["scripts/env/image.sh", "scripts/lib/image-inputs.sh", "cpp/CMakeLists.txt"]:
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
    r = run("build", "build", "25.10", "--dry-run")
    assert r.returncode == 2, (r.returncode, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1 and "25.10" in r.stderr, r.stderr
    assert "25.02" in r.stderr and "26.02" in r.stderr, r.stderr
    assert r.stdout == "" and r.calls == []


def test_build_26_02():
    line = build_line(run("build", "build", "26.02", "--dry-run"))
    assert "CUDF=26.02" in flag_values(line, "--build-arg")
    assert f"{REGISTRY}/peacockdb-build:cudf-26.02" in flag_values(line, "-t")


def test_a_clean_tree_adds_the_commit_tag():
    with Repo() as repo:
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert flag_values(line, "-t") == [
            f"{REGISTRY}/peacockdb-build:cudf-25.02", f"{REGISTRY}/peacockdb-build:cudf-25.02-{repo.sha}"
        ], line
        assert f"org.opencontainers.image.revision={repo.sha}" in flag_values(line, "--label"), line


def test_a_dirty_tree_gets_no_commit_tag():
    with Repo() as repo:
        (repo.root / "untracked.txt").write_text("x")
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert flag_values(line, "-t") == [f"{REGISTRY}/peacockdb-build:cudf-25.02"], line
        assert f"org.opencontainers.image.revision={repo.sha}-dirty" in flag_values(line, "--label"), line


def test_an_untracked_file_is_dirty_whatever_the_git_config_hides():
    with Repo() as repo:
        git(repo.root, "config", "status.showUntrackedFiles", "no")
        (repo.root / "untracked.txt").write_text("x")
        line = build_line(run("build", "build", "25.02", "--dry-run", script=repo.script))
        assert flag_values(line, "-t") == [f"{REGISTRY}/peacockdb-build:cudf-25.02"], line


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


def test_push_pushes_both_tags():
    with Repo() as repo:
        name = f"{REGISTRY}/peacockdb-build:cudf-25.02"
        images = {name: "sha256:aaa", f"{name}-{repo.sha}": "sha256:aaa"}
        r = run("push", "build", "25.02", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert [c for c in r.calls if c[0] == "push"] == [["push", f"{name}-{repo.sha}"], ["push", name]], r.calls


def test_push_dry_run_pushes_nothing():
    with Repo() as repo:
        name = f"{REGISTRY}/peacockdb-build:cudf-25.02"
        images = {name: "sha256:aaa", f"{name}-{repo.sha}": "sha256:aaa"}
        r = run("push", "build", "25.02", "--dry-run", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert r.commands() == [["docker", "push", f"{name}-{repo.sha}"], ["docker", "push", name]], r.stdout
        assert not [c for c in r.calls if c[0] == "push"], r.calls


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


def test_run_push_pushes_both_run_tags():
    with Repo() as repo:
        name = f"{REGISTRY}/peacockdb-run:cudf-26.02"
        images = {name: "sha256:aaa", f"{name}-{repo.sha}": "sha256:aaa"}
        r = run("push", "run", "26.02", script=repo.script, images=images)
        assert r.returncode == 0, r.stderr
        assert [c for c in r.calls if c[0] == "push"] == [["push", f"{name}-{repo.sha}"], ["push", name]], r.calls


def test_bad_arguments_fail_in_one_line():
    for args in [(), ("build",), ("build", "build"), ("deploy", "build", "25.02"), ("build", "kit", "25.02"),
                 ("build", "build", "25.02", "--push"), ("build", "build", "25.02", "extra")]:
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
# Pins gpu.sh's driver check reads from the tree on a GPU host, no image (scripts/env/gpu.sh).
DEVICE_RUN_PINS = {"DRIVER_MINIMUM_PTX"}


def test_the_dockerfiles_declare_every_pin_but_the_machine_s():
    """image.sh passes every pin to both Dockerfiles and BuildKit is silent about an unused one, so
    a pin neither declares is a pin nothing reads."""
    declared = {kind: set(re.findall(r"^ARG (\w+)", (REPO / "docker" / f"{kind}.Dockerfile").read_text(), re.M))
                for kind in ["build", "run"]}
    assert MACHINE_PINS <= set(read_pins()), "pins.env lost a machine pin"
    assert DEVICE_RUN_PINS <= set(read_pins()), "pins.env lost a device-run pin"
    missing = sorted((set(read_pins()) - MACHINE_PINS - DEVICE_RUN_PINS | {"CUDF", "CUDF_SUBMODULE"})
                     - (declared["build"] | declared["run"]))
    assert not missing, f"no Dockerfile declares: {missing}"
    assert "CUDF" in declared["run"], "run.Dockerfile never declares CUDF"
    for kind, names in declared.items():
        assert not names & MACHINE_PINS, f"{kind}.Dockerfile declares machine pins: {names & MACHINE_PINS}"


def test_no_claude_code_is_installed_in_the_image():
    for kind in ["build", "run"]:
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
    for kind in ("build", "run"):
        for cudf in ("25.02", "26.02"):
            read = files_read(kind, cudf)
            assert f"docker/conda/cudf-{cudf}.spec" in read, (kind, cudf, read)
            missing = sorted(read - set(image_inputs(REPO, kind, cudf)))
            assert not missing, f"the {kind} image's {cudf} label misses {missing}"
    assert "docker/conda/cudf-26.02.spec" in files_read("build", "25.02"), "the nvcc spec is no longer read"


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
    if locale is None:
        print("  SKIP test_the_inputs_are_the_same_in_every_locale: no installed locale sorts unlike C")
        return

    def inputs(loc):
        call = f'. "{REPO}/scripts/lib/image-inputs.sh"; image_inputs "{REPO}" build 25.02'
        return subprocess.run(["bash", "-c", call], check=True, capture_output=True, text=True,
                              env={**os.environ, "LC_ALL": loc, "LANG": loc}).stdout
    assert inputs("C") == inputs(locale), locale


def test_a_version_s_pin_changes_only_that_version_s_labels():
    with Repo() as repo:
        def all_labels():
            return {(kind, cudf): label(build_line(run("build", kind, cudf, "--dry-run", script=repo.script)))
                    for kind in ("build", "run") for cudf in ("25.02", "26.02")}
        before = all_labels()
        bump(repo, "GCC_26_02")
        after = all_labels()
        moved = sorted(k for k in before if before[k] != after[k])
        assert moved == [("build", "26.02")], moved


def declare(repo, kind, after, arg, value="1"):
    """Declares ARG <arg> in <kind>.Dockerfile on the line after <after>, and gives it a pin."""
    dockerfile = repo.root / "docker" / f"{kind}.Dockerfile"
    text = dockerfile.read_text()
    assert text.count(after + "\n") == 1, after
    dockerfile.write_text(text.replace(after + "\n", f"{after}\nARG {arg}\n"))
    pins = repo.root / "docker" / "pins.env"
    if f"\n{arg}=" not in pins.read_text():
        pins.write_text(pins.read_text() + f"{arg}={value}\n")


def every_label(repo):
    return {(kind, cudf): label(build_line(run("build", kind, cudf, "--dry-run", script=repo.script)))
            for kind in ("build", "run") for cudf in ("25.02", "26.02")}


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
        assert moved == [("build", "25.02"), ("build", "26.02")], moved


def test_the_inputs_refuse_a_flatbuffers_tag_named_twice():
    with Repo() as repo:
        cmake = repo.root / "cpp" / "CMakeLists.txt"
        cmake.write_text(cmake.read_text().replace("GIT_TAG        v25.12.19", "GIT_TAG v1\n  GIT_TAG v2"))
        r = run("build", "build", "25.02", "--dry-run", script=repo.script)
        assert r.returncode == 2 and "flatbuffers" in r.stderr, (r.returncode, r.stderr)
        assert r.stdout == "", r.stdout


if __name__ == "__main__":
    raise SystemExit(main(globals()))
