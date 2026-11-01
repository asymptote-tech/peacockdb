"""The embeddings cache through S3: `fetch_embeddings.sh --from-s3` and `upload-embeddings.sh`.

Both run from a copy of the tree whose `testdata/embeddings.sha256` pins small stand-ins, against
a fake `aws` on PATH that serves and stores objects in a directory and records its argv. The
origin path's base slice is a sparse file of the real size, since its check reads the sf1 prefix.
The committed pin is checked on its own, since no test here can hold the real 1.1 GB. The tests
need ~2.5 GiB free in the temporary directory: the free-space check counts the real sizes.
"""
import hashlib
import io
import json
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
TREE = ["testdata/fetch_embeddings.sh", "scripts/lib/embeddings-bucket.sh", "scripts/lib/s3.sh",
        "scripts/env/upload-embeddings.sh"]
# The storage the environment names: test_s3.py holds that every bucket script passes it.
STORAGE = {"AWS_ENDPOINT_URL": "https://s3.storage.example.test:443", "AWS_REGION": "test-north9"}
ENDPOINT = ["--endpoint-url", STORAGE["AWS_ENDPOINT_URL"], "--region", STORAGE["AWS_REGION"]]
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
PINS = {name: hashlib.sha256(data).hexdigest() for name, data in ARTIFACTS.items()}
SF1_BASE_BYTES = 8 + 800000 * 96 * 4
DEEP = "https://storage.yandexcloud.net/yandex-research/ann-datasets/DEEP"


def zeros_sha256(size):
    h, chunk = hashlib.sha256(), bytes(1 << 20)
    for _ in range(size // len(chunk)):
        h.update(chunk)
    h.update(bytes(size % len(chunk)))
    return h.hexdigest()


# The origin path's pins: the base slice is SF1_BASE_BYTES of zeros, the rest the stand-ins.
ORIGIN_PINS = {**PINS, "deep_base.sf1.fbin": zeros_sha256(SF1_BASE_BYTES)}

FAKE_AWS = r'''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
with open(os.environ["FAKE_AWS_LOG"], "a") as f:
    f.write(json.dumps(args) + "\n")
root = pathlib.Path(os.environ["FAKE_S3"])
while args and args[0] in ("--endpoint-url", "--region"):
    args = args[2:]
def obj(url):
    assert url.startswith("s3://"), url
    return root / url[len("s3://"):]
if args[:2] == ["s3", "cp"]:
    src, dst = [a for a in args[2:] if not a.startswith("--")]
    if src.startswith("s3://"):
        if not obj(src).is_file():
            sys.exit(f"fatal error: An error occurred (404) when calling the HeadObject operation: Key {src} does not exist")
        shutil.copyfile(obj(src), dst)
    else:
        obj(dst).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, obj(dst))
    sys.exit(0)
if args[:2] == ["s3api", "head-object"]:
    o = root / args[args.index("--bucket") + 1] / args[args.index("--key") + 1]
    if not o.is_file():
        print("An error occurred (404) when calling the HeadObject operation: Not Found", file=sys.stderr)
        sys.exit(254)
    print(o.stat().st_size if "--query" in args else json.dumps({"ContentLength": o.stat().st_size}))
    sys.exit(0)
sys.exit(f"fake aws: no answer for {args}")
'''

FAKE_CURL = '#!/bin/sh\nprintf "%s\\n" "$@" >> "$FAKE_CURL_LOG"\necho ---- >> "$FAKE_CURL_LOG"\nexit 7\n'

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


def bucket():
    lib = REPO / "scripts" / "lib" / "embeddings-bucket.sh"
    return subprocess.run(["bash", "-c", f'. "{lib}"; echo "$EMBEDDINGS_BUCKET"'], check=True,
                          capture_output=True, text=True).stdout.strip()


def pinned_keys():
    return sorted(f"{sha}/{name}" for name, sha in PINS.items())


def fill_origin_cache(cache, leave_out=()):
    """What a completed origin fetch leaves: the sparse base slice, the stand-ins, GloVe extracted."""
    cache.mkdir(parents=True, exist_ok=True)
    for name, data in ARTIFACTS.items():
        if name in leave_out:
            continue
        if name == "deep_base.sf1.fbin":
            with open(cache / name, "wb") as f:
                f.truncate(SF1_BASE_BYTES)
        else:
            (cache / name).write_bytes(data)
    (cache / "glove.6B.100d.txt").write_bytes(GLOVE_100D)


def curl_calls(log):
    """Each fake curl call's argv."""
    return [call.strip("\n").split("\n") for call in log.read_text().split("----\n") if call.strip()]


class Tree:
    """A copy of the scripts under test, its own pin, a fake bucket and a fake aws and curl on PATH."""

    def __init__(self, pins=PINS):
        self.pins = pins

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.root = tmp / "repo"
        for rel in TREE:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, self.root / rel)
        (self.root / "testdata" / "embeddings.sha256").write_text(
            "".join(f"{sha}  {name}\n" for name, sha in self.pins.items()))
        self.script = self.root / "testdata" / "fetch_embeddings.sh"
        self.cache = self.root / "testdata" / "embeddings-cache"
        self.s3 = tmp / "s3"
        (self.s3 / bucket()).mkdir(parents=True)
        self.bin_dir = bin_dir = tmp / "bin"
        bin_dir.mkdir()
        for name, text in (("aws", FAKE_AWS), ("curl", FAKE_CURL)):
            self.install(name, text)
        self.aws_log, self.curl_log = tmp / "aws.log", tmp / "curl.log"
        self.aws_log.touch()
        self.curl_log.touch()
        drop = ("CI", "GITHUB_ACTIONS", "PEACOCK_NO_FETCH")
        self.env = {k: v for k, v in os.environ.items() if k not in drop}
        self.env.update(PATH=f"{bin_dir}:{os.environ['PATH']}", FAKE_S3=str(self.s3),
                        FAKE_AWS_LOG=str(self.aws_log), FAKE_CURL_LOG=str(self.curl_log), **STORAGE)
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def install(self, name, text):
        (self.bin_dir / name).write_text(text)
        (self.bin_dir / name).chmod(0o755)

    def linked_script(self):
        """fetch_embeddings.sh reached through a symlink in another directory, as a cache kept outside
        the tree is made; returns the link, whose directory the cache belongs beside."""
        link = self.root.parent / "elsewhere" / "testdata" / "fetch_embeddings.sh"
        link.parent.mkdir(parents=True)
        link.symlink_to(self.script)
        return link

    def put(self, name, data=None):
        """The object the pin names for `name`, holding `data` (the stand-in by default)."""
        obj = self.s3 / bucket() / self.pins[name] / name
        obj.parent.mkdir(parents=True, exist_ok=True)
        obj.write_bytes(ARTIFACTS[name] if data is None else data)

    def put_all(self):
        for name in ARTIFACTS:
            self.put(name)

    def fetch(self, *args, script=None, **env):
        proc = subprocess.run([str(script or self.script), *args], capture_output=True,
                              text=True, env={**self.env, **env}, stdin=subprocess.DEVNULL, timeout=120)
        return proc, self.take_calls()

    def upload(self, *args):
        proc = subprocess.run([str(self.root / "scripts" / "env" / "upload-embeddings.sh"), *args],
                              capture_output=True, text=True, env=self.env, stdin=subprocess.DEVNULL, timeout=60)
        return proc, self.take_calls()

    def take_calls(self):
        calls = [json.loads(line) for line in self.aws_log.read_text().splitlines()]
        self.aws_log.write_text("")
        return calls


def copies(calls, direction):
    """The bucket keys `aws s3 cp` read (`down`) or wrote (`up`), sorted."""
    keys = []
    for argv in calls:
        args = argv[len(ENDPOINT):]
        if args[:2] != ["s3", "cp"]:
            continue
        src, dst = [a for a in args[2:] if not a.startswith("--")]
        url = src if direction == "down" else dst
        if url.startswith("s3://"):
            keys.append(url[len(f"s3://{bucket()}/"):])
    return sorted(keys)


def assert_every_call_names_the_endpoint(calls):
    assert calls, "no aws call made"
    for argv in calls:
        assert argv[:len(ENDPOINT)] == ENDPOINT, f"an aws call without the endpoint goes to AWS: {argv}"


def test_from_s3_downloads_exactly_the_pinned_keys():
    with Tree() as t:
        t.put_all()
        decoy = t.s3 / bucket() / ("0" * 64) / "deep_base.sf10.fbin"
        decoy.parent.mkdir(parents=True)
        decoy.write_bytes(b"another scale factor")
        proc, calls = t.fetch("--from-s3")
        assert proc.returncode == 0, proc.stderr
        assert copies(calls, "down") == pinned_keys(), calls
        assert_every_call_names_the_endpoint(calls)
        for name, data in ARTIFACTS.items():
            assert (t.cache / name).read_bytes() == data, name
        assert (t.cache / "glove.6B.100d.txt").read_bytes() == GLOVE_100D
        assert not (t.cache / "glove.6B.50d.txt").exists()
        assert not list(t.cache.glob("*.partial")), list(t.cache.iterdir())


def test_from_s3_refuses_a_file_whose_sha256_differs():
    with Tree() as t:
        t.put_all()
        t.put("deep_query.public.10K.fbin", b"not the pinned query set")
        proc, _ = t.fetch("--from-s3")
        assert proc.returncode != 0, proc.stdout
        assert "deep_query.public.10K.fbin" in proc.stderr and PINS["deep_query.public.10K.fbin"] in proc.stderr, \
            proc.stderr
        left = sorted(p.name for p in t.cache.iterdir())
        assert not any(n.startswith("deep_query") for n in left), f"the refused file is in the cache: {left}"


def test_from_s3_replaces_a_cached_file_whose_sha256_differs():
    with Tree() as t:
        t.put_all()
        t.cache.mkdir()
        (t.cache / "deep_groundtruth.public.10K.ibin").write_bytes(b"damaged")
        proc, _ = t.fetch("--from-s3")
        assert proc.returncode == 0, proc.stderr
        assert (t.cache / "deep_groundtruth.public.10K.ibin").read_bytes() == ARTIFACTS["deep_groundtruth.public.10K.ibin"]


def test_from_s3_fetches_nothing_the_cache_already_holds():
    with Tree() as t:
        t.put_all()
        first, _ = t.fetch("--from-s3")
        assert first.returncode == 0, first.stderr
        second, calls = t.fetch("--from-s3")
        assert second.returncode == 0, second.stderr
        assert calls == [], calls


def test_from_s3_refuses_a_scale_factor_with_no_pin_before_any_call():
    with Tree() as t:
        t.put_all()
        proc, calls = t.fetch("--from-s3", "--sf", "10")
        assert proc.returncode != 0, proc.stdout
        assert "--from-s3 serves only the scale factors" in proc.stderr and "SF=10" in proc.stderr \
            and "from the origins" in proc.stderr, proc.stderr
        assert calls == [], calls
        assert not t.cache.exists() or not list(t.cache.iterdir())


def test_ci_refuses_the_origins_and_not_from_s3():
    with Tree() as t:
        t.put_all()
        for var in ("CI", "GITHUB_ACTIONS"):
            origin, _ = t.fetch(**{var: "1"})
            assert origin.returncode == 1 and "must not run from CI/automation" in origin.stderr, (var, origin.stderr)
            assert t.curl_log.read_text() == "", f"{var} set and an origin was reached"
        proc, calls = t.fetch("--from-s3", CI="1", GITHUB_ACTIONS="1")
        assert proc.returncode == 0, proc.stderr
        assert copies(calls, "down") == pinned_keys(), calls


def test_no_fetch_refuses_both_sources_before_any_call():
    with Tree() as t:
        t.put_all()
        for args in ((), ("--from-s3",)):
            proc, calls = t.fetch(*args, PEACOCK_NO_FETCH="1")
            assert proc.returncode == 1 and "PEACOCK_NO_FETCH" in proc.stderr, (args, proc.stderr)
            assert calls == [] and t.curl_log.read_text() == "", (args, calls)
            assert not t.cache.exists(), args


def test_origin_fetches_nothing_the_cache_already_holds():
    with Tree(ORIGIN_PINS) as t:
        fill_origin_cache(t.cache)
        proc, calls = t.fetch()
        assert proc.returncode == 0, proc.stderr
        assert calls == [] and t.curl_log.read_text() == "", (calls, t.curl_log.read_text())
        verified = [line for line in proc.stdout.splitlines() if line.startswith("  cached+verified: ")]
        assert len(verified) == 4, proc.stdout


def test_origin_fetches_a_missing_query_set_from_its_origin():
    with Tree(ORIGIN_PINS) as t:
        fill_origin_cache(t.cache, leave_out=("deep_query.public.10K.fbin",))
        proc, calls = t.fetch()
        assert proc.returncode != 0, proc.stdout  # the fake curl fails
        assert calls == [], calls
        [argv] = curl_calls(t.curl_log)
        assert f"{DEEP}/query.public.10K.fbin" in argv, argv
        pairs = list(zip(argv, argv[1:]))
        assert ("-C", "-") in pairs and ("-o", str(t.cache / "deep_query.public.10K.fbin")) in pairs, argv


def test_origin_through_a_symlink_caches_beside_the_link():
    with Tree(ORIGIN_PINS) as t:
        link = t.linked_script()
        fill_origin_cache(link.parent / "embeddings-cache")
        proc, _ = t.fetch(script=link)
        assert proc.returncode == 0, proc.stderr
        assert t.curl_log.read_text() == "", t.curl_log.read_text()
        assert not t.cache.exists(), "the cache went beside the script, not the link"


def test_from_s3_through_a_symlink_caches_beside_the_link():
    with Tree() as t:
        t.put_all()
        link = t.linked_script()
        proc, calls = t.fetch("--from-s3", script=link)
        assert proc.returncode == 0, proc.stderr
        assert copies(calls, "down") == pinned_keys(), calls
        for name, data in ARTIFACTS.items():
            assert (link.parent / "embeddings-cache" / name).read_bytes() == data, name
        assert not t.cache.exists(), "the cache went beside the script, not the link"


def test_an_interrupted_unzip_leaves_no_glove_text_a_rerun_accepts():
    with Tree() as t:
        t.put_all()
        t.install("unzip", FAKE_UNZIP)
        killed, _ = t.fetch("--from-s3", FAKE_UNZIP_KILL="1")
        assert killed.returncode == -9, (killed.returncode, killed.stderr)
        assert not (t.cache / "glove.6B.100d.txt").exists(), "a killed unzip left a short glove.6B.100d.txt"
        rerun, _ = t.fetch("--from-s3")
        assert rerun.returncode == 0, rerun.stderr
        assert (t.cache / "glove.6B.100d.txt").read_bytes() == GLOVE_100D


def test_upload_puts_each_pinned_file_under_its_key():
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            (t.cache / name).write_bytes(data)
        (t.cache / "glove.6B.100d.txt").write_bytes(GLOVE_100D)
        proc, calls = t.upload()
        assert proc.returncode == 0, proc.stderr
        assert copies(calls, "up") == pinned_keys(), calls
        assert_every_call_names_the_endpoint(calls)
        for name, data in ARTIFACTS.items():
            assert (t.s3 / bucket() / PINS[name] / name).read_bytes() == data, name


def test_upload_is_idempotent():
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            (t.cache / name).write_bytes(data)
        t.put("glove.6B.zip")
        first, calls = t.upload()
        assert first.returncode == 0, first.stderr
        assert copies(calls, "up") == sorted(k for k in pinned_keys() if not k.endswith("glove.6B.zip")), calls
        second, calls = t.upload()
        assert second.returncode == 0, second.stderr
        assert copies(calls, "up") == [], calls


def test_upload_reads_the_cache_it_is_given():
    with Tree() as t:
        elsewhere = t.root.parent / "elsewhere"
        elsewhere.mkdir()
        for name, data in ARTIFACTS.items():
            (elsewhere / name).write_bytes(data)
        proc, calls = t.upload("--cache", str(elsewhere))
        assert proc.returncode == 0, proc.stderr
        assert copies(calls, "up") == pinned_keys(), calls


def test_upload_refuses_a_file_whose_sha256_differs_before_any_upload():
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            (t.cache / name).write_bytes(data)
        (t.cache / "glove.6B.zip").write_bytes(b"a different zip")
        proc, calls = t.upload()
        assert proc.returncode != 0, proc.stdout
        assert "glove.6B.zip" in proc.stderr and PINS["glove.6B.zip"] in proc.stderr, proc.stderr
        assert copies(calls, "up") == [], calls
        assert not list((t.s3 / bucket()).iterdir())


def test_upload_refuses_a_missing_file_before_any_upload():
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            if name != "glove.6B.zip":
                (t.cache / name).write_bytes(data)
        proc, calls = t.upload()
        assert proc.returncode != 0, proc.stdout
        assert "glove.6B.zip is missing" in proc.stderr, proc.stderr
        assert calls == [], calls


def test_upload_refuses_an_unknown_argument_before_any_call():
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            (t.cache / name).write_bytes(data)
        for args in (["--bogus"], ["--cache"], ["stray"]):
            proc, calls = t.upload(*args)
            assert proc.returncode != 0 and proc.stderr.strip(), (args, proc.stdout)
            assert calls == [], (args, calls)


def test_upload_has_help():
    with Tree() as t:
        proc, calls = t.upload("--help")
        assert proc.returncode == 0, proc.stderr
        assert "upload-embeddings.sh" in proc.stdout and "--cache" in proc.stdout, proc.stdout
        assert calls == []


def test_the_committed_pin_names_each_artifact_once():
    lines = (REPO / "testdata" / "embeddings.sha256").read_text().splitlines()
    entries = [line.split("  ") for line in lines if line and not line.startswith("#")]
    assert all(len(e) == 2 and re.fullmatch(r"[0-9a-f]{64}", e[0]) for e in entries), entries
    names = [name for _, name in entries]
    assert sorted(names) == sorted(ARTIFACTS), names


if __name__ == "__main__":
    raise SystemExit(main(globals()))
