# aggregate-state-types — run record

Chain B, task 3. Branch `ENS-aggregate-state-types` off `ENS-decimal-precision-at-export` at
`d5fa0d1a`; PR targets `ENS-decimal-precision-at-export`. Tasks 1 and 2 are `done` (PRs #158,
#159 green) and await the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down** (name resolution), so rust-only proofs run locally; **shad-gpu up**,
  0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from task 2's cycles; `cpp/build` present.
- Pre-dispatch: the three `bug_` pins the spec names are all in
  `tests/gpu_tests/aggregate_cases.rs` (`:268`, `:639`, `:679`), not at the spec's line numbers;
  23 registry rows carry `163`. Master's #216 ("the device's global aggregate has no Welford
  arm", `tickets.md`) arrived with the rebase and is adjacent: its two pins in
  `aggregate_dimension_cases.rs` were converted to export refusals by task 2 and are not this
  task's — this task's Welford change is the cpu's count cast, the device changes nowhere.
- The join-batching ticket is **#220** (master took #215 during the chain's rebase); the
  registry rule for this rollout names #185, #220 and whatever the 23 rows show next.
- Routing: the developer works `aggregate-state-types-impl.md` task by task — `state_type` and
  its table tests red then green, `decompose` deriving, the cpu's Welford projection, the
  finalize cast, the goldens regen (three classes and no other), then one shad-gpu cycle for
  the harness and one for the rollout at `tp1_single` and the other four modes, then the record.
- The spec's restriction holds: output types stay DataFusion's; `Sum`'s decimal rule quoted,
  not reinvented; `aggregate.cpp` untouched; nothing casts a count to `UInt64`.

## Developer notes

### Dispatch 1 — the plan's tasks 1 to 5, local

Tests first, each red for its reason, then green:
- `plan::aggregates::tests` (new `plan/aggregates/tests.rs`): `no method named state_type`,
  then green. The producer half runs each arm's DataFusion accumulator over four rows of
  each input type, the argument coerced as the logical planner coerces it
  (`data_types_with_aggregate_udf`), and asserts `state()`'s type equals `state_type`.
  `Mean` and `M2` are `stddev`'s state columns 1 and 2; `Count` is `count`'s.
- `schema_tests::avgs_state_columns_are_typed_by_the_aggregator_that_produces_each`: red
  on `(15,2)/UInt64`, green on `(25,2)/Int64` once `decompose` derives.
- `cpu_backend::tests::state_types::every_init_emits_the_state_its_decomposition_declares`
  (new): every `AggFunc` over `Int32`, `Decimal128(15,2)` and `Float64`, built through
  `CpuExec::aggregate`; red on `column 1 is Int64 in the declared state and UInt64 in the
  one DataFusion's accumulators produce` for the Welford pair, green after the cast stage.
- The two Welford cases in `cpu_backend/tests/{exec,accumulate}.rs` moved to `Int64` counts.

Deviations from the plan's letter, with reasons:
- **The count cast is a second `Stage`, not a `ProjectionExec` over the `AggregateExec`.**
  `execute_single_node` runs exactly one operator and replaces its children with the batch,
  so a projection wrapping the aggregate was fed the raw input (`Column 'stddev(v)[mean]'
  at index 2 but input schema only has 2 columns`). `aggregate_exec` now returns the
  operators a phase runs in order — one for a merge, one or two for an init — and
  `CpuExec::aggregate` makes a stage of each; `counted_as_declared` builds the cast over a
  placeholder of the aggregate's own schema.
- **`check_state_layout` takes the `Phase`** and admits `widened_decimal` at a merge only.
  An init's accumulators are the producers the table was read off, so the producer test
  proves the init exact with no escape; the merge still sums an already-widened sum.
- **`merge_m2` casts the count both ways**: DataFusion's variance accumulator reads
  `UInt64Array` in `merge_batch` and returns a `UInt64` scalar in `state()`, so `Merging`
  casts the arriving count to `u64` and the emitted one to `Int64`.
- **The finalize's numerator is the bare sum, not a cast to the output type.** The spec's
  shape — `Cast(Cast(sum, (p_out, s_out)) / Cast(count, (p_out, 0)), (p_out, s_out))` —
  went red on six cpu queries with the `avg` one digit off in the last place (`25.516472`
  against DataFusion's `25.516471`) and on q9 with `21267696830000 is too large to store in
  a Decimal128 of precision 11`. Arrow's decimal divide truncates at the numerator's scale
  plus four (`arrow-arith/numeric.rs`, "Follow postgres and MySQL adding a fixed scale
  increment of 4"), so a numerator at `s_out` lands four digits past the declaration and
  the narrowing cast then rounds, where DataFusion's `DecimalAverager` and cuDF's
  fixed-point divide truncate; and a sum cast to the output's precision overflows where the
  average would not. With the sum at its own scale `s_in` the quotient is `(p_sum + 4,
  s_in + 4)` — `s_out` exactly, since `avg_return_type` is `(min(38, p + 4), min(38, s +
  4))` and arrow caps the same way — truncated as the oracle truncates, and the outer cast
  narrows precision alone. The device ignores the numerator's scale: `expr.cpp`'s pre-scaled
  divide casts it to `e_o + e_r` itself. The plan text reads `CAST(sum / CAST(count AS
  Decimal128(p,0)) AS Decimal128(p,s))`.
- The finalize cast lands in the `.plans.txt` goldens as well as `recipe-payloads.txt`,
  since the plan text renders `final=[…]`; same class, more files than the spec named.

Golden diff, classified mechanically (`git diff -U0 -- testdata/goldens`, every changed
line reduced by the three substitutions and compared): 989 count columns `UInt64 → Int64`,
621 `avg` `$sum` columns `(p, s) → (min(38, p + 10), s)`, 224 `avg` finalizes gaining the
cast (220 in the eleven `.plans.txt`, 4 in `recipe-payloads.txt`), and the 4 digests of those
4 payloads. Zero lines outside. 24 queries move: the 23 registry rows carrying `163` plus
`tpch/shuffle-stddev`.

Verification, local (verda down, `--test-threads=2`): `--lib` `562 passed; 2 ignored`;
`test_corpus_goldens` 20; `test_cost_model` 3; `test_module_layout` 17; `test_ci_coverage` 8;
`test_golden_format` 26; the 99 cpu cells of the rollout `99 passed` under
`PCK_UPDATE_SECTIONS=1`.

### Dispatch 1 — the cpu rollout, 23 rows at all five modes

| verdict | queries | registry |
|---|---|---|
| green, all five cpu modes | tpch q1 q17 shuffle-additive-avg; tpcds q1 q6 q7 q9 q13 q14 q17 q26 q30 q32 q35 q65 q81 q85 q92 | `cpu_*` enabled |
| green with the tolerance oracle | tpcds q39 — `data_fusion_approximate`, `golden_approx_std`; the row set equals DuckDB's, `cov` differs in the last digits at every mode (the Welford merge over batches), as `shuffle-stddev` found | `cpu_*` enabled |
| tp1 green, tp4 the rollup's `UInt8` id (#189) | tpcds q18 q22 | `cpu_tp1_*` enabled, `189` added |
| the nested-loop join's projection dropped (#190), all five | tpch q22, tpcds q24 | `190` added |

A trap for the next regeneration: the golden writer renders `skipped: not enabled at this
mode` for every cell the **csv** marks disabled, whatever the run did, so sections written
under `PCK_UPDATE_SECTIONS=1` before the csv is flipped are dropped on the spot. Flip the csv
first, then run.

### Dispatch 1 — the device cycles and the rollout

shad-gpu, 0 MiB held at each cycle, every `[rmm] pool on a discrete device` line a reservation,
never `could not be built`. Each cycle: `--build` (warm, minutes, no warnings),
`--push-binaries --patch`, `--run` under `PCK_TEST_FILTER`. Both rust binaries executed each
time; the five C++ binaries too (`peacock_cpu_tests` 12, `peacock_gpu_tests` 6,
`peacock_plan_tests` 41, the two sf40 pairs 4 and 4, all `PASSED`).
- Harness, `PCK_TEST_FILTER='_cases'`: `peacockdb_core_gpu_lib` `test result: ok. 329 passed;
  0 failed`; `test_gpu_corpus` `0 passed … 37 filtered out` (nothing of its matches). The three
  former pins ran green as positive cases —
  `a_welford_init_exports_its_count_as_int64`, `a_welford_merge_exports_its_count_as_int64`,
  `a_decimal_average_finalizes_to_its_declared_type_on_both` — the last one `same(Order::Any)`
  on both engines, so the truncation agrees to the digit.
- Rollout 1, `PCK_TEST_FILTER='gpu_'`, the 21 cpu-green rows at `gpu_tp1_single`: lib `389
  passed`; corpus `15 passed; 21 failed` — 18 of them on a `skipped: not enabled at this mode`
  marker, the csv trap above (the cpu sections had been dropped), 3 real.
- Rollout 2, after the cpu rerun and `--push-binaries --patch`: `19 passed; 17 failed` — four
  new cells green, 13 on the next cause. A `--push-binaries` without `--patch` in between
  segfaulted every binary (the mirror re-ships unpatched binaries; the script's own notice names
  it), so the phases are always `--push-binaries --patch`.
- Rollout 3, the four passers at their other four modes: tpch q1 and shuffle-additive-avg green
  at all five; q17 and q85 `#152`'s build-side copy at every other mode.
- Final, on the final declarations and csv: `peacockdb_core_gpu_lib` `test result: ok. 389
  passed; 0 failed`; `test_gpu_corpus` `test result: ok. 27 passed; 0 failed` — 26 cells plus
  `the_registry_matches_the_gpu_corpus_in_both_directions`; the regen case filtered out.

### Rollout — 23 rows on the device at tp1_single, then the other four modes

Each cell's outcome read from the sink message or the golden's node line at the reported line.
No cell fails on a state type or on the finalize; the "next cause" is what #163 hid.

| verdict | queries | registry |
|---|---|---|
| green at all five device modes | tpch q1, shuffle-additive-avg | every `gpu_*` enabled, no ticket left |
| green at tp1_single, #152's build-side copy at the other four | tpch q17, tpcds q85 | `gpu_tp1_single` enabled, `152` |
| golden, `in_rows` at a BatchAccumulator (#185) | tpcds q1 q7 q13 q14 q22 q26 q32 q65 q92 | `185` added |
| golden, join batching (#220) — `output_bytes` at a join and the coalesce above it | tpcds q18 q30 q35 q81 | `220` added |
| golden, the empty answer: 12 bytes at the device's unload against the cpu's 0 (#205) | tpcds q17 | `205` added; dated line on #205 |
| device refusal, the value-form CASE in the filter (#57) | tpcds q39 | `57` kept |
| device failure in the CASE project over the scalar subqueries, `copy_if_else` "Both inputs must be of the same type" (#63's site) | tpcds q9 | `63` kept; dated line on #63 |
| device refusal, the probe-batch copy (#152) | tpcds q6 | `152` |
| cpu off, never reached a device | tpch q22, tpcds q24 (#190); tpcds q18 q22 at tp4 (#189) | `190` / `189` added |

`163` is struck from every one of the 23 rows: each row's remaining disabled cells carry a
ticket, and the two rows with none disabled carry none. No `163` anywhere in the csv.

Outside the Scope table, comment-only, each a sentence the change made false:
`plan/aggregate.rs` (`check_merges_the_state_it_was_given`'s doc), `plan/validate.rs`
(`types_across_the_edge`), `plan/mod.rs` (`PlanAgg::tag`), `tests/end_to_end.rs`
(`columns_of`), `executor/gpu_backend/gpu_tests/accumulate.rs` (the Welford merge case's doc);
`tickets.md` gained a dated line on #205 and one on #63. `#163` moved to
`archive/archived-tickets.md` with its closing paragraph, fourteen lines.

Restriction check: `git diff --stat -- cpp` is empty; `grep -rn "UInt64" peacockdb-core/src
--include=*.rs` outside `wire/generated` finds `common.rs`'s byte table, `serialize.rs`'s
literal arm, and `merge_m2.rs`'s cast of the count *into* DataFusion's accumulator — nothing
casts a count to `UInt64` on the way out; `finalize`'s `out_type` is still DataFusion's.

### Round 1 fix — the escape pinned

The finding: `check_state_layout`'s `phase == Phase::Merge` clause had no test that turns
red. `cpu_backend/tests/state_types.rs` now pins both halves, each red on its own clause
before the clause was restored:
- `an_init_declaring_a_narrower_decimal_than_it_produces_is_refused`: `sum(d)` over a
  `Decimal128(15, 2)` with its state declared at `(15, 2)` — the pre-task declaration —
  and `CpuExec::aggregate` returns `Err` containing `column 1 is Decimal128(15, 2) in the
  declared state and Decimal128(25, 2) in the one DataFusion's accumulators produce`. With
  the phase clause dropped (`|| widened_decimal(...)` alone) it fails at the `.expect`: the
  init builds. With the clause, green.
- `a_merge_over_a_widened_decimal_sum_constructs`: a `GpuAggregateBatches` summing a
  `(25, 2)` `$sum` column, built through `CpuAccumulator::aggregate`. With the escape removed
  entirely it fails on `column 1 is Decimal128(25, 2) in the declared state and Decimal128(35,
  2)` — DataFusion's sum widens by ten digits again at the merge, which is the escape's one
  remaining reason. No existing `--lib` test built a merge over a decimal state; the corpus
  rows that do are the cpu rollout, not the lib.
- `init_of` now delegates to `init_declaring`, which takes a `declare` closure over
  `state_type`'s answer, so the wrong-by-design declaration reuses the exact-case builder.

`mod.rs` is byte-identical to the commit. `--lib`: `564 passed; 0 failed; 2 ignored`, no
warnings; `git status --short testdata/` empty. `build-test.md`: `--lib` 564 → 566, cpu
1137 → 1139, `CPU backend executors` 66 → 68 (the row's count excludes `contract`, which has
its own row), verified against `--list` (566 lib entries, 69 under `cpu_backend::tests::`).

## Reviewing — 2026-09-17

Dispatch 1 committed as `636c0e96` on `1f7723c1`, pushed; PR #160 against
`ENS-decimal-precision-at-export`, base verified. The developer's one deviation from the spec's
finalize shape — the bare sum divided and the quotient cast, rather than the numerator cast
first — is a finding on the spec, recorded under Developer notes with the digit that decided
it; the reviewer judges it. Comment-only edits outside the scope table are listed there too.
