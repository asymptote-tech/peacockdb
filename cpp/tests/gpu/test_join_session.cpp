// The join session through its C ABI (join-rewrite-design.md §5.6). Every answer is
// hand-counted: tables of a few rows, rows compared as sorted text.
#include "peacock_gpu.h"
#include "peacock/rmm_pool.hpp"
#include "plan_executor_internal.h"

#if __has_include(<cudf/join/hash_join.hpp>)
#include <cudf/join/hash_join.hpp>
#else
#include <cudf/join.hpp>
#endif
#include <cudf/copying.hpp>
#include <cudf/filling.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/table/table.hpp>
#include <cudf/table/table_view.hpp>
#include "generated/gpu_plan_generated.h"

#include <arrow/api.h>
#include <arrow/c/bridge.h>
#include <arrow/io/memory.h>
#include <arrow/ipc/reader.h>
#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <iostream>
#include <limits>
#include <functional>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace fb = peacock::plan;
using Rows = std::vector<std::string>;

struct Col {  // one Arrow column with its name
  std::string name;
  std::shared_ptr<arrow::Array> array;
};

template <typename Builder, typename T>
static std::shared_ptr<arrow::Array> build(std::vector<std::optional<T>> const& v, Builder b) {
  for (auto const& x : v) (x ? b.Append(*x) : b.AppendNull()).ok();
  std::shared_ptr<arrow::Array> out;
  EXPECT_TRUE(b.Finish(&out).ok());
  return out;
}
static Col i32(std::string n, std::vector<std::optional<int32_t>> v) {
  return {std::move(n), build(v, arrow::Int32Builder())};
}
static Col i64(std::string n, std::vector<std::optional<int64_t>> v) {
  return {std::move(n), build(v, arrow::Int64Builder())};
}
static Col utf8(std::string n, std::vector<std::optional<std::string>> v) {
  return {std::move(n), build(v, arrow::StringBuilder())};
}
static Col boolean(std::string n, std::vector<std::optional<bool>> v) {
  return {std::move(n), build(v, arrow::BooleanBuilder())};
}
static Col date32(std::string n, std::vector<std::optional<int32_t>> v) {
  return {std::move(n), build(v, arrow::Date32Builder())};
}
static Col dec(std::string n, int p, int s, std::vector<std::optional<int64_t>> unscaled) {
  arrow::Decimal128Builder b(arrow::decimal128(p, s));
  for (auto const& x : unscaled) (x ? b.Append(arrow::Decimal128(*x)) : b.AppendNull()).ok();
  std::shared_ptr<arrow::Array> out;
  EXPECT_TRUE(b.Finish(&out).ok());
  return {std::move(n), out};
}
static Col ts_us(std::string n, std::vector<std::optional<int64_t>> v) {
  return {std::move(n), build(v, arrow::TimestampBuilder(arrow::timestamp(arrow::TimeUnit::MICRO),
                                                         arrow::default_memory_pool()))};
}

/// An executor with the one-node plan loaded; uploads and reads answers through the ABI.
class Session {
 public:
  explicit Session(std::vector<uint8_t> plan) : plan_(std::move(plan)) {
    EXPECT_EQ(peacock_executor_create(0, &exec_), 0);
    uint64_t n = 0;
    EXPECT_EQ(peacock_executor_begin_plan(exec_, plan_.data(), plan_.size(), &n), 0) << error();
  }
  ~Session() { peacock_executor_destroy(exec_); }
  Session(const Session&) = delete;
  Session& operator=(const Session&) = delete;
  std::string error() const { return peacock_last_error(exec_); }
  peacock_executor_t* exec() { return exec_; }

  uint64_t upload(std::vector<Col> const& cols) {
    std::vector<std::shared_ptr<arrow::Field>> fields;
    std::vector<std::shared_ptr<arrow::Array>> arrays;
    for (auto const& c : cols) {
      fields.push_back(arrow::field(c.name, c.array->type()));
      arrays.push_back(c.array);
    }
    auto batch = arrow::RecordBatch::Make(arrow::schema(fields),
                                          cols.empty() ? 0 : cols[0].array->length(), arrays);
    auto strukt = batch->ToStructArray().ValueOrDie();
    ArrowArray c_array;
    ArrowSchema c_schema;
    EXPECT_TRUE(arrow::ExportArray(*strukt, &c_array, &c_schema).ok());
    uint64_t h = 0;
    EXPECT_EQ(peacock_handle_from_arrow(exec_, &c_schema, &c_array, &h), 0) << error();
    return h;
  }

  /// The handle's rows as `a|b|NULL` lines, sorted — every join answer is a multiset.
  Rows rows(uint64_t handle) {
    uint8_t* ipc = nullptr;
    uint64_t len = 0;
    EXPECT_EQ(peacock_result_from_handle(exec_, handle, 0, UINT64_MAX, nullptr, 0, &ipc, &len), 0)
        << error();
    Rows out;
    if (len == 0) return out;
    for (auto const& b : batches(ipc, len)) {
      for (int64_t r = 0; r < b->num_rows(); ++r) {
        std::string line;
        for (int c = 0; c < b->num_columns(); ++c) {
          auto s = b->column(c)->GetScalar(r).ValueOrDie();
          line += (c ? "|" : "") + (s->is_valid ? s->ToString() : std::string("NULL"));
        }
        out.push_back(line);
      }
    }
    peacock_result_free(ipc);
    std::sort(out.begin(), out.end());
    return out;
  }

  /// The handle's Arrow schema as the export writes it — what a pad's type is read from.
  std::shared_ptr<arrow::Schema> schema(uint64_t handle) {
    uint8_t* ipc = nullptr;
    uint64_t len = 0;
    EXPECT_EQ(peacock_handle_schema(exec_, handle, &ipc, &len), 0) << error();
    auto buf = std::make_shared<arrow::Buffer>(ipc, static_cast<int64_t>(len));
    auto reader =
        arrow::ipc::RecordBatchStreamReader::Open(std::make_shared<arrow::io::BufferReader>(buf))
            .ValueOrDie();
    auto out = reader->schema();
    peacock_result_free(ipc);
    return out;
  }

 private:
  static std::vector<std::shared_ptr<arrow::RecordBatch>> batches(uint8_t* ipc, uint64_t len) {
    auto buf = std::make_shared<arrow::Buffer>(ipc, static_cast<int64_t>(len));
    auto reader =
        arrow::ipc::RecordBatchStreamReader::Open(std::make_shared<arrow::io::BufferReader>(buf))
            .ValueOrDie();
    std::vector<std::shared_ptr<arrow::RecordBatch>> out;
    std::shared_ptr<arrow::RecordBatch> b;
    while (reader->ReadNext(&b).ok() && b) out.push_back(b);
    return out;
  }

  std::vector<uint8_t> plan_;
  peacock_executor_t* exec_ = nullptr;
};

/// The three types that answer no table per probe and everything at finish.
static bool is_build_side_semi(fb::JoinType t) {
  return t == fb::JoinType_LeftSemi || t == fb::JoinType_LeftAnti || t == fb::JoinType_LeftMark;
}

/// Two schemas' names and types, which is what a pad must match. Not `Schema::Equals`,
/// which also compares nullability: a pad is null in every row and so is nullable, while
/// the matched rows it stands beside need not be.
static void expect_same_columns(std::shared_ptr<arrow::Schema> const& got,
                                std::shared_ptr<arrow::Schema> const& want) {
  ASSERT_EQ(got->num_fields(), want->num_fields()) << got->ToString();
  for (int i = 0; i < want->num_fields(); ++i) {
    EXPECT_EQ(got->field(i)->name(), want->field(i)->name());
    EXPECT_TRUE(got->field(i)->type()->Equals(*want->field(i)->type()))
        << got->field(i)->ToString() << " beside " << want->field(i)->ToString();
  }
}

static Rows sorted(Rows r) {
  std::sort(r.begin(), r.end());
  return r;
}

/// What `fn` allocated: in total, at its peak above where it started, and still held after.
struct Allocated {
  int64_t total = 0, peak = 0, net = 0;
};

template <class F>
static Allocated allocated_by(F&& fn) {
  auto& mr = peacock::stats_mr();
  if (!mr) {
    ADD_FAILURE() << "main() installs the pool and its statistics adaptor";
    return {};
  }
  mr->push_counters();
  fn();
  auto [bytes, calls] = mr->pop_counters();
  return {bytes.total, bytes.peak, bytes.value};
}

/// One field of a side's schema: the pad types a join answers with before a batch arrived.
struct SField {
  std::string name;
  fb::DataType type;
  uint8_t decimal_precision = 0;
  int8_t decimal_scale = 0;
};

/// What a `CudfJoin` says; `make_plan` serializes it as the plan's one node.
struct JoinSpec {
  fb::JoinType type = fb::JoinType_Inner;
  std::vector<std::pair<uint32_t, uint32_t>> keys;  // (build col, probe col)
  std::function<flatbuffers::Offset<fb::Expr>(flatbuffers::FlatBufferBuilder&)> filter;
  std::vector<std::pair<fb::JoinSide, uint32_t>> filter_columns;
  bool null_equals_null = false;
  std::vector<SField> build_schema, probe_schema;
  std::optional<std::vector<uint32_t>> projection;
  uint64_t chunk_bytes = 0;
};

static flatbuffers::Offset<fb::Expr> col(flatbuffers::FlatBufferBuilder& f, uint32_t i) {
  return fb::CreateExpr(f, fb::ExprNode_ColumnRef, fb::CreateColumnRef(f, i).Union());
}
static flatbuffers::Offset<fb::Expr> bin(flatbuffers::FlatBufferBuilder& f,
                                         flatbuffers::Offset<fb::Expr> l, fb::BinaryOp op,
                                         flatbuffers::Offset<fb::Expr> r) {
  return fb::CreateExpr(f, fb::ExprNode_BinaryExprNode,
                        fb::CreateBinaryExprNode(f, l, op, r).Union());
}
static flatbuffers::Offset<fb::Expr> lit_bool(flatbuffers::FlatBufferBuilder& f, bool v) {
  fb::ScalarValueBuilder sb(f);
  sb.add_type(fb::DataType_Boolean);
  sb.add_bool_val(v);
  auto sv = sb.Finish();
  return fb::CreateExpr(f, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(f, sv).Union());
}

static flatbuffers::Offset<fb::Schema> schema_of(flatbuffers::FlatBufferBuilder& f,
                                                 std::vector<SField> const& fields) {
  std::vector<flatbuffers::Offset<fb::Field>> out;
  for (auto const& sf : fields)
    out.push_back(fb::CreateField(f, f.CreateString(sf.name), sf.type, true, sf.decimal_precision,
                                  sf.decimal_scale));
  return fb::CreateSchema(f, f.CreateVector(out));
}

static std::vector<uint8_t> make_plan(JoinSpec const& s) {
  flatbuffers::FlatBufferBuilder f;
  std::vector<flatbuffers::Offset<fb::JoinKey>> keys;
  for (auto [b, p] : s.keys) keys.push_back(fb::CreateJoinKey(f, col(f, b), col(f, p)));
  auto filter = s.filter ? s.filter(f) : flatbuffers::Offset<fb::Expr>{};
  std::vector<fb::JoinFilterColumn> fcs;
  for (auto [side, i] : s.filter_columns) fcs.emplace_back(i, side);
  auto build = schema_of(f, s.build_schema), probe = schema_of(f, s.probe_schema);
  auto proj = s.projection ? f.CreateVector(*s.projection)
                           : flatbuffers::Offset<flatbuffers::Vector<uint32_t>>{};
  auto keys_v = keys.empty()
                    ? flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<fb::JoinKey>>>{}
                    : f.CreateVector(keys);
  auto fcs_v = fcs.empty() ? flatbuffers::Offset<flatbuffers::Vector<const fb::JoinFilterColumn*>>{}
                           : f.CreateVectorOfStructs(fcs);
  auto join = fb::CreateCudfJoin(f, s.type, keys_v, filter, fcs_v, s.null_equals_null, build, probe,
                                 proj, s.chunk_bytes);
  auto node = fb::CreatePlanNode(f, fb::PlanNodeKind_CudfJoin, join.Union());
  f.Finish(fb::CreateGpuPlan(f, node));
  return {f.GetBufferPointer(), f.GetBufferPointer() + f.GetSize()};
}

TEST(JoinSession, ExecuteNodeRefusesACudfJoin) {
  Session s(make_plan({.build_schema = {{"b_k", fb::DataType_Int32}},
                       .probe_schema = {{"p_k", fb::DataType_Int32}}}));
  uint64_t out = 0, n = 0;
  PeacockNodeStats st{};
  EXPECT_NE(peacock_executor_execute_node(s.exec(), 0, nullptr, nullptr, 0, &out, 1, &n, &st), 0);
  EXPECT_NE(s.error().find("runs through peacock_join_build"), std::string::npos) << s.error();
}

// --- Task 2: the lifecycle, and Inner ---------------------------------------

static JoinSpec inner_on_k() {
  return {.type = fb::JoinType_Inner,
          .keys = {{0, 0}},
          .build_schema = {{"b_k", fb::DataType_Int32}, {"b_v", fb::DataType_Utf8}},
          .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}};
}

TEST(JoinSession, AnInnerJoinAnswersEachProbeBatchAgainstOneBuild) {
  Session s(make_plan(inner_on_k()));
  auto b = s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})});
  uint64_t join = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &join, nullptr), 0) << s.error();
  uint64_t out = 0;
  ASSERT_EQ(peacock_join_probe(s.exec(), join, s.upload({i32("p_k", {2, 3}), i64("p_w", {20, 30})}),
                               &out, nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  ASSERT_EQ(peacock_join_probe(s.exec(), join,
                               s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 0})}), &out,
                               nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|a|1|10"}));  // NULL never matches NULL
  ASSERT_EQ(peacock_join_finish(s.exec(), join, &out, nullptr), 0) << s.error();
  EXPECT_EQ(out, 0u);  // Inner has no finish
  peacock_join_release(s.exec(), join);
}

TEST(JoinSession, AProbeAfterFinishIsRefused) {
  Session s(make_plan(inner_on_k()));
  uint64_t join = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &join,
                               nullptr),
            0)
      << s.error();
  ASSERT_EQ(peacock_join_finish(s.exec(), join, &out, nullptr), 0) << s.error();
  EXPECT_NE(peacock_join_probe(s.exec(), join, s.upload({i32("p_k", {1}), i64("p_w", {1})}), &out,
                               nullptr),
            0);
  EXPECT_NE(s.error().find("finished"), std::string::npos) << s.error();
}

TEST(JoinSession, AnUnknownJoinIdIsRefusedWithoutEndingTheQuery) {
  Session s(make_plan(inner_on_k()));
  uint64_t out = 0;
  auto h = s.upload({i32("p_k", {1}), i64("p_w", {1})});
  EXPECT_NE(peacock_join_finish(s.exec(), 999, &out, nullptr), 0);
  EXPECT_NE(s.error().find("unknown join"), std::string::npos) << s.error();
  EXPECT_EQ(s.rows(h).size(), 1u);  // the session is still standing
}

/// A build side big enough to measure what releasing it gives back: a session holds its
/// build table, its hash table and its matched column across every call, so it is the one
/// place in the tree where a leak is invisible to any allocation bound.
static JoinSpec inner_on_i64() {
  return {.type = fb::JoinType_Inner,
          .keys = {{0, 0}},
          .build_schema = {{"b_k", fb::DataType_Int64}, {"b_v", fb::DataType_Int64}},
          .probe_schema = {{"p_k", fb::DataType_Int64}, {"p_v", fb::DataType_Int64}}};
}

TEST(JoinSession, ReleaseWithoutFinishAndEndPlanFreeTheSession) {
  constexpr int64_t n = 1 << 18;
  std::vector<std::optional<int64_t>> k(n), v(n);
  for (int64_t i = 0; i < n; ++i) {
    k[i] = i;
    v[i] = i * 3;
  }
  Session s(make_plan(inner_on_i64()));
  uint64_t a = 0, b = 0, ha = 0, hb = 0;
  auto up_a = allocated_by([&] { ha = s.upload({i64("b_k", k), i64("b_v", v)}); });
  ASSERT_EQ(peacock_join_build(s.exec(), 0, ha, &a, nullptr), 0) << s.error();
  auto released = allocated_by([&] { peacock_join_release(s.exec(), a); });
  std::cout << "[join release] build " << up_a.net << " | release net " << released.net << "\n";
  EXPECT_LE(released.net, -(up_a.net - up_a.net / 16))
      << "release gave back " << -released.net << " of the build side's own " << up_a.net;
  peacock_join_release(s.exec(), a);  // idempotent, as handle release is
  uint64_t out = 0;
  EXPECT_NE(peacock_join_finish(s.exec(), a, &out, nullptr), 0);

  // A second session, freed with the plan rather than by hand.
  auto up_b = allocated_by([&] { hb = s.upload({i64("b_k", k), i64("b_v", v)}); });
  ASSERT_EQ(peacock_join_build(s.exec(), 0, hb, &b, nullptr), 0) << s.error();
  auto ended = allocated_by([&] { peacock_executor_end_plan(s.exec()); });
  EXPECT_LE(ended.net, -(up_b.net - up_b.net / 16))
      << "end_plan gave back " << -ended.net << " of the live session's build side";
}

// --- Task 3: Left, Right, Full and the finish -------------------------------

static JoinSpec typed(fb::JoinType t) {
  auto s = inner_on_k();
  s.type = t;
  return s;
}

/// The build every outer case uses: duplicate key 2, and a NULL key that matches nothing.
static std::vector<Col> build_k_v() {
  return {i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})};
}

TEST(JoinSession, LeftPadsEveryBuildRowNoProbeMatchedIncludingANullKey) {
  Session s(make_plan(typed(fb::JoinType_Left)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
  ASSERT_EQ(peacock_join_probe(s.exec(), j,
                               s.upload({i32("p_k", {2, std::nullopt}), i64("p_w", {20, 99})}),
                               &out, nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  // A zero-row batch, then one matching key 1 and not key 2: `matched` folds across the
  // three, so only the NULL-key build row is left to pad (#136).
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {}), i64("p_w", {})}), &out, nullptr), 0)
      << s.error();
  EXPECT_TRUE(s.rows(out).empty());
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {10})}), &out, nullptr),
      0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|a|1|10"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  // the latent finish defect: today's LeftAnti hardcodes NULL = NULL and drops "n"
  EXPECT_EQ(s.rows(out), sorted({"NULL|n|NULL|NULL"}));
}

TEST(JoinSession, RightPadsEachUnmatchedProbeRowInItsOwnCall) {
  Session s(make_plan(typed(fb::JoinType_Right)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
  ASSERT_EQ(peacock_join_probe(
                s.exec(), j, s.upload({i32("p_k", {2, 5, std::nullopt}), i64("p_w", {20, 50, 99})}),
                &out, nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20", "NULL|NULL|5|50", "NULL|NULL|NULL|99"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(out, 0u);
}

TEST(JoinSession, FullNeverReemitsAnUnmatchedBuildRowAcrossThreeBatches) {
  Session s(make_plan(typed(fb::JoinType_Full)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out, nullptr),
      0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {}), i64("p_w", {})}), &out, nullptr), 0)
      << s.error();
  EXPECT_TRUE(s.rows(out).empty());  // a zero-row batch, one zero-row table
  ASSERT_EQ(peacock_join_probe(s.exec(), j,
                               s.upload({i32("p_k", {7, std::nullopt}), i64("p_w", {70, 99})}),
                               &out, nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"NULL|NULL|7|70", "NULL|NULL|NULL|99"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|a|NULL|NULL", "NULL|n|NULL|NULL"}));
}

// `null_equals_null` is INTERSECT's and EXCEPT's semantics, so it reaches the semi family
// too: it is what makes `distinct_keys` keep a NULL key rather than drop it, and what
// `distinct_hash_join` and the per-batch distinct probe keys are built with.
TEST(JoinSession, NullEqualsNullMatchesNullKeysForEveryType) {
  struct C {
    fb::JoinType t;
    Rows probe;
    std::optional<Rows> finish;
  };
  for (auto const& c : std::vector<C>{{fb::JoinType_Inner, {"NULL|n|NULL|99"}, std::nullopt},
                                      {fb::JoinType_Left, {"NULL|n|NULL|99"}, Rows{}},
                                      {fb::JoinType_Right, {"NULL|n|NULL|99"}, std::nullopt},
                                      {fb::JoinType_Full, {"NULL|n|NULL|99"}, Rows{}},
                                      {fb::JoinType_LeftSemi, {}, Rows{"NULL|n"}},
                                      {fb::JoinType_LeftAnti, {}, Rows{}},
                                      {fb::JoinType_LeftMark, {}, Rows{"NULL|n|true"}},
                                      {fb::JoinType_RightSemi, {"NULL|99"}, std::nullopt},
                                      {fb::JoinType_RightAnti, {}, std::nullopt}}) {
    auto spec = typed(c.t);
    spec.null_equals_null = true;
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(
        peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {std::nullopt}), utf8("b_v", {"n"})}),
                           &j, nullptr),
        0)
        << s.error();
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {std::nullopt}), i64("p_w", {99})}),
                           &out, nullptr),
        0)
        << s.error();
    if (is_build_side_semi(c.t))
      EXPECT_EQ(out, 0u) << fb::EnumNameJoinType(c.t);
    else
      EXPECT_EQ(s.rows(out), sorted(c.probe)) << fb::EnumNameJoinType(c.t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    // The one build row matched its NULL twin, so nothing is left to pad or to keep.
    if (c.finish)
      EXPECT_EQ(s.rows(out), sorted(*c.finish)) << fb::EnumNameJoinType(c.t);
    else
      EXPECT_EQ(out, 0u) << fb::EnumNameJoinType(c.t);
  }
}

TEST(JoinSession, AProjectionCrossingSidesKeepsOnlyItsColumns) {
  auto spec = typed(fb::JoinType_Left);
  spec.projection = std::vector<uint32_t>{3, 1};  // p_w, b_v
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"})}),
                               &j, nullptr),
            0)
      << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out, nullptr),
      0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"20|b"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(s.rows(out), sorted({"NULL|a"}));
}

// --- Task 4: the semi family without a residual -----------------------------

// The fold this session exists for (#136): a build row matched by one batch stays matched
// through every later one, so `matched` is an OR across calls and not the last call's answer.
// Three batches with a zero-row batch between them, and no batch matches what another did.
TEST(JoinSession, LeftSemiAntiMarkFoldEveryBatchAndAnswerEachBuildRowOnce) {
  for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_LeftSemi, {"1|a", "2|b", "2|c"}},
           {fb::JoinType_LeftAnti, {"NULL|n"}},
           {fb::JoinType_LeftMark, {"1|a|true", "2|b|true", "2|c|true", "NULL|n|false"}}}) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 1;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
    // Batch 1: three probe rows on key 2 and a NULL — many-to-many, and NULL matches nothing.
    ASSERT_EQ(
        peacock_join_probe(
            s.exec(), j, s.upload({i32("p_k", {2, 2, 2, std::nullopt}), i64("p_w", {1, 2, 3, 4})}),
            &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(out, 0u) << "a build-side semi type answers no batch per probe";
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {}), i64("p_w", {})}), &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(out, 0u);
    // Batch 3 matches key 1 and not key 2, so an overwrite would lose the build rows batch 1
    // matched and a lost fold shows in every one of the three answers.
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {10})}), &out,
                                 nullptr),
              0)
        << s.error();
    EXPECT_EQ(out, 0u);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}

TEST(JoinSession, RightSemiAntiAnswerPerBatchOverDuplicateBuildKeys) {
  for (auto const& [t, want] :
       std::vector<std::pair<fb::JoinType, Rows>>{{fb::JoinType_RightSemi, {"2|20", "2|21"}},
                                                  {fb::JoinType_RightAnti, {"5|50", "NULL|99"}}}) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                 s.upload({i32("b_k", {2, 2, 2, std::nullopt}),
                                           utf8("b_v", {"a", "b", "c", "n"})}),
                                 &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(
                  s.exec(), j,
                  s.upload({i32("p_k", {2, 2, 5, std::nullopt}), i64("p_w", {20, 21, 50, 99})}),
                  &out, nullptr),
              0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(out, 0u);
  }
}

TEST(JoinSession, RightAntiOverABuildWhoseKeysAreAllNullKeepsEveryProbeRow) {
  Session s(make_plan(typed(fb::JoinType_RightAnti)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(
                s.exec(), 0,
                s.upload({i32("b_k", {std::nullopt, std::nullopt}), utf8("b_v", {"m", "n"})}), &j,
                nullptr),
            0)
      << s.error();
  ASSERT_EQ(peacock_join_probe(s.exec(), j,
                               s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 99})}),
                               &out, nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|10", "NULL|99"}));
}

// --- Task 5: absent and empty sides (§3.8, #173, #212) ----------------------

struct EmptyCase {
  fb::JoinType t;
  Rows per_probe;
  std::optional<Rows> finish;  // nullopt = this type has no finish
};

// build: no batch, or zero rows — the same answers. probe: p_k {1, NULL}, p_w {10, 99}.
static std::vector<EmptyCase> empty_build_answers() {
  return {
      {fb::JoinType_Inner, {}, std::nullopt},
      {fb::JoinType_Left, {}, Rows{}},
      {fb::JoinType_Right, {"NULL|NULL|1|10", "NULL|NULL|NULL|99"}, std::nullopt},
      {fb::JoinType_Full, {"NULL|NULL|1|10", "NULL|NULL|NULL|99"}, Rows{}},
      {fb::JoinType_LeftSemi, {}, Rows{}},
      {fb::JoinType_LeftAnti, {}, Rows{}},
      {fb::JoinType_LeftMark, {}, Rows{}},
      {fb::JoinType_RightSemi, {}, std::nullopt},
      {fb::JoinType_RightAnti, {"1|10", "NULL|99"}, std::nullopt},
  };
}

/// The same two sides under a residual the build and probe both read, so the empty-build
/// answers can be compared against the keys-only ones row for row. `p_w > b_k` is Int64
/// beside Int32, which the AST refuses, so it also exercises the pairs path's constructor.
static JoinSpec typed_with_residual(fb::JoinType t) {
  auto spec = typed(t);
  spec.filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Gt, col(f, 1)); };
  spec.filter_columns = {{fb::JoinSide_Right, 1}, {fb::JoinSide_Left, 0}};
  return spec;
}

/// The same condition with no keys: a nested loop over the same two sides.
static JoinSpec typed_nested(fb::JoinType t) {
  auto spec = typed_with_residual(t);
  spec.keys.clear();
  return spec;
}

// §5.6's one explicit full product, now over three of its eight conditions rather than one:
// `join_build` sets `empty_build` and returns before the A/R split, the hoisting and the
// hash build, so every condition shares `probe_empty_build` — and pbench leaves three lanes
// in four with no build batch, which is how task 8's first multi-lane run meets this.
TEST(JoinSession, AnEmptyOrAbsentBuildAnswersEveryType) {
  const std::vector<std::pair<const char*, JoinSpec (*)(fb::JoinType)>> conditions{
      {"keys only", typed}, {"keys + residual", typed_with_residual}, {"no keys", typed_nested}};
  for (auto const& [name, spec_of] : conditions) {
    for (bool absent : {true, false}) {
      for (auto const& c : empty_build_answers()) {
        Session s(make_plan(spec_of(c.t)));
        uint64_t b = absent ? 0 : s.upload({i32("b_k", {}), utf8("b_v", {})});
        uint64_t j = 0, out = 0;
        const std::string where = std::string(fb::EnumNameJoinType(c.t)) + " " + name +
                                  " absent=" + std::to_string(absent);
        ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &j, nullptr), 0) << s.error() << " " << where;
        ASSERT_EQ(peacock_join_probe(
                      s.exec(), j, s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 99})}),
                      &out, nullptr),
                  0)
            << s.error() << " " << where;
        if (is_build_side_semi(c.t))
          EXPECT_EQ(out, 0u) << where;
        else
          EXPECT_EQ(s.rows(out), sorted(c.per_probe)) << where;
        ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error() << " " << where;
        if (c.finish)
          EXPECT_EQ(s.rows(out), sorted(*c.finish)) << where;
        else
          EXPECT_EQ(out, 0u) << where;
      }
    }
  }
}

// What task 8 must honour: an absent build side's columns are named and ordered by
// `build_schema`, so a lane with no batch emits the same schema as one with rows. A
// disagreement shows up as an Arrow mismatch on empty lanes alone, debugged from Rust.
TEST(JoinSession, AnAbsentBuildEmitsTheSchemaABatchWouldHave) {
  std::shared_ptr<arrow::Schema> want;
  {
    Session s(make_plan(typed(fb::JoinType_Right)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j,
                                 nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {10})}), &out,
                                 nullptr),
              0)
        << s.error();
    ASSERT_EQ(s.rows(out).size(), 1u);
    want = s.schema(out);
  }
  for (bool absent : {true, false}) {
    Session s(make_plan(typed(fb::JoinType_Right)));
    uint64_t b = absent ? 0 : s.upload({i32("b_k", {}), utf8("b_v", {})});
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &j, nullptr), 0) << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {10})}), &out,
                                 nullptr),
              0)
        << s.error();
    expect_same_columns(s.schema(out), want);
  }
}

TEST(JoinSession, NoProbeBatchStillFinishesFromTheBuildSide) {  // #173
  std::vector<std::pair<fb::JoinType, Rows>> want = {
      {fb::JoinType_Left, {"1|a|NULL|NULL", "NULL|n|NULL|NULL"}},
      {fb::JoinType_Full, {"1|a|NULL|NULL", "NULL|n|NULL|NULL"}},
      {fb::JoinType_LeftSemi, {}},
      {fb::JoinType_LeftAnti, {"1|a", "NULL|n"}},
      {fb::JoinType_LeftMark, {"1|a|false", "NULL|n|false"}}};
  for (auto const& [t, rows] : want) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                 s.upload({i32("b_k", {1, std::nullopt}), utf8("b_v", {"a", "n"})}),
                                 &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), sorted(rows)) << fb::EnumNameJoinType(t);  // pads from probe_schema
  }
}

// --- Task 6: the residual — pairs, per-side conjuncts, chunking (§3.4) ------

static flatbuffers::Offset<fb::Expr> lit_i64(flatbuffers::FlatBufferBuilder& f, int64_t v) {
  fb::ScalarValueBuilder sb(f);
  sb.add_type(fb::DataType_Int64);
  sb.add_int_val(v);
  auto sv = sb.Finish();
  return fb::CreateExpr(f, fb::ExprNode_LiteralExpr, fb::CreateLiteralExpr(f, sv).Union());
}

/// `p_w > b_lim`: the filter schema is [Probe 1, Build 2], so col(0) is p_w and col(1) b_lim.
static JoinSpec residual(fb::JoinType t) {
  return {.type = t,
          .keys = {{0, 0}},
          .filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Gt, col(f, 1)); },
          .filter_columns = {{fb::JoinSide_Right, 1}, {fb::JoinSide_Left, 2}},
          .build_schema = {{"b_k", fb::DataType_Int32},
                           {"b_v", fb::DataType_Utf8},
                           {"b_lim", fb::DataType_Int32}},
          .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}};
}

// probe p_k {1, 1, 2}, p_w {3, 9, 9}: key matches (1,3) (1,9) (2,9); the filter keeps
// (1,9) (2,9), so probe row (1,3) is unmatched and Right/Full pad it (#153).
TEST(JoinSession, AnOuterResidualDecidesUnmatchedAfterTheFilter) {
  struct C {
    fb::JoinType t;
    Rows probe;
    Rows finish;
  };
  for (auto const& c :
       std::vector<C>{{fb::JoinType_Inner, {"1|a|5|1|9", "2|b|5|2|9"}, {}},
                      {fb::JoinType_Left, {"1|a|5|1|9", "2|b|5|2|9"}, {}},
                      {fb::JoinType_Right, {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}},
                      {fb::JoinType_Full, {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}}}) {
    Session s(make_plan(residual(c.t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(
                  s.exec(), 0,
                  s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"}), i32("b_lim", {5, 5})}), &j,
                  nullptr),
              0)
        << s.error();
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 1, 2}), i64("p_w", {3, 9, 9})}),
                           &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted(c.probe)) << fb::EnumNameJoinType(c.t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    if (c.t == fb::JoinType_Left || c.t == fb::JoinType_Full)
      EXPECT_EQ(s.rows(out), sorted(c.finish)) << fb::EnumNameJoinType(c.t);
  }
}

TEST(JoinSession, ChunkingThePairsGivesTheSameRows) {
  auto spec = residual(fb::JoinType_Full);
  spec.chunk_bytes = 16;  // forces one chunk per probe row
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0,
                               s.upload({i32("b_k", {1, 1, 2}), utf8("b_v", {"a", "a2", "b"}),
                                         i32("b_lim", {5, 5, 5})}),
                               &j, nullptr),
            0)
      << s.error();
  ASSERT_EQ(peacock_join_probe(s.exec(), j,
                               s.upload({i32("p_k", {1, 1, 2, 3}), i64("p_w", {3, 9, 9, 9})}), &out,
                               nullptr),
            0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|a|5|1|9", "1|a2|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3",
                                 "NULL|NULL|NULL|3|9"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_TRUE(s.rows(out).empty()) << "every build row matched under the filter";
}

// A preserved-side condition that is NULL is no match (design §3.4, D3): a NULL b_flag
// keeps the anti row and marks it false, never NULL.
TEST(JoinSession, ANullPreservedSideConditionIsNoMatch) {
  for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_LeftAnti, {"1|NULL", "1|false"}},
           {fb::JoinType_LeftSemi, {"1|true"}},
           {fb::JoinType_LeftMark, {"1|NULL|false", "1|false|false", "1|true|true"}}}) {
    JoinSpec spec{.type = t,
                  .keys = {{0, 0}},
                  .filter = [](auto& f) { return col(f, 0); },  // b_flag alone
                  .filter_columns = {{fb::JoinSide_Left, 1}},
                  .build_schema = {{"b_k", fb::DataType_Int32}, {"b_flag", fb::DataType_Boolean}},
                  .probe_schema = {{"p_k", fb::DataType_Int32}}};
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(
                  s.exec(), 0,
                  s.upload({i32("b_k", {1, 1, 1}), boolean("b_flag", {std::nullopt, false, true})}),
                  &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1})}), &out, nullptr), 0)
        << s.error();
    EXPECT_EQ(out, 0u);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}

// RightSemi/RightAnti over a residual neither conjunct of which reads both sides: the
// build-only one filters the build before any key work, the probe-only one is the
// preserved side's R per batch. No pairs are made.
TEST(JoinSession, ARightSemiAntiResidualOnOneSideAtATimeNeedsNoPairs) {
  for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_RightSemi, {"2|20"}}, {fb::JoinType_RightAnti, {"1|10", "3|30"}}}) {
    JoinSpec spec{.type = t,
                  .keys = {{0, 0}},
                  // b_flag AND p_w > 15
                  .filter =
                      [](auto& f) {
                        return bin(f, col(f, 0), fb::BinaryOp_And,
                                   bin(f, col(f, 1), fb::BinaryOp_Gt, lit_i64(f, 15)));
                      },
                  .filter_columns = {{fb::JoinSide_Left, 2}, {fb::JoinSide_Right, 1}},
                  .build_schema = {{"b_k", fb::DataType_Int32},
                                   {"b_v", fb::DataType_Utf8},
                                   {"b_flag", fb::DataType_Boolean}},
                  .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}};
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                 s.upload({i32("b_k", {1, 1, 2}), utf8("b_v", {"a", "b", "c"}),
                                           boolean("b_flag", {true, false, true})}),
                                 &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 2, 3}), i64("p_w", {10, 20, 30})}),
                           &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}

// --- Task 7: the semi family over a residual reading both sides (D1, D13) ---

/// b_k, b_lim, b_dlim | p_k, p_w, p_d. `decimals` picks the non-AST form of the same
/// comparison: cudf_ast_can_evaluate refuses any decimal operand, so the residual then
/// takes the pairs path instead of one mixed_left_semi_join.
static JoinSpec cross_residual(fb::JoinType t, bool decimals) {
  JoinSpec s{.type = t,
             .keys = {{0, 0}},
             .build_schema = {{"b_k", fb::DataType_Int32},
                              {"b_lim", fb::DataType_Int32},
                              {"b_dlim", fb::DataType_Decimal128, 15, 2}},
             .probe_schema = {{"p_k", fb::DataType_Int32},
                              {"p_w", fb::DataType_Int64},
                              {"p_d", fb::DataType_Decimal128, 15, 2}}};
  s.filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Gt, col(f, 1)); };
  s.filter_columns = decimals
                         ? std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 2},
                                                                          {fb::JoinSide_Left, 2}}
                         : std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 1},
                                                                          {fb::JoinSide_Left, 1}};
  // The decimal columns never reach an answer, so both forms are compared row for row.
  s.projection =
      t == fb::JoinType_LeftMark ? std::vector<uint32_t>{0, 1, 3} : std::vector<uint32_t>{0, 1};
  return s;
}

// build (1,5) (1,9) (2,5); probe (1,7) (2,1) (3,9). Key matches b0-p0, b1-p0, b2-p1; the
// residual keeps b0-p0 alone (7 > 5, while 7 > 9 and 1 > 5 are false).
TEST(JoinSession, TheSemiFamilyOverACrossResidualAnswersTheSameEitherPath) {
  for (bool decimals : {false, true}) {
    for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
             {fb::JoinType_LeftSemi, {"1|5", "2|5"}},
             {fb::JoinType_LeftAnti, {"1|9"}},
             {fb::JoinType_LeftMark, {"1|5|true", "1|9|false", "2|5|true"}},
             {fb::JoinType_RightSemi, {"1|7"}},
             {fb::JoinType_RightAnti, {"2|1", "3|9"}}}) {
      Session s(make_plan(cross_residual(t, decimals)));
      uint64_t j = 0, out = 0;
      ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                   s.upload({i32("b_k", {1, 1, 2}), i32("b_lim", {5, 9, 5}),
                                             dec("b_dlim", 15, 2, {500, 900, 500})}),
                                   &j, nullptr),
                0)
          << s.error();
      ASSERT_EQ(peacock_join_probe(s.exec(), j,
                                   s.upload({i32("p_k", {1, 2, 3}), i64("p_w", {7, 1, 9}),
                                             dec("p_d", 15, 2, {700, 100, 900})}),
                                   &out, nullptr),
                0)
          << s.error();
      if (is_build_side_semi(t)) {
        EXPECT_EQ(out, 0u);
        // A zero-row batch, then one whose residual passes against b3 alone: the build row
        // batch 1 matched must stay matched, on the mixed path and on the pairs path alike
        // (#136). Both of those fold `matched` at a site a single batch cannot test.
        ASSERT_EQ(
            peacock_join_probe(s.exec(), j,
                               s.upload({i32("p_k", {}), i64("p_w", {}), dec("p_d", 15, 2, {})}),
                               &out, nullptr),
            0)
            << s.error();
        EXPECT_EQ(out, 0u);
        ASSERT_EQ(
            peacock_join_probe(
                s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {9}), dec("p_d", 15, 2, {900})}),
                &out, nullptr),
            0)
            << s.error();
        EXPECT_EQ(out, 0u);
        ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
      }
      EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t) << " decimals=" << decimals;
    }
  }
}

// Several cross conjuncts: their AND is built at the cuDF AST level (NULL_LOGICAL_AND), so
// one mixed call answers both. With one conjunct a decimal comparison the AST is refused
// and the pairs path answers the same rows — the shape that would reach a null `hj`.
TEST(JoinSession, ASemiJoinWithTwoCrossConjunctsAndsThemAtTheAstLevel) {
  struct Form {
    bool decimals;
    uint64_t chunk_bytes;
  };
  for (auto const& form : std::vector<Form>{{false, 0}, {true, 0}, {true, 16}}) {
    for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
             {fb::JoinType_LeftSemi, {"1|5|20"}},
             {fb::JoinType_LeftMark, {"1|5|8|false", "1|5|20|true"}}}) {
      JoinSpec spec{.type = t,
                    .keys = {{0, 0}},
                    .build_schema = {{"b_k", fb::DataType_Int32},
                                     {"b_lim", fb::DataType_Int32},
                                     {"b_hi", fb::DataType_Int32},
                                     {"b_dlim", fb::DataType_Decimal128, 15, 2}},
                    .probe_schema = {{"p_k", fb::DataType_Int32},
                                     {"p_w", fb::DataType_Int64},
                                     {"p_d", fb::DataType_Decimal128, 15, 2}},
                    .chunk_bytes = form.chunk_bytes};
      // col(0) > col(1) AND col(2) < col(3)
      spec.filter = [](auto& f) {
        return bin(f, bin(f, col(f, 0), fb::BinaryOp_Gt, col(f, 1)), fb::BinaryOp_And,
                   bin(f, col(f, 2), fb::BinaryOp_Lt, col(f, 3)));
      };
      spec.filter_columns =
          form.decimals ? std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 2},
                                                                         {fb::JoinSide_Left, 3},
                                                                         {fb::JoinSide_Right, 1},
                                                                         {fb::JoinSide_Left, 2}}
                        : std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 1},
                                                                         {fb::JoinSide_Left, 1},
                                                                         {fb::JoinSide_Right, 1},
                                                                         {fb::JoinSide_Left, 2}};
      spec.projection = t == fb::JoinType_LeftMark ? std::vector<uint32_t>{0, 1, 2, 4}
                                                   : std::vector<uint32_t>{0, 1, 2};
      Session s(make_plan(spec));
      uint64_t j = 0, out = 0;
      ASSERT_EQ(
          peacock_join_build(s.exec(), 0,
                             s.upload({i32("b_k", {1, 1}), i32("b_lim", {5, 5}),
                                       i32("b_hi", {8, 20}), dec("b_dlim", 15, 2, {500, 500})}),
                             &j, nullptr),
          0)
          << s.error();
      // Three probe rows, so a small budget actually cuts the batch: `chunks` caps the range
      // count at the probe's rows, and one row is one range however small the budget. p1
      // fails both conjuncts against both build rows, p2 matches no key.
      ASSERT_EQ(peacock_join_probe(s.exec(), j,
                                   s.upload({i32("p_k", {1, 1, 2}), i64("p_w", {10, 3, 10}),
                                             dec("p_d", 15, 2, {1000, 300, 1000})}),
                                   &out, nullptr),
                0)
          << s.error();
      EXPECT_EQ(out, 0u);
      ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
      EXPECT_EQ(s.rows(out), sorted(want))
          << fb::EnumNameJoinType(t) << " decimals=" << form.decimals
          << " chunk_bytes=" << form.chunk_bytes;
    }
  }
}

// --- Task 9: nested loops — every type, literal true, chunked (§3.6) --------

/// No keys, condition `p_v < b_v`: filter schema [Probe 0, Build 1]. `decimals` swaps in
/// the same comparison over decimals, which the AST refuses, so `A` is empty and the
/// chunked cross of indices answers instead.
static JoinSpec nested(fb::JoinType t, bool decimals, uint64_t chunk_bytes = 0) {
  JoinSpec s{.type = t,
             .build_schema = {{"b_id", fb::DataType_Int32},
                              {"b_v", fb::DataType_Int32},
                              {"b_d", fb::DataType_Decimal128, 15, 2}},
             .probe_schema = {{"p_v", fb::DataType_Int32}, {"p_d", fb::DataType_Decimal128, 15, 2}},
             .chunk_bytes = chunk_bytes};
  s.filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Lt, col(f, 1)); };
  s.filter_columns = decimals
                         ? std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 1},
                                                                          {fb::JoinSide_Left, 2}}
                         : std::vector<std::pair<fb::JoinSide, uint32_t>>{{fb::JoinSide_Right, 0},
                                                                          {fb::JoinSide_Left, 1}};
  // The decimal columns never reach an answer, so every form is compared row for row.
  s.projection = t == fb::JoinType_LeftMark
                     ? std::vector<uint32_t>{0, 1, 3}
                     : (is_build_side_semi(t) ? std::vector<uint32_t>{0, 1}
                        : (t == fb::JoinType_RightSemi || t == fb::JoinType_RightAnti)
                            ? std::vector<uint32_t>{0}
                            : std::vector<uint32_t>{0, 1, 3});
  return s;
}

// build (1,5) (2,10); probe 3, 7, 12. Pairs passing `p_v < b_v`: (b1,3) (b2,3) (b2,7).
TEST(JoinSession, ANestedLoopAnswersEveryTypeOnTheAstAndOffIt) {
  struct C {
    fb::JoinType t;
    Rows probe;
    std::optional<Rows> finish;
  };
  const std::vector<C> cases{
      {fb::JoinType_Inner, {"1|5|3", "2|10|3", "2|10|7"}, std::nullopt},
      {fb::JoinType_Left, {"1|5|3", "2|10|3", "2|10|7"}, Rows{}},
      {fb::JoinType_Right, {"1|5|3", "2|10|3", "2|10|7", "NULL|NULL|12"}, std::nullopt},
      {fb::JoinType_Full, {"1|5|3", "2|10|3", "2|10|7", "NULL|NULL|12"}, Rows{}},
      {fb::JoinType_LeftSemi, {}, Rows{"1|5", "2|10"}},
      {fb::JoinType_LeftAnti, {}, Rows{}},
      {fb::JoinType_LeftMark, {}, Rows{"1|5|true", "2|10|true"}},
      {fb::JoinType_RightSemi, {"3", "7"}, std::nullopt},
      {fb::JoinType_RightAnti, {"12"}, std::nullopt},
  };
  struct Form {
    bool decimals;
    uint64_t chunk_bytes;
  };
  for (auto const& form : std::vector<Form>{{false, 0}, {true, 0}, {true, 16}}) {
    for (auto const& c : cases) {
      Session s(make_plan(nested(c.t, form.decimals, form.chunk_bytes)));
      uint64_t j = 0, out = 0;
      ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                   s.upload({i32("b_id", {1, 2}), i32("b_v", {5, 10}),
                                             dec("b_d", 15, 2, {500, 1000})}),
                                   &j, nullptr),
                0)
          << s.error();
      ASSERT_EQ(
          peacock_join_probe(
              s.exec(), j, s.upload({i32("p_v", {3, 7, 12}), dec("p_d", 15, 2, {300, 700, 1200})}),
              &out, nullptr),
          0)
          << s.error();
      const std::string where = std::string(fb::EnumNameJoinType(c.t)) +
                                " decimals=" + std::to_string(form.decimals) +
                                " chunk_bytes=" + std::to_string(form.chunk_bytes);
      if (is_build_side_semi(c.t))
        EXPECT_EQ(out, 0u) << where;
      else
        EXPECT_EQ(s.rows(out), sorted(c.probe)) << where;
      ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
      if (c.finish)
        EXPECT_EQ(s.rows(out), sorted(*c.finish)) << where;
      else
        EXPECT_EQ(out, 0u) << where;
    }
  }
}

/// A predicate-free nested loop of any type but Inner arrives with the literal `true` as
/// its filter (design §4.1), which is AST-able and so takes the row-wise arm.
static JoinSpec unconditional(fb::JoinType t) {
  JoinSpec s{.type = t,
             .build_schema = {{"b_id", fb::DataType_Int32}},
             .probe_schema = {{"p_v", fb::DataType_Int32}}};
  s.filter = [](auto& f) { return lit_bool(f, true); };
  // No filter_columns: the literal names no column, so there are no ordinals to map, and
  // this is the encoding a predicate-free join carries (design §1.1, §3.6, §4.1).
  return s;
}

TEST(JoinSession, APredicateFreeNestedLoopOverAnEmptySide) {
  for (auto t : {fb::JoinType_Left, fb::JoinType_Right, fb::JoinType_Full}) {
    // a zero-row probe batch: Left and Full still pad every build row at finish
    Session a(make_plan(unconditional(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(a.exec(), 0, a.upload({i32("b_id", {1, 2})}), &j, nullptr), 0)
        << a.error();
    ASSERT_EQ(peacock_join_probe(a.exec(), j, a.upload({i32("p_v", {})}), &out, nullptr), 0)
        << a.error();
    EXPECT_TRUE(a.rows(out).empty()) << fb::EnumNameJoinType(t);
    ASSERT_EQ(peacock_join_finish(a.exec(), j, &out, nullptr), 0) << a.error();
    if (t == fb::JoinType_Left || t == fb::JoinType_Full)
      EXPECT_EQ(a.rows(out), sorted({"1|NULL", "2|NULL"})) << fb::EnumNameJoinType(t);

    // an empty build side: Right and Full pad every probe row
    Session b(make_plan(unconditional(t)));
    uint64_t j2 = 0, out2 = 0;
    ASSERT_EQ(peacock_join_build(b.exec(), 0, 0, &j2, nullptr), 0) << b.error();
    ASSERT_EQ(peacock_join_probe(b.exec(), j2, b.upload({i32("p_v", {7})}), &out2, nullptr), 0)
        << b.error();
    EXPECT_EQ(b.rows(out2), t == fb::JoinType_Left ? Rows{} : sorted({"NULL|7"}))
        << fb::EnumNameJoinType(t);
  }
}

// Design §4.1's own example, which the old recipe answers with zero rows because it makes
// every predicate-free nested loop a cross join: `tiny LEFT JOIN empty ON true` owes the
// eight padded rows, not none. The filter carries no filter_columns, which is what the
// planner will write for it.
TEST(JoinSession, APredicateFreeLeftNestedLoopOverAnEmptyBuildOwesItsPaddedRows) {
  std::vector<std::optional<int32_t>> tiny(8);
  for (int i = 0; i < 8; ++i) tiny[i] = i + 1;
  for (bool absent_probe : {true, false}) {
    Session s(make_plan(unconditional(fb::JoinType_Left)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", tiny)}), &j, nullptr), 0)
        << s.error();
    if (!absent_probe) {
      ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_v", {})}), &out, nullptr), 0)
          << s.error();
      EXPECT_TRUE(s.rows(out).empty()) << "the empty side pairs with nothing";
    }
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), sorted({"1|NULL", "2|NULL", "3|NULL", "4|NULL", "5|NULL", "6|NULL",
                                   "7|NULL", "8|NULL"}))
        << "absent_probe=" << absent_probe;
  }
}

TEST(JoinSession, AnUnconditionalInnerNestedLoopIsTheCrossProduct) {
  Session s(make_plan(unconditional(fb::JoinType_Inner)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1, 2})}), &j, nullptr), 0)
      << s.error();
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_v", {7, 8, 9})}), &out, nullptr), 0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|7", "1|8", "1|9", "2|7", "2|8", "2|9"}));
}

// The keyless half of #136's fold: `probe_nested_rowwise` sets `matched` from a
// conditional_inner_join for Left and from a conditional_left_semi_join for the build-side
// semi family, and no single batch can tell an OR from an overwrite. Equality rather than
// `<`, so batch 1 matches the second build row alone and batch 3 the first alone.
TEST(JoinSession, ANestedLoopFoldsItsMatchedBitAcrossBatches) {
  for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_Left, {}},
           {fb::JoinType_LeftSemi, {"1|5", "2|10"}},
           {fb::JoinType_LeftAnti, {}},
           {fb::JoinType_LeftMark, {"1|5|true", "2|10|true"}}}) {
    JoinSpec spec{.type = t,
                  .build_schema = {{"b_id", fb::DataType_Int32}, {"b_v", fb::DataType_Int32}},
                  .probe_schema = {{"p_v", fb::DataType_Int32}}};
    spec.filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Eq, col(f, 1)); };
    spec.filter_columns = {{fb::JoinSide_Right, 0}, {fb::JoinSide_Left, 1}};
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1, 2}), i32("b_v", {5, 10})}),
                                 &j, nullptr),
              0)
        << s.error();
    for (auto const& batch : std::vector<std::vector<std::optional<int32_t>>>{{10}, {}, {5}}) {
      ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_v", batch)}), &out, nullptr), 0)
          << s.error();
    }
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}

// --- Task 10: cross (§3.7) --------------------------------------------------

static JoinSpec cross_spec() {
  return {.type = fb::JoinType_Inner,
          .build_schema = {{"b_id", fb::DataType_Int32}},
          .probe_schema = {{"p_s", fb::DataType_Utf8}}};
}

TEST(JoinSession, ACrossJoinPairsEveryBuildRowWithEveryProbeRow) {
  Session s(make_plan(cross_spec()));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1, 2})}), &j, nullptr), 0)
      << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({utf8("p_s", {"x", "y", "z"})}), &out, nullptr), 0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|x", "1|y", "1|z", "2|x", "2|y", "2|z"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(out, 0u);
}

TEST(JoinSession, ACrossJoinProjectionKeepsOneProbeColumn) {  // #207's device half
  auto spec = cross_spec();
  spec.projection = std::vector<uint32_t>{1};
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1, 2})}), &j, nullptr), 0)
      << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({utf8("p_s", {"x", "y", "z"})}), &out, nullptr), 0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"x", "x", "y", "y", "z", "z"}));
}

TEST(JoinSession, ACrossJoinOverAZeroRowSideIsOneZeroRowTable) {  // #208's device half
  for (bool empty_probe : {true, false}) {
    Session s(make_plan(cross_spec()));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(
                  s.exec(), 0,
                  s.upload({i32("b_id", empty_probe ? std::vector<std::optional<int32_t>>{1}
                                                    : std::vector<std::optional<int32_t>>{})}),
                  &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(
        peacock_join_probe(
            s.exec(), j,
            s.upload({utf8("p_s", empty_probe ? std::vector<std::optional<std::string>>{}
                                              : std::vector<std::optional<std::string>>{"x"})}),
            &out, nullptr),
        0)
        << s.error();
    EXPECT_NE(out, 0u) << "one table, of zero rows";
    EXPECT_TRUE(s.rows(out).empty());
    EXPECT_EQ(s.schema(out)->num_fields(), 2) << "the output's columns, with no rows in them";
  }
}

// Exercised on sizes rather than on a table that big: 2^31 rows of anything is more than
// any test host holds, and the check is what stands between the plan and a silent wrap.
TEST(JoinSession, ACrossJoinOverTwoBillionRowsIsRefusedByName) {
  EXPECT_EQ(peacock::join::cross_rows_or_throw(1000, 1000), 1000000);
  try {
    peacock::join::cross_rows_or_throw(70000, 40000);  // 2.8e9
    FAIL() << "no refusal";
  } catch (std::runtime_error const& e) {
    EXPECT_NE(std::string(e.what()).find("exceeds one table"), std::string::npos) << e.what();
  }
}

// A predicate-free type other than Inner reaches the session as a nested loop over the
// literal true (design §4.1), so the cross arm never has to answer one.
TEST(JoinSession, APredicateFreeOuterJoinIsNotACrossJoin) {
  auto spec = cross_spec();
  spec.type = fb::JoinType_Left;
  Session s(make_plan(spec));
  uint64_t j = 0;
  EXPECT_NE(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1})}), &j, nullptr), 0);
  EXPECT_NE(s.error().find("literal true"), std::string::npos) << s.error();
}

// --- Task 8: hoisting one-side operands into columns (§3.6's `hoist`) -------

static flatbuffers::Offset<fb::Expr> fn1(flatbuffers::FlatBufferBuilder& f, const char* name,
                                         flatbuffers::Offset<fb::Expr> arg, fb::DataType ret) {
  auto n = f.CreateString(name);
  auto args = f.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{arg});
  fb::ScalarFunctionExprNodeBuilder b(f);
  b.add_name(n);
  b.add_args(args);
  b.add_return_type(ret);
  b.add_nullable(true);
  return fb::CreateExpr(f, fb::ExprNode_ScalarFunctionExprNode, b.Finish().Union());
}

static flatbuffers::Offset<fb::Expr> cast_dec(flatbuffers::FlatBufferBuilder& f,
                                              flatbuffers::Offset<fb::Expr> e, uint8_t p,
                                              int8_t sc) {
  fb::CastExprNodeBuilder b(f);
  b.add_expr(e);
  b.add_target_type(fb::DataType_Decimal128);
  b.add_decimal_precision(p);
  b.add_decimal_scale(sc);
  return fb::CreateExpr(f, fb::ExprNode_CastExprNode, b.Finish().Union());
}

// `upper(b_s) = p_s` reads the build alone on its left, and the AST cannot evaluate `upper`.
// Hoisted into a build condition column, the comparison is two string columns, which the
// AST can take — so a Left nested loop answers it row-wise. #215's shape either way.
TEST(JoinSession, ANestedLoopHoistsAOneSideOperandAndRunsOnTheAst) {
  JoinSpec spec{.type = fb::JoinType_Left,
                .build_schema = {{"b_id", fb::DataType_Int32}, {"b_s", fb::DataType_Utf8}},
                .probe_schema = {{"p_s", fb::DataType_Utf8}}};
  spec.filter = [](auto& f) {
    return bin(f, fn1(f, "upper", col(f, 0), fb::DataType_Utf8), fb::BinaryOp_Eq, col(f, 1));
  };
  spec.filter_columns = {{fb::JoinSide_Left, 1}, {fb::JoinSide_Right, 0}};
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(
                s.exec(), 0, s.upload({i32("b_id", {1, 2}), utf8("b_s", {"x", "y"})}), &j, nullptr),
            0)
      << s.error();
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({utf8("p_s", {"X", "z"})}), &out, nullptr), 0)
      << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|x|X"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(s.rows(out), sorted({"2|y|NULL"}));
}

// Both operands hoisted and the comparison still a decimal one, which the AST refuses: the
// condition stays in `R` and the chunked cross of indices answers it (#215).
TEST(JoinSession, ANestedLoopWhoseHoistedComparisonIsStillRefusedTakesTheCross) {
  for (uint64_t chunk_bytes : {uint64_t{0}, uint64_t{16}}) {
    JoinSpec spec{
        .type = fb::JoinType_Left,
        .build_schema = {{"b_id", fb::DataType_Int32}, {"b_d", fb::DataType_Decimal128, 15, 2}},
        .probe_schema = {{"p_id", fb::DataType_Int32}, {"p_d", fb::DataType_Decimal128, 15, 2}},
        .chunk_bytes = chunk_bytes};
    spec.filter = [](auto& f) {
      return bin(f, cast_dec(f, col(f, 0), 15, 2), fb::BinaryOp_Gt, cast_dec(f, col(f, 1), 15, 2));
    };
    spec.filter_columns = {{fb::JoinSide_Left, 1}, {fb::JoinSide_Right, 1}};
    spec.projection = std::vector<uint32_t>{0, 2};  // b_id, p_id
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0,
                                 s.upload({i32("b_id", {1, 2}), dec("b_d", 15, 2, {500, 1000})}),
                                 &j, nullptr),
              0)
        << s.error();
    // Three probe rows, so a small budget cuts the cross of indices into ranges rather than
    // making one of it: `chunks` caps the range count at the probe's row count. 12.00 is
    // above both build values and 4.00 below both.
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j,
                           s.upload({i32("p_id", {7, 8, 9}), dec("p_d", 15, 2, {700, 1200, 400})}),
                           &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted({"1|9", "2|7", "2|9"})) << "chunk_bytes=" << chunk_bytes;
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_TRUE(s.rows(out).empty()) << "every build row matched, chunk_bytes=" << chunk_bytes;
  }
}

// A keyed semi join whose cross residual holds a hoistable conjunct: hoisted, both
// conjuncts are AST and one mixed_left_semi_join answers. The second form makes the other
// conjunct a decimal comparison, so the residual takes the pairs path instead — the same
// rows, chunked or not.
TEST(JoinSession, AKeyedSemiJoinAnswersTheSameWithAHoistedConjunctOrThePairsPath) {
  struct Form {
    bool decimal_second;
    uint64_t chunk_bytes;
  };
  for (auto const& form : std::vector<Form>{{false, 0}, {true, 0}, {true, 16}}) {
    for (auto const& [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
             {fb::JoinType_LeftSemi, {"1|x|5"}},
             {fb::JoinType_LeftMark, {"1|x|5|true", "1|y|50|false"}}}) {
      JoinSpec spec{.type = t,
                    .keys = {{0, 0}},
                    .build_schema = {{"b_k", fb::DataType_Int32},
                                     {"b_s", fb::DataType_Utf8},
                                     {"b_lim", fb::DataType_Int64}},
                    .probe_schema = {{"p_k", fb::DataType_Int32},
                                     {"p_s", fb::DataType_Utf8},
                                     {"p_w", fb::DataType_Int64}},
                    .chunk_bytes = form.chunk_bytes};
      const bool dec2 = form.decimal_second;
      spec.filter = [dec2](auto& f) {
        auto first =
            bin(f, fn1(f, "upper", col(f, 0), fb::DataType_Utf8), fb::BinaryOp_Eq, col(f, 1));
        auto second = dec2 ? bin(f, cast_dec(f, col(f, 2), 18, 2), fb::BinaryOp_Gt,
                                 cast_dec(f, col(f, 3), 18, 2))
                           : bin(f, col(f, 2), fb::BinaryOp_Gt, col(f, 3));
        return bin(f, first, fb::BinaryOp_And, second);
      };
      spec.filter_columns = {{fb::JoinSide_Left, 1},
                             {fb::JoinSide_Right, 1},
                             {fb::JoinSide_Right, 2},
                             {fb::JoinSide_Left, 2}};
      Session s(make_plan(spec));
      uint64_t j = 0, out = 0;
      ASSERT_EQ(peacock_join_build(
                    s.exec(), 0,
                    s.upload({i32("b_k", {1, 1}), utf8("b_s", {"x", "y"}), i64("b_lim", {5, 50})}),
                    &j, nullptr),
                0)
          << s.error();
      // Three probe rows, so `chunk_bytes = 16` cuts the batch rather than making one range
      // of it: p1 matches b2's key and string but not its limit, p2 matches no key.
      ASSERT_EQ(peacock_join_probe(s.exec(), j,
                                   s.upload({i32("p_k", {1, 1, 2}), utf8("p_s", {"X", "Y", "X"}),
                                             i64("p_w", {10, 10, 10})}),
                                   &out, nullptr),
                0)
          << s.error();
      EXPECT_EQ(out, 0u);
      ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
      EXPECT_EQ(s.rows(out), sorted(want))
          << fb::EnumNameJoinType(t) << " decimal_second=" << form.decimal_second
          << " chunk_bytes=" << form.chunk_bytes;
    }
  }
}

// Hoisting moves no row and changes no answer, so only what the call allocates can show it
// happened: a condition the AST cannot take is a cross product of INDICES over every pair,
// while the hoisted one is a row-wise conditional join. The bound is the same cuDF work on
// the same input — the identical join whose condition needs no hoisting — plus the build
// side's own bytes, which is what the one hoisted column costs.
TEST(JoinSession, HoistingKeepsANestedLoopOffTheCrossProduct) {
  constexpr int kRows = 2000, kKeys = 50;
  std::vector<std::optional<std::string>> b_s(kRows), b_u(kRows), p_s(kRows);
  for (int i = 0; i < kRows; ++i) {
    b_s[i] = "s" + std::to_string(i % kKeys);
    b_u[i] = "S" + std::to_string(i % kKeys);
    p_s[i] = "S" + std::to_string(i % kKeys);
  }
  // `hoisted` needs upper(b_s) hoisted into a column to reach the AST; `plain` compares a
  // build column the test upper-cased itself, so it is AST-able as it stands.
  auto spec = [](bool hoisted) {
    JoinSpec s{.type = fb::JoinType_Inner,
               .build_schema = {{"b_s", fb::DataType_Utf8}, {"b_u", fb::DataType_Utf8}},
               .probe_schema = {{"p_s", fb::DataType_Utf8}}};
    s.filter = [hoisted](auto& f) {
      auto lhs = hoisted ? fn1(f, "upper", col(f, 0), fb::DataType_Utf8) : col(f, 0);
      return bin(f, lhs, fb::BinaryOp_Eq, col(f, 1));
    };
    s.filter_columns = {{fb::JoinSide_Left, hoisted ? 0u : 1u}, {fb::JoinSide_Right, 0}};
    s.projection = std::vector<uint32_t>{0, 2};  // b_s, p_s — one shape for both forms
    return s;
  };
  auto run = [&](bool hoisted, int64_t* build_bytes) {
    Session s(make_plan(spec(hoisted)));
    uint64_t j = 0, out = 0;
    uint64_t b = 0;
    auto upload = allocated_by([&] { b = s.upload({utf8("b_s", b_s), utf8("b_u", b_u)}); });
    if (build_bytes) *build_bytes = upload.net;
    EXPECT_EQ(peacock_join_build(s.exec(), 0, b, &j, nullptr), 0) << s.error();
    auto probe = s.upload({utf8("p_s", p_s)});
    auto got = allocated_by(
        [&] { EXPECT_EQ(peacock_join_probe(s.exec(), j, probe, &out, nullptr), 0) << s.error(); });
    EXPECT_EQ(s.rows(out).size(), static_cast<size_t>(kRows / kKeys) * (kRows / kKeys) * kKeys);
    return got;
  };
  int64_t build_bytes = 0;
  auto plain = run(false, &build_bytes);
  auto hoisted = run(true, nullptr);
  std::cout << "[hoist] build bytes " << build_bytes << " | plain peak " << plain.peak << " total "
            << plain.total << " net " << plain.net << " | hoisted peak " << hoisted.peak
            << " total " << hoisted.total << " net " << hoisted.net << "\n";
  const int64_t slack = int64_t{1} << 16;  // allocator rounding
  EXPECT_LT(hoisted.peak, plain.peak + build_bytes + slack)
      << "peak " << hoisted.peak << ": the condition was evaluated over every pair";
  EXPECT_LT(hoisted.total, plain.total + build_bytes + slack)
      << "total " << hoisted.total << ": the pairs asked the allocator for bytes of their own";
}

// A keyed outer residual the AST *can* take: the pairs path must evaluate it all the same.
// Regression — the path once passed only the conjuncts the AST had refused, so a residual
// of matching types was dropped and every key match was emitted.
TEST(JoinSession, AnOuterResidualOfMatchingTypesIsStillApplied) {
  struct C {
    fb::JoinType t;
    Rows probe;
    Rows finish;
  };
  for (auto const& c :
       std::vector<C>{{fb::JoinType_Inner, {"1|a|5|1|9", "2|b|5|2|9"}, {}},
                      {fb::JoinType_Left, {"1|a|5|1|9", "2|b|5|2|9"}, {}},
                      {fb::JoinType_Right, {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}},
                      {fb::JoinType_Full, {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}}}) {
    auto spec = residual(c.t);
    spec.build_schema[2].type = fb::DataType_Int64;  // b_lim beside p_w: AST-able as it stands
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(
                  s.exec(), 0,
                  s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"}), i64("b_lim", {5, 5})}), &j,
                  nullptr),
              0)
        << s.error();
    ASSERT_EQ(
        peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 1, 2}), i64("p_w", {3, 9, 9})}),
                           &out, nullptr),
        0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted(c.probe)) << fb::EnumNameJoinType(c.t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    // Both build rows passed the filter against some probe row, so a finish that pads
    // anything is a `matched` the dropped conjunct corrupted — which Inner cannot show.
    if (c.t == fb::JoinType_Left || c.t == fb::JoinType_Full)
      EXPECT_EQ(s.rows(out), sorted(c.finish)) << fb::EnumNameJoinType(c.t);
    else
      EXPECT_EQ(out, 0u) << fb::EnumNameJoinType(c.t);
  }
}

// --- Task 11: typed pads, stats, regions, no exit copy ----------------------

static std::vector<Col> typed_probe() {
  return {i32("p_k", {1}),
          i32("c_i32", {7}),
          i64("c_i64", {8}),
          utf8("c_utf8", {"s"}),
          date32("c_date32", {19000}),
          dec("c_dec2", 15, 2, {1234}),
          dec("c_dec4", 38, 4, {56789}),
          boolean("c_bool", {true}),
          ts_us("c_ts", {1700000000000000})};
}

static std::vector<SField> typed_probe_schema() {
  return {{"p_k", fb::DataType_Int32},
          {"c_i32", fb::DataType_Int32},
          {"c_i64", fb::DataType_Int64},
          {"c_utf8", fb::DataType_Utf8},
          {"c_date32", fb::DataType_Date32},
          {"c_dec2", fb::DataType_Decimal128, 15, 2},
          {"c_dec4", fb::DataType_Decimal128, 38, 4},
          {"c_bool", fb::DataType_Boolean},
          {"c_ts", fb::DataType_TimestampMicrosecond}};
}

// A pad must match in type and scale what a matched row carries, so the assertion is the
// matched row's own schema rather than a list written here. Both pad sources are checked:
// the plan's probe_schema when no batch arrived, and the batch's types when one did.
TEST(JoinSession, APadCarriesTheTypeAndScaleOfTheColumnItStandsFor) {
  JoinSpec spec{.type = fb::JoinType_Left,
                .keys = {{0, 0}},
                .build_schema = {{"b_k", fb::DataType_Int32}},
                .probe_schema = typed_probe_schema()};
  std::shared_ptr<arrow::Schema> want;
  {
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1})}), &j, nullptr), 0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload(typed_probe()), &out, nullptr), 0)
        << s.error();
    EXPECT_EQ(s.rows(out).size(), 1u) << "the matched row every pad must look like";
    want = s.schema(out);
  }
  {  // no probe batch at all: the pads are typed from probe_schema
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1})}), &j, nullptr), 0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    expect_same_columns(s.schema(out), want);
  }
  {  // one probe batch, then a build row it did not match: the pads are the batch's types
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2})}), &j, nullptr), 0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload(typed_probe()), &out, nullptr), 0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out).size(), 1u);
    expect_same_columns(s.schema(out), want);
  }
}

TEST(JoinSession, StatsCountTheOutputsRowsAndItsStringBytes) {
  Session s(make_plan(inner_on_k()));
  uint64_t j = 0, out = 0;
  PeacockNodeStats st{7, 7};
  ASSERT_EQ(peacock_join_build(s.exec(), 0,
                               s.upload({i32("b_k", {1, 2}), utf8("b_v", {"abc", "de"})}), &j, &st),
            0)
      << s.error();
  EXPECT_EQ(st.rows, 0u) << "the build answers no table";
  EXPECT_EQ(st.varlen_content_bytes, 0u);
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 2, 2}), i64("p_w", {10, 20, 21})}),
                         &out, &st),
      0)
      << s.error();
  EXPECT_EQ(st.rows, 3u);                  // (1,abc) once, (2,de) twice
  EXPECT_EQ(st.varlen_content_bytes, 7u);  // "abc" + "de" + "de"
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
}

TEST(JoinSession, AProbeThatAnswersNoTableReportsNoRows) {
  Session s(make_plan(typed(fb::JoinType_LeftSemi)));
  uint64_t j = 0, out = 0;
  PeacockNodeStats st{7, 7};
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out, &st), 0)
      << s.error();
  EXPECT_EQ(out, 0u);
  EXPECT_EQ(st.rows, 0u);
  EXPECT_EQ(st.varlen_content_bytes, 0u);
}

TEST(JoinSession, EachJoinCallOpensOneRegionForItsNode) {
  ASSERT_EQ(peacock_set_node_timing(1), 0);
  {
    Session s(make_plan(typed(fb::JoinType_Left)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out,
                                 nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {10})}), &out,
                                 nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    PeacockNodeRegion regions[8]{};
    uint64_t n = 0;
    ASSERT_EQ(peacock_executor_collect_node_regions(s.exec(), regions, 8, &n), 0) << s.error();
    EXPECT_EQ(n, 4u) << "a build, two probes and a finish";
    for (uint64_t i = 0; i < n; ++i) {
      EXPECT_EQ(regions[i].seq, 0u);
      EXPECT_EQ(regions[i].partition, 0u);
      EXPECT_EQ(regions[i].call_index, i);
    }
  }
  ASSERT_EQ(peacock_set_node_timing(0), 0);
}

/// The bare cuDF a probe of this shape is: the sizer, the sized inner join and the two
/// gathers, over tables of the same rows and types. The hash table is built outside the
/// measured scope, as the session builds it in `join_build` and not in the probe.
static Allocated bare_probe_of(int64_t n) {
  cudf::numeric_scalar<int64_t> zero(0), one(1), three(3);
  auto bk = cudf::sequence(static_cast<cudf::size_type>(n), zero, one);
  auto bv = cudf::sequence(static_cast<cudf::size_type>(n), zero, three);
  auto pk = cudf::sequence(static_cast<cudf::size_type>(n), zero, one);
  auto pv = cudf::sequence(static_cast<cudf::size_type>(n), zero, three);
  cudf::table_view build{{bk->view(), bv->view()}}, probe{{pk->view(), pv->view()}};
  auto keys = build.select({0});
  auto probe_keys = probe.select({0});
  cudf::hash_join hj(keys, cudf::null_equality::UNEQUAL);
  return allocated_by([&] {
    auto size = hj.inner_join_size(probe_keys);
    auto [pi, bi] = hj.inner_join(probe_keys, size);
    auto idx = [](rmm::device_uvector<cudf::size_type> const& v) {
      return cudf::column_view{cudf::data_type{cudf::type_id::INT32},
                               static_cast<cudf::size_type>(v.size()), v.data(), nullptr, 0};
    };
    auto bg = cudf::gather(build, idx(*bi));
    auto pg = cudf::gather(probe, idx(*pi));
    EXPECT_EQ(bg->num_rows(), n);
    EXPECT_EQ(pg->num_rows(), n);
  });
}

// #154's join.cpp sites: the session moves its gathered columns into the output, so a deep
// copy at the exit would show as a second output-sized allocation at the peak. The peak is
// bounded by the same cuDF calls on tables of the same shape rather than by a formula, and
// `net` is bounded both ways — an upper bound is the only thing that can see the session
// retaining the probe batch it was handed, which no peak would move.
TEST(JoinSession, AProbeHandsItsGatheredColumnsOverWithoutACopy) {
  constexpr int64_t n = 1 << 20;  // 1:1 keys, one int64 payload a side
  std::vector<std::optional<int64_t>> k(n), v(n);
  for (int64_t i = 0; i < n; ++i) {
    k[i] = i;
    v[i] = i * 3;
  }
  Session s(
      make_plan({.type = fb::JoinType_Inner,
                 .keys = {{0, 0}},
                 .build_schema = {{"b_k", fb::DataType_Int64}, {"b_v", fb::DataType_Int64}},
                 .probe_schema = {{"p_k", fb::DataType_Int64}, {"p_v", fb::DataType_Int64}}}));
  uint64_t j = 0, out = 0, b = 0, probe = 0;
  auto build_up = allocated_by([&] { b = s.upload({i64("b_k", k), i64("b_v", v)}); });
  ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &j, nullptr), 0) << s.error();
  auto probe_up = allocated_by([&] { probe = s.upload({i64("p_k", k), i64("p_v", v)}); });
  auto got = allocated_by(
      [&] { ASSERT_EQ(peacock_join_probe(s.exec(), j, probe, &out, nullptr), 0) << s.error(); });
  auto bare = bare_probe_of(n);
  const int64_t slack = int64_t{1} << 16;  // allocator rounding
  std::cout << "[join exit] build " << build_up.net << " probe " << probe_up.net << " | bare peak "
            << bare.peak << " total " << bare.total << " | probe peak " << got.peak << " total "
            << got.total << " net " << got.net << "\n";
  EXPECT_GE(got.net, build_up.net - build_up.net / 16)
      << "the output is what the call leaves behind, the probe batch having gone with it";
  EXPECT_LT(got.net, build_up.net + slack)
      << "net " << got.net << ": the session is still holding the probe batch it consumed";
  EXPECT_LT(got.peak, bare.peak + slack)
      << "peak " << got.peak << " against the same cuDF work's " << bare.peak
      << ": a deep copy at the exit would add a second output";
}

// The two refusals the ABI promises leave the session standing are the only two: a failure
// from inside cuDF has consumed the lane's batch, so it must end the query instead. cuDF
// throws `data_type_error`, which is a `std::invalid_argument`, on mismatched key types —
// the shape that would read as a refusal if the session's marker shared that type.
TEST(JoinSession, AFailureInsideCudfEndsTheQueryWhileTheTwoRefusalsDoNot) {
  // An unknown join id leaves the session usable: a later call on a live join still answers.
  {
    Session s(make_plan(inner_on_k()));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j,
                                 nullptr),
              0)
        << s.error();
    EXPECT_NE(peacock_join_probe(s.exec(), 999, s.upload({i32("p_k", {1}), i64("p_w", {7})}), &out,
                                 nullptr),
              0);
    EXPECT_NE(s.error().find("unknown join"), std::string::npos) << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1}), i64("p_w", {7})}), &out,
                                 nullptr),
              0)
        << s.error();
    EXPECT_EQ(s.rows(out), sorted({"1|a|1|7"})) << "the session was still standing";
  }
  // A probe whose key type does not match the build's fails inside cuDF, after the batch was
  // consumed: the query ends, and the next call says so rather than answering.
  {
    JoinSpec spec = inner_on_k();
    spec.probe_schema[0].type = fb::DataType_Int64;
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j,
                                 nullptr),
              0)
        << s.error();
    EXPECT_NE(peacock_join_probe(s.exec(), j, s.upload({i64("p_k", {1}), i64("p_w", {7})}), &out,
                                 nullptr),
              0)
        << "an Int64 probe key against an Int32 build key";
    EXPECT_NE(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_NE(s.error().find("no plan loaded"), std::string::npos)
        << "a failure inside cuDF must end the query: " << s.error();
  }
}

// A keyed join whose residual names no column: the literal `true` with an empty
// `filter_columns`, which is what a planner writes for it. The pairs path evaluates the
// residual over the key matches, and a residual of constants must keep every one of them.
TEST(JoinSession, AKeyedResidualOfConstantsKeepsEveryKeyMatch) {
  for (auto t : {fb::JoinType_Inner, fb::JoinType_Left, fb::JoinType_Right, fb::JoinType_Full}) {
    auto spec = typed(t);
    spec.filter = [](auto& f) { return lit_bool(f, true); };  // and no filter_columns
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0) << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2, 5}), i64("p_w", {20, 50})}),
                                 &out, nullptr),
              0)
        << s.error();
    Rows want{"2|b|2|20", "2|c|2|20"};
    if (t == fb::JoinType_Right || t == fb::JoinType_Full) want.push_back("NULL|NULL|5|50");
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    if (t == fb::JoinType_Left || t == fb::JoinType_Full)
      EXPECT_EQ(s.rows(out), sorted({"1|a|NULL|NULL", "NULL|n|NULL|NULL"}))
          << fb::EnumNameJoinType(t);
    else
      EXPECT_EQ(out, 0u) << fb::EnumNameJoinType(t);
  }
}

// The same filter with a ColumnRef and no map is a malformed plan, and is refused by name
// rather than reading the column `residual_mask` gives the view its row count from.
TEST(JoinSession, AFilterNamingAColumnWithNoMapIsRefusedByName) {
  auto spec = typed(fb::JoinType_Inner);
  spec.filter = [](auto& f) { return col(f, 0); };  // and no filter_columns
  Session s(make_plan(spec));
  uint64_t j = 0;
  EXPECT_NE(peacock_join_build(s.exec(), 0, s.upload(build_k_v()), &j, nullptr), 0);
  EXPECT_NE(s.error().find("out of range of filter_columns"), std::string::npos) << s.error();
}

// --- Task 11b: the rest of §5.6's named cases -------------------------------

// A present-but-empty projection keeps no column, which no plan asks for: the session
// refuses it as a planner bug. An absent one keeps every column.
TEST(JoinSession, AnAbsentProjectionKeepsEveryColumnAndAnEmptyOneIsRefused) {
  auto spec = inner_on_k();
  Session all(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(all.exec(), 0, all.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j,
                               nullptr),
            0)
      << all.error();
  ASSERT_EQ(peacock_join_probe(all.exec(), j, all.upload({i32("p_k", {1}), i64("p_w", {7})}), &out,
                               nullptr),
            0)
      << all.error();
  EXPECT_EQ(all.rows(out), sorted({"1|a|1|7"}));
  spec.projection = std::vector<uint32_t>{};
  Session none(make_plan(spec));
  EXPECT_NE(peacock_join_build(none.exec(), 0, none.upload({i32("b_k", {1}), utf8("b_v", {"a"})}),
                               &j, nullptr),
            0);
  EXPECT_NE(none.error().find("zero columns"), std::string::npos) << none.error();
}

// fits_or_throw is what every pairs-producing path calls before an index map is cast to
// size_type.
TEST(JoinSession, APairCountPastSizeTypeIsRefusedByName) {
  const std::size_t past =
      static_cast<std::size_t>(std::numeric_limits<cudf::size_type>::max()) + 1;
  try {
    peacock::join::fits_or_throw(past, "an Inner probe");
    FAIL() << "no refusal";
  } catch (std::runtime_error const& e) {
    EXPECT_NE(std::string(e.what()).find("an Inner probe"), std::string::npos) << e.what();
    EXPECT_NE(std::string(e.what()).find("past what one cuDF table holds"), std::string::npos)
        << e.what();
  }
}

// A composite key with a NULL in its second column never matches under the SQL default.
TEST(JoinSession, ACompositeKeyWithANullInItsSecondColumnNeverMatches) {
  Session s(
      make_plan({.type = fb::JoinType_LeftAnti,
                 .keys = {{0, 0}, {1, 1}},
                 .build_schema = {{"b_a", fb::DataType_Int32}, {"b_b", fb::DataType_Int32}},
                 .probe_schema = {{"p_a", fb::DataType_Int32}, {"p_b", fb::DataType_Int32}}}));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(
      peacock_join_build(s.exec(), 0, s.upload({i32("b_a", {1, 1}), i32("b_b", {std::nullopt, 2})}),
                         &j, nullptr),
      0)
      << s.error();
  ASSERT_EQ(
      peacock_join_probe(s.exec(), j, s.upload({i32("p_a", {1, 1}), i32("p_b", {std::nullopt, 2})}),
                         &out, nullptr),
      0)
      << s.error();
  EXPECT_EQ(out, 0u);
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
  EXPECT_EQ(s.rows(out), sorted({"1|NULL"}));  // (1, NULL) matched nothing: kept
}

// LeftAnti and LeftMark with a residual answer from the build alone when no probe batch
// came — architecture.md's "zero-row batches change no answer" does not hold here.
TEST(JoinSession, AResidualAntiAndMarkOverNoProbeBatchAnswerFromTheBuild) {
  for (auto t : {fb::JoinType_LeftAnti, fb::JoinType_LeftMark}) {
    Session s(make_plan(residual(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(
                  s.exec(), 0,
                  s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"}), i32("b_lim", {5, 5})}), &j,
                  nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    EXPECT_EQ(s.rows(out), t == fb::JoinType_LeftAnti
                               ? sorted({"1|a|5", "2|b|5"})               // every build row
                               : sorted({"1|a|5|false", "2|b|5|false"}))  // every mark false
        << fb::EnumNameJoinType(t);
  }
}

// RightSemi and RightAnti over a build whose keys are all NULL under UNEQUAL: an empty
// build, since no probe key can ever match one.
TEST(JoinSession, ARightSemiOverAnAllNullBuildAnswersNothingAndRightAntiEveryRow) {
  for (auto t : {fb::JoinType_RightSemi, fb::JoinType_RightAnti}) {
    Session s(
        make_plan({.type = t,
                   .keys = {{0, 0}},
                   .build_schema = {{"b_k", fb::DataType_Int32}},
                   .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}}));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {std::nullopt, std::nullopt})}),
                                 &j, nullptr),
              0)
        << s.error();
    ASSERT_EQ(peacock_join_probe(s.exec(), j,
                                 s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 20})}),
                                 &out, nullptr),
              0)
        << s.error();
    EXPECT_EQ(s.rows(out), t == fb::JoinType_RightSemi ? Rows{} : sorted({"1|10", "NULL|20"}))
        << fb::EnumNameJoinType(t);
  }
}

// §3.6's middle case: one conjunct the AST takes and one it does not, so the conditional
// join makes the candidates and the rest is evaluated over them as columns.
TEST(JoinSession, ANestedLoopSplitsItsConjunctsBetweenTheAstAndTheColumns) {
  // Pairs passing `p_v < b_v` over build (1,5) (2,10) and probe 3, 7, 12: (b1,3) (b2,3)
  // (b2,7). The decimal conjunct is true for every pair here, so each type's answer is the
  // one its own branch of `derive` and `kept_probe_rows` owes.
  struct C {
    fb::JoinType t;
    Rows probe;
    std::optional<Rows> finish;
  };
  const std::vector<C> cases{
      {fb::JoinType_Inner, {"1|5|3", "2|10|3", "2|10|7"}, std::nullopt},
      {fb::JoinType_Left, {"1|5|3", "2|10|3", "2|10|7"}, Rows{}},
      {fb::JoinType_Right, {"1|5|3", "2|10|3", "2|10|7", "NULL|NULL|12"}, std::nullopt},
      {fb::JoinType_Full, {"1|5|3", "2|10|3", "2|10|7", "NULL|NULL|12"}, Rows{}},
      {fb::JoinType_LeftSemi, {}, Rows{"1|5", "2|10"}},
      {fb::JoinType_LeftAnti, {}, Rows{}},
      {fb::JoinType_LeftMark, {}, Rows{"1|5|true", "2|10|true"}},
      {fb::JoinType_RightSemi, {"3", "7"}, std::nullopt},
      {fb::JoinType_RightAnti, {"12"}, std::nullopt},
  };
  for (uint64_t chunk_bytes : {uint64_t{0}, uint64_t{16}})
    for (auto const& c : cases) {
      JoinSpec spec{.type = c.t,
                    .build_schema = {{"b_id", fb::DataType_Int32}, {"b_v", fb::DataType_Int32}},
                    .probe_schema = {{"p_v", fb::DataType_Int32}},
                    .chunk_bytes = chunk_bytes};
      // p_v < b_v AND cast(p_v as decimal) <> cast(b_v as decimal): the first is AST-able as
      // it stands, the second stays refused even with both operands hoisted.
      spec.filter = [](auto& f) {
        return bin(f, bin(f, col(f, 0), fb::BinaryOp_Lt, col(f, 1)), fb::BinaryOp_And,
                   bin(f, cast_dec(f, col(f, 0), 15, 2), fb::BinaryOp_NotEq,
                       cast_dec(f, col(f, 1), 15, 2)));
      };
      spec.filter_columns = {{fb::JoinSide_Right, 0}, {fb::JoinSide_Left, 1}};
      Session s(make_plan(spec));
      uint64_t j = 0, out = 0;
      ASSERT_EQ(peacock_join_build(
                    s.exec(), 0, s.upload({i32("b_id", {1, 2}), i32("b_v", {5, 10})}), &j, nullptr),
                0)
          << s.error();
      ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_v", {3, 7, 12})}), &out, nullptr),
                0)
          << s.error();
      const std::string where =
          std::string(fb::EnumNameJoinType(c.t)) + " chunk_bytes=" + std::to_string(chunk_bytes);
      if (is_build_side_semi(c.t))
        EXPECT_EQ(out, 0u) << where;
      else
        EXPECT_EQ(s.rows(out), sorted(c.probe)) << where;
      ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
      if (c.finish)
        EXPECT_EQ(s.rows(out), sorted(*c.finish)) << where;
      else
        EXPECT_EQ(out, 0u) << where;
    }
}

// A nested loop over no probe batch at all: the finish answers from the build alone, so
// architecture.md's "zero-row batches change no answer" does not reach this far.
TEST(JoinSession, ANestedLoopOverNoProbeBatchFinishesFromTheBuild) {
  for (auto t : {fb::JoinType_Left, fb::JoinType_Full, fb::JoinType_Right}) {
    Session s(make_plan(unconditional(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_id", {1, 2})}), &j, nullptr), 0)
        << s.error();
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0) << s.error();
    if (t == fb::JoinType_Right)
      EXPECT_EQ(out, 0u) << fb::EnumNameJoinType(t);
    else
      EXPECT_EQ(s.rows(out), sorted({"1|NULL", "2|NULL"})) << fb::EnumNameJoinType(t);
  }
}

// A side of no columns reads as no rows whatever it held, so the one refusal is
// TableResult's. Reachable only this way: an absent build side typed from a schema of no
// fields — every uploaded batch was refused one layer below before it could get here.
TEST(JoinSession, ASideOfNoColumnsIsRefused) {
  Session s(make_plan({.type = fb::JoinType_Inner,
                       .keys = {{0, 0}},
                       .build_schema = {},
                       .probe_schema = {{"p_k", fb::DataType_Int32}}}));
  uint64_t j = 0;
  EXPECT_NE(peacock_join_build(s.exec(), 0, 0, &j, nullptr), 0);
  EXPECT_NE(s.error().find("no columns"), std::string::npos) << s.error();
}

// Join tables here are tens of rows; 1 GiB is a floor, as in test_plan_executor.cpp.
constexpr std::size_t kPoolBytes = 1ull << 30;

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  peacock::install_rmm_pool(kPoolBytes);
  return RUN_ALL_TESTS();
}
