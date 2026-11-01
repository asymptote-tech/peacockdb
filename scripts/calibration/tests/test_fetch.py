"""`fetch_record.sh`: the pinned record of one device and dataset, from the key the pin implies.

The pin's path is the name: `<device>/<dataset>.sf<sf>/records.tsv` pins the object
`<device>/<dataset>.sf<sf>/<sha256>.tsv`, fetched to
`testdata/calibration/<device>/<dataset>.sf<sf>/`. A line from before devices,
`<dataset>.sf<sf>/records.tsv`, is the same rule one level up. Run in a copy of the script and
its library over a temporary tree, the bucket a directory behind a fake CLI.
"""

import hashlib
import os
import pathlib
import shutil
import subprocess
import tempfile

from harness import main

REPO = pathlib.Path(__file__).resolve().parents[3]
FAKE_AWS = """#!/bin/bash
# The bucket as a directory: `... s3 cp s3://calibration/<key> -` prints the file at <key>.
for arg; do
  case "$arg" in s3://calibration/*) exec cat "$FAKE_BUCKET/${arg#s3://calibration/}" ;; esac
done
exit 2
"""


def tree(tmp, pins, objects):
    """A repo of the fetch script alone, a pin file, and a bucket of `objects` by key."""
    for rel in ("scripts/calibration/fetch_record.sh", "scripts/lib/calibration-bucket.sh", "scripts/lib/s3.sh"):
        (tmp / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO / rel, tmp / rel)
    pin = tmp / "testdata/calibration/records.sha256"
    pin.parent.mkdir(parents=True)
    pin.write_text("".join(f"{sha}  {path}\n" for sha, path in pins))
    for key, body in objects.items():
        (tmp / "bucket" / key).parent.mkdir(parents=True, exist_ok=True)
        (tmp / "bucket" / key).write_text(body)
    aws = tmp / "aws"
    aws.write_text(FAKE_AWS)
    aws.chmod(0o755)


def fetch(tmp, name):
    env = {**os.environ, "AWS": str(tmp / "aws"), "FAKE_BUCKET": str(tmp / "bucket")}
    return subprocess.run(["bash", str(tmp / "scripts/calibration/fetch_record.sh"), name],
                          env=env, capture_output=True, text=True, timeout=30)


RECORD = "# run: device=NVIDIA H100 80GB HBM3\ndataset\tsf\ntpch\t40\n"
SHA = hashlib.sha256(RECORD.encode()).hexdigest()


def test_a_device_s_record_comes_from_its_own_key_to_its_own_directory():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        pins = [(SHA, "h100-80gb-hbm3/tpch.sf40/records.tsv"), ("0" * 64, "tpch.sf40/records.tsv")]
        tree(tmp, pins, {f"h100-80gb-hbm3/tpch.sf40/{SHA}.tsv": RECORD})
        r = fetch(tmp, "h100-80gb-hbm3/tpch.sf40")
        assert r.returncode == 0, r.stderr
        fetched = tmp / "testdata/calibration/h100-80gb-hbm3/tpch.sf40/records.tsv"
        assert fetched.read_text() == RECORD


def test_a_pin_from_before_devices_is_fetched_as_it_was():
    """The H200's lines name no device until their objects are copied under `h200/`."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp, [(SHA, "tpch.sf40/records.tsv")], {f"tpch.sf40/{SHA}.tsv": RECORD})
        r = fetch(tmp, "tpch.sf40")
        assert r.returncode == 0, r.stderr
        assert (tmp / "testdata/calibration/tpch.sf40/records.tsv").read_text() == RECORD


def test_a_name_that_is_not_a_pin_path_is_refused_before_any_fetch():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree(tmp, [(SHA, "h100-80gb-hbm3/tpch.sf40/records.tsv")], {})
        for name in ["H100/tpch.sf40", "a/b/tpch.sf40", "h100 80gb/tpch.sf40", "tpch"]:
            r = fetch(tmp, name)
            assert r.returncode != 0 and "is not" in r.stderr, (name, r.stderr)
        assert not (tmp / "testdata/calibration/h100-80gb-hbm3").exists()


if __name__ == "__main__":
    raise SystemExit(main(globals()))
