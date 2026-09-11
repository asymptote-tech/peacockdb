# #188 — review of the fix proposal

Read at master 188c23ce (the proposal says c18e063a; nothing it cites moved). Paths relative
to `/media/data/peacockdb`. Nothing built or run. cuDF read at the 25.02 header
(`~/data/miniforge3/envs/rapids-cuda-12.2/include/cudf`), the 26.02 header
(`/media/data/miniforge3/pkgs/libcudf-26.02.01-*/include/cudf`) and the 25.06a source at
`~/cudf`; parquet at `~/.cargo/registry/src/*/parquet-54.2.1`.

## 1. Verdict

**Needs changes.** The root cause and the fix (loader honours its limit per lane, C++ caps the
table read, Rust counts across calls) are right and every code claim I opened holds; what is
wrong is the test spec and the follow-up: the device executor test as written goes red for a
reason the proposal itself names as unfixed, the suggested corpus line pairs an oracle with a
golden state an existing assertion refuses, and nested-limits' first wall on a device after
this fix is an unticketed zero-column scan, not the cross-join chunking §6 attributes it to.

## 2. Findings

### F1 — nested-limits on a device is walled by an empty-projection scan, not by the cross join (important)

§6 says nested-limits' gpu cells stay off because the CPU cross join returns five batches per
call and the device one table, and proposes a ticket for that. That is true and it is not the
first thing a device hits. The `region` loader in that plan is
`GpuLoadParquet: table=region, projections=[], … schema=[]`
(`testdata/goldens/tpch.sf1/tp1-single.plans.txt:171`) — the only empty-projection loader in
either corpus (grep over all ten plan goldens: one hit, this one). On the device:

- `wire/node_writer.rs:80-100` writes `file_schema` from the node's **output** schema and no
  `projection`, so for region the C++ gets a schema with zero fields.
- `cpp/src/operators/scan.cpp:37-59`: `all_names` is empty, `projection()` is null, so
  `projected_names = all_names` = `{}` and the reader gets `.columns({})` — an explicit empty
  list, `_columns = Some({})` (25.02 `parquet.hpp:343-346`, `std::optional`).
- cuDF's `select_columns` (`~/cudf/cpp/src/io/parquet/reader_impl_helpers.cpp:1512-1550`):
  `use_names.has_value()` is true, the loop over an empty list selects nothing, the output has
  no columns, and `cudf::table::num_rows()` of a table with no columns is 0 (`table.hpp:93`,
  `:127`). The CPU reports 5 rows here (`tp1-single-mini.cpu.txt:878`, arrow's `concat_batches`
  keeps a row count over an empty schema, and parquet 54's `make_empty_array_reader` honours a
  selection, so the CPU half of the fix is safe for this loader).
- Then `cudf::cross_join` refuses it before any row is produced:
  `CUDF_EXPECTS(0 != left.num_columns(), "Left table is empty")` (`~/cudf/cpp/src/join/cross_join.cu:45-46`).

So after this fix, nested-limits' five device cells fail at `execute_node(#4 CudfCrossJoin …)`
with "Left table is empty", not at a section compare. No ticket names it (grep of
`tickets.md` and `active-tickets.md` for zero/empty column, empty projection: nothing). I
read the 25.06a source for `select_columns`; the 25.02 library source is not on this host,
so the "explicit empty list selects nothing" reading is confirmed for 25.06 and inferred for
25.02 from the identical header.

Correction: file a ticket — "a scan projected to no columns reads a zero-column, zero-row
table on the device, and cuDF's cross join refuses it" — and put its number, beside the
chunking ticket's, on registry line 131 and in `corpus_cases.inc:78-79`. The smallest fix is
not this task's; the shape is #158's family (a table the surface cannot make out of nothing:
a row count with no columns). §6's chunking ticket stays, as the second wall.

### F2 — the device executor test's `batch_bytes` assertion rests on a fixture that is not Int64-only (important)

§3 says the `test_gpu_executors` twin "asserts … `batch_bytes` of the 3-row second batch
equals the CPU's (Int64 only, so exact on both)". Both fixtures carry a string column:
`test_gpu_executors.rs:74-79` declares `k: Utf8, v: Int64` and `mapped()` (`:124-155`)
projects `vec![0, 1]` with no limit parameter; `cpu_backend/tests/source.rs:31-41` is
`grouped(keys a..f, values 1..6)`, also a string beside the Int64. They are also different
fixtures — the device's values are `2,1,4,3,6,5` (`executor_cases.inc:13-20`), the CPU's
`1..=6` — so "the same two batch lists" `[[1,2],[3]]` is the CPU's list, not the device's.

Under the proposal's case (mapping `[[[0],[1,2]]]`, limit 3) the second batch is capped 4→3
in C++ and then sliced 3→1 by `sliced()`, which prices it with
`logical_size_from_schema(schema, rows, 0)` (`gpu_backend/accumulate.rs:449-456`, hacks-audit
finding 1, which §3 and §7 say this task does not fix). With `k` projected, the CPU's byte
figure includes the string content and the device's does not, so the assertion is red after
the fix for finding 1's reason. §7 even predicts this — "a hand-written case with strings
would show a `batch_bytes` disagreement" — and §3 writes that case.

Two more things about the same case: on the device it cannot tell the C++ cap from the
Rust slice (delete the cap and 4→1 still comes out as 1 row), and the CPU test's second case
(limit 2 over `[[[0],[1,2]]]`) meets the bound on a row-group boundary and so never exercises
`with_limit` cutting inside a batch — only the first case does.

Correction: give `mapped()`/`loader()` a limit parameter (both hardcode `None`), and assert
bytes on a batch the C++ capped rather than one Rust sliced: limit 3 over `[[[0,1],[2]]]`
reads 4 rows, caps to 3, `remaining` hits 0, no slice — one batch of 3 whose stats are the
ABI's and so equal the CPU's with the string column in. Keep `[[[0],[1,2]]]`/limit 3 for the
slice path, asserting rows only, with the doc saying the bytes are finding 1's.

### F3 — the suggested corpus line pairs `live_cpu` with a section the cpu tier will freeze (important)

§5 proposes `corpus_query!(… customer LIMIT 3 …, data_fusion_subset, live_cpu)` "at all
five modes on both engines". `live_cpu` is the oracle for results over the 256 KiB cap
(`mini.result.txt:969-971`, `:1336-1338` are the two shapes it is used for); a three-row
answer is frozen into `.result.txt` like every other, and
`assert_oracle_suits_the_golden` (`tests/common/corpus_gpu.rs:119-142`) refuses `live_cpu`
against a frozen section: "a device-side cpu run for a comparison the committed section
already makes". §6 flags exactly this pairing as latent on scan-limit, then §5 writes it for
the new query. It fails on the first device run.

Correction: `golden_exact` for the device oracle. The rows are row group 0's first three in
file order at every mode on both engines (the CPU by `with_limit`, the device by the C++
cap), so one section serves all five — and if that is felt to be too strong for an
unordered LIMIT, `skip`, at the cost of the device cell proving only that the run completes.

### F4 — the minimum query, and what its shape depends on (minor)

`SELECT c_custkey FROM customer LIMIT 3` reaches both defects. The §5 claim that the three
tp4 modes plan `GpuUnload: skip=0, fetch=3` over the loader holds only because
`customer.parquet` is 12,388,477 bytes, above DataFusion's default
`repartition_file_min_size` of 10 MB, so the scan is four file partitions and the
`GlobalLimitExec` survives. `planner/translator/tests.rs:603-614` shows the other case:
`SELECT * FROM nation LIMIT 3` **at tp4** is `Unload(LoadParquet(limit=3))` with no
interval. So "the tp1 shape" is a small-table shape, #186's wrong answer is reachable at
every mode, and the more minimal reproduction is `SELECT n_nationkey FROM nation LIMIT 3`:
a 2 KB file, one row group, the bare-scan shape at all five modes. The proposal's choice is
fine for the later-straddler arm; say what it rests on.

### F5 — the golden rule rewrite is looser than the claim (minor)

§3 rewrites `a_loaders_batches_line_up_with_the_row_groups_that_made_them`
(`test_corpus_goldens.rs:289-320`) to `emitted <= count` and `Σ batch_rows ≤ limit` for a
loader carrying `limit=`. Under `early_exit=none` the true claim is stronger: a limited lane
stops short of its mapping only because it met the limit, so
`emitted == count || Σ batch_rows == limit`. As proposed, a loader that emitted nothing
passes. Hacks-audit's "assertions looser than the claim above them" is the pattern.

### F6 — first-hour edits the proposal does not list (minor)

- `PAYLOAD_QUERIES` is `[(&str, &str); 20]` (`test_plan_goldens.rs:174`); the array length
  moves with the entry.
- `test_gpu_abi.rs:29-63`: `LoadedPlan::new` plans a `const SQL`; the new ABI test needs it
  parameterised.
- `test_gpu_executors.rs:183-186`: `Session::scan` does `let [call] = recipe.calls…` and
  panics on a two-call recipe, so a limited loader cannot go through that helper — the new
  test has to drive `GpuSource` via `GpuBackend::executors_for` (as the CPU twin does via
  `CpuBackend::executors_for`, `cpu_backend/tests/source.rs:82-87`).
- `customer_scan_plan` (`cpp/tests/gpu/test_plan_executor.cpp:1094-1113`) takes a
  projection only; `limit` is `CreateCudfScan`'s sixth argument counting `_fbb`
  (`cpp/build/generated/gpu_plan_generated.h:2394-2402`), so the helper grows an argument.
- `sliced()` as `pub(super)` in `gpu_backend/mod.rs` is wider than needed: a private `fn` in
  `mod.rs` is already visible to `accumulate` and `source`.
- scan-limit's schema has five `Utf8View` columns (l_returnflag, l_linestatus,
  l_shipinstruct, l_shipmode, l_comment), not four; the four `Decimal128(15,2)` is right.

## 3. Claims verified

Opened and found true, so the consolidator can lean on the rest of the proposal:

- cuDF's exclusivity is `set_row_groups` refusing a non-empty list once `_num_rows` is set
  (`~/cudf/cpp/src/io/functions.cpp:805-811`); the 25.02 and 26.02 headers declare the same
  three setters (`parquet.hpp:219/299/306` and `:304/402`). `scan.cpp:61-63` sets `num_rows`
  first, `:77-78` the groups second — so the throw is at the row-group setter, which is the
  ticket's message. `cudf::slice` (initializer-list overload) and `table(table_view)` exist in
  both headers (`copying.hpp:512`, `table.hpp:77`; 26.02 `:501`, `:66`).
- `lanes_for` (`nodes.rs:374-389`) returns 1 for any limit and leaves batching to the mode;
  the tp1-rowgroup golden plans scan-limit at 49 batches, so `architecture.md:374`'s "one
  batch" is false of the code, and one batch would not have saved the device anyway.
- `wire/attach.rs:100-113` publishes one `PerBatch` scan call; `node_writer.rs:93` writes
  `limit: node.limit.unwrap_or(0)`; `gpu_backend/source.rs:32-44` refuses any recipe but
  `[scan]`; `read_next` (`:63-90`) calls once per mapping entry; `node_session.cpp:478-480`
  refuses an empty list; stats are computed from the returned view (`:502-505`, `:238-242`),
  so a capped table prices exactly.
- `CpuSource` (`cpu_backend/source.rs:37-109`) never reads `node.limit`. parquet 54's
  `with_limit` composes with `with_row_groups`: `apply_range` (`arrow_reader/mod.rs:840-870`)
  builds the selection over `reader.num_rows()`, which is the sum over the selected groups
  (`:639-645`).
- Only three loaders in the corpus carry a limit — scan-limit's, and nested-limits' `part`
  (limit 28) and `region` (limit 23) — in every tpch mode golden and no tpcds one; the plan
  shape of both queries is the same at all five modes; no payload query has one.
- `PerStraddlingBatch`, `PriorOutput`, `RowRange`, `Call::bare`, `AbiSymbol::SliceHandle`
  exist (`wire/mod.rs`) and `Recipe`'s `Display` (`recipes.rs:171-193`) renders the proposed
  line; `check_seq_kinds` (`read.rs:95-131`) skips bare calls; the payload coverage test
  (`test_plan_goldens.rs:507-560`) does demand a payload query for the new shape.
- `limit_positions` (`validate.rs:38-52`) refuses an interval owner under the sink, so the
  driver-owned alternative is correctly rejected. The driver runs only
  `check_canonical_form` (`partitioned.rs:91`), not `validate`.
- `tests/common/rebuild.rs:393` `source(Some(7))` is two lanes with a limit and is only
  debug-compared and rendered (`test_layout_injection.rs:33-63`); the two `validate` calls in
  that file (`:229`, `:241`) are on `merge_over_sorted()`, which uses `source(None)`. The
  injection dimensions (`injection.rs:345-367`, `:621-636`) never change a loader's lane
  count; `drained` keeps `load.limit` and is a no-op at one lane; `InjectedSource` wraps
  `CpuSource` by value (`:186-234`), so the new field composes.
- `a_limit_slices_at_most_two_batches_and_stops_the_scan` (`test_cpu_end_to_end.rs:425-495`)
  counts `NextBatch` and exhaustion is `SourceExhausted` (`single_partition.rs:394-401`), so
  `pulled == 2` holds; `most_offered > 2` is a plan-shape fact and does not move. GpuLimit's
  `satisfied_by` is `seen >= skip + fetch` (`plan/mod.rs:1034-1036`), so 28 rows still
  satisfy `GpuLimit@3` and the marker stays.
- The golden numbers: `part` loader 200000 (single modes) / 122880 (rowgroup modes) → 28;
  scan-limit's unload `batch_bytes=[[1828]]` at tp4, `early_exit=GpuUnload@1`;
  `mini.result.txt:1320` is a frozen scan-limit section under `mode=tp4-sized`.
- `cpu_backend/join.rs:110-120`, `:236-251`: the cross join is `one_call` over
  `CrossJoinExec` and `declared(joined)` keeps DataFusion's chunking.
- Recipe walk (`test_gpu_recipe_walk.rs:308`) plans no limit (`:164`, `:382`, `:462`); the
  exec-model Python carries no scan limit at all, so there is no scaffolding to fight there.
- Every `architecture.md` line cited (`:181`, `:374-378`, `:763`, `:814`, `:1043`, `:1073`)
  says what the proposal says it says.
- No C++ test depends on `set_num_rows` semantics (grep of `cpp/tests` for a scan limit:
  nothing).

## 4. Corrected proposal

Only the sections that change.

### §3 — pinning tests, corrected

- Device executor test: give `mapped()` a `limit: Option<usize>` (and `loader()` on the CPU
  side the same). Two cases: **cap only** — limit 3 over `[[[0,1],[2]]]`: one batch, the
  values of row groups 0–1 capped to three, `Exhausted` next, and `batch_bytes` equal to the
  CPU's on that batch (the stats are the ABI's, string column included); **cap then
  slice** — limit 3 over `[[[0],[1,2]]]`: batches of 2 and 1 rows, rows asserted, bytes not,
  with a line saying the second batch's bytes are hacks-audit finding 1's. Drive `GpuSource`
  through `GpuBackend::executors_for`, not `Session::scan`.
- `a_loaders_batches_line_up_with_the_row_groups_that_made_them`: for a loader whose line
  carries `limit=`, under `early_exit=none` assert `emitted == count || Σ batch_rows ==
  limit`; under any other marker `emitted <= count`; and `Σ batch_rows ≤ limit` always.
- `PAYLOAD_QUERIES` length 21; `LoadedPlan::new(sql)`; `customer_scan_plan(fbb, projection,
  limit)`.

### §5 — minimum corpus query, corrected

`SELECT n_nationkey FROM nation LIMIT 3` is the smallest: 2 KB, one row group, and it plans
as `Unload(LoadParquet(limit=3))` with no interval at **every** mode
(`translator/tests.rs:603-614`), so it shows #186 at tp4 too. Keep the customer query beside
it for the later-straddler arm and for the interval-on-unload shape, and say the latter rests
on customer.parquet being over DataFusion's 10 MB repartition threshold. If it becomes a
`corpus_query!` line, its device oracle is `golden_exact` (or `skip`), never `live_cpu`.

### §6 — cells, corrected

- `tpch/nested_limits` gpu × 5 stays off, on two walls in this order: (1) the `region` scan
  is projected to no columns and reads a zero-column, zero-row table on the device, and
  `cudf::cross_join` refuses it (new ticket, this one first — it is a device wrong answer /
  refusal, not a golden mismatch); (2) the CPU cross join returns a probe call's output as
  DataFusion chunked it, the device as one table (the ticket §6 already proposes). Registry
  line 131 and `corpus_cases.inc:78-79` name both.
- Everything else in §6 stands.

### §7 — one risk to add

The 25.02 library's `select_columns` was not read (headers only on this host); the
zero-column reading is from 25.06a source plus the identical 25.02 header. The first device
run of nested-limits after the fix settles it either way, and the finding changes which
ticket that run should be filed under, not whether this fix is right.

## 5. Complexity

**M**, agreeing with the proposal. The fix is ~60 lines and no frozen surface moves; what
makes it M rather than S is unchanged — two backends plus C++, four test tiers on two hosts,
and five plan goldens, ten execution/cost sections and one payload section moving in one
commit. F1–F3 change the test spec and the follow-up ticket, not the size.
