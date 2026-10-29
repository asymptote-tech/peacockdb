# How GPU SQL engines and NVIDIA tools decide GPU vs CPU (for "gpu-cpu-router")

Research limits: the session's WebSearch budget ran out after 2 searches. Fetches of the PG-Strom and HeavyDB docs and of the NVIDIA QualX doc failed (timeout or DNS). The Fabric, CoddSpeed, NVIDIA "~20%", Gluten, HeavyDB, Kinetica, BlazingSQL, Theseus and KX items are therefore gaps, not findings. Everything else was checked in GitHub through the gh API.

## 1. spark-rapids (NVIDIA/cudf-spark) and its tools (NVIDIA/cudf-spark-tools)

### Takeaway
NVIDIA only predicts GPU speedup offline, per application, from CPU event logs (QualX, an ML model). At runtime the plugin places each operator by capability. A cost-based optimizer that can keep plan sections on the CPU exists, but it is experimental and off by default. Published accuracy is unknown; a user and a 2026-10 benchmark both report over-prediction.

### Cited Findings
- Repo renames: `NVIDIA/spark-rapids` now resolves to `NVIDIA/cudf-spark`, and `NVIDIA/spark-rapids-tools` resolves to `NVIDIA/cudf-spark-tools` (gh API redirects). Latest releases: cudf-spark v26.08.1 (tag SHA 15899e9f), tools v26.08.4 (tag SHA 43722756) — [cudf-spark](https://github.com/NVIDIA/cudf-spark), [cudf-spark-tools](https://github.com/NVIDIA/cudf-spark-tools) (accessed 2026-10-29)
- QualX is the qualification tool's speedup engine. It is an XGBoost model trained on paired CPU and GPU runs, with per-platform default models trained mainly on NDS. It outputs an estimated GPU duration per SQL query and per application, and speedup = CPU duration ÷ predicted GPU duration. Users can train a custom model (`--custom_model_file`): CPU event logs give the features and GPU logs give the label (source: search snippets; the full page fetch timed out) — [QualX doc 26.08](https://docs.nvidia.com/cudf-spark/26.08/qualification/qualx.html), [26.04](https://docs.nvidia.com/spark-rapids/user-guide/26.04/qualification/qualx.html) (accessed 2026-10-29)
- NVIDIA blog (2025-05-15):
  - The pre-trained model was "trained on historical data from matching CPU and GPU runs", mostly NDS.
  - Features come from event logs: "disk bytes spilled, maximum heap memory used, estimated scan bandwidth", plus operator details and data size.
  - It "estimate[s] speed-up at the individual operator level", then combines that with heuristics.
  - It gives no accuracy numbers. It suggests judging models by MAPE, precision and recall.
  - It warns that the models may be inaccurate when workloads or hardware differ from NDS.
  - It has no "~20% of job" statement.
  — [NVIDIA blog](https://developer.nvidia.com/blog/predicting-performance-on-apache-spark-with-gpus) (accessed 2026-10-29)
- Issue cudf-spark-tools #1995 (opened 2025-12-15, still open), "[QST] The model performed poorly when I used my own data":
  - The user trained on more than 1600 SQLs. About 90% of the apps had speedup above 1×, so the training data was skewed.
  - The model reached MAPE 21% on high-speedup jobs but "is very prone to misclassifying low-speed tasks as high-speed tasks".
  - A maintainer (leewyang) suggested `sample_weight` in qualx-conf.yaml (set it to "auto" or assign class weights), more than 1000 sqlIDs, and more negative examples. There is no official troubleshooting procedure.
  — [#1995](https://github.com/NVIDIA/cudf-spark-tools/issues/1995), [qualx-conf.yaml@4701b31a](https://github.com/NVIDIA/spark-rapids-tools/blob/4701b31a6c774e2599def665335fa92c2792b684/user_tools/src/spark_rapids_pytools/resources/qualx-conf.yaml#L22-L25) (accessed 2026-10-29)
- HackerNoon, 2026-10-20 (author "chetan"):
  - Setup: Spark 3.5.5 with RAPIDS 25.12.0 on GKE, `g2-standard-32` nodes with one L4 each.
  - The Qualification Tool rated several workloads "Large" and estimated a "4–5× GPU speedup", yet some RAPIDS runs finished slower than CPU end-to-end. The article gives no per-job numbers.
  - The author's reasons:
    - the tool counts only SQL stage time, not GPU node provisioning or executors stuck Pending;
    - CPU fallbacks (text output, some Iceberg versions, Cartesian joins, PySpark UDFs) combined with skew;
    - CPU cores left idle with one GPU per executor;
    - join OOMs.
  - Advice: keep RAPIDS opt-in per workload; stay on CPU for "Medium" or "Not Recommended" ratings.
  — [HackerNoon](https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups) (accessed 2026-10-29)
- Runtime placement is per operator and capability-based, with fallback on by default:
  - `spark.rapids.sql.explain` (default `NOT_ON_GPU`) "Explain why some parts of a query were not placed on a GPU".
  - `spark.rapids.sql.expression.cpuBridge.enabled` (default true) wraps CPU-only expression subtrees "instead of causing plan fallbacks".
  - `spark.rapids.sql.suppressPlanningFailure` sends a whole query to the CPU on unexpected planning errors.
  - `spark.rapids.sql.mode=explainOnly|executeOnGPU`.
  - `spark.rapids.sql.enabled` is a runtime on/off switch, so it can be set per session.
  — [configs.md @v26.08.1](https://github.com/NVIDIA/cudf-spark/blob/v26.08.1/docs/configs.md), [advanced_configs.md @v26.08.1](https://github.com/NVIDIA/cudf-spark/blob/v26.08.1/docs/additional-functionality/advanced_configs.md) (accessed 2026-10-29)
- There is a cost-based "don't go to GPU" optimizer, `CostBasedOptimizer.scala`. Its source comment calls it an "Experimental cost-based optimizer that aims to avoid moving sections of the plan to the GPU when it would be better to keep that part of the plan on the CPU… we don't want to move data to the GPU just for a trivial projection".
  - How it decides: it estimates rows per operator (`RowCountPlanVisitor`). It keeps an operator on the CPU (`AvoidTransition`) if GPU cost plus transition cost is greater than CPU cost, except for exchanges.
  - Configs on main @49d3be94:

    | Config | Default |
    |---|---|
    | `spark.rapids.sql.optimizer.enabled` | **false** |
    | `.optimizer.defaultRowCount` | 1,000,000 |
    | `.optimizer.cpu.exec.default` | 0.0002 s/row |
    | `.optimizer.gpu.exec.default` | 0.0001 s/row |
    | CPU memory read/write speed | 30 GB/s |
    | GPU memory read/write speed | 320 GB/s |

  — [CostBasedOptimizer.scala @v26.08.1](https://github.com/NVIDIA/cudf-spark/blob/v26.08.1/sql-plugin/src/main/scala/com/nvidia/spark/rapids/CostBasedOptimizer.scala), [RapidsConfEntries.scala @49d3be94](https://github.com/NVIDIA/cudf-spark/blob/49d3be946ca9cd660c3b74b15a684a8aec92e058/sql-plugin/src/main/scala/com/nvidia/spark/rapids/RapidsConfEntries.scala) (accessed 2026-10-29)
- Maintainer report of GPU slower than CPU, cudf-spark #7529 (2023-01-18, open), "low cardinality joins on the GPU are really bad". At 4 partitions the GPU took 151,469 ms against 31,828 ms on CPU. The GPU only won from about 64 partitions up: 14,685 ms against 19,776 ms — [#7529](https://github.com/NVIDIA/cudf-spark/issues/7529) (accessed 2026-10-29)
- Other open performance bugs where GPU mode is slower:
  - #15683: repeated Parquet scans with a non-pushdown regex are 1.77× slower (2026-08-16).
  - #15686: an arithmetic partition predicate is slower on GPU (2026-08-17).
  — [#15683](https://github.com/NVIDIA/cudf-spark/issues/15683), [#15686](https://github.com/NVIDIA/cudf-spark/issues/15686) (accessed 2026-10-29)

### Inferences
- The tools give no runtime per-job GPU/CPU router. The decision is offline (qualify, then opt in per workload). The tools' "AutoTuner" recommends configs; it was not verified to route jobs (estimate).
- The CBO's per-row constants are flat defaults, not calibrated per operator. Being off by default suggests NVIDIA did not trust it (estimate).
- A router trained on pass-through history inherits #1995's class-imbalance problem: GPU-loses cases are rare and get misclassified.

### Gaps
- Published QualX accuracy figures were not found.
- The NVIDIA "GPUs accelerate only ~20% of a typical Spark job" claim was not found (search budget exhausted).
- Whether the AutoTuner makes any per-job GPU/CPU decision was not verified.

## 2. Presto / Velox cuDF / Gluten

### Takeaway
Velox cuDF replaces CPU operators with GPU operators when it can and falls back per operator on failure (on by default, with logging). There is no size-based logic beyond batch-concat thresholds. Presto PR #28678 adds a manual per-query on/off session switch; it does not route automatically.

### Cited Findings
- prestodb/presto PR #28678, "feat(native): Add a per-query native_cudf_enabled session property":
  - Status: opened 2026-10-19 by lga-zurich, open and not merged. The test merge SHA is dada068b.
  - The property maps to the Velox query config `cudf.enabled`.
  - Unset: the worker default applies. `false`: the whole query uses CPU operators and the in-memory exchange. `true`: fails on a worker without cuDF "instead of quietly running on the CPU".
  - It copies `cudf.enabled` into every catalog's connector config, so the cuDF Hive and Iceberg connectors don't emit `CudfVector`s into CPU operators.
  - `false` combined with `native_cudf_exchange_enabled=true` is a user error.
  - Motivation: toggling used to need a worker config edit and restart; the property makes "checking a GPU result against the CPU or measuring the GPU speed-up" possible per query.
  - Depends on velox#19637 and is stacked on presto#28398.
  — [PR #28678](https://github.com/prestodb/presto/pull/28678) (accessed 2026-10-29)
- velox PR #19637, "feat(cudf): Let a query turn GPU operators off" (opened 2026-10-19, open) — [velox#19637](https://github.com/facebookincubator/velox/pull/19637) (accessed 2026-10-29)
- Velox `CudfConfig.h` @bcdab35c:
  - `enabled{true}` ("Clients can disable here and enable it via the QueryConfig").
  - `allowCpuFallback{true}`: "Allow fallback to CPU operators if GPU operator replacement fails".
  - `logFallback{true}`: logs the reason for falling back.
  - `batchSizeMinThreshold{100000}` rows, or `batch_size_min_bytes`, to accumulate before GPU concat. This is a batching knob, not routing.
  - Operator replacement happens in `exec/ToCudf.cpp`.
  — [CudfConfig.h](https://github.com/facebookincubator/velox/blob/bcdab35c839ef3dd4af8b75d5b38ae84b9c51fa0/velox/experimental/cudf/CudfConfig.h) (accessed 2026-10-29)
- On EKS TPC-DS 1TB, Gluten/Velox had the fastest total time, and RAPIDS GPU "excels on specific query patterns" with the lowest job cost. This is per-query heterogeneity — [data-on-eks](https://awslabs.github.io/data-on-eks/docs/benchmarks/spark-rapids-gpu-benchmark) (search snippet; accessed 2026-10-29)

### Inferences
- An external router could use the PR #28678 switch, or `spark.rapids.sql.enabled`, as its actuator. Neither engine has the decision logic itself.

### Gaps
- Gluten GPU (cuDF backend) fallback behavior was not researched (the GitHub search was rejected for the repo name and the web search budget was exhausted).

## 3. Sirius (sirius-db/sirius)

### Takeaway
Fallback to DuckDB CPU is whole-query and capability- or error-driven (`enable_duckdb_fallback`, default true, silent). A size-gated CPU fallback for small queries has been proposed (#990) but not built. Size estimates exist, but only to choose the number of GPUs (#2147).

### Cited Findings
- Issue #2174 (2026-10-14, open):
  - With the `latest` release (0ef5a30) in the official DuckDB CLI, every Parquet scan silently runs on CPU. The cause is the scan-certification check from #1956: `dlsym` cannot find `ParquetScanFunction::GetFunctionSet`, because the CLI exports no DuckDB symbols.
  - With the default `enable_duckdb_fallback=true` the query "returns 1000 from the CPU and prints nothing". The only trace is a line in sirius.log.
  - With fallback off it errors: "GPU plan generation failed: Table function 'read_parquet' is not supported".
  - The setup was EMR on a g7.4xlarge with an RTX PRO 4500, TPC-H and TPC-DS SF100.
  — [#2174](https://github.com/sirius-db/sirius/issues/2174) (accessed 2026-10-29)
- Issue #990 (2026-06-22, open), "Investigate removing operators if estimated data size is small". The investigation comment (Jedi18, 2026-06-29) says:
  - Whole-query fallback exists via `run_internal_cpu_fallback_query` in `src/sirius_extension.cpp`, but it is used only "for unsupported plans or GPU execution errors, not as a size-based optimization".
  - It proposes "a plan-time, size-gated trigger: if the whole query's source cardinality is tiny, skip GPU execution entirely and let DuckDB run it on CPU".
  - It warns that mid-plan small intermediates must not trigger whole-query fallback, and that S3-backed Sirius reads cannot fall back to the CPU.
  — [#990](https://github.com/sirius-db/sirius/issues/990) (accessed 2026-10-29)
- PR #2147 (2026-10-13, open), "size GPU admission from real scan sizes":
  - `admission_bytes_per_gpu` gives each query ⌈estimate ÷ setting⌉ GPUs.
  - The old estimate, planner filtered cardinality × 32 bytes per string column, was 4–7× off for a third of TPC-H SF100/SF1000 queries (Q1 0.26×, Q3 0.20×, Q13 0.23×, Q6 7×). Under-estimates regressed SF1000 by up to 20%.
  - The new estimate takes the larger of the planner estimate and the unfiltered row count, and uses Parquet size statistics for column widths.
  - The new default is the smallest visible GPU's memory ÷ 24, clamped to [1, 8] GiB (L4 → 1 GiB, H100 → 3.3 GiB).
  — [#2147](https://github.com/sirius-db/sirius/pull/2147) (accessed 2026-10-29)

### Inferences
- The cardinality-estimate errors in #2147 show how hard size-based routing is: DuckDB planner estimates were off by up to 5×, in both directions.

## 4. Microsoft Fabric DW GPU acceleration and CoddSpeed

### Gaps
- Not researched. The search budget was exhausted before I found the 2026-06 Fabric blog or the CoddSpeed paper URL, and I have no source for how Fabric routes queries to the GPU.

## 5. Other GPU databases with hybrid placement

### Cited Findings
- ClickHouse RFC #116807, "[RFC] ClickHouse GPU v1" (2026-08-27, open):
  - Plans to build on Sirius via Substrait.
  - A `src/Planner/SiriusEligibility.cpp` "decides which subtrees are eligible for the GPU". Functions or types without a Substrait mapping "cause the query to fall back to the CPU".
  - `Decimal256` stays on the CPU.
  - Eligibility is capability-based; the RFC mentions no cost model.
  — [#116807](https://github.com/ClickHouse/ClickHouse/issues/116807) (accessed 2026-10-29)

### Gaps
- PG-Strom: the fetch of the parameters page failed (heterodb.github.io). From prior knowledge, PG-Strom adds GPU paths to the PostgreSQL planner with costs from `pg_strom.gpu_setup_cost`, `gpu_tuple_cost` and `gpu_operator_cost`, so the CPU plan wins on small tables. Unverified; needs a citation.
- HeavyDB: docs.heavy.ai did not resolve. From prior knowledge, it has `--cpu-only`, `allow-cpu-retry` and `allow-query-step-cpu-retry` (retry on CPU when the GPU runs out of memory). Unverified.
- Kinetica, BlazingSQL, Theseus and KX GPU were not researched (no search budget).

## 6. First-person complaints and requests for automatic routing

### Cited Findings
- HackerNoon (2026-10-20): the GPU was slower end-to-end despite a "Large" rating. The author recommends per-workload opt-in, which amounts to manual routing — [HackerNoon](https://hackernoon.com/benchmarking-nvidia-rapids-on-gke-estimated-vs-actual-spark-speedups) (accessed 2026-10-29)
- #1995: the user's model "is very prone to misclassifying low-speed tasks as high-speed" — [#1995](https://github.com/NVIDIA/cudf-spark-tools/issues/1995) (accessed 2026-10-29)
- #7529: a join is about 5× slower on GPU at low partition counts — [#7529](https://github.com/NVIDIA/cudf-spark/issues/7529) (accessed 2026-10-29)
- Sirius #990: a maintainer-side proposal to send tiny queries to the CPU — [#990](https://github.com/sirius-db/sirius/issues/990) (accessed 2026-10-29)
- Presto #28678: the motivation is comparing GPU and CPU per query without restarting workers — [PR #28678](https://github.com/prestodb/presto/pull/28678) (accessed 2026-10-29)

### Inferences
- No engine I found automatically routes a whole query by estimated size. The pattern everywhere is capability-based fallback, often silent (Sirius #2174, Velox `allowCpuFallback`), plus manual switches. The gap is real for a gpu-cpu-router, and so are the hard parts: misestimates (#2147) and skewed training data (#1995).

### Gaps
- Forum and Reddit posts about GPUs being slower on small inputs were not searched.
