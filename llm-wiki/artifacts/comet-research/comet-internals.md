# Comet internals at 1.2.0, and the GPU seam

## Summary
- Comet 1.2.0 (ab46419f) is DataFusion-based: DataFusion 55.2.0, Arrow-rs 59.2.0, Spark 4.1.3 default, Arrow Java 18.3.0. Driver-side Spark extension rules rewrite the physical plan and serialize each native run of operators to protobuf.
- The execution unit is one Spark task: each task sends its block's protobuf (with its own file list injected) to `Native.createPlan`; Rust builds a DataFusion 55 plan and runs partition 0, returning batches to the JVM over the Arrow C Data interface.
- Parquet is read natively via `object_store`; shuffle is compressed Arrow IPC written and read natively; memory is Spark off-heap via a JNI-backed DataFusion pool, with native operators spilling themselves.
- A GPU executor fits at one seam: the native `execute_plan`/`PhysicalPlanner::create_plan` step (protobuf `Operator` in, Arrow `RecordBatch` stream out). There is no extension point there, so it means forking Comet's native crate or a JVM-side block swap rule (estimate).
- No config writes the plan bytes. After `collect()`, a PySpark py4j walker over the final AQE plan writes each `CometNativeExec` block's protobuf and a `fallbacks.tsv` row per (query, operator, reason); see `## Recipes`. At 1.2.0 one native V1 Parquet scan serves local and `s3a://` paths (no `spark.comet.scan.impl`); S3 credentials come from the AWS profile provider.

## Versions and pin
- Tag 1.2.0 → ab46419fcc99bcd34aa4d89f7b545d5d51694a88 ([release](https://github.com/apache/datafusion-comet/releases/tag/1.2.0), accessed 2026-11-04).
- Rust: `native/Cargo.toml:27` version 1.2.0; `:41` arrow 59.2.0; `:46` parquet 59.2.0; `:47-51` datafusion 55.2.0 (+ datafusion-spark 55.2.0); `:78` object_store 0.13.2; `:88` iceberg-rust git rev. `native/core/Cargo.toml:56` prost 0.14.3 (all @ab46419f).
- JVM: `pom.xml:84` spark.version 4.1.3, `:91` arrow.version 18.3.0; profiles Spark 3.4.3, 3.5.9, 4.0.4, 4.1.3, 4.2.0 (`pom.xml:760-860` @ab46419f). Java 17, Scala 2.13.
- Gap to peacockdb: peacockdb pins DataFusion 45 and arrow 54.2.0, and already depends on `datafusion-comet-spark-expr` 0.6.0 (`peacockdb-core/Cargo.toml:20-30` @f4e2767d). DataFusion 45 and 55 types are distinct in one Rust binary, so peacockdb's translator cannot take Comet's DF55 `ExecutionPlan` directly: either peacockdb moves to DF55 or it translates from the protobuf.
- Scan history: `native_comet` was dropped as a scan option in 0.14 (`docs/source/changelog/0.14.0.md:68` @ab46419f). The `native_iceberg_compat` config and the `native_datafusion` name went in 0.17 (`docs/source/changelog/0.17.0.md:115,159-160,166` @ab46419f). At 1.2.0 there is no `spark.comet.scan.impl` key. At 1.2.0 the V1 Parquet path has one fully native scan; the legacy JVM fallback is gone (`CometExecRule.scala:457-460` @ab46419f).

## Where the plan is decided (driver)
- Entry: `CometPlugin`/`CometDriverPlugin` appends `CometSparkSessionExtensions` to `spark.sql.extensions` (`docs/source/contributor-guide/plugin_overview.md:34-48` @ab46419f).
- The extension injects (`CometSparkSessionExtensions.scala:96-125` @ab46419f): a columnar rule whose `preColumnarTransitions` is `CometRule` and `postColumnarTransitions` = `RevertNativeForTransitionHeavyStages`, `EliminateRedundantTransitions`, `CometCacheColumnarRule` (`CometRule.scala:47-51`); `CometRule` again as an AQE query-stage-prep rule; AQE optimizer rules `CometPlanAdaptiveDynamicPruningFilters`, `CometReuseSubquery`, `CometCoalesceShufflePartitions`; a planner strategy for Iceberg writes. The class comment (`:42-90`) gives the ordering with and without AQE.
- `CometRule.convert` = `CometExecRule(CometScanRule(plan))` (`CometRule.scala:170`).
- Comet disables itself entirely when: big-endian, `spark.comet.enabled=false`, Spark not in off-heap mode, Comet shuffle on without `CometShuffleManager`, INT96 conversion set (`CometSparkSessionExtensions.scala:144-203`).
- `CometScanRule`: `FileSourceScanExec` → `CometScanExec`; `BatchScanExec` → `CometBatchScanExec` (Iceberg, CSV). Fallbacks: unsupported scheme in `object_store`, multiple stores in one scan, `parquetVectorizedReaderEnabled=false`, encryption limits, `input_file_name`, row-index columns, unsupported types, AQE DPP on Spark below 3.5 (`CometScanRule.scala:101-126, 247-384`).
- `CometExecRule` walks bottom-up mapping Spark classes to serdes (`CometExecRule.scala:108-147`). Native operators: Project, Filter, Local/GlobalLimit, Expand, Generate, Hash/ObjectHash/SortAggregate, BroadcastHash/ShuffledHash/SortMerge/BroadcastNestedLoop joins, Sort, LocalTableScan, Range, InMemoryTableScan, Sample, Window, plus shims (EmptyRelation, ArrowEvalPython, WindowGroupLimit, MergeRows). "Sinks" with a JVM boundary: Coalesce, CollectLimit, TakeOrderedAndProject, Union.
- An operator converts only if all its children are already native (`tryConvertToComet`, `:1144-1199`); otherwise it stays Spark with a tagged fallback reason. Unsupported leaves can be bridged with `CometSparkToColumnarExec` (`:537-550`). Partial aggregates whose Final cannot convert are tagged and kept on Spark (buffer format mismatch, `:1001`, `:1755-1763`). Transition-heavy stages revert to Spark (`RevertNativeForTransitionHeavyStages.scala:24-60`).
- Serialization: a top-down pass calls `convertBlock()` on the first `CometNativeExec` of each native run (`CometExecRule.scala:1090-1096`), storing `SerializedPlan(Some(bytes))` after interning SQL text into `sql_text_pool` (`operators.scala:1248-1261`).
- The driver also resolves file splits, DPP and scalar subqueries in scan filters at execution time and injects them (`CometNativeScanExec.scala:155-243`).

## How the plan reaches executors
- IR: protobuf (proto3), 7 files in `native/proto/src/proto/`: `operator.proto` (package `spark.spark_operator`), `expr.proto`, `literal.proto`, `types.proto` (`spark.spark_expression`), `partitioning.proto` (`spark.spark_partitioning`), `config.proto`, `metric.proto` (@ab46419f). Java generated in `org.apache.comet.serde`, protobuf-java shaded (`spark/pom.xml:838-839`). Compiled both by prost (`native/proto/build.rs`) and protoc on the JVM side.
- Root message: `Operator { children, plan_id, sql_text_pool, oneof op_struct }` (`operator.proto:32-84`). Expressions are `Expr` trees inside operators; columns are `BoundReference{index, datatype}` (`expr.proto:491-494`). Spark semantics travel as flags: `EvalMode` LEGACY/TRY/ANSI, `fail_on_error`, `null_short_circuit`, `QueryContext` for error text.
- Unit: one **block per Spark stage fragment, run per task**. A block is a maximal native subtree between JVM boundaries (exchanges, broadcasts, sinks, Spark operators). `CometNativeExec.doExecuteColumnar` builds a `CometExecRDD` (`operators.scala:930-978`) carrying the block bytes once plus per-partition plan data (`CometExecRDD.scala:57-106`).
- In `compute`, each task injects its `NativeScan.file_partition` (or Iceberg file tasks) into the base plan and re-serializes (`CometExecRDD.scala:135-142`), then builds a `CometExecIterator` (`:144`), which calls `nativeLib.createPlan(id, inputObjects, planBytes, configProto, numParts, metrics, …, taskMemoryManager, …, batchSize, offHeap, poolType, limit, taskAttemptId, taskCPUs, …)` (`CometExecIterator.scala:140-160`) and per batch `executePlan(stage, partition, plan, arrayAddrs, schemaAddrs)` (`:266`, `Native.scala:61-130`).
- Native shuffle builds its plan on the executor: `ShuffleWriter(child = block)` with per-task output paths (`CometNativeShuffleWriter.scala:139-196, 295`).
- Native: `createPlan` decodes bytes (`jni_api.rs:814`), sets a session with `target_partitions = spark.task.cpus` (`:1073`). The first `executePlan` builds the DataFusion plan via `PhysicalPlanner::create_plan` (`jni_api.rs:1536-1550`, `planner.rs:1626`) and runs **partition 0 only**: "each Comet native execution corresponds to a single Spark partition" (`jni_api.rs:1562-1564`). One tokio runtime per executor JVM, sized to `spark.executor.cores` or `COMET_WORKER_THREADS` (`jni_api.rs:375-384, 738-741`).
- Inputs: leaves consume the block's inputs in depth-first order (`operators.scala:765`). `Scan` takes a JVM `ArrowArrayStream` via the C Stream interface (`planner.rs:1910-1950`); `ShuffleScan` takes raw compressed shuffle blocks via `CometShuffleBlockIterator`; `NativeScan` reads files itself.
- Broadcasts: the driver collects the child's Arrow batches as Arrow IPC under Spark's io codec and broadcasts them (`CometBroadcastExchangeExec.scala:112-131`; `Utils.scala:255-280`). Each task decodes them (`CometBroadcastExchangeExec.scala:296-313`) and feeds a `Scan` leaf; the hash table is rebuilt in every task.

## Data in, shuffle, results out
- Parquet: the driver sends `NativeScanCommon` once per block (schemas, data filters, projection, `object_store_options` from `fs.s3a.*` etc.); each task gets its `file_partition` (`operator.proto:146-203`). Natively this becomes a DataFusion `ParquetSource`/`DataSourceExec` (`parquet_exec.rs:86-333`) over `object_store` (`parquet_support.rs:988`; S3 in `parquet/objectstore/s3.rs`); HDFS via libhdfs/opendal. No JVM decode on this path. Spark's reader + `CometSparkToColumnarExec` is the fallback.
- Other sources: Iceberg (iceberg-rust `IcebergScan`), CSV V2 (`CsvScan`), Delta/Lance via `ContribScan` (`operator.proto:93-106`).
- Shuffle (`CometShuffleManager` required): native shuffle runs `ShuffleWriterExec` in Rust (hash/range/single/round-robin), one data file per map task, committed through Spark's `IndexShuffleBlockResolver` (`native_shuffle.md:160-202` @ab46419f). JVM columnar shuffle takes rows via `ColumnarToRow` and encodes natively. Same block format: 8-byte length, 8-byte field count, codec tag, then compressed Arrow IPC (lz4 default; zstd, snappy, none; `offsetsAsLengths` with zstd). Reads use the direct path by default: native `ShuffleScanExec` decodes blocks with no FFI (`native_shuffle.md:204-300`). Celeborn is supported in native mode.
- Results out: `prepare_output` exports each column as `FFI_ArrowArray`/`FFI_ArrowSchema` into JVM-allocated structs (`jni_api.rs:1197-1216`; `ffi.md:179-230`). Collect goes through `CometExec.getByteArrayRdd` (Arrow IPC, `operators.scala:851`). Writes: `ParquetWriter` root writes the file natively (or via a JVM `HadoopFileOutput` stream for non-local/non-HDFS), using Spark's commit protocol for names (`operator.proto:1024-1055`); Iceberg via `IcebergWrite`.

## Memory
- Comet requires off-heap mode; its budget is `spark.memory.offHeap.size`, shared with Spark (`memory_management.md:251-275` @ab46419f).
- `spark.comet.exec.memoryPool`: `fair_unified` (default) or `greedy_unified` (`CometConf.scala:1275-1285`), both calling `CometTaskMemoryManager.acquireMemory` over JNI into Spark's `TaskMemoryManager`. Stack: TaskShared → SpillReplay → TrackConsumers → CometFair (`memory_management.md:288-357`).
- Spill: DataFusion operators spill to Spark's local dirs on `try_grow` failure; native shuffle has one spill file per task (`native_shuffle.md:461-492`). Spark cannot make native code spill: `NativeMemoryConsumer.spill()` returns 0 (`memory_management.md:316-322, 634-657`).
- Untracked: Arrow builders, decompression and `object_store` buffers, which must fit in `spark.executor.memoryOverhead` (`memory_management.md:504-530`).
- GPU memory would sit outside all of this (estimate: needs its own RMM pool and admission control).

## The GPU seam (group 3, first bullet)
- **Option A, native in-plan substitution (preferred, estimate).** Fork `native/core`. In `PhysicalPlanner::create_plan` (`planner.rs:1626`) or around `planner.create_plan` in `execute_plan` (`jni_api.rs:1544`), recognise a subtree peacockdb fully supports and emit a `GpuStageExec: ExecutionPlan`.
  - Receives: the prost-decoded `spark_operator::Operator` for the task, with `file_partition` already injected; partition index; the input-slot list; the session's `object_store` and config.
  - Returns: a DataFusion 55 `SendableRecordBatchStream`; Comet's `prepare_output` hands it to the JVM over the C Data interface. A `ShuffleWriter` parent stays on the CPU and encodes Arrow IPC.
  - Inputs: keep Comet's `Scan`/`ShuffleScan` leaves as CPU children (Arrow `RecordBatch`es), or give the GPU the `NativeScan` file list so it reads Parquet itself and skips CPU decode.
  - Why a fork: `OperatorRegistry` is a private compile-time singleton (`operator_registry.rs:32-123`); `register_all_operators` is private; there is no runtime loading of builders.
- **Option B, JVM-side swap (estimate).** A second `SparkSessionExtensions` rule after `CometRule` swaps a fully-supported `CometNativeExec` block for a `GpuNativeExec` with its own JNI library. `nativeOp` and `serializedPlanOpt` are public (`operators.scala:895-905`), but `CometExecRDD` is `private[spark]` (`CometExecRDD.scala:57`) and `PlanDataInjector` is `private[comet]` (`operators.scala:76,122`), so the glue has to live in those packages, against internals that change each release. Native-shuffle stages still go through `CometNativeShuffleWriter`, which builds its plan on the executor.
- **Option C, translate whole blocks on the driver.** Fails for scans: file lists arrive per task (`CometExecRDD.scala:135-142`), so the scan part must be translated per task.
- Evidence for: the per-task contract is narrow (bytes in, Arrow C Data out); scan, filter, join and aggregate all run in one native plan per task; the vocabulary is small (25 operators, 67 expressions, 28 aggregates); peacockdb already uses Comet's murmur3 for Spark hash placement.
- Evidence against: no plug-in point; the protobuf is not a stable API (reserved fields and per-release changes, `operator.proto:919-922, 996-997`; JVM and native built together); slices are small (one task = one split or one shuffle partition, 8192-row batches by default, `CometConf.scala:999-1004`); partial/final aggregate state must match Comet/Spark layout across stages (`HashAggregate.initial_input_buffer_offset`, `operator.proto:915-934`); the broadcast build side is decoded and rebuilt in every task; shuffle encode/decode and decompression are CPU work on a 16-core H100 node; Spark-exact kernels are CPU Rust (`native/spark-expr`) and `ScalarFunc` names fall through to DataFusion and datafusion-spark registries (`comet_scalar_funcs.rs:118-345`). Comet positions itself as CPU-only: "Comet runs on the CPUs your cluster already has" (`docs/source/about/faq.md:37-38` @ab46419f).

## GPUs vs executors and tasks (group 3, last bullet)
- Comet runs N concurrent tasks per executor JVM, each its own native plan on a shared tokio runtime (`jni_api.rs:375-384`). One JVM process = one Spark executor.
- spark-rapids (@ce634969) requires exactly one GPU per executor and throws if Spark assigns more ("Spark GPU Plugin only supports 1 gpu per executor", `GpuDeviceManager.scala:162-170`). Tasks share the GPU through fractional `spark.task.resource.gpu.amount`; `GpuSemaphore` limits how many hold the GPU at once (`GpuSemaphore.scala:183-255`); `spark.rapids.sql.concurrentGpuTasks` sets the initial limit, dynamic by default (`RapidsConfResourceEntries.scala:358-379`). Spark allows fractional task amounts such as 0.25 ([Spark configuration](https://spark.apache.org/docs/latest/configuration.html), accessed 2026-11-04).
- For peacockdb (estimate): one executor per GPU (`spark.executor.resource.gpu.amount=1`) with all 16 cores; one peacockdb executor per process shared by k concurrent task sessions behind a semaphore. This assumes peacockdb's `begin_plan`/`end_plan` sessions can run concurrently on one device; architecture.md does not say — to be checked.

## Proto operators (`Operator.op_struct`, operator.proto:49-83 @ab46419f)
| kind | proto message/field | notes |
|---|---|---|
| source | `Scan scan=100` | Arrow C Stream input from the JVM (Spark scan, broadcast, sink child) |
| exec | `Projection projection=101` | `project_list` |
| exec | `Filter filter=102` | `predicate`, runtime-filter flag |
| exec | `Sort sort=103` | `sort_orders`, optional `fetch`/`skip` (TopK), dynamic filter |
| agg | `HashAggregate hash_agg=104` | Partial/Final/PartialMerge, `expr_modes`, buffer offset; also Spark SortAggregate |
| exec | `Limit limit=105` | limit, offset |
| sink | `ShuffleWriter shuffle_writer=106` | root only, built on the executor; Partitioning, codec, output file |
| exec | `Expand expand=107` | grouping sets |
| join | `SortMergeJoin sort_merge_join=108` | keys, JoinType, condition |
| join | `HashJoin hash_join=109` | BHJ and SHJ; build side; null-aware anti; dynamic filter |
| exec | `Window window=110` | WindowExpr, frames ROWS/RANGE |
| source | `NativeScan native_scan=111` | Parquet; common on the driver, `file_partition` per task |
| source | `IcebergScan iceberg_scan=112` | iceberg-rust; pools + per-task file tasks |
| write | `ParquetWriter parquet_writer=113` | native Parquet file write |
| exec | `Explode explode=114` | explode/posexplode, outer |
| source | `CsvScan csv_scan=115` | CSV V2 |
| source | `ShuffleScan shuffle_scan=116` | native decode of shuffle blocks |
| join | `BroadcastNestedLoopJoin broadcast_nested_loop_join=117` | |
| exec | `Sample sample=118` | without replacement |
| exec | `WindowGroupLimit window_group_limit=119` | row_number/rank/dense_rank top-K |
| write | `IcebergWrite iceberg_write=120` | |
| source | `RangeScan range_scan=121` | `spark.range` |
| exec | `MergeRows merge_rows=122` | MERGE INTO dispatch |
| exec | `ArrowPythonUdf arrow_python_udf=123` | Spark 4.1+ `@arrow_udf`, feature-gated |
| source | `ContribScan contrib_scan=200` | Delta/Lance envelope (`type_url` + bytes) |

JVM-only Comet nodes that never appear in the protobuf: Union, Coalesce, CollectLimit, TakeOrderedAndProject (sinks), CometShuffleExchangeExec, CometBroadcastExchangeExec. Partitioning (`partitioning.proto`): `hash_partition=1` (murmur3, `num_partitions`), `single_partition=2`, `range_partition=3` (driver-sampled bounds), `round_robin_partition=4`.

## Proto expressions (`Expr.expr_struct`, expr.proto:27-99; `AggExpr`, :143-172 @ab46419f)
| kind | proto message/field | notes |
|---|---|---|
| leaf | `Literal literal=2` | typed values, list literals |
| leaf | `BoundReference bound=3` | column ordinal + type |
| arith | `MathExpr add=4`, `subtract=5`, `multiply=6`, `divide=7` | `return_type`, `eval_mode` (ANSI overflow) |
| cast | `Cast cast=8` | Spark cast rules, timezone, eval_mode |
| compare | `BinaryExpr eq=9`, `neq=10`, `gt=11`, `gt_eq=12`, `lt=13`, `lt_eq=14` | |
| null | `UnaryExpr is_null=15`, `is_not_null=16` | |
| logic | `BinaryExpr and=17`, `or=18` | |
| order | `SortOrder sort_order=19` | direction, null ordering |
| datetime | `Hour hour=22`, `Minute minute=23`, `Second second=24` | timezone-aware |
| decimal | `CheckOverflow check_overflow=25` | decimal precision check |
| string | `BinaryExpr like=26`, `rlike=30` | rlike is regex |
| func | `ScalarFunc scalarFunc=31` | **open set keyed by `func` name**: ~308 Spark classes mapped in `QueryPlanSerde.scala:58-460`; native lookup `comet_scalar_funcs.rs:118-345` then registry |
| compare | `BinaryExpr eqNullSafe=32`, `neqNullSafe=33` | |
| bitwise | `BinaryExpr bitwiseAnd=34`, `bitwiseOr=35`, `bitwiseXor=36` | |
| arith | `MathExpr remainder=37` | |
| cond | `CaseWhen caseWhen=38` | |
| pred | `In in=39` | list, negated |
| logic | `UnaryExpr not=40` | |
| arith | `UnaryMinus unary_minus=41` | fail_on_error |
| bitwise | `BinaryExpr bitwiseShiftRight=42`, `bitwiseShiftLeft=43` | |
| cond | `IfExpr if=44` | |
| float | `NormalizeNaNAndZero normalize_nan_and_zero=45` | joins/aggs on floats |
| datetime | `TruncTimestamp truncTimestamp=47` | |
| subquery | `Subquery subquery=50` | scalar subquery result by id at run time |
| leaf | `UnboundReference unbound=51` | by name |
| pred | `BloomFilterMightContain bloom_filter_might_contain=52` | runtime bloom filter |
| nested | `CreateNamedStruct create_named_struct=53`, `GetStructField get_struct_field=54` | |
| json | `ToJson to_json=55` | |
| nested | `ListExtract list_extract=56`, `GetArrayStructFields get_array_struct_fields=57`, `ArrayInsert array_insert=58` | |
| arith | `MathExpr integral_divide=59` | `check_divide_overflow` |
| string | `ToPrettyString to_pretty_string=60` | show()/cast to string |
| nondet | `Rand rand=61`, `Rand randn=62` | seeded per partition |
| nondet | `EmptyExpr spark_partition_id=63`, `monotonically_increasing_id=64` | needs partition index |
| datetime | `UnixTimestamp unix_timestamp=65` | |
| json/csv | `FromJson from_json=66`, `ToCsv to_csv=67` | |
| datetime | `HoursTransform hours_transform=68` | Iceberg partition transform |
| nested | `ArraysZip arrays_zip=69` | |
| udf | `JvmScalarUdf jvm_scalar_udf=70` | calls back into the JVM over FFI |
| datetime | `PreciseTimestampConversion precise_timestamp_conversion=71` | |
| nondet | `Shuffle shuffle=72`, `RandStr rand_str=73`, `Uuid uuid=74` | |
| udf | `NativeScalarUdf native_scalar_udf=75` | user cdylib |
| null | `AtLeastNNonNulls at_least_n_non_nulls=76` | |
| agg | `Count count=2`, `Sum sum=3`, `Min min=4`, `Max max=5`, `Avg avg=6` | Sum/Avg carry eval_mode, type |
| agg | `First first=7`, `Last last=8` | ignore_nulls |
| agg | `BitAndAgg=9`, `BitOrAgg=10`, `BitXorAgg=11` | |
| agg | `Covariance=12`, `Variance=13`, `Stddev=14`, `Correlation=15`, `Regr regr=22` | sample/population |
| agg | `BloomFilterAgg bloomFilterAgg=16` | builds runtime filters |
| agg | `CollectSet=17`, `CollectList=21`, `ListAgg=26` | |
| agg | `Percentile=18`, `ApproxPercentile=19`, `Mode=23` | |
| agg | `HllPlusPlus hllpp=20`, `HllSketchAgg=27`, `HllUnionAgg=28` | sketches |
| agg | `MaxBy maxBy=24`, `MinBy minBy=25`, `PivotFirst pivotFirst=29` | |

Every `AggExpr` can carry a `filter` (FILTER WHERE). `Literal`/`DataType` cover bool, int8-64, float, double, string, bytes, timestamp(_ntz), decimal, date, null, list, map, struct, time, interval types and variant (`types.proto:43-70`).

## Recipes
Checked against code at ab46419f, Spark tag v4.1.3, object_store v0.13.2 and Maven Central. Nothing here was run. Spark paths are cited by file name; the full tree is in Sources.

### 1. Final plans and native plan bytes through py4j (PySpark driver)
- Run the query first. PySpark `collect()` calls `collectToPython` on the same `queryExecution` (`Dataset.scala:1504,2081 @v4.1.3`). Then `df._jdf.queryExecution().executedPlan()` is the `AdaptiveSparkPlanExec` (AQE) node.
- `isFinalPlan` is a public method (`AdaptiveSparkPlanExec.scala:229 @v4.1.3`). It becomes true at `:377`. `toString` also prints it through `stringArgs` (`:422`). Spark adds AQE only to plans with an exchange or a subquery, so the root is not always AQE.
- Accessors, all public: `executedPlan()` (`AdaptiveSparkPlanExec.scala:227`), `plan()` on `QueryStageExec` (`QueryStageExec.scala:57`), `child()` on `ReusedExchangeExec` (`Exchange.scala:48`) and on `ReusedSubqueryExec` (`basicPhysicalOperators.scala:974`). Both of those are leaves, so `children()` is empty. Also `subqueries()` (`QueryPlan.scala:520`) and `innerChildren()` (`TreeNode.scala:1030`; on plans it is `subqueries`, `QueryPlan.scala:650`), all @v4.1.3. Comet's `CometSubqueryBroadcastExec` keeps its plan in `children` (`CometSubqueryBroadcastExec.scala:55-62 @ab46419f`). On Comet nodes: `serializedPlanOpt()` and `nativeOp()` are public (`operators.scala:895-904 @ab46419f`). `SerializedPlan.plan()` is a Scala `Option[Array[Byte]]` (`operators.scala:1429-1433`). From Python, call `.isDefined()` and `.get()`. Py4J turns a Java `byte[]` into Python `bytes` ([Py4J](https://www.py4j.org/advanced_topics.html), accessed 2026-11-04). For a Scala `Seq`, use `.size()` and `.apply(i)`.
- Do not name files after `nodeName`. For scans it contains the relation and spaces (`CometNativeScanExec.scala:90-91 @ab46419f`). Use the simple class name. Fallback reasons are a tag on each node, `CometExplainInfo.FALLBACK_REASONS`. Reasons found on expressions are moved up onto their operator (`ExtendedExplainInfo.scala:318-319`; `CometExecRule.scala:1220 @ab46419f`). `ExtendedExplainInfo().generateVerboseInfo(plan)` returns the tree with `[COMET: …]` marks and the coverage line (`ExtendedExplainInfo.scala:56-61`). Use classic PySpark, where `df._jdf` exists. Put the Comet jar on the driver classpath (`spark.driver.extraClassPath`, as Comet's runner does, `benchmarks/tpc/engines/comet.toml:26,29`) so py4j can load `org.apache.comet.*`.
```python
import hashlib, os
P = "org.apache.spark.sql.execution."
AQE, QS, NATIVE = P + "adaptive.AdaptiveSparkPlanExec", P + "adaptive.QueryStageExec", "org.apache.spark.sql.comet.CometNativeExec"
REUSED = {P + "exchange.ReusedExchangeExec", P + "ReusedSubqueryExec"}

def _isa(o, fqcn):  # walk superclasses; no class loading through py4j
    c = o.getClass()
    while c is not None:
        if c.getName() == fqcn: return True
        c = c.getSuperclass()
    return False
def _seq(s): return [s.apply(i) for i in range(s.size())]
def dump(spark, name, sql, outdir):
    jvm = spark.sparkContext._jvm; os.makedirs(outdir, exist_ok=True)
    df = spark.sql(sql); nrows = len(df.collect())        # run first: AQE plan is final
    qe = df._jdf.queryExecution(); root = qe.executedPlan()
    assert not _isa(root, AQE) or root.isFinalPlan(), name
    nodes, seen, stack = [], set(), [root]
    while stack:
        n = stack.pop(); k = (n.getClass().getName(), jvm.java.lang.System.identityHashCode(n))
        if k in seen: continue
        seen.add(k); nodes.append(n)
        if _isa(n, AQE): kids = [n.executedPlan()]
        elif _isa(n, QS): kids = [n.plan()]
        elif k[0] in REUSED: kids = [n.child()]
        else: kids = _seq(n.children())
        stack += kids + _seq(n.subqueries())
    nonfinal = sum(1 for n in nodes if _isa(n, AQE) and not n.isFinalPlan())
    expl = jvm.org.apache.spark.sql.api.python.PythonSQLUtils.explainString(qe, "formatted")
    cov = jvm.org.apache.comet.ExtendedExplainInfo().generateVerboseInfo(root)
    with open(f"{outdir}/{name}.plan.txt", "w") as f: f.write(expl + "\n" + cov)
    tag, hashes, rows = jvm.org.apache.comet.CometExplainInfo.FALLBACK_REASONS(), set(), []
    for n in nodes:
        op = n.getClass().getSimpleName()
        if _isa(n, NATIVE) and n.serializedPlanOpt().isDefined():
            b = bytes(n.serializedPlanOpt().plan().get())
            h = hashlib.sha1(b).hexdigest()
            if h not in hashes:                            # reused exchanges repeat blocks
                base = f"{outdir}/{name}-{len(hashes):02d}-{op}"; hashes.add(h)
                with open(base + ".pb", "wb") as f: f.write(b)
                with open(base + ".txt", "w") as f: f.write(n.nativeOp().toString())
        v = n.getTagValue(tag)
        if v.isDefined():
            rows += [(name, op, r) for r in v.get().mkString("\n").split("\n") if r]
    with open(f"{outdir}/fallbacks.tsv", "a") as f:
        for r in rows: f.write("\t".join(x.replace("\t", " ") for x in r) + "\n")
    return nrows, len(hashes), len(rows), nonfinal
```
- Some TPC-DS files hold two SELECTs. Comet's runner splits each file on `;` (`benchmarks/tpc/tpcbench.py:160-175 @ab46419f`), so call `dump` once per statement. In Scala, `AdaptiveSparkPlanHelper.collectWithSubqueries` does the same walk, but it does not follow `ReusedExchangeExec` or `ReusedSubqueryExec` (`AdaptiveSparkPlanHelper.scala:112-129 @v4.1.3`).
- What the bytes are: one blob per native block, after SQL text is moved into `sql_text_pool` (`operators.scala:1248-1260`). It is a template shared by every task of its stage. `NativeScan` has `common` but no `file_partition` (`CometNativeScan.scala:314`). Native-shuffle blocks lack their `ShuffleWriter` root, which is built on the executor (`CometNativeShuffleWriter.scala:295`). The `.txt` is `nativeOp` as protobuf text, before the SQL text is moved. True per-task bytes need a patched build that writes `protobufQueryPlan` in the `CometExecIterator` constructor (`CometExecIterator.scala:88-140`). Decode: `protoc --proto_path=<comet>/native/proto/src/proto --decode=spark.spark_operator.Operator operator.proto < q01-00-CometHashAggregateExec.pb` (imports at `operator.proto:24-27`).

### 2. Fallback and native-plan logging
- `spark.comet.explain.fallback.enabled` is the current key. `spark.comet.explainFallback.enabled` is an older alias for it (`CometConf.scala:1186-1188 @ab46419f`); reading an alias logs a one-time deprecation warning (`:1902-1910`). With it on, `CometExecRule` logs one WARN per planning pass that has fallbacks, "Comet cannot execute some parts of this plan natively", then the explain tree (`CometExecRule.scala:1019-1026`). It runs on the driver for each AQE stage, so it repeats. Logger: `org.apache.comet.rules.CometExecRule`, log4j2.
- `spark.comet.explain.fallback.log.enabled` (alias `spark.comet.logFallbackReasons.enabled`, env `ENABLE_COMET_LOG_FALLBACK_REASONS`) logs a driver WARN per reason as it is tagged: "Comet cannot accelerate X because: …" (`CometConf.scala:1178-1184`; `CometSparkSessionExtensions.scala:333-337`).
- `spark.comet.explain.native.enabled` (`CometConf.scala:1160-1167`): each task logs the DataFusion plan at INFO, "Comet native query plan:", and again with metrics at the end (`jni_api.rs:1555-1558, 1726-1738`). This is the native logger (log4rs), not log4j2, writing to the executor process's stderr (`lib.rs:334-350`). `COMET_LOG_LEVEL` sets the level (`NativeBase.java:148-168`); pass it as `spark.executorEnv.COMET_LOG_LEVEL`. One plan per task per stage: debug runs only.
- Where output lands. Driver log4j2 goes to stderr at INFO (`conf/log4j2.properties.template:19-30 @v4.1.3`). In standalone mode, executor stderr is `$SPARK_HOME/work/<app-id>/<exec-id>/stderr` (`Worker.scala:613`, `ExecutorRunner.scala:188-191 @v4.1.3`). In `local[*]` mode it is all the driver's stderr. Neither log is in the event log. The event log does get the final plan text with the Comet section: AQE posts `SparkListenerSQLAdaptiveExecutionUpdate` with `explainString(spark.sql.ui.explainMode)` when the plan becomes final (`AdaptiveSparkPlanExec.scala:377,390,847-858 @v4.1.3`; not yet seen in a real log).
- `spark.sql.extendedExplainProviders=org.apache.comet.ExtendedExplainInfo` adds a "Comet" section to `explainString` in the extended and formatted modes (`QueryExecution.scala:358,418,471-485 @v4.1.3`). From Python: `PythonSQLUtils.explainString(qe, mode)` (`PythonSQLUtils.scala:115 @v4.1.3`). For the TSV, read the tags as `dump` does; `getFallbackReasons(plan)` returns reasons without their operators (`ExtendedExplainInfo.scala:63-65`).

### 3. Comet's TPC benchmark configuration
- The runner is `benchmarks/tpc/run.py`. It merges the common conf, then the benchmark profile, then the engine TOML (`run.py:253-262`). Comet engine, `benchmarks/tpc/engines/comet.toml:28-33 @ab46419f`:
```toml
[spark_conf]
"spark.driver.extraClassPath" = "$COMET_JAR"
"spark.executor.extraClassPath" = "$COMET_JAR"
"spark.plugins" = "org.apache.spark.CometPlugin"
"spark.shuffle.manager" = "org.apache.spark.sql.comet.execution.shuffle.CometShuffleManager"
"spark.comet.expression.Cast.allowIncompatible" = "true"
```
- Common conf, `benchmarks/tpc/run.py:107-116 @ab46419f`:
```python
COMMON_SPARK_CONF = {
    "spark.driver.memory": "8G",
    "spark.executor.memory": "16g",
    "spark.memory.offHeap.enabled": "true",
    "spark.memory.offHeap.size": "16g",
    "spark.eventLog.enabled": "true",
    "spark.eventLog.dir": os.environ.get("SPARK_EVENT_LOG_DIR", "/tmp/spark-events"),
    "spark.hadoop.fs.s3a.impl": "org.apache.hadoop.fs.s3a.S3AFileSystem",
    "spark.hadoop.fs.s3a.aws.credentials.provider": "com.amazonaws.auth.DefaultAWSCredentialsProviderChain",
}
```
- TPC-H and TPC-DS use the same profile: `"executor_instances": "2"`, `"executor_cores": "8"`, `"max_cores": "16"` (`run.py:122-137`). They differ only in the data variable and `--format parquet` for TPC-H. Nothing sets shuffle partitions or AQE, so Spark defaults apply. The published TPC-DS numbers used Comet 1.1.0 on Spark 4.2.0 with 32 executors and other memory settings (`docs/source/contributor-guide/benchmark-results/tpc-ds.md:22-72`).
- Deviations for one 128-vCPU, 512 GB executor (all estimates, untested): 1 executor, `executor_cores`/`max_cores` about 128 (Comet sizes its Tokio pool from `spark.executor.cores`, `jni_api.rs:375-384`); driver 16g, executor heap 64g, off-heap 320g, `spark.executor.memoryOverhead` 48g for untracked native memory; `spark.sql.shuffle.partitions` at least 256; `SPARK_LOCAL_DIRS` on local disk (`benchmarking_aws_ec2.md:76-88`). Keep `Cast.allowIncompatible=true` only to match Comet's runs. The provider `DefaultAWSCredentialsProviderChain` must change: native rejects it and Hadoop 3.4 does not remap it (see 4).

### 4. Native S3 scan: same scan for local disk and S3
- There is no `spark.comet.scan.impl` at 1.2.0; `CometConf.scala` has only the test-only `spark.comet.scan.enabled`, default true (`:96-101`). `native_comet` was dropped in 0.14 (`changelog/0.14.0.md:68`), the `native_datafusion` name in 0.17 (`0.17.0.md:166`). A V1 Parquet scan becomes `CometNativeScanExec` or stays Spark's own scan (`CometExecRule.scala:457-460`). So local and S3 runs share one scan. Check that `fallbacks.tsv` has no scan rows in either run.
- An `s3a://` scan still falls back for: a scheme object_store cannot parse, a scheme in `fs.comet.libhdfs.schemes`, mixed stores, a rejected path, or the vectorized reader off (`CometScanRule.scala:247-345`). Native rewrites `s3a` to `s3` (`parquet_support.rs:869`). No driver-side check of the credential provider was found, so a bad provider name fails inside the task (estimate) with "Unsupported credential provider" (`s3.rs:641-644`).
- The driver copies every `fs.s3a.*` key of the session Hadoop conf into the plan's `object_store_options`, `core-default.xml` defaults included (`NativeConfig.scala:338-380`; `CometNativeScan.scala:267-295`). Those values are in every dumped `.pb`: one more reason never to put a key in conf.
- Key mapping, per bucket first (`fs.s3a.bucket.<b>.<p>`), then global (`s3.rs:420-430`): `endpoint.region` → `Region`; `path.style.access=v` → `VirtualHostedStyleRequest=v`, and when true `/<bucket>` is appended to the endpoint; `endpoint` gets `https://` if it has no scheme; `requester.pays.enabled` → `RequestPayer` (`s3.rs:345-418`). With a custom endpoint object_store sends path-style URLs either way (`src/aws/builder.rs:1214-1220 @v0.13.2`); set `path.style.access=true` so the JVM's S3A matches. With no endpoint and no region, native probes AWS with a HEAD request (`s3.rs:161-177`).
- Providers native accepts (`s3.rs:494-516, 620-645`): Hadoop `Simple`, `Temporary`, `AssumedRole`, `IAMInstance`, `Anonymous`; SDK v1 and v2 names for Environment, InstanceProfile, Container, WebIdentity, Anonymous and **Profile**. Not accepted: `DefaultAWSCredentialsProviderChain`, v2 `DefaultCredentialsProvider`.
  - Both Profile names build aws-config 1.12.0's `ProfileFileCredentialsProvider` with no profile name (`s3.rs:973-977`; `native/Cargo.lock:643-644`). The profile then comes from `AWS_PROFILE`, default `default`; files from `AWS_CONFIG_FILE`/`~/.aws/config` and `AWS_SHARED_CREDENTIALS_FILE`/`~/.aws/credentials` ([docs.rs](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html), accessed 2026-11-04).
  - Native does not read the endpoint or region from the profile. Repeat them in `fs.s3a.*`; they are not secrets.
  - An empty provider key uses aws-config's default chain, but Hadoop 3.4.2's `core-default.xml` sets a 4-entry list without Profile ([core-default](https://hadoop.apache.org/docs/r3.4.2/hadoop-project-dist/hadoop-common/core-default.xml), accessed 2026-11-04). So set the provider explicitly. Unverified: that Nebius accepts path-style requests and signs as region `eu-west1` (the profile files are off limits).
- JVM side (listing on the driver): Hadoop 3.4 remaps the v1 Profile name to v2 `ProfileCredentialsProvider` and builds it with `create()`, which reads `AWS_PROFILE` ([Hadoop SDK upgrade](https://hadoop.apache.org/docs/r3.4.2/hadoop-aws/tools/hadoop-aws/aws_sdk_upgrade.html), accessed 2026-11-04). Spark 4.1.3 uses Hadoop 3.4.2 and AWS SDK v2 2.29.52 (`pom.xml:130,165 @v4.1.3`). Jars for `$SPARK_HOME/jars`: [hadoop-aws-3.4.2.jar](https://repo1.maven.org/maven2/org/apache/hadoop/hadoop-aws/3.4.2/hadoop-aws-3.4.2.jar) (936,997 B) and [bundle-2.29.52.jar](https://repo1.maven.org/maven2/software/amazon/awssdk/bundle/2.29.52/bundle-2.29.52.jar) (641,534,749 B) (accessed 2026-11-04). Whether hadoop-aws's compile-scope `analyticsaccelerator-s3` 1.2.1 is needed at runtime is unverified. Native TLS uses the system root certificates in `/etc/ssl/certs`, not the JVM trust store (`datasources.md:296-301`).
```properties
# (a) local disk and (b) S3: same lines; (b) adds the S3 block
spark.plugins=org.apache.spark.CometPlugin
spark.shuffle.manager=org.apache.spark.sql.comet.execution.shuffle.CometShuffleManager
spark.driver.extraClassPath=/opt/comet/comet-spark-spark4.1_2.13-1.2.0.jar
spark.executor.extraClassPath=/opt/comet/comet-spark-spark4.1_2.13-1.2.0.jar
spark.memory.offHeap.enabled=true
spark.memory.offHeap.size=320g
spark.comet.explain.fallback.enabled=true
spark.sql.extendedExplainProviders=org.apache.comet.ExtendedExplainInfo
# (b) only; also `export AWS_PROFILE=eu-west1` before spark-submit (driver S3A listing)
spark.hadoop.fs.s3a.endpoint=https://storage.eu-west1.nebius.cloud:443
spark.hadoop.fs.s3a.endpoint.region=eu-west1
spark.hadoop.fs.s3a.path.style.access=true
spark.hadoop.fs.s3a.aws.credentials.provider=software.amazon.awssdk.auth.credentials.ProfileCredentialsProvider
spark.executorEnv.AWS_PROFILE=eu-west1
```

### 5. Release jar, Spark binary, Java
- Comet jar: https://repo1.maven.org/maven2/org/apache/datafusion/comet-spark-spark4.1_2.13/1.2.0/comet-spark-spark4.1_2.13-1.2.0.jar (98,985,596 B, dated 2026-10-31; HTTP 200 checked 2026-11-04). Its pom builds against `spark-sql_2.13` 4.1.3. It bundles `org/apache/comet/linux/amd64/libcomet.so` and `linux/aarch64/libcomet.so`. The amd64 build targets `x86-64-v3`, so the CPU needs AVX2 (`installation.md:128-130`). Its highest glibc symbol version is `GLIBC_2.30` (read from the jar by HTTP range requests, accessed 2026-11-04); releases build in `ubuntu:20.04` (`dev/release/comet-rm/Dockerfile:19`).
- Spark: https://archive.apache.org/dist/spark/spark-4.1.3/spark-4.1.3-bin-hadoop3.tgz (572,874,252 B; HTTP 200 checked 2026-11-04; the dlcdn URL returned 404). Not the `-connect` tarball, which defaults to Spark Connect, where `df._jdf` does not exist (estimate).
- Java 17: `java.version` 17 (`pom.xml:120 @v4.1.3`). Comet lists Java 17 and 21 for Spark 4.1.3 (`installation.md:48,61`).

## Follow-up experiments
- Run the dump script for TPC-H q1-q22 and a TPC-DS sample on Comet 1.2.0 / Spark 4.1.3 with AQE on; count operators, `Expr` variants and `ScalarFunc` names per block for Task 5.
- Patch `CometExecIterator` on a scratch build to log per-task plan sizes and file counts, to get the real per-task slice at sf40.
- Measure the share of stage time in shuffle encode/decode and broadcast decode on the CPU; it bounds what a GPU stage can save.
- Check whether peacockdb sessions can run concurrently on one GPU, as the k-tasks-per-executor model needs.

## Sources
- Comet 1.2.0 release, commit ab46419f: https://github.com/apache/datafusion-comet/releases/tag/1.2.0 (accessed 2026-11-04). Local clone at that SHA; every `path:line` above is @ab46419f unless marked.
- Comet tree: https://github.com/apache/datafusion-comet/tree/ab46419fcc99bcd34aa4d89f7b545d5d51694a88 (accessed 2026-11-04).
- spark-rapids (NVIDIA/cudf-spark) at ce6349699ccb3bd527c223c66ba24bd708e67e18: https://github.com/NVIDIA/cudf-spark (accessed 2026-11-04).
- Spark configuration, custom resource scheduling: https://spark.apache.org/docs/latest/configuration.html (accessed 2026-11-04).
- peacockdb `architecture.md` and `peacockdb-core/Cargo.toml` at f4e2767d (local).
- Apache Spark v4.1.3 source (`AdaptiveSparkPlanExec.scala`, `QueryStageExec.scala`, `Exchange.scala`, `basicPhysicalOperators.scala`, `QueryPlan.scala`, `TreeNode.scala`, `AdaptiveSparkPlanHelper.scala`, `QueryExecution.scala`, `classic/Dataset.scala`, `PythonSQLUtils.scala`, `deploy/worker/*`, `pom.xml`, `conf/log4j2.properties.template`): https://github.com/apache/spark/tree/v4.1.3 (accessed 2026-11-04).
- Py4J type conversion (byte[] to bytes): https://www.py4j.org/advanced_topics.html (accessed 2026-11-04).
- object_store v0.13.2 `src/aws/builder.rs`: https://github.com/apache/arrow-rs-object-store/blob/v0.13.2/src/aws/builder.rs (accessed 2026-11-04).
- aws-config 1.12.0 profile provider: https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html ; profile selection in aws-sdk-rust `sdk/aws-runtime/src/env_config/source.rs` (main branch): https://github.com/awslabs/aws-sdk-rust (accessed 2026-11-04).
- Hadoop 3.4.2 S3A SDK v2 upgrade and `core-default.xml`: https://hadoop.apache.org/docs/r3.4.2/hadoop-aws/tools/hadoop-aws/aws_sdk_upgrade.html , https://hadoop.apache.org/docs/r3.4.2/hadoop-project-dist/hadoop-common/core-default.xml (accessed 2026-11-04).
- Maven Central: Comet 1.2.0 for Spark 4.1 / Scala 2.13 https://repo1.maven.org/maven2/org/apache/datafusion/comet-spark-spark4.1_2.13/1.2.0/ ; hadoop-aws 3.4.2 https://repo1.maven.org/maven2/org/apache/hadoop/hadoop-aws/3.4.2/ ; AWS SDK bundle 2.29.52 https://repo1.maven.org/maven2/software/amazon/awssdk/bundle/2.29.52/ (accessed 2026-11-04).
- Spark 4.1.3 binaries: https://archive.apache.org/dist/spark/spark-4.1.3/ (accessed 2026-11-04).
