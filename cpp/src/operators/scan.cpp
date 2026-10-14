// CudfScan -- Parquet reads, row-group selection, projection pushdown.

#include "peacock/operators.h"
#include "peacock/expr.h"

#include <cudf/column/column_factories.hpp>
#include <cudf/io/parquet.hpp>
#include <cudf/io/parquet_metadata.hpp>
#include <cudf/concatenate.hpp>
#include <cudf/table/table.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/unary.hpp>
#include <cudf/utilities/default_stream.hpp>

#include <algorithm>
#include <stdexcept>
#include <string>

namespace peacock {

/// A scan of no column: the rows the chosen row groups hold, read off the footer, as the one
/// non-null INT8 column of zeros the plan declares in their place (#63). Nothing is decoded.
static TableResult rows_of(std::vector<std::string> const& paths,
                           std::vector<cudf::size_type> const& rgs, uint64_t limit) {
  auto meta = cudf::io::read_parquet_metadata(cudf::io::source_info{paths});
  int64_t rows = 0;
  if (rgs.empty()) {
    rows = meta.num_rows();
  } else {
    auto const& groups = meta.rowgroup_metadata();
    for (auto g : rgs) rows += groups.at(static_cast<std::size_t>(g)).at("num_rows");
  }
  if (limit > 0) rows = std::min<int64_t>(rows, static_cast<int64_t>(limit));
  cudf::numeric_scalar<int8_t> zero(0, true);
  std::vector<std::unique_ptr<cudf::column>> cols;
  cols.push_back(cudf::make_column_from_scalar(zero, static_cast<cudf::size_type>(rows)));
  return TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), {"__rowmarker__"});
}

TableResult execute_scan(const fb::CudfScan* scan,
                         cudf::host_span<const uint32_t> row_groups_override) {
  if (!scan->file_paths() || scan->file_paths()->size() == 0)
    throw std::runtime_error("CudfScan: no file paths");

  // Wire-format contract (see gpu_plan.fbs::CudfScan): every path must be
  // absolute. We reject anything else with a clear error rather than
  // resolving against an implicit root.
  std::vector<std::string> paths;
  paths.reserve(scan->file_paths()->size());
  for (auto* p : *scan->file_paths()) {
    auto s = p->str();
    if (s.empty() || s.front() != '/') {
      throw std::runtime_error(
          "CudfScan: file path must be absolute (got \"" + s + "\")");
    }
    paths.push_back(std::move(s));
  }

  // Row-group pruning: decode ONLY the groups the serializer computed (the same
  // DataFusion PruningPredicate the CPU path prunes with) unless this call overrides
  // them — with the RG→partition map, or one batch of the loader.
  // An empty override defers to the node's list, and an empty list reads all (#16).
  std::vector<cudf::size_type> rgs;
  if (!row_groups_override.empty()) {
    rgs.reserve(row_groups_override.size());
    for (auto rg : row_groups_override) rgs.push_back(static_cast<cudf::size_type>(rg));
  } else if (auto* own = scan->row_groups()) {
    rgs.reserve(own->size());
    for (auto rg : *own) rgs.push_back(static_cast<cudf::size_type>(rg));
  }

  // The limit and the row groups go on before either read: cuDF refuses the two together
  // (#186), and a scan of no column meets that refusal rather than answering past it.
  auto opts = cudf::io::parquet_reader_options::builder(cudf::io::source_info{paths}).build();
  if (scan->limit() > 0) {
    opts.set_num_rows(static_cast<cudf::size_type>(scan->limit()));
  }
  if (!rgs.empty()) {
    opts.set_row_groups({rgs});
  }

  if (scan->rows_only()) return rows_of(paths, rgs, scan->limit());

  // Build column name list from file_schema + projection.
  std::vector<std::string> all_names;
  if (scan->file_schema() && scan->file_schema()->fields()) {
    for (auto* f : *scan->file_schema()->fields()) {
      all_names.push_back(f->name()->str());
    }
  }

  std::vector<std::string> projected_names;
  if (scan->projection() && scan->projection()->size() > 0) {
    for (auto idx : *scan->projection()) {
      if (idx < all_names.size()) {
        projected_names.push_back(all_names[idx]);
      }
    }
  } else {
    projected_names = all_names;
  }

  opts.set_columns(projected_names);

  auto result = cudf::io::read_parquet(opts);

  // Optional diagnostic (PEACOCK_LOG_SCAN_ROWS=1): rows actually decoded and
  // whether row-group pruning applied — evidence that a clustered-predicate scan
  // reads only surviving groups (q6 lineitem -> 983040, not 6001215).
  if (std::getenv("PEACOCK_LOG_SCAN_ROWS")) {
    std::fprintf(stderr, "[PEACOCK_SCAN] %s rows=%ld row_groups=%s(%zu)\n",
                 paths.empty() ? "?" : paths[0].c_str(), static_cast<long>(result.tbl->num_rows()),
                 rgs.empty() ? "all" : "pruned", rgs.size());
  }

  // Use column names from the reader metadata.
  std::vector<std::string> col_names;
  for (auto& ci : result.metadata.schema_info) {
    col_names.push_back(ci.name);
  }

  // Widen narrow decimals to DECIMAL128 (scale preserved). The cuDF parquet reader
  // picks the smallest fixed_point width that fits, but DataFusion — and so our
  // serialized literals and the CPU oracle — use Decimal128 throughout, and cuDF's
  // binary_operation rejects mixed fixed_point widths. The export refuses any other
  // width, so this is the only widening.
  auto cols = result.tbl->release();
  for (auto& c : cols) {
    auto id = c->type().id();
    if (id == cudf::type_id::DECIMAL32 || id == cudf::type_id::DECIMAL64) {
      c = cudf::cast(c->view(),
                     cudf::data_type{cudf::type_id::DECIMAL128, c->type().scale()});
    }
  }

  return TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), std::move(col_names));
}

}  // namespace peacock
