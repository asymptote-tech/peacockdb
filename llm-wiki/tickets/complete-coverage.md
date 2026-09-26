
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
node, which is why it is a ticket rather than a planner tweak. Not [#65](#t65), whose gid is the
ROLLUP/CUBE `__grouping_id` — the two would coexist as separate columns. Until it lands the
planner refuses the shape at plan time.
