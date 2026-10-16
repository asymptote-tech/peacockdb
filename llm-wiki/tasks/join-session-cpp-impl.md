# join-session-cpp implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a C++ join session — `CudfJoin`, the four `peacock_join_*` symbols, and the session that
answers every join type the design names — proved by a hand-counted gtest matrix, with nothing in
Rust calling it yet.

**Architecture:** the session lives in a new `cpp/src/operators/join_session.{h,cpp}` beside the
old `join.cpp`, which keeps serving today's plans until join-backend deletes it (the spec's "join.cpp
rewritten" lands as: the session written fresh here, the old file removed there). `NodeSession`
gains a join map and four methods; `gpu_executor.cpp` wraps them as C symbols with the same failure
policy as `execute_node`. Each arm group is written red first in `test_join_session.cpp`, which
drives the session only through the public C ABI (plan bytes in, Arrow batches up through
`peacock_handle_from_arrow`, answers down through `peacock_result_from_handle`).

**Tech stack:** C++20, libcudf 25.02 (shad-gpu) with the 25.10a CI leg compiling, FlatBuffers,
Arrow C++ (tests only), gtest.

**Spec:** [`join-session-cpp.md`](join-session-cpp.md) — frozen; the design it implements is
[`join-rewrite-design.md`](join-rewrite-design.md) §1, §2, §3 and §5.6. Read §3 whole before Task 3.

> **Round 1 (2026-10-09) worked this plan.** Every `Commit` step is left unticked: the developer
> never mutates git state, and the coordinator commits. The device cycles ran on nebius-gpu under
> the board's host override, not on shad-gpu. Task 8 was done after Tasks 9 and 10, and the file
> was split in four at the end of Task 10 — both recorded, with the reasons, in
> [`join-session-cpp-detail.md`](join-session-cpp-detail.md).

## Global constraints

- Prerequisite: refcounted-scatter has landed, so `TableResult` is the per-column shape
  (`owners`, `columns`, `column_names`, `view()`, `owning`, `slice`, `select`, `with`). Nothing here
  changes `TableResult`.
- Only the portable cuDF surface: `hash_join(Bk, cmp)` (two-argument constructor),
  `distinct_hash_join(keys, cmp)`, the free and `hash_join::*_size` sizers, `conditional_*`,
  `mixed_left_semi_join`, `cross_join`, `gather`, `apply_boolean_mask`, `contains`, `sequence`,
  `distinct`, `drop_nulls`, `tile`, `repeat`, `concatenate`, `binary_operation`,
  `unary_operation`, `replace_nulls`, `make_column_from_scalar`, `make_default_constructed_scalar`.
  No `filtered_join`, no `filter_join_indices`, no `hash_join::full_join`, no `scatter` with a
  join map, no `mixed_left_anti_join`, no `conditional_left_anti_join` (anti is semi plus the
  complement everywhere, design §3.4 D13).
- Headers by `__has_include(<cudf/join/join.hpp>)` as `join.cpp` does; no `CUDF_VERSION` test.
- No Rust behaviour change, no golden moves; the existing gpu tier stays green (old paths serve it).
- No rows-only arm: a side or an output of zero columns is refused as a planner bug.
- Device cycles foreground on shad-gpu (`build-test-shadgpu.sh`); never build in a workspace that
  shares a cargo target dir.
- Commits at most 10 lines of message, one per task.

## Review Focus

1. **NULL keys under UNEQUAL in a finishing type** — Left/Full must pad the NULL-key build row,
   never drop it (the latent finish defect). Test in Task 3.
2. **A NULL residual or preserved-side condition** — a NULL condition is no match: an anti join
   keeps the row, a mark is `false`, never NULL. Test in Task 6.
3. **Duplicate build keys under RightSemi/RightAnti** — `distinct_hash_join` is undefined on
   duplicates; the session must dedupe first and own the deduped table. Test in Task 4.
4. **A probe batch whose key-match count is far larger than the batch** — chunking must give the
   same rows as the unchunked path. Test in Task 6 (pairs) and Task 9 (nested loop).
5. **Zero-row and absent sides at every type** — no build batch, a zero-row build, no probe
   batch, a zero-row probe between two with rows. Test in Task 5.

## File structure

| file | responsibility |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `CudfJoin`; the union member (the `DataType` timestamp variants are repartition-keys') |
| `cpp/src/peacock/expr.h`, `cpp/src/expr.cpp` | `JoinFilterColMap` as a span |
| `cpp/src/operators/join_session.h` (new) | `JoinSession`'s declaration |
| `cpp/src/operators/join_session.cpp` (new) | the helpers of §3.0 and every arm of §3.1–§3.9 |
| `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp` | the join map; `join_build/probe/finish/release`; `CudfJoin` a leaf; `varlen_content_bytes` exposed |
| `cpp/src/operators/dispatch.cpp` | `CudfJoin` through `execute_node` refused by name |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | the four C symbols |
| `peacockdb-ffi/src/lib.rs` | their declarations |
| `cpp/tests/gpu/test_join_session.cpp` (new), `cpp/CMakeLists.txt` | the matrix; target `peacock_join_session_tests`, label `gpu` |
| `llm-wiki/architecture.md`, `build-test.md` | the symbols; the new target and its count |

---

### Task 1: `CudfJoin` on the wire, a leaf that `execute_node` refuses

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs` (after `CudfNestedLoopJoin`; the union at `:577`)
- Modify: `cpp/src/node_session.cpp:246` (`node_children`), `cpp/src/operators/dispatch.cpp` (kind name and the dispatch arm)
- Create: `cpp/tests/gpu/test_join_session.cpp`; Modify: `cpp/CMakeLists.txt` (after `peacock_plan_tests`)

**Interfaces:**
- Consumes: `fb::DataType_TimestampSecond … _TimestampNanosecond` and their `fb_to_type_id` arms
  (repartition-keys, earlier in the chain — the wire's timestamp types are its, so a join side's
  schema can name a timestamp column here).
- Produces: `fb::CudfJoin`, `fb::PlanNodeKind_CudfJoin`.

- [x] **Step 1: The fbs.** Append, never reorder:

```
/// A join answered through the session symbols (peacock_join_*), never execute_node: a leaf,
/// since its inputs arrive by call. join-rewrite-design.md §1.1.
table CudfJoin {
  join_type: JoinType;                  // the nine; no RightMark (#241)
  keys: [JoinKey];                      // ColumnRef pairs; empty = nested loop or cross
  filter: Expr;                         // residual; refs index the filter's own schema
  filter_columns: [JoinFilterColumn];   // filter schema ordinal -> (side, column)
  null_equals_null: bool = false;       // honoured by every type
  build_schema: Schema;                 // types of an absent build side, and pad types
  probe_schema: Schema;                 // pad types before any probe batch arrived
  projection: [uint32];                 // absent = keep all; present = these, never empty
  chunk_bytes: uint64 = 0;              // pair/cross scratch per call; 0 = 1 GiB
}
```

  `PlanNodeKind` gains `CudfJoin` after `CudfWindow`. The timestamp `DataType` variants and their
  `fb_to_type_id` arms already exist (repartition-keys appended them); this task only reads them.
  `chunk_bytes` is not in design §1.1: it is the "scratch budget" §3.4 and §3.6 read, which the C++
  side has no other way to learn; join-backend writes it from the planner's budget (its Task 7),
  and the gtests set it small to force chunking.
- [x] **Step 2: Check the prerequisite.** `grep -n 'TimestampMicrosecond' flatbuffers/gpu_plan.fbs
  cpp/src/expr.cpp` finds the enum value and its `fb_to_type_id` arm; if not, stop — repartition-keys
  has not landed.

- [x] **Step 3: A leaf, refused through `execute_node`.** `node_children`: `case
  fb::PlanNodeKind_CudfJoin: return {};`. `dispatch.cpp`'s kind-name switch: `"CudfJoin"`; its
  execute switch:

```cpp
      case fb::PlanNodeKind_CudfJoin:
        throw std::runtime_error(
            "CudfJoin runs through peacock_join_build/probe/finish, not execute_node");
```

- [x] **Step 4: The test target.** `CMakeLists.txt`, after `peacock_plan_tests`:

```cmake
# The join session through its C ABI: plan bytes in, Arrow batches up, IPC answers down.
add_executable(peacock_join_session_tests tests/gpu/test_join_session.cpp)
target_include_directories(peacock_join_session_tests PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/include ${CMAKE_CURRENT_SOURCE_DIR}/src ${FBS_GENERATED_DIR}/..)
target_link_libraries(peacock_join_session_tests PRIVATE peacock_gpu cudf::cudf flatbuffers
  Arrow::arrow_shared GTest::gtest)
set_target_properties(peacock_join_session_tests PROPERTIES CXX_STANDARD 20 CXX_STANDARD_REQUIRED ON)
add_dependencies(peacock_join_session_tests gpu_plan_fbs)
add_test(NAME peacock_join_session_tests COMMAND peacock_join_session_tests)
set_tests_properties(peacock_join_session_tests PROPERTIES LABELS "gpu")
```

  and, in the `# Install` block (`:321-327`), the binary joins the installed list and the rpath
  list — CI and shad-gpu run only `cpp/install/bin/peacock_*_tests` (`pipeline.yml`'s glob,
  `build-test-shadgpu.sh:414`), so a target that is only built is never run:

```cmake
install(TARGETS peacock_gpu_tests peacock_cpu_tests peacock_plan_tests peacock_tpch_tests
        peacock_tpchv_tests peacock_join_session_tests RUNTIME DESTINATION bin)
set_target_properties(peacock_cpu_tests peacock_gpu_tests peacock_plan_tests
  peacock_join_session_tests PROPERTIES INSTALL_RPATH "$ORIGIN/../lib")
```

  No `GTest::gtest_main`: the file ends with its own `main`, which installs the RMM pool and the
  statistics adaptor above it as `test_plan_executor.cpp:2114` does (Task 11's allocation check
  reads that adaptor):

```cpp
// Join tables here are tens of rows; 1 GiB is a floor, as in test_plan_executor.cpp.
constexpr std::size_t kPoolBytes = 1ull << 30;

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  peacock::install_rmm_pool(kPoolBytes);
  return RUN_ALL_TESTS();
}
```

  (`#include "peacock/rmm_pool.hpp"` joins the includes of Step 5.)
- [x] **Step 5: The harness and the first test.** `test_join_session.cpp` opens with the helpers
  every later task uses. Upload and download go through the public ABI; the plan is one `CudfJoin`
  root.

```cpp
// The join session through its C ABI (join-rewrite-design.md §5.6). Every answer is
// hand-counted: tables of a few rows, rows compared as sorted text.
#include "peacock_gpu.h"
#include "peacock/rmm_pool.hpp"
#include "generated/gpu_plan_generated.h"

#include <arrow/api.h>
#include <arrow/c/bridge.h>
#include <arrow/io/memory.h>
#include <arrow/ipc/reader.h>
#include <flatbuffers/flatbuffers.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <functional>
#include <optional>
#include <string>
#include <vector>

namespace fb = peacock::fb;
using Rows = std::vector<std::string>;

struct Col {                                          // one Arrow column with its name
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
static Col i32(std::string n, std::vector<std::optional<int32_t>> v) { return {n, build(v, arrow::Int32Builder())}; }
static Col i64(std::string n, std::vector<std::optional<int64_t>> v) { return {n, build(v, arrow::Int64Builder())}; }
static Col utf8(std::string n, std::vector<std::optional<std::string>> v) { return {n, build(v, arrow::StringBuilder())}; }
static Col boolean(std::string n, std::vector<std::optional<bool>> v) { return {n, build(v, arrow::BooleanBuilder())}; }
static Col date32(std::string n, std::vector<std::optional<int32_t>> v) { return {n, build(v, arrow::Date32Builder())}; }
static Col dec(std::string n, int p, int s, std::vector<std::optional<int64_t>> unscaled) {
  arrow::Decimal128Builder b(arrow::decimal128(p, s));
  for (auto const& x : unscaled) (x ? b.Append(arrow::Decimal128(*x)) : b.AppendNull()).ok();
  std::shared_ptr<arrow::Array> out;
  EXPECT_TRUE(b.Finish(&out).ok());
  return {n, out};
}
static Col ts_us(std::string n, std::vector<std::optional<int64_t>> v) {
  return {n, build(v, arrow::TimestampBuilder(arrow::timestamp(arrow::TimeUnit::MICRO),
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
    auto buf = std::make_shared<arrow::Buffer>(ipc, static_cast<int64_t>(len));
    auto reader = arrow::ipc::RecordBatchStreamReader::Open(
                      std::make_shared<arrow::io::BufferReader>(buf)).ValueOrDie();
    std::shared_ptr<arrow::RecordBatch> b;
    while (reader->ReadNext(&b).ok() && b) {
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

 private:
  std::vector<uint8_t> plan_;
  peacock_executor_t* exec_ = nullptr;
};

static Rows sorted(Rows r) { std::sort(r.begin(), r.end()); return r; }

/// What a `CudfJoin` says; `make_plan` serializes it as the plan's one node.
struct JoinSpec {
  fb::JoinType type = fb::JoinType_Inner;
  std::vector<std::pair<uint32_t, uint32_t>> keys;          // (build col, probe col)
  std::function<flatbuffers::Offset<fb::Expr>(flatbuffers::FlatBufferBuilder&)> filter;
  std::vector<std::pair<fb::JoinSide, uint32_t>> filter_columns;
  bool null_equals_null = false;
  std::vector<std::pair<std::string, fb::DataType>> build_schema, probe_schema;
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

static flatbuffers::Offset<fb::Schema> schema_of(
    flatbuffers::FlatBufferBuilder& f,
    std::vector<std::pair<std::string, fb::DataType>> const& fields) {
  std::vector<flatbuffers::Offset<fb::Field>> out;
  for (auto const& [n, t] : fields) out.push_back(fb::CreateField(f, f.CreateString(n), t, true));
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
  auto proj = s.projection ? f.CreateVector(*s.projection) : flatbuffers::Offset<flatbuffers::Vector<uint32_t>>{};
  auto keys_v = f.CreateVector(keys);
  auto fcs_v = f.CreateVectorOfStructs(fcs);
  auto join = fb::CreateCudfJoin(f, s.type, keys_v, filter, fcs_v, s.null_equals_null, build, probe,
                                 proj, s.chunk_bytes);
  auto node = fb::CreatePlanNode(f, fb::PlanNodeKind_CudfJoin, join.Union());
  f.Finish(fb::CreateGpuPlan(f, node));
  return {f.GetBufferPointer(), f.GetBufferPointer() + f.GetSize()};
}
```

  Then the first case, red until Step 6:

```cpp
TEST(JoinSession, ExecuteNodeRefusesACudfJoin) {
  Session s(make_plan({.build_schema = {{"b_k", fb::DataType_Int32}},
                       .probe_schema = {{"p_k", fb::DataType_Int32}}}));
  uint64_t out = 0, n = 0;
  PeacockNodeStats st{};
  EXPECT_NE(peacock_executor_execute_node(s.exec(), 0, nullptr, nullptr, 0, &out, 1, &n, &st), 0);
  EXPECT_NE(s.error().find("runs through peacock_join_build"), std::string::npos) << s.error();
}
```

- [x] **Step 6:** `build-test-shadgpu.sh --build`, then `--run`: the shad-gpu log names
  `peacock_join_session_tests` among the installed binaries it ran (if it does not, Step 4's install
  line is missing), and the case is green (it was red as "unknown node kind" before Step 3). The
  existing gpu tier green.
- [ ] **Step 7: Commit.** `git commit -m "CudfJoin on the wire: a leaf execute_node refuses"`.

### Task 2: The session's lifecycle, and Inner

**Files:**
- Create: `cpp/src/operators/join_session.h`, `cpp/src/operators/join_session.cpp`
- Modify: `cpp/src/plan_executor.h` (`NodeSession`), `cpp/src/node_session.cpp` (`Impl`, the four methods, `varlen_content_bytes` made non-static and declared in `plan_executor_internal.h`)
- Modify: `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `peacockdb-ffi/src/lib.rs`
- Modify: `cpp/CMakeLists.txt` (`join_session.cpp` into `peacock_gpu`'s sources, beside `join.cpp`)
- Test: `cpp/tests/gpu/test_join_session.cpp`

**Interfaces:**
- Consumes: `fb::CudfJoin` (Task 1); `TableResult` (refcounted-scatter).
- Produces:
  - C: `int peacock_join_build(peacock_executor_t*, uint64_t seq, uint64_t build, uint64_t* out_join, PeacockNodeStats*)`, `int peacock_join_probe(peacock_executor_t*, uint64_t join, uint64_t probe, uint64_t* out_handle, PeacockNodeStats*)`, `int peacock_join_finish(peacock_executor_t*, uint64_t join, uint64_t* out_handle, PeacockNodeStats*)`, `void peacock_join_release(peacock_executor_t*, uint64_t join)`. `*out_handle == 0` means no table.
  - C++: `uint64_t NodeSession::join_build(uint64_t seq, uint64_t build, NodeStats*)`, `uint64_t join_probe(uint64_t join, uint64_t probe, NodeStats*)`, `uint64_t join_finish(uint64_t join, NodeStats*)`, `void join_release(uint64_t join)`.
  - `class JoinSession { JoinSession(const fb::CudfJoin*, std::optional<TableResult> build); std::optional<TableResult> probe(TableResult); std::optional<TableResult> finish(); }`.

- [x] **Step 1: The lifecycle cases, red** (they fail to link until Step 3):

```cpp
static JoinSpec inner_on_k() {
  return {.type = fb::JoinType_Inner, .keys = {{0, 0}},
          .build_schema = {{"b_k", fb::DataType_Int32}, {"b_v", fb::DataType_Utf8}},
          .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}};
}

TEST(JoinSession, AnInnerJoinAnswersEachProbeBatchAgainstOneBuild) {
  Session s(make_plan(inner_on_k()));
  auto b = s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})});
  uint64_t join = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &join, nullptr), 0) << s.error();
  uint64_t out = 0;
  ASSERT_EQ(peacock_join_probe(s.exec(), join, s.upload({i32("p_k", {2, 3}), i64("p_w", {20, 30})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  ASSERT_EQ(peacock_join_probe(s.exec(), join, s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 0})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"1|a|1|10"}));                 // NULL never matches NULL
  ASSERT_EQ(peacock_join_finish(s.exec(), join, &out, nullptr), 0);
  EXPECT_EQ(out, 0u);                                          // Inner has no finish
  peacock_join_release(s.exec(), join);
}

TEST(JoinSession, AProbeAfterFinishIsRefused) {
  Session s(make_plan(inner_on_k()));
  uint64_t join = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &join, nullptr), 0);
  ASSERT_EQ(peacock_join_finish(s.exec(), join, &out, nullptr), 0);
  EXPECT_NE(peacock_join_probe(s.exec(), join, s.upload({i32("p_k", {1}), i64("p_w", {1})}), &out, nullptr), 0);
  EXPECT_NE(s.error().find("finished"), std::string::npos) << s.error();
}

TEST(JoinSession, AnUnknownJoinIdIsRefusedWithoutEndingTheQuery) {
  Session s(make_plan(inner_on_k()));
  uint64_t out = 0;
  auto h = s.upload({i32("p_k", {1}), i64("p_w", {1})});
  EXPECT_NE(peacock_join_finish(s.exec(), 999, &out, nullptr), 0);
  EXPECT_NE(s.error().find("unknown join"), std::string::npos);
  EXPECT_EQ(s.rows(h).size(), 1u);                             // the session is still standing
}

TEST(JoinSession, ReleaseWithoutFinishAndEndPlanFreeTheSession) {
  Session s(make_plan(inner_on_k()));
  uint64_t a = 0, b = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &a, nullptr), 0);
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {2}), utf8("b_v", {"b"})}), &b, nullptr), 0);
  peacock_join_release(s.exec(), a);
  peacock_join_release(s.exec(), a);                           // idempotent, as handle release is
  uint64_t out = 0;
  EXPECT_NE(peacock_join_finish(s.exec(), a, &out, nullptr), 0);
  peacock_executor_end_plan(s.exec());                         // b is freed with the plan
}
```

- [x] **Step 2: `join_session.h`.**

```cpp
#pragma once
// PRIVATE header. A join answered by calls, not by execute_node: built once, probed per
// batch, finished once (join-rewrite-design.md §1–§3).
#include "plan_executor.h"
#include "generated/gpu_plan_generated.h"

#include <cudf/join/hash_join.hpp>  // see join_session.cpp for the 25.02 fallback include
#include <cudf/column/column.hpp>
#include <cudf/table/table.hpp>

#include <memory>
#include <optional>
#include <vector>

namespace peacock {

class JoinSession {
 public:
  JoinSession(const fb::CudfJoin* desc, std::optional<TableResult> build);
  ~JoinSession();
  /// One probe batch; nullopt for the types that answer only at finish.
  std::optional<TableResult> probe(TableResult batch);
  /// Once, after the last probe; nullopt for the types with no finish.
  std::optional<TableResult> finish();

 private:
  struct State;                      // every cuDF object and column the session owns
  std::unique_ptr<State> s_;
};

}  // namespace peacock
```

  Put the header switch in the `.cpp` and forward-declare `cudf::hash_join` and
  `cudf::distinct_hash_join` in the header instead of including, so the header compiles on both
  layouts: `namespace cudf { class hash_join; class distinct_hash_join; }` (25.02 and 26.02 both
  declare non-template classes of those names).
- [x] **Step 3: `NodeSession`'s join map and methods.** In `Impl`:

```cpp
  std::unordered_map<uint64_t, std::unique_ptr<JoinSession>> joins;
  uint64_t next_join = 1;
  /// Which seq each join belongs to — its regions are that node's.
  std::unordered_map<uint64_t, uint64_t> join_seq;
```

  The methods, in `node_session.cpp` after `slice_handle`:

```cpp
namespace {
TableResult take(std::unordered_map<uint64_t, TableResult>& reg, uint64_t h, const char* who) {
  auto it = reg.find(h);
  if (it == reg.end()) throw std::runtime_error(std::string(who) + ": unknown input handle");
  TableResult t = std::move(it->second);
  reg.erase(it);
  return t;
}
}  // namespace

uint64_t NodeSession::join_build(uint64_t seq, uint64_t build, NodeStats* out_stats) {
  if (seq >= impl_->post_order.size()) throw std::runtime_error("join_build: seq out of range");
  const fb::PlanNode* node = impl_->post_order[seq];
  if (node->node_type() != fb::PlanNodeKind_CudfJoin)
    throw std::runtime_error("join_build: seq " + std::to_string(seq) + " is not a CudfJoin");
  RegionSink* sink = impl_->measuring();
  const uint64_t call = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  std::optional<TableResult> b;
  if (build != 0) b = take(impl_->registry, build, "join_build");
  ScopedNodeTimer timer(sink, seq, 0, call);
  auto session = std::make_unique<JoinSession>(node->node_as_CudfJoin(), std::move(b));
  timer.stop();
  if (out_stats) *out_stats = NodeStats{};       // the build answers no table
  uint64_t id = impl_->next_join++;
  impl_->joins.emplace(id, std::move(session));
  impl_->join_seq.emplace(id, seq);
  return id;
}

uint64_t NodeSession::join_probe(uint64_t join, uint64_t probe, NodeStats* out_stats) {
  auto it = impl_->joins.find(join);
  if (it == impl_->joins.end()) throw std::invalid_argument("join_probe: unknown join " + std::to_string(join));
  const uint64_t seq = impl_->join_seq.at(join);
  RegionSink* sink = impl_->measuring();
  const uint64_t call = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  TableResult batch = take(impl_->registry, probe, "join_probe");
  ScopedNodeTimer timer(sink, seq, 0, call);
  auto out = it->second->probe(std::move(batch));
  timer.stop();
  return register_join_output(seq, std::move(out), out_stats, sink);
}

uint64_t NodeSession::join_finish(uint64_t join, NodeStats* out_stats) {
  auto it = impl_->joins.find(join);
  if (it == impl_->joins.end()) throw std::invalid_argument("join_finish: unknown join " + std::to_string(join));
  const uint64_t seq = impl_->join_seq.at(join);
  RegionSink* sink = impl_->measuring();
  const uint64_t call = sink ? sink->take_call_index(seq, impl_->post_order.size()) : 0;
  ScopedNodeTimer timer(sink, seq, 0, call);
  auto out = it->second->finish();
  timer.stop();
  return register_join_output(seq, std::move(out), out_stats, sink);
}

void NodeSession::join_release(uint64_t join) {
  impl_->joins.erase(join);
  impl_->join_seq.erase(join);
}

/// 0 when the call answers no table; otherwise a fresh handle with its stats.
uint64_t NodeSession::register_join_output(uint64_t seq, std::optional<TableResult> out,
                                           NodeStats* out_stats, RegionSink* sink) {
  if (out_stats) *out_stats = NodeStats{};
  if (!out) return 0;
  auto tv = out->view();
  if (out_stats) *out_stats = NodeStats{static_cast<uint64_t>(tv.num_rows()), varlen_content_bytes(tv)};
  uint64_t handle = impl_->next_handle++;
  impl_->note_producer(sink, handle, seq);
  impl_->registry.emplace(handle, std::move(*out));
  return handle;
}
```

  `register_join_output` is a private member declared in `plan_executor.h` (it takes `RegionSink*`,
  which stays an incomplete type there). `std::invalid_argument` marks the two validation refusals
  so the C wrapper can keep the session (Step 4). `Impl`'s destructor needs nothing new: the
  `joins` map frees its sessions with the plan.
- [x] **Step 4: The C symbols** (`peacock_gpu.h` after `peacock_executor_slice_handle`, with the
  design's §1.2 text as their comments; `gpu_executor.cpp`):

```cpp
int peacock_join_build(peacock_executor_t* executor, uint64_t seq, uint64_t build,
                       uint64_t* out_join, PeacockNodeStats* out_stats) {
  if (!executor || !out_join) return 1;
  if (!executor->session) { executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)"; return 1; }
  try {
    *out_join = executor->session->join_build(seq, build, reinterpret_cast<peacock::NodeStats*>(out_stats));
    return 0;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    executor->session.reset();                       // work had begun: end the query
    return 1;
  }
}

/// Shared by probe and finish: an unknown join id is a validation refusal and leaves the
/// session standing; anything else ends the query, as execute_node does.
template <typename F>
static int join_call(peacock_executor_t* executor, uint64_t* out_handle, F body) {
  if (!executor || !out_handle) return 1;
  if (!executor->session) { executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)"; return 1; }
  try {
    *out_handle = body();
    return 0;
  } catch (const std::invalid_argument& e) {
    executor->last_error = e.what();
    return 1;
  } catch (const std::exception& e) {
    executor->last_error = e.what();
    executor->session.reset();
    return 1;
  }
}

int peacock_join_probe(peacock_executor_t* executor, uint64_t join, uint64_t probe,
                       uint64_t* out_handle, PeacockNodeStats* out_stats) {
  return join_call(executor, out_handle, [&] {
    return executor->session->join_probe(join, probe, reinterpret_cast<peacock::NodeStats*>(out_stats));
  });
}

int peacock_join_finish(peacock_executor_t* executor, uint64_t join, uint64_t* out_handle,
                        PeacockNodeStats* out_stats) {
  return join_call(executor, out_handle, [&] {
    return executor->session->join_finish(join, reinterpret_cast<peacock::NodeStats*>(out_stats));
  });
}

void peacock_join_release(peacock_executor_t* executor, uint64_t join) {
  if (executor && executor->session) executor->session->join_release(join);
}
```

  A probe after finish throws `std::invalid_argument("join_probe: join N is finished")` from
  `JoinSession::probe`, so it too leaves the session standing. `peacockdb-ffi/src/lib.rs`, in the
  same `extern "C"` block after `peacock_executor_slice_handle`:

```rust
        /// Build join `seq`'s session over `build` (0 = no build batch), consuming it.
        pub fn peacock_join_build(executor: *mut PeacockExecutor, seq: u64, build: u64,
            out_join: *mut u64, out_stats: *mut PeacockNodeStats) -> i32;
        /// One probe batch, consumed; `*out_handle == 0` when the type answers at finish.
        pub fn peacock_join_probe(executor: *mut PeacockExecutor, join: u64, probe: u64,
            out_handle: *mut u64, out_stats: *mut PeacockNodeStats) -> i32;
        /// The finish; `*out_handle == 0` when the type has none.
        pub fn peacock_join_finish(executor: *mut PeacockExecutor, join: u64,
            out_handle: *mut u64, out_stats: *mut PeacockNodeStats) -> i32;
        /// Free a session (idempotent).
        pub fn peacock_join_release(executor: *mut PeacockExecutor, join: u64);
```

- [x] **Step 5: The session's state and the Inner arm** (`join_session.cpp`):

```cpp
// The join session: join-rewrite-design.md §3. Names follow its pseudocode.
#include "operators/join_session.h"
#include "peacock/expr.h"

#if __has_include(<cudf/join/join.hpp>)
#include <cudf/join/join.hpp>
#include <cudf/join/hash_join.hpp>
#include <cudf/join/distinct_hash_join.hpp>
#include <cudf/join/conditional_join.hpp>
#include <cudf/join/mixed_join.hpp>
#else
#include <cudf/join.hpp>
#endif
#include <cudf/binaryop.hpp>
#include <cudf/concatenate.hpp>
#include <cudf/copying.hpp>
#include <cudf/filling.hpp>
#include <cudf/replace.hpp>
#include <cudf/reshape.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/search.hpp>
#include <cudf/stream_compaction.hpp>
#include <cudf/unary.hpp>
#include <cudf/column/column_factories.hpp>

#include <limits>
#include <numeric>
#include <stdexcept>

namespace peacock {
namespace {

using Idx = std::unique_ptr<rmm::device_uvector<cudf::size_type>>;
const cudf::data_type kBool{cudf::type_id::BOOL8};

/// One cuDF table holds at most size_type rows; a pair count past that would wrap silently into
/// a short table (design §3.9). Every path that makes pairs or an output calls this first, with
/// the sizer's count, and names the join in the refusal.
void fits_or_throw(std::size_t n, char const* what) {
  if (n > static_cast<std::size_t>(std::numeric_limits<cudf::size_type>::max()))
    throw std::runtime_error(std::string("CudfJoin: ") + what + " makes " + std::to_string(n) +
                             " rows, past what one cuDF table holds");
}

cudf::column_view idx_view(rmm::device_uvector<cudf::size_type> const& v) {
  fits_or_throw(v.size(), "an index map");           // the cast below would otherwise wrap
  return cudf::column_view{cudf::data_type{cudf::type_id::INT32},
                           static_cast<cudf::size_type>(v.size()), v.data(), nullptr, 0};
}

std::unique_ptr<cudf::table> gather_rows(cudf::table_view t, cudf::column_view idx,
                                         cudf::out_of_bounds_policy policy) {
  return cudf::gather(t, idx, policy);
}

std::vector<std::string> concat_names(std::vector<std::string> a, std::vector<std::string> const& b) {
  a.insert(a.end(), b.begin(), b.end());
  return a;
}

/// [build..., probe...] as one TableResult, then `projection` when the field is present.
TableResult emit(const fb::CudfJoin* d, TableResult build_part, TableResult probe_part) {
  TableResult t = std::move(build_part);
  t.owners.insert(t.owners.end(), probe_part.owners.begin(), probe_part.owners.end());
  t.columns.insert(t.columns.end(), probe_part.columns.begin(), probe_part.columns.end());
  t.column_names = concat_names(std::move(t.column_names), probe_part.column_names);
  if (!d->projection()) return t;
  if (d->projection()->size() == 0)
    throw std::runtime_error("CudfJoin: a present empty projection — the plan keeps __rowmarker__ instead");
  std::vector<cudf::size_type> ords(d->projection()->begin(), d->projection()->end());
  return t.select(ords);
}

}  // namespace

struct JoinSession::State {
  const fb::CudfJoin* d;
  fb::JoinType type;
  cudf::null_equality cmp;
  TableResult B;                                  // the build side, owned by the session
  std::vector<cudf::size_type> bkeys, pkeys;      // key ordinals per side
  std::unique_ptr<cudf::hash_join> hj;            // built once
  bool finished = false;

  cudf::table_view Bk() const {
    std::vector<cudf::column_view> c;
    for (auto i : bkeys) c.push_back(B.columns[i]);
    return cudf::table_view{c};
  }
  cudf::table_view keys_of(TableResult const& P) const {
    std::vector<cudf::column_view> c;
    for (auto i : pkeys) c.push_back(P.columns[i]);
    return cudf::table_view{c};
  }
};

JoinSession::JoinSession(const fb::CudfJoin* d, std::optional<TableResult> build)
    : s_(std::make_unique<State>()) {
  s_->d = d;
  s_->type = d->join_type();
  s_->cmp = d->null_equals_null() ? cudf::null_equality::EQUAL : cudf::null_equality::UNEQUAL;
  if (!build) throw std::runtime_error("CudfJoin: no build batch — Task 5 answers it");
  s_->B = std::move(*build);
  if (s_->B.columns.empty()) throw std::runtime_error("CudfJoin: a zero-column build side is a planner bug");
  if (d->keys()) {
    for (auto const* k : *d->keys()) {
      if (k->left()->node_type() != fb::ExprNode_ColumnRef || k->right()->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("CudfJoin: only ColumnRef keys");
      s_->bkeys.push_back(static_cast<cudf::size_type>(k->left()->node_as_ColumnRef()->index()));
      s_->pkeys.push_back(static_cast<cudf::size_type>(k->right()->node_as_ColumnRef()->index()));
    }
  }
  if (s_->type == fb::JoinType_Inner && !s_->bkeys.empty() && !d->filter())
    s_->hj = std::make_unique<cudf::hash_join>(s_->Bk(), s_->cmp);   // the portable ctor
  else
    throw std::runtime_error("CudfJoin: this shape arrives in a later task");
}

JoinSession::~JoinSession() = default;

std::optional<TableResult> JoinSession::probe(TableResult P) {
  if (s_->finished) throw std::invalid_argument("join_probe: the join is finished");
  if (P.columns.empty()) throw std::runtime_error("CudfJoin: a zero-column probe side is a planner bug");
  auto Pk = s_->keys_of(P);
  auto n = s_->hj->inner_join_size(Pk);               // both versions; also the output-size hint
  fits_or_throw(n, "an Inner probe");
  auto [pi, bi] = s_->hj->inner_join(Pk, n);
  auto Bg = gather_rows(s_->B.view(), idx_view(*bi), cudf::out_of_bounds_policy::DONT_CHECK);
  auto Pg = gather_rows(P.view(), idx_view(*pi), cudf::out_of_bounds_policy::DONT_CHECK);
  return emit(s_->d, TableResult::owning(std::move(Bg), s_->B.column_names),
              TableResult::owning(std::move(Pg), P.column_names));
}

std::optional<TableResult> JoinSession::finish() {
  if (s_->finished) throw std::invalid_argument("join_finish: the join is finished");
  s_->finished = true;
  return std::nullopt;
}

}  // namespace peacock
```

  `hash_join` holds a `table_view` of `Bk`; the columns it views belong to `B`'s owners, which the
  session holds, so the object never outlives what it views.
- [x] **Step 6:** device cycle: the four cases green; the existing gpu tier green.
- [ ] **Step 7: Commit.** `git commit -m "the join session's ABI and lifecycle, answering Inner"`.

### Task 3: Left, Right, Full and the finish (§3.2, §3.3)

**Files:**
- Modify: `cpp/src/operators/join_session.cpp`
- Test: `cpp/tests/gpu/test_join_session.cpp`

**Interfaces:**
- Consumes: Task 2's `State`, `emit`, `idx_view`.
- Produces (in the anonymous namespace, used by every later task):
  `std::unique_ptr<cudf::column> bools(cudf::size_type n, bool v)`,
  `std::unique_ptr<cudf::column> set_true(cudf::column_view col, cudf::column_view idx)`,
  `std::unique_ptr<cudf::column> in_range(cudf::column_view idx, cudf::size_type n)`,
  `std::unique_ptr<cudf::column> negate(cudf::column_view)`,
  `std::unique_ptr<cudf::column> null_column(cudf::data_type, cudf::size_type)`,
  `TableResult null_table(std::vector<cudf::data_type> const&, std::vector<std::string> const&, cudf::size_type)`,
  `std::vector<cudf::data_type> types_of(cudf::table_view)`, `std::vector<cudf::data_type> types_of(const fb::Schema*)`,
  `std::vector<std::string> names_of(const fb::Schema*)`;
  `State::matched` (`std::unique_ptr<cudf::column>`), `State::probe_types`/`probe_names` (from the first probe batch).

- [x] **Step 1: The cases, red.** Build `b_k {1, 2, 2, NULL}`, `b_v {a, b, c, n}`; probes as listed.

```cpp
static JoinSpec typed(fb::JoinType t) { auto s = inner_on_k(); s.type = t; return s; }

TEST(JoinSession, LeftPadsEveryBuildRowNoProbeMatchedIncludingANullKey) {
  Session s(make_plan(typed(fb::JoinType_Left)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2, std::nullopt}), i64("p_w", {20, 99})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  // the latent finish defect: today's LeftAnti hardcodes NULL = NULL and drops "n"
  EXPECT_EQ(s.rows(out), sorted({"1|a|NULL|NULL", "NULL|n|NULL|NULL"}));
}

TEST(JoinSession, RightPadsEachUnmatchedProbeRowInItsOwnCall) {
  Session s(make_plan(typed(fb::JoinType_Right)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2, 5, std::nullopt}), i64("p_w", {20, 50, 99})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20", "NULL|NULL|5|50", "NULL|NULL|NULL|99"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  EXPECT_EQ(out, 0u);
}

TEST(JoinSession, FullNeverReemitsAnUnmatchedBuildRowAcrossThreeBatches) {
  Session s(make_plan(typed(fb::JoinType_Full)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"2|b|2|20", "2|c|2|20"}));
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {}), i64("p_w", {})}), &out, nullptr), 0);
  EXPECT_TRUE(s.rows(out).empty());                            // a zero-row batch, one zero-row table
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {7, std::nullopt}), i64("p_w", {70, 99})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"NULL|NULL|7|70", "NULL|NULL|NULL|99"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"1|a|NULL|NULL", "NULL|n|NULL|NULL"}));
}

TEST(JoinSession, NullEqualsNullMatchesNullKeysForEveryOuterType) {
  auto spec = typed(fb::JoinType_Full);
  spec.null_equals_null = true;
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {std::nullopt}), utf8("b_v", {"n"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {std::nullopt}), i64("p_w", {99})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"NULL|n|NULL|99"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  EXPECT_TRUE(s.rows(out).empty());
}

TEST(JoinSession, AProjectionCrossingSidesKeepsOnlyItsColumns) {
  auto spec = typed(fb::JoinType_Left);
  spec.projection = std::vector<uint32_t>{3, 1};               // p_w, b_v
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2}), i64("p_w", {20})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"20|b"}));
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"NULL|a"}));
}
```

- [x] **Step 2: The helpers.**

```cpp
std::unique_ptr<cudf::column> bools(cudf::size_type n, bool v) {
  cudf::numeric_scalar<bool> s(v);
  return cudf::make_column_from_scalar(s, n);
}

/// col OR (row i appears in idx). contains, not scatter: idx repeats a row once per match, and
/// a scatter map with duplicate indices is undefined (copying.hpp). idx holds in-range rows only.
std::unique_ptr<cudf::column> set_true(cudf::column_view col, cudf::column_view idx) {
  cudf::numeric_scalar<cudf::size_type> zero(0);
  auto rows = cudf::sequence(col.size(), zero);
  auto hit = cudf::contains(idx, rows->view());
  return cudf::binary_operation(col, hit->view(), cudf::binary_operator::LOGICAL_OR, kBool);
}

/// 0 <= idx < n. The unmatched value is "unspecified out-of-bounds" for hash_join::left_join and
/// conditional_left_join on 25.02; only distinct_hash_join promises INT32_MIN.
std::unique_ptr<cudf::column> in_range(cudf::column_view idx, cudf::size_type n) {
  cudf::numeric_scalar<cudf::size_type> lo(0), hi(n);
  auto ge = cudf::binary_operation(idx, lo, cudf::binary_operator::GREATER_EQUAL, kBool);
  auto lt = cudf::binary_operation(idx, hi, cudf::binary_operator::LESS, kBool);
  return cudf::binary_operation(ge->view(), lt->view(), cudf::binary_operator::LOGICAL_AND, kBool);
}

std::unique_ptr<cudf::column> negate(cudf::column_view c) {
  return cudf::unary_operation(c, cudf::unary_operator::NOT);
}

std::unique_ptr<cudf::column> null_column(cudf::data_type t, cudf::size_type n) {
  auto s = cudf::make_default_constructed_scalar(t);
  s->set_valid_async(false);
  return cudf::make_column_from_scalar(*s, n);
}

/// The only maker of padded columns. Types come from the side's own table when the session has
/// seen one, from the plan's schema otherwise — so a pad always matches what matched rows carry.
TableResult null_table(std::vector<cudf::data_type> const& types,
                       std::vector<std::string> const& names, cudf::size_type n) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  for (auto t : types) cols.push_back(null_column(t, n));
  return TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), names);
}

std::vector<cudf::data_type> types_of(cudf::table_view t) {
  std::vector<cudf::data_type> out;
  for (auto const& c : t) out.push_back(c.type());
  return out;
}
std::vector<cudf::data_type> types_of(const fb::Schema* s) {
  std::vector<cudf::data_type> out;
  for (auto const* f : *s->fields()) {
    auto id = fb_to_type_id(f->data_type());
    if (id == cudf::type_id::EMPTY)
      throw std::runtime_error("CudfJoin: schema field " + f->name()->str() + " has no cuDF type");
    out.push_back(id == cudf::type_id::DECIMAL128
                      ? cudf::data_type{id, -static_cast<int32_t>(f->decimal_scale())}
                      : cudf::data_type{id});
  }
  return out;
}
std::vector<std::string> names_of(const fb::Schema* s) {
  std::vector<std::string> out;
  for (auto const* f : *s->fields()) out.push_back(f->name()->str());
  return out;
}
```

- [x] **Step 3: State and arms.** `State` gains `std::unique_ptr<cudf::column> matched;`,
  `std::vector<cudf::data_type> probe_types; std::vector<std::string> probe_names;` (set from the
  first probe batch, else from `probe_schema` at finish). The constructor builds `hj` for every
  keyed type without a filter (later tasks narrow it) and, for `Left, Full, LeftSemi, LeftAnti,
  LeftMark`, `matched = bools(|B|, false)`. `probe`:

```cpp
  auto Pk = s_->keys_of(P);
  if (s_->probe_types.empty()) { s_->probe_types = types_of(P.view()); s_->probe_names = P.column_names; }
  auto build_out = [&](rmm::device_uvector<cudf::size_type> const& bi, cudf::out_of_bounds_policy pol) {
    return TableResult::owning(gather_rows(s_->B.view(), idx_view(bi), pol), s_->B.column_names);
  };
  auto probe_out = [&](rmm::device_uvector<cudf::size_type> const& pi) {
    return TableResult::owning(gather_rows(P.view(), idx_view(pi), cudf::out_of_bounds_policy::DONT_CHECK), P.column_names);
  };
  switch (s_->type) {
    case fb::JoinType_Inner:
    case fb::JoinType_Left: {
      auto n = s_->hj->inner_join_size(Pk);
      fits_or_throw(n, "an Inner or Left probe");
      auto [pi, bi] = s_->hj->inner_join(Pk, n);
      if (s_->matched) s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return emit(s_->d, build_out(*bi, cudf::out_of_bounds_policy::DONT_CHECK), probe_out(*pi));
    }
    case fb::JoinType_Right:
    case fb::JoinType_Full: {
      auto n = s_->hj->left_join_size(Pk);
      fits_or_throw(n, "a Right or Full probe");
      auto [pi, bi] = s_->hj->left_join(Pk, n);         // probe is cuDF's left: every probe row once
      if (s_->matched) {
        auto hit = in_range(idx_view(*bi), s_->B.view().num_rows());
        auto hits = cudf::apply_boolean_mask(cudf::table_view{{idx_view(*bi)}}, hit->view());
        s_->matched = set_true(s_->matched->view(), hits->get_column(0).view());
      }
      // never hj->full_join: per batch it re-emits every unmatched build row
      return emit(s_->d, build_out(*bi, cudf::out_of_bounds_policy::NULLIFY), probe_out(*pi));
    }
    default: throw std::runtime_error("CudfJoin: this type arrives in a later task");
  }
```

  `finish`:

```cpp
  s_->finished = true;
  auto pad_types = s_->probe_types.empty() ? types_of(s_->d->probe_schema()) : s_->probe_types;
  auto pad_names = s_->probe_names.empty() ? names_of(s_->d->probe_schema()) : s_->probe_names;
  switch (s_->type) {
    case fb::JoinType_Left:
    case fb::JoinType_Full: {
      auto U = cudf::apply_boolean_mask(s_->B.view(), negate(s_->matched->view())->view());
      auto n = U->num_rows();
      return emit(s_->d, TableResult::owning(std::move(U), s_->B.column_names),
                  null_table(pad_types, pad_names, n));
    }
    default: return std::nullopt;
  }
```

- [x] **Step 4:** device cycle, `--gtest_filter='JoinSession.*'`: the five green, Task 2's still green.
- [ ] **Step 5: Commit.** `git commit -m "the join session's outer types: per-batch pads, one finish"`.

### Task 4: The semi family without a residual (§3.2, §3.3)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

**Interfaces:**
- Consumes: Task 3's helpers.
- Produces: `std::unique_ptr<cudf::table> distinct_keys(cudf::table_view K, cudf::null_equality)`;
  `State::Bd` (owned distinct build keys), `State::dhj`; `State::empty_build` (set here for an
  all-NULL build key under UNEQUAL, by Task 5 for a zero-row or absent build).

- [x] **Step 1: The cases, red.** Build `b_k {1, 2, 2, NULL}`, `b_v {a, b, c, n}`.

```cpp
TEST(JoinSession, LeftSemiAntiMarkAnswerOnlyAtFinishEachBuildRowOnce) {
  for (auto [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_LeftSemi, {"2|b", "2|c"}},
           {fb::JoinType_LeftAnti, {"1|a", "NULL|n"}},
           {fb::JoinType_LeftMark, {"1|a|false", "2|b|true", "2|c|true", "NULL|n|false"}}}) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 1;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})}), &j, nullptr), 0);
    // three probe rows on key 2 and a NULL: many-to-many, and NULL matches nothing
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2, 2, 2, std::nullopt}), i64("p_w", {1, 2, 3, 4})}), &out, nullptr), 0);
    EXPECT_EQ(out, 0u) << "a build-side semi type answers no batch per probe";
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}

TEST(JoinSession, RightSemiAntiAnswerPerBatchOverDuplicateBuildKeys) {
  for (auto [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_RightSemi, {"2|20", "2|21"}},
           {fb::JoinType_RightAnti, {"5|50", "NULL|99"}}}) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {2, 2, 2, std::nullopt}), utf8("b_v", {"a", "b", "c", "n"})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {2, 2, 5, std::nullopt}), i64("p_w", {20, 21, 50, 99})}), &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_EQ(out, 0u);
  }
}

TEST(JoinSession, RightAntiOverABuildWhoseKeysAreAllNullKeepsEveryProbeRow) {
  Session s(make_plan(typed(fb::JoinType_RightAnti)));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {std::nullopt, std::nullopt}), utf8("b_v", {"m", "n"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 99})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"1|10", "NULL|99"}));
}
```

- [x] **Step 2: `distinct_keys`.**

```cpp
std::unique_ptr<cudf::table> distinct_keys(cudf::table_view K, cudf::null_equality cmp) {
  std::vector<cudf::size_type> all(K.num_columns());
  std::iota(all.begin(), all.end(), 0);
  std::unique_ptr<cudf::table> nonnull;
  cudf::table_view src = K;
  if (cmp == cudf::null_equality::UNEQUAL) {      // a NULL key matches nothing: drop it first
    nonnull = cudf::drop_nulls(K, all);
    src = nonnull->view();
  }
  return cudf::distinct(src, all, cudf::duplicate_keep_option::KEEP_ANY, cmp,
                        cudf::nan_equality::ALL_EQUAL);
}
```

- [x] **Step 3: The arms.** Constructor, for `RightSemi`/`RightAnti` without a cross filter:

```cpp
    s_->Bd = distinct_keys(s_->Bk(), s_->cmp);              // owned: dhj views it
    if (s_->Bd->num_rows() == 0) { s_->empty_build = true; return; }
    s_->dhj = std::make_unique<cudf::distinct_hash_join>(s_->Bd->view(), s_->cmp);
```

  (Two arguments: the explicit-stream form does not compile on 26.02, where a stream cannot
  convert to `load_factor`.) `probe`:

```cpp
    case fb::JoinType_LeftSemi: case fb::JoinType_LeftAnti: case fb::JoinType_LeftMark: {
      auto dk = distinct_keys(Pk, s_->cmp);
      auto [pi, bi] = s_->hj->inner_join(dk->view());          // each build row at most once
      s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return std::nullopt;
    }
    case fb::JoinType_RightSemi: case fb::JoinType_RightAnti: {
      auto bi = s_->dhj->left_join(Pk);                        // one entry per probe row
      auto hit = in_range(idx_view(*bi), s_->Bd->num_rows());
      auto mask = s_->type == fb::JoinType_RightSemi ? std::move(hit) : negate(hit->view());
      auto kept = cudf::apply_boolean_mask(P.view(), mask->view());
      return emit_probe_only(s_->d, TableResult::owning(std::move(kept), P.column_names));
    }
```

  where `emit_probe_only(d, t)` applies `projection` the way `emit` does, over the probe's columns
  alone (and `emit_build_only` the same over the build's, for the finish below). `finish` gains:

```cpp
    case fb::JoinType_LeftSemi:
      return emit_build_only(s_->d, TableResult::owning(cudf::apply_boolean_mask(s_->B.view(), s_->matched->view()), s_->B.column_names));
    case fb::JoinType_LeftAnti:
      return emit_build_only(s_->d, TableResult::owning(cudf::apply_boolean_mask(s_->B.view(), negate(s_->matched->view())->view()), s_->B.column_names));
    case fb::JoinType_LeftMark:
      return emit_build_only(s_->d, s_->B.with(std::move(s_->matched), "mark"));   // shares B's owners
```

- [x] **Step 4:** device cycle: the three green.
- [ ] **Step 5: Commit.** `git commit -m "the join session's semi family: rows, never pairs"`.

### Task 5: Absent and empty sides (§3.8, #173, #212)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

**Interfaces:** Consumes Tasks 2–4. Produces `State::empty_build` for every reason (no batch, zero
rows, all-NULL keys).

- [x] **Step 1: The matrix, red.** For each of the nine types × build ∈ {no batch, zero rows} ×
  probe ∈ {one batch `p_k {1, NULL}`, none}, the expected output — all nine types in one table-driven
  test, plus a zero-row probe between two with rows:

```cpp
struct EmptyCase { fb::JoinType t; Rows per_probe; std::optional<Rows> finish; };
// build: no batch, or zero rows — the same answers. probe: p_k {1, NULL}, p_w {10, 99}.
static std::vector<EmptyCase> empty_build_answers() {
  return {
      {fb::JoinType_Inner,     {},                                       std::nullopt},
      {fb::JoinType_Left,      {},                                       Rows{}},
      {fb::JoinType_Right,     {"NULL|NULL|1|10", "NULL|NULL|NULL|99"},  std::nullopt},
      {fb::JoinType_Full,      {"NULL|NULL|1|10", "NULL|NULL|NULL|99"},  Rows{}},
      {fb::JoinType_LeftSemi,  {},                                       Rows{}},
      {fb::JoinType_LeftAnti,  {},                                       Rows{}},
      {fb::JoinType_LeftMark,  {},                                       Rows{}},
      {fb::JoinType_RightSemi, {},                                       std::nullopt},
      {fb::JoinType_RightAnti, {"1|10", "NULL|99"},                      std::nullopt},
  };
}

TEST(JoinSession, AnEmptyOrAbsentBuildAnswersEveryType) {
  for (bool absent : {true, false}) {
    for (auto const& c : empty_build_answers()) {
      Session s(make_plan(typed(c.t)));
      uint64_t b = absent ? 0 : s.upload({i32("b_k", {}), utf8("b_v", {})});
      uint64_t j = 0, out = 0;
      ASSERT_EQ(peacock_join_build(s.exec(), 0, b, &j, nullptr), 0) << s.error();
      ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 99})}), &out, nullptr), 0);
      bool finishing = c.finish.has_value();
      bool build_side_semi = c.t == fb::JoinType_LeftSemi || c.t == fb::JoinType_LeftAnti || c.t == fb::JoinType_LeftMark;
      if (build_side_semi) EXPECT_EQ(out, 0u);
      else EXPECT_EQ(s.rows(out), sorted(c.per_probe)) << fb::EnumNameJoinType(c.t) << " absent=" << absent;
      ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
      if (finishing) EXPECT_EQ(s.rows(out), sorted(*c.finish));
      else EXPECT_EQ(out, 0u);
    }
  }
}

TEST(JoinSession, NoProbeBatchStillFinishesFromTheBuildSide) {   // #173
  std::vector<std::pair<fb::JoinType, Rows>> want = {
      {fb::JoinType_Left,     {"1|a|NULL|NULL", "NULL|n|NULL|NULL"}},
      {fb::JoinType_Full,     {"1|a|NULL|NULL", "NULL|n|NULL|NULL"}},
      {fb::JoinType_LeftSemi, {}},
      {fb::JoinType_LeftAnti, {"1|a", "NULL|n"}},
      {fb::JoinType_LeftMark, {"1|a|false", "NULL|n|false"}}};
  for (auto const& [t, rows] : want) {
    Session s(make_plan(typed(t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, std::nullopt}), utf8("b_v", {"a", "n"})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), sorted(rows)) << fb::EnumNameJoinType(t);   // pads typed from probe_schema
  }
}
```

- [x] **Step 2: The arms.** Constructor: `B = build ? *build : null_table(types_of(build_schema),
  names_of(build_schema), 0)`; `if (B.view().num_rows() == 0) { empty_build = true; return; }` before
  any cuDF object is made, after `matched` would have been made (empty is fine: zero rows). `probe`
  with `empty_build`:

```cpp
  if (s_->empty_build) {
    switch (s_->type) {
      case fb::JoinType_LeftSemi: case fb::JoinType_LeftAnti: case fb::JoinType_LeftMark:
        return std::nullopt;
      case fb::JoinType_Right: case fb::JoinType_Full: {
        auto n = P.view().num_rows();
        return emit(s_->d, null_table(types_of(s_->B.view()), s_->B.column_names, n), std::move(P));
      }
      case fb::JoinType_RightAnti: return emit_probe_only(s_->d, std::move(P));   // every row
      default:                                                   // Inner, Left, RightSemi
        return emit_zero_rows(s_->d, P);                         // one zero-row table of the output
    }
  }
```

  `emit_zero_rows` slices both sides to zero rows (`B.slice(0, 0)`, `P.slice(0, 0)`) and emits them,
  so the table has the output's columns and types. `finish` with `empty_build`: `Left`/`Full`/the
  build-side semi family answer `emit` over zero build rows (and, for `LeftMark`, a zero-row
  `mark`). Note `types_of(B.view())` for an absent build comes from `build_schema` (Step 2's
  `null_table`), so a timestamp build column needs the `DataType` arms repartition-keys added.
- [x] **Step 3:** device cycle: both green, earlier tasks green.
- [ ] **Step 4: Commit.** `git commit -m "the join session answers an empty or absent side for every type"`.

### Task 6: The residual — pairs path, per-side conjuncts, chunking (§3.4)

**Files:**
- Modify: `cpp/src/operators/join_session.cpp`; `cpp/src/peacock/expr.h`, `cpp/src/expr.cpp` (the col map as a span), `cpp/src/operators/join.cpp` (its two callers adapt)
- Test: `cpp/tests/gpu/test_join_session.cpp`

**Interfaces:**
- Produces: `using JoinFilterColMap = cudf::host_span<fb::JoinFilterColumn const>;` in `expr.h`, and
  `JoinFilterColMap col_map_of(const flatbuffers::Vector<const fb::JoinFilterColumn*>* v)` beside it
  (Step 1a); in the session:
  `struct Conjunct { const fb::Expr* expr; bool reads_build, reads_probe; bool ast_able; };`,
  `State::cross`, `State::build_only`, `State::probe_only` (`std::vector<Conjunct>`),
  `std::unique_ptr<cudf::column> eval_on(std::vector<Conjunct> const&, TableResult const& side, fb::JoinSide)` (AND of `c IS TRUE`),
  `std::unique_ptr<cudf::column> residual_mask(cudf::table_view B, cudf::table_view P, cudf::column_view bi, cudf::column_view pi, std::vector<Conjunct> const&)`,
  `std::vector<std::pair<cudf::size_type, cudf::size_type>> chunks(std::size_t pairs, std::size_t row_bytes, cudf::size_type rows, uint64_t budget)`.

- [x] **Step 1: The cases, red.** Build `b_k {1, 2}`, `b_v {a, b}`, `b_lim {5, 5}`; filter `p_w >
  b_lim` (filter schema `[Probe 1, Build 2]`, i.e. `col(0) > col(1)`).

```cpp
static JoinSpec residual(fb::JoinType t) {
  return {.type = t, .keys = {{0, 0}},
          .filter = [](auto& f) { return bin(f, col(f, 0), fb::BinaryOp_Gt, col(f, 1)); },
          .filter_columns = {{fb::JoinSide_Right, 1}, {fb::JoinSide_Left, 2}},
          .build_schema = {{"b_k", fb::DataType_Int32}, {"b_v", fb::DataType_Utf8}, {"b_lim", fb::DataType_Int32}},
          .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}};
}
// probe p_k {1, 1, 2}, p_w {3, 9, 9}: key matches (1,3) (1,9) (2,9); the filter keeps (1,9) (2,9).
TEST(JoinSession, AnOuterResidualDecidesUnmatchedAfterTheFilter) {   // #153
  struct C { fb::JoinType t; Rows probe; Rows finish; };
  for (auto const& c : std::vector<C>{
           {fb::JoinType_Inner, {"1|a|5|1|9", "2|b|5|2|9"}, {}},
           {fb::JoinType_Left,  {"1|a|5|1|9", "2|b|5|2|9"}, {}},
           {fb::JoinType_Right, {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}},
           {fb::JoinType_Full,  {"1|a|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3"}, {}}}) {
    Session s(make_plan(residual(c.t)));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"}), i32("b_lim", {5, 5})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 1, 2}), i64("p_w", {3, 9, 9})}), &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), sorted(c.probe)) << fb::EnumNameJoinType(c.t);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    if (c.t == fb::JoinType_Left || c.t == fb::JoinType_Full) EXPECT_EQ(s.rows(out), sorted(c.finish));
  }
}

TEST(JoinSession, ChunkingThePairsGivesTheSameRows) {
  auto spec = residual(fb::JoinType_Full);
  spec.chunk_bytes = 16;                                       // forces one chunk per probe row
  Session s(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 1, 2}), utf8("b_v", {"a", "a2", "b"}), i32("b_lim", {5, 5, 5})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, 1, 2, 3}), i64("p_w", {3, 9, 9, 9})}), &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"1|a|5|1|9", "1|a2|5|1|9", "2|b|5|2|9", "NULL|NULL|NULL|1|3", "NULL|NULL|NULL|3|9"}));
}

// A preserved-side condition that is NULL is no match (design §3.4, D3): b_flag NULL keeps the anti
// row and marks false.
TEST(JoinSession, ANullPreservedSideConditionIsNoMatch) {
  for (auto [t, want] : std::vector<std::pair<fb::JoinType, Rows>>{
           {fb::JoinType_LeftAnti, {"1|NULL", "1|false"}},
           {fb::JoinType_LeftSemi, {"1|true"}},
           {fb::JoinType_LeftMark, {"1|NULL|false", "1|false|false", "1|true|true"}}}) {
    JoinSpec spec{.type = t, .keys = {{0, 0}},
                  .filter = [](auto& f) { return col(f, 0); },                 // b_flag alone
                  .filter_columns = {{fb::JoinSide_Left, 1}},
                  .build_schema = {{"b_k", fb::DataType_Int32}, {"b_flag", fb::DataType_Boolean}},
                  .probe_schema = {{"p_k", fb::DataType_Int32}}};
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 1, 1}), boolean("b_flag", {std::nullopt, false, true})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1})}), &out, nullptr), 0);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), sorted(want)) << fb::EnumNameJoinType(t);
  }
}
```

- [x] **Step 1a: The column map as a span.** A FlatBuffers vector of structs stores the structs
  inline, but its `data()` is typed `const S* const*` (`flatbuffers/vector.h:281`), so
  `{v->data(), v->size()}` does not compile; `Data()` is the raw bytes. A `true` filter carries no
  `filter_columns`, so a null vector is an empty span. In `expr.h`:

```cpp
#include <cudf/utilities/span.hpp>

// A join filter's ColumnRef(i) is filter-schema ordinal i; entry i says which side and column it
// reads. A span, so the session can pass a map it built (sides flipped, or hoisted columns added).
using JoinFilterColMap = cudf::host_span<fb::JoinFilterColumn const>;

inline JoinFilterColMap col_map_of(const flatbuffers::Vector<const fb::JoinFilterColumn*>* v) {
  if (!v) return {};
  return {reinterpret_cast<const fb::JoinFilterColumn*>(v->Data()), v->size()};
}
```

  `build_expr` keeps its pointer parameter, now `const JoinFilterColMap* col_map`; its `ColumnRef`
  arm reads `auto const& fc = (*col_map)[col->index()];` and `fc.side()`, `fc.index()` (the bounds
  check stays). `join.cpp`'s two callers (`:98`, `:233`) become `auto map = col_map_of(join->filter_columns());
  build_expr(join->filter(), ctx, &map)`; its loops over `filter_columns()` (`:356`, `:443`, `:470`)
  are unchanged. The old gpu tier must stay green: these are the semi and mark joins it runs today.
- [x] **Step 2: Splitting the residual.** At construction, when `filter()` is present:

```cpp
/// The top-level AND-chain, flattened.
void conjuncts_of(const fb::Expr* e, std::vector<const fb::Expr*>& out) {
  if (e->node_type() == fb::ExprNode_BinaryExprNode && e->node_as_BinaryExprNode()->op() == fb::BinaryOp_And) {
    conjuncts_of(e->node_as_BinaryExprNode()->left(), out);
    conjuncts_of(e->node_as_BinaryExprNode()->right(), out);
  } else out.push_back(e);
}
/// Which sides a conjunct reads, through the filter-column map.
void sides_read(const fb::Expr* e, JoinFilterColMap map, bool& b, bool& p) {
  if (e->node_type() == fb::ExprNode_ColumnRef) {
    auto side = map[e->node_as_ColumnRef()->index()].side();
    (side == fb::JoinSide_Left ? b : p) = true;
    return;
  }
  for_each_child(e, [&](const fb::Expr* c) { sides_read(c, map, b, p); });
}
```

  `for_each_child` visits the operands of `BinaryExprNode`, `UnaryExprNode`, `CastExprNode`,
  `ScalarFunctionExprNode` args, and the case/when arms (read `gpu_plan.fbs`'s `ExprNode` union and
  cover every arm that has `Expr` children; a missing arm throws naming the kind). Each conjunct is
  classified: `cross` (reads both), `build_only`, `probe_only`. `ast_able` is
  `cudf_ast_can_evaluate(e, type_table)` over the zero-row type table `join.cpp:433-451` builds
  (filter-schema order, zero-row slices of each side's column).
- [x] **Step 3: The per-side evaluator, NULL as false.**

```cpp
/// AND over `cs` of `c IS TRUE`, evaluated over one side's rows. A conjunct's ColumnRef(i) is
/// filter-schema ordinal i; columns of the other side are never read, so their slots hold this
/// side's column 0 only to give the view one row count.
std::unique_ptr<cudf::column> eval_on(std::vector<Conjunct> const& cs, cudf::table_view side,
                                      fb::JoinSide which, JoinFilterColMap map) {
  std::vector<cudf::column_view> cols;
  for (auto const& fc : map) cols.push_back(fc.side() == which ? side.column(fc.index()) : side.column(0));
  cudf::table_view ft{cols};
  auto acc = bools(side.num_rows(), true);
  cudf::numeric_scalar<bool> f(false);
  for (auto const& c : cs) {
    auto v = cudf::replace_nulls(build_column(c.expr, ft)->view(), f);       // R IS TRUE
    acc = cudf::binary_operation(acc->view(), v->view(), cudf::binary_operator::LOGICAL_AND, kBool);
  }
  return acc;
}
```

  Uses, by type (§3.1, §3.4):
  - `RightSemi`/`RightAnti`: `build_only` filters `B` at construction (`B = apply_boolean_mask(B,
    eval_on(build_only, B))`) before any key work; `probe_only` is `R` per probe batch: semi keeps
    `hit ∧ R`, anti `¬(hit ∧ R)`.
  - `LeftSemi`/`LeftAnti`/`LeftMark`: `probe_only` filters `P` before `Pk` is taken; `build_only`
    is `R` at finish: `m = matched ∧ R` (`LeftSemi` keeps `m`, `LeftAnti` `¬m`, `LeftMark` marks `m`).
  - `Inner`/`Left`/`Right`/`Full`: every conjunct goes to the pairs path below (outer types cannot
    push a one-side conjunct: it decides unmatched rows).
- [x] **Step 4: The pairs path with chunking.**

```cpp
std::unique_ptr<cudf::column> residual_mask(cudf::table_view B, cudf::table_view P,
                                            cudf::column_view bi, cudf::column_view pi,
                                            std::vector<Conjunct> const& cs, JoinFilterColMap map) {
  std::vector<std::unique_ptr<cudf::table>> gathered;
  std::vector<cudf::column_view> cols;
  for (auto const& fc : map) {
    bool build = fc.side() == fb::JoinSide_Left;
    auto src = (build ? B : P).select({static_cast<cudf::size_type>(fc.index())});
    gathered.push_back(cudf::gather(src, build ? bi : pi, cudf::out_of_bounds_policy::DONT_CHECK));
    cols.push_back(gathered.back()->get_column(0).view());
  }
  cudf::table_view ft{cols};
  auto acc = bools(bi.size(), true);
  cudf::numeric_scalar<bool> f(false);
  for (auto const& c : cs) {
    auto v = cudf::replace_nulls(build_column(c.expr, ft)->view(), f);
    acc = cudf::binary_operation(acc->view(), v->view(), cudf::binary_operator::LOGICAL_AND, kBool);
  }
  return acc;
}

/// Row ranges of the probe batch so each range's pairs fit `budget` bytes (0 = 1 GiB).
std::vector<std::pair<cudf::size_type, cudf::size_type>> chunks(std::size_t pairs, std::size_t row_bytes,
                                                               cudf::size_type rows, uint64_t budget) {
  if (budget == 0) budget = 1ull << 30;
  std::size_t bytes = pairs * (8 + row_bytes);
  std::size_t k = std::max<std::size_t>(1, (bytes + budget - 1) / budget);
  k = std::min<std::size_t>(k, std::max<cudf::size_type>(rows, 1));
  std::vector<std::pair<cudf::size_type, cudf::size_type>> out;
  for (std::size_t i = 0; i < k; ++i)
    out.emplace_back(static_cast<cudf::size_type>(rows * i / k), static_cast<cudf::size_type>(rows * (i + 1) / k));
  return out;
}
```

  `row_bytes` is Σ over filter columns of `cudf::size_of(type)` (16 for a string or other
  variable-width type). `probe` with a residual on `Inner/Left/Right/Full`:

```cpp
  if (!s_->hj) throw std::logic_error("CudfJoin: the pairs path without a hash table — a constructor arm is missing");
  auto ranges = chunks(s_->hj->inner_join_size(Pk), s_->filter_row_bytes, P.view().num_rows(), s_->d->chunk_bytes());
  std::vector<TableResult> parts;
  for (auto [lo, hi] : ranges) {
    TableResult Pc = P.slice(lo, hi);
    auto Pck = s_->keys_of(Pc);
    auto n = s_->hj->inner_join_size(Pck);
    fits_or_throw(n, "a chunk of residual pairs");          // one probe row's matches can still overflow
    auto [pi, bi] = s_->hj->inner_join(Pck, n);
    auto mask = residual_mask(s_->B.view(), Pc.view(), idx_view(*bi), idx_view(*pi), s_->all, s_->map);
    auto kept = cudf::apply_boolean_mask(cudf::table_view{{idx_view(*bi), idx_view(*pi)}}, mask->view());
    auto bk = kept->get_column(0).view(), pk = kept->get_column(1).view();
    if (s_->matched) s_->matched = set_true(s_->matched->view(), bk);
    auto Bg = TableResult::owning(cudf::gather(s_->B.view(), bk), s_->B.column_names);
    auto Pg = TableResult::owning(cudf::gather(Pc.view(), pk), Pc.column_names);
    if (s_->type == fb::JoinType_Right || s_->type == fb::JoinType_Full) {
      auto hitP = set_true(bools(Pc.view().num_rows(), false)->view(), pk);
      auto UP = cudf::apply_boolean_mask(Pc.view(), negate(hitP->view())->view());
      auto n = UP->num_rows();
      Bg = concat_rows(std::move(Bg), null_table(types_of(s_->B.view()), s_->B.column_names, n));
      Pg = concat_rows(std::move(Pg), TableResult::owning(std::move(UP), Pc.column_names));
    }
    parts.push_back(emit(s_->d, std::move(Bg), std::move(Pg)));
  }
  return concat_rows(std::move(parts));                       // one table per call
```

  `concat_rows` is `cudf::concatenate` over the parts' views, wrapped `owning` with the first part's
  names (a single part is returned as is). `B.select` above is `table_view::select`.
- [x] **Step 5:** device cycle: the three green.
- [ ] **Step 6: Commit.** `git commit -m "the join session's residual: pairs over the filter's columns, per-side conjuncts, chunks"`.

### Task 7: The semi family with a residual reading both sides (§3.1, §3.4; D1, D13)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

**Interfaces:** Consumes Task 6's `Conjunct`, `residual_mask`, `chunks`, `JoinFilterColMap`.
Produces `State::flipped` (the col map with sides swapped, for the probe as cuDF's left).

- [x] **Step 1: The cases, red.** The AST-able cross residual `p_w > b_lim` (Task 6's spec) over
  LeftSemi/LeftAnti/LeftMark/RightSemi/RightAnti, and the same with the residual made non-AST by a
  decimal comparison (`build`'s `b_dlim` Decimal128(15,2) against the probe's `p_d` Decimal128(15,2)
  — `cudf_ast_can_evaluate` refuses any decimal operand, `expr.cpp:415`): both must give the same
  rows (the D1 regression; `wire/gpu_tests/mod.rs:674`'s `SEMI_JOIN` is this shape). Build `b_k
  {1, 1, 2}`, `b_lim {5, 9, 5}`; probe `p_k {1, 2, 3}`, `p_w {7, 1, 9}`. Matches passing: `(b1,
  p1)` only (7 > 5). Expected: LeftSemi `{1|5}`; LeftAnti `{1|9, 2|5}`; LeftMark `{1|5|true,
  1|9|false, 2|5|false}`; RightSemi `{1|7}`; RightAnti `{2|1, 3|9}`. Write them as Task 6's
  table-driven loop, once with the int filter and once with the decimal one.
- [x] **Step 1b: Several cross conjuncts, red.** A LeftSemi over `p_w > b_lim AND p_w < b_hi`
  (build `b_k {1, 1}`, `b_lim {5, 5}`, `b_hi {8, 20}`; probe `p_k {1}`, `p_w {10}`): only `(b2, p1)`
  passes, so LeftSemi `{1|5|20}`, LeftMark `{1|5|8|false, 1|5|20|true}`. And the same with one of the
  two conjuncts a decimal comparison (not AST-able): the same rows, through the pairs path — the
  shape that would otherwise reach a null `hj`.
- [x] **Step 2: The matchers.** A cross residual is one AST when every cross conjunct is AST-able
  (after Task 8's hoisting): their AND is built at the cuDF AST level, in one `ExprContext`, never
  in a FlatBuffer:

```cpp
/// AND of the cross conjuncts as one cuDF AST. NULL_LOGICAL_AND, so a NULL operand next to a
/// false one is false; a NULL result is no match, as mixed_*'s header says.
cudf::ast::expression const& cross_ast(std::vector<Conjunct> const& cs, ExprContext& ctx,
                                       JoinFilterColMap const& map) {
  cudf::ast::expression const* acc = &build_expr(cs.front().expr, ctx, &map);
  for (std::size_t i = 1; i < cs.size(); ++i) {
    auto const& next = build_expr(cs[i].expr, ctx, &map);
    acc = &ctx.keep(std::make_unique<cudf::ast::operation>(
        cudf::ast::ast_operator::NULL_LOGICAL_AND, *acc, next));
  }
  return *acc;
}
```

  (A hoisted conjunct, Task 8, contributes its own AST node instead of `build_expr`'s; `cross_ast`
  takes either through `Conjunct::ast`, a `std::function<cudf::ast::expression const&(ExprContext&,
  JoinFilterColMap const&)>` that Task 8 sets.) The constructor builds `hj` whenever the pairs
  path can be taken — a semi-family type with any cross conjunct that is not AST-able, and every
  `Inner/Left/Right/Full` with keys — and `Bk` alone otherwise; the pairs path begins with

```cpp
  if (!s_->hj) throw std::logic_error("CudfJoin: the pairs path without a hash table — a constructor arm is missing");
```

  so a missed arm is an exception naming itself, never a null dereference. `probe`, AST:

```cpp
      ExprContext ctx;
      auto const& pred = cross_ast(s_->cross, ctx, left_is_build ? s_->map : s_->flipped);
      if (left_is_build) {                                     // LeftSemi/LeftAnti/LeftMark
        auto bi = cudf::mixed_left_semi_join(s_->Bk(), Pk, s_->B.view(), P.view(), pred, s_->cmp);
        s_->matched = set_true(s_->matched->view(), idx_view(*bi));
        return std::nullopt;
      }
      // RightSemi/RightAnti: the probe as cuDF's left. Anti is semi plus the complement — not
      // mixed_left_anti_join, whose header says a NULL predicate drops the row NOT EXISTS keeps.
      auto pi = cudf::mixed_left_semi_join(Pk, s_->Bk(), P.view(), s_->B.view(), pred, s_->cmp);
      auto hit = set_true(bools(P.view().num_rows(), false)->view(), idx_view(*pi));
      if (!s_->probe_only.empty())
        hit = cudf::binary_operation(hit->view(), eval_on(s_->probe_only, P.view(), fb::JoinSide_Right, s_->map)->view(),
                                     cudf::binary_operator::LOGICAL_AND, kBool);
      auto mask = s_->type == fb::JoinType_RightSemi ? std::move(hit) : negate(hit->view());
      return emit_probe_only(s_->d, TableResult::owning(cudf::apply_boolean_mask(P.view(), mask->view()), P.column_names));
```

  Non-AST `cross` (any one conjunct): Task 6's pairs path over `hj`, every cross conjunct in
  `residual_mask`,
  chunked the same way, then rows from `bi'` (`set_true(matched, bi')`) or `pi'` (`set_true(bools(|P|),
  pi')` as the mask). Note `s_->flipped`: a copy of the filter-column map with every `side`
  swapped, built once at construction and held by `State` (`std::vector<fb::JoinFilterColumn>`
  with a span over it), so `flipped` outlives every AST built from it.
- [x] **Step 3:** device cycle: green with both filters.
- [ ] **Step 4: Commit.** `git commit -m "the join session's semi family over a residual reading both sides"`.

### Task 8: Hoisting one-side operands so more conditions run on the AST (§3.6's `hoist`)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

**Interfaces:** Consumes Tasks 6–7. Produces `Conjunct::ast` for a hoisted conjunct, the extended
maps `State::map_h` / `flipped_h` (owned vectors with spans over them), and the condition tables
with their hoisted columns, `State::B_cond` and the per-batch `P_cond`. No FlatBuffer is written:
a hoisted conjunct is built directly as cuDF AST nodes.

- [x] **Step 1: The case, red.** A Left nested loop whose condition is `upper(b_s) = p_s` (filter
  columns `[Build 1, Probe 0]`): `upper` is a string function the AST refuses, but it reads the build
  alone, so it is hoisted into a build column and the comparison becomes AST-able. Build `b_id {1,
  2}`, `b_s {"x", "y"}`; probe `p_s {"X", "z"}`. Expected per probe: `{1|x|X}`; finish `{2|y|NULL}`.
  A second case, `cast(b_d as decimal) > cast(p_d as decimal)` (both operands hoisted, still a
  decimal comparison, so not AST-able), answers the same rows through Task 9's chunked cross — the
  #215 shape. A third, a keyed LeftSemi whose cross residual is `upper(b_s) = p_s AND p_w > b_lim`:
  the first conjunct hoisted, both then AST, one `mixed_left_semi_join` — the same rows as the
  pairs path gives with hoisting disabled (`chunk_bytes` small, so the pairs path also chunks).
- [x] **Step 2: The rule, bounded.** Hoisting applies to a conjunct of the form `lhs OP rhs` with
  `OP` one of `Eq, NotEq, Lt, LtEq, Gt, GtEq`, where each operand reads one side only and the
  conjunct as a whole is not AST-able. Each operand that is not a bare `ColumnRef` is evaluated over
  its side's rows with `build_column` (the side table in filter-schema order, as `eval_on` builds
  it) and appended to that side's condition table — `B_cond` once at construction, `P_cond` per
  batch — at index `i` = that table's width before the append. The conjunct's AST is then built
  directly:

```cpp
cudf::ast::ast_operator ast_op_of(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:    return cudf::ast::ast_operator::EQUAL;
    case fb::BinaryOp_NotEq: return cudf::ast::ast_operator::NOT_EQUAL;
    case fb::BinaryOp_Lt:    return cudf::ast::ast_operator::LESS;
    case fb::BinaryOp_LtEq:  return cudf::ast::ast_operator::LESS_EQUAL;
    case fb::BinaryOp_Gt:    return cudf::ast::ast_operator::GREATER;
    case fb::BinaryOp_GtEq:  return cudf::ast::ast_operator::GREATER_EQUAL;
    default: throw std::logic_error("hoist: not a comparison");
  }
}

/// One operand of a hoisted conjunct: a column of the build's (LEFT) or the probe's (RIGHT)
/// condition table — the hoisted column's index, or the bare ColumnRef's own.
struct Operand { cudf::size_type index; cudf::ast::table_reference table; };

/// `lhs OP rhs` as cuDF AST nodes owned by `ctx`. `flip` swaps LEFT and RIGHT, for the
/// RightSemi/RightAnti call that passes the probe as cuDF's left.
cudf::ast::expression const& hoisted_ast(fb::BinaryOp op, Operand lhs, Operand rhs, bool flip,
                                         ExprContext& ctx) {
  auto side = [&](cudf::ast::table_reference t) {
    if (!flip) return t;
    return t == cudf::ast::table_reference::LEFT ? cudf::ast::table_reference::RIGHT
                                                 : cudf::ast::table_reference::LEFT;
  };
  auto& l = ctx.keep(std::make_unique<cudf::ast::column_reference>(lhs.index, side(lhs.table)));
  auto& r = ctx.keep(std::make_unique<cudf::ast::column_reference>(rhs.index, side(rhs.table)));
  return ctx.keep(std::make_unique<cudf::ast::operation>(ast_op_of(op), l, r));
}
```

  The conjunct's `Conjunct::ast` captures its `(op, lhs, rhs)` and calls `hoisted_ast` with `flip`
  set for the probe-as-left call; `cross_ast` (Task 7) ANDs it with the others at the AST level. A
  literal operand is hoisted too (a constant column on the other operand's side, `make_column_from_scalar`
  over that side's rows), so both operands are always column references. After hoisting,
  `ast_able` is re-judged with `cudf_ast_can_evaluate` over the extended type table; a conjunct
  still refused stays out of the AST and sends the residual to the pairs path, with every cross
  conjunct in `residual_mask` over the plain `B`/`P` and the original map (the pairs path needs no
  hoisting: the column path evaluates anything). Hoisted columns never reach an output: `emit` reads
  `B` and `P`, not `B_cond`/`P_cond`. The `mixed_*` call passes `B_cond`/`P_cond` (each the side's
  columns, then its hoisted ones) as the conditional tables.
- [x] **Step 3:** device cycle: green.
- [ ] **Step 4: Commit.** `git commit -m "the join session hoists one-side operands into columns"`.

### Task 9: Nested loops — every type, literal `true`, chunked (§3.6)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

**Interfaces:** Consumes Tasks 6–8. Produces the keyless arms.

- [x] **Step 1: The cases, red.** Condition `p_v < b_v` (filter `[Probe 0, Build 1]`), build `b_id
  {1, 2}`, `b_v {5, 10}`, probe `p_v {3, 7, 12}`. Pairs passing: (1,3) (2,3) (2,7). Expected per
  type (columns `b_id|b_v|p_v` or one side's):
  - Inner probe `{1|5|3, 2|10|3, 2|10|7}`;
  - Left probe as Inner, finish `{}` (both build rows matched);
  - Right probe as Inner plus `NULL|NULL|12`;
  - Full as Right, finish `{}`;
  - LeftSemi finish `{1|5, 2|10}`, LeftAnti finish `{}`, LeftMark finish `{1|5|true, 2|10|true}`;
  - RightSemi probe `{3, 7}`, RightAnti probe `{12}`.
  Then the same nine with the predicate-free literal `true` over an empty probe batch and over an
  empty build (`tiny LEFT JOIN empty ON true` gives the build padded — 8 rows in pbench; here 2),
  then `chunk_bytes = 16` over the A∧R form (`p_v < b_v AND cast(p_v as decimal) <>
  cast(b_v as decimal)`) and over the A-empty form (a decimal-only condition) — same rows as unchunked.
- [x] **Step 2: The arms.** With no keys, the conjuncts split into `A` (AST-able after Task 8) and `R`:

```cpp
  // A non-empty, R empty — row-wise where the type allows
  ExprContext ctx;
  auto const& a = cross_ast(s_->A, ctx, s_->map_h);                // Task 7: AND at the AST level
  auto const& a_swapped = cross_ast(s_->A, ctx, s_->flipped_h);
  switch (s_->type) {
    case fb::JoinType_Inner: case fb::JoinType_Left: {
      auto n = cudf::conditional_inner_join_size(s_->B_cond(), P_cond, a);
      fits_or_throw(n, "a nested-loop probe");
      auto [bi, pi] = cudf::conditional_inner_join(s_->B_cond(), P_cond, a, n);
      if (s_->matched) s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return emit(s_->d, gathered(s_->B, *bi), gathered(P, *pi));
    }                                                     // never conditional_left_join on a stream
    case fb::JoinType_Right: {
      auto n = cudf::conditional_left_join_size(P_cond, s_->B_cond(), a_swapped);
      fits_or_throw(n, "a nested-loop Right probe");
      auto [pi, bi] = cudf::conditional_left_join(P_cond, s_->B_cond(), a_swapped, n);
      return emit(s_->d, gathered(s_->B, *bi, cudf::out_of_bounds_policy::NULLIFY), gathered(P, *pi));
    }
    case fb::JoinType_LeftSemi: case fb::JoinType_LeftAnti: case fb::JoinType_LeftMark: {
      auto bi = cudf::conditional_left_semi_join(s_->B_cond(), P_cond, a);
      s_->matched = set_true(s_->matched->view(), idx_view(*bi));
      return std::nullopt;
    }
    case fb::JoinType_RightSemi: case fb::JoinType_RightAnti: {
      auto pi = cudf::conditional_left_semi_join(P_cond, s_->B_cond(), a_swapped);   // anti = complement
      auto hit = set_true(bools(P.view().num_rows(), false)->view(), idx_view(*pi));
      auto mask = s_->type == fb::JoinType_RightSemi ? std::move(hit) : negate(hit->view());
      return emit_probe_only(s_->d, TableResult::owning(cudf::apply_boolean_mask(P.view(), mask->view()), P.column_names));
    }
    case fb::JoinType_Full: break;                        // the pairs recipe below
  }
```

  `gathered(T, idx, policy = DONT_CHECK)` is `TableResult::owning(cudf::gather(T.view(), idx_view(idx),
  policy), T.column_names)`; `B_cond()` is `B`'s columns plus its hoisted ones (Task 8), `P_cond`
  the batch's likewise. `Full` (and every type when `R` is non-empty) takes the pairs recipe: candidates
  `conditional_inner_join(B_cond, P_cond, a)` chunked by `conditional_inner_join_size(B_cond,
  P_cond_chunk, a)` through `chunks`, then `residual_mask` over `R`, then Task 6's per-type
  derivation (Right/Full's `hitP` and unmatched probe rows, `matched`, the semi family's rows). `A`
  empty: the chunked cross of indices —

```cpp
  cudf::size_type nb = s_->B.view().num_rows();
  for (auto [lo, hi] : chunks(std::size_t(nb) * P.view().num_rows(), s_->filter_row_bytes, P.view().num_rows(), s_->d->chunk_bytes())) {
    cudf::size_type nc = hi - lo;
    fits_or_throw(std::size_t(nb) * nc, "a chunk of the nested loop's cross product");
    cudf::numeric_scalar<cudf::size_type> from_lo(lo), zero(0);
    auto pseq = cudf::sequence(nc, from_lo);
    auto bseq = cudf::sequence(nb, zero);
    auto pi = cudf::repeat(cudf::table_view{{pseq->view()}}, nb);   // probe-major; order is free here
    auto bi = cudf::tile(cudf::table_view{{bseq->view()}}, nc);
    auto mask = residual_mask(s_->B.view(), P.view(), bi->get_column(0).view(), pi->get_column(0).view(), s_->R, s_->map);
    // keep, then derive per type exactly as the pairs recipe
  }
```

  A predicate-free nested loop of any type but Inner arrives with the literal `true` as its filter
  (design §4.1); it is AST-able, so it takes the first arm. `Full` with `A` only also uses the pairs
  recipe (never `conditional_full_join`, which has no streaming form, and not the left-join
  sentinel, D8).
- [x] **Step 3:** device cycle: the nine × three green.
- [ ] **Step 4: Commit.** `git commit -m "the join session's nested loops: every type, true, chunked"`.

### Task 10: Cross (§3.7)

**Files:** Modify `cpp/src/operators/join_session.cpp`; Test `cpp/tests/gpu/test_join_session.cpp`.

- [x] **Step 1: The cases, red.** Inner, no keys, no filter: build `{1, 2}` × probe `{x, y, z}` → 6
  rows; a zero-row build → one zero-row table; a zero-row probe → one zero-row table; a projection
  keeping one probe column → 6 rows of it (#207's device half); a build of zero *columns* is refused
  naming the planner (the explicit `__rowmarker__` keeps one, design §4.1); `nb × np` over `INT32_MAX`
  refused naming the join — exercise the check with a helper that calls it on sizes, not with a
  table that big:

```cpp
TEST(JoinSession, ACrossJoinOverTwoBillionRowsIsRefusedByName) {
  EXPECT_THROW(peacock::cross_rows_or_throw(70000, 40000), std::runtime_error);   // 2.8e9
  EXPECT_EQ(peacock::cross_rows_or_throw(1000, 1000), 1000000);
}
```

  (`cross_rows_or_throw` is declared in `join_session.h` for this test.)
- [x] **Step 2: The arm.**

```cpp
cudf::size_type cross_rows_or_throw(std::int64_t nb, std::int64_t np) {
  if (nb * np > std::numeric_limits<cudf::size_type>::max())
    throw std::runtime_error("CudfJoin: a cross join of " + std::to_string(nb) + " × " +
                             std::to_string(np) + " rows exceeds one table");
  return static_cast<cudf::size_type>(nb * np);
}
...
    cross_rows_or_throw(B.num_rows(), P.num_rows());
    if (B.num_rows() == 0 || P.num_rows() == 0) return emit_zero_rows(s_->d, P);
    auto out = TableResult::owning(cudf::cross_join(s_->B.view(), P.view()),   // neither side has zero columns
                                   concat_names(s_->B.column_names, P.column_names));
    auto nbc = s_->B.view().num_columns(), n = out.view().num_columns();
    std::vector<cudf::size_type> bo(nbc), po(n - nbc);
    std::iota(bo.begin(), bo.end(), 0);
    std::iota(po.begin(), po.end(), nbc);
    return emit(s_->d, out.select(bo), out.select(po));        // both share out's owners; #207
```
- [x] **Step 3:** device cycle green.
- [ ] **Step 4: Commit.** `git commit -m "the join session's cross join: projection, empties, the one overflow check"`.

### Task 11: Typed pads and stats (§3.0, §3.9)

**Files:** Test `cpp/tests/gpu/test_join_session.cpp` (and any fix the cases show in `join_session.cpp`).

- [x] **Step 0: Timestamps in a pad.** The `ts_us` pad below reads `TimestampMicrosecond` from
  `probe_schema` — the `DataType` arm repartition-keys added. repartition-keys' Tasks 5b and 5c map
  Arrow's timestamps to it and refuse an unmapped type instead of writing `Null`; join-backend's
  writer calls that.
- [x] **Step 1: Pads, red until right.** A Left join whose probe carries one column of each type —
  `i32`, `i64`, `utf8`, `date32`, `dec(15,2)`, `dec(38,4)`, `boolean`, `ts_us` — finished with no
  probe batch (pads from `probe_schema`, so the schema names each type: Decimal128 with its scale in
  `decimal_scale`, `TimestampMicrosecond`) and once after one probe batch (pads from the batch). Read
  the IPC schema of the finish output (`arrow::ipc::RecordBatchStreamReader::schema()`) and assert
  each probe column's type equals the probe batch's, decimal scale included. The `JoinSpec` needs
  a `decimal_scale` per field: extend `schema_of` to take a scale (and `CreateField`'s
  `decimal_precision`, `decimal_scale`).
- [x] **Step 2: Stats.** One probe of an Inner join over a `utf8` build column: `PeacockNodeStats`
  from `peacock_join_probe` has `rows` = the output's rows and `varlen_content_bytes` = the sum of
  the output strings' bytes (count them by hand); a probe answering handle 0 leaves `{0, 0}`; the
  build's stats are `{0, 0}`.
- [x] **Step 3: Regions.** With `peacock_set_node_timing(1)`: a build, two probes and a finish
  record four regions for seq 0, `partition` 0 each, `call_index` 0, 1, 2, 3
  (`peacock_executor_collect_node_regions`).
- [x] **Step 3a: No exit copy (#154's `join.cpp` sites).** The session moves gathered columns into
  its output; a deep copy at the exit would show as a second output-sized allocation at the peak.
  The statistics adaptor `main()` installs (Task 1) measures it, with the probe the only call inside:

```cpp
#include "peacock/rmm_pool.hpp"

/// What `fn` allocated: in total, at its peak above where it started, and still held after.
struct Allocated { int64_t total = 0, peak = 0, net = 0; };
template <class F>
static Allocated allocated_by(F&& fn) {
  auto& mr = peacock::stats_mr();
  if (!mr) { ADD_FAILURE() << "main() installs the pool and its statistics adaptor"; return {}; }
  mr->push_counters();
  fn();
  auto [bytes, calls] = mr->pop_counters();
  return {bytes.total, bytes.peak, bytes.value};
}

TEST(JoinSession, AProbeHandsItsGatheredColumnsOverWithoutACopy) {
  constexpr int64_t n = 1 << 20;                       // 1:1 keys, one int64 payload a side
  std::vector<std::optional<int64_t>> k(n), v(n);
  for (int64_t i = 0; i < n; ++i) { k[i] = i; v[i] = i * 3; }
  Session s(make_plan({.type = fb::JoinType_Inner, .keys = {{0, 0}},
                       .build_schema = {{"b_k", fb::DataType_Int64}, {"b_v", fb::DataType_Int64}},
                       .probe_schema = {{"p_k", fb::DataType_Int64}, {"p_v", fb::DataType_Int64}}}));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i64("b_k", k), i64("b_v", v)}), &j, nullptr), 0);
  auto probe = s.upload({i64("p_k", k), i64("p_v", v)});
  auto got = allocated_by([&] { ASSERT_EQ(peacock_join_probe(s.exec(), j, probe, &out, nullptr), 0); });
  const int64_t output = n * 4 * 8;                    // four int64 columns
  const int64_t indices = n * 2 * 4;                   // the two INT32 index maps
  EXPECT_GE(got.net, output - output / 16) << "the output is what the call leaves behind";
  EXPECT_LT(got.peak, output + indices + output / 2)
      << "peak " << got.peak << ": a deep copy at the exit would add a second " << output;
}
```

  (`stats_mr()` and `install_rmm_pool` are `cpp/include/peacock/rmm_pool.hpp`'s, as
  refcounted-scatter's `allocated_by` uses them; the input batches are uploaded before the
  measured call, and consumed by it, so they free inside it and only lower `net`.)
- [x] **Step 4:** device cycle green; the whole file green; the existing gpu tier green.
- [ ] **Step 5: Commit.** `git commit -m "the join session's pads hold their types; one region per call; no exit copy"`.

### Task 11b: The rest of §5.6's named cases

**Files:** Test `cpp/tests/gpu/test_join_session.cpp` (and any fix the cases show in `join_session.cpp`).

Five items of design §5.6's named list no earlier task pins. Each is red until the arm it names is
right; each expected answer is counted by hand.

- [x] **Step 1: The cases.**

```cpp
// A present-but-empty projection keeps no column (the plan never asks for that: the session
// refuses it as a planner bug); an absent one keeps every column.
TEST(JoinSession, AnAbsentProjectionKeepsEveryColumnAndAnEmptyOneIsRefused) {
  auto spec = inner_on_k();
  Session all(make_plan(spec));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(all.exec(), 0, all.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(all.exec(), j, all.upload({i32("p_k", {1}), i64("p_w", {7})}), &out, nullptr), 0);
  EXPECT_EQ(all.rows(out), sorted({"1|a|1|7"}));
  spec.projection = std::vector<uint32_t>{};
  Session none(make_plan(spec));
  EXPECT_NE(peacock_join_build(none.exec(), 0, none.upload({i32("b_k", {1}), utf8("b_v", {"a"})}), &j, nullptr), 0);
  EXPECT_NE(none.error().find("zero columns"), std::string::npos) << none.error();
}

// fits_or_throw is what every pairs-producing path calls before an index map is cast to size_type.
TEST(JoinSession, APairCountPastSizeTypeIsRefusedByName) {
  const std::size_t past = static_cast<std::size_t>(std::numeric_limits<cudf::size_type>::max()) + 1;
  try { peacock::join::fits_or_throw(past, "an Inner probe"); FAIL() << "no refusal"; }
  catch (std::runtime_error const& e) {
    EXPECT_NE(std::string(e.what()).find("an Inner probe"), std::string::npos) << e.what();
    EXPECT_NE(std::string(e.what()).find("past what one cuDF table holds"), std::string::npos) << e.what();
  }
}

// A composite key with a NULL in its second column never matches under the SQL default.
TEST(JoinSession, ACompositeKeyWithANullInItsSecondColumnNeverMatches) {
  Session s(make_plan({.type = fb::JoinType_LeftAnti, .keys = {{0, 0}, {1, 1}},
                       .build_schema = {{"b_a", fb::DataType_Int32}, {"b_b", fb::DataType_Int32}},
                       .probe_schema = {{"p_a", fb::DataType_Int32}, {"p_b", fb::DataType_Int32}}}));
  uint64_t j = 0, out = 0;
  ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_a", {1, 1}), i32("b_b", {std::nullopt, 2})}), &j, nullptr), 0);
  ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_a", {1, 1}), i32("p_b", {std::nullopt, 2})}), &out, nullptr), 0);
  EXPECT_EQ(out, 0u);
  ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
  EXPECT_EQ(s.rows(out), sorted({"1|NULL"}));                 // (1, NULL) matched nothing: kept
}

// LeftAnti and LeftMark with a residual answer from the build alone when no probe batch came.
TEST(JoinSession, AResidualAntiAndMarkOverNoProbeBatchAnswerFromTheBuild) {
  for (auto t : {fb::JoinType_LeftAnti, fb::JoinType_LeftMark}) {
    auto spec = residual(t);
    Session s(make_plan(spec));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {1, 2}), utf8("b_v", {"a", "b"}), i32("b_lim", {5, 5})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_finish(s.exec(), j, &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), t == fb::JoinType_LeftAnti
                               ? sorted({"1|a|5", "2|b|5"})                   // every build row
                               : sorted({"1|a|5|false", "2|b|5|false"}));     // every mark false
  }
}

// RightSemi and RightAnti over a build whose keys are all NULL under UNEQUAL: an empty build.
TEST(JoinSession, ARightSemiOverAnAllNullBuildAnswersNothingAndRightAntiEveryRow) {
  for (auto t : {fb::JoinType_RightSemi, fb::JoinType_RightAnti}) {
    Session s(make_plan({.type = t, .keys = {{0, 0}},
                         .build_schema = {{"b_k", fb::DataType_Int32}},
                         .probe_schema = {{"p_k", fb::DataType_Int32}, {"p_w", fb::DataType_Int64}}}));
    uint64_t j = 0, out = 0;
    ASSERT_EQ(peacock_join_build(s.exec(), 0, s.upload({i32("b_k", {std::nullopt, std::nullopt})}), &j, nullptr), 0);
    ASSERT_EQ(peacock_join_probe(s.exec(), j, s.upload({i32("p_k", {1, std::nullopt}), i64("p_w", {10, 20})}), &out, nullptr), 0);
    EXPECT_EQ(s.rows(out), t == fb::JoinType_RightSemi ? Rows{} : sorted({"1|10", "NULL|20"}));
  }
}
```
  (`inner_on_k()` is Task 2's spec, `residual()` Task 6's; `fits_or_throw` is exported from
  `join_session.h` in the `peacock::join` namespace for this case. If the zero-column refusal's
  message is worded otherwise in Task 10, match it.)
- [x] **Step 2: Run** on shad-gpu (`build-test-shadgpu.sh --run` with `PCK_TEST_FILTER=JoinSession`):
  each red until its arm is right, then green; record the reds in the detail file.
- [ ] **Step 3: Commit.** `git commit -m "the session's remaining named cases: projection, size, composite NULL, empty-probe residual, all-NULL build"`.

### Task 12: The wiki and the record

- [x] `llm-wiki/architecture.md`, Interfaces: the four symbols, their consume and failure rules
  (an unknown join id and a probe after finish leave the session; anything else ends the query),
  the join map's lifetime; the symbol count. `build-test.md`: a row for `peacock_join_session_tests`
  (gpu, shad-gpu) with its case count; the grand total.
- [x] Detail file: each task's device-cycle lines; the gaps below.
- [ ] `git commit -m "join-session-cpp: the C ABI documented, the record"`.

## Gaps the plan resolves on its own (for the reviewer)

- **The session's file.** Written as `join_session.{h,cpp}` beside `join.cpp`, not inside it: the old
  `execute_*_join` keep serving today's plans until join-backend, which deletes `join.cpp`.
- **`chunk_bytes`** on `CudfJoin`: the design's scratch budget has no carrier to C++ otherwise.
- **Timestamp `DataType`s** are repartition-keys': this plan consumes them (pads and absent sides
  read types from the schema). repartition-keys' Tasks 5b and 5c map Arrow's timestamps to them and
  refuse an unmapped type; join-backend's writer calls it.
- **Pads from the side's own types** when a batch has been seen, the schema otherwise.
- **`JoinFilterColMap` as a span**, so the session can pass a flipped or extended map.
- **Hoisting is bounded** to comparison conjuncts with one-sided operands, built directly as cuDF
  AST nodes (`hoisted_ast`); several cross conjuncts are ANDed at the AST level (`cross_ast`), so no
  FlatBuffer is rewritten. A conjunct still not AST-able sends the residual to the pairs path, whose
  `hj` the constructor always builds when that path can be taken.
- **The `size_type` ceiling** is checked by every path that makes pairs or an output
  (`fits_or_throw`, fed by `inner_join_size`, `left_join_size`, `conditional_*_size`), and by
  `idx_view` itself.
- **The gtest binary is installed** (`install(TARGETS)` and the rpath list) and has its own `main`
  with the RMM pool and statistics adaptor: CI and shad-gpu run only installed binaries.
