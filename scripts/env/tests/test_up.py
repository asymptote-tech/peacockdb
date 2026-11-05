"""up.sh and down.sh: an isolated ensemble instance on a CPU VM, refused before it can fail later.

Each test runs the scripts from a clone on a fake machine (`machine.py`): the docker CLI and gh
are fakes on PATH that record their argv, so nothing pulls, builds or runs a container.
"""
import grp
import json
import os
import pathlib
import pwd
import shutil
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import (GH_HELPER, IMAGE_PATH, KEYS, REGISTRY, Machine, command, flag_values, git,  # noqa: E402
                     mounts, path_without, FAKE_ROOT_ID)

USER = f"{os.getuid()}:{os.getgid()}"
USER_NAME = pwd.getpwuid(os.getuid()).pw_name
CARGO_REGISTRY = "/opt/rust/cargo/registry"


def refused(r, m, *names):
    """Exit 2, one stderr line naming each of `names`, and nothing started or pulled."""
    assert r.returncode == 2, (r.returncode, r.stdout, r.stderr)
    assert len(r.stderr.strip().splitlines()) == 1, r.stderr
    for name in names:
        assert name in r.stderr, (name, r.stderr)
    assert not [c for c in m.docker_calls() if c[0] in ("run", "pull")], m.docker_calls()


def up_ok(m, *args, **kw):
    r = m.up(*args, **kw)
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def the_instance_run(m, name, user=USER_NAME):
    runs = [c for c in m.instance_runs() if flag_values(c, "--name") == [f"peacock-{user}-{name}"]]
    assert len(runs) == 1, m.runs()
    return runs[0]


def mount_at(argv, dst):
    found = [mt for mt in mounts(argv) if mt[1] == dst]
    assert len(found) == 1, (dst, mounts(argv))
    return found[0]


def test_up_refuses_missing_login():
    with Machine() as m:
        (m.home / ".claude" / ".credentials.json").unlink()
        refused(m.up("A", "--env", m.keys_file), m, "Claude Code")
    with Machine() as m:
        refused(m.up("A", "--env", m.keys_file, env={"FAKE_GH_STATUS": "1"}), m, "gh auth login")
    with Machine() as m:
        (m.home / ".gitconfig").write_text(f'[credential "https://github.com"]\n\thelper = {GH_HELPER}\n')
        refused(m.up("A", "--env", m.keys_file), m, "user.email")
    with Machine() as m:
        (m.home / ".gitconfig").write_text("[user]\n\tname = U\n\temail = u@example.test\n")
        refused(m.up("A", "--env", m.keys_file), m, "gh auth setup-git")
    with Machine() as m:
        (m.home / ".config" / "gh" / "hosts.yml").unlink()
        refused(m.up("A", "--env", m.keys_file), m, "hosts.yml", "gh auth login --with-token --insecure-storage")
    with Machine() as m:
        r = m.up("A", "--env", m.keys_file, env={"FAKE_GH_STATUS": "1", "GH_TOKEN": "a-token-the-container-lacks"})
        refused(r, m, "gh auth login")
    with Machine() as m:
        (m.home / ".config" / "gh" / "hosts.yml").write_text("github.com:\n    user: u\n    git_protocol: https\n")
        refused(m.up("A", "--env", m.keys_file), m, "oauth_token", "keyring",
                "gh auth login --with-token --insecure-storage")
    with Machine() as m:
        hosts = m.home / ".config" / "gh" / "hosts.yml"
        hosts.write_text(hosts.read_text() + "    users:\n        u:\n            oauth_token: gho_broadOAuthValue42\n")
        r = m.up("A", "--env", m.keys_file)
        refused(r, m, "fine-grained")
        assert "gho_broadOAuthValue42" not in r.stderr + r.stdout, "a token value is printed"
    with Machine() as m:
        (m.home / ".local" / "bin" / "claude").unlink()
        refused(m.up("A", "--env", m.keys_file), m, "bootstrap-vm.sh")


def test_up_refuses_missing_key():
    for name in KEYS:
        others = {k: v for k, v in KEYS.items() if k != name}
        for keys, extra in ((others, ""), (others, f"{name}=\n")):
            with Machine() as m:
                m.write_keys(keys, extra)
                r = m.up("A", "--env", m.keys_file)
                refused(r, m, name)
                for value in KEYS.values():
                    assert value not in r.stderr + r.stdout, f"a value is printed: {r.stderr}"
    with Machine() as m:
        refused(m.up("A", "--env", m.home / "no-such.env"), m, "no-such.env")


def test_up_fetches_before_it_starts():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        runs = m.runs()
        fetches = [i for i, c in enumerate(runs) if command(c)[1][:2] == ["cargo", "fetch"]]
        assert len(fetches) == 1, runs
        fetch = runs[fetches[0]]
        registry = mount_at(fetch, CARGO_REGISTRY)
        assert registry[2] is False, f"the fetch cannot write the registry: {registry}"
        instance = the_instance_run(m, "A")
        assert runs.index(instance) > fetches[0], "the instance started before the fetch"
        assert mount_at(instance, CARGO_REGISTRY) == (registry[0], CARGO_REGISTRY, True), mounts(instance)
        worktree = flag_values(instance, "-w")[0]
        assert flag_values(fetch, "-w") == [worktree], fetch
        assert mount_at(fetch, worktree)[2] is True, "the fetch may write the worktree"


def test_up_commits_from_the_container():
    with Machine() as m:
        cmd = 'echo from-the-container > made-inside && git add made-inside && git commit -qm "made inside"'
        up_ok(m, "A", "--env", m.keys_file, env={"FAKE_CONTAINER_CMD": cmd})
        worktree = pathlib.Path(flag_values(the_instance_run(m, "A"), "-w")[0])
        sha = git(worktree, "rev-parse", "HEAD")
        git(m.clone, "cat-file", "-e", sha)
        assert git(m.clone, "log", "-1", "--format=%an <%ae>", sha) == "Machine User <machine@example.test>"
        assert worktree != m.clone and str(worktree).startswith(str(m.home)), worktree


def chain_container(user, chain, state="running"):
    return {f"peacock-{user}-{chain}": {"state": state,
                                         "labels": {"peacockdb.chain": chain, "peacockdb.user": user}}}


def test_up_refuses_duplicate_instance():
    with Machine() as m:
        for state in ("running", "exited"):
            m.world(containers=chain_container(USER_NAME, "A", state))
            r = m.up("A", "--env", m.keys_file)
            refused(r, m, f"peacock-{USER_NAME}-A", state, "down.sh A")


def test_up_refuses_a_chain_another_user_runs():
    with Machine() as m:
        m.world(containers=chain_container("bob", "A"))
        refused(m.up("A", "--env", m.keys_file), m, "bob", "peacock-bob-A")
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        other = m.other_user("carol")
        r = other.up("A", "--env", m.keys_file)
        assert r.returncode == 2 and USER_NAME in r.stderr and f"peacock-{USER_NAME}-A" in r.stderr, r.stderr
        assert len(m.instance_runs()) == 1, m.instance_runs()
        up_ok(other, "B", "--env", m.keys_file)


def test_up_names_the_container_after_user_and_chain():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        run = the_instance_run(m, "A")
        assert set(flag_values(run, "--label")) == {"peacockdb.chain=A", f"peacockdb.user={USER_NAME}"}, run
        assert flag_values(run, "-w") == [str(m.home / "peacock-instances" / "A")], run
        assert mount_at(run, str(m.home))[0] == str(m.home / "peacock-instances" / ".home" / "A")


def test_up_names_the_instance_and_its_image_for_the_device_run():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        run = the_instance_run(m, "A")
        assert f"PEACOCK_INSTANCE={USER_NAME}-A" in flag_values(run, "-e"), flag_values(run, "-e")
        assert "PEACOCK_IMAGE_TAG=cudf-25.02" in flag_values(run, "-e"), flag_values(run, "-e")
        tag = "cudf-25.02-0123456789ab"
        up_ok(m, "B", "--env", m.keys_file, "--image", tag)
        assert f"PEACOCK_IMAGE_TAG={tag}" in flag_values(the_instance_run(m, "B"), "-e")


def test_up_mounts_the_host_files_even_before_there_are_any():
    with Machine() as m:
        hosts = m.home / ".config" / "peacock" / "hosts"
        hosts.rmdir()
        up_ok(m, "A", "--env", m.keys_file)
        assert hosts.is_dir()
        assert (str(hosts), str(hosts), True) in mounts(the_instance_run(m, "A"))


def test_up_mounts_per_instance():
    with Machine() as m:
        ssh_key = m.home / ".ssh" / "gpu_key"
        ssh_key.parent.mkdir()
        ssh_key.write_text("not a real key\n")
        m.write_keys(KEYS, f"GPU_SSH_KEY_FILE={ssh_key}\n")
        up_ok(m, "A", "--env", m.keys_file)
        up_ok(m, "B", "--env", m.keys_file)
        a, b = the_instance_run(m, "A"), the_instance_run(m, "B")
        wa, wb = flag_values(a, "-w")[0], flag_values(b, "-w")[0]
        assert wa != wb and not wa.startswith(wb + "/") and not wb.startswith(wa + "/"), (wa, wb)
        assert mount_at(a, wa) == (wa, wa, False) and mount_at(b, wb) == (wb, wb, False)
        ha, hb = mount_at(a, str(m.home)), mount_at(b, str(m.home))
        assert ha[0] != hb[0] and ha[0] != str(m.home) and not ha[2] and not hb[2], (ha, hb)
        own = {wa, wb, str(m.home), "/etc/passwd", "/etc/group"}
        shared_a = sorted(mt for mt in mounts(a) if mt[1] not in own and not mt[1].startswith(wa + "/"))
        shared_b = sorted(mt for mt in mounts(b) if mt[1] not in own and not mt[1].startswith(wb + "/"))
        assert shared_a == shared_b, (shared_a, shared_b)
        home = str(m.home)
        claude_dir = f"{home}/.claude"
        want = {
            f"{m.clone}/.git": False, claude_dir: False, f"{home}/.config/gh": False,
            f"{home}/.gitconfig": True, f"{home}/.local/share/claude": True,
            f"{home}/.local/bin/claude": True, f"{home}/.config/peacock/hosts": True,
            str(ssh_key): True, CARGO_REGISTRY: True,
        }
        for dst, readonly in want.items():
            assert mount_at(a, dst)[2] is readonly, (dst, mount_at(a, dst))
        for dataset in ("tpch.sf1", "tpcds.sf1"):
            for run, wt in ((a, wa), (b, wb)):
                assert mount_at(run, f"{wt}/testdata/{dataset}")[2] is True, dataset
        env = flag_values(a, "-e")
        for want_env in (f"HOME={home}", "DISABLE_AUTOUPDATER=1", f"CLAUDE_CONFIG_DIR={claude_dir}",
                         f"PATH={home}/.local/bin:{IMAGE_PATH}"):
            assert want_env in env, (want_env, env)
        assert f"{home}/.local/bin/claude" in {mt[1] for mt in mounts(a)}
        points = [pathlib.Path(ha[0])] + [pathlib.Path(wa, "testdata", d) for d in ("tpch.sf1", "tpcds.sf1")]
        points += [pathlib.Path(ha[0]) / pathlib.Path(mt[1]).relative_to(home) for mt in mounts(a)
                   if mt[1].startswith(home + "/") and not mt[1].startswith(wa + "/")]
        for point in points:
            assert point.exists() and point.stat().st_uid == os.getuid(), f"docker would make {point} as root"


def test_up_mounts_a_provider_s_config_only_from_hosts():
    with Machine() as m:
        provider = m.home / ".provider-cli"
        provider.mkdir()
        m.write_keys(KEYS, f"PROVIDER_CONFIG_DIR={provider}\n")
        up_ok(m, "A", "--env", m.keys_file)
        mounted = {p for mt in mounts(the_instance_run(m, "A")) for p in mt[:2]}
        assert str(provider) not in mounted, mounted


def test_up_names_the_chain_s_gpu_host_only_when_recorded():
    with Machine() as m:
        hosts = m.home / ".config" / "peacock" / "hosts"
        (hosts / "h100.env").write_text("PROVIDER=ssh\nHOST=gpu.test\nUSER=ubuntu\n")
        up_ok(m, "A", "--env", m.keys_file, "--gpu-host", "h100")
        assert "PEACOCK_GPU_HOST=h100" in flag_values(the_instance_run(m, "A"), "-e")
        r = up_ok(m, "B", "--env", m.keys_file)
        assert not [e for e in flag_values(the_instance_run(m, "B"), "-e") if e.startswith("PEACOCK_GPU_HOST")]
        assert "up.sh: " not in r.stderr, r.stderr
    for name, words in (("a100", ("a100", "provision-vm.sh --gpu-host")), ("../h", ("../h",))):
        with Machine() as m:
            refused(m.up("A", "--env", m.keys_file, "--gpu-host", name), m, *words)
            assert not (m.home / "peacock-instances" / "A").exists(), "a worktree made before the refusal"


def test_up_needs_the_chain_s_gpu_host_named_when_several_are_recorded():
    with Machine() as m:
        hosts = m.home / ".config" / "peacock" / "hosts"
        for name in ("h100", "a100"):
            (hosts / f"{name}.env").write_text("PROVIDER=ssh\nHOST=gpu.test\nUSER=ubuntu\n")
        refused(m.up("A", "--env", m.keys_file), m, "--gpu-host", "a100", "h100")
        assert not (m.home / "peacock-instances" / "A").exists(), "a worktree made before the refusal"
        up_ok(m, "A", "--env", m.keys_file, "--gpu-host", "a100")


def test_up_with_no_gpu_host_recorded_says_its_gpu_steps_will_stall():
    with Machine() as m:
        r = up_ok(m, "A", "--env", m.keys_file)
        warnings = [ln for ln in r.stderr.splitlines() if ln.startswith("up.sh: ")]
        assert len(warnings) == 1 and "provision-vm.sh --gpu-host" in warnings[0], r.stderr
        assert m.instance_runs(), "the chain did not start"


def test_up_runs_as_the_machine_user():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        runs = m.runs()
        assert runs, "no docker run"
        for run in runs:
            assert flag_values(run, "--user") == [USER], run
    with Machine() as m:
        m.install("id", FAKE_ROOT_ID)
        refused(m.up("A", "--env", m.keys_file), m, "root")


def test_up_never_passes_keys_as_args():
    with Machine() as m:
        r = up_ok(m, "A", "--env", m.keys_file)
        for call in m.docker_calls():
            for value in KEYS.values():
                assert not any(value in arg for arg in call), f"a key value in docker argv: {call}"
        for value in KEYS.values():
            assert value not in r.stdout + r.stderr
        instance = the_instance_run(m, "A")
        assert flag_values(instance, "--env-file") == [str(m.keys_file)], instance
        for run in m.runs():
            if "-d" not in run:
                assert "--env-file" not in run, f"keys passed to a step that needs none: {run}"


def generation_runs(m):
    return [c for c in m.runs() if any(arg.endswith("/generate_testdata.sh") for arg in command(c)[1])]


def test_up_mounts_data_by_generator_inputs():
    with Machine() as m:
        present = m.data_dir / m.data_hash()
        for dataset in ("tpch.sf1", "tpcds.sf1"):
            (present / dataset).mkdir(parents=True)
            (present / dataset / "kept").write_text(dataset)
        up_ok(m, "A", "--env", m.keys_file)
        assert generation_runs(m) == [], generation_runs(m)
        a = the_instance_run(m, "A")
        wa = flag_values(a, "-w")[0]
        assert mount_at(a, f"{wa}/testdata/tpch.sf1") == (str(present / "tpch.sf1"), f"{wa}/testdata/tpch.sf1", True)

        gen = m.clone / "testdata" / "generate_testdata.sh"
        gen.write_text(gen.read_text() + "# a changed generator\n")
        m.commit("a changed generator")
        new = m.data_dir / m.data_hash()
        assert new != present
        up_ok(m, "B", "--env", m.keys_file)
        gens = generation_runs(m)
        assert sorted(command(c)[1][-1] for c in gens) == ["tpcds", "tpch"], gens
        for c in gens:
            assert flag_values(c, "--network") == ["none"], c
            assert "PEACOCK_NO_FETCH=1" in flag_values(c, "-e"), c
        b = the_instance_run(m, "B")
        wb = flag_values(b, "-w")[0]
        assert mount_at(b, f"{wb}/testdata/tpcds.sf1") == (str(new / "tpcds.sf1"), f"{wb}/testdata/tpcds.sf1", True)
        assert sorted(p.name for p in new.iterdir()) == ["tpcds.sf1", "tpch.sf1"], list(new.iterdir())
        assert (present / "tpch.sf1" / "kept").read_text() == "tpch.sf1", "the old set was touched"


def test_two_ups_at_once_generate_once_and_fetch_in_turn():
    with Machine() as m:
        m.world(gen_seconds=1, fetch_seconds=1)
        procs = [m.up(name, "--env", m.keys_file, background=True) for name in ("A", "B")]
        for p in procs:
            out, err = p.communicate(timeout=120)
            assert p.returncode == 0, (p.returncode, out, err)
        gens = generation_runs(m)
        assert sorted(command(c)[1][-1] for c in gens) == ["tpcds", "tpch"], gens
        sets = {mount_at(the_instance_run(m, n), flag_values(the_instance_run(m, n), "-w")[0]
                         + "/testdata/tpch.sf1")[0] for n in ("A", "B")}
        assert sets == {str(m.data_dir / m.data_hash() / "tpch.sf1")}, sets
        assert not m.world()["fetch_overlap"], "two cargo fetches wrote the shared registry at once"


def test_up_refuses_stale_image():
    with Machine() as m:
        label = m.label("25.02").split(" ")
        stale = " ".join(e if not e.startswith("docker/venv.lock=") else "docker/venv.lock=" + "0" * 64 for e in label)
        m.world(label=stale)
        r = m.up("A", "--env", m.keys_file)
        assert r.returncode == 2 and "docker/venv.lock" in r.stderr, (r.returncode, r.stderr)
        assert "docker/pins.env" not in r.stderr, r.stderr
        assert not m.instance_runs(), m.runs()
    with Machine() as m:
        m.world(label=None)
        r = m.up("A", "--env", m.keys_file)
        assert r.returncode == 2 and "peacockdb.inputs" in r.stderr, (r.returncode, r.stderr)
        assert not m.instance_runs(), m.runs()


def test_up_pulls_the_tag_it_is_given():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        assert ["pull", f"{REGISTRY}:cudf-25.02"] in m.docker_calls()
        assert command(the_instance_run(m, "A"))[0] == f"{REGISTRY}:cudf-25.02"
    with Machine() as m:
        m.world(label=m.label("26.02"))
        up_ok(m, "A", "--env", m.keys_file, "--image", "cudf-26.02-0123456789ab")
        assert ["pull", f"{REGISTRY}:cudf-26.02-0123456789ab"] in m.docker_calls()


def test_up_starts_the_watchdog_for_its_chain():
    with Machine() as m:
        up_ok(m, "B", "--env", m.keys_file)
        run = the_instance_run(m, "B")
        assert command(run)[1] == ["scripts/ensemble-watchdog.sh", "B"], run
        assert pathlib.Path(flag_values(run, "-w")[0], "scripts", "ensemble-watchdog.sh").is_file()


def test_up_refuses_bad_arguments_before_any_call():
    cases = [(), ("-a",), ("A", "B"), ("A", "--cudf", "25.10"), ("A", "--cudf"), ("x y",), ("A/B",),
             ("A", "--image", "latest"), ("A", "--bogus"), ("A", "--chain", "C"), ("A", "--vm", "vm1"),
             ("A", "--cudf", "25.02", "--image", "cudf-25.02-0123456789ab")]
    for args in cases:
        with Machine() as m:
            r = m.up(*args, "--env", m.keys_file)
            assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (args, r.returncode, r.stderr)
            assert r.stderr.startswith("up.sh: "), (args, r.stderr)
            assert m.docker_calls() == [], (args, m.docker_calls())


def test_up_takes_the_default_env_file():
    with Machine() as m:
        default = m.home / ".config" / "peacock" / "peacock.env"
        default.write_text(m.keys_file.read_text())
        up_ok(m, "A")
        assert flag_values(the_instance_run(m, "A"), "--env-file") == [str(default)]
    with Machine() as m:
        refused(m.up("A"), m, str(m.home / ".config" / "peacock" / "peacock.env"))
    with Machine() as m:
        refused(m.up("A", "--env"), m, "--env")


def test_up_refuses_a_relative_path_in_the_env_file():
    with Machine() as m:
        (m.home / "rel").mkdir()
        (m.home / "rel" / "key").write_text("k\n")
        m.write_keys(KEYS, "GPU_SSH_KEY_FILE=rel/key\n")
        r = m.up("A", "--env", m.keys_file, cwd=m.home)
        refused(r, m, "GPU_SSH_KEY_FILE", "absolute")
        assert not (m.home / "peacock-instances" / "A").exists(), "a worktree made before the refusal"


def test_up_never_runs_the_machine_s_git_hooks():
    with Machine() as m:
        hooks = m.home.parent / "global-hooks"
        hooks.mkdir()
        ran = m.home.parent / "hook-ran"
        for hook in ("post-checkout", "reference-transaction", "fsmonitor"):
            (hooks / hook).write_text(f"#!/bin/sh\necho {hook} >> {ran}\n")
            (hooks / hook).chmod(0o755)
        with open(m.home / ".gitconfig", "a") as f:
            f.write(f"[core]\n\thooksPath = {hooks}\n\tfsmonitor = {hooks / 'fsmonitor'}\n")
        up_ok(m, "A", "--env", m.keys_file)
        assert not ran.exists(), f"a git hook ran on the machine: {ran.read_text()}"


def test_up_refuses_a_clone_whose_git_runs_code():
    cases = [("hook", "post-checkout"), ("config", "core.hooksPath"), ("config", "core.fsmonitor")]
    for kind, name in cases:
        with Machine() as m:
            if kind == "hook":
                hook = m.clone / ".git" / "hooks" / name
                hook.write_text("#!/bin/sh\nexit 0\n")
                hook.chmod(0o755)
                (m.clone / ".git" / "hooks" / "pre-push.sample").write_text("# a sample runs nothing\n")
            else:
                git(m.clone, "config", name, "/somewhere")
            r = m.up("A", "--env", m.keys_file)
            refused(r, m, name)
            assert not (m.home / "peacock-instances" / "A").exists(), "a worktree made before the refusal"


def test_up_gives_the_container_the_machine_user():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        run = the_instance_run(m, "A")
        uid, gid = os.getuid(), os.getgid()
        user, group = pwd.getpwuid(uid).pw_name, grp.getgrgid(gid).gr_name
        passwd_src, _, passwd_ro = mount_at(run, "/etc/passwd")
        group_src, _, group_ro = mount_at(run, "/etc/group")
        assert passwd_ro and group_ro, "the container may write its passwd or group"
        home = mount_at(run, str(m.home))[0]
        for src in (passwd_src, group_src):
            assert not src.startswith(home + "/"), f"the instance can rewrite {src} through its home"
        lines = pathlib.Path(passwd_src).read_text().splitlines()
        ours = [ln for ln in lines if ln.split(":")[2] == str(uid)]
        assert ours == [f"{user}:x:{uid}:{gid}::{m.home}:/bin/bash"], lines
        assert "root:x:0:0:root:/root:/bin/bash" in lines, lines
        groups = pathlib.Path(group_src).read_text().splitlines()
        assert [g for g in groups if g.split(":")[2] == str(gid)] == [f"{group}:x:{gid}:"], groups
        assert "root:x:0:" in groups, groups


def cudf_mount(m, name):
    run = the_instance_run_last(m, name)
    return mount_at(run, flag_values(run, "-w")[0] + "/third_party/cudf")


def test_up_mounts_the_cudf_checkout_of_its_worktree_s_gitlink():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        first = pathlib.Path(cudf_mount(m, "A")[0])
        assert cudf_mount(m, "A")[2] is True, cudf_mount(m, "A")
        assert first == m.data_dir / "cudf" / m.cudf_shas[0], first
        assert git(first, "rev-parse", "HEAD") == m.cudf_shas[0]
        assert (first / "VERSION").read_text() == "25.02.00\n" and not (first / "README.md").exists(), \
            "not the build image's sparse paths"
        wa = pathlib.Path(flag_values(the_instance_run_last(m, "A"), "-w")[0])
        git(wa, "update-index", "--cacheinfo", f"160000,{m.cudf_shas[1]},third_party/cudf")
        git(wa, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "cudf moved in a alone")
        assert git(m.clone, "ls-files", "-s", "third_party/cudf").split()[1] == m.cudf_shas[0]
        assert m.down("A").returncode == 0
        m.world(label=m.label("25.02", tree=wa))
        up_ok(m, "A", "--env", m.keys_file)
        second = pathlib.Path(cudf_mount(m, "A")[0])
        assert second == m.data_dir / "cudf" / m.cudf_shas[1], "the mount does not follow the worktree"
        assert (second / "VERSION").read_text() == "26.02.00\n"
        m.world(label=m.label("25.02"))
        up_ok(m, "B", "--env", m.keys_file)
        assert cudf_mount(m, "B")[0] == str(first)
        assert git(first, "rev-parse", "HEAD") == m.cudf_shas[0], "the first checkout moved"


def test_up_reuses_a_cudf_checkout_without_fetching():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        m.cudf_origin.rename(m.cudf_origin.with_name("gone"))
        up_ok(m, "B", "--env", m.keys_file)
        assert cudf_mount(m, "B")[0] == cudf_mount(m, "A")[0]


def test_up_refuses_a_cudf_commit_it_cannot_fetch():
    with Machine() as m:
        m.cudf_origin.rename(m.cudf_origin.with_name("gone"))
        r = m.up("A", "--env", m.keys_file)
        assert r.returncode == 2 and "third_party/cudf" in r.stderr and m.cudf_sha in r.stderr, r.stderr
        assert not m.instance_runs(), m.runs()
        cudf = m.data_dir / "cudf"
        assert not (cudf / m.cudf_sha).exists() and not (cudf / f"{m.cudf_sha}.partial").exists(), \
            f"a partial checkout kept: {list(cudf.iterdir())}"


def test_up_does_not_read_the_clone_s_own_submodule():
    with Machine() as m:
        cudf = m.clone / "third_party" / "cudf"
        (cudf / ".git").write_text("gitdir: ../../.git/modules/third_party/cudf\n")
        up_ok(m, "A", "--env", m.keys_file)
        assert cudf_mount(m, "A")[0] == str(m.data_dir / "cudf" / m.cudf_sha)


def test_up_has_help():
    with Machine() as m:
        r = m.up("--help")
        assert r.returncode == 0 and "up.sh <chain>" in r.stdout and "--env" in r.stdout, r
        assert m.docker_calls() == []


def test_down_stops_and_removes_the_instance_and_keeps_its_worktree():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        worktree = pathlib.Path(flag_values(the_instance_run(m, "A"), "-w")[0])
        r = m.down("A")
        assert r.returncode == 0, r.stderr
        calls = [c for c in m.docker_calls() if c[0] in ("stop", "rm")]
        assert calls == [["stop", f"peacock-{USER_NAME}-A"], ["rm", f"peacock-{USER_NAME}-A"]], calls
        assert worktree.is_dir()
        up_ok(m, "A", "--env", m.keys_file)
        assert flag_values(the_instance_run_last(m, "A"), "-w") == [str(worktree)]


def the_instance_run_last(m, name):
    return [c for c in m.instance_runs() if flag_values(c, "--name") == [f"peacock-{USER_NAME}-{name}"]][-1]


def test_down_refuses_an_unknown_instance():
    with Machine() as m:
        for args in [("A",), (), ("A", "B"), ("-x",), ("A", "--vm", "vm1")]:
            r = m.down(*args)
            assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (args, r.stderr)
            assert not [c for c in m.docker_calls() if c[0] in ("stop", "rm")], m.docker_calls()
        r = m.down("--help")
        assert r.returncode == 0 and "down.sh <chain>" in r.stdout, r



def test_up_reuses_a_data_set_another_user_made():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        assert len(generation_runs(m)) == 2, generation_runs(m)
        m.cudf_origin.rename(m.cudf_origin.with_name("gone"))
        other = m.other_user("carol")
        up_ok(other, "B", "--env", m.keys_file)
        assert len(generation_runs(m)) == 2, "the second user generated the set again"
        b = the_instance_run(m, "B", user="carol")
        wb = flag_values(b, "-w")[0]
        assert wb.startswith(str(other.home) + "/"), wb
        assert mount_at(b, f"{wb}/testdata/tpch.sf1")[0] == str(m.data_dir / m.data_hash() / "tpch.sf1")
        assert mount_at(b, f"{wb}/third_party/cudf")[0] == str(m.data_dir / "cudf" / m.cudf_sha)
        assert mount_at(b, CARGO_REGISTRY)[0].startswith(str(other.home) + "/"), "the registry is per user"
        # The checkout is the first user's: git refuses a repository another uid owns unless told.
        env = flag_values(b, "-e")
        assert {"GIT_CONFIG_COUNT=1", "GIT_CONFIG_KEY_0=safe.directory",
                f"GIT_CONFIG_VALUE_0={wb}/third_party/cudf"} <= set(env), env


def test_up_leaves_the_shared_directory_writable_by_the_group():
    with Machine() as m:
        up_ok(m, "A", "--env", m.keys_file)
        dirs, locks = [], []
        for root, ds, fs in os.walk(m.data_dir):
            dirs += [pathlib.Path(root, d) for d in ds]
            locks += [pathlib.Path(root, f) for f in fs if f.endswith(".lock")]
        assert locks and dirs, (locks, dirs)
        for path in dirs + locks:
            assert path.stat().st_mode & 0o060 == 0o060, f"{path} is {oct(path.stat().st_mode)}: not the group's"
        for c in generation_runs(m):
            assert command(c)[1][:2] == ["bash", "-c"] and command(c)[1][2].startswith("umask 002 &&"), c


def test_up_refuses_a_missing_data_directory():
    with Machine() as m:
        nowhere = m.home.parent / "nowhere"
        r = m.up("A", "--env", m.keys_file, env={"PEACOCK_DATA_DIR": str(nowhere)})
        refused(r, m, str(nowhere), "bootstrap-vm.sh")


def test_the_data_directory_is_machine_wide_by_default():
    with Machine() as m:
        out = subprocess.run(["bash", "-c", f'unset PEACOCK_DATA_DIR; . "{m.clone}/scripts/lib/machine.sh"; '
                              'echo "$MACHINE_DATA"'], check=True, capture_output=True, text=True).stdout
        assert out.strip() == "/var/lib/peacock", out


FAKE_SSH_RECORD = """#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_SSH_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\\n")
sys.exit(int(os.environ.get("FAKE_SSH_STATUS", "0")))
"""


def as_laptop(m, vms):
    """No marker, `vms` in the vm file, and an ssh that records its argv."""
    (m.home / ".config" / "peacock" / "machine").unlink()
    if vms is not None:
        (m.home / ".config" / "peacock" / "vm").write_text(vms)
    m.install("ssh", FAKE_SSH_RECORD)
    log = m.home.parent / "ssh.log"
    m.env["FAKE_SSH_LOG"] = str(log)
    return lambda: [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []


def test_up_forwards_from_a_laptop_to_its_default_vm():
    with Machine() as m:
        calls = as_laptop(m, "vm1 dev@10.0.0.1\nvm2 dev2@10.0.0.2\n")
        r = m.up("A", "--cudf", "26.02", "--env", "/keys dir/peacock.env")
        assert r.returncode == 0, r.stderr
        assert calls()[-1][-2:] == ["dev2@10.0.0.2",
                                    "PEACOCK_FORWARDED=1 $HOME/peacockdb/scripts/env/up.sh A --cudf 26.02 --env /keys\\ dir/peacock.env"]
        r = m.up("--vm", "vm1", "A")
        assert r.returncode == 0, r.stderr
        assert calls()[-1][-2:] == ["dev@10.0.0.1", "PEACOCK_FORWARDED=1 $HOME/peacockdb/scripts/env/up.sh A"], calls()
        r = m.down("A", "--vm", "vm1")
        assert r.returncode == 0, r.stderr
        assert calls()[-1][-2:] == ["dev@10.0.0.1", "PEACOCK_FORWARDED=1 $HOME/peacockdb/scripts/env/down.sh A"], calls()
        r = m.up("A", env={"FAKE_SSH_STATUS": "3"})
        assert r.returncode == 3, (r.returncode, r.stderr)
        n = len(calls())
        r = m.up("A", "--vm", "vm9")
        assert r.returncode == 2 and "vm9" in r.stderr and len(calls()) == n, r.stderr
        assert m.docker_calls() == [], m.docker_calls()


def test_up_without_a_vm_says_to_provision():
    with Machine() as m:
        calls = as_laptop(m, None)
        for r in (m.up("A"), m.down("A")):
            assert r.returncode == 2 and "provision-vm.sh" in r.stderr, r.stderr
            assert len(r.stderr.strip().splitlines()) == 1, r.stderr
        assert calls() == [] and m.docker_calls() == []


def test_two_users_starting_one_chain_at_once_start_it_once():
    with Machine() as m:
        m.world(ps_seconds=2)
        other = m.other_user("carol")
        procs = [m.up("A", "--env", m.keys_file, background=True),
                 other.up("A", "--env", m.keys_file, background=True)]
        results = [(p.returncode, err) for p in procs for _, err in [p.communicate(timeout=120)]]
        assert sorted(rc for rc, _ in results) == [0, 2], results
        assert [err for rc, err in results if rc == 2][0].strip() == "up.sh: another up.sh is starting chain A", results
        assert len(m.instance_runs()) == 1, m.instance_runs()


def test_down_finds_the_container_by_its_labels():
    with Machine() as m:
        m.world(containers=chain_container("a", "b-C"))
        r = m.other_user("a-b").down("C")
        assert r.returncode == 2 and "C" in r.stderr, r.stderr
        assert not [c for c in m.docker_calls() if c[0] in ("stop", "rm")], "another user's chain was stopped"
        assert m.other_user("a").down("b-C").returncode == 0
        assert [c for c in m.docker_calls() if c[0] in ("stop", "rm")] == [["stop", "peacock-a-b-C"],
                                                                         ["rm", "peacock-a-b-C"]]


def test_up_never_forwards_twice():
    with Machine() as m:
        calls = as_laptop(m, "vm1 dev@10.0.0.1\n")
        for r in (m.up("A", env={"PEACOCK_FORWARDED": "1"}), m.down("A", env={"PEACOCK_FORWARDED": "1"})):
            assert r.returncode == 2 and "forwarded" in r.stderr and len(r.stderr.strip().splitlines()) == 1, r
        assert calls() == []


def test_up_refuses_a_token_in_the_env_file():
    for name in ("GH_TOKEN", "GITHUB_TOKEN"):
        with Machine() as m:
            m.write_keys(KEYS, f"{name}=github_pat_INENVFILE0value\n")
            r = m.up("A", "--env", m.keys_file)
            refused(r, m, name)
            assert "github_pat_INENVFILE0value" not in r.stderr + r.stdout


def test_help_needs_no_sed():
    with Machine() as m:
        system = path_without(m.home.parent, {"sed"})
        env = {"PATH": f"{m.bin}:{system}"}
        for script, first in (("up.sh", "up.sh <chain>"), ("down.sh", "down.sh <chain>"),
                              ("provision-vm.sh", "provision-vm.sh <login>@<address>"), ("gpu.sh", "gpu.sh <host>"),
                              ("host/ssh.sh", "ssh.sh <name>"), ("host/nebius.sh", "nebius.sh <name>")):
            r = m.run(script, "--help", env=env)
            assert r.returncode == 0 and first in r.stdout, (script, r.returncode, r.stderr)
            assert "set -euo" not in r.stdout and not any(ln.startswith("#") for ln in r.stdout.splitlines()), r.stdout


def test_laptop_scripts_need_bash_4_4():
    with Machine() as m:
        texts = {s: (m.clone / "scripts" / "env" / s).read_text()
                 for s in ("up.sh", "down.sh", "provision-vm.sh", "gpu.sh", "host/ssh.sh", "host/nebius.sh")}
        checks = set()
        for script, text in texts.items():
            lines = text.splitlines()
            at = next(i for i, ln in enumerate(lines) if "BASH_VERSINFO" in ln)
            first_source = next(i for i, ln in enumerate(lines) if ln.startswith((". ", "source ")))
            assert at + 1 < first_source, f"{script}: the bash check follows a source"
            assert not any(ln.startswith(("shopt", "set ")) for ln in lines[:at]), script
            checks.add("\n".join(lines[at:at + 2]))
        assert len(checks) == 1, checks
        check = checks.pop().replace("BASH_VERSINFO", "V").replace("$BASH_VERSION", "$VS")
        for version, ok in (("3 2", False), ("4 3", False), ("4 4", True), ("5 0", True)):
            r = subprocess.run(["bash", "-c", f"V=({version}); VS='{version}'\n{check}\necho passed", "up.sh"],
                               capture_output=True, text=True)
            assert ("passed" in r.stdout) is ok, (version, r.returncode, r.stdout, r.stderr)
            if not ok:
                assert r.returncode == 2 and r.stderr.startswith("up.sh: ") and "brew install bash" in r.stderr, r
                assert len(r.stderr.strip().splitlines()) == 1, r.stderr


if __name__ == "__main__":
    raise SystemExit(main(globals()))
