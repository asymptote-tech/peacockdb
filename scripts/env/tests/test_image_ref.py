"""image-ref.sh: the image a tree builds or runs in, found by its inputs tag and refused unless its
label is the tree's.

It runs in a throwaway git repo holding its inputs, as test_image.py's does. The docker CLI is a
fake on PATH that records its argv, answers `image inspect` from a map of local images and
`buildx imagetools inspect` from a map of published ones, so nothing here pulls.
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
REGISTRY = "ghcr.io/asymptote-tech"
SUBMODULE_SHA = "43505bb975f46ce4e140fdaf55192611d6e231ac"
DIGEST = "sha256:" + "b8" * 32

FAKE_DOCKER = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_DOCKER_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
local = json.loads(os.environ.get("FAKE_DOCKER_LOCAL", "{}"))
remote = json.loads(os.environ.get("FAKE_DOCKER_REMOTE", "{}"))
if a[:2] == ["image", "inspect"]:
    ref = a[-1]
    if ref not in local:
        print(f"Error response from daemon: No such image: {ref}", file=sys.stderr); sys.exit(1)
    print(local[ref]); sys.exit(0)
if a[:3] == ["buildx", "imagetools", "inspect"]:
    ref = a[3]
    if ref not in remote:
        print(f"ERROR: {ref}: not found", file=sys.stderr); sys.exit(1)
    digest, label = remote[ref]
    print(json.dumps({"name": ref, "manifest": {"digest": digest},
                      "image": {"config": {"Labels": {"peacockdb.inputs": label}}}}))
    sys.exit(0)
print(f"fake docker: unexpected call {a}", file=sys.stderr)
sys.exit(1)
'''


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True,
                          text=True).stdout.strip()


class Repo:
    """A throwaway git repo holding image-ref.sh and the inputs its image tag is made from."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.root = tmp / "repo"
        for rel in ["scripts/env/image-ref.sh", "scripts/lib/image-inputs.sh", "cpp/CMakeLists.txt"]:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, self.root / rel)
        shutil.copytree(REPO / "docker", self.root / "docker")
        (self.root / "third_party" / "cudf").mkdir(parents=True)
        git(self.root, "init", "-q", "-b", "ENS-x")
        git(self.root, "add", "-A")
        git(self.root, "update-index", "--add", "--cacheinfo", f"160000,{SUBMODULE_SHA},third_party/cudf")
        git(self.root, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "inputs")
        bin_dir = tmp / "bin"
        bin_dir.mkdir()
        (bin_dir / "docker").write_text(FAKE_DOCKER)
        (bin_dir / "docker").chmod(0o755)
        self.log = tmp / "docker.log"
        self.log.touch()
        self.env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "FAKE_DOCKER_LOG": str(self.log)}
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def inputs(self, kind, cudf):
        """The tree's own (label, inputs tag), from the library image.sh labels images with."""
        script = (f'. scripts/lib/image-inputs.sh; image_inputs . {kind} {cudf} | paste -sd" "; '
                  f'image_inputs_tag . {kind} {cudf}')
        out = subprocess.run(["bash", "-c", script], cwd=self.root, check=True, capture_output=True,
                             text=True).stdout.splitlines()
        return out[0], out[1]

    def run(self, *args, local=None, remote=None):
        env = {**self.env, "FAKE_DOCKER_LOCAL": json.dumps(local or {}),
               "FAKE_DOCKER_REMOTE": json.dumps(remote or {})}
        proc = subprocess.run([str(self.root / "scripts/env/image-ref.sh"), *args], cwd=self.root,
                              env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=60)
        proc.calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        return proc


def with_pins_hash(label, value):
    return re.sub(r"docker/pins\.env=[0-9a-f]+", f"docker/pins.env={value}", label)


def test_prefers_the_local_inputs_tag():
    with Repo() as r:
        label, tag = r.inputs("build", "25.02")
        ref = f"{REGISTRY}/peacockdb-build:{tag}"
        p = r.run("build", "25.02", local={ref: label}, remote={ref: [DIGEST, label]})
        assert p.returncode == 0, (p.returncode, p.stderr)
        assert p.stdout == f"{ref}\n", p.stdout
        assert not [c for c in p.calls if c[:1] == ["buildx"]], p.calls


def test_resolves_the_remote_digest():
    with Repo() as r:
        label, tag = r.inputs("build", "25.02")
        ref = f"{REGISTRY}/peacockdb-build:{tag}"
        p = r.run("build", "25.02", remote={ref: [DIGEST, label]})
        assert p.returncode == 0, (p.returncode, p.stderr)
        assert re.fullmatch(rf"{re.escape(REGISTRY)}/peacockdb-build@sha256:[0-9a-f]{{64}}\n", p.stdout), p.stdout
        assert p.stdout.strip().endswith(DIGEST), p.stdout
        inspected = [c for c in p.calls if c[:3] == ["buildx", "imagetools", "inspect"]]
        assert inspected and inspected[0][3] == ref, p.calls


def test_refuses_unpublished_inputs():
    with Repo() as r:
        _, tag = r.inputs("build", "25.02")
        p = r.run("build", "25.02")
        assert p.returncode == 2, (p.returncode, p.stdout, p.stderr)
        assert p.stdout == "", p.stdout
        assert tag in p.stderr, p.stderr
        assert "scripts/env/image.sh build build 25.02" in p.stderr, p.stderr
        assert "gh workflow run images.yml --ref ENS-x -f kind=build -f cudf=25.02" in p.stderr, p.stderr


def test_refuses_a_label_that_differs():
    with Repo() as r:
        label, tag = r.inputs("build", "25.02")
        ref = f"{REGISTRY}/peacockdb-build:{tag}"
        other = with_pins_hash(label, "0" * 64)
        assert other != label
        for where in ({"remote": {ref: [DIGEST, other]}}, {"local": {ref: other}}):
            p = r.run("build", "25.02", **where)
            assert p.returncode == 2, (where, p.returncode, p.stdout, p.stderr)
            assert p.stdout == "", p.stdout
            assert "docker/pins.env" in p.stderr, p.stderr
            assert "docker/conda/cudf-25.02.spec" not in p.stderr, p.stderr


def test_refuses_an_image_without_a_label():
    with Repo() as r:
        _, tag = r.inputs("run", "25.02")
        ref = f"{REGISTRY}/peacockdb-run:{tag}"
        p = r.run("run", "25.02", local={ref: "<no value>"})
        assert p.returncode == 2 and "peacockdb.inputs" in p.stderr, (p.returncode, p.stderr)
        assert p.stdout == ""


def test_refuses_the_run_image_of_a_build_only_cudf():
    with Repo() as r:
        p = r.run("run", "25.10")
        assert p.returncode == 2, (p.returncode, p.stderr)
        assert "25.10" in p.stderr and "build image only" in p.stderr, p.stderr
        assert p.calls == [], p.calls


def test_refuses_bad_arguments_before_docker():
    with Repo() as r:
        for args in (["build"], ["image", "25.02"], ["build", "24.12"], ["build", "25.02", "x"]):
            p = r.run(*args)
            assert p.returncode == 2 and p.stderr, (args, p.returncode, p.stderr)
        assert "25.02" in r.run("build", "24.12").stderr
        assert r.run("build").calls == []


if __name__ == "__main__":
    raise SystemExit(main(globals()))
