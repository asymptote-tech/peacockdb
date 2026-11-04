# Comet internals at 1.2.0, and the GPU seam

## Summary
- Comet 1.2.0 (ab46419f) is DataFusion-based: DataFusion 55.2.0, Arrow-rs 59.2.0, Spark 4.1.3 default, Arrow Java 18.3.0. Driver-side Spark extension rules rewrite the physical plan and serialize each native run of operators to protobuf.
- The execution unit is one Spark task: each task sends its block's protobuf (with its own file list injected) to `Native.createPlan`; Rust builds a DataFusion 55 plan and runs partition 0, returning batches to the JVM over the Arrow C Data interface.
- Parquet is read natively via `object_store`; shuffle is compressed Arrow IPC written and read natively; memory is Spark off-heap via a JNI-backed DataFusion pool, with native operators spilling themselves.
- A GPU executor fits at one seam: the native `execute_plan`/`PhysicalPlanner::create_plan` step (protobuf `Operator` in, Arrow `RecordBatch` stream out). There is no extension point there, so it means forking Comet's native crate or a JVM-side block swap rule (estimate).
- Plans are dumped by a driver-side script over `executedPlan`; no config writes the bytes. Decode with `protoc --decode=spark.spark_operator.Operator` against `native/proto/src/proto`.

## Versions and pin
- Tag 1.2.0 → ab46419fcc99bcd34aa4d89f7b545d5d51694a88 ([release](https://github.com/apache/datafusion-comet/releases/tag/1.2.0), accessed 2026-11-04).
- Rust: `native/Cargo.toml:27` version 1.2.0; `:41` arrow 59.2.0; `:46` parquet 59.2.0; `:47-51` datafusion 55.2.0 (+ datafusion-spark 55.2.0); `:78` object_store 0.13.2; `:88` iceberg-rust git rev. `native/core/Cargo.toml:56` prost 0.14.3 (all @ab46419f).
- JVM: `pom.xml:84` spark.version 4.1.3, `:91` arrow.version 18.3.0; profiles Spark 3.4.3, 3.5.9, 4.0.4, 4.1.3, 4.2.0 (`pom.xml:760-860` @ab46419f). Java 17, Scala 2.13.
- Gap to peacockdb: peacockdb pins DataFusion 45 and arrow 54.2.0, and already depends on `datafusion-comet-spark-expr` 0.6.0 (`peacockdb-core/Cargo.toml:20-30` @f4e2767d). DataFusion 45 and 55 types are distinct in one Rust binary, so peacockdb's translator cannot take Comet's DF55 `ExecutionPlan` directly: either peacockdb moves to DF55 or it translates from the protobuf.
- Scan history: `native_comet` and `native_iceberg_compat` scan modes were removed in 0.17 (`docs/source/changelog/0.17.0.md:115,159-160` @ab46419f). At 1.2.0 the V1 Parquet path has one fully native scan; the legacy JVM fallback is gone (`CometExecRule.scala:457-460` @ab46419f).

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

## Plan-dump recipe (verified against code)
- No config writes the protobuf. `spark.comet.explain.native.enabled=true` logs the DataFusion 55 plan *after* translation, per task, at INFO ("Comet native query plan:"), and again with metrics at the end (`jni_api.rs:1555-1558, 1726-1735`; `CometConf.scala:1160-1167`); native logs go to stderr, set `spark.executorEnv.COMET_LOG_LEVEL` (`debugging.md:177-190`). `spark.comet.explain.fallback.enabled=true` logs fallback reasons; `spark.sql.extendedExplainProviders=org.apache.comet.ExtendedExplainInfo` adds coverage to EXPLAIN; `spark.comet.explain.planOnly.enabled` logs only the Spark-level tree.
- Base confs: `spark.plugins=org.apache.spark.CometPlugin`, `spark.shuffle.manager=org.apache.spark.sql.comet.execution.shuffle.CometShuffleManager`, `spark.memory.offHeap.enabled=true`, `spark.memory.offHeap.size=…` (the `isCometLoaded` checks, `CometSparkSessionExtensions.scala:144-174`).
- Smallest hook: a driver-side spark-shell script (`-i dump.scala`), no jar. `CometNativeExec`, `serializedPlanOpt` and `nativeOp` are public (`operators.scala:895-905, 1429`); `collectWithSubqueries` from `AdaptiveSparkPlanHelper` walks AQE stages and subqueries (used by Comet's own Spark 4.1 tests, `CometMergeRowsSuite.scala:187`).
```scala
import java.nio.file.{Files, Paths}
import org.apache.spark.sql.comet.CometNativeExec
import org.apache.spark.sql.execution.adaptive.AdaptiveSparkPlanHelper
object H extends AdaptiveSparkPlanHelper
def dump(q: String, sqlText: String, dir: String): Unit = {
  val df = spark.sql(sqlText); df.collect()            // run, so AQE stages are final
  val blocks = H.collectWithSubqueries(df.queryExecution.executedPlan) {
    case n: CometNativeExec if n.serializedPlanOpt.isDefined => n }
  blocks.zipWithIndex.foreach { case (n, i) =>
    val base = f"$dir/$q-$i%02d-${n.nodeName}"
    Files.write(Paths.get(base + ".pb"), n.serializedPlanOpt.plan.get)
    Files.write(Paths.get(base + ".txt"), n.nativeOp.toString.getBytes) } }
```
- What the bytes are: one blob per native block, a template shared by every task of its stage (not per stage, not per task). `NativeScan` has `common` but no `file_partition` (`CometNativeScan.scala:314`). Native-shuffle blocks lack their `ShuffleWriter` root (the partitioning shows in Spark EXPLAIN; the root is built on the executor, `CometNativeShuffleWriter.scala:295`). Reused exchanges can repeat blocks: dedupe by bytes.
- True per-task bytes need a patched Comet build: write `protobufQueryPlan` from the `CometExecIterator` constructor (`CometExecIterator.scala:88-140`), keyed by `TaskContext` stage/partition. This covers every path, shuffle writer included.
- Decode: `protoc --proto_path=<comet>/native/proto/src/proto --decode=spark.spark_operator.Operator operator.proto < q01-00-CometHashAggregateExec.pb`. `operator.proto` imports `expr.proto`, `literal.proto`, `partitioning.proto`, `types.proto` (`operator.proto:24-27`). The `.txt` is the same tree in TextFormat, before SQL-text interning.

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
