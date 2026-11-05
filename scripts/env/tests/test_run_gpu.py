"""`run-gpu.sh`: the device run on a VM, over scripts/nebius.sh alone (gpu_host.py has the fakes).

And `create_nsys_profile.sh --ssh`: every remote call of the Nsight capture through the given
command, the way run-gpu.sh reaches a VM through `nebius.sh … rsh`.
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import BUILD_REF, DRIVER_MINIMUM, H100, HOSTNAME, RUN_REF, Host, label, log_lines  # noqa: E402
from machine import KEYS, REPO, lib  # noqa: E402

VERBS = [("prepare",), ("ship",), ("run", "list"), ("pull",)]
SECRETS = [KEYS["AWS_ACCESS_KEY_ID"], KEYS["AWS_SECRET_ACCESS_KEY"]]


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, code, *words):
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    for w in words:
        assert w in r.stderr, (w, r.stderr)


def data_hash(h):
    return lib(h.tree, "data-inputs.sh", f'data_inputs "{h.tree}"')


def prepared(h, *opts):
    ok(h.gpu(*opts, "prepare"))
    return h


def nothing_bypassed(h):
    assert not h.dev_log.exists(), h.dev_log.read_text()


# --- the VM is nebius.sh's ---------------------------------------------------------------------

def test_run_gpu_passes_instance_and_profile_to_every_nebius_call():
    with Host() as h:
        for verb in VERBS:
            ok(h.gpu(*H100, *verb))
        calls = h.nebius_calls()
        assert {v for v in h.verbs()} == {"ssh", "rsh"}, h.verbs()
        for a in calls:
            assert a[:4] == H100, a
        nothing_bypassed(h)


def test_run_gpu_without_an_instance_id_is_refused():
    # nebius.sh without --instance-id is nebius-gpu, the shared L40S: no sf40, no benchmarks.
    with Host() as h:
        for verb in VERBS:
            refused(h.gpu(*verb, vm=None), 2, "--instance-id")
            refused(h.gpu("--profile", "editor-eu", *verb, vm=None), 2, "--instance-id")
        assert h.nebius_calls() == []
        nothing_bypassed(h)


def test_run_gpu_bad_instance_id_is_2():
    with Host() as h:
        for verb in VERBS:
            r = h.gpu("--instance-id", "computeinstance-nope", *verb, env={"FAKE_NEBIUS_RC": "64"})
            refused(r, 2)
        assert "rsh" not in h.verbs(), h.verbs()
        assert h.docker_calls() == [] and h.fetches() == []


def test_run_gpu_passes_on_nebius_shs_own_codes():
    with Host() as h:
        for rc in (69, 75):
            r = h.gpu(*H100, "run", "list", env={"FAKE_NEBIUS_RC": str(rc)})
            assert r.returncode == rc, (rc, r.returncode, r.stderr)


def test_run_gpu_never_starts_or_stops_the_vm():
    with Host() as h:
        for verb in VERBS:
            ok(h.gpu(*H100, *verb))
        verbs = h.verbs()
        assert "start" not in verbs and "stop" not in verbs, verbs


def test_run_gpu_refuses_an_unknown_verb_or_option():
    with Host() as h:
        refused(h.gpu("teleport"), 2, "teleport")
        refused(h.gpu("--color", "red", "run", "list"), 2, "--color")
        refused(h.gpu("run", "everything"), 2, "everything")
        refused(h.gpu("--dir", "../up", "ship"), 2, "--dir")
        refused(h.gpu("--cudf", "24.12", "prepare"), 2, "24.12")
        assert h.nebius_calls() == []


# --- prepare -----------------------------------------------------------------------------------

def test_prepare_readies_the_vm():
    with Host() as h:
        r = ok(h.gpu(*H100, "prepare"))
        assert "tpch.sf40: fetched" in r.stdout and "tpcds.sf40: fetched" in r.stdout, r.stdout
        conf = h.root / "etc/modprobe.d/peacock-profiling.conf"
        assert conf.read_text() == "options nvidia NVreg_RestrictProfilingToAdminUsers=0\n", conf.read_text()
        sets = h.data / data_hash(h)
        assert (sets / "tpch.sf1" / "generated").is_file() and (sets / "tpcds.sf1").is_dir(), list(h.data.iterdir())
        pulled = [a[-1] for a in h.docker_calls() if a[0] == "pull"]
        assert set(pulled) == {RUN_REF, BUILD_REF}, pulled
        cudf = h.home / "peacock" / "cudf-25.02"
        assert (cudf / "lib" / "libcudf.so").read_text() == RUN_REF
        assert (cudf / ".image-id").read_text().strip() == "sha256:run1"
        assert not any("update-initramfs" in ln or "apt-get" in ln for ln in h.host_lines()), h.host_lines()
        nothing_bypassed(h)


def test_prepare_twice_copies_the_cudf_prefix_once():
    with Host() as h:
        prepared(h)
        prepared(h)
        assert [a[0] for a in h.docker_calls()].count("cp") == 1, h.docker_calls()
        h.world(images={**h.world()["images"],
                        RUN_REF: {**h.world()["images"][RUN_REF], "id": "sha256:run2"}})
        prepared(h)
        assert [a[0] for a in h.docker_calls()].count("cp") == 2
        assert (h.home / "peacock" / "cudf-25.02" / ".image-id").read_text().strip() == "sha256:run2"


def test_prepare_sends_keys_on_stdin_only():
    with Host() as h:
        prepared(h, *H100)
        fetches = h.fetches()
        assert len(fetches) == 1, fetches
        assert fetches[0]["argv"][:2] == ["--env", "-"], fetches
        assert set(fetches[0]["argv"][2:]) == {"tpch.sf40", "tpcds.sf40"}, fetches
        for secret in SECRETS:
            assert secret in fetches[0]["stdin"]
        argvs = [json.dumps(a) for a in h.nebius_calls() + h.docker_calls()] + h.host_lines()
        argvs += [json.dumps(f["argv"]) for f in fetches]
        for secret in SECRETS:
            assert not any(secret in a for a in argvs), secret


def test_prepare_refuses_an_env_file_without_the_keys():
    with Host() as h:
        (h.laptop / ".config/peacock/peacock.env").write_text("AWS_REGION=x\n")
        refused(h.gpu("prepare"), 2, "AWS_ACCESS_KEY_ID")
        assert h.nebius_calls() == []


def test_prepare_says_restart_when_counters_are_admin_only():
    with Host() as h:
        h.params.write_text("RmProfilingAdminOnly: 1\n")
        r = h.gpu(*H100, "prepare")
        refused(r, 4, "restart")
        assert any(ln.startswith("update-initramfs -u") for ln in h.host_lines()), h.host_lines()
        conf = h.root / "etc/modprobe.d/peacock-profiling.conf"
        assert "NVreg_RestrictProfilingToAdminUsers=0" in conf.read_text()
        assert h.fetches() == [] and not (h.home / "peacock" / "cudf-25.02").exists()


def test_prepare_refuses_stale_run_image():
    with Host() as h:
        images = h.world()["images"]
        images[RUN_REF]["labels"]["peacockdb.inputs"] = label(h.tree, "run", change="docker/run-tests.sh")
        h.world(images=images)
        r = h.gpu(*H100, "prepare")
        refused(r, 2, "docker/run-tests.sh", RUN_REF)
        assert h.fetches() == [] and not (h.home / "peacock" / "cudf-25.02").exists()


def test_prepare_refuses_stale_build_image():
    with Host() as h:
        images = h.world()["images"]
        images[BUILD_REF]["labels"]["peacockdb.inputs"] = label(h.tree, "build", change="docker/venv.lock")
        h.world(images=images)
        refused(h.gpu("prepare"), 2, "docker/venv.lock", BUILD_REF)
        assert not any(h.data.iterdir())


def test_prepare_refuses_old_driver():
    with Host() as h:
        h.world(driver="530.30.02")
        refused(h.gpu("prepare"), 3, "530.30.02", DRIVER_MINIMUM)
        assert h.fetches() == []


# --- ship --------------------------------------------------------------------------------------

def test_ship_dir_is_named_by_dir():
    with Host() as h:
        prepared(h, "--dir", "pinned-loads")
        ok(h.gpu("--dir", "pinned-loads", "ship"))
        d = h.vm_dir("pinned-loads")
        assert (d / "cpp/install/lib/libpeacock_gpu.so").is_file()
        assert (d / "testdata/goldens/tpch.sf1/q01.txt").read_text() == "golden\n"
        assert sorted(p.name for p in (h.home / "peacock").iterdir()) == ["cudf-25.02", "pinned-loads"]


def test_ship_defaults_to_the_staged_install():
    with Host() as h:
        prepared(h)
        other = Host.stage_install(h.tree.parent / "elsewhere", mark="other")
        ok(h.gpu("ship"))
        installed = h.vm_dir() / "cpp/install"
        assert (installed / "lib/libpeacock_gpu.so").read_text() == "staged"
        ok(h.gpu("ship", "--install", other))
        assert (installed / "lib/libpeacock_gpu.so").read_text() == "other"
        shutil.rmtree(h.tree / "cpp" / "install")
        before = len(h.nebius_calls())
        refused(h.gpu("ship"), 2, str(h.tree / "cpp" / "install"))
        refused(h.gpu("ship", "--install", h.tree / "nowhere"), 2, str(h.tree / "nowhere"))
        assert len(h.nebius_calls()) == before


def test_ship_sends_the_committed_testdata_and_links_the_data():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        t = h.vm_dir() / "testdata"
        assert (t / "tpch-queries/q01.sql").is_file() and (t / "cost-registry.csv").is_file()
        assert (t / "generate_testdata.sh").is_file()
        hashed = h.data / data_hash(h)
        for ds in ("tpch.sf1", "tpcds.sf1"):
            assert os.readlink(t / ds) == str(hashed / ds), os.readlink(t / ds)
        for ds in ("tpch.sf40", "tpcds.sf40"):
            assert os.readlink(t / ds) == str(h.data / ds), os.readlink(t / ds)
        assert not (hashed / "tpch.sf1" / "lineitem.parquet").exists(), "the tree's own dataset shipped"


def test_ship_before_prepare_says_so():
    with Host() as h:
        refused(h.gpu("ship"), 2, "prepare")


def test_ship_keeps_what_a_run_wrote():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        result = h.vm_dir() / "testdata/benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt"
        result.parent.mkdir(parents=True)
        result.write_text("host=h100\n")
        stale = h.vm_dir() / "cpp/install/rust-tests/renamed_away"
        stale.write_text("old")
        ok(h.gpu("ship"))
        assert result.is_file() and not stale.exists()


# --- run ---------------------------------------------------------------------------------------

def test_run_refuses_old_driver():
    with Host() as h:
        prepared(h)
        h.world(driver="530.30.02")
        calls = len(h.nebius_calls())
        refused(h.gpu("run", "list"), 3, "530.30.02", DRIVER_MINIMUM)
        assert "rsh" not in h.verbs()[calls:] and h.docker_runs() == []


def test_run_before_prepare_says_so():
    with Host() as h:
        images = h.world()["images"]
        del images[RUN_REF]
        h.world(images=images)
        refused(h.gpu("run", "list"), 2, "prepare", RUN_REF)
        assert h.docker_runs() == []


def test_run_refuses_a_run_image_built_from_other_inputs():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        images = h.world()["images"]
        images[RUN_REF]["labels"]["peacockdb.inputs"] = label(h.tree, "run", change="docker/run-tests.sh")
        h.world(images=images)
        refused(h.gpu("run", "list"), 2, "docker/run-tests.sh", RUN_REF)
        assert h.docker_runs() == []


def test_run_checks_the_run_image_without_pulling_it():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        before = len(h.docker_calls())
        ok(h.gpu("run", "list"))
        run_calls = h.docker_calls()[before:]
        assert not any(a[0] == "pull" for a in run_calls), run_calls
        inspected = [a for a in run_calls if a[:2] == ["image", "inspect"] and "peacockdb.inputs" in " ".join(a)]
        assert inspected and inspected[0][-1] == RUN_REF, run_calls


def test_run_passes_the_machines_hostname():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        r = ok(h.gpu("run", "list"))
        assert "run-tests.sh list: green" in r.stdout
        runs = h.docker_runs()
        assert len(runs) == 1, runs
        a = runs[0]
        assert a[a.index("--hostname") + 1] == HOSTNAME, a
        assert a[a.index("--name") + 1] == "peacock-chain-l"
        assert "--gpus" in a and a[a.index("--gpus") + 1] == "all"
        assert a[a.index("--user") + 1] == f"{os.getuid()}:{os.getgid()}"
        assert a[-2:] == [RUN_REF, "list"], a


def test_run_mounts_the_data_dir_at_its_own_path():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        ok(h.gpu("run", "list"))
        a = h.docker_runs()[0]
        mounts = [a[i + 1] for i, x in enumerate(a) if x == "-v"]
        d = h.vm_dir()
        assert f"{d}/cpp/install:/peacock/install:ro" in mounts, mounts
        assert f"{d}/testdata:/peacock/testdata" in mounts, mounts
        assert f"{h.data}:{h.data}:ro" in mounts, mounts
        assert os.readlink(d / "testdata/tpch.sf40").startswith(str(h.data) + "/")


def test_run_passes_the_filter_and_resume():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        ok(h.gpu("run", "filter", env={"PCK_TEST_FILTER": "q6 it's"}))
        a = h.docker_runs()[0]
        envs = [a[i + 1] for i, x in enumerate(a) if x == "-e"]
        assert envs == ["PCK_TEST_FILTER=q6 it's"], envs
        ok(h.gpu("run", "bench", env={"PEACOCK_BENCH_RESUME": "1"}))
        a = h.docker_runs()[1]
        assert [a[i + 1] for i, x in enumerate(a) if x == "-e"] == ["PEACOCK_BENCH_RESUME=1"], a


def test_run_passes_the_pinned_cache_and_scan_reader_switches():
    """A load's fixes are measured one at a time by these two; a run that drops them measures the
    engine's default under every name."""
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        ok(h.gpu("run", "bench", env={"PEACOCK_PINNED_HOST_CACHE_BYTES": "0",
                                      "PEACOCK_SCAN_READER": "read_parquet"}))
        a = h.docker_runs()[0]
        envs = [a[i + 1] for i, x in enumerate(a) if x == "-e"]
        assert envs == ["PEACOCK_PINNED_HOST_CACHE_BYTES=0", "PEACOCK_SCAN_READER=read_parquet"], envs


def test_run_refuses_a_run_in_progress():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        h.world(containers={"peacock-chain-l": {"state": "running", "code": 0}})
        refused(h.gpu("run", "list"), 2, "peacock-chain-l")
        refused(h.gpu("ship"), 2, "peacock-chain-l")
        assert h.docker_runs() == []


def test_run_clears_a_finished_container_and_reports_its_exit():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        h.world(containers={"peacock-chain-l": {"state": "exited", "code": 0}}, run_code=1)
        r = h.gpu("run", "list")
        assert r.returncode == 1, (r.returncode, r.stderr)
        assert len(h.docker_runs()) == 1
        assert h.world()["containers"] == {}, "the run's container is left behind"
        h.world(run_code=137)
        assert h.gpu("run", "list").returncode == 1
        h.world(run_code=2)
        assert h.gpu("run", "list").returncode == 2


def test_run_before_ship_says_so():
    with Host() as h:
        prepared(h)
        refused(h.gpu("run", "list"), 2, "ship")
        assert h.docker_runs() == []


# --- pull --------------------------------------------------------------------------------------

def test_pull_brings_the_results_the_record_and_the_log_home():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        h.world(run_output="==> run-tests.sh bench: green\n")
        ok(h.gpu("run", "bench"))
        t = h.vm_dir() / "testdata"
        (t / "benchmark-results/h100/tpch.sf40").mkdir(parents=True)
        (t / "benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt").write_text(f"host={HOSTNAME}\n")
        (t / "calibration").mkdir(exist_ok=True)
        (t / "calibration/records.tsv").write_text("# heading\nrow\n")
        r = ok(h.gpu("pull"))
        home = h.tree / "testdata"
        assert (home / "benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt").read_text() == f"host={HOSTNAME}\n"
        assert (home / "calibration/records.tsv").is_file()
        logs = h.laptop / ".cache/peacock/run-gpu/chain-l"
        assert "bench: green" in (logs / "bench.log").read_text()
        assert str(logs) in r.stdout


def test_pull_before_ship_says_so():
    with Host() as h:
        refused(h.gpu("pull"), 2, "ship")


# --- create_nsys_profile.sh --ssh --------------------------------------------------------------

FAKE_RSH = r'''#!/usr/bin/env python3
import json, os, subprocess, sys
with open(os.environ["FAKE_RSH_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
# ssh forwards none of dev's environment: a switch reaches the host only if the script sends it.
env = {k: v for k, v in os.environ.items() if not k.startswith(("PEACOCK_", "PCK_"))}
env["PATH"] = os.environ["FAKE_HOST_PATH"]
# argv: --vm <name> <host> <command…>, as `nebius.sh … rsh` takes it; the host is dropped.
sys.exit(subprocess.run(["bash", "-c", " ".join(sys.argv[4:])], env=env).returncode)
'''
FAKE_NSYS = r'''#!/usr/bin/env python3
import os, pathlib, subprocess, sys
a = sys.argv[1:]
out = pathlib.Path(a[a.index("-o") + 1])
if a[0] == "profile":
    i = 1
    while a[i].startswith("-") or a[i - 1] == "-o":
        i += 1
    out.with_suffix(".nsys-rep").write_text("rep")
    sys.exit(subprocess.run(a[i:]).returncode)
if a[0] == "export":
    out.write_text("sqlite")
'''
FAKE_BENCH = ('#!/bin/sh\necho "running 1 test"\n'
              'echo "switches: cache=${PEACOCK_PINNED_HOST_CACHE_BYTES-unset} reader=${PEACOCK_SCAN_READER-unset}"\n'
              'echo "test result: ok. 1 passed; 0 failed; 0 ignored"\n')
LOUD_SSH = '#!/bin/sh\nprintf "%s\\n" "ssh $*" >> "$FAKE_DEV_LOG"\nexit 97\n'
STUB_CALLS = r'''import sys
a = sys.argv[1:]
open(a[a.index("--out") + 1], "w").write("# calls\nrow\n")
'''


def nsys_profile(tmp, extra_env=None):
    """`create_nsys_profile.sh --trace` over fakes, the VM a directory and nsys a stub: the run
    and the tree it ran in."""
    tmp = pathlib.Path(tmp)
    tree = tmp / "tree"
    for rel in ("scripts/create_nsys_profile.sh", "scripts/lib/shadgpu-env.sh", "scripts/lib/calibration-bucket.sh",
                "scripts/lib/stage-cargo.sh", "peacockdb-core/tests/common/corpus_benchmark_cases.inc"):
        if (REPO / rel).exists():
            (tree / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, tree / rel)
    (tree / "scripts/calibration").mkdir()
    (tree / "scripts/calibration/nsys_calls.py").write_text(STUB_CALLS)
    (tree / "scripts/calibration/nsys_loads.py").write_text(STUB_CALLS)
    cases = (tree / "peacockdb-core/tests/common/corpus_benchmark_cases.inc").read_text()
    for ds in set(re.findall(r"corpus_query_benchmark!\(\s*(\w+)\s*,", cases)):
        (tree / "testdata/goldens" / f"{ds}.sf1").mkdir(parents=True)
    vm = tmp / "vm" / "peacock" / "chain-l"
    (vm / "cpp/install/rust-benchmarks").mkdir(parents=True)
    bench = vm / "cpp/install/rust-benchmarks/peacock_gpu_benchmarks"
    bench.write_text(FAKE_BENCH)
    bench.chmod(0o755)
    bins = {"host-bin": {"nsys": FAKE_NSYS}, "dev-bin": {"ssh": LOUD_SSH}, ".": {"rsh": FAKE_RSH}}
    for d, files in bins.items():
        (tmp / d).mkdir(exist_ok=True)
        for name, text in files.items():
            (tmp / d / name).write_text(text)
            (tmp / d / name).chmod(0o755)
    env = {**os.environ, "PATH": f"{tmp / 'dev-bin'}:{os.environ['PATH']}",
           "FAKE_HOST_PATH": f"{tmp / 'host-bin'}:{os.environ['PATH']}", "FAKE_RSH_LOG": str(tmp / "rsh.log"),
           "FAKE_DEV_LOG": str(tmp / "ssh.log"), "PCK_TEST_FILTER": "bench_tpch_sf40_q6_tp1_single",
           "CARGO_TARGET_DIR": str(tmp / "target"), **(extra_env or {})}
    r = subprocess.run(["bash", "scripts/create_nsys_profile.sh", "--trace", "--ssh", f"{tmp / 'rsh'} --vm h100",
                        "--host", "nebius-h100", "--remote-dir", str(vm), "--remote-cudf-root", str(tmp / "cudf")],
                       cwd=tree, env=env, capture_output=True, text=True, timeout=120)
    return r, tree


def test_nsys_profile_uses_the_given_ssh():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        r, tree = nsys_profile(tmp)
        ok(r)
        assert "the trace pass ran 1 tests" in r.stdout, r.stdout
        assert (tree / "testdata/calibration/calls.tsv").is_file()
        assert (tree / "testdata/calibration/capture.sqlite").read_text() == "sqlite"
        assert not (tmp / "ssh.log").exists(), (tmp / "ssh.log").read_text()
        calls = log_lines(tmp / "rsh.log")
        assert all(c[:3] == ["--vm", "h100", "nebius-h100"] for c in calls), calls
        commands = [" ".join(c[3:]) for c in calls]
        assert any(c.startswith("test -x") for c in commands), commands
        assert any(c == "bash" for c in commands), commands
        assert any(c.startswith("rsync --server") for c in commands), commands
        assert "switches: cache=unset reader=unset" in r.stdout, r.stdout


def test_nsys_profile_passes_the_pinned_cache_and_scan_reader_switches():
    """The traced binary runs on the host, so a switch set here must reach it there."""
    with tempfile.TemporaryDirectory() as tmp:
        r, _ = nsys_profile(pathlib.Path(tmp), {"PEACOCK_PINNED_HOST_CACHE_BYTES": "0",
                                                "PEACOCK_SCAN_READER": "read_parquet"})
        ok(r)
        assert "switches: cache=0 reader=read_parquet" in r.stdout, r.stdout


if __name__ == "__main__":
    raise SystemExit(main(globals()))
