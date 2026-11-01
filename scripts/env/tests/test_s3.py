"""The S3 endpoint and region every bucket script passes, from AWS_ENDPOINT_URL and AWS_REGION.

The embeddings scripts run in test_embeddings.py's tree; fetch_record.sh in a copy of itself and
its libraries. The aws CLI is a fake that records its argv.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from test_embeddings import ARTIFACTS, ORIGIN_PINS, Tree, fill_origin_cache  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
LIB = REPO / "scripts" / "lib"
ENDPOINT = "https://s3.elsewhere.example.test:8443"
REGION = "test-south4"
OLD = ["--endpoint-url", "https://storage.eu-north1.nebius.cloud:443", "--region", "eu-north-1"]
UNSET = {"AWS_ENDPOINT_URL": "", "AWS_REGION": ""}

FAKE_AWS = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_AWS_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
for arg in sys.argv[1:]:
    if arg.startswith("s3://calibration/"):
        sys.stdout.write(open(os.environ["FAKE_BUCKET"] + "/" + arg[len("s3://calibration/"):]).read())
        sys.exit(0)
sys.exit(2)
'''


def carries(argv):
    pairs = list(zip(argv, argv[1:]))
    return ("--endpoint-url", ENDPOINT) in pairs and ("--region", REGION) in pairs


def bash(script, env):
    return subprocess.run(["bash", "-c", script], capture_output=True, text=True, env={**os.environ, **env},
                          timeout=30)


def calibration_argv(env):
    """The aws argv fetch_record.sh makes for one pinned record, run with `env`."""
    record = "dataset\tsf\ntpch\t40\n"
    sha = hashlib.sha256(record.encode()).hexdigest()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        for rel in ("scripts/calibration/fetch_record.sh", "scripts/lib/calibration-bucket.sh", "scripts/lib/s3.sh"):
            (tmp / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, tmp / rel)
        pin = tmp / "testdata/calibration/records.sha256"
        pin.parent.mkdir(parents=True)
        pin.write_text(f"{sha}  h100-80gb-hbm3/tpch.sf40/records.tsv\n")
        obj = tmp / "bucket" / f"h100-80gb-hbm3/tpch.sf40/{sha}.tsv"
        obj.parent.mkdir(parents=True)
        obj.write_text(record)
        (tmp / "aws").write_text(FAKE_AWS)
        (tmp / "aws").chmod(0o755)
        log = tmp / "aws.log"
        r = subprocess.run(["bash", str(tmp / "scripts/calibration/fetch_record.sh"), "h100-80gb-hbm3/tpch.sf40"],
                           capture_output=True, text=True, timeout=30,
                           env={**os.environ, "AWS": str(tmp / "aws"), "FAKE_AWS_LOG": str(log),
                                "FAKE_BUCKET": str(tmp / "bucket"), **env})
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return r, calls


def test_s3_endpoint_from_env():
    env = {"AWS_ENDPOINT_URL": ENDPOINT, "AWS_REGION": REGION}
    with Tree() as t:
        t.put_all()
        r, calls = t.fetch("--from-s3", **env)
        assert r.returncode == 0, r.stderr
        assert calls and all(carries(c) for c in calls), calls
    with Tree() as t:
        t.cache.mkdir()
        for name, data in ARTIFACTS.items():
            (t.cache / name).write_bytes(data)
        t.env.update(env)
        r, calls = t.upload()
        assert r.returncode == 0, r.stderr
        assert calls and all(carries(c) for c in calls), calls
    r, calls = calibration_argv(env)
    assert r.returncode == 0, r.stderr
    assert len(calls) == 1 and carries(calls[0]), calls


def test_the_embeddings_bucket_names_no_endpoint():
    text = (LIB / "embeddings-bucket.sh").read_text() + (LIB / "s3.sh").read_text()
    for word in ("https://", "nebius", "eu-north"):
        assert word not in text, f"a storage named in the embeddings bucket's scripts: {word}"


def test_the_embeddings_refuse_an_unset_endpoint_before_any_call():
    for unset in ("AWS_ENDPOINT_URL", "AWS_REGION"):
        env = {"AWS_ENDPOINT_URL": ENDPOINT, "AWS_REGION": REGION, unset: ""}
        with Tree() as t:
            t.put_all()
            r, calls = t.fetch("--from-s3", **env)
            assert r.returncode != 0 and unset in r.stderr, (unset, r.stderr)
            assert calls == [] and not t.cache.exists(), (unset, calls)
        with Tree() as t:
            t.cache.mkdir()
            for name, data in ARTIFACTS.items():
                (t.cache / name).write_bytes(data)
            t.env.update(env)
            r, calls = t.upload()
            assert r.returncode != 0 and unset in r.stderr, (unset, r.stderr)
            assert calls == [], calls


def test_the_origins_need_no_endpoint():
    with Tree(ORIGIN_PINS) as t:
        fill_origin_cache(t.cache)
        r, calls = t.fetch(**UNSET)
        assert r.returncode == 0, r.stderr
        assert len([ln for ln in r.stdout.splitlines() if ln.startswith("  cached+verified: ")]) == 4, r.stdout
        assert calls == [], calls


def test_calibration_keeps_the_old_storage_where_the_environment_names_none():
    r = bash(f'. "{LIB}/calibration-bucket.sh"; printf "%s\\n" "${{CALIBRATION_S3[@]}}"', UNSET)
    assert r.returncode == 0, r.stderr
    assert r.stdout.split() == OLD, r.stdout
    r, calls = calibration_argv(UNSET)
    assert r.returncode == 0, r.stderr
    assert calls[0][:4] == OLD, calls


def test_calibration_takes_the_old_storage_whenever_no_endpoint_is_named():
    r, calls = calibration_argv({"AWS_ENDPOINT_URL": "", "AWS_REGION": REGION})
    assert r.returncode == 0, r.stderr
    assert calls[0][:4] == OLD, calls


def test_calibration_refuses_an_endpoint_without_a_region():
    r, calls = calibration_argv({"AWS_ENDPOINT_URL": ENDPOINT, "AWS_REGION": ""})
    assert r.returncode != 0 and "AWS_REGION" in r.stderr, r.stderr
    assert calls == [], calls


if __name__ == "__main__":
    raise SystemExit(main(globals()))
