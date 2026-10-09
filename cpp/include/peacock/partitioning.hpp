// Hash partitioning on the GPU, agreeing with this engine's CPU lane rule by construction.
//
// cuDF offers only STANDARD murmur3 (murmurhash3_x86_32 / hash_partition), which
// does NOT match Spark's murmur3 — different multi-column combine and null
// handling. The CPU twin calls comet's create_murmur3_hashes, so to make the GPU
// partition assignment agree we own a small Spark-murmur3 hash kernel and REUSE
// cuDF for the expensive scatter.
//
// NOT comet-identical, and deliberately so. Three key types depart: a decimal always hashes
// 16 bytes of the unscaled value (Spark hashes 8 at precision <= 18); every NaN is
// canonicalized before hashing (comet hashes raw bytes, where Spark's doubleToLongBits
// collapses them, so this restores Spark); and an unsigned key, which Spark has no type for
// and comet no arm for, casts or bit-casts to the next wider signed type. Each rule is
// implemented once per engine and the conformance gate is what holds the two copies together.
// llm-wiki/architecture.md, "Rehash and the comet hash", carries the reasons.
//
// The API mirrors cudf::hash_partition exactly (table_view in, (table, offsets)
// out) but in the peacock:: namespace — a drop-in at the call site.
#pragma once

#include <cudf/column/column.hpp>
#include <cudf/table/table.hpp>
#include <cudf/table/table_view.hpp>
#include <cudf/types.hpp>
#include <cudf/utilities/default_stream.hpp>
#include <cudf/utilities/memory_resource.hpp>

#include <rmm/cuda_stream_view.hpp>
#include <rmm/resource_ref.hpp>

#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

namespace peacock::partitioning {

/// Per-row Spark-compatible partition id = pmod(spark_murmur3(key_cols, seed),
/// num_partitions), as an INT32 column of length `input.num_rows()`. This is the
/// single source of truth the conformance test asserts against the production CPU lane rule
/// (seed=42, per-column left-to-right running-seed, Spark null-skip, UTF-8 bytes). Against the
/// rule and not against a second copy of it: that was #201.
/// Unsupported key column types assert rather than hash a wrong encoding.
std::unique_ptr<cudf::column> spark_partition_ids(
    cudf::table_view const& input,
    std::vector<cudf::size_type> const& key_cols,
    cudf::size_type num_partitions,
    uint32_t seed                     = 42,
    rmm::cuda_stream_view stream      = cudf::get_default_stream(),
    rmm::device_async_resource_ref mr = cudf::get_current_device_resource_ref());

/// Drop-in for cudf::hash_partition, but Spark-murmur3: scatter `input` into
/// `num_partitions` by `spark_partition_ids`, via cudf::partition (the optimized,
/// cuDF-owned scatter — we own only the cheap hash kernel).
std::pair<std::unique_ptr<cudf::table>, std::vector<cudf::size_type>> spark_hash_partition(
    cudf::table_view const& input,
    std::vector<cudf::size_type> const& key_cols,
    cudf::size_type num_partitions,
    uint32_t seed                     = 42,
    rmm::cuda_stream_view stream      = cudf::get_default_stream(),
    rmm::device_async_resource_ref mr = cudf::get_current_device_resource_ref());

}  // namespace peacock::partitioning
