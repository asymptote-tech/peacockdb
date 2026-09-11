# #192 — review of the proposal

Read-only at master `188c23ce`. Paths relative to `/media/data/peacockdb`; crate cites are the
vendored `arrow-select-54.2.1`, `arrow-data-54.3.1`, `arrow-array-54.2.1`, `arrow-cast-54.2.1`,
`datafusion-45.0.0`, `datafusion-physical-plan-45.0.0`, `datafusion-physical-optimizer-45.0.0`,
`datafusion-functions-45.0.0` under `~/.cargo/registry/src/*/`. Nothing built or run.

## 1. Verdict

**Needs changes.** The mechanism (view-buffer reference lists multiplied by `take` then summed by
`concat` along q64's join chain) is right by reading and well evidenced, but it stops one step
short of the root cause: the `Utf8View` column it multiplies exists on the CPU path only because
of the reader option #183 is about, and #183's proposed fix removes every such column — after
which `compact_views` has no reachable input. #192 should be re-scoped to a re-enable sequenced
after #183's CPU commit, with no production code of its own. The §5 corpus query would also very
likely not reproduce, because DataFusion's join-side swap puts the chain on the probe side.

**Answer to the question asked:** yes — turning off `schema_force_view_types` (183-proposal §3a)
alone dissolves #192's root cause as diagnosed, for DataFusion 45 / arrow 54. Reasoning in
finding 1.

## 2. Findings

### 1. Blocking — the root cause is one step short, and #183 removes it

The proposal's §2 asks *how* a `Utf8View` column multiplies and answers correctly. It never asks
*why* a `Utf8View` column exists on a path whose reader answers `Utf8`. It even names the place
and dismisses it: "`cpu_backend/source.rs::as_declared` — the cast leaves one block per string
column even when every value is inlined; harmless, and not where the growth is." That cast is
where the substrate for the growth is made:

- `peacockdb-core/src/lib.rs:52` registers every table through `ParquetFormat::default()`, whose
  `schema_force_view_types` defaults to `true` (`datafusion-common-45.0.0/src/config.rs:430`), so
  `infer_schema` runs `transform_schema_to_view` (`datafusion-45.0.0/src/datasource/file_format/parquet.rs:370-371`)
  and every string field is declared `Utf8View`.
- The CPU source reads `Utf8` with arrow's parquet reader and casts to the declaration
  (`cpu_backend/source.rs:112-139`). That cast is `From<&GenericByteArray>` for view arrays
  (`arrow-array-54.2.1/src/array/byte_view_array.rs:666-696`): it appends the whole values buffer
  as one block whether or not any view points into it. So every string column enters the plan
  holding exactly one data buffer — the unit that `take_byte_view` copies
  (`arrow-select-54.2.1/src/take.rs:547-557`) and `MutableArrayData` sums
  (`arrow-data-54.3.1/src/transform/mod.rs:630-635`).

With #183's change the plan declares `Utf8`, `as_declared` never casts, and no `Utf8View` reaches
the CPU path anywhere in DataFusion 45. Checked: the parquet reader answers the file type (`Utf8`
for DuckDB-written files, no embedded arrow schema); literals coerce to the column's type; every
string function returns `Utf8View` only for `Utf8View` input (`datafusion-functions-45.0.0/src/unicode/substr.rs:93-98`,
`string/btrim.rs:98-103`, likewise `ltrim`, `rtrim`, `initcap`; the rest go through
`utf8_to_str_type`); no SQL-parser option maps string types to views in this version (grep of
`datafusion-common-45.0.0/src/config.rs`); `GroupValues` and `SortExec` emit the input type. For
`Utf8`, `take_bytes` writes fresh offsets and values (`take.rs:463-544`) and `concat` goes through
`binary_capacity` + `MutableArrayData`, which copies bytes into one values buffer
(`concat.rs:203-204, 255-266`). Buffer count per column is a constant, so there is nothing to
multiply. #183 alone dissolves #192.

What that makes of the proposed code: `compact_views` matches `as_string_view_opt()` and that
arm is never `Some` once #183 lands — a branch with no reachable input, which `coding-style.md`
forbids ("no defensive code for impossible scenarios"), and the regression test over a
hand-built `Utf8View` fixture would then pin a representation no plan declares, i.e. it tests
arrow. Building it before #183 and deleting it after is the "Building around a bug" pattern the
same page names: a compaction whose only reason is a type the plan should not declare.

Correction: drop §3's code and test. #192 becomes a dependency of #183 — sequence it after
#183's CPU-side commit (the `lib.rs` change plus the `.plans.txt` and payload regeneration; the
shad-gpu rollout in #183 §6 need not finish first). Its own change is the corpus line, the
registry row, the eleven fresh golden sections, the two wiki counts, and a measured run at all
five modes on a 15 GiB-class host, since the ticket's number was a SIGKILL risk for every run of
that tier. Close the ticket with the mechanism in one paragraph and the version it was read
against: a later DataFusion can reintroduce view types by a parser option and a later arrow
gives `concat` a compacting view arm, so whoever takes #23 re-checks this shape rather than
inheriting a guard for it.

Two caveats that do not change the verdict. Nobody has measured q64 under either change; the
proposal's ≈1–1.5 GB is arithmetic, and the ticket closes on the run, not on this reading. And
the proposal's "why the CPU alone" paragraph stays true after #183 for a different reason: not
because cuDF has no view arrays, but because neither engine now declares one.

### 2. Important — the §5 "minimum corpus query" would very likely not reach the shape

The query relies on the chain staying on the build (left) side of every join, as it does in q64.
That is not how DataFusion plans it. `ListingOptions::new` sets `collect_stat: true`
(`datafusion-45.0.0/src/datasource/listing/table.rs:298`), so every scan carries exact
`num_rows` and `total_byte_size` plus per-column min/max, and `JoinSelection` swaps a
`CollectLeft` join whenever the left is bigger (`datafusion-physical-optimizer-45.0.0/src/join_selection.rs:61-85`,
applied at `:235-236`). A join's own statistics are `num_rows: Inexact(estimate)` with
`total_byte_size: Absent` (`datafusion-physical-plan-45.0.0/src/joins/utils.rs:735-741`), and
the estimate exists whenever both key columns carry min/max (`utils.rs:883-889`).

q64's golden proves both halves. Its base join has `store_returns` as build although
`store_sales` comes first in the FROM (`tpcds.sf1/tp1-single.plans.txt`, the `sr_item_sk`
join and its children) — a swap. Its `cs_ui` join has the aggregate as build — a second swap.
Above that no join swaps, because an `AggregateExec` reports unknown column statistics, the
estimate for the `cs_ui` join is `None` → `Absent`, and `should_swap_join_order` answers `false`
on `Absent` for every join above. That accident is what keeps q64's chain on the build side.

The §5 query has no aggregate in its chain. `store_returns ⋈ store`: both `total_byte_size`
exact, 287,514 rows > 12 → swap, `store` builds. Every join after: chain estimate ≈ 287K
(Inexact) against the dimension's exact rows — `date_dim` 73,049, `customer` 100,000,
`household_demographics` 7,200, `customer_address` 50,000 all smaller → swap, chain probes;
only `customer_demographics` (1,920,800) leaves the chain as build. A `take` from a probe chunk
holding one buffer yields one buffer, so the count reaches about 36 once, at the one coalesce,
and the query completes in the memory its scan costs. A developer running it first thing would
conclude the root cause is wrong.

Correction: base the chain on an aggregate so the estimate goes `Absent` and DataFusion stops
swapping, and check the plan text before trusting an RSS figure — every `GpuHashJoin` in the
chain must show `GpuCoalesceAllBatches` as its first child:

    SELECT s_store_name, count(*)
    FROM (SELECT sr_store_sk, sr_returned_date_sk, sr_customer_sk, sr_cdemo_sk,
                 sr_hdemo_sk, sr_addr_sk
          FROM store_returns
          GROUP BY sr_store_sk, sr_returned_date_sk, sr_customer_sk, sr_cdemo_sk,
                   sr_hdemo_sk, sr_addr_sk) r,
         store, date_dim, customer, customer_demographics, household_demographics,
         customer_address
    WHERE sr_store_sk = s_store_sk AND sr_returned_date_sk = d_date_sk
      AND sr_customer_sk = c_customer_sk AND sr_cdemo_sk = cd_demo_sk
      AND sr_hdemo_sk = hd_demo_sk AND sr_addr_sk = ca_address_sk
    GROUP BY s_store_name

Same arithmetic as the proposal's (≈35 chunks per join, 35⁵ references at the fifth coalesce,
35 chunks × that in flight at the sixth join). Or simply use q64 itself at `tp1-single`; the
hand-built three-arrival case is the true minimum for the mechanism. After #183 neither query
reaches anything, which is the point.

### 3. Minor — the heuristic's cost is misdescribed

"A compact column is returned untouched, so the common source-fed case costs one pass over the
views and no copy" and "the aggregate's output is already compact and passes the heuristic
untouched" are not what the copied code does. `get_buffer_memory_size` on a view array includes
the 16-byte-per-row views buffer, so `actual > 2 × ideal` holds for every column whose
non-inline bytes are under 16 B/row plus its block capacity — every short-string column, every
inline-only key from `ByteViewGroupValueBuilder`. Those would be rebuilt at every `one_batch`.
Bounded (one views copy per emit) and moot under finding 1, but the sentence would mislead the
reviewer of the diff.

## 3. Claims verified

Opened and found true:

- Disabled state: `tests/common/corpus_cases.inc:260-268` (comment and the `none, none` line);
  `testdata/cost-registry.csv:65` all five `cpu_*` and `gpu_*` `disabled`, tickets `192`.
- No budget: `tests/common/corpus.rs:65` `run::<CpuBackend>(…, None)`. Plan not refused;
  `tp1-single.plans.txt:6646` `accumulators=29688464444, certain=30244458`.
- CPU join is one `HashJoinExec` per probe batch over `[[build.clone()], [batch]]`
  (`cpu_backend/join.rs:245-249`), `CollectLeft`, no `CoalesceBatchesExec` around it
  (`single_node.rs` runs the node bare). `build_batch_from_indices` uses `compute::take`
  per column (`joins/utils.rs:1241, 1249`); `collect_left_input` concatenates the build
  (`hash_join.rs:993`), and `concat` of one array is a slice (`concat.rs:220-222`), so the
  build's buffer list survives into every output chunk.
- `take_byte_view` copies `data_buffers().to_vec()` (`take.rs:547-557`); `concat` has no view
  arm and falls to `concat_fallback` → `MutableArrayData`, which collects every input's data
  buffers (`transform/mod.rs:630-635`).
- `one_batch` is `concat_batches` and is the shared emit of Coalesce, SortedRuns, the partition
  accumulator and AggregateBatches (`cpu_backend/accumulate.rs:138-146, 164, 201, 274, 376-382`).
- The translator drops `CoalesceBatchesExec` (`planner/translator/nodes.rs:85-97`);
  DataFusion's own `push_batch` compacts through `gc_string_view_batch`
  (`coalesce/mod.rs:117, 220-268`), and the copied recipe matches it.
- q64's shape: seventeen Inner joins per branch, each build the `GpuCoalesceAllBatches` over the
  previous join and each probe a dimension loader; `s_store_name`/`s_zip` enter at the store join
  (fourth from the base). 37 joins in all.
- Join outputs chunk at 8192 — `batch_rows=[[8192,8192,…]]` in the `.cpu.txt` goldens.
- `Buffer` is `Arc<Bytes>` + ptr + len, 24 bytes.
- Goldens are logical: `CpuBatch::byte_size` → `logical_size_from_schema`, `Utf8View` content
  is Σ valid value lengths (`common.rs:115-119`); no execution golden carries `peak_bytes`.
  So the proposed compaction would move no golden, and CPU/GPU agreement is representation-only.
- Accountant figures are `get_array_memory_size` (`cpu_backend/backend.rs:131`, `join.rs:218`,
  `accumulate.rs:330-340`), so a budget would have tripped from the over-count.
- Existing accumulator fixtures are `Utf8` (`cpu_backend/tests/mod.rs:70, 121`), so the
  proposed test would indeed have needed its own view fixture and would be 3 buffers today.
- `git show 3dccf7bc` exists (the T19 wiki commit).
- #180 is keyless `count(*)` only: tpch q4, tpcds q34 and q73 carry a grouped `count(*)` and are
  enabled at all five cpu modes, so q64's grouped `cnt` is not a new wall at tp4.
- No hacks-audit scaffolding grew around #192; the `one_batch` empty arm stays #173's.

## 4. Corrected proposal

Only the sections that change.

**2. Root cause.** As the proposal states it, plus the step above it: the `Utf8View` column is
the CPU's cast to a declaration inherited from DataFusion's parquet-reader option
(`schema_force_view_types`, #183). The multiplication is a property of arrow 54's view kernels;
its substrate is #183's.

**3. Localized fix.** None in production code. Depends on #183's CPU commit. Then:
`corpus_cases.inc` q64 line → `tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup |
tp4_sized, none, data_fusion_exact, golden_exact`, the 13 GB comment replaced by one line naming
#183 as what closed it; registry row 65 `cpu_*` → `enabled`, `gpu_*` stay `disabled` with the
tickets a device run assigns (expected `152 185 187`); eleven golden sections written by
`PCK_TEST_FILTER=q64 UPDATE_CANONICAL=1 … --test test_cpu_corpus`, every existing section
byte-identical; `build-test.md` corpus N and totals; #192 archived with the mechanism and the
version it was read against. Run it once per mode on a 15 GiB host and record peak RSS and wall
time in the detail file before the registry row flips — the number, not a claim that it fits.

**4. Alternatives rejected.** Add, at the top: `compact_views` in `one_batch` — correct as a
mechanism, wrong as a layer; dead the day #183 lands, and building it first is designing around
a bug another ticket removes.

**5. Minimum corpus query.** The aggregate-based chain in finding 2, or q64 itself; verify the
build sides in the plan text before reading an RSS figure.

**7. Risks.** Add: ordering — if #192's goldens are written before #183, the `.cpu.txt` and
`.result.txt` sections carry no types and survive, but the registry flip must wait for #183's
CPU commit, since q64 without it is the 13 GB run. And: the reasoning holds for DataFusion 45
and arrow 54; #23 re-checks it.

## 5. Complexity

**S**, agreeing with the proposal but for a different reason: no code at all once sequenced
after #183. The cost is one measured five-mode run and the golden sections it writes. If a human
decides #192 must land before #183, the proposal's code is the right shape and still S, but it
carries a deletion obligation that should be written into the ticket rather than remembered.
