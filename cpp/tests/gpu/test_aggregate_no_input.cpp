/// An aggregate handed no input. A lane that sent no row out makes its init's done call with no
/// handle, and the node aggregates the zero-row table its aggr_input_schema declares: one row
/// for a keyless node, typed as a batch would have been, and no groups for a grouped one. The
/// plan holds a scan under the aggregate, as every plan the writer makes does; the scan is
/// never called, so its file is never opened.

#include "hand_tables.hpp"

#include "generated/gpu_plan_generated.h"
#include "plan_executor.h"

#include <cudf/types.hpp>
#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

namespace {

namespace fb = peacock::plan;

/// The declared input: an Int32 key, an Int64 value, a Decimal128(15, 2), a date and a
/// string. None of it is read from a file.
flatbuffers::Offset<fb::Schema> declared_input(flatbuffers::FlatBufferBuilder& fbb) {
  std::vector<flatbuffers::Offset<fb::Field>> fields;
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("k"), fb::DataType_Int32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("v"), fb::DataType_Int64, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("amount"), fb::DataType_Decimal128, true,
                                   /*decimal_precision=*/15, /*decimal_scale=*/2));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("day"), fb::DataType_Date32, true));
  fields.push_back(fb::CreateField(fbb, fbb.CreateString("name"), fb::DataType_Utf8, true));
  return fb::CreateSchema(fbb, fbb.CreateVector(fields));
}

/// A scan of `declared_input`, seq 0 in post-order. Never executed.
flatbuffers::Offset<fb::PlanNode> unread_scan(flatbuffers::FlatBufferBuilder& fbb) {
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{
      fbb.CreateString("/never/read.parquet")});
  auto schema = declared_input(fbb);
  fb::CudfScanBuilder scan(fbb);
  scan.add_file_paths(paths);
  scan.add_file_schema(schema);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfScan, scan.Finish().Union());
}

/// An Int64 literal, built field by field as the plan writer does.
flatbuffers::Offset<fb::Expr> int64_literal(flatbuffers::FlatBufferBuilder& fbb, int64_t value) {
  fb::ScalarValueBuilder sb(fbb);
  sb.add_type(fb::DataType_Int64);
  sb.add_int_val(value);
  auto scalar = sb.Finish();
  return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(fbb, scalar).Union());
}

/// `func(arg)` under `alias`.
flatbuffers::Offset<fb::AggregateFuncNode> func_over(flatbuffers::FlatBufferBuilder& fbb,
                                                     const char* func,
                                                     flatbuffers::Offset<fb::Expr> arg,
                                                     const char* alias) {
  auto name = fbb.CreateString(func);
  auto args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{arg});
  auto out = fbb.CreateString(alias);
  fb::AggregateFuncNodeBuilder node(fbb);
  node.add_name(name);
  node.add_args(args);
  node.add_alias(out);
  return node.Finish();
}

/// count(*), sum(v), sum(amount), min(day) and max(v) over `declared_input`, keyed on `k`
/// where `grouped`, as the init a done call addresses: Partial mode, seq 1 above the scan.
/// `count(*)` is `count(1)`, as the planner writes it (`recipe-payloads.txt`).
std::vector<uint8_t> unfed_aggregate(flatbuffers::FlatBufferBuilder& fbb, bool grouped) {
  auto scan = unread_scan(fbb);
  std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs{
      func_over(fbb, "count", int64_literal(fbb, 1), "n"),
      func_over(fbb, "sum", hand::ref(fbb, 1, "v"), "sum(v)"),
      func_over(fbb, "sum", hand::ref(fbb, 2, "amount"), "sum(amount)"),
      func_over(fbb, "min", hand::ref(fbb, 3, "day"), "min(day)"),
      func_over(fbb, "max", hand::ref(fbb, 1, "v"), "max(v)"),
  };
  auto funcs_vec = fbb.CreateVector(funcs);
  auto input = declared_input(fbb);
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<fb::Expr>>> groups;
  flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<flatbuffers::String>>> names;
  if (grouped) {
    groups = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{hand::ref(fbb, 0, "k")});
    names = fbb.CreateVector(
        std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString("k")});
  }
  fb::CudfAggregateBuilder builder(fbb);
  builder.add_mode(fb::AggregateMode_Partial);
  if (grouped) {
    builder.add_group_exprs(groups);
    builder.add_group_names(names);
  }
  builder.add_aggr_funcs(funcs_vec);
  builder.add_input(scan);
  builder.add_aggr_input_schema(input);
  auto agg = builder.Finish();
  fbb.Finish(
      fb::CreateGpuPlan(fbb, fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, agg.Union())));
  return {fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize()};
}

/// Seq `seq` called with no handle at all, as an init's done call makes it.
uint64_t call_with_nothing(peacock::NodeSession& session, uint64_t seq) {
  uint64_t out = 0;
  size_t produced = 0;
  peacock::NodeStats stats{};
  session.execute_node(seq, nullptr, nullptr, 0, &out, 1, &produced, &stats);
  EXPECT_EQ(produced, 1u);
  return out;
}

}  // namespace

TEST(AggregateNoInput, AKeylessNodeAnswersOneRowTypedAsItsInputDeclares) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = unfed_aggregate(fbb, /*grouped=*/false);
  peacock::NodeSession session(buf.data(), buf.size());
  const auto& result = session.table_for(call_with_nothing(session, /*seq=*/1));
  auto view = result.view();
  ASSERT_EQ(view.num_rows(), 1);
  ASSERT_EQ(view.num_columns(), 5);
  EXPECT_EQ(hand::values_of<int64_t>(view.column(0)), std::vector<int64_t>{0})
      << "count(*) over nothing";
  EXPECT_EQ(view.column(0).null_count(), 0);
  EXPECT_EQ(view.column(1).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(view.column(2).type(), (cudf::data_type{cudf::type_id::DECIMAL128, -2}))
      << "the scale is the field's, which the type id cannot carry";
  EXPECT_EQ(view.column(3).type().id(), cudf::type_id::TIMESTAMP_DAYS);
  EXPECT_EQ(view.column(4).type().id(), cudf::type_id::INT64);
  for (cudf::size_type c = 1; c < 5; ++c)
    EXPECT_EQ(view.column(c).null_count(), 1) << "column " << c << " is NULL over nothing";
  EXPECT_EQ(result.column_names,
            (std::vector<std::string>{"n", "sum(v)", "sum(amount)", "min(day)", "max(v)"}));
}

TEST(AggregateNoInput, AGroupedNodeAnswersNoGroups) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = unfed_aggregate(fbb, /*grouped=*/true);
  peacock::NodeSession session(buf.data(), buf.size());
  const auto& result = session.table_for(call_with_nothing(session, /*seq=*/1));
  EXPECT_EQ(result.view().num_rows(), 0);
  EXPECT_EQ(result.column_names.front(), "k") << "the key leads, as over any input";
}

TEST(AggregateNoInput, AnyOtherNodeHandedNothingStillRefuses) {
  flatbuffers::FlatBufferBuilder fbb;
  auto scan = unread_scan(fbb);
  // Never evaluated: the filter's first act is take_input, which refuses.
  auto filter = fb::CreateCudfFilter(fbb, hand::ref(fbb, 0, "k"), scan);
  fbb.Finish(
      fb::CreateGpuPlan(fbb, fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfFilter, filter.Union())));
  std::vector<uint8_t> buf(fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize());
  peacock::NodeSession session(buf.data(), buf.size());
  try {
    call_with_nothing(session, /*seq=*/1);
    FAIL() << "a filter handed no input answered";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("not enough input handles"), std::string::npos)
        << e.what();
  }
}
