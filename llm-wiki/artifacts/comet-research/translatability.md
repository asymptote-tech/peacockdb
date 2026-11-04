# Are Comet's plans translatable into peacockdb's?

## Summary
- Every native operator in the 104 final plans (TPC-H q1-q22, the 82 TPC-DS queries) maps onto peacockdb's node set: 2,647 supported, 2,374 adaptable, **0 missing**. No window operator appears, because the 82 TPC-DS queries are the ones without one.
- Expressions and aggregates: 26,944 instances, 93.2% supported, 6.7% adaptable, 20 missing (0.07%). The missing ones sit in 5 queries: a runtime bloom filter (TPC-H q2, q20), a struct (TPC-DS q9), a JVM codegen UDF (q24, q99). Spark already runs 31 operators of TPC-DS q9, q10, q14, q35 itself.
- So 96 of 104 queries could run every native operator on the GPU; the other 8 keep 2-29% of their operators on the CPU (split.md's rule, `plans/eligible.tsv`).
- "Adaptable" is the real work: Spark's ANSI arithmetic and decimal typing (1,625 expressions carry `eval_mode: ANSI`), Spark's aggregate buffer layout, sort-merge joins run as hash joins, and three new sources (Parquet from an object store per task, Arrow from the JVM, Comet shuffle blocks).
- The DataFusion gap (peacockdb 45, Comet 55.2) costs little if the translator reads Comet's protobuf, not a DataFusion plan: the two meet only as Arrow over the C Data interface. Reading DataFusion 55's plan instead would mean a ten-major-version upgrade of the planner.

## Method
- Input: the final executed plans dumped in Task 4 Step 2 (`plans/{tpch,tpcds}/`, Comet 1.2.0 @ab46419f on Spark 4.1.3, AQE on, sf40, local tmpfs run; 2,322 files). One protobuf-text `<q>-NN-<Op>.txt` per distinct native block (reused blocks deduplicated by hash), Spark's formatted plan `<q>.plan.txt`, and `fallbacks.tsv`. The S3 runs' plans match these (Task 4: TPC-H identical, TPC-DS differs only in AQE op counts and an expression id).
- `scripts/plan_census.py plans` parses each block (no `protoc` on dev, so a small text-format parser, tested in `scripts/tests/test_plan_census.py`), walks `Operator.children`, and counts every `op_struct`, every `Expr` variant inside it (`ScalarFunc` by its `func` name) and every `AggExpr` under `agg_exprs`. proto3 text omits enum defaults, so a missing `join_type` is `Inner`, a missing `mode` is `Partial`.
- Output: `plans/census.tsv` (dataset, query, kind, name, class, count), `plans/eligible.tsv` (per query), `plans/columns.tsv` (the Parquet columns each query scans, from the formatted plan's scan `Output`).
- The vocabulary it is classed against: peacockdb's node set (`architecture.md` "The node set"; `peacockdb-core/src/plan/mod.rs` `AggFunc`), the expressions its translator accepts (`planner/translator/expr.rs`: column, literal, binary, not, is (not) null, negative, cast, like, case, in-list, scalar function) and the scalar functions the device runs (`cpp/src/expr.cpp`: `date_part`, `substring`, `abs`, `round`, `lower`, `upper`, `concat`, `coalesce`), all at this branch's base.
- Not counted: Comet's JVM-side nodes, which never reach the protobuf (exchanges, broadcast exchanges, `CometTakeOrderedAndProject`, `CometUnion`, columnar-to-row). They stay JVM-side in any design (split.md).

**Classes.** *Supported*: peacockdb has a node or kernel with the same semantics, so a translator maps it one to one. *Adaptable*: peacockdb has the capability but needs a new parameter, mode or source, or a Spark-exact rule it lacks. *Missing*: no node or kernel.

## Operators

| Comet operator | Count (H / DS) | Class | peacockdb counterpart, and what adapting means |
|---|--:|---|---|
| `projection` | 133 / 956 | supported | `GpuProject` |
| `filter` | 94 / 574 | supported | `GpuFilter` |
| `sort` | 101 / 261 | supported | `GpuSort` + `GpuAccumulateBatchesAndSort`; a `fetch` is a top-N |
| `hash_join` Inner, LeftOuter, LeftSemi, LeftAnti | 33 / 479 | supported | `GpuHashJoin`; every type in `plan/join.rs` |
| `hash_join` Existence, null-aware anti | 1 / 2 | adaptable | Existence is `LeftMark`; null-aware anti is the NOT IN rule (`planner/not_in.rs`) |
| `broadcast_nested_loop_join` | 0 / 16 | supported | `GpuNestedLoopJoin` / `GpuCrossJoin`, build from host input |
| `native_scan` | 86 / 528 | adaptable | `GpuLoadParquet` reads local files mapped at plan time; here each task brings its own file list and reads through `object_store` |
| `scan` (Arrow from the JVM) | 34 / 514 | adaptable | new host-batch source: broadcast build sides and Spark-operator outputs arrive over the C Data interface |
| `shuffle_scan` | 134 / 466 | adaptable | new host-batch source: Comet's compressed Arrow IPC blocks, decoded on the CPU, then copied in |
| `hash_agg` (Partial, Final, PartialMerge) | 70 / 410 | adaptable | the aggregate sequence exists; its state columns must take Spark's buffer layout so a partial can cross a Spark exchange (a decimal `sum` carries an `isEmpty` column, `avg` is sum and count) |
| `sort_merge_join` | 31 / 92 | adaptable | run as `GpuHashJoin` on the same keys; the sort below it becomes dead weight the translator drops |
| `expand` | 0 / 6 | adaptable | grouping sets ride on peacockdb's aggregate (`grouping_sets`), not on a separate row-multiplying node |
| `window`, `window_group_limit`, `explode`, writers | 0 / 0 | missing | none in these plans; window is #143 |

H = TPC-H's 22 queries, DS = TPC-DS's 82; counts are operator instances over all distinct blocks. Totals: TPC-H 717 (361 supported, 356 adaptable), TPC-DS 4,304 (2,286 supported, 2,018 adaptable). Join types over all three join operators: Inner 593, LeftSemi 31, LeftOuter 18, LeftAnti 9, Existence 2, FullOuter 1 (`census.tsv` `join` rows).

## Expressions and aggregates

| Expression | Instances | Class | Note |
|---|--:|---|---|
| `bound`, `unbound`, `literal` | 16,909 | supported | column and literal |
| `is_not_null`, `is_null`, `and`, `or`, `not`, comparisons, `in` | 6,024 | supported | `in` lowers to ORs (`expand_in_list`) |
| `sort_order`, `caseWhen`, `like` | 1,199 | supported | |
| `func:` `substring`, `round`, `concat` | 38 | supported | device kernels in `cpp/src/expr.cpp` |
| `check_overflow` | 417 | adaptable | Spark's decimal precision check: NULL or an error on overflow |
| `func:unscaled_value`, `func:make_decimal` | 531 | adaptable | Spark's sum-of-small-decimals rewrite: reinterpret a decimal as its unscaled int64 and back; a cast pair on the device |
| `cast` | 296 | adaptable | Spark's cast table and ANSI errors, not DataFusion's |
| `multiply`, `subtract`, `divide`, `add` | 529 | adaptable | Spark's decimal result typing and ANSI overflow errors (1,625 nodes carry `eval_mode: ANSI`) |
| `subquery` | 14 | adaptable | a scalar subquery's value, bound at run time as a literal |
| `func:starts_with`, `ends_with`, `contains`, `datepart` | 15 | adaptable | LIKE patterns and `date_part` |
| `func:xxhash64`, `bloom_filter_might_contain` | 8 | missing | Spark's runtime bloom filter, TPC-H q2 and q20 |
| `create_named_struct` | 5 | missing | no struct type; TPC-DS q9 |
| `jvm_scalar_udf` | 3 | missing | Comet's codegen UDF calls back into the JVM; TPC-DS q24, q99 |

Aggregates: `sum` 676, `count` 128, `avg` 127, `stddev` 10, `max` 9, `min` 2 (all supported: `AggFunc`), and `bloomFilterAgg` 4 (missing; it builds q2's and q20's runtime filter).

Totals by class: TPC-H 2,782 supported, 166 adaptable, 12 missing (2,960 including 90 aggregates); TPC-DS 22,340, 1,636, 8 (23,984 including 866 aggregates).

## Per query

TPC-H, from `census.tsv` and `eligible.tsv`. Ops and expressions as supported / adaptable / missing; "GPU share" is the fraction of the query's operators (native plus Spark's) that split.md's rule puts on the GPU.

| Query | Blocks | Ops | Expressions | GPU share | | Query | Blocks | Ops | Expressions | GPU share |
|---|--:|---|---|--:|---|---|--:|---|---|--:|
| q1 | 3 | 4/5/0 | 79/24/0 | 1.000 | | q12 | 6 | 9/10/0 | 125/0/0 | 1.000 |
| q2 | 15 | 33/28/0 | 217/3/6 | 0.934 | | q13 | 6 | 9/12/0 | 51/0/0 | 1.000 |
| q3 | 7 | 14/13/0 | 103/8/0 | 1.000 | | q14 | 5 | 8/9/0 | 70/22/0 | 1.000 |
| q4 | 6 | 10/10/0 | 58/0/0 | 1.000 | | q15 | 7 | 15/16/0 | 111/22/0 | 1.000 |
| q5 | 12 | 24/21/0 | 141/8/0 | 1.000 | | q16 | 8 | 10/16/0 | 111/2/0 | 1.000 |
| q6 | 2 | 3/4/0 | 58/4/0 | 1.000 | | q17 | 6 | 15/14/0 | 86/7/0 | 1.000 |
| q7 | 12 | 22/21/0 | 190/6/0 | 1.000 | | q18 | 11 | 33/29/0 | 155/0/0 | 1.000 |
| q8 | 15 | 30/26/0 | 196/8/0 | 1.000 | | q19 | 4 | 6/7/0 | 262/8/0 | 1.000 |
| q9 | 13 | 23/22/0 | 139/12/0 | 1.000 | | q20 | 13 | 24/23/0 | 158/6/6 | 0.915 |
| q10 | 9 | 17/16/0 | 140/8/0 | 1.000 | | q21 | 14 | 25/25/0 | 149/0/0 | 1.000 |
| q11 | 7 | 17/15/0 | 79/16/0 | 1.000 | | q22 | 8 | 10/14/0 | 104/2/0 | 1.000 |

TPC-DS: 76 of 82 queries have a GPU share of 1.000. The rest:

| Query | Ops | Expressions | Spark ops | GPU share | What stays on the CPU |
|---|---|---|--:|--:|---|
| q9 | 21/21/0 | 211/40/5 | 1 | 0.860 | a struct-typed project (Spark's) and the 5 `create_named_struct` operators' ancestors |
| q10 | 19/17/0 | 104/0/0 | 13 | 0.735 | Spark runs an ExistenceJoin and a count merge (`fallbacks.tsv`) |
| q14 | 71/67/0 | 849/24/0 | 3 | 0.979 | Spark's decimal AVG at maximum precision |
| q24 | 42/42/0 | 359/11/2 | 0 | 0.929 | `jvm_scalar_udf` |
| q35 | 19/16/0 | 81/0/0 | 14 | 0.714 | as q10 |
| q99 | 16/12/0 | 214/16/1 | 0 | 0.964 | `jvm_scalar_udf` |

Every TPC-DS row, with counts, is in `plans/census.tsv`; the largest plans are q64 (183 ops, 1,186 expressions) and q66 (50 ops, 1,656 expressions).

## What the DataFusion gap costs

- **Versions.** peacockdb plans on DataFusion 45 with arrow-rs 54.2 and links `datafusion-comet-spark-expr` 0.6.0 for Spark's murmur3 (`peacockdb-core/Cargo.toml`). Comet 1.2.0 pins DataFusion 55.2.0 and arrow-rs 59.2.0 (comet-internals.md, "Versions and pin").
- **What crosses.** At the seam (comet-internals.md, "The GPU seam", option A) the GPU side receives a prost-decoded `spark_operator::Operator` and returns record batches. Neither is a DataFusion type. So a translator from Comet's protobuf to peacockdb's plan IR never touches DataFusion 55, and the Arrow batches cross by the C Data interface, which is stable across arrow-rs versions.
- **What it costs then (estimate).** A protobuf-to-IR translator of about the size of today's DataFusion-to-IR one (2,129 lines, inventory.md "Planner"), which it replaces for this product; two arrow-rs versions linked into one native library (build time and binary size, no type sharing); and Spark-exact kernels written for the device where today the CPU path borrows DataFusion's. The DataFusion-45 frontend is not used at all on this path.
- **The alternative costs more (estimate).** Translating DataFusion 55's physical plan would mean moving the planner, the CPU backend and the oracle harness ten major versions (45 to 55.2) for no gain: Comet's plan is fixed before DataFusion sees it.
- **The protobuf is not a stable API** (comet-internals.md: reserved fields, per-release changes; JVM and native built together). The translator must be pinned to one Comet release and re-checked on each.

## Evidence against
- 46% of operators and the arithmetic in every query are "adaptable", and the adaptation is Spark-exact semantics: ANSI overflow errors, decimal typing, buffer layouts, cast rules. These are the costliest kind of GPU work to get bit-exact.
- The plans are per task and small (one split or one shuffle partition per task, comet-internals.md), so many of these blocks would run on a few MB each; the count says nothing about whether running them on the GPU pays (split.md).
- The 82-query TPC-DS list was chosen because peacockdb plans those 82 on DataFusion 45 (inventory.md), so it excludes the window queries and is biased toward translatability. Real ETL uses windows, `explode`, structs and UDFs (batching.md section 5).
- Runtime bloom filters (TPC-H q2, q20) are on by default in Spark 4; a GPU subtree containing one falls back above it.

## Follow-up experiments
1. Dump the 17 TPC-DS queries left out (window queries, q27, q72) and a sample of real customer Spark jobs; re-run `plan_census.py` to see how often `window`, `explode` and UDFs appear.
2. Prototype the protobuf-to-IR translator on TPC-H q1 and q6 against Comet 1.2.0, checking results bit-for-bit against Comet under ANSI mode.
3. Per-task block sizes: patch `CometExecIterator` (comet-internals.md follow-up) to log bytes per task, to see how many blocks are too small to send to a GPU.
4. Re-run the census on Comet's next release to measure protobuf churn between releases.

## Sources
- Plans: `plans/{tpch,tpcds}/` from Task 4 (2026-11-04, cpu128, Comet 1.2.0 @ab46419f, Spark 4.1.3); census `scripts/plan_census.py` and its outputs `plans/census.tsv`, `plans/eligible.tsv`, `plans/columns.tsv`.
- Comet protobuf: `native/proto/src/proto/operator.proto` (`Operator` :32-84, `HashJoin`, `JoinType` :1159-1167, `AggregateMode` :1057-1061) and `expr.proto` (`Expr` :27-99, `AggExpr` :143-172) @ab46419fcc99bcd34aa4d89f7b545d5d51694a88, https://github.com/apache/datafusion-comet/tree/ab46419fcc99bcd34aa4d89f7b545d5d51694a88 (accessed 2026-11-04).
- peacockdb at this branch's base: `llm-wiki/architecture.md` ("The node set", "The aggregate sequence", "Joins"), `peacockdb-core/src/plan/mod.rs`, `peacockdb-core/src/planner/translator/expr.rs`, `cpp/src/expr.cpp`, `flatbuffers/gpu_plan.fbs`, `peacockdb-core/Cargo.toml`.
- `comet-internals.md` (this folder): versions, seam, protobuf stability. `llm-wiki/artifacts/product-research/inventory.md`: translator size, the 82/99 TPC-DS coverage.
