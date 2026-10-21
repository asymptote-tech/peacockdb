# oracle-cleanup implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corpus oracles named for what they compare (`duckdb(sort|no_sort, exact|approx|rel(…)[, date(…)])`,
`data_fusion(sort, …)`, `golden(sort|no_sort, …)`); `ORDER BY` answers compared row for row,
against DuckDB and, live on the device, against the cpu's section;
a `duckdb_oracle` registry column shown in `report.html`; no peacockdb-core test reads the
checkout, the source-reading tests in cost-report's `test_source_formatting`; `chunk_bytes`'s 1 GiB
in `gpu_plan.fbs`; a `null_equals_null` audit; a ticket triage.

**Architecture:** Eight tasks on one branch, in order.
1. The wiki-only triage.
2. The test-input cleanup, which removes `ticket_is_open`, so later ticket archivals cannot turn a
   case red.
3. The source-formatting binary.
4. Quoted CSV in every registry reader, together with the `duckdb_oracle` column, all before any
   cell carries a comma.
5. The grammar, still sorting everywhere.
6. Row order. `duckdb_result.py` reads the registry's `duckdb_oracle` to learn which queries keep
   their order, so the column must exist first.
7. `chunk_bytes`.
8. The `null_equals_null` audit.

**Tech stack:** Rust (peacockdb-core test harness, `inventory`, `paste`), the cost-report crate
(std only), Python 3 (`testdata/duckdb_result.py`, `scripts/ticket_widget.py`), FlatBuffers (flatc
25.12, through `flatc_fork` in Rust and CMake in C++), C++20/cuDF 25.02, DuckDB 1.5.4.

**Spec:** [`oracle-cleanup.md`](oracle-cleanup.md). Task 12 of chain K, on round-tolerance's
branch. Chain K was rebased onto master on 2026-10-20 (squashed, originals `BAK-ENS-*`), so
master's registry oracle columns, `ticket_widget.py` and `report.html` are in the base.

## Global Constraints

- **The new spellings, exactly:**
  - `duckdb(sort, exact)`, `duckdb(sort, approx)`, `duckdb(no_sort, exact)`,
    `duckdb(no_sort, approx)`;
  - `duckdb(<order>, rel(<tol>, "<column>", …))`, and any `duckdb(…)` with a trailing
    `date("<column>", …)`;
  - `data_fusion(sort, exact)`, `data_fusion(sort, approx)`;
  - `golden(sort, exact)`, `golden(sort, approx_std)`, `golden(no_sort, exact)`,
    `golden(no_sort, approx_std)`.
- **Spellings that stay:** `duckdb_fingerprint`, `duckdb_none`, `data_fusion_subset`,
  `data_fusion_disabled`, `live_cpu`, and `schema_validation_*` with its `(<modes>)`.
  `duckdb_divergent` is deleted (Task 5).
- **Tolerances keep their values:**
  - `DUCKDB_FLOAT_TOLERANCE` 1e-11, and one last-place unit for a decimal (`duckdb … approx`);
  - 1e-12 (`data_fusion … approx`);
  - `STDDEV_TOLERANCE` 1e-11 (`golden … approx_std`);
  - `rel` on q58, q61 and q66 is `1e-5`, relative to the larger magnitude.
- **Columns:** `rel` and `date` name columns as our result header prints them, as string
  literals. Positions are refused.
- **`no_sort`** compares row for row, with this comment at the comparison: "Row for row with no
  tie rule: on 2026-10-20 no corpus answer compared this way had rows tying on its sort keys
  (tpch q11 does, and is a fingerprint)."
- **Commit discipline:** every commit that changes `testdata/cost-registry.csv` or moves a ticket
  regenerates `testdata/report.html` with `python3 scripts/ticket_widget.py`.
- **No engine behaviour changes** except refusing `chunk_bytes = 0`. sf40 and wall-time
  benchmarks are out.
- **GPU work** runs on nebius-gpu through `scripts/gpu-host.sh` / `gpu-host-sync.sh` /
  `gpu-host-test.sh` (`build-test.md`, "GPU host"), in chain K's `~/peacockdb-K`.

## Review Focus

1. **A column name with spaces, dots and parentheses in `rel`.** q61's header is
   `promotional_sales.promotions / all_sales.total * Int64(100)`. Splitting on commas or
   parentheses, or stripping whitespace, must not break it. Pinned in Task 5
   (`rel_takes_a_name_with_spaces_dots_and_parentheses`).
2. **A registry cell holding commas and double quotes.** Four readers must agree. Pinned in
   Task 4, once per reader, before any cell is written quoted.
3. **A `no_sort` section read by a `sort` comparison.** `golden(sort, …)` reads a
   `mini.result.txt` section written in engine order and must sort it first. Pinned in Task 6
   (`golden_sort_sorts_a_section_written_in_engine_order`).
4. **An over-cap answer on an `ORDER BY` line.** A fingerprint has no row order, and the guard
   must not ask for `no_sort` there. Pinned in Task 6
   (`the_order_guard_skips_a_fingerprinted_section`, on tpch q11).
5. **An unbudgeted mode's `chunk_bytes`.** It is now written as the default, so `fb_text` must
   not start printing `chunk_bytes=1073741824` into every unbudgeted plan golden. Pinned in
   Task 7 (`fb_text_prints_chunk_bytes_only_when_it_is_not_the_default`).

## Before you start

- **Re-locate every site by symbol.** The line numbers below were read on master and on chain
  K's pre-rebase tip; the rebase and tasks 9–11 move them.
- **Spacing:** `stringify!` spaces its tokens (`duckdb (sort, exact)` is possible). Every
  comparison of a spelling goes through Task 5's `tight()`, which removes whitespace outside
  double quotes.
- **`testdata/report.html` may be stale on arrival.** The rebase did not run the widget. Task 1
  regenerates it first thing.
- **Detail file:** record every measured fact a restart would need in `oracle-cleanup-detail.md`.

---

### Task 1: Ticket triage that no code depends on

**Files:**
- Modify: `llm-wiki/tickets/corpus-coverage.md`, `complete-coverage.md`, `scalars.md`,
  `testinfra.md`, `llm-wiki/tickets.md`, `llm-wiki/archive/archived-tickets.md`
- Modify: every file linking `corpus-coverage.md#t264` or `complete-coverage.md#t255`
  (`git grep -n 't264\|t255'`)
- Regenerate: `testdata/report.html`

- [ ] **Step 1: Regenerate the report as it stands.** Run `python3 scripts/ticket_widget.py` and
  commit it alone if it moved: `report: regenerate after chain K's rebase`.
- [ ] **Step 2: Archive #253 (DuckDB divergence record) and #254 (untestable subset oracle)**
  under "Stale or obsolete", each with one line saying why:
  - #253: "superseded by oracle-cleanup's grammar; no line needs it";
  - #254: "one line uses it; not worth a split harness".
- [ ] **Step 3: Move #255 (nested column panic) to `scalars.md`** and #264 (computed group key)
  to `complete-coverage.md`. Move each one's anchor, header and body together. Update both
  files' `## Contents`, the index rows and counts in `tickets.md`, and every link.
- [ ] **Step 4: Shorten #258 (PR comment headroom)** to the problem (the byte count and the cap)
  and the two options.
- [ ] **Step 5: Append to archived #256** one line: "Duplicate of
  [#282](../tickets/corpus-coverage.md#t282) (scan with no row groups), the same refusal."
- [ ] **Step 6: Verify.**
  - Run `cargo test --features rust-only -p peacockdb-core --lib -- every_refusal_names_a_ticket_that_exists`
    and `cargo test -p cost-report`. Expected: PASS.
  - `python3 scripts/ticket_widget.py` prints `0 not in the wiki`.
- [ ] **Step 7: Commit** `tickets: archive #253 #254, move #255 #264, shorten #258`

---

### Task 2: No peacockdb-core test reads the checkout for its input

**Files:**
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs`:
  - delete `ticket_is_open` and `open_ticket`;
  - drop `compare_sections`'s `tickets_open` parameter.
- Modify: `duckdb_oracle/tests.rs`, and `test_support/mod.rs` (the `ticket_is_open`,
  `macro_invocations` and `corpus_lines` re-exports).
- Modify: `test_support/corpus.rs`: delete `macro_invocations`, `macro_invocations_in` if chain K
  left it, `split_arguments`, `corpus_lines` and `CORPUS_CASES`.
- Modify: `test_support/corpus/tests.rs`.
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs`: delete
  `all_modes_expands_to_the_five_in_either_position`, and make
  `every_duckdb_oracle_is_named_by_some_line` read the registry.
- Modify: `tests/test_corpus_goldens.rs`, `tests/test_corpus_goldens/benchmark.rs`.
- Move: `every_refusal_names_a_ticket_that_exists` from
  `peacockdb-core/src/planner/tests/plan_goldens.rs` to `cost-report/src/main.rs`'s test module.
- Modify: `llm-wiki/tickets/testinfra.md`, `archive/archived-tickets.md`, `tickets.md`;
  `llm-wiki/build-test.md` (~92 and ~1120, which describe the reads that go); regenerate
  `testdata/report.html`.

**Interfaces:**
- Produces: `compare_sections(oracle: &DuckdbOracle, ours: &str, duckdb: &str, kinds: &[CellKind]) -> Result<(), String>`.
- Produces: `TimedDeclaration { dataset: &'static str, query: &'static str, modes: &'static str }`
  and `LineDeclaration { dataset, query, gpu_modes, mask: &'static str }`, collected with
  `inventory` in the `test_corpus_goldens` binary; `dead_mask_entries(gpu_modes: &str, mask: &str) -> Vec<String>`.

- [ ] **Step 1: Delete `ticket_is_open`, `open_ticket`, the `tickets_open` parameter and
  `an_archived_ticket_is_not_open_and_a_listed_one_is`.** A `Divergent` that still diverges is
  `Ok(())`. Fix every caller and test that passed a closure.
- [ ] **Step 2: The two "every variant is named" tests read `registry::load_csv()`.**
  - `every_oracle_variant_is_named_by_some_line` (`test_support/corpus/tests.rs`): every
    `CpuOracle::ALL` and `GpuResultMode::ALL` variant decodes from some row's `cpu_oracle` /
    `gpu_oracle`.
  - `every_duckdb_oracle_is_named_by_some_line`: until Task 4 adds the registry column, it keeps
    reading `inventory::iter::<CorpusDeclaration>`. That is compiled in, not read from disk.
- [ ] **Step 3: The benchmark checks stop parsing the include.** The include is compiled into
  the binary instead of read at run time.
  - In `test_corpus_goldens.rs`, define `macro_rules! corpus_query_benchmark` to
    `inventory::submit!` a `TimedDeclaration`: the dataset, the query, and the modes argument
    `stringify!`ed, with `all_modes` expanded as `peacock_gpu_benchmarks.rs` expands it. Then
    `include!("common/corpus_benchmark_cases.inc")`.
  - Define `corpus_query!` there too, and `include!("common/corpus_cases.inc")`. Each line
    submits a `LineDeclaration { dataset, query, gpu_modes, mask }`, every field `stringify!`ed
    text. `mask` is the `schema_validation_*` argument's `(<modes>)`, or empty. The registry
    cannot serve the mask: its `schema_validation` holds only `enabled|disabled`.
  - `every_timed_case_is_enabled_on_a_device` iterates `TimedDeclaration` and checks against the
    `LineDeclaration` of the same query.
  - `dead_mask_entries` takes `(gpu_modes: &str, mask: &str)` instead of a parsed argument list.
    `every_mask_entry_is_one_of_its_lines_device_modes` and
    `a_mask_entry_its_line_never_runs_on_a_device_is_dead` call it with literal strings, not
    `macro_invocations_in`.
  - Keep each panic text's meaning.
- [ ] **Step 4: Delete `all_modes_expands_to_the_five_in_either_position`, `corpus_lines`,
  `macro_invocations`, `macro_invocations_in`, `split_arguments` and `CORPUS_CASES`**, and their
  re-exports.
- [ ] **Step 5: Move `every_refusal_names_a_ticket_that_exists` into cost-report.** It keeps its
  assertions. It reads `../testdata/goldens/<dataset>.sf1/<mode>.plans.txt` for the datasets and
  modes cost-report already lists, and its tickets through `TicketIndex::load`. Delete the
  peacockdb-core copy.
- [ ] **Step 6: Archive the three tickets.**
  - #252 (checkout reads remotely) under Done.
  - #266 (archived ticket accepted) under Stale: "`ticket_is_open` is gone".
  - #263 (tickets-only CI skip) under Stale, as won't fix. Its second half's workaround stays in
    the text: close and reopen the PR to force a real run.
- [ ] **Step 7: Run** `cargo test --features rust-only -p peacockdb-core --lib`, `--test
  test_cpu_corpus`, `--test test_corpus_goldens`, and `cargo test -p cost-report`. Expected:
  PASS, with the moved test listed under cost-report.
- [ ] **Step 8: Commit** `tests: no core test reads llm-wiki or the corpus include (#252)`

---

### Task 3: `test_source_formatting` in cost-report, its guard, and the coding-style rule

**Files:**
- Create: `cost-report/tests/test_source_formatting.rs` and modules under
  `cost-report/tests/test_source_formatting/` (from `test_module_layout/` and
  `test_ci_coverage/`)
- Create: `peacockdb-core/tests/test_crate_privacy.rs` (the one compile probe)
- Delete: `peacockdb-core/tests/test_module_layout.rs`, `test_module_layout/`,
  `test_ci_coverage.rs`, `test_ci_coverage/`
- Modify: `peacockdb-core/tests/test_golden_format.rs`: move
  `no_declaration_carries_a_block_left_behind_by_a_split`, `rust_sources`, `walk` and
  `blank_run_above` out.
- Modify: `scripts/build-test.sh` (the `repo_root` classifier; the `case "$base"` exclusion) and
  `.github/workflows/pipeline.yml` (the `test_ci_coverage` and `test_module_layout` steps).
- Modify: `llm-wiki/coding-style.md` (a new Antipatterns section; the mentions of
  `test_module_layout` at about 53, 74, 150 and 208), `llm-wiki/build-test.md` (the rows naming
  the moved binaries) and `llm-wiki/prompts.md` (~402 and ~429, which name
  `tests/test_ci_coverage.rs`).

**Interfaces:**
- Produces: `cargo test -p cost-report --test test_source_formatting`. Its `repo_root()` is
  `Path::new(env!("CARGO_MANIFEST_DIR")).join("..")`: the same root as before, so paths inside
  the moved tests name the same files.

- [ ] **Step 1: Move the files** with `git mv`, so history follows. `test_module_layout.rs` and
  `test_ci_coverage.rs` become modules of `test_source_formatting.rs`. Fix each path built from
  `CARGO_MANIFEST_DIR` so it still names the same file.
- [ ] **Step 2: Keep `a_private_module_is_unreachable_from_outside_the_crate` in peacockdb-core**
  as `tests/test_crate_privacy.rs`, with `compile_against_the_library` and its `Compiled` type.
  cost-report's `Cargo.toml` gains no dependency.
- [ ] **Step 3: Move `no_declaration_carries_a_block_left_behind_by_a_split`** and its helpers
  into `test_source_formatting`. It walks `peacockdb-core/src` and `peacockdb-core/tests`.
- [ ] **Step 4: Write the guard `no_core_test_reads_the_checkout`.** It walks
  `peacockdb-core/src` and `peacockdb-core/tests` and fails, naming file and line, on any line
  holding `CARGO_MANIFEST_DIR` outside `src/test_support/testdata.rs` and `build.rs`. Its
  negative case runs the same scanner over an in-memory file holding
  `Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common")` and expects one finding.
- [ ] **Step 5: CI.**
  - `pipeline.yml`: delete the two peacockdb-core steps. Add
    `cargo test -p cost-report --test test_source_formatting` beside `cargo test -p cost-report`,
    since `line_runs_target` counts only a named `--test`. Add `cargo test --features rust-only
    -p peacockdb-core --test test_crate_privacy` where `test_module_layout` ran.
  - `INTENTIONALLY_NOT_IN_CI`: the `("test_ci_coverage", NotRun)` entry is replaced by what the
    new layout needs, or removed if nothing.
  - `test_ci_coverage` also enumerates `cost-report/tests/*.rs`.
- [ ] **Step 6: `build-test.sh`.** Delete the `repo_root|\.github/workflows` grep and its
  comment. Add `test_crate_privacy` to the `case "$base"` exclusion, with the reason: "it
  invokes rustc against the crate's rlib, which a staged binary's host does not have".
- [ ] **Step 7: `coding-style.md` gains "Tests do not read source"** under Antipatterns:

  > A test does not read the checkout at run time beyond testdata: not `src/`, `tests/`,
  > `cpp/`, `scripts/`, `.github/` or `llm-wiki/`. A staged binary runs where the checkout is
  > not, and a test whose input is source text is a test of the text. A line list the test
  > needs is compiled in (`include!`), not read. cost-report is never staged, so its tests may
  > read the ticket files, and its `test_source_formatting` may read source, whose subject is
  > the source tree. `no_core_test_reads_the_checkout` holds peacockdb-core to it.

  Every other mention of `test_module_layout` in `coding-style.md` names its new home.
- [ ] **Step 8: Run.**
  - `cargo test -p cost-report --test test_source_formatting`, `cargo test -p cost-report`, and
    `cargo test --features rust-only -p peacockdb-core --test test_crate_privacy --test
    test_golden_format`. Expected: PASS.
  - Print the staged set: `bash -c 'source <(sed -n "/^rust_only_targets()/,/^}/p"
    scripts/build-test.sh); rust_only_targets'`. It holds no moved binary and not
    `test_crate_privacy`.
- [ ] **Step 9: Commit** `tests: source-reading tests move to cost-report's test_source_formatting`

---

### Task 4: Quoted CSV in every registry reader, and the `duckdb_oracle` column

**Files:**
- Modify: `peacockdb-core/src/test_support/registry.rs` (`DECLARATION_COLUMNS`, `load_csv`, a
  quote-aware split) and `test_support/mod.rs` (`CsvRow.duckdb_oracle`).
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs`:
  - `the_registry_carries_each_lines_oracles_and_schema_validation`;
  - `every_duckdb_oracle_is_named_by_some_line`, which now reads the registry.
- Modify: `cost-report/src/main.rs` (`Registry::load`).
- Modify: `scripts/ticket_widget.py` (`declarations`, `declared_cells`, the table header, the
  group-row `width`, `ORACLE_HOW`).
- Create: `scripts/test_ticket_widget.py`.
- Modify: `testdata/cost-registry.csv` (the new column, holding today's spellings);
  `.github/workflows/pipeline.yml` (run `scripts/test_ticket_widget.py`).
- Regenerate: `testdata/report.html`.

**Interfaces:**
- Produces, in every reader, a split with the same rules: RFC 4180, a field may be quoted, and
  `""` inside quotes is one `"`. A field is written quoted only when it holds a comma or a quote.
  - `registry::split_csv_line(line: &str) -> Vec<String>` in peacockdb-core;
  - a private `split_csv_line` in cost-report;
  - Python's `csv` module in `ticket_widget.py` (here) and `duckdb_result.py` (Task 6).
- Produces: header `…,tickets,duckdb_oracle,cpu_oracle,gpu_oracle,schema_validation`, and
  `CsvRow.duckdb_oracle: String` (`na` for a query with no line).

- [ ] **Step 1: Write the failing tests**, one per reader, over this row (a q61 spelling Task 5
  will write):

  `tpcds,1,q61,…,"duckdb(no_sort, rel(1e-5, ""promotional_sales.promotions / all_sales.total * Int64(100)""))","data_fusion(sort, exact)","golden(sort, exact)",enabled`

  - `registry::tests::a_quoted_field_keeps_its_commas_and_quotes`: the field reads back as
    `duckdb(no_sort, rel(1e-5, "promotional_sales.promotions / all_sales.total * Int64(100)"))`.
  - cost-report: `a_registry_row_with_a_quoted_oracle_parses`.
  - `scripts/test_ticket_widget.py::test_a_quoted_oracle_cell_is_one_field`.
  - `scripts/test_ticket_widget.py::test_a_registry_without_duckdb_oracle_still_renders`.

  `duckdb_result.py` reads no registry until Task 6, which adds its reader and its test.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement the readers.** Fix the widget's group-row `width`, which is off by one,
  as the new column makes it visible.
- [ ] **Step 4: Add the column** before `cpu_oracle`, filled with each line's DuckDB spelling as
  it is today (`duckdb_exact`, …). Extend
  `the_registry_carries_each_lines_oracles_and_schema_validation` to hold it to the line, and
  move `every_duckdb_oracle_is_named_by_some_line` onto it.
- [ ] **Step 5: The report.** `declared_cells` returns three oracle cells, DuckDB first and
  immediately before the existing cpu cell, and the header gains its `<th>`. The cell shows the
  short form and the `title` the full spelling plus `ORACLE_HOW`'s sentence.
- [ ] **Step 6: Run** `python3 scripts/ticket_widget.py`. Every query row of
  `testdata/report.html` has three oracle cells, DuckDB first. Then run `cargo test -p
  cost-report`, both Python test files, `--test test_cpu_corpus` and `--lib`. Expected: PASS.
- [ ] **Step 7: Commit** `registry: duckdb_oracle column; every reader parses quoted fields`

---

### Task 5: The oracle grammar, `rel` and `date`; `duckdb_divergent` goes

**Files:**
- Modify: `peacockdb-core/src/test_support/mod.rs` (`DuckdbOracle`), `duckdb_oracle.rs`
  (`parse`, `compare_sections`, `table`), `duckdb_oracle/tests.rs`, `corpus.rs` (`CpuOracle`),
  `device_answer.rs` (`GpuResultMode`), `registry.rs` (accepted spellings)
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs`: the macro patterns,
  `a_line_asking_datafusion_nothing_compares_its_answer_with_duckdb`,
  `an_undetermined_lines_rows_are_compared_only_against_committed_files` (it matches the literal
  spellings `duckdb_exact`, `duckdb_approx` and `golden_exact`), and the literal `golden_exact` in
  `test_gpu_corpus.rs`'s `a_device_run_under_a_regeneration_writes_no_golden`.
- Modify: `DuckdbOracle`'s doc in `test_support/mod.rs`. Columns compare by position, except
  that `rel` and `date` name theirs, as our header prints them.
- Create: `scripts/rewrite_oracles.py`
- Modify by the script: `peacockdb-core/tests/common/corpus_cases.inc` and
  `testdata/cost-registry.csv` (all three oracle columns)
- Modify: `scripts/ticket_widget.py` (`ORACLE_HOW` and the short forms for the grammar; the
  fallback's line parser, whose `[^)]*` matches no new-grammar line)
- Modify: `llm-wiki/build-test.md` (the oracle lists and counts, about 56 and 815–817; 471),
  `llm-wiki/architecture.md:840`, the include's header comment, and #251 in
  `archive/archived-tickets.md`, `tickets/corpus-coverage.md`, `tickets.md`
- Regenerate: `testdata/report.html`

**Interfaces:**
- Produces, in `test_support/mod.rs`:

```rust
pub enum RowOrder { Sort, NoSort }
pub enum Values { Exact, Approx, Rel { tolerance: f64, columns: Vec<String> } }
pub enum DuckdbOracle {
    Compare { order: RowOrder, values: Values, dates: Vec<String> },
    Fingerprint,
    None,
}
impl DuckdbOracle {
    pub fn parse(spelled: &str) -> Self;
    pub fn row_order(&self) -> RowOrder;          // Compare's order; Sort for the rest
    pub fn facets(&self) -> Vec<&'static str>;    // e.g. ["no_sort", "rel", "date"], ["fingerprint"]
    // The script writes `no_sort` for q58, q61 and q66 here, so it is a facet from this task on.
    pub const FACETS: [&'static str; 7] =
        ["sort", "no_sort", "exact", "approx", "rel", "fingerprint", "none"];
}
pub(crate) fn tight(spelled: &str) -> String;     // whitespace removed outside double quotes
```

  - **`date` is not in `FACETS`.** Interval-days, the next task, writes the first line using it.
    Until then its negative tests are what hold it.
  - **`CpuOracle`**'s variants become `SortExact`, `SortApprox`, `Subset` and `Disabled`, with
    keywords `data_fusion(sort, exact)`, `data_fusion(sort, approx)`, `data_fusion_subset` and
    `data_fusion_disabled`.
  - **`GpuResultMode`**'s variants become `Golden { order: RowOrder, approx_std: bool }` and
    `LiveCpu`. The keywords are `golden(sort|no_sort, exact|approx_std)` and `live_cpu`. `ALL`
    lists the four golden combinations and `LiveCpu`. Until Task 6, `no_sort` compares as
    `sort`.
  - **Decoding:** both decoders compare `tight(keyword)` with `tight(spelled)`.
  - **Traits:** `DuckdbOracle` is `PartialEq` only, since it holds an `f64`.
- Produces: the macro patterns `$duck:ident $(($($duck_arg:tt)*))?`,
  `$cpu_oracle:ident $(($($cpu_arg:tt)*))?` and `$gpu_oracle:ident $(($($gpu_arg:tt)*))?`, each
  passed on as `stringify!` of the whole spelling. The schema-validation arm keeps chain K's
  `$validation:ident $(($($vmode:ident)|+))?`. `CorpusDeclaration`'s fields keep their types.
- **Where the stringify happens:** outside the `$(…)+` repetition over the modes. Inside it,
  rustc refuses: "meta-variable `cpu` repeats 5 times, but `cpu_arg` repeats 1 time". So the
  arm builds each spelling once and hands it to the per-mode helper as `$x:expr`, as
  `declare_corpus_query!` and `gpu_cases!` already take theirs. `gpu_cases!`'s
  `$gpu_oracle:ident` becomes `$gpu_oracle:expr`. Task 6's DuckDB spelling for `cpu_case` and
  `gpu_case` travels the same way.

- [ ] **Step 1: Write the failing parser tests** in `duckdb_oracle/tests.rs`:

```rust
#[test] fn the_grammar_parses_each_order_and_value() {
    assert_eq!(DuckdbOracle::parse("duckdb(sort, exact)"),
               DuckdbOracle::Compare { order: RowOrder::Sort, values: Values::Exact, dates: vec![] });
    assert_eq!(DuckdbOracle::parse("duckdb (no_sort , approx)"),
               DuckdbOracle::Compare { order: RowOrder::NoSort, values: Values::Approx, dates: vec![] });
    assert_eq!(DuckdbOracle::parse("duckdb(sort, exact, date(\"next_month\"))"),
               DuckdbOracle::Compare { order: RowOrder::Sort, values: Values::Exact,
                                       dates: vec!["next_month".into()] });
}
#[test] fn rel_takes_a_name_with_spaces_dots_and_parentheses() {
    let name = "promotional_sales.promotions / all_sales.total * Int64(100)";
    assert_eq!(DuckdbOracle::parse(&format!("duckdb(no_sort, rel(1e-5, \"{name}\"))")),
               DuckdbOracle::Compare { order: RowOrder::NoSort,
                   values: Values::Rel { tolerance: 1e-5, columns: vec![name.into()] }, dates: vec![] });
}
#[test] #[should_panic(expected = "names columns by name")]
fn rel_refuses_a_position() { DuckdbOracle::parse("duckdb(sort, rel(1e-5, 2))"); }
#[test] #[should_panic(expected = "unknown duckdb_oracle")]
fn the_old_spelling_is_refused() { DuckdbOracle::parse("duckdb_exact"); }
#[test] #[should_panic(expected = "unknown duckdb_oracle")]
fn divergent_is_gone() { DuckdbOracle::parse("duckdb_divergent(251, 2)"); }
```

  And in `corpus/tests.rs` and `device_answer/tests.rs`:
  - `cpu_oracle_mode("data_fusion (sort, exact)") == CpuOracle::SortExact`;
  - `gpu_result_mode("golden(sort, approx_std)") == GpuResultMode::Golden { order: RowOrder::Sort, approx_std: true }`,
    and `golden (no_sort, exact)` parses to `{ order: NoSort, approx_std: false }`;
  - `data_fusion_refuses_no_sort`: `data_fusion(no_sort, exact)` panics, naming the accepted
    set.
- [ ] **Step 2: Run them to see them fail.**
  `cargo test --features rust-only -p peacockdb-core --lib -- duckdb_oracle::tests corpus::tests device_answer::tests`.
  Expected: compile errors naming `RowOrder` and `Values`.
- [ ] **Step 3: Write the comparison tests**, through `compare_sections`, over a two-row section
  whose header names `a` (decimal), `b` (decimal) and `d` (date):
  - `rel_passes_inside_its_tolerance`: `a` 103.719200 against 103.71926462 under
    `rel(1e-5, "a")` → `Ok`.
  - `rel_fails_just_outside_its_tolerance`: 1.0 against 1.0000200001 → `Err` naming `a`.
  - `an_unnamed_column_stays_at_approx`: `b` two last-place units off → `Err` naming `b`.
  - `rel_fails_on_a_column_the_header_does_not_have`: `rel(1e-5, "zz")` → `Err` containing `"zz"`
    and listing the header's names. Same for `date("zz")`.
  - `date_compares_the_date_part`: ours `2024-02-29` against DuckDB's `2024-02-29T00:00:00` →
    `Ok`, and against `2024-02-29T12:00:00` → `Ok`. The fixtures are written in
    `duckdb_result.py`'s `cell()` format, `%Y-%m-%dT%H:%M:%S`. Build them by calling the same
    format, not by hand.
  - `date_fails_on_another_day`: ours `2024-03-10` against `2024-03-09T12:00:00` → `Err` naming
    `d`.
  - `date_pairs_rows_under_sort`: two rows whose `d` differ only in DuckDB's time part sort and
    pair as their dates.
- [ ] **Step 4: Implement.**
  - **Parser:** it splits arguments at top-level commas, skipping quoted text.
  - **`rel`:** a cell is equal when both sides parse as `f64` and
    `|a − b| ≤ tolerance × max(|a|, |b|)`, whatever its `CellKind`.
  - **`date`:** compares the text before the `T` of DuckDB's cell with ours. Under `sort`, the
    pairing key normalises those columns the same way, so a time part cannot reorder rows.
  - **Names:** `table()` keeps the header's cells, so `rel` and `date` find columns by name.
  - **Routing:** `Compare { order: Sort, .. }` routes to `same_multiset`, as `Exact` / `Approx`
    did. `NoSort` parses, and until Task 6 replaces that `match` arm it compares as `Sort`.
  - **Deletions:** `Divergent` goes, with its parse arm, its row-level branch, its "stopped
    diverging" error and its tests.
  - **K's test:** `a_line_asking_datafusion_nothing_compares_its_answer_with_duckdb` matches
    `Compare { .. } | Fingerprint`.
  - **Coverage test:** `every_duckdb_oracle_is_named_by_some_line` checks every `FACETS` entry is
    in some registry row's `facets()`.
- [ ] **Step 5: Write `scripts/rewrite_oracles.py`.**
  - **Usage:** `--inc PATH --registry PATH [--no-sort dataset/query ...]`. It rewrites in place
    and is idempotent: it maps old spellings and re-orders new ones.
  - **DuckDB:** `duckdb_exact` → `duckdb(sort, exact)` and `duckdb_approx` → `duckdb(sort, approx)`.
    The order is `no_sort` for the queries passed in `--no-sort`, whatever the line's current
    spelling, `rel` included, and the same query's `golden(…)` takes `no_sort` too.
  - **DataFusion:** `data_fusion_exact` → `data_fusion(sort, exact)`, and
    `data_fusion_approximate` → `data_fusion(sort, approx)`.
  - **Golden:** `golden_exact` → `golden(sort, exact)`, and `golden_approx_std` →
    `golden(sort, approx_std)`.
  - **#251's lines:** `duckdb_divergent(251, …)` on tpcds q58, q61 and q66 becomes
    `duckdb(no_sort, rel(1e-5, "<name>", …))`. The names come from each query's header in
    `testdata/goldens/tpcds.sf1/mini.result.txt`, at the positions the line lists.
  - **Registry:** all three oracle cells get the same mapping, CSV-quoted. It writes with
    `csv.writer(…, lineterminator='\n', quoting=csv.QUOTE_MINIMAL)`. Python's default `\r\n`
    would rewrite every row.

  Running it twice changes nothing.
- [ ] **Step 6: Run the script.** Then update the include's header comment, `build-test.md`'s
  oracle lists and counts (and line 471's description of the DuckDB comparison), and
  `architecture.md:840`'s keywords. Update the widget's `ORACLE_HOW`, its short forms and its
  fallback parser for the grammar, and regenerate the report.
- [ ] **Step 7: Archive #251 (decimal quotient scale)** under Stale: "Replaced by
  [#288](../tickets/scalars.md#t288) (decimal division digits); its three lines are
  `rel(1e-5, …)`."
- [ ] **Step 8: Run** `--lib`, `--test test_cpu_corpus` and `--test test_corpus_goldens`
  (rust-only), the Python tests, `cargo test -p cost-report`, and `--features gpu --no-run`.
  Expected: PASS, and no golden changes.
- [ ] **Step 9: Commit** `corpus: oracles named for what they compare; rel and date; divergent goes`

---

### Task 6: `no_sort`: row order kept and compared

**Files:**
- Modify: `peacockdb-core/src/test_support/result_text.rs` (`batches_to_str`).
- Modify: `corpus.rs`: `rendered_or_fingerprint`, `result_section_body`, `cpu_case`,
  `assert_result_section`, and a new `plan_ends_in_sort`.
- Modify: `corpus_gpu.rs` (`gpu_case`, `record_gpu_result`), `device_answer.rs`
  (`device_answer_matches`) and `duckdb_oracle.rs` (`same_sequence`), with their tests.
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs` and `test_gpu_corpus.rs`: pass the DuckDB
  spelling to `cpu_case` and `gpu_case`, and add the guard.
- Modify: `testdata/duckdb_result.py` (it reads the registry: Python's `csv` module, so quoted
  cells read whole) and `testdata/test_duckdb_result.py` (its
  `test_a_quoted_oracle_cell_is_one_field`).
- Regenerate:
  - `testdata/goldens/*/duckdb-result.txt`;
  - `*mini.result.txt` (cpu, local);
  - `gpu-result.txt` (nebius-gpu);
  - `corpus_cases.inc` and the registry, by the script;
  - `testdata/report.html`.
- Modify: `llm-wiki/architecture.md` (836, "Results are compared row-sorted") and
  `build-test.md` (893 and 927–928, which golden sections keep engine order; the coverage rows
  at about 779–781). Move #287 from `tickets/corpus-coverage.md` to `archive/archived-tickets.md`
  and update `tickets.md`.

**Interfaces:**
- Consumes: Task 5's `RowOrder` and `DuckdbOracle::row_order`, and Task 4's registry
  `duckdb_oracle`.
- Produces:
  - `result_text::batches_to_str(batches: &[RecordBatch], order: RowOrder) -> String`, with
    `batches_to_sorted_str` becoming its `Sort` case;
  - `corpus::rendered_or_fingerprint(batches, order: RowOrder) -> (String, bool)`;
  - `corpus::result_section_body(batches, mode, order: RowOrder) -> String`;
  - `cpu_case(dataset, sf, query, mode, cpu_oracle, duckdb_oracle: &str)` and
    `gpu_case(…, gpu_oracle, duckdb_oracle: &str)`.
- Produces: `order_hazard::plan_ends_in_sort(plan: &Arc<dyn ExecutionPlan>) -> bool`. It walks
  the physical plan at one partition, which `order_hazard.rs` already builds. It is true when
  the root is a sort, reusing `ordering_of` (`SortExec` or `SortPreservingMergeExec`), or a
  node that `keeps_positions` over one, recursively. Also `order_hazard::ends_in_sort(dataset,
  sf, query) -> bool`, built the way `hazard_in` builds its plan.

- [ ] **Step 1: Write the failing tests:**
  - `duckdb_oracle::tests::no_sort_fails_on_two_swapped_rows`: the same two rows, swapped in ours
    → `Err` naming row 0.
  - `duckdb_oracle::tests::no_sort_passes_rows_in_the_same_order`.
  - `device_answer::tests::golden_sort_sorts_a_section_written_in_engine_order`: the section
    holds `| 2 |` then `| 1 |`, and the device answers `1`, `2` → `Ok` under
    `golden(sort, exact)`.
  - `device_answer::tests::golden_no_sort_fails_on_two_swapped_rows`: the same section and
    answer under `golden(no_sort, exact)` → `Err` naming row 0. Again under `approx_std`.
  - `device_answer::tests::golden_no_sort_passes_rows_in_the_same_order`.
  - `result_text::tests::no_sort_rendering_keeps_the_batches_order`.
  - `order_hazard::tests::plan_ends_in_sort_reads_through_limit_and_projection`, on four
    queries planned at one partition:
    - an `ORDER BY` → true;
    - `ORDER BY … LIMIT` → true;
    - a projection over `ORDER BY … LIMIT` (a subquery) → true;
    - a `GROUP BY` with no sort → false.
  - `test_duckdb_result.py::test_render_keeps_order_when_asked`: `render(names, rows,
    keep_order=True)` keeps the input order.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement.**
  - **`no_sort`:** `Compare { order: NoSort, .. }` → `same_sequence`, which pairs rows by
    position under the same cell rules. It carries the Global Constraints comment about ties.
  - **Golden comparisons:** under `golden(sort, …)`, `device_answer_matches` sorts the section's
    rows before comparing, since a section may be written in engine order. Under
    `golden(no_sort, …)` it compares the device's rows, rendered in arrival order, row for row
    with the section's: `exact` as text, and `approx_std` cell by cell within
    `STDDEV_TOLERANCE`, paired by position rather than by `sorted_str_approx`'s sort.
  - **Writers:** they take the order from the line's `DuckdbOracle::row_order()`.
  - **`duckdb_result.py`:** it keeps DuckDB's row order for every query whose registry
    `duckdb_oracle` holds `no_sort`.
  - **Coverage:** `FACETS` already holds `no_sort` (Task 5).
- [ ] **Step 4: Write the guard** `every_line_whose_query_ends_in_a_sort_says_no_sort`
  (`test_cpu_corpus.rs`), over `inventory::iter::<CorpusDeclaration>`:
  - A `Compare` line has `no_sort` exactly when `ends_in_sort` is true, and so does a line whose
    gpu oracle is `golden(…)`. A `live_cpu` line's gpu oracle is exempt.
  - A `Fingerprint` or `None` line is skipped. `the_order_guard_skips_a_fingerprinted_section`
    pins this on tpch q11.
  - The decision is a pure function, `line_order_is_right(order: RowOrder, ends_in_sort: bool)
    -> Result<(), String>`, so its failing direction has a unit test:
    `a_line_declaring_sort_over_a_sorted_plan_fails`.
  - The guard's message lists every offending line as `dataset/query`, one per line.
- [ ] **Step 5: Run the guard.** Expect it to fail, listing about 90 lines. Pass those to
  `rewrite_oracles.py --no-sort`, which sets `no_sort` on both the `duckdb(…)` and the
  `golden(…)` argument of each, and on both registry cells. Re-run until the guard passes. Record the count in the
  detail file.
- [ ] **Step 6: Regenerate.**
  - `python3 testdata/duckdb_result.py` for each dataset.
  - The cpu tier with `UPDATE_CANONICAL=1`. Only `no_sort` queries' sections may move, and only
    in row order. Check this with `git diff --stat` and a sorted-rows comparison of the old and
    new sections.
  - On nebius-gpu, a device cycle with `PCK_WRITE_GPU_RESULT=1` that runs only the `no_sort`
    queries' cases. The filter list is built from the guard's list. Running everything would let
    GPU float noise churn the other sections. Bring it home with the `rsync` of
    `value-case-impl.md`'s *Device cycle*. Never `--pull-results`: it pulls from shad-gpu.
  - **A real order defect is not fixed here.** A cell whose rows come back in the wrong order,
    on the cpu or the device, is turned off under its ticket: #202 (descending nulls on the
    wrong end) and #204 are the known ones, and anything else gets a new ticket. The line keeps
    `no_sort` and is never declared `sort` to make it pass. The detail file lists each such
    cell.
- [ ] **Step 7: Run** the cpu corpus, `--lib`, and the dispatched `GPU Tests (nebius-gpu)` CI
  job. Expected: PASS.
- [ ] **Step 8: Archive #287 (row order unchecked)** under Done. Its text is rewritten to what was
  built:
  - row for row against DuckDB, on the cpu at its authoring mode, and on the device as of its
    last recording through `gpu-result.txt`;
  - the device, live at every mode on every run, row for row against the cpu's section through
    `golden(no_sort, …)`;
  - the other four cpu modes (compared through `data_fusion` only) and `live_cpu` stay
    order-blind;
  - ties not allowed for, and the proposed key-monotonic check not taken.
- [ ] **Step 9: Commit** `corpus: ORDER BY answers kept and compared in order (#287)`

---

### Task 7: `chunk_bytes`'s 1 GiB in `gpu_plan.fbs`

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs` (`enum JoinLimits`, `CudfJoin.chunk_bytes`'s default and
  comment).
- Modify: `peacockdb-core/src/wire/mod.rs` (`join_scratch_bytes` and the re-export),
  `wire/fb_text.rs`, `wire/tests.rs`, and `executor/gpu_backend/join.rs` (`DEFAULT_CHUNK_BYTES`,
  the `0 =>` arm).
- Modify every caller that passes 0. Find them with
  `git grep -nE 'attach_recipes\(.*, 0\)|with_join_scratch\(.*0\)|PairScratch::new\(.*, 0\)|chunk_bytes = 0|chunk_bytes: 0'`
  and with these known sites:
  - `executor/gpu_backend/gpu_tests/backend.rs` (~80, ~120), `gpu_tests/exec.rs` (~298),
    `gpu_tests/mod.rs` (`Session::open`) and `gpu_tests/join.rs` (~124);
  - `gpu_tests/join.rs`'s five `PairScratch::new(…, 0)` calls (~54, ~295, ~356, ~535, ~547).
    The cases at ~535 and ~547 expect 320, which the `0 =>` arm gave, so they pass the default
    and keep their expectation;
  - `src/tests/gpu_tests/device.rs` (`Device::open`) and `src/tests/gpu_tests/script.rs`
    (`run_both`);
  - `wire/tests.rs`: `writer()` calls `with_join_scratch(0)`, and
    `a_budgeted_join_carries_its_chunk_budget` asserts 0. Both move to the default, and the
    assertion to `DEFAULT_CHUNK_BYTES`.
- Modify: `cpp/src/operators/join_residual.cpp` / `.h` (`chunks`) and `join_session.cpp` (the
  constructor).
- Modify: `cpp/tests/gpu/test_join_session.cpp`: the default at ~221, the `Form` tables at
  ~955, ~1062 and ~1371, `nested(…, chunk_bytes = 0)` at ~1018, and the `{0, 16}` loops at ~1330
  and ~1922.
- Modify: `llm-wiki/architecture.md` (the `CudfJoin` row; "a sixteenth of the budget, capped at
  1 GiB"). `join-rewrite-design.md` is frozen and is not edited.

**Interfaces:**
- Produces: `enum JoinLimits : ulong { DefaultChunkBytes = 1073741824 }` and
  `chunk_bytes: uint64 = 1073741824;`.
  - Rust: `pub(crate) const DEFAULT_CHUNK_BYTES: u64 = fb::JoinLimits::DefaultChunkBytes.0;` in
    `wire/mod.rs`. It is the one Rust name for the value, since `generated` is private to
    `wire`.
  - C++: `fb::JoinLimits_DefaultChunkBytes`.

- [ ] **Step 1: Write the failing tests:**
  - `wire::tests::the_join_limit_and_the_field_default_agree`: a `CudfJoin` built with
    `CudfJoinArgs::default()` reads back `chunk_bytes() == DEFAULT_CHUNK_BYTES == 1 << 30`.
  - `wire::tests::fb_text_prints_chunk_bytes_only_when_it_is_not_the_default`.
  - `wire::tests::an_unbudgeted_mode_writes_the_default`: `join_scratch_bytes` for
    `OneBatchPerLane` returns `DEFAULT_CHUNK_BYTES`.
  - gtest `JoinSession.AZeroChunkBudgetIsRefusedByName`: a `CudfJoin` with `chunk_bytes = 0`
    throws `std::runtime_error`, and its `what()` contains `chunk_bytes`.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement**, in order:
  1. The fbs enum and default.
  2. `join_scratch_bytes`: `Budgeted => (knobs.budget / 16).min(DEFAULT_CHUNK_BYTES)`; the
     unbudgeted modes return `DEFAULT_CHUNK_BYTES`.
  3. `fb_text` prints when the value is `!= DEFAULT_CHUNK_BYTES`.
  4. `PairScratch::new` drops its own constant and its `0 =>` arm.
  5. `JoinSession`'s constructor throws `std::runtime_error("CudfJoin: chunk_bytes = 0 …")`.
  6. `chunks` loses its `budget == 0` line, with a comment that 0 never reaches it.
  7. Every Rust caller and gtest that passed 0 passes the default:
     - `DEFAULT_CHUNK_BYTES` (Rust) or `fb::JoinLimits_DefaultChunkBytes` (C++);
     - the `{0, 16}` loops become `{DefaultChunkBytes, 16}`.
- [ ] **Step 4: Check that no hand-written copy is left.** Run
  `git grep -n '1 << 30\|1ull << 30\|1073741824' -- peacockdb-core/src cpp/src flatbuffers`.
  Expected: only `gpu_plan.fbs`'s two literals, plus unrelated `1 << 30`s, each named in the
  detail file. Known: `accumulate.rs`'s `NO_COMPACTION`, `wire/tests.rs`, `contract.rs`,
  `executor/mod.rs`. None of them may mean the join's chunk.
- [ ] **Step 5: Run.**
  - `--lib` with `UPDATE_CANONICAL` unset: plan goldens must not move.
  - The C++ cpu gtests locally.
  - On nebius-gpu: `gpu_tests::` (the `--features gpu` lib) and the join-session gtests.

  Expected: PASS.
- [ ] **Step 6: Commit** `wire: chunk_bytes's 1 GiB is gpu_plan.fbs's JoinLimits; 0 refused`

---

### Task 8: The `null_equals_null` audit

**Files:**
- Read: `peacockdb-core/src/executor/cpu_backend/join.rs`, `cpu_backend/tests/join.rs`,
  `cpp/src/operators/join_session.cpp`, `join_columns.cpp`, `cpp/tests/gpu/test_join_session.cpp`
- Modify, only to add tests: `cpu_backend/tests/join.rs`, `test_join_session.cpp`
- Write: `oracle-cleanup-detail.md` (the table); a ticket if a cell fails
- Modify only if a cell fails: `llm-wiki/architecture.md` (~488, ~1256, ~1277) and
  `gpu_plan.fbs`'s field comment

- [ ] **Step 1: Build the table** in the detail file: the nine `JoinType`s by the two engines.
  Each cell says `honoured`, `not honoured` or `untested`, with the test that shows it.
  `NullEqualsNullMatchesNullKeysForEveryType` and
  `a_null_key_matches_a_null_key_in_the_finish_pass_when_the_node_says_so` are the starting
  points. Check that each really covers every type under both flag values.
- [ ] **Step 2: For each `untested` cell, write the test**: a NULL key on both sides, under
  `true` and `false`. Name it `<engine>_<type>_null_keys_follow_null_equals_null`. Device cases
  run in Task 7's nebius-gpu round.
- [ ] **Step 3: For each failing cell, file a ticket** in `joins.md` and do not fix it:
  - the next free number (`tickets.md` says 290);
  - a label of 25 characters or fewer;
  - the failing test named, and marked `#[ignore = "#NNN"]` (Rust) or `DISABLED_` (gtest).
- [ ] **Step 4: The claim.** If every cell holds, `architecture.md`'s three sentences stay. If one
  does not, each says "except …, #NNN", and so does `gpu_plan.fbs`'s field comment.
- [ ] **Step 5: Run** the cpu join tests locally and the join-session gtests on nebius-gpu.
  Expected: PASS, apart from the tests Step 3 marked.
- [ ] **Step 6: Commit** `joins: null_equals_null audited on both engines`

---

### Closing

- [ ] **The old spellings are gone.** Run
  `rg -n 'duckdb_exact|duckdb_approx|duckdb_divergent|data_fusion_exact|data_fusion_approximate|golden_exact|golden_approx_std|ticket_is_open|corpus_lines|macro_invocations' -g '!llm-wiki/archive/**' -g '!llm-wiki/tasks/**' -g '!llm-wiki/reports/**' -g '!scripts/rewrite_oracles.py'`.
  Expected: no hits. A hit in a frozen spec or a report is history and stays.
- [ ] **Delete `scripts/rewrite_oracles.py`** in its own commit. The branch history keeps it for
  a rebase that needs it.
- [ ] **Verification:** the spec's list, item by item, with evidence in the detail file.
