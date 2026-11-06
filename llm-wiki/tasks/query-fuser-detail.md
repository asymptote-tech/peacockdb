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

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 6 | `09949303` | fuser: one shared load per file across N plans; lib 1121, goldens 61, formatting 38 |
| Task 2 Step 5 | `9ab20eed` | fuser: the ordered lists and fused answers against the corpus goldens; lib 1131, goldens 61, formatting 38 |
| Task 3 Step 5 | `0d677c19` | fuser: the mergeability hint; lib 1141, goldens 61, formatting 38 |
| Task 4 Step 5 | `018f4c70` | fuser: the pipelining hint; lib 1147, goldens 61, formatting 38 |
| Task 5 Step 5 | `54596367` | fuser: buffer or re-read, and the concurrency cap rule; lib 1171, goldens 61, formatting 38 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
