"""up.sh and down.sh: an isolated ensemble instance on a CPU VM, refused before it can fail later.

Each test runs the scripts from a clone on a fake machine (`machine.py`): the docker CLI and gh
are fakes on PATH that record their argv, so nothing pulls, builds or runs a container.
"""
import grp
import os
import pathlib
import pwd
import shutil
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import (GH_HELPER, IMAGE_PATH, KEYS, REGISTRY, Machine, command, flag_values, git,  # noqa: E402
                     mounts, FAKE_ROOT_ID)

USER = f"{os.getuid()}:{os.getgid()}"
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


def the_instance_run(m, name):
    runs = [c for c in m.instance_runs() if flag_values(c, "--name") == [f"peacock-{name}"]]
    assert len(runs) == 1, m.runs()
    return runs[0]


def mount_at(argv, dst):
    found = [mt for mt in mounts(argv) if mt[1] == dst]
    assert len(found) == 1, (dst, mounts(argv))
    return found[0]


def test_up_refuses_missing_login():
    with Machine() as m:
        (m.home / ".claude" / ".credentials.json").unlink()
        refused(m.up("a", "--env", m.keys_file), m, "Claude Code")
    with Machine() as m:
        refused(m.up("a", "--env", m.keys_file, env={"FAKE_GH_STATUS": "1"}), m, "gh auth login")
    with Machine() as m:
        (m.home / ".gitconfig").write_text(f'[credential "https://github.com"]\n\thelper = {GH_HELPER}\n')
        refused(m.up("a", "--env", m.keys_file), m, "user.email")
    with Machine() as m:
        (m.home / ".gitconfig").write_text("[user]\n\tname = U\n\temail = u@example.test\n")
        refused(m.up("a", "--env", m.keys_file), m, "gh auth setup-git")
    with Machine() as m:
        (m.home / ".config" / "gh" / "hosts.yml").unlink()
        refused(m.up("a", "--env", m.keys_file), m, "hosts.yml", "gh auth login --with-token --insecure-storage")
    with Machine() as m:
        r = m.up("a", "--env", m.keys_file, env={"FAKE_GH_STATUS": "1", "GH_TOKEN": "a-token-the-container-lacks"})
        refused(r, m, "gh auth login")
    with Machine() as m:
        (m.home / ".config" / "gh" / "hosts.yml").write_text("github.com:\n    user: u\n    git_protocol: https\n")
        refused(m.up("a", "--env", m.keys_file), m, "oauth_token", "keyring",
                "gh auth login --with-token --insecure-storage")
    with Machine() as m:
        hosts = m.home / ".config" / "gh" / "hosts.yml"
        hosts.write_text(hosts.read_text() + "    users:\n        u:\n            oauth_token: gho_broadOAuthValue42\n")
        r = m.up("a", "--env", m.keys_file)
        refused(r, m, "fine-grained")
        assert "gho_broadOAuthValue42" not in r.stderr + r.stdout, "a token value is printed"
    with Machine() as m:
        (m.home / ".local" / "bin" / "claude").unlink()
        refused(m.up("a", "--env", m.keys_file), m, "bootstrap-vm.sh")


def test_up_refuses_missing_key():
    for name in KEYS:
        others = {k: v for k, v in KEYS.items() if k != name}
        for keys, extra in ((others, ""), (others, f"{name}=\n")):
            with Machine() as m:
                m.write_keys(keys, extra)
                r = m.up("a", "--env", m.keys_file)
                refused(r, m, name)
                for value in KEYS.values():
                    assert value not in r.stderr + r.stdout, f"a value is printed: {r.stderr}"
    with Machine() as m:
        refused(m.up("a", "--env", m.home / "no-such.env"), m, "no-such.env")


def test_up_fetches_before_it_starts():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        runs = m.runs()
        fetches = [i for i, c in enumerate(runs) if command(c)[1][:2] == ["cargo", "fetch"]]
        assert len(fetches) == 1, runs
        fetch = runs[fetches[0]]
        registry = mount_at(fetch, CARGO_REGISTRY)
        assert registry[2] is False, f"the fetch cannot write the registry: {registry}"
        instance = the_instance_run(m, "a")
        assert runs.index(instance) > fetches[0], "the instance started before the fetch"
        assert mount_at(instance, CARGO_REGISTRY) == (registry[0], CARGO_REGISTRY, True), mounts(instance)
        worktree = flag_values(instance, "-w")[0]
        assert flag_values(fetch, "-w") == [worktree], fetch
        assert mount_at(fetch, worktree)[2] is True, "the fetch may write the worktree"


def test_up_commits_from_the_container():
    with Machine() as m:
        cmd = 'echo from-the-container > made-inside && git add made-inside && git commit -qm "made inside"'
        up_ok(m, "a", "--env", m.keys_file, env={"FAKE_CONTAINER_CMD": cmd})
        worktree = pathlib.Path(flag_values(the_instance_run(m, "a"), "-w")[0])
        sha = git(worktree, "rev-parse", "HEAD")
        git(m.clone, "cat-file", "-e", sha)
        assert git(m.clone, "log", "-1", "--format=%an <%ae>", sha) == "Machine User <machine@example.test>"
        assert worktree != m.clone and str(worktree).startswith(str(m.home)), worktree


def test_up_refuses_duplicate_instance():
    with Machine() as m:
        for state in ("running", "exited"):
            m.world(containers={"peacock-a": state})
            r = m.up("a", "--env", m.keys_file)
            refused(r, m, "peacock-a", state, "down.sh a")


def test_up_mounts_per_instance():
    with Machine() as m:
        ssh_key = m.home / ".ssh" / "gpu_key"
        ssh_key.parent.mkdir()
        ssh_key.write_text("not a real key\n")
        provider = m.home / ".provider-cli"
        provider.mkdir()
        m.write_keys(KEYS, f"GPU_SSH_KEY_FILE={ssh_key}\nPROVIDER_CONFIG_DIR={provider}\n")
        up_ok(m, "a", "--env", m.keys_file)
        up_ok(m, "b", "--env", m.keys_file)
        a, b = the_instance_run(m, "a"), the_instance_run(m, "b")
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
            str(ssh_key): True, str(provider): True, CARGO_REGISTRY: True,
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


def test_up_runs_as_the_machine_user():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        runs = m.runs()
        assert runs, "no docker run"
        for run in runs:
            assert flag_values(run, "--user") == [USER], run
    with Machine() as m:
        m.install("id", FAKE_ROOT_ID)
        refused(m.up("a", "--env", m.keys_file), m, "root")


def test_up_never_passes_keys_as_args():
    with Machine() as m:
        r = up_ok(m, "a", "--env", m.keys_file)
        for call in m.docker_calls():
            for value in KEYS.values():
                assert not any(value in arg for arg in call), f"a key value in docker argv: {call}"
        for value in KEYS.values():
            assert value not in r.stdout + r.stderr
        instance = the_instance_run(m, "a")
        assert flag_values(instance, "--env-file") == [str(m.keys_file)], instance
        for run in m.runs():
            if "-d" not in run:
                assert "--env-file" not in run, f"keys passed to a step that needs none: {run}"


def generation_runs(m):
    return [c for c in m.runs() if any(arg.endswith("/generate_testdata.sh") for arg in command(c)[1])]


def test_up_mounts_data_by_generator_inputs():
    with Machine() as m:
        present = m.home / "peacock-data" / m.data_hash()
        for dataset in ("tpch.sf1", "tpcds.sf1"):
            (present / dataset).mkdir(parents=True)
            (present / dataset / "kept").write_text(dataset)
        up_ok(m, "a", "--env", m.keys_file)
        assert generation_runs(m) == [], generation_runs(m)
        a = the_instance_run(m, "a")
        wa = flag_values(a, "-w")[0]
        assert mount_at(a, f"{wa}/testdata/tpch.sf1") == (str(present / "tpch.sf1"), f"{wa}/testdata/tpch.sf1", True)

        gen = m.clone / "testdata" / "generate_testdata.sh"
        gen.write_text(gen.read_text() + "# a changed generator\n")
        m.commit("a changed generator")
        new = m.home / "peacock-data" / m.data_hash()
        assert new != present
        up_ok(m, "b", "--env", m.keys_file)
        gens = generation_runs(m)
        assert sorted(command(c)[1][-1] for c in gens) == ["tpcds", "tpch"], gens
        for c in gens:
            assert flag_values(c, "--network") == ["none"], c
            assert "PEACOCK_NO_FETCH=1" in flag_values(c, "-e"), c
        b = the_instance_run(m, "b")
        wb = flag_values(b, "-w")[0]
        assert mount_at(b, f"{wb}/testdata/tpcds.sf1") == (str(new / "tpcds.sf1"), f"{wb}/testdata/tpcds.sf1", True)
        assert sorted(p.name for p in new.iterdir()) == ["tpcds.sf1", "tpch.sf1"], list(new.iterdir())
        assert (present / "tpch.sf1" / "kept").read_text() == "tpch.sf1", "the old set was touched"


def test_two_ups_at_once_generate_once_and_fetch_in_turn():
    with Machine() as m:
        m.world(gen_seconds=1, fetch_seconds=1)
        procs = [m.up(name, "--env", m.keys_file, background=True) for name in ("a", "b")]
        for p in procs:
            out, err = p.communicate(timeout=120)
            assert p.returncode == 0, (p.returncode, out, err)
        gens = generation_runs(m)
        assert sorted(command(c)[1][-1] for c in gens) == ["tpcds", "tpch"], gens
        sets = {mount_at(the_instance_run(m, n), flag_values(the_instance_run(m, n), "-w")[0]
                         + "/testdata/tpch.sf1")[0] for n in ("a", "b")}
        assert sets == {str(m.home / "peacock-data" / m.data_hash() / "tpch.sf1")}, sets
        assert not m.world()["fetch_overlap"], "two cargo fetches wrote the shared registry at once"


def test_up_refuses_stale_image():
    with Machine() as m:
        label = m.label("25.02").split(" ")
        stale = " ".join(e if not e.startswith("docker/venv.lock=") else "docker/venv.lock=" + "0" * 64 for e in label)
        m.world(label=stale)
        r = m.up("a", "--env", m.keys_file)
        assert r.returncode == 2 and "docker/venv.lock" in r.stderr, (r.returncode, r.stderr)
        assert "docker/pins.env" not in r.stderr, r.stderr
        assert not m.instance_runs(), m.runs()
    with Machine() as m:
        m.world(label=None)
        r = m.up("a", "--env", m.keys_file)
        assert r.returncode == 2 and "peacockdb.inputs" in r.stderr, (r.returncode, r.stderr)
        assert not m.instance_runs(), m.runs()


def test_up_pulls_the_tag_it_is_given():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        assert ["pull", f"{REGISTRY}:cudf-25.02"] in m.docker_calls()
        assert command(the_instance_run(m, "a"))[0] == f"{REGISTRY}:cudf-25.02"
    with Machine() as m:
        m.world(label=m.label("26.02"))
        up_ok(m, "a", "--env", m.keys_file, "--image", "cudf-26.02-0123456789ab")
        assert ["pull", f"{REGISTRY}:cudf-26.02-0123456789ab"] in m.docker_calls()


def test_up_with_a_chain_starts_the_watchdog_in_the_worktree():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file, "--chain", "C")
        run = the_instance_run(m, "a")
        assert command(run)[1] == ["scripts/ensemble-watchdog.sh", "C"], run
        assert pathlib.Path(flag_values(run, "-w")[0], "scripts", "ensemble-watchdog.sh").is_file()
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        assert command(the_instance_run(m, "a"))[1] == ["sleep", "infinity"]


def test_up_refuses_bad_arguments_before_any_call():
    cases = [(), ("A",), ("-a",), ("a", "b"), ("a", "--cudf", "25.10"), ("a", "--cudf"),
             ("a", "--chain", "x y"), ("a", "--image", "latest"), ("a", "--bogus"),
             ("a", "--cudf", "25.02", "--image", "cudf-25.02-0123456789ab")]
    for args in cases:
        with Machine() as m:
            r = m.up(*args, "--env", m.keys_file)
            assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (args, r.returncode, r.stderr)
            assert r.stderr.startswith("up.sh: "), (args, r.stderr)
            assert m.docker_calls() == [], (args, m.docker_calls())


def test_up_refuses_without_an_env_file():
    for args in [("a",), ("a", "--chain", "C"), ("a", "--env")]:
        with Machine() as m:
            refused(m.up(*args), m, "--env")


def test_up_refuses_a_relative_path_in_the_env_file():
    for name in ("GPU_SSH_KEY_FILE", "PROVIDER_CONFIG_DIR"):
        with Machine() as m:
            (m.home / "rel").mkdir()
            (m.home / "rel" / "key").write_text("k\n")
            m.write_keys(KEYS, f"{name}=rel/key\n" if name == "GPU_SSH_KEY_FILE" else f"{name}=rel\n")
            r = m.up("a", "--env", m.keys_file, cwd=m.home)
            refused(r, m, name, "absolute")
            assert not (m.home / "peacock-instances" / "a").exists(), "a worktree made before the refusal"


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
        up_ok(m, "a", "--env", m.keys_file)
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
            r = m.up("a", "--env", m.keys_file)
            refused(r, m, name)
            assert not (m.home / "peacock-instances" / "a").exists(), "a worktree made before the refusal"


def test_up_gives_the_container_the_machine_user():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        run = the_instance_run(m, "a")
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
        up_ok(m, "a", "--env", m.keys_file)
        first = pathlib.Path(cudf_mount(m, "a")[0])
        assert cudf_mount(m, "a")[2] is True, cudf_mount(m, "a")
        assert first == m.home / "peacock-data" / "cudf" / m.cudf_shas[0], first
        assert git(first, "rev-parse", "HEAD") == m.cudf_shas[0]
        assert (first / "VERSION").read_text() == "25.02.00\n" and not (first / "README.md").exists(), \
            "not the build image's sparse paths"
        wa = pathlib.Path(flag_values(the_instance_run_last(m, "a"), "-w")[0])
        git(wa, "update-index", "--cacheinfo", f"160000,{m.cudf_shas[1]},third_party/cudf")
        git(wa, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "cudf moved in a alone")
        assert git(m.clone, "ls-files", "-s", "third_party/cudf").split()[1] == m.cudf_shas[0]
        assert m.down("a").returncode == 0
        m.world(label=m.label("25.02", tree=wa))
        up_ok(m, "a", "--env", m.keys_file)
        second = pathlib.Path(cudf_mount(m, "a")[0])
        assert second == m.home / "peacock-data" / "cudf" / m.cudf_shas[1], "the mount does not follow the worktree"
        assert (second / "VERSION").read_text() == "26.02.00\n"
        m.world(label=m.label("25.02"))
        up_ok(m, "b", "--env", m.keys_file)
        assert cudf_mount(m, "b")[0] == str(first)
        assert git(first, "rev-parse", "HEAD") == m.cudf_shas[0], "the first checkout moved"


def test_up_reuses_a_cudf_checkout_without_fetching():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        m.cudf_origin.rename(m.cudf_origin.with_name("gone"))
        up_ok(m, "b", "--env", m.keys_file)
        assert cudf_mount(m, "b")[0] == cudf_mount(m, "a")[0]


def test_up_refuses_a_cudf_commit_it_cannot_fetch():
    with Machine() as m:
        m.cudf_origin.rename(m.cudf_origin.with_name("gone"))
        r = m.up("a", "--env", m.keys_file)
        assert r.returncode == 2 and "third_party/cudf" in r.stderr and m.cudf_sha in r.stderr, r.stderr
        assert not m.instance_runs(), m.runs()
        cudf = m.home / "peacock-data" / "cudf"
        assert not (cudf / m.cudf_sha).exists() and not (cudf / f"{m.cudf_sha}.partial").exists(), \
            f"a partial checkout kept: {list(cudf.iterdir())}"


def test_up_does_not_read_the_clone_s_own_submodule():
    with Machine() as m:
        cudf = m.clone / "third_party" / "cudf"
        (cudf / ".git").write_text("gitdir: ../../.git/modules/third_party/cudf\n")
        up_ok(m, "a", "--env", m.keys_file)
        assert cudf_mount(m, "a")[0] == str(m.home / "peacock-data" / "cudf" / m.cudf_sha)


def test_up_has_help():
    with Machine() as m:
        r = m.up("--help")
        assert r.returncode == 0 and "up.sh <instance>" in r.stdout and "--env" in r.stdout, r
        assert m.docker_calls() == []


def test_down_stops_and_removes_the_instance_and_keeps_its_worktree():
    with Machine() as m:
        up_ok(m, "a", "--env", m.keys_file)
        worktree = pathlib.Path(flag_values(the_instance_run(m, "a"), "-w")[0])
        r = m.down("a")
        assert r.returncode == 0, r.stderr
        calls = [c for c in m.docker_calls() if c[0] in ("stop", "rm")]
        assert calls == [["stop", "peacock-a"], ["rm", "peacock-a"]], calls
        assert worktree.is_dir()
        up_ok(m, "a", "--env", m.keys_file)
        assert flag_values(the_instance_run_last(m, "a"), "-w") == [str(worktree)]


def the_instance_run_last(m, name):
    return [c for c in m.instance_runs() if flag_values(c, "--name") == [f"peacock-{name}"]][-1]


def test_down_refuses_an_unknown_instance():
    with Machine() as m:
        for args in [("a",), (), ("a", "b"), ("-x",)]:
            r = m.down(*args)
            assert r.returncode == 2 and len(r.stderr.strip().splitlines()) == 1, (args, r.stderr)
            assert not [c for c in m.docker_calls() if c[0] in ("stop", "rm")], m.docker_calls()
        r = m.down("--help")
        assert r.returncode == 0 and "down.sh <instance>" in r.stdout, r


if __name__ == "__main__":
    raise SystemExit(main(globals()))
