# Join tickets

Joins run as a session on both engines (`architecture.md`, Joins). The tickets here are the
shapes it still refuses or answers wrong, each with a pbench query that shows it, and each its
own fix. The join rewrite's tickets are in [`archive/archived-tickets.md`](../archive/archived-tickets.md).

## Contents

- [#243 (float key equality) the cpu treats -0.0 and 0.0, and NaNs of different bits, as different keys](#t243)
- [#245 (nested-type shuffle key) a nested-type key cannot cross a shuffle](#t245)
- [#246 (LIKE column pattern) a `LIKE` whose pattern is a column is refused on the device](#t246)
- [#250 (nullable IN subquery) an `IN` subquery whose NULL answer is read is refused when its data holds NULLs](#t250)
<a id="t243"></a>
### #243 (float key equality) the cpu treats -0.0 and 0.0, and NaNs of different bits, as different keys
A float join or group key equates `-0.0` with `0.0` and every NaN with every other NaN on the
device and in DuckDB, and does not on the cpu, so the two engines answer a float-keyed join or
`GROUP BY` differently wherever those values occur. DataFusion compares and hashes floats by their
bits; cuDF and DuckDB by value, with NaNs equal.

Measured 2026-10-07 over keys `0.0, -0.0, NaN, -NaN, NULL, 1.0`: DuckDB 1.5.4 and cuDF 25.02/26.02
make **4** groups of the six, DataFusion 45 makes **6**, at one and four partitions and under both
join selections.

**The lane split this ticket also described is fixed and is out of scope now.** Comet hashes a NaN
by its raw bits, which put `NaN` and `-NaN` in different lanes at tp4 while tp1 equated them;
`repartition-keys` canonicalizes every NaN on both engines before hashing. What remains is the
cpu's equality alone.

**Fix proposed:** what Spark does — `NormalizeFloatingNumbers`, mapping `-0.0` to `0.0` and every
NaN to one canonical NaN, so the comparison only ever sees canonical values. Here that is a planner
normalization under every float join and group key, on both engines.

**Corpus queries:** none in tpch or tpcds, whose float columns hold no `-0.0` or NaN. pbench's
`float64-key-join`, `float64-key-group` and `float32-key-group` show it, landed as commented-out
`corpus_query!` lines naming this ticket. Pinned by `bug_` cases for a float-keyed join and a
float-keyed aggregate, cpu against device.

<a id="t245"></a>
### #245 (nested-type shuffle key) a nested-type key cannot cross a shuffle
A join or `GROUP BY` keyed on a struct or list column is refused at run time at the tp4 modes, on
both engines: comet's hasher has no struct or list arm, and neither has the device kernel
(`cpp/src/spark_hash_partition.cu`). The tp1 modes, which do not shuffle, answer.

The lane rule needs a definition for nested values that both engines share — hashing each child
in order, as Spark does for a struct, with a list's elements in order — and the conformance gate
extended to it.

**Corpus queries:** none. pbench's `struct-key-join`
(`SELECT f_id, d_id FROM fact JOIN dim ON f_kstruct = d_kstruct`) was written for this ticket and
could not land: the planner panics on a Struct column before it refuses, which is
[#255](complete-coverage.md#t255). The query arrives with that ticket's fix.

<a id="t246"></a>
### #246 (LIKE column pattern) a `LIKE` whose pattern is a column is refused on the device
`expr.cpp:873-877` takes a `LIKE` pattern only as a literal. A join condition that matches one
side's strings against the other side's patterns — a nested loop, since it has no key — is
refused on the device; the cpu answers. The same holds for any `LIKE` over two columns.

cuDF's `strings::like` takes a scalar pattern; a column of patterns needs a per-row arm (one
`like` per distinct pattern, scattered back, or a regex per row).

**Corpus queries:** none in tpch or tpcds. pbench's `like-column-pattern`
(`SELECT d_id, t_id FROM dim JOIN tiny ON d_name LIKE t_pat`) is the shape. Since join-backend its
cpu cells answer as DuckDB at all five modes, and its device cells were run, measured: each refuses
at the nested loop's probe, `LIKE pattern must be a string literal`. The row carries `246` alone.

<a id="t250"></a>
### #250 (nullable IN subquery) an `IN` subquery whose NULL answer is read is refused when its data holds NULLs
`x IN (S)` is three-valued: NULL when `x` is NULL and `S` is non-empty, or when `S` holds a NULL
and `x` matches nothing. Where a filter reads that NULL as a value — `IS [NOT] NULL`,
`IS [NOT] TRUE/FALSE/UNKNOWN`, a comparison (`(x IN S) = false`), `COALESCE`, a function argument
— the answer needs a NULL the engine cannot produce: DataFusion 45 plans the `IN` as a mark join,
and a mark is never NULL (`join_type.rs:56-69`). The join-rewrite chain's `NOT IN` rule (design
§3.5) answers every `IN`/`NOT IN` reached through `AND`, `OR` and `NOT` alone (it puts the
predicate in negation normal form first), and refuses these forms at plan time, by name, wherever
the data says `x` or `y` can be NULL; where neither can, `IN` is two-valued and plans.

The fix is the three-valued rewrite: `CASE WHEN EXISTS (S AND y = x) THEN true WHEN (x IS NULL AND
EXISTS (S)) OR EXISTS (S AND y IS NULL) THEN NULL ELSE false END`, planned as two mark joins and a
count, or a nullable mark join type of our own.

**Corpus queries:** pbench's `in-is-null`
(`SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL`), at all five modes, since
join-backend's `NOT IN` rule landed. The rule runs before `decorrelate_predicate_subquery`, so the
mark join [#257](df-upgrade.md#t257) needs is never built, and the query meets this refusal by name
instead of folding to an `EmptyExec`. The row carries `250` alone. An earlier note here said the
ticket could not be demonstrated until #257 was fixed; refusing earlier in the pass answered it.
