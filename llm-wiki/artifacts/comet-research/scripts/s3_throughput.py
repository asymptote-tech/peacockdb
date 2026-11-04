#!/usr/bin/env python3
"""Read throughput from S3 or a local file, printed as one TSV row (comet-research Task 3).

    s3_throughput.py --bucket B --key K --threads N [--endpoint-url U] [--profile P]
    s3_throughput.py --file F --threads N        # O_DIRECT, after dropping the page cache (sudo)

Row: vm threads bytes seconds GB_per_s source client_cpu_pct. The object (or file) is split into
--part-mb ranges; N threads take ranges from one queue, each a ranged GET (or pread) read in 8 MiB
pieces into nothing. GB is 1e9 bytes. client_cpu_pct is this process's CPU time over wall time,
100 per busy core. Credentials come from the AWS profile (AWS_PROFILE or --profile); none is ever
printed.
"""
import argparse
import mmap
import os
import socket
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

CHUNK = 8 << 20


def ranges(size: int, part: int) -> list[tuple[int, int]]:
    """Inclusive byte ranges of at most `part` bytes covering [0, size)."""
    return [(a, min(a + part, size) - 1) for a in range(0, size, part)]


def s3_reader(args):
    import boto3
    from botocore.config import Config
    session = boto3.Session(profile_name=args.profile) if args.profile else boto3.Session()
    s3 = session.client("s3", endpoint_url=args.endpoint_url,
                        config=Config(max_pool_connections=max(10, args.threads),
                                      retries={"max_attempts": 5, "mode": "standard"}))
    size = s3.head_object(Bucket=args.bucket, Key=args.key)["ContentLength"]

    def read(rng):
        body = s3.get_object(Bucket=args.bucket, Key=args.key, Range=f"bytes={rng[0]}-{rng[1]}")["Body"]
        n = 0
        while chunk := body.read(CHUNK):
            n += len(chunk)
        return n
    return size, read, f"s3://{args.bucket}/{args.key}"


def file_reader(args):
    if not args.no_drop_cache:
        subprocess.run(["sudo", "sh", "-c", "sync; echo 3 > /proc/sys/vm/drop_caches"], check=True, timeout=120)
    size = os.path.getsize(args.file)
    flags = os.O_RDONLY | (0 if args.no_direct else os.O_DIRECT)

    def read(rng):
        fd = os.open(args.file, flags)
        buf = mmap.mmap(-1, CHUNK)  # page-aligned, as O_DIRECT needs
        try:
            n, off = 0, rng[0]
            while off <= rng[1]:
                got = os.preadv(fd, [buf], off)
                if got <= 0:
                    break
                got = min(got, rng[1] + 1 - off)
                n += got
                off += got
            return n
        finally:
            buf.close()
            os.close(fd)
    return size, read, f"file:{args.file}"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--bucket")
    ap.add_argument("--key")
    ap.add_argument("--file")
    ap.add_argument("--threads", type=int, required=True)
    ap.add_argument("--part-mb", type=int, default=64, help="range size, MiB (a multiple of 8 for O_DIRECT)")
    ap.add_argument("--endpoint-url")
    ap.add_argument("--profile")
    ap.add_argument("--vm", default=socket.gethostname())
    ap.add_argument("--no-direct", action="store_true", help="plain reads (tests; tmpfs has no O_DIRECT)")
    ap.add_argument("--no-drop-cache", action="store_true")
    args = ap.parse_args(argv)
    if bool(args.file) == bool(args.bucket and args.key):
        ap.error("give either --file or --bucket and --key")
    size, read, source = (file_reader if args.file else s3_reader)(args)
    parts = ranges(size, args.part_mb << 20)
    c0, t0 = os.times(), time.perf_counter()
    with ThreadPoolExecutor(max_workers=args.threads) as pool:
        nbytes = sum(pool.map(read, parts))
    secs = time.perf_counter() - t0
    c1 = os.times()
    cpu = (c1.user - c0.user + c1.system - c0.system) / secs * 100
    if nbytes != size:
        sys.exit(f"read {nbytes} of {size} bytes")
    print(f"{args.vm}\t{args.threads}\t{nbytes}\t{secs:.3f}\t{nbytes / secs / 1e9:.3f}\t{source}\t{cpu:.0f}")


if __name__ == "__main__":
    main()
