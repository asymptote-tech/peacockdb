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

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 6 | `09949303` | fuser: one shared load per file across N plans; lib 1121, goldens 61, formatting 38 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
