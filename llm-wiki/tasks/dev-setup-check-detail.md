# dev-setup-check — run record

Spec: [`dev-setup-check.md`](dev-setup-check.md). Plan: [`dev-setup-check-impl.md`](dev-setup-check-impl.md).

## Coordinator log

- 2026-09-11 22:34Z — chain started under the watchdog in worktree
  `~/workspace/peacockdb-ENS-dev-setup-check`, hostname `mild-face-glows-fin-03` (this is `dev`).
  Task branch is the chain branch `ENS-dev-setup-check`, forked from master at `62335cf`; PR
  will target master. Board moved to `building`; developer dispatched with the impl plan.
- 2026-09-11 23:28Z — developer returned green: new case red then green, four workflows
  recorded below (exec_model timed out at 900 s; shad-gpu `--patch --run` red on glibc 2.38).
  Committed, pushed, PR #146 opened against master (3 commits, base verified); board moved to `reviewing`.
- 2026-09-11 23:35Z — review round 1: 0 blocking, 0 important, 1 nit (the `#L41` anchor on
  `build-test.md`'s cuDF smoke row now points two lines early; left as is — the spec limits that
  file to two counts and the branch never merges). Board moved to `completing`; completeness
  pass dispatched: a fresh reviewer and a fresh analyst, neither seeing the other's list.
- 2026-09-11 23:50Z — completeness pass closed. Reviewer: 0 blocking, 0 important. Analyst: 0
  blocking, 3 important, all about the record; `architecture.md` falsified sentences: none.
  Applied: a `## Not driven` section (rows of the two tables the four workflows do not reach),
  one sentence on what the exec-model timeout left unrun, and the CI evidence line below at
  `done`. Waiting on run 34658235414 (the code commit `e68f82cc`); the head rollup shows every
  job skipped because the later commits are doc-only.
- 2026-09-12 00:04Z — CI green, board `done`. Judged on run 34658235414 (`e68f82cc`, the one
  commit that carries code): Changed paths, S3 datasets, Cost report, cudf 25.02, cudf 26.02,
  build 25.02 for GPU, GPU Tests (remote) all `success`; Deploy to Pages `skipped` as on every
  PR. The later runs on `526f47d9`, `ee479740` and `2b0bab28` are doc-only pushes the `changes`
  gate skipped wholesale, so `gh pr checks 146` at head reads as all-skipped and proves nothing.
  Watched with `gh run watch --exit-status` from `claude -p` under the watchdog, no terminal.
- Pre-dispatch checks, from the coordinator's own shell: `ssh dev` and `ssh verda` both fail
  with `Could not resolve hostname` — `~/.ssh/config` carries only `shad-gpu`. So verda is
  down (local runs), and workflow 3's `--host dev` is expected to fail at the first ssh. The
  developer records that as written and does not add a config entry. (A `Host dev` →
  `localhost` entry appeared in `~/.ssh/config` at 22:35:12Z, after this check and before the
  developer reached workflow 3 — the human's doing, not the run's — so workflow 3 ran through
  ssh to itself as the spec assumed.) Present on the host:
  `~/peacockdb`, `~/miniforge3/envs/rapids-26.02`, `/media/data/peacockdb`,
  `~/data/miniforge3`, gcc-12, gcc-14, nvcc, cmake, ninja, python 3.12.3, cargo. No GPU driver
  (`nvidia-smi` fails). `testdata/tpch.sf1` and `tpcds.sf1` are symlinks into `~/peacockdb`.

## Workflows

(developer appends one section per workflow below, in the shape the plan gives)

## rust-only: test_golden_format

- command: `timeout 900 cargo test --features rust-only -p peacockdb-core --test test_golden_format`
- host: dev
- wall time: 5m40s cold compile (first cargo invocation in this worktree's `target/`), then 1s for the full target
- outcome: green
- signature: none
- notes: `test result: ok. 27 passed`. The new case `a_field_that_is_not_a_number_is_not_read_as_absent`
  was seen red first with the expected string `not a count of anything` (`panic did not contain
  expected string`, the real message being ``GpuFilter: field `output_rows=many` is not a count:
  invalid digit found in string``), then green with the string the plan gives.
  `rustfmt --edition 2021 --check` on the file reports one diff at line 299, a `vec![` block that
  predates the installed rustfmt and is not on this branch's lines; left as the style page says.

## rust-only: test_plan_goldens

- command: `timeout 1200 cargo test --features rust-only -p peacockdb-core --test test_plan_goldens`
- host: dev
- wall time: 0m11s
- outcome: green
- signature: none
- notes: `test result: ok. 19 passed`. The dependency tree was warm from the previous target, so 11s
  is the target's own compile plus the run. `testdata/tpch.sf1` and `tpcds.sf1` are symlinks into
  `/home/dmitry/peacockdb/testdata/`, so no generation step ran.

## cost-report: cargo

- command: `timeout 1200 cargo test -p cost-report`
- host: dev
- wall time: 0m02s
- outcome: green
- signature: none
- notes: `test result: ok. 37 passed`. Shares `target/` with the rust-only loop, so only the crate
  itself compiled. One pre-existing warning, not from this branch: `function `sha_links` is never
  used` at `cost-report/src/main.rs:1496`.

## cost-report: test_duckdb_cost.py

- command: `timeout 300 python3 -m pytest -q testdata/test_duckdb_cost.py`
- host: dev
- wall time: 0m00s
- outcome: green
- signature: none
- notes: `41 passed in 0.03s`. python 3.12.3, pytest 9.1.1.

## cost-report: exec_model

- command: `timeout 900 python3 -m pytest -q scripts/exec_model/tests/ --ignore=scripts/exec_model/tests/test_tpch_corpus.py -p no:cacheprovider`
- host: dev
- wall time: 15m00s
- outcome: timed out after 900 s
- signature: none printed — killed by `timeout` (rc 124) while
  `scripts/exec_model/tests/test_tpcds.py::test_corpus_q23` was running, case 286 of 306; the 285
  before it all passed (285 dots, no `F`)
- notes: the command collects `test_tpcds.py` (71 cases, TPC-DS over sf1 against a DuckDB oracle,
  the set `build-test.md` files under the manual "Exec-model corpus" row) because the `--ignore`
  names only `test_tpch_corpus.py`; that is where the 900 s went. Whether the CI cost-report step
  globs the same 306 is for the coordinator to check — the developer does not read the workflow.
  Not re-run with a narrower set: a failing workflow is recorded, not repaired.
  Coordinator, from the reviewer's reading: CI does not. `pipeline.yml` lists `test_tpch.py`,
  `test_tpch_corpus.py` and `test_tpcds.py` as run elsewhere and globs the other ten files
  (216 cases), and the cost-report job on `e68f82cc` went green. The timeout is the spec
  command's shape — an `--ignore` one file short — not a host defect; no fix task needed.
  What never started: `test_tpcds.py`'s last case (`q64`) and all 19 `test_tpch.py` cases —
  the one python set that reads sf1 parquet on dev — so this run says nothing about those.

## 26.02: build

- command: `timeout 5400 scripts/build-test.sh --host dev --local-cudf-root ~/miniforge3/envs/rapids-26.02 --build`
- host: dev
- wall time: 10m47s (23:00:56Z to 23:11:43Z)
- outcome: green
- signature: none
- notes: cmake reported `Using host cudf: 26.02.01` and `Using cuVS: 26.02.0`; ccache was picked up.
  Five C++ binaries in `cpp/build26/install/bin/` and thirteen Rust binaries staged under
  `cpp/build26/install/rust-tests/`; the cold DataFusion build was 6m59s of the total, into
  `target-cudf-rapids-26.02/`. 34 compiler warnings, all pre-existing (deprecated `cudf::round` and
  `cudf::strings::like` in `cpp/src/expr.cpp`, a gcc-14 `-Wstringop-overflow` in libstdc++, a cmake
  policy notice); none from this branch. Host: 8 cores, 31 GiB.

## 26.02: push-binaries

- command: `timeout 900 scripts/build-test.sh --host dev --push-binaries`
- host: dev
- wall time: 0m06s
- outcome: green
- signature: none
- notes: `ssh dev` resolved after all. `~/.ssh/config` carries `Host dev` → `HostName localhost`,
  `User dmitry`, `IdentityFile ~/.ssh/id_ed25519`, `StrictHostKeyChecking accept-new`; the file's
  mtime is 22:35:12Z, one minute after the coordinator's pre-check that found only `shad-gpu`, so
  somebody added it between the two. The developer did not touch it. 181 files rsynced into
  `/home/dmitry/peacockdb/cpp/install/` (it deleted a stale `bin/flatc` and the `include/flatbuffers/`
  headers that were there), then goldens and 415 committed fixtures.

## 26.02: run

- command: `timeout 3600 scripts/build-test.sh --host dev --run`
- host: dev
- wall time: 2m53s (23:12:18Z to 23:15:11Z)
- outcome: green
- signature: none
- notes: `peacock_cpu_tests` `PASSED 11 tests`; every staged Rust binary green — test_corpus_goldens
  20, test_cost_model 3, test_cpu_corpus 448 (120s), test_cpu_end_to_end 24 + 2 ignored (49s),
  test_cpu_executors 1, test_golden_format 27 (the new case ran here too), test_layout_injection 4,
  test_null_analysis 8, test_plan_goldens 19, test_planner_join_capability 13,
  test_planner_join_refusals 10, test_gpu_batch 3, test_ffi 2. The log prints no testdata path; the
  path baked into the binaries (`strings` on `test_cpu_corpus`) is
  `/home/dmitry/workspace/peacockdb-ENS-dev-setup-check/peacockdb-core/../testdata`, this worktree,
  whose `tpch.sf1`/`tpcds.sf1` symlink into `~/peacockdb/testdata/`. The `/media/data/peacockdb`
  symlink was not what this run read.

## 25.02: build

- command: `PCK_TEST_FILTER=q6 timeout 5400 scripts/build-test-shadgpu.sh --build`
- host: dev
- wall time: 11m03s (23:15:51Z to 23:26:54Z)
- outcome: green
- signature: none
- notes: cmake reported `Using host cudf: 25.02.02` and `Using cuVS: 25.02.01` from
  `~/data/miniforge3/envs/rapids-cuda-12.2` (the symlink resolves) with gcc-12 and ccache. Five C++
  binaries in `cpp/install/bin/`, and `test_inc2_conformance`, `test_gpu_abi`, `test_gpu_recipe_walk`,
  `test_gpu_executors`, `test_gpu_corpus` staged in `cpp/install/rust-tests/`; the cold cargo build
  into `target-cudf-rapids-cuda-12.2/` was 9m24s of it. Two compiler warnings, both pre-existing.

## 25.02: push-binaries

- command: `timeout 1800 scripts/build-test-shadgpu.sh --push-binaries`
- host: shad-gpu (via dev)
- wall time: 0m14s (23:27:04Z to 23:27:16Z)
- outcome: green
- signature: none
- notes: dev's key is authorised on shad-gpu; no retry fired. Five rsync passes: `cpp/install/` (177
  files), goldens (174), the registry, 142 fixtures, `setup-glibc.sh`.

## 25.02: patch+run

- command: `PCK_TEST_FILTER=q6 timeout 5400 scripts/build-test-shadgpu.sh --patch --run`
- host: shad-gpu (via dev)
- wall time: 0m04s (23:27:26Z to 23:27:30Z)
- outcome: red
- signature: `/home/info/peacockdb/cpp/install/bin/peacock_cpu_tests: /home/info/glibc-2.35/lib/libc.so.6: version `GLIBC_2.38' not found (required by /home/info/peacockdb/cpp/install/bin/peacock_cpu_tests)`
- notes: the patch step itself finished (`Verified: every shipped executable uses
  /home/info/glibc-2.35/lib/ld-linux-x86-64.so.2`); then all five C++ binaries and all five Rust
  binaries exited 1 on the same loader error before running a test, so `GPU test run FAILED`. dev is
  Ubuntu 24.04 with glibc 2.39: `objdump -T` shows the C++ binaries and `libpeacock_gpu.so` need
  `GLIBC_2.38` and the Rust binaries `GLIBC_2.39`, and shad-gpu's patch target is glibc 2.35 — older
  than what a dev-built binary needs, which a 22.04-class builder never hit. Not a `bad_alloc`, so
  not re-run. Not a bug in this branch; a host-shape finding for a task on master.

## Not driven

Rows of the two `build-test.md` tables the spec's four workflows do not reach, and why:

- `scripts/docker-build.sh` (containerized): not in the spec's list; Docker 29.2.1 is on dev, so it
  was reachable and simply not asked for. It is also the 22.04-class builder that would sidestep the
  `GLIBC_2.38` red below — a master task deciding that fix should drive it.
- `scripts/cost-report-preview.sh` (same row as cost-report): not asked for.
- `scripts/build.sh` (C++ only, 25.02): driven indirectly — `build-test-shadgpu.sh --build` calls it
  three times (`--configure`, `--build`, `--install`), so the `25.02: build` section covers it.
- shad-gpu `--all`, `--run-detached`, `--run-status`: the spec asked for three foreground calls, so
  the detached path — the documented one for a run that outlives ssh — is unproven here.
- `--host verda --all` and `--host verda-gpu --gpu --all`: `verda` does not resolve from dev; the
  same script ran as `--host dev` in three steps instead. verda-gpu was not attempted.
- `nebius`: manual, no script; nothing to drive.

## Analyst: completeness

Read as one change: `git diff master...ENS-dev-setup-check` plus this file, against the spec's
"Done when", "Constraints" and "Scope", the plan's four tasks, and the two `build-test.md` tables
the spec says are driven whole. Nothing blocking. Three important items, all about the record,
each a few lines the coordinator can write.

### Table coverage — named in the tables, absent from the record

Local build workflows (6 rows) and Remote hosts (4 rows), against the spec's four:

| table row | driven? | accounted for by the spec's four? |
|---|---|---|
| rust-only | yes (workflow 1) | yes |
| cost-report `cargo test -p cost-report` | yes (workflow 2) | yes; `scripts/cost-report-preview.sh` in the same row was not driven and is not named |
| C++ only, cudf 25.02 `scripts/build.sh` | yes, indirectly: `build-test-shadgpu.sh --build` calls `scripts/build.sh --cudf_ROOT … --gcc-version 12` three times (`--configure`, `--build`, `--install`, script lines 98–100) | in effect; neither the spec nor the `25.02: build` section says so |
| C++ + staged Rust, cudf 26.02 | yes (workflow 3) | yes |
| Rust + cudf (FFI) | yes, "via build-test scripts", in both `--build` steps | yes; the manual `scripts/cargo-cudf.sh` form is the table's own alternative |
| **Any of the above, containerized — `scripts/docker-build.sh`** | **no**. Docker 29.2.1 is installed on dev (`/usr/bin/docker`), so it was reachable | **no**, and the record does not say it was skipped |
| shad-gpu `--build --push-binaries --patch --run` | yes (workflow 4) | yes; `--run-detached` / `--run-status` and `--all` not driven, and the spec's "three foreground calls" only half-explains that — `--run-detached` + `--run-status` is the documented path for a run that outlives ssh |
| verda `--host verda --all` | no; `ssh verda` cannot resolve from dev (coordinator log) | by substitution: the same script ran as `--host dev`, in three steps rather than `--all` |
| verda-gpu `--host verda-gpu --gpu --all` | no; not even resolved | no; the `--gpu` path is unexercised. Shares verda's volume, so nothing was lost this time |
| nebius | manual | yes |

The one row that is both reachable and unaccounted for is `docker-build.sh`. It matters more than
its size: the shad-gpu red (`GLIBC_2.38`) is a dev-built binary needing a newer glibc than the
2.35 shad-gpu is patched to, and the containerized build is exactly the 22.04-class builder that
would not hit it. A master task reading "every workflow the two tables name" would take the
containerized path as proven on dev. It is not, and the record should say so in one line.

### Important 1 — record: name what was not driven

Add a short "Not driven" list to this file: `scripts/docker-build.sh` (reachable, outside the
spec's four), `build-test-shadgpu.sh --run-detached` / `--run-status` / `--all`,
`build-test.sh --host verda-gpu --gpu --all`, `scripts/cost-report-preview.sh`, and the manual
`scripts/cargo-cudf.sh` form. One line each, with why. Without it the record's coverage claim is
wider than its evidence.

### Important 2 — record: the CI half, and the head-rollup trap

The spec says the coordinator's half — PR, CI green, `done` — "is the other half of what this
task proves", and the two-line diff exists so CI runs. The record carries one sentence about CI
("the cost-report job on `e68f82cc` went green"). At this reading:

- run 34658235414 (`e68f82cc`, the code commit): `Changed paths` pass, `S3 datasets` pass,
  `Cost report` pass, `build 25.02 for GPU` pass, `Deploy cost report` skipped (not master);
  **`CI Pipeline (cudf 25.02)`, `(cudf 26.02)` and `GPU Tests (remote)` still in progress**.
- runs 34658246721 (`526f47d9`) and 34658602026 (`ee479740`), the two board commits: doc-only,
  so the `changes` gate skipped every job. `gh pr checks 146` therefore shows every job as
  `skipping` and exits 0, because it reads the head commit's rollup. A coordinator that reads
  "CI green" from `gh pr checks` on this PR reads a run that built nothing.

Before `done`, the coordinator log needs: the run id judged, per-job outcome, and the sentence
that the head rollup is all-`skipping` by design and where the real run lives. That is the
"watching CI without a terminal" evidence the spec asks for, and today it is not in the record.

### Important 3 — record: what the exec-model timeout did not run

`case 286 of 306` is exact: pytest collects the thirteen files alphabetically, the ten prototype
files are 216 cases, `test_tpcds.py` adds 71 (positions 217–287) and `test_tpch.py` 19
(288–306); q23 is the 70th TPC-DS registration, position 286. So "the 285 before it all passed"
is right, and it also means `test_tpcds.py`'s last case (q64) and **all 19 `test_tpch.py` cases
never started**. `test_tpch.py` is the dataset-matrix "TPC-H plan shapes" set and the only
python set that reads the sf1 parquet on dev, so its readiness there is unrecorded while the
section reads as if the python tiers ran through. One sentence fixes it. The 216 the cost-report
tier actually runs did all pass, so the coordinator's "no fix task needed" stands.

### Checked and found complete

- Constraints: the developer's diff is the three named files; no golden, no `Cargo.lock`, no
  dependency moved; both failures recorded and not re-run; PR open, base `master` verified, 5
  commits, unmerged. The `Host dev → localhost` entry that appeared mid-run is recorded with its
  mtime and attributed outside the run — honest, and a host-shape fact for master.
- Scope: `test_golden_format.rs` one `#[test]` pinning `count`'s panic branch, which no existing
  case reached (the three other `count(` calls all assert `Some(n)`); `test_cudf.cpp` one comment
  above the first `TEST`; `build-test.md` the two counts and nothing else. No stale copy of
  `1569` / `Rust 1135` / `26` survives outside historical proposals under `llm-wiki/reports/`.
- Plan tasks 1–4: every section the plan names exists in the plan's shape with command, host,
  wall time, outcome, signature, notes.
- Worktree: `git status` clean; only ignored build dirs, the two dataset symlinks and
  `.claude/ensemble/` remain.

### The two non-green outcomes — enough to act on without a re-run?

- exec-model timeout: **yes.** Command, rc 124, the case at the kill and its ordinal, the cause
  (`--ignore` one file short; `test_tpcds.py` is the 71-case DuckDB-oracle set CI runs from
  `exec-model-corpus.yml`), and the CI comparison are all present. Add the one sentence above
  about `test_tpch.py`. No master task is needed; the record should say that plainly.
- shad-gpu `GLIBC_2.38`: **yes.** Verbatim loader line; the patch step verified; all ten binaries
  failed at load; dev is Ubuntu 24.04 / glibc 2.39; `objdump -T` per binary (C++ and
  `libpeacock_gpu.so` need 2.38, Rust 2.39); shad-gpu's target is the hand-built 2.35
  (`scripts/setup-glibc.sh` `GLIBC_VERSION="2.35"`, host system 2.31 per that script's
  comment). A master task can choose between raising that version and building through
  `docker-build.sh` from the record alone. Not recorded, and not needed to act: which symbols
  pull the 2.38/2.39 versions.

### `architecture.md` sentences this branch falsified

None. The branch changes no engine code, plan shape, wire format, tier placement or CI wiring.
The one adjacent section, "Node display", describes the execution line's `output_rows` /
`output_bytes` fields; the new case feeds `GpuFilter: output_rows=many` to the reader and pins a
panic, which the page's format implies rather than contradicts.
