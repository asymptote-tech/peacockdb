# query-fuser — detail

Branch `ENS-query-fuser`, forked from `ENS-dag-driver` at `5a1d4702` on 2026-11-06. PR targets
`ENS-dag-driver`. Plan: [`query-fuser-impl.md`](query-fuser-impl.md).

## Progress

- 2026-11-06: branched, board `building`. Verda not available (`list_verda_instances.sh`:
  `VERDA_CLIENT_ID` unset), so CPU tests run locally. Developer dispatched at Task 1 Step 1.
- 2026-11-06, Task 1 (developer). Built to the plan; checkpoint `fuser: one shared load per file
  across N plans` reported, not committed. Baseline before any change: `--lib` 1106 passed,
  2 ignored; cost-report `test_source_formatting` 32. After: `--lib` +15 (14 in
  `prototypes::fuser::fuse::tests`, 1 in `prototypes::fuser::tests::end_to_end`), cost-report
  +6. Deviations and decisions, for the next developer:
  - **`fused_roots_answer_as_each_alone` runs on `CpuBackend`, not the mock.** The mock backend
    is `executor/driver/tests/mock.rs`, private to `executor::driver`; no path from
    `prototypes` reaches it, and exposing it would be a test-only cross-component item for
    this one test. The hand-built plans read real sf1 files instead (`lineitem`, `orders`), so
    the CPU backend runs them, which also checks the inserted projects' column selection.
  - **The sf1 test plans through `crate::tests::share::planned`**, with the query text read
    from `queries_dir_for("tpch")`, not `test_support`'s `plan_at`, which `test_support/mod.rs`
    does not declare. Same session, same knobs; no harness surface was added in Task 1. It
    runs in about 3 s: tpch q3/q5/q10 share customer, orders, lineitem and nation; re-read
    resolves 7 holds, buffering 9.
  - **`Fused` keeps the queries' own trees** (private field `originals`, a rebuilt copy), and
    **`ShareGroup` carries `members: Vec<Origin>`** beside `consumers`. `resolve_cap_holds` takes
    only a `Fused`, and a re-read needs the member's own load and the origins walk needs the
    originals; `members` says which shared source an edge's reader is (a query may read one file
    twice, `consumers == [0, 0]`). Not in the plan's interface; both are additive.
  - **`HoldRecord.hold` is the node's `name()`** (`GpuHashJoin`, or `GpuUnload` for a cap
    hold): `node_line_of` does not exist until a later task.
  - **When a refusal is recorded:** `unshared` lists a scan only when its file has more than one
    scan in the batch. Order: limit-cut first, then rows-only (a lone scan of one kind beside
    the other kind), then nulls, dropping one member at a time and re-reading the union.
    A group that falls below two members dissolves; its remaining scan reads its own load.
  - **Origins are recomputed, not tracked**: after every edit a parallel walk of each original
    tree and its fused root maps nodes back. A fused `GpuHoldBuffer` over a non-buffer original,
    and a fused `GpuProject` over an original load, are the inserted nodes. The same walk gives
    each edge into a shared source the member it stands for, which is how a held reader finds
    its group and a re-read finds its load.
  - **`row_group_metadata` shares a private `groups_of(reader, …)` with `survivor_metadata`**
    rather than `survivor_metadata` calling it, so planning still opens each file once.
  - **`partition` is relayed too** (`planner/mod.rs` → `translator/mod.rs` →
    `scan_mapping`), which the plan consumed without naming a delegate.
  - `plan/rebuild.rs` holds private copies of `schema_of` and `lanes_of`; `src/tests/rebuild.rs`
    keeps its `pub(crate)` ones, which other test files import.
- 2026-11-06, Task 2 (developer). Checkpoint `fuser: the ordered lists and fused answers
  against the corpus goldens` reported, not committed. `--lib` +10 (8 in
  `prototypes::fuser::lists::tests`, 2 in `prototypes::fuser::tests::end_to_end`).
  - **Wall time:** the three end-to-end tests take about 16 s together (18 s wall with the
    build cached, peak RSS 4.4 GB), so none is `#[ignore]`d. tpch and tpcds `first10` are
    q1-q10 of each; no first10 section at sf1 is a `skipped:` marker, so no query fell back to
    its own unfused run. The test asserts that and names any query that does.
  - **The missing-list test does not set `PEACOCK_TESTDATA_DIR`.** The lib's tests share one
    process, and ~1100 of them read testdata in parallel, so setting it would race them. Instead
    `ordered_queries(dataset)` is `ordered_queries_in(&device_cases_path("h100-80gb-hbm3"),
    dataset)`, and the test hands the private reader a path under an empty temp dir; the panic
    names `h100-80gb-hbm3/cases.csv`. The production path through `device_cases_path` is still
    covered, by `ordered_queries_read_the_h100_list` over the committed file.
  - **`line_row_order` and `result_matches` are reached through `#[cfg(test)]` entry points
    in `test_support/mod.rs`**, registered in cost-report's `TEST_ONLY_ITEMS` with
    `prototypes/fuser/tests/end_to_end.rs` as their caller: both are private to
    `test_support::corpus`, and their only caller outside it is a test in another component.
    The order comes from the sf1 registry row's `duckdb_oracle` and `gpu_oracle`; the author
    mode from the section's `mode=` line.
  - The capped test also holds the capped answers to the goldens, beyond the plan's
    capped-equals-uncapped check. `resolve_cap_holds` under a cap of one validates for both
    datasets: no `HoldCycle`.
  - Extra tests beyond the plan's: `tpch_batches_only_its_twenty_two_queries`,
    `an_empty_batch_set_panics_naming_the_dataset`, `ordered_queries_read_the_h100_list`,
    `first3_runs_whatever_the_list`.
- 2026-11-06, Task 3 (developer). Checkpoint `fuser: the mergeability hint` reported, not
  committed. `--lib` +10 (5 `prototypes::fuser::baseline::tests`, 5 `…::merge::tests`).
  - **The test plan helpers moved** from `fuse/tests.rs` into `fuser/tests.rs` (`pub(crate)`,
    beside the choosers), so the baseline, merge and later tests build plans the same way;
    `count` (a keyless aggregate) and `filter_above` were added there. `pub(crate) mod` is
    refused outside a test directory, which `fuser/tests.rs` is not, so they are items of that
    module rather than a child of their own.
  - **`node_line_of` carries no `#[allow(dead_code)]`**: `plan_text` already allows it for
    the whole component outside test builds, and in a test build the merge hint calls it.
  - **Roots are in no merge group.** With them, two identical queries make the `GpuUnload`s the
    largest identical subtree, and the plan's own case (`agg(filter(S))` twice: the aggregate is
    the entry) could not hold. A root is the crossing to the host, one per query by nature.
  - **Merge keys are interned** (`(plan line, child key ids)` → id), not concatenated text:
    a tpcds plan's nested load lines would make string keys quadratic in size.
  - A node is a group candidate only where it stands for a node of its query's own tree, so
    inserted projects (time 0) never form an entry.
  - `MergeEntry.members` are in fused pre-order; entries with equal savings keep group order.
- 2026-11-06, Task 4 (developer). Checkpoint `fuser: the pipelining hint` reported, not
  committed. `--lib` +6 (`prototypes::fuser::pipeline::tests`).
  - The test `baseline` builder moved from `merge/tests.rs` into `fuser/tests.rs`, shared.
  - **Sources are ordinals in fused pre-order**, so "missing sources appended in fused
    pre-order" is ascending ordinal order inside `pipeline_hint`.
  - **The makespan is the later of the executor's last finish and the loader's last load**;
    every load feeds some root, so the executor's finish always wins in practice.
  - A shared source's load time is the largest over the origins its fused node holds *now*:
    after a re-read the re-reading member is no longer one of them.
  - Extra test beyond the plan's: `a_root_needing_two_sources_waits_for_both`.
- 2026-11-06, Task 5 (developer). Checkpoint `fuser: buffer or re-read, and the concurrency
  cap rule` reported, not committed. `--lib` +24 (10 `holds`, 11 `cap`, 3 `loads`).
  - **`HeldReader` gained `member: Origin`**, the reader's own load. The re-read cost
    (`loads.tsv` by that load's seq) and an inserted project's bytes both need it, and Task 1's
    walk already knew it; `Walk::held_reader` now returns it inside the struct.
  - **Under the cap's hold, an edge is allowed only where nothing at or under it keeps
    state** (`plan::holds_state`, now relayed by a `plan/mod.rs` delegate from
    `validate.rs`). dag-driver refuses a waiting root whose buffer has an accumulator below it
    (`UncappedAccumulator`); without this the smallest edge is often above one.
    `a_waiting_root_is_never_buffered_above_its_state` fails with exactly that refusal when the
    check is switched off.
  - **`why` for no edge names its cause**: `no edge: the reader takes the shared load directly`
    (the path's only edge is the one above the shared load, which nothing recorded), or
    `no edge: each lies above a join that grew the rows, or above state a waiting root may not
    keep`. The plan's single string could be wrong for both.
  - The ledger's one-time charge is `S + Σ buffers + max root` from `cap_inputs` at its first
    call, so buffers another chooser made before it are counted.
  - `settle` returns the recomputed decision as soon as the cap stops falling.
  - Extra tests: `a_disallowed_edge_is_never_placed`, `only_the_named_case_is_kept`,
    `a_re_read_load_is_charged_in_full`, `a_waiting_root_is_never_buffered_above_its_state`.
  - A test of mine first failed for its own numbers (a pool of 1000 let both roots run, so
    none waited); fixed to a budget of 135, and re-checked red against the switched-off rule.

- 2026-11-06, Task 6 (developer), **in progress, stopped for context** after its CPU half
  (Part A). Not a checkpoint: the plan's commit is Step 5, after the device flow. The tree
  builds and tests green as it stands (rust-only `--lib`, goldens, source gates, and the
  gpu `--tests` check).
  - **Done (Part A):**
    - `fuser/section.rs`: `render_fused_section` (Outcome `Measured | OutOfMemory`, the run
      block passed in as text) and `parse_fused_section`, its own tree reader for
      `render_timings_of_roots`'s `root <i>:` headers and `@k` nodes (the harness's
      `parse_benchmark_section` reads neither, and its time-line helper is private).
      `FusedSection` (+ `field`, `count`, `tree_device_us`, `tree_load_us`),
      `parse_fused_section`, `fused_results_file`, `FUSED_ALL_RUNS` are `pub` in
      `prototypes/mod.rs`.
    - `fuser/benchmark.rs` (host side only): `is_out_of_memory`, `fused_results_file`,
      `fused_record_path`, `case_name`, `replace_case_rows` (drops the case's rows, then
      `append_records_to`). `test_support/record.rs` gained `append_records_to(path, …)`, which
      `append_records` calls; `test_support/mod.rs` declares it `pub(crate)`.
    - Tests: `section::tests` 6, `benchmark::tests` 4 (out of memory, file paths, rerun rows,
      `plan_files` of a fused batch). Mutation-checked: no record replacement, `@k` read as no
      edge. `test_corpus_goldens`: `every_fused_section_adds_up` (no file committed yet) over
      `benchmark_checks.rs`'s `fused_problem`, with `a_fused_section_that_adds_up_passes` and
      `a_fused_section_that_does_not_add_up_is_refused` (6 doctorings, and `all`'s run count).
    - `.gitignore`: `testdata/calibration/fused/`.
  - **Deviation:** the plan's check "the merge total is at least every entry's share" is false
    under its own model (A: filter 10, agg 100; B: filter 50, agg 1 → entry 51, total 11), so
    `fused_problem` checks the entries are largest first instead.
  - **Left (Parts B, C, docs)**, for the next developer:
    - **The test target and its gate were taken back out**, because they cannot compile before
      `benchmark_fused` exists, and a gpu build of an unbuildable target breaks the tree. Redo
      when Part C lands: `tests/prototype_fused_benchmarks.rs` (`#![cfg(feature = "gpu")]`,
      the plan's `fused_benchmark!` and eight invocations); `[[test]] name =
      "prototype_fused_benchmarks"`, `required-features = ["gpu", "test-support"]` in
      `peacockdb-core/Cargo.toml` (no `[[test]]` exists there yet; one entry overrides only its
      own target); in `ci_coverage.rs` `Exemption::ManualPrototype`, the entry, its arm in the
      stale-entry message, and a `wired_nowhere(target, workflow_lines, staged, shadgpu_text)`
      check (no `--test <target>` word pair in any folded workflow line, not in
      `gpu_job_staged_targets()`, not in `build-test-shadgpu.sh`'s `RUST_TESTS=(` line), called
      for each `ManualPrototype` entry from `every_rust_test_target_is_named_by_ci` and tested by
      `a_manual_prototype_is_wired_nowhere` with doctored inputs. All of this was written and
      green once: the guard named the target when its exemption was removed.
    - **Reachability, a plan gap:** `benchmark_fused` lives in `prototypes`, and most of what it
      consumes is private to `test_support`'s modules (`gpu_session::Session`,
      `corpus_benchmark::{second_smallest, measured_of, assert_fit_to_measure, BENCH_POOL_BYTES}`,
      `record::{record_rows, declared_steps, rows_match_the_recipes}`, `benchmark_section::
      measure_into`, `benchmark_trailer::{RunTotals, run_section}`, `benchmark_prefill::
      rounds_were_pinned`, `provenance::{of_this_run, fields}`, `corpus::plan_at`). Making them
      `pub(crate)` in their files does not reach them: each needs a `pub(crate)` delegate in
      `test_support/mod.rs` (gated `#[cfg(not(feature = "rust-only"))]` where the module is).
      `Session` is a type, so rather than move it, add two harness functions in
      `gpu_session.rs`: a fused run of `roots` under `cap` with its regions, and a prefill of
      `files` on a session over `roots`. `second_smallest` takes `Vec<Run>` (private); make
      it generic over a key, or pick the 2nd-min index locally. Engine delegates per the plan:
      `nodes_as_recorded_of_roots` (`executor/driver/index.rs` + `executor/mod.rs`),
      `timed_load_us_of_roots` (`plan_text/bench_text.rs` + `plan_text/mod.rs`).
    - **The device flow** `fuser/benchmark/device.rs` behind `#[cfg(not(feature =
      "rust-only"))]`, declared in `benchmark.rs`, with a one-expression `pub(crate) async fn`
      delegate there (a `use` re-export is refused) and in `fuser/mod.rs`, and the `pub async fn
      benchmark_fused` in `prototypes/mod.rs`. The plan's Task 6 flow, steps 1-8; the section is
      `render_fused_section`, the out-of-memory run block `--- run ---` plus the five provenance
      keys. Seqs per query: `attach_recipes` of its own tree, post-order → pre-order, the
      `CudfScan` call's seq at each load. Type-check locally with
      `CUDF_ROOT=/home/dmitry/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh check -p
      peacockdb-core --features gpu,test-support --tests` (works on this host, ~1.5 min warm).
    - **Part B**: `run-gpu.sh run <staged name>`, `stage-install.sh`, `idle_watch.py`
      `TEST_NAMES`, and their Python tests, as the plan lists them.
    - **Docs**: build-test.md "Benchmarks" paragraph and the counts (`--lib` +10 → 1183,
      `test_corpus_goldens` +3 → 64, cost-report +1 when the wiring test lands), and "The fused
      format" section here.

- 2026-11-06, Task 6 (developer, second), **to its checkpoint**: Parts B, C and the docs on top
  of part A. Checkpoint `fuser: the fused benchmark case and its file` reported, not committed.
  `--lib` 1181 → 1191 passed (+8 `prototypes::fuser::benchmark::tests`, +2
  `executor::driver::tests::render`), goldens 64, cost-report formatting 38 → 39, gpu
  `--tests` check clean, the target linked (it cannot run here: the local `libpeacock` predates
  `peacock_join_build`). Decisions and deviations:
  - **Reachability: harness parts, not `Session`.** `test_support/mod.rs` gains `pub(crate)`
    delegates under one banner: `RunTotals` (moved from `benchmark_trailer.rs`, a type cannot be
    delegated), `run_section`, `install_bench_pool` (the pool constant stays private),
    `assert_fit_to_measure`, `plan_at`, `rounds_were_pinned`, `record_rows`, `declared_steps`,
    `provenance_fields`, and two new `gpu_session.rs` functions, `prefill_for_roots` and
    `run_roots_timed` (one session per run, `assert_closed`, regions drained). All but
    `provenance_fields` are gated `not(rust-only)`. `measure_into`, `merge_section`,
    `answer_on_the_device` and the rest were reachable already.
  - **The 2nd-min is picked by index** (`second_smallest_at`), not through the harness's
    `second_smallest`, which takes its private `Run`.
  - **Resume is the fused file's own check.** `measure_into`'s `holds_measured` parses with
    `parse_benchmark_section`, which panics on `--- batch ---`, outside the failure record. So
    `benchmark_fused` keeps a section itself (`holds_this_run`: a section of the batch whose last
    five trailer keys are this run's provenance, measured or out of memory) and hands
    `measure_into` `Resume::No`.
  - **A marker batch** (a size the list does not exceed) writes its `SKIPPED` section through
    `merge_section` and returns. Today neither list is that short.
  - **A captured run** (`PEACOCK_BENCHMARK_CAPTURE`) writes the record rows and no section, as
    `benchmark_case` does. NVTX is off across the warm-up and the answer checks: the alone runs'
    calls belong to no case, and the capture's containment check would refuse them.
  - **The cap's pool is `RmmPool::Pool.maximum_bytes`**, what the run installed, not the
    request.
  - **`scan_seqs`**: per pre-order node of a query's own tree, the seq of its recipe's
    `FbKind::Scan` call, through `nodes_as_recorded`'s post-order and `attach_recipes`.
  - Engine delegates: `nodes_as_recorded_of_roots` (`driver/index.rs` → `driver/mod.rs` →
    `executor/mod.rs`, `nodes_as_recorded` now calls it) and `timed_load_us_of_roots`
    (`bench_text.rs` → `plan_text/mod.rs`, `timed_load_us` calls it). `PlanIndex::build` then
    has no rust-only caller and carries the reason at its `allow`.
  - `prototypes/mod.rs` keeps `fuser`'s `allow(dead_code)` outside test builds: some of its
    `mod.rs` API is reached by its own tests alone, since the benchmark's modules call one
    another directly. `DEVICE` moved from `lists.rs` to `fuser/mod.rs`, shared with the
    baseline and `loads.tsv` paths.
  - **`TEST_NAMES` gains `"prototype_fused_benchmarks"`, not `"prototype_fused"`**: the list
    names binaries in full and `matches` compares the first 15 characters, as it does for
    `peacockdb_core_gpu_lib`. The test feeds `ps` the 15-character comm.
  - **`run-gpu.sh run <name>`**: a name is `[A-Za-z0-9_]+`; `--exact` beside `PCK_TEST_FILTER`
    is refused before any call, naming both. The existing refusal case `run everything` reached
    the VM after this (a well-formed name), so it became `run "every thing"`, still refused
    locally; the unstaged-name refusal is its own test. `PCK_TEST_FILTER` goes both as `-e` and as
    libtest's filter.
  - **`wired_nowhere`** reads `--test <target>` as a word pair in every folded workflow line,
    `--no-run` included (a build step is wiring too), plus both staging lists.
  - `ci_coverage`'s RED: with the target present and no exemption,
    `every_rust_test_target_is_named_by_ci` named `prototype_fused_benchmarks`.
  - Not run here, environmental: `test_image_any_user.py` (no 26.02 build image) and
    `test_run_image.py` (`PEACOCK_STAGED_INSTALLS` unset), 3 cases. `shellcheck` is not
    installed; `bash -n` on both scripts.
  - Next: Task 7 (the page's table). Task 9's build is the first real run of `benchmark_fused`.

## The hold rule (spec 1a)

All numbers are the sf40-baseline tree's, per origin node: Σ `batch_bytes` and Σ `batch_rows`
over lanes and batches. The rule is `decide`, `hold_inputs` and `HoldLedger::choose` in
`prototypes/fuser/mod.rs`.
- An edge's bytes are its child's. A project inserted to select from the shared load stands for
  the reader's own load. The edge right above the shared load carries the union, which nothing
  recorded, so it is not a candidate.
- A buffer above a join runs that join early, so it also keeps the join's build child's bytes,
  for every join below it on the path.
- An edge is allowed where every join below it on the path recorded no more rows than its child
  on the path. Under the cap's hold, also only where nothing at or under it keeps state.
- Placement: the allowed edge with the fewest bytes; a tie goes to the lower edge.
- A re-read costs the reader's own load's `loads.tsv` `load_us`, keyed by the query and the seq
  of its `CudfScan` recipe. It is recorded either way.
- The choice: a buffer where one is placed and its bytes fit the headroom, else a re-read.
- Headroom: `budget − (S + B + max root_bytes)`, saturating. `S`, the roots and any earlier
  buffers come from `cap_inputs` at the first decision; `B` is what this chooser has buffered
  since. Readers are decided in the order validation names them.

## The cap rule (spec 4)

`budget = pool − pool × 10 / 100`, the pool being the rmm pool the run installed (69 GiB
unless `PEACOCK_RMM_POOL_BYTES` says otherwise). The margin covers what the accountant's peak,
a lower bound, leaves out. `S = Σ shared_bytes` and `B = Σ buffer_bytes` are charged whatever
the cap. Roots are taken largest first; the cap is the largest k with `S + B + Σ top k ≤
budget`, at least 1. `over_budget` is `S + B + max root > budget`, and the run goes ahead.
Inputs: per shared load, its member with the largest recorded load bytes, scaled from that
member's planned bytes to the union's; per buffer, its decision's bytes; per root, its
`peak_bytes` less the recorded bytes of its loads now shared. `settle` loops: the cap, then
`resolve_cap_holds` under it with the ledger, until the cap stops falling.

## The fused format

`testdata/benchmark-results/h100-80gb-hbm3/fused/<ds>.sf40/tp1_single.fused.txt`, one
`== <batch>` section per batch in `BATCH_SIZES` order (`first3`, `first10`, `first20`, `all`).
Written by `render_fused_section` and read back by `parse_fused_section`
(`prototypes/fuser/section.rs`), the page's table and `every_fused_section_adds_up` alike. A
batch the list does not exceed is `skipped: same set as all: …`; a batch that panicked is
`status=failed reason=…` and the five provenance keys, as a tree section is.

```
--- batch ---
queries=[q1,q2,q3]
outcome=measured                  # or out-of-memory, then reason=<one line>
shared=[lineitem:q1+q3,orders:q3]
unshared=[q2/lineitem:limit-cut]
reads=[lineitem:2,orders:1]
buffers=1
buffer_bytes=123456789
rereads=1
hold query=q3 table=lineitem hold=GpuHashJoin choice=buffer above=7 bytes=123456789 reread_us=48120 why=fits: 123456789 <= 900000000
cap=2
pool_bytes=74088185856
budget_bytes=66679367270
estimate_bytes=51234567890
over_budget=false
answers=match                     # measured only
--- merge ---
saved_us=812
entry saved_us=640 members=q1:3,q3:5 node=GpuFilter: …
--- pipeline ---
model=<the model, one line>
load_order=[lineitem,orders]
serial_us=…
makespan_us=…
saved_us=…
fused_measured_us=…               # measured only
pipelined_estimate_us=…           # measured only
--- tree ---                      # measured only
<render_timings_of_roots of the chosen run>
--- run ---
<run_section of the chosen run; out of memory: the five provenance keys alone>
```

Keys:
- `queries`: the roots in order, the batch's prefix of the H100 `cases.csv` list.
- `shared`: per shared load, its table and the queries reading it, a query once per scan.
  `unshared`: `<query>/<table>:<why>` per scan left alone beside others of its file, `why`
  `limit-cut`, `nulls:<column>` or `rows-only`.
- `reads`: per table, the distinct loads of it in the fused plan: one per shared load, plus each
  re-read and unshared scan. `Σ reads` is the tree's distinct `GpuLoadParquet` nodes.
- `buffers`, `buffer_bytes`, `rereads`: the count of `choice=buffer` lines, the Σ of their
  `bytes`, and the count of `choice=re-read` lines.
- `hold`: one per held reader, in the order they were resolved. `hold` is the node whose hold it
  enters (`GpuHashJoin`, or `GpuUnload` for a root waiting on the cap). `above` is the pre-order
  index, in that query's own tree, of the node the buffer sits on (`-` for a re-read). `bytes`
  is what a buffer would keep, `reread_us` what a re-read costs (`loads.tsv`), recorded either
  way. `why` is last and keeps its spaces: `fits: …`, `too large: …`, or `no edge: …`.
- `cap`, `pool_bytes`, `budget_bytes`, `estimate_bytes`, `over_budget`: the cap rule below, the
  pool being what the run installed. `estimate_bytes <= budget_bytes` unless `over_budget`.
- `answers=match`: every root's answer equalled its query run alone on the same device; a
  mismatch fails the batch, so no other value is written.
- `--- merge ---`: the merge hint. `saved_us` the batch's total, then one `entry` per group,
  largest saving first; `members` are `<query>:<pre-order index in its own tree>`; `node` is last
  and keeps its spaces.
- `--- pipeline ---`: the pipelining hint, the model stated on one line. `load_order` the tables
  in the order the model loads them; `serial_us`, `makespan_us`, `saved_us = serial − makespan`.
  `fused_measured_us` is the chosen run's `load_us + execute_us` (`device_us`), and
  `pipelined_estimate_us = fused_measured_us − saved_us`, saturating.
- `--- tree ---`: `root <i>:` before each root; a node several roots read is written once as
  `@<k> <line>` and named after as `@<k>`; each node's time line beneath it, bench-rows' format.
- `--- run ---`: bench-rows' trailer, its arithmetic over the tree with each shared node once.
  `runs` holds `MEASURED_RUNS` times, `FUSED_ALL_RUNS` for `all`; `run_us` is its 2nd-smallest.
  `pinned_cache` is the cache at the batch's end, `prefill_*` this batch's own prefill call: a
  later batch whose files an earlier one loaded reports `prefill_files=0`.
- An out-of-memory batch keeps `--- batch ---` (its holds, cap and estimate), `--- merge ---`
  and the pipeline's model lines; no tree, and its `--- run ---` is the five provenance keys,
  which is what a resumed run keeps it by.

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 6 | `09949303` | fuser: one shared load per file across N plans; lib 1121, goldens 61, formatting 38 |
| Task 2 Step 5 | `9ab20eed` | fuser: the ordered lists and fused answers against the corpus goldens; lib 1131, goldens 61, formatting 38 |
| Task 3 Step 5 | `0d677c19` | fuser: the mergeability hint; lib 1141, goldens 61, formatting 38 |
| Task 4 Step 5 | `018f4c70` | fuser: the pipelining hint; lib 1147, goldens 61, formatting 38 |
| Task 5 Step 5 | `54596367` | fuser: buffer or re-read, and the concurrency cap rule; lib 1171, goldens 61, formatting 38 |
| Task 6 (part A) | `5af21f1e` | fuser: the fused format and the benchmark's host-side rules (Task 6, part); interim, developer context; lib 1181, goldens 64, formatting 38, gpu check clean |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
