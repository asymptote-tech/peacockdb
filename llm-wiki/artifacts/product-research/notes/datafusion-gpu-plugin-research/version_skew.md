# DataFusion version skew across target hosts, and what it costs a GPU execution plugin

All facts accessed 2026-10-29 unless noted. HEAD SHAs come from `gh api repos/<r>/commits/<default-branch>`. Raw files were read from `https://raw.githubusercontent.com/<repo>/<sha>/<path>`. "lock" means the resolved Cargo.lock entry and "toml" means the Cargo.toml requirement.

## Q1. Which DataFusion version is pinned at each target's default-branch HEAD

### Takeaway
The active hosts cluster on DataFusion **54.1–55.2**. The latest release is **55.2.0 (2026-10-07)**, and peacockdb is on 45, about 10 majors behind. Upstream ships **55.x** across OpenObserve, Parseable, GreptimeDB (a fork), Comet, Spice (a fork) and Lance. Arroyo (a fork) and Beacon are on **54.1**. InfluxDB 3 and NVIDIA GQE are on **52.x** (InfluxDB through a fork). HoraeDB (43) and Seafowl (44) are dead or archived. Databend does not use DataFusion. Quickwit added DataFusion in April 2026 and removed it in September 2026.

### Cited Findings

| Target | Repo @ HEAD SHA (commit date) | toml line | lock line | Fork? |
|---|---|---|---|---|
| OpenObserve | [openobserve/openobserve@f8827251](https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/Cargo.toml#L438) (2026-10-29) | `datafusion = "55.1.0"` (L438), `datafusion-proto = "55.1.0"` | `datafusion 55.1.0`, crates.io registry | No |
| Parseable | [parseablehq/parseable@bbbb79ec](https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/Cargo.toml#L22) (2026-10-29) | `datafusion = "55.0.0"` (L22) | `datafusion 55.0.0`, registry | No |
| GreptimeDB | [GreptimeTeam/greptimedb@d33c6641](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/Cargo.toml#L134) (2026-10-29) | `datafusion = "=55.1.0"` (L134) plus about 15 `datafusion-*` crates pinned `=55.1.0`; `[patch.crates-io] datafusion = { git = "https://github.com/GreptimeTeam/datafusion.git", rev = "cc5534492d5408831395609cdafb6520c7d514b7" }` (L372) | `datafusion 55.1.0`, source `git+https://github.com/GreptimeTeam/datafusion.git?rev=cc5534492d…` | **Yes** |
| InfluxDB 3 (OSS/Core) | [influxdata/influxdb@185688a5](https://github.com/influxdata/influxdb/blob/185688a5a813016c0d26ec10abbad2ca770670ed/Cargo.toml#L167) (2026-10-22) | `datafusion = { version = "52.5.0" }` (L167); patched at L337: `datafusion = { git = "https://github.com/influxdata/arrow-datafusion.git", rev = "e9e2e6d36bf63e39f746f82ee9b33c47feb65361" }` (also -common, -execution, -expr, -proto, -sql) | `datafusion 52.5.0`, source `git+https://github.com/influxdata/arrow-datafusion.git?rev=e9e2e6d3…` | **Yes** |
| influxdb3_core | [influxdata/influxdb3_core@349d802b](https://github.com/influxdata/influxdb3_core/blob/349d802bf9a54ab5876399ab013502eeb0d52a6b/Cargo.toml#L94) (2026-01-27). The repo is **archived** (GitHub API `archived: true`) | `datafusion = { version = "50.3.0" }` (L94); patched to `influxdata/arrow-datafusion` rev `5f35b8287ea793772c7d89d412908af84af0d901` (L187) | n/a | Yes (archived) |
| HoraeDB | [apache/horaedb@8f9698cd](https://github.com/apache/horaedb/blob/8f9698cdf83d02e12b93581464114d4e7cef483a/Cargo.toml#L53) (2025-12-11). The repo is **archived** | `datafusion = "43"` (L53) | `datafusion 43.0.0`, registry | No |
| Quickwit | [quickwit-oss/quickwit@30938ed9](https://github.com/quickwit-oss/quickwit/blob/30938ed9ce48e09a802ec9a316e0c1e8ab46aea7/quickwit/Cargo.toml) (2026-10-28) | **No DataFusion** in `quickwit/Cargo.toml` or `quickwit/Cargo.lock` at HEAD (grep is empty) | none | — |
| MARIS Beacon | [maris-development/beacon@a612fcec](https://github.com/maris-development/beacon/blob/a612fcec6c8493902c7b8148e51c4316d34ac6db/Cargo.toml#L76) (2026-10-29) | `datafusion = "^54.1"` (L76); also `datafusion-ffi = "^54.1"` (L79), `datafusion-table-providers = "=0.12.1"`, `datafusion-federation = "=0.5.5"` | lock contains **two** copies: `datafusion 54.1.0` and `datafusion 53.1.0` (the 53.1 copy is presumably pulled in transitively by a contrib crate, estimate) | No |
| Apache DataFusion Comet | [apache/datafusion-comet@7f73d86f](https://github.com/apache/datafusion-comet/blob/7f73d86fb841f5662f1343cf52b6e6b31eb17dfb/native/Cargo.toml#L47) (2026-10-28) | `native/Cargo.toml` L47: `datafusion = { version = "55.2.0", default-features = false, … }`, also `datafusion-spark = "55.2.0"` | `datafusion 55.2.0`, registry | No |
| Spice AI | [spiceai/spiceai@e1460121](https://github.com/spiceai/spiceai/blob/e146012130aaf6fbf71ef9563078834a08f207d3/Cargo.toml#L282) (branch `trunk`, 2026-10-29) | `datafusion = "55.2.0"` (L282); Ballista and datafusion-federation come from spiceai forks, "branch: spiceai-55" | `datafusion 55.2.0`, source `git+https://github.com/spiceai/datafusion.git?rev=2644c0d5aad82c29c7c02a6617a71c664f55e94c` | **Yes** |
| Lance | [lancedb/lance@06315516](https://github.com/lancedb/lance/blob/0631551696d4c89f3e6a23c09f9ff0be0b534fcd/Cargo.toml#L138) (2026-10-29) | `datafusion = { version = "55.0.0", default-features = false, … }` (L138) | `datafusion 55.2.0`, registry | No |
| Arroyo | [ArroyoSystems/arroyo@d85f70e2](https://github.com/ArroyoSystems/arroyo/blob/d85f70e2453d1dd5a1ef9c4f793ef3774a3fcd76/Cargo.toml#L52) (2026-10-28) | `datafusion = { version = "=54.1.0" }` (L52); patched at L124 to `ArroyoSystems/arrow-datafusion` rev `0b17b6de84ede334a292c5e3037beb550a90d184` | `datafusion 54.1.0`, git fork | **Yes** |
| Databend | [databendlabs/databend@7d0e2073](https://github.com/databendlabs/databend/blob/7d0e20734bb6c5ad968ef04730d16da8a6da7f1d/Cargo.lock) (2026-10-29) | — | `grep -c datafusion Cargo.lock` = **0**, so Databend does not use DataFusion | — |
| Seafowl | [splitgraph/seafowl@c126a02e](https://github.com/splitgraph/seafowl/blob/c126a02e419046ce377f80a87ecef30cc8bc7d44/Cargo.toml#L17) (last commit 2025-01-08) | `datafusion = { version = "44.0.0", … }` | `datafusion 44.0.0` | No (inactive) |
| NVIDIA GQE | [rapidsai/gqe@50facc46](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/rust/gqe-cli/Cargo.toml#L35) (2026-08-10) | `rust/gqe-cli/Cargo.toml` L35: `datafusion = { version = "52.3.0", default-features = true }`, plus `datafusion-substrait = "52.3.0"`; `rust/producer/Cargo.toml` L33 is the same | `rust/Cargo.lock`: `datafusion 52.5.0`, `datafusion-substrait 52.5.0`, `substrait 0.62.2` | No |
| Upstream DataFusion main | [apache/datafusion@14aa185d](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/Cargo.toml#L126) (2026-10-29) | `datafusion = { path = "datafusion/core", version = "55.1.0" … }`. The version field on main lags; main already carries a `56.0.0` upgrade-guide page | — | — |

- **Latest release:** `datafusion` **55.2.0**, published 2026-10-07 per the [crates.io versions API](https://crates.io/api/v1/crates/datafusion/versions). 55.1.0 was published 2026-09-11 and 55.0.0 on 2026-08-18. `datafusion-ffi` is also at 55.2.0 ([docs.rs](https://docs.rs/datafusion-ffi/latest/datafusion_ffi/)).
- **Unreleased 56.0.0:** main already has the [56.0.0 upgrade notes](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/56.0.0.md), which upgrade arrow/parquet to 60.0.0 and raise the MSRV to 1.95.0. 56.0.0 is not yet on crates.io.
- **Quickwit history:**
  - Datadog announced it had acquired Quickwit on 2025-01-09 ([Datadog blog](https://www.datadoghq.com/blog/datadog-acquires-quickwit/)).
  - [PR #6276 "feat(datafusion): DataFusion metrics query layer"](https://github.com/quickwit-oss/quickwit/pull/6276) was merged 2026-04-24 (merge commit b0155f8626b7ce5241043ea1d29b811bc72f18f8). It added `quickwit-datafusion` and `quickwit-df-core` crates for SQL/Substrait over Parquet metrics. The workspace kept these crates out of default members ("DataFusion crates are opt-in and covered by their own CI lane", quickwit/Cargo.toml at b0155f86).
  - [PR #6809 "Remove Parquet code"](https://github.com/quickwit-oss/quickwit/pull/6809) was merged 2026-09-28: "Removes the unused Parquet code. Commit can be reverted if we want it back." This is the last commit that touched `quickwit/quickwit-datafusion`, and that path returns 404 at HEAD.
  - Quickwit therefore has **no DataFusion today**.
- **LanceDB:** the [lancedb/lancedb](https://github.com/lancedb/lancedb) HEAD is 005a9a353a04874c95586c7c734871d8955b1bfd. LanceDB consumes Lance; I did not separately read its lock file.

### Inferences
- The modal host version today is 55.x. Peacockdb (45) would need to move up 10 majors before it could link into most hosts.
- Even within the 55 cluster, the exact lock versions differ (55.0, 55.1, 55.2), and two of those hosts are forks. One plugin binary or crate version will not fit all of them without per-host builds.
- GQE is NVIDIA's own GPU engine, and it touches DataFusion only as a Substrait producer (the `producer` crate plus `datafusion-substrait`). It does not consume DataFusion's ExecutionPlan. This suggests Substrait as a lower-coupling interface (estimate).

### Gaps
- I did not read the InfluxDB 3 Enterprise source (it is closed). Whether Enterprise tracks the same fork rev is unknown.
- I did not identify which crate pulls `datafusion 53.1.0` into the Beacon lock.

## Q2. Forks and how far each lags upstream

### Takeaway
Four targets patch DataFusion through `[patch.crates-io]` to their own fork: GreptimeDB, InfluxDB, Spice and Arroyo. Each fork is a release tag plus a carried patch set; none is a long divergence from an old base. The lag behind the latest release is 0 majors for Spice, 0 for Greptime (55.1 vs 55.2), 1 for Arroyo, and 3 for InfluxDB.

### Cited Findings
All "ahead/behind" figures come from `gh api repos/apache/datafusion/compare/<tag>...<fork>:<rev>`.
- **GreptimeDB:** fork rev `cc5534492d5408831395609cdafb6520c7d514b7` is **24 commits ahead, 0 behind** tag `55.1.0` ([compare](https://github.com/apache/datafusion/compare/55.1.0...GreptimeTeam:datafusion:cc5534492d5408831395609cdafb6520c7d514b7)). The carried patches are mostly optimizer and limit fixes, for example "preserve global limit for multi-partition fetch", "restore scalar expression planner hook" and "preserve fetched merges during distribution enforcement".
- **InfluxDB 3:** fork rev `e9e2e6d36bf63e39f746f82ee9b33c47feb65361` is **12 ahead, 0 behind** tag `52.5.0` ([compare](https://github.com/apache/datafusion/compare/52.5.0...influxdata:arrow-datafusion:e9e2e6d36bf63e39f746f82ee9b33c47feb65361)). The patches include "skip order calculation / exponential planning", "wrap join operators with cooperative() for cancellation support", "bump arrow/parquet to 57.3.0" and a revert of #19078. The base is 3 majors behind 55.2.
- **Spice AI:** fork rev `2644c0d5aad82c29c7c02a6617a71c664f55e94c` is **454 ahead, 0 behind** tag `55.2.0` ([compare](https://github.com/apache/datafusion/compare/55.2.0...spiceai:datafusion:2644c0d5aad82c29c7c02a6617a71c664f55e94c)). The count includes merge history such as "Merge tag '54.0.0' into spiceai" and re-ported patches like "Restore safe column-statistics access (re-port #121)", "keep ORDER BY under ScalarSubqueryExec root" and "re-add HashJoin accumulator display". Spice also forks Ballista and datafusion-federation on a `spiceai-55` branch ([Cargo.toml L254, L290](https://github.com/spiceai/spiceai/blob/e146012130aaf6fbf71ef9563078834a08f207d3/Cargo.toml#L254)).
- **Arroyo:** fork rev `0b17b6de84ede334a292c5e3037beb550a90d184` is **15 ahead, 0 behind** tag `54.1.0` ([compare](https://github.com/apache/datafusion/compare/54.1.0...ArroyoSystems:arrow-datafusion:0b17b6de84ede334a292c5e3037beb550a90d184)). The patches include "[Arroyo] Integrate streaming SQL AST with DataFusion 54", "Restore reusable accumulator checkpoint APIs" and "Remove all allocations in metrics system".
- **Not forked:** OpenObserve, Parseable, Comet, Lance, Beacon and GQE all resolve DataFusion from the crates.io registry (see the lock lines in Q1).

### Inferences
- The fork pattern (release tag plus 10–450 carried patches, rebased each major) shows that even large hosts rebase onto every one or two upstream majors. A plugin has to follow at the same pace.
- Because forks patch planner and optimizer behaviour (Greptime limit and distribution enforcement, Influx ordering), a plugin's physical-plan translator may see plan shapes that differ from stock DataFusion at the same version (estimate).

### Gaps
- I did not measure the size of each fork's diff in lines of code, only commit counts.

## Q3. Release cadence and API churn between 45 and the current version

### Takeaway
DataFusion ships a new major about every 5–10 weeks: 11 majors (45 to 55) in about 18 months, with patch releases backported to older lines. Each major breaks Rust APIs that a GPU translator touches directly: ExecutionPlan, PlanProperties, Statistics, the file scan nodes, schema adaptation, PhysicalExpr and the optimizer and planner traits. Moving 45 to 55 means rewriting the scan translation (ParquetExec was removed) and the statistics, properties and children handling.

### Cited Findings
- **Major release dates** from the [crates.io versions API](https://crates.io/api/v1/crates/datafusion/versions):

  | Major | Released |
  |---|---|
  | 45.0.0 | 2025-02-07 |
  | 46.0.0 | 2025-03-07 |
  | 47.0.0 | 2025-04-20 |
  | 48.0.0 | 2025-06-11 |
  | 49.0.0 | 2025-07-27 |
  | 50.0.0 | 2025-09-16 |
  | 51.0.0 | 2025-11-19 |
  | 52.0.0 | 2026-01-12 |
  | 53.0.0 | 2026-03-23 |
  | 54.0.0 | 2026-06-08 |
  | 55.0.0 | 2026-08-18 |

- **Patch releases on older lines:** 52.5.0 shipped 2026-04-11, after 53.0.0; the 55.x line has 55.1.0 (2026-09-11) and 55.2.0 (2026-10-07) ([crates.io](https://crates.io/api/v1/crates/datafusion/versions)).
- **Upgrade guide:** the [upgrade guide](https://datafusion.apache.org/library-user-guide/upgrading.html) has one page per major. I read the sources at [docs/source/library-user-guide/upgrading/ @ 14aa185d](https://github.com/apache/datafusion/tree/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading). Page sizes are 46: 310 lines, 47: 135, 48: 244, 49: 222, 50: 330, 51: 272, 52: 669, 53: 551, 54: 892, 55: 1534, and 56 (unreleased): 1338. The pages have grown sharply in 2026.

**Concrete breaks between 45 and 55/56 that are relevant to a plugin consuming ExecutionPlan trees.** The source for each is the upgrade page for that version at the SHA above.

- **46** ([46.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/46.0.0.md)):
  - "`ParquetExec`, `AvroExec`, `CsvExec`, `JsonExec` deprecated" — "Instead of individual `ExecutionPlan`s for the different file formats they now all use `DataSourceExec` and the format specific information is embodied in new traits `DataSource` and `FileSource`" (PR #14224).
  - UDFs move to `invoke_with_args` in place of `invoke()`/`invoke_batch()`.
- **47** ([47.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/47.0.0.md)):
  - `FileScanConfig` becomes `FileScanConfigBuilder`.
  - Deprecated APIs are removed.
  - Upgrade to arrow 55 and object_store 0.12.
  - `DisplayFormatType::TreeRender` is added.
- **48** ([48.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/48.0.0.md)):
  - "`ParquetExec`, `AvroExec`, `CsvExec`, `JsonExec` Removed".
  - "Physical Expression return `Field`".
  - `Expr::Literal` gains optional metadata.
  - `FileFormat::supports_filters_pushdown` is replaced by `FileSource::try_pushdown_filters`.
  - `VARCHAR` maps to `Utf8View`.
  - `PartitionedFile` is added to `FileOpener`.
- **49** ([49.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/49.0.0.md)):
  - "Deprecating `SchemaAdapterFactory` and `SchemaAdapter`" (replaced by `PhysicalExprAdapter`).
  - `DataFusionError` variants are boxed.
  - `FieldMetadata` is introduced.
- **50** ([50.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/50.0.0.md)):
  - "Added `ExecutionPlan::reset_state`".
  - `ProjectionExpr` changes from a type alias to a struct.
  - "Refactored `DataSource::try_swapping_with_projection`".
  - "Added `PhysicalExpr::is_volatile_node`".
  - `SessionState`/`ConfigOptions` now return `&Arc<ConfigOptions>`.
  - Arrow 56.
  - A breaking FFI UDAF signature change.
- **51** ([51.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/51.0.0.md)):
  - `FileScanConfig::projection` is renamed to `projection_exprs`.
  - `TableSchema` is introduced and `FileSource::with_schema()` changes.
  - `ListingTable` moves to `datafusion-catalog-listing`.
  - datafusion-proto uses `TaskContext`.
  - Arrow 57.
- **52** ([52.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/52.0.0.md)):
  - "`SchemaAdapter` and `SchemaAdapterFactory` completely removed".
  - "Statistics handling moved from `FileSource` to `FileScanConfig`".
  - "Projection handling moved from FileScanConfig to FileSource".
  - "Partition column handling moved out of `PhysicalExprAdapter`".
  - `FileSource` constructors now take schemas up front.
  - The DFSchema API changes.
  - FFI crate updates: `FFI_TableProvider::new` gains ctx and codec arguments.
- **53** ([53.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/53.0.0.md)):
  - "`ExecutionPlan::statistics` removed".
  - "`ExecutionPlan::properties` now returns `&Arc<PlanProperties>`" instead of `&PlanProperties`. Every custom node must change its `cache: PlanProperties` field to `Arc<PlanProperties>`.
  - `FilterExec` builder methods are deprecated.
  - `HashJoinExec::try_new` adds `null_aware`.
  - Arrow 58 and object_store 0.13.
- **54** ([54.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/54.0.0.md)):
  - "`ExecutionPlan::partition_statistics` now returns `Arc<Statistics>`".
  - "Remove `as_any` from `PhysicalExpr`, … `ExecutionPlan`, `TableProvider`, … `FileSource`, `DataSource`" in favour of trait upcasting. A downcast-based translator such as peacockdb's (which matches on node type) must change its downcast idiom.
  - "`CastColumnExpr` removed in favor of field-aware `CastExpr`".
  - "Physical `EXPLAIN` now shows lowered aggregate execution forms".
  - Scalar subquery execution changes.
  - Higher-order functions and lambdas add three new `Expr` variants.
- **55** ([55.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/55.0.0.md)):
  - "`ExecutionPlan::apply_expressions` is now a **required** method on the `ExecutionPlan`, `FileSource`, and `DataSource` traits".
  - "`ExecutionPlan::partition_statistics` deprecated in favor of `statistics_from_inputs`", driven by a new `StatisticsContext`. The guide warns that the delegation is one-way: nodes that override only `statistics_from_inputs` silently return unknown statistics from `partition_statistics`.
  - "`ExecutionPlan::with_new_children` and `with_new_children_and_same_properties` deprecated" in favour of `replace_children(ReplaceChildrenOptions{ChildrenPropertiesMode::Keep|Recompute})`.
  - "`TableProvider::scan` takes `Option<&[usize]>` for the projection".
  - "`QueryPlanner` adds `Any` as a supertrait".
  - "Physical-planning state moved to an explicit `PhysicalPlanningContext`".
  - "Catalog, planner, and optimizer contracts moved to `datafusion-session`" (`TableProvider`, `QueryPlanner`, `PhysicalPlanner`, `ExtensionPlanner`, `PhysicalOptimizerRule`, `PhysicalOptimizerContext`). Old paths keep working through re-exports.
  - "Filter predicate evaluation order may differ from query text".
  - `GroupsAccumulator::convert_to_state` is now required.
  - MSRV 1.94.
- **56, unreleased** ([56.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/56.0.0.md)):
  - "Physical optimizer rules share one `StatisticsContext` per query".
  - "Physical filter pushdown resolves columns by position".
  - "`FileSource::exact_filter` controls scan equivalence properties".
  - "`TableProvider::scan_with_args` supports skip pushdown".
  - "`ensure_distribution` is deprecated in favour of `ensure_distribution_with_stats`".
  - A new `LogicalPlan::MaterializedCte` variant.
  - Arrow 60.
  - MSRV 1.95.

### Inferences
- Every major from 46 to 55 touches at least one of ExecutionPlan, PlanProperties, Statistics, FileSource/DataSourceExec or PhysicalExpr. A node-by-node ExecutionPlan translator should expect real porting work on every major, about every 2 months in 2026 (estimate).
- The 45 to 46/48 DataSourceExec rewrite alone forces a rewrite of the scan leaf translation: ParquetExec matching becomes `DataSourceExec` → `FileScanConfig` → `ParquetSource`.
- Arrow also moves a major almost every DataFusion major (55 → 56 → 57 → 58 → … → 60). A C++/cuDF executor that exchanges Arrow over the C Data Interface is insulated from the Rust arrow crate version, but the Rust side is not (estimate).
- In 2026 the cadence has slowed to about 10 weeks per major (52 → 53 → 54 → 55), compared with about 6 weeks in 2025, while each guide page has grown 2–5×. Fewer but larger breaks (estimate based on page line counts).

### Gaps
- The rendered upgrade guide on datafusion.apache.org came back empty through WebFetch, so I cited the GitHub sources at SHA 14aa185d instead. The content should be identical.
- I found no official statement of a release cadence policy. The cadence above is derived from crates.io dates.

## Q4. Does datafusion-ffi give a stable plugin ABI across majors?

### Takeaway
`datafusion-ffi` is a real C-ABI layer: stabby/async-ffi types, with FFI_TableProvider, FFI_ExecutionPlan, FFI_PhysicalExpr, FFI_Session, and from 54/55 also FFI_PhysicalOptimizerRule and FFI_QueryPlanner. It is **not ABI-stable across majors in practice**. The crate exposes `version()` returning only the major and tells users to keep producer and consumer on the same version. The stabilization issue is still open, and the FFI surface itself changed in 49, 50, 52, 54 and 55. A GPU plugin loaded over FFI would still need one build per host major.

### Cited Findings
- **README.** The [datafusion-ffi README @ 14aa185d](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/datafusion/ffi/README.md) says: "The purpose of this crate is to define interfaces between DataFusion libraries that will remain stable across different versions of DataFusion… In general, it is recommended to run the same version of DataFusion by both the producer and consumer… but this is not strictly required."
  - The stated use cases are datafusion-python and runtime loading of libraries for extra data sources.
  - Its stated limitation is that it is "designed specifically to work across Rust libraries"; for other languages it recommends a Rust wrapper crate.
  - It now uses `stabby`, after switching from the unmaintained `abi_stable` in [PR #21030](https://github.com/apache/datafusion/commit/af7904f635) (2026-04-27, included in 54.0.0 per `compare af7904f635...54.0.0` = ahead). That switch is itself an ABI break for 53 → 54.
- **Version check.** [lib.rs @ 14aa185d](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/datafusion/ffi/src/lib.rs#L51) has `pub extern "C" fn version() -> u64 { … version.major }` with the comment "we use the major version to identify compatibility over the unsafe boundary. This call is intended to be used by implementers to validate they have compatible libraries."
- **Modules in datafusion-ffi 55.2.0** ([docs.rs](https://docs.rs/datafusion-ffi/latest/datafusion_ffi/)): arrow_wrappers, catalog_provider(_list), config, execution, execution_plan, expr, physical_expr, **physical_optimizer**, plan_properties, **query_planner**, record_batch_stream, schema_provider, session, statistics, table_provider(_factory), table_source, udf/udaf/udtf/udwf, and others.
- **When each FFI type landed** (git history of `datafusion/ffi/src`, with release inclusion checked by `gh api compare`):
  - FFI initial implementation, including execution_plan: [#12920](https://github.com/apache/datafusion/pull/12920), 2024-10-30 (about 43).
  - `FFI_PhysicalExpr`: [#18916](https://github.com/apache/datafusion/pull/18916), 2025-12-03, in 52.0.0.
  - `FFI_Session`: [#19223](https://github.com/apache/datafusion/pull/19223), 2025-12-11, in 52.0.0.
  - `FFI_PhysicalOptimizerRule`: [#20451](https://github.com/apache/datafusion/pull/20451), 2026-03-26, in 54.0.0 and **not** 53.0.0.
  - `FFI_QueryPlanner`: [#24028](https://github.com/apache/datafusion/pull/24028), 2026-08-07. It is not in 55.0.0 but is in 55.1.0 and 55.2.0 (backported to branch-55).
- **Recorded FFI breaks:**
  - 50.0.0 guide: "This update is a breaking change to the FFI API. The current best practice when using the FFI crate is to ensure that all libraries that are interacting are using the same underlying Rust version. Issue #17374 has been opened to discuss stabilization" ([50.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/50.0.0.md)).
  - 52.0.0: "FFI crate updates", where `FFI_TableProvider::new(table, None)` became `FFI_TableProvider::new(table, None, ctx, None)` ([52.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/52.0.0.md)).
  - 55.0.0: `FFI_GroupsAccumulator` changes and "`FFI_LogicalExtensionCodec::task_ctx_provider` is now private" ([55.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/55.0.0.md)).
- **Issue #17374.** [apache/datafusion#17374 "Stabilize FFI Boundary"](https://github.com/apache/datafusion/issues/17374) is **OPEN**. It says: "Between DataFusion 48 and 49 there was a breaking change in the FFI boundary for record batch streams… another planned breaking change from 49 to 50… For Python this means that the datafusion-python version must match exactly the rust version used in the library."
- **First explicit cross-major compatibility attempt (56, unreleased).** The 56.0.0 guide says "`ForeignSession::create_physical_plan` is unsupported… The original `FFI_SessionRef` callback slot remains in place for ABI compatibility with DataFusion 55 consumers." It tells the session owner to export its planner as `FFI_QueryPlanner` before installing a foreign planner ([56.0.0.md](https://github.com/apache/datafusion/blob/14aa185dd23d83d100eff3e3824bffd60df15e41/docs/source/library-user-guide/upgrading/56.0.0.md)). This is the first wording I found that aims at ABI compatibility from one major to the next (55 to 56), and it removes behaviour while doing so.
- **Real consumer.** MARIS Beacon already depends on `datafusion-ffi = "^54.1"` ([Cargo.toml L79](https://github.com/maris-development/beacon/blob/a612fcec6c8493902c7b8148e51c4316d34ac6db/Cargo.toml#L79)). None of OpenObserve, Parseable, GreptimeDB or InfluxDB depend on datafusion-ffi (GitHub code search for `datafusion-ffi` in openobserve and parseable returned no hits).

### Inferences
- A plugin shipped as an FFI_PhysicalOptimizerRule or FFI_QueryPlanner `.so` is technically possible only for hosts on 54+ (rule) or 55.1+ (planner). Hosts must also add the loader code themselves, because none of the major targets loads FFI plugins today.
- Even through FFI, the plugin sees host plans as `ForeignExecutionPlan` handles. It cannot downcast them to concrete node types (`AggregateExec`, `HashJoinExec`, `DataSourceExec`), and the 56 note on "cannot restore local Rust type identities for downcasting" confirms this. Peacockdb's node-by-node translation depends on downcasting, so FFI does not remove the need to compile against the host's exact DataFusion. The more realistic portable interface is a serialized plan: datafusion-proto or Substrait (estimate).
- In practice, the cost of skew is one build per host major and per fork rev, a porting effort every 1–2 months, and fork-specific plan shapes.

### Gaps
- I did not verify the exact ABI breakage in 54 → 55 struct layouts, for example by diffing `FFI_ExecutionPlan` field lists.
- I found no project that runs a datafusion-ffi plugin across different DataFusion majors in production.

## Q5. Do the targets accept outside extension points without forking?

### Takeaway
All are extensible only at **compile time**, in Rust, through DataFusion's own builder hooks: custom TableProvider, PhysicalOptimizerRule and QueryPlanner. None loads plugins at runtime. Parseable has an explicit registration hook for extra physical optimizer rules, meant for enterprise and plugins. OpenObserve hardwires its rules and its own QueryPlanner, and gates extra functionality behind a private `o2_enterprise` crate. GreptimeDB has a compile-time `plugins` crate.

### Cited Findings
- **Parseable** ([src/query/mod.rs @ bbbb79ec](https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/query/mod.rs#L96)):
  - `/// Additional physical optimizer rules registered by enterprise/plugins. /// Must be populated BEFORE QUERY_SESSION_STATE is first accessed.`
  - `pub static ADDITIONAL_PHYSICAL_OPTIMIZER_RULES: Lazy<RwLock<Vec<Arc<dyn PhysicalOptimizerRule + Send + Sync>>>>`.
  - In `create_session_state` (around L410–419) each rule is appended with `builder.with_physical_optimizer_rule(...)`, after `with_default_features()`.
  - `pub static SCHEMA_PROVIDER: OnceCell<Box<dyn ParseableSchemaProvider>>` is a second injection point.
  - Parseable has one `impl TableProvider`, in `src/query/stream_schema_provider.rs` (GitHub code search).
  - Parseable has no custom QueryPlanner (code search returned no hits).
- **OpenObserve** ([src/search/src/datafusion/exec.rs @ f8827251](https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/src/search/src/datafusion/exec.rs#L202)):
  - A session builder holds `optimizer_rules` and `physical_optimizer_rules: Vec<Arc<dyn PhysicalOptimizerRule + Send + Sync>>`, applied through `SessionStateBuilder::with_physical_optimizer_rules(...)` (which **replaces** the default rule list) and `with_query_planner(Arc::new(OpenobserveQueryPlanner::new()))` (L279–293).
  - OpenObserve's own physical rules live in `src/search/src/datafusion/optimizer/physical_optimizer/`: aggregate_topk, index_optimizer, join_reorder, remote_scan, rewrite_match, shared_subplan.
  - Its TableProviders are `src/search/src/datafusion/table_provider/` (catalog, empty_table, enrich_table, listing_adapter, memtable, uniontable).
  - Enterprise code is behind `#[cfg(feature = "enterprise")] use o2_enterprise::…` (exec.rs L50–51, L355–361), and the root [Cargo.toml](https://github.com/openobserve/openobserve/blob/f88272514512336c2f24d5e8d7b00735094ae439/Cargo.toml) has an `enterprise = [...]` feature.
  - There is no public runtime plugin API; extension means modifying the source or building with your own crate in place of `o2_enterprise`. That last option is an estimate.
- **GreptimeDB:** [src/plugins/src/lib.rs @ d33c6641](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/src/plugins/src/lib.rs) exports compile-time hooks: `setup_datanode_plugins_pre_build/post_build`, `setup_frontend_plugins_pre_build/post_build`, `setup_flownode_plugins_*`, `setup_metasrv_plugins_*` and `setup_standalone_plugins`. These are Rust functions linked into the binary, not dynamic loading.
- **MARIS Beacon** is the only target that already links `datafusion-ffi` ([Cargo.toml L79](https://github.com/maris-development/beacon/blob/a612fcec6c8493902c7b8148e51c4316d34ac6db/Cargo.toml#L79)). I did not verify whether it loads external `.so` providers at runtime.

### Inferences
- In practice, "plugin" means a Rust crate compiled into the host, built against the host's exact DataFusion version or fork rev. For Parseable that is a small patch that pushes a rule into `ADDITIONAL_PHYSICAL_OPTIMIZER_RULES`. For OpenObserve it is an edit to `exec.rs` that adds a rule to its list or wraps `OpenobserveQueryPlanner`.
- OpenObserve's `with_physical_optimizer_rules` replaces DataFusion's defaults, so a GPU rule has to be ordered relative to OpenObserve's own rules such as remote_scan and shared_subplan. Its distributed plans contain custom nodes (RemoteScan, SharedSubplanExec) that a GPU translator would have to pass through or handle (estimate).

### Gaps
- I did not inspect how InfluxDB 3, Spice or Comet register custom physical rules. Comet is itself a Spark plugin that translates Spark plans to DataFusion, so it is a different integration shape.
- I did not inspect OpenObserve's enterprise crate, which is private.
