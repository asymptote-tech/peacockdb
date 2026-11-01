"""provision-vm.sh: one command on the laptop readies a VM for one developer.

Each test runs the script from a clone on a fake laptop (`laptop.py`): ssh, gh and the VM's sudo,
user and group tools are fakes, and the VM is a directory, so nothing leaves the machine.
"""
import getpass
import pathlib
import stat
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from laptop import PAT, Laptop  # noqa: E402
from machine import KEYS  # noqa: E402

TARGET = "ubuntu@vm1.test"


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, lap, *names):
    """Exit 2, one stderr line naming each of `names`, no ssh made and nothing kept on the laptop."""
    assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1, r.stderr
    assert r.stderr.startswith("provision-vm.sh: "), r.stderr
    for name in names:
        assert name in r.stderr, (name, r.stderr)
    assert lap.ssh_calls() == [], lap.ssh_calls()
    assert not lap.config.exists() or not any(lap.config.iterdir()), list(lap.config.iterdir())


def mode(path):
    return stat.S_IMODE(path.stat().st_mode)


def readied(lap, *extra, stdin=PAT + "\n"):
    lap.add_vm("vm1.test")
    return ok(lap.provision(TARGET, "--env", lap.received, "--user", "dev1", "--name", "first", *extra, stdin=stdin))


def test_provision_readies_a_vm():
    with Laptop() as lap:
        r = readied(lap, "--ref", "ENS-containers")
        assert mode(lap.config) == 0o700, oct(mode(lap.config))
        for name in ("peacock.env", "github-pat", "gpu_ed25519", "gpu_ed25519.pub", "vm"):
            assert mode(lap.config / name) == 0o600, (name, oct(mode(lap.config / name)))
        assert (lap.config / "peacock.env").read_text() == lap.received.read_text()
        assert (lap.config / "github-pat").read_text().strip() == PAT
        for words in ("https://github.com/settings/personal-access-tokens/new", "asymptote-tech", "peacockdb",
                      "Contents", "Pull requests", "Workflows", "Actions", "Commit statuses", "Metadata"):
            assert words in r.stdout, (words, r.stdout)
        checks = [c for c in lap.gh_calls() if c["host"] == "laptop"]
        assert [c["argv"][:2] for c in checks] == [["api", "repos/asymptote-tech/peacockdb"]], checks
        assert checks[0]["token"] == PAT
        pub = (lap.config / "gpu_ed25519.pub").read_text().strip()
        assert pub in r.stdout and "GPU host" in r.stdout, r.stdout

        vm, home = lap.vms / "vm1.test", lap.vm_home("vm1.test", "dev1")
        assert "dev1" in lap.group_members("vm1.test", "docker"), (vm / "group").read_text()
        assert "dev1" in lap.group_members("vm1.test", "peacock"), (vm / "group").read_text()
        assert (home / ".ssh" / "authorized_keys").read_text() == \
            (lap.vm_home("vm1.test", "ubuntu") / ".ssh" / "authorized_keys").read_text()
        sudoers = vm / "root" / "etc" / "sudoers.d" / "peacock-dev1"
        assert sudoers.read_text() == "dev1 ALL=(ALL) NOPASSWD:ALL\n" and mode(sudoers) == 0o440
        assert (vm / "bootstrap.log").read_text() == "dev1 --ref ENS-containers\n"
        vm_cfg = home / ".config" / "peacock"
        want = lap.received.read_text() + f"GPU_SSH_KEY_FILE={vm_cfg / 'gpu_ed25519'}\n"
        assert (vm_cfg / "peacock.env").read_text() == want
        for name in ("gpu_ed25519", "gpu_ed25519.pub"):
            assert (vm_cfg / name).read_bytes() == (lap.config / name).read_bytes(), name
        for name in ("gpu_ed25519", "gpu_ed25519.pub", "peacock.env"):
            assert mode(vm_cfg / name) == 0o600, (name, oct(mode(vm_cfg / name)))
        assert f"oauth_token: {PAT}" in (home / ".config" / "gh" / "hosts.yml").read_text()
        gitconfig = (home / ".gitconfig").read_text()
        for words in ("Ada Laptop", "ada@example.test", "!/usr/local/bin/gh auth git-credential"):
            assert words in gitconfig, (words, gitconfig)
        assert (lap.config / "vm").read_text() == "first dev1@vm1.test\n"

        calls = lap.ssh_calls()
        assert TARGET in calls[0]["argv"], calls[0]
        assert all("dev1@vm1.test" in c["argv"] for c in calls[1:]), [c["argv"] for c in calls]
        last = calls[-1]["argv"]
        assert "-t" in last and "claude auth login" in last[-1], last
        assert (home / ".claude" / ".credentials.json").is_file()


def test_provision_never_puts_the_pat_in_argv_or_the_env_file():
    with Laptop() as lap:
        r = readied(lap)
        secrets = [PAT, *KEYS.values(), (lap.config / "gpu_ed25519").read_text().splitlines()[1]]
        logs = [c["argv"] for c in lap.ssh_calls()] + [c["argv"] for c in lap.gh_calls()]
        logs.append([(lap.vms / "vm1.test" / "sudo.log").read_text()])
        for argv in logs:
            for secret in secrets:
                assert not any(secret in arg for arg in argv), f"a secret in argv: {argv}"
        for secret in secrets:
            assert secret not in r.stdout + r.stderr, "a secret is printed"
        vm_env = (lap.vm_home("vm1.test", "dev1") / ".config" / "peacock" / "peacock.env").read_text()
        assert PAT not in vm_env and "github_pat_" not in vm_env, vm_env
        assert any(c["stdin"].strip() == PAT for c in lap.ssh_calls()), "the PAT never went on ssh's stdin"


def test_provision_refuses_an_extra_key_by_name():
    for extra, name, value in (("GH_TOKEN=github_pat_LEAKED0value\n", "GH_TOKEN", "github_pat_LEAKED0value"),
                               ("FOO=bar-value-9\n", "FOO", "bar-value-9"),
                               ("github_pat_BARE0value\n", "line 5", "github_pat_BARE0value")):
        with Laptop() as lap:
            lap.add_vm("vm1.test")
            lap.write_env(KEYS, extra)
            r = lap.provision(TARGET, "--env", lap.received, stdin=PAT + "\n")
            refused(r, lap, name)
            assert value not in r.stdout + r.stderr, r.stderr


def test_provision_refuses_an_incomplete_env_file():
    for name in KEYS:
        others = {k: v for k, v in KEYS.items() if k != name}
        for extra in ("", f"{name}=\n", f"{name}={KEYS[name]}\n{name}={KEYS[name]}\n"):
            with Laptop() as lap:
                lap.write_env(others, extra)
                r = lap.provision(TARGET, "--env", lap.received, stdin=PAT + "\n")
                refused(r, lap, name)
                for value in KEYS.values():
                    assert value not in r.stdout + r.stderr, r.stderr


def test_provision_needs_an_env_file_the_first_time():
    with Laptop() as lap:
        refused(lap.provision(TARGET, stdin=PAT + "\n"), lap, "--env", "peacock.env")
    with Laptop() as lap:
        refused(lap.provision(TARGET, "--env", lap.home / "no-such.env", stdin=PAT + "\n"), lap, "no-such.env")


def test_provision_refuses_a_missing_git_identity():
    for key, kept in (("user.name", "\temail = ada@example.test\n"), ("user.email", "\tname = Ada Laptop\n")):
        with Laptop() as lap:
            (lap.home / ".gitconfig").write_text("[user]\n" + kept)
            refused(lap.provision(TARGET, "--env", lap.received, stdin=PAT + "\n"), lap, key)


def test_provision_refuses_a_token_it_cannot_use():
    with Laptop() as lap:
        r = lap.provision(TARGET, "--env", lap.received, stdin="gho_broadOAuthValue42\n")
        refused(r, lap, "github_pat_")
        assert "gho_broadOAuthValue42" not in r.stdout + r.stderr
    with Laptop() as lap:
        r = lap.provision(TARGET, "--env", lap.received, stdin=PAT + "\n", env={"FAKE_GH_DENIED": PAT})
        refused(r, lap, "asymptote-tech/peacockdb")
        assert PAT not in r.stdout + r.stderr
    with Laptop() as lap:
        refused(lap.provision(TARGET, "--env", lap.received, stdin=""), lap, "token")


def test_provision_rerun_skips_what_is_done():
    with Laptop() as lap:
        lap.add_vm("vm1.test")
        ok(lap.provision(TARGET, "--env", lap.received, stdin=PAT + "\n"))
        user = getpass.getuser()
        vm = lap.vms / "vm1.test"
        key = (lap.config / "gpu_ed25519").read_bytes()
        r = ok(lap.provision(TARGET, stdin=""))
        assert "personal-access-tokens" not in r.stdout, "the token was asked for again"
        assert (lap.config / "gpu_ed25519").read_bytes() == key, "the GPU key was made again"
        assert (vm / "useradd.log").read_text().count(f" {user}\n") == 1, (vm / "useradd.log").read_text()
        assert (vm / "bootstrap.log").read_text() == f"{user} \n", (vm / "bootstrap.log").read_text()
        logins = [c for c in lap.gh_calls() if c["host"] != "laptop" and c["argv"][:2] == ["auth", "login"]]
        assert len(logins) == 1, logins
        claude = [ln for ln in (vm / "claude.log").read_text().splitlines() if "login" in ln]
        assert len(claude) == 1, claude
        assert sum("-t" in c["argv"] for c in lap.ssh_calls()) == 1, "Claude's login opened again"
        assert (lap.config / "vm").read_text() == f"vm1.test {user}@vm1.test\n"


def test_provision_new_pat_replaces_it_everywhere():
    new = "github_pat_22NEWTOKEN0value_Qx17"
    with Laptop() as lap:
        lap.add_vm("vm1.test")
        lap.add_vm("vm2.test")
        ok(lap.provision("ubuntu@vm1.test", "--env", lap.received, "--user", "dev1", "--name", "a",
                         stdin=PAT + "\n"))
        ok(lap.provision("ubuntu@vm2.test", "--user", "dev1", "--name", "b"))
        r = ok(lap.provision("--new-pat", stdin=new + "\n", env={"FAKE_SSH_DOWN": "vm1.test"}))
        assert (lap.config / "github-pat").read_text().strip() == new
        hosts = lambda vm: (lap.vm_home(vm, "dev1") / ".config" / "gh" / "hosts.yml").read_text()  # noqa: E731
        assert f"oauth_token: {new}" in hosts("vm2.test"), hosts("vm2.test")
        assert f"oauth_token: {PAT}" in hosts("vm1.test"), "an unreachable VM was changed"
        assert "dev1@vm1.test" in r.stdout + r.stderr, (r.stdout, r.stderr)
        assert new not in r.stdout + r.stderr


def test_provision_refuses_bad_arguments_before_any_call():
    cases = [(), ("ubuntu",), (TARGET, "--user", "Bad User"), (TARGET, "--name", "x/y"), (TARGET, "--bogus"),
             (TARGET, "--env"), ("a@h", "b@h"), ("--user", "dev1"), ("--new-pat", "--name", "x"),
             (TARGET, "--ref")]
    for args in cases:
        with Laptop() as lap:
            refused(lap.provision(*args, stdin=PAT + "\n"), lap)


def test_provision_has_help():
    with Laptop() as lap:
        r = lap.provision("--help")
        assert r.returncode == 0 and "provision-vm.sh <login>@<address>" in r.stdout and "--new-pat" in r.stdout, r
        assert lap.ssh_calls() == []



def vm_env(lap, user="dev1"):
    return (lap.vm_home("vm1.test", user) / ".config" / "peacock" / "peacock.env").read_text()


def test_provision_normalises_the_env_file():
    lines = "".join(f"{k}={v}\n" for k, v in KEYS.items())
    for received in (lines.rstrip("\n").encode(), lines.replace("\n", "\r\n").encode()):
        with Laptop() as lap:
            lap.received.write_bytes(received)
            readied(lap)
            assert (lap.config / "peacock.env").read_text() == lines, (lap.config / "peacock.env").read_bytes()
            key = lap.vm_home("vm1.test", "dev1") / ".config" / "peacock" / "gpu_ed25519"
            assert vm_env(lap) == lines + f"GPU_SSH_KEY_FILE={key}\n", vm_env(lap)
    with Laptop() as lap:
        lap.add_vm("vm1.test")
        lap.config.mkdir(parents=True, mode=0o700)
        (lap.config / "peacock.env").write_text(lines.rstrip("\n"))
        ok(lap.provision(TARGET, "--user", "dev1", stdin=PAT + "\n"))
        assert vm_env(lap).splitlines()[:5] == [*lines.splitlines(), vm_env(lap).splitlines()[4]], vm_env(lap)
        assert vm_env(lap).splitlines()[4].startswith("GPU_SSH_KEY_FILE="), vm_env(lap)
    with Laptop() as lap:
        lap.add_vm("vm1.test")
        lap.config.mkdir(parents=True, mode=0o700)
        own = lap.config / "peacock.env"
        own.write_text(lines)
        ok(lap.provision(TARGET, "--env", own, "--user", "dev1", stdin=PAT + "\n"))
        assert own.read_text() == lines and mode(own) == 0o600, own.read_bytes()
        assert vm_env(lap).splitlines()[:4] == lines.splitlines(), vm_env(lap)
        assert [p.name for p in lap.config.iterdir() if p.name.startswith("peacock.env")] == ["peacock.env"]


def test_provision_takes_comments_in_the_env_file():
    with Laptop() as lap:
        lap.write_env(KEYS, "  # the storage keys, from the admin\n\n")
        lap.received.write_text("# shared storage\n\n" + lap.received.read_text())
        readied(lap)


def test_provision_refuses_a_ref_the_vm_clone_is_not_on():
    with Laptop() as lap:
        readied(lap, "--ref", "ENS-first")
        r = lap.provision(TARGET, "--user", "dev1", "--ref", "ENS-second")
        assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, r.stderr
        assert "ENS-first" in r.stderr and "ENS-second" in r.stderr, r.stderr
        ok(lap.provision(TARGET, "--user", "dev1", "--ref", "ENS-first"))
        ok(lap.provision(TARGET, "--user", "dev1"))
        assert (lap.vms / "vm1.test" / "bootstrap.log").read_text() == "dev1 --ref ENS-first\n"
        reads = [c["argv"][-1] for c in lap.ssh_calls() if "branch --show-current" in c["argv"][-1]]
        assert reads, [c["argv"] for c in lap.ssh_calls()]
        for command in reads:
            assert "-c core.hooksPath=/dev/null -c core.fsmonitor=false" in command, command


if __name__ == "__main__":
    raise SystemExit(main(globals()))
