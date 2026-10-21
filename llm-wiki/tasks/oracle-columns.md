# Every fingerprint is visible in the line, the registry and the report; q2 on the device

Kind: production

**This task closes [#56](../tickets/corpus-coverage.md#t56)** (q2 fingerprinted answer).
Fourteenth of chain K, after interval-days. It builds on oracle-cleanup's oracle grammar and on
round-tolerance's q2 measurement.

## Why

- **q2's device answer is right and its cells are off.** Round-tolerance measured it on an L40S:
  q2 passes `golden(no_sort, approx_std)` at all five device modes. But the device prints some
  `round(…, 2)` cells one ulp longer ([#60](../archive/archived-tickets.md#t60)), and a column is
  padded to its widest cell. So the device's rendering grows from 261,767 to about 367,481 bytes,
  over the 262,144-byte cap, and `gpu-result.txt` holds a fingerprint.
- **No spelling serves both sides of q2.** The cpu's section holds rows and the device's holds a
  fingerprint. `duckdb(…)` refuses the device's fingerprint, and `duckdb_fingerprint` refuses the
  cpu's rows. One `duckdb_oracle` per line names one comparison for both sides.
- **A silent fallback would hide the comparison.** Letting `duckdb(…)` fall back to the
  fingerprint would make q2 green, but its line would still read as a row comparison. The human
  wants what each comparison does to be readable from the `corpus_query!` line, the registry and
  `testdata/report.html`, for q2 and for the 24 lines that declare `duckdb_fingerprint`.
- **The report does not say what is stored.** Whether a section holds rows or a fingerprint
  decides how the cpu is held to its own committed answer, and no column says it. The
  fingerprint's hover text is also incomplete: it says "row count, per-column counts and sums,
  and a hash", and omits min and max, the tolerance, which columns are summed, and that row
  order is not checked.

## The work

1. **`duckdb(…)` takes an optional device part: `gpu: fingerprint`.**
   - **Spelling.** It comes last, after any `date(…)`:
     `duckdb(no_sort, exact, gpu: fingerprint)`. `fingerprint` is the only value it takes; the
     parser refuses any other by name, and refuses the part on `duckdb_fingerprint` and
     `duckdb_none`.
   - **The cpu case ignores it.** `duckdb_<ds>_<q>` compares the cpu's section under the rest of
     the spelling, as today. A fingerprinted cpu section under `duckdb(…)` is still refused with
     "declare duckdb_fingerprint".
   - **The device case honours it.** `duckdb_gpu_<ds>_<q>_<mode>` under `gpu: fingerprint`
     compares the device's section with `fingerprint::compare_over_cap` at
     `DUCKDB_FLOAT_TOLERANCE`. The row order, the values rule, `rel` and `date` do not apply on
     that side, since a fingerprint carries none of them.
   - **It is never silent.** Under `gpu: fingerprint`, a device section that holds rows fails,
     naming the part to drop. Without it, a fingerprinted device section fails with "declare
     `gpu: fingerprint`". So the line always says which comparison the device side gets.
2. **q2 turns on at all five device modes.** Its line becomes
   `duckdb(no_sort, exact, gpu: fingerprint)`, `data_fusion(sort, exact)`,
   `golden(no_sort, approx_std)`. Its five `gpu-result.txt` sections are recorded on nebius-gpu.
   Registry row 3 drops `56`. The cpu side stays `exact` against DuckDB, as it passes today.
3. **The registry gains a `stored` column**, after `tickets` and before `duckdb_oracle`.
   - **Value.** One word, `result` or `fingerprint`, when every side that has a section has that
     format. A side with no section, or a `skipped:` marker, is left out of that rule, so a query
     with its device cells off reads `result`. Otherwise three parts, `cpu|duckdb|gpu`, e.g.
     `result|result|fingerprint` for q2, with `—` for a side that has no section. A gpu part
     whose device modes disagree is `mixed`; none do today. A row that is not a corpus line (its
     oracle columns `na`) reads `na`, even where `duckdb-result.txt` has a section for it.
     (These are my calls.)
   - **Source.** `.result.txt` (the cpu's), `duckdb-result.txt` and `gpu-result.txt`, read from
     `testdata/goldens/`. A test holds the column to the three files in both directions, beside
     `the_registry_matches_the_cpu_corpus_in_both_directions`.
   - **Readers.** Every reader of the CSV accepts the new column: `registry.rs`, cost-report's
     `Registry::load`, `ticket_widget.py` and `duckdb_result.py`. `duckdb_result.py`'s `no_sort`
     reader keeps working on a spelling with `gpu: fingerprint`.
4. **The report's columns after Query:**

   | column | was | the cell |
   |---|---|---|
   | Result stored | — (new) | the registry's `stored`; split into cpu, DuckDB and gpu halves where it has three parts |
   | DuckDB | DuckDB oracle | the `duckdb_oracle` spelling, shortened as today; split into a cpu and a gpu half where the line has `gpu: fingerprint` (q2: `no_sort exact` \| `fingerprint`) |
   | cpu ↔ DataFusion | cpu oracle | unchanged |
   | gpu ↔ cpu | gpu oracle | unchanged |

   The mode groups, schema validation and the columns after them stay as they are.

   **A split cell is one `<td>` divided inside**, not a sub-column: each of the four is one
   column, and only q2's two cells divide. Each half carries its own hover text and a label
   (cpu, DuckDB, gpu) visible in the cell. Each header spans both header rows (`rowspan="2"`), as the oracle headers do today.
5. **Hover text, in two layers.**
   - **Each first-row header** says what the column's comparison does in general.
     - Result stored: what a section holds, and that the cpu's answer must equal its stored
       section text for text: the same rows in the line's row order with no tolerance, or the
       same fingerprint text.
     - DuckDB: the cpu's stored answer and each device mode's stored answer, each against
       DuckDB's; a split cell means the two sides compare differently.
     - cpu ↔ DataFusion: the cpu's answer against a live DataFusion run at one partition.
     - gpu ↔ cpu: the device's answer in memory against the cpu's stored section (`golden`) or a
       live cpu run at the same mode (`live_cpu`).
   - **Each cell** keeps its spelling's hover text from `ORACLE_HOW`. The fingerprint's text is
     rewritten to say exactly what is compared: the row count; each column's count of values;
     each float column's sum, min and max within 1e-11 relative; every other column, decimals
     included, through a SHA-256 over all rows' cells, exactly; row order not checked. The same
     text serves `duckdb_fingerprint` and `gpu: fingerprint`.
6. **The wiki says it.** `build-test.md` describes `gpu: fingerprint`, the `stored` column and
   the report's columns. #56 is archived as done, its text rewritten to what was built.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/test_support/duckdb_oracle.rs` and its `tests.rs` | the `gpu:` part: parse, refusals, the device case's comparison |
| `peacockdb-core/src/test_support/registry.rs` and its `tests.rs` | the `stored` column |
| `peacockdb-core/src/test_support/mod.rs` | `DuckdbOracle::Compare`'s flag, `FACETS`, the `stored_of` wrapper |
| `peacockdb-core/src/test_support/corpus_golden.rs` | `stored_of`, read from the three goldens |
| `peacockdb-core/tests/test_cpu_corpus.rs` | the `stored` column held to the goldens |
| `peacockdb-core/tests/common/corpus_cases.inc` | q2's line |
| `testdata/cost-registry.csv`, `testdata/report.html` | the `stored` column, q2's row |
| `testdata/goldens/tpcds.sf1/gpu-result.txt` | q2's five sections |
| `testdata/duckdb_result.py`, `testdata/test_duckdb_result.py` | the new column; `no_sort` read past `gpu:` |
| `cost-report/src/main.rs` | `Registry::load` and its test fixture |
| `scripts/ticket_widget.py`, `scripts/test_ticket_widget.py` | the columns, the split cells, the header and cell hover text |
| `llm-wiki/build-test.md`, `llm-wiki/tickets/corpus-coverage.md`, `archive/archived-tickets.md`, `tickets.md` | the grammar, the column, the report; #56 archived; counts |

Component-level API: none. No engine, wire or ABI change.

## Restriction

- No engine change.
- No fingerprint falls back silently: every fingerprinted comparison is named by its line.
- The cap, both writers and the fingerprint's format stay as they are.
- `duckdb_fingerprint`, `duckdb_none` and the other oracles keep their spellings and meaning.
- Per-node, plan and cost goldens get no report column.

## Verification

- The cpu corpus, `--lib`, `test_corpus_goldens`, cost-report and the Python tiers green.
- q2 green at all five device modes on nebius-gpu, its DuckDB cases included; the dispatched
  `GPU Tests (nebius-gpu)` CI job green.
- Negative cases:
  - a fingerprinted device section under `duckdb(…)` without `gpu:` fails, naming the part;
  - a device section holding rows under `gpu: fingerprint` fails;
  - a fingerprinted cpu section under `duckdb(…, gpu: fingerprint)` is still refused;
  - `gpu:` with a value other than `fingerprint`, or on `duckdb_fingerprint`, is refused;
  - a `stored` cell that disagrees with the files fails, naming the side.
- Widget tests: q2's two split cells, a one-word `stored` cell, and each header's hover text.
- `testdata/report.html` regenerated with `python3 scripts/ticket_widget.py` in the commit that
  changes the registry.
