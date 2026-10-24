# Archived task specs

Specs for tasks whose PR has merged, newest first, and for tasks dropped with their approach
rejected, marked as such at the top of the entry. Each is the contract the work was done
against, kept verbatim -- including the amendments and corrections made mid-task, since those
are the part a later reader cannot reconstruct from the diff.

The batch-partitioned rollout's task list is the first entry below, archived whole on
2026-09-08: T0 through T19 and T21 are done, T20 is now [#195](../tickets.md#t195) and T22 is
obsolete. The design it was the plan for was folded into
[`architecture.md`](../architecture.md) the same day, which is where the shapes these tasks
built are described; this list is what a number in a commit message resolves to.

The entry after it merged 2026-08-20 as PR #126, opened against ENS-bp-plan-skeleton and
retargeted to master when that base merged.



---

<!-- archived from llm-wiki/tasks/desc-null-order.md -->

**Merged 2026-10-23 as PR #191, chain K's task, one commit `3710a704`, merged into master by `09cb4bb6`.**

# A descending sort key puts its nulls where the plan says on the device

Kind: production

**This task closes [#202](../tickets/corpus-coverage.md#t202)** (desc sort null order) **and
[#258](../tickets/testinfra.md#t258)** (PR comment headroom). Fifteenth of chain K, after oracle-columns. Its corpus lines use oracle-cleanup's `no_sort`
grammar, which is what found the defect.

## Why

- **The device gets a descending key's nulls backwards.** `i32 DESC NULLS LAST` comes back with
  its nulls first, and `DESC NULLS FIRST` with its nulls last. Ascending keys are right.
- **The cause is one mapping, at two sites.** `cpp/src/operators/sort.cpp` and the merge in
  `cpp/src/node_session.cpp` map `nulls_first` to `cudf::null_order::BEFORE` and its absence to
  `AFTER`. cuDF applies `null_order` before it flips a `DESCENDING` key, so on a descending key
  `BEFORE` puts nulls last.
- **Two corpus queries are gated on it.** tpcds q34 (`c_preferred_cust_flag DESC NULLS FIRST`)
  and q71 (`ext_price DESC NULLS FIRST`) both carry nulls in that key. Their five device cells are
  off under #202. The human expects both to turn on.
- **The PR comment has no room for six more rows.** `the_pr_comment_fits_under_the_body_cap`
  holds cost-report's PR comment under GitHub's 65,536-byte body cap. At chain K's tip it is about
  64.8 KB; six pbench rows take it to about 66.6 KB (the analyst's estimate, 2026-10-21). #258
  offered two remedies and left the choice to the human, who chose both.

## The work

1. **One mapping, relative to the direction.** `BEFORE` when `nulls_first == asc`, else `AFTER`.
   - Both sites call one function, so they cannot drift apart (my call; its name and header are
     the builder's).
   - **The two sites change in one commit.** Runs sorted one way under a merge that reads them
     the other way break cuDF's merge precondition, and the device answers duplicated and
     dropped rows. Today no plan reaches that shape, because both sites share the wrong mapping.
   - A comment at the function says why the mapping depends on the direction.
2. **The five pins become agreement tests.** Each loses its `bug_` prefix and its name says what
   now holds. Each asserts the device matches the cpu:
   - `bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device` and
     `bug_a_descending_key_with_nulls_first_puts_them_last_on_the_device`
     (`gpu_tests/exec_cases.rs`, `GpuSort`)
   - `bug_an_accumulating_sort_on_a_descending_key_nulls_first_puts_them_last_on_the_device`
   - `bug_a_merge_on_a_descending_key_nulls_first_puts_them_last_on_the_device`
   - `bug_a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_duplicates_and_drops_rows_on_the_device`

   The last three are in `gpu_tests/accumulate_cases.rs`. The `nulls_flipped` helper goes if
   nothing else uses it.
3. **Two new cases fill the gaps.** `GpuSort` already covers `DESC NULLS FIRST` and
   `DESC NULLS LAST` through its two pins. The accumulating sort and the merge cover only
   `DESC NULLS FIRST` on a nullable key: their `…_on_a_descending_key_agrees` tests sort on
   `id`, which has no nulls. Each gets a `DESC NULLS LAST` case on a nullable key.
4. **q34 and q71 turn on at all five device modes.** Their lines take `all_modes` for the
   device and drop the `#202` comment. Registry rows 35 and 72 drop `202`. Their
   `gpu-result.txt` sections are recorded on nebius-gpu. The oracles stay as they are:
   `duckdb(no_sort, exact)` and `golden(no_sort, exact)`.
5. **Six pbench queries sort on a nullable key.** No pbench query sorts today. Each reads
   `fact` filtered to `f_id % 20 = 0`: 1,000 rows across all ten row groups, so the tp4 and
   rowgroup modes reach the merge, and about 5% of `f_k` is null. Each breaks ties on `f_id`, so
   `no_sort` compares a fully determined order. Each answer is about 15 KB, far under the
   256 KiB cap, since a fingerprint cannot check order.

   | query | `ORDER BY` | what it catches |
   |---|---|---|
   | `sort-desc-nulls-first` | `f_k DESC NULLS FIRST, f_id` | q34's and q71's shape |
   | `sort-desc-nulls-last` | `f_k DESC NULLS LAST, f_id` | the other half of the defect |
   | `sort-desc-default` | `f_k DESC, f_id` | DataFusion's default for `DESC`, nulls first |
   | `sort-desc-string` | `f_s DESC NULLS LAST, f_id` | a string key |
   | `sort-mixed-keys` | `f_kb NULLS LAST, f_k DESC NULLS FIRST, f_id DESC` | mixed directions, nulls in two keys |
   | `sort-desc-limit` | `f_k DESC NULLS FIRST, f_id LIMIT 100` | the sort with a fetch |

   - Each selects its sort keys and `f_id`, nothing else.
   - **Oracles:** `duckdb(no_sort, exact)`, `data_fusion(sort, exact)`, `golden(no_sort, exact)`,
     at all five modes on both engines.
   - **`sort-desc-limit`'s cut must fall past the nulls.** The builder counts the filtered nulls
     with DuckDB and records it in the detail file. If 100 does not exceed it, the limit rises
     to the count plus 50.
   - **Run on the device before the fix.** The detail file records which of the six fail there
     and how. A query that passes before the fix stays in, as coverage, and the detail file
     says so.
   - Float keys are out: NaN ordering is a separate question from #202.
6. **The PR comment makes room, and the pages stay whole** (#258). In cost-report's
   `render_markdown` only:
   - **No Features column.** Its header and its cells go from the comment's tables.
   - **pbench collapses to its summary line:** "pbench — X/Y GPU-operational", pointing at the
     full report, with no table. TPC-H and TPC-DS keep their tables.
   - **Both pages are unchanged and complete:** `testdata/report.html` (`ticket_widget.py`) and
     cost-report's html page keep the Features column and every pbench row.
   - This lands before the six pbench rows, so the size test never goes red.
7. **#202 and #258 are archived as done**, each rewritten to what was built.

## Scope

| path | change |
|---|---|
| `cpp/src/operators/sort.cpp`, `cpp/src/node_session.cpp`, and the header the shared mapping lives in | the mapping |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`, `accumulate_cases.rs` | the five pins renamed and asserting agreement; the two new `DESC NULLS LAST` cases |
| `peacockdb-core/tests/common/corpus_cases.inc` | q34 and q71 on the device; the six pbench lines |
| `testdata/pbench-queries/sort-*.sql`, `testdata/goldens/pbench.sf1/*` | the six queries and every golden they add: plans, cost, cpu, result, DuckDB, gpu |
| `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q34 and q71; the six pbench rows |
| `cost-report/src/main.rs` | `render_markdown`: no Features column, pbench as its summary line; the tests |
| `llm-wiki/tickets/corpus-coverage.md`, `tickets/testinfra.md`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #202 and #258 archived; test names and counts; what the comment holds |
| `llm-wiki/architecture.md` | "## cuDF options": the `cudf::null_order` row describes the shared mapping; its #202 link goes |

Component-level API: none. No wire, ABI or facade change.

## Restriction

- Only the null-order mapping changes in the engine.
- Only the PR comment shrinks. Neither html page loses a column or a row.
- If q34 or q71 is still red for another reason, that query's cells stay off under a new ticket
  with the measurement, and #202 still closes on the operator tests.

## Verification

- `gpu_tests::` green on nebius-gpu, the five renamed tests and the new cases included.
- q34, q71 and the six pbench queries green at all five device modes, their DuckDB device cases
  included; the six green at all five cpu modes.
- The dispatched `GPU Tests (nebius-gpu)` CI job green.
- The cpu tiers green; the cost-report job shows no regression.
- cost-report tests: the comment has no Features header or cell; pbench's part of the comment
  has its summary line and no query row; cost-report's html page still has the Features header
  and a pbench row; `the_pr_comment_fits_under_the_body_cap` green with the six rows in, its
  printed margin in the detail file.

## Completeness signoff (2026-10-22)

Solved under its constraints. `null_order_of` maps a key's nulls relative to its direction, and
`GpuSort` and the merge both call it, changed in one commit; it is the only engine change. The
five pins assert device–cpu agreement and two `DESC NULLS LAST` cases fill the gap. q34, q71 and
six pbench sorts pass at all five device modes; before the fix all 40 cells failed. The PR comment
drops Features and collapses pbench; `report.html` and cost-report's page stay whole, guarded.
Completeness reviewer and analyst: none. Shortcuts or bandaids: none. One step outside the Scope
table: `ticket_widget.py` reads a new `--widget-md` output, since it built the page from the
comment.

---

<!-- archived from llm-wiki/tasks/oracle-columns.md -->

**Merged 2026-10-23 as PR #190, chain K's task, one commit `97db3127`, merged into master by `ffb15ec7`.**

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

## Completeness signoff (2026-10-21)

Solved under its constraints. `duckdb(…, gpu: fingerprint)` parses last, is refused on the bare
oracles and with any other value, and the device side fails in both directions, so no
fingerprint falls back silently. q2 is on at all five device modes, recorded on nebius-gpu. The
registry's `stored` column is held to the three goldens both ways, including a stale
`gpu: fingerprint` line; every CSV reader takes it; the report shows the four columns with
q2's split cells. No engine change. Completeness reviewer: none; analyst: archived #56's original
shape restored, two build-test.md sentences corrected by the coordinator. Shortcuts or bandaids:
none.

---

<!-- archived from llm-wiki/tasks/interval-days.md -->

**Merged 2026-10-23 as PR #189, chain K's task, one commit `98b6b0be`, merged into master by `c21d223e`.**

# Date and timestamp arithmetic with an interval literal runs on the device

Kind: production

**This task closes [#168](../tickets/corpus-coverage.md#t168)** (interval literal on wire). Thirteenth
of chain K, after oracle-cleanup, whose oracle grammar its corpus lines use.

## Why

The wire's `ScalarValue` (`flatbuffers/gpu_plan.fbs`) has no interval type. A plan that adds an
interval literal to a column cannot cross to the device: `serialize_scalar_value`
(`wire/serialize.rs`) refuses it. Constant folding removes every other corpus interval. tpch
mixed-join's `o_orderdate + INTERVAL '90' DAY` does not fold, so its device cells are off at all
five modes. Only whole days are needed for mixed-join. This task carries the whole literal anyway,
because a month is an ordinary SQL interval and the device can do month arithmetic.

## The work

1. **An interval literal crosses the wire as three parts.** `DataType` gains
   `IntervalMonthDayNano`, appended last so no existing value's number moves. `ScalarValue` gains
   `interval_months: int32`, `interval_days: int32` and `interval_nanos: int64`.
   `IntervalYearMonth` and `IntervalDayTime` literals are written as the same type, converted to
   months, days and nanoseconds. A typed NULL interval crosses with `is_null`.
2. **The device answers `date or timestamp ± interval literal`** exactly as DataFusion does
   (arrow `add_month_day_nano`):
   - The order is months first, then days, then nanoseconds.
   - Months use `cudf::datetime::add_calendrical_months`. A day past the month's end clamps to
     its last day, so Jan 31 + 1 month is Feb 28, or Feb 29 in a leap year.
   - Days and nanoseconds are added as cuDF durations.
   - On a date, the sub-day part counts in whole days, truncated toward zero, as chrono does, so
     `date + 12 hours` is the same date.
   - On a timestamp, nanoseconds convert to the column's unit as arrow converts them.
   - Subtraction negates all three parts.
   - `interval + date` answers a date: the timestamp side gives the type, never the left
     operand.
3. **Where it runs.** An interval operand never takes the AST path. `fb_to_type_id` maps the
   new type to `EMPTY`, since cuDF has no interval type, and an `EMPTY` operand already sends a
   binary operation to the column path. `build_column_binary` gains the arm. A join residual
   reaches the same arm through `build_column`, and so does a hoisted keyless residual, which
   calls it on a zero-row table to learn the type, so the arm answers at zero rows.
   `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` gains a row that refuses the
   interval as an AST literal, by name.
4. **The refusals go.** `NOT_RUNNABLE` (`planner/tests/plan_goldens.rs`) becomes empty, and
   the uncrossable set with it. mixed-join's plan golden line becomes a plan.
   `a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where` (`wire/tests.rs`) takes a
   payload the wire still cannot carry, and names it.
5. **pbench gains queries over `fact`'s date and timestamps** (`f_dt`, `f_ts_ms`, `f_ts_us`,
   `f_ts_ns`). Between them they must reach:
   - months, with the month-end clamp and a leap day;
   - days, and a negative interval;
   - a mixed interval (`'1 month 2 days 3 hours'`) on a date and on a timestamp;
   - subtraction;
   - an interval in a filter's predicate, as an aggregate's argument, and in a join residual,
     keyed and keyless;
   - the months-then-days order, through dates where the two orders disagree (Jan 29 and 30).

   No interval expression is a group key, since the device refuses a computed group key
   (#264).

   **The SQL is the plain query.** DuckDB answers `date ± interval` as a `TIMESTAMP` (measured
   with 1.5.4), where DataFusion answers a date. That is declared on the corpus line with
   oracle-cleanup's `date("<column>")`, which compares DuckDB's cell by its date part. No query
   carries a cast for DuckDB's sake.

   **Timestamps are measured first.** DuckDB may answer `TIMESTAMP_MS` or `TIMESTAMP_NS ±
   interval` as a microsecond `TIMESTAMP`, where DataFusion keeps the column's unit. If the
   values agree and only the printed form differs, this task adds `timestamp("<column>")` to the
   grammar, next to `date`: DuckDB's cell compares as the same instant. The measurement goes in
   the detail file either way.

   **What stays out of pbench.** DuckDB floors a negative sub-day part on a date
   (`date '2024-03-10' − 12 hours` is 2024-03-09), where DataFusion truncates toward zero. That
   case is tested in the gtests only, so the DuckDB comparison is not asked about a difference
   of convention. The seconds unit is gtest-only too: parquet has no seconds unit, so `f_ts_s`
   reads as microseconds in DataFusion.

   **Where the edge cases come from.** `f_dt` spans 2024, so the month-end and leap-day cases
   come from it. The `f_ts_*` values lie in the first minutes of 2024-01-01. Changing `gen.sql`
   moves every pbench golden and is out of scope. Each query is a corpus line with cpu and
   device cells at all five modes, a registry row, and a `duckdb-result.txt` section.

## Corpus

`tpch/mixed-join`: its five device cells turn on. `168` leaves its row. The new pbench queries
land on, on both engines. All of them record `gpu-result.txt` sections at every mode.

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `DataType::IntervalMonthDayNano`; three `ScalarValue` fields |
| `peacockdb-core/src/wire/serialize.rs`, `wire/expr_writer.rs`, `wire/fb_text.rs`, `wire/tests.rs`, `wire/attach.rs`, `wire/mod.rs` | write the literal; the zoned and `Date64` refusal; its text; the refusal test; stale #168 text |
| `peacockdb-core/src/test_support/corpus_golden/tests.rs` | a fixture that used mixed-join's disabled cell moves to one that stays off |
| `cpp/src/expr.cpp`, `cpp/src/peacock/expr.h` | the type map, the literal, the AST refusal, the arithmetic arm |
| `cpp/tests/gpu/test_plan_executor.cpp` | the arithmetic gtests; the `Literals` walk row |
| `peacockdb-core/src/planner/tests/plan_goldens.rs`, `testdata/goldens/*/*.plans.txt` | `NOT_RUNNABLE` empty; mixed-join's plan |
| `peacockdb-core/src/test_support/duckdb_oracle.rs` and its tests, `scripts/ticket_widget.py` | `timestamp("<column>")`, only if the measurement asks for it |
| `testdata/pbench-queries/`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/{tpch,pbench}.sf1/*` | the new queries; mixed-join on |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/`, `archive/archived-tickets.md`, `tickets.md` | the wire type; counts; #168 archived |

Component-level API: the wire format. `DataType` gains `IntervalMonthDayNano` (appended), and
`ScalarValue` gains three fields. No C ABI symbol or facade change.

## Restriction

- Interval literals in date and timestamp `+` and `-`. These stay refused at plan time under
  [#249](../tickets/complete-coverage.md#t249) (wire nested/time types), by name:
  - an interval column;
  - interval-by-number arithmetic and interval comparisons;
  - an interval on a timestamp with a time zone, since arrow does months and days in the zone
    and the device has none;
  - an interval on a `Date64`, since the device holds it as a millisecond timestamp and would
    floor a sub-day part that DataFusion drops.
- No cpu golden moves except mixed-join's plan line and the new queries' sections.

## Verification

- Wire tests: each of the three DataFusion interval literal types round-trips into the three
  parts; a NULL interval has `is_null`.
- gtests over a hand-built date and timestamp column:
  - Jan 31 + 1 month in a leap and a non-leap year;
  - Mar 31 − 1 month;
  - `date + 36 hours` and `date − 12 hours`;
  - a mixed interval on each timestamp unit, seconds included;
  - `interval + date`.

  Each expected value is written from DataFusion's answer to the same SQL.
- The new pbench queries and mixed-join green at all five device modes on nebius-gpu, DuckDB's
  device comparison included, and on the cpu. The dispatched `GPU Tests (nebius-gpu)` CI job is
  green.

## Signoff (2026-10-21)

Solved under its constraints. An interval literal crosses the wire as months, days and
nanoseconds (`IntervalMonthDayNano` appended); the device adds months with the month-end clamp,
then days, then nanoseconds, as arrow does, in a new `interval.cpp`. Only a date or naive
timestamp ± a literal reaches the wire; every other interval use, a CASE or function argument
included, is refused at plan time naming #249. `NOT_RUNNABLE` is empty, tpch mixed-join is on at
all five device modes, six pbench queries cover the spec's list on both engines, and DuckDB's
timestamp answers compare through `date(…)` (no `timestamp(…)` facet needed: measured). #168
archived. Completeness: reviewer 0/0, analyst 0/0; no `architecture.md` sentence falsified.
Shortcuts or bandaids: none. `interval.cpp` is proven on cuDF 25.02; 26.02 only by CI's build leg.

---

<!-- archived from llm-wiki/tasks/oracle-cleanup.md -->

**Merged 2026-10-23 as PR #188, chain K's task, one commit `272819bd`, merged into master by `a9fbbe6d`.**

# Oracles that say what they compare, tests that never read Rust source, and a ticket triage

Kind: production

**This task closes [#252](../tickets/testinfra.md#t252)** (checkout reads remotely) **and
[#287](../tickets/corpus-coverage.md#t287)** (row order unchecked). Twelfth of chain K, after
round-tolerance; interval-days follows it and uses its grammar. It archives #251, #253, #254, #263
and #266 as well, which it makes moot rather than fixes.

## Why

- **Oracle names hide the comparison.** Every row oracle sorts both answers as text before
  comparing them: `duckdb_exact`, `data_fusion_exact`, `golden_exact` and their approximate twins.
  So "exact" does not mean exact, and no oracle checks the row order of an `ORDER BY` query.
- **#251's lines record a tolerance as a divergence.** q58, q61 and q66 differ from DuckDB only in
  the digits a decimal division truncates ([#288](../tickets/scalars.md#t288)): at most 1.0e-6
  relative (q58), measured 2026-10-20. Calling that a divergence keeps them under a ticket.
- **Tests read the checkout.** `ticket_is_open`, `corpus_lines` and `macro_invocations` read
  files at run time, so a staged test binary fails on a remote host (#252). A tickets-only
  commit can also turn tests red while CI skips them (#263).
- **Constants.** `chunk_bytes`'s 1 GiB is spelled three times by hand: `1 << 30` in
  `wire/mod.rs`, `DEFAULT_CHUNK_BYTES` in `gpu_backend/join.rs`, and `1ull << 30` in
  `join_residual.cpp`.

## The work

1. **One oracle grammar.** Every `corpus_query!` line takes:

       duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>", …)])
       data_fusion(sort, exact|approx)
       golden(sort|no_sort, exact|approx_std)

   - **The first argument says whether both answers are sorted before comparing.**
     `data_fusion` accepts `sort` only; it is written anyway, so every line reads the same.
     `golden(no_sort, …)` compares the device's live answer row for row against the cpu's
     committed section, which a `no_sort` line writes in engine order.
   - **`exact` and `approx` keep today's meaning.** `duckdb`'s `approx` is one unit in our last
     place for a decimal and 1e-11 relative for a float. `data_fusion`'s is 1e-12 relative.
     `golden`'s `approx_std` is `STDDEV_TOLERANCE`, 1e-11 relative on every numeric cell.
   - **`rel(<tol>, "<column>", …)`** is new, for a known numeric divergence. It takes no ticket.
     The named columns must agree within `tol`, relative to the larger magnitude, whether our
     cell is a float or a decimal. Every other column compares as `approx`.
   - **`date("<column>", …)`** is new, for a difference of type convention. DuckDB answers
     `date ± interval` as a timestamp where DataFusion answers a date. A named column's DuckDB
     cell compares by its date part.
   - **Columns are named as our result header prints them**, as string literals. Positions are
     refused. DuckDB's timestamp cells read `2024-02-29T00:00:00` (`duckdb_result.py`'s
     format), and `date` compares the part before the `T`.
   - **These keep their spellings:** `duckdb_fingerprint`, `duckdb_none`, `data_fusion_subset`,
     `data_fusion_disabled` and `live_cpu`. They are not settings of the two axes.
     `schema_validation_disabled(<modes>)` is untouched.
   - **`duckdb_divergent` is deleted.** Chain K's empty-sorts removed its two #205 lines, and
     item 3 turns #251's three into `rel`, so nothing uses it.
2. **`no_sort` compares row for row.** A line whose query's plan ends in a sort (under any limit
   or projection) declares `duckdb(no_sort, …)`, and `golden(no_sort, …)` where its gpu oracle
   is a golden. A guard test enforces both. A fingerprinted line is exempt, and so is a
   `live_cpu` one.
   - **No tie rule.** A comment at the comparison says it is safe because no corpus answer
     compared row for row has tied rows. Measured on 2026-10-20 with DuckDB 1.5.4: 90 lines are
     fully ordered, and the one query with ties, tpch q11, is a fingerprint.
   - **Writers keep order.** The writers of `duckdb-result.txt`, `*mini.result.txt` and
     `gpu-result.txt` keep the engine's row order for those queries and keep sorting the rest.
     Fingerprints are unchanged.
   - **What reaches the order check.**
     - The cpu: at the mode that authors its result section, on every run: its rendering must
       equal the committed section text for text, and that section meets DuckDB's row for row.
       The other four cpu modes are compared only through `data_fusion`, so they stay
       order-blind.
     - The device: at every mode, on every run, through `golden(no_sort, …)`; and against DuckDB
       as of the last recording of `gpu-result.txt`.
     - What else stays order-blind: `live_cpu`.
   - **A real order defect is filed, not fixed.** A cell that comes back in the wrong order is
     turned off under its ticket (#202 and #204 are the known ones), and its line keeps
     `no_sort`.
3. **The corpus moves to the grammar**, by a script, `scripts/rewrite_oracles.py`, which is
   deleted again at the end of the task. q58, q61 and q66 end in `ORDER BY`, so they become
   `duckdb(no_sort, rel(1e-5, …))`, with #251's columns by name.
4. **The registry gains a `duckdb_oracle` column, and the report shows it.**
   - The column comes before `cpu_oracle` and is held to the lines like the other two.
   - All three columns hold the line's full spelling, CSV-quoted where it has commas or quotes.
     Every reader of the CSV parses quoted fields before any cell is written quoted:
     `registry.rs`, cost-report's `Registry::load`, `ticket_widget.py` and `duckdb_result.py`.
   - `testdata/report.html` gets a DuckDB oracle column, placed immediately before the existing
     cpu oracle column: DuckDB, cpu, gpu. Like those, it shows a shortened keyword
     (`sort exact`, `no_sort approx`, `rel 1e-5`) with the full spelling and what it compares on
     hover, and for `rel` and `date`, the columns.
   - `scripts/ticket_widget.py`'s `ORACLE_HOW` moves to the grammar for all three oracles. Its
     fallback for a registry without oracle columns parses the new grammar, and a registry
     without `duckdb_oracle` still renders.
5. **No peacockdb-core test reads the checkout beyond testdata.**
   - `ticket_is_open` and its test `an_archived_ticket_is_not_open_and_a_listed_one_is` go.
   - `corpus_lines()`, `macro_invocations()` and `all_modes_expands_to_the_five_in_either_position` go.
   - `every_oracle_variant_is_named_by_some_line` and
     `every_duckdb_oracle_is_named_by_some_line` read the registry.
   - `every_timed_case_is_enabled_on_a_device` and
     `every_mask_entry_is_one_of_its_lines_device_modes` (`test_corpus_goldens/benchmark.rs`)
     stop parsing the includes at run time. That binary compiles `corpus_benchmark_cases.inc`
     and `corpus_cases.inc` in, through its own `corpus_query_benchmark!` and `corpus_query!`,
     each line submitting its modes and its schema-validation mask as text. The registry
     cannot serve the mask, since it holds only `enabled|disabled`.
   - `every_refusal_names_a_ticket_that_exists` moves from `planner/tests/plan_goldens.rs` to
     cost-report, which is never staged: `--lib` is staged and `llm-wiki/` is not. cost-report's
     `the_index_reads_the_anchors_of_every_wiki_file` and
     `the_widget_renders_a_pbench_section_beside_the_benchmarks` stay where they are.
6. **Source-reading tests move to `cost-report/tests/test_source_formatting.rs`.**
   - **What moves:** `test_module_layout` and its submodules (except the privacy probe below),
     `test_ci_coverage` with `runners.rs`, and `no_declaration_carries_a_block_left_behind_by_a_split`
     from `test_golden_format`.
   - **A new guard:** `test_source_formatting` gains
     `no_core_test_reads_the_checkout`. It fails on any `CARGO_MANIFEST_DIR` read in
     `peacockdb-core/src` or `peacockdb-core/tests` outside `test_support/testdata.rs` and
     `build.rs`.
   - **CI and staging:** CI runs it by name, `cargo test -p cost-report --test
     test_source_formatting`, since `test_ci_coverage` counts a target as run only when its CI
     line names it. `INTENTIONALLY_NOT_IN_CI` follows. `build-test.sh`'s `repo_root` classifier
     goes, since cost-report is never staged.
   - **The privacy probe stays in core.** `a_private_module_is_unreachable_from_outside_the_crate`
     moves to its own binary, `test_crate_privacy`: it reads no source, but it compiles snippets
     against the `peacockdb_core` rlib, and cost-report keeps depending on nothing. It is kept
     off remote staging by name.
   - `scripts/exec_model/tests/test_join_capability.py` stays as it is.
7. **`coding-style.md` says it.** A test does not read the checkout at run time beyond
   testdata, and a line list it needs is compiled in. cost-report, never staged, is the
   exception: its tests may read the ticket files, and its `test_source_formatting` may read
   source. The places that name `test_module_layout` follow the move.
8. **`chunk_bytes`'s 1 GiB lives in `gpu_plan.fbs`.**
   - **The schema:** `enum JoinLimits : ulong { DefaultChunkBytes = 1073741824 }`, with the
     field default `chunk_bytes: uint64 = 1073741824`. A test holds the two equal.
   - **The code:** the planner's cap (`wire/mod.rs`), the Rust executor and `join_residual.cpp`
     use the generated constant. No hand-written copy remains.
   - **Zero is refused.** A mode with no budget leaves the field out, and the session refuses 0
     with a `std::runtime_error` naming `chunk_bytes`, as its other refusals do.
   - **Every caller that passes 0 today** passes the default instead: the device test harnesses
     and the gtests.
   - The comments in `gpu_plan.fbs` and `architecture.md` follow.
9. **Audit `null_equals_null`.**
   - **The claim:** `gpu_plan.fbs` and `architecture.md` say "Every type honours
     `null_equals_null` on both engines".
   - **The check:** all nine join types, on the cpu backend and the device session. The answer
     per type and engine goes in the detail file, with the test that shows it. A type with no
     test showing it gets one.
   - **A failure is filed, not fixed.** A type or engine that does not honour it gets a ticket,
     and the claim names the exception.
10. **Ticket triage**, in `llm-wiki/tickets/`, `archive/archived-tickets.md` and the
    `tickets.md` index:
    - #251 (decimal quotient scale): archived, replaced by
      [#288](../tickets/scalars.md#t288) (decimal division digits), filed 2026-10-20.
    - #253 (DuckDB divergence record) and #254 (untestable subset oracle): archived as stale.
    - #263 (tickets-only CI skip): archived as won't fix, both halves. The second half's
      workaround (close and reopen the PR) is kept in the archived text.
    - #266 (archived ticket accepted): archived as stale, since `ticket_is_open` is gone.
    - #252 (checkout reads remotely): archived as done.
    - #287 (row order unchecked): archived as done, its text rewritten to what was built (item 2).
      The ticket's proposed monotonic key check was not taken.
    - #255 (nested column panic) moves to `scalars.md`. #264 (computed group key) moves to
      `complete-coverage.md`. Every link to either follows.
    - #258 (PR comment headroom): shortened to the problem and the two options.
    - #256, already archived as stale: a line saying it is a duplicate of #282 (scan with no row
      groups).

    `testdata/report.html` is regenerated in the same commit as any registry or ticket move.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc` | every line on the grammar |
| `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs` | the macros parse the grammar; the dropped and moved tests; the order guard |
| `peacockdb-core/tests/test_corpus_goldens.rs`, `test_corpus_goldens/benchmark.rs` | the timed and mask checks off the registry |
| `peacockdb-core/src/test_support/{mod.rs,corpus.rs,duckdb_oracle.rs,device_answer.rs,corpus_gpu.rs,result_text.rs,registry.rs}` and their `tests` | oracle types, `no_sort`, `rel`, `date`, ordered writers; `ticket_is_open`, `corpus_lines`, `macro_invocations` and `Divergent` gone |
| `peacockdb-core/src/planner/tests/plan_goldens.rs` | the refusal-ticket test moved out |
| `peacockdb-core/tests/test_gpu_corpus.rs` (the `golden_exact` literal in `a_device_run_under_a_regeneration_writes_no_golden`) | the new spelling |
| `peacockdb-core/src/test_support/order_hazard.rs` | `plan_ends_in_sort`, reusing its plan walk |
| `peacockdb-core/tests/test_module_layout*`, `test_ci_coverage*`, `test_golden_format.rs`; new `test_crate_privacy.rs` | moved out |
| `cost-report/tests/test_source_formatting.rs` (+ modules), `cost-report/src/main.rs` | the moved tests; quoted-CSV parsing; the refusal-ticket test |
| `testdata/duckdb_result.py`, `testdata/test_duckdb_result.py`, `testdata/goldens/*/{duckdb-result.txt,*mini.result.txt,gpu-result.txt}` | row order kept for `no_sort` queries; quoted CSV |
| `testdata/cost-registry.csv`, `testdata/report.html`, `scripts/ticket_widget.py` | the `duckdb_oracle` column |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/{mod.rs,fb_text.rs,tests.rs}`, `executor/gpu_backend/{join.rs,backend.rs,exec.rs}`, `gpu_tests/`, `cpp/src/operators/{join_residual.cpp,join_residual.h,join_session.cpp}`, `cpp/tests/gpu/test_join_session.cpp` | `JoinLimits`; 0 refused; every 0 caller |
| `cpu_backend/tests/join.rs`, `cpp/tests/gpu/test_join_session.cpp` | the `null_equals_null` tests the audit adds |
| `scripts/build-test.sh`, `.github/workflows/pipeline.yml` | the moved binaries; the widget's Python test |
| `llm-wiki/coding-style.md`, `build-test.md`, `architecture.md`, `prompts.md`, `tickets.md`, `tickets/`, `archive/archived-tickets.md` | the rule, the grammar, row order, `chunk_bytes`, the audit, the triage, the moved binaries' names |

Component-level API: the wire format changes. `JoinLimits` is new in `gpu_plan.fbs`;
`CudfJoin.chunk_bytes` defaults to 1 GiB and 0 is refused. No facade, trait or C ABI symbol
changes.

## Restriction

- No engine behaviour changes except refusing `chunk_bytes = 0`.
- A `null_equals_null` defect is filed, not fixed.
- Wall-time benchmarks and sf40 are out.

## Verification

- The cpu corpus, `--lib`, `test_corpus_goldens` and cost-report tiers green, on verda when it
  is up.
- `gpu-result.txt` regenerated on nebius-gpu for the `no_sort` queries. `gpu_tests::` and the
  join-session gtests green there, since every `chunk_bytes = 0` caller changed. The final
  device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Negative cases:
  - `no_sort` fails on two swapped rows.
  - `rel` fails just outside its tolerance on a named column, and on an unnamed one at
    `approx`.
  - An unknown column name in `rel` or `date` fails by name.
  - `date` fails on a different day.
  - A zero `chunk_bytes` is refused by name.
- The order guard's predicate has unit tests: Limit over Sort, Projection over Limit over Sort,
  an unsorted query, and a line that declares `sort` over a sorted plan fails.
- `no_core_test_reads_the_checkout` passes, and fails on a doctored read.

## Signoff (2026-10-21)

Solved under its constraints. Every corpus line is on the grammar; a query whose plan ends in a
sort compares row for row, held by a guard whose two halves each go red alone, and both writers
keep one row order per line. The registry and report carry `duckdb_oracle`. No core test reads
the checkout beyond testdata; the source-reading tests run in cost-report's
`test_source_formatting`, the privacy probe in `test_crate_privacy`, CI names both.
`JoinLimits.DefaultChunkBytes` is the one 1 GiB and 0 is refused. `null_equals_null` holds on all
nine join types on both engines; no ticket filed. Triage done. The order check found #202 on tpcds
q34 and q71, whose device cells are off under it, as item 2 directs. Completeness: reviewer and
analyst each 1 important (the `duckdb_none` writer order, fixed); two `architecture.md` sentences
the branch wrote corrected. Shortcuts or bandaids: none.

---

<!-- archived from llm-wiki/tasks/round-tolerance.md -->

**Merged 2026-10-23 as PR #187, chain K's task, one commit `8f054f5c`, merged into master by `00d0f95f`.**

# q78 and q2 on the device, compared within a float tolerance

Kind: production

**This task closes [#60](../tickets/corpus-coverage.md#t60)** (round() off by one ulp), as won't
fix, **and [#56](../tickets/corpus-coverage.md#t56)** (q2 CASE in partial sum), once q2 is green.
Eleventh of chain K, after decimal-literal.

## Why

The device's `round(x, p > 0)` is `cudf::round(·, p, HALF_UP)`, which rounds in two steps. So
about one value in twenty at or above 1.0 is one ulp from DataFusion's answer. In q78, 5 `ratio`
cells of 100 rows come back as `2.7800000000000002` against `2.78`: a gap of 4.4e-16, about
1.6e-16 relative. A bit-identical fix needs either three cuDF passes and two temporary columns,
or a custom kernel. Neither is worth one ulp. So the engine keeps `cudf::round`, and the device's
comparison allows the gap.

#56 asks for proof that q2's `sum(CASE WHEN <string equality> …)` is right across a plan's merges.
q2's per-node sections already match the cpu's, and its answer differs only through `round`. A
green q2 is that proof.

## The work

1. **Loosen the device comparison for q78 and q2.**
   - `gpu_oracle` becomes `golden_approx_std`: 1e-11 relative on every numeric cell, the device
     comparison that already exists.
   - `duckdb_oracle` becomes `duckdb_approx`: 1e-11 relative on a float, one last-place unit on a
     decimal. The cpu's `data_fusion_exact` stays, since the cpu is DataFusion's own kernel.
2. **Say it in the code.** The comment at `round` in `cpp/src/expr.cpp` names the two-step
   rounding, the one-ulp gap against DataFusion, and that the corpus compares it within 1e-11.
3. **No engine change.**

## Corpus

`tpcds/q78` and `tpcds/q2`: their five device cells each turn on. `60` leaves both rows and `56`
leaves q2's. Their `gpu-result.txt` sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | the comment at `round` |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q78 and q2 on, under the looser oracles |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #60 archived as won't fix, #56 as done; counts |

Component-level API: none.

## Restriction

No engine change. Only q78's and q2's lines change oracle. If q2 is red for a reason other than
`round`, #56 stays open with the measurement, and q2's cells stay off under it. A device cell further from
the golden than 1e-11 relative (a half-way value rounding to another last digit) is not this
task's either: that query's cells stay off and #60 stays open with the measurement.

## Verification

- q78 and q2 green at all five device modes on nebius-gpu, DuckDB's device comparison included;
  the dispatched `GPU Tests (nebius-gpu)` CI job green.
- The detail file records each device cell that differs from the golden, with the largest
  relative gap, so the archived #60 says how big the gap was.

## Signoff (2026-10-21)

Partly solved. q78 is on at all five device modes under `golden_approx_std` and `duckdb_approx`:
5 of its 1000 cells sit one ulp from the golden, the largest 1.76e-16 relative. #60 is archived as
won't fix with that measurement, and the comment at `round` names the gap. No engine change.
Not met: q2's half of Verification and closing #56. q2's device answer is right, but `round`'s
ulp widens its rendering past the 256 KiB result cap, the recording becomes a fingerprint, and
`duckdb_approx` refuses it. That is a scope gap, not the Restriction's case: the fix is in
`test_support/duckdb_oracle.rs`, outside Scope, and #56 now carries it. The board's "closes #56"
is the human's wording and is stale. Shortcuts: the cpu-side DuckDB check on q78's decimals now
allows one last-place unit, as the spec chose. Completeness: reviewer 0/0, analyst 1 important
(this scope gap); no `architecture.md` sentence falsified.

---

<!-- archived from llm-wiki/tasks/decimal-literal.md -->

**Merged 2026-10-23 as PR #186, chain K's task, one commit `89b112d4`, merged into master by `03eb7da1`. GitHub closed the PR when #172 merged, reading that PR's "Closes #186" (the ticket) as this PR; it was reopened and merged unchanged.**

# A bare decimal literal is a decimal column on the device

Kind: production

**This task closes [#210](../tickets/corpus-coverage.md#t210)** (AST decimal literal). Tenth of
chain K, after value-case.

## Why

cuDF's AST has no fixed-point literal. So `ast_scalar` (`cpp/src/expr.cpp`) turns a `Decimal128`
literal into a scaled double. That is right under `CAST(… AS Float64)`. But
`cudf_ast_can_evaluate` also accepts a bare or unary-wrapped decimal literal, and then
`SELECT 1.5 FROM t` computes a `FLOAT64` column where the plan declares `Decimal128(2, 1)`. The
export refuses it by name. tpcds q5's `0.00 as return_amt` and `net_loss` hit this at every
mode, and #210 is the only ticket left on its device cells.

## The work

1. **`cudf_ast_can_evaluate` refuses a decimal literal**, in its `LiteralExpr` arm, the way its
   `BinaryExprNode` arm already refuses a decimal operand. The column path then builds a real
   `fixed_point_scalar`.
2. **A decimal literal directly under a `CAST` the AST takes (to `Float64` or `Int64`) stays on
   the AST path**, through `ast_scalar` as today. That keeps every current answer and golden
   where it is.
3. **A unary minus over a decimal literal is not this task's.** The column path has no
   `Negative` arm, and DataFusion folds `-1.5` into one literal, so the corpus never builds the
   shape.
4. `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`'s `Decimal128` row says why the
   literal is not an AST literal.

## Corpus

`tpcds/q5`: its five device cells turn on. `210` leaves its registry row. Its `gpu-result.txt`
sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | `cudf_ast_can_evaluate`'s literal and cast arms |
| `cpp/tests/gpu/test_plan_executor.cpp` | the `Literals` walk row; the CAST case |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the `bug_` pin becomes an agreement case |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q5 on |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #210 archived; counts |

Component-level API: none.

## Restriction

Which path a decimal literal takes, and nothing else. No cpu golden moves. A `gpu-result.txt`
section other than q5's that moves is a finding to explain, not to accept.

## Verification

- `bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` becomes
  `a_bare_decimal_literal_agrees`: `Decimal128(15, 3, 1)` projected answers `Decimal128(3, 1)`
  `1.5` on both engines.
- gtests: `CAST(1.5 AS DOUBLE)` and `CAST(1.5 AS BIGINT)` stay AST-able and answer what they
  answer today.
- q5 green at all five device modes on nebius-gpu, DuckDB's device comparison included; the
  dispatched `GPU Tests (nebius-gpu)` CI job green.

## Signoff (2026-10-21)

Solved under its constraints. `cudf_ast_can_evaluate` refuses a bare `Decimal128` literal, so the
column path builds a `fixed_point_scalar` of the declared scale; one directly under a cast to
`Float64` or `Int64` stays on the AST and answers as before. Red then green: the walk row,
`a_bare_decimal_literal_agrees` and q5's cells; q5 on at all five device modes, only its sections
added to `gpu-result.txt`; #210 archived. Completeness: reviewer 0 blocking, 1 important (the
coordinator's own `architecture.md` wording, corrected); analyst no omissions. Shortcuts or
bandaids: none. Kept by design: the cast arm's scaled double, which can sit one ulp from arrow at
scale 2 and above. Not added: a host-only routing case for the predicate (the GPU tier pins it).

---

<!-- archived from llm-wiki/tasks/value-case.md -->

**Merged 2026-10-23 as PR #185, chain K's task, one commit `8d8a9d2f`, merged into master by `2bb5e79f`.**

# The device answers a value-form CASE

Kind: production

**This task closes [#57](../tickets/corpus-coverage.md#t57)** (value-form CASE). Ninth of chain K,
after welford-device.

## Why

`build_column_case` (`cpp/src/expr.cpp`) throws `value-form CASE not supported in column path` for
any `CASE x WHEN v THEN … END`. The throw rests on a false measurement: the gtest that saw a wrong
answer built every Int64 literal as 0 (`reports/corpus-fixes.md`, fix 7). tpcds q39 uses the form
twice per plan, and #57 is the only ticket left on its device cells.

## The work

1. **Lower the value form onto the search form.** Build the comparand column once. Each WHEN
   becomes `comparand = when` (`cudf::binary_operation`, `EQUAL`, `BOOL8`); a literal WHEN uses the
   scalar overload. The result feeds the existing `copy_if_else` fold unchanged.
2. **SQL's rules hold.** A NULL comparand or a NULL WHEN never matches, since `copy_if_else` reads
   a null condition as false. The first matching WHEN wins. No match and no ELSE answers NULL.
3. **Remove what guarded the refusal**: the throw and its comment, and the #57 clause in
   `PlanError::Unsupported`'s doc (`plan/mod.rs`).

## Corpus

`tpcds/q39`: its five device cells turn on. `57` leaves its registry row. Its `gpu-result.txt`
sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | `build_column_case` |
| `cpp/tests/gpu/test_plan_executor.cpp` | the value-form gtest |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the `bug_` pin becomes an agreement case |
| `peacockdb-core/src/plan/mod.rs` | the doc clause |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q39 on |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #57 archived; counts |

Component-level API: none. No wire, ABI or facade change.

## Restriction

The value form only. Nothing else in `build_column_case` changes. No cpu golden moves.

## Verification

- A gtest in `test_plan_executor.cpp`. Its comparand holds a NULL, one WHEN maps to NULL, and
  there are two WHENs where the first match must win. With and without ELSE.
- `bug_a_value_case_is_refused_on_the_device` becomes `a_value_case_agrees`, a cpu-vs-device
  agreement case with a string THEN, and a second with a NULL comparand row.
- q39 green at all five device modes on nebius-gpu, DuckDB's device comparison included. The final
  device run is the dispatched `GPU Tests (nebius-gpu)` CI job.

## Signoff (2026-10-21)

Solved under its constraints. The value form lowers onto the search form's `copy_if_else` fold;
NULL never matches and the first match wins, pinned by two gtests and two cpu-vs-device agreement
cases that were red with the fix reversed. q39 is on at all five device modes; #57 is archived.
Completeness: reviewer 0 blocking/0 important, analyst no omissions, no `architecture.md` sentence
falsified. Shortcuts or bandaids: none. Notes: q39's corpus cells prove no regression rather than
the new semantics (both its WHENs are literals); most of the `report.html` diff is catch-up from
earlier chain-K registry edits; this spec's `#t57` link now points at an archived ticket.

---

<!-- archived from llm-wiki/tasks/welford-device.md -->

**Merged 2026-10-23 as PR #184, chain K's task, one commit `d14e9237`, merged into master by `0b4b9fed`.**

# The device's Welford state: keyless, NULL-free and version-gated

Kind: production

**This task closes [#216](../tickets/corpus-coverage.md#t216)** (the device's global aggregate has
no Welford arm) **and [#94](../tickets/corpus-coverage.md#t94)** (MERGE_M2 count-child type is
cuDF-version-specific). Fourth and last of chain L: after keyless-identity, whose done call is this
task's only zero-row keyless call, and after aggregate-arms, whose one request builder the keyless
Welford runs through and which has already named the triple (#225) and taken the `ddof` from the
wire.

## Why it happens

**#216.** `execute_aggregate`'s keyless path (`key_cols.empty()`) reduces a stddev name with
`make_std_aggregation` whatever the phase: at the init the finished sample stddev of the argument,
at the merge the stddev of the state's first column, the count. The finalize project above then
fails (`ColumnRef index 2 out of range (cols=1)`), and a `var` name is refused by
`make_reduce_agg`. So `SELECT stddev(x) FROM t` and `SELECT var(x) FROM t` are refused on the device
in every shape. cuDF has `M2` and `MERGE_M2` for groupby only.

**#94.** The Merge arm's `MERGE_M2` casts the count child to `INT32`, which cuDF 25.02's
`group_merge_m2` requires; 25.06 and later (cuDF PR #18546) accept only `INT64` or `FLOAT64`. The 26.02 CI leg is
build-only, so nothing fails until a GPU runs a later cuDF.

**All-NULL groups.** DataFusion's Welford state over a group whose values are all NULL is
`(0, 0.0, 0.0)`, never NULL (`VarianceGroupsAccumulator::state` builds its arrays with no null
buffer). cuDF's `MEAN` and `M2` over no valid values are not pinned and may answer NULL. The answer
is right either way, since the finalize answers NULL wherever `count − ddof ≤ 0`. The state is what
differs, and a NULL child in a `MERGE_M2` input beside other lanes' real entries is cuDF's to skip
or to poison.

## The work

1. **#216, the keyless Welford.** A keyless node holding any stddev or var runs through
   aggregate-arms' grouped request builder on one constant `INT32` key column of the input's row
   count, dropped before return. The rest of the node's aggregates (a `sum` or `count` beside it)
   ride along, as in a grouped node. Every other keyless node keeps the `cudf::reduce` path, the
   cheaper pass over raw rows. The `is_std` reduce arm goes.
2. **#216, zero rows.** After keyless-identity, the only keyless call over zero rows is the done
   identity call, and a groupby over zero rows has no groups. So a routed node over zero rows
   builds its one row: `count` 0 (`INT64`), each Welford triple `(0, 0.0, 0.0)`, every other column
   the typed NULL of the type the groupby answers for it over zero rows (the wire's
   `CudfAggregate` carries no output schema; a decimal keeps its scale), named from `state_names`. This is `empty_state`'s table
   (`plan/aggregates.rs`), written once in C++.
3. **All-NULL groups.** The Welford init, grouped, grouping-set and keyless alike (the builder is
   one), applies `cudf::replace_nulls(…, 0.0)` to `$mean` and `$m2`, always. So does every Merge:
   cuDF's `MERGE_M2` itself answers NULL moments for a group whose merged counts are all 0
   (`group_merge_m2.cu`, `is_valid = count > 0`, in 25.02 and 25.10 alike), so a per-lane merge
   would hand NULL children to the cross-lane one. The state is then DataFusion's at every step,
   and no `MERGE_M2` meets a NULL child. The zero-row row of 2 already holds `(0, 0.0, 0.0)`.
4. **#94.** `aggregate.cpp` picks one `constexpr` count type from cuDF's own
   `CUDF_VERSION_MAJOR` and `CUDF_VERSION_MINOR` (`<cudf/version_config.hpp>`, shipped in 25.02 and
   26.02 alike; no CMake define, which would be a second source of the version), `INT32` before
   25.06 and `INT64` from 25.06, and the one `MERGE_M2` site aggregate-arms leaves uses it. Chain
   J's verify-26.02 runs `shuffle-stddev` on 26.02 and may already have gated this site the same
   way; if so, this item only confirms it covers the site aggregate-arms leaves, and #94 is
   archived with verify-26.02 rather than here. The widening back to `INT64` after the merge
   stays, a no-op from 25.06. No runtime probe: each cuDF version is its own build. The gate goes
   when 25.02 does.

## Corpus

A new tpch query, `testdata/tpch-queries/global-stddev.sql`, puts a keyless stddev and var in the
corpus, which no query has:

```sql
-- A keyless stddev and var: the device's Welford over no keys (#216).
SELECT stddev_samp(l_quantity) AS sd, var_samp(l_quantity) AS v FROM lineitem;
```

It reads lineitem whole, so the tp4 modes merge four lanes' states. Its oracle and golden are
`shuffle_stddev`'s (`data_fusion_approximate`, `golden_approx_std`); its five plan goldens, cpu
sections, DuckDB section, `corpus_cases.inc` line and registry row (features `stddev_var`) are
written. Every cell enabled if it passes.

pbench's `empty-dispersion-aggregates` (keyless-identity) has its gpu cells turned on and `216`
struck. Any other row tagged `216` or `94` is run at its off modes and enabled if it passes; a
failing cell takes its ticket.

## Scope

| path | change |
|---|---|
| `cpp/src/operators/aggregate.cpp`, `cpp/tests/` | the keyless route, the zero-row row, the NULL replacement, the count type |
| `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`, `aggregate_schema_cases.rs`, a new `welford_cases.rs` (`aggregate_cases.rs` is near the 1000-line cap) | the #216 pins flip; the zero-row and all-NULL cases |
| `testdata/tpch-queries/global-stddev.sql`, its goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the new query; the rows above |
| `llm-wiki/architecture.md` (Welford), `build-test.md`, `tickets/` | as in the work; counts; #216 and #94 archived; #261's "after #216" line |

Component-level API: none. No facade, trait, ABI or wire change.

## Restriction

The keyless route, the zero-row row, the NULL replacement and the count type. The keyless
`cudf::reduce` path for every other aggregate is unchanged. A Welford companion beside a DISTINCT
stays refused ([#261](../tickets/complete-coverage.md#t261)). No 26.02 device run: that is
verify-26.02's and [#260](../tickets/system-hardening.md#t260)'s.

## Tests

- The #216 pins in `aggregate_dimension_cases.rs` —
  `bug_a_global_welford_init_answers_a_finished_stddev_on_the_device`,
  `bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device`,
  `bug_a_global_stddev_finalize_is_refused_on_the_device`,
  `bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device` — and
  `bug_a_global_stddev_holds_one_finished_float64_where_the_plan_declares_the_welford_state`
  (`aggregate_schema_cases.rs`) become cpu-vs-device agreement cases, `holds_as_declared` where
  they read the handle.
- keyless-identity's identity cases for `Stddev` and `Var` gain their device half: no batch and
  zero-row batches, init and shortcut, both backends agree with `empty_state`.
- A keyless `stddev` beside a `sum` and a `count`, init and merge, both backends.
- All-NULL groups, grouped and keyless, both backends: the init's state is `(0, 0.0, 0.0)` with
  no NULL, on the device as on the cpu. Across lanes: one lane holds a group's values and another
  only NULLs for it; the merged and finalized answer matches the cpu's. A group all NULL on every
  lane finalizes to NULL.
- gtest: the count type is `INT32` under 25.02's `version_config.hpp` (the 26.02 CI leg compiles the other
  branch).

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: `ctest -L cpu` locally against cuDF 25.02; the 26.02 CI build leg green.
- device: `gpu_tests::`, the walk tests, and `test_gpu_corpus` over `shuffle-stddev`,
  `rollup-stddev`, `global-stddev`, `empty-dispersion-aggregates` and the rows above, on the GPU
  host the chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.

## Completeness signoff

Solved under its constraints. A keyless `stddev` or `var` runs on the device through the grouped
request builder on one constant key and answers `empty_state`'s row over zero rows; the Welford
state is never NULL, at the init and after every `MERGE_M2`; `kMergeM2CountType` is read from
cuDF's own version header. `global-stddev` and `empty-dispersion-aggregates` run at all five
device modes. Final pass: no blocking; one important was stale `ddof` prose (fixed), the other a
missing ticket, filed as #289 (first numbered 287, which master had already used): on 25.02 the merge's INT32 count wraps past 2^31, which the
keyless route now makes reachable from a plain `stddev(x)`. Shortcuts: none. The `INT64` branch is
built and unit-tested by CI's `cudf 26.02` leg (a 25.10a image) and has had no device run.

---

<!-- archived from llm-wiki/tasks/aggregate-arms.md -->

**Merged 2026-10-23 as PR #183, chain K's task, one commit `3325712c`, merged into master by `c804e8da`.**

# The device aggregate reads every column by reference and names every column from the plan

Kind: production

**This task closes [#164](../tickets/corpus-coverage.md#t164)** (a column ordinal reaches cuDF
unchecked, and a bad one degrades rather than throws), **[#225](../tickets/corpus-coverage.md#t225)**
(the device names every Welford state column by the same alias) **and
[#280](../tickets/corpus-coverage.md#t280)** (a `stddev` or `var` under a grouping set answers one
column where the plan declares three). Third of chain L, after keyless-identity and before
welford-device, which builds the keyless Welford on the one request builder this task leaves.

## Why it happens

The wire's rule is that a node reads every column through a `ColumnRef { index, name }` and names
every column it emits from the plan's declared schema. `execute_aggregate`
(`cpp/src/operators/aggregate.cpp`) keeps it for an init's arguments and its group keys, and breaks
it everywhere else.

- **Inputs.** The grouped Merge arms read state with a running cursor, `in_off`, and recover each
  function's width from its name, from `mergeable_agg_state` (a stddev takes 1 or 3 columns) and
  from the residual column count (an avg takes 1 or 2) (`:491-532`, `:705-738`). The plan already
  puts a `ColumnRef` to each state column in every merge call's `args`
  (`translator/aggregate.rs:173-194`), and the writer sends them; the grouped arms ignore them,
  while the keyless merge reads them (`get_values_col`). Two rules for one input.
- **Outputs.** A function's columns are named from its `alias`. The init's three Welford calls are
  folded into one function node whose alias is the SQL output name, so the triple holds three
  columns under one name (#225). `ddof` is re-derived from that name (`stddev_ddof`), which the
  plan holds (`reports/hacks-audit.md` §5).
- **Three paths, three rules.** The grouped path builds the Welford triple when
  `mergeable_agg_state` is set; the grouping-set path (`:363-423`) does not and answers one
  finished `STD` or `VARIANCE` column (#280); the keyless path reduces by name (#216, welford-device).
- **Dead arms.** The writer sends `Partial` and `Merge` only (`aggregate_writer.rs`), never `avg`
  (decomposed into sum and count), and sets `mergeable_agg_state` whenever a Welford is present.
  So every Final and Single arm, every `avg` arm, the non-mergeable stddev path, `get_values_col`'s
  Final branch and the Final guards are unreachable — the 2026-10-08 column-indexing audit.
- **Unchecked ordinals (#164).** `TableResult` (`cpp/src/plan_executor.h`) holds a table and a
  name vector with nothing tying their lengths; the name sites use `operator[]` (`filter.cpp:42`,
  `join.cpp:260,338,343`, `aggregate.cpp:169`), so a short vector is undefined behaviour.
  `infer_expr_type` (`expr.cpp:330`) answers `type_id::EMPTY` for an out-of-range `ColumnRef`,
  surfacing later as a confusing type error. No C++ reader checks a `ColumnRef`'s name; the cpu
  does (`expr_physical.rs:45`).

## The work

1. **One request builder.** A function node and the node's phase give its cuDF requests and its
   output columns, in one place, used by the grouped path and once per set by the grouping-set
   path:

   | function | Init (`Partial`) | Merge |
   |---|---|---|
   | `sum`, `min`, `max` | that aggregation over `args[0]` | that aggregation over `args[0]` |
   | `count` | `COUNT` over `args[0]`, widened to `INT64` | refused: the plan merges a count with `sum` |
   | `stddev`, `stddev_pop`, `var`, `var_pop` | `COUNT`, `MEAN`, `M2` over `args[0]` cast to `FLOAT64` | `MERGE_M2` over the struct of `args[0..3]` |

   Every input is the `ColumnRef` its `args` carry; a node's state is never located by counting.
   A function node with no `args`, or an `args` of the wrong length for its function, is refused
   naming the function. The builder hands welford-device two readbacks to extend: the init's
   `$mean` and `$m2`, and the moments a Merge reads out of `MERGE_M2`. A computed argument is
   evaluated once per node, not once per grouping set. The grouping-set path, built on it, emits the Welford triple (#280). The
   keyless path keeps `cudf::reduce` for sum, min, max and count, its merge already reading `args`;
   its Welford arm stays as it is — a finished `stddev`, a refused `var` (#216, welford-device).
2. **Names from the plan.** `AggregateFuncNode` (`flatbuffers/gpu_plan.fbs`) appends
   `state_names: [string]` and `ddof: int8`. `state_funcs` (`plan/aggregate.rs`) fills
   `state_names` for every function from its calls' `outputs` — one name for a plain function,
   three for a folded Welford — and `ddof` from `AggStateColumns.ddof`. `execute_aggregate` names
   every state column from `state_names` and reads `func->ddof()`; `stddev_ddof` goes. `alias` and
   `mergeable_agg_state` are deprecated, no slot moved, as chain K deprecated `distinct`; the
   writer stops filling them.
3. **The dead arms go.** `agg_phase` maps `Partial` and `Merge` and refuses the rest, naming the
   mode. The Final, Single and `avg` arms, the non-mergeable stddev path, `make_agg`'s `is_final`,
   `get_values_col`'s Final branch, the width recovery and its check, `has_stddev_or_var_final` and
   its guard, and the keyless Final guards go. `is_avg_name` and `avg`'s `out_decimal_*` handling go
   with them; the two fields stay on the wire, unwritten, as today.
4. **#164.** Every `TableResult` is built through a constructor that refuses a table whose column
   count differs from its name count — chain J's refcounted-scatter adds one
   (`TableResult::owning`); the sites still assembling a `TableResult` by hand go through it. One
   helper, `column_at(table, names, ref)`, resolves a `ColumnRef`: an index past the width throws
   naming the index and the width; a missing name throws; a name other than `names[index]` throws
   naming both. Every `ColumnRef` reader in `cpp/src` resolves through it — `expr.cpp`'s
   `build_column`, `build_expr`'s AST arm and `infer_expr_type` (which throws rather than answering
   `EMPTY`), `project.cpp`, `sort.cpp`, the join's keys and residual (in `join_session.cpp` once
   join-backend has deleted `join.cpp`), the sort-preserving merge's keys and the repartition keys
   (`node_session.cpp`), `aggregate.cpp`'s keys and arguments. The expression evaluators
   (`build_column`, `evaluate_column`, `cudf_ast_can_evaluate`) take the input's names, which they
   do not today. Every name site indexed with `operator[]` uses `.at()`.

   The name check is safe to turn on: the device corpus's schema validator already holds every
   batch a node emits to its node's declared names on every enabled cell, and the one known
   exception, the Welford triple's (#225), is fixed in 2. A union is a forwarder, which the
   validator never sees run, and a branch whose names differ from the union's is refused at
   planning today (`check_branch_schemas`, `plan/union.rs`); `cast_branch`
   (`planner/translator/nodes.rs`) projects such a branch to the union's names, as it already does
   one whose types differ, so the plan runs and the names the node above reads are the union's. A cell that the check
   refuses is a wrong reference found, ticketed as such.
5. **The scan's names, checked at plan time.** The device's scan reads parquet columns by name:
   the writer sends the declared output schema as `file_schema` and no `projection`
   (`wire/node_writer.rs`, `scan`), and `scan.cpp` asks cuDF's reader for those names. The cpu reads
   the plan's file ordinals (`cpu_backend/source.rs`). The two agree only while each declared name
   is the file's column name at its ordinal, which holds for every table registered from its own
   file and breaks for one whose declared schema renames or reorders the file's columns: the
   device then reads another column, or fails. `survivor_metadata`
   (`planner/translator/scan_mapping/parquet_meta.rs`), which already opens the file at plan time,
   refuses a scan where the file's root field name at a projection ordinal differs from the
   declared output field at that position, naming the ordinal and both names. No wire or C++
   change: the name-based read is then safe by construction.

## Corpus

A new tpch query, `testdata/tpch-queries/rollup-stddev.sql`, puts a Welford under a grouping set,
which neither benchmark has:

```sql
-- A stddev and a var under a rollup: the grouping-set path builds the Welford triple (#280).
select l_returnflag, l_linestatus, stddev_samp(l_quantity), var_pop(l_quantity)
from lineitem
group by rollup (l_returnflag, l_linestatus);
```

Its oracle and golden are `shuffle_stddev`'s (`data_fusion_approximate`, `golden_approx_std`); its
five plan goldens, cpu sections, DuckDB section, `corpus_cases.inc` line and registry row (features
`rollup stddev_var`) are written, schema validation enabled. Every cell enabled if it passes; a
failing one takes its ticket.

`tpch/shuffle-stddev` goes to `schema_validation_enabled`, and its `corpus_cases.inc` comment
naming #225 goes; its registry row carries no `225`. Every other
corpus cell keeps its state: nothing else this task touches changes an answer.

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `state_names`, `ddof` appended; `alias`, `mergeable_agg_state` deprecated |
| `peacockdb-core/src/plan/aggregate.rs`, `wire/aggregate_writer.rs`, `wire/fb_text.rs` | the fields filled, written and printed |
| `peacockdb-core/src/planner/translator/nodes.rs` (`cast_branch`) | a union branch projected where its names differ |
| `cpp/src/operators/aggregate.cpp` | the builder, the names, the dead arms gone |
| `cpp/src/plan_executor.h`, `cpp/src/expr.cpp`, `cpp/src/operators/{project,sort,join,filter}.cpp`, `cpp/src/node_session.cpp`, and every `TableResult` construction | #164 |
| `cpp/tests/` | the builder per function and phase; #164's refusals |
| `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs`, its tests | the scan's name check |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs` | the #225 pins flip; a rollup Welford case |
| `testdata/goldens/recipe-payloads.txt` | every aggregate payload |
| `testdata/tpch-queries/rollup-stddev.sql`, its goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the new query; shuffle-stddev |
| `llm-wiki/architecture.md` (the aggregate's wire fields, the C++ aggregate), `build-test.md`, `tickets/` | as in the work; counts; #164, #225 and #280 archived |

Component-level API: the wire format — two fields appended to `AggregateFuncNode`, two deprecated,
no slot moved. No facade, trait or ABI symbol change.

## Restriction

The aggregate's inputs, names and dead arms, #164's checks and the scan's name check. The keyless
Welford (#216), the `MERGE_M2` count type (#94) and all-NULL Welford groups are welford-device's.
Dead code outside the aggregate (`window.cpp`, `limit.cpp`, `union.cpp`), output names other nodes
copy from their input, and the device scan's read by name (checked at plan time instead, 5) do not
change. `__grouping_id` keeps its C++-built name, which is the plan's.

## Tests

- gtests, per function and phase of the builder's table: the requests made and the columns named
  from `state_names`, a `ColumnRef` that is not the next column in the input read where it points,
  and a function whose `args` have the wrong length refused.
- `bug_stddev_holds_three_identically_named_columns` and
  `bug_stddev_merge_holds_three_identically_named_columns` (`aggregate_schema_cases.rs`) assert
  the three declared names, under names without `bug_`.
- A rollup with a `stddev`, a `var` and a `sum` after them, init and merge: both backends agree, and
  the device holds the declared schema.
- #164: a `TableResult` of three columns and two names refused (it needs device memory, so it runs
  with the device tier); a `ColumnRef` past the width, one
  with no name, and one whose name is another column's, each refused with its own message, through
  `build_column` and through a sort key.
- `agg_phase` over `Final` and `Single` refused.
- The scan check: a table registered over a parquet file with a declared schema that renames one
  column, or swaps two of the same type, is refused at planning, naming the ordinal and both
  names; every corpus scan plans as before.
- Every existing aggregate case, walk test and corpus cell answers as before. The gtests that
  build a `ColumnRef` with no name, the Single-mode count tests with no args and the `avg` merge
  test are rewritten to the plan's shapes: they test arms this task removes or references it now
  checks.

## Verification bar

- rust-only: `--lib` (payloads), `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the
  registry tests both ways.
- ffi: `--lib -- ffi_tests::` and the payload test, through `scripts/cargo-cudf.sh`.
- C++: `ctest -L cpu` locally against cuDF 25.02; the 26.02 CI build leg green.
- device: every `peacock_*_tests` gtest binary, `gpu_tests::`, the walk tests, and the whole
  `test_gpu_corpus` (the name check touches every node), on the GPU host the chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.

## Completeness signoff

Solved under its constraints. One request builder serves the grouped and grouping-set paths,
reading every input through its `args` and naming every state column from `state_names`; the
dead arms are gone; every `ColumnRef` reader resolves through `column_ordinal` and every
`TableResult` through a checking constructor; a scan's names are checked at plan time.
`rollup-stddev` runs at five device modes and every enabled corpus cell runs with the schema hook
on. Final pass: no blocking; one important was a check no test could turn red (now pinned on the
cpu tier, shown red), the rest stale prose, fixed. Shortcuts: none. Outside Scope, forced by the
evaluator signature, the private constructor or the name check: `window.cpp`, `dispatch.cpp`, the
`join_*` files, and three test fixtures that read state under the wrong name.

---

<!-- archived from llm-wiki/tasks/keyless-identity.md -->

**Merged 2026-10-23 as PR #182, chain K's task, one commit `109bfffb`, merged into master by `c0f6ae18`.**

# A keyless aggregate answers one row whatever arrived

Kind: production

**This task closes [#199](../tickets/corpus-coverage.md#t199)** (a global aggregate over no
arrival drops its identity row) **and [#282](../tickets/corpus-coverage.md#t282)** (a scan with no
surviving row groups is refused at planning). Second of chain L, after grouping-id and before
aggregate-arms and welford-device; the last builds its keyless Welford on this task's done call.

## Why it happens

A keyless aggregate owes one row over any input: `SELECT count(*) FROM t WHERE false` is `0`, and
`SELECT sum(x), min(x) FROM t WHERE false` is one row of NULLs. Two sites break it.

- **The init over no batch.** A `GpuAggregate` is `Exec`, so a lane that receives no batch makes
  no call and answers nothing. The merge above then folds fewer rows than lanes, and over none
  at all it answers nothing on the device (`gpu_backend/accumulate.rs`) and, on the cpu, a merged
  `count` of NULL (`cpu_backend/accumulate.rs`, `mark_done_and_fetch`'s `!self.grouped`
  clause), which a non-nullable declaration refuses. That is q96, q88 and q90 at the three tp4
  modes: an empty lane comes from tp4-single cutting a one-row-group table into four, or from an
  Inner join with no build rows in a lane.
- **The single-node shortcut** (`translator/aggregate.rs`): a keyless `GpuAggregate` over a
  single-batch input finalizes itself, and when no batch arrives it is never called. Symmetric,
  so no cpu-vs-device comparison sees it.

A third break is the init's own: called once per batch, it answers one row per call, a zero-row
batch included. Adding a zero-row batch adds a row to its output, which `architecture.md`'s
"Zero-row batches change no answer" forbids. The merge absorbs the extra row, so no answer is
wrong; the node's rows are.

## The work

1. **What each aggregator owes over no rows.** `empty_state(agg, input) -> Result<ScalarValue,
   PlanError>` in `plan/aggregates.rs`, beside `state_type` and shaped like it, with a
   `PlanAgg::empty_state` method beside `PlanAgg::state_type`. No wildcard:

   | `PlanAgg` | empty state |
   |---|---|
   | `Count` | `Int64(Some(0))` |
   | `Mean`, `M2` | `Float64(Some(0.0))` |
   | `Sum`, `Min`, `Max` | the typed NULL of `state_type(agg, input)`, `ScalarValue::try_from(&DataType)` |
   | `MergeM2` | unreachable: it merges the Welford triple and declares no state |

   Its doc says why `Mean` is `0.0` and not NULL: `Mean` exists only inside the Welford triple,
   and DataFusion 45's `VarianceAccumulator::state` answers `(0, 0.0, 0.0)` over no rows. Neither
   backend builds a row from it: each engine computes its own identity, and the tests hold both to
   this table.
2. **The category.** `category_of` (`plan/mod.rs`) makes every `GpuAggregate` a
   `BatchAccumulator`, as `GpuLimit`, which also streams and holds nothing, already is. One node,
   one category: the mapping stays by node, not by body. The driver already makes `MarkDone` on
   every `BatchAccumulator` lane, one that saw no batch included, so it does not change.
3. **The recipe** (`wire/attach.rs`, `aggregate()`). One new `CallPattern`,
   `AtDoneIfNothingOut` ("at done, if nothing went out"). Not `AtDone`: that reads as always,
   once, at the end, and the walk tests run every `AtDone` call after the batches, so a keyless
   walk would add an identity row after its real one. Every init's recipe:

   ```
   Aggregate(init)    [batch]         per batch
   Project(finalize)  [prior output]  per batch                     (shortcut only)
   Aggregate(init)    []              at done, if nothing went out
   Project(finalize)  [prior output]  at done, if nothing went out  (shortcut only)
   ```

   The per-batch calls stay `PerBatch`: the recipe says which seq a batch goes to, and whether a
   given batch needs the call is the executor's (next item), as a limit's slice stays
   `PerStraddlingBatch` though a batch wholly inside its interval makes none. The done call reuses
   the init's seq; no second wire node. Only the aggregate's executors accept the new pattern;
   `wire/recipes.rs`, which renders recipes, prints a call with no input and a reused seq cleanly.
   Grouped and keyless share the recipe: what the done call answers is the engine's (next item),
   not the planner's.
4. **The executors.** Each backend's init is a batch accumulator holding one flag, `emitted`.
   `executors_for(…, lane)` builds one per node per lane, so the flag is one lane's, and nothing
   else keeps it: not the driver, the recipe or C++, whose node stays stateless.
   `accumulate_and_fetch` checks the row count first: a zero-row batch is dropped, which releases
   its handle, and answers nothing with no ABI call (`no_abi_calls()`), as the limit's executor
   releases a batch outside its interval (`gpu_backend/accumulate.rs`). The driver still records
   the `Accumulate`, with an empty call list; the trace and the cost registry record calls made, so
   the skip shows as no row. Otherwise it makes the init call (and the finalize call) and answers
   its output, setting `emitted` when that has rows: the device reads the row count the call
   already returns (`produced`), the cpu `RecordBatch::num_rows()`.
   `mark_done_and_fetch` makes the done call(s) when `emitted` is false and answers their output
   if it has rows, nothing otherwise. Over zero rows a keyless init answers its identity row and a
   grouped one no groups, so a grouped lane that saw no rows emits nothing — each engine's own
   answer (cuDF's groupby, DataFusion's `AggregateExec`), with no keyless case in either executor.
   A grouped init stops emitting a zero-row batch per zero-row input. The cpu (`CpuAccumulator`,
   built in `cpu_backend/backend.rs`) runs the init over
   `RecordBatch::new_empty(input schema)`; the device's done call hands the seq no input; the
   driver's mock (`executor/driver/tests/mock.rs`) scripts the new category. The walk tests
   (`wire/gpu_tests/mod.rs`), which drive recipe calls with no executor, keep the same flag per
   lane: `phases` puts `AtDoneIfNothingOut` on the done side, and the walk runs it for a lane only
   where that lane's per-batch calls returned no rows.
5. **No input on the device.** `execute_aggregate` (`cpp/src/operators/aggregate.cpp`), handed no
   input, builds a zero-row table from the node's `aggr_input_schema` (on the wire today, read by
   nothing) through `fb_to_type_id` (`cpp/src/peacock/expr.h`, already exposed), and runs as it does over any
   input: the keyless `cudf::reduce` path answers one row, the groupby no groups. Any other node
   handed no input still refuses.
6. **The merge's clause goes.** A keyless merge now always meets at least one row per lane, so
   the cpu's `!self.grouped` clause, and its `grouped` field, go; the device's merge is unchanged.
7. **#282, a scan of nothing.** `source()` (`planner/translator/nodes.rs`) maps a scan with no
   surviving row groups — every one pruned, or a file with none — to one lane that holds no batch
   (`partition_groups = [[]]`), rather than calling `partition()`, which keeps its refusal of an
   empty survivor list for every other caller. The lane's source executors make no call: the cpu's
   `CpuSource` and the device's source step one call per mapped batch, so none means the lane ends
   with nothing, the arrival 1–6 make every node answer right. The validator needs no change: it
   already accepts a lane with no batch (`plan/source.rs`, "four lanes over two row groups leave
   two of them empty") and refuses only a scan with no lane, a batch reading no row group, or one
   reading a pruned group. The memory estimate prices the lane at zero. The wire never sees an
   empty mapping: no call is made to carry one.
8. **`architecture.md`.** "Zero-row batches change no answer" loses its first break; the
   categories list the aggregate's init as a batch accumulator; the call patterns list the new one;
   "a lane that receives nothing is never runnable" and "an empty lane emits no batch at all" are
   corrected: such a lane gets its `MarkDone`, and a keyless init's emits its identity row. An empty
   grouped answer reaches the sink as nothing, and chain K's empty-sorts (in this chain's base)
   answers it with one zero-row batch under the sink's schema, so `empty-grouped-count` compares
   with DuckDB exactly.

## Corpus

Six pbench queries, in `testdata/pbench-queries/`, reaching the three ways an init meets nothing.
The first four filter `tiny` with `t_id + t_v < 0`, which holds for no row and which no row-group
statistic can prune: the scan reads `tiny`'s one row group and the filter hands the init a zero-row
batch, on the one lane at tp1, and at tp4-single on one lane with three left no batch (`tiny` is
under the small-table size, so tp4-rowgroup and tp4-sized plan it one lane and reach the zero-row
batch only). The last two scan
nothing at all (#282): pbench's `empty` table has no row group, and `t_v < 0` prunes `tiny`'s one.
Every answer checked with DuckDB 1.5 on 2026-10-08 (the first four over `empty`, which answers as
the filter does):

| name | SQL | DuckDB | off on |
|---|---|---|---|
| empty-all-null-aggregates | `SELECT sum(t_v) AS s, min(t_v) AS lo, max(t_v) AS hi FROM tiny WHERE t_id + t_v < 0` | one row: NULL, NULL, NULL | none |
| empty-count-aggregates | `SELECT count(*) AS n, count(t_v) AS nv, sum(t_v) AS s, avg(t_v) AS a FROM tiny WHERE t_id + t_v < 0` | one row: 0, 0, NULL, NULL | none |
| empty-dispersion-aggregates | `SELECT stddev_samp(t_v) AS sd, var_pop(t_v) AS vp FROM tiny WHERE t_id + t_v < 0` | one row: NULL, NULL | gpu: #216, turned on by welford-device |
| empty-grouped-count | `SELECT t_k, count(*) AS n FROM tiny WHERE t_id + t_v < 0 GROUP BY t_k` | no rows | none: a grouped init owes nothing at done |
| empty-table-count | `SELECT count(*) AS n, sum(t_v) AS s FROM empty` | one row: 0, NULL | none: a file with no row group (#282) |
| empty-pruned-count | `SELECT count(*) AS n, min(t_v) AS lo FROM tiny WHERE t_v < 0` | one row: 0, NULL | none: `t_v` is `i * 5`, never negative, so the statistics prune `tiny`'s one row group (#282) |

Each gets its plan goldens, cpu sections, DuckDB section, `corpus_cases.inc` line and registry row,
as pbench's queries do. pbench's `cross-empty-build` and `outer-on-true-empty`, refused at planning
on #282 until now, then meet the joins they test; each cell runs and takes the ticket it fails on. `max(t_pat)` is left out: a string min or max on the device is #195's.

The rows tagged `199` — q96, q88 and q90 at the tp4 modes, q32 at tp4, and any other carrying it
when this task builds — are run at those modes and enabled if they pass; a failing cell takes its
ticket. `199` is struck from their tags, and the `corpus_cases.inc` comments naming #199 are
reworded.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `empty_state`; `category_of` |
| `peacockdb-core/src/wire/mod.rs`, `wire/attach.rs`, `wire/recipes.rs` | the new pattern; the init's recipe; rendering a call with no input |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | `phases` runs the new pattern per lane, only where nothing went out |
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs`, `cpu_backend/backend.rs` | the init accumulator; the merge's clause goes |
| `peacockdb-core/src/executor/gpu_backend/accumulate.rs`, `gpu_backend/backend.rs` | the init accumulator |
| `peacockdb-core/src/executor/driver/tests/` | the mock and the lane cases |
| `cpp/src/operators/aggregate.cpp`, `cpp/tests/` | no input → zero-row table |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_dimension_cases.rs` | the identity cases |
| `testdata/goldens/*/*.plans.txt`, `testdata/goldens/recipe-payloads.txt` | every init's recipe and category |
| `peacockdb-core/src/planner/translator/nodes.rs`, the source executors, the memory estimate | #282's scan of nothing |
| `testdata/pbench-queries/empty-*.sql`, their goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the six queries; the `199` and `282` rows |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | as in the work; counts; #199 and #282 archived |

Component-level API: `ExecutorCategory` of `GpuAggregate`; one `CallPattern` variant, which
prints in the plan text. No facade, trait, ABI or wire change: the done call is an existing
seq handed no input.

## Restriction

The init, its recipe and the merge's clause. The merge's device code,
the limit (#214) and the cpu's sort and merge over zero-row batches (#205) do not change: below the
init, a source of nothing needs no fix of its own for this beyond #282's scan. The keyless Welford on the device stays as it is: a
finished `stddev`, a refused `var` (#216, welford-device).

## Tests

- #282: `select count(*) from nation where n_nationkey < 0;` (tpch) plans one lane with no batch at
  every mode and answers `0` on the cpu; a scan with survivors maps as before.
- `empty_state`: for every init `PlanAgg` over `Int32`, `Int64`, `Float64`, `Decimal128(15, 2)`
  and `Date32` where `state_type` accepts it, the value's `data_type()` is `state_type`'s.
- `category_of`: `GpuAggregate` → `BatchAccumulator`. The init's recipe: four calls and their
  patterns, grouped and keyless alike.
- Driver (mock): a keyless lane with no batch makes one done call and answers one row; one with
  only zero-row batches makes no init call and one done call; one with batches with rows makes no
  done call; zero-row batches interleaved among them change no row. A grouped lane with no batch,
  and one with only zero-row batches, makes one done call and emits nothing.
- Both backends, per `AggFunc`, through `decomposition()`: a keyless init over no batch and over
  two zero-row batches answers the row `empty_state` gives, and the shortcut answers it finalized
  (count 0, the rest NULL). For `Stddev` and `Var` the cpu half is here and the device half is
  welford-device's.
- `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device` becomes
  `a_global_merge_over_no_arrival_answers_nothing_on_both`: no merge meets no arrival now, and
  neither backend invents a row for one.
- A walk test with a lane that receives no batch: a keyless aggregate answers its identity row
  there and its one row elsewhere, matching DataFusion; a keyless walk over lanes with rows makes
  no done call.
- gtest, in a new `cpp/tests/gpu/` file (`test_plan_executor.cpp` is past the 1000-line cap):
  `execute_aggregate` handed no input answers one row, typed as declared, for a keyless
  node, and zero rows for a grouped one.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: `ctest -L cpu` locally against cuDF 25.02.
- device: `gpu_tests::`, the walk tests, and `test_gpu_corpus` over the `199` and `282` rows and the six
  pbench queries, on the GPU host the chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.

## Completeness signoff

Solved under its constraints. Every aggregate init is a batch accumulator answering its identity
row at done where nothing went out, on both engines; a scan of nothing is one lane with no batch;
the six pbench queries and every `199` and `282` cell run, `empty-dispersion-aggregates` off on
the device on #216 as planned. Final pass: no blocking; the importants were comments and
`architecture.md` sentences, fixed. Shortcuts named: an init's done call is priced at zero bytes
(an identity row of one); a keyless merge still answers nothing over no arrival, unreachable since
every init answers per lane; seven `cuda_aggregate_bytes` rises (the identity rows) accepted by the
coordinator, not pre-accepted on the board. Spec drift: pbench plans `tiny` four ways at every
tp4 mode. Left for the human: #214 now names no wrong row; `empty_state` is test-only code.

---

<!-- archived from llm-wiki/tasks/grouping-id.md -->

**Merged 2026-10-23 as PR #181, chain K's task, one commit `258dde0b`, merged into master by `a29f0b7f`.**

# The device's grouping-set id is DataFusion's; q66's quotient and the DISTINCT lowering run on a device

Kind: production

**This task closes [#65](../tickets/corpus-coverage.md#t65)** (the device's grouping-set id is not
DataFusion's value or width) **[#55](../tickets/corpus-coverage.md#t55)** (q66: two-phase
decimal aggregate ignores the partial-phase divisor cast) **and
[#262](../tickets/corpus-coverage.md#t262)** (the DISTINCT lowering's device cells have never
run). First of chain L, whose base is master once chains J and K have both merged: J closes #152,
#220 and #189, which hold most of the cells this task turns on, and K adds the DISTINCT lowering
and `tpch/rollup-distinct`, whose device cells are off on #65.

## Why it happens

**#65.** `execute_aggregate`'s grouping-set arm (`cpp/src/operators/aggregate.cpp`) sets bit `i`
for masked key `i` and builds the id from a `cudf::numeric_scalar<int32_t>`. DataFusion 45 folds
`(acc << 1) | is_null` over the keys in order, the first key highest (`group_id_array`,
`datafusion-physical-plan` `aggregates/mod.rs`), into a `u64`, and holds it at the width
`Aggregate::grouping_id_type` picks: `UInt8` up to 8 keys, `UInt16` to 16, `UInt32` to 32,
`UInt64` beyond; past 64 keys it refuses. A two-key rollup is 0, 1, 3 there and 0, 2, 3 on the
device. The merge only needs one id per set, so it is right; `GROUPING()` over the full key list
reads the device's bits, and the schema validator refuses the width.

**#55.** Most likely stale. DataFusion casts `sum(decimal / int)`'s divisor in the Partial phase
only; the old executor's Final re-evaluated the argument over state. The wire has had no Final
since: `Init` is written as `Partial` and `Merge` as `Merge`, the division runs once in `Partial`,
and a merge reads state columns by reference. What is missing is a device run.

**#262.** Chain K ran without a GPU, so distinct-companions is proven on the cpu only. Its plans
reach the wire as no device has run them: a two-stage aggregate whose outer init runs merge
aggregators over state, a `__distinct_arg` key, a grouping id narrowed by a project. Its C++ edits
(the `distinct` guard gone from `aggregate.cpp`, `test_plan_executor.cpp`'s arguments) were built
and not run. A device answer could differ from the cpu's, and nothing says so: the cells are off.

## The work

1. **#65, the id.** A static `grouping_id_column(mask, nkeys, rows)` in `aggregate.cpp` folds
   `gid = (gid << 1) | masked` over the mask in key order into a `uint64_t`, and builds the column
   from a `numeric_scalar` of `uint8_t`, `uint16_t`, `uint32_t` or `uint64_t` for `nkeys` ≤ 8,
   ≤ 16, ≤ 32, beyond. Past 64 keys it throws, naming the count, as DataFusion refuses. The
   `int32_t gid` block and its comment go. No wire change and no payload change: the payloads
   print masks, never an id.
2. **#65, the exec model.** `grouping_set_id` (`scripts/exec_model/operators/aggregates.py`)
   folds the same way, `rollup_masks`' docstring says 0, 1, 3, and the pins in
   `tests/test_operators.py` and `tests/test_end_to_end.py` go from `{0, 2, 3}` to `{0, 1, 3}`, and
   `test_operators.py`'s single-set pin from `2` to `1`.
3. **#55, the proof.** A walk test beside
   `each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them` in
   `wire/gpu_tests/mod.rs`: `SUM_OF_QUOTIENTS`, a grouped `sum(l_extendedprice / l_linenumber)`
   under an outer `sum`, through `assert_walk_matches_datafusion` at `TWO_LANES`, with the
   `PARTIAL`, `MERGE` and `FINALIZE` counts pinned from the trail as the neighbouring tests do.
   Green closes #55 as stale. Red means the defect is live: the throwing call names its phase,
   and the merge's argument is fixed here.
4. **#262, the proof.** A walk test of a two-stage DISTINCT at `TWO_LANES` through
   `assert_walk_matches_datafusion`: `l_returnflag, count(DISTINCT l_suppkey), avg(l_quantity),
   count(*)` over lineitem, grouped by `l_returnflag` — shapes DataFusion 45 answers right (not
   `avg` or `stddev` DISTINCT, which it answers wrong or refuses; distinct-companions' corpus
   note). Plus the full device gtest run, which runs K's `test_plan_executor.cpp`.
5. **The cells.** The corpus rows below are run at every device mode still off when this task
   builds, and each cell is enabled if it passes, compared with its golden and with DuckDB. A cell
   that fails gets the ticket it fails on (an open one, or a new one filed with the query and mode)
   and stays off under it. `65`, `55` and `262` are struck from the rows' tags in
   `testdata/cost-registry.csv` once no off cell is left without another ticket.

## Corpus

#65 reaches the corpus two ways.

**A wrong answer: `tpch/rollup-grouping` alone** (below), the one query reading the id's value.

**A schema refusal only: every corpus plan with a grouping-set id.** `tpch/rollup-over-join` and
tpcds q5, q14, q18, q22, q77, q80 (the plan goldens' `__grouping_id`), and chain K's
`tpch/rollup-distinct`. None reads the id: the merge needs one distinct id per set, which the
device gives, and the final project drops it, so their answers are right. But each row is
`schema_validation_enabled`, and the device corpus holds every batch a node emits to its declared
schema (`test_support/schema_validation.rs`): the init's `Int32` id against the declared `UInt8`
refuses the cell there, the divergence
`bug_grouping_sets_hold_an_int32_grouping_id_where_the_plan_declares_uint8` pins. Predicted, not
seen: on master today no device cell of theirs runs past #152, #189 or #220, which chain J
closes. Their registry tags as of 2026-10-08:

| row | tags |
|---|---|
| tpch rollup-over-join | `152 189 220` (lacks `65`; join-backend adds it if a cell then fails on it) |
| tpcds q5 | `65 152 189` |
| tpcds q14 | `65 220` |
| tpcds q18, q22 | `65 189 220` |
| tpcds q77 | `65 152 212 220` |
| tpcds q80 | `65 152 189 220` |
| tpch rollup-distinct (chain K) | `65 189` |
| pbench rollup-small-keys (chain J, repartition-keys) | `65` |

Any other row carrying `65` when this task builds is run the same way: the registry, not this
table, is the list. The id's value shows only through `GROUPING()`. The corpus's users are q70 and q86 (and q67's
rollup), window queries that never run (`32 143`); they stay off.

**#262's rows:** `tpch/distinct-functions` (off at all five device modes on `262` alone; its
oracle is DuckDB's, DataFusion's being `data_fusion_disabled`), tpcds q28 (`262` and `152`) and
`tpch/rollup-distinct` (`262` once chain K has run, with `65 189`). `262` is struck when no off
cell is left without another ticket. `distinct-functions` carries a `stddev(DISTINCT …)`, whose
float digits differ across lanes as `shuffle-stddev`'s do, so its gpu oracle goes from
`golden_exact` to `golden_approx_std`; `rollup-distinct` and q28 stay exact.

**#55's row:** tpcds q66, tags `55 152 220`. Its five device cells are off; once chain J closes
#152 and #220, `55` alone holds them.

**A new tpch query,** `testdata/tpch-queries/rollup-grouping.sql`: the ticket's simplest, from
[`reports/corpus-fixes.md`, fix 14](../reports/corpus-fixes.md#fix14).

```sql
-- GROUPING() over the full key list reads the grouping-set id itself, so its value and width
-- are the answer, not just a tag that keeps the sets apart (#65).
select n_regionkey, n_nationkey, grouping(n_regionkey, n_nationkey) as g, count(*)
from nation
group by rollup (n_regionkey, n_nationkey);
```

31 rows with `g` in {0, 1, 3} on the cpu; before the fix the device answers the five subtotal rows
with `g = 2`. Its five plan goldens, its cpu sections, its `duckdb-result.txt` section from the
pinned `testdata/duckdb_result.py`, a `corpus_cases.inc` line and a registry row (features
`rollup`) are written. Every cell enabled if it passes; a failing one takes its ticket.

## Scope

| path | change |
|---|---|
| `cpp/src/operators/aggregate.cpp`, a new `cpp/tests/gpu/` gtest file (`test_plan_executor.cpp` is past the 1000-line cap) | the id's fold and width, through hand-built plans in `peacock_plan_tests` (`grouping_id_column` is file-static, so the gtests need the device) |
| `peacockdb-core/src/tests/executor_cases.rs`, `executor/cpu_backend/tests/contract.rs`, `executor/gpu_backend/gpu_tests/contract.rs` | the rollup contract case |
| `scripts/exec_model/operators/aggregates.py`, `scripts/exec_model/tests/test_operators.py`, `test_end_to_end.py` | the model's id |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs`, `peacockdb-core/src/wire/gpu_tests/mod.rs` | the four `bug_` pins flip, `grouping_sets_as_exported` goes; the rollup cases; `SUM_OF_QUOTIENTS` |
| `testdata/tpch-queries/rollup-grouping.sql`, `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's and DuckDB's sections for it | the new query |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the two-stage DISTINCT walk |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | device modes and tags of the rows above |
| `llm-wiki/architecture.md` (grouping sets), `build-test.md`, `tickets/` | the id described; counts; #65, #55 and #262 archived |

Component-level API: none. No facade, trait, ABI or wire change; `recipe-payloads.txt` unchanged.

## Restriction

The id and the two proofs. `GROUPING()` over a subset or a reordering of the keys stays refused on the
device (#230); DataFusion 55's duplicate ordinal stays #228. A cell that fails on anything but the
id, the quotient or the DISTINCT lowering's device path is a ticket, not a fix here.

## Tests

- gtests, on the device: the id over 1, 2, 8, 9, 17, 33 and 64 keys, value and type each (`UInt8`
  at 8, `UInt16` at 9, `UInt32` at 17, `UInt64` at 33 and 64), and 65 keys refused.
- `bug_grouping_sets_carry_the_devices_own_id_and_type`,
  `bug_grouping_sets_over_zero_rows_are_zero_rows_under_the_devices_id_type`
  (`gpu_tests/aggregate_cases.rs`),
  `bug_grouping_sets_hold_an_int32_grouping_id_where_the_plan_declares_uint8`
  (`aggregate_schema_cases.rs`) and
  `bug_a_rollup_partial_holds_an_int32_grouping_id_where_the_plan_says_uint8`
  (`wire/gpu_tests/mod.rs`) become agreement cases under names without `bug_`;
  `grouping_sets_as_exported`, which swapped the bits and the type, goes.
- A rollup contract case, two keys, on both engines: the device's id equals the cpu's, value and
  type. A walk test of `rollup-grouping.sql`'s query at `TWO_LANES` against DataFusion.
- `SUM_OF_QUOTIENTS` and the two-stage DISTINCT walk, as in the work.
- The exec model's suites green with the new pins.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: the build against cuDF 25.02 locally, `ctest -L cpu` green; the id's gtests run with the
  device tier.
- exec model: every `scripts/exec_model/tests/test_*.py`.
- device: every `peacock_*_tests` gtest binary (K's `test_plan_executor.cpp` included),
  `gpu_tests::` and the walk tests, and `test_gpu_corpus` over the rows above, on the GPU host the
  chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.
The corpus run is filtered to the rows above and `rollup-grouping`.

## Completeness signoff

Solved under its constraints. The device's id folds and widens as DataFusion's, every width
boundary pinned from both sides and 65 keys refused; the exec model agrees. #55 is stale, proven
by the quotient walk; #262 closed by the two-stage DISTINCT walks, one reading `GROUPING()`
through the narrowing. 45 device cells on; q5 stays off on #210. Final pass: one blocking (a
Python test pinning tpch's query count at 41, fixed) and two important, both the same stale
`build-test.md` row count (fixed). Shortcuts: none. Outside Scope: comment-only edits in
`planner/translator/`, a shared `cpp/tests/gpu/hand_tables.hpp`, the walk harness skipping the
at-done call on an empty lane as the device does, and `testdata/test_duckdb_result.py`'s count.

---

<!-- archived from llm-wiki/tasks/empty-sorts.md -->

**Merged 2026-10-23 as PR #174, chain K's task, two commits: the task `561f6145` and the dropped wiki-anchor test `5eafc05d`, merged into master by `5987edaf`.**

# A sort over zero-row batches answers a zero-row batch, and an empty answer keeps its columns

Kind: production

**This task closes [#205](../tickets/corpus-coverage.md#t205)** (the cpu's accumulating sort and
merge answer nothing over zero-row batches). Fourth of chain K, after limits; runs without a GPU.

## Why it happens

DataFusion's `SortExec` over zero rows yields no batch at all. `SortedRuns::mark_done_and_fetch`
and `CpuPartitionAccumulator::accumulate_and_fetch` (`cpu_backend/accumulate.rs`) hand that empty
answer to `coalesce_or_nothing`, which reads it as a lane that received nothing. So a
`GpuAccumulateBatchesAndSort` or `GpuMergeSortedPartitions` whose only batches have zero rows emits
no batch on the cpu, where the device emits one of zero rows; `CpuExec::exec` and the cpu coalesce
answer zero rows under the schema, so the two cpu paths disagree with each other as well.

An empty answer also loses its columns. `tpcds/q17` answers zero rows; its `mini.result.txt`
section is a bare `++`/`++`, with no names or types, so nothing checks them, and DuckDB's answer
(`duckdb-result.txt`, chain J's duckdb-oracle) prints the header ours lacks.

## The work

1. **The sort and the merge.** When every held batch has zero rows, `SortedRuns` and
   `CpuPartitionAccumulator` skip the sort and pass the held batches to `coalesce_or_nothing`,
   which concatenates them into one zero-row batch under the schema, as the device answers. A fetch
   has no rows to cut. A lane that received nothing still answers nothing.
2. **An empty answer keeps its schema.** A query whose root received nothing answers one
   zero-row batch under the sink's declared schema. The driver makes it, once, for both backends:
   the unload is called per batch and is never told that none came, and telling it would change
   its trait. The batch is the answer only: it is not recorded as an emitted batch, so the
   execution goldens move only where the rows did. DataFusion's own empty answer, which the corpus
   compares against (`test_support/corpus.rs`), gets its columns the same way, or every zero-row
   query's cpu cells would differ on the header alone. So an answer's schema never depends on how
   its rows ran out. q17's result section is regenerated with its header: tp4-sized writes it,
   where the scatter leaves every lane of the merge empty, which is this rule's case rather than
   1's.
3. **DuckDB.** If chain J's duckdb-oracle has merged when this task builds, q17's empty-answer
   divergence against `duckdb-result.txt` goes with 2, and its line in that task's divergence list
   is struck. If not, nothing here touches DuckDB, and the helper merging duckdb-oracle after this
   task strikes what names #205 there, since #205 is archived here: q17's `duckdb_divergent(205)`,
   the tests asserting #205 open (`duckdb_oracle/tests.rs:157-158`, `:270-278`, `:322-323`) and the
   empty-answer branch in `duckdb_oracle.rs:113-115`. If duckdb-oracle has not merged when this task
   lands, this list is also written into #235's empty-answer bullet.

## Corpus

`tpcds/q17` at `tp1-single`, the one enabled corpus query answering zero rows: its result section
gains its header. Its device cell at `tp1-single`, off on `205`, stays off under
[#281](../tickets/corpus-coverage.md#t281) (chain K has no GPU), and `205` leaves the row.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs` | the sort and merge over zero rows |
| `peacockdb-core/src/executor/driver/` | the empty answer under the sink's schema |
| `peacockdb-core/src/test_support/corpus.rs` | DataFusion's empty answer with its columns |
| `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` | the three `bug_` pins |
| `peacockdb-core/src/tests/end_to_end.rs` | the empty ordered query |
| `testdata/goldens/tpcds.sf1/*mini.result.txt`, q17's `tp1-single` and `tp1-rowgroup` `.cpu.txt` and `.cost.txt` sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | q17 |
| `llm-wiki/architecture.md` ("Zero-row batches change no answer"), `build-test.md`, `tickets/` | the break struck; counts; #205 archived |

Component-level API: `RunReport::batches` (`executor/mod.rs`, public) is never empty for a query
whose root received nothing; no signature changes.

## Restriction

The cpu's two accumulating sorts and the empty answer's schema. The limit's zero-row drop is #214;
a keyless aggregate over nothing is #199 (chain L).

## Tests

- The three `bug_` pins (`bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu`,
  `bug_a_fetch_over_zero_rows_is_nothing_on_the_cpu`,
  `bug_every_lane_a_zero_row_batch_is_nothing_on_the_cpu`, `gpu_tests/accumulate_cases.rs`)
  become agreement cases under names without `bug_`; built here, run on the next GPU run (#281).
- cpu-backend cases: one zero-row batch, two, and one per lane of a merge each answer one zero-row
  batch under the schema; a lane with no batch answers nothing; a fetch over zero rows answers zero
  rows.
- End-to-end on the cpu, the five modes: `select n_name from nation where n_nationkey +
  n_regionkey < 0 order by n_name;` (tpch) answers zero rows under one `n_name` column. Not
  `n_nationkey < 0`: its statistics prune nation's one row group, and a scan of nothing is refused
  until chain L fixes #282.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- device: none.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU. No device run, no GPU cycle. This task reaches `done` when every CI
job but the GPU tests is green; the GPU jobs are not waited on. The device cases are built, not
run; #281 holds them.

## Completeness signoff

Solved under its constraints, re-proved after the rebase onto limits on master ca4dedeb. Both
rules landed as written; every Tests line exists, the unpruned predicate included. Item 3 took its
first branch: q17 and pbench `not-in-uncorrelated` are `duckdb_exact`, and the oracle's
column-count exemption is gone. The device half ran on nebius-gpu (2026-10-19): the four
accumulate cases pass, and q17's five device cells are on. Final pass: reviewer and analyst each
found nothing blocking. Deviations: q17's tp1 device cells run with the schema hook masked on
open #225; `cost-report/src/main.rs`, outside Scope, swaps its example ticket 205→214 because
archiving broke it; two DataFusion comparisons outside the corpus still take the raw empty answer.

---

<!-- archived from llm-wiki/tasks/limits.md -->

**Merged 2026-10-23 as PR #172, chain K's task, one commit `fbeb216c`, merged into master by `d00fddbf`.**

# A scan's limit is a limit node, and a limit's rows are counted once

Kind: production

**This task closes [#186](../tickets/corpus-coverage.md#t186)** (a limit pushed into the scan:
the cpu ignores it, the device refuses it) **and [#234](../tickets/corpus-coverage.md#t234)** (a
mid-plan limit is counted twice, by the driver and by its executor, and nothing compares them).
Third of chain K, after distinct-companions; the chain runs without a GPU.

## Why it happens

**#186.** DataFusion pushes a `LIMIT` into the scan (`DataSourceExec`'s `limit`) and, at tp1,
removes the limit node above it, so the scan's limit is the whole cut. `source()`
(`planner/translator/nodes.rs`) copies it onto `GpuLoadParquet.limit` and gives the scan one lane
(`lanes_for`), but each reader is left to apply it, per call. `CpuSource` never reads it:
`SELECT * FROM lineitem LIMIT 10` answers 6,001,215 rows on the cpu at the tp1 modes. `scan.cpp`
calls `set_num_rows(limit)` and then sets the call's row groups, which cuDF refuses together, so
every device mode fails its first read. And a per-call limit is right only for a scan of one batch,
which the rowgroup and sized modes do not map.

**#234.** For a mid-plan `GpuLimit`, the driver adds each consumed batch's rows to `rows_seen`
(`executor/driver/partitioned.rs`), and `settle_limit` reads it to stop pulling from below. Each
backend's `LimitStream` (`cpu_backend/accumulate.rs`, `gpu_backend/accumulate.rs`) keeps its own
`seen` and decides per batch whether to drop, forward or slice it. Two counts of one stream for
two decisions; they agree today, and a drift would be a LIMIT returning short or reading more than
it needs, with no test to notice.

## The work

1. **#186, a scan's limit is a limit node.** `source()` builds the `GpuLoadParquet` with no limit
   and, where DataFusion's scan carries one, puts a `GpuLimit` with `RowInterval { skip: 0, fetch:
   Some(n) }` directly above it. Where the scan's limit is the whole query's cut — the scan sits
   under the sink, as in `scan-limit` — the cut joins the unload's interval instead, since the
   validator refuses a `GpuLimit` whose only parent is the sink (`plan/validate.rs`). DataFusion
   pushes `skip + fetch` into the scan and keeps its own limit above for any skip, so the scan's cut
   never carries a skip. `lanes_for` keeps its limit arm: one lane, since rows from several
   lanes have no order a cut could follow. The scan's mapping is the mode's own: one batch, one per
   row group, or sized. The limit cuts the batch that straddles `n`, and once it is satisfied the
   driver's hold on its subtree (`settle_limit` → `scheduler.satisfy`) stops the scan. That alone
   bounds the read only where batches are small: at tp1-single, tp4-single and tp4-sized lineitem is
   one batch of all 49 row groups, so `LIMIT 10` would still read the file. So `source()` also
   trims a limited scan's survivors to the shortest prefix whose row counts (from the parquet
   metadata it already holds) reach `n`, before mapping: a small limit reads one row group in every
   mode.
2. **#186, the readers lose their limit.** `GpuLoadParquet.limit` goes, and with it the plan
   text's `limit=` on a source (`plan_text/node_text.rs`). The wire's `CudfScan.limit` is
   deprecated, no slot moved, as chain K deprecated `AggregateFuncNode.distinct`; the writer stops
   filling it, and `scan.cpp` drops `set_num_rows`. `CpuSource` needs nothing: it never read the
   limit.
3. **#234, one count.** The limit's executor keeps the only input count and every drop, forward
   and slice decision, as now. The driver stops counting the limit's input: it counts the rows the
   limit emits, which the lane outcome already carries, and `settle_limit` marks the limit satisfied
   when that reaches `fetch`. A limit with no `fetch` is never satisfied, and one with `fetch 0` is
   satisfied before any read, where today it pulls one batch first. The driver's number is
   then a consequence of the executor's decisions, not a second computation of them. The unload's
   interval, which only the driver decides, keeps counting its input as now.

## Corpus

`tpch/scan-limit` and `tpch/nested-limits` are the only corpus queries with a limited scan (three
at tp1-single). Their plan goldens at the five modes lose the source's `limit=`. scan-limit's cut
joins its unload's interval (`skip=0, fetch=10` at tp1, where it had none). nested-limits gains two
`GpuLimit`s, one over each limited scan: region's (`limit=23`) and part's, beside its existing one.
Their execution sections are regenerated. `mini.result.txt` does not change.
scan-limit's cpu cells at `tp1-single` and `tp1-rowgroup` turn on and `186` leaves their tags.
Their device cells, and nested-limits', stay off under [#281](../tickets/corpus-coverage.md#t281),
filed with this chain: chain K has no GPU.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/translator/nodes.rs`, `translator/mod.rs`, `plan/interval.rs` | `source()`: the limit node above the scan, or the unload's interval |
| `peacockdb-core/src/plan/source.rs`, `plan/mod.rs`, `plan_text/node_text.rs`, every `GpuLoadParquet` constructor's caller, `tests/rebuild.rs` | `GpuLoadParquet.limit` gone |
| `flatbuffers/gpu_plan.fbs`, `wire/node_writer.rs`, `wire/fb_text.rs`, `cpp/src/operators/scan.cpp` | `CudfScan.limit` deprecated; `set_num_rows` gone |
| `peacockdb-core/src/executor/driver/partitioned.rs`, `executor/driver/tests/` (the mock included) | a limit satisfied by what it emits |
| `peacockdb-core/src/tests/end_to_end/limits.rs` | the limit cases |
| `peacockdb-core/src/tests/gpu_tests/source_cases.rs` | the four #186 `bug_` cases |
| `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's sections for the two queries, `corpus_cases.inc`, `testdata/cost-registry.csv` | scan-limit, nested-limits |
| `llm-wiki/architecture.md` (a limited scan; the driver's limit), `build-test.md`, `tickets/` | as in the work; counts; #186 and #234 archived; #281 filed; #229's line on the scan's limit (`memory.md`) corrected |

Component-level API: the wire format, `CudfScan.limit` deprecated, no slot moved. No facade, trait
or ABI symbol change.

## Restriction

The scan's limit and the limit's count. The limit's handling of a zero-row batch stays as it is
([#214](../tickets/corpus-coverage.md#t214), whose fix is in the joins). The unload's interval does
not change.

## Tests

- Translator: `select n_nationkey from nation limit 3;` (tpch) plans a one-lane scan carrying no
  limit under an unload whose interval is `fetch 3`, at every mode; a limited scan below a join or
  a subquery plans a `GpuLimit` above it; a scan without a limit gains no node; a limited scan maps
  only the prefix of row groups that reaches `n`, at every mode.
- End-to-end on the cpu, the five modes: `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)`
  answers 3. Today it answers 25 at every mode: the limit sits in the scan under an aggregate, and
  no reader applies it.
- Driver (mock): a mid-plan limit is satisfied when the rows it emitted reach `fetch` and not
  before; an executor that emits fewer than its input's rows keeps the driver pulling; a limit with
  no `fetch` is never satisfied.
- End-to-end on the cpu, the five modes: `select * from lineitem limit 10` answers 10 rows, and
  `a_limit_slices_at_most_two_batches_and_stops_the_scan` holds over the scan's own mapping.
- The four `bug_` cases in `gpu_tests/source_cases.rs` lose their inputs with the scan's limit:
  they become one agreement case of the scan and three of a `GpuLimit` over the scan's batches,
  under names without `bug_`; built here, run on the next GPU run (#281).

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: the build against cuDF 25.02 locally, `ctest -L cpu`.
- device: none.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU. No device run, no GPU cycle. This task reaches `done` when every CI
job but the GPU tests is green; the GPU jobs are not waited on. Its `scan.cpp` edit and the device
cases are built, not run; #281 holds them.

## Completeness signoff

Solved under its constraints, by the shape the page already described rather than by either
ticket's proposed fix. The device half ran on nebius-gpu: the four converted `source_cases.rs`
cases green, and `tpch/scan-limit` enabled at all five device modes — which corrected its
`gpu_oracle` to `golden_exact`, its result section having always served.

Six deviations outside the Scope table, recorded in `limits-detail.md`: `Empties::fires` firing
every source's first call; a comment in `translator/common.rs`; that oracle change and the guard
term it forced in `test_cpu_corpus.rs`; `gpu_plan.fbs`'s two field docs, which stated the model
#285 is made of; and a fifth `source_cases.rs` case pinning #285. The spec's `## No GPU` and its
"device: none" bar are superseded by chain K's board note.

Filed, not fixed: **#284**, and **#285**, whose zero-column scan keeps `tpch/nested-limits` off at
every device mode — not a limit defect, older than this branch, #281 narrowed to empty-sorts'
half, and the one device gap named there: nested-limits carries the corpus's only mid-plan limit.

---

<!-- archived from llm-wiki/tasks/distinct-companions.md -->

**Merged 2026-10-23 as PR #171, chain K's task, one commit `1634e17c`, merged into master by `9160b970`.**

# A DISTINCT aggregate plans beside any companion

Kind: production

**This task closes [#62](../tickets/corpus-coverage.md#t62)** (a DISTINCT beside an avg or a count
is refused at planning). Second of chain K, on guard-checks' branch; runs beside chain J while the
GPU host is down, so everything it proves, it proves on the cpu. Files nothing new: the one shape
it still refuses with no ticket of its own is [#261](../tickets/complete-coverage.md#t261), filed
with this spec.

## Why it happens

DataFusion removes a DISTINCT only through `SingleDistinctToGroupBy`, whose precondition is that
every DISTINCT aggregate shares one argument, every other aggregate is `sum`, `min` or `max`, and
there are no grouping sets: its outer level re-applies the same function, sound only where
`f(f(x))` is `f(x)`. Anything else reaches us with the flag set, and `decompose`
(`planner/translator/aggregate.rs`) refuses it with `DISTINCT inside … (#62)`. tpcds q28 — six
global `avg, count, count(DISTINCT ss_list_price)` blocks, cross-joined — is refused at every mode.

The restriction is DataFusion's, not the shape's. This engine already separates init from merge
(`decomposition()`, `plan/aggregates.rs`): a `count` merges by `sum`, an `avg` by `sum` over each
of its two state columns. So the outer level can merge each companion's state with ordinary
aggregators instead of re-applying the function.

## The lowering

Applies when every DISTINCT aggregate in the node, of any function the engine decomposes
(`count`, `sum`, `avg`, `min`, `max`, `stddev`, `var`), has the same single argument `x` once
DataFusion's coercion casts are stripped. DataFusion casts `sum`'s and `avg`'s argument and not
`count`'s, so `count(DISTINCT c), sum(DISTINCT c)` over an `Int32` column arrives as `c` and
`CAST(c AS Int64)`. A widening cast that keeps distinct values distinct is stripped: integer to a
wider integer, integer to `Float64` (exact below 2^53), decimal to a wider decimal, a decimal of
precision ≤ 15 to `Float64`. The inner stage groups on the stripped `x`; each DISTINCT aggregate
re-applies its own casts over `__distinct_arg`. The decision is made before the node's input is
translated, so the input is translated once: a second translation reaches its sources twice, and
tp4-sized's two-pass planner refuses a plan whose passes reach different source counts. Two
`aggregate_sequence` stages replace the one:

- **Inner stage.** Groups by `(x, keys)` — `x` first — and runs the companions' inits. `x` is
  always its own key column, `__distinct_arg`, even where it is also a group key: under a grouping
  set a key can be masked to NULL, and the distinct argument never is. Shuffled as DataFusion
  shuffles the aggregate — on its keys (a subset of the inner's), or collapsed for a keyless one —
  so every row of an outer group is already in one lane. Emits state; no finalize.
- **Outer stage.** Groups by `keys`; no shuffle (a subset of the inner's keys, `plan/aggregate.rs`'s
  subset rule). Its first node is an init: each DISTINCT aggregate runs as its non-distinct twin
  over the deduplicated `x` (from values); each companion's state column runs its merge rule from
  `decomposition()` (`sum` over a count or a sum, `min`, `max`) as an init aggregator. It then
  merges and finalizes to DataFusion's final schema, so nothing above the node changes.

A NULL `x` makes one inner group, and the outer `f(x)` skips it, as SQL requires.

The four traps from `reports/bugfix-proposals/62-review.md`, built in:
1. The outer stage pairs each companion's state columns with `rule.state` by position, not by
   tag. `declared_nullable`'s `[tag]` lookup does not match this engine's own names (`avg(…)$sum`,
   `$count`) and would refuse q28's `avg` as drifted.
2. Each outer argument's type is the inner stage's declared type at its position, through
   `state_type(merge_func, …)`. `arg_type` reads the raw input's first expression, which in the
   outer stage is the wrong column or out of range.
3. Every state column the outer stage declares is nullable, and every `count` in it — companion
   or DISTINCT — finalizes as `CASE WHEN o IS NULL THEN 0 ELSE o END`: a count merged by `sum`
   answers NULL over an empty keyless input, where SQL says 0. `plan/aggregates.rs`'s count
   finalize stays the bare column; the translator wraps it for the outer stage only.
4. The shapes under "Still refused" stay refused.

And one the review of this spec found: the outer init's `sum` over a decimal state widens it again
(`state_type`), so an outer `sum` companion's state is wider than DataFusion's declared output. Its
finalize casts explicitly to the declared type (`architecture.md`: no executor changes a type the
plan did not ask for).

## Grouping sets

A ROLLUP, CUBE or GROUPING SETS under a DISTINCT plans too. The inner stage expands the sets over
`(x, keys)` with `x` unmasked in every set. DataFusion folds the id first key highest, `(acc << 1)
| masked`, so `x`'s bit is the top one and always 0: the inner `__grouping_id` is the value
DataFusion computes over `keys` alone. Its width is not always: at exactly 8, 16 or 32 keys the
extra key moves it to the next type. The outer stage groups by `keys ++ [__grouping_id]` at the
inner's width; where DataFusion's width differs, a project above the outer stage casts the id to
DataFusion's type. Not a cast in `group_by`: a merge's co-location check and the hash it carries
up read plain column keys only, so a cast key would lose the shuffle's distribution.

On the device the id is [#65](../tickets/corpus-coverage.md#t65)'s: bits from the other end and
`Int32`. There `x` is bit 0, so the inner id is twice the device's usual value — still one per set,
so the groups hold, and only the id's value is wrong, as #65 already makes it; at 8, 16 or 32 keys
the project's cast also overflows it. #65's fix removes both. No device cell reaches it before
then: every grouping-set corpus query is off on #65, #152 or #189. #65 gains a line saying so.

## Still refused

The lowering's classifier refuses these two by name, before `decompose`. Each is a `bug_` test
(`coding-style.md`), with its ticket in a comment above it:
- a `stddev` or `var` companion beside a DISTINCT —
  [#261](../tickets/complete-coverage.md#t261). Its state merges as `Merge::Combined(MergeM2)`, a
  merge-phase aggregator only, and the outer node is an init. A `stddev(DISTINCT x)` is not this:
  it reads values and plans.
- DISTINCT over two arguments, or over two different arguments —
  [#144](../tickets/complete-coverage.md#t144), now with a refusal of its own (it fell to #62's
  until now). #195's bullet saying it has none is updated.

Neither test name carries its ticket number (`coding-style.md`'s Names rule); the number is in
the comment.

Any other aggregate that still carries the flag reaches `decompose`, whose `is_distinct()` check
stays and refuses it: `DISTINCT inside <agg> in a shape the lowering does not handle`. No SQL we
know reaches it. It is the net for a shape DataFusion sends that the classifier does not
recognise, which would otherwise be computed as non-distinct on both engines, where the
cpu-vs-device comparison cannot see it. It names no ticket: it is not a known wrong answer.

## The wire

`AggregateFuncNode.distinct` goes: marked `(deprecated)` in `gpu_plan.fbs`, so `alias` and every
later field keep their slots and both generated sides lose the accessor. With it go the writer's
`distinct: false` (`wire/aggregate_writer.rs`), the C++ guard that reads it (`aggregate.cpp`,
"DISTINCT aggregate … see #62"), and the argument at the three `CreateAggregateFuncNode` calls in
`cpp/tests/gpu/test_plan_executor.cpp`. Nothing sets the flag before or after this task: the
rewrite removes DISTINCT before the wire. FlatBuffers omits a `false` at its default, so
`recipe-payloads.txt` does not move; the payload test confirms it.

## Corpus

q28's five plan goldens go from the `(#62)` refusal to a plan. q28 gains a `corpus_cases.inc`
line, its cpu modes run and each one enabled if it passes (its cpu, cost and result sections
written by the cpu tier), or ticketed if it does not. Its gpu cells stay off: q28 is a cross join,
behind #152 (join-backend). Registry row 29: the plan cells enabled, the cpu cells as run, `62`
struck.

A new tpch query, `testdata/tpch-queries/rollup-distinct.sql`, puts a DISTINCT under a grouping
set, which no corpus query does:

```sql
-- A DISTINCT aggregate under a grouping set. DataFusion's SingleDistinctToGroupBy declines
-- grouping sets, so the flag reaches the translator; the avg companion is q28's.
SELECT l_returnflag, l_linestatus,
       count(DISTINCT l_suppkey), avg(l_quantity), count(*)
FROM lineitem
GROUP BY ROLLUP(l_returnflag, l_linestatus);
```

It reads lineitem whole so the tp4 modes split it across lanes and the inner stage shuffles; about
60k inner groups. Its five plan goldens and its cpu sections are written. Cpu cells: the tp1 modes
enabled; the tp4 modes off on #189, whose shuffle cannot hash the inner merge's `__grouping_id`
(as `rollup_over_join`'s are) until repartition-keys lands. Gpu cells off at every mode on #65,
whose `Int32` id the schema validator refuses, and #189 at tp4. A new registry row: features
`rollup count_distinct`, tickets `65 189`.

A second new tpch query, `testdata/tpch-queries/distinct-functions.sql`, reaches the widened
functions, which no corpus query does:

```sql
-- DISTINCT aggregates of the functions the lowering widens to, beside companions DataFusion's
-- SingleDistinctToGroupBy declines (count(*), avg), so the flag reaches the translator.
SELECT l_returnflag,
       count(DISTINCT l_quantity) AS distinct_qty,
       sum(DISTINCT l_quantity) AS sum_distinct_qty,
       avg(DISTINCT l_quantity) AS avg_distinct_qty,
       stddev(DISTINCT l_quantity) AS stddev_distinct_qty,
       count(*) AS n,
       avg(l_extendedprice) AS avg_price
FROM lineitem
GROUP BY l_returnflag;
```

DataFusion cannot be its oracle. With these companions it runs the DISTINCT aggregates itself,
and DataFusion 45 refuses `stddev`/`var(DISTINCT)` ("not implemented"), refuses a keyless
`avg(DISTINCT)`, and answers a grouped decimal or float `avg(DISTINCT)` as the plain average,
silently: its grouped `avg` accumulator ignores the flag. So its line declares a new cpu oracle,
`data_fusion_disabled`, which skips the DataFusion compare; a comment above the line says DuckDB's
answer is to be its oracle once chain J's duckdb-oracle has merged. Until then its answer is held
by the end-to-end case below, which runs this file against a hand-written oracle. All five cpu
cells enabled; gpu cells off — never run, chain K has no GPU — under [#262](../tickets/corpus-coverage.md#t262), filed with this spec.
A new registry row: features `count_distinct`.

All three lines carry whatever fields `corpus_query!` takes when this task builds. If chain J's
duckdb-oracle has merged, each new tpch query needs its `duckdb-result.txt` section, from the
pinned `testdata/duckdb_result.py`, or `duckdb_none`; and `distinct-functions`' cpu oracle stays
`data_fusion_disabled`, its DuckDB oracle on. If repartition-keys merges after this task, its
`drop_grouping_id` lands in `sequence`, keyed on each stage's own id ordinal (n + 1 in the inner
stage), and it turns on `rollup-distinct`'s tp4 cpu cells and strikes `189`; whichever lands
second does that.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/translator/aggregate.rs`, `aggregate/distinct.rs` (new), `aggregate/tests.rs` (new) | the stage split, the two-stage lowering, the refusals |
| `peacockdb-core/src/planner/translator/tests.rs` | plan tests of the two stages |
| `peacockdb-core/src/planner/tests/join_refusals.rs` | the #62 pin replaced by two `bug_` tests |
| `peacockdb-core/src/tests/end_to_end.rs` | the answers at the five modes; `sql_answers_match_oracle`, a variant taking a separate oracle SQL |
| `peacockdb-core/src/test_support/corpus.rs` | the `data_fusion_disabled` cpu oracle |
| `flatbuffers/gpu_plan.fbs`, `wire/aggregate_writer.rs`, `cpp/src/operators/aggregate.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `distinct` deprecated; its writer, guard and test arguments |
| `peacockdb-core/src/plan/mod.rs` | `PlanError::Unsupported`'s doc drops "a mixed distinct (#62)" |
| `testdata/goldens/tpcds.sf1/*.plans.txt`, the cpu tier's q28 sections, `testdata/cost-registry.csv`, `corpus_cases.inc` | q28 |
| `testdata/tpch-queries/rollup-distinct.sql`, `distinct-functions.sql`, `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's sections for them, `testdata/cost-registry.csv`, `corpus_cases.inc` | rollup-distinct, distinct-functions |
| `llm-wiki/architecture.md` ("DISTINCT lowers to grouping", "Grouping sets", the wire's `aggr_funcs`), `build-test.md`, `tickets/` | the lowering described; counts; #62 archived, #65, #195 and #261 lines; #262 filed with this spec |

Component-level API: the wire format — `AggregateFuncNode.distinct` deprecated, no slot moves. No
facade, trait or ABI symbol changes.

## Restriction

The lowering, its refusals and the `distinct` field. No backend change: both run the two-stage
plan as they run any aggregate sequence. No device fix for #65.

## Tests

- Plan tests of the two stages: grouped at four lanes (one shuffle, hashed on the keys one column
  right; the outer stage co-located with no shuffle of its own), keyless at four lanes, `x`
  already a key, an `avg` companion (its outer `$sum` state the widened decimal), `count` and
  `sum(DISTINCT)` of one `Int32` column sharing one inner key, a ROLLUP (`x` unmasked in every
  set), and a ROLLUP over eight keys whose project casts a `UInt16` id to `UInt8` — and a
  two-key ROLLUP with no such cast.
- End-to-end on the cpu (`tests/end_to_end.rs`, the five modes):
  - against DataFusion on the same SQL, where DataFusion answers right: `count` and `sum`
    DISTINCT beside `count` and `avg` companions; an argument holding NULLs
    (`store_sales.ss_customer_sk`, 129,392 NULLs at sf1); the empty keyless case, red before the
    fix: `select count(ss_customer_sk), count(distinct ss_customer_sk) from store_sales where
    ss_quantity + ss_item_sk < 0;` (tpcds) answers `0, 0` — a predicate row-group statistics
    cannot prune, since `ss_quantity < 0` prunes every row group and the plan is refused before
    it runs;
  - against a hand-written oracle (`sql_answers_match_oracle`): `distinct-functions.sql` itself,
    against SQL DataFusion answers right — the DISTINCT aggregates over `SELECT DISTINCT
    l_returnflag, l_quantity`, joined on `l_returnflag` to the companions over `lineitem`, under
    the same column names.

  No rollup case here: the harness runs all five modes, and a rollup's tp4
  modes fail on the cpu on #189 until repartition-keys lands; `rollup-distinct`'s tp1 cpu cells
  compare its answer with DataFusion instead.
- `bug_` tests for #261 and #144.
- `decompose` called directly with a distinct `AggregateFunctionExpr` is refused with the
  generic message. This is an ordinary test, not a `bug_` one.
- Every existing plan golden but q28's unchanged (rollup-distinct's are new); `recipe-payloads.txt`
  unchanged.

## Verification bar

- rust-only: `--lib` (plan goldens, the planner's tests), `test_cpu_corpus`, `test_corpus_goldens`,
  `test_cost_model`.
- ffi: `--lib -- ffi_tests::` and the payload test, through `scripts/cargo-cudf.sh`
  (`build-test.md`'s FFI workflow).
- C++: `scripts/build.sh --cudf_ROOT <rapids-cuda-12.2> --gcc-version 12` builds `cpp/build`,
  `peacock_plan_tests` included; `ctest -L cpu`.
- device: none. q28's gpu cells wait on join-backend; the device's DISTINCT walk waits on the host.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU, so it does not contend with chain J for nebius-gpu. No device run,
no GPU cycle. This task reaches `done` when every CI job but the GPU tests is green; the GPU jobs
are not waited on. The C++ changes are built, not run on a device.

## Completeness signoff

Solved under its constraints. The lowering is as specified, and `distinct-functions` now runs it
on a device at five modes — the first such plan any device has run, the C++ change with it. #261
and #144 stay refused by name, and the wire's `distinct` is deprecated with no slot moved.

Two shortcuts, both on #225 and both earned by a run: a schema-validation mask off at the three
tp4 modes, where #225 refuses the outer init's Welford state, and `golden_approx_std` for one ULP
of `stddev`. The cast-back stays pinned by a plan test, for a better reason than first given — a
device holds `{type_id, scale}`, not a precision. Nothing else.

Owed at merge: q28's and `rollup-distinct`'s device cells, off on #152, #65 and #189 with #262
narrowed to that; #144's `Float32 → Float64` subcase; and two wiki edits nobody asked for, a
`build-test.md` antipattern bullet and a widened `coding-style.md` re-export rule.

---

<!-- archived from llm-wiki/tasks/guard-checks.md -->

**Merged 2026-10-23 as PR #170, chain K's task, one commit `2c1b99f8`, merged into master by `385a7c2d`.**

# One rule, checked wherever it is written down

Kind: production

**This task closes [#233](../tickets/corpus-coverage.md#t233)** (the plan validator does not
check that a pass-through node keeps its input's column count) **and
[#174](../tickets/corpus-coverage.md#t174)** (two clamps for one rule, and nothing compares them).
First of chain K, which runs beside chain J while the GPU host is down: everything here runs on
the cpu tier. No production behaviour changes; both tickets are a check that is missing.

## Why it happens

**#233.** `declared_width` (`plan/validate.rs`) checks a node's column count for the kinds that
compute a width, and for a Filter without a projection, against its input. The other pass-through
kinds — Sort, CoalesceAllBatches, AccumulateBatchesAndSort, Limit, MergePartitions,
EmitPartitions, MergeSortedPartitions — fall to its `_ => return Ok(())` arm and reach only
`types_across_the_edge`, which zips the node's fields with its input's. `zip` stops at the shorter
list, so a sort declaring three columns over five passes both checks. The planner cannot build
that tree; a test that rewrites a planned tree and calls `validate` can. (The ticket lists the
filter among them; `declared_width` already covers it.)

**#174.** A limit's row range is clamped twice: `RowRange::clamp` (`executor/mod.rs`, calling
`row_range::clamp` in `executor/row_range.rs`; `(offset, length)`) for the cpu, C++ `clamp_row_range` (`node_session.cpp`, `(begin, end)`) for
the device's export and slice. Each side has its own cases — 4 in `executor/row_range/tests.rs`,
10 in `cpp/tests/cpu/test_executor.cpp`'s four `ClampRowRange` tests, 14 in all — and no case
reaches both. A third copy is the driver's mock: `MockUnload::unload`
(`executor/driver/tests/mock.rs`) clamps with its own `min` arithmetic, so the driver's limit
tests check that copy and not the shipped rule.

## The work

1. **#233.** `types_across_the_edge` compares the node's field count with its input's before the
   zip. A mismatch is `PlanError::Invalid` naming the node and both counts, worded as
   `declared_width`'s message is.
2. **#174, the cases.** `testdata/fixtures/row-range-clamp.txt` holds every existing case, one
   per line: `offset length rows -> begin end`, `max` spelling `u64::MAX` (in `offset` and
   `length` only), `#` comments, grouped as to-the-end, past-the-end, empty, overflow. `begin
   end` is the expected half-open span. The fixtures README gains a paragraph for it, and its
   opening line, which says every fixture is read by two crates, is reworded.
3. **#174, Rust.** `row_range/tests.rs` reads the file through `test_support::testdata_root()`
   and checks, per line, `RowRange { offset, length }.clamp(rows)` mapped `(o, l) → (o, o + l)`
   against `(begin, end)`. Its literal cases go.
4. **#174, C++.** `peacock_cpu_tests` takes the `PEACOCK_TESTDATA_DIR` compile definition, read
   with the environment override as `peacock_plan_tests` does. `ClampRowRange` checks
   `clamp_row_range(offset, length, rows)` per line; all four tests' literal cases go.
5. **#174, the mock.** `MockUnload::unload` calls `rows.clamp(batch.rows as u64)`.
   `driver/tests/limit.rs`'s counts stay as they are.
6. Both readers fail on a malformed line, naming its number, and on a file with no case. Both
   accept the same lines: a number is digits only (no sign, no space), as Rust's `u64` parse
   takes it.
7. Every host that runs `peacock_cpu_tests` from a copied tree is handed the fixture. The GPU
   runs install every `peacock_*_tests` binary and run them by glob, `peacock_cpu_tests` included,
   from a testdata tree filled by hand-written lists: `build-test-shadgpu.sh` (beside its
   `cost-registry.csv` rsync) and `pipeline.yml`'s GPU job (its `rsync_retry` list) each gain
   `testdata/fixtures`. `build-test.sh`'s `sync_fixtures` already sweeps every tracked testdata
   file. The rust lib's row-range test runs on no GPU host: those run `gpu_tests::` only.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/plan/validate.rs`, `plan/validate/tests.rs` | #233's check and its two cases |
| `testdata/fixtures/row-range-clamp.txt`, `testdata/fixtures/README.md` | the case table |
| `peacockdb-core/src/executor/row_range/tests.rs` | reads the table |
| `peacockdb-core/src/executor/driver/tests/mock.rs` | the mock calls the clamp |
| `cpp/tests/cpu/test_executor.cpp`, `cpp/CMakeLists.txt` | reads the table; the compile definition |
| `scripts/build-test-shadgpu.sh`, `.github/workflows/pipeline.yml` | `testdata/fixtures` provisioned to the GPU host |
| `llm-wiki/build-test.md`, `llm-wiki/tickets/` | counts; both tickets archived on merge |

Component-level API: none. No facade, trait, ABI, wire or golden change.

## Restriction

The validator check, the shared table and the mock. Neither clamp's code changes; if the table
shows them disagreeing, that is a ticket, not a fix here.

## Tests

- `plan/validate/tests.rs`: a `GpuSort` over a two-column source whose declared schema the test
  overrides (its `kind` field, reachable from `plan`'s own test module; the constructor derives
  the schema) to one column is refused, naming both counts, and so is one overridden to three;
  the sort as built passes. Each fails before the change.
- The fourteen table lines pass on both sides. A deliberately wrong line, tried once by hand and
  not committed, fails both suites on that line.
- The driver's limit tests green unchanged.

## Verification bar

- rust-only: `--lib`, every target green.
- C++: `ctest --test-dir cpp/build -L cpu`, locally, `cpp/build` configured against cuDF 25.02
  (`scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`).
- device: none.

## No GPU

Chain K runs without a GPU, so it does not contend with chain J for nebius-gpu. No device run,
no GPU cycle. This task reaches `done` when every CI job but the GPU tests is green; the GPU jobs
are not waited on. Its edits to `build-test-shadgpu.sh` and to `pipeline.yml`'s GPU job are not
run here; the next GPU run after merge is the first to exercise them.

## Completeness signoff

Solved under its constraints. The validator refuses a pass-through width mismatch; both clamps
and the driver's mock now answer to `testdata/fixtures/row-range-clamp.txt`; the fourteen cases
are the exact union of the two sides' old literals, and the two clamps agree on every one, so
#174 produced no divergence ticket.

One shortcut, and it is the chain's: no GPU. The `build-test-shadgpu.sh` and `pipeline.yml`
GPU-job provisioning edits ship verified by reading and a local rsync rehearsal, not by a run on
the host. No other shortcut or bandaid.

Owed at merge: archive #233 and #174, including `tickets.md`'s corpus-coverage count 33 → 31.

---

<!-- archived from llm-wiki/tasks/join-backend.md -->

**Merged 2026-10-16 as PR #179, chain J's task, two commits: the code `4b75f649` and the board `3e1bb4ba`, merged into master by `72103bf1`.**

# Joins run on the session on both engines, with no recipe and no refusal left

Kind: production

**The cpu mirrors the device, wherever they diverge.** This task makes the device's joins the
reference: the session, one batch per call, the empty-side rules, the pads and the projections.
Any divergence between the two engines that appears while it builds — batch boundaries, a
zero-row batch against no batch, row order where a comparison reads it, intermediate column names,
nullability or types, `in_rows` and call counts, execution or cost sections — is investigated, and
where the device's answer is the designed one, the cpu backend is changed to mirror it rather than
the device bent to the cpu or the comparison loosened. The peacockdb-analyst may be dispatched to
find what the cpu backend has to change; each divergence found and its resolution goes in the
detail file, and one that cannot be resolved here is a ticket naming both engines' output.

**Risks found before building** ([`reports/join-backend-divergence.md`](../reports/join-backend-divergence.md),
2026-10-08), and what to do about each:
1. **The cost gate** fails the PR on any rise in cpu bytes per section, and #137's `IS NOT NULL`
   filters (about 73 tpcds plans at three modes) and the row marker raise them. Run the local
   `cargo run -q -p cost-report -- --cost-diff --base <base sha>` before the PR, list every rise in
   the detail file, and ask the human to accept them on the board, as chain K's header does for its
   two tasks; until then the task waits at `completeness approved`.
2. **Task 6's interim adapter** changes the cpu's goldens for #220's rows (about 82) but the plan
   regenerates them only in Task 8: regenerate them in Task 6's own commit, so it stays green.
3. **Task 14** turns on about 428 device cells: regenerate `gpu-result.txt` there too, or
   duckdb-oracle's coverage check goes red.
4. **An order-dependent selection over a device join** — an unordered `LIMIT`, or a fetch that cuts
   between tied rows whose strings differ, above a hash or nested-loop join — would flap: cuco and
   cuDF write join output through an atomic cursor, so its order varies run to run. No corpus query
   does this today. Add a check that refuses to enable a device cell on such a shape (or on a
   `data_fusion_subset` line), and one harness case pinning the cross join's emitted order
   (`Order::AsEmitted`), which `tpch/nested-limits` relies on.
5. **The empty-build rule** is decided once: the cpu calls the same rule function the device does.
6. **`architecture.md`'s "one plan run twice gives one answer, byte for byte"** does not hold on the
   device's join order; reword it to what does hold (the same rows, the same batch structure).
   Float sums reordered across a join (q39) already compare with a tolerance.

**This task closes** [#155](../archive/archived-tickets.md#t155) (umbrella: join execution through a wider C
and FlatBuffers API), [#152](../archive/archived-tickets.md#t152) (the build handle does not survive a
streamed probe), [#173](../archive/archived-tickets.md#t173) (a finish whose probe produced no keys refuses
what it could answer from the build side), [#212](../archive/archived-tickets.md#t212) (a build side that
emits no batch at all still refuses Right, Full and RightAnti), [#159](../archive/archived-tickets.md#t159)
(RightSemi/RightAnti with a residual filter has no cuDF path), [#59](../archive/archived-tickets.md#t59)
(nullable-key semantics for semi/anti/mark joins), [#80](../archive/archived-tickets.md#t80) (anti-join
`NOT IN` three-valued logic), [#137](../archive/archived-tickets.md#t137) (the planner does not drop null
join keys before the shuffle), [#220](../archive/archived-tickets.md#t220) (the cpu's joins answer several
batches per call), [#190](../archive/archived-tickets.md#t190) (the cpu drops a nested-loop join's
projection), [#207](../archive/archived-tickets.md#t207) (both backends drop a cross join's projection),
[#208](../archive/archived-tickets.md#t208) (the cpu's cross join answers nothing over a zero-row build), and
the tickets join-session-cpp built the C++ half of: [#136](../archive/archived-tickets.md#t136),
[#153](../archive/archived-tickets.md#t153), [#160](../archive/archived-tickets.md#t160), [#215](../archive/archived-tickets.md#t215),
[#63](../archive/archived-tickets.md#t63), and #154's `join.cpp` sites (so #154 closes whole). Eighth of the
join-rewrite chain.

The design is [`join-rewrite-design.md`](join-rewrite-design.md): §1.3 (validation), §3.5 (the
`NOT IN` rewrite), §4 (planner, wire, executors, driver, exec_model), §5.7 (tests). This spec does
not repeat it.

## The work

1. **Planner** (§4.1): the capability matrix as two facts; the refusals lifted (#153, #159, #160);
   the nulls.rs anti/mark refusal **narrowed, not deleted** — moved with `can_be_null` to
   `planner/nullability.rs`, it refuses a nullable `IN`/`NOT IN` subquery off a filter's AND/OR
   spine after negation normal form (under `IS NULL`, `IS [NOT] TRUE/FALSE`, a comparison, a
   function — a `NOT` no longer counts: NNF folds it into the leaf), where a mark join cannot give SQL's
   NULL — refused on [#250](../tickets/joins.md#t250), filed (design §3.5); the two unreachable
   refusals kept; the `NOT IN` rule (§3.5) — only on the AND/OR spine
   from a `Filter` root, `Not(InSubquery{negated: false})` normalized there too, every `Filter`
   visited including those inside expression subqueries, data-driven nullability opened through
   `Url::to_file_path` with `SubqueryAlias` recursing, the counts as `count(lit(1i32))` so
   `AggregateStatistics` cannot turn them into a `PlaceholderRowExec`; planner tests for
   `NOT (x IN S)`, `w = 0 OR NOT (x IN S)`, `NOT (x NOT IN S)` (folded to `x IN S`, a semi join),
   `NOT (w = 0 OR x NOT IN S)` (De Morgan, then `IN`), a `NOT IN` inside an `EXISTS`,
   `(x IN S) IS NULL` (refused, #250) and the unfiltered-`S` count, each answer checked against
   DuckDB; #137's `IS NOT NULL` filters, decided on the translated side (a side whose top node is
   a `GpuEmitPartitions`); the predicate-free nested loop as cross for Inner only, over `true`
   otherwise; the explicit `__rowmarker__` placeholder, including a scan's (`CudfScan.rows_only`);
   an `EmptyExec` arm, a new leaf `GpuEmpty{schema}` that emits no batch on either engine (review
   row 8); `IS [NOT] DISTINCT FROM` on the device — AST `NULL_EQUAL` / `NOT(NULL_EQUAL)`, column path
   `NULL_EQUALS` / `NULL_NOT_EQUALS` — and its promotion to hash keys with `null_equals_null = true`
   when every key of a keyless nested loop would be such a conjunct (review row 9).
2. **Wire** (§4.2): `CudfJoin` written for all three plan nodes, with `chunk_bytes` from the
   planner's scratch budget and both side schemas — Arrow `Timestamp(unit, _)` mapped to the fbs
   `Timestamp*` variants repartition-keys adds and rendered in `fb_text`; an unmapped type is
   already a `PlanError` in every schema (repartition-keys, #249), so `CudfJoin`'s schemas inherit
   it. Cases: a
   Left join carrying a `ts_us` probe column finished with no probe batch, a Right join whose lane
   gets no build batch. `CudfScan.rows_only` written for a zero-column scan. The join recipes,
   roles and inputs removed; the old three fbs tables and `execute_*_join` removed with them.
3. **GPU executor** (§4.3): the session typestate, `set_build(Option)`, `Option` returns,
   `owes_nothing`, `Drop` releasing; `copy_of`, `build_copy`, `finish_without_keys` go; pricing as
   §4.3 says.
4. **CPU executor** (§4.4): one DataFusion join stream per lane, `None` per probe for the
   build-side semi family, one batch per call otherwise.
5. **Driver** (§4.5) and **exec_model** (§4.6).
6. **Tests** (§5.7): the 56 join pins flip; the new harness cases, including every pbench join
   shape at node level and each `key_types` column as a join key; the planner, wire, executor,
   driver and exec_model tests named there.
7. **Corpus and pbench cells**: every cell whose last ticket closes here is run at its mode on
   shad-gpu and compared with DuckDB, and enabled if it passes; one that meets another issue gets
   that ticket on its row (or a new one).
8. **Every commit green.** The plan goldens, the cpu goldens and the cost goldens of a change are
   regenerated in the same commit (#137's tpcds filters, `__rowmarker__`, the recipe line); each
   `bug_` pin flips in the commit that changes its behaviour; the cpu's interim adapter, while the
   trait changes ahead of the stream executor, concatenates a call's chunks into one batch (#220's
   fix) rather than refusing; tpcds q9's cpu cells, on today, stay green through the
   `__rowmarker__` step (the cpu applies nested-loop and cross projections before, or with, it).
9. **Cleanup — nothing of the old join path survives.** Removed, each checked gone by a grep the
   PR quotes and by a build with no dead-code warning on either side:
   - wire: `wire/join.rs`'s recipe writers (all but `CudfJoin`'s), `attach.rs`'s `cross_join`;
     the join-only vocabulary — `Input::{BuildSide, BuildSideCopy, BatchCopy, AccumulatedKeys}`,
     `ProjectRole::{ProbeKeys, NullPad, Narrow}`, `FbKind::{HashJoin, CrossJoin, NestedLoopJoin}`
     and their `recipes.rs` renderings. `CallPattern::AtDone` and `Input::PriorOutput` stay: the
     accumulators use them (`gpu_backend/accumulate.rs`, `attach.rs`).
   - fbs and C++: `CudfHashJoin`, `CudfCrossJoin`, `CudfNestedLoopJoin`; `execute_hash_join`,
     `execute_cross_join`, `execute_nested_loop_join` and their `dispatch.cpp` arms; the map arm's
     "multi-partition joins are not implemented yet" refusal text (`node_session.cpp:584-596`),
     which describes a path no join takes any more.
   - Rust executors: `GpuJoin`'s `per_probe`/`at_done` lists, `make`, `copy_of`, `build_copy`,
     `finish_without_keys`; the cpu's `Calls` (per-call join, key project, finish join, pad
     project, `empty_build_answers_nothing`); `JoinExecutor::without_build`.
   - planner and plan: `planner/nulls.rs` (its narrowed refusal and analysis move to
     `nullability.rs`), `answers_in_one_call`,
     `per_call_join_type`, `finish_join_type`, `empty_build_answers_nothing`, the probe-side coalesce.
   - driver: `feeds_owing_build`, the scatter-drop exception, the mock's `empty_build_owes_its_probe`.
   - exec_model: `operators/recipe_join.py`, `recipe.py`'s `copy_handle`, the `cudf_calls.py`
     entries only the recipe join called.
   - wiki: `architecture.md`'s join recipes and capability matrix rewritten from the design;
     `hacks-audit.md`'s join items (P1, P4, P8, P9, P10, P13; findings 5, 11, 12) marked resolved;
     `build-test.md`'s rows; `join-rewrite-design.md` archived with the specs.
10. **The estimate.** [`reports/join-rewrite-cell-estimate.md`](../reports/join-rewrite-cell-estimate.md),
   committed with the chain's specs, predicts which cells flip and which meet another issue. The
   completeness record compares it with what turned on: each miss, either way, gets a line with its
   cause.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/` (translator `nodes.rs`, `common.rs`, `nulls.rs` → `nullability.rs`, the new `NOT IN` rule, `pipeline.rs`, `mod.rs`), `lib.rs` | §4.1, §3.5 |
| `peacockdb-core/src/plan/join.rs`, `plan/mod.rs`, `plan/validate*` | the matrix; validation; no zero-column schema |
| `peacockdb-core/src/wire/` (`join.rs`, `attach.rs`, `mod.rs`, `node_writer.rs`) | §4.2 |
| `peacockdb-core/src/executor/gpu_backend/`, `cpu_backend/`, `driver/`, `executor/mod.rs` | §4.3–§4.5 |
| `peacockdb-core/src/planner/memory_estimation.rs` | the session's pricing |
| `cpp/src/operators/join.cpp`, `project.cpp`, `dispatch.cpp`, `flatbuffers/gpu_plan.fbs` | the old join paths and tables removed; an empty projection refused |
| `cpp/src/operators/scan.cpp`, `flatbuffers/gpu_plan.fbs` (`CudfScan.rows_only`), `cpp/tests/gpu/test_plan_executor.cpp` | a zero-column scan answers `__rowmarker__`; its gtest |
| `peacockdb-core/src/wire/serialize.rs` | `serialize_join_schema`, which consumes repartition-keys' timestamp mapping and unmapped-type refusal (#249) |
| `peacockdb-core/src/plan/` (a new `GpuEmpty` node), `plan_text/node_text.rs`, `wire/attach.rs`, `executor/{cpu,gpu}_backend/backend.rs`, `planner/memory_estimation.rs` | the `EmptyExec` leaf |
| `cpp/src/expr.cpp` | `IS [NOT] DISTINCT FROM` in `fb_to_ast_op`'s caller and the column path |
| `peacockdb-core/src/tests/**`, `planner/tests/**`, `plan/tests/**`, `executor/**/tests*`, `wire/tests.rs`, `wire/gpu_tests/` | §5.7 |
| `cpp/tests/gpu/test_plan_executor.cpp` | the four `CudfHashJoin` tests onto the session |
| `scripts/exec_model/` | §4.6 |
| `peacockdb-core/Cargo.toml` | `url`, for the nullability tracer's `Url::to_file_path` |
| `testdata/goldens/**`, `recipe-payloads.txt`, `cost-registry.csv`, `corpus_cases.inc` | the session's recipe line; #137's tpcds filters; the 19 `__rowmarker__` nodes and the zero-column scan's payload; cells |
| `llm-wiki/architecture.md` (Joins), `build-test.md`, `tickets/`, `reports/` | the lasting parts of the design; counts; the closed tickets archived |

Component-level API: the `JoinExecutor`/`ProbingJoin` traits (`set_build(Option)`, `Option`
returns, `owes_nothing`; `without_build` removed); the wire's join vocabulary; three fbs tables
removed. The C ABI's old join paths go with them.

## Restriction

Joins, and the planner pieces the design names. No change to non-join nodes' recipes. No
broadcast (#140): the session is its foundation only. No float key normalization (#243).

## Registry

The estimate (`reports/join-rewrite-cell-estimate.md`) expects this task to flip 23 cpu cells
(#190 ×20, #212 ×3) and up to about 428 gpu cells, about 306 of them weighted by confidence; and
three groups to meet an out-of-chain issue instead: tpch rollup_over_join ×5 on #65 (its row lacks
`65` today), tpcds q78 ×5 on #60, tpcds q32 at tp4 on #199. Those rows gain their tickets. The
stale `183` rows' 25 join cells (tpch nested-loop-join, q4, cross-join, nested-loop-left-join,
semi-join, anti-join, q15) are run here, since #183 is closed and #152 is their real blocker.
pbench's join rows turn on with the rest — `empty-side-left-join` and `indf-full-join` among
them. Each cell enabled with its DuckDB case green.

## Verification bar

- rust-only: the full cpu tier, the planner and wire tests, `test_cpu_corpus` with every newly
  enabled cpu cell and its DuckDB case; the golden regenerations reviewed as one diff per kind
  (recipe line, #137 filters, `__rowmarker__`).
- device: the full gpu tier; `test_join_session.cpp`; every newly enabled gpu cell on shad-gpu at
  its mode; `gpu-result.txt` against DuckDB.

## Device workflow

`build-test-shadgpu.sh`: a cycle for the harness, then the corpus in batches by dataset and mode
(about five queries per round where cells are new).

## Signoff — 2026-10-14

Solved under its constraints: both engines run every join on the session; every refusal but #250's
is lifted; the 18 tickets are archived; cpu +150 and device +670 cells, each held to DuckDB. Shortcuts:
the cost gate is red (251 rises, 173 past 10%, `reports/join-backend-cost-rises.md`), pre-accepted by
the board; the dead-code bar is read as "no new warning" (26 at base and head); exec_model's cuDF-call
guard is retired rather than moved (§4.6), its model gone; `IS NOT DISTINCT FROM` keys merge to one
lane rather than shuffle (§4.1); a rows-only scan beside row groups now refuses on #186; `IntoAnyBox`
joins the crate's public surface; q66's cells are on with #55 open. Device runs were on nebius-gpu
under the board's override; shad-gpu's `gpu-tests` job and the sf40 tier are deferred.



---

<!-- archived from llm-wiki/tasks/join-session-cpp.md -->

**Merged 2026-10-16 as PR #177, chain J's task, two commits: the code `4f5b3631` and the board `c32b613b`, merged into master by `67d79d85`.**

# A join is a C++ session: built once, probed per batch, finished once

Kind: production

**This task closes no ticket on its own.** It builds the C++ half of
[#136](../archive/archived-tickets.md#t136) (build-side match tracking when the probe side streams),
[#153](../archive/archived-tickets.md#t153) (equi-join residual filter is applied after the outer gather),
[#160](../archive/archived-tickets.md#t160) (nested-loop join supports Inner and Left only),
[#215](../archive/archived-tickets.md#t215) (a left nested-loop join over a predicate the AST cannot take is
refused on the device) and [#63](../archive/archived-tickets.md#t63) (a zero-column placeholder survives the
cross join and shifts every ordinal above it), and removes #154's ten `join.cpp` sites. Each of
those is reachable only through the planner and the Rust executors, so each closes in
join-backend, where a query or a harness case first shows it fixed. Seventh of the join-rewrite
chain.

The design is [`join-rewrite-design.md`](join-rewrite-design.md): §1 (ABI and FlatBuffers), §2 (the
core), §3 (per-case pseudocode naming every cuDF call), §5.6 (tests). This spec does not repeat it.

## The work

1. `flatbuffers/gpu_plan.fbs`: `CudfJoin` (§1.1), plus `chunk_bytes: uint64` (0 means 1 GiB),
   which join-backend writes from the planner's scratch budget. Pad and absent-side schemas may
   carry the `Timestamp*` `DataType` variants repartition-keys added. `CudfHashJoin`,
   `CudfNestedLoopJoin` and `CudfCrossJoin` stay until join-backend stops writing them.
2. `cpp/include/peacock_gpu.h`, `gpu_executor.cpp`, `node_session.cpp`: the four symbols (§1.2),
   join ids in their own map in `NodeSession::Impl`, freed by `end_plan` and the error path; one
   timing region per call.
3. `cpp/src/operators/join.cpp`, rewritten as the session: §3.0–§3.10, with the validation's
   corrections in place (the `hj` for a non-AST semi residual, `contains` not scatter, matches by
   range, the owned distinct-key table, `R IS TRUE`, the per-side residual steps, chunking by
   `inner_join_size` and `conditional_inner_join_size`). The semi family's cross conjuncts are
   ANDed at the cuDF AST level (`NULL_LOGICAL_AND` over `ExprContext`-kept operands, hoisted
   operands as `ast::column_reference`s), so several conjuncts make one `mixed_*` predicate;
   `hj` is built whenever the pairs path can be taken, and a pairs call with no `hj` throws by name
   rather than dereferencing null. Every pairs-producing path checks `inner_join_size` (or
   `conditional_inner_join_size`) against `size_type`'s max, chunking further or refusing by name,
   so 2³¹+k pairs never wrap. No rows-only arm: a zero-column side is
   refused as a planner bug (§4.1's explicit `__rowmarker__`). The old `execute_hash_join`,
   `execute_cross_join` and `execute_nested_loop_join` stay callable until join-backend switches,
   then go with the old tables.
4. The latent finish defect goes with the old code: today's Left/Full finish is a `LeftAnti` with
   NULL = NULL hardcoded, which would drop NULL-key build rows it should pad. The session has no
   finish join. A gtest pins the right answer.
5. `peacockdb-ffi`: the four symbols' declarations, so join-backend can call them.
6. The gtest binary `peacock_join_session_tests` is added to `install(TARGETS …)` and the
   `INSTALL_RPATH` list (`cpp/CMakeLists.txt:321-327`) — CI and shad-gpu run only installed
   `peacock_*_tests` — with a `main()` that installs the RMM pool and statistics adaptor, as
   `test_plan_executor.cpp:2114` does. One allocation check on a session probe covers #154's
   `join.cpp` sites (join-backend closes #154 whole).

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `CudfJoin` |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `cpp/src/node_session.cpp` | the symbols, the join map |
| `cpp/src/operators/join_session.{h,cpp}` (new) | the session — beside `join.cpp`, which keeps serving today's plans until join-backend deletes it |
| `cpp/src/peacock/expr.h`, `cpp/src/expr.cpp` | `JoinFilterColMap` as a span; the AST-level AND of cross conjuncts |
| `cpp/src/plan_executor.h`, `cpp/src/operators/dispatch.cpp` | the session's handle types; `CudfJoin` has no `execute_node` arm |
| `cpp/tests/gpu/test_join_session.cpp` (new), `cpp/CMakeLists.txt` | the matrix of §5.6 |
| `peacockdb-ffi/src/lib.rs` | declarations |
| `llm-wiki/architecture.md`, `build-test.md` | the C ABI's new symbols; counts |

Component-level API: four new C ABI symbols and one fbs table, both additive. No Rust behaviour
change: nothing calls the symbols until join-backend.

## Restriction

C++ and the ABI. No planner, wire-writer, executor or driver change; no golden moves; the corpus
does not change, since nothing reaches the session yet.

## Verification bar

- device: `test_join_session.cpp` green on 25.02 (shad-gpu), the shad-gpu log naming the installed
  binary; the CI's 25.10a leg compiles it.
  Every case of §5.6's matrix and named list, hand-counted.
- device: the existing gpu tier unchanged (the old paths still serve it).

## Device workflow

`build-test-shadgpu.sh`, a cycle per arm group (hash, residual, semi family, nested loop, cross and
empty sides), then one for the whole file.

## Completeness signoff

Solved under its constraints. 53 gtests over 213 hand-counted cases, green on nebius-gpu's L40S at
cuDF 25.02 under the board's host override; the existing gpu tier unchanged. Four rounds; the
completeness pass found one blocking gap and nine important items, all closed, and each fix proved
red before being believed — #136's fold fails 10 of 53 under an overwriting `set_true`, #153's
preserved-build half 6 of 53 under an empty conjunct list.

Three deviations, each deliberate. §5.6's type × condition × build product is covered for three of
its eight conditions, not all eight: the five absent ones reach the same `probe_empty_build` arm and
the analyst judged them to add nothing. `join.cpp`'s ten #154 sites stay, as work item 6 provides —
the session carries none of those copies and join-backend closes #154 whole. `plan_executor.h`
keeps one clang-format reflow of an untouched declaration, left because the reflowed form is the
conformant one. No bandaids.



---

<!-- archived from llm-wiki/tasks/exit-copies.md -->

**Merged 2026-10-16 as PR #176, chain J's task, two commits: the code `a8fcd4a2` and the board `26c2d587`, merged into master by `b4b83a82`.**

# Operators hand their output over instead of copying it

Kind: production

**This task closes [#154](../archive/archived-tickets.md#t154)** (every operator exit path
deep-copies its output into a fresh table) for the 11 sites outside `join.cpp`; join-session-cpp
rewrites `join.cpp` and takes its 10. Sixth of the join-rewrite chain, after refcounted-scatter,
whose `TableResult` (one owner per column, final shape) it uses without changing it.

## Why it happens

`std::make_unique<cudf::column>(view)` copies the device buffer, and the operators use it to
build a table out of one they just produced, or out of their input. #154 lists the sites by kind.
Outside `join.cpp`:
- **ordinal subset of a fresh table:** `filter.cpp` 41 — a `select` over the fresh table (a repeated ordinal is two entries sharing one owner);
- **an input column kept in the output:** `project.cpp` 44, `window.cpp` 46;
- **a temporary that only needed a view:** `expr.cpp` 834, `build_column`'s `ColumnRef` arm, which
  copies a whole column the caller views one line later — about 46 of the 107 GB q19's lineitem
  filter moves at sf40, on every predicate the AST refuses;
- **unresolved:** `aggregate.cpp` 413, 642, 644, 678, 680, 759, 771, each read and classed by this
  task.

## The work

1. `expr.cpp`: the `ColumnRef` arm returns a view of the input column (`table.column(idx)`); its
   callers take `column_view` already. No ownership change.
2. `filter.cpp`: the kept columns are `select`ed from the fresh table's owners, so a repeated
   ordinal is two entries sharing one owner and answers correctly — the "moved twice, a hole" trap
   of a release cannot arise, and nothing is refused.
3. `project.cpp`, `window.cpp`: an output mixing input columns with computed ones is built with
   refcounted-scatter's `select` and `with`, so it views the input columns it keeps and owns the
   ones it computes, and the input's other columns are freed as today. A repeated
   `ColumnRef` is two entries sharing one owner, so the "moved twice, a hole" trap cannot arise;
   tpcds q84 and q85 (a column projected twice, device cells on at tp1-single) are its regression.
   If a site proves costlier to share than to copy, the task keeps that copy with a measured note
   — the PR says which.
4. `aggregate.cpp`: each of the seven read; a fresh-table site selected as in 2, an input-column
   site as in 3, a site that must copy kept with a one-line reason. Expected outcome, from reading
   them: 6 of the 11 sites outside `join.cpp` lose their copy; 5 keep it — the four Welford struct
   members and one merged-state child, whose copies are small and whose shared form would cost
   more than it saves. The PR lists each with its reason.
5. Traps: a view taken before a release dangles (`ftv` near `join.cpp` ~L372 is the known one;
   none may survive here).

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp`, `cpp/src/peacock/expr.h`, `operators/filter.cpp`, `project.cpp`, `window.cpp`, `aggregate.cpp`, `sort.cpp` | the sites; `evaluate_column`'s borrowed view |
| `cpp/tests/gpu/test_plan_executor.cpp` | the allocation cases |
| `llm-wiki/tickets/corpus-coverage.md`, `build-test.md` | #154 reworded to `join.cpp`'s sites; counts |

Component-level API: none; `TableResult` is unchanged (refcounted-scatter set it). No ABI, wire,
Rust or golden change.

## Restriction

The 11 sites; `join.cpp` is join-session-cpp's. No operator's output changes.

## Tests

One gtest per family (aggregate, filter, project, window, a non-AST predicate's `ColumnRef`)
under an RMM statistics adaptor: the call allocates the output's bytes, not twice them. A filter
with a repeated projection ordinal answers both columns correctly. Every existing test green unchanged.

## Verification bar

- device: the gtests; the full gpu tier and the corpus unchanged; the sf40 q6 and q19 benchmark
  before and after, recorded.

## Device workflow

`build-test-shadgpu.sh`, two cycles.

## Completeness signoff (2026-10-09)

Solved under its constraints. **Six of the eleven sites lose their copy, five keep it** — §4's
prediction — and `join.cpp`'s ten are untouched for the next task. Item 3 landed by a different
mechanism than the spec named: `project.cpp` assembles its handle through the public fields rather
than `select`/`with`, because no constructor can rename a kept column, with `register_handle` as
the shape check. The outcome is identical, repeated ordinals included.

**The nine gtests are the whole of the evidence, and two of them exist because the other seven
could not fail.** Seven bound an allocation, each against the same cuDF work on the same input
rather than a formula, and all seven came out exactly equal; all nine were watched red first. The
two added by the completeness pass bound a *release*, and the run that proved they were needed is
the argument for them: with both operators deliberately over-retaining, 75 of 77 cases passed —
every byte bound green at its exact figure — and only these two went red.

**Deferred, by the human host override and not by this task:** the Verification bar's sf40 q6 and
q19 benchmark before and after. sf40 lives only on shad-gpu, down throughout. So the saving at
`expr.cpp`'s `ColumnRef` arm — about 46 of the 107 GB q19's lineitem filter moves at sf40 — is
**never quantified at that scale**, and the gtest table at 122,880 rows stands in for it. That is
the one gap in this task's evidence. No bandaid applied.



---

<!-- archived from llm-wiki/tasks/stale-cells.md -->

**Merged 2026-10-16 as PR #175, chain J's task, two commits: the code `d3d78149` and the board `f982b8e7`, merged into master by `67dfe046`.**

# The device cells a closed ticket still holds off are run

Kind: production

**This task closes no ticket.** Four tpch rows keep device cells off under tickets that are
closed: `aggregate_groupby`, `shuffle_additive` and `shuffle_stddev` under #183 (utf8-everywhere,
archived done), `filter_project` under #187 (decimal precision at export, archived done). The
archived #183 kept its tag on a row "only where the other four modes have no ticket yet"
(`corpus_cases.inc:53`: "a row keeping `183` has modes never run past the string class"). So 16
device cells — `tp1_rowgroup`, `tp4_single`, `tp4_rowgroup`, `tp4_sized` on each of the four — have
never been run since their tickets closed. Second of the join-rewrite chain, right after
duckdb-oracle: none of these rows has a join, so nothing later in the chain changes them, and each
cell meets DuckDB as it turns on.

The estimate (`reports/join-rewrite-cell-estimate.md` §6) finds no known blocker on them. The
chain's other `183` rows hold join cells (tpch nested-loop-join, q4, cross-join,
nested-loop-left-join, semi-join, anti-join, q15); #152 blocks those, and join-backend runs them.

## The work

1. Run the 16 cells on shad-gpu at their modes, each compared with the cpu's golden as its line
   says and with DuckDB.
2. A cell that passes is enabled. A cell that fails gets the ticket it fails on — an open one, or
   a new one filed with the query and mode that shows it — and stays off under it.
3. `183` and `187` are struck from the four rows' tags once no off cell is left without another
   ticket (`registry.rs`'s rule); the `corpus_cases.inc:53` comment is reworded or removed when no
   row keeps `183`.
4. `shuffle_stddev` runs with schema validation disabled on #225, as its line says; a cell that
   fails on schema alone keeps `225`.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc` | the four lines' device modes; the comment |
| `testdata/cost-registry.csv` | the four rows' cells and tags |
| `llm-wiki/tickets/`, `build-test.md` | any ticket a failure needs; counts |

Component-level API: none. No code change unless a failure is fixed here, which it is not: a
failure is a ticket.

## Restriction

The 16 cells. No fix to anything they show.

## Verification bar

- device: the 16 cells run, each enabled or carrying its ticket; their DuckDB cases green for
  the enabled ones.
- rust-only: the registry tests both ways.

## Device workflow

`build-test-shadgpu.sh`, one cycle under the corpus filter for the four rows.

## Completeness signoff (2026-10-09)

Solved under its constraints. All sixteen cells ran and all sixteen passed and met DuckDB, so
item 2 is vacuous and no ticket was owed; `183` came off three rows and `187` off the fourth,
which takes `187` out of the registry entirely. Seven join rows still keep `183` and are
join-backend's, so `corpus_cases.inc`'s comment stays under item 3's own condition. No code
changed, so the Restriction held by construction.

**Measured on nebius-gpu, an L40S at cuDF 25.02, not the shad-gpu this spec names** — the human's
host override routed it, shad-gpu being down throughout. Deferred by that override and by nothing
else: `--run-benchmarks` and the `bench_` cases, Nsight, any H200 timing, the sf40 pair, and
cuDF 26.02. The DuckDB half of the evidence is re-runnable without a device, by the 38
`duckdb_gpu_tpch_` cases against the committed `gpu-result.txt`.

One golden moved and it is accepted as float noise: `shuffle-stddev mode=tp1-single`, three lines
in their last digits, 8.2e-16 relative against tolerances of 1e-11. The completeness pass found
`architecture.md` claiming a plan run twice is byte-identical, which this contradicts and
`build-test.md` already denied; that sentence now names the exception. No bandaid applied.



---

<!-- archived from llm-wiki/tasks/refcounted-scatter.md -->

**Merged 2026-10-16 as PR #173, chain J's task, two commits: the code `6a925839` and the board `bd30f15d`, merged into master by `8dfd068d`.**

# A scatter's partitions share their table instead of each copying it

Kind: production

**This task closes [#145](../tickets/corpus-coverage.md#t145) (Refcounted handles: stop copying
every partition out of a scatter) and [#197](../tickets/corpus-coverage.md#t197) (the repartition
arm still concatenates a child it can only be handed one of).** Fifth of the join-rewrite chain,
ahead of the join session so the session is written once against the shared-owner handle.
`TableResult` changes here once, to its final shape — one owner per column — which exit-copies
and the join session then use without changing it again. Drawn
from `refcounted-tables.md` §1, §5, §6 (the scatter tests) and §8, which this chain drops; its
join half (§2–§4, the retain symbol) is not needed, since the join session keeps its own build.

## Why it happens

`TableResult` is `{unique_ptr<cudf::table>, column_names}` (`cpp/src/plan_executor.h:16-19`),
so a handle owns its memory alone. The hash repartition arm (`cpp/src/node_session.cpp:517-578`)
takes its input (`combined`), writes the partitioned copy (`parted`, from `spark_hash_partition`),
then deep-copies each partition out of `parted` into its own table (`:567`). `combined` and `parted`
live until the arm returns, so the peak is about 3× the input (#145's text says 2×; inferred from
scoping, not measured), once per aggregate shuffle and once per join side.

#197: the arm concatenates `child[0]`'s handles, but only one ever arrives. The ticket's reason is
wrong — join-side emits have no coalesce below them; the guarantee is the emitter sending
`&[vec![handle]]` (`executor/gpu_backend/emit.rs:57`) — and so is its cost claim: with one handle
`:537` moves the table, so the dead branch costs nothing. The fix is deletion either way.

## The work

1. `TableResult` takes its final shape, one owner per column:

   ```cpp
   struct TableResult {
     std::vector<std::shared_ptr<cudf::column const>> owners;  // owners[i] keeps columns[i]'s buffers alive;
                                                               // two entries may share one column
     std::vector<cudf::column_view> columns;                   // a whole column of *owners[i], or a row slice of it
     std::vector<std::string> column_names;
     cudf::table_view view() const { return cudf::table_view{columns}; }
     static TableResult owning(std::unique_ptr<cudf::table>, std::vector<std::string> names);
     TableResult slice(cudf::size_type begin, cudf::size_type end) const;   // shares owners
     TableResult select(std::vector<cudf::size_type> const& ordinals) const;  // shares owners
     TableResult with(std::unique_ptr<cudf::column>, std::string name) const;  // appends a computed column
   };
   ```
   Never zero columns (a zero-column view reads 0 rows; the plan's explicit `__rowmarker__` keeps
   one). A consumer reads views; one that needs an owning column copies at that site, with a
   reason. A column is freed when the last handle viewing it goes; a slice pins its parent column,
   not the whole table.
   
   The 37 `.table` / `->table` sites across 11 files today (node_session 17, join 6, filter 4,
   project 2, gpu_executor 2, one each in aggregate, dispatch, limit, sort, union, window) read the
   view; a producer wraps its fresh table as owner and whole view. `execute_one`'s signature does
   not change. Erase-on-read stays; erasing drops a reference.
2. The scatter wraps `parted` with `owning` and registers N handles, each its `slice` — every
   partition's columns view row ranges of `parted`'s columns, no copy. `combined` is dropped as soon as `spark_hash_partition` returns, so the peak is
   2× during the partition (an out-of-place `cudf::partition` cannot do better) and 1× after.
3. #197: the arm takes exactly one handle — `owned`/`views` become one, and `child[0].size() != 1`
   is refused by name.
4. **The cost of sharing**, said in the PR and the wiki: one undrained lane keeps all of `parted`
   alive (worse under skew), and the per-partition timers of p1..N−1 drop toward zero, since the
   copy they timed is gone.
5. **String bytes of a slice.** `out_stats.varlen_content_bytes` (`node_session.cpp:229-240`) uses
   `strings_column_view::chars_size`, which reports the unsliced parent's bytes for a sliced view
   (`cudf/strings/strings_column_view.hpp:89-98`). Once partitions are views, every partition would
   report the whole table's string bytes and the string-keyed emits' `batch_bytes` in the cpu
   goldens would stop matching (tpch q1 and shuffle-additive-avg at tp4: six device cells on
   today). The count is taken from the slice's own first and last offsets. The `out_stats` gtest
   scatters a string column.
6. **The accounting ticket**, filed by this task in `memory.md` when it lands: the driver prices
   each lane's batch alone (`driver/accounting.rs`), so a released lane's bytes leave the model
   while the device still holds them behind a sibling — the model under-reports. The fix it names
   is device-reported residency (`GpuBackend::resident_bytes()`).

## Scope

| path | change |
|---|---|
| `cpp/src/plan_executor.h`, `cpp/src/table_result.cpp` (new), `cpp/CMakeLists.txt` | `TableResult` and its constructors |
| `cpp/src/node_session.cpp` | the scatter; #197; the registry |
| `cpp/src/operators/*.cpp`, `cpp/src/gpu_executor.cpp`, `dispatch.cpp` | the `.table` sites |
| `cpp/tests/gpu/test_plan_executor.cpp` | the scatter tests |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | the handle's shape; counts; #145/#197 archived; the accounting ticket |

Component-level API: `TableResult` (internal to the C++ library). No C ABI change, no wire change.

## Restriction

Ownership and the scatter only. No change to any operator's output, to the driver, or to
accounting beyond the ticket.

## Tests

gtests, an RMM statistics adaptor around each: a scatter's N handles share `parted`'s column owners (release
N−1, read the survivor); the N outputs concatenate back to the input; `out_stats` per partition
unchanged; the peak during the call is at most the partitioned table plus the scatter's own
temporaries (hashes, the partition map, a gather map, string offsets) — not a whole input higher,
which holding `combined` and the slice copies would add — and the drop against today's run is
recorded by hand in the detail file (the partitioned table, about the input's size, stays resident
after the call either way, so a before-minus-after bound cannot hold); a child of two handles is
refused. No golden moves.

## Verification bar

- device: the gtests; the full gpu tier unchanged (every answer the same); one corpus cycle at the
  tp4 modes, benchmark timings recorded before and after.

## Device workflow

`build-test-shadgpu.sh`, two cycles (gtests, then the tiers).

## Completeness signoff (2026-10-09)

Solved under its constraints: all six items of The work, the Restriction held, no golden moved,
and the handle is the listing above plus two additive accessors (`num_rows`, `num_columns`).

Two deliberate deviations. The Tests section's peak bound under-counts cuDF's strings gather by
about three times, so a string scatter's peak never moved and a formula from that wording would
have failed a correct arm; the test compares peak and requested total against partitioning the
same input alone, which discriminated on both shapes. And the name-count check sits at
`register_handle`, not in a constructor, because the fields are public and task 6's plan
assembles handles through them.

Deferred by the human host override, not by this task: the before-and-after benchmark timings,
`--run-benchmarks`, Nsight, the sf40 pair. No bandaid applied.



---

<!-- archived from llm-wiki/tasks/repartition-keys.md -->

**Merged 2026-10-16 as PR #169, chain J's task, two commits: the code `5a756452` and the board `cfd024ac`, merged into master by `7990a493`.**

# The shuffle hashes every key type the planner emits, by the rule the cpu uses

Kind: production

**This task closes [#201](../archive/archived-tickets.md#t201) (the murmur gate proves a copy of
the lane rule, not the rule), [#206](../archive/archived-tickets.md#t206) (a float or boolean
partition key is refused on the device), [#240](../archive/archived-tickets.md#t240) (a timestamp
partition key is refused on the device), [#95](../archive/archived-tickets.md#t95) (a decimal
partition key is refused on the device) and [#189](../archive/archived-tickets.md#t189) (the
shuffle cannot hash a rollup's grouping-set id).** It also makes unsigned keys shuffle on both
engines (the review's row 11; no ticket). Fourth of the join-rewrite chain: the joins
that follow shuffle on keys of these types.

## Why it happens

The device kernel's key switch (`cpp/src/spark_hash_partition.cu:144-185`) hashes STRING, INT32
and INT64, after narrowing INT8/16 and bit-casting TIMESTAMP_DAYS; every other type falls to
`CUDF_FAIL "unsupported key column cuDF type_id=N"` (`:179`). Comet's hasher, the cpu's lane rule
(`executor/cpu_backend/spark_partitioning.rs`), takes Boolean as i32, floats by their bits with
-0.0 as 0, every timestamp unit as i64, and Decimal128 by declared precision — 8 LE bytes of the
unscaled value at p ≤ 18, 16 above (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs`).
cuDF's `data_type` has no precision, and the loader widens every decimal to Decimal128, so the
kernel cannot pick comet's width itself. The gate
that proves the kernel matches comet (`cpu_backend/gpu_tests/murmur_conformance.rs`) proves it
against its own `cpu_partition_ids`, not the production `rows_per_lane` (#201). The rollup shuffle
copies DataFusion's keys, `__grouping_id: UInt8` among them, which comet has no arm for (#189).
Comet has no unsigned arm at all, nor has the kernel, so any unsigned key across a shuffle is
refused at run time on both engines; the tp1 modes, which do not shuffle, answer.

## The work

1. **#201 first.** The gate calls production `rows_per_lane` over the same columns and compares
   lane by lane; its `cpu_partition_ids` and local `pmod` go, and `pmod_handles_negative_hashes`
   tests the production `pmod`. Every new type below is proved against the real rule.
2. **#189** (corpus-fixes fix 2): `aggregate_sequence` drops the grouping id from a rollup's
   shuffle keys (`keys.retain(...)` before the shuffle, guarded on `!group.is_single()`), so it is
   never hashed. Correct because hashing a subset of the Final group's keys still puts every row
   of a group on one lane, and the validator's subset rule (`plan/aggregate.rs:236-256`) already
   admits it; the cost is a little more skew where grouping sets share visible key values. A key
   at or past `group.expr().len()` left after the drop, or an empty key list, is refused. Planner
   only; a planner test asserts `hash_keys` at tp4.
3. **Decimals hash 16 bytes on both engines** (decision D2, taken: the cpu's rule changes, not the
   wire). The lane rule must make the cpu and the device agree (`architecture.md`, "Rehash and the
   comet hash"); comet is the shared implementation of Spark's murmur3, not a promise of Spark's
   placement, and nothing reads our lane numbers from outside. So `rows_per_lane` casts each
   decimal key to `Decimal128(38, s)` before calling comet, which then always hashes the 16 LE
   bytes of the unscaled value, and the kernel hashes the same 16 bytes of the `int128` (widening
   a DECIMAL32/64 should one arrive). No wire field, no `partitioning.hpp` signature change, no FFI
   change; a decimal's lane no longer matches Spark's at p ≤ 18. A scale drift at the emit's input
   is the schema validator's to catch, where the line enables it.
4. **Kernel arms**, each red first as a live gate: Boolean (as INT32); Float32 and Float64 (their
   bits, -0.0 as +0.0, and **every NaN as one canonical bit pattern** — comet hashes a NaN by its
   bits (`utils.rs:78-105`), which would put `NaN` and `-NaN` (x86's `0/0`) in different lanes at
   tp4 while tp1 equates them on the device; so `rows_per_lane` canonicalizes NaN before calling
   comet and the kernel does the same, the decision D2 took for decimals: the engines agree, not
   Spark); Timestamp in all four units (as i64);
   Decimal128 (16 LE bytes of the unscaled `int128`, matching step 3's cpu cast). Int8, a zero-row input and an all-NULL
   key get gates too — the survey found them ungated.
4b. **Unsigned keys, one rule on both engines.** Spark has no unsigned types, so there is no
   placement to match: UInt8 and UInt16 cast to Int32, UInt32 to Int64 (value casts), UInt64
   reinterpreted as Int64 bits, then hashed as that signed type is — by `hash_keys` before comet
   and by the kernel's normalizing switch. A live gate per width, values past the signed maximum
   included.
5. **Operator harness cases** (`tests/gpu_tests/emit_cases.rs`, in-memory batches): one per key
   type, both backends placing every row in the same lane — Int8, Int16, Int32, Int64, Float32,
   Float64 (each with -0.0, 0.0, NaN, -NaN and NULL), Boolean, Date32, Timestamp in four units,
   Decimal128 at p ≤ 18 and p > 18, Utf8, UInt8, UInt16, UInt32, UInt64, a mixed composite; `emit_schema_cases.rs` the same types
   held as declared. `tests/synthetic.rs` gains `key_types(rows, seed)`, one column per hashable
   type with those special values, which join-backend's join cases reuse. The `bug_` pins for
   float, boolean (#206) and decimal (#95) flip.
6. **Timestamps on the wire.** The fbs `DataType` enum gains `TimestampSecond`,
   `TimestampMillisecond`, `TimestampMicrosecond` and `TimestampNanosecond` (an append), with the
   Rust `convert_data_type` mapping from Arrow `Timestamp(unit, _)`, their `fb_text` rendering, and
   the C++ `fb_to_type_id` arms. Today a timestamp reaches the device only because no node reads
   its type from the wire; a cast to a timestamp type (pbench's `timestamp-s-key-group`) cannot be
   written at all. join-session-cpp's pad and absent-side schemas consume these variants.
   pbench's `timestamp-s-key-group`, declared not runnable on #240, is enabled here.
   Then **an unmapped type is a `PlanError` in every schema**, never the silent `Null` of
   `serialize_schema` (`serialize.rs:136`): the plan golden says "not runnable" naming the column
   and its type ([#249](../tickets/complete-coverage.md#t249)). tpch and tpcds hold no such type, so
   no golden of theirs moves; pbench's two #249 queries are declared not runnable on it.
7. **#243's pins** (open; not fixed here): two `bug_` cases, cpu against device, over float keys
   holding -0.0, 0.0, NaN and -NaN — a float-keyed `GpuAggregate` (cpu six groups, device four)
   and a float-keyed Inner `GpuHashJoin` over one probe batch. Each asserts the divergence.

## Scope

| path | change |
|---|---|
| `cpp/src/spark_hash_partition.cu` | the arms, NaN canonicalized |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/serialize.rs`, `plan_text/`, `cpp/src` (`fb_to_type_id`) | the timestamp `DataType` variants |
| `testdata/goldens/recipe-payloads.txt` | only if a corpus payload carries a timestamp (none does today); regenerated if so |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs` | the decimal cast, the unsigned widening and the NaN canonicalization before comet |
| `peacockdb-core/src/wire/serialize.rs` and its callers in `wire/`, `wire/expr_writer.rs` | #249: an unmapped type a `PlanError` in every schema; `data_type` cites `(#249)` |
| `peacockdb-core/src/planner/tests/plan_goldens.rs` | `NOT_RUNNABLE`: `timestamp-s-key-group` leaves; the three #249 pbench queries enter |
| `peacockdb-core/src/wire/tests.rs`, `planner/translator/tests.rs`, `cpp/tests/gpu/test_plan_executor.cpp` | the timestamp types, the #249 pin, #189's planner test, the timestamp cast gtest |
| `peacockdb-core/src/planner/translator/aggregate.rs` | #189 |
| `peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs` | #201; the new gates |
| `peacockdb-core/src/tests/synthetic.rs`, `tests/gpu_tests/emit_cases.rs`, `emit_schema_cases.rs`, `aggregate_cases.rs`, `join_cases.rs` | the cases and pins |
| `cpp/tests/gpu/test_cudf.cpp` | the hardcoded comet ids, a third copy of the rule: replaced by a case read from the gate's vectors, or deleted with the reason |
| `testdata/cost-registry.csv`, `corpus_cases.inc`, `llm-wiki/` | cells, tags, counts, #95/#189/#201/#206/#240 archived |

Component-level API: the fbs `DataType` enum gains four timestamp variants (an append; no C ABI
symbol, no `partitioning.hpp` change). The cpu's lane rule changes for decimal keys (16 bytes) and
NaN float keys (one canonical NaN).

## Restriction

Key types and the lane rule only. No float equality change on the cpu: #243 stays open, reworded
by this task to the cpu's equality alone (`-0.0 ≠ 0.0` and `NaN ≠ -NaN` in DataFusion's compare
and hash), since the lane split it also described is fixed here. No join change, no change to
which keys DataFusion picks except #189's drop.

## Registry

#189's 15 cpu cells (tp4 × 3: tpch rollup_over_join; tpcds q5, q18, q22, q80) turn on, compared
with DuckDB. #95's eight rows lose `95` where it is their last ticket on a cell (most wait on
#152 too). pbench's key-type rows' tp4 device cells turn on where they pass, except the float
rows, commented out on #243; `timestamp-s-key-group` comes off `NOT_RUNNABLE` and its cells turn
on where they pass; `uint-key-group` and `uint-key-join` turn on at tp4 on both engines.

## Verification bar

- rust-only: the planner test for #189; `test_cpu_corpus` with the 15 cells and their DuckDB cases.
- device: every live gate red before its arm and green after, a NaN and a -NaN key in one lane;
  the emit cases and schema cases; the two #243 pins red as pins.
- rust-only: a timestamp cast serializes and renders; the plan golden for `timestamp-s-key-group`
  is runnable.

## Device workflow

`build-test-shadgpu.sh`; a cycle per arm group (bool and float, timestamp, decimal), then one for
the corpus cells.

## Completeness signoff (2026-10-09)

Solved under its constraints, on both engines, and **no shortcut or bandaid**: #201, #206, #240, #95 and
#189 are closed and archived, every kernel arm landed red-first under a live gate, 21 device and 24 cpu
cells are on, and the 24 left off carry a measured ticket each. #201 was the one worth having — the gate
passed with production's seed moved, because it compared the kernel against a second copy of the rule.
Three key types now depart from a counterparty by design: decimals from Spark, NaN from comet (which
restores Spark), unsigned from both; `architecture.md` carries which from which. Two Registry claims are
undelivered — the three #249 queries did not enter `NOT_RUNNABLE`, their `.sql` files being #255's, so
that step was deleted, and `uint-key-join` turned on at tp4 on the cpu only, its four device cells
measured as #152. One thing is not self-policing: nothing fails by count when `convert_data_type` gains
a member, so the next key type can land unproven on both engines; a refusal, not a wrong answer.



---

<!-- archived from llm-wiki/tasks/pbench.md -->

**Merged 2026-10-16 as PR #168, chain J's task, two commits: the code `c6a67516` and the board `f8d81ab5`, merged into master by `745bbd2d`.**

# pbench — a small dataset whose data holds what tpch and tpcds do not

Kind: production

**This task closes [#227](../tickets/corpus-coverage.md#t227)** (check schema nullability in
tests), folded in below; otherwise it closes no ticket. It adds a third corpus dataset, `pbench`, at sf1 only. Its data
deliberately holds what the two benchmarks lack: NULL join keys on both sides, NULLs under
`NOT IN`, keys of every type the shuffle must hash, duplicate and skewed keys, and empty sides.
Each in-scope ticket of the join-rewrite chain that can be shown in an answer gets a query that
shows it, registered with its cells off. Every later task in the chain turns those cells on
when its fix allows. Third of the chain, after `duckdb-oracle` and `stale-cells`, so every query is DuckDB-checked
from the start.

## Why

The rewrite's defects — #59 and #80 (NULL keys under anti, mark and `NOT IN`), the latent Left/Full
finish that drops NULL-key build rows (`join-rewrite-design.md` §5), #137's skew — are invisible
in the corpus because tpch and tpcds hold no NULL join keys where these shapes read them, and
`NOT IN` meets no NULL. Hand-written cases in `join_cases.rs` prove a node; pbench proves a query,
against DuckDB, at all five modes.

## The data

Generated by DuckDB (the pinned 1.5.4 of `duckdb_result.py`), seeded, deterministic, and
**committed** like `testdata/tpch.minimal/` — under 1 MB of parquet, so CI, shad-gpu and verda need
no generator step and no S3 entry. The generator script is committed beside it:
`testdata/pbench/gen.sql`, run by `testdata/generate_pbench.sh`, writing `testdata/pbench.sf1/`.

| table | rows | row groups | columns |
|---|---|---|---|
| `fact` | 20,000 | 10 (DuckDB rounds row groups to 2,048 rows) | `f_id` Int64; `f_k` Int32 (5% NULL, half the rows on keys 0–2, the rest spread over 0–1000); one key column per hashable type — `f_k8` Int8, `f_kb` Boolean (NULLs), `f_kf32` Float32 and `f_kf64` Float64 (NaN, -0.0, 0.0, NULL), `f_kdec15` Decimal128(15,2), `f_kdec38` Decimal128(38,4) (NULLs), `f_ts_ms`, `f_ts_us`, `f_ts_ns` Timestamp, `f_ts_s` second-valued timestamps (parquet has no second unit: queries reach `Timestamp(Second)` through `arrow_cast`), `f_dt` Date32, `f_ku32` UInt32 (values past `i32::MAX`, NULLs); `f_kstruct` Struct(a Int32, b Utf8); `f_s` Utf8 (NULLs); measures `f_qty` Int32, `f_amount` Decimal128(15,2) |
| `dim` | 2,000 | 1 | `d_id`; `d_k` Int32, every key twice (many-to-many against `fact`), 1 in 40 NULL; the same key-type columns as `fact`, its floats holding -0.0 and NaN too (so `float64-key-join` meets them), `d_ku32` and `d_kstruct` overlapping `fact`'s; `d_w` Int32; `d_name` Utf8 |
| `sub` | 200 | 1 | `s_z` Int32 (the correlation column), `s_y` Int32, 1 in 25 NULL |
| `tiny` | 8 | 1 | `t_id`, `t_k` (two distinct keys), `t_v`, `t_pat` Utf8 (`LIKE` patterns over `d_name`: `'name1%'`, `'%7'`, …) |
| `empty` | 0 | 1 | `tiny`'s schema |

A prototype of exactly this generator was run (`gen.sql` in the helper's scratchpad, reproduced
as the task's starting point) and every query below planned on DataFusion 45 at four partitions.

**The small-table rule is off for pbench.** `PlanKnobs.small_table_bytes` is 5 MB for every mode
(`test_support/mod.rs:159`), which would plan every pbench scan as one lane. pbench plans with it
at 0 through a per-dataset value; tpch and tpcds keep 5 MB. So `fact` splits across lanes at tp4,
and the one-row-group tables leave three lanes empty — the empty-lane shapes (#173, #212) for free.

## The queries

`testdata/pbench-queries/<name>.sql`, one per row. "Plans as" is DataFusion 45's physical join (or
the shuffle) at four partitions, measured on the prototype; the task confirms each from its plan
golden and replaces a query whose plan drifts. "Off on" is the expected blocking tickets; the run
decides (see Registry).

### Shuffle key types — repartition-keys (#206, #240, #95, #189)

| name | SQL | off on |
|---|---|---|
| float64-key-group | `SELECT f_kf64, count(*) AS n FROM fact GROUP BY f_kf64` | commented out: #243; gpu tp4: #206 |
| float32-key-group | `SELECT f_kf32, count(*) AS n FROM fact GROUP BY f_kf32` | commented out: #243; gpu tp4: #206 |
| bool-key-group | `SELECT f_kb, count(*) AS n FROM fact GROUP BY f_kb` | gpu tp4: #206 |
| int8-key-group | `SELECT f_k8, count(*) AS n FROM fact GROUP BY f_k8` | none expected: every cell on if it passes; a failing cell gets its ticket (the registry rule allows no off cell without one) |
| timestamp-ms-key-group | `SELECT f_ts_ms, count(*) AS n FROM fact GROUP BY f_ts_ms` | gpu tp4: #240 |
| timestamp-us-key-group | `SELECT f_ts_us, count(*) AS n FROM fact GROUP BY f_ts_us` | gpu tp4: #240 |
| timestamp-ns-key-group | `SELECT f_ts_ns, count(*) AS n FROM fact GROUP BY f_ts_ns` | gpu tp4: #240 |
| timestamp-s-key-group | `SELECT arrow_cast(f_ts_s, 'Timestamp(Second, None)') AS ts, count(*) AS n FROM fact GROUP BY 1` | not runnable on #240: the cast to `Timestamp(Second)` cannot cross the wire until repartition-keys adds the fbs timestamp types — declared in `NOT_RUNNABLE` here, enabled there |
| decimal15-key-group | `SELECT f_kdec15, count(*) AS n FROM fact GROUP BY f_kdec15` | gpu tp4: #95 |
| decimal38-key-group | `SELECT f_kdec38, count(*) AS n FROM fact GROUP BY f_kdec38` | gpu tp4: #95 |
| float64-key-join | `SELECT f_id, d_id FROM fact JOIN dim ON f_kf64 = d_kf64` | commented out: #243; gpu: #152, tp4 #206 |
| decimal15-key-join | `SELECT f_id, d_id FROM fact JOIN dim ON f_kdec15 = d_kdec15` | gpu: #152, tp4 #95 |
| rollup-small-keys | `SELECT f_k8, f_kb, sum(f_qty) AS q FROM fact GROUP BY ROLLUP (f_k8, f_kb)` | cpu and gpu tp4: #189 |
| uint-key-group | `SELECT f_ku32, count(*) AS n FROM fact GROUP BY f_ku32` | cpu and gpu tp4: folded into repartition-keys (the review's row 11: comet and the kernel have no unsigned arm), tagged `189` until then — #189 is the same missing unsigned arm, met through the grouping id, and repartition-keys closes both; to confirm from the plan golden |
| uint-key-join | `SELECT f_id, d_id FROM fact JOIN dim ON f_ku32 = d_ku32` | cpu and gpu tp4: the same; gpu: #152 |

The three float rows hold `-0.0` and NaNs of both signs, where the cpu answers differently from
DuckDB and the device (#243, open, not fixed in this chain). Each lands with its SQL file and its
goldens, and its `corpus_query!` line commented out with `// #243 — the cpu keys -0.0 and NaN by
their bits` above it, so the line is enabled by the task that fixes #243. Its registry row carries
`243` and every cell off.

### Hash joins — join-session-cpp and join-backend

| name | SQL | plans as | off on |
|---|---|---|---|
| inner-join-hot-keys | `SELECT f_id, d_id FROM fact JOIN dim ON f_k = d_k` | Inner | gpu: #152, #220 |
| left-join-null-keys | `SELECT d_id, d_k, f_id FROM dim LEFT JOIN fact ON d_k = f_k` | Left | gpu: #152 (and the latent finish defect behind it) |
| full-join-null-keys | `SELECT d_id, d_k, f_id FROM dim FULL JOIN fact ON d_k = f_k` | Full | gpu: #152 (the same) |
| right-join-null-keys | `SELECT d_id, f_id, f_k FROM dim RIGHT JOIN fact ON d_k = f_k` | Right | gpu: #152 |
| left-join-residual | `SELECT d_id, f_id FROM dim LEFT JOIN fact ON d_k = f_k AND f_qty > d_w` | Left + filter | both: #153 |
| right-join-residual | `SELECT d_id, f_id FROM dim RIGHT JOIN fact ON d_k = f_k AND f_qty > d_w` | Right + filter | both: #153 |
| full-join-residual | `SELECT d_id, f_id FROM dim FULL JOIN fact ON d_k = f_k AND f_qty > d_w` | Full + filter | both: #153 |
| exists-null-keys | `SELECT d_id FROM dim WHERE EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` | LeftSemi | gpu: #152 |
| not-exists-null-keys | `SELECT d_id FROM dim WHERE NOT EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` | LeftAnti | both: #59 |
| exists-or-mark | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM fact WHERE f_k = d_k)` | LeftMark | both: #59 |
| probe-exists-residual | `SELECT f_id FROM fact WHERE EXISTS (SELECT 1 FROM dim WHERE d_k = f_k AND d_w < f_qty)` | RightSemi + filter | both: #159 |
| probe-not-exists-residual | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM dim WHERE d_k = f_k AND d_w < f_qty)` | RightAnti + filter | both: #159 |
| anti-null-preserved-condition | `SELECT d_id FROM dim WHERE NOT EXISTS (SELECT 1 FROM fact WHERE f_k = d_k AND d_kb)` | LeftAnti + build-only filter | both: #59 (a NULL `d_kb` keeps the row) |
| not-in-uncorrelated | `SELECT f_id FROM fact WHERE f_k NOT IN (SELECT s_y FROM sub)` | RightAnti | both: #80, #59 |
| not-in-correlated | `SELECT f_id FROM fact WHERE f_k NOT IN (SELECT s_y FROM sub WHERE s_z = f_qty)` | RightAnti, two keys | both: #80, #59 |
| not-in-under-or | `SELECT f_id FROM fact WHERE f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub WHERE s_z = f_qty)` | LeftMark, two keys | both: #80, #59 |
| not-not-in | `SELECT f_id FROM fact WHERE NOT (f_k NOT IN (SELECT s_y FROM sub))` | after the rule's NNF fold, a semi join for `f_k IN (...)` | both: #80 |
| not-or-not-in | `SELECT f_id FROM fact WHERE NOT (f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub))` | after NNF, `f_qty <> 0 AND f_k IN (...)` | both: #80 |
| in-is-null | `SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL` | refused at plan time: the `IN`'s NULL is read | both: #250 |
| sparse-probe-left | `SELECT d_id, t_id FROM dim LEFT JOIN tiny ON d_k = t_k` | Right (build `tiny`) | gpu tp4: #152 |
| sparse-probe-semi | `SELECT d_id FROM dim WHERE d_k IN (SELECT t_k FROM tiny)` | RightSemi | gpu: #152 |
| finish-without-probe | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM tiny WHERE t_k = d_k)` | LeftMark (never swapped) over the two-key `tiny`: two probe lanes with no batch | gpu tp4: #173 |
| sparse-build-right | `SELECT t_id, f_id FROM tiny RIGHT JOIN fact ON t_k = f_k` | Right, build lanes with no batch | both tp4: #212 |
| sparse-build-full | `SELECT t_id, f_id FROM tiny FULL JOIN fact ON t_k = f_k` | Full, the same | both tp4: #212 |
| sparse-build-anti | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM tiny WHERE t_k = f_k)` | RightAnti, the same | both tp4: #212 |
| ts-key-join | `SELECT f_id, d_id FROM fact JOIN dim ON f_ts_us = d_ts_us` | Inner on a timestamp key (review row 28), to confirm from the plan golden | gpu: #152; gpu tp4: #240 |
| mark-cross-residual | `SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM fact WHERE f_k = d_k AND f_qty > d_w)` | LeftMark + cross filter (review row 29), to confirm from the plan golden | both: #59 |
| empty-side-left-join | `SELECT t.t_id, d.d_id FROM tiny t LEFT JOIN (SELECT * FROM dim WHERE false) d ON t.t_k = d.d_k` | Left over an `EmptyExec` (review row 8), to confirm from the plan golden | both: `155` — no arm for `EmptyExec` until join-backend's `GpuEmpty` |
| indf-full-join | `SELECT d.d_id, t.t_id FROM dim d FULL JOIN tiny t ON d.d_k IS NOT DISTINCT FROM t.t_k` | NLJ Full over `IS NOT DISTINCT FROM` (review row 9), to confirm from the plan golden; join-backend makes it a null-equal hash join | both: #160 (gpu also the AST's missing `NULL_EQUAL`, `155`) |

### Nested loops and cross joins — #160, #215, #63, #190, #207, #208

| name | SQL | plans as | off on |
|---|---|---|---|
| nl-inner | `SELECT f_id, t_id FROM fact JOIN tiny ON f_qty < t_v` | NLJ Inner | gpu: #152 |
| nl-left | `SELECT f_id, t_id FROM fact RIGHT JOIN tiny ON f_qty < t_v` | NLJ Left | gpu: #152 |
| nl-right | `SELECT f_id, t_id FROM tiny RIGHT JOIN fact ON f_qty < t_v` | NLJ Right | both: #160 |
| nl-full | `SELECT f_id, t_id FROM fact FULL JOIN tiny ON f_qty < t_v` | NLJ Full | both: #160 |
| nl-left-semi | `SELECT t_id FROM tiny WHERE EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` | NLJ LeftSemi | both: #160 |
| nl-left-anti | `SELECT t_id FROM tiny WHERE NOT EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` | NLJ LeftAnti | both: #160 |
| nl-right-semi | `SELECT f_id FROM fact WHERE EXISTS (SELECT 1 FROM tiny WHERE t_v > f_qty)` | NLJ RightSemi | both: #160 |
| nl-right-anti | `SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM tiny WHERE t_v > f_qty)` | NLJ RightAnti | both: #160 |
| nl-mark | `SELECT t_id FROM tiny WHERE t_k = 0 OR EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)` | NLJ LeftMark | both: #160 |
| nl-left-decimal | `SELECT f_id, t_id FROM fact RIGHT JOIN tiny ON CAST(t_v AS DECIMAL(20,0)) > CAST(f_qty AS DECIMAL(20,0))` | NLJ Left, non-AST | gpu: #215 |
| nl-projection | `SELECT f_id FROM fact JOIN tiny ON f_qty < t_v` | NLJ Inner, narrowed | cpu: #190; gpu: #152 |
| cross-projection | `SELECT f_id FROM fact, tiny` | Cross, narrowed | both: #207 |
| cross-empty-build | `SELECT e.t_id, t.t_id FROM empty e, tiny t` | Cross over a zero-row build | cpu: #208 |
| scalar-subquery-cross | `SELECT CASE WHEN (SELECT count(*) FROM fact WHERE f_qty BETWEEN 1 AND 20) > 100 THEN (SELECT avg(f_amount) FROM fact WHERE f_qty BETWEEN 1 AND 20) ELSE (SELECT avg(f_amount) FROM fact WHERE f_qty BETWEEN 21 AND 40) END AS b1, (SELECT count(*) FROM dim WHERE d_w >= 0) AS b2 FROM tiny WHERE t_id = 1` | predicate-free NLJs over one-row aggregates (tpcds q9's shape) | gpu: #63 (the filter keeps DataFusion from answering an unfiltered `count(*)` from statistics with a `PlaceholderRowExec`, refused on #158) |
| outer-on-true-empty | `SELECT t.t_id, e.t_id AS e_id FROM tiny t LEFT JOIN empty e ON true` | predicate-free NLJ Right over a zero-row build | both: #160 (and, once lifted, the cross-join mapping the design replaces) |

### Complete join coverage — shapes that stay open after chain J

Each carries a ticket of `joins.md`'s "Complete Join Coverage" section; the task that fixes the
ticket turns the cells on.

| name | SQL | plans as | off on |
|---|---|---|---|
| struct-key-join | `SELECT f_id, d_id FROM fact JOIN dim ON f_kstruct = d_kstruct` | Inner on a struct key, to confirm from the plan golden | all cells: #245 (no hasher arm for a nested type) and #249 (the wire names no struct type; not runnable from repartition-keys' Task 5c on) |
| like-column-pattern | `SELECT d_id, t_id FROM dim JOIN tiny ON d_name LIKE t_pat` | NLJ Inner over a column pattern, to confirm from the plan golden | gpu: #246 |
| interval-through-join | `SELECT d_id, t.iv FROM dim LEFT JOIN (SELECT t_k, INTERVAL '1' DAY AS iv FROM tiny) t ON d_k = t_k` | Left, carrying an interval column the join pads — confirm from the plan golden that `iv` crosses the join; if DataFusion hoists the literal above it, pick another construction (a `TIME` column on `tiny`, padded the same way) | not runnable: #249 (from repartition-keys on; before it, the silent `Null`) |
| struct-through-join | `SELECT d_id, d_kstruct FROM dim JOIN tiny ON d_k = t_k` | Inner, carrying a struct column | not runnable: #249 |

### Collapse to one lane — #140's cost, made visible

Each runs over a four-lane `fact` (the small-table rule is off), and its plan golden shows the
`GpuMergePartitions` that puts the join in one lane.

| name | SQL | why one lane |
|---|---|---|
| collapse-not-in | `not-in-uncorrelated` above, after the rewrite | the rewrite's joins against one-row counts |
| collapse-nested-loop | `nl-inner` above | a keyless join merges both sides |
| collapse-cross | `cross-projection` above | the same |
| collapse-collect-left | a `fact` join that DataFusion plans `CollectLeft`, if any shape here gets one | `CollectLeft` is merged, not broadcast (#140) |

These reuse queries above rather than add new ones; the spec names them so the benchmark and the
plan goldens are read for the merge. `collapse-collect-left` is a query only if the task finds a
pbench join DataFusion plans `CollectLeft`; otherwise it is recorded as not reached.

### Not covered by a query

#136, #145, #154, #197 and #201 change cost or tests, not answers. #235 is the oracle itself.

## The work

1. `testdata/pbench/gen.sql` and `testdata/generate_pbench.sh`; `testdata/pbench.sf1/*.parquet`
   committed; a determinism test regenerating into a temp dir and comparing row content with the
   committed files (not parquet bytes). The generator writes single-threaded and ordered, and puts
   -0.0 and NaN into `dim`'s floats and -NaN into `fact`'s, beside the specials of the table above.
2. `testdata/pbench-queries/*.sql`, the queries above.
3. Plumbing, wherever a dataset is named today (`git grep -lE '\btpcds\b'` outside goldens and
   wiki, about 30 files): `test_support` (the per-dataset `small_table_bytes`; `golden_dir_for`),
   `tests/common/corpus_cases.inc` (a `// --- pbench ---` section), `test_corpus_goldens.rs`'s five
   dataset lists, `test_cost_model.rs`, `cost-registry.csv` rows, `cost-report/src/main.rs` (the
   widget renders a pbench section beside TPC-H and TPC-DS, its render test gaining pbench; and a
   mode cell whose five modes are all enabled shows one larger tick instead of five small ones —
   `✔` in the PR comment, a larger `✓` on the HTML page — so a fully covered row reads at a glance),
   `scripts/exec_model` corpus loaders, the CI workflows' dataset matrix, `build-test.sh` and
   `build-test-shadgpu.sh` (pbench needs no generation and no upload), `dataset_checks.py` and
   the S3 checks (pbench is not in S3: skipped by name).
4. Goldens: `testdata/goldens/pbench.sf1/` — the five modes' plans, cpu and cost goldens,
   `mini.result.txt`, `duckdb-result.txt` (from `duckdb_result.py --dataset pbench`), and
   `gpu-result.txt` per duckdb-oracle.
5. One device cycle: every gpu cell run once; the cells that pass are enabled.

## Nullability is checked (#227)

Folded in from [#227](../tickets/corpus-coverage.md#t227) (check schema nullability in tests), so
the NULL-heavy data meets a check from its first run and the join rewrite after it is held to its
declarations. Today the schema checks compare types only.
- **cpu:** `declared_as` (`executor/cpu_backend/mod.rs:309`) refuses a batch whose column holds a
  NULL where its field is declared non-nullable, naming the column — before its early return for an
  equal schema, which today skips every check.
- **device:** the schema validators (`test_support/schema_validation.rs`, `gpu_schema_validator`
  and `cpu_schema_validator`) check the same against each handle's null counts, wherever a line's
  `schema_validation` is enabled.
- A violation found in tpch, tpcds or pbench is a ticket, not a fix here; its cells go off on it.
  #227 closes when both checks land.

## Scope

| path | change |
|---|---|
| `testdata/pbench/`, `testdata/generate_pbench.sh`, `testdata/pbench.sf1/` | the generator and the committed data |
| `testdata/pbench-queries/` | the queries |
| `testdata/goldens/pbench.sf1/` | every golden kind for pbench |
| `testdata/cost-registry.csv`, `peacockdb-core/tests/common/corpus_cases.inc` | the rows and lines |
| `peacockdb-core/src/test_support/` | the dataset, its `small_table_bytes` |
| `peacockdb-core/tests/test_corpus_goldens.rs`, `test_cost_model.rs` | pbench in the dataset lists |
| `cost-report/src/main.rs` | the pbench section |
| `testdata/duckdb_result.py` | `--dataset pbench`; a one-entry spelling map rewriting DataFusion's `arrow_cast(…, 'Timestamp(Second, None)')` as DuckDB's `::TIMESTAMP_S` |
| `scripts/` (build-test, exec_model corpus, dataset checks) | pbench where datasets are listed |
| `.github/workflows/` | pbench in the matrix, no generation step |
| `peacockdb-core/src/executor/cpu_backend/mod.rs`, `src/test_support/schema_validation.rs` | #227: a NULL where the node declares none is refused (cpu) and reported (validators) |
| `peacockdb-core/src/wire/expr_writer.rs` | a wire-type refusal cites its ticket (`(#240)` for a timestamp target, `(#249)` otherwise), so the plan goldens' meta tests accept the not-runnable lines |
| `peacockdb-core/src/planner/tests/plan_goldens.rs` | pbench in `CORPUS_DATASETS`; `NOT_RUNNABLE` gains `timestamp-s-key-group` on #240 |
| `cost-report/src/main.rs` | one larger tick for a fully enabled row |
| `llm-wiki/build-test.md`, `architecture.md` | the third dataset; counts |

Component-level API: none. Two engine changes, each a refusal that changes no answer: #227's
nullability check (`declared_as` and the schema validators), and a wire-type refusal's text naming
its ticket. The planner otherwise sees a per-dataset knob value it already takes.

## Restriction

Test data and plumbing, plus the two refusals above (#227's check, the ticket in a wire-type
refusal's text). No fix to anything a query shows; a query that fails gets its ticket on the row.
sf1 only, no scaling, no S3.

## Registry

Every pbench row lands with its tickets in the `tickets` column. cpu cells on where the cpu
answers and agrees with DuckDB; every gpu cell run once, enabled if it passes. Each row's
`duckdb_oracle` is `duckdb_exact` unless a ticket on the row says the engines diverge from
DuckDB (`duckdb_divergent(<ticket>, <positions>)`), and `duckdb_none` where the cpu does not answer
(a refused or not-runnable row) — the task that first turns its cells on moves it to its variant. A cell that fails on a ticket the table above does not
expect is a finding: the ticket it fails on goes on the row, or a new ticket is filed.

## Verification bar

- rust-only: `test_cpu_corpus` (registry ↔ CSV both ways, `every_device_cell_has_a_cpu_cell`),
  `test_corpus_goldens`, the determinism test, the widget's render test.
- device: one shad-gpu cycle over the pbench rows.
- The duckdb-oracle comparison over pbench, every row it marks exact passing.

## Device workflow

`build-test-shadgpu.sh`, one cycle for the gpu cells.

## Completeness signoff (2026-10-08, replacing the pre-cycle one)

Solved under its constraints, on both engines: 588 KB committed, 57 corpus lines, 60 registry rows, 128
cpu cells on, and the device cycle run on an L40S at cuDF 25.02 — 128 of 285 declarable gpu cells
measured, 30 enabled, 98 off on a measured ticket, `gpu-result.txt` golden 18 of 18 with its 30 sections
all agreeing with DuckDB. Five shortcuts. #227 closes half: the device null-count read has no plumbing.
The determinism check sits in no Rust tier; this task's guard proves only that its `pipeline.yml` line
exists. Three of the 63 queries are absent behind #255, leaving #245 and #249 no declared cell. Three of
the 24 refusals cite no ticket, and citing them is a third engine change the Restriction forbids.
`empty.parquet` holds zero row groups where the data table says one, so `cross-empty-build` is refused
on #256 instead of reaching #208. Two claims this frozen text cannot correct: a query per demonstrable
ticket misses eight, each now saying so on itself, and "Not covered by a query" omits #137.



---

<!-- archived from llm-wiki/tasks/duckdb-oracle.md -->

**Merged 2026-10-16 as PR #167, chain J's task, two commits: the code `f1fd9d43` and the board `bca5f8d0`, merged into master by `3a0c5d10`.**

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
it is [#263](archived-tickets.md#t263) with a by-hand interim. Three shortfalls stand as named:
a decimal column against DuckDB's double cannot be approximate on both sides (#253, with the two
other paths that cannot record a divergence); the negative tests miss `data_fusion_subset` (#254);
`duckdb_columns` was looked for and not needed, no `duckdb_divergent` line being a LIMIT tie.



---

<!-- archived from llm-wiki/tasks/gpu-host.md -->

**Merged 2026-10-15 as PR #180: one squashed commit, `1c4e6a2e`, merged into master by `d2b9daf8`.
A helper-session task, never on the board; merged before its last CI run finished.**

# The GPU host is a VM the tools start, use and stop

Kind: production

shad-gpu will be down for an extended time. The device tier moves to nebius-gpu, an L40S VM
that costs money while it runs. Today it is started and stopped by hand
(`dev:~/stop-nebius-gpu.sh`), its IP changes on every start, and chain J stalled on a changed host
key. This task makes the VM something the ensemble and CI bring up for a GPU step and take
down after it. Two independent guards stop it when nobody does.

Implemented in a helper session with subagents, on a branch with a PR. Not an ensemble task.

## The host

- Instance `computeinstance-e00tnrse7ayntnzcyt`, project `project-e00tdypyen10mmyep61c8q`. The
  boot disk `computedisk-e00h63n47zhs9h6gcw` survives a stop. NVIDIA L40S, 46 GB; Ubuntu 24.04,
  glibc 2.39; cuDF 25.02 in `~/data/miniforge3/envs/rapids-cuda-12.2`.
- The IP is ephemeral. The VM is preemptible: it can stop, change IP or vanish mid-step.
- Only what fits an L40S runs there. The sf40 binaries (`peacock_tpch_tests`,
  `peacock_tpchv_tests`), `--run-benchmarks` and Nsight captures are out until further notice.

## The work

### 1. `scripts/gpu-host.sh` — the one door to the VM

Replaces `dev:~/stop-nebius-gpu.sh`. The instance ID is a constant in the script. It uses the
`nebius` CLI with whatever profile the caller has.

- `status`: prints the state, and the IP when the instance is RUNNING.
- `start`: starts the instance, waits for RUNNING, then for ssh. Idempotent: on a running
  instance it only waits for ssh.
- `stop [--force]`: as the old script. A soft stop refuses while a process is on the card
  (exit 2); an unreachable host is stopped on the API's word. `--force` stops regardless.
- `ssh <cmd…>`: runs a command on the VM. `ip` prints the current IP.
- `install-watch`: installs the idle watcher (section 4) and its credentials on the VM.

Exit codes: 0 done; 2 card busy (soft stop); 75 no capacity or quota, so retry later; 69 the
instance no longer exists; 1 anything else.

**No cached IP.** Every call resolves the IP through the API. No ssh config entry is edited.
**Host keys:** every ssh uses `HostKeyAlias=nebius-gpu` with its own known-hosts file, so a new
IP never warns. When the key itself changes, the script checks through the API that the IP
belongs to this instance, replaces the stored key, and logs both fingerprints. That check is the
trust anchor, so no agent ever has to accept a key by hand.

### 2. `scripts/gpu-host-sync.sh` — the tree to the VM

Chain J's rsync, scripted: the working tree, uncommitted, to `~/peacockdb-<name>` on the VM.
`--delete-after`, `--exclude=.git`, `--filter=':- .gitignore'`, so ignored build dirs, target
dirs and generated data survive on both sides. Retries as `resilient_rsync` does. It generates
the sf1 tpch and tpcds data on the VM if it is missing.

### 3. `scripts/gpu-host-test.sh` — the L40S test list, detached

Runs on the VM, against a tree that holds `cpp/install/`. The list is chain J's: every
`peacock_*_tests` except the sf40 pair, and the staged rust binaries with `--test-threads=1`,
`peacockdb_core_gpu_lib` filtered to `gpu_tests::`, and `peacock_gpu_benchmarks --skip bench_`.
It runs detached under `nohup`, with a log and an rc file per binary, so a dropped ssh does not
kill it. `--status` reports which binaries are done, running or failed. It keeps the existing zero-tests
guards. The developer and CI both use it, so there is one list.

### 4. The idle watcher, on the VM

A systemd timer, enabled, firing every minute. It survives reboots. Each tick appends one line to
`/var/log/peacock-gpu-idle.log`: time, boot time, the card's processes with their start times,
build processes seen, and the decision. State lives in `/var/lib/peacock-gpu-idle/`.

- **Test activity:** any process on the card (`nvidia-smi`), or a running test binary from
  section 3's list.
- **Build activity:** `rsync`, `cargo`, `cmake`, `ninja`, `nvcc` or `cc1plus` running.
- **Idle stop:** no test activity for 1 hour, or 2 hours if build activity was seen in that
  time.
- **Hung-run cap:** the same process on the card for 4 hours.
- The clocks start at boot at the earliest, so a fresh start is never stopped on old state.
- It stops the instance through the API with `nebius compute instance stop`. It does not shut
  down the guest. The credentials are the same service account's, installed root-only by
  `install-watch`.

### 5. `.github/workflows/gpu-host-watch.yml` — the hourly checker

Scheduled hourly (`0 * * * *`) and dispatchable. It keeps no state. If the instance is RUNNING,
it reads the watcher's log over ssh and **hard-stops** on any of these:

- the log's last line is more than an hour old (the watcher is dead);
- the log shows the idle rule or the hung-run cap met;
- ssh does not answer for a 15-minute retry window.

### 6. The GPU job in `pipeline.yml`

- Triggers: `workflow_dispatch` is added. `gpu-tests` runs on a push to master and on a
  dispatch, never on `pull_request`. A PR still runs the build job.
- **Who can run it.** A dispatch needs write access. The job's first step fails unless
  `github.actor` is in the repo variable `GPU_ALLOWED_ACTORS`. The secrets sit in a GitHub
  Environment `nebius-gpu`, which only the GPU jobs reference. A fork PR gets no secrets and
  no GPU job.
- **Secrets**, placeholders until the human fills them: `NEBIUS_SA_ID`, `NEBIUS_PUBLIC_KEY_ID`,
  `NEBIUS_PROJECT_ID`, `NEBIUS_PRIVATE_KEY`, and `NEBIUS_GPU_SSH_KEY` for the VM login.
- **Steps:** install the nebius CLI; `gpu-host.sh start`, retried for up to 30 minutes, then
  fail; ship `cpp/install/` and the testdata the job ships today to a per-run directory; run
  `gpu-host-test.sh` there; always remove the run directory and soft-stop.
- The concurrency group becomes `nebius-gpu`. The glibc patch step and the sf40 precondition
  go: CI's binaries need glibc 2.35 and the VM has 2.39.

### 7. The ensemble's lifecycle

- **Every GPU step names its bring-up and its stop.** A spec or `-impl.md` step that builds for
  or runs on the device says: `gpu-host.sh start` → `gpu-host-sync.sh` → build on the VM →
  `gpu-host-test.sh` (or a named subset) → soft stop. The developer runs it.
- **A lost host mid-step** (ssh fails, the VM is preempted or moves): `status`, `start` again
  under the retry rule below, sync, rerun the step. Exit 69 (the instance is gone) is a stall:
  rebuilding the disk, the env and the data is the human's.
- **Start failures:** on exit 75 the coordinator retries every 15 minutes for 10 hours, then
  writes `stalled: gpu host unavailable` and exits.
- **Hard stops by the coordinator:** on entering `completeness approved`, on entering `done`, and on
  every exit (stalled, tight window, `stop`). Everyone else stops softly.
- **The final run is CI's.** In `completeness approved`, after its hard stop, the coordinator
  dispatches `gh workflow run pipeline.yml --ref <branch>` and waits for that run. `done` needs
  its `gpu-tests` green as well as the PR's CI. The gh account the coordinator runs as must be
  in `GPU_ALLOWED_ACTORS`.
- **No agent ever opens a Nebius credential file.** That means `~/.nebius/config.yaml`,
  `~/.nebius/credentials.yaml` and any `*.pem` under `~/.nebius/`, on any host, including the
  VM's copy. No `cat`, `grep`, `sed`, Read, or redaction attempt either: a redaction leaked the
  private key into a transcript on 2026-10-14. Agents use the CLI through `gpu-host.sh` only. The
  scripts never print these files and never echo the CLI's config. The rule goes into
  `prompts.md`'s Shared rules, so every role carries it.
- **Contention is accepted.** Two chains and CI share one VM. One party's stop can kill another's
  run, and the loser reruns its step.

## Scope

| path | change |
|---|---|
| `scripts/gpu-host.sh`, `scripts/gpu-host-sync.sh`, `scripts/gpu-host-test.sh` | new |
| `scripts/gpu-host/` | the watcher script, its systemd service and timer units |
| `scripts/tests/` (or beside the scripts) | the fake-`nebius` and watcher-rule tests |
| `.github/workflows/pipeline.yml` | `workflow_dispatch`; `gpu-tests` moved to nebius-gpu |
| `.github/workflows/gpu-host-watch.yml` | new |
| `llm-wiki/prompts.md` | Coordinator and Developer: section 7; the verda/shad-gpu GPU wording |
| `.claude/commands/ensemble.md` | wherever it routes GPU work |
| `llm-wiki/build-test.md` | a "GPU host" section in place of shad-gpu routing; Remote hosts; CI structure |
| `llm-wiki/tasks/tasks.md` | chains J and K's host overrides shrink to a pointer to that section |

Component-level API: none. No facade, trait, ABI symbol or wire change.

## Restriction

The engine is untouched. `scripts/build-test-shadgpu.sh` stays as it is. Its `--build` is what
runs on the VM, and its shad-gpu `--run` and benchmark paths are left alone. Recreating a
terminated instance, cuDF 26.02 on the VM, sf40 data and benchmarks are out. No lock between
parties sharing the VM.

## Tests

- `shellcheck` clean on every new script.
- `gpu-host.sh` against a fake `nebius` on `PATH` and a fake `ssh`: start, stop, soft stop
  refused while busy, `--force`, status with and without an IP, exit 75 on a capacity error,
  exit 69 on a missing instance, the host-key replacement when the API vouches for the IP, and a
  refusal when it does not.
- The watcher's decision as a function of a fake process list and clock: idle 1 h, build 2 h,
  the 4 h cap, and a fresh boot over old state not stopping.
- The checker's decision over sample logs: stale log, idle, cap, unreachable.
- The actor check: an actor not in the list fails the step (run with a fake `github.actor`).

## Verification bar

By hand, once, recorded in the PR:

- a real `start`, `status`, `ssh`, soft and hard `stop`;
- `install-watch`, a reboot, and the timer still firing afterwards;
- the watcher's API stop leaving the instance STOPPED (shortened thresholds);
- the checker's decision script run by hand against the running, idle VM, stopping it;
- `pipeline.yml` dispatched on this PR's branch: `gpu-tests` green on nebius-gpu, the VM
  stopped afterwards;
- a dispatch by an actor not in `GPU_ALLOWED_ACTORS` failing at the check step.

GitHub reads a scheduled workflow only from master, and only dispatches a workflow whose file
master already has. So `gpu-host-watch.yml` is verified after the merge: one dispatch, and one
scheduled run seen in the Actions log. If `pipeline.yml` cannot be dispatched on the PR branch
before master has the trigger, a one-line commit adding `workflow_dispatch` lands on master
first. No job would run on the dispatch until this branch's `gpu-tests` exists.

---

<!-- archived from llm-wiki/tasks/date-part-return-type.md -->

**Merged 2026-10-07 as PR #161: one squashed commit, `5a1eac45`, merged into master by `c936f31b`.
Last of chain B, the schema-divergence chain; the chain leaves the board with it.**

# `date_part` answers in the type the wire names

Kind: production

**This task closes [#191](../archive/archived-tickets.md#t191)** — `extract(year …)` is `Int32` in DataFusion
and on the CPU, INT16 from cuDF, and the device hands the INT16 up unchanged. Fourth of chain B.

## Why it happens

`expr.cpp:641-660`'s `date_part` arm returns `cudf::datetime::extract_datetime_component`'s
column as it comes — INT16 for every field — although the wire already says what to produce:
`ScalarFunctionExprNode.return_type` (`flatbuffers/gpu_plan.fbs:204`, "used by the
column-producing executor to allocate the result column"). In `tpch/q8` the year is born
`Int16` at the bottom `GpuProject`, rides as a group key through eight nodes that all declare
`Int32` and none of which look, and is refused at the sink because the query projects it. A
query that grouped by year without outputting it would run the whole plan unseen.

This is the opposite of [#187](active-tickets.md#t187): there the device *widens* a label it
cannot hold; here it *narrows* a value it was told the width of. And unlike
[#163](../tickets.md#t163) the declaration is right — `extract` is `Int32` to the user — so the
producer is fixed, not the label.

## The work

1. **Fix.** The `date_part` arm ends with `cudf::cast(component, cudf::data_type{fb_to_type_id(
   node->return_type())})` when the component's type differs from the declared one. All six
   fields go through the same line. A `return_type` that is not an integer type is a
   `runtime_error` naming the function — the wire promised an allocation type, not a request.
2. **Tests, three levels.**
   - `cpp/tests/gpu/test_plan_executor.cpp`: `tpch.minimal` has no date column, so a
     `CudfProject` casts `nation`'s `n_nationkey` to `Date32` first; above it a `CudfProject`
     with one `ScalarFunctionExprNode {name: "date_part", args: ["YEAR", col 0], return_type:
     Int32}`, asserting the output column's `type().id() == INT32` and the values; the same
     shape for `MONTH` and `DAY`. Red before the fix.
   - `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`: a `GpuProject` over the file's
     `input()` — `synthetic::schema()` carries `d: Date32` at index 6 — with `date_part(YEAR,
     d) as y` declared `Int32`, `run_both(...).same(Order::AsEmitted)`. Written first as the
     `bug_` pin whose refusal names `y: Int32 vs Int16`, then flipped to the green case by the
     fix. The green case's name says what it proves; the pin's name goes with the ticket.
   - The corpus: `tpch/q7`, `q8`, `q9` at `tp1_single`. `q7` and `q9` gain `191` in the registry
     first — the survey (§3) showed the class on them without the ticket.
3. **The neighbours, looked at and not fixed.** Every other arm of `expr.cpp`'s scalar dispatch
   is read against its `return_type`; a mismatch found is a ticket with the query or case that
   reaches it, not a second fix in this task. The detail file lists each arm and its verdict.

## Scope

| file | change |
|---|---|
| `cpp/src/expr.cpp` | the cast at the end of the `date_part` arm |
| `cpp/tests/gpu/test_plan_executor.cpp` | three cases |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the pin, then the green case |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | `q7`, `q9` gain `191`; the three rows' `tp1_single` gpu cells |
| `llm-wiki/tasks/active-tickets.md`, `tickets.md`, `build-test.md` | #191 closed; any neighbour tickets; counts |

Component-level API: none. No wire change, no ABI change, no Rust production code.

## Restriction

`date_part` alone. No change to how the planner emits the function, no Rust-side cast, no
change to any other scalar arm. No golden moves: the plan already declares `Int32`.

## Registry

`tpch/q7`, `q8`, `q9` at `tp1_single`; the other modes for those that pass. Each is enabled
where values match, or keeps its next ticket; `191` is struck from a row only when no disabled
cell in it is left without a ticket (`registry.rs:229-240`).

## Verification bar

- C++: the three plan-executor cases red before, green after, on a device.
- device: the harness with the case green and the pin gone; the three rows.
- rust-only: `--lib` and `test_module_layout` unchanged.

## Device workflow

`build-test-shadgpu.sh`, one cycle.

## Completeness signoff — 2026-09-17

Solved under its constraints: the `date_part` arm alone changed, casting cuDF's INT16
component to the `return_type` the wire names and refusing a non-integer one by name; three
plan-executor cases red before and green after on the device; the harness pin flipped to the
positive case; no Rust-side cast, no other arm, no golden moved. `tpch/q7`, `q8`, `q9` at
`tp1-single` now run the whole device plan and stop on the cpu's join batching beneath the
merge (#220); `191` is on no row. The neighbour survey read every other arm and found one
mismatch, `round` over `Float32` (#221, pinned), and three refusals of shapes the cpu answers
(#222–#224). Shortcuts or bandaids: none. Deviations, each on record: the spec's gtest date
came from `Date32` literals through a CASE, since cuDF makes no date from an integer (#224);
#221 sits in `tickets.md`, disabling no cell. `done` waits on CI.

---

<!-- archived from llm-wiki/tasks/bp-benchmarks.md -->

**Merged 2026-09-22 as PR #139, rebase-merged onto master as two commits split at the
author boundary: `446962ac` (rk00, PR #139's own twenty commits, squashed and re-homed onto
master's component layout) and `08418478` (the review rounds, the completeness pass and the
H200 numbers). Four of dmitry's commits sit inside the first, fused there by the ensemble's
rebase.**

# bp-benchmarks — the corpus benchmark, rebuilt on the component layout

Kind: production

A benchmark run over the corpus at sf40 under CUDA-event timing: one timing tree per
(dataset, mode), one record row per cuDF call, two Nsight captures read down to what a call
splits into and what it moved, and the panels drawn from all of it. PR #139 built this on
`c42b601`, before `drop-mode-name` (#141) and `module-layout` (#143) landed; it is rebuilt on
today's master under the decisions below, every one of which removes something the first
version carried. Nothing outside `llm-wiki/` says `bp` — `scripts/residue-gate.sh` stays empty.

## The four products

**The tree**, `testdata/benchmark-results/<dataset>.sf<sf>/<mode>.benchmark.txt`, one file per
(dataset, mode), a `== <query>` section each. A section is the plan tree with one line per node
under it: `time_us=[[…],[…]] total_us=N` — lanes outermost, one entry per call the lane made,
each entry the call's device microseconds summed over its output partitions, `1` where the
clock rounded a region to zero — every call opens one. Then a `--- run ---`
trailer: `run_us` (the chosen execution end to end, after planning), `device_us` (Σ of the
tree's `total_us`), `runs=[…]` (every measured execution, in order), `build=release`,
`allocator=` (what `install_rmm_pool` reported). The reported execution is the second-smallest
by `run_us` of ten, after one discarded warm-up; both counts are constants in the harness.
The case list is `tests/common/corpus_benchmark_cases.inc`:
`corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp4_sized)` and q19 at `tp1_single`. A
(query, mode) timed here must be enabled on the device in `corpus_cases.inc`, and a rust-only
test says so.

**The record**, `testdata/calibration/records.tsv`, one row per cuDF call, every measured
execution, seventeen columns: `dataset sf query mode node_seq node_type lane recipe_seq
recipe_kind call_index run_index in_rows in_bytes out_rows out_bytes host_us device_us`.
`node_seq` is post-order, `lane` the driving lane, `call_index` the number C++ counts to per
seq. The `#` heading carries what is constant across a run — `timing_mode=events`,
`build=release`, `allocator=…`, `capture=none|trace|metrics` — and an append under a
different heading is refused. No cell is empty: `out_rows` comes from `NodeStats`, `out_bytes`
from `logical_size_from_schema` with the schema the executor holds — the two calls whose output
no batch is built from (an aggregate before its finalize, the concat before a merge) are priced
with the node's `intermediate()` — and a middle call's `in_*` is the call before it. The
harness checks each execution's rows against the plan (`rows_match_the_recipes`) before it
writes them.

**The captures**, `scripts/create_nsys_profile.sh --trace | --metrics` (neither means both),
against the binaries `build-test-shadgpu.sh` pushed. The trace pass feeds `nsys_calls.py`,
which writes `testdata/calibration/calls.tsv`: one row per (case, node_seq, call, depth), the
case being `(dataset, sf, query, mode)` read off the harness's NVTX range — the first version
keyed without it and merged three cases into one row set. The metrics pass feeds
`nsys_hbm.py`, which joins `hbm_bytes` onto `records.tsv`'s tuple and writes
`testdata/calibration/hbm.tsv`; `--peak-bw` is required and comes from the script's knob. The
metrics pass's own record stays on the host beside its capture — it is `records.tsv` with
other microseconds, and nothing reads it but the join. Neither `.sqlite` export is committed.

**The panels**, `scripts/calibration/plot.py --record … --hbm … --out-dir
testdata/calibration/plots`: `load/`, `compute/`, `spread/`, `query/` (two terms, host and
device, side by side and never summed), `icicle/`, `hbm/`, and `index.html`. It requires every
column it reads, refuses records whose `# run:` lines differ, and states the matplotlib
version its output was rendered with — the PNGs embed it, so a re-render on another version
rewrites every file with unchanged data.

## The instrument

`NodeTiming::{Off, Events}`, process-global, `Off` by default. Under `Events` every per-call
entry point — `execute_node`, `execute_scan_rowgroups`, `slice_handle`, `result_from_handle` —
opens one region per output partition: the host clock around the whole call, a CUDA event
pair recorded on the default stream at region open and close, no synchronisation inside. The
region carries `seq, partition, call_index, host_us, device_us` and nothing else. The first
version split the host side at the operator's first device touch; across every call kind in
the sf40 record that prologue measured 0–2 µs, the clock's own resolution, and the split cost
twenty marks in `operators/*.cpp`, one in `expr.cpp` and two thread-locals. It is not carried:
operators and `expr.cpp` stay as master has them.

A CUDA failure while measuring is an error — `last_error` and a non-zero return from the
collection — never a region reported with zeros. `peacock_set_node_timing` refuses a mode it
does not name. `peacock_executor_collect_node_regions(executor, out, cap, out_count)` with
`out == NULL, cap == 0` answers the count and drains nothing; a `cap` below the count fails and
drains nothing; otherwise it copies and releases the events. Rust asks, allocates, drains.
`PeacockNodeStats` loses `time_us`, `peacock_measure_timing_floor_us` goes, and the byte
formula lives in Rust alone — C++ prices nothing.

NVTX is its own switch: `peacock_set_nvtx_ranges`, and a push/pop pair for the harness to name
the case. Node ranges are `<seq>.<call_index> <kind>` with `p<k>` inside, in a `peacockdb`
domain.

## The driver, the record and the harness

The GPU backend journals each ABI call it makes — seq, kind, what it was handed, what came
back — only while `node_timing_on()`; the driver stamps `call_index` and files the journal by
node and driving lane in `RunReport::abi_calls`. `join_regions` matches journal to regions on
`(seq, call_index)` and refuses a mismatch in either direction: a region no call claims, or a
call no region answered. `render_timings` in `plan_text` writes the node lines above;
`Measured`, `Region`, `Measurements`, `join_regions`, `node_measured` and `nodes_as_recorded`
are the driver's. Gone from the first version: `driving_lanes`, `Measured::host_us`,
`Measurements::entry`, the `measure: bool` plumbing (a handover is always priced), the
`file_stem_of` prefix strip. `CallSite {executor, node, lane}` stays because an error naming
its node and lane is worth the two fields; its doc says that and nothing more.

The harness reads two variables. `PEACOCK_RECORD_PATH` names the record file; unset, no record.
`PEACOCK_BENCHMARK_CAPTURE=trace|metrics` turns NVTX on, writes `capture=` into the heading and
leaves the tree alone — a captured run is never the published one; unset means `capture=none`;
any other value panics naming the two. It refuses a debug build (`cfg!(debug_assertions)`) and
a run without the pool. `--build-benchmarks` is `cargo test --release --no-run` into
`cpp/install/rust-benchmarks/`; no custom profile and no `build.rs` change.

## CI, tests, scripts

`peacock_gpu_benchmarks` is built in `cpp-build-2502`, staged with the GPU targets and run on
`gpu-tests` with `--skip bench_`: its own six assertions run on every job, the `bench_` cases
only under `--run-benchmarks`. It is in `RUST_TESTS`, in `gpu_runtime_targets()`, in the lists
the coverage guard compares, and not in `INTENTIONALLY_NOT_IN_CI`. `test_node_timing` stays as
the instrument's structural test — `Off` records nothing, regions equal recorded calls ×
partitions, Σ `device_us` > 0 and ≤ the wall, both modes make the same calls — with no
percentage ceiling: a bound loose enough never to flake on a shared host proves nothing, and
the effect is reported from an sf40 run instead. Rust-only tests pin what CI can reach without
a device: the trailer's arithmetic on the committed tree, the timed set ⊆ the device-enabled
set, a row's field count, and the record's preamble against `record_header()`. The source-text
tripwire over the harness files goes.

`scripts/calibration/tests/` holds pytest cases over a synthetic two-case capture, run by the
cost-report job beside the exec-model tests; the first case is two cases staying two. One
record reader serves the three scripts. In shell: `die`, `pull_one`, the patched
`LD_LIBRARY_PATH`, the sf40 directory and the "N passed" counter live once, in
`lib/shadgpu-env.sh`; `--run-benchmarks` refuses before any push when the binary is not
staged; `--pull-benchmarks` fails when nothing came home or the run died; `PCK_TEST_FILTER` is
the one filter name; the detector for a naming scheme master never had is not written.

## Wiki and tickets

`build-test.md`: the Corpus Benchmarks section describes the tree and the record as written
above, in 50–80 lines, one sentence saying the run is measured under event timing and what
that cost on sf40 and how it was measured; the data-flow diagram under Golden files; the test
table gains its rows and loses the deleted floor test, with the counts recomputed; no capitals
for emphasis. `architecture.md`, under Interfaces: the ABI count and list, the instrumentation
bullet, `NodeStats` without a time, the byte formula in Rust alone. Tickets: master's #191
owns the `date_part` width, so the first version's #197 and its cast do not come; the
`__grouping_id` width finding becomes a paragraph on #65, not a number; the counter does not
move. `refcounted-tables.md` loses only its `nodes_at_or_below_floor` sentences.

## Not in this task

The `date_part` cast in `expr.cpp` (#191, the schema chain's); ssh multiplexing, the rsync
rc=23 exit and cargo diagnostics forwarding in `shadgpu-env.sh`; replacing the hand-kept
fixture list with a `git ls-files` sweep; per-call declared schemas on the wire
(`declared-schemas.md`); `bug_` tests for #65 and #191 (the operator-cases chain's); the JIT
survey of sirius.

## Scope

Code expected to change, on master's layout:

- `cpp/include/peacock_gpu.h`; `cpp/src/{gpu_executor.cpp, node_session.cpp, plan_executor.h}`;
  `cpp/tests/cpu/test_executor.cpp`, `cpp/tests/gpu/{test_cudf.cpp, test_plan_executor.cpp}`.
- `peacockdb-ffi/src/lib.rs`.
- `peacockdb-core/src/executor/`: `mod.rs`, a new `instrument.rs`, `driver/mod.rs`, a new
  `driver/measurements.rs`, `driver/{partitioned, single_partition, mock, accounting}.rs`,
  `driver/tests/{counts, render}.rs`, `gpu_backend/{mod, backend, accumulate, emit, join,
  source}.rs`, `cpu_backend/mod.rs`. `peacockdb-core/src/plan_text/`: `mod.rs`, a new
  `bench_text.rs`.
- `peacockdb-core/tests/`: new `peacock_gpu_benchmarks.rs` and `test_node_timing.rs`;
  `test_corpus_goldens.rs`, `test_plan_goldens.rs`, `test_ci_coverage.rs`,
  `test_gpu_executors.rs` and its directory, `test_module_layout.rs` (register entries);
  `common/`: new `corpus_benchmark.rs`, `corpus_benchmark_cases.inc`, `record.rs`,
  `gpu_session.rs`; `corpus_gpu.rs`, `mod.rs`.
- `scripts/`: `build-test-shadgpu.sh`, `build-test.sh`, `lib/shadgpu-env.sh`, new
  `create_nsys_profile.sh`, new `calibration/{nsys_calls, nsys_hbm, nvtx_names, plot,
  record}.py` and `calibration/tests/`.
- `.github/workflows/pipeline.yml`; `testdata/.gitignore`; `testdata/benchmark-results/**`,
  `testdata/calibration/**`.
- `llm-wiki/{build-test.md, architecture.md, tickets.md, tasks/refcounted-tables.md}`.
- Nothing in `cpp/src/operators/`, `cpp/src/expr.cpp`, `Cargo.toml`, `peacockdb-core/build.rs`,
  `plan/`, `planner/`, `wire/`, the wire format, or `.gitignore` at the root.

Component-level API expected to change:

- The C ABI, sixteen symbols to nineteen: `peacock_measure_timing_floor_us` and
  `PeacockNodeStats::time_us` go; `peacock_set_nvtx_ranges`, `peacock_nvtx_push_range`,
  `peacock_nvtx_pop_range`, `peacock_executor_collect_node_regions`, `PeacockNodeRegion` and
  `PEACOCK_NODE_TIMING_{OFF, EVENTS}` come; `peacock_set_node_timing(int mode)` returns `int`.
  `NodeSession::collect_node_regions`, the timing mode enum and the NVTX switch in
  `plan_executor.h`.
- `executor`'s facade: `Measured`, `Region`, `Measurements`, `join_regions`, `node_measured`,
  `nodes_as_recorded`, `AbiCall`, `AbiCalls`, `CallStats::calls`, `RunReport::abi_calls`,
  `CallSite`, `collect_regions`, and `instrument::{NodeTiming, RmmPool, install_rmm_pool,
  set_node_timing, node_timing_on, set_nvtx_ranges, nvtx_range}` — declared where the
  Visibility rules put them, with any test-crate reach registered in `PUB_MODULES`.
  `CallStats` stops being `Copy`. `plan_text::render_timings`.
- Test targets: `peacock_gpu_benchmarks` and `test_node_timing`, both GPU-job targets.

## Done when

The gate is green on shad-gpu with both targets in it and the residue gate empty; the three
sf40 cases produce the two trees, the record, both captures, `calls.tsv`, `hbm.tsv` and every
panel from the commands `build-test.md` gives, each committed file with the producer the diagram
names; `grep` finds no `mark_device_start`, `thread_local`, `logical_size_from_table`,
`PEACOCK_LOG_LOGICAL_BYTES`, `PEACOCK_NVTX`, `PEACOCK_BENCHMARK_RESULTS_RO`, `build_profile`,
`EVENTS_GROSS_LIMIT` or `NotRun("GPU host only` in the tree; `cpp/src/operators/`, `expr.cpp`,
`Cargo.toml` and `build.rs` are byte-identical to master; the four `architecture.md` sentences
are true; and the wiki counts add up.

## Status — 2026-09-21

The main review is complete: rounds 1 and 2 are closed with every blocking and important
finding taken, so the task resumes at `completing` once it is rebased onto master, which has
moved by chain B's five merges (PRs #158–#160, #162, #163). New benchmark result numbers were
obtained from a run on an H200; they supersede the figures the review rounds read, and the
completeness pass checks the committed trees, record and panels against that run.


## Completeness signoff — 2026-09-22

Solved under its constraints: the four products are on the branch from the human's H200 run, the
instrument, the journal, the harness, the scripts and the tests are as specified, and the gate is
green on shad-gpu with both targets in it. Shortcuts and deviations, each named in the detail file:
the harness lives in `src/test_support/` (master moved the corpus harness there after this spec was
written) and its record test sits in `record/tests.rs` so `planner/` stays byte-identical; the
`calls.tsv` header was renamed by hand, the trace capture being on verda-gpu (unreachable) and the
rows unchanged; `records.tsv`'s heading lost one retired word; `index.html` carries its matplotlib
line from the next collection on; six files outside Scope changed as consequences of in-scope work;
both targets sit in `INTENTIONALLY_NOT_IN_CI` as `Exemption::GpuJob`, master's verified form of the
spec's sentence; the harness makes eight assertions, not six; #226 moved the counter; the sf40
event-timing figure was taken beside a 62 GB neighbour on shad-gpu; `BENCH_POOL_BYTES` is the tpch
gtest's 69 GiB, taken and not measured. Follow-up, not a ticket: `LimitStream` prices a slice with
varlen 0 as master does, so a string slice's `out_bytes` in the record reads low.

---

<!-- archived from llm-wiki/tasks/driver-output-hook.md -->

**Merged 2026-09-21 as PR #163, squashed to `7583865e`, fast-forwarded onto master after its parent.**

# The driver lets a caller look at every batch a node emits

Kind: production

**Testing infrastructure; closes no ticket.** One production change — the driver accepts a hook
called on every emitted batch — and everything that uses it lives under the `test-support`
feature: a validator that holds each device batch to its node's declared schema, driver unit
tests over a mock, and a corpus switch that turns the validator on for every cell already
enabled. Sixth and last of chain B; uses [`device-schema-harness.md`](device-schema-harness.md)'s
comparator and reader.

## Why

Between the scan and the sink nothing on the device compares a produced type to a declared one
([`reports/sink-divergence.md`](../reports/sink-divergence.md) §4: a divergence born at a
`GpuProject` rides through eight nodes and surfaces only if the column is projected out). The
harness now checks nodes one at a time; the corpus runs whole plans. A hook at the driver's
emission sites is what lets the corpus check every node of every enabled query at every mode,
for the price of one schema fetch per batch — and only when asked.

## The work

1. **The hook.** `executor/driver/partitioned.rs`: `Driver<'a, B>` holds
   `hook: Option<OutputHook<'a, B>>` where

       pub(crate) type OutputHook<'a, B> =
           Box<dyn FnMut(usize /*node*/, usize /*lane*/, &<B as Backend>::Batch) -> Result<(), String> + 'a>;

   called wherever a node's *own* output is queued: the lane's device and host arms
   (`:314`, `:324`), the emitter's per-lane outputs (`:391`), and the partition accumulator's
   (`:448`) — after `record_emitted`, before the queue or release. The forwarder at `:495`
   moves a child's batch between queues and is not an emission; it is left alone. `Err(message)`
   becomes a `StepError` naming node, lane and the message — the run fails there as a call
   failure does. `driver::run_with_hook` and `partitioned::run_with_hook` are added beside
   `run` (which calls them with `None`), and `executor::run_with_hook` is the `pub(crate)`
   facade — `pub` would need a `SURFACE` entry (`test_module_layout/visibility.rs:37`) and no
   caller outside the crate exists. With `None` nothing changes: no allocation, no branch
   beyond the `Option`. `single_partition.rs` has no `run` of its own and is untouched.
2. **The validator**, in `test_support/schema_validation.rs` under the `test-support` feature
   and `#[cfg(not(feature = "rust-only"))]` (gated like `corpus_gpu`; `cfg(test)` would hide
   it from `tests/test_gpu_corpus.rs`, which links the library without it):
   `gpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, GpuBackend>` — for
   each `GpuBatch` it calls `device_schema::schema_of(&batch)` (the batch carries its executor
   and handle; no device object is needed) and `device_divergence(node's declared schema,
   actual)`, returning the divergence string on mismatch; a sink's host batches are skipped —
   the sink node has no schema and `concat_batches` already checks them. A
   `cpu_schema_validator` compares `batch.schema()` under the same projection, for symmetry and
   the end-to-end test; the CPU backend's own `declared_as` already holds every stage, so it is
   not installed in the cpu corpus.
3. **Driver unit tests**, `executor/driver/tests/hook.rs`, rust-only over `tests/mock.rs`
   (whose `MockBatch` is rows and bytes, no schema — so the hook under test is a closure, not
   the validator): a hook that refuses at a chosen node and lane — the run fails there with
   the hook's message; the same plan with an accepting, counting hook — called once per
   emitted batch and the run passes; the same plan with `None` — the report equals `run`'s;
   a hook that refuses the third batch — the failure names the third batch's node and lane.
4. **The corpus switch.** `corpus_query!` gains a trailing argument,
   `schema_validation_enabled | schema_validation_disabled`, in both `test_gpu_corpus.rs` and
   `test_cpu_corpus.rs` (the cpu macro ignores it); `gpu_case` installs `gpu_schema_validator`
   when enabled. `corpus_cases.inc`: every row whose gpu modes are not `none` says
   `schema_validation_enabled`. A cell that then fails on schema while its values match gets
   `schema_validation_disabled` with `// #NNN` on its line and a ticket — the registry cell
   stays `enabled`, so coverage does not move. Expected: none, since tasks 1–4 cleared the
   known classes; the switch is for the queries a later rollout enables.
5. **Verification on a device.** The enabled corpus at its enabled modes with validation on —
   CI's gpu job as it stands. Plus, once and not committed: an ad hoc device case whose
   hand-built plan declares a wrong type at one `GpuProject`, run through `run_with_hook` with
   the validator to show the hook failing that node by name; the run's output goes into the
   detail file. The `cpu_schema_validator` gets one committed end-to-end test in
   `src/tests/end_to_end/`: a small query passes under it, and a hook comparing against a
   schema with one field retyped fails naming the field.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/executor/driver/partitioned.rs`, `driver/mod.rs`, `executor/mod.rs` | the hook; `run_with_hook` beside `run` |
| `peacockdb-core/src/executor/driver/tests/hook.rs` (new) | the four mock tests |
| `peacockdb-core/src/test_support/schema_validation.rs` (new), `test_support/mod.rs`, `src/tests/end_to_end/` | the validator; its cpu end-to-end test |
| `peacockdb-core/tests/test_gpu_corpus.rs`, `test_cpu_corpus.rs`, `tests/common/corpus_cases.inc` | the argument; every enabled row |
| `llm-wiki/architecture.md`, `build-test.md` | the driver section names the hook; counts |

Component-level API: `run_with_hook` beside `run` at three levels (`pub(crate)`); the
`OutputHook` type. `run` itself is unchanged. No wire change, no ABI change, no facade change.

## Restriction

The hook is the only production change. No validation code outside the `test-support`
feature; no default hook; no change to what the driver does with a batch beyond calling the
hook first. The ad hoc trigger case is not committed.

## Verification bar

- rust-only: `--lib` — the four mock tests and the cpu end-to-end test; `test_module_layout`;
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --list` (the
  macro expands with its new argument) and `-- registry` (the registry guard, which reads the
  csv, not the macro).
- device: the enabled corpus at `tp1_single` and `tp4_single` with validation on, green; the ad
  hoc trigger red at its node, recorded.
- `git diff` of `executor/driver/` shows the hook and nothing else.

## Device workflow

`build-test-shadgpu.sh`, the corpus binaries; one cycle plus the ad hoc run.

## Completeness signoff — 2026-09-17

Solved under its constraints: `run_with_hook` beside `run` at three levels, `pub(crate)`, the
one production change; with `None` the driver does exactly what it did; a refusal ends the
query as a failed call does and releases the refused batch where it was refused, pinned by a
mock test over every site; the validator under `test-support` holds each device batch to its
node's declared schema through task 5's reader and comparator, with four mock tests and the
cpu twin's end-to-end pair; every enabled device cell runs with validation on and is green,
the ad hoc trigger red at its `GpuProject` by name and not committed. Shortcuts or bandaids:
none. Deviations, on record: three of the spec's four emission sites are hooked — the unload's
host batches are a `CpuBatch` a hook on `B::Batch` cannot take, and were to be skipped;
`OutputHook` lives in `executor/mod.rs` so `test_support` can name it; `tpch/shuffle-stddev`
runs with validation off on #225, its registry cell untouched. `done` waits on CI; with it the
chain is complete.


---

<!-- archived from llm-wiki/tasks/device-schema-harness.md -->

**Merged 2026-09-21 as PR #162, squashed to `0e7804ef`; moved off date-part-return-type onto aggregate-state-types before the merge, its three `date_part` schema cases re-pinned on #191 (date-part-return-type stays open as PR #161).**

# What the device holds, read at the node

Kind: production

**Testing only; closes no ticket and fixes nothing.** A helper that reads a cuDF table's schema
into Rust, a comparator that holds it against the node's declared schema under the projection
cuDF can express, and the cases that use them — at every operator family and at chosen calls
inside the recipe walk. Fifth of chain B, after the four fixes, which is why its comparisons can
be exact: after them a red case is a defect, not a known class. A simpler replacement for the
rejected `declared-schemas`, which measured per call against declarations that had to be
invented; this measures against the declarations the plan already has.

## The helpers

1. **Interop.** `peacock_handle_schema(executor, handle, out_ipc, out_len)` — the IPC
   *schema message* alone for the handle's table — already exists: `decimal-precision-at-export`
   declared it on the chain's one header rebuild, with its two gtests, and nothing in Rust
   calls it until here.
2. **The reduced schema**, in `peacockdb-core/src/test_support/device_schema.rs`, under the
   `test-support` feature like its neighbours and `#[cfg(not(feature = "rust-only"))]` for
   the part that touches a handle:

       pub struct DeviceType { pub id: TypeId, pub scale: Option<i32> }   // cuDF's data_type
       pub struct DeviceSchema(pub Vec<(String, DeviceType)>);
       pub fn device_type_of(arrow: &DataType) -> DeviceType;             // the projection
       pub fn device_schema_of(declared: &Schema) -> DeviceSchema;
       pub fn device_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String>;
       pub fn schema_of(batch: &GpuBatch) -> DeviceSchema;                // not(rust-only)

   `TypeId` mirrors `cudf::type_id` for the types the wire admits. `device_type_of` is the
   fixed mapping cuDF's interop implements for those types — identical in 25.02 and the
   vendored 25.10, which differ only in view and narrow-decimal arms the wire no longer has:
   `Utf8/LargeUtf8 → STRING`, `Decimal128(_, s) → DECIMAL128/s`, `Date32 → TIMESTAMP_DAYS`,
   `Timestamp(unit, _) → TIMESTAMP_<unit>`, integers and floats by width, `Boolean → BOOL8`,
   `Null → EMPTY`; a type with no cuDF image panics naming it. `device_divergence` (named apart
   from `executor/errors.rs`'s `schema_divergence`) compares by position, name and
   `DeviceType`, and reports every diverging column in the sink's spelling
   (`3 o_year: Int32 vs INT16`). Precision, timezone and nullability are outside the projection
   on purpose: after tasks 1–2 they are labels the export carries, not facts the device holds.
   `schema_of` calls `peacock_handle_schema` on the batch's executor and handle (a `GpuBatch`
   carries both), decodes the schema through `arrow::ipc` and projects. It lives in
   `test_support`, not under `src/tests/`' `cfg(test)`, because the corpus binary and the
   next task's validator must see it.
3. **Reading a handle from the harness**: `Device::schema_of(&GpuBatch)` in
   `tests/gpu_tests/device.rs` and the walk's `Session::schema_of(handle)` both delegate to
   `test_support::device_schema::schema_of`.

## The cases

**Every existing case stays as it is.** No green case gains an assertion; no `bug_` case
changes what it pins. Every schema check is a new, separately named case, so the count grows and
nothing is retyped. A new case that is red is a `bug_` with a ticket naming the producer.

- **Operator harness**, one new file per family beside the existing ones
  (`src/tests/gpu_tests/<family>_schema_cases.rs`): for each node kind the family drives, a case
  that runs the node on the device alone and asserts `device_divergence(declared,
  schema_of(output handle)) == None` — join (each type, with and without a projection, on each
  key type, reusing join-cases' `keyed`/`hash_join_keyed` builders, which chain E merges
  before this chain starts), aggregate (init, merge, finalize; `sum`, `count`, `min`, `max`,
  `avg`, `stddev`; grouped and global), project (arithmetic on each numeric type, each cast the
  corpus uses, each scalar function the dispatch admits), filter, coalesce-all, emit (each key
  type), sort, scan (each parquet column type of the fixtures). A union is a forwarder with no
  executor (`coverage.rs:37`) and cannot be run alone: the case `union.cpp`'s deleted block
  described — two branches whose same-named column differs in cuDF type — is two per-branch
  `GpuProject` casts, each checked. Names say the node and the type:
  `a_sum_over_a_decimal_declares_the_state_the_device_holds`.
- **Walk spot-checks**, in `wire/gpu_tests/mod.rs`: `Walk::make` takes an `on_call(seq, kind,
  &[handle], &Session)` hook; each check is a named test that drives one of the file's queries
  with a hook that reads `schema_of` after one call and asserts a literal `DeviceSchema`.
  Intermediates have no declaration, so each expectation is written by hand with its reason;
  the state column order and names come from the query's plan golden (`aggregates.rs:64`
  orders `avg`'s state `[$sum, $count]`):

  | query | after | expected |
  |---|---|---|
  | `AVG_BY_FLAG` | `CudfAggregate{Partial}` | `[l_returnflag STRING, avg$sum DECIMAL128 s=2, avg$count INT64]` |
  | `AVG_BY_FLAG` | `CudfAggregate{Merge}` | the same |
  | `AVG_BY_FLAG` | the finalize `CudfProject` | `[STRING, DECIMAL128 s=6]` |
  | `SUM_BY_FLAG` | `CudfAggregate{Partial}` | `[STRING, DECIMAL128 s=2]` |
  | `ROLLUP` | the grouping-set aggregate | `[STRING, STRING, INT32, DECIMAL128 s=2]` — a `bug_` naming [#65](../tickets.md#t65): the plan says `UInt8` |
  | `PROJECT_OVER_FILTER` | `CudfFilter` | `[c_custkey INT64, c_nationkey INT64]` |
  | `PROJECT_OVER_FILTER` | `CudfProject` | `[doubled INT64]` |
  | `INNER_JOIN` | the join call | `[n_name STRING, r_name STRING]` plus the keys the recipe keeps, in recipe order |
  | `MAX_OF_SUMS` | the inner finalize | `[per_flag DECIMAL128 s=2]` |
  | `MAX_OF_SUMS` | the outer `max` | `[DECIMAL128 s=2]` |
  | `SEMI_JOIN` | the single join call | `[c_custkey INT64]` |

  Eleven checks. **The risk here is enshrining wrong behaviour**: an intermediate has no
  declaration to be held to, so the only thing standing between "what the device holds" and
  "what it should hold" is the expectation written above. A spot-check whose device answer
  differs from the table is therefore never made green by editing the expectation to match
  the device. It becomes a `bug_` test asserting what the device *does* hold, with a ticket
  naming what it *should* and why (the plan's declared type for the column at the nearest
  declared node, DataFusion's type for the same expression, or the table's reasoning), and
  the table's row is corrected to say both. The coordinator and the reviewer read every
  spot-check with that question: is this expectation the engine's contract, or the engine's
  habit? A green spot-check whose expectation was derived by running the device first is the
  habit, and is a finding against the task.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/test_support/device_schema.rs` (new), `test_support/mod.rs` | the reduced schema, the comparator, `schema_of`, with unit tests of the projection |
| `peacockdb-core/src/tests/gpu_tests/device.rs`, `*_schema_cases.rs` (new) | `Device::schema_of` delegating; the suite |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the hook; the spot-checks |
| `llm-wiki/build-test.md`, `tickets.md`, `active-tickets.md` | the suite's rows; every new `bug_`'s ticket |

Component-level API: `test_support::device_schema` (under the `test-support` feature). No ABI
or wire change — `peacock_handle_schema` arrived with task 2. No production Rust or C++
behaviour changes.

## Restriction

No fix rides along: a red case is a ticket. No existing case is edited. The comparator projects
exactly what cuDF stores — a wider comparison belongs to the sink, a narrower one hides bugs.
No spot-check expectation is written from a device run: the eleven rows are written from the
plan and DataFusion before the first cycle, and a row that the device contradicts is pinned as
`bug_`, not rewritten.

## Verification bar

- rust-only: `--lib` (the projection's unit tests), `test_module_layout` (the new files are
  where the layout rules put them).
- device: the whole harness, old cases untouched and green, new suite green or pinned; the
  walk file green with the spot-checks.
- `build-test.md`'s counts grow by the new cases exactly.

## Device workflow

`build-test-shadgpu.sh`; expect two or three cycles, since the suite is written family by
family.

## Completeness signoff — 2026-09-17

Solved under its constraints: `test_support::device_schema` projects a declared schema onto
cuDF's `{type_id, scale}`, reads a handle's through `peacock_handle_schema`, and names every
diverging column in the sink's spelling, with the projection's unit tests rust-only; 134 schema
cases across the eight families, each run on the device alone, and eleven walk spot-checks
whose expectations were written from the plan goldens before the first device cycle — the
reviewer found each derivable from the goldens and `state_type` alone. Every existing case is
untouched beyond `pub(crate)` on 45 builders. Two reds became pins with tickets (#225 new, #65)
and no fix rode along. Shortcuts or bandaids: none. Deviations, on record: the facade items
live in `test_support/mod.rs` by the layout rule; `GpuBatch::executor()` is one production
accessor, its test-only twin deleted; three spot-check rows read from the plan differently from
the spec's table and are recorded as read; Left and Full hash joins have no case, refusing their
first probe batch (#152). A narrow decimal at a handle is refused by arrow-rs before the decode
— loud, naming no column — which the record states. `done` waits on CI.


---

<!-- archived from llm-wiki/tasks/aggregate-state-types.md -->

**Merged 2026-09-21 as PR #160, squashed to `a08c9453`.**

# An aggregate's state is typed by the aggregator that produces it

Kind: production

**This task closes [#163](../tickets.md#t163)** — both arms: `avg` and Welford declare their
count state `UInt64` while both engines produce `Int64`, and the decimal `avg` finalize is typed
by arrow wider than the output it declares. Third of chain B.

## Why it happens

`decompose` (`planner/translator/aggregate.rs:126-157`) builds the partial node's state schema
from DataFusion's `state_fields()` — the layout of DataFusion's *own* accumulator — and
`declared_state` (`:74-92`) picks the field by name tag. But this engine does not run that
accumulator: `decomposition` (`plan/aggregates.rs:36`) rewrites `avg` into `PlanAgg::Count` +
`PlanAgg::Sum` and `stddev`/`var` into `Count` + `Mean` + `M2`, and our `Count` is `Int64` on
both backends — cuDF reduces `COUNT_VALID` to INT64 (`aggregate.cpp:262-265`, the Welford path
`:764-771`), the CPU's `avg` count comes from DataFusion's `count` UDAF (`count.rs:165`, `Int64`)
while its Welford count comes from the `stddev`/`var` UDAF (`variance.rs:115`, `UInt64`). The
declaration names one producer and the plan runs another. On the device nothing checks between
calls, so the state divides fine and the mismatch reached no sink in the survey; on the CPU
`check_state_layout` (`cpu_backend/mod.rs:441-442, 528-552`) refuses it at construction, before
any batch, which is why 23 rows have every `cpu_*` and `gpu_*` cell disabled.

`avg`'s `$sum` state is the same defect on a decimal: DataFusion's `state_fields()` declares it
at the *input* type (`average.rs:154-166`, `avg(x)$sum: Decimal128(7, 2)` in today's goldens),
but the CPU produces it through the `sum` UDAF at `(p + 10, s)`, and only the `widened_decimal`
escape in `check_state_layout` lets that through. Typing the state by its producer moves that
column too.

The finalize arm is the same principle at the next function: `finalize`'s `Avg` case
(`aggregates.rs:80-101`) casts both operands and stamps the divide with `out_type`, but what
arrow computes for `Decimal128(p,s) / Decimal128(p,0)` is wider — `(26,10)` where `(22,6)` is
declared — and the plan never asked for the narrowing.

## The work

1. **The rule.** In `plan/aggregates.rs`, beside `decomposition`:

       impl PlanAgg {
           pub(crate) fn state_type(self, input: &DataType) -> Result<DataType, PlanError>
       }

   matched without a wildcard, so a new variant does not compile until it says its type:
   - `Sum` — DataFusion's `sum::return_type`, quoted: `Decimal128(p, s) → Decimal128(min(38,
     p + 10), s)`; signed integers `→ Int64`; unsigned `→ UInt64`; floats `→ Float64`; anything
     else `PlanError::Unsupported("sum over <type>")`.
   - `Min`, `Max` — the input type.
   - `Count` — `Int64`.
   - `Mean`, `M2` — `Float64`.
   - `MergeM2` — `unreachable!("MergeM2 merges the Welford triple and declares no state")`.
     It is a merge rule, not a state producer: it appears only as `Merge::Combined` and never
     in a `state` slice, and `state_type` is called over `state` slices alone. That is a
     convention today; the test in 5 makes it a check.
2. **Use it.** `decompose` builds each state `Field` as `Field::new(name, state_type(arg)?,
   nullable)`; the name suffix is `rule.state`'s already, and `state_fields()` supplies arity
   and nullability only. `declared_state` stops returning a type.
3. **The one producer that moves.** The CPU's Welford init runs DataFusion's `stddev`/`var`
   UDAF, whose count is `UInt64`. The refusal is `check_state_layout` at construction
   (`cpu_backend/mod.rs:441`), so a cast on a batch cannot help: the init becomes a
   `ProjectionExec` over the `AggregateExec` with `CastExpr(col($count), Int64)` for that
   column and pass-through for the rest, and `check_state_layout` reads the projection's
   schema. `merge_m2.rs:44-76`'s declared layout follows the derived one. The device changes
   nowhere: its casts already say `Int64`. `widened_decimal` stays: the *merge* still produces
   `sum`'s wider type over an already-widened state, which is the escape's remaining reason.
4. **The finalize.** `finalize`'s `Avg` arm wraps the divide: `Expr::Cast { expr: divide,
   target: out_type }`. The device's pre-scaling (`expr.cpp:587-602`) already lands on the
   declared scale; the CPU stops refusing.
5. **Tests.** `plan/aggregates/tests.rs`: for each `PlanAgg` arm, run the CPU accumulator over a
   four-row batch of the arm's input type and assert the produced array's `data_type()` equals
   `state_type(arg)` — the "derived from its producer" half that keeps the table honest; plus
   `Sum` over `Decimal128(15, 2)` declares `(25, 2)` and over `Utf8` is refused. One more
   test walks `decomposition(func)` for every `AggFunc` and calls `state_type` on every entry
   of every `state` slice and every `Merge::PerColumn` list — so the whole table is exercised
   and `MergeM2`'s `unreachable!` is proven unreachable across every decomposition that
   exists, not assumed. And the producer half: for every `AggFunc`, the CPU init built by
   `init_aggregates` over a small batch has the schema `state_type` derives, exactly — the
   test that `check_state_layout` would now pass without its decimal escape for inits. The
   three `bug_` pins flip and retire with the ticket:
   `bug_a_welford_init_exports_its_count_as_int64`,
   `bug_a_welford_merge_exports_its_count_as_int64` (`gpu_tests/aggregate_cases.rs:263, 570`)
   and `bug_a_decimal_average_is_refused_on_the_cpu`. (`aggregate_cases.rs:294` is #187's pin,
   the previous task's.)
6. **Goldens.** Three classes and no other: every `avg`/`stddev`/`var` plan golden changes its
   count state column `UInt64 → Int64`; every decimal `avg`'s `$sum` state moves from the
   input type to `sum`'s `(p + 10, s)`; the `avg` finalize gains a cast in
   `recipe-payloads.txt`. Regenerate rust-only; a diff line outside those three is a finding.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `state_type`; the finalize cast |
| `peacockdb-core/src/planner/translator/aggregate.rs` | `decompose` derives; `declared_state` shrinks |
| `peacockdb-core/src/executor/cpu_backend/mod.rs`, `merge_m2.rs` | the init's `ProjectionExec`; layout follows |
| `peacockdb-core/src/plan/aggregates/tests.rs` (new), `executor/cpu_backend/tests/`, `tests/gpu_tests/aggregate_cases.rs` | the table tests; the producer test; three pins retired |
| `testdata/goldens/**`, `recipe-payloads.txt` | regenerated |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | the 23 rows |
| `llm-wiki/tickets.md`, `build-test.md`, `architecture.md` | #163 closed; counts; the aggregate paragraph says states are typed by `state_type` |

Component-level API: `PlanAgg::state_type`, `pub(crate)`. No wire change — the state schema is
already fields on the wire. No ABI change.

## Restriction

Output types stay DataFusion's — `finalize`'s `out_type` is the SQL contract and is not derived.
`Sum`'s decimal rule is quoted from DataFusion, not reinvented. No change to `aggregate.cpp`;
[#94](../tickets.md#t94) stays open and version-gated. Nothing casts a count to `UInt64`
anywhere.

## Registry

The 23 rows naming #163 at `tp1_single` on both backends: enabled where the CPU stops refusing
and the device's values match; a ticket where they do not; `163` struck from a row only when no
disabled cell in it is left without a ticket (`registry.rs:229-240`). Then the other four modes
for those that pass.

## Verification bar

- rust-only: `--lib` (the producer tests, the validation tests, plan goldens), `test_cpu_corpus`
  over the 23 rows' cpu cells.
- device: the harness with the four pins gone; the rollout.
- `git diff --stat testdata/goldens` shows only `avg`/`stddev`/`var` queries, and the diff's
  lines are the three classes above.

## Device workflow

`build-test-shadgpu.sh`, one cycle for the harness, one for the rollout.

## Completeness signoff — 2026-09-17

Solved under its constraints: `PlanAgg::state_type` types every state column by its producer,
matched without a wildcard, `Sum`'s decimal rule quoted from DataFusion; `decompose` takes only
arity and nullability from `state_fields()`; the cpu's Welford init casts its count to `Int64`
as a second stage and `check_state_layout` admits the widened decimal at a merge alone, both
pinned red-then-green; three device pins retire as positive cases; 33 goldens move in the three
classes and no other; 23 rows roll out with `163` struck from every one, 19 green on the cpu at
all five modes and four on the device. Two deviations, each with its reason on record: the
finalize divides the bare sum and casts the quotient, since arrow's decimal divide truncates at
`s_in + 4` and the spec's numerator cast would round where DataFusion and cuDF truncate; and
`merge_m2`'s `counted_unsigned` casts the arriving `Int64` count to `UInt64` on the way into
DataFusion's variance accumulator, which reads nothing else — the one line that crosses the
Restriction's letter, invisible at every plan, wire, state and answer boundary. `aggregate.cpp`
untouched; `done` waits on CI.


---

<!-- archived from llm-wiki/tasks/decimal-precision-at-export.md -->

**Merged 2026-09-21 as PR #159, squashed to `813acc18`.**

# The export is told the precision it cannot know

Kind: production

**This task closes [#187](active-tickets.md#t187)** — the device exports every decimal at
precision 38 whatever the plan declared — by handing the declared precision to the export. It
is also the chain's one wire and header rebuild: the `Utf8View`/`BinaryView` values the
previous task made dead leave the wire with their C++ arms, the dead `output_schema` fields go,
and `peacock_handle_schema`, which `device-schema-harness` needs, is declared here so the
header moves once. Second of chain B, after [`utf8-everywhere.md`](utf8-everywhere.md).

## Why it happens

cuDF's `data_type` is `{type_id, scale}` (`types.hpp`, "only `_fixed_point_scale` is stored"),
the same in 25.02 and 26.02. Precision is not a field of the column; on export
`to_arrow_schema` writes the width's maximum — 38 for DECIMAL128 — unless told otherwise, and
the way to tell it differs by version: 26.02's `column_metadata` has a `precision` field
(`rapids/include/cudf/interop.hpp:110`), 25.02's has `name` and `children_meta` only
(`rapids-cuda-12.2/include/cudf/interop.hpp:108-119`), and shad-gpu runs 25.02. So the label is
set where both versions agree: on the Arrow schema after `arrow::ImportSchema`, before the
batch is imported against it. `export_table_to_ipc` (`cpp/src/gpu_executor.cpp:51-80`) never
does that, and `peacock_result_from_handle(handle, offset, length)` is never told a
declaration. So `Decimal128(15, 2)` leaves as `decimal128(38, 2)`: same 16-byte values, same
scale, a different number in the schema message — 53 queries, 105 columns, twelve declared
precisions in the survey, scale preserved in every one.

Nothing narrows and nothing needs checking: DataFusion sizes every result precision to hold its
value, and a value that did not fit would be a wrong value, which the goldens catch. The label is
what is wrong, and the export is the only place that writes it.

## The work

1. **ABI.** `peacock_result_from_handle` gains two parameters:
   `const int32_t* decimal_precisions, uint64_t n_columns` — one entry per exported column,
   `0` meaning no declaration. `int32_t` because `column_metadata::precision` is
   `std::optional<int32_t>`. `cpp/include/peacock_gpu.h:203`, `gpu_executor.cpp:292`,
   `peacockdb-ffi/src/lib.rs:151`.
2. **Export.** `export_table_to_ipc` takes the array. After `arrow::ImportSchema` and before
   `ImportRecordBatch`, every column with a non-zero entry has its field's type replaced by
   `arrow::decimal128(precision, scale)` with the scale read off the cuDF column — the same
   buffers, only the label — so the stream's schema message says what the plan declared on
   25.02 and 26.02 alike. The DECIMAL32/64 widening loop (marked dead at `:57-58`) and its
   comment go. Two hard failures replace it, each a `runtime_error` naming the column: a
   fixed_point column whose `type_id != DECIMAL128`; a non-zero precision for a column that
   is not DECIMAL128. `n_columns` must equal the table's column count or the call fails.
   `export_table_to_ipc` stops being `static` and is declared in
   `cpp/src/plan_executor_internal.h`, so the C++ tests can hand it a table no C-API path can
   build — a DECIMAL64 column — and see the refusal.
3. **Callers.** `GpuExport::unload` (`executor/gpu_backend/mod.rs:207-250`) builds the array
   from `self.schema` — `Decimal128(p, _)` → `p`, else `0` — and passes it; its
   `concat_batches` check is unchanged and starts passing. `Device::fetch`
   (`tests/gpu_tests/device.rs:85-112`) takes a `&Schema` for the declaration and builds the
   same array, or passes zeros when the caller has none.
4. **The rule.** `plan/validate.rs`: every decimal in a node schema, literal, `Cast` target or
   scalar return type is `Decimal128`; `Decimal256` is refused now, `Decimal32`/`Decimal64`
   automatically if arrow ever adds them. Unit test with a `Decimal256` column. `expr.cpp`'s
   `fb_to_type_id` maps `Decimal128` alone; no DECIMAL32/64 arm anywhere in `cpp/src` outside
   the loader's widening (`scan.cpp:98-108`), which stays because cuDF's reader is where 64 is
   born.
5. **Wire and header, one rebuild.** `output_schema` leaves `PlanNode` and `CudfUnion` in
   `gpu_plan.fbs` (`:526`, `:608`); `wire/writer.rs:114,143` drop the `None`; `union.cpp:35-50`'s
   guarded block goes, and its comment's reason — branches are planned independently, so a
   column's cuDF type can differ per branch — moves to `architecture.md`'s union row (`:829`)
   beside the statement that the planner's per-branch cast projects are what align them;
   `test_plan_executor.cpp`'s `make_plan_node` loses its unused `schema` parameter;
   `node_session.cpp:275`'s comment stops naming `output_schema`. On the same rebuild the
   `Utf8View` and `BinaryView` values leave `fb::DataType`, `cpp/src/expr.cpp`'s five view arms
   (`:89`, `:228`, `:333`, `:336`, `:478`) go, and `test_plan_executor.cpp`'s 45
   `DataType_Utf8View` uses become `DataType_Utf8`. And the header gains one read-only
   function, for the harness two tasks on:

       int peacock_handle_schema(peacock_executor_t*, uint64_t handle,
                                 uint8_t** out_ipc, uint64_t* out_len);

   the Arrow IPC stream holding the handle's schema message alone (`to_arrow_schema` on its
   view, no rows), freed with `peacock_result_free`, 0 on success; extern in
   `peacockdb-ffi/src/lib.rs`; two gtest cases (a scan's handle reads back four typed fields
   and no batch; an unknown handle fails). Nothing in Rust calls it yet.

## Scope

| file | change |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `cpp/src/plan_executor_internal.h` | the two parameters; the label set on the imported schema; the two hard failures; widening removed; `export_table_to_ipc` exposed to tests; `peacock_handle_schema` |
| `peacockdb-ffi/src/lib.rs` | both extern signatures |
| `peacockdb-core/src/common.rs`, `executor/gpu_backend/mod.rs` | `declared_precisions` (feature-independent, unit-tested rust-only); `unload` builds and passes the array |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `fetch` takes the declaration |
| `peacockdb-core/src/plan/validate.rs`, `validate/tests.rs` | the decimal rule and its test |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/writer.rs`, `cpp/src/operators/union.cpp`, `cpp/src/node_session.cpp`, `cpp/src/expr.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `output_schema` gone; `Utf8View`/`BinaryView` gone from the wire and its C++ readers |
| `cpp/tests/gpu/test_plan_executor.cpp`, `test_cudf_nodes.cpp` | an export with a declared precision reads back at it; no declaration reads back at 38; a precision on a non-decimal is refused; `export_table_to_ipc` over a hand-built DECIMAL64 column fails naming it; the two `peacock_handle_schema` cases |
| `llm-wiki/architecture.md` | union row; "every cast is explicit" gains the sentence that the device type is `{type_id, scale}` and precision is a label the export is told |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md` | counts, #187 closed |

Component-level API: one ABI symbol's signature (`peacock_result_from_handle`) and one new
symbol (`peacock_handle_schema`); the wire loses two fields and two enum values. No facade item
changes.

## Restriction

No cast, no range check, no relabel on the Rust side after decode. The scale still comes from
the column and is still compared at the sink. Any other schema difference keeps today's
"declared vs exported" error.

## Registry

The survey's 53 decimal queries at `tp1_single`, the same rule as the task before: enabled
where sink and values pass; a ticket where values fail; `187` struck from a row only when no
disabled cell in it is left without a ticket (`registry.rs:229-240`). Cells that carried both
classes are decided here. "`Decimal128(38, 4)` agrees for the wrong reason" (survey §4) is a
real agreement after this.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, the validation test red then green.
- C++: `ctest -L cpu` and the two new plan-executor cases on a device.
- device: the harness green (`Device::fetch` with declarations); the rollout at `tp1_single`.
- `grep -rn "output_schema" cpp peacockdb-core flatbuffers` returns nothing; `grep -rn
  "DECIMAL32\|DECIMAL64" cpp/src` returns `scan.cpp` alone; `grep -rn "Utf8View\|BinaryView"
  cpp flatbuffers` returns nothing.

## Device workflow

`build-test-shadgpu.sh`. Staged binaries from before this task are incompatible with the plan
after it — both sides rebuild together, as always.

## Completeness signoff — 2026-09-17

Solved under its constraints: the export is told one precision per column and writes it onto
the imported Arrow schema — same buffers, one label, no version branch; no cast, range check or
relabel on the Rust side; the widening is two refusals naming the column; validation refuses
any decimal but `Decimal128`; the wire lost `output_schema` and the two view values with their
C++ arms, `recipe-payloads.txt` standing since both sat at the tail; `peacock_handle_schema`
declared, externed and pinned by two gtests. 53 queries rolled out at `tp1-single`: five enabled,
the rest on #185, #220 or #163; no sink shows a decimal. Shortcuts or bandaids: `n_columns == 0`
with a null array declares nothing, where the spec's letter required the count to match; the
"`Decimal32`/`Decimal64` automatically" clause is a `Decimal256` match, arrow having neither
variant; `Arrow::arrow_shared` is linked into `peacock_plan_tests` for the read-back; the
DECIMAL64 refusal case sits in `test_plan_executor.cpp`, not the ungated `test_cudf_nodes.cpp`.
Outside scope, reported: `shadgpu-env.sh`'s artifact reader drains cargo's pipe. `done` waits on
CI, the 26.02 leg being the proof the new export compiles there.


---

<!-- archived from llm-wiki/tasks/utf8-everywhere.md -->

**Merged 2026-09-21 as PR #158, squashed to `9f678e9d`; the chain's five branches were each squashed to one commit and fast-forwarded onto master in order, oldest first.**

# Strings are Utf8 from the leaf up

Kind: production

**This task closes [#183](active-tickets.md#t183)** — the sink declares `Utf8View` where the
device exports `Utf8` — by removing the declaration rather than converting the export. First of
chain B; the five tasks after it assume no view type exists in any plan.

## Why it happens

`Utf8View` enters a plan through DataFusion 45's parquet option `schema_force_view_types`,
default `true`. The option that matters is not the session's: `ListingOptions::infer_schema`
reads the `TableParquetOptions` of the `ParquetFormat` it is given, and `lib.rs:71` builds that
format with `ParquetFormat::default()` (`datafusion/src/datasource/file_format/parquet.rs:249-250,
370`); the session config is never consulted for inference. Under it every string column is
`Utf8View` before an operator exists. Everything downstream follows the input: `string_coercion`
picks `Utf8View` only "if Utf8View is in any side"
(`datafusion-expr-common/src/type_coercion/binary.rs:1148`), every string function that can
return `Utf8View` branches on `arg_types[0] == Utf8View`, and the literal rewrite
`serialize.rs:72-77` describes is the coercion following the column. One producer is
unconditional besides the scan: `TypeSignature::Comparable` over all-`Null` arguments
(`greatest`/`least`, `type_coercion/functions.rs:567`) coerces to `Utf8View` — the corpus never
reaches it, and the rule below refuses it if it ever does. cuDF has one `STRING` layout, exports
it as `utf8`, and cannot import a view array at all (`from_arrow_host.cu:431`,
`harness_cases.rs:117-121`). So the plan declares, from the leaf up, a container the device
never holds, and the two meet only at the sink — 76 queries and 204 columns in
[`reports/sink-divergence.md`](../reports/sink-divergence.md).

The corpus's strings enter through cuDF's parquet reader; the view type was never data.

## The work

1. **The option.** `lib.rs:71`: `ParquetFormat::default().with_force_view_types(false)` (the
   existing `.with_enable_pruning(true)` chain), with a comment naming this task and the reason.
   Not the session config — inference never reads it. After it every plan declares `Utf8` (or
   `LargeUtf8`, which the reader does not produce) and no coercion or function can bring a view
   back.
2. **The rule.** `plan/validate.rs` gains a structural rule run with the others: a node schema,
   a literal, a `Cast` target, a binary's out type or a scalar function's return type of
   `Utf8View`, `BinaryView`, `ListView` or `LargeListView` is `PlanError::Invalid`, naming the
   node and column. Node schemas are checked in `walk`; expressions through
   `check_expr_types` in `plan/common.rs`, called at `check_column_refs`'s five node call sites
   (`exec_ops.rs:23,47`, `plan/mod.rs:588,592,596`) and at the join's residual filter, which
   goes through `collect_column_refs` (`join.rs:329`) and would otherwise escape. Unit tests in
   `plan/validate/tests.rs` (under `rooted(...)`, since `validate` refuses a non-sink root
   first): a node declaring `Utf8View`, a `Cast` to `Utf8View`, a residual filter comparing to a
   `Utf8View` literal. The rule is what keeps a DataFusion bump ([#23](../tickets.md#t23)'s
   note) from reintroducing the type silently.
3. **Drop the Rust handling sites**, since after 1 and 2 they are unreachable code that reads
   as support: `wire/serialize.rs` `Utf8View` literal arm (`:72-77`) and type arms
   (`:130-131`); `wire/fb_text.rs:348`; `cpu_backend/spark_partitioning.rs:64`'s cast;
   `common.rs:8`'s `BinaryViewArray` import and the sizing arms at `:35,115`;
   `test_support/result_text.rs:92`'s note; `executor/errors/tests.rs:31,42` re-asserted on
   `Utf8`; `tests/gpu_tests/harness_cases.rs`'s `declaring_view_strings` and its `bug_` pin
   for #183. The wire's `Utf8View`/`BinaryView` values, `cpp/src/expr.cpp`'s five arms and
   `cpp/tests/gpu/test_plan_executor.cpp`'s 45 uses of `DataType_Utf8View` are the next task's,
   which rebuilds the wire once for three reasons; until then those values are dead on the
   wire, which the rule guarantees. Chain E's join-cases and aggregate-cases retire their own
   view cases in their Task 8 before this chain starts; if any survive, this task removes them
   and says so in the detail file.
4. **Goldens.** Every plan golden's `schema=[…]` and every literal tag in `recipe-payloads.txt`
   move `Utf8View → Utf8`; regenerate rust-only and diff — the only change is that word.
   `plan_text/tests.rs:150` and `planner/translator/schema_tests.rs:156,285,305` assert the
   view type from real plans and are re-asserted on `Utf8`.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/lib.rs` | the option |
| `peacockdb-core/src/plan/validate.rs`, `validate/tests.rs` | the rule and its tests |
| `peacockdb-core/src/wire/serialize.rs`, `fb_text.rs` | view arms removed |
| `peacockdb-core/src/plan/common.rs`, `plan/join.rs` | `check_expr_types`; the residual filter checked |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs`, `common.rs`, `test_support/result_text.rs`, `executor/errors/tests.rs` | the cast, the import and sizing arms, the note, two tests re-asserted |
| `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` | `declaring_view_strings` and the #183 pin removed |
| `peacockdb-core/src/plan_text/tests.rs`, `planner/translator/schema_tests.rs` | re-asserted on `Utf8` |
| `testdata/goldens/**/*.plans.txt`, `recipe-payloads.txt` | regenerated |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md`, `tickets.md` | counts, #183 closed, #23 unchanged |

Component-level API: none. No wire change (the next task takes the two enum values), no ABI
symbol changes, no facade item changes.

## Restriction

No cast anywhere: the fix is that nothing needs one. No change to any string function, to
`from_arrow`, to the export. A `Utf8View` that survives the option is a finding — a ticket and
the rule's refusal — not a case for a cast.

## Registry

The survey's 76 queries carrying the string class, at `tp1_single`: each `gpu_*` cell whose sink
now passes and whose values match its golden is enabled; each that fails on values keeps
`disabled` and gets a ticket naming what the values showed. Cells that also carry the decimal
class wait for the next task. `183` is struck from a row only when no disabled cell in it is
left unexplained — `registry.rs:229-240` requires a ticket on any row with a disabled cell, so
the number stays until the other modes are enabled or another ticket names them. The registry
is what the device run proves, nothing more.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, `test_golden_format`, the plan-golden regen, the
  two validation tests red before the rule and green after.
- device: the operator harness (`_cases`) green with the retired pins gone; the corpus rollout
  above at `tp1_single`.
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src testdata/goldens` returns only
  `plan/common.rs`'s rule and `plan/validate/tests.rs`; `cpp/` and `flatbuffers/` are the next
  task's grep.

## Device workflow

`build-test-shadgpu.sh`. One cycle for the harness, one for the rollout.

## Completeness signoff — 2026-09-17

Solved under its constraints: no cast added anywhere and two removed; the option off on both
doors (`read_table`'s format and the session config `register_parquet` reads); the rule refuses
a view type in every schema, intermediate, literal, cast target, binary, function return and
residual filter, pinned by tests run red first; every golden line is the word swap bar the 18
digests and q24's dissolved coercion; 76 queries rolled out at `tp1-single`, three enabled, the
rest ticketed on what the sink showed next. Outside the scope table by review: `cpu_backend/
source.rs` lost its own view cast and refuses instead; a device pin for the sink's refusal was
restored on `LargeUtf8`. Shortcuts or bandaids: none. `183` stays on ten rows whose other modes
were never run past the string class, as the spec's registry rule says. CI has not run: PR #158
conflicts with master since chain D merged, and `done` waits on the human's rebase.



---

<!-- archived from llm-wiki/tasks/case-expectations.md -->

**Merged 2026-09-16 as PR #157, squashed to `5fd6136d` (the chain's three branches squashed in order onto master; count lines and the ticket index merged additively with typed-nulls and empty-build).**

# Three expectations the cases let through

Kind: production

**Cases only; closes no ticket and fixes nothing.** An analyst's reading of join-cases and
aggregate-cases for enshrined behaviour found no green expectation written from a run and none
relaxed in review, and three places where the expectation is weaker than the case's name
claims. Third of chain E, after [`aggregate-cases.md`](aggregate-cases.md); the branch carries
both, so it edits their files in place.

## The three

1. **`same_within_welford` compares column 1's values and nothing else about it**
   (`src/tests/gpu_tests/aggregate_cases.rs:156-213`). Column 0 is compared exactly through
   `assert_same`; column 1 is read `as_primitive::<Float64Type>()` and compared within
   `WELFORD_RELATIVE`. Its name and declared type are never checked, so
   `a_grouped_stddev_finalize_agrees_within_welford` and the `var` twin
   (`aggregate_dimension_cases.rs:567-581`) would pass a renamed or retyped finalize. Right
   expectation: `names_and_types` exact over the whole batch — `compare.rs`'s own helper —
   then values approximate on column 1 alone. The tolerance stays the one place a float is
   compared inexactly.
2. **The composite-key form of #59 is sidestepped, not pinned** (`join-cases-detail.md:33-39`).
   `keyed_anti_script` first ran eight rows; row 4's `i32` is null, which under the hardcoded
   `EQUAL` matched on the device (cpu 30 rows, device 27) — [#59](../tickets.md#t59) on the
   *second* column of a composite key. The script was shrunk to four rows and the green case
   is honest, but the nine #59 pins are all single `Int32` keys, and the composite form has
   none. Right expectation: the eight-row probe as `bug_a_left_anti_join_on_a_composite_key_
   matches_a_null_in_the_second_column_on_the_device`, asserting the 27 rows the device answers
   and naming #59 beside the ticket's existing pins.
3. **`welford_partial` gives a null value a count of one** (`aggregate_cases.rs:683-694`:
   `count 1, mean f64, m2 0` for every row of `synthetic(32, seed)`, whose rows 12 and 25 carry a
   null `f64`). An init over one null row emits count 0, mean NULL, m2 0. As written, both
   backends fold a phantom `0.0` for those rows — DataFusion's `VarianceAccumulator::merge_batch`
   reads the mean ignoring validity and cuDF's `merge_m2` reads `d_means[idx]` the same way — and
   agree for the wrong reason. Two finalize cases and three `bug_` pins inherit the fixture.
   Right fixture: count 0 where the value is null. If a case then diverges, the divergence is
   real — a ticket and a `bug_`, not a fixture restored.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs` | `same_within_welford` compares names and types; `welford_partial` counts nulls as 0 |
| `peacockdb-core/src/tests/gpu_tests/join_dimension_cases.rs` | the composite-key #59 pin |
| `llm-wiki/tasks/join-cases-detail.md`, `aggregate-cases-detail.md` | the record; `build-test.md` counts and the `bug_` table |

No production code, no harness mechanism beyond the two helpers named, no other case edited.

## Restriction

No fix. A case that goes red under the tightened expectation is a ticket and a `bug_`. The
tolerance constant does not move.

## Verification bar

- rust-only: `--lib`, `test_module_layout`.
- device: `PCK_TEST_FILTER='_cases'` green with the one new pin; the two finalize cases and
  the three merge pins re-run under the corrected fixture, each still green or re-pinned with
  its reason in the detail file.

## Device workflow

`build-test-shadgpu.sh`, one cycle.

## Completeness signoff — 2026-09-16

Solved under its constraints: the three expectations tightened as the spec wrote them, no
production code, no fix, `WELFORD_RELATIVE` at `1e-9` still the one inexact comparison; one
new `bug_` (#59's composite key, cpu 30 rows against the device's 27, the three dropped rows
each a null second key) and one re-pin (#216's keyless merge, the counts' stddev computed from
its own arrivals instead of a literal `0.0`), no new ticket; the corrected fixture's six
inheritors — two finalizes and four pins, one more than the spec counted — green on the device;
`_cases` 328, `gpu_tests::` 386, rust-only `--lib` 535 and `test_module_layout` 17. Two files
outside the scope table, by decision: `tests/compare.rs`, where the comparator moved so its red
is rust-only, and `aggregate_dimension_cases.rs`, the re-pin and its imports. Shortcuts: none.


---

<!-- archived from llm-wiki/tasks/aggregate-cases.md -->

**Merged 2026-09-16 as PR #156, squashed to `15c4b30a` (the chain's three branches squashed in order onto master; count lines and the ticket index merged additively with typed-nulls and empty-build).**

# Aggregates, expressions and sorted merges along the dimensions the corpus varies

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Fifteenth in the chain, after [`join-cases.md`](join-cases.md), whose mechanism — a hand-built
node over `Given` leaves, a `Script`, `run_both`, `assert_same` — it uses unchanged. It adds
cases and nothing else. Like join-cases it declares no view type: strings are `Utf8`, and the
corpus's `Utf8View` leaves with the `schema_force_view_types` flip, a task of its own.

## Why these dimensions

Task 9's aggregate cases group on one `Int32` key over a bare column. The corpus groups on
strings and dates (tpch q1 on two string keys is its commonest shape), counts with
`count(*)` 632 times, sums a CASE expression 48 times ([#56](../tickets.md#t56)), and keeps
thirteen queries out on the stddev finalize ([#163](../tickets.md#t163)). Its project cases
cover arithmetic, a cast and CASE; the corpus's `date_part` ([#191](active-tickets.md#t191)),
`substr` (77 plans), `coalesce` (40), a cast to decimal ([#55](../tickets.md#t55)) and the
unary forms have no case. Its sorted merges sort on `id`, ascending and never null; the corpus
has 24 `desc nulls_first` merges, and [#202](../tickets.md#t202)'s second site — the merge in
`node_session.cpp` — has no pin. None of these is a mechanism gap: each is a node and a script
the harness already drives, over a shape nobody wrote down.

## The matrix

The right column names the tickets a row may land on; a row with none may still find one.

| Row | Cases | May land on |
|---|---|---|
| Group keys | `GpuAggregate` grouped on `Utf8`, on `Date32`, on `Int64`, and on two columns (`Int32` and `Utf8`); each state carried through `GpuAggregateBatches`' merge | new |
| Count and expression arguments | `count(*)`, `count(1)`, `count(col)` over nulls; `sum(a * b)`; `sum(CASE … END)` grouped | [#180](active-tickets.md#t180), [#56](../tickets.md#t56) |
| Merge arms with rows | keyless `Sum`, `Count`, `Min`, `Max` and `MergeM2` over several arrivals; `Min` and `Max` merged grouped; a merge-level finalize that computes — `avg`'s divide at done; a merge over grouping-set state, `[keys, gid, state]` | [#199](../tickets.md#t199), pinned; new |
| Welford and its finalize | a global init; the stddev and var finalize — CASE, `Sqrt` and a typed NULL over the count — grouped and global | [#163](../tickets.md#t163) |
| Project expressions | `Minus`, `Modulo`; a comparison and an `AND`/`OR` as a projected column; `IsNull`, `Not`, `Negative`; casts `→ Decimal128`, `Decimal128 → Float64`, `Date32 → Utf8`, `Utf8 → Date32`; `date_part`; `substr`, `coalesce`, `concat`, `lower`, `round`; `NOT LIKE`, `ILIKE`; CASE with a typed-NULL branch; a string literal as a column | [#191](active-tickets.md#t191), [#55](../tickets.md#t55), [#198](../archive/archived-tickets.md#t198), [#203](../tickets.md#t203) |
| Sorts and merges on real keys | `GpuSort`: `fetch` with `desc`; `fetch` at and past the row count; `fetch 0`; a string key; a date key. `GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions`, each: a `desc` key; a nullable key, nulls first and nulls last; two keys; 4 lanes | [#202](../tickets.md#t202) |

### Empty inputs

Only where a new path is reached, each its own named case, never a loop:

| Shape | Cases |
|---|---|
| a keyless merge whose every arrival has zero rows | `Sum`, once |
| a `desc` merge with one lane empty beside lanes with rows | `GpuMergeSortedPartitions`, once |
| `count(*)` over a zero-row batch | grouped and global |

Roughly 40 cases. The count is not the deliverable; a case that answers a question another case
already answered is one too many.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `exec_cases.rs`,
  `accumulate_cases.rs`: the new cases. The state fixtures an aggregate needs — `welford_state`,
  `columns`, the finalize builders — are already in `aggregate_cases.rs`; the new group-key
  states are declared beside them. `emit_cases.rs` only if the grouping-set merge needs a lane
  fixture that file already has.
- `llm-wiki/tickets.md`: a ticket per new defect; `llm-wiki/build-test.md`: the counts and the
  known-wrong table.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/`;
  `synthetic.rs` untouched.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of task 9, unchanged: every divergence gets a ticket before it gets a `bug_` test; a
`bug_` test asserts the wrong behaviour precisely — the wrong value, the refusal's message —
not merely that the two sides differ; nothing here repairs, works around or casts away what a
case finds. And:

- The Welford helper's `WELFORD_RELATIVE` is the one place a float is compared inexactly, and
  it stays the one place. A case that needs another tolerance is a finding against the harness,
  recorded in the detail file, not a second constant.
- Every case is named for its shape. No loop over functions, key types or sort options: a red
  case says which combination it is by its name.
- The kind guard does not move: no kind gains or loses a case here, only rows.

## Verification bar

- Every row above and every empty shape present as a case, green or `bug_` with its ticket in
  the comment above it.
- New tickets in `tickets.md` within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s harness row carries the new count; the known-wrong table carries every new
  `bug_`; the grand total moves with them.
- Green on shad-gpu through `build-test-shadgpu.sh`, one device cycle; the rust-only lib
  unchanged.

## Completeness signoff — 2026-09-16, rewritten after the reopening

Solved under its constraints, as amended: 65 cases against `ENS-join-cases` (29 in
`aggregate_dimension_cases.rs`, 23 in `exec_cases.rs`, 13 in `accumulate_cases.rs`), every
matrix row and empty shape named but one (`GpuAccumulateBatchesAndSort` at four lanes, a
one-lane node by definition — the detail file says why), no production code, no harness
addition, 13 `bug_` cases each with a ticket, #216–#219 new; strings plain `Utf8` and every
group-key cell a `.same()` on init and through the merge; `gpu_tests::` rung 385/385 on
shad-gpu, rust-only lib unchanged. Shortcuts, recorded in the detail file: the known-wrong table
does not exist on any base now that `declared-schemas` is rejected, so the `bug_` prefix, the
ticket comment above each case and the detail file's register are the record; the keyless
`Count` merge is folded into `Sum` and `count(1)` into `count(*)`, both plan facts.


---

<!-- archived from llm-wiki/tasks/join-cases.md -->

**Merged 2026-09-16 as PR #155, squashed to `c20a9d9b` (the chain's three branches squashed in order onto master; count lines and the ticket index merged additively with typed-nulls and empty-build).**

# The joins along the dimensions the corpus varies

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Fourteenth in the chain, after [`operator-cases.md`](operator-cases.md), whose mechanism it
uses unchanged: a hand-built node over `Given` leaves, a `Script`, `run_both`, `assert_same`.
It adds cases along the dimensions task 9 did not vary, and nothing else.

## Why a second cases task, and why joins first

Task 9 proved every kind has a case. Read against the corpus, its join cases hold three things
constant that the corpus does not: the projection (only `Inner` projects; 634 of the 693 corpus
joins do), the key (one `Int32` everywhere; the corpus has 57 composite keys and `Int64` on
nearly every join), and the nested-loop predicate (always AST-able; six of the ten corpus nested
loops go down the cross-then-mask path). A wrong ordinal in a projection, a swapped index after
cuDF swaps a `Right` join's sides, a mask column read in the wrong order — each is a silently
wrong answer, and nothing on a device has ever asked. That is why joins come before the
aggregate, expression and merge dimensions, which are the next task.

String keys are plain `Utf8`. The corpus declares its strings `Utf8View` today only because
DataFusion's parquet option `schema_force_view_types` defaults on; turning it off in
`build_session_state` removes every view type from every plan, and cuDF never held one. That flip
is its own task, and [#183](active-tickets.md#t183)'s pin in `harness_cases.rs` is that task's to
retire. This task declares no view type anywhere: a case that did would test a declaration the
engine is about to stop making.

## The matrix

The right column names the tickets a row may land on; a row with none may still find one.

| Row | Cases | May land on |
|---|---|---|
| Projection on every reachable type | `Right`, `RightSemi`, `RightAnti`, `LeftSemi`, `LeftAnti`, `LeftMark` (`Inner` exists): a projection that reorders, drops the keys, and takes columns from both sides where the type keeps both. `Left` and `Full` are not rows — [#152](../tickets.md#t152) refuses their first probe batch, pinned in task 9 | a wrong ordinal is a new ticket; [#153](../tickets.md#t153), [#159](../tickets.md#t159) are plan-time and have no row |
| Key type × three code paths | keys: two `Int32` (composite), `Int64`, `Utf8`, `Date32`; each on `Inner` (the per-batch join), `Right` (cuDF swaps the sides and the indices are swapped back), `LeftAnti` (the accumulated-keys finish) | [#45](../tickets.md#t45) |
| Residual combinations on `Inner` | residual with a projection; residual with `null_equals_null = true`; residual over two probe batches; a residual on a string column and one on a decimal column, the column-path predicate | #152 for the second batch, pinned; otherwise new |
| Nested loop, non-AST predicate | `CAST(x AS Decimal128) > y`, the cross-then-mask path (`join.cpp`, mask columns in `filter_columns` order): `Inner` and `Left`, each with and without a projection. `Left` is admitted by the planner (`plan/join.rs` checks the batch layout only) and thrown by the C++ ("only supported for Inner joins"): a ticket first, then its `bug_` | [#190](active-tickets.md#t190), [#160](../tickets.md#t160), new |

### Empty inputs

Only where a new dimension changes the code path, each its own named case, never a loop:

| Shape | Cases |
|---|---|
| a zero-row build with a projection | `Right`, `LeftAnti` |
| a `Utf8` key over zero rows | `Inner`, once |
| the non-AST nested loop | an empty build; an empty probe |

Roughly 27 cases. The count is not the deliverable; a case that answers a question another case
already answered is one too many.

## The fixture

`join_cases.rs` gets a builder local to it, `keyed(rows, seed, key)`, for the composite,
`Int64`, `Utf8` and `Date32` keys; build and probe draw from one key domain so every case has
matches and misses. `synthetic.rs` is untouched. No other helper: a case that needs more is a
finding against the harness, recorded in the detail file, not a helper added here.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/join_cases.rs` and `nested_cases.rs`: the new cases and
  the `keyed` builder.
- `llm-wiki/tickets.md`: a ticket per new defect — the `Left` nested-loop refusal at least;
  `llm-wiki/build-test.md`: the counts and the known-wrong table.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/`.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of task 9, unchanged: every divergence gets a ticket before it gets a `bug_` test; a
`bug_` test asserts the wrong behaviour precisely — the wrong value, the refusal's message —
not merely that the two sides differ; nothing here repairs, works around or casts away what a
case finds; the join scripts follow the capability matrix, build side one batch and always
first, probe streamed. And:

- Every case is named for its shape. No loop over key types or join types: a red case says
  which combination it is by its name.
- The kind guard does not move: no kind gains or loses a case here, only rows.

## Verification bar

- Every row above and every empty shape present as a case, green or `bug_` with its ticket in
  the comment above it.
- New tickets in `tickets.md` within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s harness row carries the new count; the known-wrong table carries every new
  `bug_`; the grand total moves with them.
- Green on shad-gpu through `build-test-shadgpu.sh`, one device cycle; the rust-only lib
  unchanged.

## Completeness signoff — 2026-09-16, rewritten after the reopening

Solved under its constraints, as amended: 34 cases against master (28 in
`join_dimension_cases.rs`, 6 in `nested_cases.rs`), every matrix row and empty shape named, no
production code, 5 `bug_` cases each with a ticket (two #152, one #190, two #215; #215 new),
strings plain `Utf8` and every case a `.same()` on both engines; `gpu_tests::` rung 320/320 on
shad-gpu, rust-only lib unchanged. Shortcuts, recorded in the detail file: rows 1–3 and their
builders live in `join_dimension_cases.rs`, not `join_cases.rs`, to keep each file under 1000
lines; anti and mark forms run on scripts with no null pair, so #59 is not re-pinned; the
known-wrong table `build-test.md` was to carry does not exist on any base now that
`declared-schemas` is rejected, so the `bug_` prefix, the ticket comment above each case and
the detail file's register are the record.


---

<!-- archived from llm-wiki/tasks/empty-build.md -->

**Merged 2026-09-16 as PR #153, merge commit `0c8a7480` (the board conflict resolved to master's copy). Chain D is complete and leaves the board.**

# The build side that produced nothing still answers

Kind: production

**This task closes [#175](../tickets.md#t175)** — an empty build side leaves `Right`, `Full` and
`RightAnti` owing rows they cannot make — by emitting a zero-row batch where the build side today
emits nothing at all.

It replaces the parked `empty-answers.md`, which bundled this with [#173](../tickets.md#t173) and
proposed building a capability the engine turns out not to need. A spike measured both; what follows
is what the measurement says.

## What the spike found

**The lane's table already exists and is thrown away.** Both engines build a fully typed zero-row
table for an empty lane — `cpu_backend/emit.rs:73` via `RecordBatch::new_empty(schema)`, and the
device via a deep-copied `cudf::slice(pv, {start, start})` carrying column names
(`node_session.cpp:405`). The driver then drops it at the scatter.

**But it must not stop dropping unconditionally.** Measured off the goldens: **389,331** dropped
empty batches across 692 emitter sites, of which only **244** sit on a permanently-empty lane.
Keeping them all would be roughly a million extra FFI calls to deliver 244 useful ones, since every
consumer but the forwarder makes a backend call per batch. So the drop stays as the default and is
lifted only where a join above owes rows — **the guard is the task**, not the keeping.

**`Right`, `Full` and `RightAnti` need no new mechanism.** Given a zero-row build batch,
`avail.has[BUILD_SLOT]` is true, so the lane takes `SetBuild` rather than `NoBuild` — and
`operators/join.cpp:299` already computes the answer. `Right` is `left_join(probe, build)` with
`left_policy = NULLIFY`, which *is* the build-side pad. `RightAnti` is `left_anti_join` over empty
keys, which returns every probe row. The parked spec's entire §2 — a new `ProjectRole`, a drain-walk
routing change, a `without_build` signature change — buys something the C++ already does.

**[#173](../tickets.md#t173) is smaller than its text.** Of its four named sites only one refuses:
`gpu_backend/join.rs:209`. `accumulate.rs:247` and `:387` return `Ok(empty)` under doc comments
claiming the device refuses, and the C++ throw at `node_session.cpp:278` is unreachable because the
Rust guards short-circuit first. It blocks **zero** cells in `cost-registry.csv`.

## The work

### 1. The `false` branch that was never written

The decision point already exists, in the right place, with the right information. The driver marks
the lane, the join reads its own type, and `empty_build_answers_nothing` already answers:

```rust
// driver/single_partition.rs:183 — the driver routes an empty build lane
ExecutorCategory::Join if self.awaits_build() && !avail.has[BUILD_SLOT] => LaneCall::NoBuild,

// gpu_backend/join.rs:103 — and the join asks the type what it owes
let owes_nothing = self.join_type.map_or(true, empty_build_answers_nothing);
if owes_nothing { return Ok(()); }
Err(BackendError::new("... takes a call over a build table that does not exist (#175)"))
```

**Nothing new marks anything.** `empty_build_answers_nothing` already makes the right decision in the
right place; what is missing is only what to do when it answers `false`. An earlier draft of this
spec proposed teaching a generic accumulator the join type through a plan-time or execution-time
flag. That was solving a problem that does not exist, because the decision never belonged at the
coalesce.

What `without_build` lacks is a zero-row build **table** to call with.

### 2. The table it lacks already exists, and the driver drops it

`driver/partitioned.rs:381`:

```rust
for (lane, out) in outputs.into_iter().enumerate() {
    // Empty scatter outputs are dropped here, so nothing empty traverses a chain
    // because of hash skew.
    if out.num_rows() == 0 { continue; }
```

That `out` is a fully typed zero-row table — `RecordBatch::new_empty(schema)` on the CPU
(`cpu_backend/emit.rs:73`), and on the device a deep-copied `cudf::slice(pv, {start, start})` carrying
column names (`node_session.cpp:405`). It is dropped, and one lane later the join is told its build
side is empty and refuses.

**Keep it where the join above owes rows, and nowhere else.** Not every empty lane: the spike priced
that at 389,331 dropped batches to deliver the 244 that matter — roughly a million extra FFI calls,
since every consumer but the forwarder makes a backend call per batch. The guard is
`empty_build_answers_nothing` on the consuming join's type, the same predicate `without_build` reads,
so the two cannot drift.

**Nothing here is known at plan time about emptiness, and nothing needs to be.** Emptiness is a
runtime fact the driver already has — `out.num_rows() == 0`. What the guard adds is a *static* fact,
and it is narrower than "a join is above":

> this scatter's output feeds the **build** child of a join whose type owes rows when its build is
> empty

Three conditions, not one. The lane may feed no join; it may feed one through intermediate nodes, so
it is a walk rather than a parent lookup; and it may feed the **probe** side — where keeping empty
batches would add a probe call per empty lane and the second one is refused, which is
[#152](../tickets.md#t152). The probe check is therefore not an optimisation: without it this task
*causes* the hazard §5 tells it to watch for.

`driver/index.rs` answers all three with what it already holds. `IndexedNode` carries
`parent: Option<usize>` and `children: Vec<usize>`, and the sides are already named — `PROBE_CHILD`
is 1, and `index/tests.rs:78` records that "the join's build side is its first child, which is what
makes `BUILD_SLOT` zero". So: climb `parent` from the scatter until a node whose category is `Join`,
check whether the child you came from is `children[0]`, and read that join's type. The index is built
once from the tree, so this costs nothing per batch.

**Compute it once, at index time, not per scatter output.** A walk per emitted batch would put a
tree climb inside the hot path to answer a question whose answer cannot change.

### 3. What the C++ then already computes

With a zero-row build batch the lane takes `SetBuild` rather than `NoBuild`, and
`operators/join.cpp:299` computes the answer that was missing: `Right` is `left_join(probe, build)`
with `left_policy = NULLIFY`, which **is** the build-side pad, and `RightAnti` is `left_anti_join`
over empty keys, which returns every probe row.

So the parked spec's entire §2 — a new `ProjectRole`, a drain-walk routing change, a `without_build`
signature change — buys something the C++ already does. None of it is in scope.

### 4. The comments that lie, and the ticket that is wrong

`cpu_backend/accumulate.rs:130` justifies the CPU emitting nothing *because* "the device's collapse of
no handles is a refusal (#173)" — a false premise, so the two engines agree today for a reason that is
not true. `gpu_backend/accumulate.rs:222` says "the device refuses that (#173)" where the code returns
`Ok(Vec::new())`. Both are fixed here.

Correct #173's text to the one site that refuses, and #175's corpus reach: the registry names `tpcds
q77` and `tpch q16`, not `q21` — `q21` is enabled at all five CPU modes and carries no #175.

### 5. The hazard, named rather than discovered

An empty probe batch is an extra probe call, and `gpu_backend/join.rs:303` refuses a second one — that
is [#152](../tickets.md#t152). This keeps batches on the **build** side, so it should not reach it.
Check rather than assume: a lane that gains a build batch must not gain a probe call.

### 6. Tests

- **an empty build lane reaches `SetBuild`, not `NoBuild`** — the driver-level claim, asserted where
  the routing happens rather than through the answer it eventually produces.
- **the six types that owe nothing still drop their empty lanes** — the guard from the other side.
  Without it the change is "keep every empty lane", which is the 389,331-batch version.
- **`Right` pads from the build side and `RightAnti` returns every probe row** — end to end, so "the
  C++ already computes this" is proved rather than argued.
- **a scatter feeding a probe side still drops its empties** — the condition that stops this task
  causing #152 rather than merely avoiding it. Assert it on a plan whose scatter feeds a probe,
  which is the case a test written only around the build side would never reach.
- **a build batch does not become a probe call** — the #152 hazard from the other direction.
- **`tpcds q77` and `tpch q16` run** — the two queries the registry says are waiting, on the CPU.
- **holds and releases balance over a plan with empty lanes** — every kept batch takes
  `acct.hold(held.bytes)` and owes a matching release. **Zero rows is not zero bytes**: an empty
  batch still carries buffers, which is why that rule exists. The spike concluded accounting stays
  paired by reading, and an imbalance surfaces days later as a budget error on an unrelated query.

## Scope of code changes

| file | change |
|---|---|
| `executor/driver/index.rs` | one field per indexed node, computed during `walk`: whether this node's output reaches the **build** child of a join whose type owes rows when empty. A parent climb over data the index already holds |
| `executor/driver/partitioned.rs:381` | the drop reads that field instead of dropping unconditionally |
| `executor/gpu_backend/join.rs:103` | `without_build`'s `false` branch stops erroring — or becomes unreachable, if step 2 routes the lane to `SetBuild` before it is called. **Which of the two is the design, and this spec does not choose**: routing earlier is cleaner and may be impossible if the driver cannot know the consumer at the scatter |
| `executor/cpu_backend/join.rs:185` | the CPU counterpart of whichever shape step 2 takes |
| `executor/cpu_backend/accumulate.rs:130` | a doc comment resting on a false premise |
| `executor/gpu_backend/accumulate.rs:222` | the other lying comment |
| `llm-wiki/tickets.md` | #173 cut to its one real site; #175's corpus reach corrected |
| `llm-wiki/architecture.md` | `:606` and `:856` describe the old behaviour |
| goldens | 6 `.cpu.txt`, 6 `.cost.txt`, `cost-registry.csv` |

**Not touched:** `operators/join.cpp`, which already computes both answers; `gpu_backend/join.rs`'s
`finish_without_keys`, which is #173's surviving site and lives on the **probe** side; the ABI; the
wire; and `empty_build_answers_nothing` itself, which is read and never changed.

**No marker on the plan** — not a `GpuNode` field, not a recipe field, not a plan-text attribute, and
nothing on the wire. The index field above is derived from the tree at index time and is not a
declaration anybody writes; if the work appears to need one that *is* written, the reading above is
wrong and that is a finding to report rather than a scope increase.

## Restriction

**The drop stays the default.** `driver/partitioned.rs:381` keeps dropping empty scatter outputs
everywhere except where the consuming join's type owes rows. An implementation that removes the drop
and relies on something downstream to absorb the cost is the 389,331-batch version.

**The goldens are what catch that**, and the check is a procedure rather than a hope: `.cpu.txt`
carries per-batch lists, so keeping every empty lane would not slip through, it would explode six
golden files. Review the batch-count delta and expect it in the low hundreds. A regeneration that
accepts a diff of that size without reading it is the only way this failure ships.

**No new ABI symbol, no `ProjectRole`, no `without_build` signature change, no wire change, and no
new marker.** If the work appears to need one, the reading in §1 and §2 is wrong, and that is a
finding to report rather than a scope increase.

Code changes are limited to the conditional drop, whichever of the two shapes §2 settles on, the two
doc comments, and the tests.

## Sequencing

**No build dependency on any other task.** An earlier draft placed this after
[`declared-schemas.md`](../archive/archived-tasks.md), reasoning that its zero-row query establishes that a
zero-row table carries full type information across the boundary. That rationale does not survive the
rewrite: this task never sends a zero-row table across the boundary — it hands one to a join. What it
needs is that the scatter's zero-row output is well-typed for a join to consume, and the spike
established that from the code (`cpu_backend/emit.rs:73`, `node_session.cpp:405`).

It does **not** depend on `wire-schema.md` either. The parked spec's claim that `output_schema` on the
wire was the enabling change is about #173, which blocks nothing.

Place it where it is convenient. The one real constraint is the rebase across the layout refactor,
which is what the parked `empty-answers.md` branch foundered on.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `<mode>-<tier>.cpu.txt` | **yes**, 6 files | 244 batches appear where none were counted |
| `<mode>-<tier>.cost.txt` | **yes**, 6 files | derived from the `.cpu.txt` sections |
| `*.plans.txt`, `recipe-payloads.txt` | **no change** | plan time is unaffected; nothing about the recipe moves |
| `--- memory ---` sections | **no change** | plan-time accounting |
| `<tier>.result.txt` | **no change** | digest-sorted, so batch ordering does not reach it |
| `testdata/cost-registry.csv` | **yes** | `tpcds q77` and `tpch q16` move off #175 |

## Coverage

Two queries carry #175, and the honest expectation is that they go green on the CPU and their device
cells land on [#152](../tickets.md#t152) rather than going green — the causes are ordered. Close #175
when its cells are gone from the registry, not when the code lands.

## Device workflow

`build-test-shadgpu.sh`. Five things the run must prove, in the order they are likely to fail:

1. `filtered_join` can be **constructed** from a zero-row build — the likeliest failure;
2. `left_join` and `left_anti_join` over a zero-row build;
3. `concatenate` and `merge` with zero-row members;
4. `to_arrow_host` on a zero-row string column;
5. `q77` and `q16` green on the CPU, with their device cells landing on #152.

## Adjacent, filed rather than fixed

[#199](../tickets.md#t199) — a global aggregate on an empty lane emits nothing on the device and its
identity row on the CPU, because `gpu_backend/accumulate.rs:307` lacks the CPU's `!self.grouped`
clause. A wrong answer, found by reading rather than by a run, and deliberately **not** fixed here:
it is a different bug on the same page, and the parked spec's habit of bundling the two is what this
task exists to undo.

## Completeness signoff

Solved under its constraints: one derived index field and one conditional drop, the drop still
the default everywhere else; no marker on the plan, no wire, ABI, `ProjectRole` or
`without_build` signature change; `operators/join.cpp`, `finish_without_keys` and
`empty_build_answers_nothing` untouched; the six tests of §6 green on the cpu against DataFusion
and the pad proved on the device; the goldens read — six kept batches against the spike's 244,
q16's tp4 cells enabled; #175 archived with its cells gone. Deviations, none a shortcut: q77 did
not go green — its Right outer's build side emits no batch at all, the Inner join's empty lane
dropping as §2 requires — and is #212 with the three `without_build` pins retargeted to it rather
than deleted; `.result.txt` moved one `mode=` line by the harness's authoring rule; a third lying
comment in `gpu_backend/mod.rs`; the regeneration's lost update ticketed as #213 rather than fixed.

## After the signoff: rebased without a test run (2026-09-16)

The branch reached `done` on its old base and was then rebased — first off `ENS-drop-mode-name`
onto master when chain D was split out, and it is behind master's tip again now — with no build
and no test run on the new base. The board drops it to `completing` for that reason. What the
coordinator owes: finish the rebase onto master's tip (typed-nulls first, empty-build on it),
push, and run the tiers this spec's verification bar names — rust-only, the C++ tests it added,
the device cells it claims — then write `done` if everything passes, or `building` with the
failure named in the detail file if anything does not. No new review round unless the rebase
changed a line of code; the reviewer signed off on the code, not on the base.


---

<!-- archived from llm-wiki/tasks/typed-nulls.md -->

**Merged 2026-09-16 as PR #152, merge commit `537ec6bc` (a merge, not a squash, so `ENS-empty-build`, built on it, stays a descendant and PR #153 retargets to master clean).**

# A typed null stays null through the AST path

Kind: production

**This task closes [#198](../tickets.md#t198)** — a typed NULL literal inside an expression the
device evaluates as a cuDF AST becomes a typed *zero* — by removing the second reader of
`ScalarValue.is_null` rather than by correcting it.

## Why it happens

A literal crosses the wire as `ScalarValue`, and `is_null` is what tells a null apart from a zero:
the schema says so itself — *"Without this flag a null is indistinguishable from a zero/empty
value."* The C++ reads that flag in two places and only one of them reads it.

- `build_scalar` (`expr.cpp:456`) opens with `bool valid = !sv->is_null();` and passes it to all
  ten arms. Correct.
- `build_expr`'s `LiteralExpr` arm (`expr.cpp:158`) builds the same ten scalars again for the
  AST, and every one of them passes `true`.

So the answer depends on which path the expression took, and `is_ast_able` decides. A bare literal
short-circuits to `build_scalar` at `build_column:830` and is null. `CaseExprNode` and
`LikeExprNode` are refused by `is_ast_able` and take the column path, so they are null too.

**What reaches the bug is a binary op with a numeric null literal, where both operands infer to the
same non-decimal type** — `is_ast_able`'s binary arm refuses a string literal on either side, refuses
a decimal operand, and refuses a type mismatch, so what survives is exactly `col <op> NULL::T` for a
numeric `T` matching the column. Strings escape for a reason of their own: cuDF's AST has no string
literal.

The consequence differs by operator, and one of the two is worse than a wrong value:

- **arithmetic** — `col + NULL` yields `col`, because the literal is a zero. A wrong value in one
  column.
- **comparison** — `col = NULL` is *true* for every row where `col` is 0, where SQL says the
  predicate is unknown and the row does not survive. A wrong **row count**, which no value-level
  comparison downstream can detect.

The shape was first noticed as a two-engine disagreement on an outer join's pad — the CPU answering
`NULL` where the device answered `0`, Int64 only. That account is inherited rather than reproduced,
and reproducing it is not what this task turns on: the duplication above is visible in the source and
the test in step 4 pins it directly.

**The duplication is the bug, not the missing flag.** Ten arms written twice, one copy asking the
wire and the other assuming. Correcting the ten values leaves the shape that produced them, and the
eleventh type someone adds next year has two places to be added to and one test.

## The work

### 1. One scalar builder, not two

`build_expr`'s literal arm stops building scalars. It obtains one from `build_scalar` and wraps it
in a `cudf::ast::literal`.

The wrapping needs a downcast: `cudf::ast::literal`'s four constructors take a concrete
`numeric_scalar<T>&`, `timestamp_scalar<T>&`, `duration_scalar<T>&` or `string_scalar&`, never a
`cudf::scalar&`. A `cudf::type_dispatcher` over the scalar's own `type()` is the mechanism; the
scalar stays owned by `ExprContext::scalars` exactly as today, and the `literal` borrows it.

**One arm differs on purpose and must stay differing.** cuDF's AST has no fixed-point literal, so
`build_expr` today turns a `Decimal128` into a `double`, scaled — while `build_scalar` builds a real
`fixed_point_scalar`. Do not collapse that into a flag on `build_scalar`; a function that changes
what it returns based on who is asking is the antipattern `coding-style.md` names. Convert the
*`ScalarValue`* before the call instead — an AST-representable value of the same literal — so there
is still one scalar builder and one reader of `is_null`.

### 2. The LIKE pattern, decided rather than assumed

`build_column:901` builds `cudf::string_scalar pattern(psv->string_val()->str(), true)` — the
eleventh site, hardcoding validity like the ten. Its guard above refuses a pattern with no
`string_val`, which is what a typed null serializes as, so it is arguably valid by construction.
Say which it is in code: either it goes through `build_scalar` like the rest, or it keeps its own
construction under a line stating why the guard is sufficient. A comment asserting the conclusion
without the reasoning is what this task is repairing.

### 3. The gtest literal helpers address the fields they name

`test_plan_executor.cpp:50` calls

```cpp
fb::CreateScalarValue(fbb, fb::DataType_Int64, /*bool_val=*/false, /*int_val=*/val);
```

against a signature of `(fbb, type, is_null, bool_val, int_val, …)`. The comments name the fields
they meant; the arguments land one position earlier. `int_val` is therefore **always 0** — every
`make_int64_literal(fbb, 2)` in that file builds the literal `0` — and `make_float64_literal` puts
its double in `uint_val` while `float_val` stays `0.0`. Nothing catches it: all three parameters are
implicitly convertible.

This is the doc-comment-reassigned-by-an-insertion antipattern one level up. `is_null` was inserted
as field 2 of `ScalarValue` and the call sites kept their old positions. Use designated field
assignment through `ScalarValueBuilder`, as the Rust writer does with `ScalarValueArgs::default()`,
so a field inserted later cannot silently take an argument.

**Fix this before writing the test in step 4**, or the test asserts against a literal it did not
build.

**Existing assertions will move, and every move is reviewed rather than accepted.**
`PlanExecutor.FilterNationByRegion` currently runs `n_regionkey > 0` and asserts only
`0 < rows < 25`; with the literal repaired it runs the `> 2` it always claimed. A test whose
numbers change here was testing something other than what it said.

### 4. Tests

- **a typed null inside an AST expression is null and not zero** — the test that pins the fix.
  A project of `col + NULL::Int64` over a scan: every output row null. Today every output row equals
  `col`. Red before step 1.
- **a comparison against a typed null drops the row** — the worse half, and a separate test because
  it fails differently: a filter of `col = NULL::Int64` returns no rows. Today it returns the rows
  where `col` is 0, so a test asserting only "not all rows" would pass on data with no zeroes.
- **a bare typed null is still null** — the `build_column` short-circuit, which was always correct
  and which step 1 must not disturb.
- **a null decimal literal in an AST expression is null** — the arm step 1 keeps deliberately
  different, since cuDF's AST has no fixed-point literal and a `Decimal128` becomes a scaled
  `double`. It is the only arm the delegation does not cover by construction, so it is where the
  refactor breaks if it breaks. Assert the null, and assert the non-null case still carries the
  scaled value, or the conversion can be lost without the null test noticing.
- **every wire type reaching `build_expr` produces a literal** — the type dispatch in step 1
  downcasts a `cudf::scalar` to a concrete scalar class, and a type with no arm fails at run time
  rather than at compile time. Walk the types `convert_data_type` can emit and assert each yields a
  literal or a refusal that names the type. Without this, a type added to the wire later lands in a
  dispatch that quietly has no arm for it — the invisible-absence shape, in a new place.
- **the LIKE pattern, whichever way step 2 decides** — if it delegates, a null pattern is refused by
  the existing guard and the test says so; if it keeps its own construction, the test is that the
  guard actually refuses what the comment claims it refuses. One test either way, because the point
  of step 2 is that the answer stops being an assumption.
- **the two builders cannot diverge again** — with one builder there is nothing to assert; if the
  author keeps two for a reason found on the way, this test becomes mandatory and walks every
  `fb::DataType`, asserting both agree on validity.

## Scope of code changes

Every file this touches, and nothing else compiles differently.

| file | change |
|---|---|
| `cpp/src/expr.cpp` | `build_expr`'s `LiteralExpr` arm (`:157`–`:255`) loses its ten scalar constructions and delegates to `build_scalar` (`:453`), wrapping the result in a `cudf::ast::literal` through a type dispatch. `ExprContext::scalars` keeps ownership as it does today and the `literal` borrows. Plus the decimal pre-conversion in step 1 and the LIKE decision in step 2 (`:901`) |
| `cpp/tests/gpu/test_plan_executor.cpp` | `make_int64_literal` and `make_float64_literal` (`:49`–`:66`) rebuilt with designated fields; whatever assertions move as a result |
| `cpp/tests/gpu/` | the new tests in step 4, in this file or a sibling |

**No Rust changes at all.** `serialize.rs` already writes `is_null` correctly through
`ScalarValueArgs::default()`, and nothing on the Rust side reads a literal's validity.

**No wire change**, no `.fbs` edit, no regeneration: `ScalarValue.is_null` exists and is already
populated. **No ABI symbol**, no header change, no `TableResult` change. **No golden regeneration
expected** — see the table below, and a device result that does move is the finding rather than a
golden to accept.

Net: one C++ source file, one C++ test file, and a new test. If the type dispatch in step 1 turns out
to need a helper of its own, it lives in `expr.cpp` beside `build_scalar` and not in a new
translation unit — the two builders being in one file is half of why the duplication was invisible.

## Restriction

**Code and test changes are limited to what is written above.** No other arm of `build_expr`, no
change to `is_ast_able`'s routing, no Rust change, no ABI symbol, no cleanup of `expr.cpp`'s
neighbours while passing through. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt`, `recipe-payloads.txt` | **no change** | plan text renders the Rust tree; nothing about serialization moves |
| `<mode>-<tier>.cpu.txt`, `.cost.txt` | **no change** | the CPU engine is DataFusion and was always right about nulls |
| `<tier>.result.txt` | **no change expected** — verify | a device result that moves is a query that was answering wrongly, and is worth naming in the report |
| `testdata/cost-registry.csv` | **no change** | no cell is disabled against #198 |

## What it may close in the catalog

This task runs after both `declared-schemas` tasks, so their catalog exists when it starts, and
#198 has a shadow there. A typed NULL built *valid* yields a column with no nulls at all, and the
production exporter derives nullability from `col.has_nulls()` — so a plan declaring a nullable
column exports it **non-nullable**, which is a schema divergence in the catalog's own terms. The thin
test exporter carries the declaration instead and would not show it, which makes this one of the
cases the catalog is built to separate: *the two exporters disagreeing is itself the finding.*

So if the catalog files a `bug_` test for a nullable column exporting non-nullable in a plan carrying
a null literal, **this task deletes it**, in the change that fixes the cause. Check for one before
starting; its absence means the catalog had no query reaching the AST literal path, which is worth
saying in the report either way.

## Coverage

**This task claims no cells.** It is judged on correctness: two engines that disagreed now agree.
Stated here so it is not measured against a yardstick it was never meant to move — the registry is
how a refusal is scored, and a wrong answer is not a refusal.

## Device workflow

`build-test-shadgpu.sh`. The gtests are the gate and need one cycle each for red and green.
Then the enabled device cells, because a wrong answer that nothing refused could be sitting in one:
a result that moves is the finding, not a golden to accept.

## Completeness signoff

Solved under its constraints: one scalar builder and one reader of `is_null`, the decimal arm
converted before it, the LIKE guard decided in code, the gtest helpers built field by field with
`FilterNation`'s one moved assertion explained and tightened; the seven `Literals` gtests, every
wire type walked (twelve literals, ten named refusals); `is_ast_able`, the wire, the ABI and every
header untouched; no golden moved; #198 archived. Deviations, none a shortcut: task 9's two `bug_`
pins deleted in the fixing change and their green forms restored — "no Rust changes" read as
production code; `expr.h` declared two functions for one round, restored, the walk driven through
whole plans; the spec's bare-literal premise was false (#198) and test 3 exercises `build_expr`;
the `Literals` suite left in `test_plan_executor.cpp` past the sibling threshold; decimal-as-double
carried through by the spec's instruction, ticketed #210 with its `bug_` pin; #211 filed for substr.

## After the signoff: rebased without a test run (2026-09-16)

The branch reached `done` on its old base and was then rebased — first off `ENS-drop-mode-name`
onto master when chain D was split out, and it is behind master's tip again now — with no build
and no test run on the new base. The board drops it to `completing` for that reason. What the
coordinator owes: finish the rebase onto master's tip (typed-nulls first, empty-build on it),
push, and run the tiers this spec's verification bar names — rust-only, the C++ tests it added,
the device cells it claims — then write `done` if everything passes, or `building` with the
failure named in the detail file if anything does not. No new review round unless the rebase
changed a line of code; the reviewer signed off on the code, not on the base.


---

<!-- archived from llm-wiki/tasks/walk-drives-every-plan.md -->

**Obsolete — approach rejected, never merged; PR #154 closed 2026-09-16.** Built on `ENS-declared-schemas`, rejected the same day: the walk it extended was that task's instrument, and chain B's `device-schema-harness` reads what the device holds at a call through `peacock_handle_schema` and hand-written spot-checks in the existing walk instead. Its one code idea worth keeping — a device refusal is an outcome the walk records rather than an abort — is noted here for whoever touches `wire/gpu_tests/mod.rs` next, and is not carried over as code.

# The walk drives every plan, and records what it cannot

Kind: production

`test_gpu_recipe_walk.rs` drives a recipe plan by hand — `begin_plan`, the calls each recipe names,
handles threaded between them. It is the only place in the tree that executes a plan the way the
driver does without being the driver, which makes it the only instrument that can ask what a single
call produced.

It cannot drive most of the engine. Not because the device refuses, but because **the harness panics
on call shapes it was never taught**, and because it asserts as invariants things that were merely
true of the queries it had when it was written. Those two are different and the difference is the
whole task: one is a gap to fill, the other is a claim to demote.

This task teaches the harness the shapes it panics on, turns a device refusal into a recorded outcome
instead of an abort, and leaves behind a statement of what it can and cannot drive. It **declares no
schemas and measures no types** — that is [`declared-schemas-derived.md`](../archive/archived-tasks.md),
which cannot be written against a harness that refuses its plans.

## This task is conditional

It sits behind [`sink-divergence-survey.md`](sink-divergence-survey.md) and
[`declared-schemas.md`](../archive/archived-tasks.md), and **the survey may shrink it to nothing.** If every
divergence class the corpus produces already surfaces at the sink, a harness that can drive
intermediate calls buys little, and the `bug_` tests above are worth more than the teaching.

So the first thing this task does is read `reports/sink-divergence.md` and say, in one paragraph,
which of the shapes below are worth teaching given what the survey found. **The human decides whether
it proceeds**, on that paragraph. A task that re-argues its own justification after the evidence
arrives is cheaper than one that assumes it.

## Why this is its own task

Three tasks were planned against a single sighting at the sink and each was planned wrong. The reason
they could only see the sink is this harness: everything above it panics or asserts. A catalog of
per-call schemas, a check that a join's pad has the right width, a measurement of what an aggregate's
state actually looks like — none of them are reachable, and each earlier task worked around that by
predicting instead of measuring.

So the harness is the shared blocker, and it is bought once.

## What it refuses today, and which kind each is

**The list is a grep, not a reading.** Build it with

```bash
grep -n 'panic!\|assert!\|assert_eq!\|unreachable!' <the walk>
```

— 46 sites today, ten of which are refusals rather than ordinary assertions — and classify every one.
A table assembled by reading is how the first draft of this spec listed six of ten.

**The walk's own messages classify themselves**, which is the discriminator to use. A message ending
*"and no shape here plans one"* or *"none is planned"* says the queries in this file do not produce
that shape: a **harness gap**. A message citing a ticket says the engine cannot do it: an **engine
limit**.

### Harness gaps — shapes it was never taught

The device runs these under the real driver every day.

| site | what it refuses | what it should do |
|---|---|---|
| `:221` | `Input::AccumulatedKeys`, `RowGroups`, `RowRange` — *"not a handle the walk holds"* | resolve each like any other input |
| `:230` | more than one handle where it expects one | take the set the call names |
| `:290` | a `Call::bare` — *"takes runtime bounds rather than a seq, and no shape here plans one"* | drive a call with no seq |
| `:309` | a scan's recipe of more than one call | drive it; `declared-schemas` declares per call, so this stops being true |
| `:379` | a recipe with nothing at `AtDone` — *"a streaming limit is the shape that reaches here, and none is planned"* | drive a streaming limit |
| `:466` | a sink's recipe of more than one call | same reason as `:309` |
| `:460` | an export with a row range — *"no shape here plans one"* | supply the range the pattern implies |
| `:350` | a repartition where another kind appears | say which kind, and drive it |

### Engine limits — refusals that are real, and get a `bug_` test

These are not harness gaps. The production code cannot drive the recipe, and the walk is telling the
truth about the engine.

| site | what it refuses | ticket |
|---|---|---|
| `:422` | a join whose calls are not all `PerProbeBatch` — *"a finish pass accumulates probe keys across batches"* | #136 |
| `:441` | a probe of more than one batch — *"the call consumes the build handle with no ABI symbol to copy it"* | [#152](../tickets.md#t152) |
| `driven()` | Left and Full joins, cross and nested-loop joins, the pad projections | [#175](../tickets.md#t175), [#152](../tickets.md#t152) |

**Do not teach the harness to drive these.** But do not leave them as a bare `assert!` either:
**each becomes a `bug_` test**, asserting that the engine refuses this shape, with its ticket named in
a comment above it. `coding-style.md`'s rule applies exactly — the refusal is known-wrong production
behaviour, and an assertion buried in a harness is building around it. A `bug_` test makes it
greppable, makes "which shapes can this engine not run" a search rather than a memory, and goes red
the day `refcounted-tables` closes #152 — which is the signal to delete it.

```rust
/// A streamed probe of more than one batch consumes the build handle and there is no ABI
/// symbol to copy it, so the second call has nothing to join against.
/// [#152](../../../../llm-wiki/tickets.md#t152) — delete this with the fix.
#[test]
fn bug_a_join_refuses_a_second_probe_batch() { … }
```

**A refusal treated as a crash.** `Session::execute` asserts `rc == 0`, so a plan the device declines
aborts the walk rather than reporting. Several known refusals — a cast to a non-fixed-width target, a
group key the shuffle hasher will not take — are unreachable to any test that wants to *observe* them.
Return the code and the call that produced it. **Locate this site first**: it was named from a review
and is not in the walk file itself.

Telling the two kinds apart is the judgement this task exists to make, and the reach table below is
where the judgement is written down.

## The work

### 1. The shapes, one at a time

Each row of the first table above, taught and then exercised by a query that reaches it. Order them
so each lands with its own test rather than as one change — `AccumulatedKeys` and the `AtDone` join
call are the pair that unlock the join family, and the row range and bare call are independent.

`driven()` stays a match rather than becoming a list. Its own comment is the rule this task must not
break: a kind added stops it compiling, which is why nothing has ever silently fallen out of it.

### 2. A refusal is recorded, not fatal

`Session::execute` returns the code and the call that produced it. The walk decides what to do with
it; a test asserting a refusal asserts the code and the call, and a test not expecting one fails
with both in the message.

This is what makes the four known refusals observable — [#45](../tickets.md#t45),
[#189](active-tickets.md#t189), [#95](../tickets.md#t95) and [#55](../tickets.md#t55) all abort the
walk today, so no test can say anything about them beyond that they happened.

### 3. The statement of reach

The deliverable, and the thing neither earlier spec produced: **a table of every node kind and call
shape, and whether the walk can drive it** — drivable, refused by the device with a ticket, or not
yet taught. It lives in `build-test.md` beside the walk's row, because it is what a later reader
needs before planning anything that rests on the walk.

`the_kinds_a_device_has_run_against_the_kinds_the_file_claims` already checks a version of this in
both directions. Extend it rather than adding a second register: one artifact, checked, not a prose
table that drifts.

### 4. Tests

- **each newly taught shape, driven** — one test per row of the harness-gap table, each naming a query
  that produces that shape. A shape taught with no query reaching it is not taught.
- **each engine limit, as a `bug_` test** — one per row of the second table, asserting the refusal and
  naming its ticket. These are the tests that outlive this task: they are the record of what the
  engine cannot run, and each dies with its own fix rather than all at once.
- **a refused plan reports its code and its call** — asserted on one of the four known refusals, so
  the mechanism is proved by something that actually refuses rather than by a fixture.
- **the reach table matches the kinds the walk drives**, both directions, which is the existing
  test's shape extended to call shapes.
- **a join at several probe batches** — the assertion at `:440` demoted, with the multi-batch probe
  driven and its per-call outputs still threaded correctly. This is the one most likely to be wrong
  in a way tests do not catch, because a dropped batch looks like a smaller answer rather than an
  error.

## Restriction

**No schemas.** This task declares nothing, adds no field to `Call`, and asserts nothing about types
— that is `declared-schemas`, and bundling them is how a harness change and a measurement change
become one diff nobody can review.

**No engine limit is lifted.** A `bug_` test records a refusal; it never removes one. If teaching a
shape turns out to need the engine to gain a capability, that is a different task — and it is
probably [`refcounted-tables.md`](refcounted-tables.md), which closes #152 and would delete two of
these tests on its own.

**No production code.** Everything here is the walk, `Session`, and the reach table. If driving a
shape requires a production change, that is the finding: stop and report it, because it means the
driver and the walk disagree about what the engine does, which is a ticket and possibly a bug.

Code changes are limited to `test_gpu_recipe_walk.rs` and whatever it moves to `wire/gpu_tests/`
under `test-layout.md`, the session helper it drives through, and `build-test.md`.

## Goldens

None move. The walk asserts; it writes no golden. If a newly driven shape turns out to change a
recorded batch count somewhere, that is a finding to report rather than a golden to accept.

## Coverage

**No cells.** It enables no query and re-tickets none. What it buys is that the next three tasks can
be verified at all — which is the thing the last three did not have.

## Device workflow

`build-test-shadgpu.sh`. Each taught shape needs a device cycle to prove, so order the work by what
can share one: the join family together, the row range and bare call together.

## After the signoff: rebased onto `ENS-declared-schemas` (2026-09-16)

The branch was carried onto `ENS-declared-schemas` directly, dropping the typed-nulls and empty-build
commits it used to sit on (chain D, now off master). Eleven commits, two count-line conflicts in
`build-test.md` resolved arithmetically (+1 gpu case, +3 `bug_` tests over declared-schemas' figures).
Nothing was built or run: the code change is `wire/gpu_tests/` alone and names nothing from chain D,
but the rust-only tier, the `gpu` shape on shad-gpu and the `build-test.md` counts all still need
verifying on the new base. State dropped to `completing` for that reason.


---

<!-- archived from llm-wiki/tasks/declared-schemas.md -->

**Obsolete — approach rejected, never merged; PR #151 closed 2026-09-16.** A per-call catalogue of declared-versus-exported types measures a divergence whose classes the sink survey already named and whose fixes sit elsewhere: `Utf8View` leaves every plan with `schema_force_view_types` off, decimal precision is a label the export is told (`column_metadata`), and the rest — `extract(year)`'s Int16, `avg`'s count state — are producer bugs fixed at the producer (#191, #163). Its branch `ENS-declared-schemas` stays until `walk-drives-every-plan`, which is built on it, is re-based or dropped. Its derived draft follows as the next entry.

# The schema the device is not held to

Kind: production

The engine declares a schema for every node and **only one of its two backends is held to it.**

On the CPU side `declared_as` (`executor/cpu_backend/mod.rs:239`) runs at every stage and makes the
declaration a fixed point. It absorbs exactly two things and refuses the rest: a decimal a merge
widened is cast back — unsafe cast on purpose, so a value that does not fit raises instead of
becoming a NULL indistinguishable from one the data had — and names and nullability are relabelled.
Anything else, including a column count that does not match, `RecordBatch::try_new` refuses. That is
an emulation doing its job, and it is why the CPU is right about nulls and about decimal precision:
not because DataFusion never diverges, but because something pulls it back to the declaration every
time.

It is private to `cpu_backend` and every call site is inside it (`mod.rs:188`, `accumulate.rs:431`,
`join.rs:272`), so it is not reachable from the device path even in principle.

The device side has no equivalent. It exports whatever cuDF produced, through an exporter that adds
rewrites of its own, and the first thing that compares the result against the declaration is
`concat_batches` at the sink — one table, at the end, after every intermediate divergence has either
cancelled out or been absorbed.

Two consequences of that asymmetry, both of which shape what this task may conclude. **The CPU
cannot adjudicate**: when declared and device disagree, "the CPU agrees with the declaration" is not
evidence, because it agrees by construction. And **nullability is the one relabel with nothing behind
it** — types and column counts are refused by `try_new`, but a column declared non-nullable whose data
carries nulls may simply be re-flagged. Confirm what this arrow version validates before query 8
rests on it.

This task measures the side that is not held. It declares what each **call** produces, exports each
call's output through the export that already exists, compares the two, and records every
disagreement as a `bug_` test. It **fixes nothing**.

It is the first of two. This one takes the six calls whose declaration is already in hand — scan,
filter, project, sort, coalesce-all, exporter. [`declared-schemas-derived.md`](declared-schemas-derived.md)
takes the other ten arms, whose calls emit state, keys and pads.

## Why this shape, and where it measures

An earlier attempt predicted the device's answer in Rust: a function modelling the Arrow → fb → cuDF
→ Arrow round trip, rendered into a golden, consulted at the sink. Wrong instrument twice. A model
can have an arm nobody gave behaviour to and nothing goes red — that is how a `Date64` prediction
shipped with neither a cast nor a refusal — and it duplicates C++ across the FFI with nothing
comparing the two.

But "measure, do not model" is not enough on its own, because **the engine normalizes the schema in
several places and each one hides a class of divergence.** Between cuDF's table and anything Rust can
read there are three:

- `export_table_to_ipc` widens `DECIMAL32`/`DECIMAL64` to `DECIMAL128`;
- `gpu_executor.cpp:49-90` builds `col_meta.push_back({name})` with **no precision**, so
  `to_arrow_schema` falls back to `metadata.precision.value_or(max_precision)` — **every decimal
  exports at 38, always**, whatever cuDF actually holds;
- nullability on the way out is **data-derived**: `child->flags = col.has_nulls() ? NULLABLE : 0`, so
  a batch that happens to contain no null exports as non-nullable regardless of what was declared.

So a measurement taken through the production exporter is a measurement of *our exporter*, not of
cuDF. Precision would read 38 for every decimal at every node, and nullability would read whatever
this batch's data happened to be. That is the finding that reframed this task, and the spec states it
rather than measuring through it.

**What is compared is therefore declared versus exported, and the exporter is a party that can be
wrong.** Two of the three rewrites — precision and nullability — are the exporter's own, so they are
named as limitations rather than reported as divergences (step 3). What remains unexplained is
cuDF's, and that is what the catalog records.

This is not the first per-node assertion in the tree. `executor/gpu_backend`'s device tests already
assert one node's declared output. This generalizes that to every call of every query in the list.

## The work

### 1. The boundary node declares, like every other

`NodeKind::Sink` becomes `NodeKind::Exporter { schema }`. Today a sink carries neither a layout nor a
schema, which makes the export the one call in the engine with nothing to compare against — the
exact boundary where every divergence has historically surfaced.

Eight references, four files: the variant and `layout()`/`schema()` (`plan/mod.rs:249`, `:257`,
`:264`), `GpuUnload::new` (`:970`), three `matches!` in `plan/validate.rs` (`:25`, `:48`, `:301`), one
arm in `executor/tests.rs:135`, one doc line in the rebuilder. `GpuUnload::new` holds its `input`, so
the schema is `input.kind().schema()` at the construction site. `layout()` stays `None`: a sink has
no partition layout and that has not changed.

The rename is part of it because "sink" means "consumes and produces nothing", which stops being true
the moment it declares an output. Nothing treats `schema() == None` as "this is the sink" — validation
matches the variant — so no hidden caller breaks. Eight `.expect("… is not a sink")` messages stay
true but become about a *child*; reword the ones this touches.

**Consequence:** `plan_text/node_text.rs:30` renders a schema wherever `kind().schema()` is `Some`, so
the `GpuUnload` line gains `schema=[…]` and all ten `.plans.txt` goldens move. That is the diff to
review rather than accept, and it is what `exports=` was reaching for — except as the plan's own
declaration rather than a prediction of what the device would do with it.

### 2. A schema on each call

`Call` (`wire/mod.rs`) gains `output_schema: Option<Schema>` — the schema of the table a firing of
that call produces. `None` means "not declared by this task", which is every call of the ten arms
task 2 owns.

The six arms here are the ones where nothing has to be derived: `scan`, `filter`, `project`, `sort`,
`coalesce_all_batches` and `unload` (`wire/attach.rs:100`, `:115`, `:129`, `:145`, `:228`, `:355`)
each produce their node's own schema, already in hand as `inputs[0]` or `node.kind().schema()` — and
`unload` joins them only because step 1 gave it one. **Nothing is derived and nothing is invented**,
which is what keeps every disagreement a disagreement between two facts that both predate this task.

`Schema` here is `plan::Schema`, which is `PartialEq` and not `Eq`, so `Call`'s derive list loses
`Eq`. Check what that breaks before assuming it is free; if something needs `Eq` on a `Call`, say so
in `-impl.md` and carry a key instead of the schema.

**One schema per call is an assumption, and step 5 is where it goes red.** A call fires many times —
`PerBatch` once per input batch, `PerCompaction` on every compaction and again at done. Read before
assuming: `aggregate_batches` (`attach.rs:245`) carries its intermediate-versus-final split on
*separate calls*, so nothing known contradicts it. If firings of one call disagree, the unit is
(call, firing role) and both specs change rather than being patched.

### 3. What the export can and cannot answer

The measurement uses the export that already exists. `peacock_result_from_handle` writes an IPC
stream and `StreamReader::try_new(...).schema()` reads what the device produced — no C++ change, no
new ABI symbol, no cuDF version question.

**Two dimensions it cannot answer, named here rather than measured badly.**

- **Precision.** cuDF's decimal carries a scale and no precision, and `column_metadata` has nowhere to
  put one before 26.02 — it is name and children only. So every decimal exports at 38 whatever the
  plan declared, and "what precision did the device produce" has no answer. The report records the
  class as *our exporter's default*, not as a cuDF divergence. Getting this wrong is what sent
  `wire-schema` to the wrong file.
- **Nullability.** The export derives the flag from `col.has_nulls()`, so it reports what this batch
  happened to contain rather than what was declared. A difference here is data, not a type error.

An earlier draft answered both with a second export path in C++ that cast to the declared type. It is
cut. On 25.02 it restated nothing, so a successful cast meant exported equalled declared by
construction — and the question it genuinely did ask, *does the data fit the declared precision*, is
a **value** check that does not belong in a schema catalog. `declared_as` already asks it on the CPU
side, and that is where the production fix will start.

**What is left is what the catalog is for**: the type each call declares against the type the device
handed back. That is where `Utf8View → Utf8`, an `extract`'s `Int32 → Int16`, a non-fixed-width cast
target and a `Date64` live — and none of them needs a C++ line.

### 4. A second section in `recipe-payloads.txt`

The golden gains a section carrying the declared schemas, per node and per call, **rendered from the
Rust `Call` data before `Writer` serializes anything** and labelled as such.

- **Section A and every `sha256=` line are untouched.** `Writer::push` keeps `output_schema: None`,
  the wire does not change, the C++ reads nothing new, and this task cannot break the executor.
  Putting schemas on the wire is a later task's work; when it lands, the two sections become each
  other's check.
- **Reuse `plan_text`'s `schema_text`/`type_text`** (`plan_text/node_text.rs:317`, `:329`), which
  render `Decimal128(15,2)`. **Not** `wire/fb_text.rs`'s `schema_text` (`:221`), which formats the fb
  enum with `{:?}` and prints a bare `Decimal128`, dropping `decimal_precision` and `decimal_scale`.
  The two have the same name in two modules and the wrong one loses the digits silently — which is
  how the wire renderer got that way. That drift in section A is out of scope and gets a ticket.

The golden is owned by `test_plan_goldens`, which `test-layout.md` moves to `planner/tests/` at the
rust rung. Regenerating needs `UPDATE_CANONICAL=1` **and** `PEACOCK_REWRITE_RECIPE_BYTES=1`.

### 5. One named test per query, on a device

**No generic assert-every-call loop.** Each query gets its own test asserting the schemas that query
produces — `bug_`-prefixed where the assertion records something wrong. A general "every firing
matches its declaration" assertion cannot be green beside named tests asserting the opposite for the
same firings, and every way out of that is either a whitelist or building around a bug. One test per
query has neither problem: the assertion and its exception are the same statement.

The tests live in `wire/gpu_tests/`, a new file beside the walk that `test-layout.md` moves there,
gated `#[cfg(all(test, feature = "gpu"))]`. In-crate because after `visibility.md` `wire` is
`pub(crate)` and an integration test is a separate crate that cannot name `Recipe` or `Call` at all.
The driving harness — `begin_plan`, the calls each recipe names, handles threaded between them — is
shared with the walk as `pub(crate)` within that module rather than copied.

Each test: drive its query's plan by hand; after each firing that produces a handle, export it
through the thin exporter, read the Arrow schema, and assert. Where a call's declaration is `None`,
skip it and **name task 2 in the skip**; that guard is deleted there, so it is written once rather
than scattered through the cases.

A handle that cannot be exported at all is not a schema divergence but a capability gap: a ticket and
one ignored test naming it, never a `bug_` test, because a `bug_` test has to pass today.

The harness must not assume a query has rows — today an `assert!(total_rows > 0)` would refuse query
6 outright.

### 6. The queries

**Check this list against [`reports/sink-divergence.md`](../reports/sink-divergence.md) before
writing any of it.** The survey runs immediately before this task and measures the whole corpus for
the cost of one error message; these thirteen were chosen from tickets and from reading the type
table, which is a weaker basis. Where the survey shows a class does not occur, drop its query and say
so. Where it shows a class nobody predicted, add one. A query kept only because it was written here
first is the habit this chain exists to break.

What the survey cannot tell you is where a divergence *entered* — it sees only the sink — so a class
it reports is still worth measuring per call here. What it can tell you is which classes are worth
the cost.

Added to the shared list rather than replacing it; the queries already there keep their own tests and
their own job of exercising node kinds.

**The mode is chosen per query and stated with it.** `tp1-single` is `OneBatchPerLane`, so it cannot
produce two firings of one call — fixing the mode was what left an assertion in the first draft with
no query that exercised it.

| # | query | mode | what it pins |
|---|---|---|---|
| 1 | `SELECT n_name FROM nation` | tp1-single | `Utf8View` declared; cuDF has no such type, so `Utf8` at every node. One test for the class, not one per node |
| 2 | `SELECT l_extendedprice FROM lineitem WHERE l_orderkey = 1` | tp1-single | narrow `Decimal128(15,2)` — [#187](active-tickets.md#t187). Recorded as our exporter's default, since cuDF holds no precision to have diverged |
| 3 | `SELECT CAST(l_extendedprice AS DECIMAL(38,4)) FROM lineitem WHERE l_orderkey = 1` | tp1-single | a decimal already at max precision, which agrees **for the wrong reason** — 38 is the default, not a measurement. The row exists so the report says so |
| 4 | `SELECT l_shipdate FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Date32` → `TIMESTAMP_DAYS` → back |
| 5 | `SELECT l_orderkey, l_linenumber FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Int64` and `Int32` identity |
| 6 | `SELECT n_name FROM nation WHERE n_nationkey < 0` | tp1-single | the **zero-row** batch, and whether an empty string column types as a populated one |
| 7 | `SELECT n_name AS label, n_nationkey AS id FROM nation` | tp1-single | column **names** survive the crossing. Declared-versus-device only: the CPU relabels names to the declaration, correctly, so it has no opinion to compare against |
| 8 | `SELECT CASE WHEN n_nationkey > 10 THEN n_name END FROM nation` | tp1-single | **nullability**, and what the export can say about it: the flag comes from `col.has_nulls()`, so this records the limitation rather than a divergence |
| 9 | `SELECT n_regionkey, n_nationkey FROM nation` | tp1-single | column **order and arity** — [#190](active-tickets.md#t190)'s uncaught half |
| 10 | `SELECT CAST(n_nationkey AS BIGINT), CAST(n_nationkey AS DOUBLE) FROM nation` | tp1-single | cast targets, fixed-width |
| 11 | `SELECT CAST(n_nationkey AS VARCHAR) FROM nation` | tp1-single | [#45](../tickets.md#t45) — a cast target that is not fixed-width |
| 12 | `SELECT extract(year FROM o_orderdate) FROM orders` | tp1-single | [#191](active-tickets.md#t191) — integer **narrowing**, `Int16` exported where `Int32` was declared |
| 13 | a query whose sink column is a `Date64` | tp1-single | [#200](../tickets.md#t200) — `Date64` returns `Timestamp(ms, None)`, **a type the wire cannot express**. Neither cast nor refused today |
| 14 | a query producing a `Timestamp` at the sink | tp1-single | [#200](../tickets.md#t200)'s other half: `convert_data_type` has no `Timestamp` arm, so the plan should be refused. Assert the refusal is clean and names the type rather than panicking — nobody has exercised it |
| 15 | a scan yielding several batches into a coalesce | tp1-rowgroup | **the firings of one call agree with each other.** The mode is the point: `rowgroup` is what produces more than one batch per lane. Confirm against that mode's plan text and name the query in `-impl.md` |

Two that are **not** walk queries, because they never reach a device:

- `SELECT l_shipdate + interval '1 day' FROM lineitem` — [#168](../tickets.md#t168): the wire has no
  interval, so `attach_recipes` returns `Err` and a walk driving it would panic rather than assert. It
  is a plan-time refusal test, in the same file, with no device.
- A sink column of `Binary`, `Null` or `Float16` — cuDF maps all five of that class to `EMPTY`. No
  corpus column declares one and nothing refuses them today, so the class stays untested by
  construction until a task refuses it at planning time. Named here rather than omitted.

**Named as out of reach for this task, not absent.** `UInt8` and `UInt64` do appear in plans — 55
times at `tp1-single`, as `$count:UInt64` and `__grouping_id:UInt8` — but only above aggregate and
rollup nodes, which are task 2's arms. `ORDER BY` at every mode puts `GpuSort` under a
`GpuAccumulateBatchesAndSort`, also task 2, which is why there is no sort query here despite `sort`
being one of the six. `Timestamp`, `Dictionary`, `Int8`, `Float32` and list-typed columns have no
corpus source at all and are named in the report as gaps, not silently skipped.

### 7. Divergences are catalogued, not adjudicated

A disagreement becomes a `bug_` test asserting the **observed** value, ticket named in a comment,
per `coding-style.md`'s "Building around a bug". It does not matter which side is wrong: this task
does not decide. The task that fixes it decides, with this catalog as its input.

**One test per (divergence class, node kind), not per node instance.** A cause that enters early
survives every node above it: `Utf8View` becomes `Utf8` at the scan and stays that way to the sink, so
a test per node would bury one bug under dozens. The class is what the test names.

**Never a shared table of expected divergences.** A table is a whitelist, and a whitelist is what let
a `Date64` arm ship with no behaviour at all.

### `bug_` tests get their own section in `build-test.md`

This is the first task in the chain that writes any, so it is the one that makes them findable.
`build-test.md`'s Test categories section gains a table of its own, below the categories table: one
row per `bug_` test — the test, what wrong behaviour it asserts, its ticket, and where it runs.

**It is counted separately and excluded from the grand total, and the page says why.** Every other
row counts coverage. A `bug_` test counts a defect, and adding it to a coverage number means the
number goes *up* when a bug is found — which is the reading the number exists to prevent. Its own
total, stated beside the grand total, with one sentence saying the two measure opposite things.

That table is also the answer to "which known-wrong behaviours does this engine still have", which is
a question nothing in the repo answers today. It shrinks as fixes land, and a row that cannot be
deleted when its ticket closes is a fix that did not do what it claimed.

A divergence with no ticket gets one filed. A ticket whose text the catalog contradicts — #187 is
framed as two engines disagreeing when it is a missing argument — gets corrected here, because this
is the first evidence anyone has had.

## API changes

**None.** Everything is in-crate. After `visibility.md`, `wire/mod.rs` and `plan_text/mod.rs` carry no
`pub` items at all, and this task adds none: `Call::output_schema` is `pub(crate)`, the section-B
renderer is `pub(crate)` in `plan_text`, and the tests are `#[cfg(all(test, feature = "gpu"))]`
modules inside `wire/`. `test_support` is untouched, which is what its rule requires — its `pub` API
may not name a type from `wire` or `plan_text`, and the harness signature is nothing but those.

No FFI signature changes. The thin exporter is a new test-only path, not a change to an existing one,
and it needs no new ABI symbol if it reuses the existing export entry point.

## Restriction

**This task fixes nothing.** No cast, no refusal, no correction to a declaration, no change to the
production exporter. Code changes are limited to steps 1 to 6. If a divergence looks like a one-line
fix, that is a ticket: a fix here has to be right about a device nobody has measured yet, and "it is
only one line" is what produces the cascade this task exists to avoid.

No refactor of `attach.rs` or the walk while passing through, no wire change, no C++ beyond the thin
exporter.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt`, all ten | **yes** — the `GpuUnload` line gains `schema=[…]` | step 1 gives the boundary node a schema, and plan text renders every schema it is given |
| `recipe-payloads.txt` section B | **new** | the declared schemas, rendered Rust-side |
| `recipe-payloads.txt` section A, every `sha256=` | **no change** | the wire does not change |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | nothing executes differently |
| `testdata/cost-registry.csv` | **no change** | no cell is enabled or re-ticketed by a catalog |

## Coverage

**This task claims no cells.** The registry scores refusals and this removes none. It is judged on
what it writes down: a declaration per call for six node kinds, measured on a device through an
exporter thin enough that the answer is cuDF's, and a named test for every disagreement.

Its value is to the tasks after it. Three were planned from a single sighting at the sink and each
was planned wrong: one predicted what it could have measured, one chose a C++ mechanism its cuDF
version does not have, one bundled a 35-line fix with a cross-cutting one. A catalog is what stops
the fourth.

## Device workflow

`build-test-shadgpu.sh`, in batches of about five queries as T19 does, at the modes the table names.
Section B's golden is CPU-generated and lands first, so the device run checks a declaration that is
already reviewable. Expect the divergences the tickets predict; record whatever else turns up, which
is the point.


---

<!-- archived from llm-wiki/tasks/declared-schemas-derived.md -->

**Obsolete — a draft derived from `declared-schemas`, never finalized, dropped with it 2026-09-16.**

# The calls that emit state, keys and pads

Kind: production

> **Not finalized — do not dispatch.** A review of this draft against the code found it wrong in
> several load-bearing places: nine arms remain rather than ten (`sort` is already task 1's), the
> pads carry a count and not a shape so their declaration would have to be guessed, five of its
> tickets are aimed at things a declared-versus-device catalog cannot see, and most of its queries
> are undrivable by the walk as it stands — which is why
> [`walk-drives-every-plan.md`](walk-drives-every-plan.md) now sits between this and task 1. It is
> committed as a record of the scope, not as a spec, and is rewritten before it reaches the board.

The second half of the catalog. [`declared-schemas.md`](declared-schemas.md) declared the calls whose
schema was already in hand and left every other call at `None`, skipped by one guard. This task
derives the rest and **deletes that guard**, so that after it no call in the engine produces a table
nothing has a claim about.

Same discipline throughout: it **fixes nothing**. Every disagreement between a declaration and what
the device exported becomes a `bug_` test asserting the observed value, with its ticket named in a
comment, one test per (divergence class, node kind). Same instruments too: the thin test exporter, so
the answer is cuDF's rather than our exporter's defaults; one named test per query rather than a
generic assert-every-call loop; the mode chosen per query and stated with it; and the tests in
`wire/gpu_tests/`, in-crate, sharing the walk's harness as `pub(crate)`.

## What is left

Ten of `attach_recipes`' sixteen arms (`wire/attach.rs:76`). Task 1 took six — scan, filter, project,
sort, coalesce-all, unload. These remain:

| arm | why its declaration is not simply the node's schema |
|---|---|
| `aggregate` (`:163`) | the accumulate call emits **state**, the finalize emits values |
| `aggregate_batches` (`:245`) | two `PerCompaction` calls emit state, a distinct `AtDone` finalize emits the node's rows |
| `accumulate_and_sort` (`:200`) | compaction emits runs, done emits the sorted table |
| `hash_join` (`join.rs:29`) | up to six calls: a probe-keys project, the join per probe batch, two `AtDone` finishes, a `NullPad` project and a `Narrow` project — six shapes, one node |
| `nested_loop_join` (`join.rs:343`) | one call, but its output is the node's rows only when a projection is not dropped ([#190](active-tickets.md#t190)) |
| `cross_join` (`:324`) | the product's shape |
| `merge_sorted_partitions` (`:286`) | input schema, across lanes |
| `emit_partitions` (`:305`) | the scatter: one table per lane, each the input's schema |
| `limit` (`:341`) | input schema, but only the straddling batches are called |

Task 1 left one of its own six here too: `sort`. Every `ORDER BY` in the corpus puts a `GpuSort`
under a `GpuAccumulateBatchesAndSort`, so the sort arm is declared there but no query reaches it
until this task's accumulator queries do.

`MergePartitions`, `Union` and `Interleave` return `Ok(None)` — they attach no recipe and make no
call, so there is nothing to declare and nothing to skip. Say so in the code rather than letting them
fall through the same arm as an undeclared call, or "structural, no calls" and "nobody got to it yet"
become the same thing again.

## Where each declaration comes from

Most of it exists and has simply never been written down.

- **Aggregate state**: `GpuAggregate::intermediate()` and `GpuAggregateBatches::intermediate()`
  (`plan/mod.rs:627`, `:654`) already return the state `Schema`, and `aggregate_writer::aggregate`
  already receives it. Nobody has ever compared it to what the device produces. Three tickets suspect
  it is wrong — [#163](../tickets.md#t163) signed-versus-unsigned count state,
  [#94](../tickets.md#t94) the Welford count child's width, [#55](../tickets.md#t55) a partial-phase
  operand cast absent from the final phase's input schema — which makes this the single most valuable
  measurement in either task.
- **Probe keys**: the join's key schema exists in `wire/join.rs` and is what the `ProjectRole::
  ProbeKeys` call emits.
- **The pads**: `NullPad { nulls }` and `Narrow` carry their own widths, so their output shape is a
  function of data already on the call.
- **Routing arms** — `emit_partitions`, `merge_sorted_partitions`, `limit`: the input schema,
  unchanged. Cheap, and they are in this task only because they are not in the other one.

Nothing is invented. Where a shape genuinely cannot be derived from what the recipe writer holds,
that is a finding: record it, leave the call at `None`, file a ticket naming what the writer would
need. **Do not guess a declaration.** A guessed declaration that the device contradicts produces a
`bug_` test against our own invention, which is worse than no test — it pins a fiction.

## Multi-lane plans

Task 1 planned everything at `tp1-single`. That cannot reach this task's work: `emit_partitions` is
the scatter and `merge_sorted_partitions` merges across lanes, so **neither node exists in a
single-partition plan**. This task plans at a `tp4` mode as well, and the difference is not
incidental — three of the classes below only appear above a shuffle.

The lane dimension adds an assertion task 1 had no use for: **the lanes of one call agree with each
other.** A scatter emitting four tables whose schemas differ is a bug nothing else would see, and
[#122](../tickets.md#t122) is exactly that shape suspected for decimal scale across an aggregate's
partials.

## The queries

Chosen from the tickets that name a divergence this task can reach. As in task 1 they extend the
shared list in `src/test_support/` rather than replacing it, and the existing entries keep their
assertions.

| query | what it pins |
|---|---|
| `stddev(v)` / `var(v)` over a numeric column | [#94](../tickets.md#t94) — the Welford state's count child, typed differently by different cuDF runtimes |
| a grouped `count(*)` | [#163](../tickets.md#t163) — the count state declared `UInt64` and produced `Int64` |
| `sum(<decimal> / <int>)` with a `GROUP BY` | [#55](../tickets.md#t55) — the partial phase's operand cast missing from the final phase's input schema |
| `sum(l_extendedprice * l_discount)` at a `tp4` mode | [#122](../tickets.md#t122) — decimal **scale** agreement between the partials of one aggregate |
| a shuffled `count(*)` with a `GROUP BY` | [#180](active-tickets.md#t180) — nullability introduced by a state merge, same width and same bytes |
| `GROUP BY ROLLUP(...)` above a join at `tp4` | [#189](active-tickets.md#t189) — a `UInt8` grouping-set id in the key set; with [#95](../tickets.md#t95), a group-key type the shuffle hasher refuses |
| a non-`SELECT *` nested-loop join | [#190](active-tickets.md#t190) — a dropped projection: the node declares two fields and three arrive. The ticket's uncaught half is a projection that **reorders** or drops one while keeping the count |
| a join whose key needs a string cast | [#45](../tickets.md#t45) — a cast target that is not fixed-width, refused at the join |
| a `Right`, `Full` or `LeftAnti` join | the `NullPad` and `Narrow` projects, whose shapes nothing has ever checked |
| `min`/`max` over a string or a date column | [#195](../tickets.md#t195) — a string reduce in the merge, with zero corpus uses today |
| a wide `SELECT DISTINCT` | [#195](../tickets.md#t195) — the other shape with no corpus query at all |

## What task 1 assumed, and what happens if it was wrong


This spec is written before task 1's measurements exist, so it rests on four of its assumptions.

**If any of them is refuted, this task goes to `blocked(<its state>)` and stops.** Not adapted, not
worked around, not narrowed to whatever still fits. The coordinator writes the block, names which
assumption failed and what measurement refuted it in `<task>-detail.md`, and waits for the human —
who rewrites this spec, because only the human writes a spec. A spec amended mid-flight to survive
its own falsified premise is exactly how the three tasks before this one went wrong: each discovered
that its premise was false, built around the discovery, and shipped something nobody had specified.

The block is cheap and the alternative is not. Every one of these four is knowable from task 1's
results before a line of this task is written, so the usual case is that the block never happens —
and when it does, it happens at the start rather than three days in.

1. **One schema per call.** `Call::output_schema` is a single `Option<Schema>`, so a call whose
   firings disagree has no place to say so. Nothing known contradicts it, and task 1's query 13 —
   several batches into a coalesce at `tp1-rowgroup` — is what tests it. If it fails, the unit is
   (call, firing role) and both specs change. This task is where it is most likely to fail:
   `PerCompaction` compacts here and never does in task 1.
2. **A handle can be exported mid-plan and survive it.** The whole harness rests on the export being
   readable without consuming what it read. This task's calls produce **state** handles, which are
   the likeliest to refuse — cuDF groupby state is not obviously an Arrow table at all. A state
   handle that cannot be exported is a capability gap: a ticket and one ignored test, never a `bug_`
   test, because a `bug_` test has to pass today. If most of them refuse, this task's product is that
   finding plus the declarations, and the comparison waits for a mechanism that can read them.
3. **The thin exporter reaches state handles too.** It was built in task 1 against tables of ordinary
   columns. Passing declared precision and nullability through for a state row — a Welford triple, a
   partial sum beside its count — is not the same problem, and if it cannot, this task measures
   through the production exporter and says so at every affected assertion.
4. **The catalog's shape holds at scale.** Task 1 checks six node kinds at two modes; this adds ten
   kinds across lanes. If the per-(class, node kind) rule still produces an unreadable number of
   tests, the rule is wrong and this task says so rather than thinning the coverage to fit it.

## API changes

**None**, and for the same reason as task 1: everything is in-crate. `Call::output_schema` is
`pub(crate)` and was added there; the renderer, the harness and the shared query list all exist and
are `pub(crate)` within `wire/gpu_tests/` and `plan_text`. `test_support` stays untouched, as its own
rule requires. The only deletion is task 1's `None` guard, which is why that guard was written as one
place rather than scattered through the cases.

## Restriction

**This task fixes nothing.** No cast, no refusal, no correction to a declaration, no change to an
aggregate's state layout or a join's projection. Code changes are limited to setting
`output_schema` on the ten arms above, the queries, and the tests. No wire change, no C++, no ABI
symbol, no refactor of `attach.rs` or `join.rs` while passing through. Anything else found on the way
is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `recipe-payloads.txt` section B | **grows** | ten more arms declare, so more nodes print a schema |
| `*.plans.txt` at the `tp4` modes | **no change** | task 1's `Exporter` schema already landed there; this task adds no node-line field |
| `recipe-payloads.txt` section A, every `sha256=` | **no change** | the wire still does not change |
| `*.plans.txt` | **no change** | the declaration is on the call, not the node line |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | nothing executes differently |
| `testdata/cost-registry.csv` | **no change** | no cell is enabled or re-ticketed by a catalog |

## Coverage

**This task claims no cells**, for the same reason as task 1: the registry scores refusals and this
removes none. It is judged on what it writes down — a declaration for every remaining call, checked
on a device at one and four lanes, and a named test for each disagreement.

What it is worth is the input it gives the tasks that follow. Three of them are waiting on it.

## Device workflow

`build-test-shadgpu.sh`, in batches of about five queries as T19 does, at `tp1-single` and one `tp4`
mode. Section B's golden is CPU-generated and lands first, so the device run checks a declaration
that is already reviewable. Expect the divergences the eleven tickets predict; record whatever else
turns up, which is the point.


---

<!-- archived from llm-wiki/tasks/sink-divergence-survey.md -->

**Done 2026-09-15 as a prototype — no PR, branch `ENS-sink-divergence-survey` at `cebe1ead` never merged; the product is [`reports/sink-divergence.md`](../reports/sink-divergence.md) and the sink's message, both on master.** Archived 2026-09-16; the chain it closed (`ENS-drop-mode-name`, later B) is gone from the board.

# What the corpus already knows about schema divergence

Kind: prototype

Sixty-odd device cells are disabled against schema causes, and every one of them fails at the sink.
The corpus runs those queries on a device already. They already fail. **The evidence exists and
almost nobody reads it.**

The message is better than it looks. `concat_batches` ends in `RecordBatch::try_new`, whose error is
*"column types must match schema types, expected {declared} but found {exported} at column index
{i}"* — so both types are already there, and the tickets quote them: #187 carries `expected
Decimal128(15, 2) but found Decimal128(38, 2)`, #191 carries `expected Int32 but found Int16 at
column index 0`.

Two things are missing, and they are small: the column **name**, and every column **after the
first** — `try_new` reports one mismatch and stops. A sink carrying a string and a narrow decimal is
two findings, and reporting one is how a rollout concludes that fixing the string was enough.

So this task is mostly the second half: **run the corpus and write down what comes out.** The code
change is a handful of lines. The product is a report.

**`llm-wiki/reports/sink-divergence.md`.** The branch is never merged.

## Why this before anything larger

[`declared-schemas.md`](declared-schemas.md) declares a schema per call and measures it on a device,
and it is the right shape — but it costs a harness, an exporter and a device cycle per query, and it
measures thirteen hand-picked queries. This measures **the whole corpus** for the cost of one error
message and the rollout that was going to happen anyway.

What it cannot see is anything above the sink: `concat_batches` is the only comparison on the device
path, and it happens once, at the end, on the concatenated result. So this survey answers *which
divergences reach the boundary and how often*, and says nothing about where they entered. That is the
question `declared-schemas.md` is for, and this is what tells it which classes are worth the harness.

## The work

### 1. The message names the column, and every column

`executor/gpu_backend/mod.rs:179` wraps `concat_batches`' error, which already carries both types.
Append what it lacks: the column **name**, and the columns after the first.

The decoded batches carry the device's schema — `StreamReader::try_new` parses the IPC schema message
before any batch — so comparing it against `self.schema` costs nothing and needs no prediction.

**Keep the existing sentence as the prefix.** Every ticket quotes it and every rollout greps for it;
this appends, it does not replace.

**Compare types only.** `try_new` does not check nullability, so a nullable-versus-non-nullable
difference never reaches the sink and never disabled a cell. Reporting it here would put a class in
the report that does not occur, which is worse than omitting it — the report's whole use is telling
the next task which classes are real.

### 2. Run the corpus and collect

`build-test-shadgpu.sh`, in batches of about five as T19 does, across the cells disabled against a
schema cause. The existing rollout protocol, unchanged — the only difference is that the failures are
now worth reading.

Collect verbatim. **Do not fix anything, and do not enable a cell.** A cell that now reports a
different cause than its ticket claims is a line in the report, not an edit to the registry.

### 3. The report

`llm-wiki/reports/sink-divergence.md`, and it answers four questions:

- **Which divergence classes actually reach the sink**, by (declared type → exported type), with a
  count of queries for each. This is the table the whole task exists for.
- **Which of them the tickets already name**, and which are new. #183 predicts `Utf8View → Utf8`;
  #187 predicts `Decimal128(p,s) → Decimal128(38,s)`; #191 predicts `Int32 → Int16`. A class nobody
  has filed is the finding worth having.
- **Which cells' failures disagree with the ticket they are disabled against.** The causes are
  ordered, so a cell disabled against one cause may now be reaching another; the registry says one
  thing and the device says another, and nobody has compared them.
- **What the sink cannot see.** Name the classes `declared-schemas.md` intends to catch that produced
  no evidence here, and say whether that is because they do not occur or because they cannot reach
  the boundary. An absent row and an untested row read the same otherwise.

## Restriction

**One site changes.** The error path at `executor/gpu_backend/mod.rs:179` and nothing else. No cast,
no refusal, no registry edit, no ticket closed, no golden regenerated.

**No fix, however obvious.** If the survey makes a one-line fix look irresistible, that is the
strongest possible argument for writing it down and doing it in a task that can be reviewed — the
three parked tasks are what happens when a measurement turns into a change mid-flight.

## Coverage

**No cells.** It enables none and re-tickets none. It is measured by whether the tasks after it are
planned against evidence instead of a sighting.

## What it feeds

- [`declared-schemas.md`](declared-schemas.md) — its thirteen queries were chosen from tickets and
  from reading the type table. This says which classes actually occur and how often, so the list can
  be cut to what matters or extended to what nobody predicted.
- [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — the harness is only worth buying for
  divergences that do not surface at the sink. If everything surfaces here, that task shrinks.
- The rewrite of `casts` and `wire-schema`, both of which are waiting to know whether their ticket's
  account of itself is still true.


---

<!-- archived from llm-wiki/tasks/operator-cases.md -->

**Merged 2026-09-15 as PR #150, squashed to `12e2926a`.**

# Every seq-bearing operator through the harness

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Ninth in the chain, after [`operator-harness.md`](operator-harness.md), which is the whole of
its mechanism. This task adds cases and nothing else: no helper, no production change. Each
row below is either a green comparison or a `bug_` test naming a ticket, and only running it
says which.

## Why two tasks

The harness is proven on operators that cannot hide a wrong helper. Once it holds, every
other operator is a script and a node, and a case that goes red is a fact about the engine
rather than about the harness. Keeping the cases out of the harness task also keeps that diff
reviewable: a reviewer reading the comparator should not be reading sixty cases beside it.

## The matrix

One synthetic batch or a few, a hand-built node over `Given` leaves, `run_both`, `assert_same`.
The right column names the tickets a row may land on; a row with none may still find one.

| Operator | Cases | May land on |
|---|---|---|
| `GpuFilter` | predicate on an int, on a string, on a null-yielding column; with its projection; every row passes; no row passes | |
| `GpuProject` | column copy; int, float and decimal arithmetic; a cast; CASE in both forms; LIKE; a scalar function; a typed NULL literal | [#57](../tickets.md#t57), [#198](../tickets.md#t198), [#187](active-tickets.md#t187) |
| `GpuSort` | asc and desc; nulls first and last; two keys; `fetch` | |
| `GpuAccumulateBatchesAndSort` | several batches; one; none; `fetch` | [#173](../tickets.md#t173) |
| `GpuMergeSortedPartitions` | N lanes each sorted; `fetch`; one lane empty; `Done` before any batch | [#173](../tickets.md#t173) |
| `GpuCoalesceAllBatches` | several batches; one; none | [#173](../tickets.md#t173) |
| `GpuAggregate` | each `PlanAgg`, grouped and global; the single-node shortcut carrying a finalize; grouping sets; a decimal sum's scale | [#187](active-tickets.md#t187) |
| `GpuAggregateBatches` | merge with and without its finalize; arrivals sized to cross the compaction threshold — 1 MiB on the cpu, 64 MiB on the device, so the device's is the one to size for; `merge_m2`; a count merging by sum; an average's digits | [#163](../tickets.md#t163), [#187](active-tickets.md#t187) |
| `GpuEmitPartitions` | 4 lanes and 64, each lane compared as a slot; null keys; two keys; a string key; a decimal key; one lane in and four out | [#184](active-tickets.md#t184), [#95](../tickets.md#t95), [#187](active-tickets.md#t187) |
| `GpuHashJoin` | each of the nine types × one probe batch and two × `null_equals_null` both ways × a residual filter where the matrix allows one; with a projection | [#152](../tickets.md#t152), [#159](../tickets.md#t159) |
| `GpuCrossJoin` | two batches; with a projection | |
| `GpuNestedLoopJoin` | Inner and Left with a predicate; with a projection | [#190](active-tickets.md#t190), [#160](../tickets.md#t160) |
| `GpuLoadParquet` | both backends read one parquet the test wrote from a synthetic batch: one batch per row group; a limit; row groups and a limit together | [#186](active-tickets.md#t186), [#188](active-tickets.md#t188) |

### Empty inputs

Every shape below is its own case, named for the shape, never folded into a loop over types:
a red one must say which combination reached the limit. The frozen surface cannot make a table
out of nothing ([#173](../tickets.md#t173)), an empty build side leaves three join types owing
rows ([#175](../tickets.md#t175)), and a global aggregate over nothing owes its identity row
([#199](../tickets.md#t199)) — so several of these are expected to land as `bug_` tests, and
which ones is the finding.

| Operator | Empty shapes |
|---|---|
| `GpuFilter`, `GpuProject`, `GpuSort` | a zero-row batch; a zero-row batch between two with rows |
| `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort` | no batch at all; one zero-row batch; a zero-row batch among others; a `fetch` over zero rows |
| `GpuMergeSortedPartitions` | every lane `Done` with nothing; one lane a zero-row batch beside lanes with rows; every lane a zero-row batch; lane 0 `Done` before lane 1's rows arrive |
| `GpuAggregate` | a zero-row batch, grouped; global (#199); grouping sets over zero rows |
| `GpuAggregateBatches` | no arrival; one zero-row arrival; a zero-row arrival among others; no arrival under a finalize |
| `GpuEmitPartitions` | a zero-row batch in — N zero-row lanes out, and the lane count is the assertion; a batch whose every row carries one key, so N−1 lanes get nothing; a batch of all-null keys; a stream of zero-row, rows, zero-row |
| `GpuHashJoin`, each of the nine types | a zero-row build batch with probe rows (#175 for Right, Full, RightAnti); build rows with one zero-row probe batch; both zero-row; `build: None`, never probed; a zero-row probe batch between two with rows; only zero-row probe batches then the finish — the finish whose probe produced no keys, #173's one refusing site |
| `GpuCrossJoin` | build empty; probe empty; both |
| `GpuNestedLoopJoin`, Inner and Left | build empty; probe empty |
| `GpuLoadParquet` | a parquet of zero rows |

The shapes the planner refuses are out of reach here too: the hash-join recipe arm asks the
node's capability and panics without one, so an outer join with a residual filter ([#153](../tickets.md#t153))
never reaches an executor and has no row.

The source row is the one that needs a file rather than an upload: a scan's input is a path.
The parquet writer is the test's, over `synthetic`, with the row-group size chosen so several
row groups exist.

Two of those tickets are closed by tasks below this one — [#198](../tickets.md#t198) by
`typed-nulls`, [#175](../tickets.md#t175) by `empty-build`. Their `bug_` tests here are the
record those tasks turn red and delete, which is the regression test each would otherwise
have to write.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/`: six case files — exec, aggregate, accumulate, emit,
  join, source, the aggregate split from exec because it carries its own state fixtures — and
  the kind registry entries. The source file carries the one helper this task adds, a parquet
  writer over `synthetic`, local to it.
- `llm-wiki/tickets.md`: a ticket per new defect; `llm-wiki/build-test.md`: the count.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/gpu_tests/`.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of the harness task, and:

- No new mechanism beyond the parquet writer above. A case that needs a helper the harness
  lacks is a finding against the harness task, not a helper added here.
- Every row above exists as a case. The kind guard the harness carries extends to every
  `NodeRef` kind but the three forwarders, and a kind with no case is red.
- Every divergence gets a ticket before it gets a `bug_` test, and a `bug_` test asserts the
  wrong behaviour precisely — the wrong value, the refusal's message — not merely that the two
  sides differ. The ticket is where the fix is designed, later and by another task; nothing
  here repairs, works around, or casts away what a case finds. A ticket a case cannot reproduce
  stays open — it closes when its corpus cells run — and the detail file says which.
- The join scripts follow the capability matrix: build side one batch and always first, probe
  streamed, `without_build` where the build produced nothing.

## Verification bar

- Every row present; each case green or `bug_` with its ticket in the comment above it.
- The kind guard green with only the forwarders excluded.
- New tickets in `tickets.md`, within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s row for the harness carries the new count; the grand total moves with it.
- Green on shad-gpu; the rust-only lib unchanged.

## Completeness signoff

Solved under its constraints: cases only, nothing outside `src/tests/gpu_tests/` and the wiki,
no mechanism beyond the parquet writer, no fix; every row of the matrix and the empty-input
table a case named for its shape; 204 operator cases, 128 green and 76 `bug_` asserting the
wrong answer or the refusal exactly, each with its ticket above it, going red on the fix; seven
new tickets and fifteen amended by what the device showed; the kind guard with only the three
forwarders excluded; exact comparison with no tolerance anywhere. Deviations, none a shortcut:
two fixture corrections (a padded side declared nullable, as the planner does; sorted runs
dealt round-robin so the merge key carries no ties); a seventh empty shape for the finishing
joins, the only route to #173; `nested_cases.rs` beside the spec's six files; cases beyond the
spec's rows; Welford's moments not compared, a mean not being dyadic — a harness finding; the
`avg` shortcut's count declared Int64 so its finalize can be the subject; eight `bug_` helpers
duplicated across files that belong to the harness; two handoffs written on #198 and #175 for
`typed-nulls` (whose premise the second pin shows false) and `empty-build` (whose driver fix
does not reach the harness's `without_build` route).

---

<!-- archived from llm-wiki/tasks/operator-harness.md -->

**Merged 2026-09-15 as PR #149, squashed to `3440aa05`.**

# One call sequence, two backends: the operator harness

Kind: production

**Closes no ticket and fixes nothing.** The task is a harness and the cases that prove it; what
it finds wrong it reports — a ticket and a `bug_` test — and leaves as it found it. Its two
production edits are mechanism the harness needs, not repairs, and neither changes what any
query answers.

Eighth in the chain, after [`sink-divergence-survey.md`](sink-divergence-survey.md) — a prototype
whose branch is never merged, so this one forks off `visibility`. It lands after
[`test-layout.md`](test-layout.md) on purpose: the harness is crate-level, needing both backends
and a device, and `src/tests/gpu_tests/` is the place that task creates for exactly that shape.
Writing it against `tests/*.rs` first would mean writing it twice.

## Why

Nothing today runs one operator on both backends over the same input and compares the two.
`executor_cases.inc` is eleven rows each side proves against a hand-written answer, and
`test_gpu_executors` asserts the device against answers the test wrote. A per-node divergence
therefore surfaces only at the root of a corpus query, where architecture.md's column-indexing
section says the per-node numbers cannot see it. The corpus is also numeric-aggregate heavy
([#195](../tickets.md#t195)), so most operator shapes have no query reaching them at all.

The harness makes the comparison one call: a hand-built node, a script of batches the test
wrote, both backends, the outputs compared exactly. This task builds it and proves it on the
operators whose recipe has no seq, so nothing in the FlatBuffer can hide a wrong helper.
[`operator-cases.md`](operator-cases.md) then runs everything else through it.

## What it is

**An upload.** `peacock_handle_from_arrow(executor, schema, array, out_handle)`: an Arrow C-data
import through `cudf::from_arrow` — the murmur3 hook at `gpu_executor.cpp:340` already does this
— adopted into the live session's registry as a handle, `NodeSession::adopt(TableResult)`. The
registry lives in the session, so `begin_plan` comes first; the seqless three load a plan of one
stub node, which is a shape the C++ already accepts under any forwarder's parent. The symbol is
test-only and says so where it counts: the header comment, no `AbiSymbol` names it, no recipe
can, and the extern sits in `peacockdb-ffi` beside `peacock_spark_partition_ids`, the test-only
hook already there — architecture.md's count of the ABI becomes seventeen, the conformance
group two. The `GpuBatch` a test wraps the handle in is the existing constructor. The fetch
side exists — `GpuExport` is what `GpuUnload` runs.

**A stub leaf, and the recipe code unchanged but for one arm.** `Given` — the leaf the cpu
backend's tests already have, declaring a schema and a layout and nothing else — moves up to
`src/tests/` and is shared. In `wire/attach.rs` a node outside the registry (`try_as_node_ref`
answers `None`, the case that function exists for) emits the writer's stub — the empty
`CudfScan` it already fills a forwarder's parent slot with, made reachable from `attach.rs` —
and no recipe. The stub is what keeps the plan rooted: the writer refuses a plan with no node,
and so does the C++, so a leaf that emitted nothing under an unload or a limit would leave
nothing to load. `attach_recipes(operator over Given leaves)` is then the operator's recipe, at
the last post-order, and its bytes are what `begin_plan` loads. No second recipe writer, no plan.

**A synthetic batch.** `synthetic(rows, seed)`: one fixed schema — Int32, Int64, Float64, Utf8,
Date32, Boolean, a key column with duplicates, a unique id — nulls in every column, floats
dyadic so sums compare exactly, deterministic from the seed. Zero rows is a legal argument and
a case in its own right. Decimals are a second fixture, `decimals(rows, seed)`, and not a column
of the first: the device exports every decimal at precision 38 whatever was declared
([#187](active-tickets.md#t187), open and owned by no task), so a decimal in every batch
would make every case that ticket's `bug_` test instead of the decimal cases alone.

**A comparator.** `assert_same(cpu, gpu, Order)`: slot by slot — one slot per call, and per
lane for the emitter, since output timing is a function of the call sequence on both backends
and a flattened multiset would pass a scatter that put a row in the wrong lane. A slot both
sides left empty — a call that produced no batch, which a limit outside its interval and a
one-call join's finish legitimately do — is equal; one side empty is a named difference, and it
is not the same thing as a zero-row batch. Within a slot, column names and data types, then
values, exact, after sorting by every column; `Order::AsEmitted` for the sorts, whose synthetic
keys carry no ties because neither engine's sort is stable. Nullability is not compared: the
device reports it from the data rather than the declaration, and the engine's own schema check
(`plan/validate.rs`) ignores it for the same reason. `CallStats` are not compared: the byte
formula is shared and scratch is measured. A type divergence fails — it is the
#183/#187/#191 shape, and a `bug_` test is where it belongs, not a cast in the comparator. The
comparator ships with its own red cases: a differing value, a differing type, a differing row
count, each shown to fail.

**One driver per category, generic over `B: Backend`.** `run_both(node, Script) -> Outcome`,
where `Script` names the call sequence in RecordBatches — `Exec(batches)`,
`Accumulate(batches)`, `Lanes(per-lane events)`, `Emit(batches)`, `Join { build, probe }`,
`Unload { batch, range }`, `Source` — and the harness refuses a script whose shape is not the
node's category. Executors come from `Backend::executors_for` with the post-order the tree
gives, so what is exercised is the trait, not a constructor. `Outcome` carries each side's
`Result`: a one-sided `Err` is the failure a `bug_` test then pins by message.

## Cases in this task

The three operators whose recipe carries no seq, plus the helper round trip:

- upload then fetch: whole, a row range, a range past the end, zero rows;
- `GpuUnload` through `executors_for`: whole, ranged, clamped, a zero-row batch, a range over a
  zero-row batch;
- `GpuLimit`: an interval inside one batch, one straddling two, batches entirely outside it,
  skip only, a stream of several batches, a zero-row batch inside a stream, a stream of nothing
  but zero-row batches, and an interval no batch reaches.

Empty inputs are separate cases everywhere, here and in task 9, one per shape — a zero-row
batch, no batch at all, one side or one lane empty — because the frozen surface cannot make a
table out of nothing ([#173](../tickets.md#t173)) and each shape reaches that limit by a
different route.

And a guard: every `NodeRef` kind is named by at least one case, in both directions, with the
three forwarders as the listed exclusions — they have no executor and belong to the driver,
which is tested elsewhere. Each case declares its kind in a registry the guard reads; the guard
never reads source text. Until task 9 fills the registry, the guard also carries the list of
kinds still without a case, checked both ways like the exclusions, and task 9 empties and
deletes it. It is what makes "comprehensive" a red test rather than a claim.

## Scope

Code expected to change:

- `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`: `peacock_handle_from_arrow`;
  `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp`: `NodeSession::adopt`.
- `peacockdb-ffi/src/lib.rs`: the extern, one entry.
- `peacockdb-core/src/wire/attach.rs`: the `try_as_node_ref` arm in `emit`;
  `peacockdb-core/src/wire/writer.rs`: the stub made reachable from that arm.
- `peacockdb-core/src/tests/`: `Given` lifted from `executor/cpu_backend/tests/` (whose own copy
  goes and whose call sites rename), `synthetic` and `decimals`, the comparator with its red
  cases, the kind registry and its guard.
- `peacockdb-core/src/tests/gpu_tests/`: `Device`, `Script`, `Outcome`, `run_both`, and this
  task's cases.
- `llm-wiki/build-test.md`: one row; `llm-wiki/architecture.md`: the ABI count.
- Nothing in `.github/workflows/`, `plan/`, `planner/`, `executor/`, or the wire format.

Component-level API expected to change:

- The C ABI: one additive symbol, test-only. `NodeSession`, the de facto C++ interface: `adopt`.
- `wire::attach_recipes`: accepts a leaf outside the registry, which emits a stub node and no
  recipe. Its signature and every other `wire/mod.rs` item are unchanged; `Writer` gains one
  `pub(super)` method inside `wire/`.
- `crate::tests` and `crate::tests::gpu_tests`, test modules rather than components: the items
  above are new. No component facade gains or loses an item.

## Constraints

- **No fix, anywhere.** Not in an operator, not in the C++, and not in the harness by casting or
  filtering a divergence away. A divergence or a one-sided failure is a ticket — an existing
  one where the defect is the same, a new one otherwise — and a `bug_` test asserting the wrong
  behaviour with the ticket above it. A ticket whose defect a case here no longer reproduces
  stays open: a ticket closes when its corpus cells run, not when a fix is visible in the code
  or a hand-built case passes. The detail file notes it, and nothing else moves.
- Synthetic data only. No sf1, no `testdata/`; the GPU job needs no dataset for any of this.
- Exact comparison. No tolerance argument exists.
- No driver and no forwarder: the harness calls executors, never `run`.
- Production code moves in two places only: the additive test-only symbol, and the one
  `attach.rs` arm with the writer method it calls. No production behaviour changes.
- Rung discipline as `test-layout.md` set it: the leaf, the batch and the comparator compile
  under `rust-only` in `src/tests/`; everything touching a device sits under
  `src/tests/gpu_tests/` behind `feature = "gpu"`. No new CI line — the lib binary's
  `gpu_tests::` filter already reaches it, and `test_ci_coverage` says so or goes red.

## Verification bar

- The comparator's red cases fail for the reason each names.
- Round trip, unload and limit green on shad-gpu; the kind guard green with the three
  exclusions and the pending list, and shown red once with a kind removed from it.
- `cargo test --features rust-only -p peacockdb-core --lib` still compiles and passes: the
  rust-rung modules carry no device type.
- `test_ci_coverage` green without a workflow edit.
- `build-test.md` gains one row for the harness, in its table's terms.

## Completeness signoff

Solved under its constraints: two production edits, both mechanism — the test-only upload
symbol with `adopt` behind it, and the one `attach.rs` arm — and no behaviour change, the recipe
payloads byte-identical; the harness under `src/tests/` with a driver per category over
`Backend`, exact comparison slot by slot with its red cases shown, and a kind guard red both
ways with the three forwarders excluded and thirteen kinds pending for task 9; the round trip,
seven `GpuUnload` and ten `GpuLimit` cases agreeing on both backends, so no ticket and no `bug_`
test. Deviations, none a shortcut: `pub(crate)` for the spec's `pub(super)`, and
`#[cfg(all(test, feature = "gpu"))]` for its `feature = "gpu"`, both the layout test's forms;
the guard in `gpu_tests/`, since `inventory` collects per binary; one CPU gtest beyond the file
list; `Script` allowing dead code for the four variants task 9 constructs; 26 device cases, not 21.

---

<!-- archived from llm-wiki/tasks/visibility.md -->

**Merged 2026-09-15 as PR #148, squashed to `efb41c72`.**

# 6 — the crate's API becomes the CLI's, and the walls go up

Kind: production

Last of six, after [`test-support.md`](test-support.md). That task put the corpus harness behind
the feature, so nothing outside the crate needs an engine type any more. This one takes the
surface down to what the CLI calls, deletes the registers task 2 had to build, and writes the
rules that keep it there. It is also the sweep: three tasks deferred work to "later" without
naming a task, and this is that task.

The two halves are the same thing from two ends. The **surface**: 174 bare `pub` items become
eight. The **walls**: `pub mod` survives only in `lib.rs`, and `coding-style.md`'s Visibility
section stops carrying an exemption at all.

## Where the surface stands entering this task

Measured after task 2, to be re-measured after task 3:

| | count |
|---|--:|
| bare `pub` items in `src/` | 249 |
| of those, in a component or subcomponent `mod.rs` or `lib.rs` | 174 |
| behind the nine exempt `pub mod` paths, which task 4 demotes | 75 (60 excluding the two subcomponent facades) |
| `pub mod` declarations | 15 — six components in `lib.rs`, nine exemptions |

The two rows are disjoint and sum to 249. Task 4 takes the 75, so this task starts from **174** and
ends at **eight** bare `pub` items in three files, six `pub mod`, and no register — 166 demotions,
every one checked by the compiler. Every other item becomes `pub(crate)`, which is all a sibling
component ever needed: components live in one crate, so a component API is `pub(crate)` and only
the CLI's entry points are `pub`. That is the whole of "the crate's API becomes the CLI's".

## What tasks 1-3 leave here

Each of these is stated somewhere as deferred, parked or open, and none has a task. Closing them
is this task's work, not an appendix to it.

**Formatting and wording residues.** The `parquet_meta.rs` rustfmt hunk task 1 deferred to task 2
was never applied and still reports. Three files were left unformatted in task 2's second review
round — `test_cpu_end_to_end.rs` (which task 4 moves to `src/tests/`), `cpu_backend/expr_physical.rs`, and `corpus_gpu.rs` (which task 5 moves to `src/test_support/`).
About twenty comments still use "mode" as a common noun for the thing task 1 retired.

**Guards that under-report.** `no_public_signature_names_a_type_from_a_private_module` matches only
`alias::`/`module::` prefixes, so a bare type imported out of a private module and named in a `pub`
signature passes; that becomes load-bearing here, where the private set grows by 166 items.
`names_the_module`'s reverse half misses `use peacockdb_core::executor::cpu_backend;` because there
is no `::` after the path. The super-climb reader reports one `super::super::x` at depth 0 twice.

**Two tickets to file rather than fix**, because each is a different subject: the murmur gate
re-derives `pmod` and the seed-42 pre-fill locally instead of calling `rows_per_lane`, so one rule
has two copies; and the repo is not rustfmt-clean, has no `rustfmt.toml`, and `pipeline.yml` runs
neither a fmt nor a clippy step. Only the first earns a ticket: `prompts.md` files tickets for
production behaviour and names cosmetics as the case never filed, so the formatting gap is recorded
in `build-test.md`, where CI shape lives, and not in `tickets.md`.

**One contradiction to correct in writing.** `module-layout.md` and
`peacockdb-core/tests/common/memory_limit.rs` both say `test-layout.md` creates `src/test_support/`.
It does not — it hands the feature and the module here. Fix both call sites in the commit that
moves the file, or the next reader trusts the comment over the spec.

**Baseline tooling outlives its task.** `module-layout-baselines/` is described as scaffolding
deleted when that task is archived, but `visibility-dump.py`, `case-inventory.sh` and
`compare-inventory.sh` are checks in tasks 3 and 4. They already live in `scripts/`, moved there by task 2's
completeness commit; `doc-attr-check.py`, `narrow.py` and `external-names.py` are task
2's own and die with it. This task inherits them there and adds nothing.

## The demotions

Task 3 raises the nine subcomponent walls as it moves the tests that forced them, and demotes the
75 items behind them. What is left here is the other 174: everything a component `mod.rs` declares
for its siblings, which needs `pub(crate)` and has been spelled `pub` because components are
`pub mod`.

**`#![warn(unreachable_pub)]` goes on in the first slice, but it is a backstop, not the work
list.** The lint fires on a `pub` item that is not reachable from outside the crate — and every one
of the 174 sits in a component `lib.rs` declares `pub mod`, so it reports **zero today** and would
report zero after a task that demoted nothing. It cannot measure this work.

`scripts/visibility-dump.py` is the work list: `awk '$2=="pub" && $3!="mod"' | wc -l`, 174 falling
to eight, one slice at a time. What the lint buys is the future — once a component's items are
`pub(crate)`, a `pub` written inside a private module is unreachable and the lint says so, which is
the rule enforcing itself after this task rather than during it. It stays at `warn`: the crate's
warning count is already a checked baseline, so a new one fails that check without a second
mechanism.

Each component's `mod.rs` then keeps only what the CLI needs as bare `pub` and demotes the rest:
`plan/mod.rs` (92 → 0), `executor/mod.rs` (48 → 2), `wire/mod.rs` (20 → 0), `planner/mod.rs`
(7 → 4), `plan_text/mod.rs` (3 → 0), `planner/translator/mod.rs` (2 → 0), `lib.rs` (2 → 2). A
component staying `pub mod` while every item in it is `pub(crate)` is the intended shape: the
module is nameable, its contents are not.

The eight the corpus harness forced go the same way once the harness is behind the feature — they
are ordinary component items with an unusual reason for having been `pub`, and after the move
their reason is gone.

### The hoist is not here, and this is why

Task 2 could not put the backend types behind a `mod` wall, because `test_cpu_executors` and
`test_gpu_executors` were separate crates and a separate crate cannot reach a private subcomponent.
It listed three answers — open the walls with `pub mod` (taken), declare the 14 types in
`executor/mod.rs`, or the full hoist of 14 types and 55 inherent methods — and deferred the third
as "the one the rules ask for", to be done "later at leisure".

All three answer one question: how does a **separate crate** reach those types. Task 3 answers it a
fourth way by ending the separation, so the question is gone rather than deferred.

Two things confirm it rather than assume it. Measured on the post-task-2 tree, exactly one reach
into the backend child modules comes from outside their own directory — `wire/tests.rs:831`'s
`CpuJoin`, which task 4 replaces with a two-hop `has_finish_pass` delegation rather than a hoist. `executor/driver`, the consumer that would justify
the other thirteen, names none of them: it goes through the `Backend` trait. And the hoist's
destination works against this task — types declared in `executor/mod.rs` are there to be `pub`,
while this task takes that file to two bare `pub` items, so hoisted types would land as
`pub(crate)` and be no more reachable than they were in `accumulate.rs`.

No ticket either. `coding-style.md` files tickets for production behaviour and never for
cosmetics, and a rearrangement with no consumer is the cosmetic case exactly. This paragraph is the
record, so the next reader meeting `pub(crate)` items in `accumulate.rs` does not re-derive it.

## The facades, after

Four kinds of boundary, and after this task each is exactly one thing.

- **The crate** exposes eight items in three files. That is the CLI's API and the whole of it.
- **A component** — `plan`, `planner`, `executor`, `wire`, `plan_text` — is a directory whose
  `mod.rs` declares its whole API, `pub(crate)`, reachable by sibling components and by nothing
  outside the crate. `common.rs` is a file rather than a directory: what the components share,
  declared in one place, and already at zero bare `pub`.
- **A subcomponent** — `executor/cpu_backend`, `executor/driver`, `planner/translator` and the rest
  — is declared `mod`, so it is reachable only from inside its parent component, and its API is its
  own `mod.rs`.
- **`test_support`** is a component-shaped child of the crate root whose API is bare `pub` behind a
  feature, with signatures free of engine types.

No register, no exemption, no `CROSS_COMPONENT_REACHES`. A `pub mod` below `lib.rs` is a violation
with no sanctioned form, which is what makes the rule readable at last.

## coding-style.md's Visibility section is rewritten

Task 2 wrote the rules and, honestly, the exemption beside them: nine sanctioned `pub mod`, 60
items behind them, a register checked both ways, and a paragraph explaining why the register is a
register rather than a habit. All of that was true of a tree with test crates reaching in. None of
it is true after task 3.

What goes: the exemption section entire, the `CROSS_COMPONENT_REACHES` paragraph, and the
"nine more `pub mod` exist, every one forced by a test crate" clause.

What stays, unchanged: the component and subcomponent rules, `mod` not `pub mod`, no `pub use`,
`mod.rs` bodies of one expression, three-deep nesting where the innermost earns it, absolute
`crate::` paths across a boundary, the length exemptions for `mod.rs` and `common.rs`.

What arrives:

- **The crate's API is the CLI's.** Bare `pub` in `src/` means "the binary calls this". Everything a
  component exposes to its siblings is `pub(crate)`. A new bare `pub` is a claim that the CLI needs
  it, and the layout test asks for the receipt.
- **`#![warn(unreachable_pub)]` is what keeps that true afterwards**, once the components' items
  are `pub(crate)` and a `pub` written inside a private module is genuinely unreachable. It cannot
  measure the task itself — while a component is `pub mod`, every item in it is reachable and the
  lint is silent. That is also why the rule is not merely a convention afterwards: Inside a private module `pub` and `pub(crate)` are identical to rustc —
  the module's own privacy is the wall — so the distinction is for the reader, for the blast radius
  when a module is ever opened, and for `private_interfaces`, which passes silently over a `pub`
  type that nothing can name and fires on the `pub(crate)` one. The lint is what makes the first of
  those three self-enforcing.
- **`pub mod` appears in `lib.rs` and nowhere else.** Six components; no exemption, no register.
- **A test-support signature is free of engine types.** The rule that keeps a facade from being a
  rename.
- **What the compiler enforces and what the layout test has to.** Task 2's honest three-way split
  survives the rewrite — module privacy is rustc's, sibling reach and where a `pub` appears at all
  are the layout test's, and a `pub` type that is unreachable but nominally public defeats
  `private_interfaces`, so that is the layout test's too.

## The surface, after

Bare `pub` appears **eight times in `src/` outside the feature gate, in three files**.

| Item | Declared in |
|---|---|
| `build_session_state`, `register_tables_for` | `lib.rs` |
| `plan`, `PlanKnobs`, `BatchSizing`, `SMALL_TABLE_BYTES` | `planner/mod.rs` |
| `run`, `CpuBackend` | `executor/mod.rs` |

**The eight are not closed under their own signatures, and the table has to say so.** `plan`
returns `Box<dyn GpuNode>` and `MemoryModel`; `run` takes `&dyn GpuNode` and a `B: Backend` and
returns `RunReport` and `RunError`. A `pub` item whose signature names a `pub(crate)` type is a
`private_interfaces` warning on the very item this table keeps — against a warning baseline this
task checks. So the surface is these eight **plus the types they name**, and the first slice
enumerates that closure from the signatures rather than guessing it: walk the eight, collect every
type in their parameters and returns, and keep those `pub` too. If the closure comes out large,
that is the honest size of the CLI's API and the table grows; what must not happen is eight `pub`
items sitting on types nothing outside can name.

`test_support/mod.rs` is a further file carrying bare `pub` and the only one behind a feature. The
guard distinguishes them: eight unconditional items checked by file and name, a feature-gated set
checked by signature.

## Validation

No test case moves in this task and no golden is touched, so both are pinned as invariants rather
than checked as outcomes. What moves is visibility, and a visibility regression compiles.

### Baselines

1. The `pub`/`pub(crate)` item dump with declaring files, from the end of task 3, taken with
   `scripts/visibility-dump.py`.
2. `--list` for all three lib shapes and the remaining binaries, and their leaf-name sets.
3. `sha256sum` over `testdata/goldens/`.
4. The three registers in `test_module_layout.rs` — `PUB_MODULES`, `CROSS_COMPONENT_REACHES`,
   `PUB_OUTSIDE_A_MOD_RS` — with their entry counts.

### The checks

- **The surface lands on exactly eight**, asserted as the table above — file and item, not a count.
  A count passes when one item is dropped and another added. Every other item must appear in the
  dump as `pub(crate)`, not merely as absent: an item deleted and an item demoted look the same to
  a count and different to this.
- **`pub mod` appears six times unconditionally, all in `lib.rs`**, plus `test_support` behind its
  feature — the same unconditional-versus-gated split the surface table makes. `PUB_MODULES` is
  deleted, not emptied: an empty register is an invitation.
- **`unreachable_pub` reports zero, and is known to be armed.** Run the three build shapes and
  confirm zero — which is weak evidence on its own, since it also reported zero before the task. So
  construct the violation: spell one item in a now-private implementation module `pub`, watch it
  warn, revert. That is the check; the count is not.
- **`CROSS_COMPONENT_REACHES` is deleted**, and the reach it named is already gone — task 4
  replaced it with the `has_finish_pass` delegation when it raised the `cpu_backend` wall.
- **The private-type-in-a-public-signature guard is fixed first, then relied on.** It matches only
  path-prefixed types today; with 166 newly private items it is the guard most likely to be needed
  and most likely to miss. Fix it, prove it red on a bare imported type, then run it.
- **Case counts and leaf-name sets identical to task 3's**, on all three shapes. Nothing moves
  tiers here; a count that shifts means a test followed the harness by accident.
- **Goldens byte-identical.** Nothing in this task can reach them; a diff means the harness changed
  behaviour while moving 698 lines of test code.
- **The residues are gone**: `rustfmt --check` is clean on the four files named above, and no
  comment uses "mode" as a common noun for what task 1 retired.

### Slices

Six components, one slice each, every one compiler-checked: turn the lint on first so its warning
count is the work list, then `plan`, `planner`, `executor`, `wire`, `plan_text`, `common`. After
each, the lint's count and the dump's bare-`pub` count both fall; a slice that moves neither moved
the wrong thing. Each slice ends by appending its state to `test-support-detail.md` and handing
back.

The corpus move is not here at all — [`test-support.md`](test-support.md) did it. That is the cut:
698 lines of test code changing compilation unit is a diff a reviewer must read, and ~300 one-word
demotions is a diff a reviewer can only skim, so they are judged separately or the first hides
inside the second.

## Done when

`peacockdb-core` exposes exactly the eight items in the table, every other former `pub` demoted to
`pub(crate)` rather than deleted; `pub mod` appears six times unconditionally and only in `lib.rs`,
with `test_support` gated beside them; `#![warn(unreachable_pub)]` is on and reports zero; both
registers are deleted and the reaches they sanctioned are gone; case counts, leaf-name sets and
goldens are unchanged; `coding-style.md`'s Visibility section carries rules and no register; the
carry-over list above is closed item by item, with the two tickets filed rather than fixed; and CI
is green.

## Completeness signoff

Solved under its constraints, with the surface at its honest size: 46 bare `pub` items and five
fields in five files — nine the CLI names, seven `plan` closes over, thirty `run<B: Backend>`
does, each hop named by the compiler — asserted by file and name both ways; every other former
`pub` is `pub(crate)`; `pub mod` is pinned to `lib.rs`; both registers are gone; the lint is on
and at zero; goldens, counts and leaf names hold but for one layout case renamed for its new
subject. Deviations, none a shortcut: five facade delegates only tests called were deleted and
three moved into test modules; `wire`, `plan_text` and `executor/gpu_backend` have no production
caller and say so with `cfg_attr` allows; the spec's checked-warning-count premise was false, so
`SURFACE` is the gate and the lint the signal; `RunReport.calls` is read by nothing and ~65
`pub` fields on `pub(crate)` structs stay, both under `allow` or unreachable; 66 comments, not twenty.

---

<!-- archived from llm-wiki/tasks/test-support.md -->

**Merged 2026-09-15 as PR #147, squashed to `a76563f3`.**

# 5 — the corpus harness moves behind the feature

Kind: production

Fifth of six, after [`test-layout.md`](test-layout.md) and before
[`visibility.md`](visibility.md). Task 4 created `src/test_support/` for the helpers that had two
audiences and moved most of them; this task moves the last two files, and with them the reason the
final eight items are `pub`.

Those eight are `GpuNode`, `validate`, `RunReport`, `render_run`, `GpuBackend`, `GpuContext`,
`RecipePlan` and `attach_recipes`. They exist for `test_cpu_corpus` and `test_gpu_corpus`, which
deliberately stay external. Once the harness they share is inside the crate, nothing outside names
an engine type — and [`visibility.md`](visibility.md) can then take the whole surface down to what
the CLI calls.

It is a small task with one hard claim, and that is deliberate: 698 lines of test code changing
compilation unit is a diff a reviewer has to read line by line, and it should not be sharing a
branch with three hundred one-word demotions.

## The corpus facade

`corpus.rs` (508 lines) and `corpus_gpu.rs` (190) — 698 lines — join the helpers already in
`src/test_support/`. `mode.rs` and `memory_limit.rs` moved with task 4, which needed them. Inside
the crate these two reach `pub(crate)` items, so the eight stop being `pub`.

The two corpus targets stay because they are the genuine end-to-end tier — SQL in, rows out, against committed
goldens, 456 cases — and because keeping them as two binaries keeps `inventory`'s
one-binary-per-engine property resting on two `--test` targets rather than on the `gpu` feature
producing two compilations. The feature route would work and would make the registry guard depend
on something that reads as unrelated to it.

The two binaries name **none** of the eight. They call functions whose signatures name no component type: `cpu_case(dataset, sf, query, mode, oracle)`, `authoritative_mode`, `gpu_case`.
That is what makes the facade real rather than a rename. `over_cap` travels with `corpus.rs` and
`test_corpus_goldens` reaches it the same way. `golden_text.rs` and `registry.rs` are already in
`test_support` — task 4 moved them, because targets it moved needed them too. `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` name zero crate items and are read only by binaries that stay,
so they stay in `tests/common/` untouched.

## The mechanism is already here

`test-layout.md` declared the `test-support` feature and the self dev-dependency, because the
helpers it moved had two audiences — in-crate tests and the binaries that stayed — and duplicating
one across the boundary guarantees drift. This task adds no mechanism; it adds the last two files
to the module that mechanism created, and the eight items stop being `pub` as a result.

What `#[cfg(test)]` still cannot do is unchanged: the library is compiled without `cfg(test)` when
cargo builds an integration test, which is the whole reason these eight are `pub` today.

## test_support is shaped like a component

`src/test_support/mod.rs` declares the whole API the integration tests may reach. Task 4 put the
harness half there — `Mode`, `MODES`, `MemoryLimit`, the golden-text reader, the registry loader
and the testdata root — and this task adds `cpu_case`, `gpu_case`, `authoritative_mode` and
`over_cap`. Every module below it is private with `pub(crate)` items. Same rules as a component, for a reason beyond symmetry: the signature
check below is a scan of one file only if the API lives in one file.

Being a child of the crate root it sees component facades and not their internals — the same level
as `src/tests/`, and the reason it reaches `pub(crate)` items without any of them becoming `pub`.

**No `pub` in `test_support` names a type from `plan`, `planner`, `executor`, `wire` or
`plan_text`.** That is the rule, stated by what it forbids rather than by a list of what it
allows — the module holds a `PathBuf` root, a golden-text reader and a registry loader, none of
which a list of "strings, `Mode`, `MemoryLimit`" would have permitted. A
signature mentioning `GpuNode` or `RunReport` puts the item straight back on the surface under
another name, and it compiles. This goes in `coding-style.md` beside the visibility rules, and the
layout test enforces it.

## Validation

No test case moves and no golden is touched. What moves is 698 lines between compilation units,
and the failure mode is a harness that changed behaviour while moving.

### Baselines

1. `--list` for all three lib shapes and the seven binaries, and their leaf-name sets.
2. `sha256sum` over `testdata/goldens/`.
3. The `pub`/`pub(crate)` dump from the end of task 4, taken with `scripts/visibility-dump.py`.

### The checks

- **Nothing under `peacockdb-core/tests/` names any of the eight.** Grep the whole directory, not
  the two binaries: the names live in `tests/common/corpus.rs` and `corpus_gpu.rs` today, so a grep
  of the binaries alone is green before the task starts and proves nothing. A hit means
  the facade is a rename, which is the one way this task can look done and not be.
- **No engine type in a `test_support` signature.** Scan every `pub` in `test_support/mod.rs` and
  assert no parameter or return type comes from a component. Then construct the violation, `pub fn tree() -> Box<dyn GpuNode>`, and watch the
  layout test go red. This is the guard the whole facade rests on and the one that would otherwise
  never be exercised.
- **The feature is off in a plain build**, proven by construction: reference `crate::test_support`
  from a non-test path in `lib.rs`, confirm `cargo build` fails with `E0433`, revert. A passing
  build is not evidence — the module simply is not there to break anything.
- **No CI step passes `--features test-support`.** Grep the workflows and assert absence; if one
  does, the self dev-dependency is not doing its job.
- **`inventory` still sees two binaries.** Both corpus targets keep their own registry assertion
  and both must pass — the property the "keep them external" decision exists to protect.
- **Case counts, leaf-name sets and goldens unchanged.** Nothing moves tiers here; a count that
  shifts means a test followed the harness by accident.

## Done when

`corpus.rs` and `corpus_gpu.rs` are in `src/test_support/`; the two corpus binaries reach them
through `cpu_case`, `gpu_case`, `authoritative_mode` and `over_cap` and name none of the eight;
every `pub` in `test_support` has a signature free of engine types and that guard has been seen
red; a plain `cargo build` cannot name `test_support`; no workflow passes the feature; `inventory`
still sees two binaries; case counts, leaf-name sets and goldens are unchanged; and CI is green.

## Completeness signoff

Solved under its constraints: the corpus harness is inside the crate behind the feature, the
three binaries reach it through signatures free of engine types, nothing under `tests/` names
the eight, the guard has been seen red on the spec's probe and on three further spellings, a
plain build cannot name `test_support`, no workflow passes the feature, `inventory` rests on two
`--test` targets, and goldens, leaf-name sets and the 200 bare `pub` outside `test_support` are
byte-identical to the baselines. Deviations, none a shortcut: `corpus_golden.rs` and
`cost_model.rs` moved too, since `corpus.rs` calls both and `src/` cannot see `tests/` — about
1200 lines, not 698; the facade declares 22 items, not four, for the three other suites that read
it; `Mode::knobs` and `Mode::sizing` narrowed to `pub(crate)`, the rule's first findings;
`read_back` deleted with no caller; `tests/common/mod.rs` survives as a six-line re-export shim.

---

<!-- archived from llm-wiki/tasks/test-layout.md -->

**Merged 2026-09-15 as PR #145, squashed to `1a04f096`.**

# 4 — tests down the source tree

Kind: production

Fourth of six, after [`module-layout.md`](module-layout.md) and
[`rmm-pool-budget.md`](rmm-pool-budget.md), and before [`test-support.md`](test-support.md) and
[`visibility.md`](visibility.md).

249 items in `peacockdb-core/src` are bare `pub`, measured after task 2. Most are `pub` for no
reason anyone can name — that is [`visibility.md`](visibility.md)'s subject. What this task ends is
the subset that is `pub` **because `peacockdb-core/tests/*.rs` are separate crates** seeing the
library the way crates.io would: 75 of them behind nine walls, and a hundred-odd more named
directly. This task moves the eleven targets that force them, plus the murmur gate, down
into `src/`, taking the surface from 108 test-driven items to **eight**. The last eight go in
[`test-support.md`](test-support.md).

Three things happen together, because none is worth a separate pass over the same files: the move,
the visibility sweep, and the separation of test code from production code.

## Why the demand is concentrated

Two files in `tests/common/` force 73 of the 108. `injection.rs` (822 lines) and `rebuild.rs` (623)
take a plan tree apart and put it back together, constructing every node kind and every backend
executor on the way. `join_fixture.rs` adds two more.

The other 2,488 lines of `tests/common/` — corpus, registry, golden text, result text, cost model,
mode table — need nine, and no target that keeps them touches the injector. That is what makes the
split clean rather than a judgement call.

Partial moves do not pay: 108 goes to 79 if only `common/` moves, 50 with the two executor tiers,
15 with three more, and 9 only when all eleven targets go. Do it once.

### Where a feature-gated helper pays, and where it does not

`#[cfg(test)]` cannot help an integration test at all: the library is compiled without `cfg(test)`
when cargo builds one, so a `cfg(test) pub` item is unreachable from `tests/`. That is the whole
reason these 108 exist. A **feature** can, and the mechanism costs nothing at the call site:

```toml
[features]
rust-only = ["peacockdb-ffi/rust-only"]
gpu = []                # device tests; not propagated to peacockdb-ffi, which has no device path
```

`gpu` and `rust-only` are mutually exclusive and a `compile_error!` says so. `gpu` does not
propagate to `peacockdb-ffi`: that crate is a C ABI binding with no device-conditional code, and
adding a feature there would be a knob nothing reads.

**The `test-support` feature is declared here, because here is where a helper first has two
audiences.** Nine of the eleven moving targets read `tests/common/` — `golden_text`, `registry`,
`mode`, `memory_limit` and seven `mod.rs` helpers between them — and the seven binaries that stay
read the same files from outside the crate. An in-crate `#[cfg(test)]` module cannot see
`tests/common/`, and duplicating a helper on both sides of the boundary guarantees the drift.

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so no CI step passes a flag and a plain build cannot see the module. `src/test_support/` is then
the one place a helper with two audiences lives: `crate::test_support::…` from inside,
`peacockdb_core::test_support::…` from the binaries.

**A helper moves when a moving target needs it, and not before.** `golden_text.rs`, `registry.rs`,
`mode.rs`, `memory_limit.rs` and the shared `mod.rs` helpers move; `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` are read only by binaries that stay, so they stay too, and
`corpus.rs`/`corpus_gpu.rs` are [`test-support.md`](test-support.md)'s whole subject.

**It does not pay for the injector.** `test_gpu_executors` (46 items) constructs `GpuAccumulator`,
`GpuEmitter` and `GpuJoin` and calls their methods; `test_cpu_executors` (24) does the same on the
other backend; `test_null_analysis` (18) hand-builds plan nodes. A facade over those re-exposes the
same vocabulary under test-local names — indirection, not encapsulation. Those targets move
in-crate instead, which is what takes 108 to single digits.

**It does not pay for the corpus harness either, in this task.** `corpus.rs` and `corpus_gpu.rs`
force the eight items that survive here, and moving them behind a feature is
[`test-support.md`](test-support.md)'s whole subject. They stay in `tests/common/` until then.

## Test code is never in a production file

Added to `coding-style.md`.

- No test code in a file that also carries production code. A module's unit tests are a child
  module of their own in their own file: `validate.rs` with `validate/tests.rs`, which is already
  the pattern in `expr_physical`, `accounting`, `index`, `scheduler`, `single_partition` and
  `expr_writer`.
- A test-only helper in `src/` lives in a file or directory whose name carries `test`, so a reader
  can tell test-only code from production code by the path alone. `driver/mock.rs` and
  `driver/plans.rs` are `#[cfg(test)]` today and do not say so in their names; they become
  `driver/tests/mock.rs` and `driver/tests/plans.rs`.
- Every test module declares the lowest rung it needs, per the ladder below, and its name carries
  that rung — `tests`, `ffi_tests`, `gpu_tests` — so a CI line can select one rung by path.

A dozen or so files hold inline `#[cfg(test)] mod tests { … }` and are split. **Do not work from
a list written here**: task 2 renamed and moved most of them and deleted `config.rs` outright, so
the count and the paths are both stale. Derive the set at the time of the move — the layout test's
own rule names them — and record what you found.

The mod.rs rule from `module-layout.md` applies to components and subcomponents, not to
implementation modules — an implementation module with unit tests is `foo.rs` beside `foo/tests.rs`.
Do not reach for `clippy::mod_module_files` to enforce the mod.rs rule: it cannot be scoped that
way and would reject exactly this pairing. The layout test can scope it, and has to exist anyway.

## The device guard becomes a feature

Today no `cfg` says "needs a device". `rust-only` says "no FFI linked", and `test_gpu_batch` is
`#![cfg(not(feature = "rust-only"))]` while running on a GPU-less runner. What actually keeps device
tests off CPU hosts is which binary CI runs where — and that mechanism disappears the moment those
tests are inside `--lib`.

Add a `gpu` feature. The three build shapes are a **ladder**: each rung adds a capability and
keeps everything below it.

| Build | FFI linked | Device assumed | Runs |
|---|:-:|:-:|---|
| `--features rust-only` | no | no | dataset-matrix, CPU steps |
| default | yes | no | dataset-matrix, FFI steps |
| `--features gpu` | yes | yes | shad-gpu only |

So a test declares the **lowest rung it needs**, and nothing declares what it excludes:

```rust
#[cfg(test)] mod tests;                                       // pure Rust
#[cfg(all(test, not(feature = "rust-only")))] mod ffi_tests;  // needs the FFI linked
#[cfg(all(test, feature = "gpu"))] mod gpu_tests;             // needs a device
```

Two conditions, both positive in meaning, and the sets nest rather than partition: the FFI set is
a subset of what a default build compiles, and a `gpu` build compiles all three. A reader answers
"which builds run this" from one attribute. `rust-only` is the only spelling available for the
middle rung — cargo features are additive and `rust-only` is subtractive, so there is no positive
`ffi` feature to write and inventing one would mean every ordinary build had to ask for the FFI
by name. A `compile_error!` on `all(feature = "gpu", feature = "rust-only")` keeps the ends of
the ladder exclusive.

**A module's name carries its rung whenever the rung is above the floor**, and that is what makes
a rung selectable. Cumulative shapes are the point of the ladder, but a CI line that runs a whole
shape re-runs every rung beneath it: unfiltered, the device host would drag the CPU unit cases
through `--test-threads=1` on the one serial resource in the suite, and the default-features line
would re-run what the `rust-only` line just ran.

| Rung | Module | Selected by |
|---|---|---|
| pure Rust | `tests` | nothing — it is the floor |
| FFI linked | `ffi_tests` | `-- ffi_tests::` |
| device | `gpu_tests` | `-- --test-threads=1 gpu_tests::` |

Those are path filters, not name filters, so they cannot suffer the trap `build-test.md` records
for filters that name a query. The layout test asserts name and gate imply each other in both
directions at both rungs: an `ffi_tests` module carries the `not(rust-only)` gate and a `gpu_tests`
module the `gpu` gate, and no module carries either gate without the matching name. Each CI line
then lists exactly its own rung, which is what makes "one line per rung" an accounting rather than
a slogan.

Two alternatives were considered and are not open. A runtime device check that skips is the shape
`build-test.md` already records shipping a hole — the binaries skip and exit 0, green having
verified nothing, which is why CI asserts sf40's presence itself. And `#[ignore]` already means
"disabled against a ticket" (#182), so reusing it for "needs a device" is one spelling for two
things.

## What moves into src/

Target names are the ones tasks 1 and 2 left behind, not the ones this spec was first written
against. The rung column is the ladder above: `rust` needs nothing, `ffi` needs the FFI linked,
`gpu` needs a device and lands in a `gpu_tests` module.

| Lands in | From | rung | N |
|---|---|:-:|--:|
| `plan/tests/` | `test_layout_injection` | rust | 4 |
| `planner/tests/` | `test_planner_join_capability` | rust | 13 |
| `planner/tests/` | `test_planner_join_refusals` | rust | 10 |
| `planner/tests/` | `test_null_analysis` | rust | 8 |
| `planner/tests/` | `test_plan_goldens` | rust | 19 |
| `wire/gpu_tests/` | `test_gpu_recipe_walk` | **gpu** | 10 |
| `executor/ffi_tests/` | `test_gpu_batch` | **ffi** | 3 |
| `executor/cpu_backend/tests/` | `test_cpu_executors` | rust | 1 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_executors` | **gpu** | 31 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_abi` | **gpu** | 4 |
| `executor/cpu_backend/gpu_tests/` | `test_murmur_conformance` | **gpu** | 10 |
| `src/tests/` | `test_cpu_end_to_end` | rust | 26 |
| `src/tests/` | `common/{injection,rebuild,join_fixture}.rs` | — | 0 |
| `src/test_support/` | `common/{golden_text,registry,mode,memory_limit}.rs` and the shared `mod.rs` helpers | — | 0 |
| | | | **139** |

`test_gpu_executors` is already a directory of five modules — `accumulate`, `backend`, `contract`,
`exec`, `join` — and moves as one, keeping that shape under `gpu_tests/`. `test_gpu_batch` is the
only occupant of the middle rung, which is why it is the shape proof the slices start with.

55 of the 139 are device cases. The injector and the rebuilder land at crate level rather than in
`plan/` because they construct backend executors as well as plan nodes, so they sit above both.

`src/tests/` is a crate-level `#[cfg(test)] mod tests;` declared in `lib.rs` — a peer of the
components that can see all of them. It holds the end-to-end tier and the shared test support the
component tests reach through `crate::tests::…`. The layout injector and the tree rebuilder live
here, not in `plan/` or `wire/`: they construct plan nodes, wire recipes and backend executors
alike, so they sit above every component rather than inside one.

### What makes test-only actually test-only

Three layers, and only the first is discipline.

- **The path says so.** `src/tests/…`, `component/tests/…`, `module/tests.rs`. A reader can tell
  test code from production code without opening the file.
- **One `#[cfg(test)]` at the root of each test subtree, and the gate is transitive.** `src/tests/`
  is excluded from a non-test build entirely — its files are never compiled, so `mod.rs` and the
  files below it carry no attribute of their own. Production code that references anything inside
  is a hard error in the release build (`E0433: failed to resolve`), not a warning and not a
  lint. That is the guarantee: it is the compiler, not a convention.
- **The layout test checks the shape**, because three things the compiler is happy with would
  still be wrong: a `#[cfg(test)]` attribute anywhere other than on a test-module declaration —
  which is how test code creeps back into a production file one item at a time — `coding-style.md`
  now says why the attribute cannot double as a `dead_code` silencer, and roughly twenty item-level
  uses across seven files answer to that, not the four this spec once named. `planner::translate`,
  `planner::translate_expr`, `executor::physical_expr` and `plan::state_for` — whose caller is
  `planner/translator/schema_tests.rs` — are the documented set, and they are
  **test helpers, not production items**: each exists to hand one test in one other component a
  fact it cannot reach itself. They keep `#[cfg(test)]` and stay in their component's `mod.rs`,
  because visibility pins them there — a helper must name what its own component owns while its
  caller is elsewhere, so the two can never sit together. That is the one carve-out to "a
  test-only path carries `test` in its name", and `coding-style.md` now states it. Each has
  exactly one caller today and each doc comment must name it; `translate`'s says "three of them,
  in two other components" and is already wrong, which is why the layout test reads the comment
  and the callers together rather than trusting either. **The chain still collapses**:
  `planner::translate` calls `Translator::new(..).translate(plan)` directly and
  `translator::translate` — `cfg(test)` too, and called only from here — is deleted, twelve lines
  out. What those fifteen lines buy is `plan_text/tests.rs` entire: 236 lines checking the
  renderer against what the planner emits from real SQL, which neither `plan()` nor a hand-built
  tree can stand in for. A name and a
  gate that disagree at either rung — `ffi_tests` without `not(rust-only)`, `gpu_tests` without `gpu`,
  or either gate on a module named `tests` — since the runs select by path and a mismatch either
  loses a case or drags it onto the wrong host; `driver/partitioned.rs` carries four
  item-level `#[cfg(test)]` attributes today (lines 616, 621, 626, 631); they are the first
  thing the first rule finds, and they move into the module with the tests that use them.

### Testdata paths move with the tests, and #49 is in the way

`tests/common/mod.rs` honours `PEACOCK_TESTDATA_DIR`, overriding the compile-time root "so a binary
built on one host can run on another". Every target that says `mod common` inherits that, including
the ones staged to shad-gpu. Nothing in `src/` does: seven sites bake the path instead —
`env!("CARGO_MANIFEST_DIR").join("../testdata/tpch.minimal")` in `planner/memory_estimation.rs` (×3),
`planner/translator/scan_mapping/parquet_meta.rs`, `plan_text/tests.rs` and
`planner/translator/{tests,schema_tests}.rs` — the list `tickets.md` keeps under #49. That is
the residual [#49](../tickets.md#t49) names.

Moving eleven targets in-crate walks straight into it: they lose `testdata_root()` and land beside
the seven that do it the unportable way, and the device ones then run on shad-gpu **from a binary
built on another host**, which is the case the variable exists for.

So this task closes that residual, and `test_support` is where the root belongs rather than
`src/tests/` — the moved targets, the crate's own unit tests and the seven binaries that stay all
need it, which is the same two-audience argument the feature exists for. `test_support/testdata.rs`, declared in `mod.rs`
honours `PEACOCK_TESTDATA_DIR` with the same compile-time fallback; `tests/common/mod.rs`'s
`testdata_root()` becomes a call to it rather than a second implementation, the seven `src` sites
call it too, and #49 closes with the sweep. Two spellings of one rule is what that ticket is
about, so any second copy re-files it one layer down.

### The exemption expires here, one slice at a time

`coding-style.md` says it outright: nine subcomponent paths under `executor/` are declared
`pub mod` rather than `mod` because test crates reach them, 60 bare `pub` items sit behind those
nine — 75 counting the two subcomponent facades the layout test skips — and "`test-layout.md` moves those test files into `src/`, and the whole exemption expires
with them". This task is where that happens, and it is not a consequence — it is work.

`PUB_MODULES` in `test_module_layout.rs` holds nine entries, each naming the test files that force
it, and **it is checked both ways**: an entry whose named files no longer force it is reported, and
a `pub mod` that is not in the register is a violation. The check is per **forcing file**, so a slice
that moves one edits every `forced_by` list naming it — five entries name `injection.rs` — and
demotes only the entries whose last forcer has now gone. Dropping an entry another target still
forces breaks that target and trips the reverse half of the check. Editing the list and demoting
what it empties are the same commit; leaving either for later is a red build, not a tidy-up.

Which slice closes what follows from the register's own `forced_by` lists: the injector trio takes
`cpu_backend/join` and `cpu_backend/source`, `test_cpu_executors` takes the rest of the
`cpu_backend` group, and `test_gpu_executors` and its child files take all four `gpu_backend`
entries. Task 2 recorded the trade this creates and left it deliberately: if
`test_gpu_executors.rs` alone stops naming `executor/gpu_backend`, the forward half goes red and
the three child-naming files cannot re-justify the entry. That is why the target moves whole.

**The one reach that blocks a wall is answered without a hoist.** `wire/tests.rs` names
`executor::cpu_backend::join::CpuJoin`, so raising the `cpu_backend` wall is an `E0603` on that
line. The register concluded that no delegation could carry it — true of the *type*, and it
stopped there. The test does not want the type: it builds a join per (join type, residual) cell
and asks one question, whether the executor makes a finish pass, to compare against the recipe's
`AtDone` call. So the components declare the question instead:

    // cpu_backend/mod.rs
    pub(crate) fn has_finish_pass(node: &GpuHashJoin, build: &ArrowSchema, probe: &ArrowSchema,
        ctx: Arc<TaskContext>) -> Result<bool, PlanError>

    // executor/mod.rs — the same signature, delegating to the above
    pub(crate) fn has_finish_pass(...) -> Result<bool, PlanError>

**Two delegations, not one**, because the wall this task raises is the reason: once
`cpu_backend/mod.rs` declares `mod join;` privately, `executor/mod.rs` cannot name
`cpu_backend::join::CpuJoin` either. That is the shape the tree already uses — `executor::physical_expr`
delegates to `cpu_backend::physical_expr`, which reaches `expr_physical`. Each body is one line and
drags nothing.

Take it **in the slice that raises the `cpu_backend` wall, before the demotion**, or that slice's
own `cargo test --lib` cannot pass: `wire/tests.rs` still reaches through the wall until the
delegation exists. `CpuJoin::makes_a_finish_pass` is renamed `has_finish_pass` with it, per the
predicate rule in `coding-style.md`. No hoist is needed here or in the tasks after.

`CROSS_COMPONENT_REACHES`'s single entry is that same reach and dies with it. The register itself
is deleted in task 4, not here: this task empties it, and an empty register is still a register.

## What stays a separate binary

| Target | Why it cannot fold in | rung | N |
|---|---|:-:|--:|
| `test_cpu_corpus` | `inventory` collects per linked binary; the registry needs two | rust | 448 |
| `test_gpu_corpus` | the other half of that pair, and it writes env vars | **gpu** | 8 |
| `test_golden_format` | the format reader, over strings | rust | 26 |
| `test_corpus_goldens` | committed sections against their own arithmetic | rust | 20 |
| `test_ci_coverage` | reads the workflow yaml | rust | 6 |
| `test_cost_model` | `.cost.txt` re-derived from `.cpu.txt` | rust | 3 |
| `test_module_layout` | reads the tree, as `test_ci_coverage` reads the yaml | rust | 11 |

`test_module_layout` is task 2's, not this task's to write — **this task extends it** with the
rules below rather than inventing a layout test. Where this spec says "the layout test", it means
that target.

None of the seven names an item that would otherwise have to stay `pub` beyond what `corpus.rs` and
`corpus_gpu.rs` already force. The `inventory` constraint
survives untouched, which is the one that looked fatal: it collects per linked binary and the two
corpus binaries both stay.

## test_ci_coverage shrinks to about 300 lines

It keeps its job and loses most of its subject. From 720 lines:

- The target sweep now covers seven targets, not nineteen, and `INTENTIONALLY_NOT_IN_CI` drops from
  six entries to two.
- The three GPU target lists become one staging list of one binary plus the `--lib --features gpu`
  run.
- Its own matcher unit tests stay whole. They are the reason this guard can go red at all.

It gains assertions, and they are the most important ones in the file, because the ladder puts one
CI line under each rung and nothing else says a rung stopped running. **Four things must exist**:
a `--lib` step under `--features rust-only`, a `--lib -- ffi_tests::` step at default features,
the staged lib binary in the GPU job's array **with `gpu_tests::` reaching it in the run loop**,
and the CLI build. The third is the one that is not a command line — the guard reads the staging
array and the loop, the way it already reads `for t in …` today. Each names its rung's filter, so each
lists exactly its own rung: the device line carries the 55 cases that move into `--lib` in this
task, and the default-features line the three of the middle rung, under `-- ffi_tests::`. Each gets the red-watch below — delete
the line, confirm the guard fails — because each is the only thing standing between a rung and
silence.

## Renames

**`test_inc2_conformance` becomes `test_murmur_conformance`**, and that closes a documented
exception. `coding-style.md`'s Names section opens by admitting the name breaks its own rule —
"named after an increment, which the second bullet forbids" — and justifies keeping it because
renaming would move the staging array, the exemption list and two pages. This task moves all three
anyway. Delete that paragraph; the rule no longer needs an apology beside it.

Sixteen references in ten files: `scripts/build-test.sh` (5), `pipeline.yml` (2),
`build-test.md` (2), and one each in `build-test-shadgpu.sh`, `test_cpu_executors.rs`,
`test_ci_coverage.rs`, `executor_cases.inc`, `coding-style.md`, `architecture.md` and
`cpp/tests/gpu/test_cudf.cpp`.

It is also the lowest-level test in the suite and moves in-crate with the rest: it names nothing
from `peacockdb_core`, driving comet's `create_murmur3_hashes` and one FFI symbol over raw arrays.
It lands beside `spark_partitioning.rs`, the CPU half of the invariant it protects.

That placement surfaces a gap worth a ticket, not a fix here: the test re-derives `pmod` and the
seed-42 pre-fill locally rather than calling `rows_per_lane`, so it proves the kernel matches comet
while the code that actually places rows is only transitively covered. One rule, two copies.

One of the five references in `build-test.sh` is not a comment: line 309 is a literal
`peacockdb-core:test_inc2_conformance` in a hand-maintained list, and the comments around it record
that this is the one test file the suite derivation does not catch by pattern — it was silently
skipped once. **Delete that literal rather than renaming it.** The target stops existing: it becomes
an in-crate `gpu_tests` module, and this task's own list leaves `RUST_TESTS` with one entry. A
renamed literal would name a `--test` target that is not there, which is the failure
[#176](../tickets.md#t176) describes — cargo errors late, inside the cuDF leg. The ten lines of
explanation go with it, as below. Re-count the sixteen references before editing: tasks 1 and 2
moved several of these files.

`driver/mock.rs` and `driver/plans.rs` become `driver/tests/mock.rs` and `driver/tests/plans.rs`.
`translate/schema_tests.rs` already carries the word and stays.

## build-test.md

The test table is restructured in this task, not a later one, and the axis changes. Today one
table carries every language and every kind of test; it becomes two.

**The first table is the Rust tests against production code** — `peacockdb-core` and
`peacockdb-ffi` — **blocked by rung**, because the rung is what decides where a case can run and
each block's total is one CI line's case count. A bolded header row opens each block; within a
block the rows are grouped by tier, then by category; and the Why, Examples and N columns survive
as they are.

    **cpu — `--features rust-only`: no FFI, no device**
      crate integration, external
      crate integration, internal
      component · subcomponent · module unit
    **ffi — default features: FFI linked, no device**
      component
    **gpu — `--features gpu`: shad-gpu only**
      crate integration, external · component · subcomponent

The `Runs` column goes. It said which CI job runs a row, and once the repo guards move to the
second table there is no variation left inside a rung: cpu and ffi are dataset-matrix, gpu is
shad-gpu, and the block header says so once instead of every row repeating it.

**The ffi block has two rows and five cases** — `test_gpu_batch`'s three and `peacockdb-ffi`'s
`test_ffi` — and the table should show that rather than pad it. A thin rung is a fact about this
engine: almost nothing needs the FFI linked and no device.

**The second table is everything else**: the C++ suites, the Python prototype and validators,
`cost-report`, and the repo guards — `test_ci_coverage`, which reads the workflow yaml,
`test_module_layout`, which reads the source tree, and `test_golden_format`, which tests the
harness's own format reader. None of the three runs engine code, and grouping them by what they
guard is more use than filing them by a rung they do not have.

`test_corpus_goldens` stays in the *first* table, cpu rung, crate integration external. It runs no
engine code either, but its subject is the engine's output and it goes red when the rendering
drifts, which is the distinction that matters.

Assign each file by where its test module is declared, not by eye: `driver/accounting/tests.rs` is
a unit test of an implementation module, `driver/tests/` is the subcomponent's, and `nodes/tests/`
is `plan`'s. The cross-checks are one per rung plus the binaries, and **every figure in this table
is pre-task-2**: it predates `test_module_layout`'s cases and five target renames, so rebuild it
from the baselines. The arithmetic that must hold is the page's, not this spec's — the two tables
add to the headline figure.

The two tables still have to add to the page's headline figure, which is how the page is checked.
Task 2 reported a four-case discrepancy there and fixed it in its completeness commit, so the
arithmetic is sound entering this task; keep it sound rather than re-deriving it.

Two facts the new shape makes visible that the current table cannot. The gpu block is what
shad-gpu runs, entire — **55** inside `--lib` selected by `gpu_tests::`, plus **8** in one binary —
so the cost of the serial host is one number a reader can find. And the coverage distribution
survives inside the cpu block, where `executor/driver` and `plan` are the heaviest subcomponent
and component rows and the corpus's 448 is one target rather than a tier.

## The three GPU target lists, and the two scripts

Today five GPU targets are named in three places that `test_ci_coverage` asserts agree:
`build-test.sh`'s `gpu_runtime_targets()`, `build-test-shadgpu.sh`'s `RUST_TESTS` at line 30, and
`pipeline.yml`'s staging loop. After this task the list is **one target plus a lib build**, which is
a bigger change to those scripts than to the lists.

**`build-test-shadgpu.sh`**

- `RUST_TESTS=(test_gpu_corpus)` — one entry.
- `stage_cargo_test_binary` resolves a built binary by matching `target.name` against a `--test`
  name in cargo's json. It needs a second form for the lib test target, whose json entry has
  `kind: ["lib"]` and `test: true` and whose `target.name` is the crate name. Stage it under an
  explicit filename — `peacockdb_core_gpu_lib` — because the run loop globs
  `cpp/install/rust-tests/*` and a bare crate name reads as ambiguous beside the target binaries.
- The build must pass `--features gpu`, and **the lib binary alone takes the `gpu_tests::`
  argument**: under the ladder it holds every rung, so an unfiltered run would put the CPU unit
  cases through `--test-threads=1` on the one serial host. The loop passes arguments to every
  staged binary alike, so this is a per-binary argument the loop does not have today.
- **The zero-test guard becomes load-bearing, and the two runners disagree about it.** A path
  filter that matches nothing runs no cases and exits 0, so a rename of the `gpu_tests` convention
  would be invisible without it. `pipeline.yml` arms it unconditionally — it reads `running 0
  tests` from the log and fails. `build-test-shadgpu.sh` suppresses it exactly when a filter is
  set (`elif [ "$rzero" -eq 1 ] && [ -z $filter_q ]`), on the reasonable ground that a
  `PCK_TEST_FILTER` legitimately matches nothing. **The lib binary's `gpu_tests::` argument must
  therefore not travel as `PCK_TEST_FILTER`**: it is part of what the binary is, not a
  developer's selection, and the guard must stay armed for it. Keep the two distinct in the script
  or the backstop is off in the one place a developer runs the suite by hand.

**`build-test.sh`**

- `gpu_runtime_targets()` shrinks to `test_gpu_corpus` plus the lib entry.
- **Ten lines of comment above it become obsolete and should go.** They explain that
  `test_inc2_conformance` is the only file gating per item rather than per file, which is why the
  membership test could not see it and why the list is written out rather than derived. After the
  move it is an in-crate `mod tests` gated on `feature = "gpu"` like every other device test, and
  the special case it documents no longer exists. Delete the explanation with the exception.
- `needs_cmake_targets()` derives from a file-level `#![cfg(not(feature = "rust-only"))]`, which
  still works and now finds one target. `--lib` is not a `--test` target, so nothing derives it —
  it must be named, in both the `--gpu` and the `--rust-only` paths, the way `pipeline.yml` names
  it today.
- The `--rust-only` path builds `--lib` without `gpu`; the `--gpu` path builds `--lib --features
  gpu`. Under the ladder these are not disjoint — the second is a superset — and the filter is what
  makes the runs disjoint. Say so where the lists are written, or the next reader reads the
  supersetting as a bug.
- The "derived suite must not be EMPTY" guard stays and gets closer to firing. With one derived
  target left it is one move away from being the thing that catches a mistake, so leave it.

**`pipeline.yml`** takes the same three-list change, plus one new step and one changed loop.
**shad-gpu never runs cargo** — the job stages prebuilt binaries and the remote loop executes
`$REMOTE_DIR/cpp/install/rust-tests/*` with `--nocapture --test-threads=1`. So the device rung is
a staged binary, not a command line: the staging array gains the lib target and the run loop
passes `gpu_tests::` to that one binary and to no other. The new step is the middle rung: `cargo test -p peacockdb-core --lib -- ffi_tests::` at default features on
dataset-matrix, which
**replaces the `--test test_gpu_batch` step it retires** — the job already compiles that feature
shape for `test_gpu_batch` and `peacockdb-ffi --test test_ffi`, so this is a swap, not a second
compile of the DataFusion stack, and the cache-thrash rule in `build-test.md` is not in play.
**`test_ci_coverage`** then compares three one-entry lists and asserts the four lines above.

## The surface, after this task

**Two counts, and only one of them is this task's.** Bare `pub` in `src/` is a raw number in the
hundreds — 249 measured after task 2. This task takes the 75 behind the nine walls, because a
`pub` item in a module it has just made private is what `unreachable_pub` is for and what the
demotion of that wall means; the other 174 are `pub` for no reason anyone can name and are
[`visibility.md`](visibility.md)'s subject.
What this task ends is `pub` **that an external consumer forces**, and that count reaches eight.

Sixteen items are `pub` for a reason at the end of this task, in six files: the CLI's eight, plus `GpuNode` and `validate`
(`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`, `GpuBackend` and
`GpuContext` (`executor/mod.rs`), and `render_run` (`plan_text/mod.rs`). All eight of those are
forced by `tests/common/corpus.rs` and `corpus_gpu.rs`, and [`test-support.md`](test-support.md)
removes them.

Five of them were lifted out of a subcomponent by task 2 and must stay lifted: `run` and
`RunReport` from `executor/driver`, `CpuBackend` from `executor/cpu_backend`, `GpuBackend` and
`GpuContext` from `executor/gpu_backend`, all declared in `executor/mod.rs` today. That is the rule
doing its work — the wall forces whatever must be public upward — and this task must not push any
of them back down.

## The wiki this moves

- **`build-test.md`'s test table is restructured here**, per the section above: the Rust tests
  against production code in one table blocked by rung — cpu, ffi, gpu — grouped by tier and then
  category inside each block, with Why, Examples and N kept and `Runs` dropped; the C++ suites,
  the Python sets, `cost-report` and the three repo guards in a second. The two must add to the
  headline figure, and the four-case discrepancy that sum has today is closed here.
- **`coding-style.md` gains the test-code rules** — no test code in a production file, a module's
  unit tests in a child module of their own, `test` in every test-only path, and the rung ladder:
  a test module declares the lowest build shape it needs and its name says which — `tests`,
  `ffi_tests`, `gpu_tests` — with name and gate implying each other at both rungs above the floor. It also loses the `test_inc2_conformance` exception paragraph that opens its Names
  section, since the rename closes it.
- **`coding-style.md`'s visibility section is amended, not rewritten**: the previous task states
  the rules, this one records what this task settled — the exemption register empty, `pub mod` down
  to six, and eight items still `pub` because a test crate forces them, which
  [`test-support.md`](test-support.md) unforces and [`visibility.md`](visibility.md) then removes
  along with the rest of the raw count. The
  `test_support` signature rule belongs to [`test-support.md`](test-support.md), which is where
  the corpus facade makes it load-bearing; the feature itself arrives here.
- **`architecture.md`** needs one line, not four: task 2 already corrected the driver and
  accountant paths, the wire-format writer paths and the `spark_partitioning.rs` pointer. What is
  left is the conformance gate's name at `architecture.md:971`, which this task's rename moves.

## Validation

This task moves 8,450 lines of test code between compilation units. Nothing it touches may change
what the engine computes, and no case may be lost — the two risks are opposite in kind and are
checked differently.

### Baselines

1. `--list` for `--lib` and every integration target, reduced to **leaf names** — strip module
   paths, because that is the set the move must preserve while the paths necessarily change.
2. `sha256sum` over `testdata/goldens/`. No golden may move at all in this task.
3. The `pub`/`pub(crate)` item dump from the end of task 2 — **it is a script, not a memory**:
   `visibility-dump.py`, whose output at the end of that task is `visibility-final.txt`.
   Re-run it here rather than inventing a second count, and state which of its rows the ladder
   below counts: bare `pub` only, test-gated items excluded. `case-inventory.sh` and
   `compare-inventory.sh` beside it are the leaf-name tooling for (1). All three already live in
   `scripts/`: task 2's completeness commit moved them there, because they are checks in this task
   and the two after it.
4. Warning counts from clean builds in all three feature shapes.

**Every figure in this spec predates task 2** — which renamed five targets, added
`test_module_layout` and its 35 cases, and moved the tree. Take the baselines first and work from
them. Where a measured number and a number written here disagree, the measurement is right and the
sentence is stale; say so in the detail file rather than bending the move to fit the page.

### The invariant: the case set is preserved, the paths are not

A case that disappears here is silent. Nothing goes red — a target runs 30 tests instead of 31, and
no assertion knows it was meant to run 31.

- **Leaf-name set equality.** The union across the three lib shapes and the seven binaries must
  equal the baseline union exactly. Not the count — the set, so a case deleted and another
  duplicated cannot cancel out. The lib shapes nest, so the union is taken after dedup; that is a
  consequence of the ladder and not a smell.
- **The rungs are what the counts must show**, each a separate assertion: `--lib` under
  `rust-only` lists the pure-Rust set; `--lib` at default features filtered by `ffi_tests::` lists
  **exactly the three** middle-rung cases; `--lib --features gpu` filtered by `gpu_tests::` lists
  the device set **and nothing else**. If either filtered run selects a case from a lower rung, a
  name and a gate disagree and the case is about to run on the wrong host.
- **The total is unchanged.** This task moves tests; it deletes none. Take the figure from the
  baseline rather than from this spec, which was written before task 2 added 35 cases.

### Move in slices

**A slice is a dispatch, not just a commit.** Each one ends by appending its state to
`test-layout-detail.md` — what moved, what the ladder number is now, what is unproven — and handing
back; the next slice starts in a fresh window from the baselines and that file. Task 2 was the
lesson: the same shape, five components with a commit each, run as one dispatch that spent eight
hours and died on a context limit with the work unreported.

**Slice 0 comes before any move**, because nothing can move until it exists: the `gpu` feature with
its `compile_error!`, the `test-support` feature with `test_support/testdata.rs`, declared in `mod.rs` and the seven
existing sites converted to it, and
`test_module_layout` extended with the rules above. It moves no test and its proof is that the
three build shapes still compile and the suite is unchanged.

Then one commit per target, ascending by how much it forces: `test_gpu_batch` (3 cases, 2 items)
first as the shape proof — it is also the only middle-rung target, so it proves the rung and its CI
swap together — then the four injector consumers with `injection`/`rebuild`/`join_fixture`, then the
executor tiers, then the rest. After each, the leaf-name set for that target must have moved from
its binary's list into the right module's list and nowhere else.

**Two ladders, and every slice moves one of them.** The surface as it falls — the spec's figures
were 108 → 79 → 50 → 15 → 8 and are re-derived from the baselines — and the register beside it,
**9 entries → 0**, with the `pub mod` count falling 15 → 6 as each is demoted. A slice that moves
neither moved the wrong thing.

### The device gate

- `cargo build --features gpu` and `--features rust-only` both succeed; the `compile_error!` fires
  when both are passed together, and that is checked by trying it.
- **Name and gate agree in both directions.** The layout test asserts it by reading the tree:
  construct a `gpu_tests` module without the gate, and a `gpu`-gated module not called `gpu_tests`,
  and watch each go red.
- On shad-gpu the staged lib binary, run with `--test-threads=1 gpu_tests::`, covers the device set
  in roughly what the five staged binaries took. Materially longer means the filter is selecting
  more than it should; `running 0 tests` means it is selecting nothing, and the guard must say so.

### Test code is separated

- `git grep -n '#\[cfg(test)\]' -- peacockdb-core/src` returns only test-module declarations —
  `mod tests`, `mod ffi_tests`, `mod gpu_tests` — and the cross-component entry points the
  carve-out permits in a component's `mod.rs`, each with a doc naming its caller. The tree has 21
  in eight files, including gated `use` lines above those entry points, which count as part of the
  declaration they serve. Anything else is test code in a production file, which is what
  this task exists to end, and `driver/partitioned.rs` lines 670-685 are where it starts.
- `git grep -n '#\[test\]' -- peacockdb-core/src` returns only paths containing `test`.
- The transitive gate is checked by construction: add a line in `src/tests/` referencing a private
  item, confirm `cargo build --release` still succeeds; add a line in `plan/` referencing
  `crate::tests::`, confirm it fails with `E0433`. Revert both.

### CI

`test_ci_coverage` shrinks here, so it is the one guard that must be shown red rather than merely
green. Delete each of the four asserted lines from `pipeline.yml` in turn and confirm the guard
fails on each: the device line, the default-features `--lib` line, the `rust-only` `--lib` line and
the CLI build. One rung per line, and the assertion is all that stands between a rung and silently
not running.

The two scripts change with it, per the section above; verify `build-test.sh --gpu` and
`--rust-only` both still produce a non-empty derived suite, since that guard is now one move from
firing.

### Then build-test.md

Edited last, from the measured numbers rather than from this spec. Its two tables must add to the
headline figure, and that arithmetic is how the page is checked. A spec number and a measured number
that disagree mean the move is wrong, not the page.

## Done when

exactly eight items in `peacockdb-core` are `pub` because a test crate forces them — `GpuNode` and
`validate` (`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`,
`GpuBackend` and `GpuContext` (`executor/mod.rs`) and `render_run` (`plan_text/mod.rs`), all eight
forced by `corpus.rs` and `corpus_gpu.rs` and removed by [`test-support.md`](test-support.md);
`PUB_MODULES` is empty and `pub mod` is down from 15 to six, counted by `visibility-dump.py`; the
raw bare-`pub` count is whatever task 4 inherits and is not this task's claim. No
production file contains a `#[test]`; no `#[cfg(test)]` sits anywhere but on a test-module declaration or a
carve-out entry point in a component's `mod.rs`; every test module declares the lowest rung it needs and is named for it, with name
and gate implying each other at both rungs above the floor; every test-only path in `src/` carries `test` in its name;
`crate::test_support::testdata_root()` is the only testdata root anywhere, called by the crate's
unit tests, the moved targets and `tests/common/mod.rs` alike, and [#49](../tickets.md#t49) closes
with it; a plain `cargo build` cannot name `test_support`; `test_ci_coverage` is near 300 lines and asserts one CI
line per rung plus the CLI build; `build-test.md`'s two tables add to the headline; and the leaf-name
set is the one the baselines recorded — this task moves tests, it does not delete any.

## Signoff

Solved under its constraints: the twelve targets are in `src/` at the rung the table names, the
seven binaries stay, every baseline leaf survives in all three shapes and no golden moved; the
register is empty, `pub mod` is seven (`test_support` postdates the count of six), and the eight
items a test crate still forces are the corpus harness's. Two readings closed with no blocking
finding; the three important ones — the `test-support` feature undocumented, two private-field
readers admitted by a second carve-out case, `end_to_end.rs` over the length rule — are fixed.
Shortcuts: `build-test.sh`'s lib entry has run end to end only in `--rust-only` (verda was down;
the `--gpu` and default modes are rendered, syntax-checked and their resolver run against real
cargo json); the murmur gate's three CPU-runnable cases now run on shad-gpu only; the spec's
`crate::tests::`-from-`plan/` E0433 probe was not run, every cold production build standing in.

---

<!-- archived from llm-wiki/tasks/rmm-pool-budget.md -->

**Merged 2026-09-15 as PR #144 (merge commit `72fb23f8`).**

# 3 — the pool reserves what a binary needs

Kind: production

Inserted before [`test-layout.md`](test-layout.md), and independent of it: it touches `cpp/` and
one wiki page, and nothing in the layout refactor reads either.

Closes [#178](../tickets.md#t178) tentatively. Every gtest binary that installs a pool reserves 85%
of free VRAM and caps at 95, so two processes on the card cannot both have what they asked for and
the second dies in `pool_memory_resource` with `std::bad_alloc`. Measured once: two jobs
overlapping by under two minutes, three sf40 tests down, at a 14.38 GiB peak on a 139.7 GiB device.
Not a full device — two pools.

**CI no longer collides with itself**: `gpu-tests` carries `concurrency: {group: shad-gpu,
cancel-in-progress: false}`, which is the fix the ticket proposed and it has since landed, so runs
queue rather than overlap. What remains is the card being shared with work outside this repo, which
no group of ours can serialise — and a binary that asks for 85% of a device it does not own is the
part we can fix.

## The pool is not only in sf40 tests

The premise this task started from was that only the sf40 suites take a pool. They do not. Six
binaries call `peacock::install_rmm_pool()` from `main()`:

| Binary | sf40 | Runs |
|---|:-:|---|
| `test_tpch.cpp`, `test_tpchv.cpp` | yes | every gpu-tests job |
| `test_cudf_nodes.cpp`, `test_tpch_streamed.cpp` | yes | manual |
| `test_cudf.cpp` — the GPU smoke and the murmur3 kernel | **no** | every gpu-tests job |
| `test_plan_executor.cpp` — hand-built plans over `tpch.minimal`, 19 MB | **no** | every gpu-tests job |

So an ordinary CI run puts four processes on the host, each asking for 85% of what it finds free,
one of them for a dataset of nineteen megabytes. That is the shape of the collision, and sizing
only the sf40 pair would leave it in place.

`multi_gpu.cpp` is a seventh caller with its own per-device installation. It is manual, needs two
GPUs, and never runs in CI, so it is not part of this and **keeps the percentage constants**, which
stay in the header for it alone with a comment saying so.

## The change

`install_rmm_pool()` takes an explicit byte budget. No percentage, and **no clamp**: a binary asks
for what it needs and a host that cannot supply it fails to build the pool, which is already
`RmmPoolStatus::Unavailable` — the binary carries on with rmm's default resource, correct but
unpooled, and a caller taking timings asserts as it does today. A clamp would silently hand back a
smaller pool than was asked for, and the number a benchmark reports would stop meaning what it says.

Each of the six declares its own budget as a named constant beside its `main()`, with the
measurement that justifies it in the comment. **Take the numbers, do not choose them**: the pool
already carries a `statistics_resource_adaptor`, so run each binary and read its peak. Round up to
something a reader can defend, not to a percentage.

The FFI's `peacock_install_rmm_pool` gains the same argument, because it exists so a Rust caller
gets the allocator the gtest binaries have and that is no longer a fixed thing. What it must not do
is start reading `gpu_memory_limit`, which is stored and ignored — that is [#148](../tickets.md#t148)
and a decision about the product.

## Validation

- Every GPU tier stays byte-identical. This changes how much memory is reserved, never what is
  computed.
- **Two of each in parallel.** On shad-gpu, run `peacock_tpch_tests` twice at once and confirm both
  pass; that is the failure this task exists to make unreachable, and it has never been run
  deliberately.
- Each binary's declared budget is at least its measured peak, and the four that share a CI job sum
  to well under the device.

## The ticket stays open, tentatively closed

`#178` is not deleted and not archived. It is marked **tentatively closed** with the reasoning:
the pool no longer sizes itself against the device, so two runs fit; but the host is shared with
work outside this repo, so a third party can still exhaust it and this cannot be proven closed
from here.

The instruction that goes with it is for whoever meets it next, and it is deliberately narrow: **if
a GPU tier fails with `std::bad_alloc` in `pool_memory_resource`, add a dated line to #178 saying
which run and which binary, re-run the job once, and do not debug it.** The evidence accumulates on
the ticket until there is enough of it to say whether the sizing was wrong or the neighbour was
greedy. A coordinator that stops to diagnose this spends a dispatch on a machine it does not own.

`llm-wiki/prompts.md` carries the same line in the Coordinator section, because a coordinator does
not read `tickets.md` and would otherwise never see it.

## Done when

Six binaries declare explicit byte budgets with their measured peaks recorded; `install_rmm_pool`
takes bytes and does not clamp; the percentage constants remain only for `multi_gpu.cpp` and say
so; two `peacock_tpch_tests` run concurrently on shad-gpu and both pass; every GPU tier is
byte-identical; #178 is marked tentatively closed carrying the retry-don't-debug instruction; and
`prompts.md`'s Coordinator section carries it too.

## Completeness signoff

Solved under its constraints, with one Done-when item unmet and four things named rather than
hidden. Unmet: two `peacock_tpch_tests` at once never ran — a tenant outside this repo held 53 GiB
of the card for the whole task, and 69+69 needs an idle one. Two `peacock_tpchv_tests` did run
concurrently and both took their declared budget from very different free-memory readings, which
proves the rule and not the number. Shortcuts and deviations: the spec's premise for no-clamp is
false — nothing acts on `Unavailable`, and an unpooled sf40 run loses tests rather than running
slowly — and the carry-on behaviour was kept as specified, with the failure made legible instead;
the integrated sizing regime was deleted rather than replaced, so every budget is an H200 number
([#148](../tickets.md#t148) carries the open question); `PEACOCK_RMM_POOL_BYTES` was added past the
spec to restore what the deleted override gave the swept binaries; and 24 GiB for the 100M-row node
sweep is a first try, not a bisection.

---

<!-- archived from llm-wiki/tasks/module-layout.md -->

**Merged 2026-09-10 as `beb7455e`, squashed onto master by hand; PR #143 closed unmerged as a result.**

# 2 — a component's API is its mod.rs

Kind: production

Second of four, after [`drop-mode-name.md`](drop-mode-name.md) and before
[`test-layout.md`](test-layout.md) and [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

`peacockdb-core/src/batch_partitioned/**` moves to `peacockdb-core/src/`, laid out as components
whose whole API is declared in `mod.rs` with implementation behind private modules. The previous
task deliberately left this directory alone so that every file moves once, here, rather than twice.

Today 170 top-level items are `pub` and eight of them are named by another crate. This task does not
change that — it changes where they are declared and what may reach past them.
`plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver` becomes
`executor::run` as their modules acquire those names; `peacockdb/src/main.rs` moves with them.

### Five components

The IR is neither planner nor executor: it is what passes between them, and eleven non-test
consumers span both halves. Giving it to either makes that half's internals a dependency of the
other.

```
src/lib.rs
src/common.rs              the row-byte formula, today memory.rs
src/plan/                  what a plan is
src/wire/                  what crosses the FFI
src/planner/               making both
src/executor/              running them
src/plan_text/             rendering any of it
```

`wire/` is separate from `plan/` because they are two contracts, not one. A plan is the tree the
planner builds and the driver walks; a recipe is the menu of parameterized kernels that crosses to
the C++ side, and `architecture.md` already treats it as its own subject under that name.

**It holds everything that knows what a flat buffer looks like** — the vocabulary (`Recipe`, `Seq`,
`FbKind`, `Call`, `CallPattern`, `Input`, `ProjectRole`, `AbiSymbol`, `RecipePlan`), the writers
that are `recipe/` today, the reader, the two renderers that are `plan_text/fb_text.rs` and
`plan_text/recipes.rs` today, and the generated module.

That last one forces the shape. Eleven files name `crate::generated` — nine in `recipe/`, plus both
of those renderers — and they use **73 distinct generated types** between them, so flatc's output
cannot be walled off unless its callers are inside the same wall. `plan_text/recipes.rs` belongs
with them because it parses the buffer itself (`flatbuffers::root_with_opts::<fb::GpuPlan>`); it
takes `render_plan_recipes` and `Payloads` with it, and `plan_text/mod.rs` asks `wire` for the
`--- recipes ---` section. The memory section is not the same case: `MemoryModel` is a plain struct
the renderer can walk without knowing the wire format.

With all eleven inside, `wire/mod.rs` exposes `Recipe`, `Seq`, `attach_recipes`, `payload_text` and
`render_plan_recipes`, and the 7,336 lines flatc emits are private to one component.

**Keep the `#[allow(unused_imports, dead_code, clippy::all)]`** that sits on the module today, and
say why in a comment. It is cosmetic while the module is `pub` — everything is externally reachable,
so `dead_code` cannot fire. Private, every generated type the crate does not name becomes dead code,
and there are hundreds.

`planner` therefore has no `recipes` subcomponent: writing a recipe is a fact about the wire format,
not about planning, and `attach_recipes` walks a finished tree.

`plan_text` is a peer because it renders a plan tree, a run report, the memory model and the
recipes, and because it holds the one real edge into the driver — `run_text.rs` reads `RunReport`
and `PlanIndex`. It sits above both.

### Where everything goes

| Today | Goes to | Because |
|---|---|---|
| `nodes/` | `plan/` | eleven non-test consumers across both halves |
| `node.rs` | `plan/mod.rs` | `GpuNode`, `RowInterval` — the trait the nodes implement |
| `layout.rs`, `schema.rs`, `expr.rs`, `aggregates.rs` | `plan/`, as implementation modules | one vocabulary, not four. As subcomponents every other component would have to reach through their walls to name `Expr` or `Schema`; declared in `plan/mod.rs` they are one facade |
| `validate.rs` | `plan/` | structural properties of a tree, not of planning; also removes the one executor-to-planner edge, `driver/partitioned.rs:28` |
| `error.rs` | split | `PlanError` to `plan/mod.rs`, `RunError` and `When` to `executor/mod.rs` |
| `RowGroupMeta`, `Batching`, `ScanMetadata` | `plan/` | `GpuLoadParquet` stores the mapping verbatim |
| `recipe/` entire, `plan_text/{fb_text,recipes}.rs`, and `lib.rs`'s `generated` | `wire/` | the eleven files that name `crate::generated` use 73 of its types; it cannot be private unless they are inside with it |
| `partitioner.rs`, `parquet_meta.rs`, `gpu_rowgroup_prune.rs` | `planner/translator/scan_mapping/` | all three entry points have one caller, and all three calls are inside `Translator::source`, a 20-line function (`translate/mod.rs:449,451,458`). As a peer of `translator` it would be the design's only sibling-subcomponent edge |
| `MemoryModel` | `planner/mod.rs` | already half of what `plan()` returns |
| `RunReport`, `PlanIndex`, `ROOT` | `executor/mod.rs` | what running a plan produces, plus the post-order addressing the recipes and the FFI share — not private driver bookkeeping |
| `expr_translate.rs` | `planner/translator/` | sole consumer is `translate/` |
| `nulls.rs` | `planner/` | a refusal the planner makes; sole caller `plan.rs` |
| `estimator.rs` | `planner/memory_estimation/` | as proposed |
| `executor.rs`, `backend.rs` | `executor/mod.rs` | the seven category traits and `Backend`; their cycle disappears when they share a file |
| `batch.rs`, `cpu_batch.rs`, `gpu_batch.rs` | `executor/` | `Batch` and the two implementations |
| the three `#[cfg(not(feature = "rust-only"))]` declarations in `batch_partitioned/mod.rs` | `executor/mod.rs` | `gpu_backend`, `gpu_batch` and `GpuBatch` — see below |
| `forwarder.rs` | `executor/` | routing with no backend and no executor — the driver owns it |
| `expr_physical.rs`, `spark_partitioning.rs` | `executor/cpu_backend/` | one consumer each |
| `driver/accounting.rs` | stays under `executor/driver/` | see below |
| `memory.rs` | `src/common.rs` | four consumers across planner, executor and both backends |
| `config.rs` | deleted | see below |

Result:

```
plan/mod.rs           GpuNode, RowInterval, PlanError, the eighteen nodes,
                      Expr, Schema, NodeKind, PartitionLayout, AggFunc, PlanAgg,
                      RowGroupMeta, Batching, ScanMetadata,
                      NodeRef, ExecutorCategory, category_of, node_name
plan/common.rs        check_column_refs, check_merge_keys, input_layout,
                      input_schema, rebase_through_projection
plan/{exec_ops,accumulators,aggregate,join,partition_ops,source,union,unload}.rs
plan/{expr,layout,aggregates}.rs
plan/validate.rs

wire/mod.rs           Recipe, Seq, FbKind, Call, CallPattern, Input, ProjectRole,
                      AbiSymbol, RecipePlan, Payloads, attach_recipes,
                      render_plan_recipes, payload_text, node_at, depth,
                      check_seq_kinds
wire/generated.rs     private — the include!, 7,336 flatc lines, 73 named types
wire/{node_writer,join,aggregate_writer,expr_writer,writer,read,
      fb_text,recipes}.rs

planner/mod.rs        plan(), PlanKnobs, BatchSizing, SMALL_TABLE_BYTES, MemoryModel
planner/nulls.rs
planner/{translator,memory_estimation}/
planner/translator/scan_mapping/

executor/mod.rs       Backend, the seven traits, Batch, CpuBatch, GpuBatch,
                      RunError, When, CallStats, RowRange, BatchForwarder,
                      RunReport, PlanIndex, ROOT
executor/{cpu_batch,gpu_batch,forwarder}.rs
executor/{driver,cpu_backend,gpu_backend}/

plan_text/
```

`plan/mod.rs` lands near 980 lines, measured: 561 of declarations with their doc comments, 120 for
the registry and its four exhaustive 18-arm matches, 272 of one-line delegations over 32 inherent
blocks, and the module header. It is the crate's central index and the one file a newcomer should
read, and it holds no logic — the one-expression rule is what keeps it a header rather than a
module. `wire/mod.rs` is about 300 — the vocabulary plus five delegating entry points. Nothing else
passes 400.

### A component whose API is conditionally present

`executor/` is the one component that does not have a fixed surface: `gpu_backend`, `gpu_batch` and
`GpuBatch` are `#[cfg(not(feature = "rust-only"))]` today, and the declarations move with them. So
`executor/mod.rs` carries the cfg on the declarations themselves, not only on the `mod` lines —
`GpuBatch`, `GpuBackend` and `GpuContext` exist in two of the three feature shapes and not the
third.

Two consequences. The layout test must read the cfg rather than the item, or it will report a
missing declaration under `rust-only` and a surplus one otherwise. And the `use common;` line at the
end of `executor/mod.rs` must not pull in anything device-only, since `common.rs` is compiled in
every shape — the rust-only tier boundary is exactly what a shared `common.rs` is able to breach.

### Why accounting is not its own subcomponent

`ResidentAccountant` is a field on the partitioned driver and is passed by mutable reference into
the lane state machine at four sites in `single_partition.rs`. `Held<T>` is the driver's in-flight
batch representation, `Slot` an index into its executor slots, `Trip` a `StepError` variant. Four of
the five types are driver internals; only `Underestimate` faces outward, through `RunReport`.

A sibling subcomponent would have to declare all four as API. As `executor/driver/accounting.rs`,
a private implementation module, they stay `pub(crate)` behind the driver's wall and nothing outside
`driver` can name them.

`ResidentAccountant` is the name of the thing. Retire "the enforcer" and "resident enforcer" as
second spellings for it, in the wiki and anywhere else — the type both accounts and enforces, and
enforcement is the smaller half.

### config.rs is dismantled

`MemoryLimit` moves to `src/test_support/`, which [`test-layout.md`](test-layout.md) creates for the
corpus harness that reads it; until that task lands it sits in `tests/common/` beside `bp_mode.rs`. `TargetPartitions`, `TARGET_PARTITIONS`
and `BATCH_STRESS_BUDGET` are named by nothing outside the file's own unit test and go. The module
doc describes `tp8-standard` device labels, which the legacy-mode drop retired.

## The visibility rules

For `coding-style.md`, replacing nothing that is there today.

- A component or subcomponent is a directory with `mod.rs`. Its whole API — structs, enums, traits,
  functions, constants — is declared there. Nowhere else in the component carries `pub` or
  `pub mod`.
- Implementation modules are declared `mod x;` and their items are `pub(crate)`. The module's own
  privacy is the boundary: a path through a private module is refused whatever the item says, so
  `plan::exec_ops` cannot be named from outside `plan` and `pub(super)` is not needed.
- **A subcomponent is declared `mod`, not `pub mod`** — `mod recipes;` in `planner/mod.rs`, never
  `pub mod recipes;`. `pub mod` would make `planner::recipes::Recipe` nameable crate-wide and the
  subcomponent wall would exist only on paper. What a sibling component needs is declared in the
  component's own `mod.rs`; that is what the four type moves in the table above are for.
- `lib.rs` declares the components `pub mod`, and they are the only `pub mod` in the crate.
- **Nesting may go three deep** where the innermost earns it: `planner/translator/scan_mapping/` is
  720 lines behind three entry points. The same rule applies at each level — `mod`, not `pub mod`.
  A directory with a one-item facade and a hundred lines behind it is an implementation module
  wearing a directory; the test is whether the body justifies the wall.
- `pub use` is not allowed. Inline the declaration into `mod.rs`, or into `common.rs` for what the
  implementation modules share. A child reaches into its parent; a parent never re-exports a child.
- A body in `mod.rs` is one expression. Declarations, and delegations of exactly one line.
- A struct keeps its inherent `impl`, and that block lives in `mod.rs` with one-line bodies. A trait
  is for two or more implementors. A trait per struct would also break every `const fn` and
  associated const, which trait items cannot be.
- An implementation module may implement any trait for a type declared in its own component's
  `mod.rs`, and may define free functions the `mod.rs` delegates to. It may not declare types or
  traits that form the component's API.
- Absolute `crate::` paths across a component boundary, `super::` only within one.
- `mod.rs` and `common.rs` have no length limit for this task. A limit is set afterwards from what
  they weigh.

### What that buys, exactly

Three claims, and only the first two are the compiler's.

- **A component is reachable only through its `mod.rs`.** Enforced: every implementation module is
  private, so naming one from outside is `E0603: module is private`.
- **A subcomponent is reachable only through its own `mod.rs`, and only from inside its parent
  component.** Enforced by the same mechanism, once the declaration is `mod` rather than `pub mod`.
- **Only the parent component's own code may use a subcomponent.** Enforced *across* components.
  Not enforced *within* one: Rust's rule is "the module and its descendants", and sibling
  subcomponents are descendants of the parent. There is no visibility level meaning "my parent but
  not my siblings" — `pub(super)`, `pub(crate)` and `pub(in path)` all give the same set.

**The layout as placed has no sibling-subcomponent edge left.** A sweep for calls from one
subcomponent into another found exactly one, `translator` into `scan_mapping`, and the answer was
that `scan_mapping` was misplaced rather than that the rule needed an exception. `driver` never
calls a backend — it is generic over `Backend` — and `memory_estimation` takes only types.

So two things fall to the layout test: sibling reach between implementation modules, which is the
same gap one level down, and where a `pub` appears at all, which nothing in rustc checks. Both are
readable from the tree, in the idiom `test_ci_coverage.rs` already uses. Without that test none of
these rules can go red.

## What the visibility sweep finds

170 top-level `pub` items in `src`, plus 142 `pub` methods and associated consts.

- **8** are named by another crate, and all eight by one file — `peacockdb/src/main.rs`, the CLI,
  which is the only workspace member that depends on `peacockdb-core` at all. They are
  `build_session_state`, `register_tables_for`, `plan_batch_partitioned`, `PlanKnobs`,
  `BatchSizing`, `SMALL_TABLE_BYTES`, `CpuBackend` and `batch_partitioned_driver`. That is the
  crate's real API: register tables, plan a query, run it on a backend.
- **108** are `pub` only because `peacockdb-core/tests/*.rs` are separate crates.
- **54** are named by nothing outside the crate and lose `pub` — among them `Batching`, `ColumnRef`,
  `SortOrder`, `UnaryOp`, `Translator`, `MemoryModel`, `JoinCapability`, `Forwarder`,
  `logical_size_from_schema`, `estimate`, `partition`, `translate_expr`. Eleven of the 54 are used
  only inside their own file and lose `pub` entirely: `Decomposition`, `EmittedBatch`,
  `SourceEstimate`, `StateFunc`, `all_row_groups`, `position_of`, `wire_nodes`, and the four
  `config.rs` items that are being deleted.
- 85 `pub(super)` become `pub(crate)` once the enclosing `pub mod` loses its `pub`.
- 30 `pub use` are inlined.

One leak the item count does not show, because it is a module rather than an item:
`pub mod generated` in `lib.rs` exports the whole flatc surface — 7,336 lines, 73 types named
internally and every other one reachable. It becomes `mod generated;` private to `wire/`.

The 108 stay `pub` **in this task**, and [`test-layout.md`](test-layout.md) removes them by moving
the eleven targets that force them down into `src/`. Doing that here would mean one diff in which a
layout mistake and a coverage regression look alike, so it waits — and by then every one of those
imports has already been rewritten, which is most of the work.

## Renames that fall out

- **The node `GpuJoin` becomes `GpuHashJoin`.** It is the equi-join, it serializes to
  `CudfHashJoin`, and it sits beside `GpuCrossJoin` and `GpuNestedLoopJoin` — one of three
  unqualified for no reason. The executors keep the category names `CpuJoin` and `GpuJoin`, which is
  the scheme every sibling follows and is accurate: both run all three join nodes.
  13,246 golden lines carry the old name. It is display text, so the recipe digests do not move;
  sed, then regenerate to confirm the sed rather than to author it.
- **`memory.rs` becomes `common.rs`.** It is a byte formula, not memory management, and it collides
  with `plan_text/memory.rs`, which renders the `--- memory ---` section.
- **`gpu_rowgroup_prune` loses its `gpu_`.** It runs on the CPU and serves both backends.

**`CpuUnload` and `GpuExport` stay as they are.** They are one thing under two names, differing by
backend, which the style guide's "the same thing carries the same name everywhere" reads against —
but neither collides with anything, and any fix costs more than it buys: `GpuUnload` is taken by the
plan node, and renaming `CpuUnload` to match `GpuExport` moves a name nobody is confused by. Noted
as a considered keep so the next reader does not re-derive it.

## Tests

The four tiers already exist and none of them moves.

- **Module unit** — `#[cfg(test)] mod tests { … }` inline in an implementation module. 13 sites,
  unaffected.
- **Component and subcomponent** — `#[cfg(test)] mod tests;` declared beside the implementation
  modules, files under `component/tests/`. 11 sites. Being a descendant of the component they see
  its private items but not an implementation module's, which is the boundary respected.
  `nodes/tests/`, `cpu_backend/tests/`, `driver/tests/`, `translate/tests.rs` and `recipe/tests.rs`
  are already this.
- **Crate integration** — `tests/*.rs`, held to the component API. Eleven files name an
  implementation module today: `cpu_backend::{accumulate,emit,join,source,backend}`,
  `gpu_backend::{accumulate,emit,join,backend}`, `nodes::{aggregate,join}`. Every type they reach is
  legitimately component API, so this is an import rewrite — `cpu_backend::CpuAccumulator` — and no
  test relocates.

`test_cpu_executors` and `test_gpu_executors` construct backend executors directly, so the executor
constructors stay public for tests. That is the executor contract and is fine; list them in the
component `mod.rs` deliberately rather than by accident.

Eleven of the eighteen targets then move down into `src/` in [`test-layout.md`](test-layout.md),
which is also where test code stops sharing a file with production code. Nothing here should be
written to make that harder — in particular, do not fold a test helper into a production module to
shorten an import.

## The wiki this moves

`architecture.md` is not prose about the code — it is prose *anchored to* the code, and the anchors
move. 29 of its lines carry a Rust path or module name: 37 path references in total, some as
markdown links to `../peacockdb-core/src/batch_partitioned/recipe/*.rs`, most as bare
`node_writer.rs` / `join.rs` / `scheduler.rs` in running text. Seven are in the wire-format section
alone, which is also where the largest move lands.

Correcting them is this task's, not a later cleanup's — `prompts.md` makes keeping those two pages
true the same commit's duty as the change. The work is four kinds, and only the first is mechanical.

- **Paths in links and backticks**: rewrite to the new component. `batch_partitioned/recipe/` becomes
  `wire/`, `batch_partitioned/translate/` becomes `planner/translator/`, `batch_partitioned/nodes/`
  becomes `plan/`, and the crate-root files land where the placement table says.
- **Directory names in prose**, which read as facts rather than links: "the recipe writer
  (`batch_partitioned/recipe/`)", "the types are in `batch_partitioned/`", "`driver/`,
  `partitioned.rs` owns the tree". These do not grep the same way as the links and have to be read
  for.
- **Sentences the reorganization falsifies**, which carry no path at all. "A `Gpu` name with no
  `Cudf` is one of this mode's own plan nodes" survives; the Execution section's "the types are in
  `batch_partitioned/` and the code is what they are" needs the new home; and the whole framing of
  the mode as *a* mode rather than *the* engine is the phase-1 rewrite, 185 hits across `llm-wiki`.
- **`coding-style.md` gains the visibility rules** — the section drafted above goes in whole, and
  its Small-files bullet's examples (`batch_partitioned/nodes/`, `batch_partitioned/cpu_backend/`)
  become the new paths. Its Names section loses the `test_inc2_conformance` exception paragraph in
  the next task, not this one.

`build-test.md` carries 19 such lines; correct the paths here and leave its test table alone —
[`test-layout.md`](test-layout.md) restructures it, and doing it twice means doing it wrong once.

## Validation

Nothing here may change what the engine computes, so the bar is the opposite of the previous task's:
**no golden may move at all**, and the one exception is quarantined in its own commit.

### Baselines, before the first move

1. `sha256sum` over `testdata/goldens/`.
2. `cargo test -p peacockdb-core --lib -- --list` and one `--list` per integration target. Tests do
   not move in this task, so every one of these must come back byte-identical at the end.
3. A dump of every `pub` and `pub(crate)` item with its declaring file — the visibility baseline the
   sweep is compared against.
4. Warning counts from clean builds in all three feature shapes.

### The `GpuHashJoin` commit is quarantined

It goes first, alone, and it is the only commit in the task whose diff touches `testdata/goldens/`.
Sed the 13,246 lines, then regenerate and require an empty diff — the regeneration confirms the sed
rather than authoring it. **Plain `UPDATE_CANONICAL=1`, never with `PEACOCK_REWRITE_RECIPE_BYTES`:**
without the second variable `test_plan_goldens` compares the committed payload digests against the
bytes it just built and fails naming the file; with it, it rewrites them, and the digest agrees
with itself having proved nothing. **Every later commit must show zero golden changes in `git diff --stat`,**
and that is the single most valuable check in the task: a golden that moves after this point means
the layout changed behaviour.

### One commit per component

In this order: `plan_text` and `executor/driver` first, because they are already close to the target
shape and prove the pattern cheaply; then `wire`, which is the largest single move and the one that
makes `generated` private; then `plan`; then `planner`; then the backends. After each, run the lib
unit tests plus `test_plan_goldens` — the cheap tier — so a break is localized to the
component that caused it rather than found at the end across a 138-file diff.

### Per-commit checks

- **The case inventory is byte-identical.** Tests do not move here, so any `--list` difference is a
  test that stopped compiling into its target — the most likely silent failure in the whole task.
- **Three builds, not one**: `--features rust-only`, default, and the C++-linked build. The
  rust-only tier boundary is the invariant most easily broken by a move, because pulling one type
  into a shared `mod.rs` or `common.rs` can drag an FFI type into a rust-only path, and it fails at
  link rather than at review. `executor/` is the component to watch: its API is conditionally
  present, so its `common.rs` compiles in every shape while `GpuBatch` does not.
- **The visibility sweep is mechanical, not a reading.** A script enumerates every `pub` and
  `pub(crate)` with its declaring file and asserts: no bare `pub` or `pub mod` outside a `mod.rs`;
  no `pub use`; no `pub(super)`; subcomponents declared `mod`, not `pub mod`; and the item set
  unchanged from the baseline, since this task moves declarations and does not remove any. Run it
  at every component commit.
- **Re-run task 1's strip-and-rematch after each slice.** Its residue gate excludes by line, not
  by match, so a survivor spelling anywhere on a line hides real residue sharing it. That was
  latent when task 1 closed; this task moves the files those 170 lines live in, which is exactly
  the motion that turns it live.
- **Drop the gate's `':!peacockdb-core/src'` exclusion once the directory is gone.** It exists only
  to spare `src/batch_partitioned/`, and this task removes that name. Left in place it hides the
  whole crate: task 1's completeness pass found four residues inside that tree precisely because
  nothing read it, and after this move the exclusion would blind the gate to everything the task
  touched. Run the gate without it, and expect the three mapping sites plus whatever `README.md`
  and `source.py` still say about `ParquetBatchPartitioner`.
- **Every grep in this task takes `--untracked`.** `git grep` does not see untracked files, so a
  sweep run before staging is blind to exactly the files being moved — in task 1 a gate reported
  clean while residue sat in four renamed files. This task moves every file in the crate, so the
  blindness is total until each slice is staged. Run the sweeps after `git add`, or with
  `--untracked`, and never before a move.
- **`git diff -M --summary` reports renames**, not delete-plus-add. A file reported as both changed
  more than half its content, which a path rewrite and an import fix should not do.

### The layout test must be seen red

For each rule it claims — a `pub` outside a `mod.rs`, a `pub mod` subcomponent, a `pub use`, a
sibling implementation module reaching another, a `crate::`-less cross-component path — construct
the violation, watch it fail, revert. A guard nobody has seen fail is a guard nobody knows is wired
up, and `test_ci_coverage.rs` is the worked example of doing this properly in this repo.

### Then the full suite, once

Per `coding-style.md` a behaviour-preserving refactor is verified with a representative case per
mode per binary plus the golden and meta tier; the full corpus runs once, at the end, on verda.
Check what a package-wide command actually sweeps before running it — `--features rust-only` selects
a build, not a tier.

### If a golden moves

It is not a golden to regenerate. It means the move changed behaviour, and the diff names where: a
plan line is the planner, a `--- memory ---` figure is the estimator, a payload digest is the recipe
writer. Bisect by component commit — that is what the one-commit-per-component rule buys.

## Done when

The crate builds clean under all three feature shapes with no new warnings against the recorded
count; every golden after the `GpuHashJoin` commit is untouched; the case inventory is
byte-identical; bare `pub` and `pub mod` appear only in `mod.rs` files, with no `pub use` and no
`pub(super)` anywhere in `src`; the layout test exists and has been seen red on each rule; the
visibility rules are in `coding-style.md` and `architecture.md`'s paths are correct; and CI is green.

## Completeness signoff

Solved under its constraints, with three named shortcuts and no bandaids.

1. "bare `pub` and `pub mod` appear only in `mod.rs` files" is not met as written. Nine `pub mod`
   sit outside `lib.rs` with 60 bare `pub` items behind them, every one forced by a separate test
   crate. Each is registered in `test_module_layout.rs` with the files that force it, checked in
   both directions so an entry outliving its reason goes red, and the whole exemption expires in
   task 3. `pub use` and `pub(super)` are genuinely zero.
2. The case inventory is not byte-identical. `config.rs`'s two unit tests moved to
   `test_golden_format` at net zero, which dismantling `config.rs` authorizes, and the layout test
   this task delivers gained an eleventh case. `TargetPartitions`' label round-trip is gone with
   the type; `MemoryLimit` coverage is preserved. No golden moved after the quarantined rename.
3. The device evidence transfers by argument, not by a run on the head: every `src/` change above
   the GPU-green head is import order, a doc comment moved onto the right struct, and two comment
   lines. The 170 goldens carry recipe payload digests and are byte-identical, so the wire format
   the device consumes did not move.

Two known blind spots are stated where a developer meets them rather than only here: the
cross-component reader misses whitespace before `::`, and `forced_by`'s reverse half misses a
plain module import. The spec's promised follow-up — a length limit for `mod.rs` set from what
they weigh — is not set, and `coding-style.md` defers it with no owner.

---

<!-- archived from llm-wiki/tasks/drop-mode-name.md -->

**Merged 2026-09-10 as PR #141.**

# 1 — the mode has no name any more

Kind: production

First of four. The legacy modes are gone, so "batch partitioned" and its `bp` abbreviation
distinguish nothing; every occurrence is a qualifier against an alternative that no longer exists.

This task changes **names only**. It renames identifiers, mode labels, golden filenames, one Python
module, one ticket page and the prose that carries them. It moves no Rust file and changes no
behaviour, which is what makes its validation absolute: every derived artifact must reproduce byte
for byte.

**`peacockdb-core/src/batch_partitioned/` is the one name that survives**, deliberately.
[`module-layout.md`](module-layout.md) places its contents into components, and doing the flattening
here would move every file twice for one outcome. Task 2 is where the last path goes.

The four tasks in order: this one, [`module-layout.md`](module-layout.md),
[`test-layout.md`](test-layout.md), [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

## What changes

138 files carry the name in a path; 964 content lines outside the goldens and 169 inside. (The
gate below also lands on 138 — a coincidence, not the same 138.)

`BpMode`, `BP_MODES` and `bp_mode.rs` become `Mode`, `MODES` and `mode.rs`. The lowercase gate
catches the filename but not the two identifiers, and a `BpMode` in a tree with no `bp` anywhere
else is the qualifier-against-nothing this task exists to remove.

**Mode labels lose the prefix**: `bp-tp4-sized` becomes `tp4-sized`. One table owns them, `MODES`
in `tests/common/mode.rs` (`BP_MODES` in `bp_mode.rs` before the rename above), and `ident()` derives the macro spelling by replacing hyphens — so the
table plus a sed over `corpus_cases.inc` covers the Rust side. Then `cost-registry.csv`'s fifteen
`bp_*` column headers, `testdata/fixtures/two-row-registry.csv`'s identical headers, and the
twenty-one sites in `cost-report/src/main.rs`.

**Golden files move rather than regenerate.**

- Only `bp-mini.result.txt` carries a mode inside it — the `mode=` line, 34 sections in tpch and 60
  in tpcds. `.plans.txt`, `.cpu.txt` and `.cost.txt` carry none, so those are a `git mv` and nothing
  else.
- `bp-recipe-payloads.txt` keeps its digests. They are over the flat-buffer bytes and no mode label
  crosses the wire, so a digest that moves here means something other than a rename happened. Do
  not set `PEACOCK_REWRITE_RECIPE_BYTES`.

**Entry points**: `plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver`
becomes `executor::run` in task 2, when the modules they live in acquire those names. Here they
keep their names; renaming a function whose module is about to move is one edit made twice.

**`llm-wiki/tasks/bp-tickets.md` becomes `active-tickets.md`**, staying in `tasks/`. 34 references
in eleven files — `archived-tasks.md` (11), `cost-report/src/main.rs` (6), `build-test.md` (3),
`test_cpu_end_to_end.rs` (2), `tasks.md` (2), `casts.md` (2), and one each in
`peacockdb/src/main.rs`, `tickets.md`, `wire-schema.md`, `refcounted-tables.md`. Its fourteen
`<a id="tNN">` anchors keep their ids, so every `#tNN` link still resolves; only the filename moves.
`cost-report`'s six are load-bearing — the widget resolves ticket links against that path.

**In the archives, update the link paths and leave the prose.** `archived-tasks.md` and
`archived-tickets.md` are a record of what happened; rewriting their sentences to say "the engine"
would falsify the history they exist to hold. Paths must resolve; wording stays.

**The Python prototype renames a module.** `scripts/exec_model/batch_partitioned_driver.py` becomes
`partitioned_driver.py`, and ten files import it by name. Python has no compiler to catch a miss —
the failure is an `ImportError` at collection time, so run the prototype suite before committing.

**Test targets** rename, each to what `build-test.md` already calls its tier:

| was | becomes | the tier it is |
|---|---|---|
| `test_batch_partitioned_injection` | `test_layout_injection` | Layout injection mechanism |
| `test_batch_partitioned_plans` | `test_plan_goldens` | Plan goldens — and it sits beside `test_corpus_goldens` |
| `test_cpu_batch_partitioned` | `test_cpu_end_to_end` | end to end: SQL in, rows out; its macro is already `end_to_end!` |
| `test_cpu_bp_corpus` | `test_cpu_corpus` | |
| `test_gpu_bp_corpus` | `test_gpu_corpus` | |

`test_inc2_conformance` is not renamed here — [`test-layout.md`](test-layout.md) makes it
`test_murmur_conformance` when it moves in-crate, and renaming it twice is one edit made twice.

This reaches CI twice: `.github/workflows/pipeline.yml` and
`test_ci_coverage.rs`, whose exemption table and three GPU target lists name the binaries. That
guard fails on a miss, which is the check. `exec-model-corpus.yml` carries the phrase in a comment;
`pipeline.yml` is not the only workflow to sweep.

**Four traps.**

- **`batch` alone is a domain word.** `BatchSizing`, `Batching`, `batch_rows`, `AggregateBatches`,
  `CudfCoalesceBatches` all stay. Only the two-word phrase goes.
- **The two words are not always adjacent.** `batch_single_partition_driver.py` carries the same
  qualifier with `single_` between them, so a `batch.partition` regex misses it; it becomes
  `single_partition_driver.py`, with its function and class. Left alone it would have been the only
  `batch` qualifier surviving in `scripts/`.
- **`batch→partition` is not the mode name.** Four sites — `gpu_plan.fbs:312` and `:346`,
  `node_session.cpp:220`, `gpu_rowgroup_prune.rs:151` — describe the row-group→batch→partition
  *mapping*, which is a real three-level structure and stays. A regex with `.` between the words
  matches the arrow, so a careless sweep mangles them.
- **In `llm-wiki` the phrase is the mode's name, not a qualifier** — 185 hits, plus roughly ten in
  `cpp/` and `gpu_plan.fbs`. Those sentences want rewriting to say the engine; a mechanical strip
  leaves them ungrammatical and, worse, still wrong.

## Validation

The rename is inert by construction, so the bar is that every derived artifact reproduces exactly.
"It compiles and the tests pass" proves nothing here — the tests would pass over a golden that
quietly changed.

### Baselines, before the first edit

1. `sha256sum` over every file under `testdata/goldens/`, saved outside the tree.
2. `cargo test -p peacockdb-core --lib -- --list` plus one `--list` per integration target, as the
   case-name inventory. 437 lib cases, eighteen targets.
3. The warning count from a clean `cargo build` and `cargo build --features rust-only`.
4. `git rev-parse HEAD`, so a bisect has a floor.

### The checks

- **The golden hashes move in exactly two files.** After the `git mv` and the two seds, the sha256
  list must differ from the baseline only in the two `bp-mini.result.txt` files, and there only on
  `mode=` lines. Any other hash change means the rename touched content it should not have.
- **Then regenerate anyway, and require an empty diff.** `UPDATE_CANONICAL=1` over
  `test_plan_goldens` and the corpus cpu tier rewrites every golden from a live run;
  `git diff` after it must be empty. The hash check says the files did not move; this says the
  engine still produces them.
- **The refusal message is the one exception, and it is quarantined.** `error.rs:20` renders
  `"unsupported in batch-partitioned mode: {what}"` and `translate/mod.rs:221` says "do not plan in
  batch-partitioned mode (#143)" — and those strings land in **75 lines across the ten
  `.plans.txt` goldens**. Reword them in their own commit, last: everything before it must
  regenerate to an empty diff, and that commit's regeneration must produce a diff of exactly those
  75 lines and nothing else. Same shape as task 2's `GpuHashJoin` quarantine, and the same reason —
  it separates the strong check from the one known change. The wording is the developer's, under
  two constraints: it must not say "mode", and the two sites must agree.
- **The payload digests are the sharpest instrument, and the plain regen is what reads them.**
  Under `UPDATE_CANONICAL=1` alone, `test_plan_goldens` compares the committed digests against the
  bytes it just built and fails naming the file. Setting `PEACOCK_REWRITE_RECIPE_BYTES=1` makes it
  rewrite instead of compare, which is the instrument switched off — so never set it here. A
  digest that moves during a rename means the run stops, not that the new digest gets committed.
  (`module-layout.md`'s quarantine reads this same paragraph.)
- **The case inventory maps under one transformation.** Every `--list` name must map to a baseline
  name by removing a `bp_` or `bp-` prefix, and per-target counts must match. A vanished case is a
  `#[test]` lost to a bad sed; a new one is a duplicated module.
- **`cost-report`'s own tests are the registry guard.** They read both `cost-registry.csv` and
  `testdata/fixtures/two-row-registry.csv`, so a header renamed in one file and not the other fails
  there rather than in a later task.
- **The exec-model suite runs before the commit**, not after. Its 216 cases are the only check on
  the Python module rename, and an `ImportError` there is silent until collection.
- **The prose-and-labels gate.** The naive form cannot be empty, because this task deliberately
  keeps `src/batch_partitioned/` and deliberately does not rename `plan_batch_partitioned` or
  `batch_partitioned_driver` — 132 lines outside `peacockdb-core/src` name the module path (128 in
  `peacockdb-core/tests/**`, 4 in `peacockdb/src/main.rs`) and 42 more name those two functions. All
  of it is the residue task 2 removes. So the gate excludes the four spellings that survive:

  ```
  git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' ':!peacockdb-core/src' \
    | grep -vE 'mod batch_partitioned|batch_partitioned/|batch_partitioned::|::batch_partitioned|plan_batch_partitioned|batch_partitioned_driver'
  ```

  **The pattern is `batch.?partition`, case-insensitive, and both halves of that are load-bearing.**
  `.?` rather than `.` catches `BatchPartitionedDriver`, which a separator-requiring pattern cannot
  see at all — that is how a class survived its own module's rename. And the exclusions are the
  four surviving spellings written out rather than a bare `batch_partitioned`, which would swallow
  every underscore residue including `--test test_batch_partitioned_plans` and
  `mod test_batch_partitioned_injection` — the target-rename miss the gate exists to catch.

  **Run it in a UTF-8 locale.** `→` is three bytes, and under `LC_ALL=C` a `.` matches one byte, so
  the pattern cannot span the arrow and the three mapping sites drop out — the gate reports three
  and reads as cleaner rather than as half-blind. The residue half is unaffected either way, since
  CamelCase needs zero characters and every separator form is one byte, so a C-locale run is safe
  but its count is not the documented one. `build-test.md` sends people to `LC_ALL=C` for
  cross-host comparison, so this is a real way to meet it.

  The goldens are deliberately **not** excluded. They carry no hits once the refusal is reworded,
  so excluding them buys nothing today — and they are exactly where that wording lands, so a gate
  blind to them could not catch it coming back.

  **The exclusion is line-scoped, which is the one hole left in it.** `grep -vE` drops the whole
  line, so a deliberate survivor anywhere on a line shields real residue sharing it —
  `mod batch_partitioned; // renamed from test_batch_partitioned_plans` is invisible to the gate.
  Latent rather than live in what the gate reads: stripping the six survivor spellings from every
  excluded line and re-matching returns nothing there. But 170 lines carry a survivor spelling, and
  the later tasks move the tree those lines live in, so re-run that strip-and-rematch after each
  slice rather than assuming it still holds.

  **The `peacockdb-core/src` exclusion is the larger hole, and it was live.** The pathspec drops the
  mode's own module, so the gate never reads the tree this task is named after, and the completeness
  pass found four hits inside it: `error.rs`'s two `RunError` display strings, `mod.rs`'s module doc,
  and a temp-dir name in `parquet_meta.rs`. The same pattern scoped to `peacockdb-core/src`, with the
  survivor spellings stripped per line, must land on one — `gpu_rowgroup_prune.rs:151`'s mapping
  site. Task 2 removes the directory that forces the exclusion; until then that scoped form is the
  only thing that reads inside it.

  At the finish it lands on **six**, all deliberate: the three `batch→partition` mapping sites,
  `README.md:371` and `source.py:3` naming `ParquetBatchPartitioner` (the same structure, not the
  mode), and `test_ci_coverage.rs:431`, which is task 2 residue and goes when the module does.

  Then `git grep -nE --untracked '\bbp[-_]' -- ':!peacockdb-core/src' ':!llm-wiki'` and
  `git grep -n --untracked 'bp-tickets' -- ':!llm-wiki'`, both empty. `llm-wiki` is excluded whole
  because it is rewritten by hand rather than swept, and two parts of it keep `bp` on purpose: the
  archive's 21 mode labels, which record what the modes were called at the time and would be
  falsified by an edit, and these four task specs, which have to quote both spellings to say what
  becomes what.

  **`--untracked` is not optional, and it is the trap that would have shipped residue.** `git grep`
  does not see untracked files, so every gate is blind to exactly the 42 files this task renames —
  they are `??` until staged. Run without it and the gate goes **green over the renamed files
  themselves**: the first pass here left the phrase in `mode.rs`'s module doc and `mode_named`
  panic, and in two renamed targets' module docs, with gate 1 reporting clean. The same shape bit a
  `git grep -l` sed list, and that one at least went red in the Python suite. This one would not
  have.
- **`test_ci_coverage` passes**, so a missed target rename fails there rather than silently
  un-gating a tier.

### What a device run does and does not add

A renamed golden that `mode.rs` computes a different path for fails **rust-only, before any
device**: `test_corpus_goldens` opens `cpu_golden()`, `cost_golden()` and `result_golden()` by
computed path with `.expect(...)`, and `test_cost_model` sweeps the same directory. So the device
tier is not the first reader of those filenames, and a run that claims to be proving them is
claiming the wrong thing.

Nor is "the sections resolve" the whole of it: `test_corpus_goldens` already checks committed
sections it did not write, against their own arithmetic, with no run at all. What the device run
uniquely adds is those sections checked against **a second engine's actual run** — plan shape,
`in_rows`, the per-batch lists, the bytes. That is the claim that stays true in tasks 2 and 3, and
"only the device can see this" is an easy thing to say about a filename when it is true only of a
comparison between engines.

### Done when

Every golden regenerates to an empty diff and the payload digests are byte-identical; the case
inventory maps by prefix removal with no count changing; `cost-report` and the exec-model suite are
green; the grep gates are empty; and CI is green with no new warnings against the recorded count.

## Completeness signoff

Solved under its constraints. Every derived artifact reproduces byte for byte: the 33 golden
renames differ from their baselines only on the 94 `mode=` lines and the 75 quarantined refusal
lines, the payload digests never moved, the case inventory maps 1:1 under prefix removal, and a
final `UPDATE_CANONICAL=1` regen of the plan goldens and the cpu corpus left an empty diff.

Two bandaids, both deliberate and both named above. The prose gate excludes `peacockdb-core/src`,
which hid four residues until the completeness pass; the scoped form that finds them is written
into the gate section, and task 2 removes the directory that forces the exclusion. And the spec
was not frozen — six commits rewrote it, so "the gate lands where the spec says" is partly
self-fulfilling; each of the six survivors was re-derived independently instead.

---

<!-- archived from llm-wiki/tasks/empty-answers.md -->

**Obsolete — approach rejected, never merged; PR #138 closed 2026-09-10.** Superseded by `empty-build.md`, which showed the capability it proposed is one the engine does not need.

# Answering with nothing: the table no call can build

Kind: production

Two refusals, one wall. A node owes rows it did not receive, and every entry point on the surface
loads a table by reading one — so there is no call to make.

- [#173](../tickets.md#t173) — a collapse of no handles, a merge of no runs, and a finish whose
  probe produced no keys.
- [#175](../tickets.md#t175) — a join whose build side produced no batch, where `Right`, `Full` and
  `RightAnti` owe their probe side. #175 says it itself: *"the same wall as #173, reached from the
  join instead of the accumulator."*

**Both close here.** They are one task because they are one missing capability seen from two nodes,
and separating them would mean building it twice.

**Depends on [`wire-schema.md`](wire-schema.md), and that dependency is the whole reason this is
small.** Both tickets say "unfreezing buys a make-empty-of-schema call and the refusals go", written
when nothing on the wire carried a node's schema. Once `PlanNode.output_schema` is populated, the
call is not needed at all: `execute_node` already holds the node
(`node_session.cpp:190`), so it can answer with an empty table rather than throw. **No new ABI
symbol.** The C++ said so before either of us did — `node_session.cpp:257`:

> *"A collapse of nothing has no schema to answer with: **the node's own `output_schema` is absent on
> a recipe plan**, and concatenating no views gives a table of no columns, which is not a batch
> anything above can read."*

## 1. #173 — an empty table of the declared schema

Three sites refuse, and all three want the same thing:

| site | what owes rows |
|---|---|
| `cpp/src/node_session.cpp:261-266` | a collapse with no input handles |
| `gpu_backend/accumulate.rs:331` | a merge of no runs — "the collapse of nothing under another name" |
| `gpu_backend/join.rs:236` | a finish whose probe was empty, so it has no keys to join against |

In `execute_node`, build the answer from `node->output_schema()`: one `cudf::make_empty_column` per
field (`cudf/column/column_factories.hpp:43`), assembled into a table with the declared names. The
two Rust refusals then stop being refusals — the call they could not make is a call that now
answers.

**The global aggregate is the exception and must stay one.** #173: *"a global aggregate owes its
identity row whatever arrived"* — `count` is 0, not absent. An empty-of-schema answer there would
drop a row DataFusion produces, so the aggregate arm keeps its own path and this task must not
collapse the two into "empty input, empty output".

**The CPU backend already emits nothing in the same places**, deliberately, so the two engines agree.
Check each site against its CPU counterpart as it changes: the point is not that the device stops
refusing, it is that both engines answer the same thing.

## 2. #175 — the probe side, padded or not

`empty_build_answers_nothing` (`nodes/join.rs:563`) is already the right decision in the right place;
what is missing is what to do when it returns false. Its own doc names the split:

> *"what they owe is the probe side, **padded or not**"*

- **`RightAnti` owes the probe side unpadded.** Its output is the probe columns alone, and an empty
  build side makes every probe row unmatched — so the answer *is* the probe batch. **Route it, do
  not call.** No pad, no kernel, no handle beyond the one the driver holds.
- **`Right` and `Full` owe it padded**, with typed NULLs in the build columns. That is the mirror of
  the pad that already exists: `ProjectRole::NullPad { nulls }` with `pad_project` and
  `padded_columns` (`recipe/join.rs:114`, `:261`, `:423`) appends one NULL per **probe** column a
  build-preserving join's projection keeps. This needs the same shape counting **build** columns.

So the recipe gains one role, not a mechanism. Whether that is a second `ProjectRole` variant or a
side on the existing one is the author's call — but the name must say which side is being padded,
since a reader who assumes the existing direction gets a plan that type-checks and pads the wrong
columns.

**Two corpus queries are waiting on it**, both found by T19: `tpch/q21` at `tp4-single`, and
`tpcds/q77`, whose `Right` outer at four lanes gets no build side. q77 is currently out of the
end-to-end list with `tpch/q2` carrying its claim, because writing the CPU pad alone would make the
oracle answer a query the device refuses. **Put q77 back on that list as part of this**, or the
reason it was removed outlives the reason.

## 3. Tests

### Unit, Rust

- **the empty-build decision by type** — `empty_build_answers_nothing` returns true for the six that
  end the lane and false for `Right`, `Full`, `RightAnti`. It exists; assert it names all nine so a
  tenth type cannot be added silently.
- **`RightAnti` routes rather than calls** — a lane with an empty build side and a probe batch emits
  that batch and makes no ABI call. The absence of the call is the assertion, not the rows.
- **the build-side pad counts build columns** — the mirror of
  `the_pad_project_appends_one_null_per_probe_column_the_projection_keeps`, written the same way and
  next to it, so the two directions are read together. A projection keeping no build column pads
  nothing.
- **a pad in the wrong direction is caught** — assert the emitted NULL count against a join whose
  build and probe widths **differ**. With equal widths both directions pass, which is how this ships
  wrong.

### Unit, gtest

- **a collapse of no handles answers an empty table of the declared schema** — column count, names
  and types from `output_schema`, zero rows. The test that pins the fix.
- **the types are the declared ones, not defaults** — a schema with a `Decimal128(15,2)` and a
  string, asserting both come back as themselves. An empty table of the wrong types is the failure
  this cannot afford, since nothing downstream has rows to notice with.
- **a global aggregate with no input still emits its identity row** — the exception, asserted rather
  than assumed, because the natural implementation of everything above deletes it.

### Recipe walk

- **a query with an empty lane runs end to end** — the existing harness with a shape that leaves one
  lane with no build side, driven at `target_partitions` > 1. Today it refuses; this is the first
  test where an empty lane reaches the unload.

## 4. Goldens and the device workflow

| golden | moves | why |
|---|---|---|
| `*.plans.txt` | **yes**, where a Right/Full join gains a pad | a new recipe call renders in the node's recipe line |
| `recipe-payloads.txt` | **yes** | the pad project is a payload |
| `testdata/cost-registry.csv` | **yes** | cells move off #173 and #175 |
| `<mode>-<tier>.cpu.txt`, `.cost.txt` | **yes**, for the queries that newly run | new sections, not changed ones |

Device work, in batches of about five on `shad-gpu` with `build-test-shadgpu.sh`, as T19 does: the
two known sightings first, then any cell whose ticket names #173 or #175. Expect the freed cells to
land on whatever refuses next rather than going green — the causes are ordered, and #152 and #183 sit
in front of most of the corpus. **Close #173 and #175 when their cells are gone from the registry**,
not when the code lands.

## 5. Out of scope

The other frozen-surface refusals. This task buys exactly two capabilities — an empty table of a
declared schema, and a probe side passed through padded or bare — and every other "the surface
cannot express this" stays where it is. If a third refusal looks like it would fall out for free,
that is a ticket, not an addition.

---

<!-- archived from llm-wiki/tasks/wire-schema.md -->

**Obsolete — approach rejected, never merged; PR #137 closed 2026-09-10.** The divergence it removed on the wire is now to be found by the operator harness and fixed at its source, not carried as a per-column width.

# The declared schema on the wire, and the precision the export is never told

Kind: production

**This task closes [#187](active-tickets.md#t187)** — the device widens a decimal the plan declared
narrow — by giving the export the precision it currently defaults to 38 for, rather than by casting
the result back.

Sibling of [`casts.md`](casts.md), which predicts the divergence and deliberately does not fix this
half. **Do that task first**: it writes down what the device returns for a declared type, and this
one changes the answer for decimals.

## Why it happens

cuDF's decimal type carries **scale but not precision**. `export_table_to_ipc` builds its metadata as
`col_meta.push_back({name})` — name only — so `decimals_to_arrow` falls back to
`metadata.precision.value_or(max_precision<__int128_t>())`, which is
`floor(128 · ln2 / ln10) = 38`. Every decimal exports at precision 38 with scale preserved. That is
why `tpch/filter-project` declares `(15,2)` and receives `(38,2)`, and why `q6` never hit it: its
sums declare `Decimal128(38,4)` already.

**This is a missing argument, not two engine rules disagreeing.** #187's current text frames it as
the CPU's `widened_decimal` and the device's concat reaching different verdicts on the same bytes.
That framing is wrong and the ticket should be corrected as it closes.

The Rust side already sends the value: `Field.decimal_precision` exists in `gpu_plan.fbs` and
`serialize_schema` fills it from `Decimal128(p, s)`. It is dropped on the C++ side, after arrival —
`TableResult` (`plan_executor.h:15`) is `table` + `column_names`, and once a column is in one, the
precision exists nowhere.

## The work

### 1. Write `output_schema` on the wire

`PlanNode.output_schema: Schema` already exists in `gpu_plan.fbs` and is documented "Output schema of
this node". The writer never fills it: `fb::PlanNode::create` appears once on this path, in
`Writer::push` (`recipe/writer.rs:97`), with `output_schema: None`. That is the whole change — every
node goes through that funnel, `GpuNode::schema()` is node-local with nothing to derive, and
`serialize_schema` already fills `decimal_precision`/`decimal_scale` from `Decimal128(p, s)`.

Write it for **every** node, not only where a decimal appears: `fb_text.rs`'s own header warns that
"not set" and "set to zero" are different instructions to the executor, and a conditional wire format
is a worse thing to own than a slightly larger buffer.

The C++ then reads it where `TableResult` is built (`plan_executor.h:15`, today `table` +
`column_names` only), carries a per-column precision alongside the name, and
`export_table_to_ipc` sets `column_metadata.precision` instead of leaving it empty. `operators/
union.cpp:35` already reads an `output_schema` for the neighbouring problem — branches landing
different fixed_point scales — so this is an existing mechanism reaching one more node kind rather
than a new one.

With precision on the wire, `export_type_for`'s `Decimal128(p,s) → Decimal128(38,s)` row stops being
true: the divergence is removed rather than absorbed. Update the row in
[`casts.md`](casts.md) and the prediction test that records it — the cast list never gains a decimal
arm, because pinning a fixable omission into a golden would make it look inherent.

**The C++ half was the open risk and it is closed.** The worry was that
`node_session.cpp:517` does `result.column_names = input.column_names` — names propagating from
*inputs* rather than from each node's declared output — which would mean precision had to be
threaded from an origin. It does not. `execute_node` holds the node it is executing:

```cpp
const fb::PlanNode* node = impl_->post_order[seq];   // node_session.cpp:190
```

and every `TableResult` built inside it (`:267` collapse, `:355`/`:399` repartition) is in that
scope, so `node->output_schema()` is directly readable once step 1 populates it. Line `:517` is
`NodeSession::slice_handle`, which has no node because it slices an existing handle — it copies
`input.column_names` from a `TableResult` that already exists, and precision rides along the same
way. **No threading from an origin is required, and no signature changes.**

### 2. `schema_text` renders precision and scale

`fb_text.rs:229` formats fields as `{}:{:?}` over `f.data_type()`, so `recipe-payloads.txt` prints
bare `Decimal128` while expressions on the same page print `Decimal128(23, 2)`. Two fields that are
on the wire are invisible to the golden whose job is to pin the wire — a change to either, including
one that broke step 1, would not move it. Render them for `Decimal128` fields.

### 3. Unit tests

Same idiom as [`casts.md`](casts.md) — build the node, run its recipe fn, `writer.finish()`,
`flatbuffers::root::<fb::GpuPlan>`, `node_at(seq)`, assert on payload fields.

- **precision reaches the payload** — assert `output_schema().fields()[i].decimal_precision()` is
  the declared 15 rather than 0. Reuses the `(name, precision, scale)` schema helper `casts.md`
  adds. **Red before step 1 lands**, which is the order `coding-style.md` asks for.
- **every node carries a schema, not only the ones with decimals** — a node of plain `Int64`
  columns still has `output_schema` set, since a conditional wire format is the thing step 1
  declines to own.

## Restriction

**Code and test changes are limited to what is written above.** No refactor of `node_session.cpp`
beyond carrying one field alongside `column_names`, no generalizing the export metadata past
precision, no cleanup of `TableResult`'s neighbours. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `recipe-payloads.txt` | **bytes** change on every node; text changes on decimal fields | step 1 adds `output_schema` everywhere, step 2 renders precision |
| `*.plans.txt` | **no change** | plan text renders the Rust tree, which already knew the precision |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | values and byte pricing are unaffected; only a declared type moves |
| `testdata/cost-registry.csv` | device cells move off #187 | see below |

The payload golden is the one to review rather than accept: its bytes move for every node in every
plan, and step 2 is what makes that diff legible instead of opaque.

## Device workflow

1. Run the **10 queries carrying #187** on `shad-gpu` with `build-test-shadgpu.sh`, in batches of
   about five, as T19 does: `tpcds` q16 q33 q61 q77 q90 q94 q95, `tpch` q2, `filter-project`,
   `hash-join`.
2. For each, **either enable the device cells or update the ticket**. The causes are ordered, so a
   cell that stops failing on #187 lands on whatever refuses next rather than going green — expect
   [#152](../tickets.md#t152). A cell whose cause changed is a ticket edit, not a cell that stays
   where it was.
3. Close #187 only when its cells are gone from the registry, and correct its text as it closes.

---

<!-- archived from llm-wiki/tasks/casts.md -->

**Obsolete — approach rejected, never merged; PR #136 closed 2026-09-10.** Predicting the export type at plan time and casting at the unload builds the divergence into the plan; the operator harness reports it instead.

# Export types: predicted at plan time, carried on the wire

Kind: production

Two device cells fail on the same class of thing — the device hands back a column whose Arrow type
is not the one the plan declared — and each is currently discovered at the boundary rather than
predicted before it. This task writes the prediction down where it can be checked, and closes both.

**This task closes [#183](active-tickets.md#t183)** — the device exports `Utf8` where the sink declares
`Utf8View` — by predicting the export type at plan time and casting the one divergence that is
inherent.

Its sibling [`wire-schema.md`](wire-schema.md) closes [#187](active-tickets.md#t187) with the same
derivation and the C++ half this task deliberately leaves out. Do this one first: it is Rust-only,
and the prediction it writes down is what the other one then makes true for decimals.


## Why they happen

`unload` concatenates the decoded IPC batches against the sink's declared schema
(`gpu_backend.rs:166`), and `concat_batches` requires exact type equality. Neither side is
misbehaving:

- cuDF has exactly one string type. `expr.cpp:74` maps the `Utf8View` tag to `type_id::STRING` and
  `cudf::to_arrow_schema` maps that back to `arrow::utf8()`. The divergence is inherent and the
  cast is the only place to absorb it.
- cuDF's decimal type carries **scale but not precision**. `export_table_to_ipc` builds its metadata
  as `col_meta.push_back({name})` — name only — so `decimals_to_arrow` falls back to
  `metadata.precision.value_or(max_precision<__int128_t>())`, which is 38. Every decimal exports at
  precision 38 with scale preserved. That is why `tpch/filter-project` declares `(15,2)` and receives
  `(38,2)`, and why `q6` never hit it: its sums declare `Decimal128(38,4)` already.

The second is a missing argument, not a disagreement between two engine rules. #187's current text
frames it as the CPU's `widened_decimal` and the device's concat disagreeing about the same bytes;
that framing is wrong and the ticket should be corrected when it is closed.

## The work

### 1. `export_type_for` — the composition, written down once

The round trip Arrow → fb → cuDF → Arrow lives in three files and two languages today, so nobody can
answer "what type will the device hand back for this column?" without reading `expr.cpp`. Make it a
Rust function. It is total over the declared type for every case below.

| declared | fb tag | cuDF | exported | |
|---|---|---|---|---|
| `Boolean`, `Int8`–`Int64`, `UInt8`–`UInt64`, `Float32/64` | direct | direct | same | identity |
| `Utf8` | `Utf8` | `STRING` | `Utf8` | identity |
| `Date32` | `Date32` | `TIMESTAMP_DAYS` | `Date32` | identity, via `to_arrow_schema`'s `default:` arm |
| `Utf8View` | `Utf8View` | `STRING` | `Utf8` | **#183** |
| `LargeUtf8` | `LargeUtf8` | `STRING` | `Utf8` | same shape, no corpus query reaches it |
| `Date64` | `Date64` | `TIMESTAMP_MILLISECONDS` | `Timestamp(ms, None)` | no corpus query declares it |
| `Decimal128(p,s)` | `Decimal128` | `DECIMAL128` | `Decimal128(38,s)` | **#187** — predicted here, **not cast here**; [`wire-schema.md`](wire-schema.md) removes the divergence instead |
| `Null`, `Float16`, `Binary`, `LargeBinary`, `BinaryView` | mapped | **`EMPTY`** | — | **refuse** |

The last row is not a cast. `convert_data_type` serializes all five and `fb_to_type_id` has no case
for any of them, so they reach the device as a typeless column. `export_type_for` returns `Err` and
the plan is refused at planning time, which is what happens to them today only by accident.

### 2. Derive `exports` inside `attach_recipes`

Not a separate pass and not in the recipe writers, which are the wire codec rather than a
planning phase. The recipe walk already delivers the input:

```rust
fn unload(
    _node: &GpuUnload,
    _inputs: &[&Schema],     // the sink's declared schema, already here, unused
    _writer: &mut Writer,
) -> Result<Option<Recipe>, PlanError>
```

`_inputs` is exactly what `export_type_for` needs, handed to the one function already positioned at
the sink. Deriving it there costs no second traversal and creates no second place that has to agree
with the first — the hazard `driver/index.rs:151` exists to guard for node numbering.

The result is a per-column identity-or-cast list, carried to `GpuSink::new`
(`gpu_backend.rs:124`) alongside the schema the driver already passes it.

**The cast at `unload` must be narrow.** `concat_batches` against the declared schema is today the
only check that the device produces what the plan says it produces; it is what surfaced #187. A
blanket cast-to-schema fixes #183 and destroys that. Only the arms the table marks as divergent are
cast; every other mismatch still fails, as `export_table_to_ipc`'s `DECIMAL32/64 → DECIMAL128`
widening is a named normalization rather than a general one.

Two review points, both of which lose a check if missed:

- `self.schema.fields().zip(batch.columns())` truncates silently on a column-count mismatch, which
  `concat_batches` catches today. Check the length explicitly.
- Non-string divergences would fail at `RecordBatch::try_new` rather than at `concat_batches`, so
  `"the exported stream is not the sink's rows: {error}"` moves with them or #187-class failures lose
  the message the tickets quote.

### 3. Unit tests

`recipe/tests.rs` already asserts on the buffer where the recipe cannot answer, with the idiom at
`a_finalize_project_emits_the_group_keys_the_finalize_list_leaves_out`: build the node, run its
recipe fn, `writer.finish()`, `flatbuffers::root::<fb::GpuPlan>`, `node_at(seq)`, assert on payload
fields. No GPU, no session, no plan load.

- **exports at the unload** — pure function of the input schema, so it needs neither the buffer nor
  `finish()`. A `GpuUnload` over `Utf8View` + `Decimal128(15,2)` + `Int64` derives a list naming the
  divergent columns and omitting the identity ones.
- **the `EMPTY` class refuses** — `Binary`, `Null`, `Float16` return `Err` rather than passing
  silently. This is the arm most likely to rot, since no corpus query reaches it.
- **a decimal predicts but does not cast** — `export_type_for` reports `Decimal128(15,2) →
  Decimal128(38,s)` and the unload's cast list omits it, so the prediction is recorded before its
  fix exists. `columns_of` hardcodes `DataType::Int64`, so this needs a sibling taking
  `(name, precision, scale)` — which [`wire-schema.md`](wire-schema.md) reuses.

`recipe/tests.rs` is at 887 lines against the 1000-line cap. These land around 930 — under, but the
next addition to that file forces the split rather than this one.

## Restriction

**Code and test changes are limited to what is written above.** No refactor of the surrounding
writer, no generalizing `export_type_for` beyond the table, no second cast site, no cleanup of
the recipe writers while passing through them. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt` (10 files) | every `GpuUnload` line gains `exports=` | new field; `GpuUnload` renders bare today |
| `<mode>-<tier>.cpu.txt` | **no change** | `memory.rs:43` prices `Utf8View` and `Utf8` identically at `(rows+1)*4`, and content size is Σ value lengths for both |
| `<mode>-<tier>.cost.txt` | **no change** | derived from the `.cpu.txt` sections, which do not move |
| `<tier>.result.txt` | **no change** | values are unaffected; only types were ever in question |
| `testdata/cost-registry.csv` | device cells move off #183/#187 | see the device workflow below |

`exports=` renders on every unload, including `exports=none` where nothing diverges. Omitting the
attribute would make "nothing diverges" and "the list was never computed" identical in the golden,
which is the invisible-absence shape `coding-style.md` records twice.

`exports=` is a Rust-side prediction that the C++ never receives: the unload writes no payload, so
nothing on the wire describes the sink's columns. The golden is documentation and a tripwire, and
the runtime check at `unload` is the only thing that enforces it. [`wire-schema.md`](wire-schema.md)
is what makes it checkable against the buffer.

## Device workflow

The cast and the precision are not proved by a green CPU tier — every cell they exist for is a device
cell that is currently disabled. After the code lands:

1. Run the affected corpus queries on `shad-gpu` with `build-test-shadgpu.sh`, in batches of about
   five, as T19 does. **59 queries carry #183** — every query with a string in its sink schema,
   which is derivable from `tp1-single.plans.txt` without running anything, and was: no query
   without a string in its sink carries #183, over 82 checked, with no exceptions.
2. For each, **either enable the device cells or update the ticket**. A cell that now reaches a
   different cause is a cell whose ticket changes, not a cell that stays where it was — and the
   causes are ordered, so fixing #183 will move cells onto whatever refuses next rather than turning
   them all green. Expect #152 to be the common landing place.
3. Close #183 only when its cells are gone from the registry, not when the code lands. A ticket
   whose cells are still disabled against it is not closed.

---

<!-- archived from llm-wiki/tasks/drop-legacy-modes.md -->

# Drop the legacy execution modes

Delete the six legacy execution modes — CPU full-table at tp1 and tp8, CPU partitioned,
GPU all-at-once, GPU full-table, GPU partitioned — with the planning they rest on, the
tests, the goldens, the benchmark records and the widget columns that describe them. What
stays is the batch-partitioned planner and its two backends.

## What goes

- **Rust**: `executors/` (five mode classes, the node-by-node driver, the streaming
  driver, the resident enforcer), `operators/` (the 16 `Gpu*Exec` wrappers and their
  serializers), `gpu_rule.rs` (both physical optimizer rules), `plan_serializer.rs`,
  `resident.rs`, `cpu_executor.rs`, `gpu_executor.rs`, `node_executor.rs`, and
  `CpuExecutor` in `lib.rs`.
- **C++**: `execute_plan.cpp` and the `peacock_execute` ABI entry point. `execute_node`
  stops being a recursive driver and becomes what it always was on the node path: the
  resolver that hands an operator its next already-resident input.
- **Tests**: the fourteen legacy targets and the harness that only they used
  (`common/exec_mode.rs`, `common/benchmark.rs`, `common/gpu_cases.inc`).
- **Goldens**: every `.plan.txt`, every `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`
  and `plan_bytes.sha256`; `testdata/benchmark-results/` with the harness that wrote it.
- **Widget**: the legacy table, the six legacy registry columns, and the second PR
  comment the four tables needed.

## What is kept, and where it moved

The batch-partitioned side reached into the legacy tree in four places, so each moves
rather than dies: the per-node DataFusion runner (`cpu_backend/single_node.rs`), the three
Arrow-to-wire helpers the recipe writers share (`recipe/wire.rs`), `parquet_table_name`
(`parquet_meta.rs`), and the small-table threshold, which two test files spelled
separately and is now `plan::SMALL_TABLE_BYTES`.

## Coverage this removes and does not replace

Two guards were defined as "the recipe writer against the legacy writer" and cannot
survive it: `every_field_the_legacy_writer_sets_is_set_here_or_declared_a_difference` and
the seven expression cases in `recipe/expr_writer/tests.rs`. What they added over the rest
was a second, independent producer of the same bytes; the payload digest still pins what
this writer writes. Say so in the PR rather than quietly.

The C++ operator gtests keep their coverage: `test_plan_executor.cpp` drives its
hand-built plans through `NodeSession` node by node, the way the driver does.

## Verification bar

CPU: the rust-only suite and `ctest -L cpu` locally, `cargo test -p cost-report`.
GPU: the five staged targets on shad-gpu. Both green before the PR.

**Merged 2026-09-08 as PR #140**, three commits against master. What the review added beyond
the spec: thirteen loose root files had ridden into the deletion commit and the repo root is
now ignored by extension; the free `execute_node` became `take_input`, since it executes
nothing and shared a name with the one that does; the CLI was rewritten onto the planner and
nothing compiled it, so CI builds it and `test_ci_coverage` guards that. The GPU job's rust
loop was found printing nothing and guarding nothing — its `cat` and `grep` segfaulted under
an exported glibc — which is where the per-command library path, the crash detector and the
single-GPU concurrency group came from.

---

<!-- archived from llm-wiki/tasks/batch_partitioned_executor.md -->

# batch-partitioned executor: the implementation plan (T0–T22)

**Closed 2026-09-08, and archived whole.** Every task here is done except two: T20 became
[#195](../tickets.md#t195) and T22 is obsolete as written (see its entry). It is kept verbatim
because commits, reviews and the archived specs below name these numbers, and this is where a
reader resolves them; what the tasks built is described in
[`architecture.md`](../architecture.md).

Tasks in dependency order, and the numbers now ascend with it. T13 is the one that does not:
it landed early, because both drivers over a mock backend needed none of T9–T12, and it keeps
its number because commits and reviews already name it. T21 sits out of order for the same
reason — it was split off T14 after it had been narrowed — and the tail of the list runs
T20, T22 because 21 is spent. T11 and T12 were retired in the same
renumbering — their work is T15 and T16 — so a number is never reused and an older reference
still resolves. Each task is one developer hand-off with its own proving tests.
Legacy tests stay green throughout — every task that touches shared code runs the
affected legacy subsets (one query per mode/tier per binary plus the rust-only tier, per
build-test.md).

~~**T0 — Python prototype of the whole execution model**~~ (done). All node types and both drivers
in Python, operators built with pandas, plans hand-built (no DataFusion, no planner) — an
emulation of tree execution whose purpose is to settle the push model before any Rust
exists. Lives in [`scripts/exec_model/`](../../scripts/exec_model/README.md); its tests run
in CI (cost-report, plus the TPC-H set in dataset-matrix, which has the generated sf1).

Done — struck through, and folded into this document where it changed a decision:

- ~~the trait set, both drivers, and the memory enforcer with the accounting formula~~;
- ~~the scheduling rule~~ — height, order, min-height-first with leftmost ties, every lane
  of the chosen node; the Drivers section is rewritten from it;
- ~~the backpressure rules~~ — a join in its build phase holds its whole probe subtree; a
  satisfied limit holds its whole subtree for good. Both were findings, not designs;
- ~~queues need no cap~~ and ~~`Pending` does not exist~~ — the draft's two flow-control
  mechanisms, both dropped, both because runnability is a predicate evaluated before the
  call;
- ~~pandas-backed operators~~ — filter, project, sort, the aggregate sequence with its
  partial/final decomposition, the accumulators, the hash scatter, the join capability
  matrix, and the T2 row-group partitioning policy, each written against the pandas/cuDF
  intersection with the divergences named;
- ~~every query checked against a single-shot oracle at five partitioning configs~~, the
  prototype's version of two-engine correctness;
- ~~both limit lowerings~~, as the limit rule now states them. One finding survives here:
  the tests must assert on the *calls*, since only those distinguish a limit from a filter
  applied after the transfer;
- ~~the stress surface~~ — a plan rewriter (`operators/injection.py`) rather than
  hand-written variants: one plan re-run at every partitioning, batch size, empty-lane and
  hash-placement preset, with `GpuCoalesceBatches[target]` injected above every source
  (#139's node, proving the drivers tolerate it anywhere) and sources emitting zero-row
  batches at a set probability. It carries one rule the planner's tests should quote: a
  join may be re-partitioned only when both sides are hash-partitioned on the join keys,
  since otherwise its lane count is load-bearing and splitting it joins matching slices;
- ~~empty partitions, empty batches, skewed hashes, the flow-and-backpressure surface,
  determinism (two runs, identical batch traces)~~;
- ~~validation scope~~ — partitioning and `SingleBatch` constraints in scope, schema checks
  not.

- ~~the hand-built plan corpus~~ — 22 TPC-H and 71 TPC-DS query texts rather than the
  3–4 and ~10 the plan asked for, each at three layouts and on both join backends. It was
  the piece most likely to find something and it did: [what the corpus rollout
  measured](#what-the-corpus-rollout-measured) is the section it produced, and every item
  there is a property of the design rather than of the prototype.

Closed without the **estimator** (`estimated_max_resident_size`, `target_batch_bytes`).
The prototype models scratch per executor and never derived batch sizes from a budget,
and T6 derives both in Rust directly — a prototype estimator would be a second model to
keep true against the one that ships. The corpus is what T6 will calibrate against.

~~**T1 — flatbuffer operation-name refactor**~~ (done). Nine of the fifteen legacy node-kind names
(`GpuFilter`, `GpuProject`, `GpuSort`, `GpuAggregate`, `GpuCrossJoin`,
`GpuNestedLoopJoin`, `GpuUnion`, `GpuLimit`, `GpuCoalesceBatches`) collide with the new
mode's node names. Rename the fbs tables and `PlanNodeKind` variants to a `Cudf` prefix
(`CudfScan`, `CudfFilter`, …) so the two vocabularies are visually distinct everywhere —
schema, generated code, the C++ `node_type()` switches and serializer identifiers on the
Rust side. A pure rename: FlatBuffers wire bytes carry no table names and enum ordinals
do not move, so the proof is `plan_bytes.sha256` staying byte-identical with no
regeneration, plus green legacy subsets. The same commit sweeps the llm-wiki references
(architecture.md's fb names, affected tickets, and the recipe-plan table in this spec).

Landed on master as PR #120, with `plan_bytes.sha256` byte-identical and no golden
regenerated, which is the proof the rename asked for.

~~**T2 — ParquetBatchPartitioner.**~~ The pure policy class and its unit tests: fewer
survivors than N; N=3; single row group over target; batching off ⇒ one batch per chunk;
empty survivors (explicit error — the fbs "empty map means legacy single partition"
convention must not leak in); the balance bound on uniform row groups (max−min partition
rows ≤ one row group);
fixed-output determinism case. No planner integration yet.

~~**T3 — node and trait skeleton.**~~ `GpuNode`, `PartitionLayout` (with the two-valued
`SortOrder`), `Schema` with semantics annotations, `Batch`/`CpuBatch`/`GpuBatch` shells
with the move/`!Clone`/`Drop` rules, executor trait definitions with `CallStats`,
`Backend`. Traits in their own files per coding-style. Compiles under rust-only
with the GPU side gated. Unit tests: `SortOrder` canonicalization, layout equality.
**First, before anything else in this task**, compile a skeleton: the `Backend` trait with
all seven associated types, two impls whose `Batch` types differ, `NodeExecutors<B>`, and a
generic function driving one build→probe→finish transition and one source step. It
compiles with no `dyn` anywhere (verified), and it is what pins the static-dispatch
property the GPU path depends on — the mock backend the driver tests need is then a third
impl, not a special case.

~~**T4 — translation layer, single-partition shapes.**~~ DataFusion physical plan (tp1) →
`GpuNode` tree for chains: load, filter, project, sort (+fetch), limit (root-adjacent ⇒
no node, `skip`/`fetch` set on `GpuUnload`; otherwise a `GpuLimit` node over a
planner-inserted `GpuMergePartitions` — never a coalesce), coalesce-all,
single/final aggregates, cross/nested-loop joins. Per-node-kind conscious mapping;
unrecognized node ⇒ plan-time error naming it; window ⇒ the #143 refusal. Unit tests
assert emitted constructs for simple queries.

~~**T5 — translation layer, partitioned shapes.**~~ tp4: shuffle points → Merge+Emit, the
aggregate sequence with its shortcuts and the gid rule, join side normalization (type
remap + column-order-restoring project) and build-side coalesce insertion per the
capability matrix, union/interleave with explicit branch-cast projects. The
`hashKeys ⊆ group columns` structure is produced here (validated in T8). Unit tests per
construct in tp1 and tp4, including side-swap cases.

~~**T6 — estimator pass and plan goldens.**~~ `estimated_max_resident_size` per node
(rows × width vocabulary, N-lane charging), `target_batch_bytes` derivation feeding T2's
partitioner, integration as `plan_batch_partitioned()`. Canonize all four
`<mode>.plans.txt`, memory sections included, for TPC-H and TPC-DS (minus #23's four and
window queries, which appear as refusals).

~~**T7 — schema registry.**~~ (done). The `Schema` carried in `NodeKind` populated on all
nodes, with column semantics annotations. Unit tests: hand-crafted plans produce expected
types and annotations; decimal precision/scale fidelity through project/aggregate/union-cast
paths.

The type and the annotations landed with T3-T6; the tests are PR #126. They assert on the tree
rather than on rendered text, since both engines derive their per-node bytes from the same
declared schema and a wrong type moves no golden byte — `agg_state` at the init, the per-lane
merge and the finalizing merge, and `avg`'s state columns typed by what they hold rather than
by position, which is the case the task existed for.

~~**T8 — validation.**~~ (done). `validate_schemas_and_partitions()` on every node type: partition
topology, key-distribution subset rule, sortedness requirements (merge requires
`BatchSorted`; a limit after a sort requires its input to be `is_stream_sorted()`, checked
on whichever node carries the interval — the `GpuLimit` mid-plan, the `GpuUnload`
root-adjacent), `SingleBatch`
expectations (join build, cross/nlj inputs), captured-index checks. Unit tests: manually
constructed wrong combinations error, right ones pass; then run validation over every
canonized corpus plan from T6.

Node-local validation landed with T4/T5 and is called from `plan_batch_partitioned`; the
generic pass is `batch_partitioned/validate.rs`, PR #126. It runs over every canonized plan
because the planner calls it, so a rejection renders as a `refused:` section and fails both the
golden compare and the registry cross-check. Three planner defects were found this way and
fixed there rather than ticketed: `NestedLoopJoinExec`'s dropped projection, `GpuJoin` minting
a key distribution instead of carrying one, and a non-exhaustive match that dropped the claim a
mark join earns.

~~**T13 — drivers and enforcer.**~~ (done). Both drivers over a mock `Backend` impl — the
third instantiation, alongside CPU and GPU — with the schedule and the accountant as units of
their own, and the accounting formula with its pre/post checks. What the task settled is in
[Drivers](#drivers) and [Memory accounting](#memory-accounting); what it left for T14 is every
real executor, since nothing here computes a row.

~~**T9 — additive ABI.**~~ (done). The three approved symbols in `gpu_executor.cpp` + `peacock_gpu.h`,
signatures as [GPU execution](#gpu-execution-through-the-frozen-ffi) gives them; any
*further* surface change goes through a proposal to the human, per the constraint section.
Rust bindings for all three; `GpuBatch` handle plumbing (session ref, `Drop` release,
`ManuallyDrop` consume boundary). Tests: a C++ gtest in the plan-executor suite reading
disjoint row-group subsets and asserting union == whole-scan; a gtest exporting ranges of
one handle and asserting the concatenation equals the whole, plus the empty range and the
past-the-end range; the same for slicing, plus that the input handle is released and
double-slicing it fails; Rust FFI smoke on shad-gpu. The range plumbing reaches
`UnloadExecutor::unload(batch, rows)`, so the trait's second argument lands here rather
than in T10.

Landed with two shapes worth knowing. The row range is one function, `clamp_row_range`, that
the export and the slice share, so the two cannot disagree about an overrun; and the row-group
override reaches `execute_scan` as a `cudf::host_span`, so the node's own vector and a caller's
array take one path. `RowRange` and `unload(batch, rows)` were already in from T13, so the
trait needed nothing. One test moved tier against the list above: two IPC streams do not
concatenate, so "the ranges are the whole" is asserted in `test_gpu_abi`, where arrow-rs decodes
them, and the gtest holds the contract edges instead.

~~**T14 — recipe-plan serialization.**~~ The `GpuNode` → fb-seq mapping implemented, canonized and
unit-tested. `attach_recipes()` runs after the plan is complete and hangs a recipe on each node
that drives the GPU ABI; a node that makes no ABI call gets none, which is a fact about the node
and so is worth reading off the plan. One function per node kind produces that node's recipe from
**that node alone** — no child, no parent, no tree walk. The mapping table is a per-node statement
and a function that can reach a child would let it stop being one, so the restriction is the
design rather than an economy.

A recipe is a sequence of ABI calls, each carrying the built FlatBuffers node it addresses — the
payload, not a reference to where the fields live. Two renderings, one function taking an enum:
without payloads it is a section in every `<mode>.plans.txt`, between the plan tree and
`--- memory ---`, keeping the tree shape and repeating nothing the tree already shows except the
lane count; with payloads it is a golden of its own, holding the recipes alone — no plan tree, no
memory — for a subset of queries chosen to reach every fb kind and every call shape longer than
one call. A digest of the serialized bytes rides beside the payload text, since text and bytes can
disagree and `plan_bytes.sha256` is the precedent for pinning the wire form rather than a
description of it. Unit tests cover the kinds whose recipe is more than one call, `GpuJoin` first:
the seq set and call pattern per join type, against the
[capability matrix](#join-capability-matrix).

Dense seqs are impossible, and that is a property of the fbs rather than a choice. Children are
nested (`input`, `left`/`right`, `inputs`) and `CudfScan` is the only leaf table, so a set of
addressed nodes whose arities exceed its own edge count has to be padded with stub scans — and
every stub takes a post-order slot, so it moves the seqs above it. Three rules follow: stubs
rather than a re-hung child, since a shared offset is a DAG and gets indexed twice; a call whose
input is a runtime handle hangs off the previous fb node of its own recipe, or a stub where there
is none; and a forwarder's unconsumed branch is gathered by a structural `CudfUnion`, because an
orphan is never indexed and its shift has no visible cause. The pass and the serializer are
therefore one walk: a seq is the post-order index of what was built, so it cannot be counted
before the building.

The `Expr` -> `fb::Expr` writer is its own file and is where the unit tests concentrate: every
expression variant and operator, nesting, and each scalar kind the corpus produces — decimals
with their precision and scale first, since a wrong write there is invisible in plan text and
wrong on a device. `plan_serializer.rs` serializes a DataFusion plan and keeps that one job;
`serialize_scalar_value` and `serialize_schema` are reused, `serialize_expr` cannot be, since it
downcasts `PhysicalExpr` and this IR is our own.

Post-order is the agreement to assert rather than assume: `begin_plan` indexes post-order, so the
emitted tree has to number exactly as the recipes say, and a plan simple enough to check by its
answer would answer correctly while addressing the wrong node.

Nothing executes here and nothing new runs on a GPU. `scripts/exec_model/operators/recipe.py` and
`recipe_join.py` are the starting point for the join sequences — a model, not a spec, and the fbs
and `cpp/src/operators/join.cpp` are what settle a disagreement.

The proving set is the new unit tests and the golden target, and no legacy subsets — the human
scoped it that way because the change is additive: a recipe is attached to a plan nothing reads
yet, so the only tests whose result can move are the ones that read it.

~~**T21 — a recipe plan on a live GPU, driven by hand.**~~ It needs T14 and nothing else: no driver,
no executors, no scheduling. A new test file on the shad-gpu tier plans a query over TPC-H sf1,
loads the recipe plan T14 already built, and makes exactly the calls the recipes name, threading
each call's output handle into the next one's input and exporting at the root. One helper does the
whole walk; one test per query calls it, so a failure names the query rather than a stage.

`begin_plan`'s `out_node_count` is asserted equal to the number of fb nodes the writer created —
not to the `GpuNode` count, which is a different number: stubs, structural unions and any node
with more than one call all separate the two, in both directions and in most plans. It is the
first thing this task can settle that nothing before it can: every seq indexes a post-order the
C++ builds in `index_post_order`, and until a device has parsed a buffer we wrote, our agreement
with that walk rests on two child-order functions having been read side by side. One assertion, at
the first call, in the first place both numbers exist at once.

Shapes, chosen so each call is unambiguous. Everything but the aggregates plans one partition and
one batch, which makes every recipe a single call per node and the walk a straight line: a bare
scan, a filter, a project over a filter, and the joins — inner, and one build-preserving type,
whose single probe batch takes the legacy one-call form. The aggregates plan one batch and **two**
partitions, because a merge is the operator this mode adds and one partition never performs one:
two lanes each merge their own state, the cross-lane merge folds them, and the finalize project
runs once. That is the first time `AggregateMode::Merge` and the finalize expression meet a
device.

`avg` is the case worth a test of its own. Its finalize divides a decimal by a count, and cuDF
derives a divide's result scale from its operands where arrow takes it from the declared output
type — so a wrong cast is invisible on a CPU host and wrong on a GPU, in a column whose type reads
correctly either way. Assert the digits, not the type.

The oracle is DataFusion on the same SQL — `data_fusion_exact`, the CPU tier's own vocabulary —
and deliberately neither a result golden nor our CPU executor. A golden records what the first run
produced, so a finalize whose scale is wrong from the start is pinned rather than caught; and our
CPU executor evaluates the same finalize expression the device is sent, so it agrees with a wrong
one. DataFusion computes `avg` without a Welford triple, a merge mode or cuDF's divide-scale rule,
which is what makes agreement with it evidence. Joins compare as sorted multisets, since a GPU
join's output order is not deterministic. What it deliberately leaves out is everything the
driver decides — batching, backpressure, arrival order — since every shape here is one batch;
those arrive with the executors, and the driven end-to-end over every layout is T17's.

What has a device behind it. Ten fb kinds have run on one; every other is refused by name in a
`match` over every `FbKind`, so a variant added later stops the file compiling rather than going
quietly unclassified. Still unproven on hardware: [#136](../tickets.md#t136)'s finish pass — probe
keys per batch, the concat at done, the finish join, the pad project — the whole Right family,
cross and nested-loop joins, both sort nodes, `slice_handle` and a ranged export.

~~**T10, T15 and T16 — the executors, as one task.**~~ All three land together on one branch, because
they are one question asked of three node families: what does an executor do when the recipe
already says which calls to make. Ordered inside the task as T10 then T15 then T16, since the
accumulators and the joins are the Exec executors' shapes with state added.

~~**T10 — Exec executors.**~~ Filter, project, per-batch sort, aggregate (partial/single), unload
(`GpuBatch → CpuBatch`, honouring the row range). The **GPU executor runs the recipe attached to
its node** — the calls, in order, with the handles threaded — and reuses no legacy operator code:
the recipe is the instruction set, and reaching into legacy operator internals would be a second
path to the same kernels. The **CPU executor relays to DataFusion**, where reuse with legacy is
expected rather than avoided, since both are asking DataFusion for the same operator.

~~**T15 — accumulators.**~~ `GpuCoalesceAllBatches`, `GpuAggregateBatches` (merge-only and finalizing),
`GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions`, and the mid-plan `GpuLimit`. Edge cases:
zero batches, one batch, ties for the merge (partition-major stability), fetch interaction, large
batch counts, gid-carrying aggregate merges.

~~**T16 — partition ops and joins.**~~ `GpuEmitPartitions` (per-batch scatter at a small N and a
large one, empty outputs for skewed hashes, and the lane each key lands in, asserted on both
backends — co-partitioning is what every partitioned join rests on). `GpuMergePartitions` is not
here: its mapping is `Forwarder`'s, from T13, and its service order is the driver's. `GpuJoin` with
`set_build`/`probe_and_fetch`/`finish_and_fetch`, plus cross and nested-loop joins on the same
trait. The [capability matrix](#join-capability-matrix) is emulated as a test table — per
(type × layout): stream-vs-refuse, correctness against a hand-built oracle, the GPU finish pass via
key accumulation ([#136](../tickets.md#t136)), `null_equals_null` on the finish join. That finish
pass is the one shape this mode invented with no device behind it after T21, which is why the
matrix is emulated here rather than assumed.

**What a copy costs decides the matrix, and the copy does not exist yet.** Every handle is erased
by its reader, and the frozen surface has no copy symbol, so a shape whose recipe names
`BuildSideCopy` meets a second probe batch with a dead handle. The question T16 had to settle was
whether to keep claiming those shapes stream and refuse until [#145](../tickets.md#t145), or make
a single-batch probe the matrix's permanent rule. Over the 37 hash joins in the
partitioned-tp8-standard goldens a copy would cost 0.08 of the probe stream at the median and more
than it for 12, so a permanent single-batch rule would price every join at the worst one: the
claims stand and the device refuses, naming [#152](../tickets.md#t152), with a test on the
refusal.

Left and Full outer go further and have **no device path at all** until then, which is #152's
second row rather than its first: their key project and their per-call join read the same probe
batch, so no ordering of the two leaves both an input. The finish pass's pad is therefore proved
on the CPU alone, and the device test asserts the refusal.

**How everything here is tested.** Small synthetic data, never the corpus; plans hand-constructed
rather than planned, so a test names the shape it means instead of hoping a query produces it;
`attach_recipes()` is fair game, since the recipe is what a GPU executor consumes. The oracle is
hand-constructed too: an expected result written down, not derived by the code under test. CPU and
GPU tests in separate targets so CI hosts split them. A device test writes its own parquet, which
is the ABI's doing rather than an exception: the four entry points load a table only by reading
one, so a device test's input is a scan or nothing. What the rule excludes is tpch.minimal and the
generated sf1, whose values nobody chose.

**What this task does not do.** No driver: nothing here is hooked into the schedule, and every
assertion is about one executor answering one call. That defers the whole class of claims that
read as call counts and pull counts — a limit holding nothing whatever the offset, at most two
batches sliced per query, the scan stopping — to T17, which is where a driver exists to make them.
`PlaceholderRowExec` ([#158](../tickets.md#t158)) waits for the same reason: it is a source, and a
source proves itself by what the driver pulls from it. T17 then found it cannot be discharged at
all while the surface is frozen — see the sixth entry under
[What the frozen surface costs](#what-the-frozen-surface-costs-and-what-unfreezing-would-buy).

~~**T17 — the whole path, under injection.**~~ The first task in which SQL goes in and rows come out:
planning, the recipes, the executors and both drivers running together, rather than each proved
against a fixture of the last one's shape. Every test starts from a query's text and ends at its
results, so what is under test is the join between the pieces — which is the only part four tasks
of separate proofs cannot reach.

The oracle is DataFusion on the same SQL. Not the legacy CPU executor, as this entry said before
T21: a second engine of our own agrees with us wherever we are consistently wrong, and by the time
this task runs, the finalize expression it evaluates is the one we also send to the device. The
one independent implementation in reach is the one that decomposed the aggregate differently.

Queries chosen to be interesting rather than representative, over the sf1 corpus text, and
between them covering the [join capability matrix](#join-capability-matrix): every join type this
mode claims, crossed with the layouts that make each one stream or refuse. The matrix is emulated
on synthetic data in the executors task, where each type is one executor answering one call; here
it is planned from SQL and run through the drivers, which is the first time a type's claim is
tested as the thing a user gets rather than as the thing an operator returns.

Four shapes are not join cells, so no join cover reaches them, and each is named by the query that
carries it:

| Shape | Query | Why that one |
|---|---|---|
| union lowered to an interleave | tpcds q33, q56, q60 or q66 | the claim is output lane p from lane p of each branch, so it needs four lanes; q14 also interleaves and is the trap, since its is `lanes=1` |
| union that cannot interleave | tpcds q77 | its branches disagree on lane count — 4+1+4, and the golden says `lanes=9` — which is the case [Node set](#node-set) argues in prose and nothing executes |
| both row-interval lowerings | tpch nested-limits | the root-adjacent interval becomes `GpuUnload`'s skip/fetch and the mid-plan one a `GpuLimit` over the scan; the only `OFFSET`s in either corpus, and it has no `.cpu.txt`, so this is its first execution |
| a merge with state worth merging | tpch shuffle-stddev | `GpuAggregateBatches` rides in most of the join queries as a sum; this is the Welford init, both merges and the finalize project |

Nested-loop Left is the one matrix cell no corpus query reaches, and its shape — a single-batch
probe, since #136's finish trick accumulates keys and a predicate join has none — is reachable
from no other row, so this task writes the query. The other uncovered cells stand: an Inner with
`null_equals_null` (the flag rides an INTERSECT lowering, and every corpus INTERSECT lands as a
semi form) and the three plan-time refusals, which a corpus query cannot provoke by construction.

Each query is re-run under injection, several modes rather than one, with the same answer demanded
every time. The prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) is
where to look for modes worth having — layouts re-planned rather than edited, a rebatcher above
every source, sources emitting zero-row batches at a set probability — and it is a model rather
than a specification, so a mode it lacks and this path needs is a mode to add. Rebuild rather than
edit, for the reason the prototype records: a node's partitioning is not a field, so a rewrite
re-runs the planner at a chosen `(target_partitions, batching, small_table_bytes)` and the shapes
come out consistent.

Two rules the injector carries and this one must too: a join may be re-partitioned only when both
sides are hash-partitioned on the join keys, since otherwise its lane count is load-bearing and
splitting it joins matching slices; and a degenerate hash — every key into one lane — is a legal
hash, because a shuffle's contract is co-location and nothing above it may depend on how evenly
the lanes were loaded.

It also inherits what the executors task could not assert without a driver: a limit holding
nothing whatever the offset, at most two batches sliced per query, the scan stopping — each a call
or pull count — and `PlaceholderRowExec` ([#158](../tickets.md#t158)), which is a source and so
proves itself by what a driver pulls from it.

`PlanIndex` gets the unit tests it has never had, and they belong here because this is the first
task whose failures would be read through it. Nothing tests it directly today: `PlanIndex::build`
has one caller, and the scheduler tests derive their own subtree ranges from a parents array rather
than taking the index's. Assert what the derivation decides rather than what a plan happens to
produce — pre-order numbering and the contiguous subtree range that every hold rests on, `parent`
and the snapshotted children, and the three counts a category changes: `ready_lanes` against
`lanes` for a cross-lane accumulator and an emitter, `input_lanes` for the `Done` events a
partition accumulator owes, and `slot_base` where it is lane-scoped against where it is not. Each
of those is wrong far from where it shows.

**Two numberings meet in the driver, and one walk should compute both.** `PlanIndex` is
pre-order — a subtree is a contiguous range, which is what every hold rests on — and a recipe is
keyed by post-order, because that is how the FFI addresses a node. So the index records each
node's post-order position beside its pre-order one, from the walk it already makes, and
`executors_for` takes it. What must not happen is a third derivation: `attach_recipes` numbers at
plan time and the index numbers at run time, so a test asserts the two agree over the corpus —
[#134](../tickets.md#t134) is the same pair one boundary over, and it is unchecked there.

A source executor is this task's to write, and so is the answer to a lane with no build batch: a
`set_build` that never happens because `GpuCoalesceAllBatches` emitted nothing is a driver
decision, not an executor one, and T16 left it here deliberately (the finish's own zero-key answer
is already settled).

The row range the driver hands an unload is asserted before the call. `clamp_row_range` absorbs
an offset past the end and a length past it, because a C ABI has to be total — but
`RowInterval::range_of` cannot produce either, so the tolerance can only be reached by a driver
whose `rows_seen` has drifted, and what that looks like is a `LIMIT` quietly returning short.
Assert non-empty and within the batch where the driver builds the range, so the arithmetic names
itself rather than being absorbed.

The mock backend gets a handful of its own for the same reason one level up. Every assertion in
`driver/tests/` is measured against it, so a mock that miscounts is 1255 lines of tests agreeing
with the wrong answer and staying green. Pin what a script says against what the mock does — the
scripted batch counts and sizes per source and lane, the skew pattern an emitter fills its lanes
by, and an accumulator emitting where the script says it emits. A few cases, not a suite: what is
being checked is that the instrument reads what it was set to.

It starts by wiring the executors to their traits. `Backend` names seven associated types, so no
earlier task can implement it — T16 finishes the last of them and none of them owns a source — and
`Executor`'s `resident_bytes`/`scratch_bytes` are the memory accounting this task adds. So the
executors arrive as inherent methods in the trait's shapes, and the first commit here is the one
that makes the compiler check that.

**Defects found here are fixed here.** Every task before this one proved its own layer against a
fixture; the first thing to run all of them at once will find things about their joins, and
parking those behind tickets would leave the path unproven in exactly the way this task exists to
end. T21 is the precedent: it was meant to be one test file and it found four defects, each a
rule held by a doc comment with nothing reading it, and one guard that could not go red.

~~**T17a — layout injection over corpus queries.**~~ (done). T17 runs seventeen queries at five modes and
calls it injection; it is not. Each of the five is a plan the planner would have chosen anyway —
`(target_partitions, sizing)` re-planned, with `small_table_bytes` and the budget constant. The
prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) does a different
thing: it takes one plan and rewrites it into layouts no planner would emit — lanes deliberately
drained, a degenerate hash, a rebatcher above every source cutting against the grain, sources
emitting zero-row batches at a probability. Four dimensions, none of them reachable from SQL, and
none of them exercised by any real query today. They live in `driver/tests/stress.rs` over a mock
whose answer is a script.

**Why T17 could not do it, and what this task has to build.** `GpuNode` exposes `children()` and
nothing that reconstructs a node — no `with_children`, no `rebuild` — which is why T17 re-planned
instead. But the rewrite is writable without touching production: `as_node_ref` is a public
exhaustive `NodeRef` over all eighteen kinds, each node's steering fields are `pub`, and each kind
has a public constructor. So a test-side `rebuild(node, new_children)` is eighteen arms that read
the fields and call the constructor, and because the match is exhaustive a nineteenth kind fails to
compile rather than being silently un-rewritten — the same guard `node_kind()` and `driven` already
rest on.

**Prove the rewrite before using it, and a unit test is the right size.** A rebuild that drops a
field is a plan that differs from the one under test for a reason nobody chose, and every result
after it is then about a different query. The case is small: hand-built plans rebuilt with their own
children, identical in **debug output** — not in the rendered plan. The renderer is what a golden
reads and it does not print everything: a loader's survivors and `can_be_null`, and an aggregate's
intermediate schema, reach no plan line, so a rebuild that drops `can_be_null` renders identically
and passes. Those are exactly the fields a corpus plan never varies, which is what would have made
the rendering-only form a guard that cannot go red. Debug prints every field including the private
ones and is the identity. Nothing in this case reads a committed file: the comparison is a plan
against its own rebuild, so it holds without goldens and moves when neither of them does.

**Make the field cover exhaustive rather than claimed.** Fixtures that populate every optional
field are a hand-maintained list, and the failure it misses is the one that will happen: a field
added to a node next quarter, no fixture varying it, the arm free to drop it, the test green.
Three levels, and the third is the one worth building:

- *arms* — the match over `NodeRef` is exhaustive, so a nineteenth kind fails to compile;
- *kinds* — the fixture set is asserted to reach every variant, so a kind with no fixture is red;
- *fields* — derive them from the debug output rather than listing them. Every field name a node's
  debug prints must take **at least two distinct values across the fixture set**, because a field
  constant everywhere is a field whose loss no fixture can detect. It goes red naming the field,
  and it goes red the day the field is added rather than the day something depends on it.

That last one is what makes the case exhaustive without reflection and without a maintained list.
It is also a guard that can go red on its own, which on this chain has been the exception. Not through
`driver/plans.rs` — it is `#[cfg(test)] mod plans`, so an integration test cannot see it, and its
builders leave every optional field `None`, which is the half the property needs. The fixtures
live beside `rebuild` and give each node a value in every optional field: `Filter` with a
projection as well as without, `Limit` with a fetch, `Join` with a residual. A corpus plan covers
only the combinations its queries happen to produce, and a fixture set of all-`None` nodes would
let a dropped field through in silence.

Cover the arms the way `every_node_kind_builds_the_executor_its_category_names` covers its own:
assert the fixtures reach every `NodeRef` variant, so a kind added later is a compile error in the
match and a red count here. It goes red by dropping one field from one arm. Nothing is injected
until it passes, and no corpus query is needed to prove it.

**Two mechanisms, two dimensions each, because the dimensions are not the same kind of thing.** A rebatcher is a node —
it changes the tree, and it must be placeable anywhere a batch flows, not only above a source. The
other three are behaviour a node does not carry: this engine's hash is Spark-murmur3 fixed in the
emitter, and neither an empty lane nor a zero-row batch is a field of `GpuLoadParquet`.

| Dimension | Mechanism | Why not the other one |
|---|---|---|
| a rebatcher, at any edge that still validates | tree rewrite: insert a node above the chosen child. One direction only — nothing below the loader splits a batch ([#142](../tickets.md#t142)), so the node is `GpuCoalesceAllBatches` merging a lane to one, and the finer direction is the mode axis already. A coalesce clears the sort order by construction, so an edge under a `GpuMergeSortedPartitions` or a limit-after-sort is refused at validation: assert the refusals are exactly that class rather than skipping those edges quietly, since a skipped edge is a hole nothing reports | wrapping an executor cannot do it — an exec is one batch in, one out, and a merging rebatch must hold batches across calls, which is an accumulator's job. A wrapper that held them would lie to the accountant and break the queue bound the driver guarantees |
| a drained lane | tree rewrite: move that lane's row groups into another lane | **not** a wrap — a source producing nothing for a lane loses its rows, so every oracle comparison fails and the dimension is untestable. `partition_groups` is the mapping, so moving groups keeps the lane count and every row. Safe at every plan this mode produces: `co_partitioned()` requires one lane or a ByHash distribution on the join keys, so no join here rests on scan lane alignment — the shape the prototype needed `_is_shuffled_join` for and this mode does not have |
| zero-row batches at a probability | wrap the source executor: emit an empty batch instead of advancing | not a node field; the driver already carries empty batches, so this exercises a path the operators have and this one does not |
| a degenerate hash | wrap the emit executor: route every key to one lane. **Not compatible with a plan carrying Right, Full or RightAnti**: one lane holding every key leaves the others with an empty build side, and those three owe their probe side against one, which is [#175](../tickets.md#t175)'s refusal. Drop the dimension for those plans and pin the refusal with a case, so the drop is a rule a reader meets rather than an abort they rediscover | the emitter carries key *expressions*, not a hash function — ours is fixed |

Lane counts are deliberately absent: `target_partitions` already varies them through the planner,
and a lane count no planner would choose is the one dimension whose correctness rule the prototype
had to encode (`_is_shuffled_join`, `_LaneOrigin`). Buying that rule to vary something the mode
axis already varies is not worth it here.

**Three phases, and the middle one is the point.** Plans first, selection second, execution third,
because 10 queries × 5 modes × the injection crossing is more runs than a CI tier can hold, and
choosing *which* to run is a claim that has to be visible rather than a `take(30)`.

1. **Plan.** For each enabled query, plan at all five modes. Five plans, no injection yet, and a
   plan that fails to build is a failure rather than a skip.
2. **Select.** Derive the candidate set — each plan crossed with the injection settings — and
   choose at most **30 per query**. Representative means the selection covers each dimension at its
   boundaries and each mode at least once, not a sample: the rebatcher in both directions, the
   empty-batch probability at zero and at its high setting, a drained lane where the mode has more
   than one, the degenerate hash where the plan shuffles. The chosen set is **asserted, not
   trusted** — a test over the selector alone, with no queries, that a known candidate set yields
   a cover, and that dropping a dimension from the settings makes it go red. Deterministic and
   seeded: two runs choose the same 30, or a failure is not reproducible.
3. **Run.** Each selected plan through the driver, every answer against the same oracle.

**Render the oracle once per query.** Comparing renders both sides, so an oracle rendered per
variant makes a query's cost grow with the number of variants for no reason — at thirty variants
that is most of the tier. Render once, hold the text, compare every variant against it.

**The oracle does not change and must not.** It is DataFusion on the same SQL, planned and
collected once per query at `target_partitions=1`, compared against every variant. That is what
makes injection meaningful: the answer is fixed by something that never saw the layout, so a
layout that changes the answer is a defect rather than a disagreement between two of our own
shapes. One oracle per query, not per variant — it is the expensive half and it is invariant.

**Eleven queries, and the cost axis is not the obvious one.** Injection multiplies runs, so it goes
on the cheap end of the list — but *cheap* here is the size of the result, not of the scan. A run's
cost is dominated by the oracle comparison, so `anti-join`, a `SELECT *` over 1.2M rows, is the
most expensive member of a set picked by rows scanned. Scanned rows at sf1, which is the proxy the
set was first chosen by and is kept here so the correction is legible:

| Query | rows scanned | what it carries |
|---|---:|---|
| tpch `nested-loop-join` | 40,000 | nested-loop Inner |
| tpch `nested-loop-left-join` | 40,000 | nested-loop Left, single-batch probe |
| tpch `nested-limits` | 220,000 | both row-interval lowerings, cross join |
| tpcds q45 | 899,384 | LeftMark |
| tpch `anti-join` | 1,600,000 | RightAnti |
| tpcds q8 | 3,060,404 | Inner multi-key, LeftSemi with `null_equals_null` |
| tpcds q16 | 3,087,163 | LeftAnti, LeftSemi with a filter, a mid-plan limit |
| tpcds q93 | 3,187,918 | Right outer, multi-key |
| tpcds q97 | 4,361,952 | Full outer |
| tpcds q2 | 4,401,864 | the union that cannot interleave |
| tpcds q33 | 5,281,336 | the four-lane interleave — eleventh on cost, first on merit |

q33 is in on merit, and it happens to cost little: eleventh of the seventeen, six heavier ones
left out. It is the only four-lane interleave in the
list, and an interleave is the one operator whose correctness *is* a lane correspondence — output
lane p from lane p of each branch. Excluding the one shape a perturbed lane could break, to save
0.9M rows over the tenth, would be picking the cheap set over the point of the exercise.

The six left out are the heavy end — q38, q87, `shuffle-stddev`, q20, `left-join`, and q21 at
19.5M rows, three `lineitem` scans in one query. If the budget turns out to allow more, `left-join`
is the next one worth having: a Left outer's finish pass accumulates probe keys, so its residency
is a function of how the batches arrive.

**Shape of the change.** One new test library file carrying the wrappers, the settings, the
candidate derivation, `rebuild` and the selector — `peacockdb-core/tests/common/injection.rs`,
split into a second file only if it passes the 1000-line bar. The **tests** go in
`test_cpu_batch_partitioned.rs`, not beside the mechanism: a `#[test]` under `tests/common/`
compiles into every one of the 22 binaries that declare `mod common`, so it would run 22 times and
be counted 22 times. A target of its own would avoid that and costs a `pipeline.yml` step and a
`test_ci_coverage` entry, which is not worth it for two cases. The eleven
fixtures are declared as one list: `injected_queries!` expands it into both the `INJECTED` const
and the fixtures, so a query leaves the set only by leaving the list, and everything else is
unchanged. One thing in `peacockdb-core/src` changes, and only its visibility: `validate()` becomes `pub`.
The driver validates nothing — `check_canonical_form` is limit positions and no more — so an
injected tree would have run unchecked, and a rewrite that broke a node's requirements would have
answered rather than been refused. That is the failure this task must not have: an injector
quietly generating plans the planner would never emit. It is also what makes the rebatcher's
refusal a demonstration rather than a prediction, since `validate()` is what names the node and
the order it broke.

The cases split by dataset rather than by subject. Four need none — the identity case, the
selector's cover, the rebatcher refusal and the degenerate-hash emit — and go in
`tests/test_batch_partitioned_injection.rs` with its own `pipeline.yml` step and
`test_ci_coverage` entry, so they run in seconds on any host. The two that need a query — the dimension demonstration, which plans
`nested-loop-join` over sf1, and the injected corpus set — stay in the end-to-end file. A mechanism proof buried in a tier that takes minutes is one nobody runs
while iterating.

**Measure before capping, one run at a time.** T17's seventeen queries at five modes are 85 runs in
4m39s at four threads. Eleven queries at up to 30 is 330, on the cheap end of the corpus — a number
to measure, not to assume.

The measuring pass runs **serially**, `--test-threads=1`, and times each run on its own. Four
threads contending for the same host is what the correctness tier wants and the opposite of what a
timing wants: a number taken under contention cannot be compared with another taken under different
contention, so a table built from a parallel run would rank the wrong things. Time each
(query, mode, injection setting) individually and report every one — not a total, since a total
cannot say which row to cut.

The table is the deliverable of that pass and belongs in `llm-wiki/reports/`, since it is a
measurement of a host rather than a fact about the code: one row per query, the per-setting times
across it, its total, and the grand total, with the host and thread count at the top the way the
benchmark records carry `build_profile`. Then the cap is chosen against it.

**What may be trimmed, and what may not.** If it does not fit the tier, cut runs and not cover.
The selection rule already guarantees each dimension at its boundaries and each mode at least once,
so a smaller cap is still a cover — that is what the rule is for. What must survive any trim is one
carrier per injection dimension and the shapes that only one query has: q33's interleave, the two
nested-loop forms, `nested-limits`' two row-interval lowerings. Trimming to the cheapest eleven
minus q33 would be the obvious cut and the wrong one, for the reason q33 is in the list at all.
Where a query is dropped entirely, say which dimension lost a carrier and why the remaining ones
cover it.

~~**T18 — infra for running corpus queries.**~~ (done). T17 proved the path on seventeen queries and T17a
injected layouts into eleven; the corpus is 39 tpch and 99 tpcds, each at five modes on two
engines. What stands between the two is not more test cases but a way to declare one. This task
builds that. It was split out of T20, which carried both this and the new query shapes, once it
was clear the rollout needed the infra before it needed either.

**One declaration per query.** `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes,
cpu_oracle, gpu_oracle)`, where the two mode arguments are a bitwise or over the five planning
modes the plan goldens already carry. A query's whole coverage is then one line that can be read
and diffed, rather than up to ten macro invocations that can disagree with each other. It expands
to one test case per (query, enabled mode) on each engine, so `cargo test <query>` names exactly
the runs that query has — which is what makes the filtered regeneration below possible at all.

The CPU and GPU cases go in **different binaries**, as the legacy pair does: the GPU targets are
staged to shad-gpu by the gpu-tests job, and `test_ci_coverage` verifies that job's array names
them. One macro writing into two binaries is the reason the mode arguments are separate — a query
can be enabled on the CPU at four modes and on a device at one.

**The oracle keywords already exist and are taken unchanged.** `cpu_oracle` is
`data_fusion_exact | data_fusion_approximate | data_fusion_subset`: one oracle, plain DataFusion
at `target_partitions = 1`, asked three ways — the whole answer, the whole answer at a 1e-12
relative tolerance where the sole divergence is float summation reassociation, and the count and
containment alone where the SQL does not determine which rows (see the unordered `LIMIT` below).
The third is new; the first two are legacy's, unchanged. `gpu_oracle` is
`golden_exact | golden_approx | golden_approx_std | live_cpu | skip`: the frozen result compared
exactly, at 1e-12, at 1e-11 where cuDF's variance diverges further than the convention allows,
against a live CPU run where the result is too large to commit, and not at all for a query whose
row order is undetermined. A second vocabulary for the same choice is how two families drift, so
these are the legacy sets or they are a rename of them, never a parallel set — and `live_cpu` is
that rename, applied to legacy's ten call sites in the same commit. Legacy spells it `oracle`,
which inside an argument named `gpu_oracle` says only that an oracle is an oracle, while the
choice it actually makes is between a frozen result and a live one.

There is no seventh argument for whether the run writes `.result.txt`. Legacy carries one
(`result_golden` / `no_result_golden`) because its producer and consumer are declared in
different files, and an orphan golden — written by a CPU case no GPU case reads — is silent
where a missing one is loud. Here both sides are in one declaration, so the predicate is
`gpu_oracle` naming a golden at any enabled mode, and the pairing cannot be stated wrongly.

**Goldens stop being one file per query.** Today each query carries its own `.cpu.txt`,
`.cost.txt` and `.result.txt` per mode, which is how `testdata/goldens/` reached 277 files for
tpch.sf1 and 625 for tpcds.sf1. This mode takes the shape its plan goldens already took: one
`.cpu.txt` and one `.cost.txt` per mode, each holding every query in `== <query>` sections, and
one `.result.txt` across all modes. The comparator is the plan goldens' own `section_differences`
— it names what moved, what is missing and what is out of order, and it has unit tests — so this
is a second caller rather than a second differ.

A query whose bit is clear for a mode still has a section in that mode's files, carrying a marker
that says it was skipped. An absent section and a skipped one are different facts and the file has
to hold both: a query that stopped planning is a regression, a query never enabled at that mode is
a decision, and a format that renders them alike loses the only artifact that could tell them
apart.

**`.result.txt` is one entry per query**, keyed by the query alone. The modes are supposed to
agree on results, so one of them authors it: the last mode the query declares in the fixed
sequence of five — `bp-tp4-sized` for most. Its authority comes from the declaration and not from
what happened to run, which is what keeps it well defined under a filtered regeneration. A run
that does not include the authoritative mode leaves the section untouched; it is the one golden
whose key carries no mode, so it is the one a partial regen could otherwise re-author from a mode
that is not the authority, with the body's own line moving to say so and nobody reading it. The
section still records which mode produced it: the key carries no mode because there is one entry,
and the body names one because where the modes disagree, that disagreement is what the file exists
to make visible. A result at or above `RESULT_GOLDEN_MAX_BYTES` (256 KB) keeps its section and
carries a marker saying so, rather than being deleted — and the cap is reached while rendering
rather than after it. Legacy renders the whole answer to one string and then measures it, so
`anti-join`'s 1.2 million rows are materialized in full to discover they are 240 MB and unwanted.
Here the rows are rendered one at a time against a running total and the whole set is dropped the
moment it passes the cap, so the peak is the cap and one row rather than the answer. Which also
settles the sort: a set that is never going to be written is never sorted — the legacy path deletes it, which reads as
"no golden" and as "golden not applicable" identically, and `build-test.md` states the old rule
and is corrected in the same commit.

**`live_cpu` is what the device uses when no frozen result can serve it, and there are two such
cases rather than one.** The first is legacy's: a result over the cap has no golden to compare
against, so the device is held to a live cpu run instead. The second falls out of this mode and
has no legacy counterpart — `.result.txt` holds one entry per query, authored by one mode, while
the device runs at every mode the query enables. Where the rows are the same at every mode that
is one golden serving five runs, which is the point of the single entry. Where they are not, it
is one golden that only one of them can match: `scan-limit` at `bp-tp1-single` returns different
rows from the `bp-tp4-sized` section, and `golden_exact` would fail on a correct device. So a
query whose `cpu_oracle` is `data_fusion_subset` writes `live_cpu` as its `gpu_oracle`, and is
compared against a cpu run at the *same* mode — where both walk one driver over one plan and the
rows are determined again.

**Written at the call site, never inferred from the argument beside it.** Every argument is
required and says what it means; a `gpu_oracle` deduced from a `cpu_oracle` is a behaviour
selected implicitly by an input, which is the shape `coding-style.md` names as an antipattern and
which this macro exists to avoid — seven arguments so a query's whole coverage reads off one
line. That both conditions are derivable is why a *check* can exist, never why a value would be
absent: a `golden_exact` where the cap or the subset oracle applies is asserted red, since it is a
test that fails on correct behaviour, and a `live_cpu` where neither applies is asserted red too,
since it spends a device-side live run on a comparison a committed file makes faster and harder.

**Every result comparison decides before it materializes, and today none of them do.** Three
places take the same wrong order and the fix is one idea applied three times. Containment collects
the unlimited answer and renders every row; the cap renders the whole result and then measures it;
and `assert_results_match`'s exact arm is `assert_eq!(render(actual), render(expected))`, whose
arguments Rust evaluates eagerly — so both sides are materialized in full to answer a yes-or-no
question, half a gigabyte of `String` for `anti-join`, and the rendering exists for the failure
message rather than for the comparison.

The exact arm becomes a digest: render a row, hash it, drop the string, sort the hashes, hash the
sequence. Memory goes from the answer to eight bytes a row, the verdict is unchanged, and nothing
is rendered on the green path at all. On a mismatch it re-streams and prints a bounded excerpt
around the first differing row — which is not a courtesy but the same lesson `section_differences`
carries, that a dump too large to read is a dump nobody reads, and a 240 MB `assert_eq!` diff is
that failure three orders of magnitude over. The tolerance arm keeps its map, because a digest
cannot express "within 1e-12"; it stops formatting the columns it does not key on.

No verdict changes, so no green test moves.

**A `live_cpu` comparison runs once per mode, beside the device run it checks.** It cannot reuse
another mode's answer: where the SQL does not fix the row set, a cpu run at `bp-tp4-sized` is no
more an authority on what the device returns at `bp-tp1-single` than the frozen section was, which
is the reason this value exists at all. Same rule for the over-cap case even though its result is
mode-invariant — the test is already per (query, mode), so same-mode costs nothing extra and one
rule beats two that differ by a condition a reader has to check.

What it costs is on the device leg, which is the leg that runs `--test-threads=1` on one host: a
`live_cpu` query executes twice per mode there, once on the device and once on the cpu backend
through the same driver. Legacy carries ten such queries and this mode adds `scan-limit`, so at
five modes it is on the order of fifty extra cpu runs on the gpu host. Named here rather than
discovered in a job duration, and it is part of the cost the corpus tier accepts rather than a
separate decision.

This is not the `result_golden` case one paragraph up. That argument is omitted because it has no
degrees of freedom left once the rest of the line is written — legacy needs the keyword only
because its producer and consumer live in different files. `gpu_oracle` has five values, and the
constraint pins it in two cases out of five.

A section can therefore turn from content into a marker, and back, as a result crosses the
threshold. Nothing guards that transition and nothing should: at a fixed scale factor a result's
size moves only when its answer moves, which is the thing every other check in the tier is
already watching. The marker is there so the file says why a result is absent, not to absorb a
size that oscillates.

**A run that stopped early says so, on its own line.** A satisfied limit ends the run with work
undone: lanes that were never pulled, row groups never read, batches produced and never consumed.
Every one of those shows up in the annotations as a smaller number, and a smaller number with no
stated cause is indistinguishable from a plan that produced less — which is the absent-versus-
skipped confusion this tier exists to prevent, one level down. So each section opens with
`early_exit=<node>@<ordinal>` naming the limit that was satisfied, or `early_exit=none`. Always
present, never inferred from absence: a query that ran to completion is a fact the file states.
It is stable, because [the determinism rules](#determinism-rules) pin the schedule — which
batches were consumed before the limit was met is fixed for a plan.

Beside it, `rows_skipped=N` on each node that has any: rows released without an unload call,
which the driver already counts per node precisely because the rows returned look identical
either way. It is the saving a limit buys, and the golden is where it becomes visible.

**A batch emitted and never consumed is still emitted.** It is counted where it enters a queue,
so it is in its producer's `batch_rows` and `batch_bytes` whether or not the parent ever took it;
what the parent took is its `in_rows`. The gap between the two is the work a limit threw away,
per child and per lane, and it is the quantity `early_exit` exists to explain — without the
marker it reads as a node that emitted more than its parent wanted, which is a defect
everywhere else.

The same rule decides the cost, and the answer is not the intuitive one: `.cost.txt` prices what
was produced, not what was used, because the device did that work and holding those bytes is what
the budget was spent on. A query that exits early is genuinely cheaper than one that does not,
and the batches it produced before exiting are genuinely not free.

Where the drop begins is bounded rather than recorded, and does not need a third list. Queues are
FIFO and arrival order is pinned, so what a parent took is a prefix of the child's lane — the
cumulative sum of `batch_rows[j]` reaching `in_rows`, ending mid-batch only where a limit sliced a
straddling one. So `in_rows[child][j]` lies between two adjacent prefix sums of that child's lane,
which is a tighter check than `<=` on the total and is the form to assert.

**Two checks change shape under it, and both get stronger rather than weaker.** The loader
identity — `batch_rows` having `partition_groups`' exact shape — is a plan-against-run
comparison, and early exit is exactly when the run does less than the plan. Per lane it becomes a
prefix, and per lane rather than over the flattened shape: `batch_rows[j]` is a prefix of
`partition_groups[j]` — its own lane's list, aligned from the start because a scan reads its
groups in order — with an empty lane allowed, and every lane equal on a run whose marker says
`none`. The scheduler stops where it is, so one run can have lane 0 complete, lane 1 short and
lane 2 empty. Compared flattened, that run either goes red or forces the check down to a total
length, which no longer says a batch lines up with the row groups that made it — the whole reason
the nesting is there.
The `in_rows` identity needs no marker at all, because `abandoned` is rendered beside it. That was
decided against a count rather than a hunch: keying the arithmetic on the marker would soften it
to `<=` at every node of an early-exiting section, and 84 of 99 tpcds queries carry a `LIMIT` — of
the 67 with a committed result golden, 35 return 100 rows or more, so the limit really fires. Half
the bench, not a handful. The marker is also not precise enough to carry arithmetic: it is
`any_satisfied()`, so a query returning exactly 100 rows sets it while consuming everything, and
the weakening would apply where nothing was skipped.

So the conservation law does the work — `consumed + abandoned == emitted`, one statement over the
file and over the report, rather than a law in the driver's tests and a weakening of it in the
golden. Same family as `in_flight_bytes` returning to zero and `holds == releases`, which the
report already carries: a batch that vanishes without being either consumed or abandoned is a
defect no inequality can see. Deciding it now costs one optional field in a format gaining five;
after T19 it costs regenerating eleven files per bench across a hundred queries.

The marker keeps the two jobs it is good at: telling a reader the run stopped early, and keying
the loader's prefix, which `abandoned` cannot close — a row group never read produced no batch to
abandon.

**Several test cases now write one file, and that is the task's one real hazard.** A whole-file
write is last-writer-wins, which would drop every other query's section and leave a green run. So
a regenerating write takes an advisory lock on the file, merges its section, and publishes by
writing a sibling and renaming onto the name — the rename because a crash mid-write must not
leave a truncated golden, which `point_canonical_root` already does for the same reason.

**The lock is on the file, not in the process, and the distinction is load-bearing.** Libtest runs
a binary's cases as threads in one process, so a `Mutex` would serialize them — but that is a
guarantee about one binary, and what makes it sufficient here is the separate fact that only the
CPU binary writes. An invariant, not a property of the language: the next writing binary breaks it
with nothing red, `cargo nextest` runs each case in its own process and would silently reduce a
mutex to no lock at all, and two shells regenerating at once are outside any of it. The
`canonical_root` comment records the same surprise one level down — two binaries reaching one path
at the same time, fixed with an atomic rename rather than a lock, because a lock in one process
could not see the other. `std::fs::File::lock` is stable on the toolchain in use, so this costs no
dependency.

**Partial regeneration follows from that.** A filtered run regenerates only the sections its cases
produced and leaves every other section as it is, under an environment variable of its own rather
than by widening `UPDATE_CANONICAL`, whose contract is a whole file. `PCK_TEST_FILTER` already
scopes which cases run, so the mechanism has a caller before it has a second one. What must
survive is the distinction above: a filtered regen that cannot tell "did not run" from "stopped
planning" deletes coverage silently, which is exactly what a golden exists to prevent.

**`.cost.txt` stays a pure function of its `.cpu.txt` and `cost_model.conf`**, so the derivation
is per section rather than per file, and `test_cost_model` re-derives every one.

**The GPU side reads what the CPU side wrote**, as in legacy: per-node plan shape and the
input/output statistics against that mode's `.cpu.txt` always, and `.result.txt` where the
`gpu_oracle` names a golden. It never writes either, and ignores the regeneration variable
rather than honouring it — a device that can author its own golden proves nothing against it.

One thing is easier here than in legacy, and worth not spending twice. Legacy's two engines run
separate executors, so the two rendering the same tree is a coincidence the golden exists to
check; here `batch_partitioned_driver` is generic over the backend, so both engines walk one
driver and report through one `RunReport`. The rendering is therefore written once, and what the
golden still checks is the answer the device gave, not the shape of the report it came in.

**Registry.** The five `bp_*` columns exist and mean plan enablement, declared by the golden's
section rather than by a macro — `test_batch_partitioned_plans` holds the two to each other in
both directions. Execution needs its own columns, five per engine, and those are macro-declared
through the existing link-time inventory, so the widget's three groups each have a source that
something checks. Adding ten columns to a seventeen-column csv is the point to decide whether the
row stays flat or the modes become a repeated group; the inventory tests are what must keep
working either way.

**Widget.** A batch-partitioned table per bench, repeating the legacy structure — peacockdb cost,
duckdb cost, ratio, features, tickets — with the four mode columns replaced by three: planning,
cpu execution, gpu execution. Each holds one cell per mode. A planning cell links to that query's
section in `<mode>.plans.txt`, which is also where its refusal is, so an enabled query and a
refused one link to the same file and differ only in what the reader lands on. A cpu cell links to
the query's section in that mode's `.cpu.txt`. Where not all modes are enabled, the cost columns
use the last mode in the sequence for which cpu execution is enabled, as the legacy rule does for
its own last mode.

**The cost-regression gate is a third rendering, and it degrades silently unless this task
changes it.** `--cost-diff` compares each `.cost.txt` against the same path at the PR's base and
upserts its own PR comment: improvements green, regressions red, and a non-zero exit that fails
the build. It is where a cost win becomes visible, so the new mode belongs in it. But it is
written for one file per query — `collect_cost_goldens` globs `*.cost.txt`, `read_total` takes
the *first* `peacockdb_cost=` line in a file, and `diff_label` takes the filename's first
dot-segment as the query name. Point that at `bp-tp4-sized-mini.cost.txt` and it picks the file
up, reads whichever query's section happens to be first, and labels the number with the mode: a
wrong row, rendered confidently, gating the build. The glob is what keeps it quiet — the files
are found, so nothing reports them missing.

So the differ becomes section-aware in the same way the comparator did: a row per (query, mode),
totals read per section, the label carrying both. `cost_diff` itself is unchanged, since it
already works over a map of label to number; what changes is what fills the map. Its unit tests
are the pattern to extend, and the red case is a two-section file whose second section moved —
the per-file reader reports no change, the section reader reports one.

**There are two widgets, and both get the table.** `cost-report` renders the same data twice —
`--md`, a markdown blob upserted as one PR comment keyed on a sentinel, and `--html`, the site
published to Pages from master. Close to identical rather than identical, and the differences
are the ones a format forces: markdown cannot set a row background, so a row over the ratio
threshold is flagged in the cell instead, and the mode cells have a markdown counterpart to
`mode_cells_html` rather than sharing it. A table added to one and not the other is the failure
to expect here, because the html is what a person looks at and the markdown is what the review
actually reads.

**No production change that alters what the engine computes, here or in T19.** A query that
does not run is disabled with a ticket, never fixed in passing: a fix made while enabling one
query is a fix nothing else in the branch proves, and the diff under review stops being the
infra. What this task does touch in `peacockdb-core/src` is the goldens' own surface and nothing
else — the node renderer per [Node display](#node-display) (`name@ordinal` references, layout in
place of the lane count, `fetch` and the aggregate lists wherever carried, schema in the plan
golden only), the per-batch line below, `cost_model.conf` entries for all eighteen node kinds at
once rather than as each is first seen — held there by a case red on a nineteenth, derived from
the exhaustive `as_node_ref` the way the writer's field cover is, since entered-at-once is a
moment and T19's "no `cost_model.conf`" rule needs a property — and
per-node emitted rows and bytes on `RunReport` — the one thing the engine does not already
report. Everything else the
comparison needs is there: `Batch` gives rows and bytes on both backends, the device's coming
through the frozen ABI's `PeacockNodeStats` priced against the declared schema, and
`trace`'s per-event `outputs` sums to `out_batches` per lane. What is absent is a per-node total
of what each node emitted; the driver reads both numbers already, for the accountant and the
limit interval, and keeps only aggregates. `rows_seen` is not the emitted total and must not be
reused as one — it counts rows arriving at a node for the limit rule, which is `in_rows` summed
over lanes rather than anything a node produced. Plus the
`pipeline.yml` steps `test_ci_coverage` requires, which are not engine code at all. Three more landed than that list, each approved on its own and each recorded here rather than
only in the exchange that approved it: `CpuBatch::byte_size` pricing from the plan's schema, which
is the one the headline forbids by name and which moved every budget decision in the engine; the
`gpu_backend` guard closing [#181](../tasks/active-tickets.md#t181); and `driver/mod.rs`'s `mod index` going
`pub(crate)` so the renderer walks the driver's own index rather than a second pre-order.

Naming that surface is what gives the rule an edge, and the edge only holds against a list that
is true: T19 has none of these seven left to touch, and T20 is where the engine moves again.

**Every node carries what it consumed and the size of every batch it emitted**, as parallel
structures on one continuation line under the node:

    GpuLoadParquet: table=lineitem, partition_groups=[[[0,1],[2,3]],[[4],[5,6,7]]], lanes=2, …
      in_rows=[] batch_rows=[[1500304,1500303],[1500304,1500304]] batch_bytes=[[31881456,31881440],[31881472,31881455]]

Lanes outermost and batches within, which is `partition_groups`' own nesting — so on a loader,
element `i` of lane `j` in all three lines is one batch, and the row groups that produced it sit
at the same index as its size.

`in_rows` is what the node consumed, per lane, nested by child rather than flat — a filter reads
`in_rows=[[860160,737280,…]]` and a join reads both of its sides. No space after a comma, which
is `partition_groups`' committed convention — the two nestings are read index for index and the
tail case is a 96-batch lane. Legacy prints one `in_rows`
per partition and it is the first child's, so a join's build side appears nowhere on the join's
own line; here the two sides differ in kind and the capability matrix turns on which is which,
so a format that can only show one of them is one that hides the interesting half. A source has
no children and prints `[]`, which is not the same as a child that consumed nothing. It is the
one figure the batch lists cannot imply, and the reason legacy's sub-line is dropped rather than
merely thinned: rows in against rows out, on the node's own line, is what makes a selectivity or
a skew visible without walking the tree.

Rows and bytes are separate lists rather than pairs because each is then a column a reader scans
down and a diff reports as one moved number; the cost is that a batch's two figures are not
adjacent, which the shared indexing is what makes navigable. All on one line, so a node stays one
entry however many batches it has.

This is where the per-batch record lives, rather than in a `batch-info.cpu.txt` for a chosen ten:
a separate file for a subset was worth it only while the figures were too bulky for every node,
and at a median of one batch and a p90 of six they are not. The tail is what to watch — the
worst source in the corpus is tpcds `inventory` at `bp-tp1-rowgroup` with 96 batches in one
lane, about two kilobytes across the two lists — so the comparator reports a moved section by
name and never dumps the line.

**A failing query is disabled with a ticket in [`active-tickets.md`](../tasks/active-tickets.md)**, not in the
main list. Rollout tickets arrive in bulk when a sweep hits a wall and close in bulk when it is
cleared, which is not what a triage pass reads for. The ID space is shared, so a number is never
two things. `TicketIndex::load` reads two files and gains a third here, and it is
not cosmetic: `cost-report` already exits 1 when a ticket the registry names resolves in neither
file, so the first rollout ticket filed in `active-tickets.md` fails the cost-report job until the
index reads it.

**What proves the infra, since every way it fails is quiet.** A dropped section, a filtered
regeneration that deletes coverage, a skipped marker a reader cannot tell from an absent one — none
of these turn a run red on their own, so each gets a case that goes red on demand, built from
strings rather than by editing a golden and undoing it:

- two writers interleaved at the merge keep both sections, and the same case with the lock
  removed loses one — a lock nothing can be shown to need is a lock the next refactor drops.
  Forced rather than raced: two threads that merely start together prove nothing on a fast
  machine and fail on a loaded one, so the overlap is imposed — a barrier between read and write,
  or the merge driven directly with a second writer's section already on disk. Every case here
  is deterministic or it is not a case: this tier's whole claim is that a golden means something,
  and a test that passes on the third run is a golden that means nothing;
- a filtered regeneration rewrites its own sections and leaves every other byte identical;
- a section absent because its case did not run and one absent because the query stopped planning
  are distinguished, and the run that confuses them fails;
- a skipped marker round-trips: a query whose bit is clear writes one, and clearing a bit that was
  set turns a real section into a marker rather than deleting it;
- both of `cost-report`'s renderings carry the new table, asserted per rendering rather than once;
- the section reader and the per-file reader disagree on a two-section file whose second section
  moved, which is the cost gate's red case.

**Three things have to agree, and the inventory test becomes the place they do.** Legacy holds
the macro invocations and `cost-registry.csv` to each other in both directions, per binary,
because `inventory` collects per linked binary. The corpus tier adds a third leg: the goldens.
Every `corpus_query!` registration means an enabled cell, every enabled cell means a
registration, and every enabled (query, mode) means a golden section carrying content rather than
a skipped marker — with the converse holding too, since a `disabled` cell whose section is full
is coverage nobody is reading. The plan columns stay declared by the goldens alone, as they are
today; what is new is the execution columns being declared by the macro and checked against both.
One consequence to state rather than discover: the gpu half of this runs only on the gpu host,
because that is where its registrations are linked, so a gpu-column drift does not go red on the
cpu leg.

**Soundness is checked against the file's own redundancy, not with regular expressions.** A
regexp says a line looks like a line. What is wanted is whether the numbers mean anything, and
the format carries the same quantity more than once by design, so the checks are arithmetic and
cheap:

- a node's `out_rows` is the sum of its `batch_rows` lanes, and `out_bytes` of its `batch_bytes`.
  A batch is counted where it enters a queue, so a scatter output dropped for being empty is not
  one — the sums are unaffected either way, and the batch count then means what flowed;
- the lane count of those lists is the `lanes=N` on the node line;
- a node's `in_rows` for child *k* equals that child's `out_rows`, **per lane and not only in
  total**: the index is the *child's* lane, so it lines up with that child's own `batch_rows`
  entry for entry. Indexing by the consuming node's lanes would make the identity checkable only
  in aggregate wherever the two counts differ, which is every emitter, merge and accumulator —
  the nodes it is most worth checking. An equality on every run, because the node renders
  `abandoned` after its own `batch_bytes` — rows `release_in_flight` dropped, per lane, omitted
  where zero and so absent from every node of every run that drained. It sits on the node that
  *emitted*, because that is whose output queue was dropped, so the law crosses parent to child
  exactly as `in_rows` already does. The law is
  `consumed + abandoned == the child's emitted`, with no marker in it and no `<=` anywhere. The
  two exceptions this once named were measured and are both wrong: dropping a probe batch is
  consuming it, and a satisfied limit falls short at whichever node the schedule stopped at rather
  than at the limit;
- on a loader, `batch_rows` has `partition_groups`' exact shape, which is the correspondence that
  nesting was chosen for;
- the root's `out_rows` is the row count in `.result.txt`, which the oracle checked.

That last one is the interesting one, and the rest are worth more than they look. The cost tree
has no external oracle — the golden is written by the run it will later check — but a file that
contradicts itself is a file a renderer got wrong, and a renderer is most of what could be wrong.
It is a weak oracle rather than none, and it is free: the redundancy is already committed.

**Reading it back needs one parser, and there are already two and a half.** What exists:
`ordered_sections` and `section_differences`, which split on `== <query>` and report what moved,
private to `test_batch_partitioned_plans.rs`; `parse_node_line` in `common/cost_model.rs`, which
finds `output_bytes=` and takes the leading identifier, private, and enough for the cost
derivation and nothing else; and `read_total_str` in `cost-report`, another crate reading a
`key=` off a line. None of them knows a tree, a lane, or a nested list, which is what the
arithmetic above asks for.

So: the comparator moves into `tests/common/`, where both corpus binaries can reach it — and that
move is owed anyway, since `test_batch_partitioned_plans.rs` is at 1418 lines against a
thousand-line cap. `parse_node_line` grows from `(type, bytes)` into the node's fields and its
indent depth, and `cost_text_from_cpu` becomes a caller, so the cost derivation and the soundness
checks read one structure rather than two that agree by luck. `cost-report` stays separate, since
it reads a total out of `.cost.txt` and never a node line; what must not drift between the crates
is the `== <query>` convention itself, which is why it is written here rather than inferred from
whichever file a reader opens first.

A fourth parser is the thing to refuse. Three readers of one format already disagree about what a
node line is, and the format is about to carry four more fields.

**Consolidating them saves no code, and that is not the reason to do it.** All the parsing in the
tree is about ninety lines: `ordered_sections` is fifteen, `parse_node_line` seventeen,
`cost-report`'s `field` and `read_total_str` thirteen between them, and the rest is
`section_differences` reporting. A parser that knows a tree, a lane and a nested list is larger
than the three it replaces, so the change adds lines. What it buys is one definition of the
format at the moment the format gains four fields — the divergence is the cost, never the
duplication.

**The crate boundary is where consolidation stops, on purpose.** `cost-report`'s `[dependencies]`
is empty, and deliberately: it builds in seconds in the cpu tier because it pulls in neither the
executor nor a device. Making it depend on `peacockdb-core` to share thirteen lines of `key=`
reading trades a stated property for nothing, and a new workspace crate for the same thirteen
lines is worse. It needs the convention, not the code — `== <query>` sections, one
`peacockdb_cost=` per section.

So pin the convention with a fixture rather than with shared code: one committed two-section
sample that `cost-report`'s unit tests and the test-side parser's unit tests both read, each
asserting the same extracted values. The two readers then diverge red without either crate
depending on the other, which is the property that was actually wanted.

**An unordered `LIMIT` runs at every mode, and drops the DataFusion oracle rather than the
mode.** Legacy canonizes `scan_limit` at tp1 because at tp>1 its rows and its per-node bytes vary
run to run. That is a property of legacy's executor, not of this one:
[the determinism rules](#determinism-rules) pin the schedule and require that one plan run twice
gives one answer byte for byte, so a section here is stable at every mode. What those rules
explicitly do not promise is agreement *across* plans — tp1 and tp4 may return different rows
where the SQL does not determine which, which is what an unordered `LIMIT` is.

So the mode is fine and the comparison is what narrows: DataFusion single-stream is not an
authority on *which* ten rows a four-lane plan returns. It is still an authority on the rest, and
an unordered limit determines more than it looks. The count is `max(0, min(n, |unlimited| - m))`
for `LIMIT n OFFSET m` — written with the offset from the start, because the zero-skip form is a
rule that goes wrong on the first query that carries one and T20 adds those deliberately. And the
rows are a sub-*multiset* of the unlimited result, compared as a multiset and not as a set: set
membership passes a run that returned one row twice where the oracle has it once, which is a
live failure mode for a limit over a join. Both are asked of the session that
already runs, neither needs the two plans to agree on which rows, and together they catch what a
frozen golden cannot: a limit dropped, an offset ignored, rows invented, the wrong table sliced.

That is the third `cpu_oracle` value, and it is named for what it checks rather than for what it
declines to — the count and the containment, against `data_fusion_exact`'s whole answer. So no
query's answer is frozen with no external check, `scan-limit` included. The device is held to the
cpu's section on top of that, which is a real check rather than a fallback: both engines walk one
driver over one plan with emission order pinned, so disagreeing means one of them broke the
schedule.

Two limits to state with it. An *ordered* `LIMIT` with ties at the boundary is not covered:
neither `sort_unstable_by` nor cuDF's `sorted_order` is stable, so which tied row survives is
decided by neither engine's contract and cpu-gpu agreement is not owed. And the scope is small —
the determinism section counts one corpus query with a bare `LIMIT` whose rows are undetermined,
`tpch-queries/scan-limit.sql`; the other four limit an already-single-row aggregate.

**A query carries every mode it is correct at, and a ticket for the ones it is not.** Not a
default to be tuned: all five where all five run and agree with the oracle, and where some do
not, the ones that do are enabled and the rest are `disabled` against a ticket naming which modes
and why. That is what makes the mode arguments a set rather than a switch, and it is the same
rule on both engines — a query can be correct at five modes on the cpu and at two on a device,
which is two tickets and not one.

**The cost of running all of it is accepted for now.** 119 queries at five modes is about 595 cpu
runs and as many on a device, against a corpus measurement nobody has: T17a's report covers
eleven queries chosen for being the cheapest. Measured serially, one row per query: 45.9 s for 87
cpu runs on one thread, 26.2 s on the two CI uses, and a 1.34 GB peak of which one query is
1.28 GB. It gates nothing. The one thing that would force this open
again is a job that stops finishing: pipeline.yml's dataset-matrix leg has already been lost once at
fifty-eight minutes on a tier a fraction of this size, and the device leg runs
`--test-threads=1` on one host.

**A first enablement freezes whatever the run produced, and only half of it has an oracle.** The
answer does: `assert_cpu_results_match_datafusion` builds a separate plain-DataFusion session at
`target_partitions = 1` and compares against it unconditionally, before and regardless of any
golden write, so a wrong answer cannot be frozen — and that comparison is the standing check on
every later run too, not a first-enablement rite. The `.cpu.txt` does not: under a regeneration
it is written with no comparison, and nothing independent says the plan should have that shape or
that an interior node emitted that many rows. The root is anchored — its `out_rows` is the
result's row count, which the oracle checked — and every node below it is taken on trust.

What later covers some of it is the device asserting against the same file read-only, and it is
worth being exact about which half of that assertion carries information. The tree carries none:
`batch_partitioned_driver` is generic over the backend, so both engines walk one driver over one
plan and produce the same shape by construction rather than by agreeing. That is the design and
not a weakness of it — the walk being identical is what makes the two engines comparable at all.
The evidence is the rows and the bytes: the same walk, and the device's numbers against the
CPU's. The tree assertion stays anyway, because it costs nothing and goes red on the day that
construction stops holding, which is the only day it could ever say anything.

So enabling a hundred queries is an ordinary act for the answers, and for the interior of the
cost tree it rests on two engines producing the same counts through one walk. The mitigation is
the one T19 already has: batches of five, where a tree that looks wrong is still attributable.

**`build-test.md` gains rows, not only counts**: one per new tier in the category table, and
three in the golden table — the per-mode `.cpu.txt` and `.cost.txt` and the one `.result.txt` —
each naming the partial-regeneration variable beside `UPDATE_CANONICAL`. That table's existing
`.result.txt` row says the golden is deleted above 256 KB, which is the rule this task replaces,
so the row is split rather than edited: legacy keeps its sentence and the new mode states its own.

**And enough queries to prove the infra**: about ten tpch and about ten tpcds at sf1, enabled on
the macro as it is built — **on both engines**, at every one of the five modes each is correct at,
under the same rule and with the same tickets. The device half is not deferred to the rollout. An
infra whose gpu path has never run is an infra T19 would debug at the same time as its first
query, which is the thing staging this task exists to prevent, and the `gpu_oracle` argument,
`live_cpu`, the read-only assert against the cpu's section and the staging array are all surface
that means nothing until a device has walked it. They are chosen by plan size off the committed goldens rather than by
taste, because the constraint is not what can plan — all 22 tpch benchmark queries already plan
and attach recipes at all five modes, as do 81 of the 99 tpcds — but what is small enough that a
failure is legible. The tpch ten are the smallest carrying no ticket: q6, q1, q14, q19, q12, q13,
q15, q17, q3, q10. The tpcds ten are q41, q42, q3, q43, q52, q55, q96, q15, q37, q82. A legacy
ticket on a row is not automatically a blocker here — [#97](../tickets.md#t97) is the real-8-way
join blocker and this is a different executor — so those rows are held back as unknown rather
than as known bad, and T19 is where that is settled query by query.

| bench | enabled here |
|---|---|
| tpch | `q6` `q14` `q19` `q12` `q13` `q15` `q3` `q10` |
| tpcds | `q41` `q42` `q3` `q43` `q52` `q55` `q96` (tp1 only) `q15` `q37` `q82` |

Seventeen queries at 88 (query, mode) cases, not twenty: enabling them is what found that three do
not run. `tpch/q1` and `tpch/q17` are out at every mode on [#163](../tickets.md#t163) — `avg`
declares its count state `UInt64` and the accumulator produces `Int64`, which the ticket records
from the device and which the cpu reaches too. `tpcds/q96` keeps both tp1 modes and loses the
three tp4 ones to [#180](../tasks/active-tickets.md#t180), where a shuffle puts a state merge under a
`count(*)` declared non-nullable.

The set is not topped back up to twenty. It was chosen as the smallest queries carrying no ticket
and two of them turned out to carry one, which is a fact about the corpus rather than a hole to
fill — and q96 partially enabled is worth more than a replacement would be, since it is the only
query here exercising the mode-scoped disablement the whole `cpu_modes`/`gpu_modes` shape exists
for.

~~**T19 — rollout.**~~ (done). Query-by-query enablement across the rest of the corpus on T18's macro,
starting from the twenty it already carries. No production code changes, per T18: a query that
does not run is disabled with a ticket in [`active-tickets.md`](../tasks/active-tickets.md), which is the whole
output of this task besides the enabled rows. Not one line of `peacockdb-core/src` moves here:
T18 left the goldens' own surface finished, so anything this task would have to change is by
definition a query's blocker rather than the rollout's.

**Four paths are edited by hand, one kind of file moves without being written, and a commit
touching anything else has stopped being a rollout.** By hand:

- the shared `corpus_query!` list, one file that every binary includes and reads through its own
  arm, as `gpu_cases.inc` is read today — so a query is enabled once, not once per engine;
- `testdata/cost-registry.csv`, cells rather than rows: all 138 corpus queries already have one;
- [`active-tickets.md`](../tasks/active-tickets.md), for what a query is disabled on — a whole query, or the
  subset of modes it is wrong at, which is the commoner case and the one whose ticket has to name
  the modes or the next reader re-derives them.

Every cell this task turns off carries a ticket, which is an invariant the registry already
holds: no row in it today has a `disabled` cell and an empty ticket list, and the four queries
DataFusion cannot plan at all carry [#23](../tickets.md#t23) against an `na`. A rollout is the
one thing that could break it, since it is the only task that turns cells off in bulk.

**The invariant proves less than it sounds like**, and the gap is worth knowing before leaning on it:
the tickets column is a per-row bag with no mapping to cells in either direction. A row already
carrying a legacy ticket satisfies "no disabled cell with an empty list" the moment a bp cell goes
off, with nobody having filed anything. Checked by hand at the rollout's close and the substance
holds — all 33 numbers resolve, and each of the nine rows disabling a strict subset of cpu modes has
a ticket naming those modes — but on six of the nine that ticket does not name the query, so a reader
of `tpcds/q5` opens four tickets to find which one applies.

**Two fixes ride along, both because the rollout is what breaks them.** The second is
[#194](../tickets.md#t194): `base_total` (`cost-report/src/main.rs:1623`) reads a base-side cost two
ways, and the git-ref arm — the one every PR uses — drops the `section` it was asked for and returns
the first `peacockdb_cost=` in the file. Legacy goldens are one file per query, so the two arms agree
there; the batch-partitioned per-mode files hold ~60 sections each, and this task is what fills them.
So every bp row on the cost widget is baselined against whichever query sorts first — measured on
`tpcds.sf1/bp-tp1-single-mini.cost.txt`, deltas from -99.6% to +341.3% against `q2`, none of them a
cost change. The git arm takes the same `entry_total(text, section)` path as the directory arm, and
the test is that the two arms agree on a multi-section file. The goldens are untouched: only the
baseline was wrong.

**The one test this task adds** asserts that every result section's
`mode=` is its query's last declared cpu mode, and goes in `test_corpus_goldens.rs`. Closing its
over-cap blind spot takes both halves. `over_cap`
(`common/corpus.rs`) writes the `skipped: ` prefix and no `mode=`, so an over-cap section records
everything except who produced it — and an over-cap section whose authority moved after a cut is
exactly the stale case this guard exists to catch. Four sections are over cap today, all in
`tpch.sf1/bp-mini.result.txt`: `q16` authored at `bp-tp1-rowgroup`, and `anti-join`,
`filter-project` and `semi-join` at `bp-tp4-sized`. They rewrite when their authoring mode runs, so
the guard's commit carries a four-section golden diff that the ticket has to explain.

`over_cap` gains the mode, **as its second line, after the marker**. The prefix stays first because
two readers use `starts_with(SKIPPED)` to mean *this section holds no rows*:
`corpus_gpu.rs:120` decides `frozen` by it, and moving the mode ahead of it would let a
`golden_exact` declaration pass against a section with no rows to compare. `corpus_gpu.rs:164` reads
`mode=` as the first line, but only for a `golden_*` oracle, which `assert_oracle_suits_the_golden`
has already refused for a marker — so an over-cap body never reaches it.

The guard therefore discriminates on **a `mode=` line being present**, not on `SKIPPED` being absent:
that set is the real sections and the over-cap ones, and it excludes `not enabled` markers, which
carry no author because none exists. It reads `ordered_sections` directly rather than
`sections_with_content`, whose prefix filter is what hid the over-cap case — and whose three
existing callers are undisturbed, since none of them wants the over-cap sections either.

Then the **golden sections** fill in: a query enabled at a mode stops carrying that mode's
skipped marker and starts carrying its plan, its per-batch sizes and its costs. That is eleven
files for the bench it belongs to — a `.cpu.txt` and a `.cost.txt` per mode, plus the one
`.result.txt` across modes — and twenty-two for a commit spanning both. The number is the point:
it is modes × benches and does not grow with the corpus, so the hundredth query enabled moves
the same eleven files as the first, where legacy would have added three more per (query, label)
and did, 277 of them for tpch.sf1 alone. And **`build-test.md`'s counts** move with the case
count — edited by hand like the three above, in the commit that moves them rather than in a later
sweep, which is why they are the fourth and not a second kind of thing that moves by itself.

**What it enables, and what stays out.** 120 of the corpus's 138 queries plan at all five modes
today, which is the eligibility test — not legacy enablement, since a query legacy disabled may
have been fixed since and the goldens are what know. Twenty are T18's, so a hundred are here:

| bench | enabled here |
|---|---|
| tpch (29) | `aggregate-groupby` `anti-join` `cross-join` `filter-project` `hash-join` `join-int` `left-join` `mixed-join` `nested-limits` `nested-loop-join` `nested-loop-left-join` `q11` `q16` `q18` `q2` `q20` `q21` `q22` `q4` `q5` `q7` `q8` `q9` `rollup-over-join` `scan-limit` `semi-join` `shuffle-additive` `shuffle-additive-avg` `shuffle-stddev` |
| tpcds (71) | `q1` `q10` `q11` `q13` `q14` `q16` `q17` `q18` `q19` `q2` `q21` `q22` `q23` `q24` `q25` `q26` `q29` `q30` `q31` `q32` `q33` `q34` `q35` `q38` `q39` `q4` `q40` `q45` `q46` `q48` `q5` `q50` `q54` `q56` `q58` `q59` `q6` `q60` `q61` `q62` `q64` `q65` `q66` `q68` `q69` `q7` `q71` `q73` `q74` `q75` `q76` `q77` `q78` `q79` `q8` `q80` `q81` `q83` `q84` `q85` `q87` `q88` `q9` `q90` `q91` `q92` `q93` `q94` `q95` `q97` `q99` |

`tpch/mixed-join` is CPU-only and the one query whose two engines differ: it plans, validates and
runs, and its recipes do not attach — [#168](../tickets.md#t168)'s interval `ScalarValue`, so the
crossing to a device is what fails, not the plan. `cpu_modes` carries all five and `gpu_modes`
none, which is the case the two mode arguments exist for.

The eighteen that stay out, every one against a ticket that already exists:

| held back | why | ticket |
|---|---|---|
| tpcds `q12` `q20` `q36` `q44` `q47` `q49` `q51` `q53` `q57` `q63` `q67` `q89` `q98` | the planner refuses `WindowAggExec` and `BoundedWindowAggExec` — the one capability the retired modes had and this one does not | [#143](../tickets.md#t143) |
| tpcds `q27` `q70` `q72` `q86` | DataFusion 45 does not physical-plan them at all (`plan_status=fail`) | [#23](../tickets.md#t23) |
| tpcds `q28` | a `DISTINCT` inside `count(DISTINCT …)`, refused by name | [#62](../tickets.md#t62) |

Window functions stay disabled and are not this task's to fix — [#143](../tickets.md#t143)
carries them.

**Seventeen of these hundred are already run by T17's tier, and the overlap is deliberate.**
`test_cpu_batch_partitioned` runs six queries at the five modes and eleven more at the modes plus
the injected shapes; every one of the seventeen falls in this task's list and none in T18's —
tpch `left-join` `q20` `q21` `shuffle-stddev` `anti-join` `nested-limits` `nested-loop-join`
`nested-loop-left-join`, tpcds `q38` `q87` `q2` `q8` `q16` `q33` `q45` `q93` `q97`. They are not
excluded, and a reader who finds the duplication should leave it.

The two tiers ask different questions of the same query: the injected one asks whether the
drivers tolerate a layout no planner would emit, over one plan; the corpus one asks whether the
answer, the plan and the costs match a golden, across the whole bench. Neither answers for the
other. And a corpus list defined by subtracting another list is a set held in prose — which is
exactly the defect T17a's completeness pass found in its own fixture list and closed by
generating `INJECTED` from one declaration. Reintroducing it one level up, so that removing a
query from the injected tier would silently need a matching addition here, buys a duplicate mode
run and costs the property that made the smaller list trustworthy.

**A device cell's ticket names its FIRST failure, not its only one.** The causes are ordered by how
far a plan gets: [#183](../tasks/active-tickets.md#t183) is the unload refusing an export, at the end of a plan
that ran, and [#152](../tickets.md#t152) is a join refusing its second probe batch, earlier. So a
mode whose join takes one probe batch reaches the unload and fails on the string, while a mode whose
join takes two never gets there — the same query reporting two different causes, chosen by the
partitioning rather than by its shape. `q18` is #152 at four modes and #183 at one.

Any count of cells per ticket is therefore a count of first failures. Fixing one moves cells to
another column rather than turning them green, and a cell's ticket is where a reader should start
rather than the whole of what stands in the way.

**The same shape appears on the cpu side, where the loser is not merely unmeasured but invisible.**
Three ordering questions have turned up in the rollout — #152 versus #183, #152 versus #175, and
#175 versus #189 on `tpcds/q77`, whose rollup made the hasher a candidate at exactly the three tp4
modes where the empty build side refuses first. A disabled cell runs nothing, so whether #189 would
also have rejected q77's grouping-set id cannot be read off the corpus at all; the question is not
open pending a measurement, it is closed to measurement until #175 is fixed. Any claim that a ticket
has been exhausted by the corpus should be read against that.

**The sixth batch showed the rule cleanly.** Four unrelated queries — a semi-join with a correlated
subquery, a five-way join, an eight-way join, an anti-join over a self-join — all fell the same way:
#183 at `bp-tp1-single` and #152 at the other four modes, without exception. `bp-tp1-single` is the
only mode whose joins happened to get a single probe batch, so it was the only one reaching the
unload — a correlation the eleventh batch retired, below.

The eighth batch narrowed that. #152 has two rows and only one is mode-sensitive: the build-side
copy is about a second probe batch erasing what the first consumed, so a single probe batch avoids
it, while the probe-side copy is refused at any batch count — one probe batch is still one copy.
Which half a query meets is a property of its JOIN TYPE, inner copying the build and outer the
probe: `tpcds/q97`'s outer join refuses at `bp-tp1-single` too and never reaches a second cause.

The eleventh batch replaced the mode shorthand with a batch-count rule — #152's build half fires
whenever a copying join's probe side arrives in more than one batch — and the fourteenth falsified
it. `tpcds/q11` at `bp-tp1-single` has eleven Inner joins, all copying, with probe batch counts
1,1,1,336,1,336,1,88,1,88,1 measured and recorded before the device ran; it reaches the unload and
fails on #183. A copying join took 336 probe batches at that mode and did not refuse.

So what decides it is **not known**. `tpcds/q71` and `tpcds/q2` refuse at `bp-tp1-single` and q11
does not, and all three have copying joins with streaming probe sides there — whatever separates
them is not something the goldens carry. The likeliest next question is what feeds the build side
rather than what feeds the probe, since a build that is another node's handle is consumed by its
first reader where one a scan re-materializes per call is not; that is readable in `cpp/src` and in
the recipe writer rather than in the corpus, and it belongs to #152 rather than to a rollout.

Three narrowings and one falsification, recorded because the alternative is a fourth version that
fits every batch so far. What survives is that #152's two halves differ by join type.

So the device column currently measures #152's reach rather than the engine's, and the two tickets
are ordered: fixing #152 moves most of those cells to #183 rather than turning them green, and #183
is one defect at one site. Those two, in that order, are what stand between this rollout and a
device column that is not empty — with the caveat the seventh batch added: where
`bp-tp1-single` gets past the join at all, it is not the mode that passes but the mode that gets far
enough to find out what else is wrong, and each query it reaches has its own answer. q7 fails there on a string, q2 on a decimal, q8 on an integer,
all at the unload. So #183 is not one defect at one site but one site with a family of type
mismatches under it, and #187 and #191 are two more of that family rather than separate work.

**A mode disabled after its file was last regenerated has no marker section**, and both golden
checks report it as an absence rather than as the disablement it is. The merge writes markers for
every declared section of a file, so regenerating any *enabled* query in that file fills the missing
ones in. Neither test's message says so, and a batch that only disables modes therefore looks
broken until an unrelated query is re-run.

**#163 is what decides where this rollout lands, and the number is knowable in advance.**
Seventeen of the fifty-six tpcds queries remaining after T19's ninth batch carry `avg`, whose count
state that ticket disables at every mode on both engines. So roughly a third of the tail produces no
cells at all: the ceiling is a hundred minus those seventeen, minus whatever else refuses whole,
which put it near **83** rather than near a hundred — with #163 being most of the gap, as
[#152](../tickets.md#t152) is most of the device column's. Neither is a rollout failure and neither
is fixable here.

**It closed at 68 fully enabled, 8 partially, 24 out entirely, 0 unreached** — 357 of 500 possible
cpu cells, and **zero** device cells beyond the six T18 left. So 76 rather than the 83 predicted, and
that miss is worth more than the number: the ceiling was computed twice, corrected from a wrong 88-to-90
down to 83, and was still high, because both computations counted only the blockers known at the
time. [#190](../tasks/active-tickets.md#t190) and [#192](../tasks/active-tickets.md#t192) arrived after the projection. **A
ceiling derived from known causes is a bound on optimism, not a prediction** — unknown blockers only
ever subtract.

Seventeen `avg` queries were projected against #163 after the eleventh batch and all seventeen were
RUN rather than declared. None failed to refuse and none refused for a different reason, which is
what makes the tail a measurement rather than an assumption.

**Roll out in batches of about five, not in one sweep.** A hundred queries enabled at once is a
regeneration whose diff nobody reads and a failure nobody can attribute: eleven golden files move
either way, so the batch size is the only thing that says which query moved which section. Five
is small enough that a red run names its cause without bisecting and large enough that the
regeneration cost is amortised. Each batch is its own commit — the five queries, their registry
cells, the sections they filled, and whatever went to `active-tickets.md` — so the history reads as
the rollout it was, and a batch that goes wrong is reverted without taking the ninety-five with
it.

Nothing else, and the list is still short enough to read off a diff: no `cost_model.conf`,
because T18 enters all eighteen node kinds at once rather than as each is first seen — an
exhaustive set entered piecemeal is one whose next gap is a rollout's problem; no `pipeline.yml` or `test_ci_coverage`,
because the targets exist by then; no `registry.rs`, whose columns are T18's.

That sweep is where T17's `nested-loop-left-join` gets its GPU columns. They ship `na` because
T17 could not commit a device run, which is [#116](../tickets.md#t116)'s shape — a cell with no
coverage and no blocker — so the reason is recorded here rather than left for a reader to
reconstruct from an empty column.

**Four tasks were specced during T19 and none of them is T20's.** They came out of what the rollout
found, and each closes tickets the device column is actually blocked on rather than shapes the corpus
lacks: [`refcounted-tables.md`](../tasks/refcounted-tables.md) for [#145](../tickets.md#t145) and
[#152](../tickets.md#t152), [`casts.md`](../tasks/casts.md) for [#183](../tasks/active-tickets.md#t183),
[`wire-schema.md`](../tasks/wire-schema.md) for [#187](../tasks/active-tickets.md#t187), and
[`empty-answers.md`](../tasks/empty-answers.md) for [#173](../tickets.md#t173) and
[#175](../tickets.md#t175). Order matters twice: `casts.md` before `wire-schema.md`, which writes the
schema `empty-answers.md` then reads. Between them they cover the two causes that hold 134 of the 138
disabled device rows.

**T20 — corpus shapes the benchmarks do not have.** Not done, and not a task any more: it is
[#195](../tickets.md#t195), which carries the audit, the six shapes and the engine work each
one needs.

**T22 is obsolete as written (2026-09-08).** `peacock_gpu_benchmarks` and the record tree it
wrote were deleted with the legacy modes, so there is nothing to port: a per-node measurement
for this mode starts from the protocol below rather than from that harness. The design
question it names — a node runs once per batch per lane, so a per-node figure is a sum over
calls and the call count belongs beside it — is the part worth keeping.

**T22 — per-node benchmarks for the new mode.** Port `peacock_gpu_benchmarks` to the
batch-partitioned executor, keeping the protocol that makes its numbers comparable: one
discarded warm-up, ten measured runs, the **2nd-smallest by `total_us`** reported whole, and
the floor measured over 200 samples. The run counts stay compile-time constants
(`tests/common/mod.rs`) so every record in the tree was taken at the same ones.

**A node is called many times here, and that is the port's one real design question.** The
legacy record carries one `time_us` per node, or one per partition — a node runs once. In
this mode a node runs once per batch per lane, so a per-node figure is a *sum over calls*
and the call count belongs beside it; a node at 40 ms over 200 calls and one at 40 ms over
two are different findings, and a record that cannot tell them apart measures nothing useful.
`CallStats` is already returned per call and is where the per-call figures come from.

The record gains the mode and the tier — `<query>.<mode>.benchmark.txt` alongside the legacy
`<query>.<mode>-<tp>-<tier>` — and keeps `build_profile`, `sync_floor_us`,
`nodes_at_or_below_floor` and the rest, since they mean the same thing. It also carries which
allocator measured the run: the pool landed with
[#151](../archive/archived-tickets.md#t151), [#148](../tickets.md#t148) is still open, and a
number taken without one is not comparable with a number taken with one.

Case list as the correctness tiers use, so the measured set cannot drift from the verified
one, and `test_ci_coverage` exempts the target explicitly because it asserts nothing.

---

<!-- archived from llm-wiki/tasks/schema_and_validation.md -->

# schema registry and validation (T7/T8 remainder)

**Goal.** Finish the two tasks whose implementations landed early on
`ENS-bp-plan-skeleton` but whose test surfaces did not: prove the `Schema` carried on every
node is right, and make `validate_schemas_and_partitions` a check that can go red for the
reasons it claims rather than only for the ones a real plan happens to hit.

**What already landed, so nobody rebuilds it.** `Schema` exists with `group_keys` and
`agg_state`, every node carries a populated one, the plan goldens print the declared type per
column, and node-local validation is called from `plan_batch_partitioned` with all ten
goldens passing it node by node. That half was pulled forward by a review finding — ten
guards existed and none ran on the live path.

## T7 — what the schema tests must show

- A hand-built plan carrying a project, an aggregate and a union produces the expected types
  and the expected semantics annotations, asserted on the tree rather than on rendered text.
- The annotations survive the aggregate sequence: `agg_state` is right at the init, at the
  per-lane merge and at the finalizing merge, which are three schemas for one logical
  aggregate.
- **Decimal precision and scale through project, aggregate and union-cast.** This is the
  one that earns the task: `avg`'s state columns were once typed backwards and per-node
  bytes could not show it, because both engines derive them from the same plan schema — so
  CPU and GPU agreed on the same wrong number and only a real divide would have diverged.

## T8 — what validation still owes

- The generic structural pass, over and above the per-node checks that exist.
- Manually constructed wrong combinations: each rule turned red by an input built to break
  it, per the reviewer's anchor that a guard which cannot go red is not a guard.
- Validation run over every canonized corpus plan as a standing check, not as a one-off.
- Defects in the checks themselves, including any the reviewer reported against
  `ENS-bp-plan-skeleton` and I deferred here.

**Constraints.** Every committed golden plan passes the validation this task adds. A
rejection is a planner defect until shown otherwise: stop, report it, and fix planning —
never weaken the check to fit the plan, and never regenerate a golden to silence one. The
expectation is still that no plan moves, so a golden that does move is a deliberate decision
taken with the human rather than a side effect of the regen. A test that only passes is worth
less than one shown to fail on the defect it guards.

**Verification bar.** Every rule in `validate_schemas_and_partitions` has an input that turns
it red. Decimal fidelity is asserted at each step of project → aggregate → union-cast. Every
committed golden plan passes validation, and a golden moves only where a planner fix required
it. `test_ci_coverage` names whatever targets appear.


Merged 2026-08-18 as PR #121.


---

<!-- archived from llm-wiki/tasks/join_capability_recipe.md -->

# join capability through the recipe plan (T0 extension)

**Goal.** Establish, by execution rather than argument, that the frozen FlatBuffers schema
and C++ operators can run **every** join mode in the batch-partitioned model — with the
build side complete and the probe side arriving in batches — and write the per-mode
lowering down where the implementation tasks will read it.

**What was built** (all in `scripts/exec_model/`, coordinator-owned):

- `operators/join_types.py` — `JoinType` in the fbs vocabulary and the capability matrix as
  one function, so no backend can hold a different opinion of it.
- `operators/cudf_calls.py` — the cuDF calls `cpp/src/operators/join.cpp` makes, at their
  own signatures: joins that return gather maps, `gather` with its out-of-bounds policy,
  `scatter`, `apply_boolean_mask`, `cross_join`.
- `operators/recipe.py` — the fb node structs, a handle registry that consumes on read as
  `NodeSession` does, and the node implementations mirroring join.cpp branch for branch.
- `operators/recipe_join.py` — the second join backend: every call answered by emitting fb
  seqs and making `execute_node` calls.
- `operators/joins.py` — the pandas backend, widened from five join types to all nine plus
  cross and nested loop.

**Constraints.** The two backends share no join code; agreement between them is the
evidence. The recipe backend may not reach for python where the frozen surface has no
answer — it names the gap and counts what working around it costs (`copy_handle`).

**Verification bar.** `scripts/exec_model/tests/test_end_to_end.py`: every join mode
against a SQL oracle, on both backends, at five batching/partitioning configs and across
the layout injector's presets; the emitted seq sequence asserted per mode against the
spec's table; the per-batch copy counts asserted per family; every refused shape refused
loudly on both backends. Whole prototype suite green, ~90 s.

**Outcome.** Recorded in the spec's [join capability
matrix](../architecture.md#capability-matrix): every mode is expressible,
the streamed-probe copy cost is [#152](../tickets.md#t152) quantified per family, and one
shape turned out to be a defect in the shipping engine rather than a limit of the mode —
[#153](../tickets.md#t153).


---

Merged 2026-08-04 as PRs #112 / #115 / #114.


---

<!-- archived from llm-wiki/tasks/build-test-flags.md -->

# Task: build-test.sh flag surface, failure semantics, and the regen guard

On `ENS-test-exec-mode`. Touches `scripts/build-test.sh`, `scripts/build-test-shadgpu.sh`,
`peacockdb-core/tests/test_plan_bytes.rs`, `llm-wiki/build-test.md`. The rules this
implements are already in `coding-style.md` ("Bash: the flag set is an interface, and
failure is fatal", b05a631) — that bullet was written from this script's defects.

No golden may move. No test may be added, removed or re-tiered.

## A — flag surface

1. **Delete `--cpu`.** It sets the default and has no callers anywhere in the repo.

2. **`--gpu` and `--rust-only` are mutually exclusive** and must be *rejected*, not
   resolved by argument order. Today:
   - `--rust-only --gpu` → `MODE=gpu` with `RUST_ONLY=1` still live, so the run branch
     sets `LD_ENV=":"` and the GPU binaries never get `LD_LIBRARY_PATH` — they fail to
     resolve `libpeacock_gpu` and it reads as a product fault.
   - `--gpu --rust-only` → the reverse, GPU silently ignored.
   Same two flags, opposite outcomes, no warning. Error naming the contradiction.

3. **Replace `--push-testdata` / `--pull-testdata KIND[,KIND]`** with per-kind flags:

       --push-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}
       --pull-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}

   The point is structural, not cosmetic: the argument parser *becomes* the validator.
   No comma splitting, no kind lookup at the call site, and an unknown kind is just an
   unknown flag caught by the existing `*) usage` arm before any side effect. The
   multi-kind form is generality nobody uses — every documented invocation moves exactly
   one kind. `testdata_dirs_for_kind` stays as the kind→dirs map; only its use as a
   validator goes.

4. **`--fetch-goldens` → `--pull-goldens`.** A rename, not an alias. Today it appends
   `goldens` to `PULL_TESTDATA`, so passing it alongside `--pull-testdata goldens` pulls
   twice. Keep the current ordering property: the flag must be resolved *before* the
   `--host` requirement check, so `--pull-goldens` alone still demands `--host`.

5. **`--rsync` → `--push-binaries`, in BOTH scripts.** `--rsync` names the tool rather
   than the intent. `build-test.sh`'s own first line says it mirrors
   `build-test-shadgpu.sh`, so renaming one and not the other breaks the parallel that
   makes either readable after time away.
   - It **still pushes goldens.** They are part of the payload, not an optional kind:
     binaries shipped without the fixtures they assert against is the trap that produced
     110/110 "canonical file not found". Requiring `--push-goldens` alongside would
     replace a footgun with a guarded footgun.
   - `--push-goldens` remains useful and is not redundant with it — it is the subset
     operation, refreshing fixtures without rebuilding or reshipping binaries.

6. **Require an action.** `--host x` with no `--build`/`--push-binaries`/`--run`/push/pull
   currently does nothing and exits 0.

7. **Value-taking flags check their value exists.**

8. **`usage()` states what is deliberately absent**, so the next reader does not file it
   as a gap:
   - no `--pull-binaries` — binaries flow one way, built locally and shipped;
   - `embeddings-cache` is a per-host intermediate for the tpch vector datasets
     (`fetch_embeddings.sh`, ~1.8 GB, gitignored) and is deliberately not syncable.
   Also state the mode ladder: rust-only ⊂ cpu ⊂ gpu, and that a mode which builds more
   never runs less.

## B — failure semantics

1. **`set -euo pipefail`.** `pipefail` is load-bearing: `cargo test --no-run … | python3`
   currently takes python's status, so a cargo failure is caught only by the explicit
   emptiness check afterwards.

2. **An empty derived suite is an error.** Verified: `mapfile -t A < <(helper)` with a
   helper that outputs nothing yields a zero-length array, the `for` body never runs, and
   the script exits 0. A typo in the derivation would silently run no tests and report
   success.

3. **All validation before the first side effect** — a bad flag must fail before anything
   is built, shipped or deleted.

4. **The remote heredoc keeps its deliberate `set -e` omission.** Running every test
   binary and accumulating `rc` is correct: a failing C++ test must not skip the Rust
   ones. This is the stated exception `coding-style.md` allows; keep the comment that
   says why, and keep the non-zero exit at the end.

## C — deduplicate the sync layer

1. **One `sync_goldens()`.** "Push goldens" exists twice with different flags: the
   `--push-binaries` block uses `rsync -r --delete`, `--push-goldens` uses
   `rsync -a --delete`. Same intent, different metadata handling, no shared code.

2. **Push mirrors (`--delete`) uniformly; pull is additive uniformly** — and the
   asymmetry is deliberate, so document it rather than "fixing" it. The remote is a
   *partial* mirror: `testdata/goldens/` contains `tpch.sf40/` (16 CSVs) and sf40 lives
   on shad-gpu, so mirroring downward from verda would delete fixtures that host never
   had. The destination is a git working tree.

   Known consequence, accepted: a regen deletes a `.result.txt` when a result exceeds
   256 KB (`maybe_write_result_golden`), and an additive pull cannot propagate that. The
   deletion is already announced on stderr and reaches the operator through the ssh
   heredoc — that is the handling, not a `--delete` flag armed for one rare case.

3. **Drop the `*.txt` filter on the goldens pull** — after A/B and D, not before. Its real
   job is keeping `plan_bytes.sha256` out of the round trip, which becomes the self-guard's
   job in D; keeping both would be two mechanisms for one invariant with the weaker one in
   the wrong place. Removing it also stops silently dropping the 16 sf40 CSVs.

4. **Clear `cpp/install/rust-tests` before staging**, matching `build-test-shadgpu.sh`.
   `build-test.sh` does not, so orphaned binaries from a previous mode accumulate;
   today that is mitigated only by running binaries by explicit name.

## D — move the regen guard into the test

`--update-canonical` exports `UPDATE_CANONICAL=1` to every staged binary, so the run set
doubles as the regen set and `regen_excluded()` subtracts `test_plan_bytes` back out.
That protects one invocation path only: `UPDATE_CANONICAL=1 cargo test --features
rust-only -p peacockdb-core --test test_plan_bytes` — the exact command the golden's own
header prints — still rewrites `plan_bytes.sha256` silently.

Move the refusal into `test_plan_bytes.rs`: under `UPDATE_CANONICAL`, refuse unless a
dedicated override (`PEACOCK_REGEN_PLAN_BYTES=1`) is also set, panicking with the reason —
the digests are the wire-format guard, the C++ side reads those bytes, and regenerating
rewrites the evidence instead of failing. Then **delete `regen_excluded()`**; the script
stops carrying knowledge about a test's internals.

`test_cost_model` stays in the regen set. That inclusion is a fix, not a risk: `.cost.txt`
derives from `.cpu.txt`, which a regen rewrites, so the old six-target list left every
`.cost.txt` stale and `test_cost_model` went red immediately after a "successful" regen.

## E — comments and docs

- Header comment says "Two suites" and then lists three.
- The `PUSH_TESTDATA` comment lists four kinds; `usage()` lists five (`duckdb-dynfilters`).
- The goldens-push comment justifies itself entirely in verify terms and disposes of the
  regen case in a parenthetical, which reads as "this push is redundant when
  regenerating" — the opposite of what is true. State both reasons: for a verify run the
  binaries assert against these files; for a regen the push establishes the baseline, so
  the pulled-back set is local-committed ∪ regenerated rather than remote-leftovers ∪
  regenerated, and `--delete` is the mechanism.
- `build-test-shadgpu.sh:176` references "the goldens that build-test.sh's --rust-only
  mode used to skip" — check it still says something true.
- `llm-wiki/build-test.md`: the verda row, the shad-gpu row (`--rsync` → `--push-binaries`),
  the golden-regen bullet, and a line recording that `embeddings-cache` is not syncable.

## Sequencing

**A+B → D → C → E.** A/B is the interface and the failure model and touches everything
later; D must land before C3; C is mechanical once D is in; E last, so the docs describe
the final state rather than an intermediate one.

## Verification

- `bash -n` on both scripts.
- A flag matrix: for each mode, print the derived suite and assert it matches; for each
  rejected combination, assert it errors non-zero. Include `--gpu --rust-only` in both
  orders, `--host` with no action, and a value-taking flag with no value.
- Prove the empty-suite error fires (temporarily break the derivation, confirm non-zero,
  restore).
- One real `--rust-only --build` to prove staging still produces binaries.
- `git status --porcelain testdata/goldens` must be empty at the end.

## Out of scope

`maybe_write_result_golden`'s discarded `remove_file` result and its unconditional "no
golden" message — a ticket, not this task. The `--cpu`/`--rust-only` naming axis is
resolved by A1/A2 and needs no further rename.


---

<!-- archived from llm-wiki/tasks/widget-golden-links.md -->

# Task: link CPU ✓ cells to their goldens; small-font Query and Σout columns

Branch `ENS-widget-golden-links` (off `ENS-test-exec-mode`). Cost-report widget only —
`cost-report/src/main.rs`. No changes to the registry CSV, the test suite, or any golden.

## 1. The premise, verified

Every `enabled` cell in `ftc_tp1`, `ftc_tp8` and `partitioned_cpu` **has a committed
`.cpu.txt`**. `assert_cpu_cost_canonical` runs unconditionally on every CPU macro
invocation (`common/mod.rs:847`), before and independent of the `ResultGolden` keyword —
that keyword gates only `.result.txt`. So a `✓` in those three columns always has a
golden to point at.

This does *not* extend to the GPU columns: those read the CPU golden rather than owning
one, so leave `full_table_gpu` / `partitioned_gpu` cells alone.

## 2. Which golden each ✓ points at

The device label is **not** in the CSV, and it is not one-per-column:

| Column | Golden label |
|---|---|
| `ftc_tp8` | `full_table-tp8-mini` |
| `partitioned_cpu` | `partitioned-tp8-standard` |
| `ftc_tp1` | `full_table-tp1-standard` |

**Corrected 2026-08-04.** This section originally said `ftc_tp1` is tp1-standard "except
`scan_limit`, which is `full_table-tp1-mini`". That is wrong, and the developer caught it:
`scan_limit` has **both** goldens. It is registered twice — `tp1_mini` at
`test_cpu_full_table.rs:24` and `tp1_standard` at `:188` — and `column_for` keys on the tp
count, not the memory tier, so both land in the single `ftc_tp1` cell. It is not the
exception to the column's label; it is the one query whose cell aggregates two runs.

**Decision: one label per column, `full_table-tp1-standard`.** It is the label every tp1
row uses including `scan_limit`, so the link target is predictable from the column alone.
Drop the tp1-mini candidate rather than carrying config that nothing can reach — this repo
already treats unreachable config as a hazard in its own right (`GOLDEN_INVARIANT_EXEMPT`
and `INTENTIONALLY_NOT_IN_CI` both carry staleness assertions for exactly that).

What a cell aggregating two runs should render is a real question, but a hyperlink cannot
express it and the answer would change the cell shape. Out of scope here; raise it as its
own task if the tp1-mini run ever needs to be reachable from the widget.

The fail-loud check below is what makes dropping the candidate safe: a future query
registered ONLY at tp1-mini has no golden under the single label, so the widget fails
naming it instead of rendering a dead link. That is strictly better than a silent second
candidate, because it forces the decision rather than guessing.

Link target: the same `links.golden_url(canon_rel, stem, "<label>.cpu.txt")` helper the
Σout cell uses, so dry runs with no sha degrade to plain text exactly as they do today.

Only `✓` becomes a link. `~`, `✗` and `—` stay plain — there is no golden behind them.

## 3. Small font

- **Query column:** non-numeric queries only — `aggregate_groupby`, `scan_limit`,
  `shuffle_stddev`, `hash_join`, … The discriminator already exists: `Row::number` is
  `None` for exactly these (it is `Some(n)` for `q<N>`). Numbered queries keep today's
  size.
- **PeacockDB Σout and DuckDB Σout:** small font for **both the header and the values**.
  The Ratio column is not in scope.

Both renders. They use different mechanisms and the difference is load-bearing: the HTML
report can use a CSS class (`th.modeh` is the precedent), while the PR-comment table must
use `<sub>` because GitHub strips `class`/`style` — see the `mode_cells_md` doc comment.

## 4. Watch for

- `MODE_COLUMNS` and `registry.rs::COLUMNS` are the CSV header contract. This task is
  display-only: do not touch either, and do not change any cell value.
- `ftc_cell()` currently renders `tp1✓ tp8✓` as one string inside a single `<td>`. Both
  glyphs need to become independently linkable, so that function has to return markup
  rather than a plain label — check its callers in both renders before changing its shape.
- `CPU_DEVICE` is the const the Σout cells resolve through. The new per-column labels are
  related but not the same thing; do not fold them together in a way that makes a Σout
  change silently move a mode link.

## 5. Verification

- `cargo test -p cost-report` — the widget's own unit tests, including the
  `mode_cells_md` shape asserts.
- `scripts/cost-report-preview.sh` and eyeball the generated HTML: a linked `✓` per
  enabled CPU cell, `scan_limit`'s tp1 link resolving to `full_table-tp1-mini`, small-font
  micro-query names, small-font Σout headers and values.
- Confirm the PR-comment render still parses as HTML on GitHub (`<sub>`, no class/style).
- No golden, CSV or test-suite file appears in `git status`.


---

<!-- archived from llm-wiki/tasks/test-exec-mode.md -->

# Task: execution mode explicit in test macros and golden filenames

Branch `ENS-test-exec-mode` (off `ENS-llm-wiki`). Pure refactor: **no behavior change, no
golden regeneration, no coverage change**. Every test that runs today must still run, with
the same assertions, reading the same bytes from renamed files.

## Why

Two things are inferred today that should be stated:

1. `common/mod.rs::partition_mode(device)` maps the string `"tp8-standard"` to
   `PartitionMode::RealMultiPartition` and everything else to `SinglePartition`. A device
   label silently picks an executor. Adding a device (say `tp8-mini` as a real 8-way tier,
   which #91 wants) would route it to the wrong executor with no diff to the routing code.
2. Golden filenames carry only `tp<N>-<tier>`, so `q15.tp8-mini.cpu.txt` does not say which
   CPU executor produced it. The mode is real: the same plan at tp8 produces a different
   per-node cost tree under full-table vs partitioned execution.

`node13` is an obsolete task number used as an executor name. It goes.

## 1. Macro renames

| Old | New |
|---|---|
| `cpu_result_test!` | `cpu_full_table_result_test!` |
| `cpu_result_approx_test!` | `cpu_full_table_result_approx_test!` |
| `cpu_node13_result_test!` | `cpu_partitioned_result_test!` |
| `cpu_node13_result_approx_test!` | `cpu_partitioned_result_approx_test!` |
| `gpu_test!` | `gpu_full_table_test!` **and** `gpu_partitioned_test!` |

`cpu_result_error_test!` / `cpu_result_fits_test!` keep their names — decision, not
oversight: they take a raw budget rather than a device, read no mode-tagged golden, and the
resident-OOM enforcer they drive is full-table-only by construction (#91 tracks porting it).
Say that in a one-line comment where they are defined so the asymmetry doesn't read as a
miss.

Generated test-fn names include the mode, so two modes at the same device can never collide:
`cpu_full_table_tpch_sf1_q1_tp8_mini`, `cpu_partitioned_tpch_sf1_q6_tp8_standard`,
`gpu_full_table_tpch_sf1_q1_full_table_tp1_standard`. Grep `pipeline.yml` and the
`build-test*.sh` scripts for `--exact` / filter strings built from the old names and fix them.

## 2. Call-site shapes

CPU — mode from the macro name, device stays `tp<N>_<tier>`:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, no_result_golden);
    cpu_partitioned_result_test!(tpch, 1, q6, tp8_standard, result_golden);

GPU — mode from the macro name; the device argument is the **combined golden label**, which
is the golden filename component verbatim:

    gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, golden_exact);
    gpu_partitioned_test!(tpch, 1, q3, partitioned_tp8_standard, golden_exact);

So `gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, …)` reads
`goldens/tpch.sf1/q1.full_table-tp1-standard.{cpu,result}.txt` — reconstructible from the
invocation with no lookup. The GPU run config splits cleanly: `PartitionMode` comes from the
macro name, `tp` + budget are parsed out of the label. A crossed pair
(`gpu_full_table_test!` with a `partitioned_…` label) is visible at the call site; assert it
cannot pass silently — the label's mode prefix must equal the macro's mode.

## 3. Last parameter: bool → enum

The trailing `$gen:literal` bool becomes an ident backed by a real enum in `common/mod.rs`,
so the call site names the artifact it produces:

    pub enum ResultGolden { Write, Skip }   // `result_golden` | `no_result_golden`

Mirror `gpu_result_mode`'s shape (keyword → enum, unknown keyword panics with the accepted
set). Keep the existing invariant comment about when writing is correct — it is the useful
part of that parameter.

## 4. Delete the implicit routing

- Delete `partition_mode(device: &str)` outright.
- `assert_cpu_results_match_datafusion`: replace `use_node13: bool` with an explicit
  execution-mode parameter that carries both the executor choice and the `PartitionMode`.
- `assert_gpu_query` / `assert_gpu_nodes_match_golden`: take the `PartitionMode` from the
  caller (the macro), never from a label.
- `plan_is_node13_executable` → `plan_is_partitioned_executable`, unchanged behavior; it
  stays the safety assert on the partitioned path.
- `registry.rs::column_for`: kind `"node13"` → `"partitioned"`; kind `"gpu"` splits into
  `"gpu_full_table"` / `"gpu_partitioned"` mapping straight to their columns instead of
  sniffing `device.starts_with("tp1")`. The `ftc` arm keeps its tp1/tp8 split — that one
  reads a `tp` count out of a `tp` label, which is parsing, not routing.

### The plan tier keeps a label → mode lookup (amended 2026-08-04)

"Delete `partition_mode` outright" was wrong about the plan tier, and the developer caught
it. `test_plan_bytes.rs::corpus()` builds its entire corpus by reading `.plan.txt` filenames
off disk and splitting the device out of the stem — there is no call site to state a mode at,
and `plan_bytes.sha256` keys on `<query>.<device>`, which this task freezes. The mode is
load-bearing there: `shuffle-additive` @ tp8-standard is the one plan golden whose shape
depends on `RealMultiPartition`.

So: `partition_mode` is deleted from the **execution** path — CPU results, GPU query, GPU
nodes, and the oracle `CpuExecutor` all take the mode from the macro name. One narrowly
scoped `plan_partition_mode(device)` survives for the plan tier, documented as that tier's
label contract and made **exhaustive** — an unknown label panics instead of falling through
to `SinglePartition`. That catch-all is the actual hazard this task's "Why" names, and
because `corpus()` reads labels off disk, exhaustiveness is self-enforcing: a new plan golden
with an unlisted device fails loudly instead of silently planning single-partition.

Keep it to **one** mechanism. Do not also add an explicit mode argument to `query_plan_test!`
or `plan_for` — two sources for the same fact is how a plan golden gets generated under one
mode and its byte digest under the other. Concretely: `test_query_plan_misc.rs:27`
(`plan_for(…, "tp8-standard")`) is unchanged, while `test_cpu_executor_misc.rs:41` is an
execution-tier site and does pass `PartitionMode::RealMultiPartition` explicitly.

The `coding-style.md` entry names this as a deliberate exception, so the surviving function
reads as scoped rather than missed.

**CSV columns and `testdata/cost-registry.csv` do not change.** `COLUMNS` is the committed
fixture's header contract (`load_csv` asserts on it) and `MODE_COLUMNS` in cost-report keys
off the same names. Renaming `ftc_tp1` is a separate change; not this one.

## 5. Golden filenames

New: `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`, e.g.
`q15.full_table-tp8-mini.cpu.txt`.

**Unchanged:** `.plan.txt` (plan shape is executor-independent, and
`goldens/plan_bytes.sha256` pins those names) and `.duckdb_cost.txt` (oracle, no peacock
executor involved).

`git mv` only — do not regenerate. The mapping is total and collision-free because no device
label is used by both modes today (verified: `tp8_standard` appears only under
`cpu_node13_*`; every other label only under `cpu_result_*`):

| Old | New | files |
|---|---|---|
| `*.tp8-mini.{cpu,cost}.txt` | `*.full_table-tp8-mini.…` | 129 + 129 |
| `*.tp1-mini.{cpu,cost}.txt` | `*.full_table-tp1-mini.…` | 1 + 1 |
| `*.tp1-standard.{cpu,cost,result}.txt` | `*.full_table-tp1-standard.…` | 110 + 110 + 104 |
| `*.tp8-standard.{cpu,cost,result}.txt` | `*.partitioned-tp8-standard.…` | 18 + 18 + 14 |

634 files total across `testdata/goldens/{tpch.sf1,tpcds.sf1}`. Counts are mine and worth
re-deriving — if yours differ, say so before renaming rather than after. Verify content is
untouched: the multiset of file *contents* must be identical before and after (e.g. compare
sorted `sha256sum` values of the renamed set).

## 6. File reorganization — by mode, not by memory tier

| Now | Becomes |
|---|---|
| `test_cpu_executor.rs` (tp8-mini + tp1-mini + tp8-standard) | `test_cpu_full_table.rs` (tp8-mini, tp1-mini, tp1-standard) |
| `test_cpu_h200.rs` (tp1-standard) | `test_cpu_partitioned.rs` (tp8-standard) |
| `test_gpu.rs` | `test_gpu_full_table.rs` + `test_gpu_partitioned.rs` |

`test_cpu_executor_misc.rs` / `test_gpu_executor_misc.rs` keep their names; they only need
their golden paths updated.

Registry ownership follows the files — update both the `registry_matches_csv_*` fns and their
doc comments:

- `test_cpu_full_table.rs` → `ftc_tp1` + `ftc_tp8`
- `test_cpu_partitioned.rs` → `partitioned_cpu`
- `test_gpu_full_table.rs` → `full_table_gpu`
- `test_gpu_partitioned.rs` → `partitioned_gpu`

The cross-binary caveat in `test_cpu_h200.rs`'s doc comment (scan_limit registered at
tp1-mini in one binary, tp1-standard in another) is moot once both live in
`test_cpu_full_table.rs` — delete it rather than carrying it forward. The note at the foot of
`test_gpu.rs` explaining why the cross-mode invariant lives in `test_query_plan.rs` is still
load-bearing; keep it on whichever GPU file you consider primary.

`test_gpu.rs`'s file-level doc explains the merged one-run-asserts-both design — that belongs
in both new GPU files or in the macro doc, not dropped.

## 7. Consumers that read the old names

- `cost-report/src/main.rs`: `CPU_DEVICE = "tp8-mini"` → `"full_table-tp8-mini"`. Check the
  surrounding comment about scan_limit being tp1-mini-only (~L345) — it still holds, but its
  wording names devices.
- `registry.rs::assert_cross_mode_golden_invariant`: the `("full_table_gpu", "tp1-standard")`
  / `("partitioned_gpu", "tp8-standard")` pairs become the new labels. This one is
  load-bearing — it is what catches an enabled GPU mode with no CPU golden.
- `test_ci_coverage.rs`: four new test-target names in, two out. This gate is the reason a
  renamed binary can't silently drop out of CI.
- `.github/workflows/pipeline.yml`: rust test target lists in the cpu-cpu tier and the
  GPU-remote job; `scripts/build-test.sh` and `scripts/build-test-shadgpu.sh` build/stage
  test binaries one `--test` at a time and name them.
- `test_cost_model.rs` globs `*.cpu.txt` and derives the sibling `.cost.txt` by string
  replace — should need no change; confirm rather than assume.
- `llm-wiki/build-test.md` and `architecture.md`: test-file names, device labels, golden
  naming. Same commit.

## 8. Wiki changes owed by this task

`coding-style.md` — new antipattern entry, in the voice of the existing thread-local one:
implicit routing from a label. `partition_mode(device)` turned the string `"tp8-standard"`
into `RealMultiPartition`; the executor a test ran was a side effect of how its golden was
named, and a new device label would have silently taken the wrong path with no diff to the
routing code. State the mode at the call site and pass it as a parameter.

`build-test.md` — one sentence: a refactor that must not change behavior is verified with a
representative subset (one query per mode/tier per binary) plus the full rust-only tier, not
a full CPU/GPU suite run; the goldens are the invariant.

## 9. Verification — subset only, explicitly

Do **not** run the full CPU or GPU suite.

1. The rust-only **golden/meta gates**, must be green — named by target, not by package:

       cargo test --features rust-only -p peacockdb-core \
         --test test_plan_bytes --test test_cost_model --test test_ci_coverage

   plus the registry tests, which live inside the execution targets and must be filtered to
   (`--test test_query_plan -- registry_`, and the same for `test_cpu_full_table` /
   `test_cpu_partitioned`). Together: the CSV contract, the cross-mode golden invariant, and
   the CI-coverage gate. Cheapest and highest-value gate for this change; seconds, not
   minutes.

   **Corrected 2026-08-04** — this item originally read `cargo test --features rust-only -p
   peacockdb-core` with no `--test`, which contradicted this section's own headline: nothing
   cfg's the CPU execution targets out of the rust-only build, so package-wide sweeps all 241
   + 18 of them. `--features rust-only` selects a *build*, not a tier; only `--test` selects
   the tier. `build-test.md`'s table lists the bare package command as the rust-only loop,
   which is what made this easy to mis-transcribe — the §8 sentence owed to `build-test.md`
   should make the distinction explicit rather than just saying "run a subset".
2. `cargo build --tests` clean, no new warnings.
3. CPU subset — a few per binary, chosen to cover each device label:
   `test_cpu_full_table` at tp8-mini, tp1-mini (scan_limit), tp1-standard;
   `test_cpu_partitioned` at tp8-standard (include one approx: tpcds q17).
4. GPU subset on shad-gpu — one per new binary is enough (e.g. tpch q1 full-table, tpch q6
   partitioned). This proves golden-path resolution on both binaries; it is not a
   correctness run.
5. **Test-count invariant**: capture `--list` output for the affected binaries before and
   after and show the sets correspond 1:1 under the intended renames. A refactor that
   silently drops a test is the failure mode here, and neither a green subset nor
   `test_ci_coverage` alone would catch a dropped `gpu_test!` line.

## Out of scope

CSV column renames; `ftc` as a kind name; porting the resident-OOM enforcer to the
partitioned driver (#91); regenerating any golden; re-tiering any test.

## 10. Follow-up (added 2026-08-04): fold the approx variants into an oracle argument

Human's instruction, on this same task rather than a new one.

The `_approx_` macros exist only to pass `Some(1e-12)` instead of `None` for `rel_tol`.
That is a property of how the result is compared, not a different kind of test, and
spelling it in the macro name means two names per mode where one plus an argument says
more. Delete `cpu_full_table_result_approx_test!` and `cpu_partitioned_result_approx_test!`
and add a **second-to-last** argument to the two surviving macros:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, data_fusion_exact, no_result_golden);
    cpu_partitioned_result_test!(tpcds, 1, q17, tp8_standard, data_fusion_approximate, result_golden);

Backed by a real enum in `common/exec_mode.rs`, keyword-mapped like `ResultGolden` and
`gpu_result_mode` (unknown keyword panics naming the accepted set):

    pub enum CpuOracle { DataFusionExact, DataFusionApproximate }

`DataFusionExact` → `rel_tol = None`, `DataFusionApproximate` → `Some(1e-12)`.

The name states what the oracle IS, which the old name did not: **both** variants compare
against a live plain-DataFusion run at `target_partitions = 1` (`build_session_state(1)`);
only the float tolerance differs. Nothing about the oracle changes — this is a rename of
an existing bool-in-disguise.

Move the 1e-12 rationale — float summation reassociates across partitions at tp>1, ~1 ULP,
while the `output_bytes` cost golden stays exact because a ULP does not change byte width —
onto the `DataFusionApproximate` variant, where `ResultGolden` and `GpuResultMode` keep
theirs. Do not leave it stranded on a deleted macro.

**The five call sites that become `data_fusion_approximate`** (every other CPU call site
takes `data_fusion_exact`):

| File | Query |
|---|---|
| `test_cpu_full_table.rs:65` | tpch `shuffle_stddev` @ tp8-mini |
| `test_cpu_full_table.rs:85` | tpcds `q14` @ tp8-mini |
| `test_cpu_full_table.rs:109` | tpcds `q39` @ tp8-mini |
| `test_cpu_full_table.rs:226` | tpch `shuffle_stddev` @ tp1-standard |
| `test_cpu_partitioned.rs:38` | tpcds `q17` @ tp8-standard |

Note `tpcds q14 @ tp1-standard` is **exact** today and stays exact — at tp1 there is no
reassociation. Converting it along with its tp8-mini sibling would silently loosen a check.

**Test-fn names do not change.** The approx macros already generated the same
`cpu_<mode>_<ds>_sf<sf>_<query>_<device>` pattern as the exact ones, so the 259/259
correspondence must still hold exactly. Re-run the `--list` comparison and say so; a
changed count here means something other than the intended edit happened.

Verification is §9 unchanged, and no golden may move: `rel_tol` affects only the result
compare, never `assert_cpu_cost_canonical`.
