"""`check-build-image.sh`: the CPU-VM build's image, pulled on the VM and checked against this tree.

The VM's copy of the tree comes from nebius-sync.sh without .git, and the build image's inputs
include the third_party/cudf commit the git index pins, so the check runs on dev and docker on
the VM, through scripts/nebius.sh (gpu_host.py has the fakes).
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import CPU_VM, Host, label  # noqa: E402
from machine import NO_HOST, NO_SPACE, UNPUBLISHED  # noqa: E402

SCRIPT = "scripts/env/check-build-image.sh"
# What image-ref.sh prints for a tree whose inputs the moving tag does not hold: a published image by
# digest.
GIVEN_BUILD = "ghcr.io/asymptote-tech/peacockdb-build@sha256:" + "cd" * 32


def checked(h, *args, env=None):
    return h.script(SCRIPT, *args, env=env)


def test_check_build_image_pulls_then_reads_the_label_on_the_vm():
    with Host() as h:
        r = checked(h, *CPU_VM)
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        assert r.stdout == f"{h.build_ref}\n", "the ref alone, for the build's docker run"
        calls = h.docker_calls()
        assert calls[0] == ["pull", h.build_ref], calls
        assert any(a[:2] == ["image", "inspect"] and a[-1] == h.build_ref for a in calls[1:]), calls
        assert not any(a[0] == "run" for a in calls), calls
        assert all(a[:4] == CPU_VM for a in h.nebius_calls()), h.nebius_calls()
        assert set(h.verbs()) == {"ssh"}, h.verbs()
        assert not h.dev_log.exists(), h.dev_log.read_text()


def test_check_build_image_pulls_and_checks_the_given_image():
    with Host() as h:
        images = h.world()["images"]
        images[GIVEN_BUILD] = images.pop(h.build_ref)
        h.world(images=images)
        r = checked(h, *CPU_VM, env={"PEACOCK_BUILD_IMAGE": GIVEN_BUILD})
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        assert r.stdout == f"{GIVEN_BUILD}\n", r.stdout
        calls = h.docker_calls()
        assert calls[0] == ["pull", GIVEN_BUILD], calls
        assert any(a[:2] == ["image", "inspect"] and a[-1] == GIVEN_BUILD for a in calls[1:]), calls
        assert not any(h.build_ref in a for a in calls), calls


def test_check_build_image_refuses_a_given_image_of_other_inputs():
    with Host() as h:
        images = h.world()["images"]
        images[GIVEN_BUILD] = images.pop(h.build_ref)
        images[GIVEN_BUILD]["labels"]["peacockdb.inputs"] = label(h.tree, "build", change="docker/build.Dockerfile")
        h.world(images=images)
        r = checked(h, *CPU_VM, env={"PEACOCK_BUILD_IMAGE": GIVEN_BUILD})
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert "docker/build.Dockerfile" in r.stderr and GIVEN_BUILD in r.stderr, r.stderr


def test_check_build_image_refuses_one_built_from_other_inputs():
    with Host() as h:
        images = h.world()["images"]
        images[h.build_ref]["labels"]["peacockdb.inputs"] = label(h.tree, "build", change="docker/venv.lock")
        h.world(images=images)
        r = checked(h, *CPU_VM)
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert "docker/venv.lock" in r.stderr and h.build_ref in r.stderr, r.stderr


def test_check_build_image_refuses_one_without_a_label():
    with Host() as h:
        images = h.world()["images"]
        images[h.build_ref]["labels"] = {}
        h.world(images=images)
        r = checked(h, *CPU_VM)
        assert r.returncode == 2 and "peacockdb.inputs" in r.stderr, (r.returncode, r.stderr)


def test_check_build_image_names_the_publish_of_an_unpublished_image():
    for message in UNPUBLISHED:
        with Host() as h:
            images = h.world()["images"]
            images.pop(h.build_ref)
            h.world(images=images, pull_error=message.format(ref=h.build_ref))
            r = checked(h, *CPU_VM)
            assert r.returncode == 2, (message, r.returncode, r.stderr)
            assert f"{h.build_ref} is not published" in r.stderr, r.stderr
            assert "gh workflow run images.yml --ref master -f kind=build -f cudf=25.02" in r.stderr, r.stderr
            assert r.stdout == "", r.stdout


def test_check_build_image_names_a_pull_that_failed_otherwise():
    with Host() as h:
        images = h.world()["images"]
        images.pop(h.build_ref)
        h.world(images=images, pull_error=NO_HOST)
        r = checked(h, *CPU_VM)
        assert r.returncode == 1, (r.returncode, r.stderr)
        assert f"cannot pull {h.build_ref}" in r.stderr and NO_HOST in r.stderr, r.stderr
        assert "gh workflow run images.yml --ref master -f kind=build -f cudf=25.02" in r.stderr, r.stderr


def test_check_build_image_names_a_full_disk_on_the_vm():
    with Host() as h:
        images = h.world()["images"]
        images.pop(h.build_ref)
        h.world(images=images, pull_error=NO_SPACE)
        r = checked(h, *CPU_VM)
        assert r.returncode == 2, (r.returncode, r.stderr)
        for words in ("disk is full", NO_SPACE, f"scripts/nebius.sh {' '.join(CPU_VM)} ssh 'docker system df; df -h; df -i'"):
            assert words in r.stderr, (words, r.stderr)
        assert "try again" not in r.stderr and r.stdout == "", (r.stdout, r.stderr)


def test_check_build_image_names_a_vm_lost_mid_pull():
    with Host() as h:
        r = checked(h, *CPU_VM, env={"FAKE_NEBIUS_RC": "255", "FAKE_NEBIUS_RC_ON": f"docker pull {h.build_ref}"})
        assert r.returncode == 255, (r.returncode, r.stderr)
        for words in ("the ssh to the VM dropped", f"scripts/nebius.sh {' '.join(CPU_VM)} status", "run this step again"):
            assert words in r.stderr, (words, r.stderr)
        assert "registry or network" not in r.stderr and r.stdout == "", (r.stdout, r.stderr)


def test_check_build_image_names_a_stopped_vm_at_the_label_read_and_before_the_pull():
    # The label's read drops its ssh (255); the pull finds the VM stopped, nebius.sh's refusal (1).
    stopped = {"FAKE_NEBIUS_RC": "1", "FAKE_NEBIUS_RC_TEXT": f"nebius: {CPU_VM[1]} is not RUNNING; refusing to ssh"}
    for code, env in ((255, {"FAKE_NEBIUS_RC": "255", "FAKE_NEBIUS_RC_ON": "docker image inspect"}),
                      (1, {**stopped, "FAKE_NEBIUS_RC_ON": "docker pull"})):
        with Host() as h:
            r = checked(h, *CPU_VM, env=env)
            assert r.returncode == code, (code, r.returncode, r.stderr)
            for words in ("may have been stopped", f"scripts/nebius.sh {' '.join(CPU_VM)} start", "run this step again"):
                assert words in r.stderr, (code, words, r.stderr)
            assert "registry or network" not in r.stderr and r.stdout == "", (code, r.stdout, r.stderr)


def test_check_build_image_without_an_instance_id_is_refused():
    with Host() as h:
        for args in ((), ("--profile", "eu-west1")):
            r = checked(h, *args)
            assert r.returncode == 2 and "--instance-id" in r.stderr, (r.returncode, r.stderr)
        assert h.nebius_calls() == []


def test_check_build_image_refuses_bad_input():
    with Host() as h:
        for args, word in (((*CPU_VM, "--color", "red"), "--color"), ((*CPU_VM, "--cudf", "24.12"), "24.12"),
                           ((*CPU_VM, "build"), "build")):
            r = checked(h, *args)
            assert r.returncode == 2 and word in r.stderr, (args, r.returncode, r.stderr)
        # nebius.sh refuses during the pull, whose stderr machine.sh sends to a temp file: the refusal
        # still reaches stderr, and the file is removed.
        scratch = pathlib.Path(h.tmp.name) / "scratch"
        scratch.mkdir()
        r = checked(h, *CPU_VM, env={"FAKE_NEBIUS_RC": "64", "TMPDIR": str(scratch)})
        assert r.returncode == 2, (r.returncode, r.stderr)
        assert "nebius: refused by the test" in r.stderr and "refused its options" in r.stderr, r.stderr
        assert list(scratch.iterdir()) == [], list(scratch.iterdir())
        assert h.docker_calls() == []


def test_check_build_image_passes_on_nebius_shs_own_codes():
    with Host() as h:
        for rc in (69, 75):
            r = checked(h, *CPU_VM, env={"FAKE_NEBIUS_RC": str(rc)})
            assert r.returncode == rc, (rc, r.returncode, r.stderr)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
