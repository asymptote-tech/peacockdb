// #232's re-measure (llm-wiki/archive/archived-tickets.md#t232): read_parquet against a
// one-chunk chunked_parquet_reader, each from the path and from the same file held whole in
// pinned host memory. A measurement, not a check, so the case is disabled and runs by name:
// GTEST_ALSO_RUN_DISABLED_TESTS=1 GTEST_FILTER='DISABLED_ReaderProbe.*' with PEACOCK_PROBE_FILE
// (one parquet path), PEACOCK_PROBE_COLUMNS (comma list; unset reads every column) and
// PEACOCK_PROBE_RUNS (default 10). An sf40 table needs a device pool its size: main()'s is
// 1 GiB, so set PEACOCK_RMM_POOL_BYTES.
#include <cudf/io/parquet.hpp>
#include <cudf/io/types.hpp>
#include <cudf/table/table.hpp>
#include <cudf/utilities/default_stream.hpp>
#include <cudf/utilities/span.hpp>

#include <cuda_runtime.h>

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <sstream>
#include <string>
#include <vector>

#include <gtest/gtest.h>

namespace {

struct PinnedFree {
  void operator()(std::byte* p) const { cudaFreeHost(p); }
};
using PinnedBytes = std::unique_ptr<std::byte, PinnedFree>;

// One pread returns at most 0x7ffff000 bytes on Linux, and an sf40 lineitem is ~9 GB.
void read_pinned(const std::string& path, PinnedBytes& out, std::size_t& size) {
  const int fd = open(path.c_str(), O_RDONLY);
  ASSERT_GE(fd, 0) << "cannot open " << path;
  struct stat st {};
  ASSERT_EQ(fstat(fd, &st), 0);
  size = static_cast<std::size_t>(st.st_size);
  void* p = nullptr;
  ASSERT_EQ(cudaHostAlloc(&p, size, cudaHostAllocDefault), cudaSuccess) << size << " bytes";
  out.reset(static_cast<std::byte*>(p));
  std::size_t done = 0;
  while (done < size) {
    const ssize_t n = pread(fd, out.get() + done, size - done, static_cast<off_t>(done));
    ASSERT_GT(n, 0) << "short read of " << path << " at " << done;
    done += static_cast<std::size_t>(n);
  }
  close(fd);
}

std::vector<std::string> split_columns(const char* list) {
  std::vector<std::string> out;
  if (!list) return out;
  std::stringstream in(list);
  for (std::string name; std::getline(in, name, ',');) {
    if (!name.empty()) out.push_back(name);
  }
  return out;
}

struct Read {
  int chunks = 0;
  std::int64_t rows = 0;
  cudf::size_type columns = 0;
};

Read read_once(const cudf::io::source_info& source, const std::vector<std::string>& columns,
               bool chunked) {
  auto builder = cudf::io::parquet_reader_options::builder(source);
  if (!columns.empty()) builder.columns(columns);
  const auto options = builder.build();
  Read out;
  if (!chunked) {
    const auto result = cudf::io::read_parquet(options);
    out = {1, result.tbl->num_rows(), result.tbl->num_columns()};
  } else {
    // A limit of 0 is no limit in either reader argument: one pass, one chunk.
    cudf::io::chunked_parquet_reader reader(0, options);
    while (reader.has_next()) {
      const auto result = reader.read_chunk();
      ++out.chunks;
      out.rows += result.tbl->num_rows();
      out.columns = result.tbl->num_columns();
    }
  }
  cudf::get_default_stream().synchronize();
  return out;
}

}  // namespace

TEST(DISABLED_ReaderProbe, ReadParquetAgainstOneChunk) {
  const char* file = std::getenv("PEACOCK_PROBE_FILE");
  if (!file || !*file) GTEST_SKIP() << "PEACOCK_PROBE_FILE is not set: name one parquet file";
  const std::vector<std::string> columns = split_columns(std::getenv("PEACOCK_PROBE_COLUMNS"));
  const char* runs_env = std::getenv("PEACOCK_PROBE_RUNS");
  const unsigned long runs = runs_env && *runs_env ? std::strtoul(runs_env, nullptr, 10) : 10;
  ASSERT_GE(runs, 1u) << "PEACOCK_PROBE_RUNS=" << runs_env;

  PinnedBytes pinned;
  std::size_t size = 0;
  ASSERT_NO_FATAL_FAILURE(read_pinned(file, pinned, size));
  const cudf::io::source_info from_path{std::string(file)};
  const cudf::io::source_info from_pinned{cudf::host_span<std::byte const>(pinned.get(), size)};

  struct Case {
    const char* source;
    const char* reader;
    const cudf::io::source_info* info;
    bool chunked;
  };
  const Case cases[] = {{"path", "read_parquet", &from_path, false},
                        {"path", "chunked", &from_path, true},
                        {"pinned", "read_parquet", &from_pinned, false},
                        {"pinned", "chunked", &from_pinned, true}};

  std::vector<Read> reads;
  for (const Case& c : cases) {
    Read read;
    std::vector<std::int64_t> us;
    for (unsigned long r = 0; r < runs; ++r) {
      const auto start = std::chrono::steady_clock::now();
      read = read_once(*c.info, columns, c.chunked);
      us.push_back(std::chrono::duration_cast<std::chrono::microseconds>(
                       std::chrono::steady_clock::now() - start)
                       .count());
    }
    std::string list;
    for (std::size_t i = 0; i < us.size(); ++i) list += (i ? "," : "") + std::to_string(us[i]);
    // The second-fastest run: the fastest alone can be an outlier, and the first is a warm-up.
    std::vector<std::int64_t> sorted = us;
    std::sort(sorted.begin(), sorted.end());
    std::printf("[reader-probe] source=%s reader=%s chunks=%d rows=%lld us=%lld runs=[%s]\n",
                c.source, c.reader, read.chunks, static_cast<long long>(read.rows),
                static_cast<long long>(sorted[std::min<std::size_t>(1, sorted.size() - 1)]),
                list.c_str());
    std::fflush(stdout);
    reads.push_back(read);
  }

  for (std::size_t i = 1; i < reads.size(); ++i) {
    EXPECT_EQ(reads[i].rows, reads[0].rows) << cases[i].source << " " << cases[i].reader;
    EXPECT_EQ(reads[i].columns, reads[0].columns) << cases[i].source << " " << cases[i].reader;
  }
}
