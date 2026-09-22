#pragma once

#include <cudf/table/table.hpp>
#include <cudf/utilities/span.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace peacock {

/// Result of executing a plan node: a cuDF table plus column names.
struct TableResult {
  std::unique_ptr<cudf::table> table;
  std::vector<std::string> column_names;
};

/// Per-node actual costs returned across the FFI. The byte formula lives ONLY in
/// Rust (no CPU/GPU drift): Rust applies the schema+row-derived `ColAccum` overhead
/// and adds `varlen_content_bytes`, the one data-dependent term that only C++ can
/// measure on the resident table.
struct NodeStats {
  uint64_t rows = 0;
  /// Σ over var-length (string) output columns of content bytes
  /// (offsets[n]-offsets[0]); additive across columns, so one total suffices.
  uint64_t varlen_content_bytes = 0;
};

/// How per-node regions are measured. Off by default: measuring is not free.
enum class NodeTiming : int {
  Off = 0,
  /// CUDA events around the device work and the host clock around the whole call, with
  /// no sync inside the region. Device times are not known at region close and are read
  /// afterwards by `collect_node_regions`.
  Events = 1,
};

/// Set the timing mode (process-global; `Off` by default).
///
/// Opt-in because `Events`, though cheap, still allocates an event pair per region and
/// holds it until collection.
///
/// Neither mode removes every sync: `varlen_content_bytes` reads `chars_size` back, so
/// a node with STRING outputs synchronizes regardless.
void set_node_timing(NodeTiming mode);

/// The current timing mode (see `set_node_timing`).
NodeTiming node_timing();

/// True unless the mode is `Off`.
bool node_timing_enabled();

/// Emit NVTX ranges around plan nodes and their output partitions
/// (process-global; off by default).
///
/// A separate switch from `set_node_timing` on purpose. The two answer different
/// questions — ranges say where a node's work is on a timeline, the modes say how long
/// it took — and a profiled run wants the first without the second: recording an event
/// pair is device work, and a capture would show it inside the node.
///
/// Ranges go in our own NVTX domain, so a capture keeps them apart from the ones
/// libcudf pushes from inside the calls they enclose.
void set_nvtx_ranges(bool on);

/// Whether ranges are being emitted (see `set_nvtx_ranges`).
bool nvtx_ranges();

/// Open a named range in peacockdb's NVTX domain that outlives the call, and close it.
///
/// For a benchmark harness naming the case it is about to run, so a capture holding
/// several cases can say which query each node range belongs to — seq numbering restarts
/// with every plan, so the names alone cannot. No-ops while ranges are off.
///
/// One level: a second push without a pop replaces the first rather than nesting under
/// it. Nothing in the engine calls either.
void push_harness_range(const char* name);
void pop_harness_range();

/// One timed region: which call it was, and what it cost.
///
/// Separate from `NodeStats` because the two have different consumers. The driver reads
/// stats on every call and needs two numbers; nothing on the execution path reads any of
/// these. Carrying them in the returned struct made a shipping query pay for them on
/// every output partition of every call.
struct NodeRegion {
  /// The node whose output this call handled. `execute_node` and
  /// `execute_scan_rowgroups` name it; for `slice_handle` and the export it is the node
  /// that produced the handle, which is the only seq either of those can be given.
  uint64_t seq = 0;
  uint64_t partition = 0;
  /// Calls already made against this seq when this one began; 0 for the first. Per call,
  /// so the partitions of one call share it.
  uint64_t call_index = 0;
  /// `steady_clock` across the whole call, this partition's region.
  uint64_t host_us = 0;
  /// Between the region's two CUDA events, read at collection.
  uint64_t device_us = 0;
};

/// Σ var-length content bytes over a table's columns (see `NodeStats`).
uint64_t varlen_content_bytes(const cudf::table_view& table);

/// The half-open row range `[offset, offset+length)` names in a table of `num_rows`,
/// with `length == UINT64_MAX` meaning to the end.
///
/// An offset at or past the end gives an empty range, and a range running past the end
/// clamps to it — neither throws, because the caller is a limit interval whose fetch
/// legitimately overruns the batch it straddles.
std::pair<cudf::size_type, cudf::size_type> clamp_row_range(uint64_t offset, uint64_t length,
                                                            cudf::size_type num_rows);

/// Node-by-node execution session: parses a plan once and drives ONE node at a
/// time given already-resident child inputs, keeping intermediates resident in a
/// handle registry. The only way a plan is executed.
///
/// Nodes are addressed by canonical POST-ORDER sequence (children left-to-right,
/// then the node) — the SAME order the Rust walk uses, so the caller's child
/// handles align with each node's inputs.
class NodeSession {
 public:
  /// Parse + verify the plan and index its nodes in post-order.
  NodeSession(const uint8_t* plan_bytes, uint64_t plan_len);
  ~NodeSession();
  NodeSession(const NodeSession&) = delete;
  NodeSession& operator=(const NodeSession&) = delete;

  /// Number of plan nodes (post-order positions 0..count-1).
  size_t node_count() const;

  /// Execute the node at post-order `seq`. Each child contributes a VECTOR of
  /// partition handles: `input_handles` is the flattened concatenation grouped by
  /// child, `input_child_counts[c]` = child c's partition count. Output handles go
  /// to `out_handles[0..*out_count]` (caller buffer of `out_cap`) and
  /// `out_stats[0..*out_count]` is filled PER PARTITION, so Rust can sum the
  /// ColAccum overhead per partition: Σ_p ColAccum(rows_p), NOT ColAccum(Σ rows).
  /// Input handles are CONSUMED.
  void execute_node(uint64_t seq, const uint64_t* input_handles,
                    const uint64_t* input_child_counts, size_t n_children,
                    uint64_t* out_handles, size_t out_cap, size_t* out_count,
                    NodeStats* out_stats);

  /// Execute the `CudfScan` at post-order `seq` reading exactly `row_groups` rather
  /// than the list the node carries, and register its one output table. Throws naming
  /// the kind when `seq` is any other node — this entry point has no generic arm to
  /// fall back to. `out_stats` may be null.
  uint64_t execute_scan_rowgroups(uint64_t seq, cudf::host_span<const uint32_t> row_groups,
                                  NodeStats* out_stats);

  /// Rows `[offset, offset+length)` of `handle` copied into a new owning handle
  /// (`clamp_row_range` for the edges). The input handle is CONSUMED, as every
  /// operation on a resident table is.
  uint64_t slice_handle(uint64_t handle, uint64_t offset, uint64_t length);
  /// Drain every region recorded since the last call, in execution order. Empty
  /// unless the mode was `NodeTiming::Events`.
  ///
  /// Separate from `execute_node` because the device half of the answer does not exist
  /// when a node returns, and from session destruction because that destroys the events.
  /// Call it after the root export. Collected regions are released, so a second call
  /// does not double-report.
  ///
  /// Throws on any CUDA error, having destroyed every event first: a region reported
  /// with a zero it did not measure is worse than a run that fails.
  std::vector<NodeRegion> collect_node_regions();

  /// How many regions are waiting, without draining any.
  size_t recorded_regions() const;

  /// Run `body` — the FFI's IPC export over a borrowed table — inside a region of the
  /// node that produced `handle`, so the export is a call like every other.
  ///
  /// A callback rather than the export itself: the arrow/IPC half lives in the FFI
  /// translation unit and the region machinery lives here, and this is the seam that
  /// keeps both where they are. `body` runs exactly once whatever the mode.
  void time_export(uint64_t handle, const std::function<void()>& body);

  /// Register a table the caller built and return its handle — the operator harness's
  /// upload, and nothing on the production path. Test-only by contract, not by build.
  uint64_t adopt(TableResult result);

  /// Borrow the resident table behind `handle` (for materialization at root).
  const TableResult& table_for(uint64_t handle) const;

  /// Release a resident handle (idempotent — already-consumed handles are no-ops).
  void release(uint64_t handle);

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace peacock
