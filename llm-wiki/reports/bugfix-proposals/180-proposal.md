# #180 — a shuffled count(*) merges to nullable against a non-nullable declaration

Read at master c18e063a. Paths relative to `/media/data/peacockdb`. Nothing was built or run;
every claim below is from reading code, goldens and the vendored DataFusion 45 sources
(`~/.cargo/registry/src/*/datafusion-*-45.0.0`, arrow 54.2.1).

## 1. Issue

**What the engine does wrong.** On the CPU backend, a global (keyless) `GpuAggregateBatches`
whose lane received no batch at all manufactures its "identity row" by running its *merge*
aggregators over no input (`peacockdb-core/src/executor/cpu_backend/accumulate.rs:371-374`
→ `compact()` `:351-364`). A `count` merges by `sum`
(`peacockdb-core/src/plan/aggregates.rs:58-63`), and DataFusion's `sum` of nothing is NULL,
while the state column `count(*)` is declared non-nullable (copied from DataFusion's
`Count::state_fields`, `Int64, nullable=false`). `declared_as`
(`cpu_backend/mod.rs:239-275`) relabels the merged batch onto the declared state schema with
`RecordBatch::try_new` (`:269`), and arrow refuses:

    call failed: GpuAggregateBatches lane N: the node declares … and DataFusion answered
    with …: Column 'count(*)' is declared as non-nullable but contains null values

**Where the corpus reaches it.** A lane under a keyless aggregate is empty at tp4 whenever an
Inner join's build side scattered into fewer lanes than there are, because the driver drops
empty scatter outputs (`executor/driver/partitioned.rs:381-385`), the collapse above emits
nothing (`cpu_backend/accumulate.rs:138-141`), the join lane takes `NoBuild`
(`executor/driver/single_partition.rs:317-329`; `plan/join.rs:452-461`
`empty_build_answers_nothing` is `true` for Inner; `cpu_backend/join.rs:184-192`), drains its
probe and emits nothing — so `GpuProject`, `GpuAggregate{count(1)}` and the per-lane
`GpuAggregateBatches{sum(count(*))}` on that lane see no batch. tpcds q93's enabled tp4 golden
shows exactly this lane shape under a grouped aggregate (`in_rows=[[1,0,0,0],[…]]`,
`testdata/goldens/tpcds.sf1/tp4-single-mini.cpu.txt:7454`); the three #180 queries have the
same shape under a *global* one:

- tpcds q96: `store.s_store_name = 'ese'` keeps 1 row (`tp1-single-mini.cpu.txt`, q96's
  `GpuFilter … output_rows=1`); at tp4 it is hashed on `s_store_sk` into one of four lanes
  (`tp4-single.plans.txt:19884-19895`).
- tpcds q90: `web_page.wp_char_count BETWEEN 5000 AND 5200` keeps 1 row, twice.
- tpcds q88: the q96 store filter, eight times.

**Cells it disables** (00-tickets.md row, confirmed): cpu `tpcds/q96`, `q90`, `q88` ×
`tp4-single`, `tp4-rowgroup`, `tp4-sized` — 9 cells; `corpus_cases.inc:134-137, 197-206,
258-267`; `cost-registry.csv` lines 89, 91, 97. The gpu cells of those rows are off on #152
(#185/#187 at tp1-single) and stay off. The two tp1 modes run because one lane holds every
batch: the store/web_page filter's one-row batch reaches the join's coalesce, and even a
zero-row filter answer is an empty *batch* (`CpuExec::exec`, `cpu_backend/mod.rs:173-176`),
so the init runs and emits `count = 0`; the identity path is reached only by a lane with
*no batch*.

**Corrections to the ticket's own reading.** (a) No shuffle is involved: keyless aggregates
skip the emit (`planner/translator/aggregate.rs:370-377`, `Shuffle::Collapse` → `merged`).
What tp4 adds is the per-lane merge-only `GpuAggregateBatches` (`:360-368`) plus the lane
split that lets a lane be empty. (b) The merge over real state never produces a NULL count —
`sum` over one or more non-null Int64 is non-null — so the declaration and the merge do not
disagree; the disagreement is between the declaration and the row the CPU *invents* for an
empty lane. (c) It is CPU-only in mechanism: the device's twin emits nothing for an empty
lane (`gpu_backend/accumulate.rs:307-320`, which is #199).

## 2. Root cause

Trace, tp4, tpcds q96 (`tp4-single.plans.txt:19884`):

1. `GpuEmitPartitions hash=[s_store_sk@0] 1→4` scatters the one `store` row; the CPU scatter
   returns four batches, three empty (`cpu_backend/emit.rs:56-69`).
2. `partitioned.rs:381-385` drops the three empty outputs ("nothing empty traverses a chain").
3. `GpuCoalesceAllBatches` on lanes 1–3 holds nothing; `one_batch` answers with no batch
   (`cpu_backend/accumulate.rs:138-141`).
4. The driver sends those join lanes `LaneCall::NoBuild`; `CpuJoin::without_build` returns
   `Ok(())` for Inner; the lane's probe batches are `DropProbe`d; the lane emits nothing.
5. `GpuProject` and `GpuAggregate{count(1)}` are Exec nodes, 1:1 per batch — never called.
6. The per-lane `GpuAggregateBatches{aggs=[sum(count(*)@0)]}` (no `final=`) gets `Done`
   with `state = None`. `mark_done_and_fetch` (`accumulate.rs:371`):
   `self.state.is_none() && !self.grouped` → `compact()`.
7. `compact()` runs the merge `AggregateExec{Partial, sum(count(*))}` over an empty stream.
   DataFusion's no-grouping stream emits exactly one row on exhaustion whatever arrived
   (`datafusion-physical-plan-45.0.0/src/aggregates/no_grouping.rs:136-148`), built from
   `Accumulator::state()` (`aggregates/mod.rs:1198-1216`); `SumAccumulator::state()` is its
   `evaluate()` (`datafusion-functions-aggregate-45.0.0/src/sum.rs:293-295`), which is
   `None` over nothing. DataFusion's own batch passes its own schema because *its* sum state
   field is nullable (`sum.rs:203-207`).
8. `declared(merged, &self.held)` → `declared_as` → `RecordBatch::try_new(declared, columns)`
   against our state schema, where `count(*)` is `Int64, nullable=false` — copied verbatim
   from `Count::state_fields` (`count.rs:163-167`) at
   `planner/translator/aggregate.rs:150-157`. Arrow refuses. The driver wraps it as
   `CallFailed` (`partitioned.rs:825-830`).

Why only `count`: it is the only aggregate whose state DataFusion declares non-nullable
(`count.rs:166` `false`; `sum.rs:206`, `average.rs:159/164`, `stddev.rs:117/122/124`,
`variance.rs:115-117` all `true`). So a keyless `sum`/`min`/`max` over an empty lane takes
the same path today and silently emits a NULL identity row, which the final merge then skips
— a right answer by luck of nullability, and the reason the corpus never noticed the path
before `count(*)`.

Why the identity path is wrong in principle, not just for count: the identity of an aggregate
is what its *init* produces over no rows (`count → 0`, `sum → NULL`, Welford `(0, 0.0, 0.0)`),
which is what DataFusion's fresh accumulator reports and what our init emits over an empty
batch at tp1. Running the *merge* over nothing gives the merge aggregator's identity instead,
and "count merges by sum" is exactly the row of the registry where the two differ
(`plan/aggregates.rs:58-63`, `plan/mod.rs:387-390`: "that exception is the whole content").

Second finding on the same path: the obligation is placed on the wrong node. The per-lane
merge-only node (`finalize: None`) is an intermediate; whatever it emits is merged again
above. An absent contribution and an identity contribution merge to the same answer, and the
device already emits nothing there. Only the node that *finalizes* a global aggregate owes the
SQL row — and that node, given nothing, hits the same `sum`-of-nothing NULL, so the ticket's
error is also reachable when every lane is empty (section 5, second query).

## 3. Localized fix

Two changes in one function, one small registry helper, no backend other than the CPU.

### 3a. A merge-only node owes nothing for an empty lane (`cpu_backend/accumulate.rs`)

`AggregateBatches::mark_done_and_fetch` (`:366-383`) — restrict the identity obligation to a
node that finalizes, and stop computing the identity with a merge:

```rust
/// A lane that received nothing owes nothing — with one exception. The node that
/// FINALIZES a global aggregate owes its identity row, `count 0` rather than no row; a
/// mid-plan limit that dropped every batch of its one lane, or a join lane with no build
/// side under a keyless count, is how that lane comes to be empty. A global merge that
/// does not finalize owes nothing: what it emitted would be merged again above, and an
/// absent contribution merges to the same answer as an identity one — which is also what
/// the device emits, so the per-node goldens agree.
///
/// The identity row is written from the registry (`plan::identity_state`), never computed
/// by running the merge over no input: a count merges by sum, and the sum of nothing is
/// NULL where the count of nothing is 0 — a NULL the state's declaration refuses (#180).
fn mark_done_and_fetch(mut self) -> CallResult<Vec<CpuBatch>> {
    if !self.pending.is_empty() {
        self.compact()?;
    }
    let state = match (self.state, &self.identity) {
        (Some(state), _) => state,
        (None, Some(identity)) => identity_row(&self.held, identity)?,
        (None, None) => return Ok((Vec::new(), CallStats::default())),
    };
    let Some(finalize) = self.finalize else {
        return one_batch(&self.held, &[state]);
    };
    let finalized = run_node(&finalize, vec![vec![state]], &self.ctx)?;
    one_batch(&self.output, &declared(finalized, &self.output)?)
}
```

- Delete the `(self.state.is_none() && !self.grouped)` clause at `:372`; `compact()` is never
  run over an empty input again.
- `AggregateBatches` gains `identity: Option<Vec<ScalarValue>>`, set in
  `CpuAccumulator::aggregate` (`:65-101`) as
  `(body.group_by.is_empty() && body.finalize.is_some()).then(|| plan::identity_state(state)).transpose()?`
  — computed at construction so an unannotated state fails there, in the `Result` the
  constructor already returns, and only for the one shape that needs it. The `grouped: bool`
  field (`:314-316`) becomes redundant and goes with its doc.
- New private `fn identity_row(held: &SchemaRef, identity: &[ScalarValue]) -> Result<RecordBatch, BackendError>`:
  `to_array_of_size(1)` per scalar, `RecordBatch::try_new(held.clone(), columns)`, errors
  mapped to `BackendError`. ~10 lines.
- `one_batch`'s doc (`:129-137`): "The exception is a global aggregate, which owes its
  identity row whatever arrived" → "the node that finalizes a global aggregate".

### 3b. The identity row comes from the registry (`plan/aggregates.rs`, `plan/aggregate.rs`, `plan/mod.rs`)

`plan/aggregates.rs`, beside `finalize` (`:76`):

```rust
/// What one aggregator reports over no rows: DataFusion's fresh accumulator state, which is
/// also what the init emits over an empty batch — so a global aggregate answers the same
/// whether its lane saw no rows or no batch. `count` is 0 and the Welford mean and m2 start
/// at 0.0; a sum, min or max of nothing is NULL.
pub(crate) fn identity(agg: PlanAgg, data_type: &DataType) -> Result<ScalarValue, PlanError> {
    Ok(match agg {
        PlanAgg::Count | PlanAgg::Mean | PlanAgg::M2 => ScalarValue::new_zero(data_type)
            .map_err(|e| PlanError::Invalid(format!("{} has no zero: {e}", agg.tag())))?,
        PlanAgg::Sum | PlanAgg::Min | PlanAgg::Max => null_of(data_type),
        PlanAgg::MergeM2 => return Err(PlanError::Invalid(
            "merge_m2 merges state and produces no state column of its own".into())),
    })
}
```
(`ScalarValue::new_zero` exists: `datafusion-common-45.0.0/src/scalar/mod.rs:1139`; `null_of`
is already in the file at `:155`.)

`plan/aggregate.rs`, beside `welford_owners` (`:72`), which already does the same
position → aggregator walk:

```rust
/// The state a global aggregate answers with when nothing arrived, one scalar per state
/// column, read off the annotations: each aggregate's positions paired with the aggregators
/// its decomposition says produced them. A grouped state has no identity row, and a column
/// no annotation covers has no identity — both are errors rather than NULLs, since a NULL
/// here is the wrong answer #180 started from.
pub(crate) fn identity_state(state: &Schema) -> Result<Vec<ScalarValue>, PlanError> {
    if !state.group_keys.is_empty() { return Err(PlanError::Invalid("a grouped aggregate owes no identity row".into())); }
    let mut identity = vec![None; state.fields.fields().len()];
    for columns in &state.agg_state {
        for ((_, agg), position) in decomposition(columns.func).state.iter().zip(&columns.positions) {
            let field = state.fields.field(*position as usize);
            identity[*position as usize] = Some(aggregates::identity(*agg, field.data_type())?);
        }
    }
    identity.into_iter().enumerate().map(|(ordinal, value)| value.ok_or_else(|| PlanError::Invalid(
        format!("state column {ordinal} `{}` is no aggregate's state, so it has no identity",
                state.fields.field(ordinal).name())))).collect()
}
```
`plan/mod.rs`: a `pub(crate) fn identity_state(state: &Schema) -> Result<Vec<ScalarValue>, PlanError>`
delegation next to `state_funcs` (`:1236`). No `pub use` (coding-style).

### What it deliberately does NOT touch

- The driver's empty-scatter drop (`partitioned.rs:381-385`) — the trigger, and hacks-audit
  finding 1 under "What the known bugs are propping up"; it is #173/#175's decision. This
  fix is correct with or without it: the identity path stays reachable through a mid-plan
  limit (`accumulate.rs:367-368`'s own case) and an empty `partition_groups[lane]`.
- The translator's copy of DataFusion's state nullability
  (`planner/translator/aggregate.rs:155`), `check_state_layout` (`cpu_backend/mod.rs:467`),
  `declared_as`: the declaration is right — a merged count is never NULL — and the guard
  that caught #180 stays as it is.
- The wire and the C++: no field, no symbol, no payload byte moves (`aggr_input_schema`
  carries nullability, and nullability does not change).
- `gpu_backend/accumulate.rs`: the device already emits nothing at a merge-only node. Its
  finalizing node over nothing still emits no row — that is #199, which after this fix is
  exactly and only that divergence (its text should say so; see below).
- The single-node shortcut `GpuAggregate` with `final=` over a `SingleBatch` lane that
  received nothing (a coalesce or accumulating sort over an empty lane): an Exec never runs
  on no batch, so a global aggregate there emits no row on both engines. Same family as
  #199, not #180's error, not reachable from the corpus; named in the risks.
- `scripts/exec_model` (a keyless pandas merge over an empty frame gives NaN): prototype.

### How CPU and GPU stay one engine

At every node of every corpus cell the two backends now emit the same batch count: nothing
from a merge-only node on an empty lane (device: `gpu_backend/accumulate.rs:311`; CPU: 3a),
so the `in_rows`/`batch_rows` the cpu tier authors for q96/q90/q88 at tp4 are what a device
would produce — e.g. the final node reads `in_rows=[[1]]` from the one populated lane, not
`[[4]]`. The remaining difference — a finalizing global aggregate over an all-empty input:
CPU one identity row, device none — is #199, unchanged in reachability, now with the CPU
half right instead of erroring. Merging count state by `sum` on both engines is untouched.

### Pinning tests, goldens, registry rows, comments that change with it

- `cpu_backend/tests/accumulate.rs:700-748`
  `a_global_aggregate_that_received_nothing_still_owes_its_identity_row`: today it declares
  `count(v)` nullable (`schema_of`, `tests/mod.rs:61-68` makes every field nullable), builds
  a merge-only body, and asserts a row count — so it passes on a NULL count and cannot catch
  #180 (a "test that would not catch the bug it exists for", hacks-audit's phrase). Replace
  with three:
  1. `a_finalizing_global_aggregate_that_received_nothing_answers_its_identity_row` — state
     `count(v): Int64, nullable=false` with `agg_state: [AggStateColumns{func: Count, positions: [0]}]`,
     `finalize: Some([count(v)@0])`, output non-nullable; assert one row and the value
     `Int64(0)`. Red today with the #180 message.
  2. `a_merging_global_aggregate_that_received_nothing_emits_nothing_like_the_device` — the
     same state, `finalize: None`; assert `emitted.is_empty()`. Red today.
  3. `the_identity_row_is_what_the_init_emits_over_an_empty_batch` — for a `count` body and a
     `stddev` body (the two non-NULL identities), run `CpuExec::aggregate` over one empty
     batch and compare its state row with `identity_state`. This is the guard against the
     registry drifting from DataFusion's accumulators, which is the one way 3b could go wrong.
- `test_cpu_end_to_end.rs`: two `sql_answers_match_datafusion("tpcds", …, Coverage::ModesOnly)`
  cases with the two queries in section 5 (one-row build → 3a; no-row build → 3b), run at all
  five modes against DataFusion. Red today at the three tp4 modes.
- `tests/common/corpus_cases.inc`: `:137` q96, `:206` q90, `:267` q88 → all five cpu modes;
  delete the #180 comments at `:134-136`, `:197-200` (keep the q2 sentence), `:258-259`.
- `testdata/cost-registry.csv:89,91,97`: `cpu_tp4_single`, `cpu_tp4_rowgroup`, `cpu_tp4_sized`
  → `enabled`; tickets → `152 185` (q88), `152 187` (q90), `152 185` (q96).
- Goldens, authored not regenerated: `testdata/goldens/tpcds.sf1/tp4-{single,rowgroup,sized}-mini.cpu.txt`
  gain `== q88`, `== q90`, `== q96` sections, their `.cost.txt` siblings likewise;
  `mini.result.txt` sections for the three get `mode=tp4-sized` (the last declared mode
  authors it), values unchanged (874 for q96). No `.plans.txt` moves (plan shape untouched);
  `recipe-payloads.txt` does not move. Checked by reading: every merge-only keyless
  `GpuAggregateBatches` in the enabled tp4 goldens (36 of them) has four populated lanes, and
  no enabled finalizing keyless merge received nothing, so 3a and 3b move no existing section.
- `llm-wiki/build-test.md:42`: drop the two #180 clauses ("`tpcds/q96` carries three disabled
  by #180", "`tpcds/q88` three by #180"; q90 was never listed — drift).
- `llm-wiki/tickets.md` #173 (`:266-267`): "The exception is a global aggregate, which owes its
  identity row whatever arrived" → "…the node that finalizes a global aggregate, which owes
  its identity row". #199 (`:44-52`): narrow to the finalizing node and give it the
  reachability it asks for — the second query in section 5 at tp4: CPU answers `0`, device no
  row. `active-tickets.md` #180 → archive, with the mechanism corrected (empty lane, not
  shuffle; the invented row, not the merge).
- `llm-wiki/architecture.md`: no sentence is falsified. One sentence is worth adding under
  "The aggregate sequence": a merge over an empty lane emits nothing; the node that finalizes
  a global aggregate emits the registry's identity row (`count 0`, Welford zeros, NULL
  otherwise), which the device cannot make (#199).
- hacks-audit scaffolding: finding 3 under "What the known bugs are propping up" names the
  CPU arm's `!self.grouped` clause as the deliberate divergence from the device and warns a
  sweep not to flatten it. This fix keeps the divergence (narrowed to the finalizing node) and
  removes the `grouped` field; the audit's line 459 sentence should be read with that in mind.
  Nothing else in the audit stands on this path; `declared_as`/`widened_decimal` (finding 12,
  #187) is untouched.

## 4. Alternatives rejected

- Declare the count state nullable in the translator (`aggregate.rs:155`): loosens the guard
  to admit a wrong value, moves `recipe-payloads.txt` bytes through `aggr_input_schema`, and
  turns the all-empty case from an error into a NULL `count(*)` — #163's text already says the
  fix is not loosening the guard.
- Stop the driver dropping empty scatter outputs (`partitioned.rs:383`): fixes the corpus
  cells by never making the lane empty, but rewrites every tp4 golden's batch lists and the
  accounting, is #173/#175's decision per hacks-audit, and leaves the identity path reachable
  from a mid-plan limit and an empty `partition_groups[lane]`.
- Keep the identity obligation at merge-only nodes and only fix the value: CPU per-lane
  merges would emit rows the device does not, so the cpu-authored `in_rows` of the final
  node would not match a device run.
- Match the device by emitting nothing everywhere (delete the clause): `SELECT count(*)` over
  an all-empty input returns no row — a wrong answer, and #199 names the CPU's row as right.
- Merge count state with DataFusion's `count` UDAF in Final semantics (identity 0):
  `AggregateExec` takes one mode for all aggregators, and it breaks "count merges by sum" on
  one engine only.
- A `CASE WHEN o IS NULL THEN 0` finalize for count: rewrites every count's `final=` in every
  plan golden and every payload, and admits a NULL state the declaration forbids.
- Compute the identity by running the *init* over an empty batch at the merge node: the merge
  node carries no init body or raw input schema; 3b gets the same row from the registry and
  test 3 pins it to the init.

## 5. Minimum corpus query

Both against the tpcds sf1 schema in `testdata/tpcds.sf1/`, planning mode tp4-single (also
tp4-rowgroup, tp4-sized), CPU backend (`test_cpu_corpus` / `sql_answers_match_datafusion`);
both plan today, nothing refuses them — they fail at run time with the message in section 1,
from `GpuAggregateBatches`'s `mark_done_and_fetch` → `compact` → `declared_as`.

Query A — one populated lane, exposes 3a (q96 with its two dimension joins removed):

    SELECT count(*) FROM store, store_sales
    WHERE ss_store_sk = s_store_sk AND s_store_name = 'ese'

At tp4 the `store` filter keeps one row, the emit hashes it into one lane, three join lanes
get `NoBuild`, and the per-lane `GpuAggregateBatches{sum(count(*))}` on those lanes fails with
"Column 'count(*)' is declared as non-nullable but contains null values". At tp1-single and
tp1-rowgroup it answers correctly. Whichever side DataFusion makes the build (q96's golden has
`store` as the build; `store` is written first here so the logical left is the same), the
scattered one-row side leaves three lanes without a batch — an Inner lane with a build and no
probe batch emits nothing too, having no finish — so the shape does not depend on orientation.

Query B — every lane empty, exposes 3b (and is #199's reachability on the device):

    SELECT count(*) FROM store, store_sales
    WHERE ss_store_sk = s_store_sk AND s_store_name = 'no such store'

At tp4 all four lanes are empty; after 3a alone the *finalizing* node hits the same NULL and
the same error; after 3b it answers `0`, which is what DataFusion answers. At tp1 it answers
`0` today (the filter's empty batch reaches the coalesce, the init runs). On a device at tp4
it would answer no row (#199).

## 6. Cells re-enabled

- Back on: cpu `tpcds/q96`, `tpcds/q90`, `tpcds/q88` × tp4-single, tp4-rowgroup, tp4-sized —
  9 cells, 3 new sections in each of the three tp4 `.cpu.txt`/`.cost.txt` files.
- Stay off, another ticket: every gpu cell of the three rows — tp4 modes on #152 (the
  `store_sales`/`web_sales` probe is many batches), tp1-single on #185 (q96, q88) and #187
  (q90). `build-test.md:44`'s gpu blocker list is unchanged by this fix.

## 7. Risks and unknowns

- Not run. The failing call site (`declared_as` → `RecordBatch::try_new`) is inferred from
  arrow's message text and from the only path that can put a NULL in a count state; the
  developer's first step is the unit test 1 above, red with exactly the #180 message.
- The one-row build sides are read from the tp1 execution goldens (`output_rows=1` on the
  `store` and `web_page` filters) and the lane shape from q93's enabled tp4 golden; that
  q96/q90/q88 at tp4 have no *other* failure behind #180 is unverified — the ticket was found by
  T18's enablement, so nothing has run those cells past this error.
- Cost-report gate: the widget takes a query at its last enabled cpu mode, which moves from
  tp1-rowgroup to tp4-sized for three queries; the regression gate compares against the base
  SHA and may see a byte change for them. Not a correctness risk; may need the gate's usual
  handling for a newly enabled cell.
- 3b's identity for Welford (`0, 0.0, 0.0`) and avg (`count 0, sum NULL`) is read from
  DataFusion's `VarianceAccumulator::try_new` (`variance.rs:264-271`) and
  `AvgAccumulator::state` (`average.rs:289-293`); test 3 pins it. The finalize never reads the
  Welford mean/m2 at count 0 (`aggregates.rs:124-138`), and avg's `NULL / 0` is NULL on arrow
  (`try_binary` skips null slots), so even a drift there changes no answer today; avg is
  behind #163 regardless.
- `Given::of_schema` test nodes with a global finalizing body and no `agg_state` annotation
  will now fail at construction; I found none outside the test being rewritten
  (`test_cpu_executors.rs`, `cpu_backend/tests/backend.rs`, `rebuild.rs` build grouped or
  translator-derived nodes).
- The `.result.txt` `mode=` line change is a regen artefact: run the three queries at the
  tp4 modes under `PCK_UPDATE_SECTIONS=1` (or `UPDATE_CANONICAL=1` with `PCK_TEST_FILTER`)
  and let the last mode author it.

## 8. Complexity

**S.** Two production files with real logic (`cpu_backend/accumulate.rs` ~ +25/−12,
`plan/aggregates.rs` +15, `plan/aggregate.rs` +25, `plan/mod.rs` +8), ~100 lines of tests
across `cpu_backend/tests/accumulate.rs` and `test_cpu_end_to_end.rs`, the `.inc` and CSV
edits, three doc edits (`build-test.md`, `tickets.md` #173/#199, `active-tickets.md` #180 →
archive). No C ABI, FlatBuffers, wire-format or declared-schema change; no golden is
regenerated — nine sections are authored for the first time and `.result.txt` re-stamps its
author mode. The device needs no change and is not needed to verify.
