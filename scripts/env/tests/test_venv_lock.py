"""docker/venv.lock: the image's whole venv, every package at an exact version with the sha256 of
the one file pip installs, agreeing with pins.env and installed by the Dockerfile with nothing
resolved at build time. The committed costs depend on these bytes."""
import pathlib
import re
import sys

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

DOCKER = pathlib.Path(__file__).resolve().parents[3] / "docker"
LINE = re.compile(r"(?P<name>[a-z0-9][a-z0-9._-]*)==(?P<version>[^\s=]+) --hash=sha256:[0-9a-f]{64}")

# pins.env key -> the package it pins in the venv.
PINNED = {"NUMPY_VERSION": "numpy", "PANDAS_VERSION": "pandas", "PYARROW_VERSION": "pyarrow",
          "MATPLOTLIB_VERSION": "matplotlib", "PY_DUCKDB_VERSION": "duckdb"}


def lock():
    lines = [line for line in (DOCKER / "venv.lock").read_text().splitlines() if line and not line.startswith("#")]
    bad = [line for line in lines if not LINE.fullmatch(line)]
    assert not bad, f"venv.lock lines not `name==version --hash=sha256:<64 hex>`: {bad}"
    names = [LINE.fullmatch(line)["name"] for line in lines]
    assert len(names) == len(set(names)), f"venv.lock names a package twice: {names}"
    return {m["name"]: m["version"] for m in map(LINE.fullmatch, lines)}


def pins():
    pairs = (line.partition("=") for line in (DOCKER / "pins.env").read_text().splitlines()
             if line and not line.startswith("#"))
    return {name: value for name, _, value in pairs}


def test_every_venv_line_is_pinned_and_hashed():
    assert lock()


def test_the_lock_agrees_with_pins_env():
    locked, pinned = lock(), pins()
    wrong = {pkg: (pinned[key], locked.get(pkg)) for key, pkg in PINNED.items() if locked.get(pkg) != pinned[key]}
    assert not wrong, f"pins.env vs venv.lock: {wrong}"


def test_the_image_installs_the_lock_and_resolves_nothing():
    text = (DOCKER / "build.Dockerfile").read_text()
    assert re.search(r"pip install [^\n]*--require-hashes --no-deps -r /tmp/venv\.lock", text), \
        "build.Dockerfile does not install docker/venv.lock with --require-hashes --no-deps"
    assert not re.search(r'pip install[^\n]*"\w+==\$\{', text), "build.Dockerfile still installs by pins.env version"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
