"""images.yml, the workflow that publishes the images: started by hand only, it builds and pushes
through image.sh with the job's own token, and it frees and checks the runner's disk first.

The build image has no YAML parser, so the file is read by `read_workflow` below: block mappings
and sequences indented by spaces, plain and quoted scalars, one-line flow lists and `|` blocks. It
raises AssertionError on anything else, a line it does not read included.
"""
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main, raises  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
WORKFLOW = REPO / ".github" / "workflows" / "images.yml"

# The most one image build holds on disk at once, in GB of 10^9 bytes (build-test.md, "Images"):
# the 26.02 run image holds its 6.09 GB prefix twice. The margin is for what was not measured:
# the base layers, BuildKit's own state, and compressed blobs under a containerd image store.
LARGEST_BUILD_GB = 12.2
MARGIN_GB = 3

KEY = re.compile(r"""([A-Za-z_][\w-]*):(?:\s+(.*))?$""")


def _blank(line):
    stripped = line.strip()
    return not stripped or stripped.startswith("#")


def _skip(lines, i):
    while i < len(lines) and _blank(lines[i]):
        i += 1
    return i


def _indent(line):
    return len(line) - len(line.lstrip(" "))


def _is_item(line):
    stripped = line.strip()
    return stripped == "-" or stripped.startswith("- ")


def _scalar(text):
    assert text[:1] not in "{&*!>|%@`", f"images.yml: a value this reader does not read: {text!r}"
    if text[:1] in "'\"":
        end = text.index(text[0], 1)
        while text[0] == "'" and text[end + 1:end + 2] == "'":
            end = text.index("'", end + 2)
        assert not text[end + 1:].strip() or text[end + 1:].lstrip().startswith("#"), text
        return text[1:end].replace("''", "'")
    text = re.split(r"\s+#", text, maxsplit=1)[0].strip()
    if text.startswith("["):
        assert text.endswith("]"), text
        return [_scalar(part.strip()) for part in text[1:-1].split(",") if part.strip()]
    return text


def _literal(lines, i, indent):
    """A `|` block: the lines indented past the key, blank ones included, dedented."""
    j = i
    while j < len(lines) and (not lines[j].strip() or _indent(lines[j]) > indent):
        j += 1
    body = lines[i:j]
    while body and not body[-1].strip():
        body.pop()
    margin = min(_indent(line) for line in body if line.strip())
    return "\n".join(line[margin:] for line in body) + "\n", j


def _block(lines, i):
    return (_sequence if _is_item(lines[i]) else _mapping)(lines, i, _indent(lines[i]))


def _mapping(lines, i, indent):
    out = {}
    while i < len(lines) and _indent(lines[i]) == indent and not _is_item(lines[i]):
        m = KEY.match(lines[i].strip())
        assert m, f"images.yml line {i + 1} is not `key: value`: {lines[i]!r}"
        key, rest = m[1], m[2] or ""
        assert key not in out, f"images.yml line {i + 1} repeats {key}"
        if rest in ("|", "|-"):
            out[key], i = _literal(lines, i + 1, indent)
        elif not rest.strip() or rest.lstrip().startswith("#"):
            j = _skip(lines, i + 1)
            nested = j < len(lines) and (_indent(lines[j]) > indent or (_indent(lines[j]) == indent and _is_item(lines[j])))
            out[key], i = _block(lines, j) if nested else (None, j)
        else:
            out[key], i = _scalar(rest), i + 1
        i = _skip(lines, i)
    assert i == len(lines) or _indent(lines[i]) < indent, f"images.yml line {i + 1} is indented past its mapping"
    return out, i


def _sequence(lines, i, indent):
    out = []
    while i < len(lines) and _indent(lines[i]) == indent and _is_item(lines[i]):
        rest = lines[i].strip()[1:].strip()
        if KEY.match(rest):
            lines[i] = " " * (indent + 2) + rest
            item, i = _mapping(lines, i, indent + 2)
        elif rest:
            item, i = _scalar(rest), _skip(lines, i + 1)
        else:
            item, i = _block(lines, _skip(lines, i + 1))
        out.append(item)
    return out, i


def read_workflow(path=WORKFLOW):
    lines = path.read_text().split("\n")
    for n, line in enumerate(lines, 1):
        assert "\t" not in line[:_indent(line) + 1], f"images.yml line {n} is indented by a tab"
    tree, i = _block(lines, _skip(lines, 0))
    assert _skip(lines, i) == len(lines), f"images.yml line {i + 1} was not read: {lines[i]!r}"
    return tree


def job():
    (only,) = read_workflow()["jobs"].values()
    return only


def step(name):
    (found,) = [s for s in job()["steps"] if s.get("name") == name]
    return found


def committed_cudf_versions():
    return sorted(p.name[len("cudf-"):-len(".spec")] for p in (REPO / "docker" / "conda").glob("cudf-*.spec"))


def test_the_reader_reads_what_it_is_given():
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "w.yml"
        path.write_text("a:\n  b: [x, 'y']  # note\n  c:\n    - d: 1\n      e: \"2\"\n    - f\n  g: |\n    one\n\n    two\n")
        assert read_workflow(path) == {"a": {"b": ["x", "y"], "c": [{"d": "1", "e": "2"}, "f"], "g": "one\n\ntwo\n"}}


def test_the_reader_refuses_what_it_does_not_read():
    bad = ["a:\n\tb: 1\n", "a: {b: 1}\n", "a: >-\n  x\n", "a: &x 1\n", "a: |+\n  x\n",
           "a:\n  b: 1\n   c: 2\n", "a b\n", "a: 1\na: 2\n"]
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "w.yml"
        for text in bad:
            path.write_text(text)
            with raises(AssertionError):
                read_workflow(path)


def test_a_dispatch_is_the_only_trigger():
    assert set(read_workflow()) == {"name", "on", "permissions", "concurrency", "jobs"}
    assert list(read_workflow()["on"]) == ["workflow_dispatch"]


def test_the_choices_are_the_kinds_and_the_committed_cudf_versions():
    inputs = read_workflow()["on"]["workflow_dispatch"]["inputs"]
    assert inputs["kind"]["type"] == inputs["cudf"]["type"] == "choice"
    assert inputs["kind"]["options"] == ["build", "run", "all"]
    assert inputs["cudf"]["options"] == committed_cudf_versions() + ["all"]


def test_all_runs_every_other_choice_and_one_choice_runs_itself():
    inputs = read_workflow()["on"]["workflow_dispatch"]["inputs"]
    for name, expression in job()["strategy"]["matrix"].items():
        everything = json.dumps(inputs[name]["options"][:-1], separators=(",", ":"))
        assert expression == (f"${{{{ fromJSON(inputs.{name} == 'all' && '{everything}' "
                              f"|| format('[\"{{0}}\"]', inputs.{name})) }}}}"), expression


def test_the_token_writes_packages_and_only_reads_the_repository():
    assert read_workflow()["permissions"] == {"contents": "read", "packages": "write"}
    assert "permissions" not in job()
    assert re.findall(r"secrets\.\w+", WORKFLOW.read_text()) == []


PUSH = """set -euo pipefail
docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin <<< "$GH_TOKEN"
scripts/env/image.sh push "$KIND" "$CUDF"
"""


def test_the_job_runs_its_four_steps_unconditionally():
    """No `if:` or `continue-on-error:` anywhere: a skipped or forgiven step would publish an image
    the disk gate or a failed build should have stopped."""
    assert set(job()) == {"name", "runs-on", "timeout-minutes", "strategy", "defaults", "env", "steps"}, set(job())
    shapes = [(s.get("name"), set(s)) for s in job()["steps"]]
    assert shapes == [(None, {"uses", "with"}), ("Free the disk", {"name", "run"}), ("Build", {"name", "run"}),
                      ("Push", {"name", "env", "run"})], shapes
    assert job()["steps"][0] == {"uses": "actions/checkout@v4", "with": {"persist-credentials": "false"}}


def test_the_image_is_built_and_pushed_through_image_sh_and_nothing_else():
    assert job()["env"] == {"KIND": "${{ matrix.kind }}", "CUDF": "${{ matrix.cudf }}"}
    assert step("Build")["run"] == 'scripts/env/image.sh build "$KIND" "$CUDF"'
    assert step("Push")["env"] == {"GH_TOKEN": "${{ github.token }}"}
    assert step("Push")["run"] == PUSH
    for s in job()["steps"]:
        assert not re.search(r"\bdocker\s+(image\s+)?(build|push)\b|\bbuildx\b", s.get("run", "")), s


FAKES = {
    "sudo": 'echo "sudo $*" >> "$FAKE_LOG"\n',
    "docker": ('echo "docker $*" >> "$FAKE_LOG"\n'
               'case "$*" in *DockerRootDir*) echo "$FAKE_ROOT" ;; *.Driver*) echo overlay2 ;; esac\n'),
    "df": 'echo "df $*" >> "$FAKE_LOG"\necho "    Avail"\necho "$FAKE_AVAIL_BYTES"\n',
}


def free_the_disk(avail_gb):
    with tempfile.TemporaryDirectory() as tmp:
        bin_dir = pathlib.Path(tmp) / "bin"
        bin_dir.mkdir()
        for name, body in FAKES.items():
            (bin_dir / name).write_text("#!/bin/bash\n" + body)
            (bin_dir / name).chmod(0o755)
        log = pathlib.Path(tmp) / "log"
        env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}", FAKE_LOG=str(log),
                   FAKE_ROOT="/var/lib/docker", FAKE_AVAIL_BYTES=str(round(avail_gb * 10**9)))
        proc = subprocess.run(["bash", "-c", step("Free the disk")["run"]], env=env,
                              capture_output=True, text=True, timeout=30)
        return proc, log.read_text().splitlines()


def needed_gb():
    (need,) = re.findall(r"^need_gb=(\d+)$", step("Free the disk")["run"], re.M)
    return int(need)


def test_the_disk_gate_needs_the_largest_build_and_the_margin():
    assert needed_gb() >= LARGEST_BUILD_GB + MARGIN_GB, needed_gb()


def test_the_disk_gate_counts_in_gb_of_ten_to_the_ninth_bytes():
    assert free_the_disk(needed_gb())[0].returncode == 0
    proc, calls = free_the_disk(needed_gb() - 0.001)
    assert proc.returncode != 0, proc
    assert any(c.startswith("df --output=avail -B1 ") for c in calls), calls


def test_the_disk_step_prints_docker_s_storage_driver():
    proc, calls = free_the_disk(needed_gb())
    assert "docker info --format {{.Driver}}" in calls and "overlay2" in proc.stdout, (calls, proc.stdout)


def test_a_runner_short_of_disk_stops_before_the_build_naming_both_numbers():
    proc, calls = free_the_disk(needed_gb() - 1)
    assert proc.returncode != 0, proc
    assert f"{needed_gb() - 1} GB free under /var/lib/docker" in proc.stdout + proc.stderr, proc
    assert f"needs {needed_gb()}" in proc.stdout + proc.stderr, proc


def test_the_disk_is_freed_before_it_is_measured():
    proc, calls = free_the_disk(needed_gb())
    assert proc.returncode == 0, proc
    measured = [i for i, c in enumerate(calls) if c.startswith("df ") and "/var/lib/docker" in c]
    assert measured, calls
    for freeing in ["sudo rm -rf /usr/local/lib/android ", "docker image prune --all"]:
        freed = [i for i, c in enumerate(calls) if c.startswith(freeing)]
        assert freed and freed[-1] < measured[-1], (freeing, calls)


def test_every_run_block_parses_in_bash():
    for s in job()["steps"]:
        if "run" in s:
            proc = subprocess.run(["bash", "-n"], input=s["run"], capture_output=True, text=True, timeout=30)
            assert proc.returncode == 0, (s.get("name"), proc.stderr)


def test_actions_are_pinned_to_a_major_tag_as_the_other_workflows_pin_them():
    uses = [s["uses"] for s in job()["steps"] if "uses" in s]
    assert uses, "no action is used"
    for action in uses:
        assert re.fullmatch(r"[\w.-]+/[\w.-]+@v\d+", action), action
        others = [p for p in WORKFLOW.parent.glob("*.yml") if p != WORKFLOW and f"uses: {action}" in p.read_text()]
        assert others, f"{action} is pinned by no other workflow"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
