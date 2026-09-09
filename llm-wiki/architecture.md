# peacockdb architecture

The code is authoritative. Where this page and the code disagree, the code is right and
this page is stale — fix the page (and say so) rather than the reading.

Pipeline: SQL → DataFusion logical/physical plan → the batch-partitioned node tree
(`peacockdb-core/src/batch_partitioned/`) → a recipe plan in the FlatBuffers vocabulary
(`flatbuffers/gpu_plan.fbs`) → the C++/cuDF executor, one node at a time. One tree runs on
either backend: `CpuBackend` relays a call to DataFusion, `GpuBackend` makes it through the
C ABI. What a node is, what the drivers do with it, and how memory is accounted are in
[`tasks/batch_partitioned_executor.md`](tasks/batch_partitioned_executor.md). This page
holds what sits underneath and outside that: the wire format, the C++ side, and the two
oracles the cost report reads.

## Contents

- [The wire format](#the-wire-format)
  - [From flat buffer to cuDF call](#from-flat-buffer-to-cudf-call)
  - [Cross join vs nested-loop join](#cross-join-vs-nested-loop-join)
- [Interfaces](#interfaces)
  - [The handle registry has no type](#the-handle-registry-has-no-type)
- [Rehash and the comet hash](#rehash-and-the-comet-hash)
- [C++ executor layout](#c-executor-layout)
- [Column indexing](#column-indexing)
  - [What actually guards it](#what-actually-guards-it)
  - [What nothing guards](#what-nothing-guards)
- [cuDF options](#cudf-options)
  - [What the Rust side puts in the flat buffers](#what-the-rust-side-puts-in-the-flat-buffers)
  - [Join types and NULL key equality](#join-types-and-null-key-equality)
- [Multi-GPU notes (cuDF ≥26.02)](#multi-gpu-notes-cudf-2602)
- [Cost model and the DuckDB oracle](#cost-model-and-the-duckdb-oracle)

## The wire format

**The flat buffers** are the serialized plan (`flatbuffers/gpu_plan.fbs`) — the only thing
the C++ side ever sees of a query. Wherever this page says "the flat buffers", "the wire
format" or "serialized", that is what it means.

What crosses is not the plan tree. It is a menu of parameterized kernels whose nodes exist
to be addressed: the recipe writer (`batch_partitioned/recipe/`) emits one node per call a
driver will make, and each node's recipe publishes the post-order sequence numbers its
calls name. The vocabulary is frozen — it predates this mode and the executors that first
read it — which is why prose here and in the task spec calls a call over a whole input "the
legacy call": the kernel is the one the retired whole-table modes drove.

Two spellings, and the prefix is the tell: `Cudf*` is a flat-buffer node table, the thing
the C++ dispatches on, with `GpuPlan` as the root table wrapping them. A `Gpu` name with no
`Cudf` is one of this mode's own plan nodes and never crosses.

Three of the fifteen wire kinds have no writer today: `CudfCoalesceBatches` (batching is
this mode's own and needs no node), `CudfLimit` (a limit is a row range on the export, not
a node) and `CudfWindow` (no window function in this mode yet, #143). They stay because the
kernels behind them do.

**Statement order is the wire format**: FlatBufferBuilder is a no-interning bump arena, so
reordering writes changes bytes even with identical values.
[`goldens/bp-recipe-payloads.txt`](../testdata/goldens/bp-recipe-payloads.txt) pins each
payload's bytes with a digest beside it; regenerating it to silence a red defeats its
purpose.

### From flat buffer to cuDF call

One row per wire node kind: what the plan hands the C++ side, and the cuDF it turns into.

The middle column is the fields that change what the call does — `input` / `left` / `right`
are the tree and are not repeated, and a field nothing reads is called out, because a wire
field with no consumer reads as a knob (#132). Line links are to the deciding call, not to
the whole function.

| Node | What steers it | The cuDF it becomes |
|---|---|---|
| [`CudfScan`](../flatbuffers/gpu_plan.fbs#L317) | `file_paths`, `projection`, `row_groups` (pruning survivors) or `batches[p]` (this partition's slice of them), or a list the call supplies instead of either (`execute_scan_rowgroups`, which is how one node loads a batch at a time), `limit`; `batch_size` **is read by nobody** (#132) | [`scan.cpp#L83`](../cpp/src/operators/scan.cpp#L83) — `cudf::io::read_parquet(opts)`, with `.columns(projected)`, `set_row_groups(...)` and `set_num_rows(limit)` set on `opts` first |
| [`CudfFilter`](../flatbuffers/gpu_plan.fbs#L346) | `predicate`, `projection` | [`filter.cpp#L25`](../cpp/src/operators/filter.cpp#L25) — `cudf::compute_column(tv, predicate)` for the mask, then `cudf::apply_boolean_mask(tv, mask->view())` |
| [`CudfProject`](../flatbuffers/gpu_plan.fbs#L358) | `exprs`, `aliases` | [`project.cpp#L49`](../cpp/src/operators/project.cpp#L49) — `cudf::compute_column(tv, ast)` per AST-able expr; a bare `ColumnRef` is a column copy, and LIKE/CASE/scalar functions take `build_column` instead |
| [`CudfAggregate`](../flatbuffers/gpu_plan.fbs#L375) | `mode` (Partial/Final/FinalPartitioned/Single/SinglePartitioned/Merge), `group_exprs`, `aggr_funcs` (each with its out decimal scale and `distinct`), `grouping_sets`, `mergeable_agg_state`, `aggr_input_schema` | [`aggregate.cpp#L666`](../cpp/src/operators/aggregate.cpp#L666) — `gb.aggregate(requests)` over [`groupby{keys, null_policy::INCLUDE}`](../cpp/src/operators/aggregate.cpp#L435); with no group keys it is [`cudf::reduce`](../cpp/src/operators/aggregate.cpp#L258) to one row |
| [`CudfHashJoin`](../flatbuffers/gpu_plan.fbs#L412) | `join_type`, `keys`, `filter` + `filter_columns` (residual), `null_equals_null`, `projection` | [`join.cpp#L290`](../cpp/src/operators/join.cpp#L290) — `cudf::inner_join` / `left_join` / `full_join(left_keys, right_keys, kJoinNulls)`; semi/anti take [`left_semi_join` / `left_anti_join`](../cpp/src/operators/join.cpp#L126), or their `mixed_*` forms when a residual filter must be evaluated during the join |
| [`CudfCrossJoin`](../flatbuffers/gpu_plan.fbs#L434) | nothing — the node is its two inputs | [`join.cpp#L394`](../cpp/src/operators/join.cpp#L394) — `cudf::cross_join(ltv, rtv)` |
| [`CudfNestedLoopJoin`](../flatbuffers/gpu_plan.fbs#L442) | `join_type`, `filter` + `filter_columns`, `projection` | [`join.cpp#L432`](../cpp/src/operators/join.cpp#L432) — `cudf::cross_join`, then [`apply_boolean_mask`](../cpp/src/operators/join.cpp#L478) over the filter evaluated on the crossed table |
| [`CudfSort`](../flatbuffers/gpu_plan.fbs#L455) | `exprs` (`asc`, `nulls_first` per key), `fetch`, `preserve_partitioning` | [`sort.cpp#L50`](../cpp/src/operators/sort.cpp#L50) — `cudf::sorted_order(keys, orders, null_orders)` then `cudf::gather`, and [`cudf::slice`](../cpp/src/operators/sort.cpp#L58) when `fetch` makes it a top-N |
| [`CudfCoalesceBatches`](../flatbuffers/gpu_plan.fbs#L469) | `target_batch_size` — **read by nobody** (#132) | [`dispatch.cpp#L66`](../cpp/src/operators/dispatch.cpp#L66) — `execute_passthrough`: the child's table, untouched. A GPU node is one materialized table, so there is no batching to do |
| [`CudfCoalescePartitions`](../flatbuffers/gpu_plan.fbs#L475) | nothing | [`node_session.cpp#L298`](../cpp/src/node_session.cpp#L298) — `cudf::concatenate(views)` over the input partitions; a single input has nothing to collapse and passes through |
| [`CudfRepartition`](../flatbuffers/gpu_plan.fbs#L486) | `kind`, `num_partitions`, `hash_exprs` (key ordinals) | [`node_session.cpp#L371`](../cpp/src/node_session.cpp#L371) — `spark_hash_partition(tv, key_cols, n)`, ours rather than cuDF's murmur3, then [`cudf::slice`](../cpp/src/node_session.cpp#L384) per partition into an owning table |
| [`CudfSortPreservingMerge`](../flatbuffers/gpu_plan.fbs#L495) | `exprs`, `fetch` | [`node_session.cpp#L286`](../cpp/src/node_session.cpp#L286) — `cudf::merge(views, key_cols, orders, null_orders)`, k-way and order-preserving; a concat fallback with no keys or one input (#118) |
| [`CudfUnion`](../flatbuffers/gpu_plan.fbs#L508) | `inputs`, `interleave`, `output_schema` | [`union.cpp#L62`](../cpp/src/operators/union.cpp#L62) — `cudf::concatenate(views)`, after [`cudf::cast`](../cpp/src/operators/union.cpp#L51) retypes each branch column to the declared output type (#41) |
| [`CudfLimit`](../flatbuffers/gpu_plan.fbs#L526) | `skip`, `fetch` | [`limit.cpp#L31`](../cpp/src/operators/limit.cpp#L31) — `cudf::slice(tv, {skip, end})`, and the whole table returned untouched when the range covers it |
| [`CudfWindow`](../flatbuffers/gpu_plan.fbs#L573) | `window_exprs` (partition keys, order keys, frame bounds, out decimal scale) | [`window.cpp#L106`](../cpp/src/operators/window.cpp#L106) — `cudf::grouped_rolling_window(keys, arg, preceding, following, min_periods, agg)`, which preserves input row order |

Two things recur. **A node handed one input reaches no kernel** where all it does is change
the layout rows sit in: coalesce-partitions, repartition and sort-preserving-merge pass the
table through, since one table has no layout to change. Coalesce-batches is passthrough
whatever it is handed.

And **three nodes need more than one call**, because cuDF has no fused form: filter
computes a mask and then applies it; sort takes `sorted_order` then `gather`, and a third
call to `slice` when a `fetch` makes it a top-N; union casts each branch column whose type
differs from the declared output, then concatenates once. Each intermediate in those
sequences exists because the pair could not be one call.

The nested-loop join is the one to read separately rather than filing beside filter. It
materialises the **full cartesian product** first and only then evaluates its predicate
over it — cross join, build the mask on the crossed table, apply it. That is three calls
whose first is the expensive one, and it is why broadcast joins (#140) would change the
shape rather than the constant.

### Cross join vs nested-loop join

Both are a join with no equality to hash, and which one DataFusion plans — and so which node
the translator meets — is decided by whether there is a predicate at all.

- **No join predicate ⇒ `CrossJoinExec`.** `SELECT * FROM region, nation` — a full cartesian
  product, every left row against every right row. In the corpus: the `cross-join` fixture plus
  tpcds q23, q28, q61, q77, q88, q90 — all of them pairing one-row aggregate results with no
  condition, e.g. q61 puts `sum(ss_ext_sales_price) as promotions` beside `total` so it can
  divide them.
- **A predicate that is not an equijoin ⇒ `NestedLoopJoinExec`**, carrying that predicate as
  its `filter`. `SELECT * FROM region a, nation b WHERE a.r_regionkey < b.n_regionkey` becomes
  a `GpuNestedLoopJoin` with `filter=n_regionkey@1 > r_regionkey@0`. In the corpus:
  the `nested-loop-join` fixture plus tpch q11 and q22 and tpcds q9, q14, q24, q44, q54.

The tpch pair is worth recognizing, because it is a shape rather than an accident: q11's
`having sum(ps_supplycost * ps_availqty) > (select sum(…) * 0.000002 …)` plans as a
nested-loop join whose filter is the comparison against the one-row subquery —
`filter=CAST(sum(partsupp.ps_supplycost * partsupp.ps_availqty)@0 AS Decimal128(38, 15)) > …`.
A scalar threshold is a 1×N join with an inequality, so the planner has nowhere to put it but
here. (Rewriting that into a broadcast filter is the optimization #27 was archived for.)

## Interfaces

Declarations are quoted with doc comments elided. Items marked *de facto* have no trait or
abstract base behind them, yet other code is written against them, so changing one breaks a
caller that never named it. The Rust side's own traits — `Backend`, the executor families,
`GpuNode` — are quoted in
[`tasks/batch_partitioned_executor.md`](tasks/batch_partitioned_executor.md#traits), beside
the reasons for their shape.

**[The C ABI](../cpp/include/peacock_gpu.h)** — the entire public surface of the C++ side,
plus `partitioning.hpp`. Everything else under `cpp/src/peacock/` is private to the library.

```c
const char* peacock_gpu_version(void);
typedef struct peacock_executor peacock_executor_t;

int  peacock_executor_create(uint64_t gpu_memory_limit, peacock_executor_t** out_executor);
void peacock_executor_destroy(peacock_executor_t* executor);
const char* peacock_last_error(peacock_executor_t* executor);

void peacock_result_free(uint8_t* result_bytes);

/* node-by-node: one session, one node at a time, intermediates stay resident */
typedef struct PeacockNodeStats {
  uint64_t rows; uint64_t varlen_content_bytes; uint64_t time_us;
} PeacockNodeStats;

int  peacock_executor_begin_plan(peacock_executor_t* executor, const uint8_t* plan_bytes,
                                 uint64_t plan_len, uint64_t* out_node_count);
int  peacock_executor_execute_node(peacock_executor_t* executor, uint64_t seq,
                                   const uint64_t* input_handles,
                                   const uint64_t* input_child_counts, uint64_t n_children,
                                   uint64_t* out_handles, uint64_t out_cap,
                                   uint64_t* out_count, PeacockNodeStats* out_stats);
void peacock_handle_release(peacock_executor_t* executor, uint64_t handle);
void peacock_executor_end_plan(peacock_executor_t* executor);

/* per-call entry points: what a driver decides per call — a batch's row groups, a
   limit's bounds — cannot ride a plan node, whose fields are constants.
   A row range is [offset, offset+length), UINT64_MAX meaning to the end, an offset
   past the end empty and an overrun clamped. */
int  peacock_executor_execute_scan_rowgroups(peacock_executor_t* executor, uint64_t seq,
                                             const uint32_t* row_groups, uint64_t n,
                                             uint64_t* out_handle,
                                             PeacockNodeStats* out_stats);
int  peacock_executor_slice_handle(peacock_executor_t* executor, uint64_t handle,
                                   uint64_t offset, uint64_t length, uint64_t* out_handle);
int  peacock_result_from_handle(peacock_executor_t* executor, uint64_t handle,
                                uint64_t offset, uint64_t length,
                                uint8_t** out_ipc, uint64_t* out_ipc_len);

/* benchmark instrumentation: process-global, off by default. Enabling it makes
   execute_node synchronize the default stream at every measurement boundary, so
   time_us measures execution rather than kernel submission — and serializes what
   cuDF would otherwise pipeline, which is why the correctness path never sets it.
   The floor is what an empty timed region costs; a node at or below it is
   unresolved, not cheap, and it is never subtracted. */
void     peacock_set_node_timing(int enable);
uint64_t peacock_measure_timing_floor_us(unsigned samples);

/* the conformance hook: Spark-murmur3 partition ids over one Arrow C-data batch */
int  peacock_spark_partition_ids(const void* schema, const void* array,
                                 const uint32_t* key_cols, uint64_t num_keys,
                                 uint32_t num_partitions, uint32_t seed,
                                 int32_t* out_pids, uint64_t out_cap, uint64_t* out_n);
```

**[`NodeSession`](../cpp/src/plan_executor.h#L46)** — *de facto*. No abstract base, but an
interface in every practical sense: it is what the node-by-node FFI entry points are thin wrappers over, and
the Rust `GpuNodeExecutor` is written against its shape. Nodes are addressed by canonical
post-order sequence, the same order the Rust walk uses, so child handles align across the
boundary. PIMPL, so the header exposes no cuDF internals.

```cpp
class NodeSession {
 public:
  NodeSession(const uint8_t* plan_bytes, uint64_t plan_len);
  ~NodeSession();
  NodeSession(const NodeSession&) = delete;
  NodeSession& operator=(const NodeSession&) = delete;

  size_t node_count() const;

  // Input handles are CONSUMED. out_stats is filled PER PARTITION.
  void execute_node(uint64_t seq, const uint64_t* input_handles,
                    const uint64_t* input_child_counts, size_t n_children,
                    uint64_t* out_handles, size_t out_cap, size_t* out_count,
                    NodeStats* out_stats);

  // The scan's row groups and the slice's bounds are per-call values, so they are
  // arguments here rather than fields of the node addressed by seq.
  uint64_t execute_scan_rowgroups(uint64_t seq, cudf::host_span<const uint32_t> row_groups,
                                  NodeStats* out_stats);
  uint64_t slice_handle(uint64_t handle, uint64_t offset, uint64_t length);

  const TableResult& table_for(uint64_t handle) const;
  void release(uint64_t handle);

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};
```

**[`TableResult` / `NodeStats`](../cpp/src/plan_executor.h#L13)** — the two value types every
C++ path returns. `NodeStats` carries only what C++ alone can measure: the byte formula
lives in Rust so the two engines cannot drift.

```cpp
struct TableResult {
  std::unique_ptr<cudf::table> table;
  std::vector<std::string> column_names;
};

struct NodeStats {
  uint64_t rows = 0;
  uint64_t varlen_content_bytes = 0;   // Σ over varlen columns of offsets[n]-offsets[0]
  uint64_t time_us = 0;                // per output partition; 0 unless timing is on
};
```

**[`NodeInputs` and the operator dispatch](../cpp/src/peacock/operators.h#L22)** — the
contract every operator translation unit shares. `NodeInputs` is passed explicitly rather
than through a thread-local, and that is deliberate: a per-translation-unit thread-local
would silently fork when the file was split and re-execute whole subtrees
(coding-style.md).

```cpp
struct NodeInputs {
  std::vector<TableResult>* items = nullptr;   // the caller's already-resident inputs
  size_t idx = 0;
};

TableResult execute_scan(const fb::CudfScan* scan,
                         cudf::host_span<const uint32_t> row_groups_override = {});
TableResult execute_filter(const fb::CudfFilter* filter, NodeInputs* in);
TableResult execute_project(const fb::CudfProject* proj, NodeInputs* in);
TableResult execute_aggregate(const fb::CudfAggregate* agg, NodeInputs* in);
TableResult execute_hash_join(const fb::CudfHashJoin* join, NodeInputs* in);
TableResult execute_cross_join(const fb::CudfCrossJoin* join, NodeInputs* in);
TableResult execute_nested_loop_join(const fb::CudfNestedLoopJoin* join, NodeInputs* in);
TableResult execute_sort(const fb::CudfSort* sort, NodeInputs* in);
TableResult execute_union(const fb::CudfUnion* u, NodeInputs* in);
TableResult execute_limit(const fb::CudfLimit* limit, NodeInputs* in);
TableResult execute_window(const fb::CudfWindow* win, NodeInputs* in);

TableResult execute_node(const fb::PlanNode* node, NodeInputs* in);
TableResult execute_one(const fb::PlanNode* node, std::vector<TableResult> inputs);
inline TableResult execute_passthrough(const fb::PlanNode* input_node, NodeInputs* in);
```

**[`peacock::partitioning`](../cpp/include/peacock/partitioning.hpp)** — the second public
header: our own bit-exact Spark-murmur3, because cuDF ships only standard murmur3.

```cpp
std::unique_ptr<cudf::column> spark_partition_ids(
    cudf::table_view const& input,
    std::vector<cudf::size_type> const& key_cols,
    cudf::size_type num_partitions,
    uint32_t seed                     = 42,
    rmm::cuda_stream_view stream      = cudf::get_default_stream(),
    rmm::device_async_resource_ref mr = cudf::get_current_device_resource_ref());

std::pair<std::unique_ptr<cudf::table>, std::vector<cudf::size_type>> spark_hash_partition(
    cudf::table_view const& input,
    std::vector<cudf::size_type> const& key_cols, /* … same trailing defaults … */);
```

**[`ExprContext`](../cpp/src/peacock/expr.h#L25)** — *de facto*. Expression building. cuDF AST nodes hold
references, so something must own every sub-expression for the lifetime of the call; that
ownership IS the interface.

```cpp
struct ExprContext {
  std::vector<std::unique_ptr<cudf::ast::expression>> owned;
  std::vector<std::unique_ptr<cudf::scalar>> scalars;
  cudf::ast::expression& keep(std::unique_ptr<cudf::ast::expression> e);
};

using JoinFilterColMap = flatbuffers::Vector<const fb::JoinFilterColumn*>;
cudf::ast::expression& build_expr(const fb::Expr* expr, ExprContext& ctx,
                                  const JoinFilterColMap* col_map = nullptr);
```

**[`GpuWorker` / `WorkerPool`](../cpp/tests/gpu/multi_gpu.hpp#L79)** — *de facto*, test-only
(`cpp/tests/gpu/`), and the one place the multi-GPU rules are encoded as a type: a cuDF or
cuVS object must be destroyed on its owning device's thread, so every device gets a worker
thread and a persistent stream, and work reaches a device only by `submit`.

```cpp
class GpuWorker {
 public:
  explicit GpuWorker(int device);
  ~GpuWorker();
  template <typename F> auto submit(F f) -> std::future<decltype(f())>;
};

class WorkerPool {
 public:
  explicit WorkerPool(int num_gpus);
  ~WorkerPool();
  int size() const;
  GpuWorker& operator[](int g);
  rmm::cuda_stream_view stream(int g) const;
};
```

### The handle registry has no type

The C++ side keeps intermediates alive behind opaque `u64` handles, and that is not a
class. It is two fields inside the private
[`NodeSession::Impl`](../cpp/src/node_session.cpp#L70) —
`std::unordered_map<uint64_t, TableResult> registry` and `uint64_t next_handle = 1` — with
allocation, lookup, consume-on-read and erase written inline at each of the twenty-two sites
that touch them.

So the consume-once rule the FFI documents ("input handles are CONSUMED") holds by
convention at each site rather than by construction, and it is checked only at run time:
reading an already-consumed handle throws `unknown input handle`, and
[`execute_one`](../cpp/src/operators/dispatch.cpp#L110) throws when a node consumes a
different number of inputs than it was given. A `HandleRegistry` with `insert` / `take` /
`borrow` would put the rule in one place and make double-consumption unrepresentable rather
than merely detected. Nothing needs it yet — the two checks have held — but no type is
holding this together.

## Rehash and the comet hash

A shuffle is `GpuEmitPartitions`: one lane's batch in, one batch per lane out, scattered by
hash. Both backends run it, so which lane a row lands in has to be the same number on each.

The hash is **Spark's murmur3 as implemented by comet** (seed 42), on both engines:
DataFusion's default repartition uses ahash, whose partition numbers differ from any GPU
kernel, and cuDF only exposes standard murmur3, which differs from Spark's spec
(multi-column combine, null handling). To make CPU and GPU row→partition placement
identical **by construction**, the CPU side uses comet's `create_murmur3_hashes` — in
`peacockdb-core/src/spark_partitioning.rs`, the one spelling it calls — and the GPU side
owns a bit-exact Spark-murmur3 kernel (`cpp/src/spark_hash_partition.cu`), reusing cuDF
only for the scatter. A live conformance gate (`peacock_spark_partition_ids`,
`test_inc2_conformance.rs`) proves GPU == comet over the same bytes; the per-lane row
counts a corpus golden records are the murmur3-fidelity numbers.

## C++ executor layout

`cpp/src/`: `gpu_executor.cpp` (C FFI impl), `node_session.cpp` (NodeSession: the
post-order index, the handle registry, and the multi-partition dispatch — scan-map
emission, collapse/k-way merge, hash repartition, 1:1 map), `expr.cpp` (expression/AST
building), `operators/` (per-op `execute_*` + `dispatch.cpp` with the `run_op` switch).
Node inputs are threaded explicitly (`NodeInputs{items, idx}` — never a thread-local; see
coding-style.md), and `execute_one` enforces **consumed == provided**: a node handed inputs
must consume all of them, otherwise it ran against inputs the caller did not give it.
Private headers live in `cpp/src/peacock/`; the public surface is only the C FFI
(`cpp/include/peacock_gpu.h`) plus `partitioning.hpp`, with
`plan_executor_internal.h` alongside for what the tests reach into.

## Column indexing

Nothing in the flat buffers names a column to read. Every reference is an ordinal into the
child's output table, so a node's correctness depends on the child having produced its
columns in exactly the order the planner assumed.

Where the ordinals come from and where they land:

| Reference | Written by | Read by |
|---|---|---|
| `ColumnRef.index` in any expression | [`expr_writer.rs`](../peacockdb-core/src/batch_partitioned/recipe/expr_writer.rs), off the ordinal `expr_translate` read from DataFusion's `Column::index()` | [`build_expr`](../cpp/src/expr.cpp#L140) for the AST path, [`build_column`](../cpp/src/expr.cpp#L833) for the column path |
| `projection` index lists on filter and join | [`node_writer.rs`](../peacockdb-core/src/batch_partitioned/recipe/node_writer.rs), [`join.rs`](../peacockdb-core/src/batch_partitioned/recipe/join.rs) | [`filter.cpp`](../cpp/src/operators/filter.cpp#L40), [`join.cpp`](../cpp/src/operators/join.cpp#L206) — gather by ordinal, and the name list is indexed with the same ordinal |
| join key pairs, `on=[(l@0, r@0)]` | [`join.rs`](../peacockdb-core/src/batch_partitioned/recipe/join.rs) | [`join.cpp`](../cpp/src/operators/join.cpp#L58) — ColumnRef only, anything else throws |
| `JoinFilterColumn{side, index}` | [`join.rs`](../peacockdb-core/src/batch_partitioned/recipe/join.rs) | [`expr.cpp`](../cpp/src/expr.cpp#L145) — remaps a filter-schema ordinal onto the mixed join's LEFT/RIGHT tables |
| sort keys, hash keys, group keys | [`node_writer.rs`](../peacockdb-core/src/batch_partitioned/recipe/node_writer.rs), [`aggregate_writer.rs`](../peacockdb-core/src/batch_partitioned/recipe/aggregate_writer.rs) | [`sort.cpp`](../cpp/src/operators/sort.cpp#L38), [`node_session.cpp`](../cpp/src/node_session.cpp#L255), [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp#L157) |

There are 22 `->index()` reads and 30 `.column(idx)` calls on the C++ side, so this is the
engine's most common operation and the one with the least ceremony around it.

### What actually guards it

Your presumption is nearly right — there is one real backstop, and it is not ours.

- **cuDF bounds-checks column access.** `table_view::column(i)` is `_columns.at(i)`, so an
  out-of-range ordinal throws `std::out_of_range` rather than reading garbage. The FFI catches
  `std::exception` and surfaces the message, so the failure is loud. What it is not is
  *informative*: the message is `vector::at` boilerplate with no node, no operator and no
  ordinal, because the check is three layers below the code that had the context.
- **Two explicit checks, in `expr.cpp` only.** The column path
  ([#L837](../cpp/src/expr.cpp#L837)) throws with the ordinal and the column count, which is the
  message you actually want. The type-inference helper ([#L349](../cpp/src/expr.cpp#L349))
  checks the same thing and **returns `type_id::EMPTY`** — a silent fallback that turns a bad
  ordinal into an unhelpful type error further along.
- **One arity check.** The Final-stage aggregate compares its input width against the state
  arity it expects and throws when they disagree
  ([`aggregate.cpp#L505`](../cpp/src/operators/aggregate.cpp#L505)). It is the only place a
  schema-shape mismatch is deliberately caught, and it catches width, not order.
- **The FlatBuffers verifier** checks structure — that offsets and vectors are well formed. It
  has no idea what an ordinal means.

### What nothing guards

**Column names are a parallel array with no invariant.** `TableResult` is a `cudf::table` plus a
`std::vector<std::string>`, and nothing asserts the two have the same length. The six sites that
index the names do it with `operator[]`, so a names vector shorter than the table is undefined
behaviour rather than an exception — for example
[`filter.cpp#L42`](../cpp/src/operators/filter.cpp#L42), where the same loop iteration reads
`fv.column(idx)` (checked) and `input.column_names[idx]` (unchecked). Today the checked read
happens first and throws, which is luck, not design.

**Nothing checks that a child's column *order* matches what the plan assumed.** The per-node
golden records the node line, its lane and batch lists, output rows and output bytes — not the
column list, not the types. And the bytes cannot help: both engines compute them from the
*plan's* schema via `logical_size_from_schema`, deliberately, so that CPU and GPU cannot
drift. The same choice means a node that emitted the right number of columns in the wrong
order produces identical per-node numbers on both engines. The divergence surfaces only at
the root, in the result comparison, and only for a query whose corpus line names a result
golden or an oracle — a subtree bug pinned nowhere else in the tree.

That is the honest state: the ordinal contract is enforced by cuDF's `at()` for gross violations
and by the result comparison for subtle ones, with nothing in between. Adding
`num_columns() == column_names.size()` to `TableResult`'s construction, and a per-node type
check in the GPU tiers, are the two obvious closures (#164).

## cuDF options

cuDF's defaults are not SQL's, and they are not DataFusion's. Every option below is a place
where taking the default would produce a plausible wrong answer rather than an error, so each
one is either set explicitly or carried in the flat buffers — and the ones carried in the flat buffers are carried
precisely so cuDF cannot infer something the CPU side did not.

| Option | Set at | Value | What the default would do |
|---|---|---|---|
| `parquet_reader_options` | [`scan.cpp`](../cpp/src/operators/scan.cpp#L57) | `.columns(projected)`, `set_row_groups(map ∥ pruned)`, `set_num_rows(limit)` | read every column and every row group; the row-group list is also how a partition reads only its own slice |
| `cudf::order`, `cudf::null_order` | [`sort.cpp`](../cpp/src/operators/sort.cpp#L44), [`node_session.cpp`](../cpp/src/node_session.cpp#L192) | per key from the flat buffers's `asc` / `nulls_first` | cuDF has no notion of the query's ORDER BY; the two sites must agree or a k-way merge would order differently from a sort |
| `cudf::null_equality` | [`join.cpp`](../cpp/src/operators/join.cpp#L90) ×9 | see the table below | `EQUAL` — NULL keys match, inventing rows SQL excludes |
| `cudf::out_of_bounds_policy` | [`join.cpp`](../cpp/src/operators/join.cpp#L315) | `NULLIFY` on the side that can be unmatched, `DONT_CHECK` otherwise | `DONT_CHECK` reads the `JoinNoneValue` sentinel (`INT32_MIN`) as an index and faults with `cudaErrorIllegalAddress` |
| `cudf::null_policy` (groupby) | [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp#L435), [grouping sets](../cpp/src/operators/aggregate.cpp#L392) | `INCLUDE` | `EXCLUDE` silently drops the NULL group — tpcds q15's NULL `ca_zip` row disappears |
| `cudf::null_policy` (rolling count) | [`window.cpp`](../cpp/src/operators/window.cpp#L103) | `EXCLUDE` for `COUNT(col)`, `INCLUDE` for `COUNT(*)` | one of the two is always wrong: `COUNT(*)` counts rows, `COUNT(col)` counts non-nulls |
| decimal scale | [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp#L212), [`union.cpp`](../cpp/src/operators/union.cpp#L45), [`window.cpp`](../cpp/src/operators/window.cpp#L86) | `data_type{id, -out_decimal_scale}` from the flat buffers | cuDF would re-derive a scale per operation and drift from DataFusion's |
| binary-op output type | [`expr.cpp`](../cpp/src/expr.cpp#L547) | boolean for predicates, else the wider input; division pre-scales the numerator to hit the flat buffers's `out_decimal_precision/scale` | cuDF promotes by its own rule, which is not SQL's decimal arithmetic |
| hash seed / algorithm | [`spark_hash_partition.cu`](../cpp/src/spark_hash_partition.cu#L198) | our own Spark-murmur3, seed 42, cuDF only for the scatter | cuDF ships standard murmur3, whose partition numbers differ from comet's — see [Rehash and the comet hash](#rehash-and-the-comet-hash) |
| IPC export | [`gpu_executor.cpp`](../cpp/src/gpu_executor.cpp#L39) | column names as `column_metadata`; DECIMAL32/64 cast up to DECIMAL128 | unnamed columns, and narrow decimals that the Rust arrow-ipc reader rejects outright |
| stream + memory resource | everywhere in the single-GPU path | `cudf::get_default_stream()`, current device resource | fine on device 0 and wrong anywhere else — the multi-GPU rules are in [Multi-GPU notes](#multi-gpu-notes-cudf-2602) |

### What the Rust side puts in the flat buffers

Half of the table above is not a choice the C++ side makes — it reads a value the planner
already computed and the recipe writer wrote down. That is deliberate: an option carried in
the flat buffers cannot be re-derived differently by the two engines, so anything where
cuDF's own inference could drift from DataFusion's is serialized rather than inferred. The
writers are all under
[`batch_partitioned/recipe/`](../peacockdb-core/src/batch_partitioned/recipe/), so the paths
below are relative to it.

| Flat-buffer field | Written by | Taken from | Becomes |
|---|---|---|---|
| `CudfHashJoin.null_equals_null` | `join.rs` | the node's own flag, which the planner set from `HashJoinExec::null_equals_null()` | `cudf::null_equality` (except anti/mark, below) |
| `JoinFilterColumn{side, index}` | `join.rs` | the join filter's `ColumnIndex` list | `cudf::ast::`<br>`table_reference::LEFT` / `RIGHT`,<br>plus an ordinal |
| `SortExpr.asc`, `.nulls_first` | `node_writer.rs` | the node's sort keys, from `PhysicalSortExpr::options` | `cudf::order`,<br>`cudf::null_order` |
| `CudfSort.fetch`,<br>`CudfSortPreservingMerge.fetch` | `node_writer.rs` | the node's `fetch`, `-1` where there is none | a post-sort / post-merge slice |
| `BinaryExpr`<br>`.out_decimal_precision/scale` | `expr_writer.rs` | the expression's declared output type | the binop output type, and division pre-scales to hit it |
| `CudfAggregate.mode` | `aggregate_writer.rs` | the phase: `Partial` builds state from values, `Merge` merges state into state. Never `Final`, which would also finalize, and a finalize here is a project both engines evaluate | which cuDF aggregation runs, whether state columns are merged, and whether the result is state or a value |
| `CudfRepartition.hash_exprs`,<br>`num_partitions` | `node_writer.rs` | the emit node's keys and lane count | key ordinals and N for<br>`spark_hash_partition` |
| `CudfScan.row_groups`,<br>`limit` | `node_writer.rs` | the source's surviving row groups and its pushed-down limit | `parquet_reader_options::`<br>`set_row_groups` / `set_num_rows` — though every load overrides the list per call |
| `AggregateFuncNode`<br>`.out_decimal_precision/scale` | `aggregate_writer.rs`, at zero | nothing: decomposition means no `avg` reaches a device, so the scale rides the finalize divide's own pair | **nothing** here, deliberately, and the writer says why |
| `CudfScan.batch_size`,<br>`CudfCoalesceBatches`<br>`.target_batch_size` | nobody | — | **nothing** — no C++ code reads either (#132) |

Two shapes are worth separating here. Most rows carry a value the GPU must not recompute —
decimal scales above all, since cuDF derives its own per operation and DataFusion's is what
the result is compared against. The last two rows are different: one is a field left at its
default on purpose, and the other is a pair of fields no writer sets and no reader reads,
which is the wire-format surface #132 is about.

### Join types and NULL key equality

`null_equals_null` travels in the flat buffers per join, mirroring DataFusion: `false` (the SQL default)
means a NULL key matches nothing, `true` means NULL = NULL, which is what a set operation
lowered to a join needs. Whether a join type actually honours it is the interesting part.

| Join type | cuDF call | `null_equality` | Code |
|---|---|---|---|
| Inner | `inner_join` | from the flat buffers | [join.cpp#L289](../cpp/src/operators/join.cpp#L289) |
| Left | `left_join` | from the flat buffers | [#L291](../cpp/src/operators/join.cpp#L291) |
| Full | `full_join` | from the flat buffers | [#L293](../cpp/src/operators/join.cpp#L293) |
| Right | `left_join` with sides swapped, indices swapped back | from the flat buffers | [#L295](../cpp/src/operators/join.cpp#L295) |
| LeftSemi | `left_semi_join`, `filtered_join::semi_join`, or `mixed_left_semi_join` with a residual filter | from the flat buffers | [#L116](../cpp/src/operators/join.cpp#L116) |
| RightSemi | the same, sides swapped; a residual filter is rejected | from the flat buffers | [#L153](../cpp/src/operators/join.cpp#L153) |
| LeftAnti | `left_anti_join`, `filtered_join::anti_join`, or `mixed_left_anti_join` | **hardcoded `EQUAL`** | [#L134](../cpp/src/operators/join.cpp#L134) |
| RightAnti | the same, sides swapped | **hardcoded `EQUAL`** | [#L170](../cpp/src/operators/join.cpp#L170) |
| LeftMark | `left_semi_join`-shaped, emitting one row per left row plus a boolean mark | **hardcoded `EQUAL`** | [#L224](../cpp/src/operators/join.cpp#L224) |
| Inner / Left, non-equi | `conditional_inner_join` / `conditional_left_join`, or an AST boolean mask | n/a — the predicate decides | [#L405](../cpp/src/operators/join.cpp#L405) |

Three things that table is worth reading for.

**Semi honours the flag and anti does not**, deliberately. `x IN (…)` and `EXISTS` are ordinary
three-valued predicates, so `UNEQUAL` is right and tpcds q33 needs it; a set operation lowered
to a semi join asks for `EQUAL` and gets it (q14). Anti is not symmetric: `x NOT IN (…, NULL)`
is never true for any x, which is neither `EQUAL` nor `UNEQUAL` — no cuDF setting implements
it, so anti and mark stay `EQUAL` until the planner distinguishes `NOT IN` from `NOT EXISTS`
(#80, #59).

**The equi-join default is the one that bites silently.** cuDF's `EQUAL` invents rows the SQL
oracle excludes, and the symptom is not an error but a count or sum one too large — tpcds q50,
q6 and q81 each inflated a downstream aggregate before `join_nulls` was threaded through.

**A residual filter is not optional on semi/anti.** The key-only cuDF calls ignore it, so a
LeftAnti on the key alone collapses to zero rows; those joins must take the `mixed_*` variants
that evaluate the AST during the join (TPC-H q21 is the case). RightSemi and RightAnti reject
a filter outright, because no swapped `mixed_*` variant exists.

## Multi-GPU notes (cuDF ≥26.02)

Hard-won constraints for the multi-GPU C++ path (`cpp/tests/gpu/test_multi_gpu_*`,
WorkerPool, `hash_shuffle`, `gather_here`):

- Every cudf op on a worker pinned to GPU≠0 needs a **device-local stream** —
  `cudf::get_default_stream()` is device-0-bound ("invalid device ordinal" otherwise).
- A cudf/cuVS device object must be **destroyed on its owning device's worker thread** —
  hence the worker-per-GPU pool; release partitions/results on-worker before teardown.
- Per-device **RMM pools** (with persistent per-worker streams) are what make cheap
  queries scale; pool dealloc is stream-ordered, so an object outliving a transient
  stream frees on a dead stream and crashes.
- RMM `set_per_device_resource(g, nullptr)` resets the pointer map but NOT the ref map —
  teardown must also call `reset_per_device_resource_ref(g)`.
- Benchmarking multiple queries in one process is flaky at G≥2 (process-global cudf
  stream state across WorkerPool teardowns) — one query per process (see build-test.md).

## Cost model and the DuckDB oracle

- **Peacock cost:** `.cost.txt` goldens are derived purely from the `.cpu.txt` per-node
  tree text, section by section (`tests/common/cost_model.rs`): each node's `output_bytes`
  is binned into a category and multiplied by that category's weight from
  **`testdata/cost_model.conf`** (runtime-editable; format `<category> <multiplier>
  [nodes…]`). Today every real category has multiplier 1.0 (total == Σ output_bytes);
  three placeholder phases (ram_to_vram, cuda_decompress, cuda_rle_decode) sit at 0.0, and
  `cuda_window_bytes` has no node to count until this mode has a window function (#143).
- **DuckDB oracle:** `testdata/duckdb_cost.py` runs each query through DuckDB in two
  passes (deterministic profile with join-filter-pushdown off; a second pass extracting
  only dynamic-filter min/max bounds), combines them with parquet row-group stats, and
  emits `<q>.duckdb_cost.txt` = `materialization_total` (Σ pipeline-breaker materialized
  bytes) + `storage_read_total` (decoded Arrow bytes of surviving row-groups' referenced
  columns after static ∩ dynamic pruning — deliberately the same units as the source
  node's output_bytes so the ratio is apples-to-apples).
- **Widget:** the cost report compares peacock Σout — the query's section of the last mode
  its cpu run is enabled at — against duckdb Σout; ratio ≤ 1.4 renders green. Directional
  signal only, not a benchmark. Per-query mode enablement comes from
  `testdata/cost-registry.csv` (the registry the inventory tests verify), tickets from
  `llm-wiki/tickets.md`.
