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
from gpu_host import DRIVER_MINIMUM, H100, HOSTNAME, L40S, Host, label, log_lines, until  # noqa: E402
from machine import KEYS, NO_HOST, REPO, UNPUBLISHED, lib  # noqa: E402

VERBS = [("prepare",), ("ship",), ("run", "list"), ("pull",)]
SECRETS = [KEYS["AWS_ACCESS_KEY_ID"], KEYS["AWS_SECRET_ACCESS_KEY"]]
# What CI's image-ref.sh prints: a published image by digest.
GIVEN_RUN = "ghcr.io/asymptote-tech/peacockdb-run@sha256:" + "ab" * 32
GIVEN_BUILD = "ghcr.io/asymptote-tech/peacockdb-build@sha256:" + "cd" * 32
# The run image's ID in gpu_host.py's world: what a device run is given.
RUN_ID = "sha256:run1"


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, code, *words):
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    for w in words:
        assert w in r.stderr, (w, r.stderr)


def values(argv, flag):
    return [argv[i + 1] for i, token in enumerate(argv[:-1]) if token == flag]


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
        for verb in [*VERBS, ("status",)]:
            refused(h.gpu(*verb, vm=None), 2, "--instance-id")
            refused(h.gpu("--profile", "editor-eu", *verb, vm=None), 2, "--instance-id")
        assert h.nebius_calls() == []
        nothing_bypassed(h)


def test_run_gpu_runs_cis_sequence_on_nebius_gpu():
    """gpu-tests' calls, against the L40S, named by its id with no profile, the install relative, in
    ~/peacock/ci/ as container peacock-ci: never the dir and container of a clone named peacockdb."""
    with Host() as h:
        ok(h.gpu(*L40S, "--dir", "ci", "prepare", "--no-sf40", vm=None))
        ok(h.gpu(*L40S, "--dir", "ci", "ship", "--install", "cpp/install", vm=None))
        r = ok(h.gpu(*L40S, "--dir", "ci", "run", "list", vm=None))
        assert "run-tests.sh list: green" in r.stdout, r.stdout
        for a in h.nebius_calls():
            assert a[:2] == L40S and a[2] in ("ssh", "rsh"), a
        assert (h.vm_dir("ci") / "cpp/install/lib/libpeacock_gpu.so").read_text() == "staged"
        assert [values(a, "--name") for a in h.docker_runs()] == [["peacock-ci"]], h.docker_runs()
        assert not h.vm_dir().exists()
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
        refused(h.gpu("run", "every thing"), 2, "every thing")
        refused(h.gpu("--dir", "../up", "ship"), 2, "--dir")
        refused(h.gpu("--cudf", "24.12", "prepare"), 2, "24.12")
        refused(h.gpu("prepare", "--no-sf41"), 2, "--no-sf41")
        refused(h.gpu("status", "list"), 2, "status")
        refused(h.gpu("run", "--detach"), 2, "run")
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
        assert set(pulled) == {h.run_ref, h.build_ref}, pulled
        cudf = h.home / "peacock" / "cudf-25.02"
        assert (cudf / "lib" / "libcudf.so").read_text() == RUN_ID
        assert (cudf / ".image-id").read_text().strip() == "sha256:run1"
        assert not any("update-initramfs" in ln or "apt-get" in ln for ln in h.host_lines()), h.host_lines()
        nothing_bypassed(h)


def test_prepare_pulls_the_given_images():
    with Host() as h:
        images = h.world()["images"]
        h.world(images={GIVEN_RUN: images.pop(h.run_ref), GIVEN_BUILD: images.pop(h.build_ref)})
        ok(h.gpu("prepare", env={"PEACOCK_RUN_IMAGE": GIVEN_RUN, "PEACOCK_BUILD_IMAGE": GIVEN_BUILD}))
        pulled = [a[-1] for a in h.docker_calls() if a[0] == "pull"]
        assert sorted(pulled) == sorted([GIVEN_RUN, GIVEN_BUILD]), pulled
        assert (h.home / "peacock/cudf-25.02/lib/libcudf.so").read_text() == RUN_ID
        generated = [a for a in h.docker_calls() if a[0] == "run" and "--gpus" not in a]
        assert generated and all(GIVEN_BUILD in a for a in generated), generated


def test_a_given_image_is_checked_like_the_tag():
    with Host() as h:
        images = h.world()["images"]
        images[GIVEN_RUN] = images.pop(h.run_ref)
        images[GIVEN_RUN]["labels"]["peacockdb.inputs"] = label(h.tree, "run", change="docker/run-tests.sh")
        h.world(images=images)
        refused(h.gpu("prepare", env={"PEACOCK_RUN_IMAGE": GIVEN_RUN}), 2, "docker/run-tests.sh", GIVEN_RUN)


def test_prepare_no_sf40_reads_no_env_file():
    with Host() as h:
        (h.laptop / ".config/peacock/peacock.env").unlink()
        ok(h.gpu("prepare", "--no-sf40"))
        assert h.fetches() == []
        assert (h.data / data_hash(h) / "tpch.sf1" / "generated").is_file()
        assert not (h.data / "tpch.sf40").exists() and not (h.data / "tpcds.sf40").exists()


def test_prepare_no_sf40_skips_the_nsight_setup():
    with Host() as h:
        h.params.write_text("RmProfilingAdminOnly: 1\n")
        (h.host_bin / "nsys").unlink()
        ok(h.gpu("prepare", "--no-sf40"))
        assert not (h.root / "etc/modprobe.d/peacock-profiling.conf").exists()
        assert not (h.home / "peacock" / "cudf-25.02").exists()
        assert not any(a[0] in ("create", "cp") for a in h.docker_calls()), h.docker_calls()
        assert not any(ln.startswith(("update-initramfs", "apt-get")) for ln in h.host_lines()), h.host_lines()


def test_prepare_twice_copies_the_cudf_prefix_once():
    with Host() as h:
        prepared(h)
        prepared(h)
        assert [a[0] for a in h.docker_calls()].count("cp") == 1, h.docker_calls()
        h.world(images={**h.world()["images"],
                        h.run_ref: {**h.world()["images"][h.run_ref], "id": "sha256:run2"}})
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
        images[h.run_ref]["labels"]["peacockdb.inputs"] = label(h.tree, "run", change="docker/run-tests.sh")
        h.world(images=images)
        r = h.gpu(*H100, "prepare")
        refused(r, 2, "docker/run-tests.sh", h.run_ref)
        assert h.fetches() == [] and not (h.home / "peacock" / "cudf-25.02").exists()


def test_prepare_refuses_stale_build_image():
    with Host() as h:
        images = h.world()["images"]
        images[h.build_ref]["labels"]["peacockdb.inputs"] = label(h.tree, "build", change="docker/venv.lock")
        h.world(images=images)
        refused(h.gpu("prepare"), 2, "docker/venv.lock", h.build_ref)
        assert not any(h.data.iterdir())


def test_prepare_names_the_publish_of_an_unpublished_image():
    for kind in ("run", "build"):
        with Host() as h:
            ref = h.run_ref if kind == "run" else h.build_ref
            images = h.world()["images"]
            images.pop(ref)
            h.world(images=images)
            r = h.gpu("prepare", "--no-sf40")
            refused(r, 2, f"{ref} is not published",
                    f"gh workflow run images.yml --ref master -f kind={kind} -f cudf=25.02")
            assert h.docker_runs() == [] and not any(h.data.iterdir())
    with Host() as h:
        images = h.world()["images"]
        images.pop(h.run_ref)
        h.world(images=images, pull_error=UNPUBLISHED[1].format(ref=h.run_ref))
        refused(h.gpu("prepare", "--no-sf40"), 2, f"{h.run_ref} is not published")


def test_prepare_names_a_pull_that_failed_otherwise():
    with Host() as h:
        images = h.world()["images"]
        images.pop(h.run_ref)
        h.world(images=images, pull_error=NO_HOST)
        refused(h.gpu("prepare", "--no-sf40"), 1, f"cannot pull {h.run_ref}", NO_HOST,
                "gh workflow run images.yml --ref master -f kind=run -f cudf=25.02")


def test_prepare_refuses_old_driver():
    with Host() as h:
        h.world(cards=[["530.30.02", "8.9"]])
        refused(h.gpu("prepare"), 3, "530.30.02", DRIVER_MINIMUM)
        assert h.fetches() == []


def test_prepare_refuses_a_run_image_without_a_driver_minimum():
    # Absent, docker prints "<no value>"; present and empty, it prints nothing. Both are no minimum.
    for minimum in (None, ""):
        with Host() as h:
            images = h.world()["images"]
            if minimum is None:
                del images[h.run_ref]["labels"]["peacockdb.driver-minimum"]
            else:
                images[h.run_ref]["labels"]["peacockdb.driver-minimum"] = minimum
            h.world(images=images)
            refused(h.gpu("prepare"), 2, "no peacockdb.driver-minimum label", h.run_ref)
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


def test_ship_after_prepare_no_sf40_links_sf1_alone():
    with Host() as h:
        ok(h.gpu("prepare", "--no-sf40"))
        ok(h.gpu("ship"))
        t = h.vm_dir() / "testdata"
        for ds in ("tpch.sf1", "tpcds.sf1"):
            assert os.readlink(t / ds) == str(h.data / data_hash(h) / ds)
        assert not os.path.lexists(t / "tpch.sf40") and not os.path.lexists(t / "tpcds.sf40")


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


# --- ship's transport, once the old CI runner's ----------------------------------------------------

# rsync on dev: records each call; with FAKE_RSYNC_FAIL=<text>:<n>, the first <n> calls naming <text>
# fail as a dropped connection does; with FAKE_RSYNC_BARRIER=<dir>, a call waits for 4 at once.
FAKE_RSYNC = r'''#!/usr/bin/env python3
import json, os, pathlib, sys, time
a = sys.argv[1:]
log = os.environ["FAKE_RSYNC_LOG"]
with open(log, "a") as f:
    f.write(json.dumps(a) + "\n")
fail = os.environ.get("FAKE_RSYNC_FAIL")
if fail:
    text, n = fail.rsplit(":", 1)
    if any(text in x for x in a) and sum(text in ln for ln in open(log)) <= int(n):
        sys.exit("rsync: connection unexpectedly closed (0 bytes received so far) [sender]")
barrier = os.environ.get("FAKE_RSYNC_BARRIER")
if barrier:
    pathlib.Path(barrier, str(os.getpid())).touch()
    deadline = time.time() + 20
    while len(os.listdir(barrier)) < 4:
        if time.time() > deadline:
            sys.exit("fake rsync: the streams ran one at a time")
        time.sleep(0.05)
os.execv(os.environ["FAKE_REAL_RSYNC"], ["rsync", *a])
'''


def rsync_env(h, **extra):
    """The env that puts FAKE_RSYNC first on dev's PATH; its calls are in h.tree/../rsync.log."""
    d = h.tree.parent / "rsync-bin"
    d.mkdir(exist_ok=True)
    (d / "rsync").write_text(FAKE_RSYNC)
    (d / "rsync").chmod(0o755)
    return {"PATH": f"{d}:{h.env['PATH']}", "FAKE_RSYNC_LOG": str(h.tree.parent / "rsync.log"),
            "FAKE_REAL_RSYNC": shutil.which("rsync"), "RUN_GPU_SHIP_PAUSE": "0", **extra}


def rsync_calls(h):
    return log_lines(h.tree.parent / "rsync.log")


def test_ship_keeps_symlinks_and_exec_bits():
    with Host() as h:
        prepared(h)
        lib_dir = h.tree / "cpp/install/lib"
        (lib_dir / "libx.so.1.0").write_bytes(os.urandom(4096))
        (lib_dir / "libx.so.1").symlink_to("libx.so.1.0")
        (lib_dir / "libx.so").symlink_to("libx.so.1")
        for rel in ("bin/peacock_gpu_tests", "rust-tests/test_gpu_corpus"):
            (h.tree / "cpp/install" / rel).chmod(0o755)
        ok(h.gpu("ship"))
        shipped = h.vm_dir() / "cpp/install"
        assert os.readlink(shipped / "lib/libx.so") == "libx.so.1"
        assert os.readlink(shipped / "lib/libx.so.1") == "libx.so.1.0"
        for rel in ("bin/peacock_gpu_tests", "rust-tests/test_gpu_corpus"):
            assert os.access(shipped / rel, os.X_OK), rel


def test_ship_sends_a_rebuilt_file_of_the_same_size_and_time():
    """A rebuild that keeps a binary's size and second is still sent: --checksum, not size and mtime."""
    with Host() as h:
        prepared(h)
        built = h.tree / "cpp/install/lib/libpeacock_gpu.so"
        ok(h.gpu("ship"))
        st = built.stat()
        built.write_text("STAGED")
        os.utime(built, ns=(st.st_atime_ns, st.st_mtime_ns))
        ok(h.gpu("ship"))
        assert (h.vm_dir() / "cpp/install/lib/libpeacock_gpu.so").read_text() == "STAGED"


def test_ship_restores_a_committed_file_a_run_changed():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        golden = h.vm_dir() / "testdata/goldens/tpch.sf1/q01.txt"
        golden.write_text("rewritten by a test\n")
        ok(h.gpu("ship"))
        assert golden.read_text() == "golden\n"


def test_ship_refuses_an_install_without_lib_or_rust_tests():
    with Host() as h:
        prepared(h)
        before = len(h.nebius_calls())
        for gone in ("lib", "rust-tests"):
            other = Host.stage_install(h.tree.parent / f"no-{gone}")
            shutil.rmtree(other / gone)
            refused(h.gpu("ship", "--install", other), 2, f"{other}/{gone}")
        assert len(h.nebius_calls()) == before


def test_ship_retries_a_failed_stream_and_resumes():
    with Host() as h:
        prepared(h)
        r = ok(h.gpu("ship", env=rsync_env(h, FAKE_RSYNC_FAIL="cpp/install/lib/:2")))
        lib_calls = [a for a in rsync_calls(h) if any("cpp/install/lib/" in x for x in a)]
        assert len(lib_calls) == 3, lib_calls
        assert all("--partial-dir=.rsync-partial" in a for a in lib_calls), lib_calls
        assert "lib stream: attempt 2 failed" in r.stderr, r.stderr
        assert (h.vm_dir() / "cpp/install/lib/libpeacock_gpu.so").read_text() == "staged"


def test_ship_gives_up_after_five():
    with Host() as h:
        prepared(h)
        r = h.gpu("ship", env=rsync_env(h, FAKE_RSYNC_FAIL="cpp/install/lib/:99"))
        refused(r, 1, "the lib stream failed 5 times", "connection unexpectedly closed")
        lib_calls = [a for a in rsync_calls(h) if any("cpp/install/lib/" in x for x in a)]
        assert len(lib_calls) == 5, lib_calls
        assert "the committed testdata to" not in r.stdout, r.stdout


def test_ship_runs_four_streams_at_once():
    with Host() as h:
        prepared(h)
        barrier = h.tree.parent / "barrier"
        barrier.mkdir()
        r = ok(h.gpu("ship", env=rsync_env(h, FAKE_RSYNC_BARRIER=str(barrier))))
        assert len(rsync_calls(h)) == 4, rsync_calls(h)
        for stream in ("lib", "rust-tests", "install", "testdata"):
            assert f"{stream} stream: done" in r.stdout, (stream, r.stdout)


def test_every_ship_rsync_compresses_resumes_and_checksums():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship", env=rsync_env(h)))
        calls = rsync_calls(h)
        assert len(calls) == 4, calls
        for a in calls:
            for f in ("-a", "--checksum", "--partial-dir=.rsync-partial", "--compress-choice=zstd",
                      "--compress-level=3", "--timeout=120"):
                assert f in a, (f, a)
        install = [a for a in calls if any("cpp/install/" in x for x in a)]
        assert len(install) == 3 and all("--delete" in a for a in install), install
        rest = [a for a in install if "--exclude=/lib/" in a]
        assert len(rest) == 1 and "--exclude=/rust-tests/" in rest[0], install


# --- run ---------------------------------------------------------------------------------------

def test_run_refuses_old_driver():
    with Host() as h:
        prepared(h)
        h.world(cards=[["530.30.02", "8.9"]])
        calls = len(h.nebius_calls())
        refused(h.gpu("run", "list"), 3, "530.30.02", DRIVER_MINIMUM)
        assert "rsh" not in h.verbs()[calls:] and h.docker_runs() == []


def test_run_before_prepare_says_so():
    with Host() as h:
        images = h.world()["images"]
        del images[h.run_ref]
        h.world(images=images)
        refused(h.gpu("run", "list"), 2, "prepare", h.run_ref)
        assert h.docker_runs() == []


def test_run_refuses_a_run_image_built_from_other_inputs():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        images = h.world()["images"]
        images[h.run_ref]["labels"]["peacockdb.inputs"] = label(h.tree, "run", change="docker/run-tests.sh")
        h.world(images=images)
        refused(h.gpu("run", "list"), 2, "docker/run-tests.sh", h.run_ref)
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
        assert inspected and inspected[0][-1] == h.run_ref, run_calls


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
        assert a[-2:] == [RUN_ID, "list"], a


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


def test_run_passes_the_case_timeout_and_the_answer_switch():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        env = {"PEACOCK_BENCH_RESUME": "1", "PEACOCK_BENCH_CASE_TIMEOUT_S": "1800", "PCK_WRITE_GPU_RESULT": "1"}
        ok(h.gpu("run", "bench", env=env))
        a = h.docker_runs()[0]
        envs = [a[i + 1] for i, x in enumerate(a) if x == "-e"]
        assert envs == ["PEACOCK_BENCH_RESUME=1", "PEACOCK_BENCH_CASE_TIMEOUT_S=1800", "PCK_WRITE_GPU_RESULT=1"], envs
        ok(h.gpu("run", "bench", env={"PEACOCK_BENCH_CASE_TIMEOUT_S": "", "PCK_WRITE_GPU_RESULT": ""}))
        a = h.docker_runs()[1]
        assert "-e" not in a, a


def test_run_refuses_a_run_in_progress():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        h.hold.touch()
        ok(h.gpu("run", "--detach", "list"))
        refused(h.gpu("run", "list"), 2, "follow")
        refused(h.gpu("ship"), 2, "follow")
        assert len(h.docker_runs()) == 1


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


def test_run_uses_the_given_run_image():
    with Host() as h:
        images = h.world()["images"]
        images[GIVEN_RUN] = images.pop(h.run_ref)
        h.world(images=images)
        env = {"PEACOCK_RUN_IMAGE": GIVEN_RUN}
        ok(h.gpu("prepare", env=env))
        ok(h.gpu("ship"))
        ok(h.gpu("run", "list", env=env))
        a = h.docker_runs()[0]
        assert a[-2:] == [RUN_ID, "list"], a
        inspected = {a[-1] for a in h.docker_calls() if a[:2] == ["image", "inspect"]}
        assert h.run_ref not in inspected and GIVEN_RUN in inspected, inspected


def test_detached_run_returns_while_running():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        h.hold.touch()
        r = ok(h.gpu("run", "--detach", "list"))
        assert "status" in r.stdout and "follow" in r.stdout, r.stdout
        until(lambda: h.world()["containers"].get("peacock-chain-l", {}).get("state") == "running", "it runs")
        refused(h.gpu("run", "list"), 2, "follow")


def test_status_reports_running_then_exit():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        r = h.gpu("status")
        assert r.returncode == 2 and r.stdout.splitlines()[0] == "none", (r.returncode, r.stdout, r.stderr)
        h.world(run_output="".join(f"line {i}\n" for i in range(100)), run_code=1)
        h.hold.touch()
        ok(h.gpu("run", "--detach", "filter", env={"PCK_TEST_FILTER": "q6"}))
        until(lambda: h.world()["containers"].get("peacock-chain-l", {}).get("state") == "running", "it runs")
        r = h.gpu("status")
        assert r.returncode == 5, (r.returncode, r.stdout, r.stderr)
        lines = r.stdout.splitlines()
        assert lines[0] == "running" and lines[1:] == [f"line {i}" for i in range(60, 100)], lines
        h.hold.unlink()
        until(lambda: h.gpu("status").returncode != 5, "it ended")
        r = h.gpu("status")
        assert r.returncode == 1 and r.stdout.splitlines()[0] == "exited 1", (r.returncode, r.stdout)
        assert [f.read_text().splitlines()[-1] for f in (h.vm_dir() / "logs").glob("filter-*.log")] == ["line 99"]
        h.world(run_code=0)
        ok(h.gpu("run", "--detach", "filter"))
        until(lambda: h.gpu("status").returncode != 5, "it ended")
        r = ok(h.gpu("status"))
        assert r.stdout.splitlines()[0] == "exited 0" and r.stdout.splitlines()[-1] == "line 99", r.stdout


def test_run_detach_refuses_a_staged_binary():
    with Host() as h:
        refused(h.gpu("run", "--detach", FUSED), 2, "--detach")
        assert h.nebius_calls() == []


def test_run_before_ship_says_so():
    with Host() as h:
        prepared(h)
        refused(h.gpu("run", "list"), 2, "ship")
        assert h.docker_runs() == []


# --- run, a staged binary by name -------------------------------------------------------------

FUSED = "prototype_fused_benchmarks"
FIRST3 = "bench_fused_tpch_sf40_first3_tp1_single"


def staged_fused(h):
    """Prepared and shipped, with the fused binary staged executable in the install."""
    exe = h.tree / "cpp/install/rust-benchmarks" / FUSED
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_text("staged")
    exe.chmod(0o755)
    prepared(h)
    ok(h.gpu("ship"))
    h.world(run_output="running 1 test\ntest result: ok. 1 passed\n")
    return h


def test_run_names_a_staged_binary():
    with Host() as h:
        staged_fused(h)
        r = ok(h.gpu("run", FUSED, "--exact", FIRST3, env={"PEACOCK_BENCH_RESUME": "1"}))
        assert "1 passed" in r.stdout, r.stdout
        runs = h.docker_runs()
        assert len(runs) == 1, runs
        a = runs[0]
        assert a[a.index("--entrypoint") + 1] == f"/peacock/install/rust-benchmarks/{FUSED}", a
        envs = [a[i + 1] for i, x in enumerate(a) if x == "-e"]
        assert envs == ["LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib", "PEACOCK_TESTDATA_DIR=/peacock/testdata",
                        "PEACOCK_BENCH_RESUME=1"], envs
        assert a[a.index(RUN_ID):] == [RUN_ID, "--nocapture", "--test-threads=1", "--exact", FIRST3], a
        assert not {"list", "filter", "bench"} & set(a), a
        assert a[a.index("--hostname") + 1] == HOSTNAME and a[a.index("--name") + 1] == "peacock-chain-l"
        mounts = [a[i + 1] for i, x in enumerate(a) if x == "-v"]
        assert f"{h.vm_dir()}/cpp/install:/peacock/install:ro" in mounts, mounts
        assert h.world()["containers"] == {}, "the run's container is left behind"


def test_run_staged_narrows_by_the_filter():
    with Host() as h:
        staged_fused(h)
        ok(h.gpu("run", FUSED, env={"PCK_TEST_FILTER": "tpcds"}))
        a = h.docker_runs()[0]
        assert a[a.index(RUN_ID):] == [RUN_ID, "--nocapture", "--test-threads=1", "tpcds"], a
        assert "PCK_TEST_FILTER=tpcds" in [a[i + 1] for i, x in enumerate(a) if x == "-e"], a


def test_run_staged_fails_on_a_segfault_in_the_log():
    with Host() as h:
        staged_fused(h)
        h.world(run_output="running 1 test\nSegmentation fault\n", run_code=0)
        r = h.gpu("run", FUSED, "--exact", FIRST3)
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)


def test_run_staged_fails_when_no_test_ran():
    with Host() as h:
        staged_fused(h)
        h.world(run_output="running 0 tests\ntest result: ok. 0 passed\n", run_code=0)
        r = h.gpu("run", FUSED, "--exact", "bench_fused_tpch_sf40_first4_tp1_single")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        h.world(run_output="running 1 test\n", run_code=101)
        assert h.gpu("run", FUSED, "--exact", FIRST3).returncode == 1


def test_run_staged_refuses_an_unstaged_name():
    with Host() as h:
        staged_fused(h)
        refused(h.gpu("run", "peacock_cpu_benchmarks"), 2, "peacock_cpu_benchmarks", "rust-benchmarks")
        assert h.docker_runs() == []


def test_run_staged_refuses_bad_arguments_before_the_vm():
    with Host() as h:
        refused(h.gpu("run", FUSED, "--exact", FIRST3, env={"PCK_TEST_FILTER": "q1"}), 2, "--exact",
                "PCK_TEST_FILTER")
        refused(h.gpu("run", FUSED, "--exact"), 2, "--exact")
        refused(h.gpu("run", FUSED, "--skip", "x"), 2, "--skip")
        refused(h.gpu("run", "../bin/sh"), 2, "../bin/sh")
        refused(h.gpu("run", "list", "--exact", FIRST3), 2, "list")
        assert h.nebius_calls() == []


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
        stamp = (h.vm_dir() / "run/state").read_text().split()[3]
        assert [p.name for p in (home / "calibration").glob("records*.tsv")] == [f"records-{stamp}.tsv"]
        logs = h.laptop / ".cache/peacock/run-gpu/chain-l"
        assert ["bench: green" in f.read_text() for f in logs.glob("bench-*.log")] == [True]
        assert str(logs) in r.stdout


def test_pull_brings_the_gpu_answers():
    with Host() as h:
        prepared(h)
        ok(h.gpu("ship"))
        g = h.vm_dir() / "testdata/goldens"
        (g / "tpch.sf40").mkdir(parents=True, exist_ok=True)
        (g / "tpch.sf40/gpu-result.txt").write_text("cudf=25.02\n== q1 mode=tp1-single\nrow\n")
        (g / "tpch.sf40/gpu-result-26.02.txt").write_text("cudf=26.02\n")
        (g / "tpch.sf40/duckdb-result.txt").write_text("written on the VM\n")
        r = ok(h.gpu("pull"))
        home = h.tree / "testdata/goldens/tpch.sf40"
        assert (home / "gpu-result.txt").read_text() == "cudf=25.02\n== q1 mode=tp1-single\nrow\n"
        assert (home / "gpu-result-26.02.txt").is_file()
        assert not (home / "duckdb-result.txt").exists(), "only the device's answers come home"
        assert "gpu-result" in r.stdout


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
STUB_LOADS = STUB_CALLS.replace("# calls", "# loads")
# As nsys_calls.py refuses a capture of a query with a join: a message and exit 1, nothing written.
REFUSING_CALLS = 'import sys\nsys.exit("capture does not match q3\'s recipes")\n'


STUB_HBM = r"""import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_HBM_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
open(a[a.index("--out") + 1], "w").write("# hbm\nrow\n")
"""
LOGGING_BENCH = ('#!/bin/sh\nprintf "%s\\n" "$*" >> "$FAKE_BENCH_LOG"\necho "running 1 test"\n'
                 'echo "test result: ok. 1 passed; 0 failed; 0 ignored"\n')


def nsys_tree(tmp, bench_text=FAKE_BENCH, calls=STUB_CALLS):
    """A tree holding create_nsys_profile.sh, a VM directory holding a benchmark binary, and the
    env that reaches the VM through a fake rsh alone."""
    tree = tmp / "tree"
    for rel in ("scripts/create_nsys_profile.sh", "scripts/lib/shadgpu-env.sh", "scripts/lib/peacock-config.sh",
                "scripts/lib/calibration-bucket.sh",
                "scripts/lib/stage-cargo.sh", "scripts/lib/bench-cases.sh", "scripts/calibration/device_cases.py",
                "scripts/calibration/record.py", "scripts/calibration/nvtx_names.py",
                "peacockdb-core/tests/common/corpus_benchmark_cases.inc"):
        if (REPO / rel).exists():
            (tree / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, tree / rel)
    (tree / "scripts/calibration").mkdir(parents=True, exist_ok=True)
    (tree / "scripts/calibration/nsys_calls.py").write_text(calls)
    (tree / "scripts/calibration/nsys_loads.py").write_text(STUB_LOADS)
    (tree / "scripts/calibration/nsys_hbm.py").write_text(STUB_HBM)
    cases = (tree / "peacockdb-core/tests/common/corpus_benchmark_cases.inc").read_text()
    for ds in set(re.findall(r"corpus_query_benchmark!\(\s*(\w+)\s*,", cases)):
        (tree / "testdata/goldens" / f"{ds}.sf1").mkdir(parents=True)
    vm = tmp / "vm" / "peacock" / "chain-l"
    (vm / "cpp/install/rust-benchmarks").mkdir(parents=True)
    bench = vm / "cpp/install/rust-benchmarks/peacock_gpu_benchmarks"
    bench.write_text(bench_text)
    bench.chmod(0o755)
    bins = {"host-bin": {"nsys": FAKE_NSYS}, "dev-bin": {"ssh": LOUD_SSH}, ".": {"rsh": FAKE_RSH}}
    for d, files in bins.items():
        (tmp / d).mkdir(exist_ok=True)
        for name, text in files.items():
            (tmp / d / name).write_text(text)
            (tmp / d / name).chmod(0o755)
    env = {**os.environ, "PATH": f"{tmp / 'dev-bin'}:{os.environ['PATH']}",
           "FAKE_HOST_PATH": f"{tmp / 'host-bin'}:{os.environ['PATH']}", "FAKE_RSH_LOG": str(tmp / "rsh.log"),
           "FAKE_DEV_LOG": str(tmp / "ssh.log"), "FAKE_BENCH_LOG": str(tmp / "bench.log"),
           "FAKE_HBM_LOG": str(tmp / "hbm.log"), "CARGO_TARGET_DIR": str(tmp / "target"),
           "PEACOCK_CARD_LOCK": str(tmp / "peacock-card.lock")}
    env.pop("PCK_TEST_FILTER", None)
    return tree, vm, env


def nsys_profile(tmp, tree, vm, env, *args):
    return subprocess.run(["bash", "scripts/create_nsys_profile.sh", *args, "--ssh", f"{tmp / 'rsh'} --vm h100",
                           "--host", "nebius-h100", "--remote-dir", str(vm), "--remote-cudf-root", str(tmp / "cudf")],
                          cwd=tree, env=env, capture_output=True, text=True, timeout=120)


def test_nsys_profile_uses_the_given_ssh():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp)
        r = nsys_profile(tmp, tree, vm, {**env, "PCK_TEST_FILTER": "bench_tpch_sf40_q6_tp1_single"}, "--trace")
        ok(r)
        assert "the trace pass ran 1 tests" in r.stdout, r.stdout
        assert (tree / "testdata/calibration/calls.tsv").is_file()
        assert (tree / "testdata/calibration/loads.tsv").is_file()
        assert (tree / "testdata/calibration/capture.sqlite").read_text() == "sqlite"
        assert not (tmp / "ssh.log").exists(), (tmp / "ssh.log").read_text()
        calls = log_lines(tmp / "rsh.log")
        assert all(c[:3] == ["--vm", "h100", "nebius-h100"] for c in calls), calls
        commands = [" ".join(c[3:]) for c in calls]
        assert any(c.startswith("test -x") for c in commands), commands
        assert any(c == "bash" for c in commands), commands
        assert any(c.startswith("rsync --server") for c in commands), commands
        assert "switches: cache=unset reader=unset" in r.stdout, r.stdout


def test_nsys_profile_writes_the_load_breakdown_when_the_call_breakdown_refuses():
    """nsys_calls.py refuses any capture of a query with a join; loads.tsv does not depend on it,
    so it is written, and the run still fails for the refusal."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp, calls=REFUSING_CALLS)
        r = nsys_profile(tmp, tree, vm, {**env, "PCK_TEST_FILTER": "bench_tpch_sf40_q6_tp1_single"}, "--trace")
        assert r.returncode != 0, (r.stdout, r.stderr)
        assert (tree / "testdata/calibration/loads.tsv").is_file(), (r.stdout, r.stderr)


def test_nsys_profile_passes_the_pinned_cache_and_scan_reader_switches():
    """The traced binary runs on the host, so a switch set here must reach it there."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp)
        switches = {"PCK_TEST_FILTER": "bench_tpch_sf40_q6_tp1_single", "PEACOCK_PINNED_HOST_CACHE_BYTES": "0",
                    "PEACOCK_SCAN_READER": "read_parquet"}
        r = nsys_profile(tmp, tree, vm, {**env, **switches}, "--trace")
        ok(r)
        assert "switches: cache=0 reader=read_parquet" in r.stdout, r.stdout


DEVICE_LIST = """\
dataset,sf,query,modes,status,note
tpch,40,q6,tp1_single,ok,
tpch,40,q1,tp1_single,wrong,#7
tpch,40,q9,,skipped,timeout: the case ran past 1800 s
tpcds,40,q37,tp1_single,ok,
"""


def test_cases_and_device_reach_the_passes():
    """`--cases` takes a device list's `ok` and `wrong` rows as the cases of both passes, the
    `skipped` row none; `--device` reads the records from the device's own directory."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp, LOGGING_BENCH)
        listed = tmp / "h100-cases.csv"
        listed.write_text(DEVICE_LIST)
        for ds in ("tpch", "tpcds"):
            record = tree / f"testdata/calibration/h100-80gb-hbm3/{ds}.sf40/records.tsv"
            record.parent.mkdir(parents=True)
            record.write_text("# run: device=NVIDIA H100 80GB HBM3\ndataset\n" + ds + "\n")
        r = nsys_profile(tmp, tree, vm, env, "--cases", listed, "--device", "h100-80gb-hbm3")
        ok(r)
        names = ["bench_tpch_sf40_q6_tp1_single", "bench_tpch_sf40_q1_tp1_single", "bench_tpcds_sf40_q37_tp1_single"]
        runs = (tmp / "bench.log").read_text().splitlines()
        assert runs[0] == " ".join(["--nocapture", "--test-threads=1", *names, "--exact"]), runs
        assert runs[1:] == [f"--nocapture --test-threads=1 {name} --exact" for name in names], runs
        assert "bench_tpch_sf40_q9_tp1_single" not in "".join(runs)
        assert "counters, case 3 of 3: tpcds.sf40 q37 tp1-single" in r.stdout, r.stdout
        readers = log_lines(tmp / "hbm.log")
        records = [a[i + 1] for a in readers for i, w in enumerate(a) if w == "--record"]
        assert set(records) == {"testdata/calibration/h100-80gb-hbm3/tpch.sf40/records.tsv",
                                "testdata/calibration/h100-80gb-hbm3/tpcds.sf40/records.tsv"}, readers
        cased = [a[a.index("--case") + 1] for a in readers if "--case" in a]
        assert cased == ["tpch.sf40 q6 tp1-single", "tpch.sf40 q1 tp1-single", "tpcds.sf40 q37 tp1-single"], cased
        assert (tree / "testdata/calibration/hbm.tsv").is_file()
        assert "--record testdata/calibration/h100-80gb-hbm3/tpch.sf40/records.tsv" in r.stdout, r.stdout


TWO_MODE_LIST = """\
dataset,sf,query,modes,status,note
tpch,40,q6,tp1_single|tp4_sized,ok,
tpch,40,q8,tp1_single,ok,
tpcds,40,q37,tp1_single|tp4_sized,ok,
"""


def test_cases_with_a_mode_take_that_mode_alone():
    """A list written at two modes makes a case per mode; `--mode` keeps one, for both passes,
    and the counters merge is held to that mode's calls of the two-mode record."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp, LOGGING_BENCH)
        listed = tmp / "h100-cases.csv"
        listed.write_text(TWO_MODE_LIST)
        for ds in ("tpch", "tpcds"):
            record = tree / f"testdata/calibration/h100-80gb-hbm3/{ds}.sf40/records.tsv"
            record.parent.mkdir(parents=True)
            record.write_text("# run: device=NVIDIA H100 80GB HBM3\ndataset\n" + ds + "\n")
        r = nsys_profile(tmp, tree, vm, env, "--cases", listed, "--mode", "tp4_sized",
                         "--device", "h100-80gb-hbm3")
        ok(r)
        names = ["bench_tpch_sf40_q6_tp4_sized", "bench_tpcds_sf40_q37_tp4_sized"]
        runs = (tmp / "bench.log").read_text().splitlines()
        assert runs[0] == " ".join(["--nocapture", "--test-threads=1", *names, "--exact"]), runs
        assert runs[1:] == [f"--nocapture --test-threads=1 {name} --exact" for name in names], runs
        readers = log_lines(tmp / "hbm.log")
        cased = [a[a.index("--case") + 1] for a in readers if "--case" in a]
        assert cased == ["tpch.sf40 q6 tp4-sized", "tpcds.sf40 q37 tp4-sized"], cased
        merges = [a for a in readers if "--parts" in a]
        assert len(merges) == 1 and merges[0][merges[0].index("--mode") + 1] == "tp4-sized", readers
        assert not any("--mode" in a for a in readers if "--case" in a), readers

        refused(nsys_profile(tmp, tree, vm, env, "--mode", "tp4_sized"), 1, "--mode")
        refused(nsys_profile(tmp, tree, vm, env, "--cases", listed, "--mode", "tp9_wide"), 1, "tp9_wide")
        refused(nsys_profile(tmp, tree, vm, env, "--cases", listed, "--mode", "tp4_rowgroup"), 1,
                "tp4_rowgroup")
        refused(nsys_profile(tmp, tree, vm, env, "--fused", "tpch.sf40", "--mode", "tp4_sized"), 1, "--mode")


def test_cases_with_a_filter_narrow_the_trace_pass():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp, LOGGING_BENCH)
        listed = tmp / "h100-cases.csv"
        listed.write_text(DEVICE_LIST)
        r = nsys_profile(tmp, tree, vm, {**env, "PCK_TEST_FILTER": "_tpch_"}, "--trace", "--cases", listed,
                         "--device", "h100-80gb-hbm3")
        ok(r)
        runs = (tmp / "bench.log").read_text().splitlines()
        assert runs == ["--nocapture --test-threads=1 bench_tpch_sf40_q6_tp1_single "
                        "bench_tpch_sf40_q1_tp1_single --exact"], runs
        refused(nsys_profile(tmp, tree, vm, env, "--cases", tmp / "missing.csv"), 1, "missing.csv")
        refused(nsys_profile(tmp, tree, vm, env, "--device", "H100 80GB"), 1, "--device")


# --- create_nsys_profile.sh --fused ------------------------------------------------------------

BATCHES = ["first3", "first10", "first20", "all"]
# The fused binary: logs its arguments and the record it was given, fails a batch named in
# FAKE_FAIL_BATCH.
FUSED_BENCH = ('#!/bin/sh\nprintf "%s record=%s\\n" "$*" "${PEACOCK_RECORD_PATH-unset}" >> "$FAKE_BENCH_LOG"\n'
               'case "$*" in *"_${FAKE_FAIL_BATCH:-none}_tp1_single"*) echo "boom"; exit 101 ;; esac\n'
               'echo "running 1 test"\necho "test result: ok. 1 passed; 0 failed; 0 ignored"\n')


# As the real reader, which cannot open a record that is not there.
STRICT_HBM = STUB_HBM.replace(
    "with open(os.environ",
    "missing = [a[i + 1] for i, w in enumerate(a) if w == \"--record\" and not os.path.exists(a[i + 1])]\n"
    "if missing:\n    sys.exit(f\"no record {missing}\")\nwith open(os.environ", 1)


def fused_tree(tmp, records=BATCHES):
    """nsys_tree with the fused binary staged beside the corpus one, and on the host a clean record
    per batch in `records`, as the fused runs leave them."""
    tree, vm, env = nsys_tree(tmp)
    (tree / "scripts/calibration/nsys_hbm.py").write_text(STRICT_HBM)
    bench = vm / "cpp/install/rust-benchmarks/prototype_fused_benchmarks"
    bench.write_text(FUSED_BENCH)
    bench.chmod(0o755)
    for batch in records:
        record = vm / f"testdata/calibration/fused/tpch.sf40/{batch}.records.tsv"
        record.parent.mkdir(parents=True, exist_ok=True)
        record.write_text(f"# run: capture=none\ndataset\ttpch\n{batch}\n")
    return tree, vm, env


def fused_dir(tree):
    return tree / "testdata/calibration/fused/tpch.sf40"


def test_fused_profile_runs_one_capture_per_batch():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = fused_tree(tmp)
        r = ok(nsys_profile(tmp, tree, vm, env, "--fused", "tpch.sf40", "--trace"))
        runs = (tmp / "bench.log").read_text().splitlines()
        assert runs == [f"--nocapture --test-threads=1 bench_fused_tpch_sf40_{b}_tp1_single --exact record=unset"
                        for b in BATCHES], runs
        for b in BATCHES:
            assert (fused_dir(tree) / f"{b}.calls.tsv").is_file(), (b, r.stdout)
            assert (fused_dir(tree) / f"{b}.loads.tsv").is_file(), (b, r.stdout)
        assert not (fused_dir(tree) / "failed.tsv").exists()
        assert not (tree / "testdata/calibration/calls.tsv").exists(), "the corpus's files are not touched"


def test_fused_profile_joins_each_batch_against_its_own_record():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = fused_tree(tmp, records=["first3", "first10", "all"])
        r = nsys_profile(tmp, tree, vm, env, "--fused", "tpch.sf40", "--metrics")
        assert r.returncode == 1, (r.returncode, r.stdout, r.stderr)
        runs = (tmp / "bench.log").read_text().splitlines()
        assert len(runs) == 4 and all(run.endswith("record=" + str(vm / "testdata/calibration/fused/tpch.sf40"
                                                                       "/records-metrics.tsv")) for run in runs), runs
        readers = log_lines(tmp / "hbm.log")
        joins = [a for a in readers if "--capture" in a]
        joined = ["first3", "first10", "all"]  # first20 has no record, so its join is refused
        assert [a[a.index("--case") + 1] for a in joins] == [f"tpch.sf40 {b} tp1-single" for b in joined], joins
        assert [a[a.index("--record") + 1] for a in joins] == [
            f"testdata/calibration/fused/tpch.sf40/{b}.records.tsv" for b in joined], joins
        (merge,) = [a for a in readers if "--parts" in a]
        records = [merge[i + 1] for i, w in enumerate(merge) if w == "--record"]
        assert records == [f"testdata/calibration/fused/tpch.sf40/{b}.records.tsv"
                           for b in ("first3", "first10", "all")], merge
        assert len([w for w in merge if w == "--parts"]) == 3, merge
        assert (fused_dir(tree) / "hbm.tsv").is_file(), r.stdout
        failed = (fused_dir(tree) / "failed.tsv").read_text().splitlines()
        assert len(failed) == 1 and failed[0].startswith("first20\tmetrics\t"), failed
        for b in BATCHES:
            assert (vm / f"testdata/calibration/fused/tpch.sf40/{b}.records.tsv").exists() == (b != "first20"), \
                "a clean record is never removed or replaced"


def test_fused_profile_refuses_a_filter():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = fused_tree(tmp)
        for extra_env, args, word in (({"PCK_TEST_FILTER": "first3"}, (), "PCK_TEST_FILTER"),
                                      ({}, ("--cases", tmp / "x.csv"), "--cases"),
                                      ({}, ("--device", "h100-80gb-hbm3"), "--device")):
            r = nsys_profile(tmp, tree, vm, {**env, **extra_env}, "--fused", "tpch.sf40", *args)
            assert r.returncode != 0, (word, r.stdout, r.stderr)
            assert "--fused" in r.stderr and word in r.stderr, (word, r.stderr)
        r = nsys_profile(tmp, tree, vm, env, "--fused", "tpch")
        assert r.returncode != 0 and "tpch" in r.stderr, r.stderr
        assert not (tmp / "rsh.log").exists(), (tmp / "rsh.log").read_text()


def test_fused_profile_lists_a_failed_capture_and_goes_on():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = fused_tree(tmp)
        r = nsys_profile(tmp, tree, vm, {**env, "FAKE_FAIL_BATCH": "first10"}, "--fused", "tpch.sf40", "--trace")
        assert r.returncode != 0, (r.stdout, r.stderr)
        failed = (fused_dir(tree) / "failed.tsv").read_text().splitlines()
        assert [line.split("\t")[:2] for line in failed] == [["first10", "trace"]], failed
        for b in ("first3", "first20", "all"):
            assert (fused_dir(tree) / f"{b}.calls.tsv").is_file(), b
        assert not (fused_dir(tree) / "first10.calls.tsv").exists()
        assert "failed.tsv" in r.stderr, r.stderr


if __name__ == "__main__":
    raise SystemExit(main(globals()))
