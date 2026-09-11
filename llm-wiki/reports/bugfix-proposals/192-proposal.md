# #192 — tpcds/q64 needs 13 GB of host memory and no budget stops it

Read-only analysis at master `c18e063a`. Paths are relative to `/media/data/peacockdb`; arrow and
DataFusion cites are the vendored crates under `~/.cargo/registry/src/*/`.

## 1. Issue

The CPU backend's run of `tpcds/q64` grows past 13 GB of host RSS on a 15 GiB host and is killed
before it finishes. The ticket records five samples (11.2 → 13.1 GB), one mode, not which mode,
and "stopped rather than finished". Nothing refuses the plan: it plans at all five modes
(`testdata/goldens/tpcds.sf1/tp1-single.plans.txt:6375`, `--- memory ---` says
`accumulators=29688464444, certain=30244458` — only the 30 MB *certain* part is checked against the
2 GiB budget), and the corpus tier runs it unbudgeted
(`peacockdb-core/tests/common/corpus.rs:65`, `run::<CpuBackend>(…, None)`).

Disabled: `corpus_query!(tpcds, 1, q64, none, none, …)` at
`peacockdb-core/tests/common/corpus_cases.inc:268` (comment at `:260-262`); registry row 65 of
`testdata/cost-registry.csv` has all five `cpu_*` cells `disabled` on `192`. Five cpu cells, and
the five gpu cells behind them. 00-tickets.md's row is correct as written.

The ticket's framing — "the model counts device bytes, RSS is host bytes, and no budget stops it"
— is true but is not the defect. q64's *data* is small at sf1 (the widest intermediate is ≈290K
rows, the answer is 2 rows; DuckDB's oracle `testdata/goldens/tpcds.sf1/q64.duckdb_cost.txt` puts
the whole run at 286 MB materialized). 13 GB is not data. It is bookkeeping that grows
geometrically along q64's join chain, on the CPU backend only, for a reason traced below.

## 2. Root cause

Three facts, each read in the code.

**(a) A joined batch keeps every data buffer of the side it was taken from.** The CPU join runs
DataFusion's `HashJoinExec` once per probe batch over `[[build.clone()], [batch]]`
(`peacockdb-core/src/executor/cpu_backend/join.rs:245-249`). DataFusion builds each output column
with `arrow::compute::take`, and for a `Utf8View` column arrow 54's `take_byte_view` is

    GenericByteViewArray::new_unchecked(new_views, array.data_buffers().to_vec(), new_nulls)

(`arrow-select-54.2.1/src/take.rs:547-557`) — the *views* are gathered, the whole `Vec<Buffer>`
of the input is copied by reference. An 8192-row output chunk taken from a build side holding L
data buffers holds L data buffers.

**(b) A coalesce sums them.** `GpuCoalesceAllBatches` on the CPU is `concat_batches`
(`cpu_backend/accumulate.rs:138-146`, `one_batch`). arrow 54 has no view arm in `concat`
(`arrow-select-54.2.1/src/concat.rs:214-262` falls to `concat_fallback` → `MutableArrayData`),
and `MutableArrayData` collects the data buffers of *every* input
(`arrow-data-54.3.1/src/transform/mod.rs:630-635`):

    DataType::BinaryView | DataType::Utf8View => arrays.iter()
        .flat_map(|x| x.buffers().iter().skip(1)).map(Buffer::clone).collect()

So a coalesce of c chunks each holding L buffers emits one batch holding c·L buffers — all
references to the same few bytes, none of them dropped.

**(c) q64 is a chain of the two.** Its plan (`tp1-single.plans.txt:6375+`) is seventeen Inner
joins in a line, each one's build side the `GpuCoalesceAllBatches` over the previous join, each
probe side a dimension table: `(sr⋈ss)⋈cs_ui⋈d1⋈store⋈customer⋈d2⋈d3⋈cd1⋈cd2⋈promotion⋈hd1⋈hd2
⋈ad1⋈ad2⋈ib1⋈ib2⋈item`, twice (cs1 and cs2). DataFusion chunks a join's output at
`batch_size` = 8192 (visible in every golden: `batch_rows=[[8192,8192,…]]`), so a coalesce above
a join that emits R rows concatenates c = ⌈R/8192⌉ chunks. `s_store_name` and `s_zip`
(`Utf8View`, from `store`, join 4) then carry L_k = Π c_j buffers at the k-th coalesce:

| join | probe | rows out (est.) | chunks | `s_store_name` buffers in the coalesce above |
|---|---|--:|--:|--:|
| 4 | store | ≈52K | 7 | 7 |
| 5 | customer | ≈50K | 7 | 49 |
| 6, 7 | d2, d3 | ≈48K | 6-7 | ≈2.4K |
| 8, 9 | cd1, cd2 | ≈37K | 5-6 | ≈72K |
| 10, 11, 12 | promotion, hd1, hd2 | ≈33K | 4-5 | ≈8M |
| 13 | ad1 | ≈32K | 4 | ≈32M |
| 14 | ad2 | ≈31K | 4 | ≈128M |

A `Buffer` is 24 bytes (`Arc<Bytes>` + ptr + len). At join 13–14 the buffer vectors of two
columns are 1.5–6 GB, `concat_fallback` copies every input's vector once more through
`to_data()` and builds the output's on top, and every `take` in the next join copies it again per
chunk — 3–4× the resident figure in flight, i.e. 10–20 GB, slow enough (10⁸ `Arc` increments per
clone) to be sampled five times on the way. That is the observed shape: 11.2 → 13.1 GB, still
growing, killed. All five modes have the chain; the tp4 ones add an emit/coalesce per lane and
land in the same regime.

**Why the CPU alone, and why only q64 so far.** cuDF has no view arrays: the device's collapse is
`cudf::concatenate`, which copies characters and emits one compact column — the GPU side cannot
multiply. DataFusion itself does not either: its plan for the same SQL puts a
`CoalesceBatchesExec` above every hash join, and `BatchCoalescer::push_batch` compacts view
columns on a heuristic (`datafusion-physical-plan-45.0.0/src/coalesce/mod.rs:118-127`,
`gc_string_view_batch` at `:207-266`). The translator drops that node —
`planner/translator/nodes.rs:85-97`, "batching is this mode's own concern" — and the engine's own
batching node does the concatenation without the compaction. The growth is c^k, so it needs a
long chain with a string column entering early: q64's is the longest in the corpus (399 nodes,
37 joins); every enabled query is under ≈8 joins deep and peaks at ≤1.3 GB
(the T19 timing table, `git show 3dccf7bc`).

What the accountant would have said: `CpuProbingJoin::resident_bytes` and `Coalesce::held_bytes`
are `get_array_memory_size` sums (`cpu_backend/backend.rs:131`, `join.rs:218`), which count every
reference at its buffer's full capacity. With a budget of 2 GiB the run would have tripped early —
but from the same over-count, not from a model that priced the strings. The ticket's "no budget
stops it" is a true sentence about a symptom.

## 3. Localized fix

**One rule, in one function: the one batch a CPU accumulator emits carries compact view columns.**

File `peacockdb-core/src/executor/cpu_backend/accumulate.rs`, function `one_batch` (`:138`):

    fn one_batch(schema: &SchemaRef, held: &[RecordBatch]) -> CallResult<Vec<CpuBatch>> {
        if held.is_empty() {
            return Ok((Vec::new(), CallStats::default()));   // unchanged (#173's arm)
        }
        let batch = concat_batches(schema, held.iter()).map_err(…)?;
        Ok((vec![CpuBatch::new(compact_views(batch))], CallStats::default()))
    }

and a new private helper in the same file, DataFusion's `gc_string_view_batch` recipe verbatim
(it is a private `fn` in DF 45, so it is copied rather than called; ≈25 lines):

    /// A view column leaves an accumulator with at most one data buffer. `take` copies the
    /// whole buffer list of the side it gathers from and `concat` sums its inputs' lists, so
    /// a chain of joins over coalesced build sides multiplies the count at every step —
    /// tpcds q64 reached 10⁸ references to one 12-row column (#192). DataFusion compacts in
    /// the `CoalesceBatchesExec` the translator drops; this is that compaction, and its
    /// heuristic: copy only where the buffers held exceed twice the bytes the views use.
    fn compact_views(batch: RecordBatch) -> RecordBatch {
        let columns = batch.columns().iter().map(|column| {
            let Some(strings) = column.as_string_view_opt() else { return Arc::clone(column) };
            let ideal: usize = strings.views().iter()
                .map(|v| { let len = (*v as u32) as usize; if len > 12 { len } else { 0 } })
                .sum();
            if strings.get_buffer_memory_size() <= ideal * 2 { return Arc::clone(column) }
            let mut builder = StringViewBuilder::with_capacity(strings.len());
            if ideal > 0 { builder = builder.with_fixed_block_size(ideal as u32) }
            for value in strings.iter() { builder.append_option(value) }
            Arc::new(builder.finish()) as ArrayRef
        }).collect();
        let options = RecordBatchOptions::new().with_row_count(Some(batch.num_rows()));
        RecordBatch::try_new_with_options(batch.schema(), columns, &options)
            .expect("the same schema over the same rows")
    }

Notes for the implementer:

- `with_fixed_block_size(ideal)` is what makes the result *one* buffer; arrow's own
  `StringViewArray::gc()` (`arrow-array-54.2.1/src/array/byte_view_array.rs:468`) grows blocks
  8 KiB → 2 MiB and would leave ⌈content / 2 MiB⌉ buffers — bounded, not one. Use the builder.
- The heuristic is DataFusion's, not a new one: all-inline columns (`ideal == 0`) drop their
  buffers outright; a column whose references are ≥3× its live bytes is rewritten; a compact
  column is returned untouched, so the common source-fed case costs one pass over the views and no
  copy. Cover `BinaryView` the same way only if the plan can declare one — today no corpus column
  is `BinaryView`; a `match` on the data type is the exhaustive form the coding style asks for.
- Why `one_batch` rather than `Coalesce::mark_done_and_fetch` alone: `one_batch` is already the
  shared "everything held becomes one batch" of `Coalesce`, `SortedRuns`, the partition
  accumulator and `AggregateBatches`' final emit (`:164, :201, :274, :376-382`). The chain that
  multiplies runs through `Coalesce` (the planner puts one under every build side and under the
  emit beneath a final aggregate — architecture.md, Joins and The aggregate sequence), but the
  invariant is an accumulator's, and stating it once at the shared site means no second
  accumulator can reintroduce the growth. The sorted forms pay at most one copy of what they
  already sorted; the aggregate's output is already compact and passes the heuristic untouched.

**What it deliberately does not touch.**

- `cpu_backend/join.rs` — no per-chunk compaction at the join; the coalesce beneath the next build
  resets the count, and probe-side chains are additive, not multiplicative (a `take` preserves the
  count, only a concat multiplies it).
- `cpu_backend/source.rs::as_declared` — the cast leaves one block per string column even when
  every value is inlined; harmless, and not where the growth is.
- `run_cpu`'s `None` budget and everything in `#182`. Passing `Some(BUDGET)` in the corpus tier is
  a decision about every cell of the tier, its trip on q64 would have come from the
  `get_array_memory_size` over-count rather than from the model, and after this fix q64 needs
  ≈1–1.5 GB (its store_sales read, the same order as `tpch/q19`'s lineitem) — no budget is
  needed to make it safe.
- The planner, the wire, the C++ (`gpu_backend/accumulate.rs`'s collapse is `cudf::concatenate`
  and already compact), `plan_text`, the accountant's formula.
- `llm-wiki/tasks/rmm-pool-budget.md` is on separate ground: it sizes the RMM *device* pool the
  gtest binaries reserve on a shared card (#178). Neither task touches the other's files and
  neither orders the other. The one thing to carry across is its rule — *take the numbers, do not
  choose them*: after this lands, measure q64's peak RSS in the corpus binary the way T19's timing
  table did, and record it in the detail file rather than asserting it fits.

**How CPU and GPU stay one engine.** The compaction changes representation, never rows, values,
types or order. Every golden byte is logical: `CpuBatch::byte_size` is
`logical_size_from_schema` over `array_content_size`, and for `Utf8View` that is Σ value lengths
(`common.rs:115-119`), which a rewrite of the buffers does not move. The device tier reads the
cpu's sections read-only and compares `batch_rows`/`batch_bytes` — unchanged. No wire field, no
ABI symbol, no plan shape.

**Tests, goldens, registry, comments that move with it.**

- New unit test, `cpu_backend/tests/accumulate.rs`, red before the fix: three arrivals whose
  `Utf8View` column is `take`n from one source array (values longer than 12 bytes so the data is
  in a buffer), driven through `coalesce()`; assert the emitted column's `data_buffers().len()
  <= 1`. Today it is 3. The existing fixtures are `Utf8`, so it needs a view fixture of its own.
  Name it as a claim about the invariant, not the ticket
  (`a_coalesce_emits_view_columns_with_one_buffer_however_many_its_arrivals_shared`).
- `corpus_cases.inc:260-268`: the q64 line becomes
  `tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact,
  golden_exact`; the three-line comment about 13 GB goes, replaced by one naming why the device
  column stays off (below).
- `testdata/cost-registry.csv:65`: `cpu_*` → `enabled` ×5, `gpu_*` stay `disabled`, tickets
  `192` → `152 183 185 187`.
- Goldens: eleven *new* sections written by `PCK_TEST_FILTER=q64 UPDATE_CANONICAL=1 … --test
  test_cpu_corpus` — five `tpcds.sf1/<mode>-mini.cpu.txt`, five `.cost.txt`, one
  `mini.result.txt` (q64's answer is 2 rows at sf1, under the cap, so `golden_exact` holds and
  `each_declarations_two_oracles_suit_each_other` stays green). Every existing section must come
  back byte-identical; a `git diff --stat` on `testdata/goldens/` that shows anything but q64
  sections added is a finding.
- `llm-wiki/build-test.md:7,42`: the corpus row's N (447 → 452) and the totals; the "37 queries"
  clause is already drift (00-tickets.md notes it) and this is the commit to correct it.
- `llm-wiki/tasks/active-tickets.md`: #192 closes to `archive/archived-tickets.md` with the
  mechanism in one paragraph, since the ticket text attributes the memory to the wrong quantity.
- `architecture.md`: no sentence is falsified. If a human wants the fact recorded, it is one
  clause on the `GpuCoalesceAllBatches` row of the node table ("on the CPU it also compacts view
  columns, since a concat of view arrays keeps every input's buffers") — growth, so optional.
- hacks-audit scaffolding: none grew around #192 beyond the corpus line and the registry row. The
  empty-lane arm of `one_batch` (audit "known bugs" §3, `accumulate.rs:139`) is untouched and stays
  attributed to #173. Nothing new is built around a bug: no flag, no per-query switch.

## 4. Alternatives rejected

- **`Some(BUDGET)` in `run_cpu`.** Turns the SIGKILL into a clean `BudgetExceeded`, re-enables
  nothing, trips for the wrong reason (the over-count), and puts an unknown number of enabled
  cells at risk at 2 GiB — #182's change, not this one's.
- **Keep DataFusion's `CoalesceBatchesExec` as a plan node.** Moves every plan golden and gives the
  C++ a node it has no kernel for; the engine's batching is its own by design.
- **Compact at `CpuJoin::set_build`.** Also bounds the chain, but it is an accumulator-family
  property fixed in the join, leaves the pre-emit coalesce and sorted outputs one level of
  multiplication, and has no GPU twin to mirror.
- **Compact per output chunk in `probe_and_fetch`.** Copies once per chunk instead of once per
  coalesce, and does not touch a coalesce over a source's chunks.
- **Drive DataFusion's `BatchCoalescer` with `target = usize::MAX`.** Reuses the exact code, but
  `push_batch` compacts per push (≤ c buffers out rather than 1) and silently drops zero-row
  pushes, so the "received nothing emits nothing" rule would need a separate received flag. Viable
  fallback if review prefers reuse over 25 copied lines.
- **Join reordering (#20) or a larger `batch_size`.** Shorten k or shrink c; the mechanism stays.
- **arrow upgrade** (later `concat` compacts sparse views on its own). A DataFusion bump moves
  every golden (#23) — not a fix for one query.

## 5. Minimum corpus query

Six 1:1 lookups off `store_returns` in FROM order, one short string column entering at the first;
no new tables, tpcds sf1:

    SELECT s_store_name, count(*)
    FROM store_returns, store, date_dim, customer, customer_demographics,
         household_demographics, customer_address
    WHERE sr_store_sk = s_store_sk AND sr_returned_date_sk = d_date_sk
      AND sr_customer_sk = c_customer_sk AND sr_cdemo_sk = cd_demo_sk
      AND sr_hdemo_sk = hd_demo_sk AND sr_addr_sk = ca_address_sk
    GROUP BY s_store_name

Mode `tp1-single`, CPU. It plans (no refusal anywhere) and today it climbs into the tens of GB:
`store_returns` is one 288K-row batch, each join emits ≈35 chunks, the coalesce above the fifth
join holds 35⁵ ≈ 5·10⁷ references to `store`'s 12-row `s_store_name` buffer (≈1.3 GB of `Buffer`
structs for one column), and the sixth join's 35 output chunks copy that vector each (≈45 GB in
flight) — killed before the aggregate runs. Drop two tables and it completes but the coalesce above
the third join already holds 35³ ≈ 43K buffers for a 12-value column; drop to two joins and the
coalesce above the first holds 35, which is the assertion the unit test makes on three arrivals.
After the fix every coalesce emits ≤ 1 buffer per view column and the query runs in the memory
its `store_returns` read costs.

## 6. Cells re-enabled

- **Come back:** `tpcds/q64` cpu at `tp1-single`, `tp1-rowgroup`, `tp4-single`, `tp4-rowgroup`,
  `tp4-sized` — five cells, the plan-size batch's fourth query.
- **Stay off:** all five `tpcds/q64` gpu cells. #152 at every mode (the second join's probe is the
  first join's 36-chunk stream, so a build side is copied before a second probe batch even at
  `tp1-single`); behind it #183 (ten `Utf8View` columns reach the unload), #187 (`s1`/`s2`/`s3` are
  decimal sums) and #185 (`GpuAggregateBatches`' `in_rows`). Registry tickets `152 183 185 187`.

## 7. Risks and unknowns

- **Nothing here was run.** The attribution rests on the arrow and DataFusion sources cited, the
  plan shape in the golden, and arithmetic; the ticket recorded neither the mode nor whether the
  RSS was the engine's run or the oracle's. The oracle is DataFusion with its own
  `CoalesceBatchesExec` compaction in place, and every other corpus oracle ran at ≤1.3 GB, so the
  engine's run is the only candidate with the c^k shape — but the first thing the developer
  should do is run the query in §5 at two joins and print `data_buffers().len()` on the coalesce
  output, which settles it in seconds.
- **q64 may fail on something else once it runs.** Nothing in its shape is new to the corpus —
  grouped `count(*)` runs at tp4 elsewhere, Inner joins with residuals stream, decimal sums are
  cast back by `declared_as` — but the T19 rollout never got to compare its answer.
- **Wall time.** 399 nodes, 37 joins, `customer_demographics` hashed four times as a 1.9M-row
  probe; likely tens of seconds per mode in `dataset-matrix`. Measure, per rmm-pool-budget's rule.
- **`peak_bytes` moves for any query whose view columns pass an accumulator** — the accountant's
  CPU figure is `get_array_memory_size` and shared references were counted at full capacity. Only
  #182's two `#[ignore]`d cases read that number, and their constants are re-derived when #182 is
  picked up. No golden renders it.
- **`ideal as u32`** caps a single compacted column at 4 GiB of live string bytes — DataFusion's own
  cap; no sf1 column is near it. Say so at the site.
- **Not fixed, only bounded:** the CPU `resident_bytes` over-count of shared buffers (a build side
  that is a filtered slice of a big table still reports the whole table's capacity). It refuses
  nothing today because nothing production passes a budget; it belongs with #182's accounting
  work, not here.
- The `CpuPartitionAccumulator` and `SortedRuns` paths now copy string content once at emit where
  the heuristic fires. For a large unsorted-string result (`anti-join`'s 240 MB is a join output,
  not a sort) this is at most one copy of what was just sorted.

## 8. Complexity

**S.** Two source files (`cpu_backend/accumulate.rs` +≈35 lines, its `tests/accumulate.rs` +≈30),
one corpus line, one registry row, eleven golden sections written fresh (none regenerated), two
wiki edits. No C ABI, FlatBuffers, wire-format or declared-schema change; the CPU/GPU twin is
already compact on the device side. Existing goldens must be byte-identical, and that is the
cheapest thing to verify.
