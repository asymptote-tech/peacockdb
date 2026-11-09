"""`run-gpu.sh` and the VM's reach: a sync is retried whole when its transport drops.

gpu_host.py has the fakes; test_run_gpu.py has the verbs themselves.
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import HOSTNAME, Host  # noqa: E402


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def test_a_sync_whose_transport_drops_is_retried():
    with Host() as h:
        r = ok(h.gpu("prepare", "--no-sf40", env={"FAKE_NEBIUS_RSH_FAILS": "1"}))
        assert "rsync: attempt 1 of 4 exited 255" in r.stderr, r.stderr
        assert (h.vm_dir() / "scripts" / "env" / "fetch-sf40.sh").is_file()
        ok(h.gpu("ship"))
        t = h.vm_dir() / "testdata" / "benchmark-results" / "h100" / "tpch.sf40"
        t.mkdir(parents=True)
        (t / "tp1_single.benchmark.txt").write_text(f"host={HOSTNAME}\n")
        drops = h.verbs().count("rsh") + 1
        r = ok(h.gpu("pull", env={"FAKE_NEBIUS_RSH_FAILS": str(drops)}))
        assert "rsync: attempt 1 of 4 exited 255" in r.stderr, r.stderr
        home = h.tree / "testdata" / "benchmark-results" / "h100" / "tpch.sf40" / "tp1_single.benchmark.txt"
        assert home.read_text() == f"host={HOSTNAME}\n"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
