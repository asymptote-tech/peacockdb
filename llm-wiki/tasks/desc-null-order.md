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
