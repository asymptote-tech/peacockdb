"""spec_hashes.py over a synthetic explicit spec, the anaconda.org lookup replaced by a dict."""
import io
import json
import pathlib
import sys
import tempfile
import urllib.error

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
import spec_hashes  # noqa: E402
from harness import main, raises  # noqa: E402

CF = "https://conda.anaconda.org/conda-forge"
NVCC = f"{CF}/noarch/cuda-nvcc-dev_linux-64-12.2.140-ha770c72_1.conda"
ZLIB = f"{CF}/linux-64/libzlib-1.3.1-hb9d3cd8_2.conda"
HEADER = "# platform: linux-64\n@EXPLICIT\n"
MD5 = {NVCC: "a" * 32, ZLIB: "b" * 32}


def test_every_url_line_gets_its_md5():
    assert spec_hashes.hashed(HEADER + f"{NVCC}\n{ZLIB}\n", MD5.get) == (
        HEADER + f"{NVCC}#{'a' * 32}\n{ZLIB}#{'b' * 32}\n"
    )


def test_an_agreeing_hash_is_kept():
    text = HEADER + f"{ZLIB}#{'b' * 32}\n"
    assert spec_hashes.hashed(text, MD5.get) == text


def test_a_disagreeing_hash_is_refused_by_name():
    with raises(ValueError, match="libzlib-1.3.1-hb9d3cd8_2.conda"):
        spec_hashes.hashed(HEADER + f"{ZLIB}#{'0' * 32}\n", MD5.get)


def record_opener(md5):
    def opener(url, timeout):
        return io.BytesIO(json.dumps({"basename": "linux-64/libzlib-1.3.1-hb9d3cd8_2.conda", "md5": md5}).encode())
    return opener


def not_found(url, timeout):
    raise urllib.error.HTTPError(url, 404, "Not Found", None, None)


def run_main(text, opener):
    """main over a spec holding `text`: (the one-line exit message or None, the spec afterwards)."""
    with tempfile.TemporaryDirectory() as tmp:
        spec = pathlib.Path(tmp) / "cudf.spec"
        spec.write_text(text)
        try:
            spec_hashes.main([str(spec)], opener)
            message = None
        except SystemExit as exit_:
            message = str(exit_.code)
        return message, spec.read_text()


def test_a_missing_record_fails_in_one_line_naming_the_package():
    text = HEADER + ZLIB + "\n"
    message, after = run_main(text, not_found)
    assert message.startswith("spec_hashes.py: ") and "\n" not in message, message
    assert ZLIB in message and "404" in message, message
    assert after == text


def test_main_writes_nothing_when_a_hash_disagrees():
    text = HEADER + f"{NVCC}\n{ZLIB}#{'0' * 32}\n"
    message, after = run_main(text, record_opener("b" * 32))
    assert message is not None and "libzlib" in message, message
    assert after == text


def test_main_fills_a_line_without_a_hash():
    message, after = run_main(HEADER + ZLIB + "\n", record_opener("b" * 32))
    assert (message, after) == (None, HEADER + f"{ZLIB}#{'b' * 32}\n")


def test_api_url_splits_a_dashed_name():
    assert spec_hashes.api_url(NVCC) == (
        "https://api.anaconda.org/dist/conda-forge/cuda-nvcc-dev_linux-64/12.2.140/noarch/"
        "cuda-nvcc-dev_linux-64-12.2.140-ha770c72_1.conda"
    )


def test_packages_read_name_version_build_and_md5():
    (package,) = spec_hashes.packages(HEADER + f"{NVCC}#{'a' * 32}\n")
    assert (package.name, package.version, package.build, package.md5) == (
        "cuda-nvcc-dev_linux-64", "12.2.140", "ha770c72_1", "a" * 32
    )


def test_a_url_off_anaconda_is_refused_by_name():
    url = "https://example.org/linux-64/foo-1.0-0.conda"
    with raises(ValueError, match="example.org/linux-64/foo-1.0-0.conda"):
        spec_hashes.hashed(HEADER + url + "\n", MD5.get)


def test_a_text_without_explicit_is_refused():
    with raises(ValueError, match="@EXPLICIT"):
        spec_hashes.packages(f"{ZLIB}\n")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
