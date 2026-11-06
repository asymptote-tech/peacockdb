# tp4-sized — detail

Branch `ENS-tp4-sized`, forked from `ENS-batching-report` at `325b1825` on 2026-11-06. PR targets
`ENS-batching-report`. Plan: [`tp4-sized-impl.md`](tp4-sized-impl.md).

## Progress

- 2026-11-06: branched, board `building`. Verda unavailable (`VERDA_CLIENT_ID` unset); CPU tests
  local. Developer dispatched at Task 1 Step 1.
- Inherited from query-fuser and batching-report (their detail files): `install-watch` on the
  H100 runs without `--key-file` (editor-eu's key is embedded); the H100 metrics pass needs
  `PCK_BENCH_HBM_FREQ=10000`; dev's disk had ~7 GB free on 2026-11-06 and captures come home;
  `testdata/calibration/hbm-parts/` is an untracked leftover never to be staged.
  batching-report's generator inserts tp4_sized columns when `<ds>.sf40/tp4_sized.benchmark.txt`
  exists; its hand-written Reading in `llm-wiki/reports/query-batching.md` is outside `--check`
  and must be re-read for the tp4_sized sums. CI runs that page's `--check`.

- 2026-11-06, Task 1 (developer). Departures from the plan, the tree winning:
  - `H100_MODES` lives in `peacockdb-core/tests/test_corpus_goldens/benchmark_lists.rs`, not
    `benchmark.rs` (the plan's `:625` is `PREDATES_PROVENANCE`'s use there). Both tests went
    beside it, in `benchmark_lists.rs`.
  - Red/green: `the_h100_list_is_written_at_tp1_single_and_tp4_sized` failed with modes
    `["tp1_single"]` (left) against `["tp1_single", "tp4_sized"]`; `the_h100_tree_is_held_to_both_modes_of_its_list`
    passed at once, containers' rule, as the plan said. After the one-entry change the binary
    ran 66/66, no warnings, `cases.csv` untouched (no `tp4_sized.benchmark.txt` yet: a mode
    with no file is not read, `device_cases::lines`).
  - `build-test.md` brought along: the cpu tier's counts (`test_corpus_goldens` 64 → 66, total
    3729 → 3731) and "sf40 baseline"'s `H100_MODES` sentence, which said tp4-sized "adds its own".
  - The plan's Task 2 Step 6 profiles tpcds in one trace pass. build-test.md "sf40 baseline"
    step 7 says sf40-baseline needed three runs: `nsys_calls.py` refuses the 14 tpcds cases whose
    sf40 join order differs from the sf1 goldens, and `set -e` then skips the rest. Task 2 will
    follow build-test.md's three-run recipe, with `PCK_BENCH_HBM_FREQ=10000` on the metrics pass.

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 5 | tp4-sized: the h100 list is written at tp1_single and tp4_sized | reported; coordinator commits |
| Task 1 Step 5 | `49a0d619` | tp4-sized: the h100 list is written at tp1_single and tp4_sized; test_corpus_goldens 66, cpu suite green |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
