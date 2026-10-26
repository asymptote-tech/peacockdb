#!/usr/bin/env python3
"""Write each package's md5 into explicit conda specs, as `conda list --explicit --md5` does.

The md5 is anaconda.org's record of the file (api.anaconda.org/dist/...), so a spec exported
without hashes is completed without the host that holds the environment. Every line is checked
against its record before a spec is written: a hash that agrees is kept, a line without one is
filled, and a hash that differs refuses the whole spec.
"""
import argparse
import json
import pathlib
import re
import sys
import urllib.request
from typing import NamedTuple

URL = re.compile(
    r"(?P<url>https://conda\.anaconda\.org/(?P<channel>[^/]+)/(?P<subdir>[^/]+)/"
    r"(?P<file>(?P<name>[^/]+)-(?P<version>[^-/]+)-(?P<build>[^-/]+)\.(?:conda|tar\.bz2)))"
    r"(?:#(?P<md5>.*))?"
)
MD5 = re.compile(r"[0-9a-f]{32}")


class Package(NamedTuple):
    url: str
    channel: str
    subdir: str
    file: str
    name: str
    version: str
    build: str
    md5: str


def package(line):
    match = URL.fullmatch(line)
    if not match:
        raise ValueError(f"not a conda.anaconda.org package URL: {line}")
    return Package(**{**match.groupdict(), "md5": match["md5"] or ""})


def body_start(lines):
    if "@EXPLICIT" not in lines:
        raise ValueError("not an explicit spec: no @EXPLICIT line")
    return lines.index("@EXPLICIT") + 1


def is_package_line(line):
    return bool(line) and not line.startswith("#")


def packages(text):
    lines = text.splitlines()
    return [package(line) for line in lines[body_start(lines):] if is_package_line(line)]


def api_url(url):
    p = package(url)
    return f"https://api.anaconda.org/dist/{p.channel}/{p.name}/{p.version}/{p.subdir}/{p.file}"


def hashed(text, md5_of):
    """`text` with every package line carrying `#<md5>`, md5_of(url) giving each record's hash."""
    lines = text.splitlines()
    start = body_start(lines)
    for i in range(start, len(lines)):
        if is_package_line(lines[i]):
            held = package(lines[i])
            md5 = md5_of(held.url)
            if not (isinstance(md5, str) and MD5.fullmatch(md5)):
                raise ValueError(f"no md5 for {held.url}: {md5!r}")
            if held.md5 and held.md5 != md5:
                raise ValueError(f"{held.url}: the spec's md5 {held.md5} differs from anaconda.org's {md5}")
            lines[i] = f"{held.url}#{md5}"
    return "\n".join(lines) + "\n"


def fetch_md5(url, opener):
    p = package(url)
    try:
        with opener(api_url(url), timeout=30) as reply:
            record = json.load(reply)
    except OSError as e:
        raise OSError(f"anaconda.org has no record for {url}: {e}") from e
    if record.get("basename") != f"{p.subdir}/{p.file}":
        raise ValueError(f"anaconda.org answered for {record.get('basename')!r}, not {url}")
    return record.get("md5")


def main(argv, opener=urllib.request.urlopen):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("specs", nargs="+", type=pathlib.Path, help="explicit specs, rewritten in place")
    specs = parser.parse_args(argv).specs
    missing = [str(s) for s in specs if not s.is_file()]
    if missing:
        sys.exit(f"spec_hashes.py: no such spec: {' '.join(missing)}")
    for spec in specs:
        try:
            text = hashed(spec.read_text(), lambda url: fetch_md5(url, opener))
        except (OSError, ValueError) as e:
            sys.exit(f"spec_hashes.py: {spec}: {e}")
        spec.write_text(text)
        print(f"{spec}: {len(packages(text))} packages hashed")


if __name__ == "__main__":
    main(sys.argv[1:])
