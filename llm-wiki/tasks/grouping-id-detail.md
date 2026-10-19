# grouping-id — working record

## Standing facts

- Fifth of chain K (folded in from chain L), branch `ENS-grouping-id` off `ENS-empty-sorts`
  (`ad2bd77b`); its PR targets `ENS-empty-sorts`. The chain's base is master `ca4dedeb`, which
  carries chain J (#152, #220, #189 closed) and the DISTINCT lowering.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable: no `VERDA_CLIENT_ID`). The final device run is the dispatched `GPU Tests
  (nebius-gpu)` CI job.
- Pulling `gpu-result.txt` home from nebius-gpu: rsync it back through `gpu-host.sh rsh` before
  the next sync, which overwrites the VM's copy (learned on empty-sorts).
- q17's tp1 device cells are masked on #225, set by empty-sorts; not this task's.

## Dispatch log

### 2026-10-19 — `building`, developer dispatched

nebius-gpu started for the dispatch. Developer told: spec and `grouping-id-impl.md`, red build
first, corpus run filtered to the spec's rows and `rollup-grouping`.

## Developer notes (round 1)

### What moved since the plan was written

- The registry already carries no `55` and no `262` on q66, q28 and `distinct_functions`: their
  five device cells were on before this task (join-backend's task 14 for q66; chain K's resumed
  distinct-companions for q28 and distinct-functions). `distinct_functions`' gpu oracle was
  already `golden_approx_std`. So Task 5's candidates were the `65` rows alone: tpcds q5, q14,
  q18, q22, q77, q80; tpch `rollup_over_join`, `rollup_distinct`, `rollup_grouping`; pbench
  `rollup_small_keys`. q67 has no candidate (cpu `na`, window functions, #32 #143).
- `python3` on this host already carries DuckDB 1.5.4; no venv was needed for
  `testdata/duckdb_result.py`. Its tpch run moved only the new `rollup-grouping` section.

### Things a successor should know

- `test_a_masked_key_is_null_rather_than_absent` (exec model) passed vacuously on the red run:
  selecting id 1 under the old fold found no rows, and `.all()` of nothing is true. It now
  asserts the selection holds the two `(a)` rows first.
- The plan's walk counts were checked before any device run by rendering the four new walk
  queries' plans and recipes from a throwaway rust-only test (not kept): quotient 4 partials,
  5 merges, 3 finalizes; DISTINCT beside companions 4 partials, 6 merges, 2 finalizes (the merge
  count is pinned too); the eight-key DISTINCT rollup two inits, both `UInt16`; nation's scan at
  `TWO_LANES` maps one row group to lane 0 and none to lane 1.
- **The walker ran an accumulator's at-done chain over a lane with no batch.** First seen on the
  red build: `a_grouping_over_every_key_answers_datafusions_id` failed with
  `execute_node(#2, [0] handles) failed: … a collapse with no input handles …` at the per-lane
  merge on nation's empty lane 1, before the id mattered. Every device accumulator
  (`gpu_backend/accumulate.rs`) emits nothing and calls nothing at done over no arrivals; the
  walker did not. `per_lane` in `wire/gpu_tests/mod.rs` now pushes an empty lane instead. No
  earlier walk had an empty lane, so nothing else moved. A test-harness defect, not production.
- One pre-existing warning in the gpu lib build, untouched by this task: unused `AsArray` in
  `src/tests/gpu_tests/aggregate_dimension_cases.rs:9` (the file is unchanged on this branch).
- Comments naming #65 that the fix made false were corrected in place: `planner/translator/tests.rs`
  (two) and `planner/translator/aggregate.rs` (`drop_grouping_id`'s doc), the last outside the
  spec's scope table but a comment-only edit.
- **The trial recording kept one section per dataset**, not 45. `merged_cells`
  (`test_support/corpus_golden.rs`) drops every section whose cell the *registry* does not
  enable, and the plan's trial left the registry untouched, so each write dropped the trial
  sections before it. Working as designed. The order that works: settle the cells, write the
  registry, carry it, then record the passing cells on the final build (done so; 45 sections
  added, none removed).

### Device runs (nebius-gpu L40S, `~/peacockdb-K`, cuDF 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2`)

One sync (`gpu-host-sync.sh --dest peacockdb-K`, 41 GB free before every build); after it, files
travelled by name with `rsync -aR … gpu-host.sh rsh`. Logs on the VM in `~/K-logs/gid/`.

| build | tree | command | result |
|---|---|---|---|
| red (rc 0, no warning) | sync, `aggregate.cpp` reversed by patch | `peacock_plan_tests --gtest_filter=GroupingId.*` | 0/8: seven `type id 3` (INT32), `SixtyFiveKeys…` "answered" |
| red | same | `peacockdb_core_gpu_lib` over the two harness files' `grouping_sets`, `wire::gpu_tests::`, the contract | 21 pass, 8 fail: both harness cases (schema differs), schema case (`UInt8 vs INT32`), rollup partial (`Int32`), narrowed id (`[Int32, Int32]`), the contract (export refuses `UInt8 vs Int32`), the GROUPING() walk and the kinds walk (both on the walker's empty-lane collapse, see above). The quotient and DISTINCT walks pass |
| green (rc 0, no warning) | fix restored, walker fix carried | `peacock_plan_tests` | 89/89 |
| green | same | `peacock_gpu_tests` / `peacock_join_session_tests` | 2/2, 53/53 |
| green | same | `peacockdb_core_gpu_lib gpu_tests::` | 713/713 |
| green | trial `.inc` | `test_gpu_corpus` over the candidate rows plus q28, q66, distinct-functions, `PCK_WRITE_GPU_RESULT=1` | 60 pass, 5 fail (q5, all modes) |
| final (rc 0, no warning) | final `.inc` and registry carried | `test_gpu_corpus` over the nine rows now on, `PCK_WRITE_GPU_RESULT=1` | 45/45, sections brought home |
| final | same | `gpu-host-test.sh run` (the L40S list) | `ALL rc=0`: cpu 12, gpu 2, join session 53, plan 89, gpu lib 713, `test_gpu_corpus` 835, `test_node_timing` 1, benchmarks 8 (3 `bench_` skipped) |

Deferred, per the board's host note: the sf40 binaries (`peacock_tpch_tests`,
`peacock_tpchv_tests`), `--run-benchmarks`, Nsight, any H200 timing. VM soft-stopped (rc 0).

### Cells

| row | gpu modes before | verdict | tags after |
|---|---|---|---|
| tpch `rollup_over_join` | none | 5/5 pass | — |
| tpch `rollup_distinct` | none | 5/5 pass (golden_exact kept) | — (was `65 262`) |
| tpch `rollup_grouping` (new) | — | 5/5 pass | — |
| tpcds q14, q18, q22, q77, q80 | none | 5/5 pass each | — (was `65`) |
| pbench `rollup_small_keys` | none | 5/5 pass | — |
| tpcds q5 | none | 0/5: `GpuProject lane 0: the output hook refused a batch: 4 return_amt: Decimal128(7, 2) vs FLOAT64; 5 net_loss: …` — #210 as written | `210` (was `65 210`) |
| tpcds q67 | `na` | no candidate | `32 143` (was `32 65 143`) |
| q28, q66, distinct-functions | all five | rerun, 15/15 pass; nothing to turn on | unchanged (no `55`/`262` left) |

Every enabled cell also meets DuckDB: `test_cpu_corpus`'s 45 new `duckdb_gpu_*` cases and 9 new
`device_order_*` cases are green. No new ticket. Next free number is still 287.

### Local proof (this host; verda unreachable)

- exec model: 11 CI files 232 pass (`test_operators.py` 64, `test_end_to_end.py` 18); 
  `test_tpch_corpus.py` 22/22, `test_tpcds.py` 71/71.
- rust-only: `--lib` 818 pass + 2 ignored (#182 pair); `test_cpu_corpus` 2071; `test_corpus_goldens`
  28; `test_cost_model` 3; `test_golden_format` 43; `test_module_layout` 18; `test_ci_coverage` 11.
- C++ `cpp/build` against 25.02: build rc 0, `ctest -L cpu` 1/1 (12 gtests).
- `cost-report --cost-diff --base ENS-empty-sorts` (`ad2bd77b`): `870 compared, 0 changed,
  0 regression(s), 0 over 10%`, rc 0.
- `recipe-payloads.txt` unchanged.
- After the last device run, one assert in `a_summed_quotient…` was rewrapped to rustfmt's shape (whitespace only); the gpu lib recompiled clean locally. The four other rustfmt diffs in `wire/gpu_tests/mod.rs` are on HEAD too.

### 2026-10-19 — green on both engines; PR opened; `reviewing`

Coordinator applied the developer's wiki list: `build-test.md` counts and the device-cell prose,
`architecture.md` "Grouping sets" (the id's width and fold), #65 and #262 archived under Done and
#55 under Stale, `tickets.md` 99 open (corpus-coverage 24), and #65 references in #210, #228,
#230 and #144 rewritten or relinked. Review round 1 dispatched.
- PR #181 → ENS-empty-sorts.

### 2026-10-19 — review round 1: 0 blocking, 3 important, 5 nits

1. Important: the 16 and 32 width boundaries are pinned only from above — add 16-key `UInt16`
   and 32-key `UInt32` gtests.
2. Important: the reason for not hashing the id is false in three places — the cpu hashes
   unsigned keys since repartition-keys (`spark_partitioning.rs:90`). `build-test.md` fixed by the
   coordinator (the subset rule makes hashing it unnecessary); the two translator comments
   (`translator/tests.rs:910`, `translator/aggregate.rs:590-591`) go to the developer.
3. Important: no test reads the id's value under the DISTINCT lowering — `grouping(<all keys>)`
   in a DISTINCT rollup walk against DataFusion.
Nits to the developer: `hand_tables.hpp:48` history clause and dead null branch;
`test_operators.py:554-555` comment; `aggregate.cpp:339-350` comment over the cap. Coordinator:
the stray space in `build-test.md`, and #262's link in the board note. nebius-gpu started
21:01Z; developer resumed.

## Developer, review round 1 (on af11d265)

- **16 and 32 pinned from below.** `GroupingId.SixteenKeysStayUInt16` ({32768, 65535}) and
  `GroupingId.ThirtyTwoKeysStayUInt32` ({2^31, UINT32_MAX}). Mutation A (`<= 16`→`< 16`,
  `<= 32`→`< 32` in `grouping_id_column`): exactly those two red, the other eight green.
- **The id's value under the DISTINCT lowering is now read.** `DISTINCT_UNDER_A_WIDE_ROLLUP`
  gains `grouping(<all eight keys>) AS g`, so `held`'s oracle compare reads the value through
  the `UInt16`→`UInt8` narrowing. The planner takes it, and DataFusion answers it (checked from a
  throwaway rust-only render, not kept; the two-key form plans too, g in {0, 1, 3}). Mutation B
  (fold from the other end: `gid |= bit i`): the walk red on the answer ("differs from the oracle
  … first difference at sorted row 0"), as is the GROUPING() walk; cuDF's narrowing cast
  truncates silently rather than refusing, so only the answer could see it. No new walk: the
  walk count stays 25.
- **Hash-reason comments** (`planner/translator/tests.rs`, `planner/translator/aggregate.rs`):
  the subset rule makes hashing the id unnecessary; the cpu does cast `UInt8`/`UInt16` keys to
  `Int32` (`cpu_backend/spark_partitioning.rs`).
- Nits: `hand::ref` always writes a name, its comment trimmed; the exec-model comment; the
  grouping-set arm's in-function comment cut to four lines pointing at architecture.md.
- One mistake on the way, no effect: the chain's first start deleted its own script
  (`rm -f r2*` matched `r2chain.sh`) before patching anything; restarted as `chain-round2.sh`.

### Device runs, round 2 (resynced tree; 41 GB free before each build; logs `~/K-logs/gid/r2-*`)

| build | command | result |
|---|---|---|
| A, thresholds mutated (rc 0) | `peacock_plan_tests --gtest_filter=GroupingId.*` | 8/10: `SixteenKeysStayUInt16`, `ThirtyTwoKeysStayUInt32` red |
| B, fold reversed (rc 0) | same | 3/10 (one key, eight keys, 65 refused stay green) |
| B | gpu lib: the eight-key DISTINCT walk, the GROUPING() walk | 0/2, both on the answer |
| green (rc 0, `aggregate.cpp` md5 = workspace) | `gpu-host-test.sh run` | `ALL rc=0`: cpu 12, gpu 2, join session 53, plan 91, gpu lib 713, `test_gpu_corpus` 835, `test_node_timing` 1, benchmarks 8 |

Local: C++ build rc 0, no warning; `ctest -L cpu` 1/1; `test_operators.py` 64; rust-only
`--lib` 818 + 2 ignored; gpu lib compiles (the one pre-existing `AsArray` warning). Records
unchanged (md5 equal both sides). VM soft-stopped, rc 0.

### 2026-10-19 — round 1 closed; `completing`

All three important and three nits fixed by the developer, each new test shown red on a broken
build; full L40S list green again (plan 91). Coordinator: `build-test.md` C++ 180, total 5180,
plan-executor 91 and its prose, the walk's GROUPING() under the narrowing. Completeness pass
dispatched: reviewer and analyst.
