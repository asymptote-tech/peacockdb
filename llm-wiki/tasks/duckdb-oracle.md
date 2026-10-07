# Every corpus answer is checked against DuckDB

Kind: production

**This task closes [#235](../tickets/corpus-coverage.md#t235)** (no independent oracle checks the
result goldens), in the parts that make DuckDB that oracle. First of the join-rewrite chain, so
every later task's newly enabled cell meets DuckDB inside that task's own verification.

Left out of #235, on purpose: the negative tests of the cpu and device helpers
(`assert_result_section`, `assert_result`), and the `ALL` consts on `CpuOracle` and
`GpuResultMode`. #235 stays open for those, reworded to them alone.

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
   - `duckdb_approx` — numbers within the relative tolerance the device's `golden_approx_std`
     uses, for the decimal-truncation and float-reassociation rows #235 lists;
   - `duckdb_divergent(<ticket>)` — one variant taking a ticket number: the comparison must
     fail, and the ticket must be open in `llm-wiki/tickets/` (a closed or unknown number fails);
   - `duckdb_columns(<positions>)` — compare only the listed column positions, as a multiset:
     for a LIMIT window whose cutoff ties, where both engines keep valid but different rows. Used
     only after the tie is confirmed by hand; the PR says so;
   - `duckdb_fingerprint` — the section is at or over the 256 KB cap on both sides, so both hold its
     fingerprint (step 3) and the comparison reads that; numeric columns compared within
     `duckdb_approx`'s tolerance, since a sum of floats reassociates;
   - `duckdb_none` — DuckDB or we do not answer (a refused or not-enabled query, a DuckDB
     `failed:`), so there is nothing to compare.
   `DuckdbOracle::ALL` lists the six, and a test asserts each is named by some line. The oracle is
   explicit both ways: a fingerprinted section under any oracle but `duckdb_fingerprint` fails, and
   so does `duckdb_fingerprint` over a section that is not fingerprinted.
   **Mode sugar:** `all_modes` stands for `tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup |
   tp4_sized` in `cpu_modes` and `gpu_modes`; both macros expand it to the five, and every line
   that lists all five today is rewritten to it. Both corpus
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
   col <i>: nonnull=<n> [sum=<x> min=<x> max=<x>]      -- the numeric triple where the column is numeric
   hash: <sha256 of the sorted rendered rows over the non-numeric columns>
   ```
   A column is numeric when its values parse as numbers on either side, so a decimal on ours and a
   double on DuckDB's compare as numbers. Under `duckdb_fingerprint` the comparison checks `rows`,
   each column's `nonnull`, the numeric triples within tolerance, and the hash exactly. A mismatch
   names the column. Six sections take it today: tpch q11, q16, anti-join,
   filter-project, semi-join; tpcds q98.
4. **`gpu-result.txt`.** The device's corpus run writes its answers beside `mini.result.txt`
   under a regeneration variable on shad-gpu, the same sections, rendering and fingerprint,
   pulled home with the benchmark tree. A record, never an authority: the device still asserts
   against the cpu's `mini.result.txt`, and `test_gpu_corpus`'s check that a device run writes no
   cpu golden stays. The comparison of step 2 runs over it too (`duckdb_gpu_<dataset>_<query>`),
   for every section it holds.
5. **Every line's oracle**, from the comparison's first run: `duckdb_exact` where it passes,
   `duckdb_approx` for #235's digit rows, `duckdb_fingerprint` for the six over-cap sections, `duckdb_none` for the 18 DuckDB-only and the 4
   not-enabled queries, `duckdb_divergent(<ticket>)` for a real divergence — a new ticket where
   none exists. #235's "decimal avg truncates at a fixed scale" is filed as a ticket of its own if
   any row needs more than `duckdb_approx`'s tolerance.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs` | the ninth argument; the comparison cases |
| `peacockdb-core/tests/common/corpus_cases.inc` | every line's `duckdb_oracle` |
| `peacockdb-core/src/test_support/` | `DuckdbOracle` and `ALL`; the comparator; the fingerprint writer and reader; `gpu-result.txt`'s writer |
| `testdata/duckdb_result.py` | the fingerprint for over-cap sections |
| `testdata/goldens/{tpch,tpcds}.sf1/` | `duckdb-result.txt` and `mini.result.txt` regenerated with fingerprints; `gpu-result.txt` |
| `scripts/build-test-shadgpu.sh` | the regeneration variable, the pull home |
| `llm-wiki/build-test.md`, `tickets/corpus-coverage.md` | the new tier and counts; #235 reworded to what is left |

Component-level API: none outside the test harness. No engine change.

## Restriction

The oracle only. No fix to any divergence it finds; each is a ticket and a `duckdb_divergent`
line. No change to how the cpu or device tiers assert.

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
