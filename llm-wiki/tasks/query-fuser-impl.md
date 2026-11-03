# query-fuser implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** fuse the tp1_single plans of the first 3/10/20/all queries of each dataset's batch set
(sf40-baseline's H100 `cases.csv`, read at run time) into one
DAG whose scans of a file are one shared `GpuLoadParquet`. Where a reader of a shared scan sits
inside a hold, buffer it or re-read the table, by a recorded rule. Run each batch on the H100
under a memory-derived concurrency cap, check every answer, and record load, execute, peak,
cap, reads per table and two hints (merge, pipeline), with an Nsight profile per batch and a
fused-batches table on bench-rows' report page.

**Architecture:** a prototype module `peacockdb-core/src/prototypes/fuser/`, inside a
`prototypes` component built only with `test-support` (its only caller is the benchmark
harness), which no engine file may name. `fuse` rebuilds each query's tree. Every
`GpuLoadParquet` of one file becomes a clone of one `plan::SharedSource` (dag-driver) over the
union load. Each consumer reads it through a column-ref `GpuProject` (zero-copy on both
backends: `cpp/src/operators/project.cpp:31-37`, Arrow's `project`) and through its own filters.
dag-driver's validation names every edge where a shared source enters a hold from outside; for
each, the fuser asks a caller-supplied chooser for a buffer (`GpuHoldBuffer` on one edge of the
reader's path) or a re-read (the reader's own load back). Plain functions read the sf40-baseline
tree through bench-rows' `parse_benchmark_section` and the H100 `loads.tsv`: `merge_hint`,
`pipeline_hint`, `cap_for`, and the hold rule (`place_buffer`, `decide`). The harness,
`prototypes/fuser/benchmark.rs`, runs a batch with dag-driver's `run_roots` and writes
`tp1_single.fused.txt`.

**Tech stack:** Rust (peacockdb-core, test_support, the `prototype_fused_benchmarks` and
`test_corpus_goldens` binaries, cost-report's `test_source_formatting`), bash
(`create_nsys_profile.sh`), Python 3 stdlib for the script test, Nsight Systems on nebius-h100.

**Spec:** [`query-fuser.md`](query-fuser.md). Siblings consumed: [`dag-driver-impl.md`](dag-driver-impl.md)
(`SharedSource`, `PlanDag`, `validate_dag`, `validate_cap`, `PlanError::UnbufferedHold`,
`new_hold_buffer`, `run_roots`, `FusedRun`, `estimate_roots`, `attach_recipes_for_roots`,
`Session::open_roots`, `render_timings_of_roots`),
[`bench-rows-impl.md`](bench-rows-impl.md) (node and trailer format, `parse_benchmark_section`,
`BenchSection`, `BenchNode`, `RunTotals`, `run_section`, `timed_load_us`, `LOAD_NODE`),
[`pinned-loads-impl.md`](pinned-loads-impl.md) (`nsys_loads.py`, `loads.tsv`),
[`sf40-baseline-impl.md`](sf40-baseline-impl.md) (the committed `.benchmark.txt` trees and
`testdata/calibration/h100-80gb-hbm3/<ds>.sf40/loads.tsv`),
[`new-hosts-impl.md`](new-hosts-impl.md) (`run-gpu.sh`, `create_nsys_profile.sh --ssh`),
[`containers-rebase-impl.md`](containers-rebase-impl.md) Task 4 (`DeviceCase`, `CaseStatus`,
`device_cases_path`, `parse_device_cases`: the H100 `cases.csv`'s one reader), bench-rows Task 4
(`render_benchmark_report`, `ReportTable`, `ReportCell`, `section_href`,
`the_benchmark_report_matches_the_trees`).

## Global Constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- Branch `ENS-query-fuser` on top of `ENS-dag-driver`: master's engine plus containers' harness
  and scripts (`ENS-containers-rebase`), bench-rows, pinned-loads, sf40-baseline and dag-driver.
  The harness files (`corpus_benchmark.rs`, `benchmark_trailer.rs`, `benchmark_section.rs`,
  `record.rs`, `peacock_gpu_benchmarks.rs`, `create_nsys_profile.sh`) are containers'. Line
  numbers below are `origin/ENS-containers`'s, so find each by the function named beside it.
- Prototype: the coordinator skips the reviewer and the PR. The tests are the item-6 answer
  checks, one unit test per hint's arithmetic, the cap rule and the hold rule, and the
  source-formatting gate. Nothing more.
- **Prototype code lives apart from the engine.** Everything this task writes in Rust, but
  for the engine helpers it names (`plan/rebuild.rs`, `row_group_metadata`, `node_line_of`,
  `nodes_as_recorded_of_roots`, `timed_load_us_of_roots`, `append_records_to`), is under
  `peacockdb-core/src/prototypes/fuser/`, compiled only under `test-support`. No file outside
  `prototypes/` and the tests names `prototypes::` (Task 1's gate).
- **sf40 only, no fallback.** The hold rule and the cap read sf40-baseline's files; a batch at
  any other scale is refused by `benchmark_fused`, and a missing baseline node or `loads.tsv`
  row panics naming it. The sf1 CPU answer checks (spec item 6) pass the fuser an explicit
  test chooser, never a guessed figure.
- No planner change. Every existing golden stays byte-identical: plan, `.cpu.txt`, `.cost.txt`,
  `recipe-payloads.txt`, the device tier and the committed benchmark trees.
- Only sources are merged. The merge hint is a report, not an execution.
- Mode: tp1_single only (`MODES[0]`, one lane, `Batching::Off`). `fuse` refuses
  `Batching::Sized` with `PlanError::Unsupported`.
- Device results directory: `h100-80gb-hbm3`, the harness's `device_slug` of the card.
- `peak_bytes` is a **lower bound** on device memory (bench-rows "The format"). The cap's margin
  exists because of that, and every report of it says "accountant's peak".
- coding-style.md: files < 1000 lines; each component's API is declared in its `mod.rs` through
  one-expression delegates; tests go in `foo/tests.rs`; rustfmt on the touched leaf files only.
- Hosts: nebius-h100 (`computeinstance-e00e6yena4nzjgaym0`, profile `editor-eu`), runs as
  `dmitry`. Every ssh from the Bash tool needs the sandbox off. A VM this task starts is stopped
  by it. Never open `~/.nebius/*` or any `*.pem`.

**Golden check** (the last step of every code task):

```
cargo test --features rust-only -p peacockdb-core --lib                       # all pass
cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens  # all pass
cargo test -p cost-report --test test_source_formatting                        # layout gates, prototypes gate
git status --short testdata/                                                    # empty
```

## Review Focus

1. **A scan cut by a pushed limit** (`SELECT … FROM t LIMIT n`, no filter): the translator kept
   only the covering prefix of row groups (`planner/translator/nodes.rs:563-569,612`). No filter
   rejects the extra groups, so the scan must stay unshared, with the reason given. → Task 1,
   `a_limit_cut_scan_is_not_shared`.
2. **A consumer whose own row groups had no NULL in a column where the union's groups do.**
   Null handling was planned from `can_be_null` (`plan/mod.rs:906-910`), and that is a statistic
   of the survivors. The scan stays unshared, reason `nulls`. → Task 1,
   `a_scan_whose_columns_gain_nulls_is_not_shared`.
3. **A rows-only scan** (`count(*)`, empty projection, `__rowmarker__` schema) beside scans with
   columns. A column selection cannot make the marker, so it shares only with rows-only scans.
   → Task 1, `a_rows_only_scan_shares_only_with_rows_only_scans`.
4. **A held reader whose smallest recorded edge lies above a join that grew the rows** (a
   fan-out inner join): the buffer must stay below that join, or it would keep more than the
   numbers said. → Task 5, `a_buffer_stays_below_a_join_that_grew_the_rows`.
5. **A batch whose shared sources and buffers alone exceed the budget:** the cap is 1,
   `over_budget=true`, and the run still goes ahead. Running out of memory then writes a section
   with its cap. It is not a failed case. → Task 5, `shared_bytes_over_budget_give_cap_one_and_say_so`;
   Task 6, `an_out_of_memory_error_is_recognised`.
6. **The H100 `cases.csv` missing from the tree the fused binary runs in** (it is read at run
   time now, not compiled in): a batch must not silently run an empty or default list. → Task 2,
   `ordered_queries_without_the_list_panic_naming_it`; Task 9 Step 2 checks the shipped file.

---

### Task 1: the fuser shares scans, and resolves every held reader

**Files:**
- Create: `peacockdb-core/src/plan/rebuild.rs`. Move `rebuild` and `scan_of` out of
  `src/tests/rebuild.rs:37-163` verbatim. The fixtures and `rebuild_tree` stay and call
  `crate::plan::rebuild`. Declare it in `plan/mod.rs` as `pub(crate) mod rebuild;`.
- Modify: `planner/translator/scan_mapping/parquet_meta.rs:34-140`. Extract the per-group loop
  into `pub(crate) fn row_group_metadata(path: &str, projected: &[usize], keep: Option<&[u32]>) -> Result<ScanMetadata, PlanError>`.
  `survivor_metadata` calls it. Declare it in `planner/mod.rs` so the fuser can reach it.
- Create: `peacockdb-core/src/prototypes/mod.rs` — the component. In `lib.rs`:
  `#[cfg(feature = "test-support")] pub mod prototypes;`, beside `test_support`. Its module doc:
  what a prototype is (code that measures an idea, not code that ships), that `fuser` runs at
  sf40 only, and that no engine module may depend on anything here. It declares
  `pub(crate) mod fuser;` and, from Task 6, the `pub` delegates the integration tests call.
- Create: `prototypes/fuser/mod.rs` (module doc: query-fuser, sf40 only, the detail file),
  `prototypes/fuser/fuse.rs`, `prototypes/fuser/fuse/tests.rs`.
- Create: `cost-report/tests/test_source_formatting/prototypes.rs`, declared in
  `cost-report/tests/test_source_formatting.rs`.
- Test: `prototypes/fuser/fuse/tests.rs` (hand-built plans, `src/tests/rebuild.rs` fixtures, the
  mock backend) and `src/tests/end_to_end/fused.rs` (declared in `end_to_end.rs`, sf1 CPU).
- Docs: `llm-wiki/architecture.md` — a new top-level "Prototypes" section after "Execution",
  and its line in "Contents".

**Interfaces:**
- Consumes: `SharedSource::new(load: GpuLoadParquet)`, `validate_dag`, `PlanDag::of`,
  `PlanError::UnbufferedHold { hold, reader, slot, producer, what }`, `new_hold_buffer(input)`,
  `run_roots::<B>(roots, ctx, budget, cap) -> Result<FusedRun, RunError>` (dag-driver Tasks 2,
  4, 5); `partition(&[RowGroupMeta], usize, Batching)` (`scan_mapping/partition.rs:20`).
- Produces (declared in `prototypes/fuser/mod.rs`):

```rust
pub(crate) struct Query<'a> { pub(crate) name: String, pub(crate) tree: &'a dyn GpuNode }
pub(crate) enum Unshared { LimitCut, Nulls { column: String }, RowsOnly }   // Display: `limit-cut`, `nulls:<col>`, `rows-only`
pub(crate) struct ShareGroup { pub(crate) table: String, pub(crate) consumers: Vec<usize> /* query ordinals, repeats allowed */ }
/// One edge where a shared source enters a hold from outside, as dag-driver's validation names it.
pub(crate) struct HeldReader {
    pub(crate) query: usize,                // root ordinal
    pub(crate) group: usize,                // ShareGroup ordinal
    pub(crate) hold: usize,                 // fused pre-order of the join (or the waiting root, Task 5)
    pub(crate) path: Vec<(usize, usize)>,   // edges (parent, slot), fused pre-order, from the one above
                                            // the source (or above its inserted project) up to the
                                            // hold's entry: (join, PROBE_CHILD), or (root, 0)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice { ReRead, Buffer { edge: (usize, usize) /* one of path */ } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoldDecision { pub(crate) choice: Choice, pub(crate) bytes: u64, pub(crate) reread_us: u64, pub(crate) why: String }
pub(crate) struct HoldRecord { pub(crate) query: usize, pub(crate) table: String, pub(crate) hold: String /* node_line_of the hold's node, name only */, pub(crate) above: Option<usize> /* origin pre-order of the buffer's child */, pub(crate) decision: HoldDecision }
pub(crate) struct Fused {
    pub(crate) roots: Vec<Box<dyn GpuNode>>,            // one per query, in input order
    pub(crate) shared: Vec<ShareGroup>,                  // file order of first appearance
    pub(crate) unshared: Vec<(usize, String, Unshared)>, // (query ordinal, table, why)
    pub(crate) holds: Vec<HoldRecord>,                   // in the order they were resolved
    pub(crate) reads: Vec<(String, usize)>,              // per table, file order: distinct loads of it in the DAG
    pub(crate) origins: Vec<Vec<Origin>>,                // by fused PlanDag pre-order index
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Origin { pub(crate) query: usize, pub(crate) node: usize /* pre-order index in that query's own tree */ }
pub(crate) type Chooser<'c> = dyn FnMut(&Fused, &HeldReader) -> HoldDecision + 'c;
pub(crate) fn fuse(queries: &[Query<'_>], batching: Batching, choose: &mut Chooser<'_>) -> Result<Fused, PlanError>;
pub(crate) fn resolve_cap_holds(fused: Fused, cap: NonZeroUsize, choose: &mut Chooser<'_>) -> Result<Fused, PlanError>;
#[cfg(test)] pub(crate) fn re_read_every_hold(_: &Fused, _: &HeldReader) -> HoldDecision;   // why = "test: re-read"
#[cfg(test)] pub(crate) fn buffer_every_hold_lowest(_: &Fused, h: &HeldReader) -> HoldDecision; // Buffer at path[0], why = "test: lowest edge"
```

Rules (a function each in `fuse.rs`):
- **Grouping:** loads are grouped by `GpuLoadParquet.file`, across queries and within one. A load
  is unshared when its parent is a `GpuLimit`, or a `GpuUnload` with `interval.is_some()` (the
  two shapes `covering_prefix` leaves). A rows-only load joins only rows-only loads. A group of
  one stays as it was.
- **Union load:** the projection is the sorted union of file ordinals. The survivors are the
  union by `index`, in file order, re-read through `row_group_metadata(file, union, Some(indices))`
  so `bytes` and `can_be_null` are the union's. `partition_groups = partition(&groups, 1,
  batching)`, with lanes = 1 (`[[]]` when no survivor, as `nodes.rs:572-581` does). The schema
  takes each union ordinal's field from a member that projects it.
- **Null check:** after the union, a member is dropped from the group, with reason
  `Nulls { column }`, if one of its columns has `can_be_null` false in its own load and true in
  the union. Then the union is recomputed without it. Repeat until stable.
- **Consumers:** a member whose projection equals the union reads `Box::new(shared.clone())`.
  Any other member reads `GpuProject::new(shared, exprs, member_schema)`, where each expr is
  `Expr::column(position_in_union, file_column_name)` and the name is kept. Every other node is
  rebuilt with `plan::rebuild::rebuild`.
- **Held readers (spec 1a):** after sharing, loop: `validate_dag(&roots, cap)` (the cap the fuser will run with, so capped roots are settled too); on
  `UnbufferedHold { hold, reader, slot, producer, .. }` build the `HeldReader` (the path climbs
  from the crossing edge, or from the edge above an inserted project when `reader` is one, to
  the hold's entry; inside a region every node has one reader, so the path is unique), call
  `choose`, apply, record, and validate again. Any other error, `HoldCycle` included, ends `fuse` with that error. Apply:
  - `ReRead`: the edge above the source (or above its inserted project) gets a clone of that
    member's own original `GpuLoadParquet`; the member leaves the group's `consumers`. A group
    left with one consumer keeps its `SharedSource` (one reader is legal).
  - `Buffer { edge }`: `new_hold_buffer` is inserted on that edge.
  A buffer placed higher than the crossing shrinks the hold, so a later named edge is never one
  an earlier choice already covered; the loop ends because each pass removes one named edge.
- `resolve_cap_holds`: the same loop over `validate_cap(roots, Some(cap))` (dag-driver Task 7).
- **Reads (spec 1b):** `reads[table]` = distinct `GpuLoadParquet` nodes of that table in
  `PlanDag::of(roots)` — a shared load once, plus one per re-read and per unshared scan.
- **Origins:** walk each original tree and its fused root together in pre-order. An inserted
  project or buffer has no origin. A shared load collects one origin per member; a re-read load
  has its member's.

- [ ] **Step 1: failing tests** (`prototypes/fuser/fuse/tests.rs`). Plans are hand-built over
  the `rebuild.rs` fixtures' tables, plus one real file for re-reading metadata:
  `testdata/tpch.sf1/lineitem.parquet`. Every call passes `&mut re_read_every_hold` unless named.
  - `two_scans_of_one_file_become_one_shared_load`: q0 projects `{0,4}` and keeps groups
    `{0,1}`; q1 projects `{4,5}` and keeps `{1,2}`. Result: `shared.len() == 1`; the load's
    projection is `[0,4,5]` and its survivors are `[0,1,2]`; `PlanDag::of(roots)` has one node
    with two readers; `reads == [("lineitem", 1)]`.
  - `each_consumer_reads_its_own_columns`: q0's child is a `GpuProject` whose exprs are
    `[col(0,"l_orderkey"), col(1,"l_quantity")]` and whose schema equals q0's original load's.
    `a_consumer_with_the_union_projection_reads_the_source_directly`.
  - `a_limit_cut_scan_is_not_shared`: `GpuUnload{interval: Some}` directly over a load, and
    `GpuLimit` over a load. Both appear in `unshared` with `LimitCut`; `shared` is empty.
  - `a_scan_whose_columns_gain_nulls_is_not_shared`: q1's load has `can_be_null[c] = false` and
    the union's is `true`. It appears in `unshared` with `Nulls { column: "<name>" }`.
  - `a_rows_only_scan_shares_only_with_rows_only_scans`: three loads of one file, two rows-only
    and one with columns. One group of two, and the third is unshared with `RowsOnly`.
  - `two_scans_of_one_file_in_one_query_share`: one query, a join over two loads of one file,
    with `buffer_every_hold_lowest`. One shared group, `consumers == [0, 0]`, one hold record.
  - `a_held_reader_is_re_read`: q0 = `unload(join(b, filter(L)))`, q1 = `unload(filter(L))`:
    one `HoldRecord` with `ReRead`, q0's filter reads its own load, `reads == [(b's table, 1), ("lineitem", 2)]`,
    and the fused roots validate.
  - `a_held_reader_is_buffered_where_the_chooser_says`: the same plans with a chooser returning
    `Buffer { edge: path[1] }` (above the filter): a `GpuHoldBuffer` sits there,
    `reads` lineitem `1`, `above == Some(<q0's filter pre-order>)`.
  - `the_chooser_sees_each_named_edge_once`: two held readers in one hold; a counting chooser
    is called twice with distinct paths.
  - `origins_map_every_fused_node_back`: for every non-inserted fused node, the node's `name()`
    equals the origin node's.
  - `sized_batching_is_refused`.
  - `fused_roots_answer_as_each_alone` (mock backend, `driver/tests/mock.rs`): each
    `answers[i]` equals the same query's `run` alone, for both test choosers.
  - cost-report (`prototypes.rs`): `no_engine_file_names_prototypes` — over `tree::sources()`,
    skipping files whose first component is `prototypes`, any path with a component in
    `walls::TEST_DIRS`, and files named `tests.rs`: `code_only(read(rel))` contains no
    `prototypes::`; the failure lists every offending file. `the_gate_catches_a_use` — the
    private matcher it uses is true for `"use crate::prototypes::fuser;"` and
    `"crate::prototypes::fuser::fuse(…)"`, false for `"// prototypes::"` after `code_only`.
    `prototypes_build_only_with_test_support` — in `lib.rs` the line before
    `pub mod prototypes;` is `#[cfg(feature = "test-support")]`.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- prototypes::fuser::` → FAIL to compile.
- [ ] **Step 3:** implement as above. `rustfmt` the leaf files.
- [ ] **Step 4:** the new tests pass. `-- tests::rebuild planner::` pass unchanged.
  `cargo test -p cost-report --test test_source_formatting` passes; adding a scratch
  `use crate::prototypes::fuser;` to `executor/mod.rs` makes `no_engine_file_names_prototypes`
  fail naming that file (then revert it).
- [ ] **Step 5: sf1 on the CPU** (`src/tests/end_to_end/fused.rs`):
  `tpch_q3_q5_q10_fused_answer_as_alone`, once with each test chooser. Plan each with
  `plan_at("tpch","1",q,&MODES[0])` (`test_support/corpus.rs:48`), fuse, and
  `run_roots::<CpuBackend>` with no budget and no cap. Each `answers[i]`, as
  `batches_to_sorted_str`, equals that query's `run_cpu` answer. With
  `buffer_every_hold_lowest`, `shared` names `lineitem`, `orders` and `customer`. Run
  `cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::fused` → PASS.
- [ ] **Step 6:** golden check. `architecture.md` "Prototypes" (new section, short):
  prototype code lives in `peacockdb-core/src/prototypes/<name>/`, compiled only under
  `test-support` as `test_support/` is, so `cargo build` cannot name it; each prototype's module
  doc says what it is and the scale it runs at; **no file outside `prototypes/` and the tests
  names `prototypes::`**, which `cost-report`'s `test_source_formatting/prototypes.rs` enforces.
  Then one entry: `fuser` (query-fuser) — fuses N tp1_single plans into one DAG on dag-driver,
  shares only sources, refuses three shapes (limit-cut, nulls, rows-only), buffers or re-reads
  every held reader, sf40 only; pointer to the detail file. Commit
  `fuser: one shared load per file across N plans`.

### Task 2: the ordered lists, and fused answers against the corpus goldens

**Files:**
- Create: `prototypes/fuser/lists.rs`, `prototypes/fuser/lists/tests.rs`
- Test: `src/tests/end_to_end/fused.rs`

**Interfaces:**
- Consumes: sf40-baseline's one ordered list, the H100 tree's
  `testdata/benchmark-results/h100-80gb-hbm3/cases.csv` (containers-rebase Task 4's format: `ok`
  and `wrong` rows are cases, a `skipped` row is not). Read at run time through
  `parse_device_cases(&read(device_cases_path("h100-80gb-hbm3")))`, by the lib's tests and by
  `prototype_fused_benchmarks` alike; nothing compiles it in. No other path or reader is written.
- Produces (declared in `prototypes/fuser/mod.rs`):
  - `pub(crate) fn batch_set(dataset: &str, cases: &[DeviceCase]) -> Vec<String>`: in file
    order, `dataset`'s `ok` and `wrong` rows whose modes carry `tp1_single`, their queries; for
    tpch only q1-q22 (`^q([1-9]|1[0-9]|2[0-2])$`: the micro queries are not batched). Panics
    naming the dataset when the result is empty.
  - `pub(crate) fn ordered_queries(dataset: &str) -> Vec<String>`: `batch_set` over the H100
    file, which panics naming the path when the file is missing. The one entry point: the lib's
    tests and `benchmark_fused` both call it.
  - `pub(crate) const BATCH_SIZES: [(&str, Option<usize>); 4] = [("first3", Some(3)), ("first10", Some(10)), ("first20", Some(20)), ("all", None)];`
  - `pub(crate) fn batches(list: &[String]) -> Vec<(&'static str, Result<Vec<String>, String>)>`.
    `Err(marker)` is `"same set as all: the list holds {n} queries"` for a sized batch with
    `n <= size`, other than `first3`, which is always run.

- [ ] **Step 1: failing tests.** `batch_set_takes_tp1_single_rows_in_file_order` (tpch rows
  `q2` `ok`, `aggregate_groupby` `ok`, `q1` `ok` at `tp1_single|tp4_sized`, `q5` `ok` at
  `tp4_sized` only, `q9` `skipped`, `q13` `wrong` `#7`, tpcds `q3` `ok` → tpch `[q2, q1, q13]`,
  tpcds `[q3]`). `ordered_queries_without_the_list_panic_naming_it` (with `PEACOCK_TESTDATA_DIR`
  at an empty temp dir: the panic names `h100-80gb-hbm3/cases.csv`). `batches_take_prefixes_of_the_list` (25 names: lengths
  3/10/20/25). `a_short_list_marks_first20_as_all` (18 names: `first20` is `Err` containing
  `"18"`, and `all` has 18).
  `fused_batches_answer_the_corpus_goldens_on_the_cpu`: for tpch and tpcds at sf1 tp1_single,
  the `first10` batch of `ordered_queries` is fused with `buffer_every_hold_lowest` and run with
  `run_roots::<CpuBackend>`. Each root's answer meets its `.result.txt` section,
  `section_of(result_golden(ds,"1"), q)`, through `result_matches(section, author_mode, order, batches)`.
  `author_mode` is the `mode=` line of the section, and `order` is the one the CPU corpus tier
  uses for that query. A query whose section holds no rows (a marker) is compared against its
  own unfused `run_cpu` answer instead, and the test names it.
- [ ] **Step 2:** run `-- tests::end_to_end::fused prototypes::fuser::lists` → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** PASS. Record the wall time in the detail file. If it is over 120 s, mark the
  test `#[ignore = "sf1 corpus, run with --ignored"]` and name the command in build-test.md.
- [ ] **Step 5:** golden check. build-test.md gets the new rows and counts. Commit
  `fuser: the ordered lists and fused answers against the corpus goldens`.

### Task 3: the mergeability hint

**Files:**
- Create: `prototypes/fuser/merge.rs`, `prototypes/fuser/merge/tests.rs`, `prototypes/fuser/baseline.rs`, `prototypes/fuser/baseline/tests.rs`
- Modify: `plan_text/mod.rs`, which adds `pub(crate) fn node_line_of(node: &dyn GpuNode) -> String`
  delegating to `node_text::node_line` (now `pub(super)`)

**Interfaces:**
- Consumes: `Fused`, `Origin` (Task 1); `PlanDag` (dag-driver); `BenchSection`, `BenchNode`,
  `parse_benchmark_section` (bench-rows Task 3).
- Produces:
  - `pub(crate) struct Baseline { pub(crate) per_query: Vec<QueryTimes> }` and
    `pub(crate) struct QueryTimes { pub(crate) names: Vec<String>, pub(crate) children: Vec<Vec<usize>>, pub(crate) total_us: Vec<u64>, pub(crate) batch_rows: Vec<u64> /* Σ over lanes and batches */, pub(crate) batch_bytes: Vec<u64> /* Σ over lanes and batches */, pub(crate) load_us: u64, pub(crate) execute_us: u64, pub(crate) peak_bytes: u64 }`
    (by pre-order index). `Baseline::of(sections: &[BenchSection<'_>]) -> Baseline` panics
    when a section lacks `batch_rows`, `batch_bytes`, `load_us`, `execute_us` or `peak_bytes`.
    `Baseline::check(&self, queries: &[Query]) -> Result<(), String>` requires each query's
    pre-order names to equal its section's, naming the first mismatch.
  - `pub(crate) struct MergeEntry { pub(crate) node: String /* node_line_of */, pub(crate) members: Vec<Origin> /* subtree roots */, pub(crate) saved_us: u64 }`
  - `pub(crate) struct MergeHint { pub(crate) entries: Vec<MergeEntry>, pub(crate) saved_us: u64 }`
  - `pub(crate) fn merge_hint(fused: &Fused, baseline: &Baseline) -> MergeHint`

Model, written as the doc comment of `merge_hint`:
- Each fused node above a shared source gets a key, computed bottom-up: `node_line_of(node)`
  plus the children's keys. A `SharedSource` is keyed by its `sharing_key()`. A
  `GpuHoldBuffer` is transparent: its key is its child's, since it computes nothing.
- A **group** is two or more distinct fused nodes with one key whose subtree reaches a shared
  source. An **entry** is a group that no other group of the same size contains member for
  member (each member's parent is in it).
- A member's time `t_m` is Σ baseline `total_us` over its subtree's origin nodes. Loads,
  inserted projects and buffers are excluded. Then entry `saved_us = Σ t_m − max t_m`: one copy
  stays, the dearest one.
- The batch's `saved_us = Σ over every group of (Σ own total_us − max own total_us)`. Each
  node is in one group, so nothing is counted twice.
- Entries are sorted by `saved_us`, largest first.

- [ ] **Step 1: failing tests.** `identical_filters_over_one_source_are_one_entry` (two queries
  `filter(p, S)`, baseline filter times 30 and 50: one entry, `saved_us == 30`, and its members
  name both origins). `different_predicates_are_no_entry`. `only_the_largest_identical_subtree_is_an_entry`
  (`agg(filter(S))` twice: one entry, the aggregate, with `saved_us` = both nodes' sum minus the
  larger member's). `a_nested_group_of_fewer_members_is_counted_once_in_the_total` (filter
  shared by 3, aggregate above it by 2: the total is the filter group's plus the aggregate
  group's, node by node). `a_buffer_does_not_split_a_group` (`filter(p, buffer(S))` beside
  `filter(p, S)`: one entry). `a_baseline_of_another_plan_is_refused` (names differ: `check` is
  `Err` naming the query and index).
- [ ] **Step 2:** run `-- prototypes::fuser::merge prototypes::fuser::baseline` → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** PASS.
- [ ] **Step 5:** golden check. Commit `fuser: the mergeability hint`.

### Task 4: the pipelining hint

**Files:** Create `prototypes/fuser/pipeline.rs`, `prototypes/fuser/pipeline/tests.rs`

**Interfaces:**
- Produces:

```rust
pub(crate) struct RootNeeds { pub(crate) execute_us: u64, pub(crate) sources: Vec<usize> }
pub(crate) struct PipelineHint {
    pub(crate) load_order: Vec<usize>,   // source ordinals, in the order the model loads them
    pub(crate) serial_us: u64,           // Σ loads + Σ executes
    pub(crate) makespan_us: u64,
    pub(crate) saved_us: u64,            // serial_us − makespan_us
}
pub(crate) fn pipeline_hint(load_us: &[u64], roots: &[RootNeeds]) -> PipelineHint;
pub(crate) fn pipeline_inputs(fused: &Fused, baseline: &Baseline) -> (Vec<u64>, Vec<RootNeeds>);
```

Model, written as the doc comment:
- One loader and one executor, each sequential, running at the same time.
- A root starts once its last source is loaded and the executor is free.
- Load order is greedy: next comes the root whose missing sources have the least total load
  time (ties go by root order), and its missing sources are appended in fused pre-order.
- Roots execute in the order they become ready.
- `pipeline_inputs`: a source is every distinct `GpuLoadParquet` in the fused DAG, so a
  re-read is a source of its own. A shared source's load time is the **largest** baseline
  `total_us` among its origin load nodes. The union reads at least as much as any member, so
  this is a lower bound. A re-read's is its origin's. A root's `execute_us` is its baseline
  trailer's `execute_us`.
- Against the fused run (Task 6): `pipelined_estimate_us = (fused load_us + fused execute_us)
  saturating_sub saved_us`.

- [ ] **Step 1: failing tests.** `the_cheapest_root_loads_first`: loads `[100, 10]`, roots
  `{50, [0]}` and `{20, [1]}`. Expect `load_order == [1, 0]`, `serial_us == 180`,
  `makespan_us == 160`, `saved_us == 20`. `a_shared_source_is_loaded_once`: loads `[40]`, two
  roots needing `[0]` with execute 10 and 10. Expect `serial_us == 60`, `makespan_us == 60`,
  `saved_us == 0`. `execution_waits_for_the_executor`: loads `[5, 5]`, roots `{100, [0]}` and
  `{1, [1]}`. Expect makespan `106`. `inputs_take_the_largest_member_load`.
  `a_re_read_is_a_source_of_its_own` (a fused pair with one re-read: 2 sources for one table).
- [ ] **Step 2:** run `-- prototypes::fuser::pipeline` → FAIL. **Step 3:** implement. **Step 4:** PASS.
- [ ] **Step 5:** golden check. Commit `fuser: the pipelining hint`.

### Task 5: the memory rules — buffer or re-read, where a buffer goes, and the concurrency cap

**Files:** Create `prototypes/fuser/holds.rs`, `prototypes/fuser/holds/tests.rs`,
`prototypes/fuser/loads.rs`, `prototypes/fuser/loads/tests.rs`, `prototypes/fuser/cap.rs`,
`prototypes/fuser/cap/tests.rs`

**Interfaces:**
- Consumes: `Fused`, `HeldReader`, `HoldDecision`, `Choice`, `Chooser`, `resolve_cap_holds`
  (Task 1); `Baseline` (Task 3); `testdata/calibration/h100-80gb-hbm3/<ds>.sf40/loads.tsv`
  (pinned-loads format, committed by sf40-baseline Task 10); `attach_recipes` (each query's own
  tree, for its loads' `CudfScan` seqs).
- Produces:

```rust
// loads.rs
pub(crate) struct LoadTimes { /* (query, recipe_seq) → load_us */ }
pub(crate) fn read_load_times(path: &Path, dataset: &str, sf: &str, mode: &str) -> LoadTimes;  // skips the `#` preamble; keeps rows of that case
impl LoadTimes { pub(crate) fn load_us(&self, query: &str, recipe_seq: u64) -> u64; }        // panics naming the key: no fallback

// holds.rs
pub(crate) struct EdgeCost { pub(crate) edge: (usize, usize), pub(crate) bytes: u64, pub(crate) allowed: bool }
pub(crate) struct HoldInputs { pub(crate) reread_us: u64, pub(crate) edges: Vec<EdgeCost> /* path order, lowest first */ }
pub(crate) fn place_buffer(edges: &[EdgeCost]) -> Option<&EdgeCost>;
pub(crate) fn decide(inputs: &HoldInputs, headroom_bytes: u64) -> HoldDecision;
pub(crate) fn hold_inputs(fused: &Fused, held: &HeldReader, baseline: &Baseline, loads: &LoadTimes, seqs: &[Vec<Option<u64>>] /* per query, CudfScan seq by pre-order */, names: &[String]) -> HoldInputs;
pub(crate) struct HoldLedger<'b> { /* budget, baseline, loads, seqs, names, Σ buffer bytes so far */ pub(crate) pool_bytes: u64 }
impl<'b> HoldLedger<'b> { pub(crate) fn choose(&mut self, fused: &Fused, held: &HeldReader) -> HoldDecision; }

// cap.rs
pub(crate) const MARGIN_PERCENT: u64 = 10;
pub(crate) struct CapInputs { pub(crate) pool_bytes: u64, pub(crate) shared_bytes: Vec<u64>, pub(crate) buffer_bytes: Vec<u64>, pub(crate) root_bytes: Vec<u64> }
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CapDecision { pub(crate) cap: NonZeroUsize, pub(crate) budget_bytes: u64, pub(crate) estimate_bytes: u64, pub(crate) over_budget: bool }
pub(crate) fn cap_for(inputs: &CapInputs) -> CapDecision;
pub(crate) fn cap_inputs(fused: &Fused, baseline: &Baseline, pool_bytes: u64) -> CapInputs;
pub(crate) fn scaled_bytes(measured: u64, plan_from: u64, plan_to: u64) -> u64;   // measured × to / from, in u128; from == 0 gives measured
pub(crate) fn settle(fused: Fused, baseline: &Baseline, ledger: &mut HoldLedger<'_>) -> Result<(Fused, CapDecision), PlanError>;
```

**The hold rule** (spec 1a), written as the doc comment of `decide` and in the detail file:
- Recorded numbers are the sf40-baseline tree's, per origin node: Σ `batch_bytes`, Σ
  `batch_rows`. An edge's bytes are its child's origin's Σ `batch_bytes`; an inserted project's
  are its member's origin load's. The edge directly above a shared source carries the union,
  which nothing recorded, so the path starts above it (or above the inserted project).
- **Placement** (`place_buffer`): on the path from the shared scan up to the held join's probe
  input, the allowed edge whose recorded `batch_bytes` is smallest; a tie goes to the lower
  edge, which runs less early. An edge is **allowed** when every join below it on the path has
  recorded output rows (Σ `batch_rows`) ≤ the recorded rows of its child on the path: the join
  did not increase cardinality there. Every join type is checked; inner is the one that can
  fail it.
- **Cost of a buffer:** `bytes` = the edge's bytes + Σ, over the joins below it on the path, of
  their build child's recorded Σ `batch_bytes`: the buffer runs those joins early, so their
  build tables are live too.
- **Cost of a re-read:** `reread_us` = `loads.tsv` `load_us` of the reader's own origin load
  (`(query, recipe_seq)`, the seq of its `CudfScan` recipe), the same quantity as the tree's
  `GpuLoadParquet` `total_us` (pinned-loads). It is recorded either way.
- **The choice** (`decide`): `Buffer` at the placed edge when one exists and
  `bytes ≤ headroom_bytes`; else `ReRead`. `why` is `"fits: {bytes} <= {headroom}"`,
  `"too large: {bytes} > {headroom}"` or `"no edge: a join below every edge grew the rows"`.
- **Headroom** (`HoldLedger::choose`): `budget_bytes − (S + B + max root_bytes)`, saturating,
  where `S` and `root_bytes` are `cap_inputs` of the fused plan at its first call and `B` is the
  bytes of the buffers chosen so far. So readers are decided in the order validation names
  them, and each buffer spends what it keeps.
- **The cap is a hold too** (dag-driver Task 7): with cap k < N, roots k.. wait, and a shared
  source they read needs a buffer or a re-read. `settle` loops: `cap_for(cap_inputs(…))`; if
  `cap < N`, `resolve_cap_holds(fused, cap, ledger)`, recompute the cap; until it is stable
  (the cap only falls, so it ends, at 1 at the latest).

**The cap rule**, written as the doc comment of `cap_for` and in the detail file:
- `budget = pool_bytes − pool_bytes × MARGIN_PERCENT / 100`. The pool is the rmm pool the run
  installed: `RmmPool::Pool.maximum_bytes`, 69 GiB unless `PEACOCK_RMM_POOL_BYTES` says
  otherwise. That pool is the device memory the engine may hold. The margin covers what the
  accountant's peak leaves out (call scratch, cuDF temporaries, fragmentation).
- `S = Σ shared_bytes` and `B = Σ buffer_bytes` are **charged whatever the cap**. The cap bounds
  executor state only. A buffer keeps its batches until its hold lifts, whether the hold is a
  join's or the cap's. `S` stays charged in full, though no queue holds a source's batches any
  more (dag-driver 3b): readers above a source may still accumulate it, and the full charge
  keeps the rule an upper bound.
- Take `root_bytes` in descending order. `cap` is the largest k for which `S + B + Σ top k ≤
  budget`, and at least 1. `estimate_bytes = S + B + Σ top cap`. `over_budget = S + B +
  max(root_bytes) > budget`.
- `cap_inputs`, per shared group: take the member with the largest baseline load `batch_bytes`
  (Σ), and scale it from that member's planned bytes (Σ `survivors.bytes` of its original
  load) to the union load's planned bytes. Per buffer: its `HoldRecord.decision.bytes`. Per root:
  baseline `peak_bytes` minus Σ baseline `batch_bytes` of its loads that are now shared,
  saturating (a re-read load is not shared, so it is not subtracted).

- [ ] **Step 1: failing tests.** holds: `the_buffer_goes_on_the_smallest_recorded_edge` (edges
  bytes `[100, 20, 50]`, all allowed → edge 1); `a_tie_goes_to_the_lower_edge`;
  `a_buffer_stays_below_a_join_that_grew_the_rows` (path filter → inner join → project, the
  join's rows 10 over a probe child's 5, the project's edge smallest: placed below the join);
  `a_buffer_above_a_join_charges_its_build` (edge 30 above a join with build 40, rows not grown
  → `bytes == 70`); `a_buffer_that_fits_is_chosen` (headroom 100, bytes 70: `Buffer`, why
  `"fits: 70 <= 100"`); `a_buffer_that_does_not_fit_means_a_re_read` (headroom 60: `ReRead`, why
  `"too large: 70 > 60"`, `reread_us` from the inputs); `no_allowed_edge_means_a_re_read`;
  `the_ledger_spends_headroom_in_order` (two held readers of 60 each, headroom 100: `Buffer`,
  then `ReRead`); `hold_inputs_read_the_baseline_and_loads_tsv` (a hand-built fused pair, a
  `Baseline` from a literal `.benchmark.txt` section, a literal `loads.tsv`). loads:
  `a_loads_tsv_reads_back` (preamble skipped, two cases, one kept); `a_missing_load_panics_naming_it`.
  cap: `the_cap_admits_the_largest_roots_that_fit`: pool 100, shared `[30]`, buffers `[]`, roots
  `[40, 20, 10]`. Expect `CapDecision { cap: 2, budget_bytes: 90, estimate_bytes: 90, over_budget: false }`.
  `every_root_fits`: shared `[0]`, roots `[1, 1, 1]`, pool 100. Expect cap 3.
  `shared_bytes_over_budget_give_cap_one_and_say_so`: shared `[95]`, roots `[1]`, pool 100.
  Expect cap 1 and `over_budget`. `shared_bytes_count_whatever_the_cap`: shared `[60]`, roots
  `[20, 20]`, pool 100. Expect cap 1 and estimate 80 (60 + 20 + 20 = 100 > 90).
  `buffer_bytes_count_whatever_the_cap`: shared `[30]`, buffers `[25]`, roots `[30, 10]`, pool
  100. Expect cap 1, estimate 85 (55 + 30 + 10 = 95 > 90), not over budget.
  `scaling_is_by_planned_bytes` (`scaled_bytes(100, 50, 75) == 150`, and no overflow at
  `u64::MAX / 2`). `a_root_is_charged_its_peak_less_its_shared_loads`.
  `settling_buffers_the_roots_that_wait` (mock plans: three roots over one source, a cap that
  comes out at 2: root 2's reader gets a decision, the result validates under `validate_cap`).
- [ ] **Step 2:** run `-- prototypes::fuser::holds prototypes::fuser::loads prototypes::fuser::cap` → FAIL. **Step 3:** implement. **Step 4:** PASS.
- [ ] **Step 5:** golden check. Commit `fuser: buffer or re-read, and the concurrency cap rule`.

### Task 6: the fused benchmark case and its file

**Files:**
- Create: `prototypes/fuser/benchmark.rs`, `prototypes/fuser/benchmark/tests.rs`,
  `prototypes/fuser/section.rs`, `prototypes/fuser/section/tests.rs`
- Modify: `prototypes/mod.rs` (the `pub` delegates below, which the integration tests call as
  `peacockdb_core::prototypes::…`)
- Modify: `test_support/corpus_benchmark.rs` (`second_smallest`, `measured_of` and
  `assert_fit_to_measure` become `pub(crate)`; no behaviour change), `test_support/record.rs`
  (`pub(crate) fn append_records_to(path: &Path, rows, meta)`, which `append_records` calls with
  the env path)
- Modify: `executor/driver/index.rs:138` + `executor/mod.rs`, adding `nodes_as_recorded_of_roots(roots)`
  (`nodes_as_recorded` delegates); `plan_text/bench_text.rs` + `plan_text/mod.rs`, adding
  `timed_load_us_of_roots(roots, times)` (`timed_load_us` delegates)
- Create: `peacockdb-core/tests/prototype_fused_benchmarks.rs` (the 8 cases; a test target of its
  own, not `peacock_gpu_benchmarks`); Modify `peacockdb-core/Cargo.toml` (`[[test]] name =
  "prototype_fused_benchmarks"`, `required-features = ["gpu", "test-support"]`, which keeps it
  out of every CPU mode's build)
- Modify: `scripts/env/stage-install.sh` and `scripts/env/run-gpu.sh` (stage and run it by name),
  `scripts/env/tests/test_gpu.py`
- Modify: `cost-report/tests/test_source_formatting/ci_coverage.rs` (the wiring guard's
  "manual, prototype" entry)
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` (the fused-tree check), `.gitignore`
  (`testdata/calibration/fused/`)
- Docs: `llm-wiki/build-test.md` "Benchmarks" (the fused cases, file and command);
  `llm-wiki/tasks/query-fuser-detail.md` (the format below, as spec item 7 requires)

**Interfaces:**
- Consumes: Tasks 1-5; `Session::open_roots`, `run_roots`, `attach_recipes_for_roots`,
  `render_timings_of_roots(roots, times, report)` (dag-driver Tasks 5, 9, 10); `RunTotals`,
  `run_section` (bench-rows Task 2); `measure_into`, `Resume`, `MEASURED_RUNS` (= 10),
  `record_rows`, `declared_steps`, `rows_match_the_recipes`, `of_this_run`, `device_slug`,
  `install_rmm_pool`, `nvtx_range`.
- Produces (declared in `prototypes/mod.rs`, delegating to `fuser`):
  - `pub async fn benchmark_fused(dataset: &str, sf: &str, batch: &str)`. It panics unless
    `sf == "40"`: `"the fuser runs at sf40 only: its rules read sf40-baseline's files"`. Its
    tests are `bench_fused_{tpch,tpcds}_sf40_{first3,first10,first20,all}_tp1_single`, made by a
    `fused_benchmark!` macro in `prototype_fused_benchmarks.rs`. Its queries are the batch's
    entry of `batches(&ordered_queries(dataset))` (Task 2), read from the H100 `cases.csv` when
    the test runs, so the test names are fixed and the list is not compiled in.
  - `prototype_fused_benchmarks.rs`: the `fused_benchmark!` macro and its eight invocations,
    nothing else: no case macro, no `include!`.
  - `stage-install.sh`: `cargo test --no-run --release --features gpu,test-support --test
    prototype_fused_benchmarks` into `cpp/install/rust-benchmarks/`, stripped as the others.
    `run-gpu.sh <vm options> run prototype_fused_benchmarks`: runs that staged binary by name in the run
    image with `PCK_TEST_FILTER` and `PEACOCK_BENCH_RESUME`, `--nocapture --test-threads=1`, as
    `run bench` runs `peacock_gpu_benchmarks`.
  - No CI step runs or stages it. `ci_coverage.rs` gains `Exemption::ManualPrototype` ("manual,
    prototype: run by hand through `run-gpu.sh <vm options> run <target>`") and the entry
    `("prototype_fused_benchmarks", Exemption::ManualPrototype)` in `INTENTIONALLY_NOT_IN_CI`.
    VERIFIED: no workflow line names the target (no `--test`, no staging array entry), and
    `build-test-shadgpu.sh`'s `RUST_TESTS` does not name it.
  - `pub fn fused_results_file(device: &str, dataset: &str, sf: &str) -> PathBuf` =
    `benchmark-results/<device>/fused/<ds>.sf<sf>/tp1_single.fused.txt`. The extension keeps it
    out of the `.benchmark.txt` checks.
  - Record: `testdata/calibration/fused/<ds>.sf<sf>/records.tsv`, written on every clean run
    (gitignored; the metrics pass reads it).
  - `pub const FUSED_ALL_RUNS: usize = 3;`. The `all` batch measures 3 runs after the warm-up,
    and reports the 2nd-min. The others measure `MEASURED_RUNS`.
  - `pub(crate) fn is_out_of_memory(error: &RunError) -> bool` (in `fuser`): the text contains,
    ignoring case, `out of memory`, `out_of_memory` or `bad_alloc`.
  - `pub fn parse_fused_section(body: &str) -> Option<FusedSection<'_>>` and
    `pub struct FusedSection<'a> { pub batch: Vec<(&'a str, &'a str)>, pub holds: Vec<&'a str>, pub merge: Vec<&'a str>, pub pipeline: Vec<(&'a str, &'a str)>, pub nodes: Vec<BenchNode<'a>>, pub roots: Vec<usize>, pub trailer: Vec<(&'a str, &'a str)> }`,
    with `field`/`count` as `BenchSection` has them. It returns `None` for `SKIPPED`/`FAILED`.

**The format** (one section per batch, `== first3` … `== all`, in `BATCH_SIZES` order):

```
== first3
--- batch ---
queries=[q1,q2,q3]
outcome=measured                      # or out-of-memory
reason=<one line>                     # out-of-memory only
shared=[lineitem:q1+q3,orders:q3]     # table:consumer queries, a query repeated per scan
unshared=[q2/lineitem:limit-cut]      # [] when none
reads=[lineitem:2,orders:1]           # table:times read; a shared load once, plus each re-read and unshared scan
buffers=1
buffer_bytes=123456789
rereads=1
hold query=q3 table=lineitem hold=GpuHashJoin choice=buffer above=7 bytes=123456789 reread_us=48120 why=fits: 123456789 <= 900000000
hold query=q1 table=lineitem hold=GpuHashJoin choice=re-read above=- bytes=0 reread_us=51034 why=no edge: a join below every edge grew the rows
cap=2
pool_bytes=74088185856
budget_bytes=66679367270
estimate_bytes=51234567890
over_budget=false
answers=match                         # measured only; a mismatch panics, so this never says otherwise
--- merge ---
saved_us=812
entry saved_us=640 members=q1:3,q3:5 node=GpuFilter: …     # node= last; its value keeps its spaces
--- pipeline ---
model=one loader and one executor; roots start when their sources are loaded; cheapest-first loads
load_order=[lineitem,orders]
serial_us=… makespan_us=… saved_us=…        # one key per line in the file
fused_measured_us=…
pipelined_estimate_us=…
--- tree ---
<render_timings_of_roots output: `root <i>:` headers, `@k` shared nodes, `GpuHoldBuffer` lines, bench-rows' time line>
--- run ---
<run_section(RunTotals{…}) as bench-rows writes it, ending with the five provenance keys>
```

`hold` lines carry `why=` last, its value keeping its spaces, as `node=` does; `above` is the
origin pre-order index, in that query's own tree, of the node the buffer sits on. An
out-of-memory batch keeps `--- batch ---` (its holds, cap and estimate), `--- merge ---` and the
model lines of `--- pipeline ---`, writes no `--- tree ---`, and its `--- run ---` holds only the
five provenance keys, so resume still recognises it. A first20 marker is
`SKIPPED<the batches() marker>`, as `declared_for` writes markers.

Flow of `benchmark_fused`, in this order, mirroring `corpus_benchmark::benchmark_case`/`measure`:
1. Refuse any sf but 40. Install the pool, set events timing and run `assert_fit_to_measure`.
2. Plan each query with `plan_at` at sf40 tp1_single. Build the baseline: parse the baseline's
   `tp1_single.benchmark.txt` sections, then `Baseline::check`. Read `loads.tsv` with
   `read_load_times`, and each query's own `CudfScan` seqs through `attach_recipes`.
3. `fuse` with a `HoldLedger` as the chooser, then `settle` → the fused plan and its
   `CapDecision`.
4. Compute `merge_hint`.
5. One warm-up, then the measured runs, each through `Session::open_roots` and `run_roots`
   with `Some(cap)`, inside `nvtx_range("{ds}.sf{sf} {batch} tp1-single")`. An `is_out_of_memory`
   error on any run makes the out-of-memory body; any other error panics.
6. Take the 2nd-min run, then `pipeline_hint` with the measured totals.
7. Check the answers: run each query alone (`Session::open` + `run`), then
   `assert_results_match(&alone, &answers[i], Some(1e-9), what)`.
8. Write the record rows to the fused record, then `measure_into` the section.

- [ ] **Step 1: failing tests.** `section/tests.rs`: `a_measured_section_reads_back`
  (a literal section: `count("cap") == Some(2)`; `load_us + execute_us == device_us`;
  `tree_load_us` over the nodes, each shared node once, equals `load_us`; `roots == [0, 4]`;
  a merge line keeps the spaces of `node=`, a hold line those of `why=`; `holds.len() == 2`).
  `an_out_of_memory_section_reads_back_with_its_cap` (no nodes, `field("outcome") ==
  Some("out-of-memory")`, trailer = the five keys, holds kept). `a_marker_is_not_timed`.
  `benchmark/tests.rs`: `an_out_of_memory_error_is_recognised` (`RunError` texts from rmm's
  `std::bad_alloc: out_of_memory: RMM failure` and cuDF's `CUDA error … out of memory`: true;
  `"no plan loaded"`: false). `test_corpus_goldens/benchmark.rs`: `every_fused_section_adds_up`,
  which walks every `*.fused.txt` (passing when there is none). For measured sections it checks
  the bench-rows arithmetic, `runs` has `MEASURED_RUNS` entries (`FUSED_ALL_RUNS` for `all`),
  `run_us` is its 2nd-smallest, `estimate_bytes <= budget_bytes` unless `over_budget`, the merge
  total is at least every entry's share,
  `pipelined_estimate_us == fused_measured_us.saturating_sub(saved_us)`; `buffers` and
  `rereads` equal the counts of `choice=buffer` and `choice=re-read` hold lines, `buffer_bytes`
  the Σ of the buffer lines' `bytes`, `Σ reads` the number of distinct `GpuLoadParquet` nodes in
  the tree, and the tree's `GpuHoldBuffer` lines number `buffers`.
  In `ci_coverage.rs`: `a_manual_prototype_is_wired_nowhere` (the check over a workflow text
  holding `--test prototype_fused_benchmarks` → `Err` naming it; over today's workflows → `Ok`).
  In `test_gpu.py`: `test_run_names_a_staged_binary` (`run prototype_fused_benchmarks` → the
  docker command runs `rust-benchmarks/prototype_fused_benchmarks` with the filter) and, in the
  suite that covers `stage-install.sh`, `test_stage_install_builds_the_prototype_target`.
- [ ] **Step 2:** run `cargo test --features rust-only -p peacockdb-core --lib -- prototypes::fuser::section prototypes::fuser::benchmark`
  → FAIL to compile; `cargo test -p cost-report --test test_source_formatting -- manual_prototype`
  and `python3 scripts/env/tests/test_gpu.py` → FAIL.
- [ ] **Step 3:** implement. `prototype_fused_benchmarks.rs` (`#![cfg(feature = "gpu")]`) gets
  `macro_rules! fused_benchmark { ($dataset:ident, $batch:ident) => { paste::paste! { #[tokio::test] async fn [<bench_fused_ $dataset _sf40_ $batch _tp1_single>]() { peacockdb_core::prototypes::benchmark_fused(stringify!($dataset), "40", stringify!($batch)).await; } } } }`
  and its eight invocations.
- [ ] **Step 4:** the new tests pass; `cargo test -p cost-report --test test_source_formatting`
  → PASS, `every_rust_test_target_is_named_by_ci` included. Then `cargo check -p peacockdb-core
  --features gpu,test-support --tests` where `cpp/install` exists. Otherwise Task 9's staged build is that check.
- [ ] **Step 5:** golden check (the prototypes gate included: nothing in `test_support/`, `executor/` or
  `plan_text/` names `prototypes::`). Docs: build-test.md "Benchmarks" gets a short paragraph:
  the fused cases, their file, the record path, `FUSED_ALL_RUNS`, sf40 only, and the run
  command. The detail file gets "The fused format" (the block above, plus key definitions), the
  hold rule and the cap rule. Commit `fuser: the fused benchmark case and its file`.

### Task 7: the page's fused-batches table

**Files:**
- Create: `prototypes/fuser/report.rs`, `prototypes/fuser/report/tests.rs`
- Modify: `prototypes/mod.rs` (the `pub` delegate below)
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` — `the_benchmark_report_matches_the_trees`
  passes `fused_report_tables(&root)` as its `extra` (a test may name `prototypes::`, as
  `every_fused_section_adds_up` does)
- Docs: `llm-wiki/build-test.md`, bench-rows' page paragraph: the fused tables, and that the
  page is regenerated in the commit that adds fused files

**Interfaces:**
- Consumes: Task 6's `parse_fused_section`, `FusedSection::{field, count}`, `fused_results_file`'s
  layout; bench-rows' `ReportTable`, `ReportCell`, `section_href`.
- Produces: `pub fn fused_report_tables(root: &Path) -> Vec<ReportTable>` (in `prototypes/mod.rs`,
  delegating to `fuser::report`). Per device directory under `root` holding
  `fused/<ds>.sf<sf>/tp1_single.fused.txt`, by name, then dataset, then sf: one table, id
  `fused-<device>-<ds>-sf<sf>`, heading `Fused batches: <device>, <ds> sf<sf>`, columns `batch |
  queries | outcome | fused load (ms) | fused execute (ms) | accountant's peak ≥ (GiB) | cap |
  reads`. One row per section in file order: the batch name linked to its section; `queries=`
  and `reads=` as written; a measured batch's `load_us`, `execute_us`, `peak_bytes` from its
  `--- run ---`; `out of memory (cap k)` (class `off`) with `—` for the numbers; a marker reads
  `same set as all` (class `skip`). No fused file anywhere → no table.

- [ ] **Step 1: failing tests** (`report/tests.rs`, temp trees from Task 6's literal sections):
  `a_measured_batch_is_a_row` (cells `1.0`/`0.5`, cap `2`, the link's `#L` the section's line);
  `an_out_of_memory_batch_shows_its_cap`; `a_marker_reads_same_set_as_all`;
  `no_fused_file_means_no_table`.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- prototypes::fuser::report` → FAIL to compile.
- [ ] **Step 3:** implement; wire the page's test. **Step 4:** PASS; `the_benchmark_report_matches_the_trees`
  PASS unchanged (no fused file is committed yet).
- [ ] **Step 5:** golden check. Commit `fuser: the report page's fused-batches table`.

### Task 8: a profile per fused batch

**Files:**
- Modify: `scripts/create_nsys_profile.sh` (as new-hosts and pinned-loads left it)
- Test: `scripts/env/tests/test_gpu.py` (new-hosts' fakes for ssh and rsync)
- Docs: `llm-wiki/build-test.md`, under "Benchmark data flow"

**Interfaces:**
- Consumes: `nsys_calls.py --capture --out` (no `--plans-dir`), `nsys_loads.py --capture --out`
  (pinned-loads), `nsys_hbm.py --record … --capture … --case … --peak-bw … --out` and `--parts … --out`.
- Produces: `create_nsys_profile.sh --fused <dataset>.sf<sf> [--trace|--metrics] [--ssh …] [--host …] [--remote-dir …] [--remote-cudf-root …]`.
  - The cases are that dataset's four batches. Test `bench_fused_<ds>_sf<sf>_<batch>_tp1_single`
    of the staged `rust-benchmarks/prototype_fused_benchmarks` binary (not `peacock_gpu_benchmarks`),
    case `"<ds>.sf<sf> <batch> tp1-single"`.
  - `PCK_TEST_FILTER` with `--fused` is refused, naming the contradiction.
  - The trace pass makes **one capture per batch** (`--exact`), runs `nsys_calls.py` and
    `nsys_loads.py` on each, and writes `testdata/calibration/fused/<ds>.sf<sf>/<batch>.calls.tsv`
    and `<batch>.loads.tsv`.
  - The metrics pass joins each batch's capture against `testdata/calibration/fused/<ds>.sf<sf>/records.tsv`,
    then merges the parts into `…/hbm.tsv`.
  - A batch whose capture or join fails is appended to `…/failed.tsv`
    (`batch<TAB>pass<TAB>reason`), and the loop goes on. Say why at the site: every batch must
    get its own attempt. The exit status is non-zero if any batch failed.
  - The merge covers the batches that succeeded. `nsys_hbm.py` is given only those batches'
    records: filter the record's rows by case into a temp file.

- [ ] **Step 1: failing tests.** `test_fused_profile_runs_one_capture_per_batch`: fake ssh logs
  four `--exact bench_fused_tpch_sf40_…` invocations of `prototype_fused_benchmarks` in
  `BATCH_SIZES` order.
  `test_fused_profile_refuses_a_filter` (exit non-zero, both flags named, no ssh call).
  `test_fused_profile_lists_a_failed_capture_and_goes_on`: the fake ssh fails for `all`, so
  `failed.tsv` names `all` and `trace`, the other three `calls.tsv` exist, and the exit is non-zero.
- [ ] **Step 2:** `python3 scripts/env/tests/test_gpu.py` → the three FAIL.
- [ ] **Step 3:** implement. `bash -n` and `shellcheck scripts/create_nsys_profile.sh` show no
  new finding.
- [ ] **Step 4:** every `scripts/env/tests/test_*.py` that needs no image → `0 failed`.
- [ ] **Step 5:** build-test.md: the `--fused` line under "Benchmark data flow", plus the file
  layout `testdata/calibration/h100-80gb-hbm3/fused/<ds>.sf40/{<batch>.calls.tsv,<batch>.loads.tsv,hbm.tsv,failed.tsv}`.
  Commit `fuser: an Nsight profile per fused batch`.

### Task 9: the runs on the H100

**Files:** `testdata/benchmark-results/h100-80gb-hbm3/fused/{tpch,tpcds}.sf40/tp1_single.fused.txt`;
the detail file.

- [ ] **Step 1: Build first.** Make the staged install the way new-hosts' build-test.md section
  does: `stage-install.sh` in the build image on nebius-cpu128, then rsync to `dev`; stop the CPU VM.
- [ ] **Step 2: Start the H100 and ship** on `dev`:
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu start`
  (exit 0: RUNNING; exit 75, no capacity: record it, retry later). Then `scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu prepare && scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu ship --install <dir>`.
  Expected: exit 0 from both, and `<dir>/rust-benchmarks/prototype_fused_benchmarks*` exists.
  Then the files the batches read at run time are in the shipped tree:
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu ssh 'ls /home/dmitry/peacock/<i>/testdata/benchmark-results/h100-80gb-hbm3/cases.csv /home/dmitry/peacock/<i>/testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/tp1_single.benchmark.txt /home/dmitry/peacock/<i>/testdata/calibration/h100-80gb-hbm3/{tpch,tpcds}.sf40/loads.tsv'`
  → five paths. A missing one: `ship` is extended to carry it (with a `test_gpu.py` case), not
  copied by hand.
- [ ] **Step 3: TPC-H.** `PCK_TEST_FILTER=bench_fused_tpch_sf40_ scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu run prototype_fused_benchmarks && scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu pull`.
  Pass: exit 0 (an out-of-memory batch still passes), and the file holds four sections. Each is
  measured, out-of-memory, or the first20 marker. Every measured section says `answers=match`.
  If the 4-hour watcher stops the process, run again with `PEACOCK_BENCH_RESUME=1`, and record
  the segments.
- [ ] **Step 4: TPC-DS.** Same as Step 3 with `bench_fused_tpcds_sf40_`.
- [ ] **Step 5: Read back.** On `dev`:
  `cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- every_fused_section_adds_up`
  → PASS over both files. A case that failed for another reason (a panic, or a wrong answer
  naming a query) is a bug. File a ticket if it is the engine's. If it is the fuser's, fix it
  with a regression test in Task 1 (or Task 5, for a hold or cap decision), then rerun that batch.
- [ ] **Step 6:** keep the VM up for Task 10. The detail file records, per batch: queries,
  shared and unshared counts, reads per table, buffers and re-reads with their bytes and
  `reread_us`, cap, outcome, load/execute/peak, and the wall time. It also names the protocol:
  1 warm-up, 2nd-min of 10, and of 3 for `all`. Regenerate the page:
  `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_report_matches_the_trees`
  → the two fused tables appear; without `UPDATE_CANONICAL` → PASS. Commit the fused files and
  `testdata/benchmark-report.html` together:
  `fuser: fused batches on the H100, tpch and tpcds sf40`.

### Task 10: the profiles on the H100

**Files:** `testdata/calibration/h100-80gb-hbm3/fused/{tpch,tpcds}.sf40/`; the detail file.

- [ ] **Step 1:** for `ds` in `tpch.sf40 tpcds.sf40`:
  `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12 scripts/create_nsys_profile.sh --fused $ds --ssh 'scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu rsh' --host nebius-h100 --remote-dir /home/dmitry/peacock/<i> --remote-cudf-root /home/dmitry/peacock/cudf-25.02`.
  Here `<i>` is the `--dir` `run-gpu.sh` shipped to. Expected: both passes run, a
  `<batch>.calls.tsv` and `<batch>.loads.tsv` per batch, and one `hbm.tsv`. An out-of-memory
  batch shows up in `failed.tsv` with its reason. A non-zero exit that comes only from
  `failed.tsv` batches is a pass.
- [ ] **Step 2: Check.** Every `loads.tsv` row has `load_us > 0`, `host_read_us ≤ load_us` and
  `unattributed_us ≤ load_us` (both clipped to the load's windows), and `off_thread_device_us ≤
  h2d_us + decompress_us + decode_us + other_device_us` — only what pinned-loads-impl.md's column
  semantics guarantee. No sum of the parts is held to `load_us`: kvikio's host read contains its
  bounce copies, and off-thread device time overlaps. Paste one batch's per-load read+copy vs decode split into the
  detail file.
- [ ] **Step 3:** `git mv`/copy `testdata/calibration/fused/<ds>/` (except `records.tsv`) to
  `testdata/calibration/h100-80gb-hbm3/fused/<ds>/`. Captures stay on the host.
- [ ] **Step 4: Stop the VM.**
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu stop`
  → `STOPPED`. Run the golden check (`git status --short testdata/` shows only the new
  committed files before the commit). Commit `fuser: Nsight profiles of every fused batch on the H100`.

---

## Self-review notes

- Spec 1 (fuser: one load per file, union projection and row groups, sizing redone, zero-copy
  selection, the refusal named, nothing else merged) → Task 1. 1a (buffer or re-read per held
  reader, recorded with why; placement at the smallest recorded `batch_bytes` edge, above a join
  only when its rows did not grow, charging its build; a plain function of sf40 numbers and the
  budget; sf40 only) → Task 1 (mechanism), Task 5 (rule), Task 6 (recorded). 1b (reads per
  table) → Task 1 (`Fused.reads`), Task 6 (`reads=`). 2 (merge hint, baseline times) → Task 3.
  3 (pipeline hint, model stated, against the fused measured) → Tasks 4, 6. 4 (cap rule, plain
  function, peak and batch_bytes; now also buffers' bytes) → Task 5. 5 (runs, load/execute
  separate, cap, peak, hints, protocol, OOM as an outcome) → Tasks 6, 9. 6 (device answers per
  batch; CPU sf1 against the goldens) → Task 6 step 7 of the flow, Tasks 1, 2. 7 (results path,
  format documented) → Task 6. 8 (the page's fused-batches table, through
  `parse_fused_section`) → Tasks 7, 9. Profiles → Tasks 8, 10. Constraints: `prototypes/fuser/` under
  `test-support`, module docs, the source-formatting gate and architecture.md "Prototypes" →
  Task 1.
- Names used across tasks: `Query`, `Fused`, `Origin`, `Unshared`, `ShareGroup`, `HeldReader`,
  `Choice`, `HoldDecision`, `HoldRecord`, `Chooser`, `fuse`, `resolve_cap_holds`,
  `re_read_every_hold`, `buffer_every_hold_lowest`, `Baseline`, `QueryTimes`, `merge_hint`,
  `MergeHint`, `MergeEntry`, `pipeline_hint`, `pipeline_inputs`, `RootNeeds`, `PipelineHint`,
  `LoadTimes`, `read_load_times`, `EdgeCost`, `HoldInputs`, `place_buffer`, `decide`,
  `hold_inputs`, `HoldLedger`, `settle`, `cap_for`, `cap_inputs`, `CapInputs`, `CapDecision`,
  `MARGIN_PERCENT`, `scaled_bytes`, `batch_set`, `ordered_queries`, `fused_report_tables`,
  `BATCH_SIZES`, `batches`, `prototype_fused_benchmarks`, `Exemption::ManualPrototype`,
  `benchmark_fused`, `fused_results_file`, `FUSED_ALL_RUNS`, `is_out_of_memory`,
  `parse_fused_section`, `FusedSection`, `node_line_of`, `nodes_as_recorded_of_roots`,
  `timed_load_us_of_roots`, `append_records_to`, `row_group_metadata`.
- dag-driver names consumed match its plan: `render_timings_of_roots(roots, times, report)`
  (its Task 9), `run_roots`/`FusedRun` (Task 5), `new_hold_buffer`/`UnbufferedHold` (Task 4),
  `validate_cap` (Task 7).
- Spec gaps, decided: (a) item 6's sf1 CPU check has no sf1 baseline for the hold rule, so it
  passes the fuser an explicit test chooser; the benchmark path has no fallback. (b) "reads per
  table: once, plus one per re-read" leaves out unshared scans, which also read the table; they
  are counted too, and listed in `unshared=`. (c) The spec's hold is a join's; dag-driver's cap
  is a hold too, so readers in roots that wait under the cap go through the same rule
  (`settle`).
