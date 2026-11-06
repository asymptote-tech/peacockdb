"""`fetch_embeddings.sh` over the pinned origins.

It runs from a copy of itself whose `testdata/embeddings.sha256` pins small stand-ins, against a
fake `curl` on PATH that records its argv and serves a file only where a test names one. The base
slice is a sparse file of the real size, since its check reads the sf1 prefix. The committed pin
is checked on its own, since no test here can hold the real 1.1 GB. The tests need ~2.5 GiB free
in the temporary directory: the free-space check counts the real sizes.
"""
import hashlib
import io
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
TREE = ["testdata/fetch_embeddings.sh"]
GLOVE_100D = b"the 0.1 0.2\nof 0.3 0.4\n"


def glove_zip():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("glove.6B.100d.txt", GLOVE_100D)
        z.writestr("glove.6B.50d.txt", b"not extracted\n")
    return buf.getvalue()


ARTIFACTS = {
    "deep_base.sf1.fbin": b"deep base slice",
    "deep_query.public.10K.fbin": b"deep query set",
    "deep_groundtruth.public.10K.ibin": b"deep groundtruth",
    "glove.6B.zip": glove_zip(),
}
SF1_BASE_BYTES = 8 + 800000 * 96 * 4
DEEP = "https://storage.yandexcloud.net/yandex-research/ann-datasets/DEEP"


def zeros_sha256(size):
    h, chunk = hashlib.sha256(), bytes(1 << 20)
    for _ in range(size // len(chunk)):
        h.update(chunk)
    h.update(bytes(size % len(chunk)))
    return h.hexdigest()


# The base slice is SF1_BASE_BYTES of zeros, the rest the stand-ins.
PINS = {**{name: hashlib.sha256(data).hexdigest() for name, data in ARTIFACTS.items()},
        "deep_base.sf1.fbin": zeros_sha256(SF1_BASE_BYTES)}

# curl that records its argv and fails, or, for a URL whose last part names a file in
# FAKE_CURL_SERVE, writes that file to its -o.
FAKE_CURL = r'''#!/usr/bin/env python3
import os, pathlib, shutil, sys
args = sys.argv[1:]
with open(os.environ["FAKE_CURL_LOG"], "a") as f:
    f.write("\n".join(args) + "\n----\n")
served = pathlib.Path(os.environ.get("FAKE_CURL_SERVE", "/nonexistent"))
url = [a for a in args if a.startswith("https://")][-1]
source = served / url.rsplit("/", 1)[-1]
if not source.is_file() or "-o" not in args:
    sys.exit(7)
shutil.copyfile(source, args[args.index("-o") + 1])
'''

# unzip extracting glove.6B.100d.txt where it was asked to, to stdout or into -d. With
# FAKE_UNZIP_KILL set it writes part of the member and then SIGKILLs the shell running
# fetch_embeddings.sh, as a power cut or an OOM kill would, so no handler of the script runs.
FAKE_UNZIP = r'''#!/usr/bin/env python3
import os, signal, sys
args = sys.argv[1:]
text = GLOVE_TEXT
if os.environ.get("FAKE_UNZIP_KILL"):
    text = text[:7]
if "-p" in args:
    sys.stdout.write(text); sys.stdout.flush()
else:
    with open(args[args.index("-d") + 1] + "/glove.6B.100d.txt", "w") as f:
        f.write(text)
if os.environ.get("FAKE_UNZIP_KILL"):
    os.kill(os.getppid(), signal.SIGKILL)
'''.replace("GLOVE_TEXT", repr(GLOVE_100D.decode()))


def fill_cache(cache, leave_out=()):
    """What a completed fetch leaves: the sparse base slice, the stand-ins, GloVe extracted."""
    cache.mkdir(parents=True, exist_ok=True)
    for name, data in {**ARTIFACTS, "glove.6B.100d.txt": GLOVE_100D}.items():
        if name in leave_out:
            continue
        if name == "deep_base.sf1.fbin":
            with open(cache / name, "wb") as f:
                f.truncate(SF1_BASE_BYTES)
        else:
            (cache / name).write_bytes(data)


def curl_calls(log):
    """Each fake curl call's argv."""
    return [call.strip("\n").split("\n") for call in log.read_text().split("----\n") if call.strip()]


class Tree:
    """A copy of the script under test, its own pin, and a fake curl on PATH."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.root = tmp / "repo"
        for rel in TREE:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.root / rel)
        (self.root / "testdata" / "embeddings.sha256").write_text(
            "".join(f"{sha}  {name}\n" for name, sha in PINS.items()))
        self.script = self.root / "testdata" / "fetch_embeddings.sh"
        self.cache = self.root / "testdata" / "embeddings-cache"
        self.served = tmp / "served"
        self.served.mkdir()
        self.bin_dir = tmp / "bin"
        self.bin_dir.mkdir()
        self.install("curl", FAKE_CURL)
        self.curl_log = tmp / "curl.log"
        self.curl_log.touch()
        drop = ("CI", "GITHUB_ACTIONS", "PEACOCK_NO_FETCH")
        self.env = {k: v for k, v in os.environ.items() if k not in drop}
        self.env.update(PATH=f"{self.bin_dir}:{os.environ['PATH']}", FAKE_CURL_LOG=str(self.curl_log),
                        FAKE_CURL_SERVE=str(self.served))
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def install(self, name, text):
        (self.bin_dir / name).write_text(text)
        (self.bin_dir / name).chmod(0o755)

    def serve(self, url_name, data):
        """The origin answers a URL ending in `url_name` with `data`."""
        (self.served / url_name).write_bytes(data)

    def linked_script(self):
        """fetch_embeddings.sh reached through a symlink in another directory, as a cache kept outside
        the tree is made; returns the link, whose directory the cache belongs beside."""
        link = self.root.parent / "elsewhere" / "testdata" / "fetch_embeddings.sh"
        link.parent.mkdir(parents=True)
        link.symlink_to(self.script)
        return link

    def fetch(self, *args, script=None, **env):
        return subprocess.run([str(script or self.script), *args], capture_output=True,
                              text=True, env={**self.env, **env}, stdin=subprocess.DEVNULL, timeout=120)


def test_ci_refuses_the_origins():
    with Tree() as t:
        for var in ("CI", "GITHUB_ACTIONS"):
            proc = t.fetch(**{var: "1"})
            assert proc.returncode == 1 and "must not run from CI/automation" in proc.stderr, (var, proc.stderr)
            assert t.curl_log.read_text() == "", f"{var} set and an origin was reached"
            assert not t.cache.exists(), var


def test_no_fetch_refuses_before_any_call():
    with Tree() as t:
        proc = t.fetch(PEACOCK_NO_FETCH="1")
        assert proc.returncode == 1 and "PEACOCK_NO_FETCH" in proc.stderr, proc.stderr
        assert t.curl_log.read_text() == "", t.curl_log.read_text()
        assert not t.cache.exists()


def test_origin_fetches_nothing_the_cache_already_holds():
    with Tree() as t:
        fill_cache(t.cache)
        proc = t.fetch()
        assert proc.returncode == 0, proc.stderr
        assert t.curl_log.read_text() == "", t.curl_log.read_text()
        verified = [line for line in proc.stdout.splitlines() if line.startswith("  cached+verified: ")]
        assert len(verified) == 4, proc.stdout


def test_origin_fetches_a_missing_query_set_from_its_origin():
    with Tree() as t:
        fill_cache(t.cache, leave_out=("deep_query.public.10K.fbin",))
        proc = t.fetch()
        assert proc.returncode != 0, proc.stdout  # the fake curl serves nothing here
        [argv] = curl_calls(t.curl_log)
        assert f"{DEEP}/query.public.10K.fbin" in argv, argv
        pairs = list(zip(argv, argv[1:]))
        assert ("-C", "-") in pairs and ("-o", str(t.cache / "deep_query.public.10K.fbin")) in pairs, argv


def test_origin_through_a_symlink_caches_beside_the_link():
    with Tree() as t:
        link = t.linked_script()
        fill_cache(link.parent / "embeddings-cache")
        proc = t.fetch(script=link)
        assert proc.returncode == 0, proc.stderr
        assert t.curl_log.read_text() == "", t.curl_log.read_text()
        assert not t.cache.exists(), "the cache went beside the script, not the link"


def test_an_interrupted_unzip_leaves_no_glove_text_a_rerun_accepts():
    with Tree() as t:
        fill_cache(t.cache, leave_out=("glove.6B.zip", "glove.6B.100d.txt"))
        t.serve("glove.6B.zip", ARTIFACTS["glove.6B.zip"])
        t.install("unzip", FAKE_UNZIP)
        killed = t.fetch(FAKE_UNZIP_KILL="1")
        assert killed.returncode == -9, (killed.returncode, killed.stderr)
        assert (t.cache / "glove.6B.zip").read_bytes() == ARTIFACTS["glove.6B.zip"]
        assert not (t.cache / "glove.6B.100d.txt").exists(), "a killed unzip left a short glove.6B.100d.txt"
        rerun = t.fetch()
        assert rerun.returncode == 0, rerun.stderr
        assert (t.cache / "glove.6B.100d.txt").read_bytes() == GLOVE_100D


def test_the_committed_pin_names_each_artifact_once():
    lines = (REPO / "testdata" / "embeddings.sha256").read_text().splitlines()
    entries = [line.split("  ") for line in lines if line and not line.startswith("#")]
    assert all(len(e) == 2 and re.fullmatch(r"[0-9a-f]{64}", e[0]) for e in entries), entries
    names = [name for _, name in entries]
    assert sorted(names) == sorted(ARTIFACTS), names


if __name__ == "__main__":
    raise SystemExit(main(globals()))
