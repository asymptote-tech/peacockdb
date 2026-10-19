/// The grouping-set id is DataFusion's `__grouping_id`. The masks fold in key order, the
/// first key highest, at the narrowest unsigned width with a bit per key; past 64 keys refused.
/// Every key is the one hand-built column `k` again, so a set's groups are k's two values or,
/// fully masked, one row; only the id is read.

#include "hand_tables.hpp"

#include <gtest/gtest.h>

#include <cstdint>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
namespace fb = peacock::plan;

/// The resident input: k = 1 1 2 2 2.
std::vector<peacock::TableResult> the_keys() {
  std::vector<std::unique_ptr<cudf::column>> columns;
  columns.push_back(hand::column_of<int32_t>({1, 1, 2, 2, 2}));
  std::vector<peacock::TableResult> inputs;
  inputs.push_back(hand::table_of(std::move(columns), {"k"}));
  return inputs;
}

/// A Partial grouped on `nkeys` copies of `k` under `masks`, with count(k).
flatbuffers::Offset<fb::PlanNode> grouping_sets_node(flatbuffers::FlatBufferBuilder& fbb, int nkeys,
                                                     const std::vector<std::vector<bool>>& masks) {
  std::vector<flatbuffers::Offset<fb::Expr>> keys, nulls;
  std::vector<flatbuffers::Offset<flatbuffers::String>> names;
  for (int i = 0; i < nkeys; ++i) {
    keys.push_back(hand::ref(fbb, 0, "k"));
    nulls.push_back(hand::null_of(fbb, fb::DataType_Int32));
    names.push_back(fbb.CreateString("k" + std::to_string(i)));
  }
  std::vector<flatbuffers::Offset<fb::GroupingSetMask>> sets;
  for (const auto& mask : masks) {
    std::vector<uint8_t> bits(mask.begin(), mask.end());
    auto values = fbb.CreateVector(bits);
    sets.push_back(fb::CreateGroupingSetMask(fbb, values));
  }
  auto count_name = fbb.CreateString("count");
  auto count_alias = fbb.CreateString("n");
  auto count_arg = hand::ref(fbb, 0, "k");
  auto count_args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{count_arg});
  fb::AggregateFuncNodeBuilder count(fbb);
  count.add_name(count_name);
  count.add_args(count_args);
  count.add_alias(count_alias);
  auto count_func = count.Finish();
  auto funcs =
      fbb.CreateVector(std::vector<flatbuffers::Offset<fb::AggregateFuncNode>>{count_func});
  auto key_vec = fbb.CreateVector(keys);
  auto name_vec = fbb.CreateVector(names);
  auto null_vec = fbb.CreateVector(nulls);
  auto null_names = fbb.CreateVector(names);
  auto set_vec = fbb.CreateVector(sets);
  fb::CudfAggregateBuilder agg(fbb);
  agg.add_mode(fb::AggregateMode_Partial);
  agg.add_group_exprs(key_vec);
  agg.add_group_names(name_vec);
  agg.add_aggr_funcs(funcs);
  agg.add_null_exprs(null_vec);
  agg.add_null_names(null_names);
  agg.add_grouping_sets(set_vec);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, agg.Finish().Union());
}

std::vector<bool> none_masked(int nkeys) { return std::vector<bool>(nkeys, false); }
std::vector<bool> all_masked(int nkeys) { return std::vector<bool>(nkeys, true); }

/// `nkeys` keys with only `position` masked.
std::vector<bool> masked_at(int nkeys, int position) {
  auto mask = none_masked(nkeys);
  mask[position] = true;
  return mask;
}

/// The id column a grouping-set Partial answers: its type, and the value each set carries.
struct IdColumn {
  cudf::type_id type;
  std::set<uint64_t> values;
};

IdColumn id_column(int nkeys, const std::vector<std::vector<bool>>& masks) {
  flatbuffers::FlatBufferBuilder fbb;
  auto result = hand::run(fbb, grouping_sets_node(fbb, nkeys, masks), the_keys());
  EXPECT_EQ(result.column_names.at(nkeys), "__grouping_id");
  auto id = result.view().column(nkeys);
  IdColumn out{id.type().id(), {}};
  auto widen = [&](auto zero) {
    auto host = hand::values_of<decltype(zero)>(id);
    out.values.insert(host.begin(), host.end());
  };
  switch (out.type) {
    case cudf::type_id::UINT8:
      widen(uint8_t{});
      break;
    case cudf::type_id::UINT16:
      widen(uint16_t{});
      break;
    case cudf::type_id::UINT32:
      widen(uint32_t{});
      break;
    case cudf::type_id::UINT64:
      widen(uint64_t{});
      break;
    default:
      ADD_FAILURE() << "the id is not an unsigned integer: type id " << static_cast<int>(out.type);
  }
  return out;
}

}  // namespace

TEST(GroupingId, OneKeyIsUInt8) {
  auto ids = id_column(1, {none_masked(1), all_masked(1)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{0, 1}));
}

TEST(GroupingId, ATwoKeyRollupFoldsTheFirstKeyHighest) {
  // ROLLUP(a, b) is (a, b), (a), (): 0, 1, 3. Bit i for masked key i gave the middle set 2.
  auto ids = id_column(2, {none_masked(2), masked_at(2, 1), all_masked(2)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{0, 1, 3}));
}

TEST(GroupingId, EightKeysStayUInt8) {
  auto ids = id_column(8, {masked_at(8, 0), masked_at(8, 7), all_masked(8)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT8);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{1, 128, 255}));
}

TEST(GroupingId, NineKeysAreUInt16) {
  auto ids = id_column(9, {masked_at(9, 0), all_masked(9)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT16);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{256, 511}));
}

TEST(GroupingId, SeventeenKeysAreUInt32) {
  auto ids = id_column(17, {masked_at(17, 0), all_masked(17)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT32);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{65536, 131071}));
}

TEST(GroupingId, ThirtyThreeKeysAreUInt64) {
  auto ids = id_column(33, {masked_at(33, 0), all_masked(33)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT64);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{uint64_t{1} << 32, (uint64_t{1} << 33) - 1}));
}

TEST(GroupingId, SixtyFourKeysFillTheWord) {
  auto ids = id_column(64, {masked_at(64, 0), all_masked(64)});
  EXPECT_EQ(ids.type, cudf::type_id::UINT64);
  EXPECT_EQ(ids.values, (std::set<uint64_t>{uint64_t{1} << 63, UINT64_MAX}));
}

TEST(GroupingId, SixtyFiveKeysAreRefusedByCount) {
  // DataFusion refuses past 64 keys; an id that wrapped would answer the wrong sets.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = grouping_sets_node(fbb, 65, {none_masked(65), all_masked(65)});
  try {
    hand::run(fbb, node, the_keys());
    FAIL() << "a grouping set over 65 keys answered";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("65 keys"), std::string::npos) << e.what();
  }
}
