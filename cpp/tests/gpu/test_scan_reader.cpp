/// The scan's reader: the one-chunk chunked parquet reader against read_parquet over the same
/// options (#232). A chunk limit set by mistake, or a cuDF whose "0 = no limit" changed, returns
/// several chunks; their concatenation must still be read_parquet's table.

#include "peacock/operators.h"

#include <cudf/aggregation.hpp>
#include <cudf/binaryop.hpp>
#include <cudf/io/parquet.hpp>
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>

#include <gtest/gtest.h>

#include <cstdlib>
#include <string>
#include <vector>

namespace {

std::string customer_path() {
  const char* env = std::getenv("PEACOCK_TESTDATA_DIR");
  return std::string(env ? env : PEACOCK_TESTDATA_DIR) + "/tpch.minimal/customer.parquet";
}

/// customer.parquet holds two row groups, 122880 and 27120 rows; four columns of four kinds.
cudf::io::parquet_reader_options customer_options() {
  return cudf::io::parquet_reader_options::builder(cudf::io::source_info{customer_path()})
      .columns({"c_custkey", "c_name", "c_nationkey", "c_acctbal"})
      .build();
}

std::vector<std::string> names_of(cudf::io::table_with_metadata const& t) {
  std::vector<std::string> names;
  for (auto const& info : t.metadata.schema_info) names.push_back(info.name);
  return names;
}

void expect_equal(cudf::io::table_with_metadata const& a, cudf::io::table_with_metadata const& b) {
  ASSERT_EQ(names_of(a), names_of(b));
  ASSERT_EQ(a.tbl->num_columns(), b.tbl->num_columns());
  ASSERT_EQ(a.tbl->num_rows(), b.tbl->num_rows());
  ASSERT_GT(a.tbl->num_rows(), 0);
  auto all = cudf::make_all_aggregation<cudf::reduce_aggregation>();
  const cudf::data_type bool8{cudf::type_id::BOOL8};
  for (cudf::size_type i = 0; i < a.tbl->num_columns(); ++i) {
    auto same = cudf::binary_operation(a.tbl->view().column(i), b.tbl->view().column(i),
                                       cudf::binary_operator::NULL_EQUALS, bool8);
    auto every = cudf::reduce(same->view(), *all, bool8);
    EXPECT_TRUE(static_cast<cudf::numeric_scalar<bool>&>(*every).value())
        << names_of(a)[i] << " differs between the two readers";
  }
}

}  // namespace

TEST(ScanReader, OneChunkEqualsReadParquet) {
  const auto opts = customer_options();
  const auto chunked = peacock::read_scan_table(opts, peacock::ScanReader::Chunked);
  const auto whole = peacock::read_scan_table(opts, peacock::ScanReader::ReadParquet);
  EXPECT_EQ(chunked.chunks, 1u);
  EXPECT_EQ(whole.chunks, 1u);
  EXPECT_EQ(chunked.table.tbl->num_rows(), 150000);
  expect_equal(chunked.table, whole.table);
}

TEST(ScanReader, SeveralChunksConcatenateToReadParquet) {
  const auto opts = customer_options();
  const auto chunked = peacock::read_scan_table(opts, peacock::ScanReader::Chunked, 1 << 20);
  const auto whole = peacock::read_scan_table(opts, peacock::ScanReader::ReadParquet);
  EXPECT_GT(chunked.chunks, 1u);
  expect_equal(chunked.table, whole.table);
}

TEST(ScanReader, RowGroupSelectionIsKept) {
  auto opts = customer_options();
  opts.set_row_groups({{1}});
  const auto chunked = peacock::read_scan_table(opts, peacock::ScanReader::Chunked);
  const auto whole = peacock::read_scan_table(opts, peacock::ScanReader::ReadParquet);
  EXPECT_EQ(chunked.table.tbl->num_rows(), 27120);
  expect_equal(chunked.table, whole.table);
}
