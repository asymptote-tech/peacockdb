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

- 2026-11-06, Task 7 (developer). Checkpoint `fuser: the report page's fused-batches table`
  reported, not committed. `--lib` 1191 → 1197 passed (+6 `prototypes::fuser::report::tests`),
  goldens 64 with the page test unchanged (no fused file committed), formatting 39.
  - **The page's units are bench-rows' own**: `benchmark_report.rs`'s `ms` and a new `gib` (the
    peak cell's former inline format) are `pub(crate)`, reached as `test_support::report_ms` and
    `report_gib`, so the fused tables cannot drift from the main ones.
  - **A failed batch** (`status=failed reason=…`) is a row too, outcome `failed: <reason>`
    (class `off`), which the plan did not list; test `a_failed_batch_says_why`.
  - Order: device directory name, then dataset, then sf as a number (`sf40` before `sf100`);
    test `tables_go_by_device_then_dataset_then_scale`. A row's link is its `== <batch>` line,
    as the main tables link a section.
  - The link-line mutation (`at` for `at + 1`) failed two tests.

- 2026-11-06, Task 8 (developer). Checkpoint `fuser: an Nsight profile per fused batch`
  reported, not committed. `--lib` 1197 → 1199 passed (`benchmark::tests` 12 → 14: one replaced,
  three added), `test_run_gpu.py` 45 → 49. Two Task 6 defects found while wiring the metrics
  pass, fixed here with tests that were red against Task 6's behaviour:
  - **A captured fused run wrote into the clean record.** Its heading says `capture=metrics`, so
    the append panicked on the heading mismatch (and had it not, the profiler's times would
    have replaced the clean run's). Now `record_path_for`: a clean run writes the batch's record,
    a captured one only `PEACOCK_RECORD_PATH` (or nothing), as a captured corpus case does. Test
    `a_captured_run_never_writes_the_clean_record`.
  - **The record's `query` was the test name** (`bench_fused_…`), the case range's word is the
    batch, and `nsys_hbm.py` joins rows to calls by `(dataset, sf, query, mode)`, so no fused join
    could find its rows. The rows' `query` is now the batch (`batch_meta`, `case_range`). Test
    `the_record_names_a_batch_as_its_case_range_does`. `case_name` and `replace_case_rows` are gone.
  - **A record per batch**, `calibration/fused/<ds>.sf40/<batch>.records.tsv`, written whole by
    each run (`write_batch_record`), replacing the plan's one `records.tsv` per dataset. The
    heading's `allocator=` line carries the free memory the pool saw, which is per process, and
    Task 9 runs one process per batch: a shared file would have refused the second batch's
    append whenever free memory moved by 0.1 GiB, failing that batch. Test
    `a_rerun_batch_writes_its_record_afresh` (two headings, the second kept whole).
  - **`create_nsys_profile.sh --fused <ds>.sf<sf>`**: the host-pass helpers (`switches`,
    `remote_pass`) moved above the corpus's checks, unchanged, so the fused branch runs before
    them and exits; the corpus path is untouched. `PCK_TEST_FILTER`, `--cases` and `--device`
    beside `--fused` are refused before any call. The metrics pass needs no record filtering:
    each batch joins against its own record on the host, and the merge is given only the
    batches that joined, record and part each. A batch with no record (a marker, or a run that
    never wrote one) fails its join and is a `failed.tsv` line. Trace captures come home as
    `capture-<batch>.sqlite` beside the derived files.
  - The plan's failed-capture test fails `all`; it fails `first10` here, so it shows the batches
    after a failure still run (a `return` in place of `continue` turned it red). Its metrics test
    gives the reader a stub that refuses a missing `--record`, as the real one does.
  - `shellcheck` is not installed here: Step 3's check is `bash -n` only, and the reviewer or
    the CPU VM should run `shellcheck scripts/create_nsys_profile.sh`.

**For Task 9 (a fresh developer): what this chain left you.**
- Binary: `cpp/install/rust-benchmarks/prototype_fused_benchmarks`, staged by
  `scripts/env/stage-install.sh` (release, `--features gpu,test-support`) beside
  `peacock_gpu_benchmarks`. `<dir>/rust-benchmarks/prototype_fused_benchmarks*` is Step 1's check.
  It has never run: this host's `libpeacock` predates `peacock_join_build`, so Task 9's run is
  the first execution of `benchmark_fused` (it type-checks and links).
- Tests: `bench_fused_{tpch,tpcds}_sf40_{first3,first10,first20,all}_tp1_single`; run one per
  process, `run-gpu.sh … run prototype_fused_benchmarks --exact <test>`. `--exact` beside
  `PCK_TEST_FILTER` is refused. The log is `~/peacock/<i>/logs/prototype_fused_benchmarks.log`
  (each run overwrites it).
- What a batch reads at run time, all in the shipped committed testdata: the H100 `cases.csv`,
  `benchmark-results/h100-80gb-hbm3/<ds>.sf40/tp1_single.benchmark.txt` and
  `calibration/h100-80gb-hbm3/<ds>.sf40/loads.tsv`. `every_batched_query_has_its_baseline_and_its_loads`
  checks every batched query has both, locally.
- What it writes on the host: `benchmark-results/h100-80gb-hbm3/fused/<ds>.sf40/tp1_single.fused.txt`
  and `calibration/fused/<ds>.sf40/<batch>.records.tsv`. `run-gpu.sh pull` brings both (all of
  `benchmark-results/` and `calibration/`). Do not set `PEACOCK_RECORD_PATH` or
  `PEACOCK_PINNED_HOST_CACHE_BYTES` for the clean runs.
- Resume: `PEACOCK_BENCH_RESUME=1` keeps a batch whose section (measured or out of memory) ends
  with this run's five provenance keys; a failed section runs again.
- Profiles (Step 5): `create_nsys_profile.sh --fused <ds>.sf40` with the H100 arguments and
  `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12`, after both datasets' clean runs, so
  each batch's record is on the host for the metrics pass. It writes locally into
  `testdata/calibration/fused/<ds>.sf40/`; the trace captures (`capture-*.sqlite`) are large and
  are not to be committed.
- Task 10 Step 2's copy to `calibration/h100-80gb-hbm3/fused/<ds>.sf40/` is `<batch>.calls.tsv`,
  `<batch>.loads.tsv`, `hbm.tsv` and `failed.tsv` only: not `*.records.tsv`, not `capture-*`.
- Watch: `idle_watch.py` names the binary in full; Step 2's `install-watch` puts that list on the
  VM before any fused run. A batch over 4 hours is stopped by the cap.
- Not exercised anywhere yet: the out-of-memory path end to end, the first20 marker path (neither
  list is short enough), and the answer check against each query alone on the device.

- 2026-11-06, Task 9 (developer), **stopped on a fuser bug after tpch first3 and first10**;
  the fix is made locally, test first, and not committed. Nothing is committed or staged.
  - **Build (Step 1).** nebius-cpu128: `df -h /` 60 GB free; `nebius-sync.sh … --no-data`,
    `check-build-image.sh` exit 0, `stage-install.sh` in the build image exit 0 (about 6 min,
    warm `target-img-25.02`), rsynced to `/home/dmitry/build/peacock/query-fuser/install`
    (749 MB, both `rust-benchmarks/` binaries), VM stopped.
  - **H100 (Step 2).** `df -h /` 79 GB free, card idle. `install-watch` **without**
    `--key-file`: editor-eu's key is embedded, and build-test.md says install-watch refuses a
    key file for it (the plan's `--key-file ~/.nebius/sa-key.pem` is drift). Exit 0; the
    installed watcher names `prototype_fused_benchmarks`. `prepare` exit 0 in 11 min (sf40
    kept 8 + 24 files, the time is their checksums). `ship --install` exit 0; the five
    run-time inputs are on the VM.
  - **tpch (Step 3).** `first3` measured: `run_us=933135` (load 593180, execute 335989), cap 3,
    8 buffers, 1 re-read, accountant's peak 42.4 GB, `answers=match`, prefill 8 files
    42.4 GB in 22.5 s; 35 s wall. `first10` measured: `run_us=4065107` (load 3261477, execute
    786620), cap 1, `over_budget=true`, 0 buffers and 38 re-reads (lineitem read 9 times),
    accountant's peak 26.8 GB, `answers=match`; 73 s wall. `first20` and `all` **failed** in
    4 s each, before any device work, `status=failed` sections written:
    `does not settle under its cap: invalid plan: GpuCoalesceAllBatches (node 24) sits below a
    GpuHoldBuffer of the root GpuUnload (node 10), which waits for the cap`. A code bug, not
    an out-of-memory outcome, so per the dispatch: pulled, H100 stopped, nothing more run.
  - **Not run:** the four tpcds batches and both profiles (Steps 4 and 5).
  - **Root cause.** `hold_inputs` forbade state below a buffer only for the cap's own hold
    (`dag.roots.contains(&held.hold)`). A *join's* hold, decided in `fuse` before any cap
    exists, may buffer above a `GpuCoalesceAllBatches`. When `settle` then makes that root
    wait, dag-driver's `capped_roots_keep_their_state` refuses it (`UncappedAccumulator`), and
    `resolve` passed that refusal straight out. `first10` (cap 1) passed only because none of
    its join buffers sat above state. RED: `a_join_hold_in_a_root_that_waits_is_never_buffered_above_its_state`
    (`cap::tests`) failed with the device's exact refusal.
  - **Fix** (`fuse.rs`, `holds.rs`, `mod.rs`): `HeldReader.waits` (its root's ordinal is at or
    past the cap being resolved) replaces the cap-hold test in `hold_inputs`, so every hold in
    a waiting root keeps state above its buffer. `resolve` under a cap answers
    `UncappedAccumulator` by taking out the lowest buffer above the named node, and its
    record (`without_the_buffer_above`, copying the DAG as `edited` does, the shared-source
    copy now one helper, `shared_copy`); validation then names that reader again and it is
    decided with `waits`. The ledger's `B` is now the buffers the fused plan holds at each
    decision, not a running sum, so a buffer taken out stops being charged. A mutation
    leaving `waits` false makes `resolve` loop (the test hangs; killed at 60 s).
  - **Checks:** `--lib` 1199 → 1200 passed, 2 ignored; `test_corpus_goldens` 63 of 64 with
    the pulled fused file present: `the_benchmark_report_matches_the_trees` fails only because
    the page lacks the new fused tables (Task 10 regenerates it), and passes with the file set
    aside, as does `every_fused_section_adds_up` over it; cost-report formatting 39;
    `cargo-cudf.sh check --features gpu,test-support --tests` clean; rustfmt on the touched
    files. build-test.md: `--lib` 1201 → 1202 and the cap row 24 → 25.
  - **Pulled, not committed:** `testdata/benchmark-results/h100-80gb-hbm3/fused/tpch.sf40/tp1_single.fused.txt`
    (first3, first10 measured; first20, all failed), `testdata/calibration/fused/tpch.sf40/{first3,first10}.records.tsv`
    (gitignored), the logs in `~/.cache/peacock/run-gpu/peacockdb-beta/`. The pull also
    brought `testdata/calibration/hbm-parts/` (79 `bench_tpcds_sf40_*` parts): sf40-baseline's
    metrics-pass leftovers on the VM, untracked, not this task's; do not commit them.
  - **For the rerun (a new H100 session, after the fix is committed and restaged):**
    - Rebuild on nebius-cpu128 first: the shipped binary has the bug.
    - The first3 and first10 sections were written by the old binary. Their decisions should
      be the same under the fix (their join buffers held no state below, and the cap's holds
      were already ruled this way), but `PEACOCK_BENCH_RESUME=1` keeps a section by its five
      provenance keys, which do not name the binary. Running tpch first3 without resume costs
      about 2 minutes and leaves one binary's file.
    - Step 5 needs `PCK_BENCH_HBM_FREQ=10000` beside the plan's two variables: build-test.md
      ("sf40 baseline") says the H100 read 101 % of peak at the default 20 kHz.
    - dev's `/` has 12 GB free. The trace pass brings each batch's `capture-<batch>.sqlite`
      home, and the corpus's capture was 1.2 GB, so watch `df -h /` during Step 5.

- 2026-11-06, Task 9 (developer), **rerun after the fix (`03b7af97`): every batch measured,
  both profiles complete.** Nothing committed or staged.
  - The first run's pulled files were set aside to `/tmp/qf-run1/`, and the VM's old
    `fused/` results and records removed after `ship`, so the new files are one binary's
    (resume keys a section by provenance, which does not name the binary).
  - **Build.** nebius-cpu128 (60 GB free): sync, `check-build-image.sh` exit 0,
    `stage-install.sh` exit 0 in 4 min, the install rsynced to
    `/home/dmitry/build/peacock/query-fuser/install`, VM stopped.
  - **H100.** 79 GB free, card idle. `install-watch` (no key file) exit 0. `prepare` skipped:
    run at 05:39 this day, and the fix touched no image input (`run` checks the label and
    would have refused, exit 2). `ship` exit 0, the five inputs present.
  - **Batches**, one process each; tpch first3 without `PEACOCK_BENCH_RESUME`, the rest with
    `=1`. All eight exit 0, `outcome=measured`, `answers=match`; none out of memory, no
    marker. µs are the 2nd-min run, then load and execute; peak is the accountant's. These are
    round 0's figures, before review round 1; the rerun's are in the Task 10 entry.

    | batch | run_us | load_us | execute_us | cap | over_budget | buffers | re-reads | peak GB | wall |
    |---|--:|--:|--:|--:|---|--:|--:|--:|--:|
    | tpch first3 | 931199 | 590808 | 336098 | 3 | false | 8 | 1 | 42.4 | 475 s |
    | tpch first10 | 4062112 | 3259796 | 785957 | 1 | true | 0 | 38 | 26.8 | 73 s |
    | tpch first20 | 6259660 | 4791518 | 1440222 | 1 | true | 37 | 40 | 54.2 | 102 s |
    | tpch all | 7222597 | 5408423 | 1781233 | 1 | true | 40 | 46 | 54.2 | 63 s |
    | tpcds first3 | 595427 | 286540 | 305166 | 3 | false | 12 | 1 | 3.7 | 169 s |
    | tpcds first10 | 4360983 | 1588374 | 2744943 | 1 | true | 67 | 19 | 45.1 | 72 s |
    | tpcds first20 | 12060453 | 6261687 | 5735083 | 1 | true | 92 | 118 | 34.7 | 163 s |
    | tpcds all | 36987469 | 22305327 | 14443467 | 1 | true | 102 | 642 | 37.3 | 234 s |

    tpch first3's 475 s and tpcds first3's 169 s are mostly the prefill on a page cache the
    restart emptied; the run before took 35 s for tpch first3.
  - **Profiles**, `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12
    PCK_BENCH_HBM_FREQ=10000` (the frequency is build-test.md's; the plan's Step 5 omits it).
    tpch and tpcds each exit 0: both passes over all four batches, a `<batch>.calls.tsv` and
    `<batch>.loads.tsv` per batch, `hbm.tsv` merged from 4 parts (tpch 1467 calls, tpcds
    7112). No `failed.tsv`: no batch failed a capture or a join. Trace captures 21-565 MB
    each; dev's `/` went from 12 GB to 8.9 GB free.
  - `every_fused_section_adds_up` passes over both files, locally, with the VMs stopped.
  - **Pulled, not committed:** `testdata/benchmark-results/h100-80gb-hbm3/fused/{tpch,tpcds}.sf40/tp1_single.fused.txt`;
    `testdata/calibration/fused/{tpch,tpcds}.sf40/` (gitignored): the per-batch `calls.tsv`,
    `loads.tsv`, `records.tsv`, `hbm.tsv`, `hbm-parts/`, `records-metrics.tsv` and the
    `capture-*` files; logs in `~/.cache/peacock/run-gpu/peacockdb-beta/` and
    `/tmp/qf2-*.log`. `testdata/calibration/hbm-parts/` came back with the pull
    (sf40-baseline's leftover on the VM): untracked, not to be committed.
  - **For Task 10:** copy to `calibration/h100-80gb-hbm3/fused/<ds>.sf40/` only
    `<batch>.calls.tsv`, `<batch>.loads.tsv` and `hbm.tsv` (no `failed.tsv` exists this time);
    not `*.records.tsv`, `records-metrics.tsv`, `hbm-parts/` or `capture-*`. The page test
    fails until regenerated, because the new fused tables are missing from it.

- 2026-11-06, Task 10 (developer), Steps 1-4 **to the two commits** (not made). All local,
  every VM stopped.
  - **Step 1, read back.** `every_fused_section_adds_up` passes over both files. All eight
    sections are `outcome=measured` with `answers=match`; no section failed, so no ticket.
  - **Protocol.** One process per batch. The batch's files are prefilled once into the pinned
    host cache before any run (`prefill_*` below; no batch failed `prefill:` or `pinned
    cache:`), then 1 warm-up and the 2nd-smallest of 10 measured runs, of 3 for `all`, every
    run reading pinned buffers. The rmm pool is the benchmark's 69 GiB. Each root's answer is
    checked against its query run alone on the device.
  - **Per batch** (replaced by the rerun after review round 2, below; round 1's numbers are
    in git at `6e66896b`, round 0's at `57be22b8`). The completeness pass's rerun executed the
    same plans: its times are within ±1.5 % of these, and its file is the committed one. Shared and unshared are counts of loads;
    buffers are count and Σ bytes; re-reads count and Σ `reread_us`; load and execute in ms;
    peak is the accountant's (a lower bound), GB; cache is `pinned_cache=`'s `resident`, then
    this batch's prefill; wall is the test process's.

    | batch | queries | shared | unshared | buffers | re-reads | cap | over budget | outcome | load | execute | peak | cache; prefill | wall |
    |---|---|--:|--:|---|---|--:|---|---|--:|--:|--:|---|--:|
    | tpch.sf40 first3 | 3 (q1..q3) | 4 | 0 | 8, 5.42 GB | 1, Σ 43 ms | 3 | false | measured | 591 | 336 | 42.4 | 39.5GiB; 8 files, 42.4 GB, 461.5 s | 474 s |
    | tpch.sf40 first10 | 10 (q1..q10) | 5 | 0 | 19, 14.01 GB | 23, Σ 1679 ms | 1 | true | measured | 2138 | 807 | 46.3 | 39.5GiB; 8 files, 42.4 GB, 22.0 s | 61 s |
    | tpch.sf40 first20 | 20 (q1..q20) | 8 | 0 | 42, 18.84 GB | 35, Σ 2709 ms | 1 | true | measured | 3421 | 1481 | 55.6 | 39.5GiB; 8 files, 42.4 GB, 21.4 s | 86 s |
    | tpch.sf40 all | 22 (q1..q22) | 8 | 0 | 48, 24.08 GB | 38, Σ 2982 ms | 1 | true | measured | 3729 | 1821 | 57.5 | 39.5GiB; 8 files, 42.4 GB, 22.1 s | 57 s |
    | tpcds.sf40 first3 | 3 (q1..q3) | 3 | 0 | 12, 4.27 GB | 1, Σ 34 ms | 3 | false | measured | 284 | 305 | 3.7 | 13.9GiB; 8 files, 14.9 GB, 161.1 s | 169 s |
    | tpcds.sf40 first10 | 10 (q1..q10) | 9 | 0 | 62, 27.91 GB | 24, Σ 1168 ms | 2 | false | measured | 1511 | 2759 | 40.9 | 14.6GiB; 16 files, 15.7 GB, 13.6 s | 71 s |
    | tpcds.sf40 first20 | 20 (q1..q23) | 9 | 0 | 108, 9.73 GB | 101, Σ 5180 ms | 2 | false | measured | 5235 | 5746 | 30.8 | 14.7GiB; 20 files, 15.8 GB, 8.2 s | 152 s |
    | tpcds.sf40 all | 79 (q1..q99) | 18 | 0 | 346, 8.05 GB | 402, Σ 18741 ms | 1 | false | measured | 17290 | 14523 | 43.3 | 14.7GiB; 24 files, 15.8 GB, 7.7 s | 221 s |

    Reads per table, from each section (one per shared load, plus each re-read and unshared
    scan):
    - tpch.sf40 first3: `reads=[lineitem:1,region:1,nation:1,supplier:1,part:1,partsupp:2,customer:1,orders:1]`
    - tpch.sf40 first10: `reads=[lineitem:4,region:1,nation:8,supplier:6,part:1,partsupp:3,customer:5,orders:3]`
    - tpch.sf40 first20: `reads=[lineitem:7,region:1,nation:7,supplier:9,part:2,partsupp:6,customer:6,orders:5]`
    - tpch.sf40 all: `reads=[lineitem:8,region:1,nation:7,supplier:10,part:2,partsupp:6,customer:6,orders:6]`
    - tpcds.sf40 first3: `reads=[store:1,date_dim:1,store_returns:2,customer:1,web_sales:1,catalog_sales:1,item:1,store_sales:1]`
    - tpcds.sf40 first10: `reads=[store:2,date_dim:2,store_returns:2,customer:3,web_sales:6,catalog_sales:5,item:3,store_sales:6,catalog_page:1,catalog_returns:1,web_site:1,web_returns:1,customer_address:3,promotion:1,customer_demographics:2,reason:1]`
    - tpcds.sf40 first20: `reads=[store:5,date_dim:2,store_returns:2,customer:15,web_sales:16,catalog_sales:17,item:20,store_sales:24,catalog_page:1,catalog_returns:2,web_site:1,web_returns:1,customer_address:5,promotion:1,customer_demographics:3,reason:1,household_demographics:1,call_center:1,warehouse:1,inventory:2]`
    - tpcds.sf40 all: `reads=[store:16,date_dim:27,store_returns:15,customer:44,web_sales:50,catalog_sales:42,item:44,store_sales:77,catalog_page:2,catalog_returns:14,web_site:4,web_returns:12,customer_address:28,promotion:3,customer_demographics:11,reason:2,household_demographics:6,call_center:3,warehouse:8,inventory:5,ship_mode:3,income_band:5,time_dim:1,web_page:4]`
  - **The first3 walls** are the prefill reading from a page cache the VM's restart had
    emptied (tpch `prefill_us` 462 s, tpcds 161 s); the later batches found the files cached.
  - **Against the queries run alone.** The summed sf40-baseline `run_us` of a batch's queries
    (`tp1_single.benchmark.txt`, same device and protocol, unchanged since sf40-baseline),
    against the fused run's, ms:

    | batch | queries | Σ alone (load + execute) | fused (load + execute) | fused / Σ alone |
    |---|--:|---|---|--:|
    | tpch first3 | 3 | 1004 (667 + 334) | 930 (591 + 336) | 0.93
    | tpch first10 | 10 | 3501 (2709 + 782) | 2958 (2138 + 807) | 0.84
    | tpch first20 | 20 | 6355 (4917 + 1419) | 4924 (3421 + 1481) | 0.77
    | tpch all | 22 | 7488 (5707 + 1760) | 5578 (3729 + 1821) | 0.74
    | tpcds first3 | 3 | 718 (407 + 308) | 593 (284 + 305) | 0.83
    | tpcds first10 | 10 | 6238 (3458 + 2758) | 4298 (1511 + 2759) | 0.69
    | tpcds first20 | 20 | 13962 (8168 + 5748) | 11041 (5235 + 5746) | 0.79
    | tpcds all | 79 | 38671 (24066 + 14455) | 32043 (17290 + 14523) | 0.83

  - **The merge hint**, across queries only since the completeness pass (`7b1db00e`), against
    round 2's, which also counted a query's repeats of itself. µs; the largest entry's members
    as `<query>:<pre-order index>`:

    | batch | round 2 saved_us, entries | saved_us, entries | largest entry |
    |---|--:|--:|---|
    | tpch first3 | 266, 1 | 0, 0 | — |
    | tpch first10 | 266, 1 | 0, 0 | — |
    | tpch first20 | 4416, 4 | 20, 1 | 20, `GpuCoalesceAllBatches` over supplier, q2:36 + q5:16 |
    | tpch all | 13965, 5 | 20, 1 | 20, the same |
    | tpcds first3 | 77539, 2 | 0, 0 | — |
    | tpcds first10 | 272407, 11 | 282, 1 | 282, `GpuCoalesceAllBatches` over date_dim, q1:15 + q1:26 + q7:15 |
    | tpcds first20 | 658387, 18 | 1518, 3 | 631, `GpuCoalesceAllBatches`, q4 ×3 + q11 ×2 |
    | tpcds all | 1050938, 79 | 92846, 31 | 32108, `GpuAggregateBatches` by customer name, q38:32 + q87:32 |

    A group spanning two queries keeps every member, a query's own repeats among them
    (tpcds first20's largest holds three q4 copies). Merging identical subtrees across queries
    would save at most 93 ms of tpcds all's 14.5 s of execute, and nothing measurable in tpch.
  - **An honest reading.** Every batch runs faster than its queries alone, by 7-31 %. The two
    first3 batches run at cap 3 and tpcds first10 and first20 at cap 2, within the 90 % budget.
    tpcds all runs at cap 1 within budget: a second root does not fit beside its 8 GB of
    buffers. tpch first10, first20 and all run at cap 1 over budget (an estimate of 86-91 GB
    against 66.7 GB). Over budget a buffer smaller than its re-read is kept, so the large
    batches now buffer most holds: tpch all 48 buffers and 38 re-reads, lineitem loaded 8
    times; tpcds all 346 buffers and 402 re-reads, `date_dim` loaded 27 times. Execute is
    within 5 % of the summed alone execute in every batch, so whatever is saved is load time
    (tpcds all: 17.3 s of load against 24.1 s alone). The fused time still grows faster than
    the batch size (tpch 3 → 22 queries: 0.93 → 5.6 s; tpcds 3 → 79: 0.59 → 32 s), largely
    because later queries cost more alone. One cost of pricing the plan as it stands: inside
    a resolve, a re-read charges its root its own load at once, while the group's union is
    rebuilt only when the resolve ends. So the ledger can read over budget mid-resolve though
    the settled plan is within it; tpcds first10 did, from its 32nd decision, after q4's two
    `catalog_sales` re-reads, and net 5 of round 1's buffers became re-reads (6 to re-reads, 1 to a buffer) (+97 ms). The
    merge hint finds little to merge across queries (above); each section's pipelining hint
    estimates what overlapping loads with execution would add.
  - **Step 2, the profiles** (the round-2 rerun's, `ec6935dd`, built from `51b27ddf`; they
    stand for the completeness pass's binary, whose executed plans are the same). All 725 rows of the eight `loads.tsv` have
    `load_us > 0`, `host_read_us ≤ load_us`, `unattributed_us ≤ load_us` and
    `off_thread_device_us ≤ h2d_us + decompress_us + decode_us + other_device_us` (a script
    over the files; no violation). `host_read_source=none` throughout: the reads come from the
    pinned cache, so no OS read falls in a load's window. tpch first3's loads, µs per execution
    (median), by recipe seq:

    | seq | load_us | host_read_us | h2d_us | h2d_bytes | decompress_us | decode_us | other_device_us |
    |--:|--:|--:|--:|--:|--:|--:|--:|
    | 0 | 247200 | 0 | 63953 | 2898624785 | 74885 | 42688 | 12655 |
    | 12 | 1332 | 0 | 19 | 2351 | 12 | 76 | 84 |
    | 15 | 1330 | 0 | 19 | 4470 | 29 | 89 | 85 |
    | 17 | 163880 | 0 | 600 | 31676072 | 107709 | 56441 | 100 |
    | 19 | 39475 | 0 | 1526 | 49631672 | 29357 | 8484 | 153 |
    | 22 | 41389 | 0 | 7614 | 328784853 | 27202 | 1093 | 604 |
    | 40 | 41469 | 0 | 7610 | 328784853 | 27191 | 1093 | 602 |
    | 60 | 36285 | 0 | 798 | 26432232 | 29246 | 4817 | 119 |
    | 63 | 48826 | 0 | 7231 | 299761060 | 31423 | 2187 | 331 |

    The shared lineitem load (seq 0): 64 ms copying 2.9 GB (45 GB/s, pinned) against 118 ms
    decompressing and decoding, 13 ms of other device work and 57 ms unattributed, of 247 ms.
    Copied with `cp -p` to `testdata/calibration/h100-80gb-hbm3/fused/<ds>.sf40/`:
    `<batch>.calls.tsv`, `<batch>.loads.tsv`, `hbm.tsv` (no `failed.tsv` exists).
  - **A gap the plan left, fixed here:** `testdata/.gitignore`'s `/calibration/*-*/*/*`
    ignored the device's `fused/<ds>.sf40/` directories, so the profiles' files could not be
    committed. Added: the dataset directory under `fused/` re-included, everything in it
    ignored, then `*.calls.tsv`, `*.loads.tsv`, `hbm.tsv` and `failed.tsv` re-included.
    `git check-ignore` shows a `*.records.tsv` there, the device's fetched records and the
    `calibration/fused/` scratch area all still ignored. The file is not an image or data
    input (`scripts/lib/image-inputs.sh`, `data-inputs.sh`).
- 2026-11-06: plan complete (Task 10 committed 5551e9b8, 57be22b8). PR #200 against `ENS-dag-driver`; board `reviewing`; reviewer round 1 dispatched.
- 2026-11-06: review round 1 returned (see "Review round 1"); developer dispatched on B1, I1, I2 and the code nits, then an H100 rerun.

- 2026-11-06, review round 1 (developer): B1, I1, I2 and nits 2-8, local, test first; nits 1
  and 9 were the coordinator's. Checkpoint reported, not committed; the H100 rerun waits for it.
  - **B1, a group rebuilt over the readers it has left.** `resolve` ends with `regrouped`, so
    `fuse` and every `settle` round do: each group's load is rebuilt as the union of its
    remaining members' own loads (`union_of`, the metadata read again), every reader re-pointed
    through `consumer`, and a member left alone reads its own load, its group dropped from
    `shared`. The cap reads the rebuilt loads, since `settle` computes it after each resolve.
    RED: `a_group_left_with_one_reader_reads_its_own_load` (`shared` still held q1 alone) and
    `a_group_that_lost_a_reader_loads_only_what_the_rest_read` (the union kept the re-read
    q0's column: `[0, 1, 2, 4]`); the second also runs the fused roots on the cpu against each
    alone. Four tests pinned the old behaviour and were changed: `a_held_reader_is_re_read`
    and `section::a_rendered_section_reads_back` (no group of one in `shared`),
    `a_re_read_load_is_charged_in_full` (q1's root keeps its load, `[1000, 500]`, nothing
    shared), `plan_files_of_a_fused_batch_is_its_queries_union` (buffers, so lineitem stays
    shared), and `tpch_q3_q5_q10_fused_answer_as_alone`, which now asks for the three shared
    tables under buffering only, no group of one under either chooser, and still checks every
    answer under both.
  - **I1.** After the chooser decides a waiting reader, a buffer with state at or under its
    edge is refused, `PlanError::Invalid` naming the chooser, the edge, the query, the table and
    the stateful node (`refuse_state_below`). RED: `a_chooser_buffering_a_waiting_reader_above_state_is_refused`
    hung until killed at 60 s; green it returns the error.
  - **I2.** `decide` takes `Headroom::Left(bytes) | Over`; `HoldInputs.reread_bytes` is the
    reader's own load's recorded `batch_bytes`. Over budget, a placed buffer is kept where its
    bytes are below `reread_bytes` (`over budget: b < re-read r`), else a re-read
    (`over budget: b >= re-read r`). "The hold rule" below says so. Three `decide` tests and a
    `reread_bytes` assertion in `hold_inputs_read_the_baseline_and_loads_tsv`; RED was the
    build (no `Headroom`, no `reread_bytes`).
  - **Nits.** 2: `merge_hint`'s and `hold_inputs`'s docs cut to 8 lines, the latter pointing to
    "The hold rule". 3: the gate reads `prototypes` as a whole identifier, `mod prototypes;`
    aside, so `use crate::prototypes as p;` and `use crate::{plan, prototypes};` are caught
    (RED on the first); architecture.md, build-test.md and `prototypes/mod.rs` say so. 4:
    `RowsOnly`'s doc says the refusal covers a lone scan on either side of the split, as
    `share_groups` does. 5: `fused_problem` refuses a measured section without
    `answers=match` (two doctorings, RED on `answers=mismatch`). 6: `baseline_for` says why
    the first section is the one (`merged_text` writes each query once). 8:
    `create_nsys_profile.sh`'s usage gives `PCK_BENCH_HBM_FREQ=10000` and why.
  - **Nit 7 not done, deliberately.** A held reader's path never climbs through a buffer: the
    regions validation names stop at buffers (`region_of_edge` and `held_region_of_node` are
    `Through::NoBuffer`), so a reader below a buffer is outside the region. Building one
    (nested joins, the outer hold buffered under the inner join) showed the inner join is
    never named. Looking through a buffer in `recorded()` would guard a case validation
    cannot produce.
  - **Not done here, for the reviewer's eye:** a buffer placed for a reader whose group a
    later re-read dissolves stays, though its reader now reads its own load. It is still
    correct, only kept bytes nobody shares. With I2 buffering more over budget, this may
    matter in the rerun's figures.
  - **Timing:** the fuser suite ran 13 s, then 157 s, then 13 s on the same code;
    `a_capped_first10_answers_the_corpus_goldens_on_the_cpu` once took 167 s alone and three
    times 11-12 s. Without the regroup the suite was 13 s once. Not explained.
  - **Checks:** `--lib` 1206 passed, 2 ignored (+6: fuse 3, holds 3); `test_corpus_goldens`
    64; cost-report formatting 39; `test_run_gpu.py` 49; `cargo-cudf.sh check --features
    gpu,test-support --tests` clean; rustfmt on every touched file; `bash -n` on the script.
    build-test.md: `--lib` 1208, the fuser row 17, the hold-rule row 28.

- 2026-11-06, Task 9 and Task 10 Steps 1-4 (developer), **the H100 rerun after review round 1
  (`0d46064d`): every batch measured, both profiles complete.** Nothing committed or staged.
  - **Set aside first.** dev's `/` had 8.6 GB free. The old scratch area's eight
    `capture-*.sqlite` were deleted, and the rest of `testdata/calibration/fused/` moved to
    `/tmp/qf-run2/calibration-fused/` (11 GB free after). On the H100, after `ship`, the shipped
    round-0 `benchmark-results/h100-80gb-hbm3/fused/` and the old `calibration/fused/` were
    removed, so resume could keep no stale section. The committed local copies stayed in place
    and the pull overwrote them.
  - **Build.** nebius-cpu128 (60 GB free): sync, `check-build-image.sh` exit 0,
    `stage-install.sh` exit 0, the install rsynced to `/home/dmitry/build/peacock/query-fuser/install`
    (`prototype_fused_benchmarks` staged 08:51), VM stopped.
  - **H100.** 77 GB free, card idle. `install-watch` (no key file) exit 0. `prepare` skipped:
    nothing under `docker/` changed since its run at 05:39, and `run` checks the image label.
    `ship` exit 0, the five inputs present.
  - **Batches**, one process each; tpch first3 with `PEACOCK_BENCH_RESUME` unset, the rest
    `=1`. All eight exit 0, `outcome=measured`, `answers=match`, none out of memory. The
    figures are the Task 10 entry's table above.
  - **Profiles**, `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12
    PCK_BENCH_HBM_FREQ=10000`. tpch exit 0 in 14 min (1451 calls from 4 parts), tpcds exit 0
    in 28 min (7077 calls from 4 parts). No `failed.tsv`. dev's `/` ended at 8.0 GB free after
    the pull.
  - **Task 10.** `every_fused_section_adds_up` passes over both files. The `loads.tsv` checks
    pass over 988 rows. `cp -p` of `<batch>.calls.tsv`, `<batch>.loads.tsv` and `hbm.tsv` to
    `testdata/calibration/h100-80gb-hbm3/fused/<ds>.sf40/`: all 18 files changed. The page
    test failed before regeneration (the fused rows were round 0's) and passes after; its diff
    is the eight fused rows. Golden check: `--lib` 1206 passed, 2 ignored; `test_corpus_goldens`
    64; cost-report formatting 39.
  - **What changed against round 0**, ms of the 2nd-min run:
    - tpch first3 and tpcds first3: the same within noise (929 against 931, 595 against 595).
      Each has one group of one fewer in `shared` (tpch's `partsupp:q2`, tpcds's
      `store_returns:q1`), each left alone by its re-read.
    - tpch first10: 4062 → 2939, from 16 % slower than alone to 16 % faster. Load 3260 →
      2122. The over-budget rule (I2) now buffers 19 holds (14.0 GB) and re-reads 23, where
      round 0 re-read all 38; lineitem is read 4 times, not 9. Groups 8 → 5. Still cap 1 and
      over budget (estimate 94.2 → 90.3 GB). Peak 26.8 → 46.3 GB, from the buffers.
    - tpch first20 and all: 6260 → 5844 and 7223 → 6835, with the same holds (37/40 and
      40/46 buffers/re-reads). Only the loads changed: each group now loads the union of the
      readers it kept (B1), so load fell 420 and 388 ms. The estimate fell 112 → 84 GB and the
      peak 54.2 → 44.3 GB, but both stay cap 1 and over budget.
    - tpcds first10, first20, all: cap 1 over budget → cap 2 within budget. The estimate fell
      78.9 → 66.1, 70.1 → 56.3 and 118.4 → 54.1 GB, because the rebuilt groups are narrower
      (groups 10 → 9, 12 → 9, 24 → 12). Time 4361 → 4201, 12060 → 11658, 36987 → 36442, all
      of it load; execute is unchanged, so running two roots at once saved nothing measurable.
      Holds barely moved (first20 92/118 → 94/117, all 102/642 → 104/641).
    - Profiles: Σ `load_us` per batch fell with the loads (tpch first10 3484 → 2266 ms over
      46 → 31 loads; tpcds all 25170 → 24348 ms).
  - **Logs:** `/tmp/qf3-*.log` (stage, ship, each batch, each profile, pull, the golden check);
    `~/.cache/peacock/run-gpu/peacockdb-beta/`. `testdata/calibration/hbm-parts/` is still the
    untracked sf40-baseline leftover; not to be committed.
- 2026-11-06: H100 rerun after round 1 committed (6e66896b, cf5ec195). Review round 2 returned; developer dispatched on its I1 and nit 7, then a full H100 rerun.

- 2026-11-06, review round 2 (developer): I1, nit 7 and nit 2, local, test first. Checkpoint
  reported, not committed; the H100 rerun waits for it.
  - **I1.** `HoldLedger::choose` reads `S`, `B` and the largest root from `cap_inputs` over the
    fused plan at every call; the cached `charged` is gone. RED:
    `a_cap_hold_is_priced_after_a_re_read_dissolved_a_group` (`cap::tests`): q0's held lineitem
    reader re-reads (`too large: 60 > 10`), which dissolves lineitem's group; q2 then waits
    under cap 2, and its customer reader re-read against the stale headroom of 10 where it
    fits the freed 50 (`fits: 20 <= 50`). "The hold rule" says so, and that within one
    resolve the loads are as the last rebuild left them (groups are rebuilt as a resolve ends).
  - **Nit 7.** `hold_inputs`' `recorded()` looks through an inserted `GpuHoldBuffer` (no
    origins) to its child. RED: `a_buffered_build_is_priced_as_what_it_buffers`
    (`holds::tests`): the nested joins beside a customer reader; customer, the inner join's
    build, buffered under that join, then the lineitem edge through it cost `30 + 1000` (the
    reader's own load) where it is `30 + 40`. Each fix disabled alone turns its own test red
    and leaves the other green.
  - **Left as it is:** a build child that is itself a shared load, or a project inserted over
    one, still falls to the held reader's own load in `recorded()`: such a node stands for
    several members, and `hold_inputs` cannot tell which is the build's. Not seen in the
    figures; a pricing approximation in prototype code.
  - **Nit 2.** build-test.md's header now matches the tables: 8143, Rust 6298 (the tables
    summed to 8141 before this round's two tests); the cpu block 3728, `--lib` 1210; the
    hold-rule row 30, its prose naming both new cases.
  - **Checks:** `--lib` 1208 passed, 2 ignored; `test_corpus_goldens` 64; cost-report
    formatting 39; `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh
    check -p peacockdb-core --features gpu,test-support --tests` clean, no warnings; rustfmt on
    `holds.rs`, `holds/tests.rs`, `cap/tests.rs`, and `mod.rs` checked unchanged.
- 2026-11-06, Task 9 and Task 10 Steps 1-4 (developer), **the H100 rerun after review round 2
  (`51b27ddf`): every batch measured, both profiles complete.** Nothing committed or staged.
  - **As round 1's rerun:** round 1's local `capture-*.sqlite` deleted and the scratch area
    moved to `/tmp/qf-run3/` (9.2 GB free on dev after); the CPU-VM build (59 GB free there,
    `check-build-image.sh` and `stage-install.sh` exit 0, `prototype_fused_benchmarks` staged
    10:51); the H100 (77 GB free, card idle), `install-watch` without a key file, `prepare`
    skipped (no `docker/` change), `ship`, the five inputs present, the shipped round-1 fused
    output and the VM's `calibration/fused/` removed; tpch first3 without
    `PEACOCK_BENCH_RESUME`, the rest `=1`. All eight exit 0, `outcome=measured`,
    `answers=match`, none out of memory. Profiles with the three HBM variables: tpch exit 0 in
    13 min (1456 calls from 4 parts), tpcds exit 0 in 26 min (7078 calls), no `failed.tsv`.
    dev's `/` ended at 7.2 GB free after the pull.
  - **Task 10.** `every_fused_section_adds_up` passes; the `loads.tsv` checks pass over 725
    rows; the 18 profile files copied, all changed; the page test failed before
    regeneration and passes after, its diff the eight fused rows. Golden check: `--lib` 1208
    passed, 2 ignored; `test_corpus_goldens` 64; cost-report formatting 39.
  - **What changed against round 1**, ms of the 2nd-min run. Two changes act: holds priced
    against the plan as it stands (I1), and a buffered build priced as what it buffers (nit 7).
    Over the eight sections the reasons moved from `fits` 362, `too large` 853, `over budget`
    38 to `fits` 142, `too large` 12, `over budget` 1098: the ledger now sees the re-read
    loads in their roots, so it is over budget for most decisions, and over budget it keeps a
    buffer smaller than its re-read.
    - tpch first3, tpcds first3, tpch first10: the same decisions; 929 → 930, 595 → 593,
      2939 → 2958.
    - tpch first20: 5844 → 4924 (−16 %). Buffers/re-reads 37/40 → 42/35 (8.5 → 18.8 GB
      buffered), lineitem read 16 → 7 times, load 4372 → 3421. Peak 44.3 → 55.6 GB; still cap 1
      over budget (estimate 84.4 → 85.9 GB).
    - tpch all: 6835 → 5578 (−18 %). 40/46 → 48/38, lineitem 19 → 8, load 5020 → 3729. Peak
      44.3 → 57.5 GB; cap 1 over budget (estimate 84.6 → 91.2 GB).
    - tpcds first10: 4201 → 4298 (+2.3 %). 67/19 → 62/24, load 1415 → 1511: the ledger read
      over budget mid-resolve (see "An honest reading"), though the settled plan is cap 2
      within budget as before.
    - tpcds first20: 11658 → 11041 (−5 %). 94/117 → 108/101, `date_dim` read 17 → 2 times,
      load 5838 → 5235. Cap 2.
    - tpcds all: 36442 → 32043 (−12 %). 104/641 → 346/402, `date_dim` 144 → 27, groups
      12 → 18, load 21718 → 17290. Cap 2 → 1, still within budget: buffers 6.6 → 8.1 GB and
      the estimate 54.1 → 57.2 GB leave no room for a second root. Peak 18.5 → 43.3 GB.
    - Execute moved by at most 3 % in any batch (tpch first20 1444 → 1481).
    - Profiles: Σ `load_us` fell with the re-reads (tpch all 5370 → 3875 ms over 54 → 46
      loads; tpcds all 24348 → 19861 ms over 665 → 426); tpcds first10 rose 1558 → 1699 ms.
  - **Logs:** `/tmp/qf4-*.log` (stage, ship, each batch, each profile, pull);
    `~/.cache/peacock/run-gpu/peacockdb-beta/`. `testdata/calibration/hbm-parts/` is still the
    untracked sf40-baseline leftover.
- 2026-11-06: H100 rerun after round 2 committed. **Decided:** tpcds first10's 2.3 % slowdown (a group's shared load is rebuilt only at the end of a resolve, so mid-resolve re-reads overstate the ledger) is accepted as a stated limit of the prototype, not fixed: seven of eight batches got faster or held, and another rebuild-per-decision change would need a fourth H100 session for a ~0.1 s effect. Reviewer round 3 dispatched.
- 2026-11-06: review round 3: 0 blocking, 0 important, 1 nit (wording, fixed by the coordinator). Board `completing`; completeness pass (fresh reviewer, fresh analyst) dispatched.
- 2026-11-06, completeness pass (developer): the merge hint across queries only, local, test
  first. Checkpoint reported, not committed; the H100 rerun of the eight batches waits for it.
  - `groups_of` (`merge.rs`) keeps a group only where its members come from two or more
    queries. A group that does keeps every member, a query's own repeat among them, as spec
    item 2's "across queries" reads. RED: `a_query_repeating_a_subtree_with_itself_is_no_entry`
    (`merge::tests`): one query joining a coalesced filter over lineitem with the same filter,
    the probe's through a buffer, gave one entry with both members in q0 (30 µs). Green with
    the filter, red again with it reverted alone.
  - `merge_hint`'s comment, "The fused format"'s `--- merge ---` and build-test.md (the merge
    row 6, `--lib` 1211, the cpu block 3729, the header 8144 / Rust 6299; the tables sum to
    8144) say so.
  - **Checks:** `--lib` 1209 passed, 2 ignored; `test_corpus_goldens` 64; cost-report
    formatting 39; `scripts/cargo-cudf.sh check -p peacockdb-core --features gpu,test-support
    --tests` clean; no warnings; rustfmt on `merge.rs`, `merge/tests.rs`, `mod.rs`.
- 2026-11-06, Task 9 Steps 1-4 and 6 and Task 10 Steps 1 and 4 (developer), **the H100 rerun
  after the completeness fix (`7b1db00e`): the eight batches, no profiles.** Nothing committed
  or staged.
  - As the round-2 rerun: round 2's scratch area moved to `/tmp/qf-run4/`; the CPU-VM build
    (60 GB free, `check-build-image.sh` and `stage-install.sh` exit 0, binary staged 12:52);
    the H100 (77 GB free, card idle), `install-watch` without a key file, `prepare` skipped
    (no `docker/` change), `ship`, the five inputs present, the shipped fused output and the
    VM's `calibration/fused/` removed; tpch first3 without `PEACOCK_BENCH_RESUME`, the rest
    `=1`. All eight exit 0, `outcome=measured`, `answers=match`.
  - **The plans did not change.** Against round 2 (`755656e0`), per batch: the `--- batch ---`
    block (holds, cap, estimate, shares, reads) is identical line for line; every tree is
    identical but for `time_us` and `total_us` (same nodes, `batch_rows`, `batch_bytes`); the
    accountant's peak is identical; run, load and execute are within ±1.5 % (tpcds first20
    +0.7 %, tpch first10 −1.3 %). So 51b27ddf's profiles (`ec6935dd`) describe these plans, and
    were not rerun. Only `--- merge ---` changed (the table in the Task 10 entry).
  - **Task 10.** `every_fused_section_adds_up` passes; the page test failed before
    regeneration and passes after, its diff the eight fused rows (their load and execute).
    Golden check: `--lib` 1209 passed, 2 ignored; `test_corpus_goldens` 64; cost-report
    formatting 39.
  - **Logs:** `/tmp/qf6-*.log`; `~/.cache/peacock/run-gpu/peacockdb-beta/`.
- 2026-11-06: completeness approved (signoff in the spec). All three VMs hard-stopped 13:35Z. CI device run dispatched 2026-11-06T13:36:02Z on 6f67bf5b: run 42226803958. Waiting for it and the PR CI before `done`.
- 2026-11-06: CI run 42226803958 green, GPU Tests (nebius-gpu) included; the PR's CI green. VMs hard-stopped. Board `done`.

## The hold rule (spec 1a)

All numbers are the sf40-baseline tree's, per origin node: Σ `batch_bytes` and Σ `batch_rows`
over lanes and batches. The rule is `decide`, `hold_inputs` and `HoldLedger::choose` in
`prototypes/fuser/mod.rs`.
- An edge's bytes are its child's. A project inserted to select from the shared load stands for
  the reader's own load. The edge right above the shared load carries the union, which nothing
  recorded, so it is not a candidate.
- A buffer above a join runs that join early, so it also keeps the join's build child's bytes,
  for every join below it on the path. A build child that is a buffer an earlier decision
  inserted is priced as the node it buffers (review round 2, nit 7).
- An edge is allowed where every join below it on the path recorded no more rows than its child
  on the path. In a root that waits for the cap, also only where nothing at or under it
  keeps state, for every hold there, a join's as well as the cap's (`HeldReader.waits`). A
  join's buffer placed above state before the cap was known, in a root that then waits, is
  taken out with its record, and its reader decided again under that rule.
- Placement: the allowed edge with the fewest bytes; a tie goes to the lower edge.
- A re-read costs the reader's own load's `loads.tsv` `load_us`, keyed by the query and the seq
  of its `CudfScan` recipe. It is recorded either way. It also adds that load's recorded
  `batch_bytes` back to its root (`reread_bytes`): the load is no longer shared.
- Headroom: `budget − (S + B + max root_bytes)`; over budget when `S + B + max root_bytes >
  budget`. All three come from `cap_inputs` over the fused plan at each decision (review
  round 2, I1): a group a re-read dissolved, and rebuilt, frees its bytes for the decisions
  after, and a buffer taken out is no longer charged. Within one resolve the shared loads are
  as the last rebuild left them, since groups are rebuilt when a resolve ends. Readers are
  decided in the order validation names them.
- The choice, where a buffer is placed. Within the budget: the buffer where its bytes fit the
  headroom, else a re-read. Over budget (review round 1, I2, the coordinator's decision): the
  buffer where its bytes are below `reread_bytes`, else a re-read; a buffer then costs less
  than the re-read it replaces. No allowed edge: a re-read.
- A re-read takes its reader out of its group. After every resolve, in `fuse` and in each
  `settle` round, each group's shared load is rebuilt over the members left: the union of
  their own loads, every inserted project re-pointed at it. A member left alone reads its own
  load, and its group is gone from `shared` (review round 1, B1). The cap then reads the
  rebuilt loads.
- A chooser that buffers a waiting reader above state is refused, `PlanError::Invalid` naming
  the chooser, rather than looping between it and validation (review round 1, I1).

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
  largest saving first. A group's members come from two or more queries (the completeness
  pass): a query repeating a subtree with itself over a shared load is no entry; `members` are `<query>:<pre-order index in its own tree>`; `node` is last
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

## Review round 1 (2026-11-06)

1 blocking, 2 important, 9 nits. All routed to the developer except nits 1 and 9 (markdown, fixed by the coordinator).

- **B1.** A re-read leaves the shared load loading the union of every original member (`fuse.rs:371-382`). tpch first10: all 8 groups end with one reader; q1 loads a 12-column lineitem union alone (342 ms vs 155 ms alone). The 8 single-reader groups add 511 ms, nearly all of first10's "16 % slower". Multi-reader groups also keep stale unions (first20/all lineitem: 14 columns where 9 are read). The cap's `S` uses the same unions. Fix: after resolving (and after each `settle` round) rebuild each group's load over its remaining members, re-point the inserted projects; a group of one reads its member's own load; recompute the cap; regression test; rerun all eight batches and the profiles on the H100; correct "An honest reading".
- **I1.** The resolve loop terminates only if the chooser honours `waits` (`fuse.rs:348-356`). Fix: after `choose` for a waiting reader, check the chosen edge has no state at or below it, else error naming the chooser; regression test with a chooser that always buffers at `path.last()` under a cap.
- **I2.** Over budget, the hold rule re-reads every hold, even 5-byte buffers, though a re-read adds the reader's whole load to its root (`holds.rs:139`, `decide` `holds.rs:20-51`, `cap.rs:59-62`). The plan specified the rule, so this is a coordinator decision. **Decided:** when over budget, buffer when the buffer's bytes are below what the re-read adds to its root; record the change in "The hold rule". Spec 1a's "plain function of those numbers and the memory budget" still holds.
- Nits for the developer: 2 (doc comments over 10 lines, `mod.rs:255-266`, `358-370`), 3 (prototypes gate misses `use crate::prototypes as p;`), 4 (`Unshared::RowsOnly` doc vs label), 5 (`fused_problem` does not require `answers=match`), 6 (`baseline_for` first match without a comment), 7 (`recorded()` should look through `GpuHoldBuffer`), 8 (`create_nsys_profile.sh:47` example lacks `PCK_BENCH_HBM_FREQ=10000`).
- Reviewer confirmed: 03b7af97 sound; `testdata/.gitignore` admits only the four profile kinds; Review Focus 1-7 hold. Outside the diff: `!/calibration/*-*/` re-includes `calibration/hbm-parts/`, so never `git add testdata` wholesale.

## Review round 2 (2026-11-06)

0 blocking, 1 important, 2 nits. B1, I1, I2 and the round-1 nits confirmed fixed; the rerun is consistent.

- **I1 (round 2).** The hold ledger takes `S` and the largest root from `cap_inputs` once, at the first decision inside `fuse`, before `regrouped` shrinks the unions (`holds.rs:138`). Later cap-hold decisions are priced against a stale `S`. tpcds all: about 11 GB of headroom never offered, 90 cap holds re-read that together needed 0.60 GB and carry 1.72 s of load; tpcds first20: 18 such re-reads; tpch first20/all err the other way. **Decided:** compute `S` and the largest root from `cap_inputs` at every `choose`; update "The hold rule"; regression test (a re-read dissolves a large group, and a cap hold that fits only in the freed headroom is buffered); rerun all eight batches and the profiles from one binary.
- **Nit 7 (round 1), reopened and taken:** `recorded()` (`holds.rs:77`) prices a join's build child that is an inserted buffer as the reader's own load. Seen in tpch first3 q2 (`tp1_single.fused.txt` lines 14-15: supplier charged 66 MB where nation's build is 493 bytes). Look through a buffer to its child; nested-join test.
- **Nit 2:** `build-test.md:7` header should match the tables (8141 / Rust 6296 at cf5ec195); the developer updates it with this round's new tests.
- Not a finding: buffers left when a group dissolves (at most 0.44 GB); dropping them in `regrouped` is optional.

## Completeness pass (2026-11-06)

- Reviewer (what is wrong): 0 blocking, 0 important; every Done-when item met, recomputed from the committed files. Note: the fused format, hold and cap rules live only in this detail file, which the post-merge archive step deletes; architecture.md, build-test.md and `fuser/mod.rs` point here.
- Analyst (what is missing): 1 important. The merge hint groups identical subtrees without requiring members from two or more queries (`merge.rs:88-101`). The fuser also shares two scans of one file inside one query, so within-query repeats dominate `saved_us` (tpcds all: 1,182 ms within, at most 94 ms across; tpcds first3: 77.5 ms, all within). Spec item 2 asks for "across queries". **Decided:** restrict merge groups to members spanning two or more queries; regression test; rerun the eight batches on the H100 (the executed plans do not change, so the profiles from 51b27ddf stand and are not rerun). For batching-report: part of the measured load saving is deduplication inside one query (`shared=` shows `q14+q14+q14`), which a batch of one would also show.
- Falsified sentences (analyst) fixed by the coordinator: architecture.md "Prototypes" (`rows-only` covers a lone column scan) and "Interfaces" (instrumentation: `prototype_fused_benchmarks`); build-test.md: the `test-support` feature paragraph, `case-inventory.sh`'s gpu note, "Fused batches" (one shared load per file plus re-reads), stage-install's staged binaries, the run image's `rust-benchmarks/`, the Runs legend.

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 6 | `09949303` | fuser: one shared load per file across N plans; lib 1121, goldens 61, formatting 38 |
| Task 2 Step 5 | `9ab20eed` | fuser: the ordered lists and fused answers against the corpus goldens; lib 1131, goldens 61, formatting 38 |
| Task 3 Step 5 | `0d677c19` | fuser: the mergeability hint; lib 1141, goldens 61, formatting 38 |
| Task 4 Step 5 | `018f4c70` | fuser: the pipelining hint; lib 1147, goldens 61, formatting 38 |
| Task 5 Step 5 | `54596367` | fuser: buffer or re-read, and the concurrency cap rule; lib 1171, goldens 61, formatting 38 |
| Task 6 (part A) | `5af21f1e` | fuser: the fused format and the benchmark's host-side rules (Task 6, part); interim, developer context; lib 1181, goldens 64, formatting 38, gpu check clean |
| Task 6 Step 5 | `7e194089` | fuser: the fused benchmark case and its file; lib 1191, goldens 64, formatting 39, gpu check clean; target links, first device run is Task 9 |
| Task 7 Step 5 | `7eae3641` | fuser: the report page's fused-batches table; lib 1197, goldens 64, formatting 39, gpu check clean |
| Task 8 Step 5 | `a203912b` | fuser: an Nsight profile per fused batch; lib 1199, goldens 64, formatting 39, test_run_gpu 49; shellcheck not run (not installed) |
| Task 9 fix | `03b7af97` | fuser: a join hold in a root that waits is never buffered above its state; found on the H100 (tpch first20/all); lib 1200, formatting 39; tpch first3/first10 measured, rerun pending |
| Review round 1 fixes | `0d46064d` | fuser: review round 1: a group rebuilt over its remaining readers, a looping chooser refused, buffers over budget below the re-read; B1, I1, I2, nits 2-6, 8; nit 7 declined (a held path never passes a buffer); lib 1206, goldens 64, formatting 39 |
| Review round 2 fixes | `51b27ddf` | fuser: review round 2: holds priced against the plan as it stands, a buffered build priced as what it buffers; I1 (round 2), nits 7 and 2; lib 1208 passed, goldens 64, formatting 39 |
| Completeness fix | `7b1db00e` | fuser: the merge hint groups subtrees across queries only; completeness pass; lib 1209 passed, goldens 64, formatting 39 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
| nebius-cpu128 | 2026-11-06 05:17:59 | 2026-11-06 05:26:58 | 0.15 |
| nebius-h100 | 2026-11-06 05:27:02 | 2026-11-06 05:44:50 | 0.30 |
| nebius-cpu128 | 2026-11-06 06:06:11 | 2026-11-06 06:12:23 | 0.10 |
| nebius-h100 | 2026-11-06 06:12:26 | 2026-11-06 07:25:00 | 1.21 |
| nebius-cpu128 | 2026-11-06 08:46:04 | 2026-11-06 08:52:41 | 0.11 |
| nebius-h100 | 2026-11-06 08:52:46 | 2026-11-06 10:01:42 | 1.15 |
| nebius-cpu128 | 2026-11-06 10:46:41 | 2026-11-06 10:53:06 | 0.11 |
| nebius-h100 | 2026-11-06 10:53:09 | 2026-11-06 11:58:03 | 1.08 |
| nebius-cpu128 | 2026-11-06 12:47:15 | 2026-11-06 12:53:23 | 0.10 |
| nebius-h100 | 2026-11-06 12:53:30 | 2026-11-06 13:22:59 | 0.49 |
