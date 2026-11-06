"""run-tests.sh in the run image, over fake binaries: which binaries each mode runs, with which
arguments and library path, and what fails a run. The fakes are shell scripts mounted at
/peacock/install; each records its argv and environment under /peacock/testdata/calls/. The
container runs as a uid with no passwd entry and no network.

Needs the run image (`scripts/env/image.sh build run 25.02`), so it is not in the cheap tier.
"""
import pathlib
import re
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
IMAGE = "ghcr.io/asymptote-tech/peacockdb-run:cudf-25.02"
SCRIPT = REPO / "docker" / "run-tests.sh"
LIBRARY_PATH = "/peacock/install/lib:/opt/cudf/lib"
THREADS = ["--nocapture", "--test-threads=1"]

# A fake test binary: records what it was given, prints its lines, then exits or kills itself.
FAKE = """#!/bin/bash
n=${0##*/}
for a in "$@"; do printf '%s\\n' "$a"; done > "/peacock/testdata/calls/$n.argv"
printf 'LD_LIBRARY_PATH=%s\\nPEACOCK_TESTDATA_DIR=%s\\nPEACOCK_RECORD_PATH=%s\\nPEACOCK_TPCH_VEC_PARAMS=%s\\n' \\
  "${LD_LIBRARY_PATH-unset}" "${PEACOCK_TESTDATA_DIR-unset}" "${PEACOCK_RECORD_PATH-unset}" "${PEACOCK_TPCH_VEC_PARAMS-unset}" \\
  > "/peacock/testdata/calls/$n.env"
[ -e /peacock/testdata/calibration/records.tsv ] && echo stale-record-present > "/peacock/testdata/calls/$n.record"
cat <<'OUT'
@OUT@
OUT
@END@
"""

RUST_OK = "running 2 tests\ntest a ... ok\ntest b ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored"
RUST_ZERO = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored"
CPP_OK = "[==========] 3 tests ran.\n[  PASSED  ] 3 tests."
CPP_ZERO = "[==========] 0 tests ran.\n[  PASSED  ] 0 tests."


def fake(out, end="exit 0"):
    return FAKE.replace("@OUT@", out).replace("@END@", end)


def the_list(**over):
    """The staged layout of a gpu build: the C++ suites with the sf40 pair, the four rust binaries."""
    files = {
        "bin/peacock_gpu_tests": fake(CPP_OK), "bin/peacock_cpu_tests": fake(CPP_OK),
        "bin/peacock_tpch_tests": fake(CPP_OK), "bin/peacock_tpchv_tests": fake(CPP_OK),
        "rust-tests/peacockdb_core_gpu_lib": fake(RUST_OK), "rust-tests/test_gpu_corpus": fake(RUST_OK),
        "rust-tests/test_node_timing": fake(RUST_OK), "rust-tests/peacock_gpu_benchmarks": fake(RUST_OK),
        "lib/libpeacock_gpu.so": "",
    }
    files.update(over)
    return {k: v for k, v in files.items() if v is not None}


class Run:
    def __init__(self, proc, calls, testdata):
        self.returncode, self.stdout, self.stderr = proc.returncode, proc.stdout, proc.stderr
        self.calls, self.testdata = calls, testdata

    def argv(self, name):
        return self.calls[name]["argv"]

    def env(self, name):
        return self.calls[name]["env"]


def run(*args, files, env=None, testdata_files=None, testdata_writable=True):
    with tempfile.TemporaryDirectory() as tmp:
        install, testdata = pathlib.Path(tmp) / "install", pathlib.Path(tmp) / "testdata"
        for rel, text in files.items():
            path = install / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            path.chmod(0o755)
        (testdata / "calls").mkdir(parents=True)
        for rel, text in (testdata_files or {}).items():
            (testdata / rel).parent.mkdir(parents=True, exist_ok=True)
            (testdata / rel).write_text(text)
        for p in [*testdata.rglob("*"), testdata]:
            p.chmod(0o777 if p.is_dir() else 0o666)
        if not testdata_writable:
            testdata.chmod(0o555)
        envs = [f"-e{k}={v}" for k, v in (env or {}).items()]
        proc = subprocess.run(
            ["docker", "run", "--rm", "--network", "none", "--user", "4242:4242", *envs,
             "-v", f"{install}:/peacock/install:ro", "-v", f"{testdata}:/peacock/testdata",
             "-v", f"{SCRIPT}:/opt/run/run-tests.sh:ro", IMAGE, *args],
            capture_output=True, text=True, timeout=300)
        calls = {}
        for f in (testdata / "calls").glob("*.argv"):
            name = f.name.removesuffix(".argv")
            env_lines = (testdata / "calls" / f"{name}.env").read_text().splitlines()
            calls[name] = {"argv": f.read_text().splitlines(),
                           "env": dict(line.split("=", 1) for line in env_lines),
                           "record": (testdata / "calls" / f"{name}.record").exists()}
        left = sorted(str(p.relative_to(testdata)) for p in testdata.rglob("*") if "calls" not in p.parts)
        testdata.chmod(0o777)
    return Run(proc, calls, left)


def test_list_runs_the_l40s_list_with_each_binary_s_arguments():
    r = run("list", files=the_list())
    assert r.returncode == 0, (r.stdout, r.stderr)
    assert sorted(r.calls) == ["peacock_cpu_tests", "peacock_gpu_benchmarks", "peacock_gpu_tests",
                               "peacockdb_core_gpu_lib", "test_gpu_corpus", "test_node_timing"], sorted(r.calls)
    assert r.argv("peacock_gpu_tests") == [] and r.argv("peacock_cpu_tests") == []
    assert r.argv("peacockdb_core_gpu_lib") == THREADS + ["gpu_tests::"]
    assert r.argv("test_gpu_corpus") == THREADS and r.argv("test_node_timing") == THREADS
    assert r.argv("peacock_gpu_benchmarks") == THREADS + ["--skip", "bench_"]
    assert "peacock_tpch_tests" in r.stdout and "sf40" in r.stdout, r.stdout


def test_only_the_binaries_get_the_library_path():
    r = run("list", files=the_list())
    assert r.returncode == 0, r.stderr
    for name, call in r.calls.items():
        assert call["env"]["LD_LIBRARY_PATH"] == LIBRARY_PATH, (name, call["env"])
        assert call["env"]["PEACOCK_TESTDATA_DIR"] == "/peacock/testdata", (name, call["env"])


def test_a_failing_binary_fails_the_run_and_every_other_still_runs():
    r = run("list", files=the_list(**{"rust-tests/test_gpu_corpus": fake("test result: FAILED. 1 passed; 1 failed", "exit 101")}))
    assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
    assert len(r.calls) == 6, sorted(r.calls)
    assert "test_gpu_corpus" in r.stderr, r.stderr


def test_a_binary_that_runs_zero_tests_fails_the_run():
    for rel, out in [("rust-tests/test_node_timing", RUST_ZERO), ("bin/peacock_gpu_tests", CPP_ZERO)]:
        r = run("list", files=the_list(**{rel: fake(out)}))
        name = rel.split("/")[1]
        assert r.returncode == 1, (rel, r.stdout, r.stderr)
        assert name in r.stderr and "0 tests" in r.stderr, r.stderr
        assert len(r.calls) == 6, sorted(r.calls)


def test_a_segfault_fails_the_run():
    r = run("list", files=the_list(**{"rust-tests/test_gpu_corpus": fake("running 1 test", "kill -SEGV $$")}))
    assert r.returncode == 1, (r.stdout, r.stderr)
    assert "test_gpu_corpus" in r.stderr and "SIGSEGV" in r.stderr, r.stderr
    assert len(r.calls) == 6, sorted(r.calls)


def test_a_segfault_in_a_log_fails_a_run_that_exited_zero():
    out = CPP_OK + "\nSegmentation fault (core dumped)"
    r = run("list", files=the_list(**{"bin/peacock_cpu_tests": fake(out)}))
    assert r.returncode == 1, (r.stdout, r.stderr)
    assert "peacock_cpu_tests" in r.stderr and "segfault" in r.stderr, r.stderr


def test_an_empty_list_is_refused_before_anything_runs():
    for gone, word in [(["bin/peacock_gpu_tests", "bin/peacock_cpu_tests", "bin/peacock_tpch_tests",
                         "bin/peacock_tpchv_tests"], "bin/peacock_*_tests"),
                       (["rust-tests/peacockdb_core_gpu_lib", "rust-tests/test_gpu_corpus",
                         "rust-tests/test_node_timing", "rust-tests/peacock_gpu_benchmarks"], "rust-tests"),
                       (["lib/libpeacock_gpu.so"], "libpeacock_gpu.so")]:
        r = run("list", files=the_list(**{g: None for g in gone}))
        assert r.returncode == 2 and r.calls == {}, (gone, r.returncode, r.stdout, r.stderr)
        assert r.stderr.startswith("run-tests.sh: ") and word in r.stderr, r.stderr
        assert len(r.stderr.strip().splitlines()) == 1, r.stderr


def test_filter_narrows_every_rust_binary_but_the_lib():
    r = run("filter", files=the_list(**{"rust-tests/test_node_timing": fake(RUST_ZERO)}), env={"PCK_TEST_FILTER": "q3"})
    assert r.returncode == 0, (r.stdout, r.stderr)
    assert r.argv("peacockdb_core_gpu_lib") == THREADS + ["gpu_tests::"]
    assert r.argv("test_gpu_corpus") == THREADS + ["q3"] and r.argv("test_node_timing") == THREADS + ["q3"]
    assert r.argv("peacock_gpu_benchmarks") == THREADS + ["--skip", "bench_", "q3"]
    assert "peacock_gpu_tests" in r.calls, "the C++ suites run under a filter too"


def test_a_filter_that_matches_nothing_fails():
    zero = {f"rust-tests/{n}": fake(RUST_ZERO) for n in ["test_gpu_corpus", "test_node_timing", "peacock_gpu_benchmarks"]}
    r = run("filter", files=the_list(**zero), env={"PCK_TEST_FILTER": "nomatch"})
    assert r.returncode == 1, (r.stdout, r.stderr)
    assert "nomatch" in r.stderr and "matched no test" in r.stderr, r.stderr


def test_the_lib_running_zero_tests_fails_under_a_filter():
    r = run("filter", files=the_list(**{"rust-tests/peacockdb_core_gpu_lib": fake(RUST_ZERO)}), env={"PCK_TEST_FILTER": "q3"})
    assert r.returncode == 1 and "peacockdb_core_gpu_lib" in r.stderr, (r.stdout, r.stderr)


def test_a_filter_with_nothing_but_the_lib_to_narrow_is_refused():
    only_lib = {f"rust-tests/{n}": None for n in ["test_gpu_corpus", "test_node_timing", "peacock_gpu_benchmarks"]}
    r = run("filter", files=the_list(**only_lib), env={"PCK_TEST_FILTER": "q3"})
    assert r.returncode == 2 and r.calls == {}, (r.returncode, r.stdout, r.stderr)
    assert "peacockdb_core_gpu_lib" in r.stderr and len(r.stderr.strip().splitlines()) == 1, r.stderr


def test_a_testdata_mount_that_is_not_writable_is_refused():
    for mode in ["list", "bench"]:
        r = run(mode, files={**the_list(), **BENCH}, testdata_writable=False)
        assert r.returncode == 2 and r.calls == {}, (mode, r.returncode, r.stdout, r.stderr)
        assert "/peacock/testdata" in r.stderr and "writable" in r.stderr, r.stderr


def test_an_exit_above_128_that_names_no_signal_is_an_exit():
    r = run("list", files=the_list(**{"rust-tests/test_gpu_corpus": fake(RUST_OK, "exit 255")}))
    assert r.returncode == 1, (r.stdout, r.stderr)
    assert "test_gpu_corpus exited 255" in r.stderr and "signal" not in r.stderr, r.stderr


def test_list_and_filter_refuse_the_other_s_filter():
    r = run("list", files=the_list(), env={"PCK_TEST_FILTER": "q3"})
    assert r.returncode == 2 and r.calls == {} and "PCK_TEST_FILTER" in r.stderr and "filter" in r.stderr, r.stderr
    r = run("filter", files=the_list())
    assert r.returncode == 2 and r.calls == {} and "PCK_TEST_FILTER" in r.stderr, r.stderr


BENCH = {"rust-benchmarks/peacock_gpu_benchmarks": fake(RUST_OK), "lib/libpeacock_gpu.so": ""}


def test_bench_runs_the_release_binary():
    r = run("bench", files=BENCH, testdata_files={"calibration/records.tsv": ""})
    assert r.returncode == 0, (r.stdout, r.stderr)
    assert sorted(r.calls) == ["peacock_gpu_benchmarks"], sorted(r.calls)
    call = r.calls["peacock_gpu_benchmarks"]
    assert call["argv"] == THREADS, call["argv"]
    assert call["env"] == {"LD_LIBRARY_PATH": LIBRARY_PATH, "PEACOCK_TESTDATA_DIR": "/peacock/testdata",
                           "PEACOCK_RECORD_PATH": "/peacock/testdata/calibration/records.tsv",
                           "PEACOCK_TPCH_VEC_PARAMS": "/peacock/testdata/tpch-vec-queries/query_params.jsonl"}, call["env"]
    assert not call["record"], "an empty record was still there when the benchmark started"
    assert "benchmark-results" in r.testdata, r.testdata


def test_bench_refuses_an_unpulled_record():
    r = run("bench", files=BENCH, testdata_files={"calibration/records.tsv": "rows of an earlier run\n"})
    assert r.returncode == 2 and r.calls == {}, (r.returncode, r.stdout, r.stderr)
    assert "calibration/records.tsv" in r.stderr and "pull it home, then remove it" in r.stderr, r.stderr
    assert len(r.stderr.strip().splitlines()) == 1, r.stderr


def test_bench_takes_the_filter():
    r = run("bench", files=BENCH, env={"PCK_TEST_FILTER": "bench_tpch_sf40_q6_tp1_single"})
    assert r.returncode == 0, r.stderr
    assert r.argv("peacock_gpu_benchmarks") == THREADS + ["bench_tpch_sf40_q6_tp1_single"]


def test_bench_that_runs_nothing_or_fails_is_red():
    for out, end in [(RUST_ZERO, "exit 0"), ("test result: FAILED. 0 passed; 1 failed", "exit 101")]:
        r = run("bench", files={**BENCH, "rust-benchmarks/peacock_gpu_benchmarks": fake(out, end)})
        assert r.returncode == 1 and "peacock_gpu_benchmarks" in r.stderr, (out, r.stdout, r.stderr)


def test_bench_without_the_benchmark_binary_is_refused():
    r = run("bench", files=the_list())
    assert r.returncode == 2 and r.calls == {} and "rust-benchmarks/peacock_gpu_benchmarks" in r.stderr, r.stderr


def test_bad_arguments_fail_in_one_line():
    for args in [(), ("run",), ("list", "extra")]:
        r = run(*args, files=the_list())
        assert r.returncode == 2 and r.calls == {}, (args, r.returncode, r.stderr)
        assert r.stderr.startswith("run-tests.sh: ") and len(r.stderr.strip().splitlines()) == 1, (args, r.stderr)


def test_help():
    r = run("--help", files=the_list())
    assert r.returncode == 0 and "run-tests.sh list" in r.stdout and "run-tests.sh bench" in r.stdout, r.stdout


def test_only_the_binaries_command_line_sets_the_library_path():
    """Exported, the prefix's libtinfo would load into bash itself; the binaries get it per command."""
    text = SCRIPT.read_text()
    lines = [line.strip() for line in text.splitlines() if "LD_LIBRARY_PATH" in line and not line.lstrip().startswith("#")]
    assert lines == ['env LD_LIBRARY_PATH="$LIBS" PEACOCK_TESTDATA_DIR="$TESTDATA" "$@" 2>&1 | tee "$log"'], lines
    assert not re.search(r"^\s*export\b", text, re.M), "run-tests.sh exports a variable"


def shell_value(text, name):
    (value,) = re.findall(rf"^{name}=(.*)$", text, re.M)
    return value.strip('"')


def test_the_list_rules_are_nebius_test_s():
    """scripts/nebius-test.sh is the old path's one list; the run image must run the same one."""
    old = (REPO / "scripts" / "nebius-test.sh").read_text()
    new = SCRIPT.read_text()
    assert shell_value(old, "SF40").split() == shell_value(new, "SF40").split()
    ((lib, rung),) = re.findall(r'\[ "\$name" = (\w+) \] && rung=(\S+)', old)
    assert (lib, rung) == (shell_value(new, "LIB_STAGED"), shell_value(new, "LIB_RUNG")), (lib, rung)
    (bench,) = re.findall(r'case \$name in (\w+)\) skip="--skip bench_"', old)
    assert bench == shell_value(new, "BENCH_TARGET") and "--skip bench_" in new, bench
    assert "--test-threads=1" in old and "args=(--nocapture --test-threads=1)" in new


def test_both_images_carry_the_committed_script():
    want = (REPO / "docker" / "run-tests.sh").read_text()
    for cudf in ["25.02", "26.02"]:
        proc = subprocess.run(["docker", "run", "--rm", "--network", "none", "--entrypoint", "cat",
                               f"ghcr.io/asymptote-tech/peacockdb-run:cudf-{cudf}", "/opt/run/run-tests.sh"],
                              capture_output=True, text=True, timeout=120)
        assert proc.returncode == 0 and proc.stdout == want, (cudf, proc.stderr)
        # The image's own entrypoint, unmounted, as a uid with no passwd entry.
        image = ["docker", "run", "--rm", "--network", "none", "--user", "4242:4242",
                 f"ghcr.io/asymptote-tech/peacockdb-run:cudf-{cudf}"]
        usage = subprocess.run([*image, "--help"], capture_output=True, text=True, timeout=120)
        assert usage.returncode == 0 and "run-tests.sh list" in usage.stdout, (cudf, usage)
        bare = subprocess.run(image, capture_output=True, text=True, timeout=120)
        assert bare.returncode == 2 and bare.stdout == "", (cudf, bare)
        assert bare.stderr.startswith("run-tests.sh: ") and len(bare.stderr.strip().splitlines()) == 1, (cudf, bare.stderr)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
