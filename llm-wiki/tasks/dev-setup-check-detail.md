# dev-setup-check — run record

Spec: [`dev-setup-check.md`](dev-setup-check.md). Plan: [`dev-setup-check-impl.md`](dev-setup-check-impl.md).

## Coordinator log

- 2026-09-11 22:34Z — chain started under the watchdog in worktree
  `~/workspace/peacockdb-ENS-dev-setup-check`, hostname `mild-face-glows-fin-03` (this is `dev`).
  Task branch is the chain branch `ENS-dev-setup-check`, forked from master at `62335cf`; PR
  will target master. Board moved to `building`; developer dispatched with the impl plan.
- 2026-09-11 23:28Z — developer returned green: new case red then green, four workflows
  recorded below (exec_model timed out at 900 s; shad-gpu `--patch --run` red on glibc 2.38).
  Committed, pushed, PR opened against master; board moved to `reviewing`.
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
