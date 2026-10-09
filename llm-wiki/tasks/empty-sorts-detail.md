# empty-sorts — working record

## Standing facts

- Fourth and last of chain K, branch `ENS-empty-sorts` off `ENS-limits`, PR will target that
  branch. Closes [#205](../archive/archived-tickets.md#t205).
- **Chain K runs without a GPU.** No device build, no device run, no GPU cycle. Every build and
  run is local — verda does not resolve from this host, checked again at this dispatch. The three
  `gpu_tests/accumulate_cases.rs` cases are built, not run; #281 holds them. The task reaches
  `done` when every CI job but the GPU tests is green.
- **The cost gate's rises are pre-accepted by the human** (2026-10-08, on the board): this task
  is `done` when the cost-report job's only regressions are the sections this file lists from a
  local `--cost-diff` run — expected q17 at tp1-single and tp1-rowgroup, +24 bytes. Any other
  regression is a finding. So the local `--cost-diff` output has to land in this file before CI
  is read, or there is nothing to compare the job against.
- The chain's base is master `31c56bea`. The three tasks below this one are `done`.

## The spec's DuckDB conditional resolves to "has not merged"

Checked on this branch, 2026-10-09. The two `duckdb-result.txt` goldens exist — they landed
2026-09-28 with #235 — but no `duckdb_oracle` module exists anywhere under
`peacockdb-core/src/`, and chain J's `duckdb-oracle` task is `blocked(completeness approved)`
with PR #167 unable to get a CI run. So:

- nothing in this task touches DuckDB;
- the list of things the helper must strike when `duckdb-oracle` is merged **after** this task
  goes into [#235](../tickets/corpus-coverage.md#t235)'s empty-answer bullet, since #205 is
  archived here and the references to it over there would outlive it. That list is q17's
  `duckdb_divergent(205)`, the tests asserting #205 open (`duckdb_oracle/tests.rs:157-158`,
  `:270-278`, `:322-323`) and the empty-answer branch in `duckdb_oracle.rs:113-115`.

The #235 edit is markdown, so the coordinator writes it rather than the developer.

## For the human

- **The cost gate is red on this PR, by design, and these are the only rows.** Local
  `--cost-diff` against the PR's base `4581bca3` (`ENS-limits`), run on the round-1 tree:
  `565 compared, 2 changed, 2 regression(s)`, `rc=1`. The complete regression list, with the
  `peacockdb_cost=` figures read off `git diff 4581bca3 -- testdata/goldens/tpcds.sf1/*.cost.txt`:

  | section | base | branch | Δ |
  |---|--:|--:|--:|
  | `tpcds.sf1/q17 tp1-single-mini` | 293556446 | 293556470 | +24 |
  | `tpcds.sf1/q17 tp1-rowgroup-mini` | 293557678 | 293557702 | +24 |

  No other section moved, in either direction. Both rises are the predicted ones: the
  accumulating sort and the unload each emit the 12-byte zero-row batch the device emits, so
  `cuda_sort_bytes` goes 12 → 24 and `vram_to_ram_bytes` 0 → 12 in each of the two sections.
  CI fails `cost-report` on any rise (`cost-report/src/main.rs:1210`, no tolerance), so this
  task reaches `done` only once the human accepts these two rows.
- **The driver's synthesized batch is not an emitted batch, and the gate proves it.** The rule-2
  batch is made in `Driver::answer` and never recorded, so no zero-row query's `output_rows` or
  `output_bytes` moved: the two rows above are rule 1's sort emission alone, and the three tp4
  q17 sections — where rule 2 supplies the whole answer — did not move at all.
- **`RunReport::batches` is never empty.** A documented guarantee on a `pub` field the CLI reads
  (`peacockdb/src/main.rs`), with no signature change; the spec's Scope says
  "Component-level API: none" and does not mention it.

## Dispatch log

### 2026-10-09 — round 1, developer dispatched

Branch cut from `ENS-limits` at `4581bca3`, board moved `approved to build` → `building` in the
same commit.

#### Round 1 evidence

**Green, per target, on the final tree** (rust-only, local, `-- --test-threads=2`; no device run):

| target | result |
|---|---|
| `--lib` | 645 passed, 0 failed, 2 ignored (#182); 647 collected |
| `test_cpu_corpus` | 569 passed, 0 failed — includes `the_registry_matches_the_cpu_corpus_in_both_directions` |
| `test_corpus_goldens` | 26 passed |
| `test_cost_model` | 3 passed |
| `test_golden_format` | 26 passed |
| `test_module_layout` | 17 passed |
| `test_ci_coverage` | 9 passed |

The two ignored `--lib` cases are the pre-existing #182 pair (the budget boundary and the
rebatcher's peak), not this task's.

**The scan of Task 1 Step 1** printed exactly the seven lines the plan predicted — q17's
`GpuUnload` at all five modes and its `GpuAccumulateBatchesAndSort` at the two tp1 modes. No
extra section, so no golden beyond the ones below moves.

**Where each rule lives, and how the two stay distinct.**

- Rule 1 is `sorted_and_cut` (`cpu_backend/accumulate.rs`), called by
  `SortedRuns::mark_done_and_fetch` and by the tail of
  `CpuPartitionAccumulator::accumulate_and_fetch`. Its test is `held.iter().all(num_rows == 0)`,
  and on that branch it hands the *held* batches to `coalesce_or_nothing` rather than
  DataFusion's sorted output. Both callers' own `is_empty` early returns are gone, and that is
  the point: `all` is vacuously true over no batch, so a lane that received nothing falls through
  the same branch into `coalesce_or_nothing`, whose `held.is_empty()` guard is now the one place
  that decides "received nothing → answer nothing". One rule, one owner; the two cases are told
  apart by `held.is_empty()` inside `coalesce_or_nothing`, never by the sort's output, which
  cannot tell them apart at all once DataFusion has eaten the batch.
- Rule 2 is `Driver::answer` (`driver/partitioned.rs`), read by `Driver::report` and nowhere
  else. It is the *answer* and not an emission: it runs after the step loop, takes the sink's
  input's declared schema off the index, and touches neither `results`' recording path,
  `emitted`, the trace, `abi_calls` nor the accountant. The pins: `emitted[ROOT]` empty and
  `count(Unload) == 0` in
  `a_sink_that_received_nothing_answers_one_zero_row_batch_under_its_inputs_columns`, and the
  cost gate below, where the three tp4 q17 sections — the ones rule 2 answers in full — did not
  move a byte.
- `an_empty_sort_and_merge_each_emit_one_zero_row_batch` reads the sort's and the merge's own
  `emitted` entries off a real run, so neither half of `sorted_and_cut` can be reverted behind
  rule 2's answer.

**Goldens that moved, with the deletion audit by `git diff --numstat`** (against
`4581bca3`; five golden files and the registry, nothing else):

| file | +/− | what moved | sections |
|---|---|---|--:|
| `tpcds.sf1/mini.result.txt` | 4/2 | q17's `++`/`++` → the bordered 15-column header, under `mode=tp4-sized` | 82 → 82 |
| `tpcds.sf1/tp1-single-mini.cpu.txt` | 4/4 | q17's unload and sort: `batch_rows=[[]] batch_bytes=[[]]` → `[[0]]`/`[[12]]`, `output_bytes` 0 → 12 | 82 → 82 |
| `tpcds.sf1/tp1-rowgroup-mini.cpu.txt` | 4/4 | the same | 82 → 82 |
| `tpcds.sf1/tp1-single-mini.cost.txt` | 3/3 | `cuda_sort_bytes` 12 → 24, `vram_to_ram_bytes` 0 → 12, `peacockdb_cost` +24 | 82 → 82 |
| `tpcds.sf1/tp1-rowgroup-mini.cost.txt` | 3/3 | the same | 82 → 82 |
| `testdata/cost-registry.csv` | 1/1 | q17's tpcds row, ticket `205` → `281`; 141 rows → 141 | — |

No file lost a section (#213's failure mode) and no line count fell: `mini.result.txt` 8462 →
8464, the other four byte-for-byte in length. No tp4 `.cpu.txt`, no `.cost.txt` outside tp1 and
no plan golden moved.

**The device-only cases: what is proved and what is not.**

Built, twice, on the final tree, against cuDF 25.02 on this host, never run:

    CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh \
        test -p peacockdb-core --lib --features gpu --no-run        # succeeded
    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run

Proved, with no device touched at all — the binary was read, not executed:

- the four cases compile and link: `nm -C` finds
  `peacockdb_core::tests::gpu_tests::accumulate_cases::{one_zero_row_batch_sorts_to_zero_rows_on_both,
  a_fetch_over_zero_rows_is_zero_rows_on_both, every_lane_a_zero_row_batch_merges_to_zero_rows_on_both,
  one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both}`;
- they are *collected* by the harness: the four libtest descriptor names
  `tests::gpu_tests::accumulate_cases::<name>` are in the binary's rodata, and so are the four
  `operator_case!` inventory `case:` strings the coverage guard reads;
- no retired name survives: `grep -c 'bug_.*_on_the_cpu'` on the source is `0`, and
  `strings | grep -c sorts_to_nothing_on_the_cpu` on the binary is `0`;
- `test_gpu_corpus` still compiles over `report.batches`, whose type did not change.

Not proved, and #281 is what holds it: that the device actually answers one zero-row batch in
all four shapes. In particular
`one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both` is a new assertion about
`gpu_backend/accumulate.rs`'s `held.is_empty()` arm that no run has ever checked. A `--list` was
deliberately not run either, so no `--features gpu` binary was executed in this round.

**Prepared but not applied: the coordinator's wiki edits.** The dispatch reserves `llm-wiki/` to
the coordinator bar this file, `empty-sorts-impl.md` and `build-test.md`'s counts, which
overrides the plan's Task 5 Steps 1, 3, 4 and 5 and the spec's Scope row for
`architecture.md`/`tickets/`. Those four steps were written, verified and then reverted; the exact
diff is `/tmp/empty-sorts/wiki-coordinator.patch` (168 lines), and it is reproducible from the
plan. What it contained, all of it still owing:

1. `architecture.md`, "Zero-row batches change no answer": the last bullet drops the sort and
   merge, leaving `- A producer that drops a zero-row batch exposes the breaks above: the limit
   ([#214](tickets/corpus-coverage.md#t214)).`
2. `architecture.md`, the schedule section's Python-model sentence gains: `Its driver answers
   nothing where the sink received nothing (`partitioned_driver.py`, `results`); the engine's
   answers one zero-row batch under the sink's input columns (`Driver::answer`).` — checked
   against `scripts/exec_model/partitioned_driver.py`, which returns `self.results` unchanged.
3. `corpus-coverage.md`, #281's **Corpus queries:** → "`tpch/scan-limit`, `tpch/nested-limits`
   and `tpcds/q17` at every device mode, each off on this ticket once chain K has merged."
4. #205 archived: its block, anchor included, moved from `corpus-coverage.md` (and dropped from
   that file's contents list) to the top of `archive/archived-tickets.md`'s `## Done` with the
   `> **Done.**` note the plan dictates; `tickets.md`'s corpus-coverage row 33 → 32 with #205
   struck from its id list, and the open total 117 → 116. Both verified by summing the rows.
5. `corpus-coverage.md`, #199's fix: "(#214's limit, #205's sort, the joins)" → "(#214's limit,
   the joins)".
6. `build-test.md`'s device-corpus row: "the device's own tickets (#57, #63, #205)" → "(#57,
   #63, #281)". Left as `#205` because it is a ticket reference rather than a count.
7. #235's empty-answer bullet, which the dispatch already assigns to the coordinator.

`as #205 says` in #214 (`corpus-coverage.md`) and in the cross-join ticket (`joins.md`) stay as
they are — the number resolves in the archive. No `.rs`, `.inc`, `.csv`, `.cpp` or `.py` file in
the tree names #205 any more.

**build-test.md's counts, every figure summed from the rows rather than from a delta.**
Measured with `--list`: `--lib` 647, `test_cpu_corpus` 569, `test_corpus_goldens` 26,
`test_cost_model` 3. Per-row, measured the same way: `executor::driver::tests::` 116,
`executor::cpu_backend::tests::` 77 less the 1 contract case = 76, `tests::end_to_end::` 39, and
a parse of the `gpu_tests` sources gives the operator harness 336 and the schema row 134
unchanged.

| row / header | was | now |
|---|--:|--:|
| End to end | 37 | 39 |
| Drivers over a mock backend | 113 | 116 |
| CPU backend executors | 68 | 76 |
| cpu block header / `--lib` | 1232 / 634 | 1245 / 647 |
| Operator harness (gpu) | 335 | 336 |
| gpu block header / `--lib -- gpu_tests::` | 576 / 536 | 577 / 537 |
| Rust | 1903 | 1917 |
| Grand total | 2378 | 2392 |

Re-summed after the edit: cpu rows 1245 = header, ffi 7 = header, gpu rows 577 = header,
everything-else Rust 88, so Rust = 1245 + 7 + 577 + 88 = 1917, and 1917 + 94 + 381 = 2392. The
three row descriptions whose counts moved were updated with them; `#[ignore]`'s "two of the 37 …
so 35 run" became "two of the 39 … so 37 run".

**Outside this task's scope, found and deliberately not fixed.**

- `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs:9` has an unused import,
  `AsArray`, which is the one warning the `--features gpu` build emits. Pre-existing: the file is
  untouched here and was last written by `device-schema-harness` (0e7804ef), not by chain K. Only
  a gpu-feature build sees it, which is why a green rust-only run never reported it. Cosmetic, so
  no ticket under the "only production behaviour gets a ticket" rule — a one-line deletion for
  whoever next opens that file.
