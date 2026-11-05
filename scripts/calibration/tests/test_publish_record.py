"""`publish_record.sh`: split records into the bucket by content, from dev, and pinned.

`build-test-shadgpu.sh --pull-benchmarks` publishes through shad-gpu; a run under `bench-each.sh`
is joined and split on dev, so its files are published from here. Each object is
`<device>/<dataset>.sf<sf>/<sha256>.tsv`, read back before its pin line is written. Run in a copy
of the script and its library over a temporary tree, the bucket a directory behind a fake CLI.
"""

import hashlib
import os
import pathlib
import shutil
import subprocess
import tempfile

from harness import main

REPO = pathlib.Path(__file__).resolve().parents[3]
# The bucket as a directory. `s3 cp <file> s3://calibration/<key>` stores the file, `s3 cp
# s3://calibration/<key> -` prints it; FAKE_DAMAGE makes the read-back print other bytes.
FAKE_AWS = """#!/bin/bash
printf '%s\\n' "$*" >> "$FAKE_AWS_LOG"
args=("$@")
n=${#args[@]}
src=${args[n-2]} dst=${args[n-1]}
case "$dst" in
  s3://calibration/*) key=${dst#s3://calibration/}; mkdir -p "$FAKE_BUCKET/$(dirname "$key")"
                      exec cp "$src" "$FAKE_BUCKET/$key" ;;
esac
case "$src" in
  s3://calibration/*) [ -z "${FAKE_DAMAGE:-}" ] || { echo damaged; exit 0; }
                      exec cat "$FAKE_BUCKET/${src#s3://calibration/}" ;;
esac
exit 2
"""
PINS = ("b6a0372ee13ab154a54cdd9490b1e48b135bad73d7a788b88c3a72be304d0c90  tpcds.sf40/records.tsv\n"
        "1013141a77e9fa367176ed8f99167bfdb23e84c72b4e5155efafae918a77def2  tpch.sf40/records.tsv\n")
RECORDS = {"h100-80gb-hbm3/tpch.sf40": "# run: device=NVIDIA H100 80GB HBM3\ndataset\ttpch\n",
           "h100-80gb-hbm3/tpcds.sf40": "# run: device=NVIDIA H100 80GB HBM3\ndataset\ttpcds\n"}


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def tree(tmp):
    for rel in ("scripts/calibration/publish_record.sh", "scripts/lib/calibration-bucket.sh", "scripts/lib/s3.sh"):
        (tmp / rel).parent.mkdir(parents=True, exist_ok=True)
        if (REPO / rel).exists():
            shutil.copy2(REPO / rel, tmp / rel)
    pin = tmp / "testdata/calibration/records.sha256"
    pin.parent.mkdir(parents=True)
    pin.write_text(PINS)
    for name, text in RECORDS.items():
        (tmp / "split" / name).mkdir(parents=True)
        (tmp / "split" / name / "records.tsv").write_text(text)
    aws = tmp / "aws"
    aws.write_text(FAKE_AWS)
    aws.chmod(0o755)
    (tmp / "bucket").mkdir()


def publish(tmp, *args, env=None):
    env = {**os.environ, "AWS": str(tmp / "aws"), "FAKE_BUCKET": str(tmp / "bucket"),
           "FAKE_AWS_LOG": str(tmp / "aws.log"), **(env or {})}
    env.pop("AWS_ENDPOINT_URL", None)
    return subprocess.run(["bash", str(tmp / "scripts/calibration/publish_record.sh"), *map(str, args)],
                          env=env, capture_output=True, text=True, timeout=30, cwd=tmp)


def aws_calls(tmp):
    log = tmp / "aws.log"
    return log.read_text().splitlines() if log.exists() else []


def test_each_file_is_uploaded_by_its_sha_and_pinned():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp)
        r = publish(tmp, "--from", tmp / "split", *RECORDS)
        assert r.returncode == 0, (r.stdout, r.stderr)
        for name, text in RECORDS.items():
            assert (tmp / "bucket" / name / f"{sha(text)}.tsv").read_text() == text
            assert (tmp / "testdata/calibration" / name / "records.tsv").read_text() == text
            assert not (tmp / "split" / name / "records.tsv").exists()
        pins = (tmp / "testdata/calibration/records.sha256").read_text().splitlines()
        assert pins == sorted(pins, key=lambda line: line.split()[1]), pins
        assert f"{sha(RECORDS['h100-80gb-hbm3/tpch.sf40'])}  h100-80gb-hbm3/tpch.sf40/records.tsv" in pins
        assert PINS.splitlines()[0] in pins and PINS.splitlines()[1] in pins, pins
        assert all("--endpoint-url https://storage.eu-north1.nebius.cloud:443" in c for c in aws_calls(tmp))


def test_a_republished_name_replaces_its_own_pin_line_only():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp)
        pin = tmp / "testdata/calibration/records.sha256"
        pin.write_text(PINS + f"{'0' * 64}  h100-80gb-hbm3/tpch.sf40/records.tsv\n")
        r = publish(tmp, "--from", tmp / "split", "h100-80gb-hbm3/tpch.sf40")
        assert r.returncode == 0, (r.stdout, r.stderr)
        lines = pin.read_text().splitlines()
        assert [ln for ln in lines if "h100-80gb-hbm3/tpch.sf40" in ln] == [
            f"{sha(RECORDS['h100-80gb-hbm3/tpch.sf40'])}  h100-80gb-hbm3/tpch.sf40/records.tsv"], lines
        assert len(lines) == 3, lines


def test_a_missing_file_or_a_bad_name_is_refused_before_any_upload():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp)
        for args in (["--from", tmp / "split", "h100-80gb-hbm3/tpch.sf40", "h100-80gb-hbm3/tpch.sf1"],
                     ["--from", tmp / "split", "H100/tpch.sf40"],
                     ["--from", tmp / "split"],
                     ["h100-80gb-hbm3/tpch.sf40"]):
            r = publish(tmp, *args)
            assert r.returncode != 0, (args, r.stdout)
        assert aws_calls(tmp) == []
        assert (tmp / "testdata/calibration/records.sha256").read_text() == PINS


def test_a_read_back_that_differs_is_not_pinned():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp)
        r = publish(tmp, "--from", tmp / "split", "h100-80gb-hbm3/tpch.sf40", env={"FAKE_DAMAGE": "1"})
        assert r.returncode == 1, (r.stdout, r.stderr)
        assert "reads back" in r.stderr, r.stderr
        assert (tmp / "testdata/calibration/records.sha256").read_text() == PINS
        assert (tmp / "split/h100-80gb-hbm3/tpch.sf40/records.tsv").exists()


if __name__ == "__main__":
    raise SystemExit(main(globals()))
