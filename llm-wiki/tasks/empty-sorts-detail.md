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

### 2026-10-09 — done

CI run [37911463584](https://github.com/asymptote-tech/peacockdb/actions/runs/37911463584) on
`dfae29b7`. Green: changes, both dataset-matrix legs, cpp-build-2502, s3-datasets; deploy-pages
skipped as a master-push job.

Two jobs red, both the cases the board says not to wait on.

**cost-report** — the gate, reporting exactly what the human pre-accepted and nothing more: *2
regressions*, `tpcds.sf1/q17` at tp1-rowgroup-mini and tp1-single-mini, the +24 bytes
`## For the human` predicted from the local run. No third regression and nothing moved downward,
which is also the check on rule 2: q17's three tp4 sections, where the driver's answer *is* the
whole answer, did not move a byte, so the synthesized batch never reached `emitted`.

**GPU Tests** — `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`, at
the rsync step, for the fourth time today. The host is unreachable, so no pool was built: not
[#178](../tickets/testinfra.md#t178) and not this branch.

Everything the chain can test is green, so the task is `done` and the human merges. #281 holds
the device half, including the one case here that is a new assertion rather than a converted pin.

### 2026-10-09 — rebased onto ENS-limits, and the GPU half dispatched

The chain's base moved: master `31c56bea` → `bc9b6e2f`, and the three tasks below this one took
their GPU halves. `git rebase --onto ENS-limits pre-rebase2-K-limits ENS-empty-sorts`, recovery
tag `pre-rebase2-K-empty-sorts` at the old head `da47434f`. The task's own diff is unchanged by
the move: 27 files, 938 insertions, 197 deletions against either base, byte for byte the same
`--stat`. **The standing facts at the top of this file are stale from here on** — the chain now
has a GPU host, the base is no longer `31c56bea`, and #205's half of #281 is the only half left.

Five conflicts, all bookkeeping, resolved by ownership:

- `tasks.md`: the replayed side, the branch's own state progression.
- `tickets.md`'s contents: the new base's list minus `#205`, so `#285` (filed by limits) survives.
  33 corpus-coverage tickets, 117 open, both re-summed from the rows rather than deltaed — and
  every row's count now checked against the length of its own id list.
- `corpus-coverage.md`'s #281, twice: the new base's narrowed text kept both times, since limits'
  half is closed and the replayed side still described both tasks. The completeness pass's
  fourth-case sentence folded into it.
- `build-test.md`'s three count headers: the replayed commit's own delta applied to the new base's
  absolutes, then every figure re-summed from the rows. cpu 1254 = `--lib` 653 + 569 + 29 + 3;
  gpu 588 = 538 + 38 + 11 + 1; ffi 7; everything-else Rust 90; Rust 1939; grand total 2414.
- `build-test.md`'s device-corpus prose: the new base's paragraph with `#205` → `#281`.

**One row needed hands that no conflict marker showed.** Both branches edited *Operator harness*
from 335 to 336 — limits' new `bug_` case and this branch's new mixed-merge case — so git saw one
identical change and collapsed the two `+1`s into one. Set to **337** by that reasoning, not by
measurement: the coordinator cannot build. It is the one figure on the page to re-measure first.

`cost-registry.csv` auto-merged to exactly one changed row, tpcds q17's ticket `205` → `281`,
which is this task's own query, so row ownership held.

#### The dispatch

verda does not resolve from this host, checked at this dispatch, so every CPU run is local.
nebius-gpu answers: card idle (0 MiB of 46068), `df -h /` 21 GB free, `~/peacockdb-K` present
with its `testdata/`.

#### The brief: two jobs, re-prove then measure

**Job 1 — re-prove the cpu half on the new base.** The rebase carried code, so nothing on this
branch is proven until it runs again. The spec's rust-only bar, locally, `-- --test-threads=2`:
`--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`, plus `test_golden_format`,
`test_module_layout` and `test_ci_coverage`. Red here is the rebase having broken something and
is the first thing to fix. The cells limits enabled (`tpch/scan-limit` at five modes) and
distinct-companions' (`tpch/distinct-functions`) are new neighbours in the corpus binaries.

**Job 2 — the GPU half, which is what #281 holds.** Everything below has run on no device:

- the four `GpuAccumulateBatchesAndSort`/`GpuMergeSortedPartitions` cases in
  `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` —
  `one_zero_row_batch_sorts_to_zero_rows_on_both`, `a_fetch_over_zero_rows_is_zero_rows_on_both`,
  `every_lane_a_zero_row_batch_merges_to_zero_rows_on_both`, and
  `one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both`. The first three are
  converted `bug_` pins; the fourth is a **new assertion no run has ever checked**, about
  `gpu_backend/accumulate.rs`'s `held.is_empty()` arm over one zero-row lane and one silent lane.
- **`tpcds/q17`'s device cell at `tp1-single`**, off today: `corpus_cases.inc:287` declares its
  gpu modes `none`, and `cost-registry.csv` row `tpcds,1,q17` carries ticket `281`.

Run the whole device tier, not only these — a cell enabled by limits or distinct-companions going
red on this base is a finding too. Then: **a cell that passes is enabled; a cell that fails stays
off naming a ticket, and you report what the ticket must say rather than writing it** (ticket
markdown is the coordinator's). `corpus_cases.inc`, `cost-registry.csv` and the comment above
line 285 are yours.

The prediction is agreement, and the reason it is worth measuring is that agreement is a
prediction: the cpu's `sorted_and_cut` now hands held batches to `coalesce_or_nothing`, and
`gpu_backend/accumulate.rs` flattens `per_lane` and branches on `held.is_empty()` the same way.
If the device disagrees, the finding is which of the two is wrong — say which, with the batch
the device actually emitted.

**Job 3 — `build-test.md`'s counts are yours on this task.** Measure every figure with `--list`
on the final tree, both shapes, and re-sum the headers from the rows. Start with *Operator
harness*, set to 337 by reasoning above; the gpu block header 588 and `--lib -- gpu_tests::` 538
rest on it. `test_gpu_corpus` 38 moves to 39 if q17's cell turns on.

#### The host, and the five traps this chain has already paid for

nebius-gpu, `dmitry@89.169.109.150`, cuDF 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2`,
working dir `~/peacockdb-K` — never `~/peacockdb-J`, and never delete anything under it. The
board's chain-K note and chain J's host override above it are the full rules; the recipe limits
used, which worked:

    rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ dmitry@89.169.109.150:peacockdb-K/
    # on the host, in ~/peacockdb-K, after  . ~/peacock-env.sh
    find . -path ./target -prune -o \( -name '*.rs' -o -name '*.inc' \) -print | xargs touch
    ./scripts/build-test-shadgpu.sh --build          # detached; never --run, which ssh-es to shad-gpu
    export LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib
    export PEACOCK_TESTDATA_DIR=$PWD/testdata
    cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests:: -- --test-threads=1
    cpp/install/rust-tests/test_gpu_corpus -- --test-threads=1
    cpp/install/rust-tests/test_node_timing -- --test-threads=1
    cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_ -- --test-threads=1
    cpp/install/bin/peacock_gpu_tests ; cpp/install/bin/peacock_plan_tests

1. **`rsync -a` can restore a host-side source with an older mtime**, and cargo then skips the
   rebuild and the run reports the previous build's behaviour. `touch` the sources; then confirm
   the staged binary is *this* tree by a literal a test reads — `--list` naming the four
   accumulate cases, and `strings | grep -c sorts_to_nothing_on_the_cpu` returning 0. An exit
   code is not that proof.
2. **A binary whose tests all filter out runs zero tests and passes.** Report ran/passed/filtered
   per binary, as limits' section does, so a zero-test pass cannot read as green.
3. **`df -h /` before every build**: 21 GB free, and `conda clean -a` finds nothing to reclaim, so
   there is no slack. Below 20 GB follow chain J's cleanup rule; below 10 GB do not build and
   record it here as an obstacle.
4. **The card is shared with chain J.** A memory failure with chain J's run on the card
   (`nvidia-smi` shows it) is waiting and rerunning, not a ticket.
5. **Never a foreground command without `timeout`**, and never end your turn with a detached
   build still running — poll its log and stay in the turn.

The sf40 pair (`peacock_tpch_tests`, `peacock_tpchv_tests`), `--run-benchmarks` and Nsight are out
of every chain-K task. Record any skip here as deferred.

#### The cost gate

The human pre-accepted exactly two rows for this task: `tpcds.sf1/q17` at `tp1-single` and
`tp1-rowgroup`, +24 bytes each. Any third regression, or any figure moving downward, is a finding.
Re-run `--cost-diff` against the new base (`ENS-limits`) and put the output here — the earlier run
was against `4581bca3`, which no longer exists in the chain.

#### Ownership on this round

`llm-wiki/` is the coordinator's except this file, `empty-sorts-impl.md`, and `build-test.md`'s
counts. The spec is frozen; its signoff is already written and will be rewritten by the
coordinator when this round closes. Code, `.inc`, `.csv`, goldens and `.github/workflows/` are
yours. You never look at CI.

### 2026-10-09 — round 2: the cpu half re-proved, the GPU half run on nebius-gpu

Written as the round ran, not after it, so a restarted coordinator reads measurements rather
than intentions.

#### Job 1 — the cpu half re-proved on the new base (`ENS-limits` = `3e306482`)

Green, locally, `--features rust-only`, `-- --test-threads=2`. verda still does not resolve from
this host, so every CPU run is local, as the dispatch said.

| target | ran | passed | failed | ignored | filtered |
|---|--:|--:|--:|--:|--:|
| `--lib` | 651 | 651 | 0 | 2 (#182) | 0 |
| `test_cpu_corpus` | 569 | 569 | 0 | 0 | 0 |
| `test_corpus_goldens` | 29 | 29 | 0 | 0 | 0 |
| `test_cost_model` | 3 | 3 | 0 | 0 | 0 |
| `test_golden_format` | 26 | 26 | 0 | 0 | 0 |
| `test_module_layout` | 17 | 17 | 0 | 0 | 0 |
| `test_ci_coverage` | 9 | 9 | 0 | 0 | 0 |

No binary ran zero tests. The two ignored `--lib` cases are the pre-existing #182 pair. **No
warning of any kind** in the seven logs (`grep -h '^warning' *.log` empty), so the rebase
introduced none. Logs: `/tmp/empty-sorts-r2/{lib,cpu_corpus,corpus_goldens,cost_model,golden_format,module_layout,ci_coverage}.log`.

The rebase carried the code intact: nothing on this branch needed a fix to go green on the new
base, and the new neighbours in the corpus binaries — limits' `tpch/scan-limit` at five modes and
distinct-companions' `tpch/distinct-functions` — are green beside this task's cells.

#### The cost gate against `ENS-limits`, and it is no longer red

    cost-diff: 565 compared, 2 changed, 2 regression(s), 0 over 10%      rc=0

| section | base | branch | Δ |
|---|--:|--:|--:|
| `tpcds.sf1/q17 tp1-single-mini` | 293556446 | 293556470 | +24 |
| `tpcds.sf1/q17 tp1-rowgroup-mini` | 293557678 | 293557702 | +24 |

Exactly the two rows the human pre-accepted, exactly +24 bytes each, read off the
`peacockdb_cost=` lines at both refs. No third regression, 0 improvements, so nothing moved
downward either. **`rc=0`, where round 1's run against `4581bca3` was `rc=1`** — master's gate
now fails only past +10% (`REGRESSION_FAIL_PCT`), and the rebase brought that in. The two rows
are +0.00% of a 280 MB Σout, so the `cost-report` job is expected **green** on this PR and the
pre-acceptance is no longer load-bearing. Artifacts: `/tmp/empty-sorts-r2/cost_diff.{html,md}`.

#### The GPU host, and the build proved by a literal rather than an exit code

nebius-gpu, `dmitry@89.169.109.150`, `~/peacockdb-K`, cuDF 25.02. `df -h /` **21 GB free before
the build and 21 GB after** — above the 20 GB floor, so chain J's cleanup rule was never reached
and **nothing was deleted**. `nvidia-smi` 0 MiB of 46068 used before the run; chain J never
contended for the card.

Sync was the recipe in the dispatch, uncommitted, with no commit made to build:

    rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ dmitry@89.169.109.150:peacockdb-K/

257 files transferred, 0 deleted. **The rsync-mtime antipattern was checked, not assumed**: a
second `rsync -ain` with the same filters listed nothing, so the host's content equals the
worktree's; the 568 `*.rs`/`*.inc` files were `touch`ed on the host anyway before building.
Build: `./scripts/build-test-shadgpu.sh --build`, detached, never `--run`. Log
`~/K-logs/empty-sorts-build1.log`, **zero warnings in it**. All four Rust binaries staged.

Three literals prove the staged tree is this branch rather than the previous build's:

- `peacockdb_core_gpu_lib tests::gpu_tests::accumulate_cases --list` reports **37 tests** and
  names all four of this task's cases — `one_zero_row_batch_sorts_to_zero_rows_on_both`,
  `a_fetch_over_zero_rows_is_zero_rows_on_both`,
  `every_lane_a_zero_row_batch_merges_to_zero_rows_on_both` and
  `one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both`;
- `strings … | grep -oE 'bug_[a-z_]*_on_the_cpu' | sort -u` finds **none of the three retired
  names**; the five it does find are the cross-join and nested-loop projection pins in other case
  files, pre-existing and untouched. (A bare `grep -c 'bug_.*_on_the_cpu'` prints 42 and means
  nothing: `strings` emits long concatenated lines, so `.*` spans two unrelated symbols. Use
  `-oE`.)
- `nm -C cpp/install/lib/libpeacock_gpu.so | grep -c set_num_rows` is **0**, limits' marker, so
  the staged C++ is at least that branch's `scan.cpp` and not distinct-companions'.

#### Job 2, part 1 — the device tier is green, and the four accumulate cases pass

Per binary, on the **final** tree (build 4), `--test-threads=1`, log
`~/K-logs/empty-sorts-tier-final.log`:

| binary | ran | passed | failed | filtered |
|---|--:|--:|--:|--:|
| `peacockdb_core_gpu_lib gpu_tests::` | 538 | 538 | 0 | 657 |
| `test_gpu_corpus` | 38 | 38 | 0 | 0 |
| `test_node_timing` | 1 | 1 | 0 | 0 |
| `peacock_gpu_benchmarks --skip bench_` | 8 | 8 | 0 | 3 |
| `cpp/install/bin/peacock_gpu_tests` | 4 | 4 | 0 | — |
| `cpp/install/bin/peacock_plan_tests` | 56 | 56 | 0 | — |

Every binary executed tests; none is a zero-test pass. The 657 filtered out of the lib binary
are its cpu and ffi rungs — and that number is now checked rather than taken on trust:
`--list` with no filter reports **1195**, and 653 (rust-only `--lib`) + 4 (`ffi_tests::`) + 538
(`gpu_tests::`) = 1195, so the three rungs partition the binary exactly. The 3 filtered out of
the benchmark binary are its `bench_` cases.

**All four accumulate cases pass on a device**, named individually in the log:

    tests::gpu_tests::accumulate_cases::one_zero_row_batch_sorts_to_zero_rows_on_both ... ok
    tests::gpu_tests::accumulate_cases::a_fetch_over_zero_rows_is_zero_rows_on_both ... ok
    tests::gpu_tests::accumulate_cases::every_lane_a_zero_row_batch_merges_to_zero_rows_on_both ... ok
    tests::gpu_tests::accumulate_cases::one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both ... ok

The fourth is the one no run had ever checked — `gpu_backend/accumulate.rs`'s `held.is_empty()`
arm over one zero-row lane and one silent lane. It agrees.

**The signoff's caveat is now closed by composition, not by one case.** Those four assert only
cpu-device agreement, so alone they cannot tell "one zero-row batch on both" from "nothing on
both". But `run_both` drives the production `CpuBackend` through `drive`, and the cpu side's
*absolute* claim — one batch, zero rows, under the node's declared columns — is pinned by
`assert_one_empty_batch` in `cpu_backend/tests/accumulate.rs` over the same four shapes, green
locally. device == cpu and cpu == one zero-row batch, so the device emits one zero-row batch.

**One invocation trap, paid for here.** The first tier run reported `lib` FAILED with 5 failures
and `gpu_corpus`/`node_timing`/`benchmarks` at *0 tests passed*. Cause: `<binary> -- --test-threads=1`.
A libtest binary parses `--` itself, so everything after it is a **positional filter** — the
threads flag never applied (the lib ran parallel against a process-wide RMM pool, and 3 `abi`
and 2 `accumulate` cases went red on it) and the three binaries with no filter of their own
matched nothing and passed having run nothing. The shipped runner's order is flags first, filter
last (`build-test-shadgpu.sh`: `"$t" --nocapture --test-threads=1 $skip "${args[@]}"`), and that
is what the re-run used. Both logs are kept: `empty-sorts-tier1.log` is the wrong one,
`empty-sorts-tier2.log` and `-tier-final.log` the right ones. **Trap 2 caught this** — the
per-binary ran/passed/filtered table is what made a zero-test pass visible as one.

#### Job 2, part 2 — q17's device cells: all five off, and each cause measured

**The cell is not enabled.** It was run three ways rather than once, because the first failure
masked the ones under it.

| declaration | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| all five on, hook on | **#225** | **#225** | **#152** | **#152** | **#152** |
| two tp1 on, hook masked | **#220** | **#152** | — | — | — |

1. **With the schema hook on, both tp1 cells refuse at `GpuAggregate`** on #225 — nine columns,
   the three Welford triples named for their alias:

       GpuAggregate lane 0: the output hook refused a batch: 6 stddev(store_sales.ss_quantity)$count:
       Int64 vs stddev(store_sales.ss_quantity) INT64; 7 …$mean: Float64 vs … FLOAT64; 8 …$m2 …
       (the same for store_returns.sr_return_quantity and catalog_sales.cs_quantity)

   #225 verbatim: the plan declares `<out>$count`/`$mean`/`$m2`, the device holds all three under
   `<out>`. q17's `features` is `stddev_var avg top_n`, so it was always going to meet this.
2. **The three tp4 cells refuse at `GpuHashJoin`** on #152: *this join's recipe copies its build
   side per probe batch and the ABI has no copy: probe batch 2 has no build side left, since the
   call for batch 1 erased it (#152)*. Same message at tp4-single, tp4-rowgroup and tp4-sized.
3. **Masking the hook at the two tp1 modes** (the mechanism `corpus_cases.inc`'s own header
   documents, and the remedy #225's body records for `tpch/shuffle-stddev`) moved each cell to
   the next cause: **tp1-rowgroup joins the other three on #152**, and **tp1-single reaches the
   golden comparison and fails on #220.**

**tp1-single's divergence is #220, and the cpu is the side that is wrong.** First differing line
is q17's `GpuAggregate` node line in `tp1-single-mini.cpu.txt`:

    expected  …batches=multiple, output_rows=0, output_bytes=312
    actual    …batches=multiple, output_rows=0, output_bytes=12

26 zero-row batches of 12 bytes against the device's one. The probe chain carries 26 batches from
the deepest join down the plan, and q17 matches nothing, so the cpu's join hands back one empty
batch per probe batch that matched nothing while the device answers one table per call. That is
#220's text word for word, and #220 says which engine breaks the contract: *"`architecture.md`'s
rule does [break]: no executor returns more than one batch per call per output lane. `CpuExec::exec`
keeps that rule by concatenating."* So **the cpu is wrong and the device is right**; the fix is
#220's, on the cpu, in `cpu_backend/join.rs`'s `declared`.

**This divergence is not this task's, and the run is positive evidence the task's fix is right
on a device.** `git diff ENS-limits..HEAD` on that golden touches exactly four lines — the
`GpuUnload` and `GpuAccumulateBatchesAndSort` node lines and their stats — and the 312-byte
`GpuProject`/`GpuAggregate` below the join is the base's. The differ prints the **first**
differing line, so everything above it matched: `GpuUnload`, `GpuAccumulateBatchesAndSort`,
`GpuSort`, `GpuProject` and `GpuAggregateBatches` with their stats lines, **the two lines this
branch rewrote among them**. Before #205's fix the cpu's sort rendered `batch_rows=[[]]` where
the device emits one zero-row batch, so that cell would have diverged at the sort as well. It no
longer does. The device and the cpu now agree byte for byte at the two nodes this task changed,
and disagree only below them, on a break that predates it.

#### What the registry and the case list now say, and what the tickets must say

Mine, and applied:

- `corpus_cases.inc`: q17's declaration is **unchanged** (`none`, `schema_validation_enabled`) —
  the mask and the enabled modes were measurements, reverted. The comment above the T19-batch-19
  group replaces "q17 (zero rows) waits on its first device run (#281)" with the three measured
  causes and the date.
- `cost-registry.csv`, row `tpcds,1,q17`: tickets `281` → **`152 220 225`**. #281 has now run, so
  it no longer explains the cells; these three do, and all five device cells stay `disabled`. The
  registry's rule that a disabled cell names a ticket (`registry.rs`, `off == 0 || !tickets.is_empty()`)
  is what makes the column load-bearing rather than decorative. The shape matches the
  neighbours: q15, q16 and q19 carry `152 220` for the same reason.

**No new ticket is needed** — the three that explain q17 all exist and all already describe this
exact failure. What the coordinator owes in `tickets/`, as markdown:

1. **#281 can be archived in full.** Both halves are measured: limits' closed on 2026-10-09, and
   this round ran the four accumulate cases (all pass) and q17's device cell (off, reassigned).
   Nothing is left that #281 holds. Its registry row reference is already gone.
2. **#220 gains `tpcds/q17` at `tp1-single`.** Its "**Corpus queries:**" line says "82 registry
   rows carry `220`" — 83 now. Worth naming q17 among "the first ones seen", since it is the
   first cell where the divergence is *only* over zero-row batches: both engines answer zero
   rows, and the goldens disagree on how many empty batches carried them.
3. **#225 gains `tpcds/q17`.** Its corpus list is `tpch/shuffle-stddev` and
   `tpch/distinct-functions` (schema validation only); q17 is a third, refusing at its
   `GpuAggregate` on three Welford triples at both tp1 modes. Unlike those two, q17's cell cannot
   be rescued by a mask — #220 and #152 sit under it — so it is listed as blocked rather than as
   schema-validation-only.
4. **#152** needs no edit: it carries no corpus-query list, and q17's row now names it.

Also **prose drift for the coordinator** (`build-test.md` is the coordinator's except its
counts): the *Corpus, device* row says the off cells are held by "#152, #95, #220 and the
device's own tickets (#57, #63, #281)". #281 is no longer one of them.

#### Job 3 — `build-test.md`'s counts, every figure measured and every header re-summed

Measured with `--list` on the final tree, both shapes: rust-only locally, `gpu` from build 4's
staged binaries on nebius-gpu. **Every figure on the page is right, including the one set by
reasoning. The page needs no count change.**

*Target totals, `--list`*

| figure the page states | measured | |
|---|--:|---|
| cpu `--lib` 653 | 653 | ✓ |
| cpu `test_cpu_corpus` 569 | 569 | ✓ |
| cpu `test_corpus_goldens` 29 | 29 | ✓ |
| cpu `test_cost_model` 3 | 3 | ✓ |
| ffi `--lib -- ffi_tests::` 4 | 4 | ✓ |
| ffi `peacockdb-ffi --test test_ffi` 3 | 3 | ✓ (three `#[test]`s; no ffi file in this diff) |
| gpu `--lib -- gpu_tests::` 538 | 538 | ✓ |
| gpu `test_gpu_corpus` 38 | 38 | ✓ (q17 stays off, so it does not become 39) |
| gpu `peacock_gpu_benchmarks` 11 | 11 | ✓ |
| gpu `test_node_timing` 1 | 1 | ✓ |
| `test_golden_format` 26 | 26 | ✓ |
| `test_ci_coverage` 9 | 9 | ✓ |
| `test_module_layout` 17 | 17 | ✓ |
| `cost-report` 38 | 38 | ✓ |
| C++ cuDF GPU smoke 4 | 4 | ✓ (ran) |
| C++ Plan-executor 56 | 56 | ✓ (ran) |

*The **Operator harness** row, which the rebase left at 337 by reasoning.* **Measured 337.** The
`gpu_tests::` list grouped by module, non-schema modules under `tests::gpu_tests::`:
accumulate_cases 37 + aggregate_cases 20 + aggregate_dimension_cases 29 + coverage 1 +
emit_cases 15 + exec_cases 57 + harness_cases 24 + join_cases 89 + join_dimension_cases 29 +
nested_cases 24 + script 4 + source_cases 8 = **337**. The collapsed `+1` was real and the
coordinator's arithmetic was right; nothing to correct.

*The rest of the gpu block, same grouping* — schema modules (`*_schema_cases`) 7 + 32 + 6 + 33 +
4 + 38 + 6 + 8 = **134**; `wire::gpu_tests` **21**;
`executor::cpu_backend::gpu_tests::murmur_conformance` **10**;
`executor::gpu_backend::gpu_tests::` less `abi` (11 + 2 + 1 + 12 + 6) = **32**; `abi` **4**. Sum
337 + 134 + 21 + 10 + 32 + 4 = **538** = the block's `--lib` figure, and 538 + 38 + 11 + 1 =
**588** = the block header. Both hold.

*The cpu block's 45 rows, each against its module path* — all 45 match, and they sum to exactly
**653**, so no row is double-counted and none is missing. The four rows whose row name does not
name every module they cover, written down so nobody re-derives them: *End to end* 39 =
`tests::end_to_end` 27 + accounting 3 + dimensions 3 + limits 4 + schema_validation 2;
*Driver internals* 44 = `driver::accounting` 14 + `driver::index` 5 + `driver::scheduler` 15 +
`driver::single_partition` 10; *Plan types* 41 = `plan::validate` 33 + `plan::aggregate` 4 +
`plan::layout` 4; *Forwarders and row ranges* 4 = `executor::forwarder` 3 + `executor::row_range` 1.
*Translator expressions and scan mapping* 27 = `translator::expr` 13 + `scan_mapping::partition` 8
+ `scan_mapping::parquet_meta` 6. *CPU backend executors* 76 is the `cpu_backend::tests::` modules
less the 1 `contract` case, which the page counts in its own row.

*Headers re-summed from the rows, not deltaed* — cpu rows 1254 = 653 + 569 + 29 + 3 = header;
ffi 7 = 4 + 3; gpu rows 588; everything-else Rust 26 + 9 + 17 + 38 = 90; Rust 1254 + 7 + 588 + 90
= **1939**; C++ 94; Python 381; grand total **2414**. Every one is what the page says.

#### Warnings: one, pre-existing, and the build log cannot tell you

`grep warning` over `empty-sorts-build[1-4].log` finds **nothing**, and that is not evidence.
`--build` stages each binary through `cargo test --no-run --message-format=json | python3 …`, so
every diagnostic leaves as a JSON object on **stdout** and is drained by the artifact reader; the
log holds only cargo's stderr progress lines. Checked directly instead, with a plain
`scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`: **exactly one
warning**, `unused import: AsArray`, `tests/gpu_tests/aggregate_dimension_cases.rs:9:42`. That is
round 1's pre-existing one — the file is untouched by this branch and only a gpu-feature build
sees it. Left as round 1 left it, cosmetic and unfiled. The rust-only logs carry no warning at
all.

#### Host state, and nothing deferred beyond the chain's standing exclusions

Four builds and three tier runs on nebius-gpu. `df -h /` **21 GB free before and after every
one**; `du` unchanged; nothing deleted, under `~/peacockdb-K` or anywhere else, and
`~/peacockdb-J` never touched. `nvidia-smi` 0 MiB before and after every run — chain J never
contended, so the shared-card rule was never exercised. No process left running on the host
(`pgrep` for the runner and the build script: 0).

Deferred, and only what the chain's note already excludes: the sf40 pair
(`peacock_tpch_tests`, `peacock_tpchv_tests`), `--run-benchmarks` and Nsight captures. Nothing
else was skipped. Logs on the host: `~/K-logs/empty-sorts-build[1-4].log`,
`empty-sorts-tier{1,2,-final}.log`, `empty-sorts-q17-all5.log`, `empty-sorts-q17-masked.log`,
`empty-sorts-warncheck.log`.

#### A real red the spec's bar does not reach: `cargo test -p cost-report`

**Found and fixed this round.** `cost-report`'s
`tests::the_index_reads_the_anchors_of_every_wiki_file` was **red on this branch** and has been
since round 1:

    assertion `left == right` failed
      left: Some("llm-wiki/archive/archived-tickets.md")
     right: Some("llm-wiki/tickets/corpus-coverage.md")

The test reads the **real wiki** and asserts three example ticket numbers resolve to three
different files, so that `TicketIndex::load` is proved to read anchors from `tickets.md`,
`tickets/*.md` and `archive/*.md`. Its corpus-coverage example was **#205** — which this task
archived. Verified as the branch's own doing and not this round's edits: `git grep 'id="t205"'`
finds it in `tickets/corpus-coverage.md` at `ENS-limits` and in `archive/archived-tickets.md` at
`HEAD`, and the round's working-tree changes touch no cost-report file.

The fix is the one the test's own doc comment prescribes — *"archiving the ticket named here turns
this red, and the fix is to swap in a currently-open number rather than to look for a bug in
`path_for`"*. Swapped **205 → 214**, chosen for durability: of the 33 anchors in
`corpus-coverage.md`, only ten (56, 57, 60, 168, 202, 204, 210, 214, 217, 285) are not on any
chain's closes list on the board, and #214 is the most legible of them — it is #205's sibling in
`architecture.md`'s *Zero-row batches change no answer* bullet and nothing plans to close it.
Red before (1 failed), green after (**38 passed**), and the assertion is not vacuous: #214's
anchor really is in `corpus-coverage.md`, so a broken `path_for` still reddens it.

**Why the bar missed it.** The spec's *Verification bar* is the four rust-only targets, and the
dispatch adds the three repo guards. `cargo test -p cost-report` is in none of them — it is the
`cost-report` CI job, which the branch's `done` section read only for its **gate** output. A
round that archives a ticket must run this crate, because it is the one target that reads
`llm-wiki/` as an input. Nothing else in the tree names `205` any more: a grep over `*.rs`,
`*.inc`, `*.csv`, `*.cpp`, `*.h`, `*.py`, `*.yml` and `*.sh` is empty, which makes round 1's
claim to that effect true *now* and false when it was written (it did not cover `cost-report/`).

#### Everything else CI runs that this host can run, run because of the above

| what | where | result |
|---|---|---|
| `cargo test -p cost-report` | local | 38 passed (after the fix) |
| `--lib -- ffi_tests::` | nebius-gpu, staged gpu lib | 4 ran, 4 passed, 1191 filtered |
| `peacockdb-ffi --test test_ffi` | nebius-gpu | 3 ran, 3 passed |
| C++ `peacock_cpu_tests` (`ctest -L cpu`'s 12) | nebius-gpu | 12 passed, 7 suites |

`test_ffi` reproduced `build-test.md`'s loader trap exactly: `cargo test -p peacockdb-ffi --test
test_ffi` dies `libcudf.so: cannot open shared object file`, exit 127, because nothing puts the
FFI crate's `OUT_DIR/lib` on the loader path. Run the built binary with
`LD_LIBRARY_PATH=target-cudf-*/debug/build/peacockdb-ffi-*/out/lib:$CUDF_ROOT/lib` and it passes.

**One pre-existing warning in `cost-report`**, left alone: `function sha_links is never used`
(`cost-report/src/main.rs:1538`), an uncalled test helper. `git diff ENS-limits..HEAD -- cost-report/`
is empty, so it predates the branch; this round's only edit to that file is the one-token ticket
swap above. Deleting code whose caller's disappearance I did not investigate is the scope-creep
the style guide names, so it is reported rather than silenced.

#### Job 3, continued — the C++ and Python columns measured too

Since one red hid outside the rust-only bar, the other two columns of the grand total were
measured rather than carried over.

*C++ 94*, by `--gtest_list_tests` on the staged binaries and by counting `TEST`/`TEST_F` in the
five suites this host does not build: `peacock_cpu_tests` **12** (ran), `peacock_gpu_tests` **4**
(ran), `peacock_plan_tests` **56** (ran), `peacock_tpch_tests` **4** and `peacock_tpchv_tests`
**4** (listed, not run — sf40 is out of every chain-K task), `test_tpch_streamed` **4**,
`test_cudf_nodes` **1**, `test_multi_gpu_tpch` **4**, `test_multi_gpu_tpchv` **4**,
`test_basic_multi_gpu` **1** (manual / 2gpu, not built here). Sum **94** ✓.

*Python 381*, by running the two CI python steps the way the workflow runs them — direct
`python3 <file>`, not pytest, because the files carry a `__main__` footer and `tests/harness.py`
reads the source back: `testdata/test_duckdb_cost.py` **41 passed**; the exec-model prototype set
(the glob less the three the step names as running elsewhere) **216 passed** across ten files,
summed from each file's own `N passed` line — 17 + 4 + 18 + 21 + 23 + 63 + 24 + 22 + 15 + 9;
`scripts/exec_model/tests/test_tpch.py` **19 passed** (the dataset-matrix one, run locally
against the committed sf1); the calibration set **9 passed** of 12, test_plot.py's **3** counted
statically. Sum 41 + 216 + 19 + 93 + 12 = **381** ✓.

The manual exec-model corpus figure **93** is measured rather than trusted: `test_tpch_corpus.py`
has 22 `def test_` and `test_tpcds.py` has none of its own — it **generates** one per entry of
`plans_tpcds.QUERIES`, which imports to **71**. 22 + 71 = 93.

**Two things this host cannot run**, neither a code risk and neither in this diff:
`scripts/calibration/tests/test_plot.py` (3 cases) imports `matplotlib`, which is absent both
locally and on nebius-gpu — CI installs it, and the branch touches neither `plot.py` nor the
test. And the sf40 / 2gpu C++ suites, excluded by the chain's note.

**A scoping trap worth recording.** `pytest scripts/exec_model/tests/` sweeps
`test_tpch_corpus.py` and `test_tpcds.py` — the 93-case manual set that `exec-model-corpus.yml`
shards across three runners — and runs for tens of minutes. The `cost-report` job names those
three files in an `elsewhere` array and skips them, with a `[ -f ]` check so a rename cannot
silently drop one. Run the step's own loop, not the glob.

#### Disk on nebius-gpu: an obstacle, recorded, and the named cleanup reclaims nothing

`df -h /` was **21 GB free** before and after all four `--build` runs and all three tier runs. It
is **15 GB free** at the end of the round. The 6 GB went on the two extra diagnostic builds this
round added — the plain `cargo-cudf.sh … --features gpu --no-run` warning check and the
`peacockdb-ffi --test test_ffi` build — both into `~/peacockdb-K/target-cudf-rapids-cuda-12.2`,
now 12 GB.

Chain J's cleanup rule was followed and **yields nothing**, measured rather than assumed:

- `~/miniforge3/bin/conda clean -a --dry-run` → *no unused tarballs, no index caches, no unused
  packages, no tempfiles, no logfiles*. The 13 GB in `~/miniforge3/pkgs` is hard-linked into the
  live envs. This confirms limits' finding rather than re-deriving it.
- the #260 debug build in `~/peacockdb` is **991 MB in total**, 847 MB of it `testdata/`, so
  `cpp/build26` and `target-cudf-rapids` there are already gone or never existed.

What is left is reclaimable but **costly or forbidden**, so nothing was deleted:

- `~/peacockdb-J` 29 GB — chain J's, never to be touched;
- `~/peacockdb-K/cpp/install/rust-tests` 4.6 GB — the four unstripped ~1.2 GB staged binaries. A
  re-stage after deletion is a copy from the warm cache, so this is the cheapest 4.6 GB on the
  host if the next round needs it;
- two `target-cudf-*/debug/build/peacockdb-ffi-*/out` trees, 654 MB and 662 MB — one is stale,
  and which one is live takes care to establish; deleting the live one forces a cmake
  reconfigure of flatbuffers, gtest and `libpeacock_gpu.so`;
- the 12 GB target dir itself is the warm DataFusion-at-opt-3 cache; deleting it costs the next
  round a cold rebuild (#85).

**For the next GPU build on this host:** 15 GB free is above the chain note's 10 GB "do not
build" floor and below its 20 GB "clean first" one, and the named cleanup is already spent. The
4.6 GB of staged binaries is the one safe lever.

#### What this round changed, file by file

| file | change |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc` | the T19-batch-19 comment: q17's three measured device causes in place of "waits on its first device run (#281)". **The declaration is byte-identical to the committed one** — `none`, `schema_validation_enabled`. |
| `testdata/cost-registry.csv` | row `tpcds,1,q17`: tickets `281` → `152 220 225`. No state cell moved; all five device cells stay `disabled`. |
| `cost-report/src/main.rs` | `the_index_reads_the_anchors_of_every_wiki_file`: `path_for("205")` → `path_for("214")`, the fix the test's own doc prescribes for an archived example. One token. |
| `llm-wiki/tasks/empty-sorts-detail.md` | this round's record. |

**Deliberately not changed**, each with its reason:

- **q17's device cells are not enabled.** All five were measured and all five fail on open
  tickets that are other chains' work (#225 and #220 on the cpu/aggregate side, #152 on the
  join). The spec names only `tp1-single`, and the brief's rule is pass → enable, fail → off
  plus a ticket.
- **No `schema_validation_disabled` mask was landed for q17**, though it was measured. It is the
  mechanism `corpus_cases.inc`'s header documents and #225's body records for
  `tpch/shuffle-stddev`, and it does move the cell past #225 — but only onto #220 and #152, so it
  buys no coverage and would leave a cell unvalidated for nothing. Reverted after measuring.
- **No new ticket written.** All three causes are open and already describe these failures
  exactly; ticket markdown is the coordinator's in any case.
- **No `llm-wiki/` prose touched beyond this file.** `build-test.md` needs no count change (every
  figure measured correct) and its device-corpus ticket list is prose, so #281's removal from it
  is reported rather than applied. `architecture.md`, `tickets.md` and `tickets/` untouched.
- **The `AsArray` unused import** (`gpu_tests/aggregate_dimension_cases.rs:9`) and
  **`cost-report`'s uncalled `sha_links`** both left: pre-existing, cosmetic, in files this task
  has no other business in.
- **`cost-report/src/main.rs` not rustfmt'd.** `rustfmt --check` reports **73 hunks at `HEAD` and
  73 in this tree** — the file predates the installed rustfmt — and the `205` line was already
  one of them, in exactly the same way `214` is. So the one-token change adds no violation, and
  formatting the file would bury it under 73 hunks.
- **sf40, `--run-benchmarks`, Nsight**: out of every chain-K task, deferred.

#### What the coordinator still owes

1. `tickets/`: archive **#281** in full (both halves measured); add `tpcds/q17` to **#220**'s
   corpus list (82 → 83 registry rows) and to **#225**'s; **#152** needs no edit.
2. `build-test.md`'s *Corpus, device* prose: "#57, #63, #281" no longer holds — q17's cell is
   #152/#220/#225's now.
3. The spec's **completeness signoff** is stale in its last paragraph: the device half is no
   longer "built and not run", #281 no longer holds it, and the caveat that the four cases
   "cannot tell one zero-row batch on both from nothing on both" is now answered by composition
   with the cpu-side absolute pins.

### 2026-10-09 — reviewing, and the control file said `stop`

The developer's round 2 is green with evidence and committed, so the board is at `reviewing` on
PR #174 (base `ENS-limits`, verified: 7 commits, 27 changed files, which is the task and not the
chain). The control file carried `stop`, read after the dispatch returned, so the reviewer was
not dispatched and the run ends here rather than at the end of the cycle. Nothing is running.

**The ticket markdown the developer asked for, applied — it had to land in the same commit**, not
in a later one: `cost-registry.csv`'s q17 row no longer names `281`, so leaving #281 open would
have shipped a commit whose wiki and data disagree.

1. **#281 archived in full**, both halves measured. Its block moved to `archive/archived-tickets.md`'s
   `## Done` with a `Closed by …` note naming both tasks, the four passing accumulate cases and
   q17's three measured causes. Dropped from `corpus-coverage.md` and from that file's contents
   list. `tickets.md`: corpus-coverage 33 → 32, open 117 → 116, both re-summed from the rows, and
   every row's count re-checked against the length of its own id list.
2. **#205's closing note corrected** in the archive. It ended "the mixed merge has one, built and
   not run (#281)" — false since this round, and the link pointed at a block that had just moved.
   Now "all four pass on a device", linked within the archive.
3. **#220 gains `tpcds/q17`**, named as the one cell whose divergence is over empty batches alone.
   Its count went 82 → **88**, not 83: the figure was already stale by five before this branch, and
   88 is measured — `awk -F, 'NR>1 && $NF ~ /(^| )220( |$)/'` over `cost-registry.csv` gives 87 at
   `ENS-limits` and 88 with q17's row.
4. **#225 gains `tpcds/q17`**, with the note that a mask does not rescue its cell because #220 and
   #152 sit under it. **#152** needed no edit, as the developer said.
5. **`build-test.md`'s device-corpus row** named #281 among "the device's own tickets"; it now names
   **#225**, which is what actually holds q17's two tp1 cells as the file declares them.
6. **The board's chain-K note** linked #281 at `../tickets/corpus-coverage.md#t281`; repointed at
   the archive. The links in `limits.md`, `limits-impl.md` and `limits-detail.md` were left: a
   frozen spec and the working records of a finished task. The cost widget resolves a number by
   whichever file holds its anchor (`TicketIndex::load`), so none of them breaks the report.

**What the next coordinator owes, in order.** The task is at `reviewing` with no finding
outstanding, so the next step is the reviewer, then the completeness pass, then CI, then `done`.

- **Dispatch the reviewer** on this branch — round 1's reviewer and analyst saw only the cpu half.
  The new material is the GPU round: the three measured q17 declarations and the conclusion that
  **the cpu is the wrong side under #220**, the `cost-report` test literal `205` → `214`, and the
  ticket markdown above.
- **The spec's completeness signoff is stale and must be rewritten** at `completeness approved`.
  Its last paragraph still says the device half is "built and not run" and names #281 as holding
  it; both are false now. The spec's one-later-write rule is already spent on the old signoff, so
  this is a rewrite of that block, not a second append.
- **The GPU half changed no `architecture.md` sentence that the branch had not already changed** —
  but that is the analyst's reading to make, not this file's claim.
- **CI**: the cost gate should now be **green**, not red-and-accepted. `--cost-diff` gives `rc=0`
  on this base because master's +10% tolerance came across in the rebase, so the board's
  pre-acceptance for this task is no longer load-bearing. A red `cost-report` is a finding again.
  `gpu-tests` stays red on shad-gpu being unreachable; the device evidence is the nebius-gpu run
  recorded above.
- **The host has 15 GB free**, down from 21 across four builds, and the named cleanup reclaims
  nothing. Above the chain note's 10 GB floor, so a build may still start, but the next one should
  read `df -h /` first and expect to have to reclaim the 4.6 GB of staged binaries.

### 2026-10-09 — review round 2 dispatched

Control file empty at startup, so the `stop` from the last run is spent. Board read at
`reviewing`; PR #174 verified open, base `ENS-limits`, 8 commits and 28 files — the task and not
the chain (the eighth commit is the previous run's board write). **verda does not resolve from
this host**, checked at this dispatch, so any CPU run a finding provokes is local.

The reviewer is dispatched on the whole branch rather than on the GPU half alone. Round 1's
reviewer and analyst read the cpu half at `53fc7289`; everything since is unread by any reviewer:
the rebase onto `ENS-limits`, the four converted accumulate cases as measured rather than as
built, the three q17 device declarations in `corpus_cases.inc` and `cost-registry.csv`, the
`cost-report` test literal `205` → `214`, and the ticket markdown (#281 archived, #205's closing
note, #220's count to 88, #225's q17 entry, `build-test.md`'s device-corpus row).

Two things the reviewer is told are the coordinator's and not findings: the stale completeness
signoff in the spec, which is rewritten at `completeness approved`, and `architecture.md`
sentences the branch falsified, which are the analyst's reading at `completing`.

CI at dispatch: `Changed paths` and `S3 datasets metadata check` green, the three pipeline legs
and `cost-report` still pending on run 38004462130. Not waited on — the only wait is
`completeness approved` → `done`.

### 2026-10-09 — review round 2: 0 blocking, 0 important, 7 nits; `completing`

The reviewer read the whole branch and found **nothing blocking and nothing important**, so the
board goes to `completing`. The control file then said `stop`, so the completeness pass was not
dispatched and the run ends here.

**What it verified, so nobody re-derives it.** The composition claim this round rested on holds:
`same_slot` returns `Ok` for `(None, None)`, so the four device cases are relative only — but
`run_both` drives the production `CpuBackend`, `sorted_and_cut`'s new arm is insensitive to fetch,
direction and schema, and `assert_one_empty_batch` pins the cpu side absolutely over the same four
shapes, so device-only, cpu-only and joint regressions are each caught by one half. q17's
declaration is byte-identical between `ENS-limits` and HEAD (only its group comment grew a line).
The registry row satisfies `off == 0 || !tickets.is_empty()`, both directions, and no row names
`205` or `281` any more. `path_for("214")` has teeth: `<a id="t214">` occurs exactly once in the
tree, in `tickets/corpus-coverage.md`, and `add`'s first-wins order cannot shadow it. Every count
re-checked arithmetically: `tickets.md` 116 open, #220's 88 rows, `build-test.md`'s 2414 with the
per-row deltas matching the diff. The rebase lost none of limits' edits. q17's new result section
is byte-identical to `duckdb-result.txt`'s, so the signoff's claim is true. `partitioned.rs` is
996 lines, under the impl plan's 1000 cap.

**Two nits applied here**, both markdown, both a false sentence rather than a preference:

1. **#205's closing note said "as proposed"** and the preserved *Fix proposed* block ends "and
   `tpcds/q17`'s device cell turns on at `tp1-single`" — so a reader of #205 alone concluded the
   cell is enabled. Now "except q17's device cell", with the three measured causes, taking #186's
   established `and again not as proposed` form from the same file.
2. **The archive header named a file that does not exist** — `tasks/active-tickets.md`, among the
   files `TicketIndex::load` reads. It reads three classes (`main.rs:341-343`), and that path is
   absent from the tree. Dropped. It is the sentence describing the mechanism this branch
   exercised twice, which is why it was worth the edit.

**Five nits left, with the reviewer's fixes, for the next run.** None is blocking or important, and
the completeness pass drops nits — but these were found at a findings round, so they are recorded
rather than dropped, and the first one is not a nit's worth of consequence:

3. **`cost-report/src/main.rs:1728` repeats the red this round just paid for.** The companion
   assertion is `path_for("152")`, and **#152 is on chain J's join-backend closes list**
   (`tasks.md:119`, `approved to build`) — verified, not taken on trust. So archiving #152 at that
   merge turns `cargo test -p cost-report` red exactly as archiving #205 did, and no task's
   verification bar runs that crate. The assertion is also redundant for class coverage: #214 on
   the line above already covers `tickets/*.md`. **This is a test body, so it is the developer's,
   not the coordinator's.** Fix: swap `152` for #243, #245, #246 or #250 — all four verified in
   `tickets/joins.md` and on no chain's closes list — or drop the line as redundant.
4. `corpus_cases.inc:284-285` — q17's new clause sits inside the sentence "On the device at
   tp1-single, the other modes never run:" while itself saying all five were measured, so one
   sentence asserts both. Fix: take q17 out of the colon-list and give it its own sentence, as the
   q64/q88 group at `:260-262` already does. Comment-only in a code file, so the coordinator's.
5. `accumulate_cases.rs:181` — the four cases' comment should name the cpu-side absolute pin they
   compose with, since deleting `assert_one_empty_batch` would make all four silently vacuous and
   neither file names the other. One clause. Comment-only, so the coordinator's.
6. Wrap drift left by the re-wraps: `tickets/corpus-coverage.md:72` at 105 columns against the
   file's ~98, `:195-197`'s orphan `merge a keyless`, and `tickets/joins.md:122-123` leaving
   `Pinned by` alone on a line. `accumulate_cases.rs:202` at 101 columns is inside an
   `operator_case!` body rustfmt does not enter, and the file already carries two such lines.
7. Ticket length against coding-style's 15-line cap: #225 33, #199 42, #220 43, #235 86. Every one
   was already over before this branch, which adds 2-5 lines to each. The one compressible thing
   the reviewer named is #225's new three lines saying the same thing twice.

#### CI on the code head, and the cost gate is green as predicted

Run **38004462130** on `9f491834`, the last commit that carries code. `conclusion: failure`, and
the only failing job is the one the chain's note excludes:

| job | result |
|---|---|
| Changed paths | success |
| S3 datasets metadata check | success |
| CI Pipeline (cudf 25.02) | success |
| CI Pipeline (cudf 26.02) | success |
| CI Pipeline (build 25.02 for GPU) | success |
| **Cost report (coverage + ratio)** | **success** |
| Deploy cost report to Pages | skipped (master pushes only) |
| **GPU Tests (remote)** | **failure** — `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out` |

**The cost gate is green, so the board's pre-acceptance was indeed not load-bearing** — the
local `rc=0` prediction held, and a red `cost-report` is a finding again from here on. The
`gpu-tests` red is shad-gpu being unreachable at the rsync step, exit 255, before any binary ran:
not a test failure, and no finding. The device evidence stays the nebius-gpu run recorded above.

So **CI is already satisfied in the chain's `done` terms.** What is left between here and `done`
is the completeness pass and the signoff rewrite, not a wait on a pipeline.

Later runs on this branch are documentation-only pushes and the `changes` job skips every job
under them by design (`pipeline.yml`'s two layers). A restarted coordinator reading `skipping`
across the board should read the newest run that carries code, not the newest run.
