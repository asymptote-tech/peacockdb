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
  (`peacockdb/src/main.rs`), with no signature change. This is what the spec authorized rather
  than something beyond it: its Scope says "Component-level API: `RunReport::batches`
  (`executor/mod.rs`, public) is never empty for a query whose root received nothing; no
  signature changes."

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

`as #205 says` in #214 (`corpus-coverage.md`) and in the cross-join ticket (`joins.md`) were
left for the coordinator, who repointed both at the archive in the past tense rather than relying
on the bare number resolving there. No `.rs`, `.inc`, `.csv`, `.cpp` or `.py` file in the tree
names #205 any more.

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

### 2026-10-09 — round 1 green, PR #174 open

Committed as `388c8940`, pushed, PR #174 against `ENS-limits` — base verified, two commits.

The wiki half was an ownership conflict the developer flagged rather than resolved on its own
authority, which was right: the spec's Scope row and the impl plan's Task 5 put `architecture.md`
and `tickets/` on the developer, and the coordinator's standing rule is that markdown is the
coordinator's. The developer wrote and verified all of it, reverted it, and left the diff at
`/tmp/empty-sorts/wiki-coordinator.patch` with the text in this file. Applied here with three
additions it did not cover, each a bare `#205` that stopped being true rather than a dangling
link:

- `build-test.md`'s device-corpus line listed q17's cell under "the device's own tickets
  (#57, #63, #205)". That cell is held by #281 now.
- #199's body named "#205's sort" among the sources of nothing below a keyless init. The
  sentence's claim still holds; the ticket it names is closed, so it says so.
- #214's body said "as #205 says", and the cross-join ticket in `joins.md` said it in the same
  words. Both repointed at the archive, past tense, so the two parallel sentences keep one form.

The board's link moved to the archive; the spec's did not, since the spec is frozen.

### 2026-10-09 — review round 1: 0 blocking, 1 important, 5 nits

The important finding was mine, and it was a spec deliverable, not a staleness nit: **#235's
empty-answer bullet, wiki item 7, never got written.** The detail file recorded the resolution and
the strike list and said the coordinator owns the edit; items 1 to 6 landed and 7 did not. All
three of the bullet's claims had become false — q17 renders its header, the cpu emits a batch, and
the "candidate ticket to render the declared schema" is this task. Now rewritten in the past tense
with the four-item list the spec's work item 3 asks for, because #235 is where the duckdb-oracle
merger looks and the spec is archived with the task.

Three nits were markdown and went with it. The detail file misquoted the frozen spec as saying
"Component-level API: none" where it in fact authorizes exactly the `RunReport::batches`
guarantee this branch makes — a misquote that would have read, in the signoff, as the branch
exceeding its declared API scope. The cross-join ticket's "as #205 says" is the twin of #214's
and now takes the same form. And #205's closing note in the archive takes the chain's
`Closed by <task> (chain K)` form instead of being the file's only blockquote, so a reader does
not have to go to the board to learn which task closed it.

Two nits are code and are left, with the reasons, since nothing blocking or important is
outstanding and the chain is at its end:

- `test_support/corpus.rs`'s `count_of` now routes through `oracle_answer`, which makes its
  `expect("a count returns a row")` unreachable: a count that answered nothing would panic two
  lines on with an arrow index message instead of that sentence. `count(*)` always returns a row,
  so this is a worse message for a case that cannot arise — the one call site the rename reached
  that had no use for the new behaviour.
- `an_empty_sort_and_merge_each_emit_one_zero_row_batch` runs its query twice at all five modes,
  once through `sql_answers_match_datafusion` and once to read `report.emitted`, because the
  harness hands no report back. Ten runs of a `customer` scan where five would do.

#### Checked and found right, so nobody re-checks it

- **Rule 1's new branch is exactly equivalent to the two early returns it replaced.**
  `held.iter().all(…)` is vacuously true over an empty `held`, and `coalesce_or_nothing`'s
  `held.is_empty()` guard returns the same value and the same `CallStats` as both deleted returns.
  `run_node` is now called on a strict subset of the old inputs, so no new sort runs.
- **The new `concat_batches` over arrived batches is not a new strictness hazard**, though it is
  the thing to worry about: arrow requires an exact schema match, and the all-zero-rows path never
  reached it before. But every producer relabels to its declared schema before emitting
  (`declared_as`, and `check_batch_schema` on the scan, which exists for exactly this), and
  `Coalesce::mark_done_and_fetch` already concatenates raw arrivals under the declared schema on
  every corpus query. Same assumption, already load-bearing.
- **Rule 2's schema source is right in every shape, at field level and not just at names.**
  `plan/validate.rs` refuses any root that is not a sink and `check_output_schema` reads the
  schema from the same `root.children()[0].kind().schema()`, holding its names and types against
  DataFusion's planned schema; a row-bearing answer's schema is the same `kind().schema().fields`
  forced by `declared_as`. So "an answer's schema never depends on how its rows ran out" holds.
- **Rule 2 cannot fire when it should not.** The error and budget-trip paths return `Err` and
  never reach `report()`. `LIMIT 0` and an offset past the end are genuinely zero-row answers and
  now carry their header. A dropped-answer bug is still caught: the schema half of the digest
  matches, the row count does not.
- **No caller can reach `answer()` with a non-sink root** — every driver-test root is an
  `unload`, every other `run::<B>` caller feeds a planner-produced tree, and
  `single_partition.rs` produces no `RunReport`, so the invariant has one owner.
- **The mixed merge is right on a device**: `gpu_backend/accumulate.rs` guards on `held.is_empty()`
  over the flattened lanes, so one zero-row batch plus one silent lane makes the call and emits
  one batch, which is what the cpu now does. `merged(2, None)` sidesteps #204 and zero rows make
  #217 moot.
- **The new guards go red when reverted**, including the trap the oracle change exists to close:
  without it both sides would be empty and `columns_of` would pass vacuously.
- **The goldens moved exactly where the rows did**, and q17's three tp4 sections are correctly
  untouched — there the merge's lanes receive nothing, which is rule 2's case, and rule 2's batch
  is never recorded. The result section is authored at tp4-sized, the last declared mode, which is
  why its header comes from rule 2 rather than rule 1.
- **Exactly two cost regressions**, verified by extracting every `peacockdb_cost=` line from every
  `.cost.txt` at both refs: 565 sections each side, 2 changed, both +24 on q17. Nothing moved
  downward.
- **The documented Python-model divergence is accurate**, and rule 1 moved the engine *toward* the
  model rather than away: `operators/accumulators.py` already emits one empty batch
  unconditionally.

### 2026-10-09 — completeness pass: 0 blocking, 4 important, all of them prose

The reviewer found **nothing** blocking or important — the first clean code reading in this chain
— and the analyst found four, every one a sentence the branch falsified or left unwritten. Applied
in the commit that carries this section.

**Analyst's four.**

1. The branch narrowed *Zero-row batches change no answer*'s last known-break bullet from plural
   to singular when it struck #205, which made it assert the limit is the only producer that drops
   a zero-row batch. [#208](../tickets/joins.md#t208) is the same shape and still open — the cpu's
   cross join over a zero-row build side — and with #205 gone it is the **last surviving
   asymmetric** one, #214 being symmetric and saying so. Restored to the plural, naming #208 and
   which of the two is asymmetric.
2. Rule 2 is an engine-wide invariant and the only place `architecture.md` recorded it was a
   parenthetical inside the Python-model sentence of *The scheduling rule*. The section a reader
   actually goes to says nothing, because its requirement is about a *node's* output rows and the
   driver's answer is no node's output — so that reader would still conclude q17 at tp4-sized
   answers no batch. One sentence added after the requirement paragraph.
3. #281 undercounted what this chain built and never ran: the branch adds a **fourth** accumulate
   case, `one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both`, which is not a
   converted pin but the first assertion anywhere about the device's `held.is_empty()` arm over a
   mixed lane script. That fact lived only in this file, which the helper deletes at archive time,
   and the frozen spec names only the three pins. #281 now records it.
4. `RunReport::batches`' doc carried the never-empty half of its guarantee and not the half the
   branch's own driver tests had to assert: that the batch is in no other field, so `batches.len()`
   no longer relates to the unload's calls or to `Σ emitted[ROOT]`.

The analyst also noted that the Python model differs from the engine in a second place at the
empty case — its accumulators emit one empty batch even over no arrival, where the engine keeps a
lane that received nothing answering nothing, which is the half of rule 1 the branch deliberately
kept. Folded into the same sentence.

#### One latent defect, deliberately not filed

The reviewer found it and reached the same conclusion, which is why it is here rather than in
`tickets/`. `seed()` iterates `refresh(node)` then `settle_limit(node)` in pre-order, so a parent
is refreshed before its children are satisfied and nothing recomputes its readiness afterwards.
For `unload(coalesce_all(limit(…, skip, Some(0))))` the limit is satisfied at seed, every ancestor
keeps the stale `false` it was given, `scheduler.next()` returns `None` on the first step, and the
accumulator above never receives `mark_done` — so it never emits the batch its `SingleBatch`
output owes. Under a global aggregate that would be a wrong answer, and rule 2 would dress it as a
well-formed empty answer with a header.

**Not reachable from SQL**, which is why it is not a ticket: DataFusion 45's `EliminateLimit`
folds `fetch == 0` to an empty relation, and `satisfied_by`/`satisfied_by_emitted` can only be
true at seed when `fetch == 0`, so no plan reaches a mid-plan `GpuLimit` with `fetch: Some(0)`.
Mid-run satisfaction is fine — `step` does `settle_limit`, then `refresh(node)`, then
`refresh(parent)`, which is the ordering `seed` lacks. Worth knowing because the existing pin
`a_mid_plan_limit_of_no_rows_is_satisfied_before_any_pull_whatever_its_skip` puts a *filter* above
the limit, which owes nothing, so it does not cover the accumulator shape: a future change that
lets a zero-fetch limit reach a plan would need that case first.

#### Verified independently by the reviewer, so nobody re-derives it

- q17's new result section is **byte-identical** to `duckdb-result.txt`'s q17 section once the
  `mode=` line is dropped, so #235's empty-answer divergence is closed in fact.
- 565 cost sections each side, identical section sets, exactly 2 changed (+24 each on q17),
  nothing moved downward. 172 golden files, `== ` section counts identical in every one.
- Rule 2's schema source matches the device's: `GpuExport::new` takes `input(0)`, the same
  `root.children()[0].kind().schema().fields` that `answer()` reads.
- Rule 1's granularity matches the device's: `gpu_backend/accumulate.rs` flattens `per_lane` and
  branches on `held.is_empty()` too, so one zero-row lane plus one silent lane makes the merge call
  on a device as it now does on the cpu. A `fetch` is moot there — `first_rows` over a zero-row
  batch is the identity — and the sort-order claim holds vacuously, `SortOrder` being a plan-level
  declaration that a zero-row batch satisfies.
