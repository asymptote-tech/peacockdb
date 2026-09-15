#pragma once

#include <cudf/table/table.hpp>
#include <cudf/utilities/span.hpp>

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace peacock::plan {
struct Schema;
}
namespace fb = peacock::plan;

namespace peacock {

/// Result of executing a plan node: a cuDF table plus column names.
struct TableResult {
  std::unique_ptr<cudf::table> table;
  std::vector<std::string> column_names;
};

/// Per-node actual costs returned across the FFI. On this path the byte formula still
/// lives only in Rust (no CPU/GPU drift): Rust applies the schema+row-derived `ColAccum`
/// overhead and adds `varlen_content_bytes`, the one data-dependent term only C++ can
/// measure on the resident table. `logical_bytes` below is a second implementation of
/// that formula, not consumed here — see its comment.
struct NodeStats {
  uint64_t rows = 0;
  /// Σ over var-length (string) output columns of content bytes
  /// (offsets[n]-offsets[0]); additive across columns, so one total suffices.
  uint64_t varlen_content_bytes = 0;
};

/// How per-node regions are measured. Off by default: measuring is not free.
enum class NodeTiming : int {
  Off = 0,
  /// CUDA events around the device work, host clock around the host work, no sync
  /// inside the region. Device times are not known at region close and are read
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
/// questions -- ranges say where a node's work is on a timeline, the modes say how
/// long it took -- and a profiled run wants the first WITHOUT the second: recording
/// an event pair is device work, and a capture would show it inside the node.
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

/// Mark where the current timed region begins touching the device — after the decode and
/// the registry lookups, immediately before issuing device work.
///
/// First call in a region wins: put it at every point that could be first and let
/// idempotence sort out which one was.
///
/// Placement is the point of the split. `cudaEventRecord` on an idle stream timestamps
/// when the stream REACHES it, so a mark at the top of a node bills the host prologue as
/// device work — exactly what `host_setup_us` isolates. No-op when timing is off or no
/// region is open, so operators can call it unconditionally.
void mark_device_start();

/// One collected region: which node output partition it belongs to, and what the
/// device spent on it.
/// One timed region: which call it was, and everything measured about it.
///
/// Separate from [`NodeStats`] because the two have different consumers. The driver reads
/// stats on every call and needs two numbers; nothing on the execution path reads any of
/// these. Carrying them in the returned struct made a shipping query pay for them on every
/// output partition of every call.
struct NodeRegion {
  uint64_t seq = 0;
  uint64_t partition = 0;
  /// Calls already made against this seq when this one began; 0 for the first. Per CALL,
  /// so the partitions of one call share it.
  uint64_t call_index = 0;
  uint64_t host_setup_us = 0;
  uint64_t host_submit_us = 0;
  /// Microseconds between the region's start and stop events. Present only for regions
  /// that recorded BOTH — see `NodeSession::collect_node_regions`.
  uint64_t device_us = 0;
  /// Rows this call answered with, for this output partition.
  ///
  /// The driver gets the same figure in `NodeStats` and keeps it; this copy is for the
  /// calibration record, whose row is one CALL. A node driving several calls hands its
  /// caller only the last one's, so the middle calls' outputs exist nowhere else.
  uint64_t rows = 0;
  /// The same total Rust derives with `logical_size_from_schema`, recomputed from cuDF
  /// types.
  ///
  /// The only figure for a call in the middle of a node's chain: it hands its raw handle
  /// on, so no batch is built from it and nothing in Rust ever priced it. The record's
  /// `out_bytes` is one row per CALL, middle calls included.
  uint64_t logical_bytes = 0;
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
/// A table's `output_bytes` under the RUST byte formula
/// (`peacockdb-core/src/memory.rs::logical_size_from_schema`), given its already measured
/// var-length content total.
///
/// A second implementation of a rule kept in one place elsewhere, so the reason is stated:
/// a call in the middle of a node's chain hands its handle on, and nothing in Rust prices
/// its output. Models that formula, not cuDF's layout — BOOL8 is a bit per row here, and
/// the bitmap is charged to every column. Unhandled types throw rather than contributing
/// zero. One ambiguity is irreducible: `fb_to_type_id` collapses the three Utf8 kinds onto
/// one STRING, so a `LargeUtf8` column is under-counted here and nothing can notice.
uint64_t logical_size_from_table(const cudf::table_view& table, uint64_t varlen_content);

/// Everything a finished output partition is worth reporting, before the split between
/// what the driver reads and what only a measurement does.
///
/// Takes the whole `TableResult` rather than a view because it needs the column NAMES:
/// `execute_project` synthesizes a `__rowcount__` column for an empty projection (cuDF
/// has no 0-column table with rows), which is absent from `output_schema` and excluded
/// from both byte fields — a device representation detail must not reach the logical byte
/// axis — while `rows` still comes from the full table.
struct CallOutcome {
  uint64_t rows = 0;
  uint64_t varlen_content_bytes = 0;
  uint64_t logical_bytes = 0;
};

CallOutcome call_outcome(const TableResult& result);

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
  /// Drain every event pair recorded since the last call, one entry per region in
  /// execution order. Empty unless the mode was `NodeTiming::Events`.
  ///
  /// Separate from `execute_node` because the answer does not exist when a node returns,
  /// and from session destruction because that destroys the events. Call it after the root
  /// `materialize`. Collected regions are released, so a second call does not
  /// double-report.
  ///
  /// Incomplete pairs are dropped, not zeroed: a node that threw leaves a start and no
  /// stop, and `cudaEventElapsedTime` on such a pair fails and takes the collection down.
  std::vector<NodeRegion> collect_node_regions();

  /// Borrow the resident table behind `handle` (for materialization at root).
  const TableResult& table_for(uint64_t handle) const;

  /// Release a resident handle (idempotent — already-consumed handles are no-ops).
  void release(uint64_t handle);

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace peacock
