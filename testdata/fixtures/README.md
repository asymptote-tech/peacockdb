# fixtures

Committed input files, each read by a test rather than written by a run, and nothing
regenerates them: they are samples of a convention, not goldens of a run.

`sectioned-cost.txt` is a two-section `.cost.txt`, the numbers a real q6 section's shape,
shortened. `cost-report` reads a total out of one section; the test side's parser reads the same
sections. `cost-report` has no dependencies on purpose, so the two readers share no code — this
file is what holds them to one convention, and both assert the same values off it so they diverge
red rather than silently.

`two-row-registry.csv` is loaded through `Registry::load` so a widget test starts on the far side of
the loader. Every other widget test builds its rows by hand, downstream of the seam where a column
`load` does not read renders as a plausible em-dash rather than as an error.

`row-range-clamp.txt` is the row-range clamp's cases, `offset length rows -> begin end`. The rule
is written twice, `RowRange::clamp` in Rust and `clamp_row_range` in C++, because the cpu backend
never crosses the ABI. Each side's test reads this file and asserts its own clamp, so a case added
here reaches both, and a drift fails the drifting side on the shared line.

`device-cases.csv` is a device case list (`testdata/benchmark-results/<device>/cases.csv`) with one
row of each kind: `ok` at two modes, `wrong` with its ticket, `skipped` with a reason holding a
comma, and a tpcds row. `parse_device_cases` in Rust and `device_cases.py` for scripts each read it
to the same literal, so a drift fails the drifting side.

`duckdb-results.tsv` is DuckDB's results file (`benchmark-results/<tree>/<ds>.sf<sf>/duckdb.tsv`,
`benchmarks/duckdb_minimal.sh --results`): the `# ` line's nine keys, the column line, an `ok` row
and a `failed` row. `parse_duckdb_results` in Rust reads it to a literal, and
`benchmarks/tests/test_duckdb_minimal.py` holds the script's output to its keys and columns. A
script that reads the format is tested against it too, so a drift fails the drifting side.
