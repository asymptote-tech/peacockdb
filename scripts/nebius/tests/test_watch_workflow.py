"""nebius-watch.yml and the nebius-setup action, read as text: one leg per VM, each skippable alone.

No YAML parser (stdlib only): the helpers read the block layout the files are written in. The
two shell steps that decide a leg's fate, the secrets check and the check itself, are run for real.
"""
import os, pathlib, re, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github/workflows/nebius-watch.yml"
ACTION = ROOT / ".github/actions/nebius-setup/action.yml"
PIPELINE = ROOT / ".github/workflows/pipeline.yml"
H100 = "computeinstance-e00e6yena4nzjgaym0"
CPU = "computeinstance-e01t2mm4265y8x88ey"
LEGS = [
    {"vm": "nebius-gpu", "instance": '""', "profile": "ci", "prefix": "NEBIUS"},
    {"vm": "nebius-h100", "instance": H100, "profile": "ci", "prefix": "NEBIUS"},
    {"vm": "nebius-cpu128", "instance": CPU, "profile": "eu-west1", "prefix": "NEBIUS_EU_WEST1"},
]


def indent(line):
    return len(line) - len(line.lstrip(" "))


def block(lines, at):
    """The lines under lines[at], up to the next line indented no deeper than it."""
    out = []
    for line in lines[at + 1:]:
        if line.strip() and indent(line) <= indent(lines[at]):
            break
        out.append(line)
    return out


def legs(text):
    lines = text.splitlines()
    at = next(i for i, l in enumerate(lines) if l.strip() == "include:")
    items = []
    for line in block(lines, at):
        m = re.match(r"\s*(- )?(\w+):\s*(.*)$", line)
        if not m:
            continue
        if m.group(1):
            items.append({})
        items[-1][m.group(2)] = m.group(3).strip()
    return items


def steps(text):
    """Each step of the one job, as its own list of lines."""
    lines = text.splitlines()
    at = next(i for i, l in enumerate(lines) if l.strip() == "steps:")
    out = []
    for line in block(lines, at):
        if re.match(r"\s*- ", line) and (not out or indent(line) == indent(out[0][0])):
            out.append([line])
        elif out:
            out[-1].append(line)
    return out


def step(text, sid):
    found = [s for s in steps(text) if any(l.strip() == f"id: {sid}" for l in s)]
    assert len(found) == 1, (sid, found)
    return found[0]


def key(lines, name):
    """The value of `name:` in a step, its first occurrence."""
    for l in lines:
        m = re.match(rf"\s*(- )?{re.escape(name)}:\s*(.*)$", l)
        if m:
            return m.group(2).strip()
    return None


def run_block(lines):
    """A step's `run: |` body, dedented."""
    at = next(i for i, l in enumerate(lines) if re.match(r"\s*(- )?run: \|", l))
    body = block(lines, at)
    pad = min(indent(l) for l in body if l.strip())
    return "\n".join(l[pad:] for l in body) + "\n"


def run_blocks(text):
    lines = text.splitlines()
    out = []
    for i, l in enumerate(lines):
        if re.match(r"\s*(- )?run:", l):
            out.append(l.split("run:", 1)[1] + "\n" + "\n".join(block(lines, i)))
    assert out, "no run: block found"
    return out


def test_workflow_has_three_legs():
    text = WORKFLOW.read_text()
    assert legs(text) == LEGS, legs(text)
    assert H100 in text and CPU in text


def test_workflow_cpu_leg_uses_eu_west1():
    cpu = [l for l in legs(WORKFLOW.read_text()) if l["vm"] == "nebius-cpu128"]
    assert cpu and cpu[0]["prefix"] == "NEBIUS_EU_WEST1" and cpu[0]["profile"] == "eu-west1", cpu


def test_workflow_legs_are_independent():
    text = WORKFLOW.read_text()
    assert re.search(r"^\s*fail-fast: false\s*$", text, re.M), "a matrix leg's failure would cancel the others"
    setup = step(text, "setup")
    assert key(setup, "uses") == "./.github/actions/nebius-setup", setup
    assert key(setup, "continue-on-error") == "true", setup
    assert key(setup, "if") == "steps.secrets.outputs.present == 'true'", setup


def test_workflow_profile_failure_is_a_notice():
    text = WORKFLOW.read_text()
    notice = [s for s in steps(text) if key(s, "if") == "steps.setup.outcome == 'failure'"]
    assert len(notice) == 1, notice
    body = run_block(notice[0]) if any("run: |" in l for l in notice[0]) else key(notice[0], "run")
    assert "::notice::" in body and "could not be created; skipped" in body, body
    check = step(text, "check")
    assert key(check, "if") == "steps.setup.outcome == 'success'", check


def test_workflow_secrets_reach_only_env():
    text = WORKFLOW.read_text()
    lines = text.splitlines()
    uses = [i for i, l in enumerate(lines) if "secrets" in l and "${{" in l]
    assert uses, "no secret is used"
    for i in uses:
        parent = next(lines[j] for j in range(i - 1, -1, -1) if lines[j].strip() and indent(lines[j]) < indent(lines[i]))
        assert parent.strip() in ("env:", "with:"), (lines[i], parent)
    for body in run_blocks(text):
        assert "${{" not in body, body
        assert "secrets" not in body, body


def test_gpu_tests_untouched():
    lines = PIPELINE.read_text().splitlines()
    at = [i for i, l in enumerate(lines) if "uses: ./.github/actions/nebius-setup" in l]
    assert len(at) == 1, at
    with_at = at[0] + 1
    assert lines[with_at].strip() == "with:", lines[with_at]
    given = [l.split(":", 1)[0].strip() for l in block(lines, with_at) if l.strip()]
    assert given == ["sa-id", "public-key-id", "project-id", "private-key", "ssh-key"], given


def test_action_profile_input_defaults_to_ci():
    text = ACTION.read_text()
    lines = text.splitlines()
    at = next(i for i, l in enumerate(lines) if l.strip() == "profile:")
    assert key(block(lines, at), "default") in ("ci", "'ci'", '"ci"'), block(lines, at)
    assert re.search(r'^\s*PROFILE: \$\{\{ inputs\.profile \}\}\s*$', text, re.M), "profile reaches the script through env"
    assert re.search(r'nebius" profile create "\$PROFILE"', text), "the profile is created under its input's name"
    for body in run_blocks(text):
        assert "${{" not in body, body


def secrets_step(env):
    body = run_block(step(WORKFLOW.read_text(), "secrets"))
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "out"
        out.write_text("")
        p = subprocess.run(["bash", "-e", "-c", body], capture_output=True, text=True, timeout=30,
                           env={"PATH": os.environ["PATH"], "GITHUB_OUTPUT": str(out), **env})
        return p.returncode, p.stdout, out.read_text()


def test_secrets_step_present():
    full = {"VM": "nebius-cpu128", "PREFIX": "NEBIUS_EU_WEST1", "SA_ID": "s", "PUBLIC_KEY_ID": "k",
            "PROJECT_ID": "p", "PRIVATE_KEY": "-----BEGIN", "SSH_KEY": "-----BEGIN"}
    rc, out, outputs = secrets_step(full)
    assert rc == 0 and "::notice::" not in out, (rc, out)
    assert outputs.splitlines() == ["present=true"], outputs


def test_secrets_step_absent_is_a_notice():
    full = {"VM": "nebius-cpu128", "PREFIX": "NEBIUS_EU_WEST1", "SA_ID": "s", "PUBLIC_KEY_ID": "k",
            "PROJECT_ID": "p", "PRIVATE_KEY": "-----BEGIN", "SSH_KEY": "-----BEGIN"}
    for gone, name in (("SA_ID", "NEBIUS_EU_WEST1_SA_ID"), ("PRIVATE_KEY", "NEBIUS_EU_WEST1_PRIVATE_KEY"),
                       ("SSH_KEY", "NEBIUS_GPU_SSH_KEY")):
        rc, out, outputs = secrets_step({**full, gone: ""})
        assert rc == 0, (gone, rc, out)
        assert out.startswith("::notice::nebius-cpu128: ") and name in out and "skipped" in out, (gone, out)
        assert outputs.splitlines() == ["present=false"], (gone, outputs)


def check_argv(leg):
    """What the check step hands watch.sh for one leg, through a stub watch.sh."""
    check = step(WORKFLOW.read_text(), "check")
    body = run_block(check) if any("run: |" in l for l in check) else key(check, "run")
    with tempfile.TemporaryDirectory() as d:
        stub = pathlib.Path(d) / "scripts/nebius/watch.sh"
        stub.parent.mkdir(parents=True)
        stub.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$@"\n')
        stub.chmod(0o755)
        instance = "" if leg["instance"] == '""' else leg["instance"]
        p = subprocess.run(["bash", "-e", "-c", body], capture_output=True, text=True, timeout=30, cwd=d,
                           env={"PATH": os.environ["PATH"], "VM": leg["vm"], "INSTANCE": instance,
                                "PROFILE": leg["profile"], "CLOCK_OFFSET": "0"})
        assert p.returncode == 0, (leg, p.stderr)
        return p.stdout.splitlines()


def test_check_step_runs_watch_per_leg():
    gpu, h100, cpu = LEGS
    assert check_argv(gpu) == ["nebius-gpu", "--profile", "ci"]
    assert check_argv(h100) == ["nebius-h100", "--instance-id", H100, "--profile", "ci"]
    assert check_argv(cpu) == ["nebius-cpu128", "--instance-id", CPU, "--profile", "eu-west1"]
    check = step(WORKFLOW.read_text(), "check")
    env = block(check, next(i for i, l in enumerate(check) if l.strip() == "env:"))
    for name, value in (("VM", "matrix.vm"), ("INSTANCE", "matrix.instance"), ("PROFILE", "matrix.profile")):
        assert any(re.match(rf"\s*{name}: \$\{{\{{ {re.escape(value)} \}}\}}\s*$", l) for l in env), (name, env)


if __name__ == "__main__":
    sys.exit(main(globals()))
