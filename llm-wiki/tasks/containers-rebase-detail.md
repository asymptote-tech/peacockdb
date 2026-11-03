# containers-rebase — detail

## Run log

- 2026-11-03: coordinator start. Board `building`. Preconditions (plan Task 1 Step 1) hold:
  origin/master `dc90b51c` carries this plan (Step 7a present); `611b5dcd..origin/ENS-containers`
  = 16; merge-base of ENS-optimizer and ENS-containers = `611b5dcd`.
  Tips: `origin/ENS-containers` = `b71758e9`, `origin/ENS-optimizer` = `561411b4`.
- verda: not reachable (no `VERDA_CLIENT_ID`); CPU runs local on dev.
- Surgery state lives in `.claude/ensemble/L.surgery` until Task 1 Step 5 folds it in here.

## Surgery

Folded from `.claude/ensemble/L.surgery` at Task 1 Step 5; kept in step with it through Step 10.

```
# containers-rebase surgery state (coordinator)
fork-board = e06992b7e23f467e8e921eb25cb4ff5b4e1555d7  (ENS-containers-rebase: master + board commit e06992b7)
origin/master = dc90b51c94f0c644697c3ad050b7925106e5b60b
origin/ENS-containers = b71758e9a2922436a7b47b4a7d976c276587c5f8
origin/ENS-optimizer = 561411b41123568146e1bb13d99e968ca4ab04c7
rerere.enabled = false
step: 3 (cherry-pick -x 02f6a4d8 onto detached origin/master)
step 3 done: fork-carried = f3db66b1b12a65764b6e15d06827bcf40c2b339b (02f6a4d8 carried; conflict benchmark.rs resolved per table + macro_invocations_in in corpus.rs/mod.rs + 2 unit tests in corpus/tests.rs)
step: 4 (git branch -f containers-replay origin/ENS-containers; git rebase --empty=keep --onto fork-carried 611b5dcd containers-replay)
step 4: stops 462ed26b, 8437d6cd (empty, kept -C) done; at f75cba7e build-test.md
f75cba7e: build-test.md resolved by coordinator (master's text + Images and GPU host scripts rows + header totals +204 python, interim; recount in Task 5)
199bde5f: README.md = ours (master has neither line); build-test.md: header +20 image tests, Images row containers', Datasets embeddings paragraph + bucket bullet + calibration-record bullet (from optimizer base text, documents carried fetch_record.sh/records.sha256)
d2a0f88c: build-test.md resolved by coordinator (11 hunks: counts interim +delta, prose paragraph merged keeping master's mask sentences, Calibration + cuDF smoke rows containers', benchmark-flow/derived-text/record hunks containers'); Rust files -> developer
d2a0f88c: Rust resolved by developer (device_answer: cudf_release + master doc; benchmark.rs: use + Provenance/device_slug, body d2a0f88c's), cargo check green
cb8c3637: build-test.md resolved by coordinator (counts interim; golden table = master's + case-list row only, optimizer rows dropped; Corpus benchmarks section containers'); benchmark.rs -> developer (no compile check, Step 7a ports)
cb8c3637 done (developer kept master mask tests + CASE_LIST inside hunk; not compiled). stop at 615b60e8 build-test.md
615b60e8: build-test.md = containers' tpcds sf40 row + bullet
b50d9b2a: build-test.md header +48 image tests
b71758e9: build-test.md header +40 python
b71758e9 -> fc0796b9 had markers committed (script error); fixed and amended (author kept): header = master's + 208 image and launcher tests
step 5 done: ENS-containers-rebase = replay + board cherry-pick c21b90d9; containers-replay deleted. next: 7a (developer), 7, 8, 9, 10 push
```

Resolution notes beyond the plan's table:
- `02f6a4d8` carry: the developer also added two unit tests for `macro_invocations_in` in
  `test_support/corpus/tests.rs` (no containers commit touches that file).
- `cb8c3637`: HEAD's side of the body hunk also held master's mask tests (`1634e17c`) and the
  carry's `CASE_LIST`; both kept, cb8c3637's side taken whole after them.
- `build-test.md` stops were resolved by the coordinator, containers' prose onto master's text.
  Counts in the header and tier headers are interim deltas; Task 5 recounts. The optimizer-only
  golden rows (stats sidecar, cardinality, run.py, cost_report.html) that cb8c3637's side carried
  as context were dropped. The calibration-record bullet in "Datasets" (from the optimizer base's
  text, not a containers commit) was kept: it documents `fetch_record.sh` and `records.sha256`,
  which the carried `02f6a4d8` brings.
- `b71758e9`: a resolution script failed and the rebase continued with conflict markers in
  `build-test.md`'s header; caught before the branch moved, fixed and amended into the replayed
  commit (`--amend --no-edit`, rk00's author and date kept). No other replayed commit holds a
  marker (`git grep` over each).
