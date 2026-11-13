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
    # Sirius `stable` (sirius-bench in llm-wiki/archive/archived-tasks.md): its commit, and its
    # CUDA 13 linux_amd64 extension as the release published it, mirrored in the private
    # calibration bucket.
    "SIRIUS_COMMIT": "8d17afec481c56903b61b74a9787423b470298a1",
    "SIRIUS_EXTENSION_S3": r"s3://calibration/sirius/[0-9a-f]{64}\.duckdb_extension\.gz",
    "SIRIUS_EXTENSION_SHA256": "01e4797cff315985e66ff829dde3e3bec715482ced2a6142ff1334577638ed1b",
    "SIRIUS_EXTENSION_BYTES": "921613272",
    "SIRIUS_EXTENSION_UNPACKED_SHA256": SHA256,
    # Sirius's Apache-2.0 LICENSE at SIRIUS_COMMIT, committed as docker/sirius-LICENSE.
    "SIRIUS_LICENSE_SHA256": "027e24575ca837b017b90fd60a6424126fcc886ba09acf44cfc55797e8e187b2",
    "SIRIUS_DUCKDB": r"1\.5\.6",
    "SIRIUS_LIBCUDF": r"26\.08\.01",
    "DRIVER_MINIMUM_SIRIUS": r"580\.65\.06",
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


def test_the_sirius_mirror_is_named_by_its_content():
    """The calibration bucket never replaces an object: a new extension is a new key."""
    pins = read_pins()
    assert pins["SIRIUS_EXTENSION_S3"] == (
        f"s3://calibration/sirius/{pins['SIRIUS_EXTENSION_SHA256']}.duckdb_extension.gz"), pins


LOCK_LINE = re.compile(r"(?P<name>[a-z0-9][a-z0-9._-]*)==(?P<version>[^\s=]+) --hash=sha256:[0-9a-f]{64}")


def test_the_sirius_venv_is_duckdb_and_pyarrow_at_their_pins():
    """Sirius's Python: the DuckDB its extension was built for, and pyarrow for the Arrow fetch,
    at the version the build image's venv holds. Nothing else, every line hashed."""
    lines = [line for line in (DOCKER / "sirius-venv.lock").read_text().splitlines()
             if line and not line.startswith("#")]
    bad = [line for line in lines if not LOCK_LINE.fullmatch(line)]
    assert not bad, f"sirius-venv.lock lines not `name==version --hash=sha256:<64 hex>`: {bad}"
    pins = read_pins()
    locked = {m["name"]: m["version"] for m in map(LOCK_LINE.fullmatch, lines)}
    assert locked == {"duckdb": pins["SIRIUS_DUCKDB"], "pyarrow": pins["PYARROW_VERSION"]}, locked


if __name__ == "__main__":
    raise SystemExit(main(globals()))
