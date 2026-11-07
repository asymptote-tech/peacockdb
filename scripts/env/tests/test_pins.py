"""The images' committed inputs: docker/pins.env against the spec, and the explicit conda specs."""
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
import spec_hashes  # noqa: E402
from harness import main  # noqa: E402

DOCKER = pathlib.Path(__file__).resolve().parents[3] / "docker"

SEMVER = r"\d+\.\d+\.\d+"
SHA256 = r"[0-9a-f]{64}"

# Every key pins.env holds, each with the value it must match in full. Literal values are the
# spec's pins; the committed costs and device answers depend on those bytes.
PINS = {
    "GCC_25_02": "12",
    "GCC_25_10": "14",
    "GCC_26_02": "14",
    "NVCC_VERSION": r"12\.9\.86",
    "DRIVER_MINIMUM_25_02": r"535\.104\.05",
    "DRIVER_MINIMUM_26_02": r"575\.57\.08",
    "CMAKE_VERSION": r"4\.2\.3",
    "CMAKE_SHA256": SHA256,
    "NINJA_VERSION": r"1\.12\.1",
    "NINJA_SHA256": SHA256,
    "RUST_VERSION": SEMVER,
    "DUCKDB_VERSION": r"1\.5\.4",
    "DUCKDB_SHA256": SHA256,
    "PYTHON_VERSION": r"3\.13\.\d+",
    "PYTHON_STANDALONE_TAG": r"\d{8}",
    "PYTHON_SHA256": SHA256,
    "PANDAS_VERSION": r"3\.0\.6",
    "PYARROW_VERSION": r"25\.0\.1",
    "NUMPY_VERSION": SEMVER,
    "MATPLOTLIB_VERSION": SEMVER,
    "PY_DUCKDB_VERSION": r"1\.5\.4",
    "CLAUDE_CODE_VERSION": SEMVER,
    "SUPERPOWERS_VERSION": SEMVER,
    "SUPERPOWERS_SHA": r"[0-9a-f]{40}",
    "GH_VERSION": SEMVER,
    "GH_SHA256": SHA256,
    "AWS_CLI_VERSION": SEMVER,
    "AWS_CLI_SHA256": SHA256,
    "NEBIUS_VERSION": SEMVER,
    "NEBIUS_SHA256": SHA256,
    "MICROMAMBA_VERSION": r"\d+\.\d+\.\d+-\d+",
    "MICROMAMBA_SHA256": SHA256,
}

# (name, version, build or None) each spec must hold: the cuDF the committed device answers ran on.
IDENTITY = {
    "25.02": [
        ("libcudf", "25.02.02", "cuda12_250303_g8139f3c84f_0"),
        ("libarrow", "19.0.1", None),
        ("cuda-version", "12.2", None),
    ],
    "25.10": [
        ("libcudf", "25.10.00", "cuda12_251008_f4e35ca0"),
        ("libarrow", "21.0.0", None),
        ("cuda-version", "12.9", None),
    ],
    "26.02": [
        ("libcudf", "26.02.01", "cuda12_260205_5b9658c4"),
        ("libarrow", "21.0.0", None),
        ("cuda-version", "12.9", None),
    ],
}


def read_pins():
    pins = {}
    for line in (DOCKER / "pins.env").read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, _, value = line.partition("=")
        # docker --env-file and `source` both let a later line win, so a duplicate is a silent override.
        assert name not in pins, f"pins.env sets {name} twice"
        pins[name] = value
    return pins


def read_spec(version):
    path = DOCKER / "conda" / f"cudf-{version}.spec"
    assert path.is_file(), f"{path} is missing: conda list --explicit --md5 of the cuDF {version} environment"
    return path.read_text()


def test_pins_match_the_spec():
    pins = read_pins()
    assert set(pins) == set(PINS), f"pins.env keys: extra {set(pins) - set(PINS)}, missing {set(PINS) - set(pins)}"
    wrong = {k: v for k, v in pins.items() if not re.fullmatch(PINS[k], v)}
    assert not wrong, f"pins.env values that do not match: {wrong}"
    assert tuple(map(int, pins["RUST_VERSION"].split("."))) >= (1, 85, 0), pins["RUST_VERSION"]


def test_specs_hold_the_pinned_cudf():
    for version, wanted in IDENTITY.items():
        text = read_spec(version)
        assert "# platform: linux-64" in text.splitlines(), f"cudf-{version}.spec: no linux-64 platform line"
        held = spec_hashes.packages(text)
        for name, pkg_version, build in wanted:
            assert any(
                (p.name, p.version) == (name, pkg_version) and build in (None, p.build) for p in held
            ), f"cudf-{version}.spec holds no {name} {pkg_version} {build or ''}"


def test_specs_hash_every_package():
    for version in IDENTITY:
        packages = spec_hashes.packages(read_spec(version))
        unhashed = [p.url for p in packages if not re.fullmatch(r"[0-9a-f]{32}", p.md5)]
        assert not unhashed, f"cudf-{version}.spec: {len(unhashed)} lines without an md5, first {unhashed[:3]}"


# A build-only version: a build image for CI and no run image (build-test.md, "Images"). It has a
# spec, and no driver minimum, which only a run image reads.
BUILD_ONLY = ["25.10"]


def test_a_build_only_version_has_no_driver_minimum():
    pins = read_pins()
    for version in BUILD_ONLY:
        read_spec(version)
        assert f"DRIVER_MINIMUM_{version.replace('.', '_')}" not in pins, version
    for version in set(IDENTITY) - set(BUILD_ONLY):
        assert f"DRIVER_MINIMUM_{version.replace('.', '_')}" in pins, version


if __name__ == "__main__":
    raise SystemExit(main(globals()))
