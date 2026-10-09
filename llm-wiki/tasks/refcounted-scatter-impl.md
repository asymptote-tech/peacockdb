# refcounted-scatter implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `TableResult` takes its final shape — one owner per column, views over them — and the
hash repartition hands its N partitions out as slices of one table instead of N copies; the
arm takes exactly one input handle (#197). Closes #145 and #197.

**Architecture:** One header change (`cpp/src/plan_executor.h`) with four constructors in a new
`cpp/src/table_result.cpp`; every `.table` site migrated mechanically to `view()`/`owning()`;
then three behaviour changes, each red first: a slice's string bytes, the one-handle refusal,
and the shared scatter.

**Tech stack:** C++17/20 against libcudf 25.02 (shad-gpu) and 26.02/25.10a (CI), gtest on a
device; no Rust, wire or golden change.

**Spec:** [`refcounted-scatter.md`](refcounted-scatter.md) — frozen (9e563348).

## Global constraints

- `TableResult` changes here once, to its final shape; exit-copies and the join session use it
  without changing it.
- No C ABI change, no wire change, no Rust change, no golden moves. Every answer is unchanged.
- `slice_handle` and the sort-preserving merge's fetch keep copying: a view there would pin the
  whole batch, which is what those copies exist to avoid (`node_session.cpp:709-713`).
- Commits at most 10 lines; device cycles foreground (`scripts/build-test-shadgpu.sh --build`,
  `--push-binaries --patch`, `--run`); `peacock_plan_tests` runs whole each cycle.
- Never share a cargo target dir across worktrees; never build in the primary checkout.

## What the run changed in this plan

Three deviations, each measured rather than reasoned:

- **Task 4's peak bound is measured, not a formula.** `partition_alone` runs the same
  `spark_hash_partition` call in a scope of its own, and the assertion is that the arm's peak is
  that peak and nothing on top. The plan's `rows * 12 + offsets` under-counts cuDF's own gather
  temporaries by a factor of three on a string column, so a hand formula would have refused a
  correct arm.
- **The peak case scatters a fixed-width table, not the string one.** cuDF's gather inside
  `cudf::partition` peaks at 2.59x the input over a string column — above `parted` + the copies —
  so on that shape removing the copies cannot lower the peak, and no bound discriminates. The
  fixed-width shape (`c_custkey`, `c_nationkey`, `c_acctbal`) does: 6,973,648 to 4,962,592 bytes.
  `Scatter.AStringColumnsGatherCostsMoreThanTheCopiesDid` records the string shape's figures.
  The string column stays in the stats and sharing cases, which is what the spec's §5 asks for.
- **`scatter_plan` took a projection instead of growing a third near-copy.** One helper over
  (table, fields, projection) with two named wrappers, and `customer_scan_plan` now reads its
  field list from the same place.

## Review focus

1. **A zero-column table reaching `owning()`.** The rule refuses it; today only tpch
   nested-limits' zero-projection scan would make one, and its device cells are off (#186).
   Expected: a refusal naming the rule, never a table that reads as 0 rows. Pinned in Task 1.
2. **A producer whose names do not match its columns.** `owning()` refuses a mismatch; the
   whole gpu tier must stay green, so a producer that trips it is a latent naming defect —
   fixed in Task 1 if trivial, otherwise ticketed and the check reported. Pinned in Task 1.
3. **A slice kept past its siblings.** Releasing N−1 partitions must leave the survivor readable
   and its rows right. Pinned in Task 4.
4. **`slice_handle` still owning its rows.** After the change a slice of 1/10 of a batch must not
   keep the batch alive. Pinned in Task 4.
5. **Empty partitions.** A 64-lane scatter of a small input leaves most lanes with zero rows;
   each is a zero-row slice whose stats are 0 rows and 0 string bytes. Pinned in Task 3.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/plan_executor.h:16-19` | `TableResult`, final shape |
| `cpp/src/table_result.cpp` (new), `cpp/CMakeLists.txt:151` | `owning`, `slice`, `select`, `with` |
| `cpp/src/node_session.cpp` | `varlen_content_bytes` by slice edges; the repartition arm (#145, #197); every `.table` site |
| `cpp/src/gpu_executor.cpp`, `cpp/src/operators/*.cpp` | the `.table` sites, mechanically |
| `cpp/tests/gpu/test_plan_executor.cpp` | the `.table` uses; the new cases |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/corpus-coverage.md`, `tickets/memory.md` | the handle's shape; counts; #145/#197 closed; the accounting ticket |

---

### Task 1: `TableResult`'s final shape, every site migrated

**Files:**
- Modify: `cpp/src/plan_executor.h:16-19`
- Create: `cpp/src/table_result.cpp`; Modify: `cpp/CMakeLists.txt` (beside `src/node_session.cpp`, `:151`)
- Modify: every site `grep -n '\.table\b\|->table\b\|return {' cpp/src/*.cpp cpp/src/operators/*.cpp` lists
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (end of file, before `main`)

**Interfaces:**
- Produces (used by every later task, exit-copies and join-session-cpp):

```cpp
// plan_executor.h — a handle's table: one view per column, each kept alive by its own owner.
struct TableResult {
  std::vector<std::shared_ptr<cudf::column const>> owners;  // owners[i] keeps columns[i]'s buffers
                                                            // alive; two entries may share one
  std::vector<cudf::column_view> columns;                   // a whole column of *owners[i], or a row slice
  std::vector<std::string> column_names;                    // same length as columns

  cudf::table_view view() const { return cudf::table_view{columns}; }
  cudf::size_type num_rows() const { return columns.empty() ? 0 : columns.front().size(); }
  cudf::size_type num_columns() const { return static_cast<cudf::size_type>(columns.size()); }

  static TableResult owning(std::unique_ptr<cudf::table> table, std::vector<std::string> names);
  TableResult slice(cudf::size_type begin, cudf::size_type end) const;      // shares owners
  TableResult select(std::vector<cudf::size_type> const& ordinals) const;   // shares owners
  TableResult with(std::unique_ptr<cudf::column> column, std::string name) const;  // appends
};
```

- [x] **Step 1: The unit case, red (it does not compile).** After `host_int64_column` (`:1174`):

```cpp
static std::unique_ptr<cudf::column> int64_column(std::vector<int64_t> const& values) {
  auto col = cudf::make_numeric_column(cudf::data_type{cudf::type_id::INT64},
                                       static_cast<cudf::size_type>(values.size()));
  cudaMemcpy(col->mutable_view().data<int64_t>(), values.data(), values.size() * sizeof(int64_t),
             cudaMemcpyHostToDevice);
  return col;
}

TEST(TableResult, SlicesAndSelectionsShareTheirColumnsOwners) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  cols.push_back(int64_column({1, 2, 3, 4}));
  cols.push_back(int64_column({5, 6, 7, 8}));
  auto t = peacock::TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), {"a", "b"});
  ASSERT_EQ(t.num_columns(), 2);
  EXPECT_EQ(t.num_rows(), 4);

  auto s = t.slice(1, 3);
  EXPECT_EQ(s.num_rows(), 2);
  EXPECT_EQ(host_int64_column(s.view().column(0)), (std::vector<int64_t>{2, 3}));
  EXPECT_EQ(t.owners[0].use_count(), 2) << "the slice shares the column, it does not copy it";

  auto twice = t.select({1, 1});
  EXPECT_EQ(twice.owners[0], twice.owners[1]) << "a column selected twice is one owner";
  EXPECT_EQ(twice.column_names, (std::vector<std::string>{"b", "b"}));

  auto plus = t.with(int64_column({9, 9, 9, 9}), "c");
  EXPECT_EQ(plus.num_columns(), 3);
  EXPECT_EQ(plus.owners[0], t.owners[0]);
  EXPECT_EQ(plus.column_names.back(), "c");
}

TEST(TableResult, ATableOfNoColumnsIsRefused) {
  // A table_view of no columns reads as 0 rows whatever it held, which is why the plan
  // carries an explicit __rowmarker__ placeholder; constructing one is a planner defect.
  EXPECT_THROW(peacock::TableResult::owning(std::make_unique<cudf::table>(), {}),
               std::runtime_error);
}

TEST(TableResult, NamesMustMatchColumns) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  cols.push_back(int64_column({1}));
  EXPECT_THROW(peacock::TableResult::owning(std::make_unique<cudf::table>(std::move(cols)),
                                            {"a", "b"}),
               std::runtime_error);
}
```

- [x] **Step 2: The struct** — replace `plan_executor.h:15-19` with the block under *Interfaces*,
  adding `#include <cudf/column/column_view.hpp>` and `<cudf/table/table_view.hpp>`.
- [x] **Step 3: `cpp/src/table_result.cpp`**, and add `src/table_result.cpp` to the library's
  source list in `cpp/CMakeLists.txt` after `src/node_session.cpp`:

```cpp
// TableResult's four constructors (plan_executor.h): every handle's table is built by one of
// these, so the owner-per-column rule and its refusals live in one place.

#include "plan_executor.h"

#include <cudf/copying.hpp>

#include <stdexcept>
#include <string>

namespace peacock {

TableResult TableResult::owning(std::unique_ptr<cudf::table> table, std::vector<std::string> names) {
  if (!table || table->num_columns() == 0)
    throw std::runtime_error(
        "TableResult: a table of no columns reads as no rows; the plan's __rowmarker__ "
        "placeholder exists so that none is ever made");
  if (names.size() != static_cast<size_t>(table->num_columns()))
    throw std::runtime_error("TableResult: " + std::to_string(names.size()) + " names for " +
                             std::to_string(table->num_columns()) + " columns");
  TableResult out;
  out.column_names = std::move(names);
  for (auto& column : table->release()) {
    std::shared_ptr<cudf::column const> owner{std::move(column)};
    out.columns.push_back(owner->view());
    out.owners.push_back(std::move(owner));
  }
  return out;
}

TableResult TableResult::slice(cudf::size_type begin, cudf::size_type end) const {
  TableResult out;
  out.owners = owners;
  out.column_names = column_names;
  out.columns.reserve(columns.size());
  for (auto const& column : columns) out.columns.push_back(cudf::slice(column, {begin, end}).front());
  return out;
}

TableResult TableResult::select(std::vector<cudf::size_type> const& ordinals) const {
  TableResult out;
  for (auto i : ordinals) {
    if (i < 0 || i >= num_columns())
      throw std::runtime_error("TableResult::select: ordinal " + std::to_string(i) + " of " +
                               std::to_string(num_columns()) + " columns");
    out.owners.push_back(owners[i]);
    out.columns.push_back(columns[i]);
    out.column_names.push_back(column_names[i]);
  }
  if (out.columns.empty())
    throw std::runtime_error("TableResult::select: no ordinals is a table of no columns");
  return out;
}

TableResult TableResult::with(std::unique_ptr<cudf::column> column, std::string name) const {
  if (column->size() != num_rows())
    throw std::runtime_error("TableResult::with: a column of " + std::to_string(column->size()) +
                             " rows beside " + std::to_string(num_rows()));
  TableResult out = *this;
  std::shared_ptr<cudf::column const> owner{std::move(column)};
  out.columns.push_back(owner->view());
  out.owners.push_back(std::move(owner));
  out.column_names.push_back(std::move(name));
  return out;
}

}  // namespace peacock
```

- [x] **Step 4: Migrate every site, no behaviour change.** Three rewrites cover them all:
  - `X.table->view()` → `X.view()`; `X.table->num_rows()` → `X.num_rows()`;
    `X.table->num_columns()` → `X.num_columns()` (dispatch.cpp:93; filter.cpp:22-29;
    sort.cpp:19; aggregate.cpp:136; union.cpp:28; limit.cpp:17; project.cpp:25,33;
    window.cpp:37; join.cpp:46,47,354,394,413,414; node_session.cpp:398,431,489,528,572,615,668,706;
    gpu_executor.cpp:375,419).
  - `return {std::move(T), std::move(N)};` and `return {std::make_unique<cudf::table>(C), N};`
    → `return TableResult::owning(std::move(T), std::move(N));` and
    `TableResult::owning(std::make_unique<cudf::table>(std::move(C)), std::move(N))`
    (filter.cpp:44,48; union.cpp:31; sort.cpp:62; scan.cpp:112; limit.cpp:33; join.cpp:214,217,
    273,276,379,383,397,528,532; window.cpp:116; aggregate.cpp:326,433,829; project.cpp:30,63);
    `gpu_executor.cpp:497` → `peacock::TableResult::owning(std::move(table), std::move(names))`.
  - `node_session.cpp`'s collapse arm (`:441-486`): build `std::unique_ptr<cudf::table> merged`
    from `cudf::merge`/`cudf::concatenate` as today, the fetch slice as today
    (`std::make_unique<cudf::table>(sliced[0])` — an owning copy, kept), then
    `TableResult result = TableResult::owning(std::move(merged), owned[0].column_names);`.
    `slice_handle` (`:707-713`): `TableResult result = TableResult::owning(std::make_unique<cudf::table>(cudf::slice(input.view(), {begin, end}).front()), input.column_names);`
    — still a copy, on purpose (Global constraints). The repartition arm (`:517-578`), behaviour
    unchanged until Tasks 3-4: `views.push_back(owned.back().view())`; `combined` becomes
    `std::unique_ptr<cudf::table> joined; cudf::table_view tv = owned.size() == 1 ? owned[0].view()
    : (joined = cudf::concatenate(views))->view();`; and each partition stays a copy,
    `TableResult part = TableResult::owning(std::make_unique<cudf::table>(slice), column_names);`.
  - Tests: `sed -i 's/\.table->view()/.view()/g; s/\.table->num_rows()/.num_rows()/g; s/\.table->num_columns()/.num_columns()/g' cpp/tests/gpu/test_plan_executor.cpp`,
    then fix by hand what the compiler names.
- [x] **Step 5: Device cycle.** The three new cases green; `peacock_plan_tests` and the gpu tier
  otherwise unchanged. A producer refused by `owning()` for a name mismatch is fixed here when
  the fix is the names it should have passed; otherwise the case is recorded in the detail file
  and the check kept.
- [x] **Step 6: Commit.** `git commit -m "TableResult: one owner per column, views over them (#145)"`.

### Task 2: A slice's string bytes are its own

**Files:**
- Modify: `cpp/src/node_session.cpp:229-240` (`varlen_content_bytes`)
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (after Task 1's cases)

**Interfaces:**
- Consumes: `TableResult::slice` (Task 1).
- Produces: `varlen_content_bytes(table_view)` correct over sliced views — Task 4's stats rely on it.

- [x] **Step 1: The case, red.**

```cpp
TEST(VarlenBytes, ASliceCountsOnlyItsOwnRows) {
  // chars_size reads the unsliced parent's last offset, so once a scatter partition is a
  // slice every partition would report the whole table's string bytes and the accountant
  // would price four batches at four times what they hold.
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = customer_scan_plan(fbb, {0, 1});
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> groups{1};
  uint64_t handle = session.execute_scan_rowgroups(0, groups, nullptr);
  const auto& whole = session.table_for(handle);
  const auto n = whole.num_rows();
  ASSERT_GT(n, 2);
  auto all = peacock::varlen_content_bytes(whole.view());
  auto first = peacock::varlen_content_bytes(whole.slice(0, n / 2).view());
  auto second = peacock::varlen_content_bytes(whole.slice(n / 2, n).view());
  EXPECT_LT(first, all);
  EXPECT_EQ(first + second, all);
  EXPECT_EQ(peacock::varlen_content_bytes(whole.slice(0, 0).view()), 0u);
}
```

- [x] **Step 2: Device cycle:** red, `first == all`.
- [x] **Step 3: The fix.** In `node_session.cpp`, above `varlen_content_bytes`, add
  `#include <cudf/copying.hpp>` (for `get_element`) and `<cudf/scalar/scalar.hpp>`, and:

```cpp
// The content bytes of a strings column's own rows. `chars_size` reads the unsliced parent's
// last offset ("does not reflect a sliced parent column view", strings_column_view.hpp), and a
// scatter partition is a slice of the scatter's table (#145) — so read the slice's two edges.
static uint64_t string_content_bytes(cudf::column_view const& col) {
  if (col.size() == 0) return 0;
  cudf::strings_column_view sv(col);
  auto offsets = sv.offsets();
  auto at = [&](cudf::size_type i) -> int64_t {
    auto s = cudf::get_element(offsets, i);
    if (offsets.type().id() == cudf::type_id::INT64)
      return static_cast<cudf::numeric_scalar<int64_t> const&>(*s).value();
    return static_cast<cudf::numeric_scalar<int32_t> const&>(*s).value();
  };
  return static_cast<uint64_t>(at(sv.offset() + sv.size()) - at(sv.offset()));
}
```

  and in `varlen_content_bytes` replace the `chars_size(...)` term with
  `string_content_bytes(col)`. `plan_executor.h`'s comment on `set_node_timing` ("reads
  `chars_size` back") becomes "reads two offsets back".
- [x] **Step 4: Device cycle:** green; `ScanRowGroups.TheStatsCarryTheVarlenBytes` still green.
- [x] **Step 5: Commit.** `git commit -m "varlen bytes from a slice's own offsets, not its parent's"`.

### Task 3: The repartition arm takes exactly one handle (#197)

**Files:**
- Modify: `cpp/src/node_session.cpp:500-538` (the comment naming #197, the gather)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

- [x] **Step 1: The case, red** (today the arm concatenates two and answers). A two-column
  scatter plan helper first, used here and in Task 4:

```cpp
/// customer's (c_custkey, c_name) under a hash repartition on c_custkey into `lanes` — the
/// string column is what the varlen stats and the shared-slice cases need.
static std::vector<uint8_t> customer_scatter_plan(flatbuffers::FlatBufferBuilder& fbb,
                                                  uint32_t lanes) {
  auto path = fbb.CreateString(parquet_path("customer"));
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {{"c_custkey", fb::DataType_Int64}, {"c_name", fb::DataType_Utf8}});
  auto scan = fb::CreateCudfScan(fbb, paths, schema, fbb.CreateVector(std::vector<uint32_t>{0, 1}));
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
  auto keys = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 0, "c_custkey")});
  auto rp = fb::CreateCudfRepartition(fbb, fb::PartitioningKind_Hash, lanes, keys, scan_node);
  return finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfRepartition, rp.Union()));
}

TEST(Scatter, TwoInputHandlesAreRefusedNotConcatenated) {
  // The emitter sends one batch a call (gpu_backend/emit.rs), so a second handle is a caller
  // defect the arm names rather than a case it serves.
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = customer_scatter_plan(fbb, 4);
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g0{0}, g1{1};
  uint64_t in[2] = {session.execute_scan_rowgroups(0, g0, nullptr),
                    session.execute_scan_rowgroups(0, g1, nullptr)};
  uint64_t counts[1] = {2};
  uint64_t out[4] = {};
  size_t produced = 0;
  try {
    session.execute_node(1, in, counts, 1, out, 4, &produced, nullptr);
    FAIL() << "two handles were concatenated";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("exactly one handle"), std::string::npos) << e.what();
  }
}

TEST(Scatter, EmptyLanesAreZeroRowSlices) {
  // region's five rows into 64 lanes: 59 or more lanes get nothing, and each must still be a
  // handle of zero rows and zero string bytes.
  flatbuffers::FlatBufferBuilder fbb;
  auto path = fbb.CreateString(parquet_path("region"));
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {{"r_regionkey", fb::DataType_Int32}, {"r_name", fb::DataType_Utf8}});
  auto scan = fb::CreateCudfScan(fbb, paths, schema, fbb.CreateVector(std::vector<uint32_t>{0, 1}));
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
  auto keys = fbb.CreateVector(
      std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 0, "r_regionkey")});
  auto rp = fb::CreateCudfRepartition(fbb, fb::PartitioningKind_Hash, 64, keys, scan_node);
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfRepartition, rp.Union()));
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{0};
  peacock::NodeStats scan_stats{};
  uint64_t in = session.execute_scan_rowgroups(0, g, &scan_stats);
  uint64_t counts[1] = {1};
  std::vector<uint64_t> out(64);
  std::vector<peacock::NodeStats> stats(64);
  size_t produced = 0;
  session.execute_node(1, &in, counts, 1, out.data(), 64, &produced, stats.data());
  ASSERT_EQ(produced, 64u);
  uint64_t rows = 0, bytes = 0, empty = 0;
  for (size_t p = 0; p < 64; ++p) {
    rows += stats[p].rows;
    bytes += stats[p].varlen_content_bytes;
    if (stats[p].rows == 0) {
      ++empty;
      EXPECT_EQ(stats[p].varlen_content_bytes, 0u);
      EXPECT_EQ(session.table_for(out[p]).num_rows(), 0);
    }
  }
  EXPECT_GE(empty, 59u);
  EXPECT_EQ(rows, scan_stats.rows);
  EXPECT_EQ(bytes, scan_stats.varlen_content_bytes);
}
```

- [x] **Step 2: Device cycle:** the first red (no throw), the second green (and it must stay so).
- [x] **Step 3: The fix.** Replace the gather (`:517-538`) with:

```cpp
    if (child[0].size() != 1)
      throw std::runtime_error(
          "NodeSession::execute_node: a Hash repartition is handed exactly one handle per "
          "call — the emitter sends one batch a call (gpu_backend/emit.rs) — and got " +
          std::to_string(child[0].size()) + " (#197)");
    auto it = impl_->registry.find(child[0][0]);
    if (it == impl_->registry.end())
      throw std::runtime_error("NodeSession::execute_node: unknown input handle");
    TableResult input = std::move(it->second);
    impl_->registry.erase(it);
```

  and delete the comment block at `:500-509` that says the concat has no caller, replacing it
  with one line: "One handle per call: the emitter's contract (#197)." `combined` becomes
  `input.view()` in the `spark_hash_partition` call; the per-partition copies stay until Task 4.
- [x] **Step 4: Device cycle:** both green; `NodeRegions.*` unchanged.
- [x] **Step 5: Commit.** `git commit -m "the repartition arm takes exactly one handle (#197)"`.

### Task 4: Partitions share the partitioned table (#145)

**Files:**
- Modify: `cpp/src/node_session.cpp:540-578` (the scatter loop)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Consumes: `TableResult::owning`, `slice` (Task 1); `varlen_content_bytes` (Task 2); the
  one-handle arm (Task 3); `peacock::stats_mr()` (`cpp/include/peacock/rmm_pool.hpp:86`, the
  statistics adaptor `main()` installs above the pool).

- [x] **Step 1: Three cases.** The allocation probe first, reused by exit-copies:

```cpp
/// What `fn` allocated: the bytes it asked for in total, its high-water mark above where it
/// started, and what it left allocated — from the statistics adaptor main() installs.
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

TEST(Scatter, PartitionsShareTheirTableAndSurviveTheirSiblings) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = customer_scatter_plan(fbb, 4);
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{1};
  uint64_t in = session.execute_scan_rowgroups(0, g, nullptr);
  auto keys = keys_of(session.table_for(in));
  uint64_t counts[1] = {1};
  uint64_t out[4] = {};
  size_t produced = 0;
  session.execute_node(1, &in, counts, 1, out, 4, &produced, nullptr);
  ASSERT_EQ(produced, 4u);
  EXPECT_EQ(session.table_for(out[0]).owners[0].use_count(), 4)
      << "four partitions, one partitioned column, no copies";
  std::vector<int64_t> back;
  for (auto h : out) {
    auto part = keys_of(session.table_for(h));
    back.insert(back.end(), part.begin(), part.end());
  }
  std::sort(back.begin(), back.end());
  std::sort(keys.begin(), keys.end());
  EXPECT_EQ(back, keys) << "the partitions concatenate back to the input";
  auto survivor = keys_of(session.table_for(out[2]));
  session.release(out[0]);
  session.release(out[1]);
  session.release(out[3]);
  EXPECT_EQ(session.table_for(out[2]).owners[0].use_count(), 1);
  EXPECT_EQ(keys_of(session.table_for(out[2])), survivor);
}

TEST(Scatter, HoldsTheInputOnlyUntilTheTableIsPartitioned) {
  // The input arrives allocated (outside this scope); inside it the scatter allocates the
  // partitioned table and frees the input. Copying every partition out, with input and
  // partitioned table both still held, put the peak a whole input higher.
  //
  // The bound is the partitioned table plus the scatter's own temporaries — per row a 4-byte
  // hash, a 4-byte partition id and a 4-byte gather map, and a string column's offsets — and
  // nothing else. The old arm exceeds it by a whole input (the copies); a ratio such as 1.5x
  // would sit inside those temporaries on a narrow table and prove nothing.
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = customer_scatter_plan(fbb, 4);
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{0};
  uint64_t in = 0;
  auto scan = allocated_by([&] { in = session.execute_scan_rowgroups(0, g, nullptr); });
  ASSERT_GT(scan.net, 0);
  const auto& input = session.table_for(in);
  const int64_t rows = input.num_rows();
  int64_t offsets = 0;
  for (auto const& c : input.columns)
    if (c.type().id() == cudf::type_id::STRING) offsets += (rows + 1) * int64_t{4};
  const int64_t temporaries = rows * 12 + offsets + (int64_t{1} << 16);  // + allocator slack
  uint64_t counts[1] = {1};
  uint64_t out[4] = {};
  size_t produced = 0;
  auto scatter = allocated_by(
      [&] { session.execute_node(1, &in, counts, 1, out, 4, &produced, nullptr); });
  std::cout << "[scatter] input " << scan.net << " peak " << scatter.peak << "\n";  // the record
  EXPECT_LE(scatter.peak, scan.net + temporaries)
      << "peak " << scatter.peak << " above the partitioned table (" << scan.net
      << ") and the scatter's temporaries (" << temporaries << ")";
}

TEST(Slice, OwnsItsRowsSoTheBatchCanGo) {
  // slice_handle keeps its copy: a view would hold the whole batch for a tenth of its rows.
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = customer_scan_plan(fbb, {0, 1});
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{0};
  uint64_t in = 0;
  auto scan = allocated_by([&] { in = session.execute_scan_rowgroups(0, g, nullptr); });
  const auto rows = static_cast<uint64_t>(session.table_for(in).num_rows());
  uint64_t sliced = 0;
  auto slice = allocated_by([&] { sliced = session.slice_handle(in, 0, rows / 10); });
  EXPECT_LT(slice.net, -scan.net / 2) << "the batch is freed; only a tenth of it is held";
  EXPECT_EQ(session.table_for(sliced).owners[0].use_count(), 1);
}
```

- [x] **Step 2: Device cycle:** the first red (`use_count` 1), the second red (peak about 2× the
  input, above the bound by about one input), the third green (and it must stay so). Copy the
  `[scatter] input … peak …` line into the detail file: it is the "before".
- [x] **Step 3: The fix.** After `spark_hash_partition` returns:

```cpp
    auto [parted, offsets] = peacock::partitioning::spark_hash_partition(
        input.view(), key_cols, static_cast<cudf::size_type>(n));
    // The input goes as soon as the partitioned copy exists: holding it to the end of the
    // arm, with every partition copied out beside both, was a peak of three inputs (#145).
    input = TableResult{};
    const cudf::size_type total = parted->num_rows();
    TableResult whole = TableResult::owning(std::move(parted), std::move(column_names));
    for (size_t p = 0; p < n; ++p) {
      cudf::size_type start = offsets[p];
      cudf::size_type end = (p + 1 < n) ? offsets[p + 1] : total;
      std::optional<ScopedNodeTimer> own;
      if (p > 0) own.emplace(sink, seq, p, call_index);
      // A view of the partitioned table, sharing its columns' owners: no copy.
      TableResult part = whole.slice(start, end);
      if (p == 0) shared_timer.stop(); else own->stop();
      auto ptv = part.view();
      ...  // stats, handle, registry — unchanged
    }
```

  Rewrite the comment above the arm: the partitions share one table; a lane that does not drain
  keeps all of it alive; the p1..N−1 regions time only the slice.
- [x] **Step 4: Device cycle:** the three green; `NodeRegions.EveryCallOpensOneRegionPerOutputPartition`
  green (still N regions); the full gpu tier green. Copy the new `[scatter]` line beside the old
  one in the detail file and check the drop: `peak_before − peak_after ≥ 0.8 × input` (the copies,
  less allocator rounding). A smaller drop means a copy survived; find it before committing.
- [x] **Step 5: Commit.** `git commit -m "a scatter's partitions share one table, the input freed at once (#145)"`.

### Task 5: The tiers, the benchmark, the wiki

- [x] **Step 1: Device cycle, the whole gpu tier and the corpus at the tp4 modes.** Every answer
  unchanged (the corpus device cells on today stay green). The benchmark timings of the tp4
  device cells on today (tpch q1, shuffle-additive-avg and the other enabled tp4 cells) before
  Task 1 and after Task 4, into the detail file: the p1..N−1 regions drop toward zero.
- [x] **Step 2: The accounting ticket** in `llm-wiki/tickets/memory.md` (the next number from
  `tickets.md`, counter and counts updated): "the driver's memory model under-reports a shared
  scatter's partitions" — the driver prices each lane's batch alone
  (`executor/driver/accounting.rs`), so a released lane's bytes leave the model while the device
  holds them behind a sibling; the fix is device-reported residency
  (`GpuBackend::resident_bytes()`); no corpus query; a pin would compare the model with the
  statistics adaptor's `value` after releasing three of four partitions.
- [x] **Step 3: The wiki.** `architecture.md`: the handle is a per-column-owner `TableResult`; a
  scatter's partitions share; slices and the merge's fetch still copy, and why. `build-test.md`:
  `peacock_plan_tests` count (+9) and the row text. #145 and #197 closed per `tickets.md`'s rule,
  with the commit.
- [x] **Step 4: Commit.** `git commit -m "#145, #197 closed: the scatter shares; the accounting ticket filed"`.

### Task 6: The record

- [x] Detail file: the gtest output of each red and green step, the peak numbers of
  `HoldsTheInputOnlyUntilTheTableIsPartitioned` before and after, the benchmark table, any
  producer `owning()` refused in Task 1. `git commit -m "refcounted-scatter: the record"`.
