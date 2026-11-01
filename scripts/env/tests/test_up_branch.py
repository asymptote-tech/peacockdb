"""up.sh: the branch a chain's new worktree is on, never guessed.

A new chain's base and first task come from the board `--board` names, origin/master by default;
a chain that ran before is resumed only by naming its branch with `--branch`. Run on the fake
machine of `machine.py`, whose origin carries the board of chains A, B and E.
"""
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import BOARD, Machine, git  # noqa: E402
from test_up import flag_values, refused, the_instance_run_last, up_ok  # noqa: E402


def worktree_of(m, chain):
    return pathlib.Path(flag_values(the_instance_run_last(m, chain), "-w")[0])


def board_with(chain_a_state):
    return BOARD.replace("[`alpha.md`](alpha.md) — state: approved to build",
                         f"[`alpha.md`](alpha.md) — state: {chain_a_state}")


def test_up_resumes_a_branch_named_unlike_its_task():
    with Machine() as m:
        sha = m.push_branch("ENS-alpha-run", board_with("reviewing — PR #7"))
        up_ok(m, "A", "--branch", "ENS-alpha-run", "--env", m.keys_file)
        wa = worktree_of(m, "A")
        assert git(wa, "branch", "--show-current") == "ENS-alpha-run"
        assert git(wa, "rev-parse", "HEAD") == sha
        assert git(wa, "rev-parse", "--abbrev-ref", "ENS-alpha-run@{upstream}") == "origin/ENS-alpha-run"


def test_up_refuses_a_chain_that_ran_before():
    with Machine() as m:
        m.push_branch("ENS-alpha-run", board_with("building"))
        m.push_branch("ENS1-alpha-copy", board_with("blocked(done) — PR #9"))
        m.push_branch("ENS-not-yet", board_with("approved to build"))
        m.push_branch("ENS-beta-run", BOARD.replace("[`beta.md`](beta.md) — state: approved to build",
                                                     "[`beta.md`](beta.md) — state: done"))
        r = m.up("A", "--env", m.keys_file)
        refused(r, m, "chain A already ran on", "ENS-alpha-run", "ENS1-alpha-copy", "--branch")
        assert "ENS-not-yet" not in r.stderr and "ENS-beta-run" not in r.stderr, r.stderr
        assert not (m.home / "peacock-instances" / "A").exists()
    with Machine() as m:
        m.push_branch("ENS-alpha", board_with("approved to build"))
        refused(m.up("A", "--env", m.keys_file), m, "ENS-alpha", "--branch")
    with Machine() as m:
        git(m.clone, "branch", "ENS-alpha")
        refused(m.up("A", "--env", m.keys_file), m, "ENS-alpha")


def test_up_refuses_an_unknown_branch():
    with Machine() as m:
        refused(m.up("A", "--branch", "ENS-nope", "--env", m.keys_file), m, "ENS-nope", "origin")
        assert not (m.home / "peacock-instances" / "A").exists()
    with Machine() as m:
        m.push_branch("ENS-elsewhere", BOARD.replace("## Chain A (base: master)", "## Chain Q (base: master)"))
        refused(m.up("A", "--branch", "ENS-elsewhere", "--env", m.keys_file), m, "ENS-elsewhere", "A")


def test_up_brings_a_local_branch_up_to_its_origin():
    with Machine() as m:
        sha = m.push_branch("ENS-alpha-run", board_with("building"))
        git(m.clone, "branch", "ENS-alpha-run", "master")
        up_ok(m, "A", "--branch", "ENS-alpha-run", "--env", m.keys_file)
        assert git(worktree_of(m, "A"), "rev-parse", "HEAD") == sha, "the local branch was not fast-forwarded"
    with Machine() as m:
        m.push_branch("ENS-alpha-run", board_with("building"))
        git(m.clone, "checkout", "-q", "-b", "ENS-alpha-run", "master")
        (m.clone / "diverged.txt").write_text("a local commit origin lacks\n")
        git(m.clone, "add", "diverged.txt")
        git(m.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "diverged")
        git(m.clone, "checkout", "-q", "master")
        refused(m.up("A", "--branch", "ENS-alpha-run", "--env", m.keys_file), m,
                "ENS-alpha-run", "origin/ENS-alpha-run", "diverged")


def test_up_starts_a_new_chain_at_its_base():
    with Machine() as m:
        base = git(m.clone, "rev-parse", "origin/master")
        (m.clone / "local-only.txt").write_text("not on origin\n")
        git(m.clone, "add", "local-only.txt")
        git(m.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "local only")
        up_ok(m, "B", "--env", m.keys_file)
        wb = worktree_of(m, "B")
        assert git(wb, "branch", "--show-current") == "ENS-beta", "not the lowest-numbered task's branch"
        assert git(wb, "rev-parse", "HEAD") == base, "not origin's base"


def test_up_reads_a_new_chain_from_the_board_it_is_given():
    scratch = BOARD + ("\n## Chain S (base: ENS-scratch-board)\n\n"
                       "### 1. [`scratch-task.md`](scratch-task.md) — state: new\n")
    with Machine() as m:
        sha = m.push_branch("ENS-scratch-board", scratch)
        refused(m.up("S", "--env", m.keys_file), m, "S", "origin/master")
        up_ok(m, "S", "--board", "origin/ENS-scratch-board", "--env", m.keys_file)
        ws = worktree_of(m, "S")
        assert git(ws, "branch", "--show-current") == "ENS-scratch-task"
        assert git(ws, "rev-parse", "HEAD") == sha, "not the base the section names"
    with Machine() as m:
        refused(m.up("A", "--board", "origin/no-such-ref", "--env", m.keys_file), m, "origin/no-such-ref")


def test_up_refuses_a_chain_the_board_lacks():
    for chain, names in (("Z", ("Z", "master")), ("E", ("E", "no task"))):
        with Machine() as m:
            refused(m.up(chain, "--env", m.keys_file), m, *names)
            assert not (m.home / "peacock-instances" / chain).exists()
    with Machine() as m:
        board = m.clone / "llm-wiki" / "tasks" / "tasks.md"
        board.write_text(board.read_text() + "\n## Chain F (base: master)\n\n### 1. [`f.md`](f.md) — state: new\n")
        git(m.clone, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "F, not pushed")
        refused(m.up("F", "--env", m.keys_file), m, "F", "master")


def test_up_rerun_keeps_the_worktree_s_branch():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        wa = worktree_of(m, "A")
        git(wa, "checkout", "-q", "-b", "ENS-alpha-two")
        assert m.down("A").returncode == 0
        up_ok(m, "A", "--env", m.keys_file)
        assert worktree_of(m, "A") == wa
        assert git(wa, "branch", "--show-current") == "ENS-alpha-two"
        assert m.down("A").returncode == 0
        up_ok(m, "A", "--branch", "ENS-alpha-two", "--env", m.keys_file)
        assert m.down("A").returncode == 0
        refused_now = m.up("A", "--branch", "ENS-alpha", "--env", m.keys_file)
        assert refused_now.returncode == 2 and "ENS-alpha-two" in refused_now.stderr, refused_now.stderr
        refused_now = m.up("A", "--board", "origin/master", "--env", m.keys_file)
        assert refused_now.returncode == 2 and "--board" in refused_now.stderr, refused_now.stderr



def test_board_ran_reads_a_state_past_approved_to_build():
    lib = pathlib.Path(__file__).resolve().parents[2] / "lib" / "board.sh"
    for state, ran in (("reviewing — PR #7", True), ("blocked(done) — PR #9", True),
                       ("rebase needed(reviewing)", True), ("completeness approved", True),
                       ("blocked(approved to build)", False), ("blocked(new)", False),
                       ("approved to build (resumed 2026-11-01)", False), ("new", False)):
        r = subprocess.run(["bash", "-c", f'. "{lib}"; board_ran A alpha'], input=board_with(state),
                           capture_output=True, text=True)
        assert (r.returncode == 0) is ran, (state, r.returncode, r.stderr)


def test_up_refuses_a_base_whose_board_lacks_the_chain():
    scratch = BOARD + "\n## Chain S (base: master)\n\n### 1. [`scratch-task.md`](scratch-task.md) — state: new\n"
    with Machine() as m:
        m.push_branch("ENS-scratch-board", scratch)
        refused(m.up("S", "--board", "origin/ENS-scratch-board", "--env", m.keys_file), m, "S", "origin/master")
        assert not (m.home / "peacock-instances" / "S").exists()
        assert "ENS-scratch-task" not in git(m.clone, "branch", "--list", "ENS-scratch-task")


def test_up_refuses_a_branch_checked_out_elsewhere():
    with Machine() as m:
        m.push_branch("ENS-alpha-run", board_with("building"))
        old = git(m.clone, "rev-parse", "master")
        elsewhere = m.home / "elsewhere"
        git(m.clone, "worktree", "add", "-q", "-b", "ENS-alpha-run", str(elsewhere), "master")
        refused(m.up("A", "--branch", "ENS-alpha-run", "--env", m.keys_file), m, "ENS-alpha-run", str(elsewhere))
        assert git(m.clone, "rev-parse", "ENS-alpha-run") == old, "the branch moved under another worktree"

if __name__ == "__main__":
    raise SystemExit(main(globals()))
