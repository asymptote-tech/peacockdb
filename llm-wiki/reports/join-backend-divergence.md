# join-backend: can the device's join be right and still fail or flap against the cpu?

Read-only diagnosis, 2026-10-08. Nothing built or run. No `ENS-join-backend` or
`ENS-join-session-cpp` branch exists on origin (`git branch -r` after `git fetch`), so this
reasons from `join-backend.md`, `join-backend-impl.md`, `join-session-cpp(-impl).md`,
`join-rewrite-design.md`, the join tickets, today's code, the committed goldens, the
`ENS-duckdb-oracle` and `ENS-pbench` branches, DataFusion 45's join streams
(`~/.cargo/registry/.../datafusion-physical-plan-45.0.0/src/joins/`), and cuco/cuDF source
(`~/data/...rapids-cuda-12.2/include/cuco`, `third_party/cudf`).
`sort_cuts.py`, named below, was a throwaway awk-style scan of the `.cpu.txt` goldens for sorts
whose fetch cuts a batch; it is not committed.

## Verdict

**Unlikely for the cpu-vs-device comparisons.** The one thing those comparisons are strict
about is batch structure, and the design pins it on both engines: one batch per call, `None`
in exactly the same places, and the same `owes_nothing` rule. Row order inside a batch is
invisible to every comparison:
- the harness, the gtests and every result compare sort their rows;
- the execution section counts rows and logical bytes per batch, and neither depends on order.

Order matters only through a selection above the join that depends on order: an unordered
LIMIT, or a fetch cut that falls on a tie between rows whose string columns differ. Today's
corpus and pbench have no such shape over a hash or nested-loop join (checked below).

**Two deterministic CI failures do sit in the plan**, both cpu-side and neither about
correctness:
- **The cost-regression gate.** It will go red on #137's filters and `__rowmarker__`. Nothing
  in the spec, the plan or chain J's header provides for that.
- **The task-6 adapter commit.** It moves the cpu's per-node goldens but regenerates nothing
  until task 8.

## 1. What each backend emits per call

Device: design §3 and join-session-cpp. Cpu: plan task 8, which is DataFusion 45's stream fed
through `ChannelExec` and concatenated per call.

| shape | device, per call | cpu, per call | order inside the batch | differs by design? |
|---|---|---|---|---|
| Inner, hash | probe: one table of the pairs (may be 0 rows); finish: `None` | probe: `concat(chunks)`, or `new_empty` when there are none; finish: `None` | device: cuco `retrieve`, warp buffers flushed through `atomic_counter->fetch_add` (`open_addressing_ref_impl.cuh` ~1183), **not reproducible run to run**; cpu: probe-row-major | order only |
| Left | probe: the pairs; finish: unmatched build rows, padded | the same (`process_unmatched_build_batch`) | probe as Inner; finish in build-row order on both (`apply_boolean_mask` vs bitmap ascending) | pair order only |
| Right | probe: the pairs plus unmatched probe rows (`hj.left_join`); with a residual, `concat(pairs, unmatched)` | probe: the pairs plus unmatched within the alignment range (`adjust_indices_by_join_type`) | device not reproducible; cpu deterministic | order only |
| Full | as Right; finish as Left | as Right; finish as Left | as above | order only |
| LeftSemi / LeftAnti / LeftMark | probe: `None` (handle 0); finish: `B[m]` / `B[¬m]` / `B` with `mark` | probe: DataFusion yields a zero-row chunk, and the cpu returns `None`; finish: from the bitmap | finish in build order on both | none |
| RightSemi / RightAnti | `distinct_hash_join.left_join`, then a mask; probe order | DataFusion keeps probe order | equal, deterministic (via `mixed_*` with a cross residual: gather order) | none to minor |
| Nested loop, any type | `conditional_*`, or candidates plus a mask, or a chunked cross product of indices ("row order is free here", §3.6) | `NestedLoopJoinExec`, `BatchSplitter` chunks concatenated | device: `conditional_join_kernels.cuh:86` `fetch_add` → not reproducible | order only |
| Cross (Inner, no predicate) | `cudf::cross_join(B, P)`, left-major; a zero-row side gives one zero-row table | `CrossJoinStream`: left-major per probe batch; an empty build ends the stream at once, so `new_empty` | **identical and deterministic** | none |
| Empty build | `owes_nothing` = `taken.rows == 0 && empty_build_answers_nothing(type)` (renamed): Inner, Left, LeftSemi, LeftAnti, LeftMark, RightSemi, and cross as Inner. The driver drops the probe batches with no call and **no finish**. Right and Full answer P padded; RightAnti answers P | the same shared function (plan: "as the device's does"); DataFusion HashJoin has no empty-build short-circuit | – | none |
| No probe batch | finish answers from the build: Left and Full padded, LeftAnti `B`, LeftSemi zero rows, LeftMark `B` with `false` | `ExhaustedProbeSide` → the same rows | build order on both | none |
| Zero-row probe batch | one zero-row table (`None` for the build-side semi family) | one zero-row chunk (`BatchSplitter` slices 0 rows, `last=true`) → one zero-row batch | – | none |
| Accumulated keys / finish pass | gone | gone (`Calls`, key project, finish join, pad project) | – | – |

Schemas:
- **Cpu.** `concat_batches(&declared_output, …)`, and finish goes through `declared_as`, so
  names and types are the declaration's.
- **Device names.** The build handle's names, then the probe handle's, or `names_of(side
  schema)` when a side never arrived; the mark column is `"mark"`.
- **Device pad types.** Taken from the first probe batch's actual types, or from
  `probe_schema` when no probe batch arrived.

These equal the declaration unless an upstream handle already diverges from its own (#65,
#225). When one does, the validator flags that upstream node first.

## 2. Which comparisons are order- or batch-sensitive

| comparison | granularity | row order | batch boundaries / slots |
|---|---|---|---|
| Operator harness `same` (`join_cases`, `join_dimension_cases`, `nested_cases`) | one slot per call, a slot's batches concatenated (`one_table`) | **normalized**: all 47 + 26 + 14 (+3) join `Order::` uses are `Any` | **not** normalized: slot count; "no batch" is not "a zero-row batch" |
| Session gtests (`test_join_session.cpp`) | per call | normalized (`Rows sorted`) | per call |
| Plan-executor gtests moved onto the session | whole result | `JoinProjectSort` sorts on `n_name`, which is unique | – |
| Cpu corpus vs DataFusion at tp1 | answer | multiset digest | free |
| `mini.result.txt`: `golden_exact`, `_approx`, `_approx_std` | answer | `batches_to_sorted_str`; approx keyed on non-numeric cells | free |
| `live_cpu` | answer, device vs cpu at the same mode | multiset digest, **exact** | free; cannot judge an undetermined row set |
| DuckDB (duckdb-oracle): `duckdb-result.txt` vs `mini.result.txt` and the device's `gpu-result.txt` per (query, mode) | answer | multiset; the fingerprint hashes sorted rows | free; `gpu-result.txt` never compared run to run |
| **Device execution section**: the device asserts the cpu-authored `.cpu.txt` section, `assert_section`, exact text | per node: `output_rows`, `output_bytes`, `in_rows`[child][lane], `batch_rows` and `batch_bytes`[lane][batch], `abandoned`, `early_exit` | **insensitive** to order inside a batch: bytes = declared width × rows + Σ string content, `common.rs` (one formula for both engines) | **fully sensitive**: deliberately not normalized (#220: "Not the comparator") |
| A gpu execution golden? | **none**. `.cpu.txt` and `.cost.txt` are cpu-authored; the device only reads them; `gpu-result.txt` is answers only | – | – |
| Schema validator (device corpus hook) | every batch every node queues, zero-row ones included | – | names, `type_id` and scale; never nullability or precision |
| Cost goldens and the cost-report gate | Σ`output_bytes` per (query, mode) vs the base SHA, **exact, no tolerance** | cpu only | per-batch overheads count: (rows+7)/8 bitmap, (rows+1)×4 offsets |
| `test_corpus_goldens`, `test_node_timing`, the benchmark record | structural | – | deterministic call sets |

## 3. Candidate divergences

| # | divergence | likelihood | where it shows | effect |
|---|---|---|---|---|
| C1 | **Cost gate regressions** (cpu Σout vs base): #137's `GpuFilter(IS NOT NULL)` nodes (~73 tpcds plans × 3 tp4 modes) add a node's worth of bytes; `__rowmarker__` adds an Int8 column on 19 tpcds nodes per mode; the cross arm's new probe merge may add a `GpuMergePartitions` line. Offsets (improvements): #220's fewer per-batch overheads, the probe-side coalesce removed, zero-row scatter outputs dropped | **high, near certain** | `cost-report` job, every section where the additions win | **fails** deterministically; under chain J's header (`done` = CI green but gpu-tests) the task cannot reach `done`. Not in the spec, the plan or the header (chain K's header has exactly this acceptance) |
| C2 | An **order-dependent selection over a device hash or nested-loop join**: an unordered LIMIT, or a fetch cut that ties between rows whose string columns differ | low today, **high if one is added** | the device section's `batch_bytes` at and above the cut; the result under `live_cpu` (exact); `gpu-result.txt` vs DuckDB | would **flap**: cuco `retrieve` and the conditional-join kernels write through an atomic cursor. Today: the only interval over a join with no sort between is `tpch/nested-limits`, over a **cross** join (awk scan of both datasets' plans). Per-batch `GpuSort` fetch cuts exist in tpch q2 q3 q10 q21 and tpcds q1 q5 q7 q14 q15 q18 q19 q21 q22 q26 q30 q33 q35 q40 q46 q52 q55 q56 q59 q60 q62 q68 q69 q75 q76 q78 q79 q80 q81 q93 (`sort_cuts.py` over the `.cpu.txt` goldens). Their keys are unique (group keys, unique names), or tied rows differ only in fixed-width columns (q33, q59, q75, rollup grouping-set ties), so bytes match; DuckDB needed no `duckdb_columns`, so no tie straddles a final cut. tpch q10 and q3 tie only on equal decimal revenue sums. **pass** |
| C3 | `nested-limits`' answer depends on cross-join **emission order**: rows 4–23 of a 200-row product | low (device cells off on #186, chain K) | result and section once enabled | **pass** while both stay left-major (`cudf::cross_join` and `CrossJoinStream` both are); a deterministic **fail** if either side reorders (§3.6's "order is free" chunk loop, a future chunked cross) |
| C4 | The task-6 **adapter** concatenates the cpu's per-call chunks (the spec's interim #220 fix), so `.cpu.txt` and `.cost.txt` move for the multi-chunk join rows (#220's 82). Task 6 verifies `--lib` only; the regeneration is scheduled in task 8 | medium | `test_cpu_corpus` at the task-6 commit | that commit **fails**; head green after task 8; breaks "every commit green" |
| C5 | Enabling ~428 device cells without regenerating `gpu-result.txt` (device-written) | medium-low | duckdb-oracle's rust-only coverage guard, both ways | deterministic **fail** in CI; task 14 names the DuckDB cases, not the regeneration |
| C6 | The harness calls `finish_and_fetch` after `owes_nothing` (task 6 step 5); the driver never does | low | none | **pass** (both engines agree); the production sequence (drain, no finish) is pinned only on the mock |
| C7 | Lane-dependent names and pad types (handle-derived vs side-schema-derived) | low | validator, harness | **pass** unless an upstream already diverges (#65, #225 rows stay off) |
| C8 | Float sums reordered by the device's join order | low | `golden_exact` | tpcds q39 is `golden_approx_std`; q17 answers 0 rows; avg over integers sums integer-valued doubles exactly; pbench decimals are exact. **pass** |
| C9 | The partial-aggregate `in_rows` above a join (the q93/q38/q96 mechanism) | resolved | merge `in_rows` | **pass**: one batch per call on both; groups per batch do not depend on order |
| C10 | `owes_nothing` computed differently per engine | low | slot counts, section batch lists | **pass** if the cpu calls the same `plan/join.rs` function (the device's code in task 7 does; task 8's text says only "as the device's does") |
| C11 | #205: the cpu's sort over zero-row batches emits nothing; the device emits one zero-row batch | low | section | only q17 (an empty answer) has the shape, and it stays off (#205, #225); chain K fixes it |

## 4. What the plan already does, and what to add

Already there:
- `Option` returns, so one batch per call is in the type on both engines.
- `None` per probe for the build-side semi family on both (Review Focus 2).
- Three probe batches with a zero-row one between, for every type.
- No-build and no-probe cases per type.
- `Order::Any` in every join harness case.
- Sorted gtests and sorted result compares.
- The cpu goldens regenerated with the stream (task 8).
- A DuckDB case per enabled cell.

Add:
1. **The cost gate (C1).** Run `cost-report --cost-diff` locally on the final head and list the
   expected regressions (section, mode, bytes) in `join-backend-detail.md`. Get the human's
   acceptance into chain J's header, as chain K's header does for limits and empty-sorts.
   Without it the task stalls at `completeness approved`.
2. **Task 6 (C4).** Run `test_cpu_corpus` and regenerate `.cpu.txt` and `.cost.txt` in the
   adapter's commit, or move the concatenation's golden churn there explicitly.
3. **Task 14 (C5).** Name the `gpu-result.txt` regeneration per batch: duckdb-oracle's
   regeneration variable and `--pull-results`.
4. **A guard for C2**, since nothing stops it arriving later. Either:
   - a rust-only check that no enabled device cell sits on a `data_fusion_subset` line, or on
     a plan with a row interval over a hash or nested-loop join with no sort between; or
   - a device subset oracle (count plus multiset containment against the cpu run, as
     `assert_subset_of_unlimited` does) in place of `live_cpu`'s exact compare for such
     lines.
5. **C3.** One harness case pinning cross-join emission order with `Order::AsEmitted` on both
   engines, which is what `nested-limits` rests on, or a comment on that query naming the
   dependency.
6. **C10.** Task 8's cpu `owes_nothing` calls `empty_build_owes_nothing` itself: one function,
   two backends.
7. **`architecture.md`, "Determinism rules"** (a correction, not growth). "what must hold is
   that one plan run twice gives one answer, byte for byte" is true of the cpu. On the device it
   is false for an order-dependent selection above a hash or nested-loop join, since the
   emission order is not reproducible. No ticket until a query shows it: nothing in today's
   corpus or pbench does.
8. Optional (C6): a harness case that drives the production sequence for an `owes_nothing`
   lane: no probe call, no finish.

No production ticket is warranted by any of the above: none is a wrong answer, a crash or a
refusal.
