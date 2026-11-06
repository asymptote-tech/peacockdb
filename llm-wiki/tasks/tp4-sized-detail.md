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

## Checkpoints

| step | commit | note |
|---|---|---|

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
