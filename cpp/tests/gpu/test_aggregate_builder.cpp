/// The aggregate's one request builder, per function and phase: what each function emits and
/// under which names, read where its args point, and the shapes it refuses.

#include "hand_tables.hpp"

#include <cudf/column/column_factories.hpp>

#include <gtest/gtest.h>

#include <algorithm>
#include <cmath>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
namespace fb = peacock::plan;
using Offsets = std::vector<flatbuffers::Offset<fb::Expr>>;

flatbuffers::Offset<fb::AggregateFuncNode> func(flatbuffers::FlatBufferBuilder& fbb,
                                                const char* name, Offsets args,
                                                std::vector<std::string> const& state,
                                                int8_t ddof = 0) {
  auto n = fbb.CreateString(name);
  auto a = fbb.CreateVector(args);
  auto s = fbb.CreateVectorOfStrings(state);
  fb::AggregateFuncNodeBuilder b(fbb);
  b.add_name(n);
  b.add_args(a);
  b.add_state_names(s);
  b.add_ddof(ddof);
  return b.Finish();
}

/// A CudfAggregate grouped on `groups`; `sets` non-empty makes it a grouping-set node, with an
/// INT32 NULL placeholder per key.
flatbuffers::Offset<fb::PlanNode> aggregate(
    flatbuffers::FlatBufferBuilder& fbb, fb::AggregateMode mode, Offsets groups,
    std::vector<std::string> const& group_names,
    std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs,
    std::vector<std::vector<uint8_t>> const& sets = {}) {
  auto g = fbb.CreateVector(groups);
  auto gn = fbb.CreateVectorOfStrings(group_names);
  auto f = fbb.CreateVector(funcs);
  Offsets nulls;
  std::vector<flatbuffers::Offset<fb::GroupingSetMask>> masks;
  for (size_t i = 0; !sets.empty() && i < groups.size(); ++i)
    nulls.push_back(hand::null_of(fbb, fb::DataType_Int32));
  for (auto const& set : sets)
    masks.push_back(fb::CreateGroupingSetMask(fbb, fbb.CreateVector(set)));
  auto n = fbb.CreateVector(nulls);
  auto nn = fbb.CreateVectorOfStrings(sets.empty() ? std::vector<std::string>{} : group_names);
  auto m = fbb.CreateVector(masks);
  fb::CudfAggregateBuilder b(fbb);
  b.add_mode(mode);
  b.add_group_exprs(g);
  b.add_group_names(gn);
  b.add_aggr_funcs(f);
  b.add_null_exprs(n);
  b.add_null_names(nn);
  b.add_grouping_sets(m);
  return fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfAggregate, b.Finish().Union());
}

/// k: 1 1 2 2 2; v: 10 20 30 40 50.
peacock::TableResult rows() {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2, 2, 2}));
  c.push_back(hand::column_of<int64_t>({10, 20, 30, 40, 50}));
  return hand::table_of(std::move(c), {"k", "v"});
}

std::vector<peacock::TableResult> one(peacock::TableResult t) {
  std::vector<peacock::TableResult> in;
  in.push_back(std::move(t));
  return in;
}

/// Column `at` of a result grouped on an INT32 key, by key: cuDF's groupby orders nothing.
template <typename T>
std::map<int32_t, T> by_key(peacock::TableResult const& r, int at) {
  auto keys = hand::values_of<int32_t>(r.view().column(0));
  auto values = hand::values_of<T>(r.view().column(at));
  std::map<int32_t, T> out;
  for (size_t i = 0; i < keys.size(); ++i) out[keys[i]] = values[i];
  return out;
}

std::string refusal(flatbuffers::FlatBufferBuilder& fbb, flatbuffers::Offset<fb::PlanNode> node,
                    peacock::TableResult input) {
  try {
    hand::run(fbb, node, one(std::move(input)));
  } catch (std::runtime_error const& e) {
    return e.what();
  }
  return "(not refused)";
}

}  // namespace

TEST(AggregateBuilder, PlainFunctionsAreNamedFromTheirStateNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"sum(v)"}),
                         func(fbb, "min", {hand::ref(fbb, 1, "v")}, {"min(v)"}),
                         func(fbb, "max", {hand::ref(fbb, 1, "v")}, {"max(v)"}),
                         func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"count(v)"})});
  auto out = hand::run(fbb, node, one(rows()));
  EXPECT_EQ(out.column_names,
            (std::vector<std::string>{"k", "sum(v)", "min(v)", "max(v)", "count(v)"}));
  EXPECT_EQ(out.view().column(4).type().id(), cudf::type_id::INT64) << "every count is INT64";
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 30}, {2, 120}}));
  EXPECT_EQ((by_key<int64_t>(out, 2)), (std::map<int32_t, int64_t>{{1, 10}, {2, 30}}));
  EXPECT_EQ((by_key<int64_t>(out, 3)), (std::map<int32_t, int64_t>{{1, 20}, {2, 50}}));
  EXPECT_EQ((by_key<int64_t>(out, 4)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
}

TEST(AggregateBuilder, AWelfordInitEmitsItsTripleUnderItsThreeNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "s$count", "s$mean", "s$m2"}));
  EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 15.0}, {2, 40.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 50.0}, {2, 200.0}}));
}

TEST(AggregateBuilder, AMergeReadsEachStateColumnWhereItsArgPoints) {
  // [k, a, b], merged as sum(b) then max(a): the first function's state is not the column
  // after the keys, so a cursor would hand sum the a's (3, 3) and max the b's (200, 300).
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2}));
  c.push_back(hand::column_of<int64_t>({1, 2, 3}));
  c.push_back(hand::column_of<int64_t>({100, 200, 300}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "sum", {hand::ref(fbb, 2, "b")}, {"b"}),
                         func(fbb, "max", {hand::ref(fbb, 1, "a")}, {"a"})});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "a", "b"})));
  EXPECT_EQ(out.column_names, (std::vector<std::string>{"k", "b", "a"}));
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 300}, {2, 300}}));
  EXPECT_EQ((by_key<int64_t>(out, 2)), (std::map<int32_t, int64_t>{{1, 2}, {2, 3}}));
}

TEST(AggregateBuilder, ACountAtMergeIsRefused) {
  // The plan merges a count's state with sum (`count(*): sum(count(*)@1)`), and the cpu's
  // merge refuses a count: one at Merge is a hand-built plan, refused by name, keyed or not.
  for (bool keyed : {true, false}) {
    flatbuffers::FlatBufferBuilder fbb;
    auto count = func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"});
    auto node =
        keyed ? aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"}, {count})
              : aggregate(fbb, fb::AggregateMode_Merge, {}, {}, {count});
    auto why = refusal(fbb, node, rows());
    EXPECT_NE(why.find("`count` at Merge"), std::string::npos) << "keyed " << keyed << ": " << why;
  }
}

TEST(AggregateBuilder, AWelfordMergeReadsTheTripleWhereItsArgsPoint) {
  // The state arrives as [k, m2, count, mean]; a merge that counted columns would read m2 as
  // the count. Chan's merge of (2, 1.5, 2) and (4, 4, 8): n 6, mean 19/6,
  // m2 = 2 + 8 + 2.5² · 2·4/6.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1}));
  c.push_back(hand::column_of<double>({2.0, 8.0}));
  c.push_back(hand::column_of<int64_t>({2, 4}));
  c.push_back(hand::column_of<double>({1.5, 4.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(
      fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
      {func(fbb, "stddev",
            {hand::ref(fbb, 2, "s$count"), hand::ref(fbb, 3, "s$mean"), hand::ref(fbb, 1, "s$m2")},
            {"s$count", "s$mean", "s$m2"}, 1)});
  auto out =
      hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "s$m2", "s$count", "s$mean"})));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "s$count", "s$mean", "s$m2"}));
  EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64) << "widened back";
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{6});
  EXPECT_NEAR(hand::values_of<double>(out.view().column(2))[0], 19.0 / 6.0, 1e-12);
  EXPECT_NEAR(hand::values_of<double>(out.view().column(3))[0], 10.0 + 6.25 * 8.0 / 6.0, 1e-12);
}

TEST(AggregateBuilder, AGroupingSetBuildsTheWelfordTripleAndWhatFollowsIt) {
  // #280: ROLLUP(k) with a stddev and a sum after it. The grand total is the set with k masked.
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1),
                 func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"sum(v)"})},
                {{0}, {1}});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"k", "__grouping_id", "s$count", "s$mean",
                                                        "s$m2", "sum(v)"}));
  ASSERT_EQ(out.num_rows(), 3) << "two keys and the grand total";
  ASSERT_EQ(out.view().column(1).type().id(), cudf::type_id::UINT8) << "one key: a UInt8 id";
  auto ids = hand::values_of<uint8_t>(out.view().column(1));
  auto counts = hand::values_of<int64_t>(out.view().column(2));
  auto sums = hand::values_of<int64_t>(out.view().column(5));
  ASSERT_EQ(std::count(ids.begin(), ids.end(), uint8_t{0}), 2) << "the set keyed on k: two groups";
  ASSERT_EQ(std::count(ids.begin(), ids.end(), uint8_t{1}), 1) << "the grand total: one row";
  for (size_t row = 0; row < ids.size(); ++row) {
    if (ids[row] != 1) continue;
    EXPECT_EQ(counts[row], 5) << "the grand total counts every row";
    EXPECT_EQ(sums[row], 150) << "and the sum after the triple is its own, not a neighbour";
  }
}

TEST(AggregateBuilder, AWelfordOverZeroRowsIsNoGroupsTypedAsDeclared) {
  for (auto mode : {fb::AggregateMode_Partial, fb::AggregateMode_Merge}) {
    bool merging = mode == fb::AggregateMode_Merge;
    std::vector<std::unique_ptr<cudf::column>> c;
    c.push_back(hand::column_of<int32_t>({}));
    c.push_back(hand::column_of<int64_t>({}));
    c.push_back(hand::column_of<double>({}));
    c.push_back(hand::column_of<double>({}));
    flatbuffers::FlatBufferBuilder fbb;
    Offsets args = merging ? Offsets{hand::ref(fbb, 1, "s$count"), hand::ref(fbb, 2, "s$mean"),
                                     hand::ref(fbb, 3, "s$m2")}
                           : Offsets{hand::ref(fbb, 1, "s$count")};
    auto node = aggregate(fbb, mode, {hand::ref(fbb, 0, "k")}, {"k"},
                          {func(fbb, "stddev", args, {"s$count", "s$mean", "s$m2"}, 1)});
    auto out =
        hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "s$count", "s$mean", "s$m2"})));
    EXPECT_EQ(out.num_rows(), 0) << fb::EnumNameAggregateMode(mode);
    ASSERT_EQ(out.num_columns(), 4) << fb::EnumNameAggregateMode(mode);
    EXPECT_EQ(out.view().column(1).type().id(), cudf::type_id::INT64);
    EXPECT_EQ(out.view().column(2).type().id(), cudf::type_id::FLOAT64);
    EXPECT_EQ(out.view().column(3).type().id(), cudf::type_id::FLOAT64);
  }
}

TEST(AggregateBuilder, AFunctionGivenTheWrongNumberOfArgsIsRefusedByName) {
  struct Case {
    const char* name;
    fb::AggregateMode mode;
    int args;
    int states;
  };
  for (auto const& bad : {Case{"sum", fb::AggregateMode_Partial, 2, 1},
                          Case{"count", fb::AggregateMode_Partial, 0, 1},
                          Case{"stddev", fb::AggregateMode_Merge, 1, 3},
                          Case{"var_pop", fb::AggregateMode_Partial, 3, 3}}) {
    flatbuffers::FlatBufferBuilder fbb;
    Offsets args;
    for (int i = 0; i < bad.args; ++i) args.push_back(hand::ref(fbb, 1, "v"));
    std::vector<std::string> state(bad.states, "s");
    auto node = aggregate(fbb, bad.mode, {hand::ref(fbb, 0, "k")}, {"k"},
                          {func(fbb, bad.name, args, state)});
    auto why = refusal(fbb, node, rows());
    EXPECT_NE(why.find(std::string("`") + bad.name + "`"), std::string::npos) << why;
    EXPECT_NE(why.find("args"), std::string::npos) << why;
  }
}

TEST(AggregateBuilder, AFunctionWhoseStateNamesDoNotMatchItsColumnsIsRefused) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                        {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s"}, 1)});
  auto why = refusal(fbb, node, rows());
  EXPECT_NE(why.find("`stddev` emits 3 state columns and names 1"), std::string::npos) << why;
}

// --- The Welford state is never NULL -------------------------------------------------------
//
// DataFusion's state for a group with no valid value is (0, 0.0, 0.0)
// (VarianceGroupsAccumulator::state). cuDF answers NULL there: MEAN and M2 over no valid
// value, and MERGE_M2 wherever every merged count is 0 (group_merge_m2.cu).

namespace {

flatbuffers::Offset<fb::AggregateFuncNode> merged_stddev(flatbuffers::FlatBufferBuilder& fbb,
                                                         uint32_t at) {
  return func(fbb, "stddev",
              {hand::ref(fbb, at, "s$count"), hand::ref(fbb, at + 1, "s$mean"),
               hand::ref(fbb, at + 2, "s$m2")},
              {"s$count", "s$mean", "s$m2"}, 1);
}

/// [k, s$count, s$mean, s$m2] from host values, one state per row.
peacock::TableResult states(std::vector<int32_t> k, std::vector<int64_t> n,
                            std::vector<double> mean, std::vector<double> m2) {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>(k));
  c.push_back(hand::column_of<int64_t>(n));
  c.push_back(hand::column_of<double>(mean));
  c.push_back(hand::column_of<double>(m2));
  return hand::table_of(std::move(c), {"k", "s$count", "s$mean", "s$m2"});
}

}  // namespace

TEST(AggregateBuilder, AWelfordInitOverAnAllNullGroupIsZeroNotNull) {
  // k 1 holds 10 and 20; k 2 holds only NULLs.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2, 2}));
  c.push_back(hand::nullable_column_of<int64_t>({10, 20, 0, 0}, {true, true, false, false}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "v"})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 0}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 15.0}, {2, 0.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 50.0}, {2, 0.0}}));
}

TEST(AggregateBuilder, AMergeOfOnlyEmptyStatesIsZeroNotNull) {
  // k 1: two lanes that held only NULLs for it; k 2: one real state.
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Merge, {hand::ref(fbb, 0, "k")}, {"k"},
                        {merged_stddev(fbb, 1)});
  auto out =
      hand::run(fbb, node, one(states({1, 1, 2}, {0, 0, 3}, {0.0, 0.0, 2.0}, {0.0, 0.0, 2.0})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 0}, {2, 3}}));
  EXPECT_EQ((by_key<double>(out, 2)), (std::map<int32_t, double>{{1, 0.0}, {2, 2.0}}));
  EXPECT_EQ((by_key<double>(out, 3)), (std::map<int32_t, double>{{1, 0.0}, {2, 2.0}}));
}

TEST(AggregateBuilder, ANaNAndANegativeZeroAreValuesNotNulls) {
  // k 1: 1.0 and NaN, whose moments are NaN in DataFusion's Welford too; k 2: one -0.0.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int32_t>({1, 1, 2}));
  c.push_back(hand::column_of<double>({1.0, std::nan(""), -0.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {hand::ref(fbb, 0, "k")}, {"k"},
                {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"k", "v"})));
  for (int at : {2, 3}) EXPECT_EQ(out.view().column(at).null_count(), 0) << out.column_names.at(at);
  EXPECT_EQ((by_key<int64_t>(out, 1)), (std::map<int32_t, int64_t>{{1, 2}, {2, 1}}))
      << "a NaN is counted";
  auto mean = by_key<double>(out, 2);
  auto m2 = by_key<double>(out, 3);
  EXPECT_TRUE(std::isnan(mean.at(1)) && std::isnan(m2.at(1))) << "NaN stays NaN, not 0.0";
  EXPECT_EQ(mean.at(2), 0.0) << "-0.0 or 0.0, a value";
  EXPECT_EQ(m2.at(2), 0.0);
}

TEST(AggregateBuilder, AKeylessNodeNamesItsColumnsFromItsStateNames) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                        {func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"}),
                         func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"total"})});
  auto out = hand::run(fbb, node, one(rows()));
  EXPECT_EQ(out.column_names, (std::vector<std::string>{"n", "total"}));
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{150});
}

// --- A keyless Welford (#216) ---------------------------------------------------------------
//
// cuDF has M2 and MERGE_M2 for groupby only: a keyless node holding a stddev or var runs whole
// through the grouped builder on one constant key, which it drops.

TEST(AggregateBuilder, AKeylessWelfordInitIsTheTripleOfAllItsRows) {
  // v: 10 20 30 40 50 — n 5, mean 30, m2 400 + 100 + 0 + 100 + 400.
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                {func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names, (std::vector<std::string>{"s$count", "s$mean", "s$m2"}))
      << "the constant key is dropped";
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<double>(out.view().column(1)), std::vector<double>{30.0});
  EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{1000.0});
}

TEST(AggregateBuilder, AKeylessVarMergeMergesEveryArrivingState) {
  // AWelfordMergeReadsTheTripleWhereItsArgsPoint's two states with no key, under `var`, which
  // the reduce path refused by name.
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int64_t>({2, 4}));
  c.push_back(hand::column_of<double>({1.5, 4.0}));
  c.push_back(hand::column_of<double>({2.0, 8.0}));
  flatbuffers::FlatBufferBuilder fbb;
  auto node = aggregate(
      fbb, fb::AggregateMode_Merge, {}, {},
      {func(fbb, "var",
            {hand::ref(fbb, 0, "s$count"), hand::ref(fbb, 1, "s$mean"), hand::ref(fbb, 2, "s$m2")},
            {"s$count", "s$mean", "s$m2"}, 1)});
  auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), {"s$count", "s$mean", "s$m2"})));
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{6});
  EXPECT_NEAR(hand::values_of<double>(out.view().column(1))[0], 19.0 / 6.0, 1e-12);
  EXPECT_NEAR(hand::values_of<double>(out.view().column(2))[0], 10.0 + 6.25 * 8.0 / 6.0, 1e-12);
}

TEST(AggregateBuilder, AKeylessWelfordCarriesTheFunctionsBesideIt) {
  flatbuffers::FlatBufferBuilder fbb;
  auto node =
      aggregate(fbb, fb::AggregateMode_Partial, {}, {},
                {func(fbb, "count", {hand::ref(fbb, 1, "v")}, {"n"}),
                 func(fbb, "stddev", {hand::ref(fbb, 1, "v")}, {"s$count", "s$mean", "s$m2"}, 1),
                 func(fbb, "sum", {hand::ref(fbb, 1, "v")}, {"total"})});
  auto out = hand::run(fbb, node, one(rows()));
  ASSERT_EQ(out.column_names,
            (std::vector<std::string>{"n", "s$count", "s$mean", "s$m2", "total"}));
  ASSERT_EQ(out.num_rows(), 1);
  EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{5});
  EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{30.0});
  EXPECT_EQ(out.view().column(4).type().id(), cudf::type_id::INT64);
  EXPECT_EQ(hand::values_of<int64_t>(out.view().column(4)), std::vector<int64_t>{150});
}

TEST(AggregateBuilder, AKeylessWelfordOverZeroRowsAnswersItsEmptyState) {
  // A done call's input. A groupby over no rows has no group, and a keyless node owes one
  // row, plan/aggregates.rs's empty_state: a `count` 0, the triple (0, 0.0, 0.0), the
  // decimal sum NULL at its scale. At Merge a count arrives as a `sum`, so it is NULL there.
  const cudf::data_type cents{cudf::type_id::DECIMAL128, -2};
  for (auto mode : {fb::AggregateMode_Partial, fb::AggregateMode_Merge}) {
    const bool merging = mode == fb::AggregateMode_Merge;
    flatbuffers::FlatBufferBuilder fbb;
    std::vector<std::unique_ptr<cudf::column>> c;
    std::vector<std::string> names;
    std::vector<flatbuffers::Offset<fb::AggregateFuncNode>> funcs;
    if (merging) {
      c.push_back(hand::column_of<int64_t>({}));
      c.push_back(hand::column_of<int64_t>({}));
      c.push_back(hand::column_of<double>({}));
      c.push_back(hand::column_of<double>({}));
      c.push_back(cudf::make_empty_column(cents));
      names = {"n", "s$count", "s$mean", "s$m2", "total"};
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 0, "n")}, {"n"}));
      funcs.push_back(merged_stddev(fbb, 1));
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 4, "total")}, {"total"}));
    } else {
      c.push_back(hand::column_of<double>({}));
      c.push_back(cudf::make_empty_column(cents));
      names = {"v", "amount"};
      funcs.push_back(func(fbb, "count", {hand::ref(fbb, 0, "v")}, {"n"}));
      funcs.push_back(
          func(fbb, "stddev", {hand::ref(fbb, 0, "v")}, {"s$count", "s$mean", "s$m2"}, 1));
      funcs.push_back(func(fbb, "sum", {hand::ref(fbb, 1, "amount")}, {"total"}));
    }
    auto node = aggregate(fbb, mode, {}, {}, funcs);
    auto out = hand::run(fbb, node, one(hand::table_of(std::move(c), names)));
    auto const* at = fb::EnumNameAggregateMode(mode);
    ASSERT_EQ(out.column_names,
              (std::vector<std::string>{"n", "s$count", "s$mean", "s$m2", "total"}))
        << at;
    ASSERT_EQ(out.num_rows(), 1) << at << ": a keyless node answers one row over nothing";
    EXPECT_EQ(out.view().column(0).type().id(), cudf::type_id::INT64) << at;
    EXPECT_EQ(out.view().column(0).null_count(), merging ? 1 : 0) << at;
    if (!merging)
      EXPECT_EQ(hand::values_of<int64_t>(out.view().column(0)), std::vector<int64_t>{0});
    for (int i = 1; i < 4; ++i) EXPECT_EQ(out.view().column(i).null_count(), 0) << at << " " << i;
    EXPECT_EQ(hand::values_of<int64_t>(out.view().column(1)), std::vector<int64_t>{0}) << at;
    EXPECT_EQ(hand::values_of<double>(out.view().column(2)), std::vector<double>{0.0}) << at;
    EXPECT_EQ(hand::values_of<double>(out.view().column(3)), std::vector<double>{0.0}) << at;
    EXPECT_EQ(out.view().column(4).type(), cents) << at << ": the scale is the sum's";
    EXPECT_EQ(out.view().column(4).null_count(), 1) << at;
  }
}
