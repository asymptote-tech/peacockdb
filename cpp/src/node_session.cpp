// NodeSession: node-by-node execution over a parsed plan, keeping intermediates
// resident in a handle registry, plus the two free functions only its stats and its
// row ranges need. `node_children` is the session's own notion of child order, and the
// one thing a caller needs to address the same nodes it does.

#include "peacock/operators.h"
#include "peacock/expr.h"
#include "peacock/partitioning.hpp"
#include "operators/join_session.h"
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
  /// and no seq, and this is the only thing that can name one for them. `kAdopted` marks a
  /// table the harness uploaded, which no node produced.
  static constexpr uint64_t kAdopted = UINT64_MAX;
  std::unordered_map<uint64_t, uint64_t> produced_by;
  std::deque<RegionSlot> slots;

  /// The next call's index for `seq`, consuming it.
  uint64_t take_call_index(uint64_t seq, size_t node_count) {
    if (calls_made.size() != node_count) calls_made.assign(node_count, 0);
    return calls_made[seq]++;
  }

  /// The seq behind a handle. Throws for one it cannot name — a handle from before the
  /// mode was turned on, or an adopted one — rather than charging a real node's seq: the
  /// journal on the other side would name another, and the join would not close.
  uint64_t producer_of(uint64_t handle, const char* what) const {
    auto it = produced_by.find(handle);
    if (it == produced_by.end())
      throw std::runtime_error(std::string(what) + ": handle " + std::to_string(handle) +
                               " was produced before timing was turned on, so no node can be "
                               "charged for it");
    if (it->second == kAdopted)
      throw std::runtime_error(std::string(what) + ": handle " + std::to_string(handle) +
                               " was adopted from Arrow, which no node produced, so nothing "
                               "can be charged for it");
    return it->second;
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

// `offsets[last] - offsets[first]`, both edges in one pair of async copies and one sync.
// `cudf::get_element` would allocate a device scalar and launch a kernel per read, and this
// runs for every string column of every node output of every batch.
template <typename T>
static int64_t offset_span(const cudf::column_view& offsets, cudf::size_type first,
                           cudf::size_type last, rmm::cuda_stream_view stream) {
  T edges[2] = {0, 0};
  const T* data = offsets.data<T>();
  // A copy that fails synchronously never enqueues and leaves nothing sticky, so the sync
  // would report success and the span would read 0 bytes as an answer.
  auto err =
      cudaMemcpyAsync(&edges[0], data + first, sizeof(T), cudaMemcpyDeviceToHost, stream.value());
  if (err == cudaSuccess)
    err =
        cudaMemcpyAsync(&edges[1], data + last, sizeof(T), cudaMemcpyDeviceToHost, stream.value());
  if (err == cudaSuccess) err = cudaStreamSynchronize(stream.value());
  if (err != cudaSuccess)
    throw std::runtime_error(std::string("varlen_content_bytes: reading string offsets: ") +
                             cudaGetErrorString(err));
  return static_cast<int64_t>(edges[1]) - static_cast<int64_t>(edges[0]);
}

// The content bytes of a strings column's own rows. `chars_size` reads the unsliced
// parent's last offset ("does not reflect a sliced parent column view",
// strings_column_view.hpp), and a scatter partition is a slice of the scatter's table
// (#145) — so a sliced view reads its own two edges instead.
static uint64_t string_content_bytes(const cudf::column_view& col) {
  if (col.size() == 0) return 0;
  cudf::strings_column_view sv(col);
  auto stream = cudf::get_default_stream();
  auto offsets = sv.offsets();
  // Every column but a scatter partition is the whole of its parent, and for those
  // `chars_size` is already the answer in one read.
  if (sv.offset() == 0 && sv.size() + 1 == offsets.size())
    return static_cast<uint64_t>(sv.chars_size(stream));
  const auto first = sv.offset();
  const auto last = sv.offset() + sv.size();
  return static_cast<uint64_t>(offsets.type().id() == cudf::type_id::INT64
                                   ? offset_span<int64_t>(offsets, first, last, stream)
                                   : offset_span<int32_t>(offsets, first, last, stream));
}

uint64_t varlen_content_bytes(const cudf::table_view& table) {
  uint64_t total = 0;
  for (cudf::size_type i = 0; i < table.num_columns(); ++i) {
    auto col = table.column(i);
    // Flat string columns only — no nested List types reach here. Matches the Rust
    // ColAccum content term (Σ value byte lengths = offsets[n]-offsets[0]).
    if (col.type().id() == cudf::type_id::STRING) total += string_content_bytes(col);
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
    // A leaf: a session's build and probe tables arrive through peacock_join_build and
    // peacock_join_probe, so the plan carries no child stubs for them.
    case fb::PlanNodeKind_CudfJoin:
      return {};
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
  /// Live join sessions, in their own map: a join id is not a handle, and a session
  /// outlives every table it was given. Freed here, so `end_plan` and the error path free
  /// them as they free handles.
  std::unordered_map<uint64_t, std::unique_ptr<JoinSession>> joins;
  uint64_t next_join = 1;
  /// Which seq each join belongs to — its regions are that node's.
  std::unordered_map<uint64_t, uint64_t> join_seq;
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

  /// Every handle a consumer can read enters here, so this is where the handle's own
  /// shape is checked: one owner and one name per column, and never zero columns — a
  /// zero-column `table_view` reads 0 rows whatever it held. A check in `TableResult`'s
  /// constructors cannot stand in, because its fields are public and a caller may
  /// assemble one without calling any of them (#164).
  uint64_t register_handle(TableResult result, RegionSink* measuring_sink, uint64_t seq) {
    if (result.columns.empty())
      throw std::runtime_error(
          "NodeSession: a handle of no columns reads as no rows; the plan's placeholder "
          "column exists so that none is ever registered");
    if (result.column_names.size() != result.columns.size() ||
        result.owners.size() != result.columns.size())
      throw std::runtime_error("NodeSession: a handle of " +
                               std::to_string(result.columns.size()) + " columns under " +
                               std::to_string(result.column_names.size()) + " names and " +
                               std::to_string(result.owners.size()) + " owners (#164)");
    uint64_t handle = next_handle++;
    note_producer(measuring_sink, handle, seq);
    registry.emplace(handle, std::move(result));
    return handle;
  }

  /// 0 when a call answers no table; otherwise a fresh handle with its stats, as a
  /// one-output `execute_node` registers one.
  uint64_t register_join_output(uint64_t seq, std::optional<TableResult> out, NodeStats* out_stats,
                                RegionSink* measuring_sink) {
    if (out_stats) *out_stats = NodeStats{};
    if (!out) return 0;
    auto tv = out->view();
    if (out_stats)
      *out_stats = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
    return register_handle(std::move(*out), measuring_sink, seq);
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
        auto tv = result.view();
        if (out_stats)
          out_stats[p] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
        out_handles[p] = impl_->register_handle(std::move(result), sink, seq);
        // map entries are stored in partition order 0..n-1
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
      views.push_back(owned.back().view());
    }
    // A collapse of nothing has no schema to answer with: the node declares no schema of
    // its own, and concatenating no views gives a table of no columns, which is not a
    // batch anything above can read. Both backends emit nothing for an empty lane
    // instead, so reaching this is a driver that called a node it had no batches for (#173).
    if (views.empty())
      throw std::runtime_error(
          "NodeSession::execute_node: a collapse with no input handles has no columns to "
          "answer with — an empty lane emits nothing rather than calling this");
    std::unique_ptr<cudf::table> merged;

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
      merged = cudf::merge(views, key_cols, orders, null_orders);
      // Apply the SPM's own fetch (top-N) AFTER the global merge (-1 = unlimited).
      if (spm->fetch() >= 0) {
        auto n = std::min(static_cast<cudf::size_type>(spm->fetch()), merged->num_rows());
        std::vector<cudf::size_type> slice_indices{0, n};
        auto sliced = cudf::slice(merged->view(), slice_indices);
        // An owning copy of the top N, not a view: a view would pin the whole merged
        // table, which is the memory this fetch exists to give back.
        merged = std::make_unique<cudf::table>(sliced[0]);
      }
    } else {
      // CudfCoalescePartitions, or an SPM with no sort keys / a single partition:
      // a plain in-order concat is the correct collapse.
      merged = cudf::concatenate(views);
    }
    timer.stop();
    TableResult result = TableResult::owning(std::move(merged), owned[0].column_names);
    auto tv = result.view();
    if (out_stats)
      out_stats[0] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
    out_handles[0] = impl_->register_handle(std::move(result), sink, seq);
    *out_count = 1;
    return;
  }

  // CudfRepartition Hash → scatter the ONE input table into N partitions by
  // Spark-murmur3 (comet-identical) hash of the key columns, so per-partition row
  // counts match the CPU twin by construction; the live conformance gate proves
  // the kernel is bit-equal to comet. One handle per call: the emitter's contract (#197).
  if (node->node_type() == fb::PlanNodeKind_CudfRepartition &&
      node->node_as_CudfRepartition()->kind() == fb::PartitioningKind_Hash) {
    const fb::CudfRepartition* rp = node->node_as_CudfRepartition();
    size_t n = static_cast<size_t>(rp->num_partitions());
    if (n == 0 || n > out_cap)
      throw std::runtime_error("NodeSession::execute_node: bad Hash repartition out count");

    if (child[0].size() != 1)
      throw std::runtime_error(
          "NodeSession::execute_node: a Hash repartition is handed exactly one handle per "
          "call — the emitter sends one batch a call (gpu_backend/emit.rs) — and got " +
          std::to_string(child[0].size()) + " (#197)");
    auto it = impl_->registry.find(child[0][0]);
    if (it == impl_->registry.end())
      throw std::runtime_error("NodeSession::execute_node: unknown input handle");
    TableResult input = std::move(it->second);
    impl_->registry.erase(it);
    std::vector<std::string> column_names = input.column_names;
    // The hash-scatter is shared by all N partitions and charged to p0, so
    // Σ-over-partitions still equals the node total; p1..N-1 now time only their slice.
    // p0's region stays open across the scatter rather than closing and reopening,
    // because N output partitions must cost exactly N timed regions.
    ScopedNodeTimer shared_timer(sink, seq, 0, call_index);

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

    auto [parted, offsets] = peacock::partitioning::spark_hash_partition(
        input.view(), key_cols, static_cast<cudf::size_type>(n));
    // The input goes as soon as the partitioned copy exists: holding it while every
    // partition was copied out beside it cost a whole input of allocation per call (#145).
    input = TableResult{};
    const cudf::size_type total = parted->num_rows();
    // The N partitions share these columns, so an undrained lane keeps the whole
    // partitioned table alive — worse under skew than the copies were.
    TableResult whole = TableResult::owning(std::move(parted), std::move(column_names));
    for (size_t p = 0; p < n; ++p) {
      cudf::size_type start = offsets[p];
      cudf::size_type end = (p + 1 < n) ? offsets[p + 1] : total;
      // p0 finishes the shared region opened above; p1..N-1 open their own.
      std::optional<ScopedNodeTimer> own;
      if (p > 0) own.emplace(sink, seq, p, call_index);
      // A row-range view sharing the partitioned table's column owners: no copy.
      TableResult part = whole.slice(start, end);
      if (p == 0)
        shared_timer.stop();
      else
        own->stop();
      auto ptv = part.view();
      if (out_stats)
        out_stats[p] = NodeStats{static_cast<uint64_t>(ptv.num_rows()), varlen_content_bytes(ptv)};
      out_handles[p] = impl_->register_handle(std::move(part), sink, seq);
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
    auto tv = result.view();
    if (out_stats)
      out_stats[p] = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
    out_handles[p] = impl_->register_handle(std::move(result), sink, seq);
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
  auto tv = result.view();
  if (out_stats)
    *out_stats = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
  return impl_->register_handle(std::move(result), sink, seq);
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
  // A limit carries no seq of its own, so the region is the sliced node's: what the call
  // costs belongs beside the work that produced the rows it trims. Named before the
  // handle is consumed, so a refusal leaves the registry as it was.
  RegionSink* sink = impl_->measuring();
  const uint64_t seq = sink ? sink->producer_of(handle, "slice_handle") : 0;
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  TableResult input = std::move(it->second);
  impl_->registry.erase(it);
  OptionalRange node_range(
      [&] { return std::to_string(seq) + "." + std::to_string(call_index) + " slice_handle"; });

  auto [begin, end] = clamp_row_range(offset, length, input.view().num_rows());
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  // An owning copy of the kept rows rather than `input.slice`, so the input table can go:
  // a view would keep the whole batch resident, which is the cost the mid-plan limit
  // exists to avoid.
  TableResult result = TableResult::owning(
      std::make_unique<cudf::table>(cudf::slice(input.view(), {begin, end}).front()),
      input.column_names);
  timer.stop();
  // The trimmed rows are still that node's output, so the export downstream of a limit
  // names the same seq the slice did.
  return impl_->register_handle(std::move(result), sink, seq);
}

namespace {
/// A resident input by handle, consumed. The join calls take one table each, so they do
/// not go through `execute_node`'s child-vector resolution.
TableResult take_handle(std::unordered_map<uint64_t, TableResult>& registry, uint64_t handle,
                        const char* who) {
  auto it = registry.find(handle);
  if (it == registry.end()) throw std::runtime_error(std::string(who) + ": unknown input handle");
  TableResult t = std::move(it->second);
  registry.erase(it);
  return t;
}
}  // namespace

uint64_t NodeSession::join_build(uint64_t seq, uint64_t build, NodeStats* out_stats) {
  if (seq >= impl_->post_order.size())
    throw std::runtime_error("NodeSession::join_build: seq out of range");
  const fb::PlanNode* node = impl_->post_order[seq];
  if (node->node_type() != fb::PlanNodeKind_CudfJoin)
    throw std::runtime_error("NodeSession::join_build: seq " + std::to_string(seq) + " is a " +
                             fb::EnumNamePlanNodeKind(node->node_type()) + ", not a CudfJoin");
  RegionSink* sink = impl_->measuring();
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  OptionalRange node_range(
      [&] { return std::to_string(seq) + "." + std::to_string(call_index) + " join_build"; });
  std::optional<TableResult> batch;
  if (build != 0) batch = take_handle(impl_->registry, build, "NodeSession::join_build");
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  auto session = std::make_unique<JoinSession>(node->node_as_CudfJoin(), std::move(batch));
  timer.stop();
  if (out_stats) *out_stats = NodeStats{};  // the build answers no table
  const uint64_t id = impl_->next_join++;
  impl_->joins.emplace(id, std::move(session));
  impl_->join_seq.emplace(id, seq);
  return id;
}

uint64_t NodeSession::join_probe(uint64_t join, uint64_t probe, NodeStats* out_stats) {
  auto it = impl_->joins.find(join);
  // JoinRefusal, so the C wrapper keeps the session for a call that named a join wrongly and
  // ends the query for one that failed mid-work. Nothing has been consumed at this point.
  if (it == impl_->joins.end())
    throw JoinRefusal("NodeSession::join_probe: unknown join " + std::to_string(join));
  const uint64_t seq = impl_->join_seq.at(join);
  RegionSink* sink = impl_->measuring();
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  OptionalRange node_range(
      [&] { return std::to_string(seq) + "." + std::to_string(call_index) + " join_probe"; });
  TableResult batch = take_handle(impl_->registry, probe, "NodeSession::join_probe");
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  auto out = it->second->probe(std::move(batch));
  timer.stop();
  return impl_->register_join_output(seq, std::move(out), out_stats, sink);
}

uint64_t NodeSession::join_finish(uint64_t join, NodeStats* out_stats) {
  auto it = impl_->joins.find(join);
  if (it == impl_->joins.end())
    throw JoinRefusal("NodeSession::join_finish: unknown join " + std::to_string(join));
  const uint64_t seq = impl_->join_seq.at(join);
  RegionSink* sink = impl_->measuring();
  const uint64_t call_index = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  OptionalRange node_range(
      [&] { return std::to_string(seq) + "." + std::to_string(call_index) + " join_finish"; });
  ScopedNodeTimer timer(sink, seq, 0, call_index);
  auto out = it->second->finish();
  timer.stop();
  return impl_->register_join_output(seq, std::move(out), out_stats, sink);
}

void NodeSession::join_release(uint64_t join) {
  impl_->joins.erase(join);
  impl_->join_seq.erase(join);
}

uint64_t NodeSession::adopt(TableResult result) {
  // No node produced it, and the sink says so rather than leaving the handle unknown: a
  // slice or an export of it under timing is then refused naming the adoption.
  return impl_->register_handle(std::move(result), impl_->measuring(), RegionSink::kAdopted);
}

void NodeSession::time_export(uint64_t handle, const std::function<void()>& body) {
  RegionSink* sink = impl_->measuring();
  const uint64_t seq = sink ? sink->producer_of(handle, "result_from_handle") : 0;
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
