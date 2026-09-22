// NodeSession: node-by-node execution over a parsed plan, keeping intermediates
// resident in a handle registry, plus the two free functions only its stats and its
// row ranges need. `node_children` is the session's own notion of child order, and the
// one thing a caller needs to address the same nodes it does.

#include "peacock/operators.h"
#include "peacock/expr.h"
#include "peacock/partitioning.hpp"
#include "plan_executor_internal.h"

#include <cudf/concatenate.hpp>
#include <cudf/copying.hpp>
#include <cudf/merge.hpp>
#include <cudf/sorting.hpp>
#include <cudf/strings/strings_column_view.hpp>
#include <cudf/table/table.hpp>
#include <cudf/utilities/default_stream.hpp>

#include <cuda_runtime.h>
#include <nvtx3/nvtx3.hpp>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <deque>
#include <optional>
#include <utility>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <vector>

namespace peacock {

// ============================================================================
// Per-node timing (measurement mode)
// ============================================================================
// Off by default. The contract on `set_node_timing` in plan_executor.h says why
// measuring costs the normal path anything at all.

namespace {
std::atomic<NodeTiming> g_node_timing{NodeTiming::Off};

// ----------------------------------------------------------------------------
// NVTX ranges
// ----------------------------------------------------------------------------
// Our own domain, so a capture can separate our node boundaries from the ranges
// libcudf pushes from inside the calls those boundaries contain. Same reason the two
// are not one switch: a profiled run wants the boundaries without the event pairs,
// whose recording is itself device work nsys would attribute to the node.
struct peacock_domain {
  static constexpr char const* name{"peacockdb"};
};
using scoped_range = ::nvtx3::scoped_range_in<peacock_domain>;

std::atomic<bool> g_nvtx{false};

/// The range the harness opens, one per benchmark case, holding every node range inside.
///
/// A node range is named `<seq>.<call_index> <kind>` and seq numbering restarts per plan,
/// so q6 and q19 both open with `0.0 CudfScan`. Nesting is what tells a capture which
/// query a call belonged to, without anyone naming it on a command line.
///
/// One level: cases do not nest, so a stack would be machinery for a shape nothing
/// produces. The engine never calls this — only the two ABI entry points reach it, and
/// only the harness calls those.
std::optional<scoped_range>& harness_range() {
  static std::optional<scoped_range> range;
  return range;
}

/// A range that exists only when ranges are on. `std::optional` rather than a branch
/// at each site: the range has to outlive the `if`, and a scope that closes at the
/// brace would time the check instead of the work.
///
/// Takes a callable rather than the name: composing it is a concatenation and an
/// allocation, and a shipping query would pay both on every call for a string nothing
/// reads.
class OptionalRange {
 public:
  template <class MakeName>
  explicit OptionalRange(MakeName&& make_name) {
    if (g_nvtx.load(std::memory_order_relaxed)) range_.emplace(make_name().c_str());
  }

 private:
  std::optional<scoped_range> range_;
};

inline uint64_t us_since(std::chrono::steady_clock::time_point t0,
                         std::chrono::steady_clock::time_point t1) {
  return static_cast<uint64_t>(
      std::chrono::duration_cast<std::chrono::microseconds>(t1 - t0).count());
}

[[noreturn]] void throw_cuda(const char* what, cudaError_t err) {
  throw std::runtime_error(std::string("node timing: ") + what + ": " + cudaGetErrorString(err));
}

/// One region's slot: the CUDA events while they are in flight, and what has been
/// measured about the call so far.
struct RegionSlot {
  NodeRegion out;
  cudaEvent_t start = nullptr;
  cudaEvent_t stop = nullptr;
};

/// All the state a measurement needs and execution does not.
///
/// Held behind a pointer that is null while timing is off, so a shipping query neither
/// allocates this nor writes to it. The line matters more than the bytes: a measurement
/// field added to the session or to `NodeStats` is paid on every call of every query,
/// and there is nothing to stop the next one but where the first one went.
struct RegionSink {
  /// Calls made against each seq so far, indexed by it. Sized on first use, so the count
  /// is a coordinate within one measured run rather than a process-wide tally.
  std::vector<uint64_t> calls_made;
  /// Which node produced each live handle. The slice and the export are given a handle
  /// and no seq, and this is the only thing that can name one for them.
  std::unordered_map<uint64_t, uint64_t> produced_by;
  std::deque<RegionSlot> slots;

  /// The next call's index for `seq`, consuming it.
  uint64_t take_call_index(uint64_t seq, size_t node_count) {
    if (calls_made.size() != node_count) calls_made.assign(node_count, 0);
    return calls_made[seq]++;
  }

  /// The seq behind a handle, 0 for one this sink never saw — a handle from before the
  /// mode was turned on.
  uint64_t producer_of(uint64_t handle) const {
    auto it = produced_by.find(handle);
    return it == produced_by.end() ? 0 : it->second;
  }
};

/// Stopwatch over one output partition's work. When timing is off it touches neither
/// the clock nor the driver, so the disabled path is one relaxed load.
///
/// Nothing inside the region drains the stream, so a node's reported time is the time it
/// would have taken unobserved — which is the property every benchmark record rests on.
/// A CUDA failure here throws rather than degrading to host-only: a region reported with
/// a device time it never measured is indistinguishable from a fast node.
class ScopedNodeTimer {
 public:
  ScopedNodeTimer(RegionSink* sink, uint64_t seq, uint64_t partition, uint64_t call_index) {
    // Before the early return, and closed by `stop`: the range has to span the same
    // interval the host and device numbers do, or a capture and a record disagree about
    // what "this region" was. On the nvtx switch alone, so a profiled run can leave
    // timing off — recording an event pair is device work of its own.
    if (g_nvtx.load(std::memory_order_relaxed))
      range_.emplace(("p" + std::to_string(partition)).c_str());
    if (!sink) return;
    sink->slots.push_back(RegionSlot{});
    slot_ = &sink->slots.back();
    slot_->out.seq = seq;
    slot_->out.partition = partition;
    slot_->out.call_index = call_index;
    // cudaEventDefault, not cudaEventDisableTiming: the flag that makes an event cheap
    // is exactly the flag that makes cudaEventElapsedTime refuse it.
    if (auto err = cudaEventCreateWithFlags(&slot_->start, cudaEventDefault); err != cudaSuccess)
      throw_cuda("cudaEventCreateWithFlags", err);
    if (auto err = cudaEventCreateWithFlags(&slot_->stop, cudaEventDefault); err != cudaSuccess)
      throw_cuda("cudaEventCreateWithFlags", err);
    t0_ = std::chrono::steady_clock::now();
    if (auto err = cudaEventRecord(slot_->start, cudf::get_default_stream().value());
        err != cudaSuccess)
      throw_cuda("cudaEventRecord", err);
  }

  ScopedNodeTimer(const ScopedNodeTimer&) = delete;
  ScopedNodeTimer& operator=(const ScopedNodeTimer&) = delete;

  /// Close the region. Idempotent: a second call does nothing, so a region can be
  /// stopped early without double-counting.
  void stop() {
    // First, and outside the slot check: the range is on its own switch, and the work
    // after this call belongs to the next node, not to this region.
    range_.reset();
    if (!slot_ || stopped_) return;
    stopped_ = true;
    if (auto err = cudaEventRecord(slot_->stop, cudf::get_default_stream().value());
        err != cudaSuccess)
      throw_cuda("cudaEventRecord", err);
    slot_->out.host_us = us_since(t0_, std::chrono::steady_clock::now());
  }

 private:
  bool stopped_ = false;
  RegionSlot* slot_ = nullptr;
  std::optional<scoped_range> range_;
  std::chrono::steady_clock::time_point t0_{};
};
}  // namespace

void set_node_timing(NodeTiming mode) { g_node_timing.store(mode, std::memory_order_relaxed); }

NodeTiming node_timing() { return g_node_timing.load(std::memory_order_relaxed); }

bool node_timing_enabled() { return node_timing() != NodeTiming::Off; }

void set_nvtx_ranges(bool on) { g_nvtx.store(on, std::memory_order_relaxed); }

void push_harness_range(const char* name) {
  if (!g_nvtx.load(std::memory_order_relaxed) || name == nullptr) return;
  // `emplace` on an engaged optional destroys the old range first and constructs the new
  // one after, which is pop-then-push in NVTX's own stack — the only order that leaves
  // that stack balanced if a caller pushes twice without popping.
  harness_range().emplace(name);
}

void pop_harness_range() { harness_range().reset(); }

bool harness_range_is_open() { return harness_range().has_value(); }

bool nvtx_ranges() { return g_nvtx.load(std::memory_order_relaxed); }

uint64_t varlen_content_bytes(const cudf::table_view& table) {
  uint64_t total = 0;
  for (cudf::size_type i = 0; i < table.num_columns(); ++i) {
    auto col = table.column(i);
    // Flat string columns only — no nested List types reach here. Matches the Rust
    // ColAccum content term (Σ value byte lengths = offsets[n]-offsets[0]).
    if (col.type().id() == cudf::type_id::STRING) {
      total += static_cast<uint64_t>(
          cudf::strings_column_view(col).chars_size(cudf::get_default_stream()));
    }
  }
  return total;
}

// Children of a plan node in canonical order — MUST match the Rust walk's child
// order so the caller's input handles line up with each node's inputs. Declared in
// plan_executor_internal.h for the tests that drive a plan node by node.
std::vector<const fb::PlanNode*> node_children(const fb::PlanNode* node) {
  switch (node->node_type()) {
    case fb::PlanNodeKind_CudfScan:
      return {};
    case fb::PlanNodeKind_CudfFilter:
      return {node->node_as_CudfFilter()->input()};
    case fb::PlanNodeKind_CudfProject:
      return {node->node_as_CudfProject()->input()};
    case fb::PlanNodeKind_CudfAggregate:
      return {node->node_as_CudfAggregate()->input()};
    case fb::PlanNodeKind_CudfHashJoin:
      return {node->node_as_CudfHashJoin()->left(), node->node_as_CudfHashJoin()->right()};
    case fb::PlanNodeKind_CudfCrossJoin:
      return {node->node_as_CudfCrossJoin()->left(), node->node_as_CudfCrossJoin()->right()};
    case fb::PlanNodeKind_CudfNestedLoopJoin:
      return {node->node_as_CudfNestedLoopJoin()->left(),
              node->node_as_CudfNestedLoopJoin()->right()};
    case fb::PlanNodeKind_CudfSort:
      return {node->node_as_CudfSort()->input()};
    case fb::PlanNodeKind_CudfCoalesceBatches:
      return {node->node_as_CudfCoalesceBatches()->input()};
    case fb::PlanNodeKind_CudfCoalescePartitions:
      return {node->node_as_CudfCoalescePartitions()->input()};
    case fb::PlanNodeKind_CudfRepartition:
      return {node->node_as_CudfRepartition()->input()};
    case fb::PlanNodeKind_CudfSortPreservingMerge:
      return {node->node_as_CudfSortPreservingMerge()->input()};
    case fb::PlanNodeKind_CudfUnion: {
      std::vector<const fb::PlanNode*> kids;
      if (auto* in = node->node_as_CudfUnion()->inputs()) {
        for (flatbuffers::uoffset_t i = 0; i < in->size(); ++i) kids.push_back(in->Get(i));
      }
      return kids;
    }
    case fb::PlanNodeKind_CudfLimit:
      return {node->node_as_CudfLimit()->input()};
    case fb::PlanNodeKind_CudfWindow:
      return {node->node_as_CudfWindow()->input()};
    default:
      throw std::runtime_error("node_children: unsupported PlanNodeKind: " +
                               std::to_string(node->node_type()));
  }
}

struct NodeSession::Impl {
  std::vector<uint8_t> buf;  // own the plan bytes so fb pointers stay valid
  const fb::GpuPlan* plan = nullptr;
  std::vector<const fb::PlanNode*> post_order;
  std::unordered_map<uint64_t, TableResult> registry;
  uint64_t next_handle = 1;
  /// Everything only a measurement reads, or null while timing is off — see
  /// `RegionSink`. Owned here, not by the timer, whose whole point is that it ends
  /// before the answer does.
  std::unique_ptr<RegionSink> sink;

  void index_post_order(const fb::PlanNode* node) {
    for (auto* child : node_children(node)) index_post_order(child);
    post_order.push_back(node);
  }

  /// The sink, created on first use. Null-returning while timing is off, which is what
  /// keeps a shipping query from allocating it.
  RegionSink* measuring() {
    if (!node_timing_enabled()) return nullptr;
    if (!sink) sink = std::make_unique<RegionSink>();
    return sink.get();
  }

  /// Remember which node a handle came out of, for the two entry points that are handed
  /// one and no seq. Measurement-only: `sink` is null on a shipping query.
  void note_producer(RegionSink* measuring_sink, uint64_t handle, uint64_t seq) {
    if (measuring_sink) measuring_sink->produced_by[handle] = seq;
  }

  ~Impl() {
    // Events outlive their regions by design, so the session is the only thing that can
    // free them — a plan ending without a collection must not leak them.
    if (!sink) return;
    for (auto& slot : sink->slots) {
      if (slot.start) cudaEventDestroy(slot.start);
      if (slot.stop) cudaEventDestroy(slot.stop);
    }
  }
};

NodeSession::NodeSession(const uint8_t* plan_bytes, uint64_t plan_len)
    : impl_(std::make_unique<Impl>()) {
  impl_->buf.assign(plan_bytes, plan_bytes + plan_len);
  impl_->plan = fb::GetGpuPlan(impl_->buf.data());
  if (!impl_->plan) throw std::runtime_error("failed to parse FlatBuffer GpuPlan");
  flatbuffers::Verifier verifier(impl_->buf.data(), impl_->buf.size(), /*max_depth=*/1024);
  if (!impl_->plan->Verify(verifier))
    throw std::runtime_error("FlatBuffer verification failed");
  auto* root = impl_->plan->root();
  if (!root) throw std::runtime_error("GpuPlan has no root node");
  impl_->index_post_order(root);
}

NodeSession::~NodeSession() = default;

size_t NodeSession::node_count() const { return impl_->post_order.size(); }

void NodeSession::execute_node(uint64_t seq, const uint64_t* input_handles,
                               const uint64_t* input_child_counts, size_t n_children,
                               uint64_t* out_handles, size_t out_cap, size_t* out_count,
                               NodeStats* out_stats) {
  if (seq >= impl_->post_order.size())
    throw std::runtime_error("NodeSession::execute_node: seq out of range");
  const fb::PlanNode* node = impl_->post_order[seq];
  // Once per call: every output partition this call emits carries the same index,
  // because what is counted is the ABI call and not what it produced.
  RegionSink* sink = impl_->measuring();
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  // One per call, so the per-partition ranges below nest inside it. `<seq>.<call>`
  // because seq alone does not identify a range — a batched run drives one seq many
  // times, and every repeat would carry the same name. Address first, kind after: the
  // address is what a record and an Nsight export join on.
  OptionalRange node_range([&] {
    return std::to_string(seq) + "." + std::to_string(call_index) + " " +
           fb::EnumNamePlanNodeKind(node->node_type());
  });

  // Each child contributes a VECTOR of partition handles; the flat
  // `input_handles` is grouped by child via `input_child_counts`.
  std::vector<std::vector<uint64_t>> child(n_children);
  size_t off = 0;
  for (size_t c = 0; c < n_children; ++c) {
    size_t cnt = input_child_counts ? static_cast<size_t>(input_child_counts[c]) : 0;
    child[c].assign(input_handles + off, input_handles + off + cnt);
    off += cnt;
  }

  // CudfScan with an explicit RG→batch→partition MAP → emit N partitions, one per
  // ScanBatch, each a set_row_groups read of that entry's row groups. This is the
  // SAME map the Rust CpuNodeExecutor / golden generator replay, so per-partition
  // row counts match by construction. EMPTY map => fall through to the generic
  // path (single-partition read of `row_groups`).
  if (node->node_type() == fb::PlanNodeKind_CudfScan) {
    const fb::CudfScan* scan = node->node_as_CudfScan();
    if (scan->batches() && scan->batches()->size() > 0) {
      size_t n = scan->batches()->size();
      if (n > out_cap)
        throw std::runtime_error("NodeSession::execute_node: out_handles buffer too small");
      for (size_t p = 0; p < n; ++p) {
        const fb::ScanBatch* b = scan->batches()->Get(static_cast<flatbuffers::uoffset_t>(p));
        const auto* map_groups = b->row_groups();
        ScopedNodeTimer timer(sink, seq, p, call_index);
        TableResult result = execute_scan(
            scan, map_groups
                      ? cudf::host_span<const uint32_t>{map_groups->data(), map_groups->size()}
                      : cudf::host_span<const uint32_t>{});
        timer.stop();
        auto tv = result.table->view();
        if (out_stats)
          out_stats[p] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
        uint64_t handle = impl_->next_handle++;
        impl_->note_producer(sink, handle, seq);
        impl_->registry.emplace(handle, std::move(result));
        out_handles[p] = handle;  // map entries are stored in partition order 0..n-1
      }
      *out_count = n;
      return;
    }
  }

  // Partition-COLLAPSING nodes → concatenate ALL M child partitions into ONE
  // output (BUFFERING: the full table goes resident), in partition-index order to
  // match the Rust CpuNodeExecutor's `collapses_partitions` concat. NOT a
  // per-partition passthrough.
  //   - CudfCoalescePartitions: the explicit M→1 concat before a Hash repartition.
  //   - CudfSortPreservingMerge: N sorted partitions → one (q1's top ORDER BY node).
  if (node->node_type() == fb::PlanNodeKind_CudfCoalescePartitions ||
      node->node_type() == fb::PlanNodeKind_CudfSortPreservingMerge) {
    if (out_cap < 1)
      throw std::runtime_error("NodeSession::execute_node: out_handles buffer too small");
    std::vector<TableResult> owned;
    std::vector<cudf::table_view> views;
    owned.reserve(child[0].size());
    views.reserve(child[0].size());
    for (uint64_t h : child[0]) {
      auto it = impl_->registry.find(h);
      if (it == impl_->registry.end())
        throw std::runtime_error("NodeSession::execute_node: unknown input handle");
      owned.push_back(std::move(it->second));
      impl_->registry.erase(it);
      views.push_back(owned.back().table->view());
    }
    // A collapse of nothing has no schema to answer with: the node declares no schema of
    // its own, and concatenating no views gives a table of no columns, which is not a
    // batch anything above can read. Both backends emit nothing for an empty lane
    // instead, so reaching this is a driver that called a node it had no batches for (#173).
    if (views.empty())
      throw std::runtime_error(
          "NodeSession::execute_node: a collapse with no input handles has no columns to "
          "answer with — an empty lane emits nothing rather than calling this");
    TableResult result;
    result.column_names = owned[0].column_names;

    const fb::CudfSortPreservingMerge* spm =
        (node->node_type() == fb::PlanNodeKind_CudfSortPreservingMerge)
            ? node->node_as_CudfSortPreservingMerge()
            : nullptr;
    // Everything above is host-side bookkeeping (handle lookups, table_view moves);
    // the device work is the merge/concat + optional top-N slice below.
    ScopedNodeTimer timer(sink, seq, 0, call_index);
    if (spm && spm->exprs() && spm->exprs()->size() > 0 && views.size() > 1) {
      // (#99) SortPreservingMerge is a K-WAY MERGE by the SPM's sort keys, NOT a
      // concat: concat leaves the output only per-partition-sorted, so a downstream
      // LIMIT/fetch picks the wrong top-N. cudf::merge's precondition holds because
      // each input was sorted upstream by the SAME CudfSort spec. Column-ref keys
      // only; an expression sort key would need per-partition materialization, so
      // throw rather than silently mis-merge.
      std::vector<cudf::size_type> key_cols;
      std::vector<cudf::order> orders;
      std::vector<cudf::null_order> null_orders;
      for (flatbuffers::uoffset_t i = 0; i < spm->exprs()->size(); ++i) {
        auto* se = spm->exprs()->Get(i);
        auto* expr = se->expr();
        if (!expr || expr->node_type() != fb::ExprNode_ColumnRef)
          throw std::runtime_error(
              "CudfSortPreservingMerge: expression sort key not supported by the k-way "
              "merge (needs per-partition materialization) — file an increment");
        key_cols.push_back(
            static_cast<cudf::size_type>(expr->node_as_ColumnRef()->index()));
        orders.push_back(se->asc() ? cudf::order::ASCENDING : cudf::order::DESCENDING);
        null_orders.push_back(se->nulls_first() ? cudf::null_order::BEFORE
                                                : cudf::null_order::AFTER);
      }
      result.table = cudf::merge(views, key_cols, orders, null_orders);
      // Apply the SPM's own fetch (top-N) AFTER the global merge (-1 = unlimited).
      if (spm->fetch() >= 0) {
        auto n = std::min(static_cast<cudf::size_type>(spm->fetch()),
                          result.table->view().num_rows());
        std::vector<cudf::size_type> slice_indices{0, n};
        auto sliced = cudf::slice(result.table->view(), slice_indices);
        result.table = std::make_unique<cudf::table>(sliced[0]);
      }
    } else {
      // CudfCoalescePartitions, or an SPM with no sort keys / a single partition:
      // a plain in-order concat is the correct collapse.
      result.table = cudf::concatenate(views);
    }
    timer.stop();
    auto tv = result.table->view();
    if (out_stats)
      out_stats[0] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
    uint64_t handle = impl_->next_handle++;
    impl_->note_producer(sink, handle, seq);
    impl_->registry.emplace(handle, std::move(result));
    out_handles[0] = handle;
    *out_count = 1;
    return;
  }

  // CudfRepartition Hash → scatter the ONE input table into N partitions by
  // Spark-murmur3 (comet-identical) hash of the key columns, so per-partition row
  // counts match the CPU twin by construction; the live conformance gate proves
  // the kernel is bit-equal to comet. Post-lowering the child is a
  // CudfCoalescePartitions (single handle), but concat defensively anyway.
  //
  // That concat has no caller: the planner puts a GpuCoalesceAllBatches above the merge
  // feeding an emit, so this arm is handed exactly one handle per call. The modes that
  // lowered a shuffle differently are gone, so nothing will grow a second caller — #197
  // retires it.
  if (node->node_type() == fb::PlanNodeKind_CudfRepartition &&
      node->node_as_CudfRepartition()->kind() == fb::PartitioningKind_Hash) {
    const fb::CudfRepartition* rp = node->node_as_CudfRepartition();
    size_t n = static_cast<size_t>(rp->num_partitions());
    if (n == 0 || n > out_cap)
      throw std::runtime_error("NodeSession::execute_node: bad Hash repartition out count");

    // Gather + concat the child partitions into one table (matches the CPU concat).
    std::vector<TableResult> owned;
    std::vector<cudf::table_view> views;
    owned.reserve(child[0].size());
    views.reserve(child[0].size());
    for (uint64_t h : child[0]) {
      auto it = impl_->registry.find(h);
      if (it == impl_->registry.end())
        throw std::runtime_error("NodeSession::execute_node: unknown input handle");
      owned.push_back(std::move(it->second));
      impl_->registry.erase(it);
      views.push_back(owned.back().table->view());
    }
    std::vector<std::string> column_names =
        owned.empty() ? std::vector<std::string>{} : owned[0].column_names;
    // The concat + hash-scatter is shared by all N partitions and charged to p0, so
    // Σ-over-partitions still equals the node total; only the slice copies below are
    // separable. p0's region stays open across both rather than closing and reopening,
    // because N output partitions must cost exactly N timed regions.
    ScopedNodeTimer shared_timer(sink, seq, 0, call_index);
    std::unique_ptr<cudf::table> combined =
        (owned.size() == 1) ? std::move(owned[0].table) : cudf::concatenate(views);

    // Hash keys: ColumnRef indices into the (partial-agg output) table. ColumnRef
    // keys only for now — the group-by columns.
    std::vector<cudf::size_type> key_cols;
    if (auto* exprs = rp->hash_exprs()) {
      for (flatbuffers::uoffset_t i = 0; i < exprs->size(); ++i) {
        const fb::Expr* e = exprs->Get(i);
        if (e->node_type() != fb::ExprNode_ColumnRef)
          throw std::runtime_error("CudfRepartition: only ColumnRef hash keys supported");
        key_cols.push_back(static_cast<cudf::size_type>(e->node_as_ColumnRef()->index()));
      }
    }

    auto tv = combined->view();
    auto [parted, offsets] = peacock::partitioning::spark_hash_partition(
        tv, key_cols, static_cast<cudf::size_type>(n));
    const cudf::size_type total = parted->num_rows();
    const cudf::table_view pv = parted->view();
    for (size_t p = 0; p < n; ++p) {
      cudf::size_type start = offsets[p];
      cudf::size_type end = (p + 1 < n) ? offsets[p + 1] : total;
      // p0 finishes the shared region opened above; p1..N-1 open their own.
      std::optional<ScopedNodeTimer> own;
      if (p > 0) own.emplace(sink, seq, p, call_index);
      // One owning table per partition (slice → deep copy so each handle owns memory).
      cudf::table_view slice = cudf::slice(pv, {start, end}).front();
      TableResult part;
      part.column_names = column_names;
      part.table = std::make_unique<cudf::table>(slice);
      if (p == 0)
        shared_timer.stop();
      else
        own->stop();
      auto ptv = part.table->view();
      if (out_stats)
        out_stats[p] = NodeStats{static_cast<uint64_t>(ptv.num_rows()), varlen_content_bytes(ptv)};
      uint64_t handle = impl_->next_handle++;
      impl_->note_producer(sink, handle, seq);
      impl_->registry.emplace(handle, std::move(part));
      out_handles[p] = handle;
    }
    *out_count = n;
    return;
  }

  // Output partition count. Ordinary ops MAP over their children's partitions (all
  // children carry the same count), so n_out = child[0]'s count. Partition-changing
  // ops (CudfScan map, CudfCoalescePartitions, Hash repartition) returned above.
  size_t n_out = (n_children > 0) ? child[0].size() : 1;
  if (n_out == 0) n_out = 1;
  if (n_out > out_cap)
    throw std::runtime_error("NodeSession::execute_node: out_handles buffer too small");
  // The per-partition MAP arm reads child[c][p] for every c, so every child must
  // carry the same partition count. Partitioned joins (mismatched counts) are not
  // implemented — fail LOUDLY rather than read out of bounds.
  for (size_t c = 1; c < n_children; ++c) {
    if (child[c].size() != n_out)
      throw std::runtime_error(
          "NodeSession::execute_node: children have mismatched partition counts "
          "(multi-partition joins are not implemented yet)");
  }

  for (size_t p = 0; p < n_out; ++p) {
    std::vector<TableResult> inputs;
    inputs.reserve(n_children);
    for (size_t c = 0; c < n_children; ++c) {
      uint64_t h = child[c][p];  // partition p of child c (ordinary op maps per partition)
      auto it = impl_->registry.find(h);
      if (it == impl_->registry.end())
        throw std::runtime_error("NodeSession::execute_node: unknown input handle");
      inputs.push_back(std::move(it->second));
      impl_->registry.erase(it);
    }
    ScopedNodeTimer timer(sink, seq, p, call_index);
    TableResult result = execute_one(node, std::move(inputs));
    timer.stop();
    auto tv = result.table->view();
    if (out_stats)
      out_stats[p] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
    uint64_t handle = impl_->next_handle++;
    impl_->note_producer(sink, handle, seq);
    impl_->registry.emplace(handle, std::move(result));
    out_handles[p] = handle;
  }
  *out_count = n_out;
}

uint64_t NodeSession::execute_scan_rowgroups(uint64_t seq,
                                             cudf::host_span<const uint32_t> row_groups,
                                             NodeStats* out_stats) {
  if (seq >= impl_->post_order.size())
    throw std::runtime_error("NodeSession::execute_scan_rowgroups: seq out of range");
  // Refused here rather than only at the C wrapper: `execute_scan` reads an empty
  // override as "no override" and falls back to the node's own list, so a caller that
  // named a set would silently get a whole-table read.
  if (row_groups.empty())
    throw std::runtime_error(
        "NodeSession::execute_scan_rowgroups: empty row-group list — name at least one");
  const fb::PlanNode* node = impl_->post_order[seq];
  // Once per call: every output partition this call emits carries the same index,
  // because what is counted is the ABI call and not what it produced.
  RegionSink* sink = impl_->measuring();
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  // The same range `execute_node` opens, because this is the same thing from a capture's
  // side: one call against one seq. Without it a batched scan -- most of a query at sf40 --
  // is the one region a capture cannot see.
  OptionalRange node_range([&] {
    return std::to_string(seq) + "." + std::to_string(call_index) + " " +
           fb::EnumNamePlanNodeKind(node->node_type());
  });
  if (node->node_type() != fb::PlanNodeKind_CudfScan)
    throw std::runtime_error(std::string("NodeSession::execute_scan_rowgroups: seq ") +
                             std::to_string(seq) + " is a " +
                             fb::EnumNamePlanNodeKind(node->node_type()) + ", not a CudfScan");

  ScopedNodeTimer timer(sink, seq, 0, call_index);
  TableResult result;
  try {
    result = execute_scan(node->node_as_CudfScan(), row_groups);
  } catch (const std::exception& e) {
    // cuDF names neither the node nor the list it was handed, and the caller here is a
    // partitioner's mapping — an index it cannot read is a planner defect, so the
    // message has to carry where the request came from.
    std::string groups;
    for (auto rg : row_groups) groups += (groups.empty() ? "" : ", ") + std::to_string(rg);
    throw std::runtime_error("NodeSession::execute_scan_rowgroups: seq " + std::to_string(seq) +
                             " reading row groups [" + groups + "]: " + e.what());
  }
  timer.stop();
  auto tv = result.table->view();
  if (out_stats)
    *out_stats = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
  uint64_t handle = impl_->next_handle++;
  impl_->note_producer(sink, handle, seq);
  impl_->registry.emplace(handle, std::move(result));
  return handle;
}

// Shared by the export and the slice so the two cannot disagree. Its twin on the other
// side of the ABI is `RowRange::clamp`, which the CPU backend applies to a batch that
// never crosses it — the same rule in two languages, and the backends answering a limit
// differently is what keeping them together prevents.
std::pair<cudf::size_type, cudf::size_type> clamp_row_range(uint64_t offset, uint64_t length,
                                                            cudf::size_type num_rows) {
  const uint64_t rows = static_cast<uint64_t>(num_rows);
  const uint64_t begin = std::min(offset, rows);
  // Against `rows - begin` rather than `begin + length`, which overflows at the
  // to-the-end sentinel.
  const uint64_t take = std::min(length, rows - begin);
  return {static_cast<cudf::size_type>(begin), static_cast<cudf::size_type>(begin + take)};
}

uint64_t NodeSession::slice_handle(uint64_t handle, uint64_t offset, uint64_t length) {
  auto it = impl_->registry.find(handle);
  if (it == impl_->registry.end())
    throw std::runtime_error("NodeSession::slice_handle: unknown input handle");
  TableResult input = std::move(it->second);
  impl_->registry.erase(it);

  // A limit carries no seq of its own, so the region is the sliced node's: what the call
  // costs belongs beside the work that produced the rows it trims.
  RegionSink* sink = impl_->measuring();
  const uint64_t seq = sink ? sink->producer_of(handle) : 0;
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  OptionalRange node_range(
      [&] { return std::to_string(seq) + "." + std::to_string(call_index) + " slice_handle"; });

  auto [begin, end] = clamp_row_range(offset, length, input.table->view().num_rows());
  TableResult result;
  result.column_names = input.column_names;
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  // An owning copy of the kept rows, so the input table can go: a view would keep the
  // whole batch resident, which is the cost the mid-plan limit exists to avoid.
  result.table =
      std::make_unique<cudf::table>(cudf::slice(input.table->view(), {begin, end}).front());
  timer.stop();
  uint64_t out = impl_->next_handle++;
  // The trimmed rows are still that node's output, so the export downstream of a limit
  // names the same seq the slice did.
  impl_->note_producer(sink, out, seq);
  impl_->registry.emplace(out, std::move(result));
  return out;
}

uint64_t NodeSession::adopt(TableResult result) {
  uint64_t handle = impl_->next_handle++;
  impl_->registry.emplace(handle, std::move(result));
  return handle;
}

void NodeSession::time_export(uint64_t handle, const std::function<void()>& body) {
  RegionSink* sink = impl_->measuring();
  const uint64_t seq = sink ? sink->producer_of(handle) : 0;
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  OptionalRange node_range([&] {
    return std::to_string(seq) + "." + std::to_string(call_index) + " result_from_handle";
  });
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  body();
  timer.stop();
}

std::vector<NodeRegion> NodeSession::collect_node_regions() {
  std::vector<NodeRegion> out;
  if (!impl_->sink) return out;
  out.reserve(impl_->sink->slots.size());
  std::optional<std::pair<const char*, cudaError_t>> failure;
  for (auto& slot : impl_->sink->slots) {
    // Synchronize on the stop event, not the stream: the stream may have moved on to
    // work that belongs to nobody's region, and draining that would bill this
    // collection for it.
    if (auto err = cudaEventSynchronize(slot.stop); err != cudaSuccess && !failure)
      failure = {"cudaEventSynchronize", err};
    float ms = 0.0f;
    if (auto err = cudaEventElapsedTime(&ms, slot.start, slot.stop); err != cudaSuccess && !failure)
      failure = {"cudaEventElapsedTime", err};
    slot.out.device_us = static_cast<uint64_t>(ms * 1000.0f);
    out.push_back(slot.out);
    // Destroyed on the failing path too, and that is why the throw waits for the end of
    // the loop: a partial drain would leave the rest of the events owned by nobody.
    cudaEventDestroy(slot.start);
    cudaEventDestroy(slot.stop);
  }
  // A second call reports nothing rather than everything twice, and a session driven
  // across many plans does not accumulate events without bound.
  impl_->sink->slots.clear();
  if (failure) throw_cuda(failure->first, failure->second);
  return out;
}

size_t NodeSession::recorded_regions() const { return impl_->sink ? impl_->sink->slots.size() : 0; }

const TableResult& NodeSession::table_for(uint64_t handle) const {
  auto it = impl_->registry.find(handle);
  if (it == impl_->registry.end())
    throw std::runtime_error("NodeSession::table_for: unknown handle");
  return it->second;
}

void NodeSession::release(uint64_t handle) { impl_->registry.erase(handle); }


}  // namespace peacock
