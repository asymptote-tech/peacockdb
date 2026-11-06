"""gpu.sh: what a tree keeps of its runs, and the name it runs under.

The tree's instance record outside an instance, each bench's calibration record on the host, `pull`
bringing records and results home, and `pull --clear`. The rig is test_gpu.py's (`gpuhost.py`).
"""
import hashlib
import os
import pathlib
import re
import shutil
import signal
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpuhost import (INSTANCE, NAME, SLUG, GpuRig, dropped, ok, prepared, refused, ship_dir,  # noqa: E402
                     shipped, until)


RECORD = ".gpu-test/.instance"
TREE_ALONE = {"PEACOCK_INSTANCE": None}


def recorded(tree):
    """The tree's record: its instance's name, its id, its path."""
    name, tree_id, path = (tree / RECORD).read_text().splitlines()
    assert re.fullmatch(r"[0-9a-f]{32}", tree_id), tree_id
    return name, tree_id, path


def write_record(tree, name, tree_id="0" * 32, path=None):
    (tree / ".gpu-test").mkdir(exist_ok=True)
    (tree / RECORD).write_text(f"{name}\n{tree_id}\n{path or tree.resolve()}\n")


def owner(rig, instance):
    return (ship_dir(rig, instance) / ".tree").read_text().strip()


def test_a_tree_records_its_instance_and_later_commands_take_it():
    with GpuRig() as rig:
        record = rig.tree / RECORD
        refused(rig.gpu(NAME, "ship", env=TREE_ALONE), "--instance")
        refused(rig.gpu(NAME, "ship", "--instance", "a/b", env=TREE_ALONE), "a/b")
        assert not record.exists(), "a refused name was recorded"
        prepared(rig, "--instance", "dev2-B", env=TREE_ALONE)
        name, tree_id, path = recorded(rig.tree)
        assert (name, path) == ("dev2-B", str(rig.tree.resolve())), (name, path)
        assert owner(rig, "dev2-B") == tree_id, owner(rig, "dev2-B")
        for args in (("ship",), ("run", "list"), ("pull",), ("cancel",)):
            r = ok(rig.gpu(NAME, *args, env=TREE_ALONE))
            said = [ln for ln in r.stdout.splitlines() if RECORD in ln]
            assert len(said) == 1 and "dev2-B" in said[0], (args, r.stdout)
        assert (ship_dir(rig, "dev2-B") / "cpp/install/lib/libpeacock_gpu.so").is_file()
        assert [p.name for p in (rig.host_home() / "peacock").iterdir()] == ["dev2-B"]
        r = ok(rig.gpu(NAME, "ship", "--instance", "dev2-B", env=TREE_ALONE))
        assert RECORD not in r.stdout, r.stdout
        assert recorded(rig.tree) == (name, tree_id, path)


def test_an_instance_other_than_the_tree_s_record_is_refused_before_any_ssh():
    with GpuRig() as rig:
        record = rig.tree / RECORD
        write_record(rig.tree, "dev2-B")
        kept = record.read_text()
        for args in (("ship",), ("run", "list"), ("pull",), ("cancel",), ("prepare",)):
            refused(rig.gpu(NAME, *args, "--instance", "dev3-C", env=TREE_ALONE), "dev2-B", "dev3-C", RECORD)
        assert rig.calls("ssh") == [] and rig.calls("rsync") == [], rig.calls("ssh")
        assert record.read_text() == kept, record.read_text()
        record.unlink()
        prepared(rig, "--instance", "dev3-C", env=TREE_ALONE)
        assert recorded(rig.tree)[0] == "dev3-C", record.read_text()


def copy_of(rig):
    copy = rig.root / "copy"
    shutil.copytree(rig.tree, copy, symlinks=True)
    return copy


def gpu_in(rig, tree, *args, **kw):
    """gpu.sh run from <tree>, the rig's host and fakes unchanged."""
    own, rig.tree = rig.tree, tree
    try:
        return rig.gpu(*args, **kw)
    finally:
        rig.tree = own


def test_a_copied_tree_is_refused_the_record_it_carries():
    with GpuRig() as rig:
        prepared(rig, "--instance", "dev2-B", env=TREE_ALONE)
        ok(rig.gpu(NAME, "ship", env=TREE_ALONE))
        copy = copy_of(rig)
        calls = len(rig.calls("ssh"))
        for args in (("ship",), ("pull", "--clear"), ("ship", "--instance", "dev2-B")):
            refused(gpu_in(rig, copy, NAME, *args, env=TREE_ALONE), RECORD, str(rig.tree.resolve()), "--instance")
        assert len(rig.calls("ssh")) == calls, rig.calls("ssh")[calls:]
        (copy / RECORD).unlink()
        ok(gpu_in(rig, copy, NAME, "ship", "--instance", "dev3-C", env=TREE_ALONE))
        assert recorded(copy)[0] == "dev3-C" and recorded(copy)[1] != recorded(rig.tree)[1]
        ok(rig.gpu(NAME, "ship", env=TREE_ALONE))


def test_a_second_tree_under_a_taken_name_is_refused_on_the_host():
    with GpuRig() as rig:
        prepared(rig, "--instance", "dev2-B", env=TREE_ALONE)
        ok(rig.gpu(NAME, "ship", env=TREE_ALONE))
        ok(rig.gpu(NAME, "run", "bench", env=TREE_ALONE))
        d = ship_dir(rig, "dev2-B")
        before = {p: p.read_bytes() for p in d.rglob("*") if p.is_file()}
        other = copy_of(rig)
        (other / RECORD).unlink()
        (other / "cpp/install/lib/libpeacock_gpu.so").write_text("the other tree's build\n")
        synced = len(rig.calls("rsync"))
        for args in (("ship",), ("pull",), ("pull", "--clear"), ("run", "list"), ("cancel",), ("prepare",)):
            refused(gpu_in(rig, other, NAME, *args, "--instance", "dev2-B", env=TREE_ALONE), "dev2-B",
                    "another tree")
        assert not (other / RECORD).exists(), "a name the host refused was recorded"
        assert len(rig.calls("rsync")) == synced, rig.calls("rsync")[synced:]
        assert {p: p.read_bytes() for p in d.rglob("*") if p.is_file()} == before
        assert not list((other / "testdata").glob("calibration/records-*.tsv"))
        ok(rig.gpu(NAME, "pull", "--clear", env=TREE_ALONE))
        assert list((rig.tree / "testdata" / "calibration").glob("records-*.tsv"))


def test_an_instance_and_a_tree_outside_one_never_share_a_dir():
    with GpuRig() as rig:
        shipped(rig)
        assert owner(rig, INSTANCE) == f"instance:{INSTANCE}", owner(rig, INSTANCE)
        refused(rig.gpu(NAME, "ship", "--instance", INSTANCE, env=TREE_ALONE), INSTANCE, "another tree")
        prepared(rig, "--instance", "dev2-B", env=TREE_ALONE)
        (rig.tree / RECORD).unlink()
        refused(rig.gpu(NAME, "ship", env={"PEACOCK_INSTANCE": "dev2-B"}), "dev2-B", "another tree")


def test_an_instance_neither_reads_nor_writes_the_tree_s_record():
    with GpuRig() as rig:
        shipped(rig)
        assert not (rig.tree / ".gpu-test").exists(), sorted((rig.tree / ".gpu-test").rglob("*"))
        record = rig.tree / RECORD
        write_record(rig.tree, "dev2-B", path="/elsewhere")
        r = ok(rig.gpu(NAME, "ship"))
        assert RECORD not in r.stdout + r.stderr, (r.stdout, r.stderr)
        assert [p.name for p in (rig.host_home() / "peacock").iterdir()] == [INSTANCE]
        refused(rig.gpu(NAME, "ship", "--instance", "dev2-B"), "--instance", INSTANCE)
        assert record.read_text().startswith("dev2-B\n"), record.read_text()


def test_each_bench_run_keeps_its_own_record():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "bench"))
        ok(rig.gpu(NAME, "run", "bench"))
        on_host = ship_dir(rig) / "testdata" / "calibration"
        records = sorted(on_host.glob("records-*.tsv"))
        assert len(records) == 2 and not (on_host / "records.tsv").exists(), list(on_host.iterdir())
        assert records[0].read_text() != records[1].read_text()
        ok(rig.gpu(NAME, "pull"))
        local = rig.tree / "testdata" / "calibration"
        for record in records:
            assert (local / record.name).read_bytes() == record.read_bytes(), record.name
        (local / records[0].name).write_text("rows from somewhere else\n")
        os.utime(local / records[0].name, (1_000_000_000, 1_000_000_000))
        r = rig.gpu(NAME, "pull")
        assert r.returncode == 1 and records[0].name in r.stderr, (r.returncode, r.stderr)
        assert (local / records[0].name).read_text() == "rows from somewhere else\n"


def test_a_killed_bench_s_record_comes_home():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold").write_text("")
        dropped(rig, "bench")
        pid, _, _, stamp, *_ = (ship_dir(rig) / "run" / "state").read_text().split()
        os.killpg(int(pid), signal.SIGKILL)
        until(lambda: not rig.is_locked(rig.data_dir() / ".device.lock"), "the killed run let go of the card")
        (rig.host / "device.busy").unlink()  # the killed container is gone from the card
        (rig.host / "hold").unlink()
        for container in (rig.host / "containers").iterdir():
            container.unlink()  # the killed run's container is gone from the host too
        on_host = ship_dir(rig) / "testdata" / "calibration"
        assert (on_host / "records.tsv").is_file(), "the killed run left its record at the run's path"
        ok(rig.gpu(NAME, "pull"))
        interrupted = on_host / f"records-{stamp}-interrupted.tsv"
        assert interrupted.is_file() and not (on_host / "records.tsv").exists(), sorted(on_host.iterdir())
        assert (rig.tree / "testdata" / "calibration" / interrupted.name).read_bytes() == interrupted.read_bytes()
        r = rig.gpu(NAME, "run", "bench")
        assert r.returncode == 0, (r.returncode, r.stderr)
        ok(rig.gpu(NAME, "pull"))
        assert len(list((rig.tree / "testdata" / "calibration").glob("records-*.tsv"))) == 2


def test_pull_clear_removes_the_host_s_records_that_are_home_and_keeps_the_rest():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "bench"))
        on_host = ship_dir(rig) / "testdata" / "calibration"
        (record,) = on_host.glob("records-*.tsv")
        r = ok(rig.gpu(NAME, "pull", "--clear"))
        local = rig.tree / "testdata" / "calibration"
        assert (local / record.name).read_text().startswith("# run: the host's record of"), local
        assert not record.exists(), sorted(on_host.iterdir())
        assert any(record.name in ln and "removed" in ln for ln in r.stdout.splitlines()), r.stdout

        ok(rig.gpu(NAME, "run", "bench"))
        ok(rig.gpu(NAME, "run", "bench"))
        differs, same = sorted(on_host.glob("records-*.tsv"))
        same_bytes = same.read_bytes()
        (local / differs.name).write_text("rows from somewhere else\n")
        r = rig.gpu(NAME, "pull", "--clear")
        assert r.returncode == 1 and differs.name in r.stderr, (r.returncode, r.stderr)
        assert sorted(p.name for p in on_host.iterdir()) == [differs.name], sorted(on_host.iterdir())
        assert (local / same.name).read_bytes() == same_bytes
        assert (local / differs.name).read_text() == "rows from somewhere else\n"
        lines = r.stdout.splitlines()
        assert any(same.name in ln and "removed" in ln for ln in lines), r.stdout
        assert any(differs.name in ln and "kept" in ln and "differs" in ln for ln in lines), r.stdout


def test_pull_clear_is_refused_while_the_instance_s_run_goes():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "bench"))
        (rig.host / "hold").write_text("")
        run = rig.gpu(NAME, "run", "bench", background=True)
        on_host = ship_dir(rig) / "testdata" / "calibration"
        until(lambda: (on_host / "records.tsv").exists(), "the run writes its record")
        before = sorted(p.name for p in on_host.iterdir())
        synced = len(rig.calls("rsync"))
        refused(rig.gpu(NAME, "pull", "--clear"), "still going", "--clear")
        assert len(rig.calls("rsync")) == synced, rig.calls("rsync")[synced:]
        assert sorted(p.name for p in on_host.iterdir()) == before, sorted(on_host.iterdir())
        (rig.host / "hold").unlink()
        assert run.wait(timeout=60) == 0, run.communicate()


def test_the_host_s_clear_is_refused_while_the_instance_s_run_goes():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "bench"))
        on_host = ship_dir(rig) / "testdata" / "calibration"
        (record,) = on_host.glob("records-*.tsv")
        (rig.host / "hold").write_text("")
        run = rig.gpu(NAME, "run", "bench", background=True)
        until(lambda: (on_host / "records.tsv").exists(), "the run writes its record")
        sums = rig.root / "sums"
        sums.write_text(f"{hashlib.sha256(record.read_bytes()).hexdigest()} {record.name}\n")
        with sums.open() as stdin:
            r = rig.on_host("clear", stdin=stdin)
        assert r.returncode == 2 and "still going" in r.stderr, (r.returncode, r.stdout, r.stderr)
        assert record.exists() and (on_host / "records.tsv").exists(), sorted(on_host.iterdir())
        (rig.host / "hold").unlink()
        assert run.wait(timeout=60) == 0, run.communicate()


def test_the_host_s_clear_keeps_records_tsv_and_what_did_not_come_home():
    with GpuRig() as rig:
        shipped(rig)
        on_host = ship_dir(rig) / "testdata" / "calibration"
        on_host.mkdir(parents=True)
        for name in ("records.tsv", "records-a.tsv", "records-b.tsv"):
            (on_host / name).write_text(f"rows of {name}\n")
        sums = rig.root / "sums"
        sums.write_text("".join(f"{hashlib.sha256((on_host / n).read_bytes()).hexdigest()} {n}\n"
                                for n in ("records.tsv", "records-a.tsv")))
        with sums.open() as stdin:
            r = rig.on_host("clear", stdin=stdin)
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        assert sorted(p.name for p in on_host.iterdir()) == ["records-b.tsv", "records.tsv"], sorted(on_host.iterdir())
        lines = r.stdout.splitlines()
        assert any("records-a.tsv" in ln and "removed" in ln for ln in lines), r.stdout
        assert any("records-b.tsv" in ln and "kept" in ln and "not pulled home" in ln for ln in lines), r.stdout
        assert any(ln.startswith("kept records.tsv") for ln in lines), r.stdout


def test_pull_brings_the_results_home_and_keeps_a_newer_local_file():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "pull"))
        assert not (rig.tree / "testdata" / "benchmark-results").exists(), "a host with no run brings nothing"
        ok(rig.gpu(NAME, "run", "bench"))
        result = pathlib.Path("testdata/benchmark-results") / SLUG / "tpch.sf40" / "tp1-single.benchmark.txt"
        ok(rig.gpu(NAME, "pull"))
        assert (rig.tree / result).read_text() == "q1 measured on the host\n"
        logs = sorted((rig.tree / ".gpu-test" / NAME / "logs").iterdir())
        assert logs[0].name.startswith("bench-") and "green" in logs[0].read_text(), logs

        (rig.tree / result).write_text("edited here, after the pull\n")
        future = time.time() + 3600
        os.utime(rig.tree / result, (future, future))
        r = rig.gpu(NAME, "pull")
        assert r.returncode == 1 and str(result) in r.stderr, (r.returncode, r.stderr)
        assert (rig.tree / result).read_text() == "edited here, after the pull\n"
        os.utime(rig.tree / result, (1_000_000_000, 1_000_000_000))
        ok(rig.gpu(NAME, "pull"))
        assert (rig.tree / result).read_text() == "q1 measured on the host\n"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
