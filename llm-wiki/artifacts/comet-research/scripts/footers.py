#!/usr/bin/env python3
"""Per-column compressed and uncompressed bytes of the sf40 Parquet, from footers read by range.

    footers.py BUCKET… > footers.tsv

Reads each `.parquet` object's footer from eu-north1 (default profile) with ranged GETs, never the
data. Prints `dataset	table	column	row_groups	compressed_bytes	uncompressed_bytes`, the dataset
being the bucket name less `-sf40`. No credential is ever printed.
"""
import io
import sys

ENDPOINT = "https://storage.eu-north1.nebius.cloud:443"


def column_sizes(md):
    """(column, row_groups, compressed, uncompressed) per column, summed over row groups."""
    out = []
    for c in range(md.num_columns):
        comp = sum(md.row_group(r).column(c).total_compressed_size for r in range(md.num_row_groups))
        unc = sum(md.row_group(r).column(c).total_uncompressed_size for r in range(md.num_row_groups))
        out.append((md.schema.column(c).name, md.num_row_groups, comp, unc))
    return out


class RangeFile(io.RawIOBase):
    """A read-only file of `size` bytes whose reads become `get(first, last)` calls."""

    def __init__(self, size, get):
        self.size, self.get, self.pos = size, get, 0

    def readable(self):
        return True

    def seekable(self):
        return True

    def seek(self, offset, whence=io.SEEK_SET):
        base = {io.SEEK_SET: 0, io.SEEK_CUR: self.pos, io.SEEK_END: self.size}[whence]
        self.pos = base + offset
        return self.pos

    def tell(self):
        return self.pos

    def read(self, n=-1):
        end = self.size if n < 0 else min(self.size, self.pos + n)
        if end <= self.pos:
            return b""
        data = self.get(self.pos, end - 1)
        self.pos = end
        return data

    def readinto(self, b):
        data = self.read(len(b))
        b[:len(data)] = data
        return len(data)


def main(argv):
    import boto3
    import pyarrow.parquet as pq
    s3 = boto3.Session(profile_name="default").client("s3", endpoint_url=ENDPOINT)
    print("dataset\ttable\tcolumn\trow_groups\tcompressed_bytes\tuncompressed_bytes")
    for bucket in argv[1:]:
        ds = bucket.removesuffix("-sf40")
        objs = [o for page in s3.get_paginator("list_objects_v2").paginate(Bucket=bucket)
                for o in page.get("Contents", []) if o["Key"].endswith(".parquet")]
        for o in sorted(objs, key=lambda o: o["Key"]):
            def get(first, last, key=o["Key"]):
                r = s3.get_object(Bucket=bucket, Key=key, Range=f"bytes={first}-{last}")
                return r["Body"].read()
            md = pq.ParquetFile(RangeFile(o["Size"], get)).metadata
            table = o["Key"].rsplit("/", 1)[-1].removesuffix(".parquet")
            for col, rgs, comp, unc in column_sizes(md):
                print(f"{ds}\t{table}\t{col}\t{rgs}\t{comp}\t{unc}")


if __name__ == "__main__":
    main(sys.argv)
