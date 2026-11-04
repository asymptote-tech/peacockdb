"""install-watch on any of the three VMs: one self-contained profile, made on dev from a copy of
the caller's config, and the install file that names the watcher's instance and kind.

Each test's HOME holds an empty ~/.nebius/config.yaml and a k.pem holding FAKE. The fake nebius
never reads either; a FAKE in any argv or output means the script read the key."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "nebius.sh")
H100 = "computeinstance-e00e6yena4nzjgaym0"
CPU = "computeinstance-e01t2mm4265y8x88ey"


def setup(**over):
    w = World(state="RUNNING", **over)
    home = w.dir / "home"
    (home / ".nebius").mkdir(parents=True)
    (home / ".nebius" / "config.yaml").write_text("")
    (home / "k.pem").write_text("FAKE")
    (w.dir / "tmp").mkdir()
    return w, home


def install(w, home, *args):
    # The real default source, ~/.nebius/config.yaml under the fake HOME; temp dirs under the world.
    env = {**w.env(), "HOME": str(home), "TMPDIR": str(w.dir / "tmp")}
    del env["GPU_HOST_NEBIUS_CONFIG"]
    p = subprocess.run([SCRIPT, *args], env=env, capture_output=True, text=True, timeout=60,
                       stdin=subprocess.DEVNULL)
    return p.returncode, p.stdout, p.stderr


def calls(w, kind):
    return [c for c in w.read()["calls"] if c[0] == kind]


def profile_calls(w):
    return [c for c in calls(w, "nebius") if "profile" in c or "config" in c]


def ssh_commands(w):
    return [" ".join(c) for c in calls(w, "ssh")]


def test_install_watch_writes_the_instance_and_kind():
    w, home = setup()
    rc, out, err = install(w, home, "--instance-id", H100, "install-watch")
    assert rc == 0, (rc, err)
    writes = [c for c in ssh_commands(w) if "/etc/peacock-gpu-idle.json" in c]
    assert len(writes) == 1, ssh_commands(w)
    assert H100 in writes[0] and "command -v nvidia-smi" in writes[0], writes[0]
    # The timer must never tick before the file exists: it would stop nebius-gpu.
    starts = [i for i, c in enumerate(ssh_commands(w)) if "enable --now" in c]
    at = ssh_commands(w).index(writes[0])
    assert starts and (starts[0] > at or (starts[0] == at and
                       writes[0].index("/etc/peacock-gpu-idle.json") < writes[0].index("enable --now"))), writes


def test_install_watch_keeps_one_profile():
    w, home = setup(profiles=["default", "editor-eu", "eu-west1"], active_profile="default")
    rc, out, err = install(w, home, "--profile", "editor-eu", "install-watch")
    assert rc == 0, (rc, err)
    pc = profile_calls(w)
    deleted = [c[c.index("delete") + 1] for c in pc if "delete" in c]
    assert sorted(deleted) == ["default", "eu-west1"], pc
    assert any(c[-2:] == ["activate", "editor-eu"] for c in pc), pc
    assert w.read()["profiles"] == ["editor-eu"] and w.read()["active_profile"] == "editor-eu"
    for c in pc:
        assert "-c" in c, c
        config = pathlib.Path(c[c.index("-c") + 1])
        assert config.name == "config.yaml" and home / ".nebius" not in config.parents, c


def test_install_watch_ships_one_profile_only():
    w, home = setup(profiles=["default", "eu-west1"])
    rc, out, err = install(w, home, "install-watch")
    assert rc == 0, (rc, err)
    shipped = [c for c in calls(w, "rsync") if c[-1].endswith("nebius-config.yaml")]
    assert len(shipped) == 1, calls(w, "rsync")
    sources = [a for a in shipped[0][1:-1] if not a.startswith("-") and a != shipped[0][shipped[0].index("-e") + 1]]
    assert len(sources) == 1, shipped
    source = pathlib.Path(sources[0])
    assert source.name == "config.yaml" and w.dir / "tmp" in source.parents, source
    for c in calls(w, "rsync"):
        for a in c[1:]:
            assert str(home / ".nebius") not in a and not a.endswith(".pem"), c
    assert not any((w.dir / "tmp").iterdir()), list((w.dir / "tmp").iterdir())


def test_install_watch_removes_its_temp_dir_on_failure():
    w, home = setup(root_profile="broken")
    rc, out, err = install(w, home, "install-watch")
    assert rc == 1, (rc, err)
    assert not any((w.dir / "tmp").iterdir()), list((w.dir / "tmp").iterdir())


def test_install_watch_never_reads_the_key_file():
    w, home = setup(profiles=["default", "eu-west1"])
    pem = str(home / "k.pem")
    rc, out, err = install(w, home, "--instance-id", CPU, "--profile", "eu-west1", "install-watch", "--key-file", pem)
    assert rc == 0, (rc, err)
    named = [c for c in w.read()["calls"] if any(pem in a for a in c)]
    assert len(named) == 1, named
    c = named[0]
    assert c[0] == "nebius" and "update" in c and c[c.index("update") + 1] == "eu-west1", c
    assert c[c.index("--private-key-file") + 1] == pem, c
    assert "--skip-auth" in c, c
    log = (w.dir / "state" / "nebius.log").read_text()
    for text in [" ".join(c) for c in w.read()["calls"]] + [out, err, log]:
        assert "FAKE" not in text, text


def test_install_watch_profile_needs_no_pem_on_the_vm():
    config = {"endpoint": "api.nebius.cloud", "service-account-id": "serviceaccount-w1",
              "public-key-id": "publickey-w1", "private-key-file-path": "/home/u/.nebius/eu-west1-sa.pem"}
    w, home = setup(profiles=["default", "eu-west1"], profile_config=config)
    rc, out, err = install(w, home, "--profile", "eu-west1", "install-watch", "--key-file", str(home / "k.pem"))
    assert rc == 0, (rc, err)
    pc = profile_calls(w)
    update = next(i for i, c in enumerate(pc) if "update" in c)
    for flag, value in (("--endpoint", "api.nebius.cloud"), ("--service-account-id", "serviceaccount-w1"),
                        ("--public-key-id", "publickey-w1")):
        assert pc[update][pc[update].index(flag) + 1] == value, pc[update]
    unset = [i for i, c in enumerate(pc) if c[-3:] == ["config", "unset", "private-key-file-path"]]
    assert unset and unset[0] > update and "eu-west1" in pc[unset[0]], pc
    assert w.read()["profile_config"]["private-key-file-path"] == ""
    commands = ssh_commands(w)
    assert not [c for c in commands if ".pem" in c], commands
    assert "HOME=/root" in commands[-1] and "compute instance get" in commands[-1], commands[-1]


def test_install_watch_refuses_a_missing_key_file():
    w, home = setup()
    rc, out, err = install(w, home, "--profile", "eu-west1", "install-watch", "--key-file", str(home / "absent.pem"))
    assert rc == 1, (rc, err)
    assert "--key-file" in err, err
    assert not calls(w, "ssh") and not calls(w, "rsync"), w.read()["calls"]


def test_install_watch_refuses_a_key_path_profile_without_a_key_file():
    # Shipped as it is, the profile names a PEM that exists on dev only.
    config = {"endpoint": "api.nebius.cloud", "service-account-id": "serviceaccount-w1",
              "public-key-id": "publickey-w1", "private-key-file-path": "/home/u/.nebius/eu-west1-sa.pem"}
    w, home = setup(profiles=["default", "eu-west1"], profile_config=config)
    rc, out, err = install(w, home, "--profile", "eu-west1", "install-watch")
    assert rc == 1, (rc, err)
    assert "--key-file" in err and "eu-west1" in err, err
    assert not calls(w, "ssh") and not calls(w, "rsync"), w.read()["calls"]


def test_install_watch_refuses_an_unexpected_profile_list_without_echoing_it():
    w, home = setup(profile_list="default [default]\n    private-key: |\n        SECRETLINE")
    rc, out, err = install(w, home, "install-watch")
    assert rc == 1, (rc, err)
    log = (w.dir / "state" / "nebius.log").read_text()
    for text in (out, err, log):
        assert "SECRETLINE" not in text and "private-key" not in text, text
    assert not calls(w, "ssh") and not calls(w, "rsync"), w.read()["calls"]
    assert not any((w.dir / "tmp").iterdir())


def test_install_watch_rejects_an_unknown_option():
    for args in (["--bogus"], ["--key-file"]):
        w, home = setup()
        rc, out, err = install(w, home, "install-watch", *args)
        assert rc == 64, (args, rc, err)
        assert not w.read()["calls"], (args, w.read()["calls"])


if __name__ == "__main__":
    raise SystemExit(main(globals()))
