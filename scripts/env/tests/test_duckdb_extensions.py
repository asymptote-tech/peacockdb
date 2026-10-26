"""testdata/generate_testdata.sh with PEACOCK_DUCKDB_EXTENSION_DIR: the DuckDB extensions are looked
up there, before INSTALL, and without it the SQL DuckDB is fed is unchanged.

A copy of the script runs against a fake duckdb that records the SQL on its stdin."""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

SCRIPT = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "generate_testdata.sh"

FAKE_DUCKDB = r'''#!/usr/bin/env bash
if [ "${1:-}" = --version ]; then echo "v1.5.4 fake"; exit 0; fi
cat >> "$FAKE_DUCKDB_SQL"
'''


def sql_for(bench, extension_dir=None):
    """The SQL generate_testdata.sh feeds duckdb for `bench` at sf1."""
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root / "testdata").mkdir()
        shutil.copy2(SCRIPT, root / "testdata" / SCRIPT.name)
        duckdb = root / "duckdb"
        duckdb.write_text(FAKE_DUCKDB)
        duckdb.chmod(0o755)
        log = root / "sql.txt"
        env = {k: v for k, v in os.environ.items() if k != "PEACOCK_DUCKDB_EXTENSION_DIR"}
        env.update(DUCKDB=str(duckdb), FAKE_DUCKDB_SQL=str(log))
        if extension_dir is not None:
            env["PEACOCK_DUCKDB_EXTENSION_DIR"] = extension_dir
        proc = subprocess.run([str(root / "testdata" / SCRIPT.name), "--bench", bench], env=env,
                              capture_output=True, text=True, timeout=60)
        assert proc.returncode == 0, proc.stderr
        return log.read_text().replace(str(root), "<root>")


def test_the_extension_directory_is_set_before_install():
    for bench in ("tpch", "tpcds"):
        lines = sql_for(bench, "/opt/duckdb-ext").splitlines()
        setting = lines.index("SET extension_directory='/opt/duckdb-ext';")
        assert setting < lines.index(f"INSTALL {bench};"), (bench, lines[:8])


def test_without_the_variable_the_sql_is_unchanged():
    for bench in ("tpch", "tpcds"):
        plain = sql_for(bench)
        assert "extension_directory" not in plain, plain[:300]
        with_dir = sql_for(bench, "/opt/duckdb-ext").replace("SET extension_directory='/opt/duckdb-ext';\n", "")
        assert with_dir == plain, bench


if __name__ == "__main__":
    raise SystemExit(main(globals()))
