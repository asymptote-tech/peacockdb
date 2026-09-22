
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Welford aggregation

<a id="t225"></a>
### #225 — the device names every Welford state column by the same alias

The plan declares a `stddev` or `var` state as `<out>$count`, `<out>$mean`, `<out>$m2`; the
device holds all three under `<out>`, at the init and at the merge alike, the types as declared.

The wire folds the triple into one `AggregateFuncNode` with one `alias` (`plan/aggregate.rs`,
`state_funcs`, `welford: true`), and `aggregate.cpp`'s Partial and Merge arms push each child
under it. No answer is wrong today: the merge packs the triple by offset and the finalize reads
it by ordinal; a reader resolving a state column by name would take the wrong one. The fix is
the three names on the wire, or the suffixes appended in `aggregate.cpp`. Pinned by
`bug_stddev_holds_three_identically_named_columns` (the init) and
`bug_stddev_merge_holds_three_identically_named_columns` (the merge)
in `gpu_tests/aggregate_schema_cases.rs`. 2026-09-17: the corpus's schema validator refuses
`tpch/shuffle-stddev` at its `GpuAggregate` on these names, so its row says
`schema_validation_disabled`; the cell stays enabled and its values match.

**Corpus queries:** `tpch/shuffle-stddev` (schema validation only).

**Fix proposed:** add `state_names: [string]` to `AggregateFuncNode`. `state_funcs` fills it
from the owner's `positions` in the state schema. `aggregate.cpp`'s Partial and Merge arms name
the triple from it; the Final arm, which answers one column, already names it right. Not
suffixes in C++: `$count`/`$mean`/`$m2` is DataFusion's convention, and a second copy drifts. Then both `bug_` tests expect `None`.

<a id="t216"></a>
### #216 — the device's global aggregate has no Welford arm

A keyless `stddev` answers one finished `Float64` on the device where the plan declares the
`[count, mean, m2]` state, so the finalize above it fails; a keyless `var` is refused outright.

`aggregate.cpp`'s grouped path honours `mergeable` and emits the triple with `MERGE_M2`; its
keyless path (`key_cols.empty()`) tests `is_stddev_name` alone and reduces that name with
`make_std_aggregation`, whatever the phase — at the init the sample stddev of the argument, at
the merge the stddev of the state's first column, the count — and the finalize project refuses
with `ColumnRef index 2 out of range (cols=1)`; a `var` name falls to `make_reduce_agg`'s
`unsupported aggregate function: var`. So `SELECT stddev(x) FROM t` and `SELECT var(x) FROM t`
are refusals on the device in every shape. Pinned in `aggregate_dimension_cases.rs` by the
four `bug_` cases `…welford_init_answers_a_finished_stddev…`, `…keyless_welford_merge…`,
`…global_stddev_finalize_is_refused…` and `…keyless_var_merge_is_refused…`; the init's one
column read at the handle by `aggregate_schema_cases.rs`'s `bug_a_global_stddev_holds_…`.

**Corpus query:** none yet; the simplest is `select stddev_samp(l_quantity), var_samp(l_quantity)
from lineitem;` (tpch).

**Fix proposed:** in `execute_aggregate`, a keyless node with any stddev or var runs whole through
the grouped path on one constant `INT32` key, dropped before return — cuDF has `M2`/`MERGE_M2`
for groupby only. The plan and recipes do not change. Empty input still answers one row, built
per aggregator: a count 0, the rest NULL. The keyless `is_stddev_name` arm goes. After #225, so
its tests can assert `holds_as_declared`.

<a id="t94"></a>
### #94 — MERGE_M2 count-child type is cuDF-version-specific
The stddev/var Merge arm (`cpp/src/operators/aggregate.cpp`, the `AggPhase::Merge` Welford
branch) casts `valid_count` to INT32 for the 25.02 GPU runtime; 25.10 and later accept only
INT64 or FLOAT64 (`group_merge_m2.cu`). The Final arm casts the same way but never runs. The
26.02 CI leg is build-only so it stays green — this bites at the next GPU-remote cuDF bump.
Switch or version-gate the type then; the comment marks the site.

The plan never sees this type: it declares every `$count` state `Int64`, and so do the columns
between calls. The INT32 lives inside the Merge call alone — cast in before `MERGE_M2`, widened
back to INT64 before it returns. On 25.10 and later the count stays INT64 throughout.

**Fix proposed:** gate at compile time. `cpp/CMakeLists.txt` passes `cudf_VERSION`'s major and
minor as `PEACOCK_CUDF_VERSION_*` defines; `aggregate.cpp` picks one `constexpr` count type from
them — INT32 before 25.10, INT64 after — and both `MERGE_M2` sites use it. No single type serves
both versions, and each cuDF version is its own build, so no runtime probe. The gate goes when
25.02 does.

## Aggregates

<a id="t199"></a>
### #199 — a global aggregate over no arrival drops its identity row

`gpu_backend/accumulate.rs:307` answers an empty lane with nothing. The CPU counterpart has a
`!self.grouped` clause and answers with the identity row — `count` is 0, not absent.

So a global aggregate whose lane received no rows disagrees between the engines: the CPU emits one
row and the device emits none. A wrong answer rather than a refusal, and nothing refuses it.
Shown on a device by `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device`
(`gpu_tests/aggregate_cases.rs`), which also shows the CPU's row is sum's identity, not count's:
a count merges by sum, so a merged count over nothing is NULL there where SQL says 0. The init over
a zero-row batch keeps its row on both.

A second site, on both backends: the single-node shortcut (`translator/aggregate.rs:322-339`), a
keyless `GpuAggregate` over a single-batch input. When no batch arrives the node is never called
(`single_partition.rs:192,248-251`) and no row comes out — symmetric, so the cpu-vs-device
comparison cannot see it. A `GpuLimit` keeps its input's single-batch layout, so a limit over a
sort, merge or coalesce feeds it, and #214's dropped batch lands here. The rule for both sites is
one: a keyless aggregate answers one row whatever arrived. No corpus query is known to reach it.

**Fix proposed:** at the init, where both sites start. `GpuAggregate` moves from `Exec` to a new
category, `ExecWithDone`: per-batch calls as now, plus a done call through which a keyless lane
that received no batch emits its identity state row; a grouped one emits nothing. The row comes
from `decomposition()` (`plan/aggregates.rs`), which gains an empty-input value per state column —
`0` for `PlanAgg::Count`, NULL for the rest — so both backends build the same row. The shortcut
finalizes it (count 0, the rest NULL); a merge never meets no arrival, and its count sums to 0,
not NULL. Every aggregate sequence starts with a `GpuAggregate` (`aggregate_sequence`, its only
builder), so the init covers them all. The driver owes the done call to every lane, one that saw
no batch included — today such a lane gets no call. Sources of nothing below the init (#214's
limit, #205's sort, the joins) need no fix of their own for this; between init and merge a keyless
sequence only collapses lanes, which keeps the one-row batch.

## Sort / Limit

<a id="t217"></a>
### #217 — a sort with `fetch 0` keeps every row on the device

`GpuSort` with `fetch: Some(0)` answers zero rows on the cpu and the whole batch on the device.

`sort.cpp` applies its slice under `sort->fetch() > 0`, and the wire writes `-1` for no fetch
(`node_writer.rs`, `fetch_of`), so zero is a fetch the device reads as none. The merge in
`node_session.cpp` tests `>= 0` and is right. `LIMIT 0` under an `ORDER BY` is the SQL shape;
DataFusion usually plans it away, so no corpus cell reaches it. Pinned by
`bug_a_fetch_of_zero_keeps_every_row_on_the_device` (`gpu_tests/exec_cases.rs`).

**Corpus query:** none, and likely none possible: `select l_orderkey from lineitem order by
l_orderkey limit 0;` (tpch) is the shape, but DataFusion is expected to turn `LIMIT 0` into an
empty relation before a sort exists. Unconfirmed; a plan-golden run of that query settles it.

**Fix proposed:** `sort.cpp:54` tests `sort->fetch() >= 0`, as `node_session.cpp`'s merge does.
Safe: every `CudfSort` writer sets `fetch` explicitly — `fetch_of`'s `-1` for none, the
accumulating sort's `-1`, and the three C++ tests. The `bug_` pin flips to a green case.

<a id="t214"></a>
### #214 — a limit drops a zero-row batch on both backends

A `GpuLimit` handed a batch of zero rows emits nothing for it, on the cpu and on the device alike.

Both `LimitStream`s take their cut from the one `RowInterval::range_of` (`plan/interval.rs`),
which answers `None` when `start < stop` is false — and it is false for `n_rows == 0` whatever
the interval — so the batch is released as if it lay outside the interval. Nothing and a zero-row
batch are different arrivals downstream, as #205 says: a build side of `filter → limit → coalesce`
with zero survivors reaches `without_build` and #175's refusal for Right, Full and RightAnti,
where the same filter without the limit pads and answers. Symmetric, so the harness's cpu-vs-device
comparison is green by construction. Pinned by `bug_a_stream_of_one_zero_row_batch_is_dropped_on_both`
and its two neighbours (`gpu_tests/harness_cases.rs`), which want the zero-row batch under the
schema. Numbered past #213, which the branches above this one have taken.

**Corpus query:** none; the only full joins, tpcds q51 and q97, have their `LIMIT` at the top.
Simplest shape: `select n_name, r_name from (select * from region where r_regionkey < 0 limit 1)
r right join nation on n_regionkey = r_regionkey;` (tpch) — refused where the same query without
the `limit` answers. Unconfirmed that the filter hands the limit a zero-row batch.

**Fix proposed:** in the joins, not the limit — a join reads no batch on either side as an empty
side. Build: `without_build` pads Right, Full and RightAnti rather than refusing (#212). Probe:
the finish answers over zero keys (#173, the device's unfiltered LeftSemi/LeftMark), and the
one-call forms make their call over zero rows — a hash LeftAnti/LeftMark with a residual filter
(every build row, or every row marked false) and a nested-loop Left (the build rows padded); both
answer nothing today, on both backends, unticketed. The keyless aggregate's same gap, both
its sites, is #199's and deferred. Then this ticket drops with its three `bug_` pins.

<a id="t205"></a>
### #205 — the cpu's accumulating sort and merge answer nothing over zero-row batches

A `GpuAccumulateBatchesAndSort` or `GpuMergeSortedPartitions` whose only batches have zero rows
emits no batch on the cpu, where the device emits one of zero rows.

DataFusion's `SortExec` over zero rows yields no batch at all, and `SortedRuns::mark_done_and_fetch`
and `CpuPartitionAccumulator::accumulate_and_fetch` (`cpu_backend/accumulate.rs`) hand that empty
answer to `coalesce_or_nothing`, which reads it as the lane that received nothing. `CpuExec::exec` concatenates
the same empty answer under the declared schema and gets zero rows, and the cpu coalesce does too, so
the two cpu paths disagree with each other as well as with the device. Downstream, nothing and a
zero-row batch are different arrivals: a global merge over nothing is #199's site. Pinned by
`bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu` and its two neighbours
(`gpu_tests/accumulate_cases.rs`). First corpus cell to reach it: `tpcds/q17` at `tp1-single`,
which answers zero rows — 12 bytes at the device's unload against the cpu's 0 (2026-09-17).

**Corpus query:** `tpcds/q17` at `tp1-single`, its device cell off on this ticket. The 12 bytes
are the device's one zero-row batch: three `Utf8` columns, one 4-byte offset each. Simplest:
`select n_name from nation where n_nationkey < 0 order by n_name;` (tpch).

**Fix proposed:** `SortedRuns::mark_done_and_fetch` and the merge's `accumulate_and_fetch` skip
the sort when every held batch has zero rows and hand the held batches to
`coalesce_or_nothing`, which answers one zero-row batch under the schema, as the device does.
The empty-arrival arm stays nothing. The three `bug_` pins flip to green cases.

## Scalars

<a id="t191"></a>
### #191 — the device exports Int16 for an extracted year the plan declared Int32

`tpch/q8` at `tp1-single`: `the exported stream is not the sink's rows: expected Int32 but found
Int16 at column index 0`. That column is `o_year`, an `extract(year from o_orderdate)` — DataFusion
types it `Int32` and the device answers `Int16`.

**Not [#187](../archive/archived-tickets.md#t187), and merging them would lose the distinction.** That one is the
device *widening* a decimal, to 38 whatever the declaration says. This is the device *narrowing* an
integer, to the natural width for a year rather than to a maximum. Opposite direction, different
type family, and a fix for either says nothing about the other.

Not new behaviour either, only newly reached: `extract_year -> INT16` was already on record as a
place where the DataFusion type is an imperfect proxy for the cuDF one. What is new is a corpus
query whose unload sees it.

One cell, `tpch/q8` at `tp1-single` — which is the only mode that gets far enough to reach the
unload, the other four stopping at [#152](../tickets.md#t152). Pinned at the project by
`bug_a_year_extracted_from_a_date_is_exported_as_int16` (`gpu_tests/exec_cases.rs`).
`utf8-everywhere`'s rollout, 2026-09-16, added `tpch` q7 and q9 at the same mode, so three
registry rows carry it.

**Corpus queries:** `tpch/q7`, `q8` and `q9` — the only corpus queries that extract a date
field. All three stop here at `tp1-single`; their other four modes stop earlier, at #152.
Simplest: `select extract(year from o_orderdate) from orders;` (tpch).

## Repartitioning

<a id="t206"></a>
### #206 — a float or boolean partition key is refused on the device

A `GpuEmitPartitions` hashing a `Float64` or a `Boolean` column is refused by the device's kernel,
where comet's hasher answers it on the cpu.

`spark_hash_partition.cu`'s type switch takes STRING, INT8-64 and DATE32 and fails on everything
else: `unsupported key column cuDF type_id=10` for a double, `11` for a boolean, `27` for a decimal
(that one is #95). Spark hashes a double as its long bits and a boolean as an int, and comet's
`create_murmur3_hashes` does both, so the cpu lane assignment is defined and the device's is a
refusal. Any `GROUP BY` or join key of either type at more than one lane reaches it. Pinned by
`bug_a_float_key_is_refused_on_the_device` and `bug_a_boolean_key_is_refused_on_the_device`
(`gpu_tests/emit_cases.rs`).

**Corpus query:** none — no `tp4` plan golden hashes a Float64 or Boolean key, and the sf1 data
has no float column. Simplest, at any `tp4` mode: `select cast(l_quantity as double) q, count(*)
from lineitem group by q;` and `select l_quantity > 25 b, count(*) from lineitem group by b;`
(tpch).

<a id="t95"></a>
### #95 — a decimal partition key is refused on the device
murmur3 covers int/date/timestamp/composite/null; decimal deferred (float indefinitely).
Needed by the first shuffle on a decimal key (tpch q18 `o_totalprice`, q10 `c_acctbal`,
tpcds `i_current_price`). Dispatch by *logical* precision (≤18 → low 8 LE bytes of int128;
>18 → raw 16B LE) and thread precision through the partition FFI. Until then
`spark_hash_partition.cu`'s type switch fails with `unsupported key column cuDF type_id=27`,
which is what [#184](tasks/active-tickets.md#t184)'s q15 hits on `total_revenue`. The cpu's comet
hasher takes the decimal, so the shape is a refusal on one side. Pinned by
`bug_a_decimal_key_is_refused_on_the_device` (`gpu_tests/emit_cases.rs`).

**Corpus queries:** eight hash a decimal key at every `tp4` mode — tpch q2 (`ps_supplycost`),
q10 (`c_acctbal`), q15 (`total_revenue`, precision 38), q18 (`o_totalprice`); tpcds q24, q37,
q82 (`i_current_price`), q75 (`sales_amt`, precision 31). Their `tp4` device cells are off, on
blockers that refuse first (#152, #184); q15 and q75 need the >18 path.

## Performance

Tickets pulled ahead from the performance path to fix earlier

<a id="t154"></a>
### #154 — every operator exit path deep-copies its output into a fresh table
`std::make_unique<cudf::column>(view)` deep-copies the device buffer, and 21 sites under
`cpp/src/` do it — 10 in `join.cpp`, 7 in `aggregate.cpp` — mostly to a table the same
function just produced.

`execute_hash_join` is worst per exit: `cudf::gather` returns an owning table, the code copies
each column into `all_cols` (~L337, ~L342), then copies the kept ones again if the node projects
(~L376). `release()` moves instead; `scan.cpp` L103 and `join.cpp` L254 are the pattern
(`union.cpp`'s site went with its `output_schema` block in decimal-precision-at-export), and it
is C++-internal — no header, fbs, Rust or golden moves. Five kinds: whole table
freshly produced (`join.cpp` 202, 337, 342, 512, 515), mechanical; ordinal subset (`join.cpp`
211, 270, 376, 525, `filter.cpp` 41), needing an assert the ordinals are distinct; a column of
an **input** table kept in the output (`join.cpp` 259, `project.cpp` 44, `window.cpp` 46),
changing who destroys what under `NodeInputs`; a temporary that only ever needed a view
(`expr.cpp` 834, below); and `aggregate.cpp` 413, 642, 644, 678, 680, 759, 771, unresolved without
reading. Traps: a view taken before the release dangles (`ftv` ~L372), and a repeated projection
ordinal moves one column twice leaving a hole — a wrong answer, not a throw, which is why it
needs the assert and not the observation. Land before [#155](#t155).

The `expr.cpp` site is the cheapest to fix and the most expensive to leave. `build_column`'s
`ColumnRef` arm copies the whole column and the caller takes `->view()` of the copy one line
later; every consumer (`cudf::binary_operation`, `unary_operation`, the function arms) takes
a `column_view`, and the input table outlives the call. Returning `table.column(idx)` — or
resolving `ColumnRef` leaves in `build_column_binary` before recursing — needs no ownership
change. It fires once per `ColumnRef` leaf per batch on every predicate `is_ast_able` rejects
(a decimal operand, a string literal, LIKE, CASE): q6's filter copies five lineitem columns per
batch (`l_shipdate` ×2, `l_discount` ×2, `l_quantity`), and q19's copies string columns, offsets
and chars. The sf40 HBM reading puts it at ~46 of the 107 GB q19's lineitem filter moves, and
17× the useful traffic at its part filter. The `And` chain's intermediate bool columns are a
separate cost — one kernel per node, which only fusion (JIT, or stitching back into the AST)
removes — and not this ticket's.

