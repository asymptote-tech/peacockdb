
# Wall-time benchmarks

<a id="t226"></a>
### #226 — a benchmark tree does not say which device, driver, CUDA or cuDF produced it
The `--- run ---` trailer and the record's `# run:` heading carry `build=`, `allocator=` and
`capture=`, and nothing about the hardware or the stack. Two hosts now write the same files:
shad-gpu (cuDF 25.02, driver-side CUDA 12.5 compat) through `build-test-shadgpu.sh` and
verda-gpu (cuDF 26.02, driver 580) through `build-test.sh`, both H200s today — and
`--pull-benchmarks` from either overwrites `benchmark-results/tpch.sf40/*.benchmark.txt` and
`calibration/records.tsv` in place. A `git diff` shows numbers moving and cannot say whether
the code, the cuDF version or the card moved them; a plot drawn from a mixed record says
nothing either.

Add to both the trailer and the heading, as constants of a run: `device=` (the
`cudaDeviceProp` name), `driver=` (`cudaDriverGetVersion`), `cuda=` (`cudaRuntimeGetVersion`,
the toolkit libcudf was built with), `cudf=` (`CUDF_VERSION_MAJOR.MINOR.PATCH` from
`cudf/version_config.hpp`), and `host=` (the machine name). The C++ side knows all four
numbers and the Rust harness knows none, so this is one ABI query returning a struct of
strings, priced like the `allocator=` line: `install_rmm_pool` already reports what it found,
and this is the same shape one call earlier. The record's heading check — an append under a
different heading is refused — then does what it should: a 26.02 row cannot land under a
25.02 heading. `nsys_hbm.py` joins the capture onto the record's coordinates and should refuse
a capture whose `TARGET_INFO_GPU` device name differs from the record's.

<a id="t69"></a>
### #69 — DuckDB cost oracle: multi-threaded golden generation for larger SF
`gen_duckdb_cost.sh` pins `PRAGMA threads=1` because `operator_rows_scanned` scales with
thread count (`output_bytes`/`output_rows` are thread-invariant). Fine at sf1, too slow
at sf10/sf100. Simplest fix: parallelize across queries, keep per-query threads=1; or
drop/normalize the thread-sensitive field.
