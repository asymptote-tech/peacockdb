# A one-row relation crosses into the join cluster

Kind: production

The prototype's optimizer (`scripts/exec_model/optimizer/`) reorders a cluster of joins with
DPhyp, and a `GpuCrossJoin` walls a cluster (`optimizer.md`, "Walls"). DPhyp gets only edges with
keys, so a relation crossed in above a cluster stays where the plan put it, however large the
side it multiplies.

tpcds q23 shows the cost (#320). `best_ss_customer` crosses the one-row `max_store_sales` onto
the 2.75M rows of `store_sales ⋈ customer`, twice since the CTE is inlined twice: 88 MB of cross
join per copy at tp1. At tp4 the 4-lane join output is also merged onto one lane for it, since
the translator puts both sides of a cross join on one lane (`planner/translator/nodes.rs`).
DuckDB crosses the same relation onto `customer`, 100k rows, before the hash join with
store_sales. The estimate is about 60 MB less per copy, q23's final ratio from 2.73 to about 2.55.

A relation of at most one row reorders freely among inner joins. Crossed in anywhere, it adds
its columns to every row, or empties the result wherever it sits. This task lets such a relation
into the cluster, so DPhyp places it, and rebuilds the cross join where DPhyp put it. The
optimizer is judged on CPU, by the prototype's renders and the final ratio. The pieces are the
ones a nested-loop join with a one-row side extends (#321) and the port to Rust carries over.

## Which relations hold at most one row

One predicate over a plan node, `at_most_one_row`, used by every step below:

- an aggregate with no group-by that emits the final result. A per-lane partial aggregate with no
  group-by is one row per lane, not one (tpcds q23 at tp4: `GpuAggregate`, `lanes=4`, below a
  `GpuMergePartitions`);
- a node with `fetch` 0 or 1; `GpuEmpty`;
- a filter, a project, a cross join of two such relations, or wiring (`_WIRING`, a merge) over
  one.

Nothing else counts, whatever its estimate says. A one-row relation's rows are the plan's own
estimate of its root, as for any relation.

## Into the cluster

- **The wall.** A `GpuCrossJoin` with a side for which `at_most_one_row` holds joins the cluster of
  inner joins beside it, above or below. That side becomes a relation of the cluster, and the other
  side is collected as a hash join's side is. A cluster needs a hash join: cross joins of one-row
  relations alone (tpcds q28, q61, q88, q90) form none, and their plans do not change.
- **Inner joins only.** Where the cluster holds a non-inner join, the cross join stays a wall, as
  today. Inside a non-inner join's kept or preserved side it would turn the unmatched rows'
  NULLs into its values.
- **The edge.** A one-row relation `r` has an edge to every other relation of the cluster: no
  keys, no residual, selectivity 1. Its estimate is `rows(S ∪ {r}) = rows(S) × rows(r)`, and its
  cost is the existing one, `rows(S ∪ {r}) × width(needed(S ∪ {r}))`. C_out then attaches `r`
  where the rows that carry its columns are fewest. The edge type carries an optional residual
  and stays without one here: #321 fills it.
- **Orientation.** A join that only one-row edges cross builds its one-row side; between two
  one-row relations DPhyp's left side builds, as on any tie.
- **Disassembly.** A join with a key edge across it is a hash join, the one-row edges ignored.
  A join with only one-row edges across it is rebuilt as a `GpuCrossJoin` wired as the translator
  wires one: the build one batch on one lane, the probe merged onto one lane, the output on one
  lane and hashed on nothing. Above it the existing rules shuffle it onto a join's keys or back
  onto the root's hash. A one-row relation does not change a join's lanes: a hash join runs on
  the lanes the plan's join over its other relations had, and the cluster's root comes back onto
  the plan root's lanes (tpcds q23 at tp4: the hash join on four, the root merged onto one).

Every other rule — dynamic filters, a replan's clusters, the names `MJ<n>` — takes a cluster
with one-row relations as it takes any. A dynamic filter q23 gets today still applies where its
build and key survive.

## Done when

1. `at_most_one_row` holds for each form above and fails for a per-lane partial aggregate, an
   aggregate with a group-by, `fetch` 2, a scan and a hash join.
2. q23's two `best_ss_customer` clusters hold `max_store_sales` as a relation, and DPhyp crosses
   it onto `customer` at every mode. A cluster with a non-inner join beside a one-row cross join
   keeps the wall. tpcds q28, q61, q88 and q90 are unchanged.
3. The rebuilt plan runs in the prototype and every answer equals DuckDB's, at tp1 and tp4.
4. `run.py` over the whole corpus at all five modes writes the goldens, the `.optimizer.txt`,
   `.optimizer.md`, `.plandiff.txt` and `.costs.txt` files and `cost_report.html`. Every plan that
   moves other than q23's is explained in the detail file. q23's optimized cost and final ratio
   per mode are stated beside the estimate above, and any gap is explained.
5. `optimizer.md` and `design.md` describe the one-row rule, the edge, its cost and the rebuilt
   cross join, so the port can follow them without the code. #101's q23 paragraph carries the
   measured figures. #320 is archived.
6. Every prototype tier is green: cheap and dataset in CI, corpus by its workflow or by hand.

## Scope

| path | change |
|---|---|
| `scripts/exec_model/optimizer/` (`multijoin.py`, `join_order.py`, `disassembly.py`, and others only where a cluster with one-row relations needs them) | the predicate, the wall, the edge, its estimate and cost, orientation, the rebuilt cross join |
| `scripts/exec_model/tests/optimizer/` | the tests for each |
| `scripts/exec_model/testdata/goldens/` | the corpus outputs, made again |
| `scripts/exec_model/optimizer.md`, `scripts/exec_model/design.md` | the one-row rule |
| `llm-wiki/tickets/optimizer.md`, `llm-wiki/tickets.md`, `llm-wiki/archive/archived-tickets.md` | #101's q23 figures; #320 archived |

Component-level API: `multijoin.at_most_one_row(node) -> bool`; `Edge` with no keys and no
residual is a one-row edge. Nothing in the engine changes: `peacockdb-core/`, `cpp/`,
`peacockdb-dphyp/`, the wire format.

## Constraints

- The corpus runs on CPU. Nothing here measures the optimizer on the device.
- No test is weakened, and every answer equals DuckDB's.
- A plan that moves and costs more than before is a finding in the detail file, not a kept
  change, unless it is explained.

## Not in this task

A nested-loop join with a one-row side (#321); a cross join of two relations of more than one
row; a one-row cross join beside a non-inner join; the engine's cross join on more than one lane;
the optimizer in Rust.
