# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain J (base: master)

The join rewrite, and what it stands on. Joins leave the recipe architecture for a C++ session
(built once, probed per batch, finished once) behind four new C symbols; every non-join node keeps
its recipes. Shared design: [`join-rewrite-design.md`](join-rewrite-design.md). Replaces chain A's
refcounted-tables, whose scatter half is task 5 and whose join half the session makes unnecessary.

### 1. [`duckdb-oracle.md`](duckdb-oracle.md) — closes [#235](../tickets/corpus-coverage.md#t235) — state: approved to build

Every corpus line names its `duckdb_oracle`; a comparison case per line against
`duckdb-result.txt`, over `mini.result.txt` and the device's new `gpu-result.txt`; a fingerprint
for sections over the 256 KB cap. First, so every cell a later task enables meets DuckDB.

### 2. [`stale-cells.md`](stale-cells.md) — closes no ticket — state: approved to build

The 16 device cells four tpch rows keep off under closed tickets (#183, #187), never run since:
run, enabled or ticketed, tags struck.

### 3. [`pbench.md`](pbench.md) — closes [#227](../tickets/corpus-coverage.md#t227) — state: approved to build

A third dataset, sf1 only, committed: NULL keys on both sides, `NOT IN` over NULLs, every
hashable key type, skew, empty sides. One query per demonstrable ticket of the chain, cells off;
each later task turns its own on. Folds in #227: both engines check declared non-nullability.

### 4. [`repartition-keys.md`](repartition-keys.md) — closes [#201](../tickets/corpus-coverage.md#t201), [#206](../tickets/corpus-coverage.md#t206), [#240](../tickets/corpus-coverage.md#t240), [#95](../tickets/corpus-coverage.md#t95), [#189](../tickets/corpus-coverage.md#t189) — state: approved to build

The murmur gate onto the production lane rule first; then float, boolean, timestamp and decimal
keys on the device (decimals hashed as 16 bytes on both engines), and the rollup's grouping id
out of the shuffle. #243's pins.

### 5. [`refcounted-scatter.md`](refcounted-scatter.md) — closes [#145](../tickets/corpus-coverage.md#t145), [#197](../tickets/corpus-coverage.md#t197) — state: approved to build

`TableResult` takes its final shape, one owner per column; a scatter's partitions share them. Peak
3× → 2× during the partition, 1× after. Files the accounting ticket on landing.

### 6. [`exit-copies.md`](exit-copies.md) — closes [#154](../tickets/corpus-coverage.md#t154) (outside `join.cpp`) — state: approved to build

The 11 operator exit copies outside `join.cpp`, `expr.cpp`'s `ColumnRef` copy the costliest.

### 7. [`join-session-cpp.md`](join-session-cpp.md) — closes no ticket on its own (the C++ half of #136, #153, #160, #215, #63; #154's `join.cpp` sites) — state: approved to build

`CudfJoin`, the four symbols, `join.cpp` rewritten per the design's §3; a gtest matrix. Nothing
calls it until task 8.

### 8. [`join-backend.md`](join-backend.md) — closes [#155](../tickets/joins.md#t155), [#152](../tickets/joins.md#t152), [#173](../tickets/joins.md#t173), [#212](../tickets/joins.md#t212), [#159](../tickets/joins.md#t159), [#59](../tickets/joins.md#t59), [#80](../tickets/joins.md#t80), [#137](../tickets/joins.md#t137), [#220](../tickets/joins.md#t220), [#190](../tickets/joins.md#t190), [#207](../tickets/joins.md#t207), [#208](../tickets/joins.md#t208), and, with task 7's C++, [#136](../tickets/joins.md#t136), [#153](../tickets/joins.md#t153), [#160](../tickets/joins.md#t160), [#215](../tickets/joins.md#t215), [#63](../tickets/joins.md#t63), [#154](../tickets/corpus-coverage.md#t154) — state: approved to build

Planner, wire, both executors and the driver onto the session; every join refusal lifted but #250's off-spine IN; the
`NOT IN` rewrite; the corpus and pbench cells turned on, compared with the committed estimate.

### 9. [`verify-26.02.md`](verify-26.02.md) — closes no ticket — state: approved to build

Every tier on shad-gpu's cuDF 26.02 environment, fixes, and the tpch sf40 benchmark of the cells
the chain turned on, against 25.02; the evidence #244 waits on.
