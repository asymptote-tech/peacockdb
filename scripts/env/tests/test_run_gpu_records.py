"""`run-gpu.sh`: what a tree keeps of its runs, and whose a VM dir is.

A tree's id in `.run-gpu-tree`, beside the path it was made for, or `PEACOCK_TREE_ID`; each VM dir
records the id of the tree that made or claimed it in `.tree`, and refuses every other tree's call
before it touches the dir. gpu_host.py has the fakes; test_run_gpu_runs.py has the runner.
"""
import os
import pathlib
import re
import shutil
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import Host, until  # noqa: E402
from test_run_gpu_runs import STAGED, held, ok, on_card, refused, shipped, staged  # noqa: E402

RECORD = ".run-gpu-tree"
CI = {"PEACOCK_TREE_ID": "ci"}


def recorded(tree):
    """The tree's record: its id and the path it was made for."""
    tree_id, path = (tree / RECORD).read_text().splitlines()
    assert re.fullmatch(r"[0-9a-f]{32}", tree_id), tree_id
    return tree_id, path


def owner(h, d="chain-l"):
    return (h.vm_dir(d) / ".tree").read_text().strip()


def tree_copy(h, where):
    """A copy of the Host's tree, under the same name: `where`/chain-l beside dev/."""
    copy = h.tree.parent.parent / where / h.tree.name
    shutil.copytree(h.tree, copy, symlinks=True)
    return copy


def gpu_in(h, tree, *args, env=None):
    """<tree>'s run-gpu.sh, with the Host's VM and fakes."""
    own, h.tree = h.tree, tree
    try:
        return h.gpu(*args, env=env)
    finally:
        h.tree = own


def snapshot(d):
    """Every path under <d>, a file with its bytes: what a refused call must leave as it was."""
    return {str(p.relative_to(d)): p.read_bytes() if p.is_file() and not p.is_symlink() else None
            for p in d.rglob("*")}


def test_a_tree_records_its_id_and_its_dir_on_the_vm():
    with Host() as h:
        refused(h.gpu("--dir", "a/b", "ship"), 2, "a/b")
        assert not (h.tree / RECORD).exists(), "a refused call made the record"
        shipped(h)
        tree_id, path = recorded(h.tree)
        assert path == str(h.tree.resolve()), path
        assert owner(h) == tree_id, owner(h)
        for args in (("ship",), ("run", "list"), ("pull",), ("cancel",), ("--dir", "second", "ship")):
            ok(h.gpu(*args))
        assert recorded(h.tree) == (tree_id, path)
        assert owner(h) == tree_id and owner(h, "second") == tree_id, (owner(h), owner(h, "second"))


def test_a_copied_tree_is_refused_its_record_before_any_vm_call():
    with Host() as h:
        shipped(h)
        copy = tree_copy(h, "copy")
        kept = (copy / RECORD).read_text()
        calls = len(h.nebius_calls())
        for args in (("prepare", "--no-sf40"), ("ship",), ("run", "list"), ("run", STAGED), ("pull",),
                     ("cancel",), ("status",), ("follow",), ("--dir", "copy", "ship")):
            r = gpu_in(h, copy, *args)
            refused(r, 2, RECORD, str(h.tree.resolve()), "remove", "--dir")
        assert len(h.nebius_calls()) == calls, h.nebius_calls()[calls:]
        assert (copy / RECORD).read_text() == kept
        (copy / RECORD).unlink()
        ok(gpu_in(h, copy, "--dir", "copy", "ship"))
        copy_id, path = recorded(copy)
        assert path == str(copy.resolve()) and copy_id != recorded(h.tree)[0], (copy_id, path)
        assert owner(h, "copy") == copy_id
        ok(h.gpu("ship"))


def test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears():
    with Host() as h:
        other = tree_copy(h, "other")
        (other / "cpp/install/lib/libpeacock_gpu.so").write_text("the other tree's build\n")
        staged(h)
        d = h.vm_dir()
        result = d / "testdata/benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt"
        result.parent.mkdir(parents=True)
        result.write_text("the first tree's result\n")
        before, rsh = snapshot(d), h.verbs().count("rsh")
        for args in (("prepare", "--no-sf40"), ("ship",), ("run", "list"), ("run", STAGED), ("pull",),
                     ("cancel",)):
            refused(gpu_in(h, other, *args), 2, "~/peacock/chain-l on the VM is another tree's", "--dir")
        assert h.verbs().count("rsh") == rsh, "a refused call synced"
        assert h.docker_runs() == []
        assert snapshot(d) == before, "a refused call touched the dir"
        assert not (other / "testdata/benchmark-results/h100").exists(), "a refused pull brought a file home"
        held(h, "list")
        for args in (("ship",), ("cancel",), ("run", "list")):
            refused(gpu_in(h, other, *args), 2, "another tree's", "--dir")
        assert on_card(h) and len(h.docker_runs()) == 1, h.docker_runs()
        h.hold.unlink()
        until(lambda: h.gpu("status").stdout.startswith("exited 0"), "the first tree's run ended green")
        ok(gpu_in(h, other, "--dir", "other", "ship"))
        assert (h.vm_dir("other") / "cpp/install/lib/libpeacock_gpu.so").read_text() == "the other tree's build\n"
        assert (d / "cpp/install/lib/libpeacock_gpu.so").read_text() == "staged"


def test_a_dir_made_before_tree_is_claimed_by_the_first_tree():
    with Host() as h:
        other = tree_copy(h, "other")
        shipped(h)
        (h.vm_dir() / ".tree").unlink()
        old_log = h.vm_dir() / "logs" / "list.log"
        old_log.parent.mkdir()
        old_log.write_text("a run before the runner\n")
        r = ok(gpu_in(h, other, "cancel"))
        assert "claimed" in r.stdout, r.stdout
        assert owner(h) == recorded(other)[0], owner(h)
        assert old_log.read_text() == "a run before the runner\n"
        refused(h.gpu("ship"), 2, "another tree's", "--dir")
        r = ok(gpu_in(h, other, "ship"))
        assert "claimed" not in r.stdout, r.stdout


def test_a_fixed_tree_id_owns_its_dir_and_writes_no_record():
    with Host() as h:
        other = tree_copy(h, "other")
        (other / RECORD).write_text(f"{'0' * 32}\n/elsewhere\n")
        ok(h.gpu("--dir", "ci", "prepare", "--no-sf40", env=CI))
        ok(h.gpu("--dir", "ci", "ship", env=CI))
        ok(gpu_in(h, other, "--dir", "ci", "ship", env=CI))
        ok(gpu_in(h, other, "--dir", "ci", "cancel", env=CI))
        assert owner(h, "ci") == "ci", owner(h, "ci")
        assert not (h.tree / RECORD).exists(), "a fixed id wrote a record"
        assert (other / RECORD).read_text() == f"{'0' * 32}\n/elsewhere\n"
        calls = len(h.nebius_calls())
        for bad in ("", "a/b", "c i"):
            refused(h.gpu("--dir", "ci", "ship", env={"PEACOCK_TREE_ID": bad}), 2, "PEACOCK_TREE_ID")
        assert len(h.nebius_calls()) == calls, h.nebius_calls()[calls:]
        refused(h.gpu("--dir", "ci", "ship"), 2, "another tree's", "--dir")


def test_a_record_that_cannot_be_made_is_refused_before_any_vm_call():
    for make in ("dangling", "directory"):
        with Host() as h:
            if make == "dangling":
                (h.tree / RECORD).symlink_to(h.tree / "nowhere")
            else:
                (h.tree / RECORD).mkdir()
            refused(h.gpu("ship"), 2, RECORD)
            assert h.nebius_calls() == [], (make, h.nebius_calls())
            assert not (h.vm_dir() / ".tree").exists(), make


def test_the_vm_refuses_an_empty_tree_id():
    with Host() as h:
        script = h.tree / "scripts/env/run-gpu.sh"
        fn = 'eval "$(sed -n \'/^vm_tree_claimed() {/,/^}/p\' "$0")"; vm_tree_claimed chain-l ""'
        r = subprocess.run(["bash", "-c", fn, str(script)], env={"HOME": str(h.home), "PATH": os.environ["PATH"]},
                           capture_output=True, text=True, timeout=30)
        refused(r, 2, "no tree id")
        assert not (h.vm_dir() / ".tree").exists()


def test_status_and_follow_claim_nothing():
    with Host() as h:
        other = tree_copy(h, "other")
        r = h.gpu("status")
        assert r.returncode == 2 and r.stdout.startswith("none"), (r.returncode, r.stdout, r.stderr)
        refused(h.gpu("follow"), 2, "status")
        assert not (h.tree / RECORD).exists(), "status or follow made the record"
        assert not h.vm_dir().exists(), "status or follow made the dir"
        shipped(h)
        ok(h.gpu("run", "list"))
        r = ok(gpu_in(h, other, "status"))
        assert r.stdout.startswith("exited 0"), r.stdout
        r = ok(gpu_in(h, other, "follow"))
        assert "green" in r.stdout, r.stdout
        assert not (other / RECORD).exists(), "status or follow made the record"
        assert owner(h) == recorded(h.tree)[0], owner(h)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
