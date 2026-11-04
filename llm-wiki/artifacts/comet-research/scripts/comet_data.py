#!/usr/bin/env python3
"""sf40 data for the CPU VM: reuse or fetch a local copy, then fill the eu-west1 buckets.

    comet_data.py need DATASET…                       bytes the datasets' Parquet takes
    comet_data.py prepare DATASET --dest DIR [--reuse-root ROOT]
    comet_data.py upload DATASET --src DIR

`prepare` reuses an existing copy under ROOT (chain L's default /var/lib/peacock), as
ROOT/<ds>-sf40, <ds>.sf40, <ds>_sf40 or <ds>/sf40, only if every Parquet object of the eu-north1
bucket is there at the same size; DIR then becomes a symlink to it. Otherwise it fetches what DIR
lacks, first checking that --min-free-gb (100) stays free for Spark's spill. `upload` puts into
the eu-west1 bucket of the same name what is missing or of the wrong size there, then checks
every object's size against eu-north1. Both are idempotent, and both print one line with bytes,
seconds and GB/s. eu-north1 is the default profile with its endpoint; eu-west1 is the `eu-west1`
profile. No credential is ever printed.
"""
import argparse
import os
import shutil
import sys
import time
from pathlib import Path

NORTH = ("default", "https://storage.eu-north1.nebius.cloud:443")
WEST = ("eu-west1", "https://storage.eu-west1.nebius.cloud:443")


def reusable(listing: dict[str, int], d: Path) -> bool:
    """True if every .parquet object in the listing is in `d` at the same size."""
    for key, size in listing.items():
        if key.endswith(".parquet"):
            f = Path(d) / key
            if not f.is_file() or f.stat().st_size != size:
                return False
    return True


def to_transfer(src: dict[str, int], dst: dict[str, int]) -> list[str]:
    """The .parquet keys of `src` that `dst` lacks or holds at another size."""
    return sorted(k for k, s in src.items() if k.endswith(".parquet") and dst.get(k) != s)


def client(region):
    import boto3
    from botocore.config import Config
    profile, endpoint = region
    return boto3.Session(profile_name=profile).client(
        "s3", endpoint_url=endpoint, config=Config(max_pool_connections=64))


def listing(s3, bucket: str) -> dict[str, int]:
    out = {}
    for page in s3.get_paginator("list_objects_v2").paginate(Bucket=bucket):
        for o in page.get("Contents", []):
            out[o["Key"]] = o["Size"]
    return out


def transfer_config():
    from boto3.s3.transfer import TransferConfig
    return TransferConfig(max_concurrency=32, multipart_chunksize=64 << 20, multipart_threshold=64 << 20)


def report(what, nbytes, secs):
    rate = nbytes / secs / 1e9 if secs > 0 else 0.0
    print(f"{what}\tbytes={nbytes}\tseconds={secs:.1f}\tGB_per_s={rate:.3f}")


def cmd_need(datasets):
    s3 = client(NORTH)
    print(sum(s for ds in datasets for k, s in listing(s3, f"{ds}-sf40").items() if k.endswith(".parquet")))


def cmd_prepare(ds, dest: Path, reuse_root: Path, min_free_gb: float):
    s3 = client(NORTH)
    src = listing(s3, f"{ds}-sf40")
    for cand in (f"{ds}-sf40", f"{ds}.sf40", f"{ds}_sf40", f"{ds}/sf40"):
        d = reuse_root / cand
        if d.is_dir() and reusable(src, d):
            if dest.is_symlink() or not dest.exists():
                dest.unlink(missing_ok=True)
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.symlink_to(d)
            print(f"{ds}\treused {d}")
            return
    dest.mkdir(parents=True, exist_ok=True)
    have = {p.name: p.stat().st_size for p in dest.glob("*.parquet")}
    keys = to_transfer(src, have)
    missing = sum(src[k] for k in keys)
    free = shutil.disk_usage(dest).free
    if free - missing < min_free_gb * 1e9:
        sys.exit(f"{ds}: {free / 1e9:.0f} GB free, {missing / 1e9:.0f} GB to fetch, "
                 f"and {min_free_gb:.0f} GB must stay free for Spark's spill")
    t, cfg = time.perf_counter(), transfer_config()
    for k in keys:
        tmp = dest / (k + ".part")
        s3.download_file(f"{ds}-sf40", k, str(tmp), Config=cfg)
        os.replace(tmp, dest / k)
    report(f"{ds}\tfetched {len(keys)} objects from eu-north1", missing, time.perf_counter() - t)


def cmd_upload(ds, src_dir: Path):
    north, west = client(NORTH), client(WEST)
    src = listing(north, f"{ds}-sf40")
    keys = to_transfer(src, listing(west, f"{ds}-sf40"))
    t, cfg = time.perf_counter(), transfer_config()
    for k in keys:
        west.upload_file(str(src_dir / k), f"{ds}-sf40", k, Config=cfg)
    report(f"{ds}\tuploaded {len(keys)} objects to eu-west1", sum(src[k] for k in keys), time.perf_counter() - t)
    bad = to_transfer(src, listing(west, f"{ds}-sf40"))
    if bad:
        sys.exit(f"{ds}: eu-west1 sizes differ from eu-north1 for {bad}")
    print(f"{ds}\teu-west1 matches eu-north1")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    n = sub.add_parser("need"); n.add_argument("datasets", nargs="+")
    p = sub.add_parser("prepare"); p.add_argument("dataset"); p.add_argument("--dest", required=True)
    p.add_argument("--reuse-root", default="/var/lib/peacock")
    p.add_argument("--min-free-gb", type=float, default=100)
    u = sub.add_parser("upload"); u.add_argument("dataset"); u.add_argument("--src", required=True)
    a = ap.parse_args(argv)
    if a.cmd == "need":
        cmd_need(a.datasets)
    elif a.cmd == "prepare":
        cmd_prepare(a.dataset, Path(a.dest).expanduser(), Path(a.reuse_root), a.min_free_gb)
    else:
        cmd_upload(a.dataset, Path(a.src).expanduser())


if __name__ == "__main__":
    main()
