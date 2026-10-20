#include <peacock_gpu.h>
#include "peacock/operators.h"
#include "plan_executor.h"
#include "plan_executor_internal.h"

#include <cudf/version_config.hpp>
#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <cstdlib>
#include <fstream>
#include <functional>
#include <sstream>
#include <string>
#include <vector>

TEST(PeacockGpu, Version) {
  EXPECT_STREQ(peacock_gpu_version(), "0.1.0");
}

// Host-only unit tests for the decimal-scale / AST-routing helpers: they run on the
// non-GPU CI tiers (every push), where the GPU result tests do not, so a regression
// in these pure rules cannot slip through.

namespace {
namespace fb = peacock::plan;

cudf::data_type dec(int32_t scale) {
  return cudf::data_type{cudf::type_id::DECIMAL128, scale};
}
}  // namespace

// binop_output_type: predicates → BOOL8; fixed_point arithmetic result scale
// must follow DataFusion (cuDF scale = base-10 exponent, negative for
// fractional). A MUL/DIV swap here is exactly the regression this guards.
TEST(DecimalScale, BinopOutputType) {
  // Comparisons / logical ops → boolean, regardless of operand types.
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Gt, dec(-2), dec(-2)).id(),
            cudf::type_id::BOOL8);
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Eq, dec(-4), dec(-2)).id(),
            cudf::type_id::BOOL8);

  // ADD / SUB: result scale = min(exponent) = max fractional digits.
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Plus, dec(-2), dec(-4)).scale(), -4);
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Minus, dec(-4), dec(-2)).scale(), -4);

  // MUL: exponents add.
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Multiply, dec(-2), dec(-3)).scale(), -5);

  // DIV: exponents subtract.
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Divide, dec(-6), dec(-2)).scale(), -4);

  // Decimal arithmetic stays DECIMAL128.
  EXPECT_EQ(peacock::binop_output_type(fb::BinaryOp_Multiply, dec(-2), dec(-3)).id(),
            cudf::type_id::DECIMAL128);
}

namespace {

// Build a standalone fb::Expr (column ref) and return the finished buffer.
flatbuffers::DetachedBuffer make_column_ref(flatbuffers::FlatBufferBuilder& b,
                                            uint32_t index) {
  auto cr = fb::CreateColumnRef(b, index, b.CreateString("c"));
  auto e = fb::CreateExpr(b, fb::ExprNode_ColumnRef, cr.Union());
  b.Finish(e);
  return b.Release();
}

// Build `left <op> right` over two column refs at indices 0 and 1.
flatbuffers::DetachedBuffer make_binary(flatbuffers::FlatBufferBuilder& b,
                                        fb::BinaryOp op) {
  auto l_cr = fb::CreateColumnRef(b, 0, b.CreateString("l"));
  auto l = fb::CreateExpr(b, fb::ExprNode_ColumnRef, l_cr.Union());
  auto r_cr = fb::CreateColumnRef(b, 1, b.CreateString("r"));
  auto r = fb::CreateExpr(b, fb::ExprNode_ColumnRef, r_cr.Union());
  auto bin = fb::CreateBinaryExprNode(b, l, op, r);
  auto e = fb::CreateExpr(b, fb::ExprNode_BinaryExprNode, bin.Union());
  b.Finish(e);
  return b.Release();
}

// A 0-row column_view carrying only a type (enough for type inference).
cudf::column_view typed_col(cudf::data_type dt) {
  return cudf::column_view{dt, 0, nullptr, nullptr, 0};
}

// A standalone ColumnRef table; `name` null writes none.
const fb::ColumnRef* finished_ref(flatbuffers::FlatBufferBuilder& b, uint32_t index,
                                  const char* name) {
  auto text = name ? b.CreateString(name) : flatbuffers::Offset<flatbuffers::String>{};
  b.Finish(fb::CreateColumnRef(b, index, text));
  return flatbuffers::GetRoot<fb::ColumnRef>(b.GetBufferPointer());
}

}  // namespace

// cudf_ast_can_evaluate: same-type non-decimal operands fuse via AST; any decimal operand
// or a type mismatch routes to the column path.
TEST(AstRouting, CudfAstCanEvaluate) {
  // int32 < int32  →  AST-able.
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_binary(b, fb::BinaryOp_Lt);
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                        typed_col(cudf::data_type{cudf::type_id::INT32})};
    EXPECT_TRUE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"l", "r"}));
  }
  // int32 < int64  →  type mismatch, column path.
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_binary(b, fb::BinaryOp_Lt);
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                        typed_col(cudf::data_type{cudf::type_id::INT64})};
    EXPECT_FALSE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"l", "r"}));
  }
  // decimal * decimal  →  decimal operand, column path.
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_binary(b, fb::BinaryOp_Multiply);
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(dec(-2)), typed_col(dec(-2))};
    EXPECT_FALSE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"l", "r"}));
  }
  // Bare column ref is trivially AST-able.
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_column_ref(b, 0);
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32})};
    EXPECT_TRUE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"c"}));
  }
}

// #164: a ColumnRef is resolved by index AND name, as the planner's own check does
// (expr_physical.rs); each wrong reference has its own message.
TEST(ColumnRefs, TheNamedColumnResolves) {
  flatbuffers::FlatBufferBuilder b;
  EXPECT_EQ(peacock::column_ordinal(finished_ref(b, 1, "m"), {"n", "m"}), 1);
}

TEST(ColumnRefs, AnIndexPastTheWidthIsRefusedNamingBoth) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 2, "n"), {"n", "m"});
    FAIL() << "an index past the width resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "ColumnRef n@2 is past the 2 columns its input has");
  }
}

TEST(ColumnRefs, AReferenceWithNoNameIsRefused) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 0, nullptr), {"n", "m"});
    FAIL() << "an unnamed reference resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(),
                 "ColumnRef @0 carries no name, so nothing checks it reads the column the plan "
                 "meant");
  }
}

TEST(ColumnRefs, ANameOtherThanTheColumnsIsRefusedNamingBoth) {
  flatbuffers::FlatBufferBuilder b;
  try {
    peacock::column_ordinal(finished_ref(b, 0, "m"), {"n", "m"});
    FAIL() << "a reference named for another column resolved";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "ColumnRef m@0 reads n at that position");
  }
}

TEST(ColumnRefs, ColumnAtBorrowsTheColumnTheRefNames) {
  flatbuffers::FlatBufferBuilder b;
  std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                      typed_col(cudf::data_type{cudf::type_id::INT64})};
  auto got = peacock::column_at(cudf::table_view{cols}, {"n", "m"}, finished_ref(b, 1, "m"));
  EXPECT_EQ(got.type().id(), cudf::type_id::INT64);
}

TEST(ColumnRefs, TheAstRouteResolvesAReferenceUnderAnOperator) {
  // make_binary writes l@0 < r@1; the names here say r is at 0.
  flatbuffers::FlatBufferBuilder b;
  auto buf = make_binary(b, fb::BinaryOp_Lt);
  auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
  std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32}),
                                      typed_col(cudf::data_type{cudf::type_id::INT32})};
  EXPECT_THROW(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"r", "l"}),
               std::runtime_error);
  EXPECT_TRUE(peacock::cudf_ast_can_evaluate(expr, cudf::table_view{cols}, {"l", "r"}));
}

namespace {

// `wrap` builds an expression around a ColumnRef Expr. No shape here passes through
// infer_expr_type, so the ColumnRef arm's check is the only one before build_expr's AST arm.
using Wrap = std::function<flatbuffers::Offset<fb::Expr>(flatbuffers::FlatBufferBuilder&,
                                                         flatbuffers::Offset<fb::Expr>)>;

void expect_each_bad_ref_refused(Wrap const& wrap) {
  struct Bad {
    uint32_t index;
    const char* name;
    const char* message;
  };
  const Bad bads[] = {
      {0, "c", "ColumnRef c@0 reads x at that position"},
      {0, nullptr,
       "ColumnRef @0 carries no name, so nothing checks it reads the column the plan meant"},
      {1, "x", "ColumnRef x@1 is past the 1 columns its input has"},
  };
  std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32})};
  auto evaluate = [&](uint32_t index, const char* name) {
    flatbuffers::FlatBufferBuilder b;
    auto text = name ? b.CreateString(name) : flatbuffers::Offset<flatbuffers::String>{};
    auto ref =
        fb::CreateExpr(b, fb::ExprNode_ColumnRef, fb::CreateColumnRef(b, index, text).Union());
    b.Finish(wrap(b, ref));
    return peacock::cudf_ast_can_evaluate(flatbuffers::GetRoot<fb::Expr>(b.GetBufferPointer()),
                                          cudf::table_view{cols}, {"x"});
  };
  for (auto const& bad : bads) {
    try {
      evaluate(bad.index, bad.name);
      ADD_FAILURE() << "resolved: " << bad.message;
    } catch (std::runtime_error const& e) {
      EXPECT_STREQ(e.what(), bad.message);
    }
  }
  EXPECT_TRUE(evaluate(0, "x"));
}

}  // namespace

TEST(ColumnRefs, TheAstRouteResolvesABareReference) {
  expect_each_bad_ref_refused([](auto&, auto ref) { return ref; });
}

TEST(ColumnRefs, TheAstRouteResolvesAReferenceUnderACast) {
  expect_each_bad_ref_refused([](flatbuffers::FlatBufferBuilder& b, auto ref) {
    auto cast = fb::CreateCastExprNode(b, ref, fb::DataType_Int64);
    return fb::CreateExpr(b, fb::ExprNode_CastExprNode, cast.Union());
  });
}

TEST(ColumnRefs, TheAstRouteResolvesAReferenceUnderAUnaryOperator) {
  expect_each_bad_ref_refused([](flatbuffers::FlatBufferBuilder& b, auto ref) {
    auto neg = fb::CreateUnaryExprNode(b, fb::UnaryOp_Negative, ref);
    return fb::CreateExpr(b, fb::ExprNode_UnaryExprNode, neg.Union());
  });
}

// #164: a handle's owners, columns and names are one list read by one ordinal, so a
// TableResult assembled from parts is refused unless the three agree, and refused empty.
TEST(TableResultInvariant, PartsOfDifferentLengthsAreRefused) {
  auto int64 = typed_col(cudf::data_type{cudf::type_id::INT64});
  try {
    peacock::TableResult::of({nullptr, nullptr}, {int64, int64}, {"a"});
    FAIL() << "two columns took one name";
  } catch (std::runtime_error const& e) {
    EXPECT_STREQ(e.what(), "TableResult: 2 columns, 2 owners and 1 names");
  }
}

TEST(TableResultInvariant, NoColumnsAreRefused) {
  EXPECT_THROW(peacock::TableResult::of({}, {}, {}), std::runtime_error);
}

// The planner writes Partial and Merge only; an aggregate finalizes in a project. A Final or
// Single mode is a hand-built plan the arms that served it no longer exist for, so it is refused
// by name before any input is read — which is why this runs with no device.
TEST(AggregatePhase, FinalAndSingleAreRefusedNamingTheMode) {
  for (auto mode : {fb::AggregateMode_Final, fb::AggregateMode_FinalPartitioned,
                    fb::AggregateMode_Single, fb::AggregateMode_SinglePartitioned}) {
    flatbuffers::FlatBufferBuilder b;
    // An input schema, so the no-input arm has something to build from if the mode is not
    // refused first.
    auto field = fb::CreateField(b, b.CreateString("v"), fb::DataType_Int64, true);
    auto schema =
        fb::CreateSchema(b, b.CreateVector(std::vector<flatbuffers::Offset<fb::Field>>{field}));
    fb::CudfAggregateBuilder agg(b);
    agg.add_mode(mode);
    agg.add_aggr_input_schema(schema);
    auto node = fb::CreatePlanNode(b, fb::PlanNodeKind_CudfAggregate, agg.Finish().Union());
    b.Finish(fb::CreateGpuPlan(b, node));
    try {
      peacock::execute_one(fb::GetGpuPlan(b.GetBufferPointer())->root(), {});
      ADD_FAILURE() << fb::EnumNameAggregateMode(mode) << " was not refused";
    } catch (std::runtime_error const& e) {
      EXPECT_NE(std::string(e.what()).find(std::string("mode ") + fb::EnumNameAggregateMode(mode)),
                std::string::npos)
          << e.what();
    }
  }
}

TEST(PeacockGpu, ExecutorCreateDestroy) {
  peacock_executor_t* ex = nullptr;
  ASSERT_EQ(peacock_executor_create(/*gpu_memory_limit=*/0, &ex), 0);
  ASSERT_NE(ex, nullptr);
  peacock_executor_destroy(ex);
}

TEST(PeacockGpu, ExecutorNullOut) {
  EXPECT_NE(peacock_executor_create(0, nullptr), 0);
}

// The harness's upload refuses before it reads either Arrow struct: a null output, and
// an executor with no plan loaded, since the registry it adopts into is the session's.
TEST(PeacockGpu, HandleFromArrowNeedsASession) {
  peacock_executor_t* ex = nullptr;
  ASSERT_EQ(peacock_executor_create(0, &ex), 0);
  int schema = 0, array = 0;
  uint64_t handle = 0;
  EXPECT_NE(peacock_handle_from_arrow(ex, &schema, &array, nullptr), 0);
  EXPECT_NE(peacock_handle_from_arrow(ex, &schema, &array, &handle), 0);
  EXPECT_STREQ(peacock_last_error(ex), "no plan loaded (call peacock_executor_begin_plan first)");
  peacock_executor_destroy(ex);
}

// The timing switch, on the tier that has no GPU because it needs none: it is a
// process-global mode, and the whole safety argument for the benchmark work is that the
// correctness suite never leaves it on. Stuck on, it would leak a CUDA event pair per
// region until the session ends, and nothing would fail.
TEST(NodeTiming, DefaultsOff) {
  EXPECT_EQ(peacock::node_timing(), peacock::NodeTiming::Off);
  EXPECT_FALSE(peacock::node_timing_enabled());
}

TEST(NodeTiming, SwitchRoundTrips) {
  peacock::set_node_timing(peacock::NodeTiming::Events);
  EXPECT_EQ(peacock::node_timing(), peacock::NodeTiming::Events);
  EXPECT_TRUE(peacock::node_timing_enabled());
  peacock::set_node_timing(peacock::NodeTiming::Off);
  EXPECT_EQ(peacock::node_timing(), peacock::NodeTiming::Off);
  EXPECT_FALSE(peacock::node_timing_enabled());
}

// The ABI takes an int, so a caller can name a mode this build does not have. Refusing
// it is what makes that visible: read as off, the caller measures nothing and the run
// reports zeros it has no reason to doubt.
TEST(NodeTiming, TheAbiRefusesAModeItDoesNotName) {
  EXPECT_EQ(peacock_set_node_timing(PEACOCK_NODE_TIMING_EVENTS), 0);
  EXPECT_NE(peacock_set_node_timing(7), 0);
  EXPECT_EQ(peacock::node_timing(), peacock::NodeTiming::Events) << "a refused mode changed it";
  EXPECT_EQ(peacock_set_node_timing(PEACOCK_NODE_TIMING_OFF), 0);
  EXPECT_FALSE(peacock::node_timing_enabled());
}

// The collection's contract has two halves — (NULL, 0) asks the count, a buffer of at least
// that many takes the regions — and one shape that is neither: a null buffer with a
// capacity. Refused before the session is consulted, so nothing is drained into it.
TEST(NodeRegions, ANullBufferWithACapacityIsRefused) {
  peacock_executor_t* ex = nullptr;
  ASSERT_EQ(peacock_executor_create(0, &ex), 0);
  uint64_t count = 7;
  EXPECT_NE(peacock_executor_collect_node_regions(ex, nullptr, 4, &count), 0);
  EXPECT_STREQ(peacock_last_error(ex),
               "collect_node_regions: a null buffer with capacity 4 — (NULL, 0) asks the count");
  peacock_executor_destroy(ex);
}

// The harness range is one level: a case does not nest inside a case, so a second push
// replaces the first. A stack instead would leave the outer one open after this pop, and
// every later case would be captured inside a query it did not belong to. NVTX touches no
// device, which is why this sits on the cpu tier.
TEST(NvtxRanges, ASecondPushReplacesTheFirstRatherThanNesting) {
  // No guard yet: the switch is off by default, and a push while it is off is a no-op —
  // which is what lets the harness call this unconditionally.
  peacock::push_harness_range("tpch.sf40 q6 tp1-single");
  EXPECT_FALSE(peacock::harness_range_is_open());

  struct RangesOn {
    RangesOn() { peacock::set_nvtx_ranges(true); }
    ~RangesOn() { peacock::set_nvtx_ranges(false); }
  } ranges;
  peacock::push_harness_range("tpch.sf40 q6 tp1-single");
  EXPECT_TRUE(peacock::harness_range_is_open());
  peacock::push_harness_range("tpch.sf40 q19 tp1-single");
  peacock::pop_harness_range();
  EXPECT_FALSE(peacock::harness_range_is_open());
}

namespace {

std::string testdata_dir() {
  const char* env = std::getenv("PEACOCK_TESTDATA_DIR");
  return env ? std::string(env) : std::string(PEACOCK_TESTDATA_DIR);
}

struct ClampCase {
  int line;
  uint64_t offset, length, rows, begin, end;
};

// Digits only, as the Rust reader takes them: std::stoull alone would take "-1" (wrapping to
// UINT64_MAX), "+5" and leading spaces, and throw without naming the line.
uint64_t clamp_count(const std::string& field, int line) {
  const bool digits = !field.empty() && field.find_first_not_of("0123456789") == std::string::npos;
  if (!digits) {
    ADD_FAILURE() << "row-range-clamp.txt:" << line << ": `" << field << "` is not a count";
    return 0;
  }
  try {
    return std::stoull(field);
  } catch (const std::out_of_range&) {
    ADD_FAILURE() << "row-range-clamp.txt:" << line << ": `" << field << "` does not fit a u64";
    return 0;
  }
}

// `max` is the to-the-end sentinel, allowed in offset and length only.
uint64_t clamp_bound(const std::string& field, int line) {
  return field == "max" ? UINT64_MAX : clamp_count(field, line);
}

std::vector<ClampCase> clamp_cases() {
  const std::string path = testdata_dir() + "/fixtures/row-range-clamp.txt";
  std::ifstream in(path);
  EXPECT_TRUE(in.is_open()) << "cannot read " << path;
  std::vector<ClampCase> cases;
  std::string text;
  for (int line = 1; std::getline(in, text); ++line) {
    std::istringstream fields(text);
    std::vector<std::string> f;
    for (std::string w; fields >> w;) f.push_back(w);
    if (f.empty() || f[0][0] == '#') continue;
    if (f.size() != 6 || f[3] != "->") {
      ADD_FAILURE() << "row-range-clamp.txt:" << line
                    << ": expected `offset length rows -> begin end`, got `" << text << "`";
      continue;
    }
    cases.push_back({line, clamp_bound(f[0], line), clamp_bound(f[1], line),
                     clamp_count(f[2], line), clamp_count(f[4], line), clamp_count(f[5], line)});
  }
  EXPECT_FALSE(cases.empty()) << path << " holds no case";
  return cases;
}

}  // namespace

// The row range every per-batch caller's bounds go through, on the tier that needs no
// device: it is arithmetic, and reaching it through a scan on the GPU host is the long
// way round to a case that cannot fail there for a reason worth knowing.
TEST(ClampRowRange, AnswersEveryCaseInTheSharedTable) {
  for (const auto& c : clamp_cases()) {
    const auto rows = static_cast<cudf::size_type>(c.rows);
    EXPECT_EQ(
        peacock::clamp_row_range(c.offset, c.length, rows),
        std::make_pair(static_cast<cudf::size_type>(c.begin), static_cast<cudf::size_type>(c.end)))
        << "row-range-clamp.txt:" << c.line;
  }
}

// #94: MERGE_M2 takes its count child as INT32 before cuDF 25.06 and as INT64 from 25.06
// (cuDF #18546).
TEST(MergeM2CountType, Int32Before25_06AndInt64From25_06) {
  EXPECT_EQ(peacock::merge_m2_count_type(25, 2), cudf::type_id::INT32) << "25.02, the GPU hosts'";
  EXPECT_EQ(peacock::merge_m2_count_type(25, 4), cudf::type_id::INT32);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 6), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(25, 10), cudf::type_id::INT64);
  EXPECT_EQ(peacock::merge_m2_count_type(26, 2), cudf::type_id::INT64) << "26.02";
}

// The type the aggregate casts to, as this build's cuDF headers choose it. 25.02 is the GPU
// hosts'; CI's other leg compiles INT64.
TEST(MergeM2CountType, TheLinkedCudfChoosesIt) {
#if CUDF_VERSION_MAJOR == 25 && CUDF_VERSION_MINOR == 2
  EXPECT_EQ(peacock::kMergeM2CountType, cudf::type_id::INT32) << "cuDF 25.02";
#else
  EXPECT_EQ(peacock::kMergeM2CountType, cudf::type_id::INT64)
      << "cuDF " << CUDF_VERSION_MAJOR << "." << CUDF_VERSION_MINOR;
#endif
}

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
