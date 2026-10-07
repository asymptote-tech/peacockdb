# peacockdb-dphyp — design

The cheapest bushy join tree over a hypergraph of relations, by DPhyp (Moerkotte & Neumann,
"Dynamic Programming Strikes Back", SIGMOD 2008). The crate holds no statistics and no notion of
build or probe: the caller prices sets of relations, the crate enumerates every way to join them
along the edges and returns the cheapest tree. No dependencies, so it builds in seconds; it is a
`cdylib` for the Python prototype (`scripts/exec_model/optimizer/dphyp.py`) and an `rlib` for a
Rust caller, which nothing is yet.

    src/lib.rs        the Rust API: `solve`, `JoinTree`, `Solved`, `Unsolved`
    src/ffi.rs        the C ABI: `dphyp_solve`
    src/enumerate.rs  the enumeration, as the paper's four procedures
    src/tests.rs      against a brute-force DP and the paper's pair counts

## Inputs

- **Relations** are `0..n`, 1 ≤ `n` ≤ 64. A set of them is a `RelSet` (`u64`), bit `r` for
  relation `r`.
- **Edges** are `(left, right)` pairs of relation masks, identified by their index in the slice.
  An edge is simple where each side has one bit and a hyperedge where either has more; it joins
  a set holding all of `left` to one holding all of `right`. Direction carries no meaning: each
  edge is stored both ways. An edge is malformed where a side is empty, the two sides overlap,
  or a bit lies outside `0..n`. Two edges between the same sides are allowed.
- **The cost callback**, `set_cost(set) -> f64`, is the cost of the join that produces `set`.
  The crate adds it up: a relation costs 0, and a tree costs Σ `set_cost` over its joins, which
  is C_out when the caller answers with a set's output size. Its contract:
  - it is called only for a set that is the union of a connected pair DPhyp reached, so never for
    one relation, never for a disconnected set, and at most once per set within a call — the
    answer is cached, so the callback needs no memo of its own;
  - it is called synchronously, during `solve`, and never after it returns;
  - it should return a finite number. Plans are compared with a strict `<`, so of two plans of
    equal cost the first found stays, and a NaN never replaces a plan nor is replaced by one;
  - it cannot fail. The C ABI has no error channel, and no returned cost stops the enumeration
    (a NaN is kept like any other), so a caller whose pricing can fail holds the failure itself
    and discards the result.
- **The pair budget**, `max_pairs`: the connected pairs DPhyp may enumerate. Enumeration stops
  as soon as the count passes it, so exactly `max_pairs` pairs still solve: a chain of three
  relations has 4 pairs, solves at a budget of 4 and is exhausted at 3.

## Outputs

`solve` returns `Result<Solved, Unsolved>`.

- `Solved.tree` is a `JoinTree`: `Rel(r)`, or `Join { left, right, edge }`. `edge` is the index
  of one edge between the two sides — the first in input order with one side within `left` and
  the other within `right`. Other edges between the same sides exist and are the caller's to
  find. `left` and `right` are not build and probe; a caller orients the tree itself.
- `Solved.cost` is Σ `set_cost` over the tree's joins; `Solved.pairs` the connected pairs
  enumerated.
- `Unsolved`, checked in this order:
  - `RelationCount(n)` — `n` is 0 or above 64;
  - `Edge(i)` — edge `i`, the first malformed one;
  - `BudgetExhausted` — more pairs than `max_pairs`;
  - `Disconnected` — no edge path joins every relation, so there is no tree without a cross
    product, which DPhyp never makes.

## The C ABI

```c
typedef double (*SetCost)(uint64_t set, void *ctx);

int32_t dphyp_solve(uint32_t n_relations,
                    const uint64_t *edge_left, const uint64_t *edge_right, uint32_t n_edges,
                    SetCost set_cost, void *ctx, uint32_t max_pairs,
                    int32_t *out_tree, uint32_t out_capacity, uint32_t *out_len);
```

- Edges are two parallel arrays of `n_edges` masks each; with `n_edges` = 0 the pointers are not
  read. `ctx` is passed to every `set_cost` call untouched.
- The tree is written to `out_tree` in postfix, one `int32` per node: `r ≥ 0` is relation `r`,
  `-1 - e` is a join on edge `e` of the two trees written just before it, left then right. A tree
  over `n` relations is `2n − 1` values, and `*out_len` is set to that. Reading it back is a
  stack machine: push a relation; at a join pop the right tree, then the left.
- `max_pairs` is 32-bit here; the Rust API takes 64.
- Return codes:

  | code | meaning | `out_tree`, `out_len` |
  |---|---|---|
  | 0 | solved | written |
  | 1 | `BudgetExhausted` | untouched |
  | 2 | `Disconnected` | untouched |
  | -1 | `RelationCount`: `n_relations` outside `1..=64` | untouched |
  | -2 | `Edge`: a malformed edge (its index is not returned) | untouched |
  | -3 | `out_capacity` below `2n − 1` | untouched |

  The capacity is checked first, for any `n_relations` ≥ 1, so 65 relations with a capacity
  below 129 return -3, not -1.
- Safety: the arrays are as long as the counts say, `out_len` points to one `uint32`, and
  `set_cost` is safe to call with `ctx` while the call runs. The crate allocates and frees its
  own memory; the caller owns every buffer.

For `chain(3)` with costs by popcount the call returns `0 1 2 -2 -1`, `(0 ⋈₀ (1 ⋈₁ 2))`, after 4
pairs; the left-deep tree ties with it, and the first found is kept. The callback is asked
`{1,2}`, `{0,1}` and `{0,1,2}`, each once, though `{0,1,2}` is reached by two pairs.

## The algorithm

Dynamic programming over connected subgraphs: the best plan of a set is the cheapest join of two
disjoint connected sets that an edge links, both already planned. What DPhyp adds is the order of
enumeration. Every *csg–cmp pair* — a connected subgraph and a connected complement linked to it
by an edge — is emitted exactly once, and nothing else is: no pair that needs a cross product,
and no pair whose halves are not yet planned.

- Relations are ordered by index. For each relation `r`, from the highest down, the csg `{r}` is
  emitted and grown only into relations above `r` (`enumerate_csg_rec`, excluding `0..=r`).
- For each csg, its complements are seeded from its neighbours above its lowest relation and grown
  the same way (`emit_csg`, `enumerate_cmp_rec`). That ordering is what makes each pair appear
  once rather than twice.
- The neighbourhood N(S, X) takes, for each edge leaving `S` into relations outside `S ∪ X`, the
  lowest relation of its far side as its representative. A hyperedge is entered through that
  representative, and the set it builds is used only once it is connected.
- A set has a plan exactly when it is connected, so the plan table is also the connectivity test.
  Subsets are generated smallest first, so a subset is always planned before a superset.
- `emit_csg_cmp` counts the pair, asks `set_cost` for the union once, and keeps the cheaper of the
  union's current plan and this split.

The work is the number of csg–cmp pairs, which depends on the graph's shape, not on `2^n`:
(n³ − n)/6 for a chain, (n − 1)·2^(n−2) for a star, (n³ − 2n² + n)/2 for a cycle, and
(3ⁿ − 2^(n+1) + 1)/2 for a clique. A budget exists because the clique and the star grow
exponentially. On simple graphs DPhyp enumerates exactly DPccp's pairs; hyperedges are what it
adds.

## Tests

`cargo test -p peacockdb-dphyp`, six tests in `src/tests.rs`, run by `pipeline.yml`'s cost-report
job.

- **The optimum.** Against a brute-force DP that tries every split of every set, smallest sets
  first: chains, stars and cliques of 2 to 6 relations and cycles of 3 to 6, 20 cost seeds each.
  The cost must be the brute force's, the tree's own recomputed cost the reported one, and the
  tree must cover every relation.
- **Hyperedges.** Two pairs joined only by `{0,1}–{2,3}` must join as those two pairs on edge 2;
  a chain plus `{0,2}–{3}` must match the brute force.
- **One pair each.** The pairs enumerated for chains, stars and cliques of 2 to 12 relations and
  cycles of 3 to 12 are the closed forms above (Moerkotte & Neumann 2006, "Analysis of two
  existing and one new dynamic programming algorithm").
- **The budget.** A 14-clique exhausts a budget of 10,000; a 14-chain (455 pairs) does not.
- **Refusals.** 0 and 65 relations, an edge outside `0..n`, overlapping sides, a disconnected
  graph; one relation solves to `Rel(0)`.
- **The C ABI.** The postfix tree read back as a stack machine, the callback counted through
  `ctx`, and a capacity of `2n − 2` refused with -3.

The Python side of the ABI is `scripts/exec_model/tests/optimizer/test_dphyp.py`, in the
prototype's cheap tier, a cost that raises among its cases.

## How the prototype calls it

`scripts/exec_model/optimizer/dphyp.py` loads the library `PEACOCK_DPHYP_LIB` names, once, and
refuses without it, naming the build — a skipped test would read as a passing one. `solve` packs
the edges into two `uint64` arrays, wraps the cost function as a `CFUNCTYPE(c_double, c_uint64,
c_void_p)` callback with a null `ctx`, sizes the output at `2n − 1`, clamps `max_pairs` to 32
bits, and turns a non-zero code into `Unsolved(reason)`: `budget`, `disconnected`, `relation
count`, `edge` or `capacity`. ctypes would swallow an exception raised in the callback and hand
the crate an unset value (0 in practice), so the wrapper holds the first one, answers every later
set 0 without pricing it, and raises it once `dphyp_solve` returns: a `StatsError` while pricing
stops the optimizer. The postfix comes back as nested `(left, right)` tuples, the prototype's
tree.

`optimizer/join_order.ordered` makes one call per join cluster:

- the relations are the cluster's, the edges its key edges only — each between two relations,
  since a hash join needs a key; residuals are placed on joins afterwards;
- the cost of a set is its C_out term in bytes (`SetEstimates.cost`), and every set asked is
  recorded for the optimizer's report;
- the budget is `MAX_PAIRS`, 10,000, DuckDB's; on `budget` the cluster keeps the plan's order,
  and any other refusal is raised;
- the tree's sides are then oriented by `join_order.orient` and the joins rebuilt by
  `optimizer/disassembly.py`, which finds every edge between two sides itself.

The library is built with `cargo build --release -p peacockdb-dphyp` and found at
`<target dir>/release/libpeacockdb_dphyp.so`. It links against the host's glibc, so it is built
on the host that loads it.
