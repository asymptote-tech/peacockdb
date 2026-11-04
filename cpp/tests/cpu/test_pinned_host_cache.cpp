// The pinned host file cache with no device: a counting malloc allocator stands in for
// cudaHostAlloc, so the LRU, lease, reload, fallback and prefill rules run on every CPU tier.
// The allocator poisons a buffer before freeing it, so a read of a freed entry fails a memcmp.

#include "peacock/pinned_host_cache.h"

#include <gtest/gtest.h>

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <atomic>
#include <cerrno>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <future>
#include <mutex>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

namespace {

using Cache = peacock::PinnedHostCache;
using peacock::PrefillOutcome;
using Status = PrefillOutcome::Status;

constexpr std::size_t KiB = 1024;
constexpr std::size_t kRoomy = 1024 * KiB;
constexpr auto kHang = std::chrono::seconds(10);

// The allocator's state. HostAllocator is two plain function pointers, so it lives here.
// A freed block is poisoned and kept until the test ends: malloc would hand the same address
// to the next fill, and a pointer could then not say which buffer it was.
struct Heap {
  std::mutex m;
  std::condition_variable cv;
  bool gated = false;           // allocate blocks until open()
  bool refuse = false;          // every call returns nullptr
  std::size_t refuse_call = 0;  // this 1-based call returns nullptr; 0 = none
  std::size_t calls = 0, blocked = 0, outstanding = 0, peak = 0, violations = 0;
  std::set<void*> live, freed, guarded;

  ~Heap() {
    for (void* p : freed) std::free(p);
  }

  void open() {
    std::lock_guard lock(m);
    gated = false;
    cv.notify_all();
  }
  bool wait_blocked(std::size_t n) {
    std::unique_lock lock(m);
    return cv.wait_for(lock, kHang, [&] { return blocked >= n; });
  }
  bool was_freed(const void* p) {
    std::lock_guard lock(m);
    return freed.count(const_cast<void*>(p)) > 0;
  }
  std::size_t call_count() {
    std::lock_guard lock(m);
    return calls;
  }
};

Heap* heap = nullptr;

void* counting_allocate(std::size_t bytes) {
  std::unique_lock lock(heap->m);
  const std::size_t call = ++heap->calls;
  heap->outstanding += bytes;
  heap->peak = std::max(heap->peak, heap->outstanding);
  ++heap->blocked;
  heap->cv.notify_all();
  heap->cv.wait(lock, [] { return !heap->gated; });
  --heap->blocked;
  if (heap->refuse || call == heap->refuse_call) {
    heap->outstanding -= bytes;
    return nullptr;
  }
  void* p = std::malloc(bytes);
  heap->live.insert(p);
  return p;
}

void counting_deallocate(void* p, std::size_t bytes) {
  std::lock_guard lock(heap->m);
  if (heap->guarded.count(p)) ++heap->violations;
  std::memset(p, 0xDD, bytes);
  heap->live.erase(p);
  heap->freed.insert(p);
  heap->outstanding -= bytes;
}

constexpr peacock::HostAllocator kCounting{counting_allocate, counting_deallocate};

std::vector<std::byte> pattern(std::size_t size, int seed) {
  std::vector<std::byte> bytes(size);
  for (std::size_t i = 0; i < size; ++i)
    bytes[i] = static_cast<std::byte>((seed * 131 + i * 7 + i / 251) & 0xFF);
  return bytes;
}

bool span_equals(std::span<const std::byte> span, std::vector<std::byte> const& bytes) {
  return span.size() == bytes.size() && std::memcmp(span.data(), bytes.data(), span.size()) == 0;
}

class PinnedHostCache : public ::testing::Test {
 protected:
  void SetUp() override {
    heap = new Heap;
    char tmpl[] = "/tmp/pinned-cache-XXXXXX";
    ASSERT_NE(mkdtemp(tmpl), nullptr);
    dir_ = tmpl;
  }
  void TearDown() override {
    EXPECT_TRUE(heap->live.empty()) << heap->live.size() << " buffers never freed";
    delete heap;
    heap = nullptr;
    std::filesystem::remove_all(dir_);
  }

  // Writes `size` bytes of `seed`'s pattern; a rewrite moves the mtime a second on, so a
  // filesystem with coarse timestamps still sees the change.
  std::string write(std::string const& name, std::size_t size, int seed = 0) {
    const std::string path = dir_ + "/" + name;
    struct stat before {};
    const bool existed = ::stat(path.c_str(), &before) == 0;
    const auto bytes = pattern(size, seed);
    std::ofstream(path, std::ios::binary | std::ios::trunc)
        .write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(size));
    if (existed) {
      timespec times[2] = {before.st_atim, before.st_mtim};
      times[1].tv_sec += 1;
      EXPECT_EQ(utimensat(AT_FDCWD, path.c_str(), times, 0), 0);
    }
    return path;
  }
  std::string path(std::string const& name) const { return dir_ + "/" + name; }

  std::string dir_;
};

const void* data_of(Cache::Lease const& lease, std::size_t i = 0) {
  return lease.spans().at(i).data();
}

TEST_F(PinnedHostCache, HitsReuseTheBuffer) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  const void* first = nullptr;
  {
    auto lease = cache.acquire({a});
    ASSERT_TRUE(lease.cached());
    first = data_of(lease);
    EXPECT_TRUE(span_equals(lease.spans()[0], pattern(KiB, 1)));
  }
  auto again = cache.acquire({a});
  ASSERT_TRUE(again.cached());
  EXPECT_EQ(data_of(again), first);
  EXPECT_EQ(cache.stats().fills, 1u);
}

TEST_F(PinnedHostCache, LeastRecentlyUsedIsEvicted) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2), c = write("c", KiB, 3),
             d = write("d", KiB, 4);
  Cache cache(3 * KiB, kCounting);
  const void* pa = data_of(cache.acquire({a}));
  const void* pb = data_of(cache.acquire({b}));
  cache.acquire({c});
  EXPECT_EQ(data_of(cache.acquire({a})), pa);
  EXPECT_TRUE(cache.acquire({d}).cached());
  EXPECT_EQ(cache.stats().evictions, 1u);
  EXPECT_TRUE(heap->was_freed(pb));
  EXPECT_EQ(data_of(cache.acquire({a})), pa);
}

TEST_F(PinnedHostCache, LeasedEntryIsNeverEvicted) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2), c = write("c", KiB, 3);
  Cache cache(2 * KiB, kCounting);
  auto held = cache.acquire({a});
  ASSERT_TRUE(held.cached());
  const void* pb = data_of(cache.acquire({b}));
  EXPECT_TRUE(cache.acquire({c}).cached());
  EXPECT_TRUE(heap->was_freed(pb));
  EXPECT_FALSE(heap->was_freed(data_of(held)));
  EXPECT_TRUE(span_equals(held.spans()[0], pattern(KiB, 1)));
}

TEST_F(PinnedHostCache, ConcurrentReadAndEviction) {
  const auto a = write("a", KiB, 1);
  const std::vector<std::string> others = {write("b", KiB, 2), write("c", KiB, 3),
                                           write("d", KiB, 4), write("e", KiB, 5)};
  const auto expected = pattern(KiB, 1);
  Cache cache(2 * KiB, kCounting);
  auto held = cache.acquire({a});
  ASSERT_TRUE(held.cached());
  {
    std::lock_guard lock(heap->m);
    heap->guarded.insert(const_cast<void*>(data_of(held)));
  }
  auto reader = std::async(std::launch::async, [&] {
    int mismatches = 0;
    for (int i = 0; i < 1000; ++i)
      mismatches += std::memcmp(held.spans()[0].data(), expected.data(), KiB) != 0;
    return mismatches;
  });
  auto churn = std::async(std::launch::async, [&] {
    for (int i = 0; i < 1000; ++i)
      for (auto const& p : others) cache.acquire({p});
  });
  ASSERT_EQ(reader.wait_for(kHang), std::future_status::ready);
  ASSERT_EQ(churn.wait_for(kHang * 6), std::future_status::ready);
  EXPECT_EQ(reader.get(), 0);
  EXPECT_EQ(heap->violations, 0u);
  EXPECT_LE(cache.stats().resident_bytes, 2 * KiB);
  heap->guarded.clear();
}

TEST_F(PinnedHostCache, SecondReaderWaitsForTheFill) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  heap->gated = true;
  auto first = std::async(std::launch::async, [&] { return cache.acquire({a}); });
  ASSERT_TRUE(heap->wait_blocked(1));
  auto second = std::async(std::launch::async, [&] { return cache.acquire({a}); });
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  heap->open();
  ASSERT_EQ(first.wait_for(kHang), std::future_status::ready);
  ASSERT_EQ(second.wait_for(kHang), std::future_status::ready);
  auto l1 = first.get();
  auto l2 = second.get();
  ASSERT_TRUE(l1.cached());
  ASSERT_TRUE(l2.cached());
  EXPECT_EQ(data_of(l1), data_of(l2));
  EXPECT_EQ(cache.stats().fills, 1u);
  EXPECT_EQ(heap->call_count(), 1u);
}

TEST_F(PinnedHostCache, AFailedFillWakesItsWaiters) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  heap->gated = true;
  heap->refuse = true;
  std::vector<std::future<bool>> readers;
  readers.push_back(std::async(std::launch::async, [&] { return cache.acquire({a}).cached(); }));
  ASSERT_TRUE(heap->wait_blocked(1));
  for (int i = 0; i < 3; ++i)
    readers.push_back(std::async(std::launch::async, [&] { return cache.acquire({a}).cached(); }));
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  heap->open();
  for (auto& r : readers) {
    ASSERT_EQ(r.wait_for(kHang), std::future_status::ready);
    EXPECT_FALSE(r.get());
  }
  EXPECT_EQ(cache.stats().fallbacks, 4u);
  EXPECT_EQ(cache.stats().resident_bytes, 0u);
}

TEST_F(PinnedHostCache, ConcurrentFillsStayUnderCapacity) {
  const std::vector<std::string> files = {write("a", KiB, 1), write("b", KiB, 2),
                                          write("c", KiB, 3)};
  Cache cache(2 * KiB, kCounting);
  heap->gated = true;
  std::vector<std::future<Cache::Lease>> fills;
  for (auto const& f : files)
    fills.push_back(std::async(std::launch::async, [&cache, f] { return cache.acquire({f}); }));
  ASSERT_TRUE(heap->wait_blocked(2));
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  heap->open();
  std::vector<Cache::Lease> leases;
  for (auto& f : fills) {
    ASSERT_EQ(f.wait_for(kHang), std::future_status::ready);
    leases.push_back(f.get());
  }
  EXPECT_LE(std::count_if(leases.begin(), leases.end(), [](auto& l) { return l.cached(); }), 2);
  EXPECT_LE(heap->peak, 2 * KiB);
  EXPECT_LE(cache.stats().resident_bytes, 2 * KiB);
}

TEST_F(PinnedHostCache, ReleaseEvictsWhileOverCapacity) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2), c = write("c", KiB, 3);
  Cache cache(2 * KiB, kCounting);
  std::optional<Cache::Lease> old_a(cache.acquire({a}));
  write("a", KiB, 9);
  auto new_a = cache.acquire({a});
  ASSERT_TRUE(new_a.cached());
  EXPECT_EQ(cache.stats().resident_bytes, 2 * KiB);
  EXPECT_FALSE(cache.acquire({b}).cached());
  old_a.reset();
  EXPECT_EQ(cache.stats().resident_bytes, KiB);

  std::optional<Cache::Lease> lease_c(cache.acquire({c}));
  ASSERT_TRUE(lease_c->cached());
  const void* pc = data_of(*lease_c);
  cache.set_capacity(KiB);
  EXPECT_FALSE(heap->was_freed(pc));
  EXPECT_EQ(cache.stats().resident_bytes, 2 * KiB);
  lease_c.reset();
  EXPECT_TRUE(heap->was_freed(pc));
  EXPECT_LE(cache.stats().resident_bytes, KiB);
}

// Linux returns at most 0x7ffff000 bytes per read, and a signal can interrupt one.
ssize_t short_pread(int fd, void* buf, std::size_t count, off_t offset) {
  static std::atomic<int> calls{0};
  if (++calls == 2) {
    errno = EINTR;
    return -1;
  }
  return ::pread(fd, buf, std::min<std::size_t>(count, 100), offset);
}

TEST_F(PinnedHostCache, ShortReadsAreLooped) {
  const auto small = write("small", KiB, 1), big = write("big", 4 * KiB, 2);
  Cache cache(kRoomy, kCounting, peacock::FileReader{short_pread});
  auto l1 = cache.acquire({small});
  auto l2 = cache.acquire({big});
  ASSERT_TRUE(l1.cached());
  ASSERT_TRUE(l2.cached());
  EXPECT_TRUE(span_equals(l1.spans()[0], pattern(KiB, 1)));
  EXPECT_TRUE(span_equals(l2.spans()[0], pattern(4 * KiB, 2)));
}

ssize_t truncated_pread(int fd, void* buf, std::size_t count, off_t offset) {
  if (offset >= 512) return 0;
  return ::pread(fd, buf, std::min<std::size_t>(count, 512 - offset), offset);
}

TEST_F(PinnedHostCache, ShortReadAtEofIsAChangedFile) {
  const auto a = write("a", KiB, 1);
  Cache cache(4 * KiB, kCounting, peacock::FileReader{truncated_pread});
  EXPECT_FALSE(cache.acquire({a}).cached());
  EXPECT_EQ(cache.stats().fallbacks, 1u);
  EXPECT_TRUE(heap->live.empty());
  const PrefillOutcome out = cache.prefill({a});
  EXPECT_EQ(out.status, Status::Io);
  EXPECT_EQ(out.error, ESTALE);
}

TEST_F(PinnedHostCache, NoDeviceIsOff) {
  using peacock::DeviceKind;
  EXPECT_EQ(peacock::classify_device(false, 0, 0), DeviceKind::NoDevice);
  EXPECT_EQ(peacock::classify_device(true, 0, 0), DeviceKind::NoDevice);
  EXPECT_EQ(peacock::classify_device(true, 1, 0), DeviceKind::Discrete);
  EXPECT_EQ(peacock::classify_device(true, 1, 1), DeviceKind::Integrated);
}

TEST_F(PinnedHostCache, ChangedFileIsReloaded) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  cache.acquire({a});
  write("a", KiB, 2);
  {
    auto lease = cache.acquire({a});
    ASSERT_TRUE(lease.cached());
    EXPECT_TRUE(span_equals(lease.spans()[0], pattern(KiB, 2)));
    EXPECT_EQ(cache.stats().reloads, 1u);
  }
  write("a", 2 * KiB, 3);
  auto lease = cache.acquire({a});
  ASSERT_TRUE(lease.cached());
  EXPECT_TRUE(span_equals(lease.spans()[0], pattern(2 * KiB, 3)));
  EXPECT_EQ(cache.stats().reloads, 2u);
}

TEST_F(PinnedHostCache, ChangedFileWhileLeasedKeepsTheOldBytes) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  {
    auto l1 = cache.acquire({a});
    write("a", 1536, 2);
    auto l2 = cache.acquire({a});
    ASSERT_TRUE(l1.cached());
    ASSERT_TRUE(l2.cached());
    EXPECT_TRUE(span_equals(l2.spans()[0], pattern(1536, 2)));
    EXPECT_TRUE(span_equals(l1.spans()[0], pattern(KiB, 1)));
  }
  EXPECT_EQ(cache.stats().resident_bytes, 1536u);
}

TEST_F(PinnedHostCache, ScanThatDoesNotFitReadsEveryPath) {
  const auto a = write("a", KiB, 1), big = write("big", 4 * KiB, 2);
  Cache cache(3 * KiB, kCounting);
  auto lease = cache.acquire({a, big});
  EXPECT_FALSE(lease.cached());
  EXPECT_TRUE(lease.spans().empty());
  EXPECT_EQ(cache.stats().fallbacks, 1u);
  EXPECT_EQ(cache.stats().resident_bytes, 0u);
  EXPECT_EQ(cache.stats().files, 0u);
}

TEST_F(PinnedHostCache, AllocationFailureFallsBack) {
  const auto a = write("a", KiB, 1);
  Cache cache(kRoomy, kCounting);
  heap->refuse = true;
  bool cached = true;
  EXPECT_NO_THROW(cached = cache.acquire({a}).cached());
  EXPECT_FALSE(cached);
  EXPECT_EQ(cache.stats().fallbacks, 1u);
}

std::size_t count_of(std::string const& text, std::string const& needle) {
  std::size_t n = 0;
  for (auto at = text.find(needle); at != std::string::npos; at = text.find(needle, at + 1)) ++n;
  return n;
}

TEST_F(PinnedHostCache, FallbackIsSaidOnce) {
  const auto big = write("big", 4 * KiB, 2);
  Cache cache(3 * KiB, kCounting);
  testing::internal::CaptureStderr();
  cache.acquire({big});
  cache.acquire({big});
  const std::string err = testing::internal::GetCapturedStderr();
  EXPECT_EQ(count_of(err, "[pinned-cache]"), 1u) << err;
  EXPECT_EQ(count_of(err, big), 1u) << err;
  EXPECT_EQ(cache.stats().fallbacks, 2u);
}

TEST_F(PinnedHostCache, MissingFileIsLeftToTheReader) {
  Cache cache(kRoomy, kCounting);
  bool cached = true;
  EXPECT_NO_THROW(cached = cache.acquire({path("missing")}).cached());
  EXPECT_FALSE(cached);
  EXPECT_EQ(heap->call_count(), 0u);
}

TEST_F(PinnedHostCache, ZeroCapacityIsOff) {
  const auto a = write("a", KiB, 1);
  Cache cache(0, kCounting);
  testing::internal::CaptureStderr();
  EXPECT_FALSE(cache.acquire({a}).cached());
  const std::string err = testing::internal::GetCapturedStderr();
  EXPECT_EQ(heap->call_count(), 0u);
  EXPECT_EQ(cache.stats().fallbacks, 0u);
  EXPECT_EQ(count_of(err, "[pinned-cache]"), 0u) << err;
}

TEST_F(PinnedHostCache, SetCapacityEvictsTheUnleased) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2);
  Cache cache(kRoomy, kCounting);
  auto held = cache.acquire({a});
  const void* pb = data_of(cache.acquire({b}));
  cache.set_capacity(KiB);
  EXPECT_TRUE(heap->was_freed(pb));
  EXPECT_FALSE(heap->was_freed(data_of(held)));
  EXPECT_EQ(cache.stats().capacity_bytes, KiB);
  EXPECT_EQ(cache.stats().resident_bytes, KiB);
}

// This is also the GB10 branch, without a GB10: the predicate is a pure function of the
// attribute the device reports.
TEST_F(PinnedHostCache, DefaultIsThreeQuartersOfTheSmallerOfRamAndCgroup) {
  EXPECT_EQ(peacock::default_pinned_host_cache_bytes(196'000'000'000, std::nullopt),
            147'000'000'000u);
  EXPECT_EQ(peacock::default_pinned_host_cache_bytes(196'000'000'000, 64'000'000'000),
            48'000'000'000u);
  EXPECT_EQ(peacock::default_pinned_host_cache_bytes(196'000'000'000, 400'000'000'000),
            147'000'000'000u);
  EXPECT_TRUE(peacock::is_discrete_for_pinning(0));
  EXPECT_FALSE(peacock::is_discrete_for_pinning(1));
}

TEST_F(PinnedHostCache, CgroupLimitIsParsed) {
  EXPECT_EQ(peacock::parse_cgroup_memory_limit("max\n"), std::nullopt);
  EXPECT_EQ(peacock::parse_cgroup_memory_limit("8589934592\n"), 8589934592u);
  EXPECT_EQ(peacock::parse_cgroup_memory_limit(""), std::nullopt);
  EXPECT_EQ(peacock::parse_cgroup_memory_limit("lots"), std::nullopt);
}

// The prefill: capacity 4 KiB unless said.

TEST_F(PinnedHostCache, PrefillMakesEveryFileResident) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2);
  Cache cache(4 * KiB, kCounting);
  const PrefillOutcome out = cache.prefill({a, b});
  EXPECT_EQ(out.status, Status::Ok);
  EXPECT_EQ(out.files, 2u);
  EXPECT_EQ(out.bytes, 2 * KiB);
  EXPECT_EQ(out.needed, 2 * KiB);
  EXPECT_EQ(out.limit, 4 * KiB);
  EXPECT_GT(out.us, 0u);
  const std::size_t fills = cache.stats().fills;
  auto lease = cache.acquire({a, b});
  ASSERT_TRUE(lease.cached());
  EXPECT_TRUE(span_equals(lease.spans()[0], pattern(KiB, 1)));
  EXPECT_TRUE(span_equals(lease.spans()[1], pattern(KiB, 2)));
  EXPECT_EQ(cache.stats().fills, fills);
}

TEST_F(PinnedHostCache, PrefillIsIdempotent) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2);
  Cache cache(4 * KiB, kCounting);
  EXPECT_EQ(cache.prefill({a, b}).status, Status::Ok);
  EXPECT_EQ(heap->call_count(), 2u);
  for (auto const& paths : {std::vector<std::string>{a, b}, std::vector<std::string>{b, a, a}}) {
    const PrefillOutcome out = cache.prefill(paths);
    EXPECT_EQ(out.status, Status::Ok);
    EXPECT_EQ(out.files, 0u);
    EXPECT_EQ(out.bytes, 0u);
    EXPECT_EQ(out.needed, 2 * KiB);
    EXPECT_EQ(heap->call_count(), 2u);
  }
}

TEST_F(PinnedHostCache, PrefillRefusesBeforePinningAnything) {
  const auto a = write("a", KiB, 1), big = write("big", 4 * KiB, 2), c = write("c", KiB, 3);
  Cache cache(3 * KiB, kCounting);
  cache.acquire({c});
  const std::size_t calls = heap->call_count();
  const PrefillOutcome out = cache.prefill({a, big});
  EXPECT_EQ(out.status, Status::DoesNotFit);
  EXPECT_EQ(out.needed, 5 * KiB);
  EXPECT_EQ(out.limit, 3 * KiB);
  EXPECT_EQ(heap->call_count(), calls);
  EXPECT_EQ(cache.stats().files, 1u);
  EXPECT_EQ(cache.stats().evictions, 0u);
  const std::size_t fills = cache.stats().fills;
  EXPECT_TRUE(cache.acquire({c}).cached());
  EXPECT_EQ(cache.stats().fills, fills);
}

TEST_F(PinnedHostCache, PrefillCountsLeasesAgainstTheLimit) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2), c = write("c", KiB, 3);
  Cache cache(2 * KiB, kCounting);
  auto la = cache.acquire({a});
  auto lb = cache.acquire({b});
  const PrefillOutcome out = cache.prefill({c});
  EXPECT_EQ(out.status, Status::DoesNotFit);
  EXPECT_EQ(out.needed, KiB);
  EXPECT_EQ(out.limit, 0u);
  EXPECT_TRUE(span_equals(la.spans()[0], pattern(KiB, 1)));
  EXPECT_TRUE(span_equals(lb.spans()[0], pattern(KiB, 2)));
  EXPECT_EQ(cache.stats().evictions, 0u);
}

TEST_F(PinnedHostCache, PrefillReloadsAChangedFile) {
  const auto a = write("a", KiB, 1);
  Cache cache(4 * KiB, kCounting);
  EXPECT_EQ(cache.prefill({a}).status, Status::Ok);
  write("a", KiB, 2);
  const PrefillOutcome out = cache.prefill({a});
  EXPECT_EQ(out.status, Status::Ok);
  EXPECT_EQ(out.files, 1u);
  EXPECT_EQ(cache.stats().reloads, 1u);
  auto lease = cache.acquire({a});
  ASSERT_TRUE(lease.cached());
  EXPECT_TRUE(span_equals(lease.spans()[0], pattern(KiB, 2)));
}

TEST_F(PinnedHostCache, PrefillEvictsBetweenCalls) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2), c = write("c", KiB, 3);
  Cache cache(2 * KiB, kCounting);
  EXPECT_EQ(cache.prefill({a, b}).status, Status::Ok);
  const void* pa = data_of(cache.acquire({a}));
  cache.acquire({b});
  const PrefillOutcome out = cache.prefill({c});
  EXPECT_EQ(out.status, Status::Ok);
  EXPECT_EQ(cache.stats().evictions, 1u);
  EXPECT_TRUE(heap->was_freed(pa));
  EXPECT_EQ(cache.stats().files, 2u);
  const std::size_t fills = cache.stats().fills;
  EXPECT_TRUE(cache.acquire({b, c}).cached());
  EXPECT_EQ(cache.stats().fills, fills);
}

TEST_F(PinnedHostCache, PrefillOfAMissingFileIsIo) {
  const auto a = write("a", KiB, 1);
  Cache cache(4 * KiB, kCounting);
  const PrefillOutcome out = cache.prefill({a, path("missing")});
  EXPECT_EQ(out.status, Status::Io);
  EXPECT_EQ(out.failed_index, 1u);
  EXPECT_EQ(out.error, ENOENT);
  EXPECT_EQ(heap->call_count(), 0u);
}

TEST_F(PinnedHostCache, PrefillAllocationRefusedIsIo) {
  const auto a = write("a", KiB, 1), b = write("b", KiB, 2);
  Cache cache(4 * KiB, kCounting);
  heap->refuse_call = 2;
  const PrefillOutcome out = cache.prefill({a, b});
  EXPECT_EQ(out.status, Status::Io);
  EXPECT_EQ(out.error, ENOMEM);
  EXPECT_EQ(out.failed_index, 1u);
  EXPECT_EQ(cache.stats().files, 1u);
  const std::size_t fills = cache.stats().fills;
  EXPECT_TRUE(cache.acquire({a}).cached());
  EXPECT_EQ(cache.stats().fills, fills);
}

TEST_F(PinnedHostCache, PrefillWhenOffIsOff) {
  const auto a = write("a", KiB, 1);
  Cache cache(0, kCounting);
  EXPECT_EQ(cache.prefill({a}).status, Status::Off);
  EXPECT_EQ(heap->call_count(), 0u);
}

// The process's cache: what sizes it, in order, and the one line it says at first use.

peacock::PinnedCacheConfig discrete_host(std::size_t ram) {
  peacock::PinnedCacheConfig config;
  config.device = peacock::DeviceKind::Discrete;
  config.host_ram_bytes = ram;
  return config;
}

TEST_F(PinnedHostCache, EnvironmentBytesAreADecimalCount) {
  EXPECT_EQ(peacock::parse_pinned_host_cache_bytes("0"), 0u);
  EXPECT_EQ(peacock::parse_pinned_host_cache_bytes("147000000000"), 147'000'000'000u);
  for (const char* bad : {"", "1G", "-1", " 12", "12 ", "lots", "18446744073709551616"}) {
    try {
      peacock::parse_pinned_host_cache_bytes(bad);
      ADD_FAILURE() << '"' << bad << "\" was taken as a byte count";
    } catch (std::runtime_error const& e) {
      EXPECT_NE(std::string(e.what()).find("PEACOCK_PINNED_HOST_CACHE_BYTES"), std::string::npos)
          << e.what();
    }
  }
}

TEST_F(PinnedHostCache, TheEnvironmentWinsOverTheSessionOverTheDefault) {
  using peacock::PinnedCacheSource;
  using peacock::PinnedCacheState;
  auto config = discrete_host(196'000'000'000);
  auto sized = peacock::size_pinned_cache(config);
  EXPECT_EQ(sized.state, PinnedCacheState::On);
  EXPECT_EQ(sized.source, PinnedCacheSource::Default);
  EXPECT_EQ(sized.capacity_bytes, 147'000'000'000u);

  config.cgroup_limit_bytes = 64'000'000'000;
  EXPECT_EQ(peacock::size_pinned_cache(config).capacity_bytes, 48'000'000'000u);

  config.session_bytes = 0;
  sized = peacock::size_pinned_cache(config);
  EXPECT_EQ(sized.state, PinnedCacheState::Off);
  EXPECT_EQ(sized.source, PinnedCacheSource::Session);
  EXPECT_EQ(sized.capacity_bytes, 0u);

  config.env_bytes = KiB;
  sized = peacock::size_pinned_cache(config);
  EXPECT_EQ(sized.state, PinnedCacheState::On);
  EXPECT_EQ(sized.source, PinnedCacheSource::Env);
  EXPECT_EQ(sized.capacity_bytes, KiB);
}

TEST_F(PinnedHostCache, NoDeviceAndAnIntegratedOnePinNothing) {
  using peacock::PinnedCacheState;
  auto config = discrete_host(196'000'000'000);
  config.session_bytes = kRoomy;
  config.device = peacock::DeviceKind::NoDevice;
  auto sized = peacock::size_pinned_cache(config);
  EXPECT_EQ(sized.state, PinnedCacheState::NoDevice);
  EXPECT_EQ(sized.source, peacock::PinnedCacheSource::Session);
  EXPECT_EQ(sized.capacity_bytes, 0u);
  config.device = peacock::DeviceKind::Integrated;
  sized = peacock::size_pinned_cache(config);
  EXPECT_EQ(sized.state, PinnedCacheState::Integrated);
  EXPECT_EQ(sized.capacity_bytes, 0u);
}

TEST_F(PinnedHostCache, FirstUseSaysWhereTheCapacityCameFrom) {
  constexpr std::size_t GiB = 1024 * 1024 * KiB;
  auto said = [](peacock::PinnedCacheConfig const& config) {
    return peacock::describe_pinned_cache(config, peacock::size_pinned_cache(config));
  };
  auto config = discrete_host(200 * GiB);
  EXPECT_EQ(said(config), "150.0 GiB of 200.0 GiB host RAM (default) on a discrete device");
  config.cgroup_limit_bytes = 64 * GiB;
  EXPECT_EQ(said(config), "48.0 GiB of 64.0 GiB cgroup limit (default) on a discrete device");
  config.session_bytes = GiB;
  EXPECT_EQ(said(config), "1.0 GiB (pinned_host_cache_bytes, session) on a discrete device");
  config.env_bytes = 0;
  EXPECT_EQ(said(config), "off (pinned_host_cache_bytes = 0, env)");
  config.device = peacock::DeviceKind::Integrated;
  EXPECT_EQ(said(config), "off: integrated device (pageable memory access)");
  config.device = peacock::DeviceKind::NoDevice;
  EXPECT_EQ(said(config), "off: no CUDA device");
}

}  // namespace
