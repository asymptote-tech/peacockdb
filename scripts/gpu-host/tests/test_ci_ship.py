"""ci-ship.sh, with real rsync over a transport that runs the "remote" side in a local home dir.

What it must keep: lib/'s symlinks and the exec bits, a cache that persists across runs and
mirrors each one (a file the run lost is gone there too), a run dir that is a copy of the cache,
retries that end red with no run dir, and the wire flags.
"""
import os, pathlib, stat, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[1] / "ci-ship.sh")
TESTDATA_DIRS = ["tpch-queries", "tpcds-queries", "tpch.minimal", "goldens", "tpch-vec-queries",
                 "pbench-queries", "pbench.sf1", "fixtures"]
TESTDATA_FILES = ["generate_testdata.sh", "cost-registry.csv"]
# rsync's -e transport: drop the host, run the command with the fake VM home as cwd.
RSH = '#!/bin/sh\nshift\ncd "$FAKE_HOME" && exec sh -c "$*"\n'
# One remote command, with ~ meaning the fake VM home.
SSH = '#!/bin/sh\ncd "$FAKE_HOME" && HOME="$FAKE_HOME" exec sh -c "$*"\n'


def executable(path, body):
    path.write_text(body); path.chmod(path.stat().st_mode | stat.S_IEXEC)


def checkout():
    """A runner-side checkout: an unpacked cpp/install/, the testdata subset and the L40S list."""
    r = pathlib.Path(tempfile.mkdtemp(prefix="shiproot-"))
    lib = r / "cpp/install/lib"; lib.mkdir(parents=True)
    (lib / "libx.so.1.0").write_bytes(os.urandom(4096))
    (lib / "libx.so.1").symlink_to("libx.so.1.0"); (lib / "libx.so").symlink_to("libx.so.1")
    (r / "cpp/install/bin").mkdir(); executable(r / "cpp/install/bin/peacock_gpu_tests", "#!/bin/sh\n")
    (r / "cpp/install/rust-tests").mkdir(); executable(r / "cpp/install/rust-tests/test_gpu_corpus", "#!/bin/sh\n")
    for d in TESTDATA_DIRS:
        (r / "testdata" / d).mkdir(parents=True); (r / "testdata" / d / "a.txt").write_text(d)
    for f in TESTDATA_FILES:
        (r / "testdata" / f).write_text(f)
    (r / "scripts").mkdir(); executable(r / "scripts/gpu-host-test.sh", "#!/bin/sh\n")
    return r


def fakes(rsync=None):
    d = pathlib.Path(tempfile.mkdtemp(prefix="shipfakes-"))
    executable(d / "rsh", RSH); executable(d / "ssh", SSH)
    if rsync:
        executable(d / "rsync", rsync)
    return d


def run(root, run_dir="peacockdb-ci/run-1-1", home=None, rsync=None, attempts="2"):
    home = home or pathlib.Path(tempfile.mkdtemp(prefix="shiphome-"))
    f = fakes(rsync)
    env = {"PATH": "/usr/bin:/bin", "FAKE_HOME": str(home), "GPU_SHIP_ROOT": str(root),
           "GPU_SHIP_RSH": str(f / "rsh"), "GPU_SHIP_SSH": str(f / "ssh"),
           "GPU_SHIP_ATTEMPTS": attempts, "GPU_SHIP_PAUSE": "0", "FAKES": str(f)}
    if rsync:
        env["GPU_HOST_RSYNC"] = str(f / "rsync")
    p = subprocess.run([SCRIPT, run_dir], capture_output=True, text=True, timeout=120, env=env)
    return p.returncode, p.stdout + p.stderr, home, f


def test_run_dir_holds_the_tree_with_symlinks_and_exec_bits():
    rc, out, home, _ = run(checkout())
    assert rc == 0, out
    lib = home / "peacockdb-ci/run-1-1/cpp/install/lib"
    assert (lib / "libx.so").is_symlink() and os.readlink(lib / "libx.so") == "libx.so.1", out
    assert (lib / "libx.so.1").is_symlink(), out
    for exe in ("cpp/install/bin/peacock_gpu_tests", "cpp/install/rust-tests/test_gpu_corpus",
                "scripts/gpu-host-test.sh"):
        assert os.access(home / "peacockdb-ci/run-1-1" / exe, os.X_OK), exe
    for d in TESTDATA_DIRS:
        assert (home / "peacockdb-ci/run-1-1/testdata" / d / "a.txt").read_text() == d
    for f in TESTDATA_FILES:
        assert (home / "peacockdb-ci/run-1-1/testdata" / f).exists(), f


def test_the_cache_persists_and_mirrors_each_run():
    root = checkout()
    rc, out, home, _ = run(root)
    assert rc == 0, out
    (root / "testdata/goldens/a.txt").unlink(); (root / "testdata/goldens/b.txt").write_text("new")
    (root / "cpp/install/lib/libx.so.1.0").write_bytes(os.urandom(4096))
    rc, out, home, _ = run(root, run_dir="peacockdb-ci/run-2-1", home=home)
    assert rc == 0, out
    for tree in ("peacockdb-ci/cache", "peacockdb-ci/run-2-1"):
        goldens = home / tree / "testdata/goldens"
        assert sorted(p.name for p in goldens.iterdir()) == ["b.txt"], (tree, out)
        assert (home / tree / "cpp/install/lib/libx.so.1.0").read_bytes() == \
            (root / "cpp/install/lib/libx.so.1.0").read_bytes(), tree


def test_a_rerun_of_the_same_run_dir_starts_clean():
    root = checkout()
    rc, out, home, _ = run(root)
    assert rc == 0, out
    (home / "peacockdb-ci/run-1-1/stale").write_text("left by a test")
    rc, out, home, _ = run(root, home=home)
    assert rc == 0 and not (home / "peacockdb-ci/run-1-1/stale").exists(), out


def test_a_stream_that_keeps_failing_is_red_and_no_run_dir_is_made():
    rsync = '#!/bin/sh\necho "$*" >> "$FAKES/calls"\nexit 12\n'
    rc, out, home, f = run(checkout(), rsync=rsync, attempts="2")
    assert rc == 1, out
    calls = (f / "calls").read_text().splitlines()
    # Four streams, two attempts each; the testdata stream's second call is never reached.
    assert len(calls) == 8, calls
    assert "failed 2 times" in out and "::error::" in out, out
    assert not (home / "peacockdb-ci/run-1-1").exists(), out


def test_a_flaky_stream_recovers_on_retry():
    # The first call of all fails; every other call is real rsync.
    rsync = ('#!/bin/sh\nif mkdir "$FAKES/failed" 2>/dev/null; then exit 30; fi\n'
             'exec rsync "$@"\n')
    rc, out, home, _ = run(checkout(), rsync=rsync, attempts="2")
    assert rc == 0 and "attempt 1 failed" in out, out
    assert (home / "peacockdb-ci/run-1-1/testdata/goldens/a.txt").exists(), out


def test_every_call_compresses_resumes_and_mirrors():
    rsync = '#!/bin/sh\necho "$*" >> "$FAKES/calls"\n'
    rc, out, _, f = run(checkout(), rsync=rsync)
    assert rc == 0, out
    calls = (f / "calls").read_text().splitlines()
    assert len(calls) == 5, calls
    for c in calls:
        for flag in ("-a", "--checksum", "--delete", "--partial-dir=.rsync-partial",
                     "--compress-choice=zstd", "--timeout=120"):
            assert flag in c.split(), (flag, c)
    assert any("--exclude=/lib/" in c and "--exclude=/rust-tests/" in c for c in calls), calls


def test_refuses_a_run_dir_that_is_the_cache_or_outside_home():
    for bad in ("peacockdb-ci/cache", "peacockdb-ci/cache/x", "/tmp/x", "../x"):
        rc, out, home, _ = run(checkout(), run_dir=bad)
        assert rc == 2 and "refusing" in out, (bad, out)
        assert not (home / "peacockdb-ci").exists(), bad


def test_an_install_without_lib_is_refused_before_any_transfer():
    root = checkout()
    subprocess.run(["rm", "-rf", str(root / "cpp/install/lib")], check=True)
    rc, out, home, _ = run(root)
    assert rc == 2 and "unpack the binaries first" in out, out
    assert not (home / "peacockdb-ci").exists(), out


if __name__ == "__main__":
    raise SystemExit(main(globals()))
