"""`run-gpu.sh` and the VM's reach: a sync is retried whole when its transport drops; the build
image reaches what prepare generates at a container path; a run is given the image whose labels
were read, by its ID; every card's driver is held to what that image needs; a second prepare
rewrites the files it wrote on the VM.

gpu_host.py has the fakes; test_run_gpu.py has the verbs themselves.
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import DRIVER_MINIMUM, DRIVER_MINIMUM_PTX, HOSTNAME, Host, label, until  # noqa: E402
from machine import lib  # noqa: E402


STAGED = "prototype_fused_benchmarks"


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, code, *words):
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    for w in words:
        assert w in r.stderr, (w, r.stderr)


def shipped(h):
    """Prepared and shipped, with a staged binary in the install."""
    exe = h.tree / "cpp/install/rust-benchmarks" / STAGED
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_text("staged")
    exe.chmod(0o755)
    ok(h.gpu("prepare", "--no-sf40"))
    ok(h.gpu("ship"))
    return h


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


def test_prepare_generates_sf1_at_a_container_path():
    with Host() as h:
        ok(h.gpu("prepare", "--no-sf40"))
        data_set = h.data / lib(h.tree, "data-inputs.sh", f'data_inputs "{h.tree}"')
        runs = [a for a in h.docker_calls() if a[0] == "run" and "-d" not in a]
        assert len(runs) == 2, runs
        for run in runs:
            assert f"type=bind,src={data_set}.partial,dst=/peacock/stage" in run, run
            assert run[run.index("-w") + 1] == "/peacock/stage", run
            assert "/peacock/stage/generate_testdata.sh" in run, run
        assert (data_set / "tpch.sf1" / "generated").is_file(), list(h.data.iterdir())


# --- the run image by its ID; every card's driver ------------------------------------------------

def test_run_runs_the_image_id_its_labels_were_read_from():
    with Host() as h:
        shipped(h)
        images = h.world()["images"]
        stale = {"id": "sha256:run2", "labels": {**images[h.run_ref]["labels"],
                                                "peacockdb.inputs": label(h.tree, "run", change="docker/run-tests.sh")}}
        for args in (("run", "list"), ("run", STAGED)):
            h.world(moves={h.run_ref: [1, stale]}, images=images)
            before = len(h.docker_calls())
            ok(h.gpu(*args))
            calls = h.docker_calls()[before:]
            inspects = [a for a in calls if a[:2] == ["image", "inspect"] and a[-1] == h.run_ref]
            assert len(inspects) == 1, inspects
            (run,) = [a for a in calls if a[:1] == ["run"] and "--gpus" in a]
            assert "sha256:run1" in run and h.run_ref not in run and "sha256:run2" not in run, run


def test_prepare_copies_cudf_from_the_image_id_it_read():
    with Host() as h:
        images = h.world()["images"]
        stale = {**images[h.run_ref], "id": "sha256:run2"}
        # The tag moves after prepare's second read of it: the pull's label check, then the ID's.
        h.world(moves={h.run_ref: [2, stale]})
        ok(h.gpu("prepare"))
        cudf = h.home / "peacock" / "cudf-25.02"
        assert (cudf / ".image-id").read_text().strip() == "sha256:run1"
        assert (cudf / "lib" / "libcudf.so").read_text() == "sha256:run1"


def test_every_card_s_driver_is_held_to_the_minimum():
    with Host() as h:
        shipped(h)
        h.world(cards=[["550.54.15", "8.9"], ["530.30.02", "8.9"]])
        refused(h.gpu("run", "list"), 3, "530.30.02", DRIVER_MINIMUM)
        refused(h.gpu("prepare", "--no-sf40"), 3, "530.30.02", DRIVER_MINIMUM)
        assert h.docker_runs() == []


def test_a_card_below_compute_capability_8_is_refused():
    with Host() as h:
        shipped(h)
        h.world(cards=[["580.65.06", "7.5"]])
        refused(h.gpu("run", "list"), 3, "7.5", "compute capability")
        assert h.docker_runs() == []


def test_a_card_past_9_needs_the_driver_that_compiles_its_ptx():
    with Host() as h:
        shipped(h)
        h.world(cards=[["560.35.03", "10.0"]])
        refused(h.gpu("run", "list"), 3, "560.35.03", DRIVER_MINIMUM_PTX, "PTX", "10.0")
        assert h.docker_runs() == []
        for card in (["575.51.03", "10.0"], ["580.65.06", "12.0"], ["560.35.03", "9.0"], ["560.35.03", "8.0"]):
            h.world(cards=[card])
            ok(h.gpu("run", "list"))
        assert len(h.docker_runs()) == 4


def test_a_driver_answer_that_is_no_version_is_refused():
    with Host() as h:
        shipped(h)
        for answer in ("N/A", "580.65.06 extra", "580, 8.9", "580.65.06", "580.65.06, 8.9, x", "580.65.06, 8"):
            h.world(cards=[answer])
            r = h.gpu("run", "list")
            refused(r, 2, "nvidia-smi", f"'{answer}'")
        h.world(cards=[])
        refused(h.gpu("run", "list"), 2, "nvidia-smi", "no GPU")
        assert h.docker_runs() == []


def test_a_second_prepare_rewrites_the_profiling_conf():
    # uutils' install (Ubuntu 26.04) refuses /dev/stdin onto a file that exists.
    with Host() as h:
        conf = h.root / "etc/modprobe.d/peacock-profiling.conf"
        conf.parent.mkdir(parents=True, exist_ok=True)
        conf.write_text("an earlier prepare's\n")
        ok(h.gpu("prepare"))
        assert conf.read_text() == "options nvidia NVreg_RestrictProfilingToAdminUsers=0\n", conf.read_text()
        assert list(h.vm_tmp.iterdir()) == [], list(h.vm_tmp.iterdir())


def test_a_driver_query_that_exits_255_is_not_a_dropped_ssh():
    # nvidia-smi's 255 is "other error"; ssh passes a remote status through, and 255 is its own.
    with Host() as h:
        shipped(h)
        text = "Unable to determine the device handle for GPU0000:01:00.0: Unknown Error"
        h.world(smi_fails=[255, text])
        r = h.gpu("run", "list")
        refused(r, 2, "driver query", text, "exit 254 (255 on the VM)")
        assert "ssh to the VM dropped" not in r.stderr and "run this step again" not in r.stderr, r.stderr
        assert h.docker_runs() == []


def test_a_failing_nvidia_smi_is_refused_naming_its_answer():
    with Host() as h:
        shipped(h)
        for code, text in ((6, "No devices were found"),
                           (9, "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver."),
                           (18, "Failed to initialize NVML: Driver/library version mismatch")):
            h.world(smi_fails=[code, text])
            refused(h.gpu("run", "list"), 2, "driver query", text, f"exit {code}")
        h.world(smi_fails=None)
        (h.host_bin / "nvidia-smi").unlink()
        r = h.gpu("run", "list")
        refused(r, 2)
        # The shell's own text, on stderr on the VM, is in the refusal itself.
        (line,) = [ln for ln in r.stderr.splitlines() if "driver query" in ln]
        assert "exit 127" in line and "nvidia-smi: command not found" in line, r.stderr
        assert h.docker_runs() == []


def test_nebius_sh_s_own_exit_at_the_driver_query_passes_on():
    with Host() as h:
        shipped(h)
        for rc in (69, 75):
            r = h.gpu("run", "list", env={"FAKE_NEBIUS_RC": str(rc), "FAKE_NEBIUS_RC_ON": "nvidia-smi"})
            assert r.returncode == rc, (rc, r.returncode, r.stderr)
        assert h.docker_runs() == []


def test_a_card_is_held_to_the_higher_of_the_two_minima():
    """26.02's run image needs a newer driver than the PTX does: a card past 9.x is held to the
    image's minimum then, not the PTX's."""
    with Host() as h:
        shipped(h)
        images = h.world()["images"]
        images[h.run_ref]["labels"].update({"peacockdb.driver-minimum": "575.57.08",
                                            "peacockdb.driver-minimum-ptx": "575.51.03"})
        h.world(images=images)
        for card in (["575.51.03", "10.0"], ["575.55.00", "8.9"]):
            h.world(cards=[card])
            r = h.gpu("run", "list")
            refused(r, 3, card[0], "575.57.08")
            assert "PTX" not in r.stderr, r.stderr
        h.world(cards=[["575.57.08", "10.0"]])
        ok(h.gpu("run", "list"))
        assert len(h.docker_runs()) == 1


def test_a_run_image_without_the_ptx_minimum_is_refused():
    for minimum in (None, ""):
        with Host() as h:
            shipped(h)
            images = h.world()["images"]
            if minimum is None:
                del images[h.run_ref]["labels"]["peacockdb.driver-minimum-ptx"]
            else:
                images[h.run_ref]["labels"]["peacockdb.driver-minimum-ptx"] = minimum
            h.world(images=images)
            for args in (("run", "list"), ("prepare", "--no-sf40")):
                refused(h.gpu(*args), 2, "peacockdb.driver-minimum-ptx", "scripts/env/image.sh", h.run_ref)
            assert h.docker_runs() == []


def test_the_runner_judges_only_what_it_is_told_to():
    """A staged binary's judgment, a segfault or `running 0 tests` in its log, is the caller's to ask
    for: a run told not to ends with its container's code, whatever its name."""
    with Host() as h:
        h.world(run_output="running 0 tests\n")
        for judged, code in ((0, "0"), (1, "1")):
            d = f"judged-{judged}"
            ok(h.vm_call("vm_started", d, "probe", h.card_lock, judged, "sha256:run1", "probe"))
            status = h.vm_dir(d) / "run" / "status"
            until(status.exists, "the run's status")
            assert status.read_text().split()[1] == code, (judged, status.read_text())


if __name__ == "__main__":
    raise SystemExit(main(globals()))
