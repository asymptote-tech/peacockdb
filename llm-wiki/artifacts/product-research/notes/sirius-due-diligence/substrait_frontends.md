# Sirius plan input formats: Substrait and non-DuckDB frontends

Conventions. Code is cited as path:line at:
- Sirius `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (2026-10-28). Permalink base S = https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619
- Sirius's Substrait consumer submodule `substrait` = fork `sirius-db/duckdb-substrait-extension` at `922489d3c8c77140b189ad3847554de141378b04` (2026-10-12, "Port the embedded Substrait reader to DuckDB v2", Matthijs Brobbel). Base F = https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04
- DataFusion 45 producer: `datafusion-substrait` 45.0.0 crate source, `src/logical_plan/producer.rs`. Base D = https://docs.rs/crate/datafusion-substrait/45.0.0/source/src/logical_plan/producer.rs
GitHub issues were read with `gh` on 2026-10-28; every web claim was accessed 2026-10-28.

## Q1. What does Sirius accept as a plan, and through which entry point?

### Takeaway
Sirius's only non-SQL plan input is a **serialized Substrait `Plan` protobuf (binary, not JSON) holding a logical relational plan**. It is passed through a C++ library API, `sirius::ffi::Context::execute_substrait(plan_bytes, ArrowArrayStream*)` or `sirius::ffi::Fragment::build(plan_bytes)`, which has Rust bindings (`sirius-sys`, `sirius`). No SQL table function such as `gpu_execution_substrait` exists today: the DuckDB extension's `gpu_execution(...)` takes a SQL string. The consumer is pinned to Substrait proto **v0.89.0**.

### Cited Findings
- Public API: `Context::execute_substrait(const std::string& plan, std::uintptr_t out_stream_addr)` "Executes a serialized Substrait plan on the GPU, writing the results to the Arrow C Data Interface stream" — include/sirius/ffi.hpp:67-73 — [S/include/sirius/ffi.hpp#L67-L73](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/include/sirius/ffi.hpp#L67-L73)
- Multi-fragment API: `Fragment::build(const std::string& substrait_plan)` "Lower and plan `substrait_plan` against the declared streams. Creates a view `sirius_stream_<id>` for each declared input stream". Input stream columns are declared with **DuckDB type-name strings** (`BIGINT`, `DECIMAL(15,2)`), and outputs are broadcast or hash-partitioned — ffi.hpp:96-146, 211-214 — [S/include/sirius/ffi.hpp#L96-L146](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/include/sirius/ffi.hpp#L96-L146)
- The header's own description: "DuckDB is used only to lower a Substrait plan to a DuckDB `LogicalOperator` (the translation step) and to own the catalog. Execution runs directly on the Sirius engine, not through DuckDB's query pipeline." — ffi.hpp:45-52 — [S/include/sirius/ffi.hpp#L45-L52](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/include/sirius/ffi.hpp#L45-L52)
- Binary only: `duckdb::SubstraitToDuckDB transformer(conn.context, substrait_plan, /*json=*/false);` — src/sirius_ffi.cpp:123 — [S/src/sirius_ffi.cpp#L123](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L123). The consumer can also parse JSON (`JsonStringToMessage`), but the FFI never uses that path — from_substrait.cpp:85-96 — [F/src/from_substrait.cpp#L85-L96](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L85-L96)
- Only the from-Substrait direction is compiled into the Sirius library: "NOT the substrait SQL functions, to_substrait, or a second loadable extension" — cmake/sirius-substrait.cmake:1-7, included at CMakeLists.txt:52 — [S/cmake/sirius-substrait.cmake](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/cmake/sirius-substrait.cmake)
- The DuckDB-extension SQL entry point `gpu_execution` binds `input.inputs[0].ToString()` as a **SQL query string**. The old Substrait include is commented out (`// #include "from_substrait.hpp"`) — src/sirius_extension.cpp:98, 478-505 — [S/src/sirius_extension.cpp#L478-L505](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_extension.cpp#L478-L505). A grep of the tree found no `gpu_execution_substrait` or `gpu_processing_substrait` symbol at this SHA.
- Rust: `SiriusContext::execute_substrait(&mut self, plan: &[u8])`, where "`plan` is the protobuf-encoded `substrait::Plan`; its reads must be resolvable by the embedded DuckDB used for lowering (e.g. `local_files`…)" — rust/crates/sirius/src/lib.rs:63-74 — [S/rust/crates/sirius/src/lib.rs#L63-L74](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/rust/crates/sirius/src/lib.rs#L63-L74). The bindings came from sirius-db/sirius #835 — rust/README.md:1-5 — [S/rust/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/rust/README.md)
- Linking: the Rust crates link `libsirius.so`. "Until a real `libsirius.so` exists, `build.rs` symlinks the DuckDB extension (`sirius.duckdb_extension`) to it" — rust/README.md "Linkage" — [S/rust/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/rust/README.md). Standalone-library status is tracked in open issue #1728 "Sirius as a standalone library" (mbrobbel, 2026-09-08) — [issue #1728](https://github.com/sirius-db/sirius/issues/1728)
- Substrait version. The fork's `scripts/update_substrait.py:8` pins `GITHUB_TAG = "d61a9403…" # v0.89.0`, and commit 718db03 (2026-05-12, Andrew Coleman/IBM) "upgrade substrait to v0.89.0" lists the "change of URI to URN; changed representation of grouping expressions; changed time/timestamp to precision_timestamp" — [F/scripts/update_substrait.py](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/scripts/update_substrait.py). The vendored generated protos that Sirius compiles (`third_party/substrait/...pb.cc`, sirius-substrait.cmake:13-18) follow v0.89: `AggregateRel.Grouping` has only `expression_references`, and `FetchRel.count`/`offset` are `[deprecated = true]` beside `count_expr` (third_party/substrait/substrait/algebra.pb.h:6288-6332, 6517). Inconsistency: the fork's `substrait` git submodule still points at substrait-io commit 29844f1b = tag v0.78.0, and `to_substrait.cpp:2477` stamps minor version 78. Neither is compiled into Sirius.
- Fork lineage: `sirius-db/duckdb-substrait-extension` is a GitHub fork of `substrait-io/duckdb-substrait-extension`, which is still active (pushed 2026-10-20) — `gh api repos/sirius-db/duckdb-substrait-extension` — [github.com/sirius-db/duckdb-substrait-extension](https://github.com/sirius-db/duckdb-substrait-extension)
- Physical vs logical: the paper calls Substrait "a unified IR for physical and logical representations of query plans" (§2.2) and says Sirius works by "consuming a universal query plan format (e.g., Substrait)" (§3.1) — [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3). In code, physical relations (HashJoinRel, MergeJoinRel, NestedLoopJoinRel, ExchangeRel, ExpandRel) are rejected (see Q2), so only the logical subset is accepted.
- Read sources resolvable in the FFI context: the embedded DuckDB is in-memory (`DuckDB(nullptr)`) with only `CoreFunctionsExtension` and `ParquetExtension` loaded ("Substrait lowering uses core functions and resolves local_files reads to parquet_scan") — src/sirius_ffi.cpp:186-189 — [S/src/sirius_ffi.cpp#L186-L189](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L186-L189)
- Sirius-specific extension to LocalFiles: before the DuckDB consumer runs, Sirius parses the plan bytes itself to pull byte-ranged parquet splits (`start`/`length` on LocalFiles items), because "DuckDB's consumer and parquet binding cannot carry them" — src/sirius_ffi.cpp:112-121 and src/planner/substrait_scan_ranges.{hpp,cpp} — [S/src/sirius_ffi.cpp#L112-L121](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L112-L121). Added by #2043 (Alexander Ocsa, 2026-10-19).
- Marketing claim: "It plugs into existing databases via the standard Substrait query format, requiring no query rewrites… Sirius currently supports DuckDB and Starrocks (coming soon)" — docs/README.md:13 — [S/docs/README.md#L13](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L13)

### Inferences
- The contract for an external producer is "a Substrait plan the **DuckDB Substrait consumer** (Sirius fork, v2 branch) can lower", not "a spec-conformant Substrait plan". Everything that consumer drops or rejects is unavailable.
- A producer has two table sources it can use without help: `local_files` parquet (whole file or byte range), and `named_table` reads of `sirius_stream_<id>` views that `Fragment` creates for streamed inputs. A `named_table` naming an arbitrary catalog table fails in the FFI's empty in-memory catalog (estimate: not tested, but follows from sirius_ffi.cpp:186-189 and from_substrait.cpp:803-822). `iceberg_table` reads lower to `iceberg_scan`, which the FFI's DuckDB does not load (estimate: likely fails at bind).

### Gaps
- No public spec documents the accepted Substrait subset. The subset is defined only by the consumer's code.
- I could not find when or if an older `gpu_*_substrait` SQL function existed: the clone is shallow (depth 200, back to 2026-10-02) and the README at tag `stable` was not retrievable via the API.

## Q2. Which Substrait relations, expressions, functions and types are supported or rejected?

### Takeaway
Supported relations: Read (NamedTable, LocalFiles-parquet, VirtualTable, IcebergTable), Filter, Project (with emit), Aggregate (incl. grouping sets, DISTINCT measures), Sort, Fetch (but **deprecated `count`/`offset` fields only**), Join (INNER/LEFT/RIGHT/OUTER/LEFT_SEMI/RIGHT_SEMI/LEFT_SINGLE/LEFT_MARK; **no ANTI**), Cross, Set (UNION ALL, MINUS_PRIMARY, INTERSECTION_PRIMARY; 2 inputs only), ConsistentPartitionWindowRel, Write (CTAS/INSERT/DELETE), Reference (CTE).

Rejected: ExtensionLeaf/Single/Multi, Exchange, Expand, HashJoin, MergeJoin, NestedLoopJoin, Update, Ddl, subquery expressions, window-function expressions, and type-argument functions.

Functions are matched **by bare name only**: the extension URI/URN and the `:signature` suffix are dropped, and the name is passed to DuckDB's binder after a 15-entry rename table. Any later GPU-side gap is a second filter (Sirius physical planning has no CPU fallback on the FFI path).

### Cited Findings
**Relations** (from_substrait.cpp, TransformOp switch at 1275-1306)
- Dispatched: kJoin, kCross, kFetch, kFilter, kProject, kAggregate, kRead, kSort, kWindow, kSet, kWrite, kReference. `default: throw NotImplementedException("Unsupported relation type %s")` — [F/src/from_substrait.cpp#L1275-L1306](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L1275-L1306). Not dispatched, so rejected: ExtensionSingle/Multi/Leaf, HashJoin, MergeJoin, NestedLoopJoin, Exchange, Expand, Update, Ddl. `GetCommon` additionally throws for Write/Update/Ddl — 597-643.
- Plan shape: "The last relation is the root. Others could be CTEs" (referenced by `ReferenceRel.subtree_ordinal`) — 1374-1386, 1267-1273.
- ReadRel (801-1035):
  - `named_table` uses **only `names(0)`** with `DEFAULT_SCHEMA`, trying a table and then a view (803-822).
  - `local_files` accepts **parquet only**, with `uri_file` or `uri_path` (not `uri_path_glob`/`uri_folder`), and calls `parquet_scan(..., binary_as_string=false, hive_partitioning=false)`. Other formats throw "Unsupported type of local file" (823-856).
  - `virtual_table` accepts literal `values`, `expressions`, or empty with base_schema, which becomes a `1=0` filter (857-923).
  - `iceberg_table` uses `iceberg_scan` with snapshot id/timestamp (924-952).
  - Anything else throws (953-955).
  - If `projection` is absent, `base_schema` is aligned to the physical columns **by name**; a missing name throws "schema mismatch" (957-996). `ReadRel.filter` becomes a FilterRelation. `best_effort_filter` is ignored (not referenced).
  - [F/src/from_substrait.cpp#L801-L1035](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L801-L1035)
- Join types: INNER, LEFT, RIGHT, LEFT_SINGLE, LEFT_SEMI, RIGHT_SEMI, LEFT_MARK, OUTER. **LEFT_ANTI, RIGHT_ANTI, RIGHT_SINGLE and RIGHT_MARK throw** "Unsupported join type" — 539-575 — [F/src/from_substrait.cpp#L539-L575](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L539-L575). The StarRocks translator works around this: "left/right anti as outer join + `is_null` filter, null-aware left anti as mark join + `not`" — experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs:38 — [S/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs)
- FetchRel reads only the deprecated fields: `idx_t limit = slimit.count() == -1 ? … : slimit.count(); idx_t offset = slimit.offset();` — 585-591 — [F/src/from_substrait.cpp#L585-L591](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L585-L591). Open bug #1963 (aocsa, 2026-10-01): "Current Substrait producers write `count_expr` and `offset_expr` instead… every `LIMIT n` is lowered as `LIMIT 0` and DuckDB's optimizer folds the plan to `EMPTY_RESULT`… A Substrait plan over a regular table scan would return 0 rows with no error." The upstream fix `3a83368` ("fix Substrait 0.98.0 breaking API changes", #245) is on the fork's `main` but is **not an ancestor of the pinned 922489d** (checked with `git merge-base --is-ancestor`) — [issue #1963](https://github.com/sirius-db/sirius/issues/1963)
- AggregateRel (708-786):
  - Reads top-level `grouping_expressions` plus `Grouping.expression_references`, and maps them to DuckDB grouping sets.
  - `AggregationInvocation DISTINCT` becomes a DISTINCT aggregate. `count` with no args becomes `count_star`. `grouping` becomes GROUPING_FUNCTION.
  - **`Measure.filter`, aggregate `sorts`, and `phase` are not read** (not referenced in 741-783, so they are silently dropped).
  - [F/src/from_substrait.cpp#L708-L786](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L708-L786)
- SetRel: UNION_ALL becomes UNION (all), MINUS_PRIMARY becomes EXCEPT, INTERSECTION_PRIMARY becomes INTERSECT. Other ops (UNION_DISTINCT, MINUS_MULTISET, INTERSECTION_MULTISET…) throw. More than 2 inputs throws "The amount of inputs (%d) is not supported" — 1186-1215 — [F/src/from_substrait.cpp#L1186-L1215](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L1186-L1215)
- Window: only `ConsistentPartitionWindowRel` (kWindow) is handled (1067-1184), with ROWS/RANGE bounds (~1135-1165). Window functions embedded as expressions are not handled (see expressions).

**Expressions** (TransformExpr 474-498)
- Supported: Literal, Selection (**direct struct field references only**; otherwise "Can only have direct struct references in selections", 202-206), ScalarFunction, IfThen, Cast, SingularOrList (IN), Nested (struct/list/map).
- `kSubquery` and everything else, including `WindowFunction`, `SwitchExpression`, `MultiOrList`, `Enum` and `DynamicParameter`, throw "Unsupported expression type" — 492-497 — [F/src/from_substrait.cpp#L474-L498](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L474-L498)
- Function arguments: value and enum args are accepted. Type args throw "Type arguments in Substrait expressions are not supported yet!" (222-226).

**Function resolution** (46-104, 208-305, 498-504)
- At parse time only `extensions[].extension_function().{function_anchor,name}` is read into `functions_map`. **Extension URIs/URNs are never consulted** (97-103).
- `RemoveExtension`/`RemapFunctionName` cut the name at the first `:` (dropping the compound signature) and apply one rename table: modulus→mod, std_dev→stddev, starts_with→prefix, ends_with→suffix, substring→substr, char_length→length, is_nan→isnan, is_finite→isfinite, is_infinite→isinf, like→`~~`, extract→date_part, bitwise_and→`&`, bitwise_or→`|`, bitwise_xor→xor, octet_length→strlen (46-51).
- Special-cased into DuckDB operator nodes: and, or, lt, lte, gt, gte, equal, not_equal (string constant turns into `!~~`), is_null, is_not_null, not, is_not_distinct_from, between, coalesce, extract (enum subfield). Any other name is passed to DuckDB's binder as `FunctionExpression(name)` (232-305). So "supported functions" = any DuckDB function whose name or remapped name matches.
- An anchor missing from extensions throws "Could not find aggregate function %s" (498-504).
- [F/src/from_substrait.cpp#L46-L104](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L46-L104)

**Types and literals**
- Types (325-409): bool, i8, i16, i32, i64, decimal, date, precision_time, varchar, string, binary, fp32, fp64, precision_timestamp (precisions mapped; unsupported ones throw), precision_timestamp_tz (**microsecond only**), list, map, struct, uuid, interval_day. Anything else throws "Substrait type not yet supported" (e.g. interval_year, interval_compound, fixed_char, fixed_binary, user-defined) — [F/src/from_substrait.cpp#L325-L409](https://github.com/sirius-db/duckdb-substrait-extension/blob/922489d3c8c77140b189ad3847554de141378b04/src/from_substrait.cpp#L325-L409)
- Literals (107-195): fp64, fp32, string, decimal (16-byte), boolean, i8, i32, i64, date, precision_time, interval_year_to_month, interval_day_to_second, varchar. **Not i16, precision_timestamp, binary, fixed_char, uuid, list/map/struct literals** ("literals of this type are not implemented"). A typed `null` literal becomes untyped `Value(SQLNULL)` (108-110). The Doris translator wraps every typed NULL in a cast to work around this (G-18).

**Lowering wrappers and Sirius-side filters**
- After the consumer builds a DuckDB `Relation`, Sirius runs DuckDB's `Planner` and then the DuckDB `Optimizer` with these disabled: IN_CLAUSE, COMPRESSED_MATERIALIZATION, STATISTICS_PROPAGATION, COLUMN_LIFETIME, LATE_MATERIALIZATION, plus `unsupported_optimizers` (PARTIAL_AGGREGATE_PUSHDOWN, GROUPING_SETS, AGGREGATE_REUSE, DISTINCT_AGGREGATE_REWRITE, SCALAR_FN_PUSHDOWN, …). The comment says these are "DuckDB optimizer rewrites this no-fallback FFI path cannot safely execute" — src/sirius_ffi.cpp:126-147, 200-226; src/planner/unsupported_optimizers.hpp:26-31 — [S/src/sirius_ffi.cpp#L200-L226](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L200-L226)
- Sirius-side coverage limits as stated by the Doris integrators (semantic gaps G-01..G-19):
  - LARGEINT is narrowed silently to 64 bits.
  - `DECIMAL(p<=4)` is stored as INT16 by DuckDB, which "has no cuDF carrier".
  - ARRAY/MAP/STRUCT: "Sirius only passes nested columns through".
  - DuckDB "re-derives expression types", so every projection must be cast back.
  - "The GPU's DOUBLE→DECIMAL cast truncates the last digit where DuckDB rounds".
  - `concat` NULL semantics differ.
  - experimental/doris/docs/semantics-gaps.md:9-27 — [S/experimental/doris/docs/semantics-gaps.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/docs/semantics-gaps.md)
- StarRocks translator doc: "Decimal arithmetic is **not exact**… evaluated in FP64 because the GPU expression and aggregate paths cannot consume decimal arithmetic; decimal slots of precision > 18 likewise map to FP64" — starrocks-plan-translator/src/lib.rs (crate docs, ~lines 66-71) — [S/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs)

### Inferences
- The accepted surface is DuckDB-dialect Substrait. Function names must be DuckDB names (or one of 15 remaps). Translators in-tree emit DuckDB names such as `year`, `month` and `day` rather than spec `extract` (starrocks expr_translator.rs:578). URIs/URNs are irrelevant to the consumer.
- Silently dropped fields (Measure.filter, aggregate phase, FetchRel.count_expr, best_effort_filter) are a correctness hazard for any external producer: these give **wrong results, not errors**.
- After the consumer step, the GPU planner is a second gate. Not all DuckDB logical operators or functions the consumer can produce run on GPU, and the FFI path has no CPU fallback ("no-fallback FFI path", sirius_ffi.cpp:200-201). Mapping GPU operator coverage was out of scope here.

### Gaps
- No machine-readable list of which DuckDB functions the GPU expression executor implements was gathered here; that belongs with GPU-operator coverage research.
- Runtime behavior for an aggregate whose `Grouping` uses the removed v0.7x field is inferred, not observed (see Q4).

## Q3. Is DuckDB still in the loop?

### Takeaway
Yes. The chain is **Substrait bytes → duckdb-substrait (fork) `SubstraitToDuckDB::TransformPlan()` → DuckDB `Relation` → DuckDB `Planner` (binder) → DuckDB `Optimizer` (with Sirius-incompatible rules disabled) → DuckDB `LogicalOperator` → Sirius physical plan generator → Sirius GPU operators**. DuckDB does catalog, binding, type derivation and logical optimization; it does not execute. A native Substrait → Sirius path is only an open proposal (#2192, on top of the #1750 physical plan builder).

### Cited Findings
- `lower_substrait` does `SubstraitToDuckDB … TransformPlan()`, then `Planner::CreatePlan(RelationStatement)`, then `Optimizer::Optimize`, then `ColumnBindingResolver` — src/sirius_ffi.cpp:106-147 — [S/src/sirius_ffi.cpp#L106-L147](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L106-L147)
- `streaming_fragment` takes a "Bound, optimized DuckDB logical plan from Substrait bytes, SQL, or similar" — src/exec/streaming_fragment.hpp:57 — [S/src/exec/streaming_fragment.hpp#L57](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/exec/streaming_fragment.hpp#L57). The physical planner dispatches on `duckdb::LogicalOperatorType::*` (src/planner/sirius_physical_plan_generator.cpp and sirius_plan_*.cpp).
- Maintainer statement (mbrobbel, #841, 2026-06-02): "Currently it's DuckDB plans. We use the DuckDB Substrait extension to convert Substrait (protobuf) bytes into DuckDB plans… Starrocks plan fragments -> Substrait…". The issue body says the goal is to convert StarRocks fragments "into Substrait plans, which we'll convert to DuckDB plans and then to Sirius plans" — [issue #841](https://github.com/sirius-db/sirius/issues/841)
- Open #2192 "Build a Substrait consumer" (mbrobbel, 2026-10-14): "Build our own Substrait consumer on top of #1750. This also helps with #545." — [issue #2192](https://github.com/sirius-db/sirius/issues/2192)
- Open #1750 "Add a physical plan and plan builder interface" (mbrobbel, 2026-09-10): "To support external front-ends we should expose a physical plan builder interface… lowest level interface… executed directly (no additional planning, validation or other optimization passes)… Interface must be independent of other dependencies/front-ends… Typed expressions and schemas" — [issue #1750](https://github.com/sirius-db/sirius/issues/1750)
- Open #545 "Tracking issue: decouple Sirius internal from DuckDB" (2026-04-02) and open #557 (Phase 8: move all DuckDB-facing code incl. plan generation into `src/duckdb_adapter/`; core engine with "zero DuckDB includes") — [issue #545](https://github.com/sirius-db/sirius/issues/545), [issue #557](https://github.com/sirius-db/sirius/issues/557)
- Done so far: #704 (closed 2026-06-05) "non-DuckDB frontend smoke test proof". It builds a `sirius::ast::node` expression tree by hand and runs it through `gpu_expression_executor`, which proves **expression-level** decoupling only — [issue #704](https://github.com/sirius-db/sirius/issues/704)
- #1807 (open, Exchange in Sirius): map StarRocks `DATA_STREAM_SINK` to Substrait `ExchangeRel`… "We need to rewrite the plan so it works with the DuckDB Substrait importer" — [issue #1807](https://github.com/sirius-db/sirius/issues/1807)
- Cost of the DuckDB detour: the Relation API "re-binds the whole subtree at every level… Q21 spent 10 s lowering against 4.6 s executing" before the parquet metadata cache was enabled — src/sirius_ffi.cpp:190-197 — [S/src/sirius_ffi.cpp#L190-L197](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_ffi.cpp#L190-L197)
- Paper framing: "DuckDB supports Substrait and has an extension framework, allowing us to integrate Sirius with zero modification to DuckDB's codebase" (§3.2.1) — [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3)

### Inferences
- A product on Sirius today inherits DuckDB's binder and optimizer semantics (type re-derivation, function overload resolution, decimal rules) between its planner and the GPU. A DataFusion-planned query is therefore re-planned by DuckDB's optimizer, which may reorder joins or choose build sides. `Fragment::declare_input_cardinality` exists "so the optimizer can pick a join's build side" (ffi.hpp:115-118).
- The roadmap (#1750, #2192, #557) points to a native physical-plan builder and Sirius-owned consumer. None of it was merged at b9e05ffc (all open), so its timeline is unknown.

### Gaps
- No timeline or design doc for #1750/#2192 beyond the issue bodies, which have no comments.

## Q4. Can a non-DuckDB producer drive Sirius? DataFusion compatibility

### Takeaway
Yes, and the in-tree evidence is real: **every in-tree Substrait producer that feeds Sirius is non-DuckDB, hand-written Rust** (substrait-rs 0.63/0.65), namely the Doris and StarRocks translators and the Rust/C++ FFI tests. None use DataFusion, Isthmus/Calcite, Ibis or Gluten. Those producers deliberately emit the DuckDB consumer's dialect. **DataFusion 45's stock producer has several concrete incompatibilities with the pinned consumer**: some give silent wrong results (LIMIT→0, DISTINCT grouping lost) and some are hard errors (anti joins, window expressions, subqueries, LIKE arity, named-table resolution). A DataFusion-based product would need its own Substrait emitter or a rewrite pass.

### Cited Findings
**Non-DuckDB producers in tree**
- Doris translator: crate `doris-plan-translator`, `TPipelineFragmentParams` → Substrait, using `substrait = "0.63.0"` and `substrait-explain = "0.9.0"` (experimental/doris/Cargo.toml:25-33). Emits URNs like `extension:io.substrait:functions_comparison` (crates/doris-plan-translator/src/lib.rs:75-86) — [S/experimental/doris/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/README.md)
- StarRocks translator: `substrait = { version = "0.65.0", features = ["protox"] }` (experimental/starrocks/Cargo.toml:32), with the same URN constants (starrocks-plan-translator/src/lib.rs:115-125) — [S/experimental/starrocks/Cargo.toml](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/starrocks/Cargo.toml)
- Rust crate test builds a `local_files` plan with `substrait = "0.65"` (rust/crates/sirius/Cargo.toml:25-26; src/lib.rs:160-204). The C++ test comment reads "Builds Substrait in the test because the FFI has no SQL helper" (test/cpp/exec/test_sirius_ffi_embedder.cpp:18) — [S/test/cpp/exec/test_sirius_ffi_embedder.cpp](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/test/cpp/exec/test_sirius_ffi_embedder.cpp)
- Doris CPU differential: translator plans are also run through upstream DuckDB's Substrait consumer at "pre-v2 reader at `a7e045b`", which "checks translator output against that consumer, not the v2 reader embedded in Sirius"; 22/22 TPC-H pass — experimental/doris/README.md:75-84 — [S/experimental/doris/README.md#L75-L84](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/README.md)
- Issue search on sirius-db/sirius for datafusion, gluten, calcite, isthmus and ibis returned no frontend-related issue. The one "datafusion" hit is #2020 (UNION on GPU), unrelated (gh search, 2026-10-28) — [issue #2020](https://github.com/sirius-db/sirius/issues/2020)

**DataFusion 45 producer vs pinned consumer (code-level comparison)**
- (a) **LIMIT. Silent wrong result.** DataFusion 45 emits `FetchRel` with `CountMode::CountExpr` / `OffsetMode::OffsetExpr` (producer.rs:695-714 for Limit; 740-753 for Sort with fetch) — [D producer.rs](https://docs.rs/crate/datafusion-substrait/45.0.0/source/src/logical_plan/producer.rs). The consumer reads only deprecated `count()`/`offset()`, so LIMIT n becomes LIMIT 0 (from_substrait.cpp:585-591; [issue #1963](https://github.com/sirius-db/sirius/issues/1963), still open; the fix is not in the pinned SHA).
- (b) **SELECT DISTINCT. Silent wrong result (estimate).** DataFusion 45 `from_distinct` emits `AggregateRel{grouping_expressions: [], groupings: [Grouping{grouping_expressions: <cols>, expression_references: []}]}` using the deprecated per-Grouping field (producer.rs:792-815). The consumer's v0.89 protos no longer have that field (only `expression_references`, algebra.pb.h:6517), so it is parsed as an unknown field and dropped, leaving an empty grouping set and no measures. Regular GROUP BY in DF45 uses the new form (producer.rs:1129-1141) and is fine.
- (c) **Anti joins. Hard error.** DF45 maps `JoinType::LeftAnti → join_rel::JoinType::LeftAnti` (producer.rs:1077-1080), and DataFusion decorrelates NOT EXISTS / NOT IN into LeftAnti. The consumer throws "Unsupported join type" (from_substrait.cpp:567-570). RightSemi/RightAnti are `not_impl` in DF45 itself (producer.rs:1081).
- (d) **Window functions. Hard error.** DF45 `from_window` emits a `ProjectRel` whose expressions are `WindowFunction` expressions (producer.rs:907-935). The consumer only understands `ConsistentPartitionWindowRel`, and `TransformExpr` throws "Unsupported expression type" for window_function (from_substrait.cpp:492-497).
- (e) **Subqueries. Hard error** if any remain after optimization: DF45 emits `RexType::Subquery` for IN-subquery (producer.rs:~1691), and the consumer rejects `kSubquery` (from_substrait.cpp:493).
- (f) **LIKE/ILIKE.** DF45 always emits 3 args (expr, pattern, escape-char literal, possibly NULL) under the names `like`/`ilike` (producer.rs:2030-2060). The consumer maps `like`→`~~` and passes all 3 args. DuckDB's `~~` is binary, so a bind error is likely (estimate). Independent corroboration: in the Substrait consumer-testing matrix, `string.like` passes DataFusion→DataFusion but fails DataFusion→DuckDB (see below).
- (g) **Function names.** DF45 registers DataFusion's own function names (`register_function(fun.name())`, producer.rs:1231, 1430; no URI, `extension_uris: vec![]`, 452). The consumer ignores URIs, so only names matter: they must match DuckDB names. Matrix failures DataFusion→DuckDB include `char_length`, `trim`, `is_nan`, `count_star`, `approx_distinct`.
- (h) **Reads.** DF45 emits every `TableScan` as `ReadRel::NamedTable{names: table_name.to_vec()}` plus `base_schema` and a `MaskExpression` projection (producer.rs:540-578). The consumer takes only `names(0)` (a qualified `schema.table` would resolve the schema name as a table) and looks it up in the FFI's empty in-memory catalog. A DataFusion integration must rewrite reads to `local_files` parquet items, or to the `sirius_stream_<id>` views of `Fragment`. Empty relations are emitted as empty `VirtualTable` with base_schema, which the consumer handles (from_substrait.cpp:885-915).
- (i) **Aggregate details.** DF45 emits `AggregateFunction` with `phase` and optional `Measure.filter` (FILTER clause) and ordering. The consumer ignores filter, sorts and phase, so `SUM(x) FILTER (WHERE …)` silently loses the filter (estimate; based on from_substrait.cpp:741-783 not reading them). Upstream DataFusion also has open #25100 "Substrait producer emits AGGREGATION_PHASE_UNSPECIFIED for every aggregate and window function" (2026-09-09) — [apache/datafusion#25100](https://github.com/apache/datafusion/issues/25100)
- (j) **Proto version skew.** DF45 depends on substrait-rs `0.52` (datafusion-substrait 45.0.0 Cargo.toml `[dependencies.substrait] version = "0.52"`). The consumer is at spec v0.89, so fields renamed or removed between them, like (b), are silently dropped by protobuf rather than rejected (estimate on which other fields are affected). DataFusion had its own Substrait spec-version bump issues, e.g. [apache/datafusion#10412](https://github.com/apache/datafusion/issues/10412) "Substrait integration doesn't recognize typed functions" (closed 2024-05-07).

**Third-party cross-producer evidence (dated, upstream DuckDB consumer, not Sirius's fork)**
- Substrait consumer-testing "Producer x Consumer x Function" CSV (101 function tests), last updated 2024-12-12:

  | Producer → consumer | Passing |
  |---|---|
  | duckdb→duckdb | 95 |
  | datafusion→duckdb | 66 |
  | datafusion→datafusion | 58 |
  | duckdb→datafusion | 53 |
  | isthmus→duckdb | 41 |
  | ibis→duckdb | 27 |
  | isthmus→datafusion | 1 |

  Functions passing DF→DF but failing DF→DuckDB: approx_distinct, count_star, factorial, is_nan, char_length, like, trim — [consumer_results.csv](https://raw.githubusercontent.com/substrait-io/consumer-testing/main/app/consumer_results.csv); matrix described in [SUPPORT_MATRIX.md](https://github.com/substrait-io/consumer-testing/blob/main/SUPPORT_MATRIX.md). The README notes CI passes "irrespective of whether or not consumers and consumers adhere to the Substrait spec" — [substrait-io/consumer-testing](https://github.com/substrait-io/consumer-testing)
- Other open cross-producer pain upstream: [apache/datafusion#14831](https://github.com/apache/datafusion/issues/14831) "substrait generated by Apache Calcite does not run in DataFusion" (open, 2025-02-22).

### Inferences
- Feasibility for peacockdb (DataFusion 45 planner): technically workable, but not by piping `datafusion_substrait::logical_plan::producer::to_substrait_plan` output straight into `execute_substrait`. Expect to write a custom emitter like Doris/StarRocks did: DuckDB function names, deprecated FetchRel fields, anti-join rewrites, parquet `local_files` reads, casts back to declared types, and avoiding windows and set ops beyond 2-input UNION ALL. The Doris translator's 19-item gap list (semantics-gaps.md) is a good template for the semantic checks needed.
- Because two failure classes are silent (LIMIT→0, dropped FILTER/DISTINCT grouping), any such integration needs a differential test harness against CPU results. Doris built one (`cpu-diff.sh`, `validate_tpch_results.py`).
- The plan reaching the GPU is DuckDB-optimized, not DataFusion-optimized. peacockdb's own physical decisions (join order, exchange placement) would not survive, except via `Fragment` boundaries and declared cardinalities.

### Gaps
- No public evidence of anyone running DataFusion-, Isthmus/Calcite-, Ibis- or Gluten/Velox-produced plans on Sirius was found (Sirius issues/PRs searched; web search found nothing).
- Items (b), (f), (i) are code-reading inferences, not executed. Running DF45 TPC-H plans through the fork's `from_substrait` on CPU (no GPU needed) would confirm them. That was out of scope (no builds).
- consumer-testing results are from Dec 2024 and the upstream DuckDB consumer. Applicability to the 2026 fork is approximate.

## Q5. Existing non-DuckDB frontends: Doris, StarRocks, others

### Takeaway
Both frontends are **Rust "pseudo-backend" processes**: they impersonate a Doris BE or a StarRocks CN, receive the FE's native Thrift plan fragments, translate them to Substrait in Rust, and call the Sirius FFI. Neither Doris nor StarRocks is modified, and neither emits Substrait itself.
- **StarRocks**: led by Sirius maintainers (Matthijs Brobbel, Alexander Ocsa), started 2026-05-28. It translates per fragment, with exchanges as `sirius_stream_<id>` reads, two-phase SUM, joins, and multi-CN byte-range splits.
- **Doris**: contributed by Apache Doris PMC member Mingyu Chen (`morningman`) in #1842, merged 2026-10-06. It stitches all fragments into one plan on one node; TPC-H 22/22 validate on GPU.

Both live under `experimental/` and are prototypes, not production. No Spark/Gluten, Presto/Velox, ClickHouse or DataFusion frontend exists in tree or issues.

### Cited Findings
**Doris** (`experimental/doris`)
- "A Rust process that joins an **unmodified, official Apache Doris FE** as a backend node and runs the plan fragments it receives on the embedded Sirius engine. The Doris side is configuration only". Pipeline: `exec_plan_fragment` with `TPipelineFragmentParamsList` (TCompact thrift) → `doris-plan-translator` → Substrait → `engine.rs`. Results come back over `fetch_data` (MySQL row encoding) — experimental/doris/README.md:1-20 — [S/experimental/doris/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/README.md)
- Pinned to the Doris FE release 4.1.4. The `doris/` submodule is used only for `gensrc/thrift` and `gensrc/proto` IDL (README.md:11, 26; submodule commit ad35a140).
- Status: "Today every query runs as one fused plan on one node; the next steps are executing the FE's fragments as engine fragments, then multi-node" (README.md:144-146). Single-plan stitching folds multi-phase aggregation (G-13/G-14). Refused: window functions (G-11), UNION/INTERSECT/EXCEPT (G-12), functions outside an allowlist (G-15/G-16), LARGEINT, DECIMAL256, nested types, and more — semantics-gaps.md.
- Commit history at b9e05ffc: introduced by `0b0c4e1` 2026-10-06, author "Mingyu Chen (Rayner)", "feat(experimental): Doris pseudo-BE — Sirius as an Apache Doris backend, TPC-H 22/22 on the GPU (#1842)". Later commits are maintenance (DuckDB v2 migration #2143, protox codegen #2344) (`git log -- experimental/doris`).
- Tracking issue #2025 (morningman, 2026-10-06, open):
  - TPC-H 22/22 validate "at SF0.1 on the merged head, and at SF1, SF10 and SF100 on an L40S before the rebase, where SF100 took 154.4 s with Doris's own BE and 40.6 s with this backend."
  - Limits: "One node only… One query at a time… `execute_substrait` has no interrupt hook… `local()` parquet only, read as whole files."
  - Plan: map `DATA_STREAM_SINK` to `ExchangeRel` and `EXCHANGE_NODE` to `ReadRel` via #1807, then multi-backend.
  - Maintainers answered that the backends stay on `ffi` APIs until the #1728 library API exists.
  - [issue #2025](https://github.com/sirius-db/sirius/issues/2025)
- History: #137 "[Doris] Progress on Apache Doris" was opened 2026-01-03 by dataroaring ("member of the Apache Doris PMC"). On 2026-06-10 Sirius (roaramburu) wrote: "we're deprioritizing Doris support for the foreseeable future". morningman picked it up from the Doris side on 2026-09-19 with draft #1842 — [issue #137](https://github.com/sirius-db/sirius/issues/137)
- The paper's earlier Doris integration used a different design: Doris "cannot export Substrait plans", so the authors "introduce minimal modifications to its codebase to convert its internal query plans into the Substrait format", and "each compute node translates its assigned fragment into Substrait and forwards it to the local Sirius execution engine" (§3.2.1, §3.3). The distributed mode then supported only TPC-H Q1, Q3, Q6 and "does not support functions such as avg" (§3.4, §4.3) — [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3). The in-tree 2026 version instead leaves Doris unmodified.

**StarRocks** (`experimental/starrocks`)
- Crate doc: "converts a StarRocks `TExecPlanFragmentParams` (the Thrift plan a StarRocks frontend ships to a backend) into a `substrait` `Plan`… translates one fragment at a time… An `EXCHANGE_NODE` is a fragment boundary: it translates to a stream read of `sirius_stream_<node_id>`" — starrocks-plan-translator/src/lib.rs:1-11 — [S/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/starrocks/crates/starrocks-plan-translator/src/lib.rs)
- Supported nodes:
  - FILE_SCAN→ReadRel(local files), HDFS_SCAN→ReadRel(named table), SELECT→Filter, PROJECT→Project.
  - AGGREGATION→Aggregate ("one-phase, or two-phase `SUM`").
  - EXCHANGE→ReadRel(`sirius_stream_<id>`).
  - SORT→Project+Sort ("global row-number top-N only").
  - HASH_JOIN→Join (anti via outer+is_null or mark+not).
  - NESTLOOP_JOIN→Join (inner/cross only).
  - Aggregates: sum/count/min/max/avg/multi_distinct_*. Scalar functions are allowlisted.
  - lib.rs:26-62
- Process side: brpc `PInternalService` and a thrift CN service (src/brpc.rs, compute_node_service.rs, fragment_executor.rs). Byte-range split of parquet across CNs (#2043).
- Commit history (`gh api commits?path=experimental/starrocks`): first commit `4dc2bd0a` 2026-05-28 Matthijs Brobbel "Setup Starrocks experiment (#816)". Then #832 "Make StarRocks shim CN-only" (06-02), #852 translator crate (06-08), #856 CN thrift RPC skeleton (06-09), #941 "translate FE plan fragments over BRPC PInternalService" (06-16), #1990 (10-07), #2037 "translate exchanges, two-phase SUM and join fragments" (Alexander Ocsa, 10-14), #2043 byte-range splits (10-19).
- Status signals: open #1691 "test(starrocks): end-to-end TPC-H Q16/Q21/Q22 through the CN", #2197 runtime filters, #1963 (LIMIT bug found "on the 2-CN StarRocks path, TPC-H Q3… Q10… fail"). docs/README.md:13 still says "Starrocks (coming soon)" — [issue #1691](https://github.com/sirius-db/sirius/issues/1691), [issue #1963](https://github.com/sirius-db/sirius/issues/1963)
- Shared design: "The engine actor, executor seam, MySQL row encoder, thrift listener, and Substrait extension registry currently have counterparts under `experimental/starrocks/`… changes to common behavior must be checked in both backends" (doris README.md:37-42). Decision in #2025: no shared crate for now; both wait for the `sirius` crate on the #1728 API.

**Others**
- Issue/PR search found no Spark/Gluten, Presto/Velox, ClickHouse, Calcite, Ibis or DataFusion frontend. The "clickhouse" hit #129 is a ClickBench crash and the "velox" hit #1885 is a fuzzing tool (gh search, 2026-10-28) — [issue #1885](https://github.com/sirius-db/sirius/issues/1885)
- The paper lists DuckDB and Doris as current hosts, with other "substrait-compatible" systems as future work (§3.4) — [arXiv 2508.04701v3](https://arxiv.org/html/2508.04701v3)
- The CODEOWNERS component discussion groups "DuckDB submodule glue, Substrait, StarRocks, Rust FFI bindings" as one "Integrations" component — [issue #1266](https://github.com/sirius-db/sirius/issues/1266)

### Inferences
- Status: both are prototypes (directory `experimental/`, single-node or limited multi-node, no cancellation, one query at a time). No evidence of production use (estimate).
- The pattern (host FE unmodified → native plan fragments → Rust translator → Substrait in DuckDB dialect → FFI) is the one validated route for a non-DuckDB system. peacockdb would follow the same pattern with DataFusion `LogicalPlan`/`ExecutionPlan` as the source instead of Thrift fragments. The StarRocks translator's `sirius_stream_<id>` exchange convention plus `ffi::Fragment` is the closest analog for a distributed executor.
- The ecosystem around these frontends is a strong signal of engagement: an Apache Doris PMC member authored the Doris backend, and the RAPIDS/NVIDIA-adjacent maintainers own StarRocks.

### Gaps
- Exact merge dates and review threads of #1842 and #2037 were not read in full. Authorship and dates come from git and the GitHub commits API.
- No public StarRocks- or Doris-side blog or issue mentioning Sirius was searched beyond the Sirius tracker (out of the call budget).

## Q6. GitHub issues and discussions on Substrait and frontends

### Takeaway
The Substrait-relevant issues show a maintainer team that treats the DuckDB Substrait consumer as a temporary bridge. They plan a native physical-plan builder (#1750) and their own consumer (#2192), and they are finding consumer bugs through the StarRocks path (#1963).

### Cited Findings
`gh issue list -R sirius-db/sirius --search substrait --state all`, 2026-10-28:
- #2192 OPEN 2026-10-14 mbrobbel "Build a Substrait consumer" ("on top of #1750. This also helps with #545") — [link](https://github.com/sirius-db/sirius/issues/2192)
- #1963 OPEN 2026-10-01 aocsa "Substrait FetchRel count_expr is ignored, so every LIMIT lowers to LIMIT 0" — [link](https://github.com/sirius-db/sirius/issues/1963)
- #1807 OPEN 2026-09-16 mbrobbel "Move exchange into Sirius" (ExchangeRel/ReadRel mapping; NIXL; "rewrite the plan so it works with the DuckDB Substrait importer") — [link](https://github.com/sirius-db/sirius/issues/1807)
- #1750 OPEN 2026-09-10 mbrobbel "Add a physical plan and plan builder interface" — [link](https://github.com/sirius-db/sirius/issues/1750)
- #1728 OPEN 2026-09-08 mbrobbel "Sirius as a standalone library" — [link](https://github.com/sirius-db/sirius/issues/1728)
- #2025 OPEN 2026-10-06 morningman "Tracking issue: Apache Doris backend (experimental/doris) roadmap" — [link](https://github.com/sirius-db/sirius/issues/2025)
- #1691 OPEN aocsa StarRocks E2E TPC-H Q16/Q21/Q22; #1635 OPEN byte-range split; #2197 OPEN CN runtime filters; #1090 OPEN heartbeat readiness — [#1691](https://github.com/sirius-db/sirius/issues/1691)
- #1590 CLOSED 2026-08-17 aocsa "non-blocking fragment execution and a push/pull FFI surface"; #839 CLOSED "Stream session support"; #841 CLOSED "Starrocks plan fragment to substrait conversion"; #137 CLOSED Doris progress — [#841](https://github.com/sirius-db/sirius/issues/841)
- Decoupling program: #545 (open), #556/#666/#704 (closed, expression-system decoupling), #557 (open, adapter layer) — [#545](https://github.com/sirius-db/sirius/issues/545)
- Older: #54 CLOSED 2025-07-02 "build as an official DuckDB Extensions" (matched by search only).

### Inferences
- The Substrait input is being actively hardened (2026-05 through 10) by people building non-DuckDB frontends, which favors a DataFusion-based product. But the interface is explicitly unstable: `ffi` APIs are to be superseded by the #1728 API, and the consumer itself may be replaced (#2192). Any integration built now should expect API churn (estimate).

### Gaps
- GitHub Discussions were not checked (the repo may not use them).
- No comments exist yet on #2192 or #1750, so the design of the future native consumer/builder (e.g. whether it accepts Substrait physical relations or a Sirius-specific IR) is unknown.
