# oracle-columns implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every fingerprint comparison is named by its `corpus_query!` line, its registry row and
its report cell; tpcds q2 runs on the device at all five modes; #56 (q2 fingerprinted answer) is
archived as done.

**Architecture:** No engine change.
- **The DuckDB oracle gains an optional device part**, `gpu: fingerprint`. It is a flag on
  `DuckdbOracle::Compare`. The cpu case ignores it, and the device case compares by fingerprint
  when it is set and refuses a fingerprint when it is not.
- **The registry gains a `stored` column**, held to the three golden files by a test.
- **The report** renders a Result stored column and renames the oracle columns. Each header
  says what its comparison does.

**Tech stack:** Rust test support, the corpus, the registry, `ticket_widget.py`, nebius-gpu.

**Spec:** [`oracle-columns.md`](oracle-columns.md). Fourteenth of chain K, after interval-days.

## Global Constraints

- No engine change. The cap, both writers and the fingerprint's format stay as they are.
- No fingerprint falls back silently: every fingerprinted comparison is named by its line.
- `duckdb_fingerprint`, `duckdb_none` and the other oracles keep their spellings and meaning.
- `gpu:` takes `fingerprint` only. It comes last in `duckdb(…)`, after any `date(…)`.
- `stored` sits after `tickets` and before `duckdb_oracle`. Its values: `result`,
  `fingerprint`, `na`, or three parts `cpu|duckdb|gpu`, each `result`, `fingerprint` or `—`,
  the gpu part also `mixed`. `—` appears only as a part, never as the whole cell: a corpus line
  with no section on any side is a panic in `stored_of`, and none exists today.
- Per-node, plan and cost goldens get no report column.
- `testdata/report.html` is regenerated with `python3 scripts/ticket_widget.py` in every commit
  that changes `testdata/cost-registry.csv`.
- GPU: nebius-gpu, `~/peacockdb-K`, through `value-case-impl.md`'s *Device cycle*. There is no
  fix to reverse, so one build.

## Review Focus

1. **The cpu side must not loosen.** `gpu: fingerprint` must not reach `duckdb_<ds>_<q>`. Pinned
   by Task 1's `a_fingerprinted_cpu_section_is_refused_under_a_gpu_fingerprint_line`.
2. **A device section that holds rows under `gpu: fingerprint`.** A line that keeps the part
   after its query drops under the cap would pass by fingerprint while claiming nothing. Pinned
   by Task 1's `rows_on_the_device_under_gpu_fingerprint_fail`.
3. **A `skipped:` or `failed:` section in `stored`.** It is no section, not a format. Pinned by
   Task 3's `stored_cell` cases, which take `None` for both.
4. **A registry without `stored`.** The widget's fallback must still render, as it does for a
   registry without `duckdb_oracle`. Pinned by Task 4's
   `test_a_registry_without_stored_still_renders`.
5. **`gpu:` in the wrong place.** `duckdb(sort, exact, gpu: fingerprint, date("d"))` must be
   refused, not read as a date list. Pinned by Task 1's `gpu_comes_last`.

---

### Task 1: `gpu: fingerprint` in the DuckDB oracle

**Files:**
- Modify: `peacockdb-core/src/test_support/mod.rs` (`DuckdbOracle::Compare`, `facets`, ~523-595)
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs` (`GRAMMAR`, `parse`, `judge`, a new
  `compare_device_section`)
- Test: `peacockdb-core/src/test_support/duckdb_oracle/tests.rs`

**Interfaces:**
- Produces: `DuckdbOracle::Compare { order, values, dates, gpu_fingerprint: bool }`.
- Produces: `pub(crate) fn compare_device_section(oracle: &DuckdbOracle, ours: &str, duckdb: &str,
  kinds: &[CellKind]) -> Result<(), String>` in `duckdb_oracle.rs`.
- Produces: `facets()` adds `"gpu_fingerprint"` last when the flag is set.

- [ ] **Step 1: Write the failing tests** in `duckdb_oracle/tests.rs`. The helpers `exact()`,
  `approx()` and `compare_on` gain `gpu_fingerprint: false`. A new `fp()` is
  `fingerprint_of_rendered(OURS, &[false, false])`.
  - `the_gpu_part_parses`: `parse("duckdb(no_sort, exact, gpu: fingerprint)")` is
    `Compare { order: NoSort, values: Exact, dates: [], gpu_fingerprint: true }`; with a
    `date("d")` before it, `dates == ["d"]` and the flag is set; its `facets()` is
    `["no_sort", "exact", "gpu_fingerprint"]`.
  - `the_gpu_part_takes_fingerprint_only`: `#[should_panic(expected = "gpu: takes fingerprint
    only")]` on `duckdb(sort, exact, gpu: rows)`.
  - `gpu_comes_last`: `#[should_panic]` on `duckdb(sort, exact, gpu: fingerprint, date("d"))`.
  - `the_bare_oracles_take_no_gpu_part`: `#[should_panic(expected = "unknown duckdb_oracle")]`
    on `duckdb_fingerprint(gpu: fingerprint)`, and a second test on `duckdb_none(gpu:
    fingerprint)`.
  - `a_fingerprinted_device_section_is_compared_under_gpu_fingerprint`:
    `compare_device_section(&gpu_fp(), &fp(), OURS, &KINDS) == Ok(())`, where `gpu_fp()` is
    `exact()` with the flag set.
  - `a_fingerprinted_device_section_without_the_part_fails`: under `exact()` the error
    contains ``declare `gpu: fingerprint` ``.
  - `rows_on_the_device_under_gpu_fingerprint_fail`: `compare_device_section(&gpu_fp(), OURS,
    DUCK, &KINDS)` errs containing ``drop `gpu: fingerprint` ``.
  - `a_fingerprinted_cpu_section_is_refused_under_a_gpu_fingerprint_line`:
    `compare(&gpu_fp(), &fp(), DUCK, &KINDS)` errs containing `declare duckdb_fingerprint`.
  - `the_device_side_under_duckdb_fingerprint_is_unchanged`:
    `compare_device_section(&DuckdbOracle::Fingerprint, &fp(), OURS, &KINDS) == Ok(())`.
- [ ] **Step 2: Run** `cargo test --features rust-only -p peacockdb-core --lib
  test_support::duckdb_oracle`. Expected: FAIL to compile on `gpu_fingerprint`.
- [ ] **Step 3: Implement.**
  - `GRAMMAR` reads `duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>",
    …)][, gpu: fingerprint])|duckdb_fingerprint|duckdb_none`.
  - `parse` accepts up to two arguments after the values. A tight `gpu:fingerprint` is the last
    one; `gpu:<other>` panics with ``{spelled}: gpu: takes fingerprint only, got `<other>` ``.
    Anything else in last place goes to `unknown()`.
  - `compare_device_section`: under `Compare { gpu_fingerprint: true, .. }`, a fingerprinted
    `ours` goes to `fingerprint::compare_over_cap(ours, duckdb, DUCKDB_FLOAT_TOLERANCE)`, and
    rows err with ``the line declares `gpu: fingerprint` and the device's section holds rows —
    drop `gpu: fingerprint` ``. Under `Compare { gpu_fingerprint: false, .. }`, a fingerprinted
    `ours` errs with ``the device's section is fingerprinted (over the cap) — declare `gpu:
    fingerprint` ``. Every other case delegates to `compare_sections`.
  - `judge` calls `compare_device_section` where `mode.is_some()`, else `compare_sections`.
  - The doc on `DuckdbOracle::Compare` names the flag and says only the device case reads it.
- [ ] **Step 4: Run** the Step 2 command. Expected: PASS. Then `--test test_cpu_corpus`
  (rust-only). Expected: PASS. `FACETS` stays unchanged here on purpose:
  `every_duckdb_oracle_is_named_by_some_line` fails on a facet no line uses, so
  `"gpu_fingerprint"` joins `FACETS` in Task 2, in the commit that adds q2's line.
- [ ] **Step 5: Commit** `duckdb oracle: an optional gpu: fingerprint part for the device side`

### Task 2: q2 on the device

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`: `tpcds, 1, q2` and the comment above
  it that names #56; the grammar comment at the top (lines 10–11); the tpcds "THE DEVICE
  COLUMN" comment (lines 131–133)
- Modify: `peacockdb-core/src/test_support/mod.rs` (`DuckdbOracle::FACETS`: 9 entries,
  `"gpu_fingerprint"` last, in this commit because q2's line is its first user)
- Modify: `testdata/cost-registry.csv` (row 3), `testdata/report.html`
- Regenerate: `testdata/goldens/tpcds.sf1/gpu-result.txt`

**Interfaces:**
- Consumes: Task 1's `gpu_fingerprint` flag.

- [ ] **Step 1: The line.** `corpus_query!(tpcds, 1, q2, all_modes, all_modes, duckdb(no_sort,
  exact, gpu: fingerprint), data_fusion(sort, exact), golden(no_sort, approx_std),
  schema_validation_enabled);`. Its comment says the device's `round(…, 2)` cells print one ulp
  longer (#60), so the device's rendering crosses the cap and is fingerprinted.
  - The grammar comment's `duckdb(…)` line reads
    `duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>", …)][, gpu:
    fingerprint])`, and one sentence says `gpu: fingerprint` compares the device's side by
    fingerprint and leaves the cpu's under the rest of the spelling.
  - The tpcds "THE DEVICE COLUMN" comment becomes true for this tree: every tpcds line runs on
    the device wherever it runs on the cpu, but two: q34 and q71, off under #202 (desc sort null
    order). It no longer names q2 or #56.
- [ ] **Step 2: The row.** Five `gpu_*` cells `enabled`; `tickets` empty; `duckdb_oracle`
  `"duckdb(no_sort, exact, gpu: fingerprint)"`; `gpu_oracle` `"golden(no_sort, approx_std)"`.
  Add `"gpu_fingerprint"` to `FACETS`. Then `python3 scripts/ticket_widget.py`.
- [ ] **Step 3: Run locally** `--test test_cpu_corpus` (rust-only), skipping
  `duckdb_gpu_tpcds_q2_` and `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`.
  `duckdb_tpcds_q2`, `every_duckdb_oracle_is_named_by_some_line` and
  `the_registry_matches_the_cpu_corpus_in_both_directions` must pass. Expected: PASS.
- [ ] **Step 4: The device.** One build, then
  `$G ssh "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpcds_q2_"`
  and the `rsync`. Expected: 5 tests ran, 5 passed. `git diff --stat testdata/goldens/` shows
  only `tpcds.sf1/gpu-result.txt`, five sections added, each opening `fingerprint: rows=2513`.
  Never `--pull-results`: it pulls from shad-gpu.
- [ ] **Step 5: Run locally** `--test test_cpu_corpus` in full. Expected: PASS, the five
  `duckdb_gpu_tpcds_q2_*` cases included. Record in the detail file each mode's largest relative
  gap on a float triple, read with a throwaway script. Round-tolerance measured 2.5e-14.
- [ ] **Step 6: Commit** `corpus: tpcds q2 on the device, its DuckDB side by fingerprint`

### Task 3: the registry's `stored` column

**Files:**
- Modify: `peacockdb-core/src/test_support/registry.rs` (header, `CsvRow`, validation, a new
  `Stored` and `stored_cell`) and `registry/tests.rs`
- Modify: `peacockdb-core/src/test_support/corpus_golden.rs` (a new `pub(crate) fn stored_of`)
  and `peacockdb-core/src/test_support/mod.rs` (a `pub fn stored_of` wrapper calling it, as
  `result_golden` is wrapped at `mod.rs:762`). A `pub` item in `corpus_golden.rs` would break
  cost-report's `a_components_api_is_declared_in_its_mod_rs`, and a `pub use` of a
  `pub(crate)` item does not compile.
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs` (the new test)
- Modify: `cost-report/src/main.rs` (`a_registry_row_with_a_quoted_oracle_parses`'s fixture)
- Modify: `testdata/test_duckdb_result.py` (`test_a_quoted_oracle_cell_is_one_field`)
- Modify: `testdata/cost-registry.csv`, `testdata/report.html`

**Interfaces:**
- Produces: `pub(crate) enum Stored { Result, Fingerprint }`.
- Produces: `pub(crate) fn stored_cell(cpu: Option<Stored>, duckdb: Option<Stored>, gpu:
  &[Stored]) -> String` in `registry.rs`. `gpu` holds one entry per device mode with a section.
  It panics when no side has a section (`None`, `None`, `&[]`), naming the case.
- Produces: `pub(crate) fn stored_of(dataset: &str, sf: &str, query: &str) -> String` in
  `corpus_golden.rs`, and `pub fn stored_of` with the same signature in `test_support/mod.rs`.
  It reads `result_golden`, `duckdb_golden` and `gpu_result_golden(…, None)`. A `skipped:`
  section, a DuckDB `failed:` section and an absent one are `None`. A comment says a corpus line
  with no section on any side panics through `stored_cell`, and that none exists today.
- Produces: `pub(crate) fn stored_mismatch(dataset: &str, query: &str, got: &str, want: &str)
  -> Option<String>` in `registry.rs`: `None` when they agree, else the mismatch line below.
- Produces: `CsvRow.stored: String`.

- [ ] **Step 1: Write the failing tests.** In `registry/tests.rs`, `stored_cell_names_each_shape`,
  with `R = Stored::Result` and `F = Stored::Fingerprint`:
  - `(Some(R), Some(R), &[R; 5])` → `"result"`; `(Some(R), Some(R), &[])` → `"result"`;
  - `(Some(F), Some(F), &[F; 5])` → `"fingerprint"`;
  - `(Some(R), Some(R), &[F; 5])` → `"result|result|fingerprint"`;
  - `(Some(F), Some(R), &[])` → `"fingerprint|result|—"`;
  - `(Some(R), Some(R), &[R, F])` → `"result|result|mixed"`.

  `stored_cell_refuses_a_query_with_no_section`: `#[should_panic(expected = "no section on any
  side")]` on `(None, None, &[])`.

  `a_wrong_stored_cell_names_its_side`:
  - `stored_mismatch("tpcds", "q2", "result|result|result", "result|result|fingerprint")` is
    `Some` and contains `tpcds/q2`, `the registry says result|result|result` and
    `(gpu differs)`;
  - `stored_mismatch("tpch", "q1", "result", "fingerprint")` contains `(cpu differs)`;
  - `stored_mismatch("tpcds", "q2", "result|result|fingerprint", "result|result|fingerprint")`
    is `None`.

  In `test_cpu_corpus.rs`, `the_registry_stored_column_matches_the_goldens_in_both_directions`:
  - each row whose four declaration columns are `na` has `stored == "na"`;
  - each other row goes through `stored_mismatch(…, &row.stored, &stored_of(dataset, sf,
    query))`;
  - each query with a section in any of the three files has a registry row.

  A mismatch is one line: `{dataset}/{query}: the registry says {got}, the goldens say
  {want}`, followed by `({side} differs)` naming the first of cpu, duckdb, gpu whose part
  differs once both are read as three parts. All are collected, then asserted together.

  In `testdata/test_duckdb_result.py`, `test_a_quoted_oracle_cell_is_one_field`: the fixture's
  header gains `stored` after `tickets`, each row a value for it, and a q2 row
  `tpcds,1,q2,,result|result|fingerprint,"duckdb(no_sort, exact, gpu: fingerprint)",
  "data_fusion(sort, exact)"`. The assertion becomes `{"q61", "q2"}`.
- [ ] **Step 2: Run** `--lib test_support::registry` and `--test test_cpu_corpus
  the_registry_stored`. Expected: FAIL to compile.
- [ ] **Step 3: Implement.**
  - The header check expects `"stored"` after `"tickets"`. The declared slice starts one later.
  - A row parses `stored`. It must be `na` exactly when the four declaration columns are `na`,
    else one word or three parts as Global Constraints says. The error names the row.
  - Fill the CSV column with a throwaway script, run once and not committed. q2 reads
    `result|result|fingerprint`. Then `python3 scripts/ticket_widget.py`.
  - cost-report's `Registry::load` and `duckdb_result.py` read by header name and need no code
    change. Their fixtures gain the column, so each proves it reads past it.
- [ ] **Step 4: Run** the Step 2 commands, then `cargo test -p cost-report` and `python3
  testdata/test_duckdb_result.py`. Expected: PASS.
- [ ] **Step 5: Commit** `registry: a stored column, held to the goldens`

### Task 4: the report's columns

**Files:**
- Modify: `scripts/ticket_widget.py` (`ORACLE_HOW`, `oracle_view`, `declarations`,
  `declared_cells`, the header build in the table rewrite, `width`, the `<style>` block)
- Test: `scripts/test_ticket_widget.py`
- Regenerate: `testdata/report.html`

**Interfaces:**
- Consumes: the registry's `stored` column (Task 3) and q2's spelling (Task 2).
- Produces: `declarations(root)` maps `(dataset, query)` to `(stored, duck, cpu, gpu,
  validation)`. `stored` is `None` on a registry without the column or from the include fallback.
- Produces: `split_gpu(spelled) -> tuple[str, str | None]`: `("duckdb(no_sort, exact)",
  "fingerprint")` for q2's spelling, `(spelled, None)` for every other one.

- [ ] **Step 1: Write the failing tests** in `test_ticket_widget.py`. First the three existing
  `Registry` tests move to the five-tuple and four oracle cells:
  - `test_a_quoted_oracle_cell_is_one_field`: the `tree` header gains `stored` before
    `duckdb_oracle` and the row `result`; `decl == ("result", Q61, "data_fusion(sort, exact)",
    "golden(no_sort, exact)", "enabled")`; `oracles.count("<td") == 4`.
  - `test_a_registry_without_duckdb_oracle_still_renders`: `decl == (None, None,
    "data_fusion_exact", "golden_exact", "enabled")`; `oracles.split("</td>")[:4]` has 4 cells;
    `>—` in cells 0 and 1; `>exact` in cell 2.
  - `test_the_fallback_reads_a_line_on_the_grammar`: `decl == (None, Q61, "data_fusion(sort,
    exact)", "golden(no_sort, exact)", "enabled")`.
  - `test_no_declaration_renders_four_dashes`: `tw.declared_cells(None)[0].count("<td") == 4`,
    every one `—`. The 20 rows with no declaration must not render one cell short.

  Then the new ones:
  - `test_q2s_two_cells_split`: a registry row with `stored` `result|result|fingerprint` and q2's
    spelling. `declared_cells` returns HTML with two `class="decl split"` cells. The first holds
    three `class="half"` spans labelled `cpu`, `DuckDB` and `gpu`. The second holds two, `cpu`
    with `no_sort exact` and `gpu` with `fingerprint`. The `gpu` half's `title` contains
    `SHA-256`.
  - `test_a_one_word_stored_cell_does_not_split`: `stored` `fingerprint` renders one plain `decl`
    cell reading `fingerprint`.
  - `test_a_registry_without_stored_still_renders`: a registry without the column renders `—` in
    the stored cell, and the four other cells as today.
  - `test_each_header_says_what_it_compares`: the rewritten table's first header row holds
    `Result stored`, `DuckDB`, `cpu ↔ DataFusion` and `gpu ↔ cpu`, each with a `title` equal to
    its `HEADER_HOW` text.
  - `test_the_fingerprint_text_names_what_it_compares`: `ORACLE_HOW["duckdb_fingerprint"]` and
    `FINGERPRINT_HOW` each contain `sum, min and max`, `1e-11`, `decimals included`, `SHA-256`
    and `Row order is not checked`.
- [ ] **Step 2: Run** `python3 scripts/test_ticket_widget.py`. Expected: FAIL.
- [ ] **Step 3: Implement.** The texts are fixed copy:
  - `FINGERPRINT_HOW = "Compared by fingerprint: the row count; each column's count of values;
    each float column's sum, min and max within 1e-11 relative; every other column, decimals
    included, through a SHA-256 over all rows' cells, exactly. Row order is not checked."`
  - `ORACLE_HOW["duckdb_fingerprint"] = "Both stored answers are over the size cap. " +
    FINGERPRINT_HOW`.
  - `STORED_HOW`: `result` → "The stored section holds the rows.", `fingerprint` → "The answer
    is over the 256 KiB cap: the stored section holds its fingerprint.", `—` → "No section is
    stored for this side.", `mixed` → "The device modes stored different formats."
  - `HEADER_HOW`:
    - `Result stored`: "What each side's committed section holds: the rows, or a fingerprint
      over the 256 KiB cap. The cpu's answer must equal its own section text for text: the same
      rows in the line's row order with no tolerance, or the same fingerprint text. Split into
      cpu, DuckDB and gpu where the sides differ."
    - `DuckDB`: "The cpu's stored answer, and each device mode's stored answer, against
      DuckDB's (duckdb-result.txt). A split cell means the two sides compare differently."
    - `cpu ↔ DataFusion`: "The cpu's answer at each mode against a live DataFusion run at
      target_partitions = 1."
    - `gpu ↔ cpu`: "The device's answer, in memory, against the cpu's stored section (golden)
      or a live cpu run at the same mode (live_cpu)."
  - A split cell is one `<td class="decl split">` holding one `<span class="half"
    title="…"><sub>label</sub> short</span>` per side. A `<style>` rule stacks the halves
    (`td.decl.split .half {{ display: block; }}`).
  - `declared_cells(None)` returns four dash cells, not three.
  - The header: `side(text)` takes the `HEADER_HOW` title. The order after Query is Result
    stored, DuckDB, cpu ↔ DataFusion, gpu ↔ cpu. `width` gains one.
  - Then `python3 scripts/ticket_widget.py`.
- [ ] **Step 4: Run** the Step 2 command. Expected: PASS. Open `testdata/report.html` and
  confirm q2's row shows both splits and a `duckdb_fingerprint` row shows `fingerprint` twice.
- [ ] **Step 5: Commit** `report: Result stored and the comparison columns, each header saying
  what it compares`

### Task 5: the wiki, #56, and the final runs

**Files:**
- Modify: `llm-wiki/build-test.md`
- Modify: `llm-wiki/tickets/corpus-coverage.md` → `llm-wiki/archive/archived-tickets.md` (#56),
  `llm-wiki/tickets.md`

- [ ] **Step 1: `build-test.md`.**
  - The oracle grammar names `gpu: fingerprint`: the device side by fingerprint, the cpu side
    under the rest of the spelling, and refusals both ways.
  - The registry section names `stored`, its values and the test that holds it.
  - The report's description names the five columns after Query.
  - The test counts follow the new tests.
- [ ] **Step 2: Archive #56** under Done. Its text says what was built: q2's line declares `gpu:
  fingerprint`, its five device cells are on, and the largest gap from Task 2 Step 5. Drop it
  from `corpus-coverage.md`'s Contents and the `tickets.md` index, and fix the counts.
- [ ] **Step 3: Run** `cargo test --features rust-only -p peacockdb-core --lib`, `--test
  test_cpu_corpus`, `--test test_corpus_goldens`, `cargo test -p cost-report`, `python3
  scripts/test_ticket_widget.py` and `python3 testdata/test_duckdb_result.py`. Expected: PASS.
- [ ] **Step 4: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
- [ ] **Step 5: Commit** `oracle-columns: #56 archived; build-test.md`
