# Every corpus answer is checked against DuckDB

Kind: production

**This task closes [#235](../archive/archived-tickets.md#t235)** (no independent oracle checks the
result goldens), whole. First of the join-rewrite chain, so
every later task's newly enabled cell meets DuckDB inside that task's own verification.

With it, #235's two harness items (steps 7 and 8): the corpus helpers proven to fail on a wrong
answer, and every oracle enum held to the lines that use it.

## What exists

`testdata/duckdb_result.py` (DuckDB 1.5.4 pinned, threads=1, DataFusion's NULL order and integer
division) and both `testdata/goldens/{tpch,tpcds}.sf1/duckdb-result.txt`, landed 2026-09-28: tpch 39
sections (5 `skipped:` over the cap), tpcds 99 (1 `skipped:`), none `failed:`. Nothing reads
them. `corpus_query!` takes 8 arguments (`tests/test_cpu_corpus.rs:22`, `tests/test_gpu_corpus.rs:16`).
The first run's differences are #235's: names (by position, not a divergence), decimal `avg` and
division digits, float last digits, one empty answer (tpcds q17, #205), 18 tpcds queries DuckDB
alone answers, and the over-cap sections.

## The work

1. **The ninth argument.** `corpus_query!` gains `duckdb_oracle` as its first oracle argument,
   before `cpu_oracle`:
   `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle, gpu_oracle, schema_validation)`.
   It is written on every line — no default, no line without it, so a query's whole coverage still
   reads off its line. Values:
   - `duckdb_exact` — rows as multisets by column position, numbers equal as rendered;
   - `duckdb_approx` — a decimal cell equal to one unit in the last place our rendering carries
     (`|ours − duck| ≤ 10^−s`, `s` our cell's digits after the point), a float cell within a
     relative 1e-11: for #235's decimal-truncation rows (DataFusion's fixed-scale `avg` and division
     differ from DuckDB's double by up to 1.2e-5 relative on tpch q1, 1e-6 on tpcds q58 — measured,
     far past any float tolerance) and its float-reassociation rows;
   - `duckdb_divergent(<ticket>, <positions>)` — one variant taking a ticket number and the column
     positions that diverge: the ticket must be open in `llm-wiki/tickets/` (a closed or unknown
     number fails); the row count and every column not named are checked as `duckdb_approx` checks
     them, and the named columns must still differ (one that stopped diverging fails, so the line
     is updated). Empty positions mean a row-level divergence: the row count alone is checked. A
     line that diverges in one column is not a line that checks nothing;
   - `duckdb_fingerprint` — the section is at or over the 256 KB cap on both sides, so both hold its
     fingerprint (step 3) and the comparison reads that; numeric columns compared within
     `duckdb_approx`'s tolerance, since a sum of floats reassociates;
   - `duckdb_none` — DuckDB or we do not answer (a refused or not-enabled query, a DuckDB
     `failed:`), so there is nothing to compare.
   `DuckdbOracle::ALL` lists the five, and a test asserts each is named by some line.
   **A sixth, only if needed:** `duckdb_columns(<positions>)` — compare only the listed column
   positions, as a multiset, for a LIMIT window whose cutoff ties, where both engines keep valid
   but different rows. It is added only if the first run (step 6) finds a line that needs it,
   confirmed by hand and named in the PR; if no line needs it, it is not added at all. The oracle is
   explicit both ways: a fingerprinted section under any oracle but `duckdb_fingerprint` fails, and
   so does `duckdb_fingerprint` over a section that is not fingerprinted.
   **Mode sugar:** `all_modes` stands for `tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup |
   tp4_sized` in `cpu_modes` and `gpu_modes`; both macros expand it to the five, and every line
   that lists all five today is rewritten to it — and every reader that parses a line's modes
   (`test_corpus_goldens/benchmark.rs::modes()`, `:356-362`, among them) expands it. Both corpus
   binaries carry the argument through `declare_corpus_query!`.
2. **The comparison**, in `test_cpu_corpus` (rust-only, no device): one case per line,
   `duckdb_<dataset>_<query>`, reading the query's section from `duckdb-result.txt` and from
   `mini.result.txt` (the authority's section, the last enabled mode's, as the widget reads it).
   Row count first, then rows by position. A section absent on either side under any oracle but
   `duckdb_none` fails, as does `duckdb_none` over two sections that both exist.
3. **The over-cap fingerprint** (decided: option (c)). A section at or above the 262144-byte cap
   is written by both writers — `duckdb_result.py` and the engine's result-golden writer — as a
   fingerprint instead of `skipped:`:
   ```
   fingerprint: rows=<n>
   col <i>: nonnull=<n> [sum=<x> min=<x> max=<x>]      -- the triple where the column is approximate
   hash: <sha256 of the sorted rendered rows over the exact columns>
   ```
   A column is **exact** when it renders identically on both sides — integers, strings, dates,
   booleans, timestamps — and goes into the hash; only a float column, or a decimal whose scale
   differs between the sides (a decimal on ours, a double on DuckDB's), is **approximate** and
   compared by its triple. An all-integer over-cap join is then checked row for row, not by sums
   alone. Under `duckdb_fingerprint` the comparison checks `rows`, each column's `nonnull`, the
   triples within `duckdb_approx`'s tolerance, and the hash exactly. A mismatch
   names the column. Five sections take it today: tpch q16, anti-join, filter-project,
   semi-join; tpcds q98. tpch q11 joins them when join-backend turns its cpu cells on (#190).
4. **`gpu-result.txt`.** The device's corpus run writes its answers beside `mini.result.txt`
   under a regeneration variable on shad-gpu — each section written **before** the device asserts
   against the cpu, so an answer that differs from the cpu is recorded, the case DuckDB settles — the same sections, rendering and fingerprint,
   pulled home with the benchmark tree. A record, never an authority: the device still asserts
   against the cpu's `mini.result.txt`, and `test_gpu_corpus`'s check that a device run writes no
   cpu golden stays. **Keyed by query and mode**: one section per enabled device cell,
   `== <query> mode=<mode>`, so every device cell meets DuckDB, not only the last mode (a mode-
   dependent device answer — #243's lane split, a shuffle defect — shows there). The comparison of
   step 2 runs over every section (`duckdb_gpu_<dataset>_<query>_<mode>`). Two guards:
   - **Coverage, both ways, rust-only in CI:** every enabled device cell has its section, and every
     section is an enabled cell's. A task that turns a device cell on or off without regenerating
     fails. Values are not compared against an earlier run: GPU float reductions are not
     reproducible run to run, nor across cuDF versions.
   - **One version per file:** the committed file is cuDF 25.02's (shad-gpu's). A run on another
     version (verify-26.02) writes `gpu-result-<version>.txt` beside it, gitignored, compared with
     DuckDB the same way and never committed over the 25.02 file.
   **When to regenerate**, written into `build-test.md` by this task: run a cycle with
   `PCK_WRITE_GPU_RESULT=1` and `--pull-results`, and inspect the diff of `gpu-result.txt`, every
   time a change might move a device answer — a device code change, a cell turned on, a cuDF
   update — and in every task that turns device cells on. A moved section is read before it is
   committed, and a moved answer the change did not intend is a finding.
5. **One rendering.** `duckdb_result.py` renders a timestamp as arrow-rs does — `NaiveDateTime`'s
   form, no trailing zeros (`…00.001`, not `isoformat()`'s `…00.001000`; `arrow-cast`
   `display.rs:495`) — so a millisecond timestamp is not a false divergence; a comparator case
   pins it.
6. **Every line's oracle**, from the comparison's first run: `duckdb_exact` where it passes,
   `duckdb_approx` for #235's digit rows, `duckdb_fingerprint` for the five over-cap sections, `duckdb_none` for the 18 DuckDB-only and the 4
   not-enabled queries, `duckdb_divergent(<ticket>, <positions>)` for a real divergence — a new
   ticket where none exists. A line left at `duckdb_none` moves to its variant in the task that
   first turns its cells on (join-backend for tpch q11 and q22, tpcds q24 and q54). #235's "decimal avg truncates at a fixed scale" is filed as a ticket of its own if
   any row needs more than `duckdb_approx`'s tolerance.

7. **The helpers fail on a wrong answer.** `assert_result_section` (`test_support/corpus.rs:430`)
   and `assert_result` (`test_support/corpus_gpu.rs:97`) read their golden from the fixed testdata
   path, so no test can show them failing. Each takes the golden section as an argument (the
   corpus cases pass the file's section, as today); negative tests hand them a doctored section —
   a wrong row, a missing row, a digit past the tolerance — and assert the failure, for the cpu
   helper under each `CpuOracle` and the device helper under `golden_exact` and `golden_approx_std`.
   Today only the string comparator is tested (`tests/test_golden_format.rs`).
   The device helper's comparison is pure Rust, so it moves out of the device-gated `corpus_gpu`
   module into an ungated function `assert_result` calls; its negative tests then run in the
   rust-only tier, with `GpuResultMode` beside it.
8. **Every oracle enum is held to its lines.** `CpuOracle::ALL` and `GpuResultMode::ALL`, each with
   the test `DuckdbOracle::ALL` has (step 1): every variant is named by some `corpus_query!` line.
   The variants no line names go first, with their keyword arms: `GpuResultMode::Skip` (`skip`) and
   `GpuResultMode::GoldenApprox` (`golden_approx`) — 113 lines say `golden_exact`, 2
   `golden_approx_std`, 5 `live_cpu`.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs` | the ninth argument; the comparison cases |
| `peacockdb-core/tests/common/corpus_cases.inc` | every line's `duckdb_oracle` |
| `peacockdb-core/src/test_support/` | `DuckdbOracle` and `ALL`; the comparator; the fingerprint writer and reader; `gpu-result.txt`'s writer |
| `testdata/duckdb_result.py` | the fingerprint for over-cap sections |
| `testdata/goldens/{tpch,tpcds}.sf1/` | `duckdb-result.txt` and `mini.result.txt` regenerated with fingerprints; `gpu-result.txt` |
| `scripts/build-test-shadgpu.sh` | the regeneration variable, the pull home |
| `.github/workflows/pipeline.yml` | the Python test of `duckdb_result.py`'s rendering in CI |
| `testdata/test_duckdb_result.py` (new) | timestamps rendered as arrow-rs does |
| `peacockdb-core/tests/test_golden_format.rs` | the fingerprint's round trip |
| `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` | `modes()` expands `all_modes` |
| `testdata/.gitignore` | `gpu-result-*.txt` (a non-25.02 run's file) |
| `llm-wiki/build-test.md`, `tickets/corpus-coverage.md` | the new tier and counts; when to regenerate `gpu-result.txt`; #235 archived |

Component-level API: none outside the test harness. No engine change.

## Restriction

The oracle only. No fix to any divergence it finds; each is a ticket and a `duckdb_divergent`
line. No change to what the cpu or device tiers assert (step 7 changes only where their helpers
read the golden section from).

## Verification bar

- rust-only: `test_cpu_corpus` with every `duckdb_*` case green, the `ALL` test, the registry
  tests; `test_golden_format` with the fingerprint's round trip; one doctored section per oracle
  variant, failing as it should (a wrong row, a closed ticket under `duckdb_divergent`, a
  fingerprint off in one column, a fingerprinted section declared `duckdb_exact`); `all_modes`
  expanding to the same cases as the five spelled out.
- device: one shad-gpu cycle writing `gpu-result.txt`; its comparison green, or each failing
  section a ticket.

## Device workflow

`build-test-shadgpu.sh`, one cycle with the regeneration variable set.

## Completeness signoff (2026-10-08, revised after the device cycle)

Solved under its constraints, on both engines. Every corpus line names a `duckdb_oracle`, a case
per line compares our answer with DuckDB's, four over-cap sections agree by fingerprint across the
two writers, and both corpus helpers are shown failing. The device half ran last, on nebius-gpu
under the host override: 26 sections at cuDF 25.02, every comparison green and none declining.

No bandaid, no assertion weakened. One shortcut: making the wiki a test input left CI able to pass
a tickets-only commit that reddens the rust tier, and the fix is a cost decision the human owns, so
it is [#263](../tickets/testinfra.md#t263) with a by-hand interim. Three shortfalls stand as named:
a decimal column against DuckDB's double cannot be approximate on both sides (#253, with the two
other paths that cannot record a divergence); the negative tests miss `data_fusion_subset` (#254);
`duckdb_columns` was looked for and not needed, no `duckdb_divergent` line being a LIMIT tie.
