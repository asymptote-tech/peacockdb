"""gpu-host-test.sh's list, over stub binaries: what runs, with what, and what is red."""
import os, pathlib, stat, subprocess, sys, tempfile, time
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "gpu-host-test.sh")
GTEST_OK = 'echo "$0 $*" >> "$TREE/argv"; echo "[  PASSED  ] 3 tests."'
GTEST_ZERO = 'echo "$0 $*" >> "$TREE/argv"; echo "[  PASSED  ] 0 tests."'
RUST_OK = 'echo "$0 $*" >> "$TREE/argv"; echo "running 2 tests"; echo "test result: ok. 2 passed; 0 failed"'
RUST_FAIL = 'echo "$0 $*" >> "$TREE/argv"; echo "running 1 tests"; echo "test result: FAILED. 0 passed; 1 failed"; exit 101'


def tree(bins=None, rust=None):
    t = pathlib.Path(tempfile.mkdtemp(prefix="gputree-"))
    (t / "cpp/install/bin").mkdir(parents=True); (t / "cpp/install/rust-tests").mkdir(parents=True)
    (t / "testdata").mkdir()
    bins = bins if bins is not None else {"peacock_gpu_tests": GTEST_OK, "peacock_plan_tests": GTEST_OK,
                                           "peacock_tpch_tests": GTEST_OK, "peacock_tpchv_tests": GTEST_OK}
    rust = rust if rust is not None else {"test_gpu_corpus": RUST_OK, "peacockdb_core_gpu_lib": RUST_OK,
                                          "peacock_gpu_benchmarks": RUST_OK, "test_node_timing": RUST_OK}
    for d, group in (("bin", bins), ("rust-tests", rust)):
        for name, body in group.items():
            f = t / "cpp/install" / d / name
            f.write_text("#!/bin/bash\n" + body + "\n"); f.chmod(f.stat().st_mode | stat.S_IEXEC)
    return t


def run(t, *extra, env=None):
    p = subprocess.run([SCRIPT, "run", "--foreground", *extra, str(t)], capture_output=True, text=True,
                       timeout=60, env={"PATH": "/usr/bin:/bin", "TREE": str(t), "HOME": str(t), **(env or {})})
    return p.returncode, p.stdout + p.stderr, (t / "argv").read_text() if (t / "argv").exists() else ""


def test_sf40_pair_never_runs():
    rc, out, argv = run(tree())
    assert rc == 0, out
    assert "peacock_tpch_tests" not in argv and "peacock_tpchv_tests" not in argv


def test_rust_args():
    rc, out, argv = run(tree())
    lines = argv.splitlines()
    assert any(l.endswith("peacock_gpu_benchmarks --nocapture --test-threads=1 --skip bench_") for l in lines), argv
    assert any("peacockdb_core_gpu_lib --nocapture --test-threads=1 gpu_tests::" in l for l in lines), argv
    assert any(l.endswith("test_gpu_corpus --nocapture --test-threads=1") for l in lines), argv


def test_failure_is_red_and_the_rest_still_run():
    t = tree(rust={"test_gpu_corpus": RUST_FAIL, "test_node_timing": RUST_OK})
    rc, out, argv = run(t)
    assert rc != 0 and "test_node_timing" in argv
    assert (t / ".gpu-test/test_gpu_corpus.rc").read_text().strip() != "0"


def test_zero_tests_is_red():
    rc, out, _ = run(tree(bins={"peacock_gpu_tests": GTEST_ZERO}))
    assert rc != 0 and "ran 0 tests" in out


def test_missing_binary_dir_is_red():
    t = tree(bins={}, rust={})
    rc, out, _ = run(t)
    assert rc != 0 and "no peacock_*_tests" in out


def test_status_after_run():
    t = tree()
    run(t)
    p = subprocess.run([SCRIPT, "status", str(t)], capture_output=True, text=True, timeout=30)
    assert "ALL rc=0" in p.stdout, p.stdout


def test_status_says_died_when_the_run_pid_is_dead():
    t = tree()
    st = t / ".gpu-test"; st.mkdir()
    (st / "peacock_gpu_tests.log").write_text("[ RUN      ] x\n")
    dead = subprocess.Popen(["true"]); dead.wait()  # a pid that existed and is gone
    boot = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text().strip()
    (st / "run.pid").write_text(f"{dead.pid} {boot}\n")
    p = subprocess.run([SCRIPT, "status", str(t)], capture_output=True, text=True, timeout=30)
    assert "ALL died" in p.stdout and "ALL running" not in p.stdout, p.stdout
    assert p.returncode != 0


def test_status_says_died_when_the_boot_id_differs():
    t = tree()
    st = t / ".gpu-test"; st.mkdir()
    (st / "peacock_gpu_tests.log").write_text("[ RUN      ] x\n")
    (st / "run.pid").write_text(f"{os.getpid()} not-this-boot\n")  # the pid is alive, the boot is not ours
    p = subprocess.run([SCRIPT, "status", str(t)], capture_output=True, text=True, timeout=30)
    assert "ALL died" in p.stdout and p.returncode != 0, p.stdout


def test_status_says_running_when_the_run_pid_is_alive():
    t = tree()
    st = t / ".gpu-test"; st.mkdir()
    (st / "peacock_gpu_tests.log").write_text("[ RUN      ] x\n")
    boot = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text().strip()
    (st / "run.pid").write_text(f"{os.getpid()} {boot}\n")  # alive, and ours to signal
    p = subprocess.run([SCRIPT, "status", str(t)], capture_output=True, text=True, timeout=30)
    assert "ALL running" in p.stdout and p.returncode == 0, p.stdout


def test_detached_run_records_pid_and_boot_id():
    t = tree()
    p = subprocess.run([SCRIPT, "run", str(t)], capture_output=True, text=True, timeout=30,
                       env={"PATH": "/usr/bin:/bin", "TREE": str(t), "HOME": str(t)})
    assert p.returncode == 0, p.stderr
    boot = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text().strip()
    pid, got_boot = (t / ".gpu-test/run.pid").read_text().split()
    assert got_boot == boot and pid.isdigit()
    for _ in range(100):  # let the detached run finish before the tree is dropped
        if (t / ".gpu-test/ALL.rc").exists():
            break
        time.sleep(0.1)


def test_binaries_see_the_library_path_but_the_script_does_not_carry_it():
    t = tree(bins={"peacock_gpu_tests": 'echo "LD=$LD_LIBRARY_PATH TD=$PEACOCK_TESTDATA_DIR" >> "$TREE/seen";'
                                         ' echo "[  PASSED  ] 3 tests."'}, rust={"test_gpu_corpus": RUST_OK})
    shim = t / "shim"; shim.mkdir()  # a grep that records the environment the script itself carries
    g = shim / "grep"
    g.write_text('#!/bin/bash\necho "LD=${LD_LIBRARY_PATH-unset}" >> "$TREE/script_env"\nexec /usr/bin/grep "$@"\n')
    g.chmod(g.stat().st_mode | stat.S_IEXEC)
    rc, out, _ = run(t, env={"PATH": f"{shim}:/usr/bin:/bin", "GPU_HOST_CUDF_LIB": "/cudf/lib"})
    assert rc == 0, out
    assert (t / "seen").read_text().strip() == f"LD={t}/cpp/install/lib:/cudf/lib TD={t}/testdata"
    envs = set((t / "script_env").read_text().split())
    assert envs == {"LD=unset"}, envs


if __name__ == "__main__":
    raise SystemExit(main(globals()))
