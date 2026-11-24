# one-row-cross-join — detail

## Run log

- 2026-11-23 to 2026-11-24: the code committed in five steps, `e44877f4` (`at_most_one_row`) to
  `595849d0` (the reports name a one-row edge and a cross join).
- 2026-11-24, 11:21 MSK: `test_cardinality_corpus.py`: 3 passed, its golden unchanged; q23's
  estimates did not move.
- 2026-11-24, 11:21 to 15:44 MSK: `run.py --jobs 1` over tpch and tpcds at all five modes, then
  `cost_report.py`. 650 runs in 15805.8 s (4 h 23 min), 0 failed. The goldens under
  `scripts/exec_model/testdata/goldens/` were written again; 51 files changed.
- 2026-11-24, 15:45 to 18:01 MSK: the corpus tier of the engine tests at tp4-single, each file
  whole (below, "The corpus tier").
- 2026-11-24, 20:16 MSK: q23 alone at tp1-single in `test_engine_optimized.py` and
  `test_engine_reassembled.py`: 1 passed, 0 failed in each.
- 2026-11-24, 20:17 MSK: `test_ordered_corpus.py` on the tree with the new goldens: 2 passed,
  0 failed.

## q23

### The plan

`best_ss_customer` crosses the one-row `max_store_sales` onto `store_sales ⋈ customer`. The CTE is
inlined twice, once under the catalog_sales branch and once under the web_sales branch. Both copies
moved the same way at every mode: DPhyp chose `((r0 r1) r2)`, `max_store_sales` crossed onto
`customer`, and the hash join with `store_sales` above it.

- **tp1.** A `GpuCrossJoin` of `max_store_sales` and `customer`, 100,000 rows of 16 B, is the hash
  join's build. The hash join projects `tpcds_cmax` with its other columns.
- **tp4.** The cross join runs on one lane, as the translator wires one. Its output is shuffled on
  `c_customer_sk` and coalesced per lane as the build of the hash join, which keeps its four lanes.
  A new `GpuMergePartitions` above the hash join brings the cluster's root back onto the plan
  root's one lane. Before, the merge sat below the cross join, on the hash join's output.

In `.optimizer.txt` each copy's cluster holds three relations, `agg(MJ<n>)` among them, with two
lines `edge r0-r1: one row crossed in, keeps every row` and `edge r0-r2: …`. The order still reads
`join r0 x r1: 100000.0 rows`, since a join line names no join kind. `.optimizer.md` shows the plan's
order as `[agg(MJ4)] cross join ([customer#3] join store_sales#3)` and DPhyp's as
`[[agg(MJ4)] cross join customer#3] join store_sales#3`. By the same estimates, DPhyp's order
prices at 142,124,594 B of C_out and the plan's order at 232,952,674 B; both figures are the new
cluster's. Before the change the cluster held no cross join and priced at 93,253,080 B.

The `MJ<n>` names of q23 swapped in pairs: MJ3 with MJ4 and MJ7 with MJ8. The names follow the
plan in pre-order. The cluster that now begins at the cross join is met before the cluster inside
`max_store_sales`, so it takes the lower number. Where MJ1 and MJ5 read the result, the relation
`agg([agg(MJ3)] cross join MJ4) (f)` is now `agg(MJ3) (f)`. The `fired` line counts
`reordered=6` where it counted 4: q23 has eight clusters before and after, and the two that now
hold the one-row relation are reordered. Probes, flips, replans and refusals are unchanged.

### Cost against the estimate

The spec estimated about 60 MB less per copy and a final ratio from 2.73 to about 2.55. In the table,
costs are `peacockdb_cost` of the `optimized` line of `<mode>.costs.txt`, in bytes. The ratio is
optimized over planned, as `run.py` prints it. The projected final ratio is the engine's
`-mini.cost.txt` figure at that mode, times the ratio, over DuckDB's 817,991,805 B.

| mode | optimized before | optimized after | less | ratio | projected final ratio |
|---|---|---|---|---|---|
| tp1-single | 1,953,242,506 | 1,693,945,354 | 259,297,152 | 0.71 → 0.62 | 2.73 → 2.37 |
| tp1-rowgroup | 1,964,352,840 | 1,705,055,688 | 259,297,152 | 0.71 → 0.62 | 2.75 → 2.39 |
| tp4-single | 3,349,497,877 | 3,091,800,725 | 257,697,152 | 0.67 → 0.62 | 4.63 → 4.27 |
| tp4-rowgroup | 3,353,427,325 | 3,095,730,173 | 257,697,152 | 0.67 → 0.62 | 4.63 → 4.28 |
| tp4-sized | 3,349,416,605 | 3,091,719,453 | 257,697,152 | 0.67 → 0.62 | 4.63 → 4.27 |

The report's final ratio is the least projected one, tp1-single's: 2.73 to 2.37. The planned cost
did not change at any mode.

Each copy costs once. The catalog_sales copy runs in a run that a replan stopped, which keeps its
result, `agg(MJ3) (f)`, as a memory source. The final plan reads it back inside `[…] join m6`
(`m9` at tp1, `m10` at tp4). The web_sales copy runs in the final plan. The `stopped` line and
the final plan each fell by one copy's saving.

### Why the saving is twice the estimate

Per copy at tp1, from the `.cpu.txt` nodes, in bytes:

| node | before | after | change |
|---|---|---|---|
| `GpuCrossJoin` | 88,032,384 | 1,600,000 | −86,432,384 |
| `GpuHashJoin` customer ⋈ store_sales, output | 66,024,288 | 88,032,384 | +22,008,096 |
| `GpuProject` between the hash join and the cross join | 66,024,288 | gone | −66,024,288 |
| `GpuCoalesceAllBatches`, the hash join's build | 800,000 | 1,600,000 | +800,000 |
| sum | | | −129,648,576 |

Two copies make 259,297,152, the tp1 figure. At tp4 the build is also shuffled: `GpuEmitPartitions`
grows by 800,000 B per copy, so a copy saves 128,848,576 B. The merge onto one lane grows from
66,024,288 B to 88,032,384 B, and a `GpuMergePartitions` costs nothing in the cost model
(`cuda_forward_bytes 0.0`).

The estimate's 60 MB is close to the first, second and fourth rows: 88 MB of cross join less
1.6 MB of new cross join, 22 MB of wider hash join output and 0.8 MB of wider build. It leaves
out the `GpuProject` of 66 MB. That project only reordered the hash join's columns for the cross
join. Once the cross join is inside the cluster, disassembly folds it into the hash join's
projection. At tp4 the spec's q23 paragraph counts the 4-lane merge as a cost of the cross join.
The merge stays, at the cluster's root as the spec's "Disassembly" says, and a merge costs nothing
either way.

### Dynamic filters

The dynamic-filter block of q23's `.optimizer.txt` section, before and after, is the same at all
five modes, 25 lines each. Six filters apply on the plan as written: `catalog_sales` 12 → 1 row
groups, `web_sales` 6 → 1, and `store_sales#1`, `#2`, `#4` and `#5` 24 → 16 each. Their builds,
`p0` to `p5` (`date_dim`), run first as before. None of them reads the relations that moved:
`store_sales#3` and `#6` carry no candidate. `p2` and `p5` sit in the cluster inside
`max_store_sales`, which DPhyp orders as before: `[p2] join ([customer#2] join store_sales#2)`
becomes `[customer#2] join ([p2] join store_sales#2)` both before and after the change. Every
filter applies after the move.

## Stale goldens on master

The goldens on master before this change were written on 2026-10-20 (`f61c456b`, authored then) and
carried through a rebase, committed 2026-11-04, without being made again. Commits that reached
master after they were written change the corpus's inputs or the prototype. So the new run moves
these sections too, none of them from the one-row rule.

| section | bench | what moved | commit |
|---|---|---|---|
| `distinct-functions`, `rollup-distinct` | tpch | new sections: the queries joined the engine's goldens | `1634e17c` distinct-companions |
| `rollup-grouping` | tpch | new section | `258dde0b` grouping-id |
| `rollup-stddev` | tpch | new section | `3325712c` aggregate-arms |
| `global-stddev` | tpch | new section | `d14e9237` welford-device |
| `nested-limits`, `scan-limit` | tpch | cost up: the scan reads whole row groups | `fbeb216c` limits, `50af35a5` |
| `q28` | tpcds | `skipped: refused … count(DISTINCT …) (#62)` became a run, `plan unchanged` | `1634e17c` distinct-companions |

- **The new tpch sections.** Each commit added its query to the engine's `<mode>.plans.txt` and
  `-mini.cpu.txt`, which `run.py` reads. Each new section is `nothing fired` and `plan unchanged`,
  optimized equal to planned, at every mode.
- **nested-limits and scan-limit.** `fbeb216c` takes the limit off the scan: the engine's plans no
  longer print `limit=` on `GpuLoadParquet`. In nested-limits a `GpuLimit` above each scan carries
  it; in scan-limit the `GpuUnload: skip=0, fetch=10` at the root does. `50af35a5` removed the
  prototype's scan limit to match. The scan now reads its row group whole: scan-limit's
  `storage_read_bytes` went from 1,776 to 21,228,704, nested-limits' from 229 to 983,045. Planned
  and optimized are still equal.
- **Their engine figures.** `fbeb216c` also changed the engine's `-mini.cost.txt` totals. Before
  it, nested-limits had 1,626,484 at tp1-single, tp4-single and tp4-sized and 999,884 at the two
  rowgroup modes; scan-limit had 1,048,882,882 at tp4-single and tp4-sized, 21,476,312 at
  tp4-rowgroup and none at tp1. After it, nested-limits has 1,000,118 and scan-limit 21,476,312 at
  every mode. With the five figures equal, the report names tp1-single as the best mode of each,
  the first on a tie.
- **q28.** Before `1634e17c` the engine refused q28's `count(DISTINCT …)` (#62). It plans now: a
  tree of `GpuCrossJoin`s over one-row aggregates, with no hash join. Such cross joins form no
  cluster, so nothing fires and the plan is unchanged, at every mode. q28 is now a run of the spec's
  rule that one-row relations alone make no cluster. q61, q88 and q90 did not move.

`cost_report.html` changed its figures only in these rows, q23's and the summary line. Many other
rows changed only their `#L` links into `.optimizer.txt`, whose line numbers shifted. In the
summary, tpch went from 43 queries and 215 runs to 48 and 240. tpcds went from 405 runs to 410 and
from 90 skipped to 85. Its runs where optimized costs the same as planned went from 9 to 14: q28 at
five modes.

## The .cpu.txt diffs

The `.cpu.txt` files changed in these sections only, at every mode:

- tpcds q23: the cross join, the hash join and the merge above, as in "The plan".
- tpcds q28: a run where a skip was.
- tpch `distinct-functions`, `rollup-distinct`, `rollup-grouping`, `rollup-stddev`, `global-stddev`:
  new runs.
- tpch `nested-limits` and `scan-limit`: the scan's rows and bytes are those of a whole row group
  (`part` 122,880 rows where 28 were). nested-limits gains a `GpuLimit` over each limited scan;
  scan-limit's limit is its `GpuUnload`'s `fetch=10`.

The `.optimizer.txt`, `.optimizer.md`, `.plandiff.txt` and `.costs.txt` files changed in the same
sections, except that nested-limits and scan-limit kept their `.optimizer.txt`, `.optimizer.md`
and `.plandiff.txt` sections, since nothing fires in either.

## The corpus tier

Each file ran whole at its default mode, tp4-single, on 2026-11-24 from 15:45 to 18:01 MSK. Every
answer equals DuckDB's.

| file | passed | failed | wall |
|---|---|---|---|
| `tests/plans/test_engine_answers.py` | 130 | 0 | 2814 s |
| `tests/optimizer/test_engine_dynamic_filters.py` | 73 | 0 | 870 s |
| `tests/optimizer/test_engine_reassembled.py` | 111 | 0 | 2709 s |
| `tests/optimizer/test_engine_optimized.py` | 111 | 0 | 1740 s |

q23 passed in all four. q28 is in `test_engine_answers.py` only: the other three take a query
only where it has a join cluster or a dynamic-filter candidate.

At tp1, q23 alone ran with `PCK_MODE=tp1-single` on 2026-11-24 at 20:16 MSK, and its answer
equals DuckDB's:

| file | test | passed | failed |
|---|---|---|---|
| `tests/optimizer/test_engine_optimized.py` | `test_tpcds_q23` | 1 | 0 |
| `tests/optimizer/test_engine_reassembled.py` | `test_tpcds_q23` | 1 | 0 |

The optimizer moved no other plan at any mode. So the other queries' tp1 answers rest on
`run.py`'s check that the optimized plan answers as the planned plan does: 650 runs, no failure.
