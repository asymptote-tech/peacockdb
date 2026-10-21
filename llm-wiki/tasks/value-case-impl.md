# value-case implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The device answers `CASE x WHEN v THEN … END` by lowering it onto the search form's
`copy_if_else` fold. tpcds q39's five device cells turn on (#57).

**Architecture:** One function changes, `build_column_case` (`cpp/src/expr.cpp`).
- It builds the comparand once.
- Each WHEN becomes a `BOOL8` column, `comparand = when`, from `cudf::binary_operation`. A
  literal WHEN uses the scalar overload.
- The fold that already exists then runs unchanged.

The tests are a gtest, the `bug_` pin turned into agreement cases, and q39 on the device. The
code and the corpus edits are both in the tree before the one sync, so the green build records
q39 in the same session.

**Tech stack:** C++20/cuDF 25.02 (gtest, `cpp/tests/gpu/test_plan_executor.cpp`, built into
`peacock_plan_tests`), the Rust device harness (`peacockdb-core/src/tests/gpu_tests/exec_cases.rs`,
`--features gpu`), the corpus (`corpus_cases.inc`, `testdata/cost-registry.csv`), nebius-gpu.

**Spec:** [`value-case.md`](value-case.md). Task 9 of chain K, on welford-device's branch. That
branch was rebased onto master on 2026-10-20.

## Global Constraints

- **SQL's rules hold:**
  - a NULL comparand or a NULL WHEN never matches;
  - the first matching WHEN wins;
  - a WHEN may map to NULL;
  - no match and no ELSE answers NULL of the THEN type.
- **Nothing else changes:** no wire, ABI or facade change, and no cpu golden moves. Nothing else
  in `build_column_case` changes.
- **GPU:** nebius-gpu, in chain K's `~/peacockdb-K`, through the *Device cycle* below. sf40 and
  benchmarks are out.

## Review Focus

1. **A NULL comparand row:** `copy_if_else` reads the null condition as false and falls to ELSE.
   Pinned by the gtest's row 4 and by `a_value_case_without_else_answers_null`.
2. **Two WHENs that both match:** the first must win, and the backward fold gets this right only
   if each condition is the WHEN's own. Pinned by the gtest's duplicate `1`.
3. **A WHEN that is a column, not a literal:** the scalar fast path must not be the only path.
   Pinned by the gtest's column WHEN.
4. **A WHEN that maps to NULL:** q39's `WHEN 0 THEN NULL`. Pinned by the gtest's `WHEN 2 THEN NULL`.
5. **No ELSE:** the null fill takes the THEN type. Pinned by the gtest's no-ELSE plan.

## Device cycle

One sync per task, with a red build (the fix reversed) and then a green build. Every command
runs in the foreground: a backgrounded build chain dies mid-build with no error. The plan
gtests live in `peacock_plan_tests`, not `peacock_gpu_tests`.

```bash
G=scripts/gpu-host.sh
DIR=peacockdb-K
B="cd ~/$DIR && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build"
FIX=cpp/src/expr.cpp
timeout 60 git diff -R -- $FIX > /tmp/value-case-red.patch   # the fix, still uncommitted, reversed
timeout 2400 $G start              # exit 75: no capacity; the coordinator owns the retry
timeout 3600 scripts/gpu-host-sync.sh --dest "$DIR"
timeout 60 $G ssh 'ls -d ~/data/miniforge3/envs/rapids-cuda-12.2/lib ~/miniforge3/envs/rapids-cuda-12.2/lib'
CUDF_LIB='<the one that exists>'
R="cd ~/$DIR && . ~/peacock-env.sh && export LD_LIBRARY_PATH=\$PWD/cpp/install/lib:$CUDF_LIB PEACOCK_TESTDATA_DIR=\$PWD/testdata &&"
timeout 300 $G ssh "cd ~/$DIR && patch -p1" < /tmp/value-case-red.patch
timeout 10800 $G ssh "$B"          # red build: run the red set
timeout 300 $G ssh "cd ~/$DIR && patch -p1 -R" < /tmp/value-case-red.patch
timeout 10800 $G ssh "$B"          # green build: run the green set, then record
timeout 600 rsync -a -e "$G rsh" "nebius-gpu:$DIR/testdata/goldens/" testdata/goldens/ --include='*/' --include='gpu-result.txt' --exclude='*'
timeout 600 $G stop
```

- **One binary per `ssh`**, with `--test-threads=1` for Rust. A gtest filter that matches
  nothing exits 0, so each run's log is checked for the number of tests it ran.
- **The red set:** every test named red must fail for the reason named. A test that is green in
  the red build is a finding, to settle with `superpowers:systematic-debugging` before going on.
- **The corpus edits travel in the same sync:** `test_gpu_corpus` gets q39's device tests only
  from a line that enables them at compile time.
- **Never `--pull-results`:** it pulls from shad-gpu. Use the `rsync` above.
- **If the host is lost:** `$G status`, `$G start`, sync, and rerun the step.

---

### Task 1: The value form, its tests, and q39's cells

**Files:**
- Modify: `cpp/src/expr.cpp` (`build_column_case`, ~787)
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (`peacock_plan_tests`)
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` (the `#57` block, ~347)
- Modify: `peacockdb-core/src/plan/mod.rs` (`PlanError::Unsupported`'s doc, ~54)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (q39's line and the comments that cite
  #57 or q39's device cells: ~125, ~182–184, ~190–192, ~261–264, re-located by grep)
- Modify: `testdata/cost-registry.csv` (q39's row), `scripts/exec_model/operators/expressions.py`
  (~354, "what #57 withheld")
- Regenerate: `testdata/goldens/tpcds.sf1/gpu-result.txt`, `testdata/report.html`
- Modify: `llm-wiki/build-test.md` (~658–662 "but for four" and its ticket links; the counts:
  `test_cpu_corpus` +6, `test_gpu_corpus` +5, `peacock_plan_tests` +1, the gpu lib's exec_cases
  +1), `llm-wiki/tickets/corpus-coverage.md` → `archive/archived-tickets.md` (#57), `tickets.md`

**Interfaces:**
- Consumes: the gtest helpers `make_int64_literal`, `make_null_literal(fbb, fb::DataType_Int32)`,
  `make_string_literal`, `make_col_ref`, the `region` scan at ~780, `WholePlan`, and
  `fb::CreateCaseExprNode(fbb, expr, arms, else)`.
- Produces: no new symbol.

- [x] **Step 1: Write the gtest** `CaseValueForm.MatchesFirstNullNeverAndElseOrNull`.
  - **The plan:** a project over `region`, whose `r_regionkey` is Int32 0..4. The comparand is
    `CASE WHEN r_regionkey = 4 THEN NULL::Int32 ELSE r_regionkey END`, so row 4 holds a NULL
    comparand of the same type.
  - **The WHENs, in order:**
    - `1` → `'one'`;
    - `1` again → `'again'`;
    - `2` → `NULL::Utf8`;
    - a `NULL::Int32` literal → `'null'`;
    - the column `r_regionkey` itself → `'self'`.

    ELSE is `'other'`. The literals are Int32, like the comparand.
  - **Expected rows 0..4:** `self`, `one`, NULL, `self`, `other`.
    - Row 1: the first WHEN wins over the duplicate and the self-match.
    - Row 2: WHEN 2 maps to NULL, ahead of the self-match.
    - Row 4: the comparand is NULL, so nothing matches, the NULL WHEN included.
  - **A second plan, the same without ELSE:** the same rows, except row 4 is NULL.
- [x] **Step 2: Write the Rust agreement cases**, replacing
  `bug_a_value_case_is_refused_on_the_device`:
  - `a_value_case_agrees`: `CASE key WHEN lit_i32(1) THEN 'one' WHEN lit_i32(2) THEN 'two' ELSE
    'other' END` over `input()`, where `key` is Int32 0..6 and NULL every 11th row. Then
    `run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted)`.
  - `a_value_case_without_else_answers_null`: the same with no ELSE. The NULL-`key` rows and
    the rows matching nothing answer NULL on both engines.
- [x] **Step 3: Implement** `build_column_case`'s value form.
  - Delete the throw and its comment.
  - Build `auto comparand = evaluate_column(c->expr(), table, names);` once.
  - Per WHEN, the condition is `cudf::binary_operation(comparand.view(), <when>,
    cudf::binary_operator::EQUAL, data_type{BOOL8})`. `<when>` is a scalar when the WHEN is a
    `LiteralExpr`, built the way the column path builds a literal scalar, and `evaluate_column`
    otherwise.
  - The backward `copy_if_else` fold is unchanged and takes these conditions. A comment says
    that a null condition is false in `copy_if_else`, which is what keeps SQL's rule.
- [x] **Step 4: Edit `plan/mod.rs`'s doc** so `PlanError::Unsupported` no longer lists "a
  value-form CASE (#57)". Fix `expressions.py:354`'s sentence to say the device answers the
  value form.
- [x] **Step 5: Turn q39's cells on.**
  - Its line: `all_modes, none` → `all_modes, all_modes`.
  - Every comment in the include that names q39's device cells or #57 says what is true now.
  - Its registry row: the five `gpu_*` cells `enabled`, and `57` struck from `tickets`. The
    oracle columns stay `data_fusion_approximate,golden_approx_std`.
  - Then `python3 scripts/ticket_widget.py`.
- [x] **Step 6: Run locally:**
  - `cargo test --features rust-only -p peacockdb-core --lib`;
  - `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- device_order_tpcds_q39`,
    which checks the order hazard with no GPU;
  - compile `--features gpu --no-run`.

  Expected: PASS.
- [x] **Step 7: The device, red then green** (*Device cycle*, `FIX=cpp/src/expr.cpp`).
  - **Red build:** these must fail on `value-form CASE not supported`:
    - `$R cpp/install/bin/peacock_plan_tests --gtest_filter='CaseValueForm.*'` (2 tests ran);
    - `$R cpp/install/rust-tests/peacockdb_core_gpu_lib --test-threads=1 gpu_tests::exec_cases::a_value_case`.
  - **Green build:** both pass. Then:
    - `peacock_plan_tests --gtest_filter='Case*:Literals.*'` passes;
    - `$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpcds_q39`
      runs 5 tests, and all 5 pass;
    - the `rsync`, and `git diff --stat testdata/goldens/` shows only `tpcds.sf1/gpu-result.txt`,
      with five q39 sections added.

    A cell that fails still records its section, because the case records before it asserts.
    Such a cell goes back off under a ticket, and its section is removed.
- [x] **Step 8: Run locally** `--test test_cpu_corpus` (rust-only), which holds the
  `duckdb_gpu_tpcds_q39_*` cases and
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`. Expected: PASS.
- [x] **Step 9: Archive #57 (value-form CASE)** under Done, and update the index and
  `build-test.md` (the counts, and the "but for four" passage).
- [ ] **Step 10: Commit**, in two commits:
  - `expr: the device answers a value-form CASE (#57)`: the C++, the gtest, `exec_cases.rs`,
    `plan/mod.rs` and `expressions.py`;
  - `corpus: tpcds q39 on the device; #57 archived`: the rest.
- [ ] **Step 11: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
