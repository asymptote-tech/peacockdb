/// #164: a handle's columns and names are one list read by one ordinal, so every TableResult is
/// built through a constructor that refuses a mismatch, and every ColumnRef reader checks the
/// reference's bounds and name.

#include "hand_tables.hpp"

#include <gtest/gtest.h>

#include <stdexcept>
#include <string>

TEST(TableResultInvariant, ThreeColumnsAndTwoNamesAreRefused) {
  std::vector<std::unique_ptr<cudf::column>> columns;
  for (int i = 0; i < 3; ++i) columns.push_back(hand::column_of<int64_t>({1, 2}));
  try {
    hand::table_of(std::move(columns), {"a", "b"});
    FAIL() << "a table of three columns took two names";
  } catch (std::runtime_error const& e) {
    std::string what = e.what();
    EXPECT_NE(what.find('3'), std::string::npos) << what;
    EXPECT_NE(what.find('2'), std::string::npos) << what;
  }
}

namespace {
namespace fb = peacock::plan;

/// [n, m], both INT64.
peacock::TableResult nm() {
  std::vector<std::unique_ptr<cudf::column>> c;
  c.push_back(hand::column_of<int64_t>({3, 1, 2}));
  c.push_back(hand::column_of<int64_t>({30, 10, 20}));
  return hand::table_of(std::move(c), {"n", "m"});
}

struct BadRef {
  uint32_t index;
  const char* name;
  const char* says;
};
const BadRef kBadRefs[] = {
    {2, "n", "ColumnRef n@2 is past the 2 columns its input has"},
    {0, nullptr, "ColumnRef @0 carries no name"},
    {0, "m", "ColumnRef m@0 reads n at that position"},
};

/// A sort of nm() on `key`: what it refused with, or "(not refused)".
std::string sort_refusal(flatbuffers::FlatBufferBuilder& fbb, flatbuffers::Offset<fb::Expr> key) {
  auto spec = fb::CreateSortExprNode(fbb, key, /*asc=*/true, /*nulls_first=*/false);
  auto specs = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::SortExprNode>>{spec});
  fb::CudfSortBuilder b(fbb);
  b.add_exprs(specs);
  b.add_fetch(-1);
  auto node = fb::CreatePlanNode(fbb, fb::PlanNodeKind_CudfSort, b.Finish().Union());
  std::vector<peacock::TableResult> in;
  in.push_back(nm());
  try {
    hand::run(fbb, node, std::move(in));
  } catch (std::runtime_error const& e) {
    return e.what();
  }
  return "(not refused)";
}
}  // namespace

TEST(ColumnRefs, ASortKeyRefusesEachBadReference) {
  for (auto const& bad : kBadRefs) {
    flatbuffers::FlatBufferBuilder fbb;
    auto why = sort_refusal(fbb, hand::ref(fbb, bad.index, bad.name));
    EXPECT_NE(why.find(bad.says), std::string::npos) << why;
  }
}

TEST(ColumnRefs, AComputedKeyRefusesEachBadReferenceInsideIt) {
  // CAST(ref AS FLOAT64): not a bare ColumnRef, so the column path evaluates it, and the
  // reference inside resolves through the same check.
  for (auto const& bad : kBadRefs) {
    flatbuffers::FlatBufferBuilder fbb;
    auto inner = hand::ref(fbb, bad.index, bad.name);
    auto cast = fb::CreateCastExprNode(fbb, inner, fb::DataType_Float64);
    auto key = fb::CreateExpr(fbb, fb::ExprNode_CastExprNode, cast.Union());
    auto why = sort_refusal(fbb, key);
    EXPECT_NE(why.find(bad.says), std::string::npos) << why;
  }
}

TEST(ColumnRefs, AWellNamedSortKeyStillSorts) {
  flatbuffers::FlatBufferBuilder fbb;
  auto why = sort_refusal(fbb, hand::ref(fbb, 1, "m"));
  EXPECT_EQ(why, "(not refused)");
}
