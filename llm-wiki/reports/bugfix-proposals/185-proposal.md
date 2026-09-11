# #185 — proposal

Read at master `c18e063a`, no build, no run. Paths relative to `/media/data/peacockdb`;
`core/` = `peacockdb-core/src/`, `tests/` = `peacockdb-core/tests/`.

## 1. Issue

**The ticket's title is a misdiagnosis.** `GpuAggregateBatches` does not report anything: the
`in_rows` figure is the driver's, engine-agnostic, and correct on both backends. What actually
diverges is the **CPU backend's join executor**, which hands DataFusion's output to the driver
as it was chunked — one `RecordBatch` per `batch_size` (8192) rows of join output, one per build
row for a cross join, and a trail of zero-row chunks plus one final chunk for a finish pass —
while the device answers every join call with exactly one table. The CPU-authored execution
goldens therefore encode DataFusion's internal chunking as the engine's batch structure, and a
device can never reproduce it.

Where: `core/executor/cpu_backend/join.rs:236-250` (`CpuProbingJoin::probe_and_fetch`) and
`:254-265` (`finish_and_fetch`), both returning `declared(chunks, …)` — a `Vec<CpuBatch>` with
one element per DataFusion chunk (`:269-273`). The device twin returns at most one batch per call
(`core/executor/gpu_backend/join.rs:164-179`, `out.extend(prior)`; `:185-198`). Every other CPU
executor already keeps the one-output rule: `CpuExec::exec` concatenates its stages' chunks and
says so in its doc ("One batch in, one batch out — the contract every `Exec` node keeps",
`core/executor/cpu_backend/mod.rs:173-210`); the accumulators go through `one_batch`
(`cpu_backend/accumulate.rs:138-146`); the source concatenates (`cpu_backend/source.rs:97`).
The join is the one leak.

Why it surfaces at the aggregate's `in_rows`: at tp1-single every lane is one batch out of the
loader, so a join's chunking is the *only* thing that makes a lane multi-batch (verified: in
both `tp1-single-mini.cpu.txt` files every node with a multi-batch lane is a join, a forwarder
over several lanes, or a 1:1 node above a join). Above the join, `GpuProject`/`GpuFilter` are
1:1 so they inherit the chunk count; `GpuAggregate` runs one partial groupby per chunk; and the
first line in *text order* where a chunk count becomes a row count is the parent's
`in_rows=` line — the `GpuAggregateBatches`, whose consumed rows are Σ per-chunk group counts.
The node lines above it are all `batches=single` and identical. The differ
(`tests/common/golden_text.rs:159-187`) prints only the first differing line plus a
"(+N more lines)" count, which is how "differs in `in_rows` and in nothing else" was read off a
section in which every line from the join up differs.

The numbers confirm it exactly. With one probe batch the device's `GpuAggregate` runs a single
groupby over the whole join output, so its output *is* the final group set, and the parent's
`in_rows` equals the merge's own `output_rows`:

| cell (tp1-single) | CPU `in_rows` = Σ per-chunk groups | device = one groupby | chunks the CPU join emitted |
|---|---|---|---|
| tpcds/q96 (`tpcds.sf1/tp1-single-mini.cpu.txt:5089-5100`) | 34 (34 chunks × 1 row, keyless) | 1 | `[31,46,…,3]`, 34 chunks from one 5457-row probe batch |
| tpcds/q93 (`:4927+`) | 7486 | 7169 = the merge's output | 36 chunks `[198,206,…]` from one probe batch |
| tpcds/q38 (`:1707+`) | 12446 | 11788 = the merge's output | 18 × 8192-chunks |
| tpch/q14 (`tpch.sf1/tp1-single-mini.cpu.txt:497+`) | 10 (keyless) | 1 | `[8192×9, 2255]` from one 75983-row probe |
| tpch/q14 at tp4-sized (`tp4-sized-mini.cpu.txt:831+`) | `[3,3,3,3]` | `[1,1,1,1]` | `[8192,8192,2718]` per lane from one probe batch per lane |
| tpch/join-int (`:821+`) | 4600 = 184 chunks × 25 groups | 25 = the merge's output | 184 × 8192 from one 1.5M-row probe |
| tpch/q3 (`:83+`) | 21242 | (would be 11620) | `[8192,8192,8192,5943]` |

Disabled today (per `00-tickets.md`, confirmed against `corpus_cases.inc` and
`testdata/cost-registry.csv` rows 39, 49, 88, 89, 94, 97, 103, 114, 128): the **gpu tp1-single**
cell of tpcds q38 q48 q87 q88 q93 q96 and tpch q3 q14 join_int — nine cells, the only ones in the
rollout where the device completed the plan and the golden text disagreed. Corrected scope: the
same divergence sits behind every device cell currently attributed to #183/#187 whose query has a
join emitting >8192 rows per probe batch or a finish pass (e.g. tpch q4's LeftSemi finish,
`tp1-single-mini.cpu.txt:131-132`, `batch_rows=[[0×18, 52523]]` on the CPU against one table on
the device). Fixing #183 or #187 without this would move those cells from "refused at the unload"
to "golden mismatch at the first join". It also explains the #152 puzzle recorded at
`corpus_cases.inc:222-225`: q11's "336 or 88 probe batches at tp1-single" are CPU chunk counts;
on the device those joins saw one probe batch each, so #152 did not fire.

## 2. Root cause

The driver records `in_rows` as the sum of `Held::rows()` over the child batches it pops
(`core/executor/driver/partitioned.rs:596-614` `take_input` → `:809-811` `record_consumed`), and
records the child's `batch_rows` from the same `Held` when it was queued (`:308-313` →
`:797-807` `record_emitted`). `Held::rows()` reads the batch's `num_rows`
(`driver/accounting.rs:49-51`); a `GpuBatch`'s is an immutable field set from the ABI's
`PeacockNodeStats.rows` (`gpu_backend/mod.rs:196-210`, filled by C++ from `tv.num_rows()` at
`cpp/src/node_session.cpp:463-464`). So on both engines `in_rows(parent) ≡ Σ batch_rows(child)`
by construction; `test_corpus_goldens.rs:250` asserts exactly this law. The divergence can only
be in the child's batch list — and it is.

The chain on the CPU:

1. `CpuProbingJoin::probe_and_fetch` (`cpu_backend/join.rs:236-250`) runs the per-call
   `HashJoinExec` through `run_node` → `execute_single_node`
   (`cpu_backend/single_node.rs:25-55`), which collects **every** batch DataFusion's stream
   yields, then maps them 1:1 into `Vec<CpuBatch>` via `declared` (`:269-273`).
2. DataFusion 45's `HashJoinStream::process_probe_batch` bounds each output batch at the
   session's `batch_size` (`lookup_join_hashmap(…, self.batch_size, state.offset)`,
   `datafusion-physical-plan-45.0.0/src/joins/hash_join.rs:1472-1479`) and yields one
   `RecordBatch` per bound, including zero-row ones. `build_session_state` leaves `batch_size`
   at the default 8192 (`core/lib.rs:25-36`). Hence `[8192, 8192, …, rest]` per probe batch.
3. `CrossJoinExec` yields one batch per build row per probe batch (golden:
   `tp1-single-mini.cpu.txt:875-876`, `in_rows=[[5],[23]] batch_rows=[[23,23,23,23,23]]`).
   `NestedLoopJoinExec{Left}` yields the padded rows as a separate batch (`:903-904`,
   `[[50,1]]`).
4. `finish_and_fetch` (`:254-265`) runs the finish `HashJoinExec` over the concatenated keys as
   the probe; for LeftSemi/LeftAnti the per-probe-chunk outputs are empty and the answer comes
   at probe exhaustion, so it yields k zero-row chunks and one real one (`:131-132`, `:715-716`).
5. The driver queues every element of the `Vec` as a separate batch
   (`driver/partitioned.rs:308-313`), so each downstream 1:1 node runs once per chunk and each
   `GpuAggregate` groups per chunk; the goldens record all of it as the engine's batch shape.

On the device, `GpuProbingJoin::probe_and_fetch` makes the recipe's calls and returns `prior`
— one handle (`gpu_backend/join.rs:164-179`); `finish_and_fetch` likewise (`:185-198`). cuDF's
join returns one table; `execute_node` reports one `NodeStats` per output handle.

This violates a rule `architecture.md` already states: "no executor may return more than one
batch per call per output lane, which is the queue bound the whole flow-control argument rests
on" (Grouping sets, line 290-291) and "queues are self-bounding at one batch per lane" (The
scheduling rule). The CPU join breaks both today (`peak_queued` at a join is the chunk count).

## 3. Localized fix

**One file, two call sites, one helper. No ABI, fbs, wire, planner or driver change.**

`core/executor/cpu_backend/join.rs`:

- Replace `fn declared(batches: Vec<RecordBatch>, schema: &SchemaRef) -> Result<Vec<CpuBatch>>`
  (`:269-273`) with
  `fn one_declared(batches: Vec<RecordBatch>, schema: &SchemaRef) -> Result<CpuBatch, BackendError>`:
  map each chunk through `declared_as` (keeps the per-chunk widened-decimal cast exactly as
  today, so the concat sees identical schemas), then `concat_batches(schema, chunks.iter())`
  (already imported at `:13`). arrow's `concat_batches` returns `RecordBatch::new_empty(schema)`
  for zero chunks (`arrow-select-54.2.1/src/concat.rs:281-283`), so a probe that matched
  nothing or a finish that owes nothing still yields one typed empty batch — which is what the
  device produces (a zero-row table from `CudfHashJoin`, or the anti join + pad).
- `probe_and_fetch` `:250`: `Ok((declared(joined, &self.calls.output)?, …))` →
  `Ok((vec![one_declared(joined, &self.calls.output)?], …))`. The early return for the
  build-side semi family (`per_call: None` → `Vec::new()`, `:241-243`) stays: the device emits
  nothing there too (`gpu_backend/join.rs:170-176`).
- `finish_and_fetch` `:265`: same substitution. The early return for `finish: None`
  (`:255-257`) stays.
- Doc on `CpuProbingJoin` (or the two methods): one batch per call whatever DataFusion chunked
  its answer into — the contract `CpuExec::exec` keeps and the one a kernel meets by returning
  one table. Four lines, per coding-style's cap.

Optional and separable: hoist the "declared_as each, then concat" shape shared with
`CpuExec::exec` (`cpu_backend/mod.rs:186-194`) into one helper in `cpu_backend/mod.rs`. Not
required; the fix reads fine inside `join.rs`.

**Regression test, red before the fix** (`cpu_backend/tests/join.rs`, beside `drive`): a
`TaskContext` from `SessionContext::new_with_config(SessionConfig::new().with_batch_size(1))`
so DataFusion must chunk even the three-row fixture; then
(a) Inner over `dim()` × the whole `FACT` probe → `probe_and_fetch` returns exactly one batch
holding both matched rows; (b) `GpuCrossJoin` over the same fixture → one batch of 9 rows where
DataFusion yields three; (c) LeftAnti → `finish_and_fetch` returns exactly one batch (DataFusion
yields empty chunks plus the answer). Name it as a claim, e.g.
`a_join_answers_each_call_with_one_batch_whatever_datafusion_chunked`. Existing join tests flatten
through `rows_of`/`drive` and keep passing; `finished.is_empty()` at `:233` and `:562` are for
types with no finish and are unaffected.

**What it deliberately does not touch**: the driver (no guard on "≤1 output per lane call" —
`driver/tests/flow.rs:250-257` `a_build_side_that_produced_two_batches_is_an_error` scripts the
mock accumulator to emit two, so a generic guard would need that test re-shaped; leave it for a
hardening task); the GPU backend; `GpuAggregateBatches` on either side; `CpuExec::exec`;
`empty_build_answers_nothing` / `without_build` (#175 scaffolding, respected as is); the
`Vec<B::Batch>` return type of `probe_and_fetch`/`finish_and_fetch` (legitimately empty for the
build-side semi family and for `finish: None`).

**CPU and GPU agreement**: after the change both backends answer a probe call with exactly one
batch and a finish call with exactly one (or none where no finish exists), so batch structure at
every join — and therefore at every 1:1 node and every partial aggregate above it — is identical
by construction. At tp1-single every lane in the CPU golden becomes single-batch except
forwarders, which is what the device already produces. Row counts, group counts and bytes then
agree because both engines price a batch from the same schema formula.

**What must change with it**:

- Execution goldens: every `<mode>-mini.cpu.txt` and derived `<mode>-mini.cost.txt` for tpch and
  tpcds (10 + 10 files) — regenerated by the cpu corpus tier under `UPDATE_CANONICAL=1` (needs
  the sf1 dataset; verda). Node `output_rows` change only at partial `GpuAggregate`s above
  joins (Σ per-chunk groups → one groupby) and at the merges' `in_rows`; `batch_rows` collapse
  to one entry at joins and the 1:1 nodes above them; `output_bytes` shift by per-batch
  bitmap/offset rounding (`core/common.rs:23-59`) and by the smaller partial-aggregate output;
  `abandoned=[92]` on `nested-limits` disappears (one 115-row batch is consumed with a row range
  instead of four spare chunks being dropped). `mini.result.txt` is expected byte-identical —
  concatenation preserves DataFusion's emission order, so an unordered `LIMIT` over a join
  selects the same rows; verify in the regen diff. Plan goldens, `recipe-payloads.txt` and the
  `--- memory ---` sections do not move (plan-time).
- `tests/common/corpus_cases.inc`: gpu column `none` → `tp1_single` on tpcds q38 q48 q87 q88
  q93 q96 and tpch q3 q14 join_int; rewrite the #185 comments at lines 27-33, 142-149, 210-216,
  222-228, 260-264, 272-275 to name the real cause; the q11/#152 sentence at 222-225 resolves.
- `testdata/cost-registry.csv` rows 39, 49, 88, 89, 94, 97, 103, 114, 128: `gpu_tp1_single` →
  `enabled`, drop `185` from `tickets` (keep 152 and the rest). The registry↔CSV tests hold both
  ways; `every_device_cell_has_a_cpu_cell_at_the_same_mode` (`tests/test_cpu_corpus.rs:87-88`)
  comment "six hand-chosen cells" → fifteen.
- `llm-wiki/build-test.md` line 44 (device corpus row): "Six cells today … #185" → fifteen, drop
  #185; the Rust total moves by +9 cases. `tasks/active-tickets.md` #185 → archive with the
  corrected diagnosis; `00-tickets.md`'s "code paths" column for #185 → `cpu_backend/join.rs`.
- `architecture.md` Joins: optionally one sentence that both backends answer a join call with
  one batch, the CPU by concatenating what DataFusion chunked at its `batch_size`. No sentence
  is falsified; two ("no executor may return more than one batch per call per output lane",
  "queues are self-bounding at one batch per lane") become true of the CPU.
- `test_cpu_end_to_end.rs:427-483` (`a_limit_slices_at_most_two_batches_and_stops_the_scan`)
  asserts `UnloadRange <= 2`, `NextBatch == 2`, `peak_queued[limit] <= 1` — all still hold
  with one cross-join batch; no change.

**hacks-audit**: nothing named there is #185's; the audit did not read `cpu_backend/join.rs`
past line 140. Its item 11 ("the device tier is six cells") becomes fifteen. Nothing to remove;
the #175 join scaffolding (`without_build`, `empty_build_answers_nothing`) is adjacent and must
be left alone.

## 4. Alternatives rejected

- Chunk the device's join output at 8192 to match the CPU — extra `slice_handle` calls and
  copies per chunk, against "a kernel takes a whole input and answers with a whole table".
- Compare goldens modulo batch boundaries — gives up the per-node claim the golden exists for,
  and does not help: partial-aggregate `output_rows`/`in_rows` are genuinely batch-dependent.
- Set DataFusion `batch_size` to `usize::MAX` on the join's `TaskContext` — `HashJoinExec` still
  emits the exhaustion batch separately, `CrossJoinExec` still emits one per build row,
  `NestedLoopJoinExec{Left}` still splits the padding; and it changes DataFusion's own memory
  behaviour.
- Coalesce or refuse multi-output calls in the driver — the rule belongs in the executor that
  breaks it; the driver has no backend concat; a protocol guard collides with the mock-driven
  `a_build_side_that_produced_two_batches_is_an_error`.
- Fix at `GpuAggregateBatches` / the `in_rows` recording (the ticket's framing) — there is
  nothing wrong there; it would mask the real divergence.

## 5. Minimum corpus query

Smallest, tpch sf1, tp1-single (tp4-single plans identically: both tables are under the
small-table threshold, so no shuffle), both backends:

    SELECT count(*) FROM region, nation;

Plans today (`GpuCrossJoin` with `GpuCoalesceAllBatches` over `region` as build and `nation` as
probe, then `GpuProject exprs=[]`, `GpuAggregate count(1)`, `GpuAggregateBatches`, `GpuUnload`),
runs on both engines, returns 125 on both, and is refused nowhere. CPU section: `GpuCrossJoin
batch_rows=[[25,25,25,25,25]]` (one chunk per region row), `GpuAggregate batch_rows=[[1,1,1,1,1]]`,
`GpuAggregateBatches in_rows=[[5]]`. Device: `[[125]]`, `[[1]]`, `in_rows=[[1]]`. The corpus
shape (hash join above the 8192 bound), still small:

    SELECT count(*) FROM supplier s JOIN nation n ON s.s_nationkey = n.n_nationkey;

`supplier` is one row group (10000 rows) so one probe batch at every mode; the CPU join emits
`[8192, 1808]`, the device `[10000]`; `GpuAggregateBatches in_rows=[[2]]` vs `[[1]]`. After the
fix both read `[[1]]` on both engines.

## 6. Cells re-enabled

Back on: gpu tp1-single of tpcds q38, q48, q87, q88, q93, q96 and tpch q3, q14, join_int — the
nine registry rows carrying `185`, all of which already run to completion on the device with
matching results. Stay off: their tp1-rowgroup / tp4-* cells on #152 (a real multi-batch probe
copies the build side), and for q96/q88 the tp4 modes on #180 (cpu). Not counted but unblocked
downstream: every #183/#187 tp1-single cell whose query has a join over 8192 output rows per
probe batch or a finish pass, which would otherwise fail on this after those tickets close.

## 7. Risks and unknowns

- No device run was possible. The diagnosis rests on (a) the driver's `in_rows ≡ Σ child
  batch_rows` arithmetic, (b) every CPU golden's join batch list being 8192-chunked / one per
  build row / zero-chunks-plus-answer, (c) DataFusion 45's `lookup_join_hashmap(batch_size)`, and
  (d) the six recorded device numbers each equalling one groupby over the whole probe output.
  The ticket's "batch_rows agree on every node" is incompatible with (a) and is read as the
  differ's first-line-only output.
- Other divergences may sit behind the first differing line in those nine cells (varlen bytes
  of string columns at a join, a type the unload refuses); nothing in the corpus comments
  suggests any, but only the device run settles it.
- `mini.result.txt` byte-identity for unordered `LIMIT`s over joins (`nested-limits`,
  `cross-join`) — expected to hold, verify in the regen diff.
- Float low bits: a partial aggregate over one chunk vs several changes summation order for
  Float64 sums / Welford states above joins. No enabled corpus query has one (the `avg` queries
  are out on #163; `shuffle-stddev` joins nothing); tpch/tpcds sums are decimal. If one
  appears, its oracle becomes `data_fusion_approximate`.
- Cost totals fall at partial aggregates above joins; the cost-report regression gate reads
  history — confirm a decrease is not flagged.
- CPU transient: the concat copies the whole per-call join output once (tpch q18's Left join is
  ~190 MB at tp1-single). Acceptable on the 15 GiB hosts; #192 (q64 at 13 GB) may get marginally
  worse. The `#[ignore]`d budget tests (#182) may need their constants re-derived when revived.
- The `nested-limits` `abandoned=[92]` line vanishing changes what the early-exit rendering
  exercises in the corpus (the `abandoned` field would then appear nowhere in the goldens);
  `driver/tests/render.rs` still covers it.

## 8. Complexity

**S.** Code: one file (`cpu_backend/join.rs`), ~15 lines changed, plus one ~50-line regression
test. No frozen surface moves: C ABI, FlatBuffers schema, wire format, declared-schema contract
and plan goldens are untouched. The bulk of the diff is mechanical: 20 execution/cost goldens
regenerated on a dataset host, nine `corpus_cases.inc` lines and nine registry rows flipped,
comments and two wiki pages corrected, one ticket closed with a corrected diagnosis.
