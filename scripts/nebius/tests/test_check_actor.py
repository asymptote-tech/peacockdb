"""Only an actor on GPU_ALLOWED_ACTORS reaches the GPU host."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[1] / "check-actor.sh")


def run(actor, allowed):
    return subprocess.run([SCRIPT, actor, allowed], capture_output=True, text=True)


def rc(actor, allowed):
    return run(actor, allowed).returncode


def test_listed_passes():
    assert rc("alephonea", "alephonea,romak00") == 0


def test_unlisted_fails():
    assert rc("stranger", "alephonea,romak00") != 0


def test_empty_list_fails():
    assert rc("alephonea", "") != 0


def test_no_substring_match():
    assert rc("alephone", "alephonea") != 0 and rc("x", "x-y") != 0


def test_spaces_tolerated():
    assert rc("romak00", "alephonea, romak00") == 0


# The refusal is the script's own exit 1 with a reason in the log, not a crash that also
# happens to be non-zero.
def test_refusal_says_why():
    r = run("stranger", "alephonea,romak00")
    assert r.returncode == 1 and "stranger is not in GPU_ALLOWED_ACTORS" in r.stderr, r


# An empty entry ("a,,b", a trailing comma) must not admit an empty actor.
def test_empty_actor_fails():
    assert rc("", "alephonea,,romak00,") != 0


if __name__ == "__main__":
    raise SystemExit(main(globals()))
