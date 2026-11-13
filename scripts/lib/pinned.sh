# shellcheck shell=bash
#
# A tool fetched onto a VM at its docker/pins.env version, as the images take it: the download
# checked against its pinned sha256 before anything is unpacked. Each function prints one line to
# stderr when it refuses.

# pin <pins.env> <name> — the value of <name>, which the file holds once.
pin() {
  local line
  line=$(grep "^$2=" "$1" || true)
  if [[ ! "$line" =~ ^$2=[^[:space:]]+$ ]]; then
    echo "${0##*/}: $1 holds no single $2" >&2
    return 2
  fi
  echo "${line#*=}"
}

# download_checked <url> <sha256> <file> — <url> into <file>, kept only when its sha256 is
# <sha256>. Returns 1, naming the download, when it cannot be had or is not that. Called as a
# statement, so set -e stops the caller.
download_checked() {
  local got
  if ! curl -fsSL --retry 3 -o "$3" "$1"; then
    rm -f "$3"
    echo "${0##*/}: cannot download $1" >&2
    return 1
  fi
  got=$(sha256sum < "$3")
  got=${got%% *}
  if [ "$got" != "$2" ]; then
    rm -f "$3"
    echo "${0##*/}: ${1##*/} has sha256 $got, pinned $2" >&2
    return 1
  fi
}

# unzip_into <zip> <dir> — every member with its mode, by python3: a fresh VM may have no unzip.
unzip_into() {
  python3 -I - "$1" "$2" << 'PY'
import os, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    for member in z.infolist():
        path = z.extract(member, sys.argv[2])
        mode = member.external_attr >> 16 & 0o777
        if mode:
            os.chmod(path, mode)
PY
}
