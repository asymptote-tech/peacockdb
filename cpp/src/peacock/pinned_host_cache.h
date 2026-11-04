#pragma once
// PRIVATE header -- must stay under src/, NOT include/ (see plan_types.h).
//
// Whole parquet files kept in pinned host memory, so a scan's H2D copy is a DMA from pinned
// pages with no host copy before it (llm-wiki/tasks/pinned-loads.md, item 0a). An entry is
// freed only when no lease holds it: eviction runs between reads, never under one.

#include <sys/types.h>

#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <list>
#include <memory>
#include <mutex>
#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <unordered_map>
#include <unordered_set>
#include <vector>

namespace peacock {

// The default capacity is min(host RAM, the cgroup limit) × 3/4 (the human's decision,
// 2026-11-03): pinned pages cannot be swapped, so the rest is left to the OS and cuDF.
inline constexpr std::size_t kPinnedHostCacheRamNumerator = 3, kPinnedHostCacheRamDenominator = 4;

// The one discrete/integrated predicate, for the cache and cuDF's pinned pool. It reads
// cudaDeviceProp::pageableMemoryAccess, never `integrated`: a device that reads pageable
// memory directly gains nothing from pinning it.
bool is_discrete_for_pinning(int pageable_memory_access);

std::size_t default_pinned_host_cache_bytes(std::size_t host_ram_bytes,
                                            std::optional<std::size_t> cgroup_limit_bytes);

// A cgroup memory file's text: "max" (v2, no limit) → nullopt; a decimal → its bytes.
std::optional<std::size_t> parse_cgroup_memory_limit(std::string_view text);

// /sys/fs/cgroup/memory.max (v2), else /sys/fs/cgroup/memory/memory.limit_in_bytes (v1);
// unreadable → nullopt. Only the root files: inside a cgroup namespace (a docker run) that is
// the container's own limit. A v1 "unlimited" is a value above host RAM, which min() drops.
std::optional<std::size_t> cgroup_memory_limit_bytes();

// nullptr from allocate is a refusal, never a throw.
struct HostAllocator {
  void* (*allocate)(std::size_t bytes);
  void (*deallocate)(void* p, std::size_t bytes);
};

// cudaHostAlloc / cudaFreeHost. A refusal clears the runtime's last error, or cuDF's next
// cudaPeekAtLastError would throw it from an unrelated call.
HostAllocator cuda_pinned_allocator();

struct FileReader {
  ssize_t (*pread)(int fd, void* buf, std::size_t count, off_t offset);
};
FileReader posix_file_reader();

// Reads exactly `size` bytes from offset 0: Linux returns at most 0x7ffff000 bytes per call,
// and sf40's lineitem is about 9 GB. EINTR is retried. EOF before `size` → false with errno
// ESTALE (the file shrank under the read); a -1 → false with its errno.
bool read_whole_file(int fd, std::byte* dst, std::size_t size, FileReader reader);

enum class DeviceKind { NoDevice, Discrete, Integrated };
// !query_ok or device_count <= 0 → NoDevice; else is_discrete_for_pinning decides.
DeviceKind classify_device(bool query_ok, int device_count, int pageable_memory_access);

struct PinnedCacheStats {
  std::size_t capacity_bytes = 0, resident_bytes = 0, files = 0;
  std::size_t fills = 0, fill_bytes = 0, fallbacks = 0, evictions = 0, reloads = 0;
  std::uint64_t fill_us = 0;  // host IO into pinned memory, allocation included
};

struct PrefillOutcome {
  enum class Status { Ok, DoesNotFit, Io, Off };
  Status status = Status::Ok;
  std::uint64_t files = 0, bytes = 0, us = 0;  // read from disk by this call; wall µs
  std::uint64_t needed = 0;                    // Σ size of the distinct named paths
  std::uint64_t limit = 0;                     // capacity − bytes leased outside the named set
  std::size_t failed_index = 0;                // Io: the index into paths
  int error = 0;                               // Io: errno (ENOMEM when the allocator refused)
};

// Entries are keyed by path and validated by (size, mtime) on every acquire and prefill; a
// changed file is reloaded and the stale copy is freed when its last lease ends.
// resident_bytes counts every buffer still allocated, stale and in-flight ones included, and
// never passes the capacity: a fill reserves its bytes under the lock before it reads.
class PinnedHostCache {
  struct Entry;
  struct FileKey;

 public:
  PinnedHostCache(std::size_t capacity_bytes, HostAllocator allocator,
                  FileReader reader = posix_file_reader());
  ~PinnedHostCache();
  PinnedHostCache(PinnedHostCache const&) = delete;
  PinnedHostCache& operator=(PinnedHostCache const&) = delete;

  // All or nothing: either every path is resident for the lease's lifetime, or none is held
  // and the caller reads the paths, so a scan never mixes sources.
  class Lease {
   public:
    Lease(Lease&& other) noexcept;
    Lease& operator=(Lease&& other) noexcept;
    ~Lease();
    bool cached() const { return cache_ != nullptr; }
    std::vector<std::span<const std::byte>> const& spans() const { return spans_; }

   private:
    friend class PinnedHostCache;
    Lease() = default;
    void release();
    PinnedHostCache* cache_ = nullptr;
    std::vector<std::shared_ptr<Entry>> entries_;
    std::vector<std::span<const std::byte>> spans_;
  };

  Lease acquire(std::vector<std::string> const& paths);
  // Between calls only: it may evict any entry outside `paths` that no lease holds.
  PrefillOutcome prefill(std::vector<std::string> const& paths);
  // Shrinking evicts unleased entries now, least recently used first.
  void set_capacity(std::size_t bytes);
  PinnedCacheStats stats() const;

 private:
  struct Named;
  std::optional<std::vector<Named>> stat_distinct(std::vector<std::string> const& paths,
                                                  PrefillOutcome* failure) const;
  std::size_t limit_locked(std::vector<Named> const& named) const;
  std::vector<std::shared_ptr<Entry>> reserve_locked(std::vector<Named> const& named,
                                                     std::vector<std::shared_ptr<Entry>>& fills);
  int fill(Entry& entry);
  void finish_fill_locked(Entry& entry, int error, std::uint64_t us);
  void release_locked(std::vector<std::shared_ptr<Entry>> const& entries);
  void make_room_locked(std::size_t incoming);
  void free_locked(Entry& entry);
  void say_fallback_locked(std::string const& path, std::size_t size, std::string const& why);

  HostAllocator allocator_;
  FileReader reader_;
  mutable std::mutex mutex_;
  std::condition_variable filled_;
  std::size_t capacity_;
  std::size_t resident_ = 0;
  std::size_t stale_ = 0;  // replaced while leased: allocated, but no longer any file's contents
  std::unordered_map<std::string, std::shared_ptr<Entry>> entries_;
  std::list<Entry*> lru_;  // front = most recently used; current entries only
  std::unordered_set<std::string> said_;
  PinnedCacheStats counters_;
};

// The process's cache. The C ABI's PEACOCK_PINNED_CACHE_* values mirror these two in order.
enum class PinnedCacheState { On, Off, Integrated, NoDevice };
enum class PinnedCacheSource { Default, Session, Env };

// What sizes it: the device first, then PEACOCK_PINNED_HOST_CACHE_BYTES, then the session's
// pinned_host_cache_bytes (the last set in the process), then the default from host RAM.
struct PinnedCacheConfig {
  DeviceKind device = DeviceKind::NoDevice;
  std::optional<std::size_t> env_bytes, session_bytes;
  std::size_t host_ram_bytes = 0;
  std::optional<std::size_t> cgroup_limit_bytes;
};

struct PinnedCacheSizing {
  PinnedCacheState state = PinnedCacheState::Off;
  PinnedCacheSource source = PinnedCacheSource::Default;
  std::size_t capacity_bytes = 0;
};

PinnedCacheSizing size_pinned_cache(PinnedCacheConfig const& config);
// The line said at the cache's first use, after "[pinned-cache] ".
std::string describe_pinned_cache(PinnedCacheConfig const& config, PinnedCacheSizing const& sized);
// PEACOCK_PINNED_HOST_CACHE_BYTES's text: decimal bytes and nothing else, or a throw naming it.
std::size_t parse_pinned_host_cache_bytes(std::string_view text);

// Every executor's: the cache is per process, not per device. Built at first use from the
// current device and the environment, and leaked on purpose, so no static destructor unpins a
// buffer a thread still reads at exit. Throws while the environment variable is not a count.
PinnedHostCache& pinned_host_cache();

struct PinnedCacheStatus {
  PinnedCacheSizing sizing;
  PinnedCacheStats stats;
};
PinnedCacheStatus pinned_host_cache_status();
// nullopt asks for the default. The last set wins; shrinking evicts unleased files now.
PinnedCacheStatus set_pinned_host_cache_bytes(std::optional<std::size_t> session_bytes);

}  // namespace peacock
