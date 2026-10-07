
# Complete coverage

Tickets for MVP SQL functionality milestone

<a id="t195"></a>
### #195 — the corpus is numeric-aggregate heavy, and six shapes have no query at all
Measured off `tp1-single.plans.txt` over the 61 enabled queries and the four largest held
back: node counts run 33 to 267 while distinct node kinds run 6 to 12, median 9. More corpus
tests scale, not surface, so each shape below wants a hand-written query over the existing
tables, no new dataset, plus the engine work it needs.

- `ORDER BY … LIMIT n OFFSET m`: every corpus limit has `skip=0`, so both lowerings' offset
  half is synthetic-only; shape it away from [#166](#t166)'s two droppers.
- `min`/`max` over a string or date column: zero uses, and the merge is a string reduce.
- a join on a nullable key: [#59](#t59), [#80](#t80) and [#137](#t137) rest on there being none.
- a shuffle keyed on a decimal: not a query but [#95](#t95)'s kernel work, and the murmur3
  conformance gate extended to cover it.
- two `DISTINCT` args over different expressions: [#144](#t144) has no refusal of its own, and
  `count_distinct` marks queries this mode handles, so a grep for one finds the wrong two.
- a wide `SELECT DISTINCT`: dedup whose state is the whole row, the compaction worst case.

<a id="t161"></a>
### #161 — aggregate shapes the planner refuses: FILTER, and functions with no decomposition
Two refusals in the aggregate arm. A `FILTER (WHERE …)` clause has no lowering, and an
aggregate function outside the decomposition registry is refused by name.

Only the second is reachable: `SELECT median(v) FROM tiny` is refused by name, while
`sum(v) FILTER (WHERE v > 0)` does not parse in DataFusion 45 at all, so that refusal is
constructor-only until the parser gains the clause. The FILTER form would lower to a CASE
inside the aggregate argument and needs no new node; the registry gap is per function and each
wants its merge aggregator stated. Neither shape appears in either benchmark, which is why
they are refusals rather than work.


<a id="t144"></a>
### #144 — multiple DISTINCT arguments need a gid-multiplying expand
**Priority: low** — no query in either benchmark has this shape.

`count(DISTINCT a), count(DISTINCT b)` over different expressions is the one distinct shape the
lowering cannot express.

Single-distinct lowers to grouping on the distinct argument, and non-distinct companions ride
along because Σ over the inner groups recovers each total. Two distinct arguments break it: one
grouping can dedup `a` or `b`, not both, and grouping by `(a, b)` dedups neither. The standard
fix is Spark's `RewriteDistinctAggregates` — an expand multiplies each row into one per distinct
argument tagged with a group id, the inner aggregate groups by `(keys, gid, args)`, and the
outer computes each count from the rows carrying its own gid. That needs a new row-multiplying
node, which is why it is a ticket rather than a planner tweak. Not [#65](corpus-coverage.md#t65), whose gid is the
ROLLUP/CUBE `__grouping_id` — the two would coexist as separate columns. Until it lands the
planner refuses the shape at plan time.

<a id="t243"></a>
### #243 — the cpu treats -0.0 and 0.0, and NaNs of different bits, as different keys
A float join or group key equates `-0.0` with `0.0` and every NaN with every other NaN on the
device and in DuckDB, and does not on the cpu, so the two engines answer a float-keyed join or
`GROUP BY` differently wherever those values occur. At the tp4 modes the device can also disagree
with itself.

Measured on 2026-10-07 over keys `0.0, -0.0, NaN, -NaN, NULL, 1.0` (scratch probes, parquet
written by DuckDB):

| engine | `-0.0 = 0.0` | `NaN = NaN` | `NaN = -NaN` | groups of the six |
|---|---|---|---|---|
| DuckDB 1.5.4 | yes | yes | yes | 4 |
| DataFusion 45, 1 and 4 partitions, Partitioned and CollectLeft | no | yes (same bits) | no | 6 |
| cuDF 25.02 and 26.02 (`inner_join`, `hash_join`, `distinct_hash_join`, `groupby`) | yes | yes | yes | 4 |

DataFusion compares and hashes floats by their bits; cuDF and DuckDB by value, with NaNs equal.
The lane rule adds a third behaviour: comet's hasher, the cpu's lane rule and the one the device
kernel must match, maps `-0.0` to `0` but hashes a NaN by its raw bits
(`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:78-105`). So at tp4 `NaN` and `-NaN`
land in different lanes and stop matching or grouping on the device, while tp1 merges them
(inferred from the hasher's code, not run).

Spark avoids all three by normalizing float keys before a join or an aggregate
(`NormalizeFloatingNumbers`: `-0.0` to `0.0`, every NaN to one canonical NaN), so its hash only
ever sees canonical values. The fix here is the same: the planner normalizes every float join and
group key under the key, on both engines, and the lane rule then never sees `-0.0` or a
non-canonical NaN.

**Corpus queries:** none in tpch or tpcds, whose float columns hold no `-0.0` or NaN. pbench's
`float64-key-join`, `float64-key-group` and `float32-key-group` show it (`tasks/pbench.md`); they
land as commented-out `corpus_query!` lines naming this ticket. Pins: `bug_` cases in the operator
harness for a float-keyed join and a float-keyed aggregate, cpu against device (`repartition-keys`).

