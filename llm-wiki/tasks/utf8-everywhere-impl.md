# Utf8 everywhere implementation plan

**Goal:** No plan declares a view type: `schema_force_view_types` off, a validation rule that
refuses any view type, every handling site deleted, goldens regenerated, the survey's 76 string
queries rolled out — closing #183.

**Architecture:** One option flip removes the only producer; one rule in `plan/validate.rs`
keeps it removed; everything else is deletion and regeneration. No cast anywhere.

**Tech stack:** Rust (rust-only loop for everything but the rollout), the wire (`gpu_plan.fbs`
loses two enum values, both sides regenerate), C++ (`expr.cpp` arms deleted), `shad-gpu` for
the harness and the rollout.

**Spec:** [`utf8-everywhere.md`](utf8-everywhere.md) — frozen.

## Global constraints

- No cast, no conversion, no new handling of any view type. A view type that survives is a
  finding: the rule refuses it and a ticket says where it came from.
- Deletions only where the spec lists them; a site not in the list that turns out to mention a
  view type is recorded in the detail file before it is touched.
- Every golden change is `Utf8View → Utf8` and nothing else; any other diff line is a finding,
  not a regen to accept.
- `rustfmt` on touched files; commit messages at most 10 lines.
- The device cycle is foreground: `scripts/build-test-shadgpu.sh --build && … --push-binaries
  --patch`, then `PCK_RUN_CPP=0 PCK_TEST_FILTER='<filter>' scripts/build-test-shadgpu.sh --run`.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/lib.rs` | the option, in `build_session_state` |
| `peacockdb-core/src/plan/common.rs` | `check_expr_types`, called from `check_column_refs` so every node's expressions are covered by the existing 19 call sites |
| `peacockdb-core/src/plan/validate.rs` | `no_view_types(node)` over the node schema, in `walk` |
| `peacockdb-core/src/plan/validate/tests.rs` | the rule's tests |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/{serialize,fb_text}.rs`, `cpp/src/expr.cpp` | the wire and its two readers lose the view arms |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs`, `common.rs`, `test_support/result_text.rs`, `tests/gpu_tests/harness_cases.rs` | handling sites deleted |
| `testdata/goldens/**`, `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | regenerated; rolled out |

---

### Task 1: The option and the rule

**Files:**
- Modify: `peacockdb-core/src/lib.rs:38-48`
- Modify: `peacockdb-core/src/plan/common.rs:29` (`check_column_refs`), `plan/validate.rs:61-70` (`walk`)
- Test: `peacockdb-core/src/plan/validate/tests.rs`

**Interfaces:**
- Produces: `plan::common::check_expr_types(expr: &Expr, site: &str) -> Result<(), PlanError>`; `plan::validate::no_view_types(node: &dyn GpuNode) -> Result<(), PlanError>`.

- [ ] **Step 1: Write the failing tests.** In `validate/tests.rs`, beside the existing structural
  tests (copy their node-building shape):

```rust
#[test]
fn a_node_declaring_a_view_type_is_refused() {
    let leaf = given_leaf(&[("s", DataType::Utf8View)]);           // the file's leaf builder
    let err = validate(leaf.as_ref()).unwrap_err().to_string();
    assert!(err.contains("s: Utf8View"), "{err}");
    assert!(err.contains("view type"), "{err}");
}

#[test]
fn a_cast_to_a_view_type_is_refused() {
    let leaf = given_leaf(&[("s", DataType::Utf8)]);
    let project = project_over(leaf, vec![NamedExpr::new(
        Expr::Cast { expr: Box::new(Expr::column(0, "s")), target: DataType::Utf8View }, "v")]);
    let err = validate(project.as_ref()).unwrap_err().to_string();
    assert!(err.contains("GpuProject") && err.contains("Utf8View"), "{err}");
}
```

  If the file has no `given_leaf`/`project_over` helpers, use whatever it builds nodes with;
  the assertions are the contract.

- [ ] **Step 2: Run them red.** `cargo test --features rust-only -p peacockdb-core --lib --
  plan::validate::tests` — both fail: no rule exists.

- [ ] **Step 3: The rule.** In `plan/common.rs`:

```rust
pub(crate) fn is_view_type(t: &DataType) -> bool {
    matches!(t, DataType::Utf8View | DataType::BinaryView
               | DataType::ListView(_) | DataType::LargeListView(_))
}

/// Every type an expression names — a literal's, a cast's target, a scalar function's return,
/// a binary's out type — is one the device can hold. cuDF has no view layout, and the parquet
/// option that produced them is off; one that appears was minted, and this is where it is caught.
pub(crate) fn check_expr_types(expr: &Expr, site: &str) -> Result<(), PlanError> {
    let refuse = |what: &str, t: &DataType| Err(PlanError::Invalid(format!(
        "{site}: {what} is {t}, a view type the device cannot hold")));
    match expr {
        Expr::Literal(v) if is_view_type(&v.data_type()) => refuse("a literal", &v.data_type()),
        Expr::Cast { target, .. } if is_view_type(target) => refuse("a cast target", target),
        Expr::ScalarFunction { return_type, .. } if is_view_type(return_type) =>
            refuse("a scalar function's return type", return_type),
        Expr::Binary { out_type, .. } if is_view_type(out_type) => refuse("a binary's type", out_type),
        _ => Ok(()),
    }?;
    // then recurse exactly as check_column_refs does over children
    ...
}
```

  Call it at the top of `check_column_refs` (`common.rs:29`) so every node that checks its
  column refs — all 19 — checks its types. In `validate.rs`, add to `walk` after
  `declared_width`:

```rust
fn no_view_types(node: &dyn GpuNode) -> Result<(), PlanError> {
    let Some(schema) = node.kind().schema() else { return Ok(()) };
    for (at, field) in schema.fields.fields().iter().enumerate() {
        if is_view_type(field.data_type()) {
            return Err(PlanError::Invalid(format!(
                "{}: column {at} {}: {} is a view type the device cannot hold",
                node.name(), field.name(), field.data_type())));
        }
    }
    Ok(())
}
```

- [ ] **Step 4: The option.** `lib.rs:41`, after `target_partitions`:

```rust
// Strings are Utf8 from the leaf up. The parquet reader is the only producer of Utf8View
// in DataFusion 45, and cuDF has no view layout (#183, utf8-everywhere.md).
config.options_mut().execution.parquet.schema_force_view_types = false;
```

- [ ] **Step 5: Green.** Same command as step 2: both pass. Then `cargo test --features
  rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens` — expect red on every
  golden with a string: that is Task 3's regen, not a failure to fix here.

- [ ] **Step 6: Commit.**
```bash
git add peacockdb-core/src/lib.rs peacockdb-core/src/plan
git commit -m "no plan declares a view type: the parquet option is off and validation refuses one"
```

### Task 2: Delete every handling site

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs:28-36` (`DataType`), `peacockdb-core/src/wire/serialize.rs:72-77,130-131`, `wire/fb_text.rs:348`, `cpp/src/expr.cpp:89,228,333,336,478`
- Modify: `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs:64`, `common.rs:35,115`, `test_support/result_text.rs:92`
- Modify: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` (`declaring_view_strings`, the #183 pin at `:134-161`)

- [ ] **Step 1: The wire.** Remove `Utf8View` and `BinaryView` from `enum DataType` in
  `gpu_plan.fbs`. Regenerate: `cargo build --features rust-only -p peacockdb-core` reruns
  `build.rs` (flatc-fork) for Rust; the C++ header regenerates in the cmake build. Every arm
  naming the two values now fails to compile — that is the list for step 2.

- [ ] **Step 2: Follow the compiler.** Delete each failing arm: `serialize.rs` literal arm
  (`DfScalarValue::Utf8View`) and the two type arms; `fb_text.rs:348`'s `Utf8View` from its
  pattern; `expr.cpp`'s five. Then the sites the compiler cannot see: `spark_partitioning.rs:64`
  (the `Utf8View => cast` arm; plain `Utf8` falls through as it does today), `common.rs:35,115`
  (drop `Utf8View`/`BinaryView` from the patterns), `result_text.rs:92`'s comment sentence.

- [ ] **Step 3: The harness.** In `harness_cases.rs` delete `declaring_view_strings` and the
  `bug_…utf8view…` pin with its `// #183` block. `grep -rn "utf8view\|Utf8View\|declaring_view"
  peacockdb-core/src/tests` must return nothing; if join-cases' or aggregate-cases' Task 8
  left a survivor, delete it and name it in the detail file.

- [ ] **Step 4: Rust-only proof.** `cargo test --features rust-only -p peacockdb-core --lib --
  wire` and `-- executor::cpu_backend` green; `cargo test --features rust-only -p peacockdb-core
  --test test_module_layout` green. `grep -rn "Utf8View\|BinaryView" peacockdb-core/src
  cpp/src flatbuffers` returns only `plan/common.rs`'s rule and `validate/tests.rs`.

- [ ] **Step 5: Commit.**
```bash
git add flatbuffers peacockdb-core/src cpp/src
git commit -m "the wire, the readers and the harness stop handling view types"
```

### Task 3: Goldens and the planner tests

**Files:**
- Modify: `peacockdb-core/src/plan_text/tests.rs:150`, `planner/translator/schema_tests.rs:156,285,305`
- Regenerate: `testdata/goldens/**/*.plans.txt`, `testdata/goldens/recipe-payloads.txt`

- [ ] **Step 1: Re-assert the three tests on `Utf8`** — each asserts a real plan's string type;
  change the expected type, nothing else. Run them: green.

- [ ] **Step 2: Regenerate.** `UPDATE_CANONICAL=1 cargo test --features rust-only -p
  peacockdb-core --lib -- planner::tests::plan_goldens` and the recipe-payloads test the same
  way (`grep -rn UPDATE_CANONICAL peacockdb-core/src/wire` for its name).

- [ ] **Step 3: Read the diff.** `git diff testdata/goldens | grep '^[-+]' | grep -v '^[-+][-+]'
  | grep -v 'Utf8View\|Utf8' | head` must be empty; `git diff --stat` lists only files with a
  string column. Record the file count in the detail file.

- [ ] **Step 4: Full rust-only tier.** `cargo test --features rust-only -p peacockdb-core` —
  every target green.

- [ ] **Step 5: Commit.**
```bash
git add testdata/goldens peacockdb-core/src
git commit -m "goldens: Utf8View is Utf8 in every schema and literal"
```

### Task 4: The device — harness, then the rollout

**Files:**
- Modify: `testdata/cost-registry.csv`, `peacockdb-core/tests/common/corpus_cases.inc`, `llm-wiki/tasks/active-tickets.md`, `llm-wiki/tickets.md`

- [ ] **Step 1: Harness cycle.** Build, ship, patch; `PCK_TEST_FILTER='_cases'` run. Expected:
  every family green, the retired pins gone from the counts. Any red is a finding — stop and
  record it before going on.

- [ ] **Step 2: The rollout.** From `reports/sink-divergence.md` §1's string class, list the 76
  queries. In `corpus_cases.inc` set `gpu_modes` to `tp1_single` for each; run the corpus
  binary (`PCK_TEST_FILTER='gpu_'`). Read each cell: green → registry `gpu_tp1_single`
  `enabled`, `183` struck from `tickets`; red on values → keep `disabled`, a ticket (fifteen
  lines at most) naming the query and what the values showed, its number in `tickets`; red
  above the sink → its existing ticket stays. Never enable a cell you did not see green.

- [ ] **Step 3: Close #183** in `active-tickets.md`: one paragraph naming the fix, the count of
  cells enabled, and the tickets opened. `build-test.md` counts.

- [ ] **Step 4: Commit.**
```bash
git add testdata/cost-registry.csv peacockdb-core/tests/common/corpus_cases.inc llm-wiki
git commit -m "#183 closed: the string class is gone from the sink; N cells enabled"
```

### Task 5: The record

- [ ] Detail file: every deletion site, the golden file count, the harness run's `test result:`
  lines, the rollout table (query, verdict, ticket). `build-test.md`: the `bug_` table loses the
  pin; the grand total moves accordingly.
- [ ] `git commit -m "utf8-everywhere: the record"`.
