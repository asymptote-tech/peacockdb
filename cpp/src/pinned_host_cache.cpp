// The pinned host file cache (peacock/pinned_host_cache.h). One mutex guards the map, the LRU
// list and the byte counts; a fill reads its file outside it, having reserved its bytes first.

#include "peacock/pinned_host_cache.h"

#include <cuda_runtime_api.h>

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <charconv>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iterator>
#include <sstream>
#include <utility>

namespace peacock {

bool is_discrete_for_pinning(int pageable_memory_access) { return pageable_memory_access == 0; }

std::size_t default_pinned_host_cache_bytes(std::size_t host_ram_bytes,
                                            std::optional<std::size_t> cgroup_limit_bytes) {
  const std::size_t m =
      cgroup_limit_bytes ? std::min(host_ram_bytes, *cgroup_limit_bytes) : host_ram_bytes;
  constexpr std::size_t n = kPinnedHostCacheRamNumerator, d = kPinnedHostCacheRamDenominator;
  return m / d * n + m % d * n / d;
}

std::optional<std::size_t> parse_cgroup_memory_limit(std::string_view text) {
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.back())))
    text.remove_suffix(1);
  std::size_t bytes = 0;
  const auto [end, ec] = std::from_chars(text.data(), text.data() + text.size(), bytes);
  if (text.empty() || ec != std::errc{} || end != text.data() + text.size()) return std::nullopt;
  return bytes;
}

std::optional<std::size_t> cgroup_memory_limit_bytes() {
  for (const char* file :
       {"/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes"}) {
    std::ifstream in(file);
    if (!in) continue;
    std::stringstream text;
    text << in.rdbuf();
    return parse_cgroup_memory_limit(text.str());
  }
  return std::nullopt;
}

namespace {

void* pinned_allocate(std::size_t bytes) {
  void* p = nullptr;
  if (cudaHostAlloc(&p, bytes, cudaHostAllocDefault) != cudaSuccess) {
    cudaGetLastError();
    return nullptr;
  }
  return p;
}

void pinned_deallocate(void* p, std::size_t) { cudaFreeHost(p); }

std::string format_bytes(std::size_t bytes) {
  const char* units[] = {"B", "KiB", "MiB", "GiB", "TiB"};
  double value = static_cast<double>(bytes);
  std::size_t unit = 0;
  while (value >= 1024 && unit + 1 < std::size(units)) {
    value /= 1024;
    ++unit;
  }
  char text[32];
  std::snprintf(text, sizeof text, "%.1f %s", value, units[unit]);
  return text;
}

std::string refusal_reason(int error) {
  if (error == ENOMEM) return "pinned allocation refused";
  return std::string("read failed: ") + std::strerror(error);
}

std::uint64_t micros_since(std::chrono::steady_clock::time_point start) {
  return static_cast<std::uint64_t>(std::chrono::duration_cast<std::chrono::microseconds>(
                                        std::chrono::steady_clock::now() - start)
                                        .count());
}

}  // namespace

HostAllocator cuda_pinned_allocator() { return {pinned_allocate, pinned_deallocate}; }

FileReader posix_file_reader() { return {::pread}; }

bool read_whole_file(int fd, std::byte* dst, std::size_t size, FileReader reader) {
  std::size_t done = 0;
  while (done < size) {
    const ssize_t n = reader.pread(fd, dst + done, size - done, static_cast<off_t>(done));
    if (n < 0 && errno == EINTR) continue;
    if (n < 0) return false;
    if (n == 0) {
      errno = ESTALE;
      return false;
    }
    done += static_cast<std::size_t>(n);
  }
  return true;
}

DeviceKind classify_device(bool query_ok, int device_count, int pageable_memory_access) {
  if (!query_ok || device_count <= 0) return DeviceKind::NoDevice;
  return is_discrete_for_pinning(pageable_memory_access) ? DeviceKind::Discrete
                                                         : DeviceKind::Integrated;
}

struct PinnedHostCache::FileKey {
  std::size_t size = 0;
  timespec mtime{};
  bool operator==(FileKey const& o) const {
    return size == o.size && mtime.tv_sec == o.mtime.tv_sec && mtime.tv_nsec == o.mtime.tv_nsec;
  }
};

struct PinnedHostCache::Named {
  std::string path;
  FileKey key;
  std::size_t first_index = 0;  // into the caller's paths
};

// `counted`: its size is in resident_ (from the reservation until the buffer is freed).
// `stale`: replaced by a reload while leased, so its size is also in stale_.
struct PinnedHostCache::Entry {
  enum class State { Filling, Ready, Failed };
  std::string path;
  FileKey key;
  std::byte* data = nullptr;
  State state = State::Filling;
  int error = 0;
  std::size_t leases = 0;
  bool current = true;
  bool counted = true;
  bool stale = false;
  std::list<Entry*>::iterator lru;
};

PinnedHostCache::PinnedHostCache(std::size_t capacity_bytes, HostAllocator allocator,
                                 FileReader reader)
    : allocator_(allocator), reader_(reader), capacity_(capacity_bytes) {}

PinnedHostCache::~PinnedHostCache() {
  for (auto& [path, entry] : entries_)
    if (entry->data) allocator_.deallocate(entry->data, entry->key.size);
}

PinnedHostCache::Lease::Lease(Lease&& other) noexcept
    : cache_(std::exchange(other.cache_, nullptr)),
      entries_(std::move(other.entries_)),
      spans_(std::move(other.spans_)) {}

PinnedHostCache::Lease& PinnedHostCache::Lease::operator=(Lease&& other) noexcept {
  if (this != &other) {
    release();
    cache_ = std::exchange(other.cache_, nullptr);
    entries_ = std::move(other.entries_);
    spans_ = std::move(other.spans_);
  }
  return *this;
}

PinnedHostCache::Lease::~Lease() { release(); }

void PinnedHostCache::Lease::release() {
  if (!cache_) return;
  std::lock_guard lock(cache_->mutex_);
  cache_->release_locked(entries_);
  cache_ = nullptr;
  entries_.clear();
  spans_.clear();
}

// A path that cannot be stat'ed is left to cuDF's own read, which names it.
std::optional<std::vector<PinnedHostCache::Named>> PinnedHostCache::stat_distinct(
    std::vector<std::string> const& paths, PrefillOutcome* failure) const {
  std::vector<Named> named;
  for (std::size_t i = 0; i < paths.size(); ++i) {
    if (std::any_of(named.begin(), named.end(), [&](Named const& n) { return n.path == paths[i]; }))
      continue;
    struct stat st {};
    if (::stat(paths[i].c_str(), &st) != 0) {
      if (failure) {
        failure->status = PrefillOutcome::Status::Io;
        failure->failed_index = i;
        failure->error = errno;
      }
      return std::nullopt;
    }
    named.push_back({paths[i], {static_cast<std::size_t>(st.st_size), st.st_mtim}, i});
  }
  return named;
}

// Bytes a reservation cannot reclaim: leased entries that are not the named files' current
// contents, and stale copies still leased.
std::size_t PinnedHostCache::limit_locked(std::vector<Named> const& named) const {
  std::size_t leased = stale_;
  for (auto const& [path, entry] : entries_) {
    if (entry->leases == 0) continue;
    const bool is_named = std::any_of(named.begin(), named.end(), [&](Named const& n) {
      return n.path == path && n.key == entry->key;
    });
    if (!is_named) leased += entry->key.size;
  }
  return leased >= capacity_ ? 0 : capacity_ - leased;
}

// Leases every named file: the resident ones as they are, the rest as new Filling entries
// whose bytes are reserved here. The caller has checked that the named files fit the limit.
std::vector<std::shared_ptr<PinnedHostCache::Entry>> PinnedHostCache::reserve_locked(
    std::vector<Named> const& named, std::vector<std::shared_ptr<Entry>>& fills) {
  std::vector<std::shared_ptr<Entry>> held(named.size());
  std::vector<bool> reload(named.size(), false);
  std::size_t incoming = 0;
  for (std::size_t i = 0; i < named.size(); ++i) {
    auto found = entries_.find(named[i].path);
    if (found != entries_.end() && found->second->key == named[i].key) {
      held[i] = found->second;
      ++held[i]->leases;
      lru_.splice(lru_.begin(), lru_, held[i]->lru);
      continue;
    }
    if (found != entries_.end()) {
      std::shared_ptr<Entry> old = found->second;
      entries_.erase(found);
      lru_.erase(old->lru);
      old->current = false;
      if (old->leases == 0) {
        free_locked(*old);
      } else {
        old->stale = true;
        stale_ += old->key.size;
      }
      reload[i] = true;
    }
    incoming += named[i].key.size;
  }
  make_room_locked(incoming);
  for (std::size_t i = 0; i < named.size(); ++i) {
    if (held[i]) continue;
    auto entry = std::make_shared<Entry>();
    entry->path = named[i].path;
    entry->key = named[i].key;
    entry->leases = 1;
    resident_ += entry->key.size;
    lru_.push_front(entry.get());
    entry->lru = lru_.begin();
    entries_.emplace(entry->path, entry);
    if (reload[i]) ++counters_.reloads;
    held[i] = entry;
    fills.push_back(entry);
  }
  return held;
}

// Reads the file into a new buffer with the lock released; the reservation and the filler's
// lease keep the entry alive and counted meanwhile. Returns 0 or the errno.
int PinnedHostCache::fill(Entry& entry) {
  const auto start = std::chrono::steady_clock::now();
  int error = 0;
  void* p = allocator_.allocate(entry.key.size);
  if (!p) {
    error = ENOMEM;
  } else {
    entry.data = static_cast<std::byte*>(p);
    const int fd = ::open(entry.path.c_str(), O_RDONLY | O_CLOEXEC);
    if (fd < 0) {
      error = errno;
    } else {
      if (!read_whole_file(fd, entry.data, entry.key.size, reader_)) error = errno;
      ::close(fd);
    }
  }
  std::lock_guard lock(mutex_);
  finish_fill_locked(entry, error, micros_since(start));
  return error;
}

void PinnedHostCache::finish_fill_locked(Entry& entry, int error, std::uint64_t us) {
  if (error == 0) {
    entry.state = Entry::State::Ready;
    ++counters_.fills;
    counters_.fill_bytes += entry.key.size;
    counters_.fill_us += us;
  } else {
    entry.state = Entry::State::Failed;
    entry.error = error;
    if (entry.current) {
      entries_.erase(entry.path);
      lru_.erase(entry.lru);
      entry.current = false;
    }
    free_locked(entry);
  }
  filled_.notify_all();
}

void PinnedHostCache::release_locked(std::vector<std::shared_ptr<Entry>> const& entries) {
  for (auto const& entry : entries)
    if (--entry->leases == 0 && !entry->current && entry->counted) free_locked(*entry);
  make_room_locked(0);
}

// Least recently used first, and never a leased entry: a Filling one is leased by its filler.
void PinnedHostCache::make_room_locked(std::size_t incoming) {
  auto it = lru_.end();
  while (resident_ + incoming > capacity_ && it != lru_.begin()) {
    Entry* entry = *--it;
    if (entry->leases != 0) continue;
    it = lru_.erase(it);
    entry->current = false;
    free_locked(*entry);
    ++counters_.evictions;
    entries_.erase(entries_.find(entry->path));  // last: it drops the entry's last owner
  }
}

void PinnedHostCache::free_locked(Entry& entry) {
  if (entry.data) allocator_.deallocate(entry.data, entry.key.size);
  entry.data = nullptr;
  if (entry.counted) resident_ -= entry.key.size;
  if (entry.stale) stale_ -= entry.key.size;
  entry.counted = entry.stale = false;
}

void PinnedHostCache::say_fallback_locked(std::string const& path, std::size_t size,
                                          std::string const& why) {
  if (!said_.insert(path).second) return;
  std::fprintf(stderr, "[pinned-cache] %s (%s) not cached: %s; read from its path\n", path.c_str(),
               format_bytes(size).c_str(), why.c_str());
}

PinnedHostCache::Lease PinnedHostCache::acquire(std::vector<std::string> const& paths) {
  const auto named = stat_distinct(paths, nullptr);
  if (!named) return Lease{};
  std::unique_lock lock(mutex_);
  if (capacity_ == 0) return Lease{};
  const std::size_t limit = limit_locked(*named);
  std::size_t needed = 0;
  for (Named const& n : *named) {
    needed += n.key.size;
    if (needed > limit) {
      ++counters_.fallbacks;
      say_fallback_locked(n.path, n.key.size, "does not fit in " + format_bytes(capacity_));
      return Lease{};
    }
  }
  std::vector<std::shared_ptr<Entry>> fills;
  auto held = reserve_locked(*named, fills);
  lock.unlock();
  for (auto const& entry : fills) fill(*entry);
  lock.lock();
  filled_.wait(lock, [&] {
    return std::none_of(held.begin(), held.end(),
                        [](auto const& e) { return e->state == Entry::State::Filling; });
  });
  for (auto const& entry : held) {
    if (entry->state != Entry::State::Failed) continue;
    ++counters_.fallbacks;
    say_fallback_locked(entry->path, entry->key.size, refusal_reason(entry->error));
    release_locked(held);
    return Lease{};
  }
  Lease lease;
  lease.cache_ = this;
  for (std::string const& path : paths) {
    auto entry =
        std::find_if(held.begin(), held.end(), [&](auto const& e) { return e->path == path; });
    lease.spans_.emplace_back((*entry)->data, (*entry)->key.size);
  }
  lease.entries_ = std::move(held);
  return lease;
}

PrefillOutcome PinnedHostCache::prefill(std::vector<std::string> const& paths) {
  const auto start = std::chrono::steady_clock::now();
  PrefillOutcome out;
  const auto named = stat_distinct(paths, &out);
  if (!named) return out;
  for (Named const& n : *named) out.needed += n.key.size;
  std::unique_lock lock(mutex_);
  if (capacity_ == 0) {
    out.status = PrefillOutcome::Status::Off;
    return out;
  }
  out.limit = limit_locked(*named);
  if (out.needed > out.limit) {
    out.status = PrefillOutcome::Status::DoesNotFit;
    return out;
  }
  std::vector<std::shared_ptr<Entry>> fills;
  auto held = reserve_locked(*named, fills);
  lock.unlock();
  auto index_of = [&](Entry const& entry) {
    return std::find_if(named->begin(), named->end(),
                        [&](Named const& n) { return n.path == entry.path; })
        ->first_index;
  };
  for (std::size_t i = 0; i < fills.size(); ++i) {
    if (const int error = fill(*fills[i])) {
      out.status = PrefillOutcome::Status::Io;
      out.error = error;
      out.failed_index = index_of(*fills[i]);
      std::lock_guard cancel(mutex_);
      for (std::size_t j = i + 1; j < fills.size(); ++j)
        finish_fill_locked(*fills[j], ECANCELED, 0);
      break;
    }
    ++out.files;
    out.bytes += fills[i]->key.size;
  }
  lock.lock();
  filled_.wait(lock, [&] {
    return std::none_of(held.begin(), held.end(),
                        [](auto const& e) { return e->state == Entry::State::Filling; });
  });
  for (auto const& entry : held) {
    if (out.status != PrefillOutcome::Status::Ok || entry->state != Entry::State::Failed) continue;
    out.status = PrefillOutcome::Status::Io;
    out.error = entry->error;
    out.failed_index = index_of(*entry);
  }
  release_locked(held);
  out.us = micros_since(start);
  return out;
}

void PinnedHostCache::set_capacity(std::size_t bytes) {
  std::lock_guard lock(mutex_);
  capacity_ = bytes;
  make_room_locked(0);
}

PinnedCacheStats PinnedHostCache::stats() const {
  std::lock_guard lock(mutex_);
  PinnedCacheStats out = counters_;
  out.capacity_bytes = capacity_;
  out.resident_bytes = resident_;
  out.files =
      static_cast<std::size_t>(std::count_if(entries_.begin(), entries_.end(), [](auto const& kv) {
        return kv.second->state == Entry::State::Ready;
      }));
  return out;
}

}  // namespace peacock
