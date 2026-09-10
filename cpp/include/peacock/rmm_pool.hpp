// Pooled device allocator: one function, installing a pool of the size its caller asks for.
// rmm's default is a cudaMalloc/cudaFree per allocation, so a query that materializes dozens
// of multi-GiB intermediates pays a driver round trip each — TPC-H q1 over the whole table
// measured 76.5 s that way on GB10. It lives under cpp/include/ because a Rust caller cannot
// include a C++ header and must get the same allocator the gtest binaries have
// (llm-wiki/archive/archived-tickets.md #151). Callers pass the bytes they measured rather
// than a share of the device, so two binaries fit on one card (llm-wiki/tickets.md #178).
// Nothing under cpp/src/ installs it for the engine, so a shipping query still allocates the
// expensive way and gpu_memory_limit is still stored and ignored — that is #148, a decision
// about the product rather than about measurement.
#pragma once

// RMM flattened rmm/mr/device/*.hpp into rmm/mr/*.hpp after 25.02, and peacock_tpch_tests
// builds on both CI legs, so both spellings are accepted. Same treatment as the cudf join
// header in test_tpch.cpp.
#if __has_include(<rmm/mr/cuda_memory_resource.hpp>)
#  include <rmm/mr/cuda_memory_resource.hpp>
#  include <rmm/mr/per_device_resource.hpp>
#  include <rmm/mr/pool_memory_resource.hpp>
#  include <rmm/mr/statistics_resource_adaptor.hpp>
#else
#  include <rmm/mr/device/cuda_memory_resource.hpp>
#  include <rmm/mr/device/per_device_resource.hpp>
#  include <rmm/mr/device/pool_memory_resource.hpp>
#  include <rmm/mr/device/statistics_resource_adaptor.hpp>
#endif
#include <rmm/aligned.hpp>

#include <cuda_runtime.h>

#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <memory>

namespace peacock {

// Pool sizes must be aligned to rmm's allocation granularity.
inline std::size_t pool_align_down(std::size_t n) {
  return n - (n % rmm::CUDA_ALLOCATION_ALIGNMENT);
}

// The budget a binary actually takes: the constant it declared, or PEACOCK_RMM_POOL_BYTES
// when that is set. Explicit bytes, never a percentage — a share of the device is what #178
// was. It is here for the binaries whose peak scales with a knob of their own, which would
// otherwise have to be rebuilt to be swept, and for a host that is not the H200 every budget
// in this tree was measured on. A value under rmm's granularity is no pool and says so.
inline std::size_t pool_budget_bytes(std::size_t declared) {
  const char* env = std::getenv("PEACOCK_RMM_POOL_BYTES");
  if (!env || !*env) return declared;
  return static_cast<std::size_t>(std::strtoull(env, nullptr, 10));
}

// Percentage sizing for multi_gpu.cpp alone, which installs a pool per worker thread on the
// device that worker owns: manual, two GPUs, never in CI, and so no part of #178's collision.
// It takes these unconditionally, since multi-GPU means discrete parts — VRAM is the GPU's
// own there, so reserving most of it up front costs the host nothing and buys a query whose
// working set fits without a mid-query growth event. Every other caller passes bytes.
inline constexpr int kDiscreteInitialPercent = 85;
inline constexpr int kDiscreteMaximumPercent = 95;

// What install_rmm_pool() actually did — not what it was asked to do.
//
// Unavailable is a host problem, not a configuration: nothing asks for it. The two pool sizes
// are 0 when it happens, but free_bytes is still filled — the request against what the device
// had left is the whole diagnosis. Nothing acts on the state either; see install_rmm_pool,
// where an unpooled sf40 run loses tests rather than running slowly.
struct RmmPoolStatus {
  enum class State {
    Installed,    // a pool is the current device resource
    Unavailable,  // the pool could not be built; the default resource is still in place
  };
  State state = State::Unavailable;
  bool integrated = false;  // reported for the log line; nothing here sizes by device kind
  std::size_t free_bytes = 0;
  std::size_t initial_bytes = 0;
  std::size_t maximum_bytes = 0;
};

using StatsMr =
    rmm::mr::statistics_resource_adaptor<rmm::mr::pool_memory_resource<rmm::mr::cuda_memory_resource>>;

// Function-local static rather than a namespace-scope one: this is a header, and rmm keeps
// a non-owning pointer to whatever is installed, so the object must outlive every test.
inline std::unique_ptr<StatsMr>& stats_mr() {
  static std::unique_ptr<StatsMr> mr;
  return mr;
}

// Installs a pool of `bytes` — or of PEACOCK_RMM_POOL_BYTES, see pool_budget_bytes — for the
// current device, and returns what happened. Call before any cuDF work; the resources are
// function-local statics because rmm keeps a non-owning pointer to the current one.
//
// The request is never clamped: a host that cannot meet it keeps the default resource and reports
// Unavailable, because a pool smaller than the one asked for silently changes what every number
// taken over it means.
//
// Idempotent, and load-bearing now that the FFI reaches it: a second call returns the first one's
// outcome whatever it asks for, rather than dropping a resource live allocations point into.
inline const RmmPoolStatus& install_rmm_pool(std::size_t bytes) {
  static RmmPoolStatus status;
  static bool done = false;
  if (done) return status;

  // Aligned down first, because that is the size the pool would take: a request under rmm's
  // granularity leaves nothing, and a pool of nothing is built happily and then fails every
  // allocation with "Maximum pool size exceeded". Rejected before the idempotence latch, so a
  // caller that asked for no pool has not spent the one installation this process gets.
  const std::size_t requested = pool_budget_bytes(bytes);
  const std::size_t size = pool_align_down(requested);
  if (size == 0) {
    std::fprintf(stderr,
                 "[rmm] pool of %zu bytes could not be built (under rmm's %zu-byte allocation "
                 "granularity); leaving the default resource in place\n",
                 requested, static_cast<std::size_t>(rmm::CUDA_ALLOCATION_ALIGNMENT));
    return status;  // Unavailable
  }
  done = true;

  int device = 0;
  cudaGetDevice(&device);
  cudaDeviceProp prop{};
  cudaGetDeviceProperties(&prop, device);
  status.integrated = prop.integrated != 0;

  // Free memory is reported, never used for sizing: it is what turns a failed reservation
  // into a diagnosis — the request against what the device actually had left.
  std::size_t free_bytes = 0, total = 0;
  if (cudaMemGetInfo(&free_bytes, &total) == cudaSuccess) status.free_bytes = free_bytes;

  // Initial == maximum: the caller asked for the working set it measured, so reserve it up
  // front and leave no growth event to pay for mid-query.
  static auto upstream = std::make_unique<rmm::mr::cuda_memory_resource>();
  static std::unique_ptr<rmm::mr::pool_memory_resource<rmm::mr::cuda_memory_resource>> pool;
  // A request the device cannot meet fails here, a neighbour holding most of the card being the
  // ordinary cause. Reported, not aborted — but nothing acts on Unavailable, and an unpooled sf40
  // run does not run slowly, it loses tests to cudaErrorMemoryAllocation. The "could not be
  // built" line below is the diagnosis; without it that looks like a bug in the engine.
  try {
    pool = std::make_unique<rmm::mr::pool_memory_resource<rmm::mr::cuda_memory_resource>>(
        upstream.get(), size, size);
    // The statistics adaptor sits ABOVE the pool, so it counts what the query asked for
    // rather than what the pool reserved. That distinction is the whole reason it is here:
    // once a pool owns the memory, cudaMemGetInfo stops moving during a query, so the
    // fixture's free-memory delta reads a peak of nearly zero — a number that looks like a
    // triumph and measures nothing. See peak_allocated_bytes().
    stats_mr() = std::make_unique<StatsMr>(pool.get());
  } catch (const std::exception& e) {
    std::fprintf(stderr,
                 "[rmm] pool of %.1f GiB could not be built with %.1f GiB free (%s); "
                 "leaving the default resource in place\n",
                 size / 1073741824.0, free_bytes / 1073741824.0, e.what());
    pool.reset();
    return status;  // Unavailable
  }
  rmm::mr::set_current_device_resource(stats_mr().get());

  status.state = RmmPoolStatus::State::Installed;
  status.initial_bytes = size;
  status.maximum_bytes = size;
  std::fprintf(stderr, "[rmm] pool on %s: %.1f GiB reserved of %.1f GiB free\n",
               prop.integrated ? "an integrated device" : "a discrete device", size / 1073741824.0,
               free_bytes / 1073741824.0);
  return status;
}

// Peak bytes handed out since the pool was installed, or 0 when there is no pool (in which
// case the caller's cudaMemGetInfo delta is still the right measurement).
inline std::size_t peak_allocated_bytes() {
  if (!stats_mr()) return 0;
  return static_cast<std::size_t>(stats_mr()->get_bytes_counter().peak);
}

// Per-test scoping. The adaptor cascades nested counters, so pushing at the start of a test
// and popping at its end makes the peak that test's high-water mark rather than the whole
// run's — without it the second test in a binary inherits the first one's peak and every
// number after the first is wrong in the flattering direction.
inline void begin_peak_scope() {
  if (stats_mr()) stats_mr()->push_counters();
}
inline void end_peak_scope() {
  if (stats_mr()) stats_mr()->pop_counters();
}

}  // namespace peacock
