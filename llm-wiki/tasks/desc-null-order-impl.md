# desc-null-order implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A descending sort key puts its nulls where the plan says on the device. Five pins
become agreement tests, two operator cases and six pbench queries are added, and tpcds q34 and
q71 turn on at all five device modes (#202). First, the PR comment drops its Features column
and collapses pbench to its summary line, so the six rows fit (#258).

**Architecture:**
- **One mapping.** `null_order_of(const fb::SortExprNode*)`, declared in
  `cpp/src/peacock/operators.h` and defined in `cpp/src/operators/sort.cpp`, answers `BEFORE`
  when `nulls_first == asc`, else `AFTER`. `execute_sort` and the k-way merge in
  `node_session.cpp` both call it, so the two sites cannot drift.
- **Tests.** The five `bug_` pins assert agreement. `DESC NULLS LAST` on a nullable key is added
  for the accumulating sort and the merge.
- **The PR comment** (`render_markdown` in `cost-report/src/main.rs`) loses its Features column
  and shows pbench as one summary line. Both html pages are untouched.
- **Corpus.** Six pbench queries first run on the cpu, their device cells off under #202. Then
  the fix turns them on with q34 and q71.

**Tech stack:** C++20/cuDF 25.02, the Rust operator harness (`gpu_tests::`), the corpus,
DuckDB 1.5.4, nebius-gpu.

**Spec:** [`desc-null-order.md`](desc-null-order.md). Task 15 of chain K, after oracle-columns.

## Global Constraints

- **Only the null-order mapping changes in the engine.** No wire, ABI or facade change.
- **The mapping:** `cudf::null_order::BEFORE` when `se->nulls_first() == se->asc()`, else
  `AFTER`. cuDF applies `null_order` before it flips a `DESCENDING` key.
- **Both sites change in one commit** (Task 3). A merge reading runs under another mapping than
  the one that sorted them breaks cuDF's merge precondition.
- **The six pbench queries:** `fact` filtered to `f_id % 20 = 0`; they select their sort keys and
  `f_id`, nothing else; ties broken on `f_id`; float keys out.
- **Their oracles:** `duckdb(no_sort, exact)`, `data_fusion(sort, exact)`,
  `golden(no_sort, exact)`, at all five modes on both engines.
- **q34 and q71 keep their oracles,** `duckdb(no_sort, exact)` and `golden(no_sort, exact)`. If
  either is still red for another reason, its cells stay off under a new ticket with the
  measurement, and #202 still closes on the operator tests.
- **GPU:** nebius-gpu, `~/peacockdb-K`, through `value-case-impl.md`'s *Device cycle*, with
  `FIX="cpp/src/operators/sort.cpp cpp/src/node_session.cpp cpp/src/peacock/operators.h"`.
- **Settled, not open:** no host-only C++ test of `null_order_of`. The seven operator cases and
  eight corpus lines on the device pin it, and `cpp/tests/cpu/` is outside the spec's Scope. The
  six pbench lines land in Task 2 with their device cells off (`none`, ticket 202), and Task 3's
  red build is the run before the fix that the spec asks for.
- **Task 2 follows oracle-columns,** so the registry already has its `stored` column, between
  `tickets` and `duckdb_oracle`. A row without it is 23 fields against a 24-field header, and
  every registry reader refuses it.

- **Only the PR comment shrinks** (#258). `testdata/report.html` and cost-report's html page
  keep the Features column and every pbench row. Task 1 lands before the six rows, so
  `the_pr_comment_fits_under_the_body_cap` never goes red.

## Review Focus

1. **A later key's direction, not the first key's.** `sort-mixed-keys` mixes `ASC NULLS LAST`,
   `DESC NULLS FIRST` and `DESC`; each key's null order must come from its own `asc`. Pinned by
   that query at all five device modes.
2. **The fetch after a merge.** At tp4, `sort-desc-limit` is a top-100 cut after the k-way merge.
   The cut must fall past the nulls, or the query cannot see the defect. Pinned by Task 2 Step 2's
   count and the query's five device cells.
3. **Runs that each carry a null.** Only a consistent mapping keeps cuDF's merge precondition.
   Pinned by `a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_agrees` and
   the new `a_merge_on_a_descending_key_nulls_last_agrees`, both over three round-robin runs.
4. **A third site.** Any other `cudf::null_order` built from `nulls_first` would keep the defect.
   `git grep -n null_order cpp/src` after Task 3 shows only `null_order_of`'s body and its two
   callers. Window `order_by` keys are not sorted on the device: DataFusion sorts upstream.
5. **The page losing what the comment lost.** `render_markdown` and `render_html` share row
   data; a change in the shared part would empty the page too. Pinned by Task 1's
   `the_page_keeps_features_and_every_pbench_row`.
6. **A string key.** cuDF compares strings through another comparator. Pinned by
   `sort-desc-string`.

---

### Task 1: The PR comment makes room (#258)

**Files:**
- Modify: `cost-report/src/main.rs` (`render_markdown`, ~1093: the table header's
  `<th><sub>Features</sub></th>` and the row's features cell; the per-dataset loop) and its
  `tests` module

**Interfaces:**
- Produces: a comment with no Features column, and pbench as one line. Nothing else reads it.

- [ ] **Step 1: The tests,** in cost-report's `tests`, over the real registry as
  `the_pr_comment_fits_under_the_body_cap` builds it (`all_datasets`, `render_markdown(…, false,
  &links, None)`):
  - `the_comment_carries_no_features_column`: the body does not contain `Features`.
  - `the_comment_collapses_pbench_to_its_summary_line`: the body contains
    `pbench — {operational}/{total} GPU-operational` from pbench's `Dataset`, and none of
    pbench's query stems (`anti-null-preserved-condition`, `bool-key-group`) appears in it.
    TPC-H's `q1` row is still there.
  - `the_page_keeps_features_and_every_pbench_row`: `render_html(&datasets, "https://p/",
    &links, None, None)` contains `<th>Features</th>` and every pbench row's stem.
- [ ] **Step 2: Run** `cargo test -p cost-report`. Expected: the first two FAIL, the third PASSES.
- [ ] **Step 3: `render_markdown`.** Drop the Features header and cell. For the dataset labelled
  `pbench`, write one line in place of its `<details>` block:
  `**pbench** — {operational}/{total} GPU-operational; its rows are on the full report.` with
  `full_report_ref(published, pages_url)` after it. Update the comment above the anchors: the
  Features column and pbench's rows are on the page only.
- [ ] **Step 4: Run** `cargo test -p cost-report`. Expected: PASS. Record the size test's printed
  margin in the detail file.
- [ ] **Step 5: Commit** `cost-report: the PR comment drops Features and collapses pbench (#258)`

### Task 2: Six pbench sort queries on the cpu

**Files:**
- Create: `testdata/pbench-queries/sort-desc-nulls-first.sql`, `sort-desc-nulls-last.sql`,
  `sort-desc-default.sql`, `sort-desc-string.sql`, `sort-mixed-keys.sql`, `sort-desc-limit.sql`
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (six lines in the pbench section, in
  name order, after `scalar_subquery_cross`; the pbench "BOTH COLUMNS … but these" comment)
- Modify: `testdata/cost-registry.csv` (six pbench rows, in name order)
- Regenerate: `testdata/goldens/pbench.sf1/*.plans.txt`, `*-mini.cpu.txt`, `*-mini.cost.txt`,
  `mini.result.txt`, `duckdb-result.txt`; `testdata/report.html`

- [ ] **Step 1: The queries,** one file each, the plain query on one line:

```sql
-- sort-desc-nulls-first
SELECT f_id, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_k DESC NULLS FIRST, f_id
-- sort-desc-nulls-last
SELECT f_id, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_k DESC NULLS LAST, f_id
-- sort-desc-default
SELECT f_id, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_k DESC, f_id
-- sort-desc-string
SELECT f_id, f_s FROM fact WHERE f_id % 20 = 0 ORDER BY f_s DESC NULLS LAST, f_id
-- sort-mixed-keys
SELECT f_id, f_kb, f_k FROM fact WHERE f_id % 20 = 0
  ORDER BY f_kb NULLS LAST, f_k DESC NULLS FIRST, f_id DESC
-- sort-desc-limit
SELECT f_id, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_k DESC NULLS FIRST, f_id LIMIT 100
```

- [ ] **Step 2: Count the nulls `sort-desc-limit` must cut past.** Run:

```bash
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
/tmp/duckdb-1.5.4/bin/python -c "import duckdb; print(duckdb.sql(\"SELECT count(*) FILTER (WHERE f_k IS NULL), count(*) FILTER (WHERE f_s IS NULL), count(*) FILTER (WHERE f_kb IS NULL), count(*) FROM 'testdata/pbench.sf1/fact.parquet' WHERE f_id % 20 = 0\").fetchall())"
```

  Expected: 1,000 rows; `f_s` 53 nulls and `f_kb` 59 (fixed by `i % 19` and `i % 17`); `f_k`
  near 50 (random). Record all four in the detail file. If the `f_k` count is 100 or more, the
  limit becomes that count plus 50, in the SQL file and in the detail file.
- [ ] **Step 3: The lines and rows.** Each line:

  `corpus_query!(pbench, 1, sort_desc_nulls_first, all_modes, none, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled); // device: #202`

  The same for `sort_desc_nulls_last`, `sort_desc_default`, `sort_desc_string`,
  `sort_mixed_keys` and `sort_desc_limit`. Each registry row: the five plan and five cpu cells
  `enabled`, the five gpu cells `disabled`, `plan_status` `ok`, `tickets` `202`, `stored`
  `result` (between `tickets` and `duckdb_oracle`), the three oracles CSV-quoted as their
  neighbours are, `schema_validation` `enabled`. `features` is empty, except `top_n` for
  `sort_desc_limit`. The pbench "BOTH COLUMNS … but these" comment gains one sentence: on the
  device alone, the six `sort-*` queries are off under #202 (desc sort null order).
- [ ] **Step 4: Write the cpu sections and DuckDB's.**

```bash
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- sort_desc_ sort_mixed_keys
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset pbench
git diff --stat testdata/
```

  Only the six queries' sections are added, all in `pbench.sf1`. Never pass `--only`:
  `duckdb_result.py` writes only the sections it ran. Each `mini.result.txt` section holds
  rows, not a fingerprint: a fingerprint cannot check order. If one is fingerprinted, stop and
  tell the coordinator.
- [ ] **Step 5: Read one section.** `sort_desc_nulls_first`'s `mini.result.txt` section opens with
  its null `f_k` rows in `f_id` order; `sort_desc_nulls_last`'s ends with them.
- [ ] **Step 6: Run locally:** `--lib`, `--test test_cpu_corpus`, `--test test_corpus_goldens`,
  `--test test_cost_model` (rust-only); `cargo test -p cost-report`;
  `python3 scripts/ticket_widget.py`. Expected: PASS, the `no_sort` guard included.
- [ ] **Step 7: Commit** `corpus: pbench sorts on a nullable key, on the cpu`

### Task 3: One null-order mapping, and the device cells on

**Files:**
- Modify: `cpp/src/peacock/operators.h` (the declaration; `#include <cudf/types.hpp>`)
- Modify: `cpp/src/operators/sort.cpp` (the definition and its comment; `execute_sort`'s loop)
- Modify: `cpp/src/node_session.cpp` (the merge's loop, ~545)
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` (two pins; `input_ordered_by` and
  the imports only it used go)
- Modify: `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` (three pins, two new cases;
  `nulls_flipped` goes; `null_keys_in_one_run`'s comment loses its #202 account)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (q34, q71 and the six pbench lines:
  device `all_modes`, the `// device: #202 …` comments gone; the pbench "BOTH COLUMNS" comment
  drops the sentence Task 2 added)
- Modify: `llm-wiki/architecture.md` ("## cuDF options", the `cudf::order`, `cudf::null_order`
  row, ~1344), in this commit, since the fix falsifies it
- Modify: `testdata/cost-registry.csv` (q34 row 35, q71 row 72 and the six pbench rows: gpu cells
  `enabled`, `202` struck)
- Regenerate: `testdata/goldens/tpcds.sf1/gpu-result.txt`, `pbench.sf1/gpu-result.txt`,
  `testdata/report.html`

**Interfaces:**
- Produces: `cudf::null_order null_order_of(const fb::SortExprNode* key);` in namespace
  `peacock`. It answers `BEFORE` when `key->nulls_first() == key->asc()`, else `AFTER`.

- [ ] **Step 1: The tests.** Each renamed pin keeps its node and its input, and asserts
  `run_both(…).same(Order::AsEmitted)`:

  | was | now |
  |---|---|
  | `bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device` | `a_descending_key_with_nulls_last_agrees` |
  | `bug_a_descending_key_with_nulls_first_puts_them_last_on_the_device` | `a_descending_key_with_nulls_first_agrees` |
  | `bug_an_accumulating_sort_on_a_descending_key_nulls_first_puts_them_last_on_the_device` | `an_accumulating_sort_on_a_descending_key_nulls_first_agrees` |
  | `bug_a_merge_on_a_descending_key_nulls_first_puts_them_last_on_the_device` | `a_merge_on_a_descending_key_nulls_first_agrees` |
  | `bug_a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_duplicates_and_drops_rows_on_the_device` | `a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_agrees` |

  The `answered` id list goes with the last pin. Two new cases, beside their ascending twins:

```rust
// GpuAccumulateBatchesAndSort
fn an_accumulating_sort_on_a_descending_key_nulls_last_agrees() {
    let order = vec![by(1, false, false), by(0, true, false)];
    let runs = runs_ordered(48, 1, 3, &order);
    run_both(&sorted_by(order, None), Script::Accumulate(runs)).same(Order::AsEmitted);
}
// GpuMergeSortedPartitions
fn a_merge_on_a_descending_key_nulls_last_agrees() {
    let order = vec![by(1, false, false), by(0, true, false)];
    let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
    run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
}
```

  Each comment says what the case holds, not the defect. `key_descending_nulls_first`'s comment
  keeps its description of the keys.
- [ ] **Step 2: The mapping.** `null_order_of` in `sort.cpp`, with a comment: cuDF applies
  `null_order` before it flips a `DESCENDING` key, so "nulls first" on a descending key is
  `AFTER`; the sort and the merge must share this, or a merge over runs breaks cuDF's merge
  precondition. Both loops call it. `git grep -n null_order cpp/src` shows no other site.
  - `architecture.md`'s row: its third cell becomes "per key from the flat buffers's `asc` /
    `nulls_first`, through `null_order_of` (`sort.cpp`), which both sites call: `BEFORE` when
    `nulls_first == asc`, since cuDF applies the null order before it flips a descending key".
    The #202 link goes. The fourth cell stays.
- [ ] **Step 3: The lines and rows** as in Files. `python3 scripts/ticket_widget.py`.
- [ ] **Step 4: Run locally.**
  - `cargo build --features rust-only -p peacockdb-core --tests`: no warnings.
  - `gpu_tests` compiles only under `--features gpu`, so the changed `exec_cases.rs` and
    `accumulate_cases.rs` need their own check before the device:
    `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p
    peacockdb-core --lib --features gpu --no-run`. Expected: builds, no warnings (dropped
    imports, a dead `nulls_flipped` or `input_ordered_by`).
  - `--test test_cpu_corpus` (rust-only), skipping what needs Step 5's recordings:
    `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`,
    `duckdb_gpu_tpcds_q34_`, `duckdb_gpu_tpcds_q71_` and `duckdb_gpu_pbench_sort_`. As
    oracle-columns Task 2 Step 3 does. Expected: PASS, the registry pairing included.
- [ ] **Step 5: The device, red then green** (*Device cycle*, with the `FIX` above). Runs, one
  binary per `ssh`:
  - `$R cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 descending_key` (the five
    renamed tests, the two new ones, and the existing `…_on_a_descending_key_agrees` pair);
  - `$R cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpcds_q34_ tpcds_q71_ sort_desc_ sort_mixed_keys`
    (40 tests).

  **Red build:** the five renamed tests and the two new ones fail; q34's and q71's ten cells
  fail. Record in the detail file which of the 30 pbench cells fail and how. A pbench query
  green in the red build stays in, as coverage, and the detail file says so. Any other test
  that fails is a finding.
  **Green build:** the first run passes, with every test in `gpu_tests::accumulate_cases` and
  `gpu_tests::exec_cases` (one run each). Then the corpus run again with
  `PCK_WRITE_GPU_RESULT=1`: 40 tests pass. Then the `rsync`. `git diff --stat testdata/goldens/`
  shows only the two `gpu-result.txt` files: ten sections added in tpcds, thirty in pbench. A
  cell that fails goes back off under a new ticket, and its section is removed.
- [ ] **Step 6: Run locally** `--test test_cpu_corpus` in full, the four skipped in Step 4
  included, and `cargo test -p cost-report`. Expected: PASS.
- [ ] **Step 7: Commit** `sort: a descending key's null order is relative to its direction (#202)`

### Task 4: The wiki and the ticket

**Files:**
- Modify: `llm-wiki/build-test.md` (~697: the device corpus paragraph no longer names #202 for
  q34 and q71; ~737: the Operator harness row links `a_descending_key_with_nulls_last_agrees`;
  the counts; the six queries in the pbench list)
- Modify: `llm-wiki/tickets/corpus-coverage.md` and `tickets/testinfra.md` →
  `archive/archived-tickets.md`, `tickets.md`

- [ ] **Step 1: `build-test.md`,** as in Files. The header total and each tier's N move by what
  `--list` shows. Expected: +3 cost-report cases; +2 operator cases; +30 cpu corpus cases and +6 cpu DuckDB cases
  (`duckdb_pbench_sort_*`); +40 device corpus cases and +40 DuckDB device cases (30 pbench, 10
  for q34 and q71).
- [ ] **Step 2: Archive #202 (desc sort null order)** under Done, rewritten to what was built:
  the mapping, the five tests now asserting agreement, the two new cases, the six queries, q34
  and q71 on. Archive #258 (PR comment headroom) the same way: both remedies taken, the margin
  Task 1 recorded and the margin after Task 3. Update the index and both files' Contents.
  Any `build-test.md` sentence describing the comment's columns follows.
- [ ] **Step 3: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
- [ ] **Step 4: Commit** `wiki: #202 and #258 archived; pbench sorts on a nullable key`
