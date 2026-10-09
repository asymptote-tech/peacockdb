# limits — working record

## Standing facts

- Third of chain K, branch `ENS-limits` off `ENS-distinct-companions`, PR will target that
  branch. Closes [#186](../tickets/corpus-coverage.md#t186) and
  [#234](../tickets/corpus-coverage.md#t234).
- **Chain K runs without a GPU.** No device build, no device run, no GPU cycle. Every build and
  run is local — verda does not resolve from this host (checked again at this dispatch). The
  `scan.cpp` edit and the four `gpu_tests/source_cases.rs` cases are built, not run; #281 holds
  them. The task reaches `done` when every CI job but the GPU tests is green.
- **The cost gate's rises are pre-accepted by the human** (2026-10-08, on the board): this task
  is `done` when the cost-report job's only regressions are the sections this file lists from a
  local `--cost-diff` run — expected `nested-limits` at tp1-rowgroup and tp4-rowgroup, +228
  bytes. Any other regression is an ordinary finding. So the local `--cost-diff` output has to
  land in this file before CI is read, or there is nothing to compare the job against.
- The chain's base is master `31c56bea`. Both tasks below this one were rebased onto it and
  re-proved today; `guard-checks-detail.md` records what master brought across. Nothing in this
  task's scope overlaps master's `64ced62e`, which touched `plan_text` and the join-projection
  rendering.

## Dispatch log

### 2026-10-09 — round 1, developer dispatched

Branch cut from `ENS-distinct-companions` at `aa0335bf`, board moved
`approved to build` → `building` in the same commit.

#### Round 1 evidence

**Per-target counts, measured at the branch head.** rust-only `--lib` 631 passed, 2 ignored
(#182), 633 cases; `test_cpu_corpus` 569; `test_corpus_goldens` 26; `test_cost_model` 3;
`test_module_layout` 17; `test_golden_format` 26; `test_ci_coverage` 9; `cargo test -p
cost-report` 36. C++ `scripts/build.sh --build` against cuDF 25.02 rc=0 with no warning, and
`ctest -L cpu` 1/1. `build-test.md`'s rows were re-summed against these, not deltaed: cpu block
1231, Rust 1902, grand total 2377, and each header equals its own row sum.

**The row-group prefix, mode by mode.** scan-limit's loader maps `[[[0]]]` at all five modes
where it mapped all 49 (tp1-single, tp4-single, tp4-sized) or 49 one at a time (the rowgroup
modes); nested-limits' part loader maps `[[[0]]]` where it mapped `[[[0,1]]]` or `[[[0],[1]]]`.
`a_limited_scan_maps_only_the_row_groups_that_reach_its_limit_at_every_mode` pins the boundary
on the minimal part (122,880 + 77,120): `LIMIT 122880` maps `[0]`, `LIMIT 122881` maps `[0, 1]`,
no limit maps `[0, 1]`.

**Golden sections that moved**, by the section differ, in all fifteen tpch files: exactly
`nested-limits` and `scan-limit`, and nothing else. `mini.result.txt`, `recipe-payloads.txt` and
every `tpcds.sf1` golden are untouched. Deletion audit by `git diff --numstat` rather than `grep
-c '^-[^-]'`, which silently misses a deleted line beginning with `-` (it scored
`nested-limits.sql`'s rewritten SQL comment as zero deletions): every deletion is accounted for
— in `source_cases.rs` the four `bug_` cases with `gpu_refuses_with`, `cpu_answered`,
`four_sixteens` and `GROUPS_AND_LIMIT`; in `end_to_end/limits.rs` `most_offered` and
`limit_node`; in `rebuild.rs` only rewritten `source(...)` calls and the split fixture, with 57
`Box::new(Gpu…)` fixtures before and after.

**The device-only code: what was proved.** No device ran. `cpp/src/operators/scan.cpp` compiles
against cuDF 25.02 (the local 25.02 build, which is what CI's `cpp-build-2502` compiles). The
gpu rung was built with `scripts/cargo-cudf.sh test --features gpu --no-run` and
`--test test_gpu_corpus --no-run`, both rc=0, and the staged lib binary's `--list gpu_tests::`
reports 536 cases — unchanged, the four `bug_` cases out and four agreement cases in — and names
all four new ones under `tests::gpu_tests::source_cases`. So they compile and are collected, not
merely compiled. `the_registry_matches_the_gpu_corpus_in_both_directions` needs no device and was
run from the built `test_gpu_corpus` binary: it passes, so the registry's gpu column still agrees
with `corpus_cases.inc` after the `186` → `281` edit. **Not proved:** that either backend answers
correctly for the four cases, and that `scan.cpp` without `set_num_rows` reads right on a device.
#281 holds both.

**The gpu rung's build found a caller the rust-only build cannot see**:
`executor/gpu_backend/gpu_tests/mod.rs:141` passed the deleted `limit` argument. The rust-only
`--lib` was green with it still there, so the `--no-run` gpu build is load-bearing for a
signature change, not a formality.

**`Empties::fires` now fires every source's first call** (`tests/injection.rs`). The injected
stamp is `mix(mix(seed, post_order), lane)`, so inserting the two `GpuLimit`s moved
nested-limits' part source from post-order 3 to 4; both sources make exactly two calls, four
coin flips at 50%, and under the new stamps none landed. `run_and_check`'s `empty_batches() > 0`
then failed at `tp1-single rebatch=sources/empties=50%` — a guard going red for a reason with
nothing to do with the engine. Proved by instrumenting the wrapper (`fires=false` on every call
of both sources, stamps and post-orders printed) rather than inferred. The guard is still
reachable: forcing the wrapper to `Empties::Never` turns it red, and restoring it green.

**The impl plan's Task 4 has two steps in the wrong order.** Its Step 3 writes scan-limit's two
tp1 sections and its Step 4 enables them in `cost-registry.csv`, but the skipped placeholder is
driven by the registry (`test_support/corpus_golden.rs`, the `enabled`/`skip` filter), not by
`corpus_cases.inc`. Under `PCK_UPDATE_SECTIONS=1` with the registry still `disabled` the writer
republishes `skipped: not enabled at this mode` and the cell passes against it — green, with the
section never written. The registry edit has to come first.

## Cost gate

`cargo run -q -p cost-report -- --cost-diff --base aa0335bf` (the merge-base with
`ENS-distinct-companions`) at the branch head: 563 sections compared, 7 changed, 2 regressions,
5 improvements, `rc=1`. CI's cost-report job fails this PR on the regressions until the human
accepts them.

| section | base | branch | why |
|---|--:|--:|---|
| tpch/nested-limits, tp1-rowgroup | 999,872 | 1,000,100 | the `GpuLimit 0..+28` over the part scan emits 28 Int64 rows (228 bytes); part was already read one row group at a time here, so nothing offsets them |
| tpch/nested-limits, tp4-rowgroup | 999,872 | 1,000,100 | the same |

These are the two the board pre-accepted, at the +228 bytes it predicted, and no other section
regressed.

Improvements: nested-limits at tp1-single, tp4-single and tp4-sized, 1,626,472 → 1,000,100 (the
part scan maps one row group, not two). scan-limit at tp4-single and tp4-sized, 1,048,882,882 →
21,476,312 (lineitem maps one row group, not 49). scan-limit's two new tp1 sections are new
against a `skipped` base and the gate omits them; both read `peacockdb_cost=21476312`, as the
tp4 modes do.

### 2026-10-09 — round 1 green, PR #172 open

Committed as `7fdaeb22`, pushed, PR #172 against `ENS-distinct-companions` — base verified,
13 commits, so the diff under review is this task and not the chain.

Three things from round 1 worth carrying forward rather than re-deriving:

- **The deletion audit this chain has been running is unsound.** `git diff <file> | grep -c
  '^-[^-]'` scores a deleted SQL or markdown comment as zero, because the line itself starts
  with `--`. It read `testdata/tpch-queries/nested-limits.sql` as 0 deletions where it has 1.
  Use `git diff --numstat`.
- **The impl plan's Task 4 has steps 3 and 4 the wrong way round.** The `skipped: not enabled
  at this mode` placeholder is written from `testdata/cost-registry.csv`, not from
  `corpus_cases.inc`, so a `PCK_UPDATE_SECTIONS=1` run with the registry still `disabled`
  republishes the placeholder and the cell passes against it — green with the section never
  written. The registry edit has to come first. Fixed in `limits-impl.md`.
- **The cost-report job on PR #172 will be red**, and that is the pre-accepted case: exactly
  the two `nested-limits` rowgroup sections at exactly +228 bytes, with nothing else
  regressed. `## Cost gate` above has the rows.

Outside this task's scope and deliberately not fixed: an unused `AsArray` import at
`peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs:9`, a warning on the gpu rung
alone and so invisible to every CPU build and to CI. Pre-existing from `0e7804ef`, cosmetic,
nothing behaves wrongly — no ticket under the house rule. Worth fixing by whoever is next in
that file.
