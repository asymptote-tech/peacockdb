# guard-checks implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The plan validator refuses a pass-through node whose column count differs from its
input's (#233), and both row-range clamps — Rust's and C++'s — are checked against one committed
case table, with the driver's mock calling the shipped clamp (#174).

**Architecture:** One count comparison ahead of the pairwise zip in `types_across_the_edge`. A
text fixture in `testdata/fixtures/` that a Rust unit test and a C++ gtest each parse and assert
against their own clamp; neither calls the other.

**Tech stack:** Rust (`--features rust-only`, `--lib`), C++20 gtest in `peacock_cpu_tests`
(links cuDF, touches no device), CMake.

**Spec:** [`guard-checks.md`](guard-checks.md).

## Global constraints

- No GPU: no device run and no GPU cycle on this chain (it must not contend with chain J for
  nebius-gpu). The task reaches `done` when every CI job but the GPU tests is green. The `build-test-shadgpu.sh` and
  `pipeline.yml` GPU-job edits go in untested; they first run on the next GPU run after merge.
- No production behaviour changes. Neither clamp's code changes; a disagreement the table shows
  is a ticket, not a fix here.
- No facade, trait, ABI, wire or golden change.
- Commit messages at most 10 lines (`coding-style.md`).
- Build in a workspace, never in the primary checkout; never share a cargo target dir across
  worktrees.
- Verification is local: rust-only `--lib`, and `ctest -L cpu` with `cpp/build` configured
  against cuDF 25.02: `scripts/build.sh --configure --build --cudf_ROOT
  ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12` (`build-test.md`: `cpp/build` stays
  25.02; the `rapids` env is 26.02).

## Review focus

1. **A pass-through node declaring more columns than its input**, not only fewer. Expected:
   refused the same way. Pinned in Task 1, step 1 (two cases).
2. **A test that proves nothing.** A Filter without a projection is already width-checked by
   `declared_width`, which runs first with the same wording; a case built on it passes before the
   change. Expected: Task 1's cases use a `GpuSort` and fail before the change.
3. **A malformed or empty fixture** (a typo, a missing `->`, a truncated file). Expected: both
   suites fail naming the line, never pass with zero cases. Pinned in Tasks 2 and 3.
4. **`max` in the `rows` column.** Not a case today; the parsers accept `max` only in `length`
   and `offset`. Expected: rows is a plain integer; `max` there is a malformed line. Tasks 2, 3.
5. **The C++ test run from an installed tree on the GPU host.** `build-test-shadgpu.sh:403` and
   `pipeline.yml:597` run every `cpp/install/bin/peacock_*_tests`, `peacock_cpu_tests` included,
   with `PEACOCK_TESTDATA_DIR` set to a remote tree filled from hand-written lists. Expected: the
   fixture is there. Task 3, step 6 adds it to both lists. (`build-test.sh`'s default cpu mode
   exports no `PEACOCK_TESTDATA_DIR`; its binaries read the compile-time path, as the Rust unit
   tests already do.)

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/validate.rs` | `types_across_the_edge`: the count check |
| `peacockdb-core/src/plan/validate/tests.rs` | #233's cases |
| `testdata/fixtures/row-range-clamp.txt` | the case table (new) |
| `testdata/fixtures/README.md` | one paragraph for it |
| `peacockdb-core/src/executor/row_range/tests.rs` | the Rust reader and assertion |
| `cpp/tests/cpu/test_executor.cpp` | the C++ reader and assertion |
| `cpp/CMakeLists.txt` | `PEACOCK_TESTDATA_DIR` for `peacock_cpu_tests` |
| `scripts/build-test-shadgpu.sh`, `.github/workflows/pipeline.yml` | `testdata/fixtures` to the GPU host |
| `peacockdb-core/src/executor/driver/tests/mock.rs` | `MockUnload` calls the clamp |
| `llm-wiki/build-test.md` | case counts |

---

### Task 1: The validator checks a pass-through node's column count (#233)

**Files:**
- Modify: `peacockdb-core/src/plan/validate.rs` (`types_across_the_edge`, ~l.291)
- Test: `peacockdb-core/src/plan/validate/tests.rs`

**Interfaces:** none produced.

`declared_width` already checks a Filter without a projection (`validate.rs:222-224`, the same
message), and it runs before `types_across_the_edge`, so a filter case passes before the change and
proves nothing. The case is a `GpuSort`: it falls to `declared_width`'s `_ => return Ok(())` arm.
Its constructor derives the schema from its input, so the test overrides the private `kind`
field, which `plan::validate::tests` can reach as a descendant of `plan`.

- [ ] **Step 1: Write the failing tests**, after `a_column_that_changes_type_across_an_edge_is_refused`:

```rust
/// A sort over a two-column source, declaring `declared` instead of the schema its
/// constructor derived. The planner never builds it; a test rewriting a planned tree can.
fn sort_declaring(declared: &[&str]) -> Box<dyn GpuNode> {
    let input = source(schema_of(&["a", "b"]), PartitionLayout::new(1));
    let mut sort = GpuSort::new(
        input,
        vec![ColumnOrder {
            column: 0,
            ascending: true,
            nulls_first: false,
        }],
        None,
    );
    let layout = sort.kind.layout().expect("a sort has a layout").clone();
    sort.kind = NodeKind::Intermediate {
        layout,
        schema: schema_of(declared),
    };
    Box::new(sort)
}

#[test]
fn a_pass_through_node_declaring_fewer_columns_than_its_input_is_refused() {
    // A sort carries its input's columns. Comparing them pairwise stops at the shorter
    // list, so a dropped column passed both checks.
    invalid(
        validate(rooted(sort_declaring(&["a"])).as_ref()),
        "GpuSort: it declares 1 columns and its input produces 2",
    );
}

#[test]
fn a_pass_through_node_declaring_more_columns_than_its_input_is_refused() {
    invalid(
        validate(rooted(sort_declaring(&["a", "b", "c"])).as_ref()),
        "GpuSort: it declares 3 columns and its input produces 2",
    );
}

#[test]
fn a_pass_through_node_declaring_its_inputs_columns_passes() {
    validate(rooted(sort_declaring(&["a", "b"])).as_ref())
        .expect("a sort carrying both columns is valid");
}
```

  If `NodeKind::Intermediate` has fields beyond `layout` and `schema`, or `GpuSort`'s name is not
  `"GpuSort"` (`plan/mod.rs`'s name table), match them; the message after the name is what
  matters.

- [ ] **Step 2: Run them; the first two fail.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- plan::validate::tests::a_pass_through
```

  Expected: the "fewer" and "more" cases FAIL (`expected an invalid plan … got Ok(())`); the
  passing case PASSES. If "fewer" or "more" passes here, the case reaches another check: stop
  and find which before going on.

- [ ] **Step 3: Implement.** In `types_across_the_edge`, after `let (ours, theirs) = (…);` and
  before the zip:

```rust
    let (declared, carried_width) = (ours.fields.fields().len(), theirs.fields.fields().len());
    if declared != carried_width {
        return Err(PlanError::Invalid(format!(
            "{}: it declares {declared} columns and its input produces {carried_width}",
            node.name()
        )));
    }
```

  Update the function's doc comment: it now checks the count, then the fields pairwise. Remove
  the sentence in `declared_width`'s `_ => return Ok(())` arm that says the pairwise compare
  "does not check the count".

- [ ] **Step 4: Run the validator tests and the plan goldens.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- plan::validate planner::tests::plan_goldens
```

  Expected: all PASS — the planner never builds the refused shape, so no golden moves.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/plan/validate.rs peacockdb-core/src/plan/validate/tests.rs
git commit -m "#233: the validator checks a pass-through node's column count"
```

---

### Task 2: The case table, and the Rust clamp reads it (#174)

**Files:**
- Create: `testdata/fixtures/row-range-clamp.txt`
- Modify: `testdata/fixtures/README.md`
- Modify: `peacockdb-core/src/executor/row_range/tests.rs` (whole file)

**Interfaces:**
- Produces: the fixture's format, which Task 3's C++ parser reads — one case per line,
  `offset length rows -> begin end`, fields separated by runs of spaces; `max` (= `u64::MAX`)
  allowed in `offset` and `length` only; `#` starts a comment line; blank lines ignored.

- [ ] **Step 1: Write the fixture.**

```
# The row-range clamp: offset length rows -> begin end
#
# The rows of a batch a limit keeps, as the half-open span [begin, end). Read by both clamps'
# tests: Rust's RowRange::clamp (executor/row_range/tests.rs) and C++'s clamp_row_range
# (cpp/tests/cpu/test_executor.cpp). `max` is u64::MAX, the to-the-end sentinel, and is
# allowed in offset and length only.

# A range to the end of the batch
0    max   100   ->  0    100
40   max   100   ->  40   100
0    max   4     ->  0    4
3    max   4     ->  3    4

# A range past the end is clamped to the rows that are there
90   1000  100   ->  90   100
0    100   100   ->  0    100
1    100   4     ->  1    4

# A range at or past the end is empty
100  10    100   ->  100  100
500  max   100   ->  100  100
9    1     4     ->  4    4
0    0     100   ->  0    0
0    max   0     ->  0    0

# The sentinel does not overflow: the length is taken against the rows remaining, because
# offset + max wraps and the wrapped end lands inside the batch
1    max   100   ->  1    100
max  max   100   ->  100  100
```

- [ ] **Step 2: The README.** Its opening line says the fixtures are "a format two crates read
  independently"; this one is read by a crate and the C++ tests, so reword it to "a format two
  readers read independently". Then a paragraph after `two-row-registry.csv`'s:

```markdown
`row-range-clamp.txt` is the row-range clamp's cases, `offset length rows -> begin end`. The rule
is written twice, `RowRange::clamp` in Rust and `clamp_row_range` in C++, because the cpu backend
never crosses the ABI. Each side's test reads this file and asserts its own clamp, so a case added
here reaches both, and a drift fails the drifting side on the shared line.
```

- [ ] **Step 3: Replace `row_range/tests.rs`** with a reader and one test. The old two tests'
  four cases are lines of the table now, beside C++'s ten.

```rust
use super::RowRange;
use crate::test_support::testdata_root;

/// One line of `testdata/fixtures/row-range-clamp.txt`.
struct Case {
    line: usize,
    range: RowRange,
    rows: u64,
    span: (u64, u64),
}

/// Digits only, as the C++ reader takes them: `u64::from_str` accepts a leading `+`.
fn count(field: &str, line: usize) -> u64 {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        panic!("row-range-clamp.txt:{line}: `{field}` is not a count");
    }
    field
        .parse()
        .unwrap_or_else(|_| panic!("row-range-clamp.txt:{line}: `{field}` does not fit a u64"))
}

fn bound(field: &str, line: usize) -> u64 {
    if field == "max" { u64::MAX } else { count(field, line) }
}

fn cases() -> Vec<Case> {
    let path = testdata_root().join("fixtures/row-range-clamp.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut cases = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        let [offset, length, rows, "->", begin, end] = fields.as_slice() else {
            panic!("row-range-clamp.txt:{line}: expected `offset length rows -> begin end`, got `{trimmed}`");
        };
        cases.push(Case {
            line,
            range: RowRange {
                offset: bound(offset, line),
                length: bound(length, line),
            },
            rows: count(rows, line),
            span: (count(begin, line), count(end, line)),
        });
    }
    assert!(!cases.is_empty(), "{} holds no case", path.display());
    cases
}

/// The clamp answers `(offset, length)`; the table speaks the half-open span C++ answers,
/// so the length is added back here and nowhere else.
#[test]
fn the_clamp_answers_every_case_in_the_shared_table() {
    for case in cases() {
        let (offset, length) = case.range.clamp(case.rows);
        assert_eq!(
            (offset, offset + length),
            case.span,
            "row-range-clamp.txt:{}: {:?} over {} rows",
            case.line,
            (case.range.offset, case.range.length),
            case.rows
        );
    }
}
```

  If `RowRange` is not `Debug`, the message prints the two fields, as written. If the lib test
  build does not see `crate::test_support` under `rust-only`, check how
  `planner/tests/plan_goldens.rs` imports it (it does) and match.

- [ ] **Step 4: Run it.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- executor::row_range
```

  Expected: PASS, one test. Then, by hand and not committed, change `3 max 4 -> 3 4` to
  `3 max 4 -> 3 5`: FAIL naming `row-range-clamp.txt:<n>`. Then put `3 max` alone on a line:
  FAIL with "expected `offset length rows -> begin end`". Revert both.

- [ ] **Step 5: Commit.**

```bash
git add testdata/fixtures/row-range-clamp.txt testdata/fixtures/README.md peacockdb-core/src/executor/row_range/tests.rs
git commit -m "#174: the Rust clamp reads the shared case table"
```

---

### Task 3: The C++ clamp reads the same table (#174)

**Files:**
- Modify: `cpp/CMakeLists.txt` (the `peacock_cpu_tests` block, ~l.185)
- Modify: `cpp/tests/cpu/test_executor.cpp` (the four `ClampRowRange` tests, l.206-231)
- Modify: `scripts/build-test-shadgpu.sh` (~l.244), `.github/workflows/pipeline.yml` (~l.503-508)

**Interfaces:**
- Consumes: Task 2's fixture format.

- [ ] **Step 1: The compile definition.** After `target_link_libraries(peacock_cpu_tests …)`:

```cmake
# The fixtures the tests read; PEACOCK_TESTDATA_DIR in the environment overrides it, as for
# peacock_plan_tests, so an installed binary on another host reads that host's tree.
target_compile_definitions(peacock_cpu_tests PRIVATE
  PEACOCK_TESTDATA_DIR="${PEACOCK_TESTDATA_DIR}"
)
```

  `PEACOCK_TESTDATA_DIR` is set at `cpp/CMakeLists.txt:175`, above this block. If it is set
  after it, move the block below.

- [ ] **Step 2: Replace the four `ClampRowRange` tests** (`ToTheEndSentinel`,
  `PastTheEndClamps`, `AtOrPastTheEndIsEmpty`, `TheSentinelDoesNotOverflow`) with a reader and
  one test. Keep the comment above them; add `<cstdlib> <fstream> <sstream> <string> <vector>`
  to the includes if missing.

```cpp
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

TEST(ClampRowRange, AnswersEveryCaseInTheSharedTable) {
  for (const auto& c : clamp_cases()) {
    const auto rows = static_cast<cudf::size_type>(c.rows);
    EXPECT_EQ(peacock::clamp_row_range(c.offset, c.length, rows),
              std::make_pair(static_cast<cudf::size_type>(c.begin),
                             static_cast<cudf::size_type>(c.end)))
        << "row-range-clamp.txt:" << c.line;
  }
}
```

  Every malformed field fails through `ADD_FAILURE` naming the line; nothing throws.

- [ ] **Step 3: Build and run.**

```bash
scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12
ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Expected: PASS; `ClampRowRange.AnswersEveryCaseInTheSharedTable` is one test where four were.
  Drop `--configure` once `cpp/build` is configured in this workspace.

- [ ] **Step 4: The same hand checks as Task 2, step 4** — a wrong `end`, then a short line —
  each FAILS naming the line. Revert.

- [ ] **Step 5: Run with the override.**

```bash
PEACOCK_TESTDATA_DIR=$PWD/testdata ./cpp/build/peacock_cpu_tests --gtest_filter='ClampRowRange.*'
PEACOCK_TESTDATA_DIR=/nonexistent ./cpp/build/peacock_cpu_tests --gtest_filter='ClampRowRange.*'
```

  Expected: the first PASSES; the second FAILS with "cannot read /nonexistent/…" and "holds no
  case" — never a pass over zero cases.

- [ ] **Step 6: Provisioning to the GPU host.** Both GPU runs install every
  `peacock_*_tests` binary and run them by glob (`build-test-shadgpu.sh:403`,
  `pipeline.yml:597`), `peacock_cpu_tests` included, against a remote testdata tree filled by
  hand-written lists that omit `testdata/fixtures`. A grep for the binary's name finds nothing:
  the glob hides it. Add the directory to both lists:
  - `build-test-shadgpu.sh`, beside the `cost-registry.csv` rsync (~l.244):

```bash
  # The fixtures the C++ cpu tests read (peacock_cpu_tests runs here by glob).
  resilient_rsync -a testdata/fixtures "$REMOTE:$REMOTE_REPO/testdata/"
```

  - `pipeline.yml`'s GPU job, in the `rsync_retry` list (~l.503-508), a line
    `testdata/fixtures \` before `testdata/cost-registry.csv \`.

  The rust lib's row-range test runs on neither host: they run `--lib` with `gpu_tests::` only.

- [ ] **Step 7: Commit.**

```bash
git add cpp/CMakeLists.txt cpp/tests/cpu/test_executor.cpp scripts/build-test-shadgpu.sh .github/workflows/pipeline.yml
git commit -m "#174: the C++ clamp reads the shared case table; the GPU host gets the fixtures"
```

---

### Task 4: The driver's mock calls the shipped clamp, and the docs (#174)

**Files:**
- Modify: `peacockdb-core/src/executor/driver/tests/mock.rs` (`MockUnload::unload`, ~l.565)
- Modify: `llm-wiki/build-test.md`

- [ ] **Step 1: Replace the mock's arithmetic.**

```rust
impl UnloadExecutor<Mock> for MockUnload {
    fn unload(&mut self, batch: MockBatch, rows: RowRange) -> CallResult<CpuBatch> {
        // The shipped rule, so the driver's limit tests are facts about it.
        let (_, taken) = rows.clamp(batch.rows as u64);
        Ok((cpu_batch(taken as usize), self.script.stats()))
    }
}
```

- [ ] **Step 2: Run the driver tests.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- executor::driver
```

  Expected: PASS, every count in `driver/tests/limit.rs` unchanged.

- [ ] **Step 3: The counts in `build-test.md`.** The page is already 5 short before this task:
  the Rust header (l.7) says 1852 and the tables sum to 1857. Recount every row this task touches
  from the code, then set the headers to the sums. Expected, from the review at dbf44bcc:

| count | where | now | after |
|---|---|--:|--:|
| Forwarders and row ranges | l.332 | 5 | 4 |
| Plan types | l.344 | 38 | 41 |
| cpu block, `--lib` | l.25 | 601 | 603 |
| cpu block | l.25 | 1185 | 1187 |
| C++ CPU/FFI unit | l.559 | 15 | 12 |
| Rust (header) | l.7 | 1852 | 1859 |
| C++ (header) | l.7 | 97 | 94 |
| grand total | l.7 | 2330 | 2334 |

  (Plan types +3: Task 1's three validator cases.) Update the row-range rows' descriptions: the
  two clamps share a case table.

- [ ] **Step 4: The full verification bar.**

```bash
cargo test --features rust-only -p peacockdb-core --lib
ctest --test-dir cpp/build -L cpu --output-on-failure
```

  Expected: both green.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/executor/driver/tests/mock.rs llm-wiki/build-test.md
git commit -m "#174: the driver's mock clamps with RowRange::clamp; counts"
```

  #233 and #174 are archived at merge by the helper, not on this branch.
