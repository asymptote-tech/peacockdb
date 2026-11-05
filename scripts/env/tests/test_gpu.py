"""gpu.sh: the device run on a GPU host, from a built tree.

    gpu.sh <host> prepare | ship | run <list|filter|bench> | pull | start | stop [--force]

Each test runs gpu.sh from a built tree against a GPU host that is a directory (`gpuhost.py`):
ssh, rsync, docker, nvidia-smi and aws are fakes, so nothing leaves the machine. Starting and
stopping a host its adapter powers is test_gpu_power.py's.
"""
import contextlib
import fcntl
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpuhost import (APPS_QUERY, BUILD, CAP, COMMIT, DRIVER, DRIVER_QUERY, HOSTNAME, INSTANCE, NAME,  # noqa: E402
                     RUN, SLUG, USER, GpuRig, device_runs, image_of, install, ok, prepared, refused, ship_dir,
                     shipped, stderr_line, until)
from machine import KEYS, REPO, command, flag_values, mounts, path_without  # noqa: E402

TOOLS = ["scripts/env/on-gpu-host.sh", "scripts/lib/peacock-config.sh", "scripts/lib/machine.sh",
         "scripts/lib/data-inputs.sh", "scripts/lib/s3.sh", "testdata/generate_testdata.sh", "docker/pins.env"]


def test_prepare_puts_everything_on_the_host_once():
    with GpuRig() as rig:
        prepared(rig)
        for rel in TOOLS:
            assert (ship_dir(rig) / rel).read_bytes() == (rig.tree / rel).read_bytes(), rel
        pulls = [c[-1] for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"]
        assert pulls == [f"{RUN}:cudf-25.02", f"{BUILD}:cudf-25.02"], pulls
        data = rig.data_dir()
        for dataset in ("tpch.sf1", "tpcds.sf1"):
            assert (data / rig.data_hash() / dataset / "generated").is_file(), sorted(p.name for p in data.iterdir())
        generators = [c for c in rig.host_calls(rig.host, "docker")
                      if c[0] == "run" and any(w.endswith("/generate_testdata.sh") for w in c)]
        assert len(generators) == 2 and all("--network" in c and "none" in c for c in generators), generators
        ids = json.loads((rig.host / "ids.json").read_text())
        fetches = [c for c in rig.host_calls(rig.host, "docker") if c[0] == "run" and "fetch" in c]
        assert len(fetches) == 2, fetches
        for c in generators + fetches:  # the build image by the ID whose label was checked, as the run image is
            assert ids[command(c)[0]]["ref"] == f"{BUILD}:cudf-25.02", command(c)
        for dataset, bucket in (("tpch.sf40", "tpch-sf40"), ("tpcds.sf40", "tpcds-sf40")):
            got = data / "sf40" / dataset
            for f in (rig.buckets / bucket).iterdir():
                assert (got / f.name).read_bytes() == f.read_bytes(), (dataset, f.name)
            assert (got / "SHA256SUMS").is_file(), sorted(p.name for p in got.iterdir())
        fetches = rig.calls("aws")
        assert len(fetches) == 2 and all(f["keys"] for f in fetches), fetches
        for f in fetches:
            assert f["argv"][f["argv"].index("--endpoint-url") + 1] == KEYS["AWS_ENDPOINT_URL"], f
            assert f["argv"][f["argv"].index("--region") + 1] == KEYS["AWS_REGION"], f
        assert not any("cpp/install" in str(p) for p in ship_dir(rig).rglob("*")), "prepare ships no binaries"

        before = len(rig.host_calls(rig.host, "docker"))
        ssh_before = len(rig.calls("ssh"))
        prepared(rig)
        again = rig.host_calls(rig.host, "docker")[before:]
        assert not [c for c in again if c[0] == "run"], again
        assert len(rig.calls("aws")) == 2, rig.calls("aws")
        for call in rig.calls("ssh")[ssh_before:]:
            assert KEYS["AWS_SECRET_ACCESS_KEY"] not in call["stdin"], call


def test_prepare_installs_what_is_missing():
    with GpuRig() as rig:
        host = rig.add_host("bare.test", docker=False, toolkit=False, rsync=False)
        rig.write_host_file("bare", PROVIDER="ssh", HOST="bare.test", USER=USER)
        ok(rig.gpu("bare", "prepare"))
        sudo = rig.host_calls(host, "sudo")
        installs = [line.split()[3:] for line in sudo if line.startswith("apt-get install -y ")]
        for package in ("docker.io", "rsync", "nvidia-container-toolkit"):
            assert sum(package in i for i in installs) == 1, (package, sudo)
        for words in ("nvidia-ctk runtime configure --runtime=docker", f"usermod -aG docker {USER}"):
            assert words in sudo, (words, sudo)
        assert (host / "bin" / "docker").is_file() and (host / "bin" / "rsync").is_file()
        assert (rig.data_dir(host) / rig.data_hash() / "tpch.sf1").is_dir()
        prepared(rig)
        assert rig.host_calls(rig.host, "sudo") == [], "a host with everything installs nothing"


def test_prepare_makes_the_default_data_directory_only_when_it_is_missing():
    default = pathlib.Path("/var/lib/peacock")
    with GpuRig() as rig:
        rig.set_environment(rig.host)
        r = rig.gpu(NAME, "prepare")
        made = [line for line in rig.host_calls(rig.host, "sudo") if line.startswith("install -d")]
        if default.exists():  # this machine has one: prepare must leave it as it is
            assert made == [], made
        else:
            refused(r, str(default))
            assert made == [f"install -d -o {USER} -g docker -m 2775 {default}"], made


def test_prepare_never_makes_an_explicit_data_directory():
    with GpuRig() as rig:
        for n, explicit in enumerate((rig.root / "unmounted-disk", "relative/data")):
            host = rig.add_host(f"h{n}.test")
            rig.set_environment(host, PEACOCK_DATA_DIR=explicit)
            rig.write_host_file(f"h{n}", PROVIDER="ssh", HOST=host.name, USER=USER)
            refused(rig.gpu(f"h{n}", "prepare"), str(explicit))
            assert not any(line.startswith("install -d") for line in rig.host_calls(host, "sudo"))
            assert not pathlib.Path(explicit).exists()


def test_a_relative_data_directory_is_refused_where_it_exists():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host_home() / "peacock-data").mkdir()
        rig.set_environment(rig.host, PEACOCK_DATA_DIR="peacock-data")
        refused(rig.gpu(NAME, "run", "list"), "absolute", "peacock-data")
        assert device_runs(rig) == []


def test_a_data_directory_it_cannot_write_is_refused():
    with GpuRig() as rig:
        rig.data_dir().chmod(0o555)
        try:
            refused(rig.gpu(NAME, "prepare"), str(rig.data_dir()))
        finally:
            rig.data_dir().chmod(0o2775)
        assert [c for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"] == []
        shipped(rig)
        missing = rig.root / "nowhere"
        rig.set_environment(rig.host, PEACOCK_DATA_DIR=missing)
        refused(rig.gpu(NAME, "run", "list"), str(missing))
        assert not missing.exists() and device_runs(rig) == []


def test_prepare_refuses_a_host_without_key_login():
    with GpuRig() as rig:
        (rig.host_home() / ".ssh" / "authorized_keys").write_text("ssh-ed25519 AAAAsomeoneelse\n")
        refused(rig.gpu(NAME, "prepare"), "provision-vm.sh --gpu-host", NAME, "Permission denied")
        assert len(rig.calls("ssh")) == 1 and rig.calls("rsync") == [], rig.calls("ssh")
        assert not (rig.host_home() / "peacock").exists()


def test_run_refuses_old_driver():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "driver").write_text(f"580.65.06, {CAP}\n530.30.02, {CAP}\n")
        before = len(rig.host_calls(rig.host, "docker"))
        rsync_before = len(rig.calls("rsync"))
        refused(rig.gpu(NAME, "run", "list"), "530.30.02", "535.104.05", NAME, code=3)
        refused(rig.gpu(NAME, "run", "list", "--cudf", "26.02"), "530.30.02", "575.57.08", code=3)
        assert rig.host_calls(rig.host, "docker")[before:] == []
        assert len(rig.calls("rsync")) == rsync_before

        host = rig.add_host("old.test", driver="560.35.03")
        rig.write_host_file("old", PROVIDER="ssh", HOST="old.test", USER=USER)
        refused(rig.gpu("old", "prepare", "--cudf", "26.02"), "560.35.03", "575.57.08", code=3)
        assert rig.host_calls(host, "docker") == [] and not (rig.host_home(host) / "peacock").exists()
        ok(rig.gpu("old", "prepare"))


def test_a_card_past_compute_capability_9_needs_the_driver_that_compiles_its_ptx():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "driver").write_text("570.86.10, 10.0\n")
        refused(rig.gpu(NAME, "run", "list"), "570.86.10", "575.51.03", "10", code=3)
        for answer in ("580.65.06, 12.0", "570.86.10, 9.0", "570.86.10, 8.0"):
            (rig.host / "driver").write_text(answer + "\n")
            ok(rig.gpu(NAME, "run", "list"))
        assert len(device_runs(rig)) == 3
        (rig.host / "driver").write_text("580.65.06, 7.5\n")
        refused(rig.gpu(NAME, "run", "list"), "7.5", "compute capability")
        assert len(device_runs(rig)) == 3


def test_a_driver_answer_that_is_no_version_is_refused():
    with GpuRig() as rig:
        shipped(rig)
        for answer in ("N/A", "580.65.06 extra", "", "580, 8.9", "580.65.06", f"580.65.06, {CAP}, x"):
            (rig.host / "driver").write_text(answer + "\n")
            r = rig.gpu(NAME, "run", "list")
            refused(r, "nvidia-smi")
            assert answer in r.stderr, (answer, r.stderr)
        assert device_runs(rig) == []


def test_the_host_fakes_answer_only_their_exact_query():
    with GpuRig() as rig:
        env = {**os.environ, "FAKE_VM": str(rig.host)}
        smi = str(rig.host / "bin" / "nvidia-smi")
        r = subprocess.run([smi, *DRIVER_QUERY], env=env, capture_output=True, text=True)
        assert r.returncode == 0 and r.stdout == f"{DRIVER}, {CAP}\n", r
        for wrong in (["--query-gpu=driver_version", "--format=csv,noheader"],
                      ["--query-gpu=name", "--format=csv,noheader"], [DRIVER_QUERY[0], "--format=csv"], [APPS_QUERY[0]]):
            assert subprocess.run([smi, *wrong], env=env, capture_output=True).returncode != 0, wrong
        docker = str(rig.host / "bin" / "docker")
        assert subprocess.run([docker, "pull", f"{RUN}:cudf-25.02"], env=env).returncode == 0
        for fmt in ("{{.Config.Labels}}", "{{json .Config.Labels}}", "{{.ID}}"):
            r = subprocess.run([docker, "image", "inspect", "--format", fmt, f"{RUN}:cudf-25.02"], env=env,
                               capture_output=True)
            assert r.returncode != 0, fmt


def test_prepare_refuses_stale_run_image():
    with GpuRig() as rig:
        stale = rig.label("run", "25.02").replace("docker/run-tests.sh=", "docker/run-tests.sh=0")
        rig.publish_images(rig.host, run_label=stale)
        refused(rig.gpu(NAME, "prepare"), "docker/run-tests.sh")
        assert not any(p.is_dir() for p in rig.data_dir().iterdir()), list(rig.data_dir().iterdir())
        assert [c[-1] for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"] == [f"{RUN}:cudf-25.02"]
        rig.publish_images(rig.host, run_label="<no value>")
        refused(rig.gpu(NAME, "prepare"), "peacockdb.inputs")


def test_prepare_names_a_pull_that_fails():
    with GpuRig() as rig:
        rig.publish_images(rig.host, drop={f"{RUN}:cudf-25.02"})
        r = rig.gpu(NAME, "prepare")
        assert r.returncode == 1, (r.returncode, r.stderr)
        assert f"docker pull {RUN}:cudf-25.02" in r.stderr.splitlines()[-1], r.stderr


def moved_tag(rig, label):
    """The run tag on the host now names an image labelled <label>, as another tree's pull leaves it."""
    images = json.loads((rig.host / "images.json").read_text())
    images[f"{RUN}:cudf-25.02"] = label
    (rig.host / "images.json").write_text(json.dumps(images))


def test_run_refuses_an_image_another_tree_pulled_since():
    with GpuRig() as rig:
        shipped(rig)
        moved_tag(rig, rig.label("run", "25.02").replace("docker/runtime-spec.sh=", "docker/runtime-spec.sh=0"))
        refused(rig.gpu(NAME, "run", "list"), "docker/runtime-spec.sh", "prepare")
        assert device_runs(rig) == []


def test_the_image_run_is_the_one_whose_label_was_read():
    with GpuRig() as rig:
        shipped(rig)
        good = rig.label("run", "25.02")
        moved_tag(rig, good.replace("docker/run-tests.sh=", "docker/run-tests.sh=0"))
        (rig.host / "move.json").write_text(json.dumps({f"{RUN}:cudf-25.02": good}))
        refused(rig.gpu(NAME, "run", "list"), "docker/run-tests.sh")
        assert device_runs(rig) == []
        ok(rig.gpu(NAME, "run", "list"))
        (run,) = device_runs(rig)
        ids = json.loads((rig.host / "ids.json").read_text())
        assert command(run)[0].startswith("sha256:") and ids[command(run)[0]]["label"] == good, run


def test_ship_dir_is_instance_named():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "ship", "--instance", "dev2-B", env={"PEACOCK_INSTANCE": None}))
        for instance in (INSTANCE, "dev2-B"):
            d = ship_dir(rig, instance)
            assert (d / "cpp/install/lib/libpeacock_gpu.so").read_text() == "cpp/install/lib/libpeacock_gpu.so\n"
            for rel in ("testdata/tpch-queries/q1.sql", "testdata/goldens/tpch/q1.cpu.txt",
                        "testdata/tpch-vec-queries/query_params.jsonl", "testdata/cost-registry.csv"):
                assert (d / rel).is_file(), (instance, rel)
            data = ("tpch.sf1", "tpcds.sf1", "tpch.sf40", "tpcds.sf40")
            assert not any(p.name in data for p in d.rglob("*")), instance
        assert sorted(p.name for p in (rig.host_home() / "peacock").iterdir()) == sorted(["dev2-B", INSTANCE])

        (ship_dir(rig) / "testdata" / "benchmark-results").mkdir()
        (ship_dir(rig) / "testdata" / "benchmark-results" / "kept.txt").write_text("a run's result\n")
        (rig.tree / "cpp/install/bin/peacock_ffi_tests").unlink()
        ok(rig.gpu(NAME, "ship"))
        assert not (ship_dir(rig) / "cpp/install/bin/peacock_ffi_tests").exists()
        assert (ship_dir(rig) / "testdata" / "benchmark-results" / "kept.txt").is_file()

        refused(rig.gpu(NAME, "ship", env={"PEACOCK_INSTANCE": None}), "--instance")
        refused(rig.gpu(NAME, "ship", "--instance", "dev2-B"), "--instance", INSTANCE)


def test_the_fixtures_are_ci_ship_s():
    def array(path, name):
        script = f'eval "$(sed -n "/^{name}=(/,/)/p" "$1")"; printf "%s\\n" "${{{name}[@]}}"'
        return set(subprocess.run(["bash", "-c", script, "_", str(path)], check=True, capture_output=True,
                                  text=True).stdout.split())
    ci = array(REPO / "scripts/nebius/ci-ship.sh", "TESTDATA")
    gpu = REPO / "scripts/env/gpu.sh"
    assert ci and array(gpu, "SHIPPED") == ci - {"testdata/generate_testdata.sh"}, (ci, array(gpu, "SHIPPED"))
    assert "testdata/generate_testdata.sh" in array(gpu, "TOOLS")


def test_run_passes_the_machines_hostname():
    with GpuRig() as rig:
        shipped(rig)
        r = ok(rig.gpu(NAME, "run", "list"))
        assert "green" in r.stdout, r.stdout
        (run,) = device_runs(rig)
        assert flag_values(run, "--hostname") == [HOSTNAME], run
        assert flag_values(run, "--gpus") == ["all"], run
        assert flag_values(run, "--user") == [f"{os.getuid()}:{os.getgid()}"], run
        assert command(run)[1] == ["list"] and image_of(rig, run) == f"{RUN}:cudf-25.02", command(run)
        d, data = ship_dir(rig), rig.data_dir()
        assert sorted(mounts(run)) == sorted([
            (str(d / "cpp" / "install"), "/peacock/install", True),
            (str(d / "testdata"), "/peacock/testdata", False),
            (str(data / rig.data_hash() / "tpch.sf1"), "/peacock/testdata/tpch.sf1", True),
            (str(data / rig.data_hash() / "tpcds.sf1"), "/peacock/testdata/tpcds.sf1", True),
            (str(data / "sf40" / "tpch.sf40"), "/peacock/testdata/tpch.sf40", True),
            (str(data / "sf40" / "tpcds.sf40"), "/peacock/testdata/tpcds.sf40", True)]), mounts(run)
        assert not [e for e in flag_values(run, "-e") if e.startswith(("PCK_TEST_FILTER", "PEACOCK_BENCH_RESUME"))]
        logs = sorted((d / "logs").iterdir())
        assert [p.suffix for p in logs] == [".log", ".status"], logs
        assert logs[0].name.startswith("list-") and "green" in logs[0].read_text() and logs[1].read_text() == "0\n"


def test_run_passes_the_filter_and_the_resume():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "filter", env={"PCK_TEST_FILTER": "tpch_q1"}))
        ok(rig.gpu(NAME, "run", "bench", env={"PEACOCK_BENCH_RESUME": "1", "PCK_TEST_FILTER": "q6"}))
        prepared(rig, "--cudf", "26.02")
        ok(rig.gpu(NAME, "run", "bench", "--cudf", "26.02"))
        filt, resumed, plain = device_runs(rig)
        assert flag_values(filt, "-e") == ["PCK_TEST_FILTER=tpch_q1"] and command(filt)[1] == ["filter"], filt
        assert sorted(flag_values(resumed, "-e")) == ["PCK_TEST_FILTER=q6", "PEACOCK_BENCH_RESUME=1"], resumed
        assert flag_values(plain, "-e") == [] and image_of(rig, plain) == f"{RUN}:cudf-26.02", plain
        (rig.host / "run_status").write_text("1")
        r = rig.gpu(NAME, "run", "list")
        assert r.returncode == 1, (r.returncode, r.stderr)


def test_run_bench_needs_sf40():
    with GpuRig() as rig:
        shipped(rig)
        for f in (rig.data_dir() / "sf40" / "tpcds.sf40").iterdir():
            f.unlink()
        (rig.data_dir() / "sf40" / "tpcds.sf40").rmdir()
        refused(rig.gpu(NAME, "run", "bench"), "tpcds.sf40", "prepare")
        assert device_runs(rig) == []
        ok(rig.gpu(NAME, "run", "list"))


def test_one_device_run_at_a_time_per_host():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "ship", env={"PEACOCK_INSTANCE": "dev2-B"}))
        (rig.host / "hold").write_text("")
        first = rig.gpu(NAME, "run", "list", background=True)
        until(lambda: rig.host_calls(rig.host, "runs"), "the first run started")
        second = rig.gpu(NAME, "run", "list", env={"PEACOCK_INSTANCE": "dev2-B"}, background=True)
        logs = ship_dir(rig, "dev2-B") / "logs"
        until(lambda: any("waiting" in f.read_text() for f in logs.glob("list-*.log")), "the second run waits")
        # A run that says it waits and does not reaches docker within moments; this one never may.
        deadline = time.time() + 2
        while time.time() < deadline and len(device_runs(rig)) < 2:
            time.sleep(0.05)
        (rig.host / "hold").unlink()
        outs = [(p.wait(timeout=60), *p.communicate()) for p in (first, second)]
        assert [o[0] for o in outs] == [0, 0], outs
        events = rig.host_calls(rig.host, "runs")
        assert [e["event"] for e in events] == ["start", "end", "start", "end"], events
        waiting = [line for o in outs for line in (o[1] + o[2]).splitlines() if "waiting" in line]
        assert len(waiting) == 1 and HOSTNAME in waiting[0] and waiting[0] in outs[1][1], outs


def dropped(rig, *args, env=None):
    """gpu.sh run <args>, its session dropped once the device run has started."""
    first = rig.gpu(NAME, "run", *args, env=env, background=True)
    until(lambda: rig.host_calls(rig.host, "runs"), "the run started")
    os.killpg(first.pid, signal.SIGKILL)
    first.wait()


def test_a_dropped_session_leaves_one_run_that_a_rerun_follows():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold").write_text("")
        dropped(rig, "list")
        assert rig.is_locked(rig.data_dir() / ".device.lock"), "the detached run holds the device lock"
        refused(rig.gpu(NAME, "run", "bench"), "list")
        moved_tag(rig, "a label another tree's pull left")
        rerun = rig.gpu(NAME, "run", "list", background=True)
        stderr_line(rerun, "still going")
        (rig.host / "hold").unlink()
        out, _ = rerun.communicate(timeout=60)
        assert rerun.returncode == 0 and "green" in out, (rerun.returncode, out)
        assert [e["event"] for e in rig.host_calls(rig.host, "runs")] == ["start", "end"]
        assert len(device_runs(rig)) == 1, device_runs(rig)
        assert not rig.is_locked(rig.data_dir() / ".device.lock")
        moved_tag(rig, rig.label("run", "25.02"))
        ok(rig.gpu(NAME, "run", "list"))
        assert len(device_runs(rig)) == 2, "a run that ended is not followed again"
        statuses = sorted((ship_dir(rig) / "logs").glob("*.status"))
        assert [p.read_text() for p in statuses] == ["0\n", "0\n"], statuses


def test_ship_refuses_while_a_run_goes_on_and_changed_binaries_are_not_followed():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold").write_text("")
        dropped(rig, "list")
        lib = ship_dir(rig) / "cpp/install/lib/libpeacock_gpu.so"
        (rig.tree / "cpp/install/lib/libpeacock_gpu.so").write_text("rebuilt\n")
        refused(rig.gpu(NAME, "ship"), "list", "run list follows it")
        assert lib.read_text() == "cpp/install/lib/libpeacock_gpu.so\n"
        lib.write_text("rebuilt\n")
        refused(rig.gpu(NAME, "run", "list"), "changed since")
        (rig.host / "hold").unlink()
        until(lambda: len(rig.host_calls(rig.host, "runs")) == 2, "the held run ended")
        until(lambda: not rig.is_locked(rig.data_dir() / ".device.lock"), "the run let go of the card")
        ok(rig.gpu(NAME, "ship"))
        ok(rig.gpu(NAME, "run", "list"))
        assert len(device_runs(rig)) == 2


def test_the_s3_keys_never_rest_on_the_host():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "bench"))
        values = [KEYS["AWS_ACCESS_KEY_ID"], KEYS["AWS_SECRET_ACCESS_KEY"]]
        carried = [c for c in rig.calls("ssh") if values[1] in c["stdin"]]
        assert len(carried) == 1, [c["argv"] for c in carried]
        for call in rig.calls("ssh"):
            for v in values:
                assert v not in " ".join(call["argv"]), call["argv"]
        for call in rig.host_calls(rig.host, "docker"):
            for v in values:
                assert v not in " ".join(call), call
        for root in (rig.host_home(), rig.data_dir(), rig.host):
            for p in root.rglob("*"):
                if p.is_file():
                    text = p.read_bytes()
                    for v in values:
                        assert v.encode() not in text, p


def test_a_broken_sf40_set_is_fetched_again():
    with GpuRig() as rig:
        prepared(rig)
        (rig.data_dir() / "sf40" / "tpch.sf40" / "lineitem.parquet").write_text("truncated")
        prepared(rig)
        buckets = [next(w for w in f["argv"] if w.startswith("s3://")) for f in rig.calls("aws")]
        assert buckets == ["s3://tpch-sf40/", "s3://tpcds-sf40/", "s3://tpch-sf40/"], buckets
        assert (rig.data_dir() / "sf40" / "tpch.sf40" / "lineitem.parquet").read_text() == "tpch lineitem at sf40"


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


def failed(r, *words):
    """Exit 1 with gpu.sh's own last line naming each of <words>."""
    assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
    last = r.stderr.strip().splitlines()[-1]
    assert last.startswith("gpu.sh: "), r.stderr
    for word in words:
        assert word in last, (word, r.stderr)


def test_a_tool_s_failure_is_exit_1_never_3():
    with GpuRig() as rig:
        shipped(rig)
        failed(rig.gpu(NAME, "ship", env={"FAKE_RSYNC_EXIT": "3"}), "rsync")
        failed(rig.gpu(NAME, "pull", env={"FAKE_RSYNC_EXIT": "3"}), "rsync")
        for verb, command_words in ((("run", "list"), "nvidia-smi"), (("run", "list"), "on-gpu-host.sh label"),
                                    (("prepare",), "sf40-missing"), (("ship",), "/cpp/install "),
                                    (("ship",), "tar --unlink-first")):
            (rig.host / "drop_on").write_text(command_words)
            r = rig.gpu(NAME, *verb)
            failed(r, "ssh")
            assert "driver answers" not in r.stderr and "is not on" not in r.stderr, r.stderr
        (rig.host / "drop_on").write_text("bash -s -- hold")
        failed(rig.gpu(NAME, "run", "list"), "ssh", "holding")
        (rig.host / "drop_on").unlink()
        (rig.host / "exit_on").write_text("sf40-missing 3")
        failed(rig.gpu(NAME, "prepare"), "listing sf40", "exit 3")
        (rig.host / "exit_on").unlink()
        bare = rig.add_host("bare.test")
        rig.write_host_file("bare", PROVIDER="ssh", HOST="bare.test", USER=USER)
        (rig.host_home(bare) / "peacock" / INSTANCE).mkdir(parents=True)
        (rig.host_home(bare) / "peacock" / INSTANCE / "scripts").write_text("a file where a directory goes\n")
        failed(rig.gpu("bare", "prepare"), "scripts", "bare")


def test_an_instance_on_a_pinned_image_runs_that_image():
    with GpuRig() as rig:
        pinned = {"PEACOCK_IMAGE_TAG": f"cudf-25.02-{COMMIT}"}
        shipped(rig, env=pinned)
        pulls = [c[-1] for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"]
        assert pulls == [f"{RUN}:cudf-25.02-{COMMIT}", f"{BUILD}:cudf-25.02-{COMMIT}"], pulls
        ok(rig.gpu(NAME, "run", "list", env=pinned))
        (run,) = device_runs(rig)
        assert image_of(rig, run) == f"{RUN}:cudf-25.02-{COMMIT}", run
        refused(rig.gpu(NAME, "run", "list", "--cudf", "26.02", env=pinned), f"cudf-25.02-{COMMIT}")
        refused(rig.gpu(NAME, "run", "list", "--image", f"cudf-25.02-{COMMIT}", env=pinned), "--image")
        ok(rig.gpu(NAME, "run", "list", "--image", f"cudf-25.02-{COMMIT}", "--instance", INSTANCE,
                   env={"PEACOCK_INSTANCE": None}))


def test_gpu_refuses_bad_arguments_before_any_call():
    with GpuRig() as rig:
        cases = [
            ((), {}, "name the host"),
            ((NAME,), {}, "prepare, ship, run, pull, start or stop"),
            ((NAME, "reboot"), {}, "reboot"),
            ((NAME, "run"), {}, "list, filter or bench"),
            ((NAME, "run", "list", "bench"), {}, "bench"),
            ((NAME, "ship", "--cudf", "26.02"), {}, "--cudf"),
            ((NAME, "pull", "--cudf", "26.02"), {}, "--cudf"),
            ((NAME, "ship", "--force"), {}, "--force"),
            ((NAME, "start", "now"), {}, "now"),
            ((NAME, "prepare", "--cudf", "24.10"), {}, "24.10"),
            ((NAME, "prepare", "--image", "latest"), {}, "latest"),
            ((NAME, "prepare", "--image", "cudf-25.02", "--cudf", "25.02"), {}, "--cudf"),
            ((NAME, "run", "filter"), {}, "PCK_TEST_FILTER"),
            ((NAME, "run", "list"), {"PCK_TEST_FILTER": "q1"}, "PCK_TEST_FILTER"),
            ((NAME, "run", "bench"), {"PEACOCK_BENCH_RESUME": "yes"}, "PEACOCK_BENCH_RESUME"),
            ((NAME, "run", "list"), {"PEACOCK_BENCH_RESUME": "1"}, "PEACOCK_BENCH_RESUME"),
            ((NAME, "run", "filter"), {"PEACOCK_BENCH_RESUME": "0", "PCK_TEST_FILTER": "q1"}, "bench"),
            ((NAME, "run", "list"), {"PEACOCK_IMAGE_TAG": "cudf-9"}, "cudf-9"),
            ((NAME, "ship"), {"GPU_SSH_KEY_FILE": None}, "GPU_SSH_KEY_FILE"),
            ((NAME, "ship"), {"GPU_SSH_KEY_FILE": "relative/key"}, "GPU_SSH_KEY_FILE"),
            (("nowhere", "ship"), {}, "provision-vm.sh --gpu-host"),
            ((NAME, "ship", "--instance", "a/b"), {"PEACOCK_INSTANCE": None}, "a/b"),
            ((NAME, "prepare"), {"AWS_SECRET_ACCESS_KEY": None}, "AWS_SECRET_ACCESS_KEY"),
            ((NAME, "run", "list"), {"PEACOCK_HOLD_BEAT": "6", "PEACOCK_HOLD_TIMEOUT": "1"}, "PEACOCK_HOLD_BEAT"),
            ((NAME, "run", "list"), {"PEACOCK_HOLD_BEAT": "60", "PEACOCK_HOLD_TIMEOUT": "60"}, "PEACOCK_HOLD_BEAT"),
            ((NAME, "run", "list"), {"PEACOCK_HOLD_TIMEOUT": "5m"}, "PEACOCK_HOLD_TIMEOUT"),
            ((NAME, "run", "list"), {"PEACOCK_HOLD_BEAT": "0.5"}, "PEACOCK_HOLD_BEAT"),
        ]
        for args, env, words in cases:
            refused(rig.gpu(*args, env=env), words)
        (rig.tree / "cpp/install/lib/libpeacock_gpu.so").unlink()
        refused(rig.gpu(NAME, "ship"), "libpeacock_gpu.so")
        assert rig.calls("ssh") == [] and rig.calls("rsync") == [], rig.calls("ssh")


def test_gpu_has_help():
    with GpuRig() as rig:
        r = ok(rig.gpu("--help"))
        assert "gpu.sh <host>" in r.stdout and not any(ln.startswith("#") for ln in r.stdout.splitlines()), r.stdout
        usage = [ln for ln in r.stdout.splitlines() if "gpu.sh <host>" in ln]
        for verb in ("ship", "pull"):
            line = next(ln for ln in usage if verb in ln.split())
            assert "--instance" in line and "--cudf" not in line and "--image" not in line, line
        for verb in ("prepare", "run"):
            line = next(ln for ln in usage if verb in ln.split())
            assert "--instance" in line and "--cudf" in line and "--image" in line, line


def test_a_host_tool_s_own_status_is_exit_1():
    with GpuRig() as rig:
        for code in ("255", "2"):
            r = rig.gpu(NAME, "prepare", env={"FAKE_AWS_EXIT": code})
            assert r.returncode == 1, (code, r.returncode, r.stderr)
            assert "fetching sf40 failed" in r.stderr.splitlines()[-1] and "lost" not in r.stderr, (code, r.stderr)


def test_a_rerun_with_another_environment_is_not_followed():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold").write_text("")
        dropped(rig, "filter", env={"PCK_TEST_FILTER": "tpch_q1"})
        refused(rig.gpu(NAME, "run", "filter", env={"PCK_TEST_FILTER": "q6"}), "PCK_TEST_FILTER=tpch_q1",
                "PCK_TEST_FILTER=q6")
        rerun = rig.gpu(NAME, "run", "filter", env={"PCK_TEST_FILTER": "tpch_q1"}, background=True)
        stderr_line(rerun, "still going")
        (rig.host / "hold").unlink()
        assert rerun.wait(timeout=60) == 0, rerun.communicate()
        assert len(device_runs(rig)) == 1


def test_the_shared_files_stay_group_writable():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        data = rig.data_dir()
        for path in (data / ".activity.lock", data / ".device.lock", data / "runs"):
            assert path.stat().st_mode & 0o020, (path, oct(path.stat().st_mode))


def test_a_run_whose_instance_is_busy_is_neither_started_nor_followed():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        state = ship_dir(rig) / "run" / "state"
        lock = rig.data_dir() / "runs" / f"{USER}-{INSTANCE}.lock"
        with open(lock, "a") as held:  # another session of this instance, a ship's or a run's being started
            fcntl.flock(held, fcntl.LOCK_EX)
            r = rig.gpu(NAME, "run", "list")
            refused(r, "busy")
            r = rig.gpu(NAME, "ship")
            refused(r, "busy")
        assert len(device_runs(rig)) == 1 and state.is_file()


def test_a_container_of_the_instance_still_on_the_host_refuses_a_new_run():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "containers").mkdir()
        (rig.host / "containers" / f"peacock-run-{USER}-{INSTANCE}").write_text("")
        refused(rig.gpu(NAME, "run", "list"), f"peacock-run-{USER}-{INSTANCE}")
        assert device_runs(rig) == []
        (rig.host / "containers" / f"peacock-run-{USER}-{INSTANCE}").unlink()
        ok(rig.gpu(NAME, "run", "list"))
        assert flag_values(device_runs(rig)[0], "--name") == [f"peacock-run-{USER}-{INSTANCE}"]


def test_a_hold_whose_caller_falls_silent_lets_go():
    with GpuRig() as rig:
        prepared(rig)
        script = (ship_dir(rig) / "scripts/env/on-gpu-host.sh").read_bytes()
        # As gpu.sh sends it: the script on stdin, then the heartbeat on the same stream, here never.
        hold = subprocess.Popen(["bash", "-s", "--", "hold", "1"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                text=False, cwd=rig.host_home(),
                                env={**rig.on_host_env(), "PATH": rig.on_host_env()["PATH"]})
        hold.stdin.write(script)
        hold.stdin.flush()
        assert hold.stdout.readline() == b"held\n"
        assert hold.wait(timeout=10) == 0
        hold.stdin.close()
        lock = rig.data_dir() / ".activity.lock"
        with open(lock) as f:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        hold = rig.on_host("hold", "1", stdin=subprocess.PIPE, background=True)
        assert hold.stdout.readline() == "held\n"
        assert hold.wait(timeout=10) == 0, hold.communicate()
        hold.stdin.close()
        lock = rig.data_dir() / ".activity.lock"
        with open(lock) as f:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)


def test_a_command_longer_than_the_hold_s_timeout_keeps_it_by_its_heartbeat():
    with GpuRig() as rig:
        shipped(rig)
        beat = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"}
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env=beat, background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        time.sleep(3.5)
        busy = rig.on_host("busy")
        assert busy.stdout.strip() == "busy: a gpu.sh step is running on the host", busy
        (rig.host / "hold_smi").unlink()
        assert run.wait(timeout=60) == 0, run.communicate()


def test_a_run_that_ends_before_it_is_followed_gives_its_status():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "run_status").write_text("1")
        r = rig.gpu(NAME, "run", "list")
        assert r.returncode == 1, r
        _, mode, log, stamp, *_ = (ship_dir(rig) / "run" / "state").read_text().split()
        r = rig.on_host("follow", "list", stamp)
        assert r.returncode == 1 and "green" in r.stdout, (r.returncode, r.stdout, r.stderr)
        r = rig.on_host("follow", "list", "20000101T000000.000Z")
        assert r.returncode == 2 and "no run" in r.stderr, (r.returncode, r.stderr)
        for args, words in ((("bench", stamp), "list"), (("list", stamp, "PCK_TEST_FILTER=q6"), "PCK_TEST_FILTER=q6")):
            r = rig.on_host("follow", *args)
            assert r.returncode == 2 and words in r.stderr and "green" not in r.stdout, (args, r.returncode, r.stderr)
            for told in ("ended", "status 1", log, "again", "starts"):
                assert told in r.stderr, (told, r.stderr)
            for false_advice in ("follows it", "wait for it", "gives its status"):
                assert false_advice not in r.stderr, (false_advice, r.stderr)
            assert " not " not in r.stderr, ("the mode said twice, or a double negative", r.stderr)
        (ship_dir(rig) / "cpp/install/lib/libpeacock_gpu.so").write_text("rebuilt\n")
        r = rig.on_host("follow", "list", stamp)
        assert r.returncode == 2 and "changed since" in r.stderr, (r.returncode, r.stderr)


MAC_TOOLS = {
    "shasum": r"""#!/usr/bin/env python3
import hashlib, sys
a = sys.argv[1:]
assert a[:2] == ["-a", "256"], a
files = a[2:] or ["-"]
for f in files:
    data = sys.stdin.buffer.read() if f == "-" else open(f, "rb").read()
    print(f"{hashlib.sha256(data).hexdigest()}  {f}")
""",
    "paste": """#!/bin/sh
# macOS's paste: -s and -d need a file operand, - for stdin.
for a in "$@"; do last=$a; done
case "$last" in -s*|-d*) echo "usage: paste [-s] [-d delimiters] file ..." >&2; exit 1 ;; esac
exec /usr/bin/paste "$@"
""",
    "sort": """#!/bin/sh
for a in "$@"; do case "$a" in -V|--version-sort) echo "sort: invalid option -- V" >&2; exit 2 ;; esac; done
exec /usr/bin/sort "$@"
""",
}


def test_gpu_sh_runs_on_a_laptop_with_macos_s_tools():
    with GpuRig() as rig:
        mac = rig.root / "mac-bin"
        (rig.root / "mac-system").mkdir()
        for name, text in MAC_TOOLS.items():
            install(mac / name, text)
        system = path_without(rig.root / "mac-system", {"sha256sum", "timeout", "gtimeout", "paste", "sort",
                                                        "docker", "nvidia-smi", "nvidia-ctk", "aws", "nebius",
                                                        "rsync"})
        laptop = {"PATH": f"{rig.bin}:{mac}:{system}"}
        shipped(rig, env=laptop)
        ok(rig.gpu(NAME, "run", "bench", env=laptop))
        ok(rig.gpu(NAME, "pull", env=laptop))
        stale = rig.label("run", "25.02").replace("docker/run-tests.sh=", "docker/run-tests.sh=0")
        rig.publish_images(rig.host, run_label=stale)
        subprocess.run([str(rig.host / "bin" / "docker"), "pull", f"{RUN}:cudf-25.02"], check=True,
                       env={**os.environ, "FAKE_VM": str(rig.host)})
        refused(rig.gpu(NAME, "run", "list", env=laptop), "docker/run-tests.sh")
        rig.nebius_host()
        r = rig.adapter("nebius", "h100", "start", env=laptop)
        assert r.returncode == 0, (r.returncode, r.stderr)


def test_a_gpu_sh_killed_alone_lets_go_of_its_hold():
    with GpuRig() as rig:
        shipped(rig)
        beat = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"}
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env=beat, background=True)
        try:
            until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            os.kill(run.pid, signal.SIGKILL)  # the pid alone, as an OOM kill or a supervisor's timeout does
            run.wait()
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the orphaned hold lets go", seconds=20)
            (rig.host / "hold_smi").unlink()
            ok(rig.gpu(NAME, "run", "list"))
        finally:
            try:
                os.killpg(run.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def test_a_second_user_s_first_prepare_joins_the_group_before_it_holds():
    with GpuRig() as rig:
        (rig.host / "groups").write_text(f"{USER}\n")
        rig.data_dir().chmod(0o555)  # another user's directory, writable only through the docker group
        try:
            ok(rig.gpu(NAME, "prepare"))
        finally:
            rig.data_dir().chmod(0o2775)
        assert f"usermod -aG docker {USER}" in rig.host_calls(rig.host, "sudo")
        assert (rig.data_dir() / rig.data_hash() / "tpch.sf1").is_dir()
        holds = [c for c in rig.calls("ssh") if c["argv"][-1].startswith("bash -s -- hold ")]
        assert len(holds) == 2, "unheld before the install, held again in a session that has the group"


def test_a_run_during_a_ship_is_busy():
    with GpuRig() as rig:
        shipped(rig)
        gate = rig.root / "rsync-gate"
        gate.write_text("")
        before = len(rig.calls("rsync"))
        ship = rig.gpu(NAME, "ship", env={"FAKE_RSYNC_GATE": str(gate)}, background=True)
        try:
            until(lambda: len(rig.calls("rsync")) > before, "the ship's rsync starts")
            refused(rig.gpu(NAME, "run", "list"), "busy")
            assert device_runs(rig) == [], "no run on binaries half shipped"
        finally:
            gate.unlink()
        assert ship.wait(timeout=60) == 0, ship.communicate()
        ok(rig.gpu(NAME, "run", "list"))


def test_a_lock_held_without_a_live_unfinished_run_is_busy():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        state = ship_dir(rig) / "run" / "state"
        pid, mode, log, stamp, digest, envhash = state.read_text().split()
        dead = subprocess.Popen(["true"])
        dead.wait()
        unfinished = pathlib.Path(log).with_name("list-20000101T000000.000Z.log")
        unfinished.write_text("a run that never wrote its status\n")
        lock = rig.data_dir() / "runs" / f"{USER}-{INSTANCE}.lock"
        cases = ((str(os.getpid()), log), (str(dead.pid), str(unfinished)))  # alive and ended; dead with no status
        for pid_, log_ in cases:
            state.write_text(f"{pid_} {mode} {log_} {stamp} {digest} {envhash}\n")
            with open(lock, "a") as held:
                fcntl.flock(held, fcntl.LOCK_EX)
                refused(rig.gpu(NAME, "run", "list"), "busy")
        assert len(device_runs(rig)) == 1


def test_the_portable_timeout_says_124_and_passes_the_command_s_own_status():
    with GpuRig() as rig:
        (rig.root / "plain").mkdir()
        system = path_without(rig.root / "plain", {"timeout", "gtimeout"})
        script = f'. "{REPO}/scripts/lib/portable.sh"; type timeout | head -1; '
        for call, want in (("timeout 1 sleep 5", 124), ("timeout 37 sh -c 'exit 7'", 7)):
            start = time.time()
            r = subprocess.run(["bash", "-c", script + f"{call}; echo status $?"], env={"PATH": str(system)},
                               capture_output=True, text=True, timeout=30)
            assert "function" in r.stdout and f"status {want}" in r.stdout, (call, r.stdout, r.stderr)
            assert time.time() - start < 4, call
        assert sleeps_of("37") == [], "the watcher's sleep outlived the command it watched"


def test_a_hold_lost_mid_command_is_named():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        home = str(rig.host_home())
        holds = [int(pid) for pid in os.listdir("/proc") if pid.isdigit() and _holds(pid, home)]
        assert len(holds) == 1, holds
        os.kill(holds[0], signal.SIGKILL)
        (rig.host / "hold_smi").unlink()
        out, err = run.communicate(timeout=60)
        assert run.returncode == 0 and "green" in out, (run.returncode, err)
        assert "the hold on l40s ended early" in err, err


def _holds(pid, home):
    """Whether process <pid> is a host-side hold session of the host whose home is <home>."""
    try:
        return (open(f"/proc/{pid}/cmdline").read().split("\0")[:4] == ["bash", "-s", "--", "hold"]
                and os.readlink(f"/proc/{pid}/cwd") == home)
    except OSError:
        return False


def sleeps_of(seconds):
    """The pids of this user's `sleep <seconds>` processes."""
    found = []
    for pid in os.listdir("/proc"):
        try:
            if pid.isdigit() and open(f"/proc/{pid}/cmdline").read().split("\0")[:2] == ["sleep", seconds]:
                found.append(pid)
        except OSError:
            pass
    return found


def server_argv(rig, index):
    """The command real rsync has its remote shell run for the fake rsync's call <index>, user and host
    dropped: the argv of the rsync server that call starts on a real host. Replayed under the call's own
    RSYNC_PROTECT_ARGS, which takes the paths out of that argv."""
    shim = rig.root / "server-argv"
    shim.write_text('#!/bin/sh\n[ "$1" = -l ] && shift 2\nshift\necho "$@" > "$0.out"\nexit 1\n')
    shim.chmod(0o755)
    args = list(rig.calls("rsync")[index])
    args[args.index("-e") + 1] = str(shim)
    args = [re.sub(r"^[^@/:]+@[^:/]+:", "u@h:", a) for a in args]
    protect = json.loads(pathlib.Path(f"{rig.logs['rsync']}.env").read_text().splitlines()[index])
    env = {k: v for k, v in os.environ.items() if k != "RSYNC_PROTECT_ARGS"}
    if protect is not None:
        env["RSYNC_PROTECT_ARGS"] = protect
    subprocess.run(["rsync", *args], capture_output=True, timeout=30, cwd=rig.tree, env=env)
    return pathlib.Path(f"{shim}.out").read_text().split()


STAND_IN = """#!/bin/sh
# A host's rsync server, held while its gate exists.
while [ -e "$(dirname "$0")/gate" ]; do sleep 0.05; done
"""


def stand_in(rig, argv):
    """A process whose argv is <argv>, as a host's rsync server's is, until its gate goes or it is killed."""
    d = rig.root / f"stand-in-{len(list(rig.root.glob('stand-in-*')))}"
    d.mkdir()
    install(d / argv[0], STAND_IN)
    (d / "gate").write_text("")
    return subprocess.Popen([str(d / argv[0]), *argv[1:]], start_new_session=True), d / "gate"


def laptop_side(rig, pgid):
    """Process group <pgid> but for the host's hold session and its children: what a laptop that
    vanished takes with it, the host's side of the hold left as it is."""
    procs = {}
    for pid in os.listdir("/proc"):
        try:
            if pid.isdigit():
                fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
                procs[int(pid)] = (int(fields[1]), int(fields[2]))
        except OSError:
            pass
    holds = {pid for pid in procs if _holds(pid, str(rig.host_home()))}

    def held(pid):
        while pid > 1 and pid not in holds:
            pid = procs.get(pid, (0, 0))[0]
        return pid in holds
    return [pid for pid, (_, group) in procs.items() if group == pgid and not held(pid)]


def vanished(rig, *, timeout):
    """A ship whose laptop vanished mid-rsync: its gpu.sh and rsync gone, a stand-in of the rsync server
    it started left on the host. The laptop has RSYNC_PROTECT_ARGS=1, which ship overrides.
    Returns the ship, the stand-in and its gate."""
    gate = rig.root / "rsync-gate"
    gate.write_text("")
    before = len(rig.calls("rsync"))
    env = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": timeout, "FAKE_RSYNC_GATE": str(gate),
           "RSYNC_PROTECT_ARGS": "1"}
    ship = rig.gpu(NAME, "ship", env=env, background=True)
    until(lambda: len(rig.calls("rsync")) > before, "the ship's rsync starts")
    argv = server_argv(rig, before)
    assert argv[:2] == ["rsync", "--server"] and argv[-2:] == [".", f"peacock/{INSTANCE}/cpp/install/"], argv
    assert f"--timeout={timeout}" in argv, argv
    server, server_gate = stand_in(rig, argv)
    for pid in laptop_side(rig, ship.pid):
        with contextlib.suppress(ProcessLookupError):  # the heartbeat's sleep may end in between
            os.kill(pid, signal.SIGKILL)
    ship.wait()
    gate.unlink()
    return ship, server, server_gate


def ended(*processes):
    for process in processes:
        for kill in (os.killpg, os.kill):
            try:
                kill(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait()


def test_a_ship_killed_alone_keeps_its_locks_while_the_host_s_rsync_server_writes():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))  # a state, so the instance's probe answers at once
        ship, server, gate = vanished(rig, timeout="2")
        try:
            # The hold's stdin ended with the fake ssh: time for a hold that does not wait on the server
            # to let go, so that its not waiting shows.
            time.sleep(4)
            refused(rig.gpu(NAME, "run", "list"), "busy")
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            assert len(device_runs(rig)) == 1, "no run on binaries half shipped"
            gate.unlink()
            server.wait(timeout=10)
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the hold lets go once the server ends",
                  seconds=20)
            ok(rig.gpu(NAME, "run", "list"))
        finally:
            ended(ship, server)


def test_a_half_open_ship_server_has_the_hold_s_timeout_and_holds_the_instance_until_it_ends():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        ship, server, _ = vanished(rig, timeout="6")  # its --timeout keeps it sending, so the host's TCP ends it
        try:
            time.sleep(1)  # past the hold's read, ended with the fake ssh
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            ended(server)
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the hold lets go once the server ends",
                  seconds=10)
        finally:
            ended(ship, server)


def test_a_pull_s_server_of_the_instance_holds_no_ship():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        before = len(rig.calls("rsync"))
        ok(rig.gpu(NAME, "pull"))
        argv = server_argv(rig, before)
        assert argv[:3] == ["rsync", "--server", "--sender"], argv
        server, _ = stand_in(rig, argv)  # another machine's pull, going on or half-open
        before = len(rig.calls("rsync"))
        ship = rig.gpu(NAME, "ship", background=True)
        try:
            out, err = ship.communicate(timeout=60)
            assert ship.returncode == 0 and "shipped to" in out, (ship.returncode, out, err)
            calls = rig.calls("rsync")[before:]
            assert len(calls) == 2 and all("--timeout=300" in c for c in calls), calls
            assert server.poll() is None
            assert rig.on_host("busy").stdout.strip() == "idle"
        finally:
            ended(ship, server)


def test_an_instance_s_hold_waits_on_no_server_of_another_instance():
    with GpuRig() as rig:
        shipped(rig)
        # dev1.A's pattern, its dot unescaped, matches dev1-A's server.
        server, _ = stand_in(rig, ["rsync", "--server", "-logDtpre.iLsfxCIvu", "--delete", ".",
                                   f"peacock/{INSTANCE}/cpp/install/"])
        hold = rig.on_host("hold", "1", f"peacock/{INSTANCE.replace('-', '.')}", background=True)
        try:
            assert hold.communicate(timeout=20)[0] == "held\n"
        finally:
            ended(hold, server)


def test_each_of_ship_s_two_servers_holds_the_instance_s_hold():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "ship", env={"RSYNC_PROTECT_ARGS": "1"}))  # a laptop's, which ship overrides
        calls = rig.calls("rsync")[-2:]
        assert all("--delete" in c for c in calls) and "--relative" in calls[1], calls
        for index in (-2, -1):
            server, gate = stand_in(rig, server_argv(rig, index))
            hold = rig.on_host("hold", "1", f"peacock/{INSTANCE}", background=True)
            try:
                with contextlib.suppress(subprocess.TimeoutExpired):
                    hold.communicate(timeout=3)
                    raise AssertionError(f"the hold let go under the server of {calls[index]}")
                gate.unlink()
                assert hold.communicate(timeout=15)[0] == "held\n"
            finally:
                ended(hold, server)


def test_a_hold_that_lapses_while_the_command_works_is_named():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env={"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"},
                      background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        # The heartbeat stalled, as on a laptop asleep or a gpu.sh stopped: the beater and its sleep.
        beaters = []
        until(lambda: beaters.extend(_beaters(run.pid)) or beaters, "the heartbeat is found")
        for pid in beaters:
            os.kill(pid, signal.SIGSTOP)
        until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the host's hold times out", seconds=20)
        for pid in beaters:
            os.kill(pid, signal.SIGCONT)
        (rig.host / "hold_smi").unlink()
        out, err = run.communicate(timeout=60)
        assert run.returncode == 0 and "green" in out, (run.returncode, err)
        assert "the hold on l40s ended early (exit 0)" in err, err


def _beaters(gpu_pid):
    """gpu.sh's heartbeat subshell and its sleep: the sleep whose parent's parent is gpu.sh."""
    def parent(pid):
        return int(open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[1])
    for pid in sleeps_of("1"):
        try:
            loop = parent(pid)
            if parent(loop) == gpu_pid:
                return [loop, int(pid)]
        except OSError:
            pass
    return []


def test_a_command_neither_waits_for_nor_leaves_its_heartbeat_s_sleep():
    with GpuRig() as rig:
        shipped(rig)
        start = time.time()
        ok(rig.gpu(NAME, "run", "list", env={"PEACOCK_HOLD_BEAT": "29"}))
        assert time.time() - start < 15, f"{time.time() - start:.1f}s: the release waited out the heartbeat's sleep"
        assert sleeps_of("29") == [], "the heartbeat's sleep outlived the command"


def test_a_second_install_waits_and_says_so():
    machine = pathlib.Path("/run/lock/peacock-install.lock")

    def stamp():
        return machine.stat().st_mtime_ns if machine.exists() else None
    with GpuRig() as rig:
        before = stamp()
        with open(rig.host / "install.lock", "a") as lock:  # another user's prepare, mid-install
            fcntl.flock(lock, fcntl.LOCK_EX)
            prepare = rig.gpu(NAME, "prepare", background=True)
            line = stderr_line(prepare, "waiting on")
            assert HOSTNAME in line and str(rig.host / "install.lock") in line, line
            deadline = time.time() + 2
            while time.time() < deadline:
                assert prepare.poll() is None, "prepare ended while another install held the lock"
                assert not [c for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"], "pulled while waiting"
                time.sleep(0.05)
        assert prepare.wait(timeout=60) == 0, prepare.communicate()
        assert stamp() == before, "the rig's own lock, never the machine's"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
