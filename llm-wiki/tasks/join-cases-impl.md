# Join cases implementation plan

**Goal:** Every row of `join-cases.md`'s matrix a case — projection on every reachable hash-join
type, composite and non-`Int32` keys on the three join code paths, residual combinations, the
non-AST nested loop, the `Utf8View` pins — each green or a `bug_` test naming a ticket.

**Architecture:** Cases only, in the harness task 9 left: a hand-built node over `Given` leaves,
a `Script`, `run_both`, `.same(order)`. Two builders are added, both local to their file: a
`keyed` batch whose key column carries the requested type, and a `hash_join_keyed` node over it.
`Utf8View` is a *declaration* on a `Given` leaf over an ordinary `Utf8` batch, never a retyped
batch — cuDF's `from_arrow` cannot upload a `Utf8View` array, and the corpus never does either.
A case that diverges is run once green-form to read what the engine does, then rewritten as
`bug_…` asserting that, with its ticket above it.

**Tech stack:** Rust at the gpu rung; `shad-gpu` through `scripts/build-test-shadgpu.sh`.

**Spec:** [`join-cases.md`](join-cases.md) — frozen. The matrix is the checklist; every task
below cites its row.

## Global constraints

- **Cases only.** Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside
  `src/tests/gpu_tests/`. `synthetic.rs` is untouched. A case that would pass with a production
  change is a ticket and a `bug_` test.
- **No new mechanism** beyond `keyed` and `hash_join_keyed` in `join_cases.rs`. A case that needs
  more is a finding against the harness, recorded in the detail file.
- **Every divergence gets a ticket before it gets a `bug_` test** — an existing one where the
  defect is the same, a new one in `llm-wiki/tickets.md` (fifteen lines at most) otherwise. A
  `bug_` test asserts the wrong behaviour precisely: the wrong slot through `assert_same` against
  a hand-written expectation, or the refusal's message through `gpu_refuses()`.
- **No fix, anywhere.** Not in an operator, not in the harness by casting a divergence away.
- **Every `Utf8View` case but the pins projects the column out of its output**, so the join is
  what the comparison reads.
- **Every case is named for its shape**; no loop over key types or join types.
- **The join scripts follow the capability matrix**: build first and one batch, probe streamed.
- The kind guard (`coverage.rs`) does not move: no kind gains or loses a case, only rows.
- Commit messages at most 10 lines; `rustfmt` on the files you touched.

## The device cycle

Every task ends with one. Build, ship, patch, run the family, read every line:

```bash
scripts/build-test-shadgpu.sh --build && scripts/build-test-shadgpu.sh --push-binaries --patch
PCK_RUN_CPP=0 PCK_TEST_FILTER='tests::gpu_tests::join_cases' scripts/build-test-shadgpu.sh --run
```

Substitute the family. Run it in the foreground: a backgrounded chain is killed mid-build with
no error. A refusal's message is in the run log under the test's name.

## File structure

| file | responsibility |
|---|---|
| `src/tests/gpu_tests/join_cases.rs` | `GpuHashJoin`: projection rows, `keyed`, `hash_join_keyed`, the key-type rows, the residual rows, their empties |
| `src/tests/gpu_tests/nested_cases.rs` | `GpuNestedLoopJoin`: the non-AST predicate, its empties |
| `src/tests/gpu_tests/exec_cases.rs`, `accumulate_cases.rs`, `emit_cases.rs` | one #183 pin each |
| `src/tests/gpu_tests/harness_cases.rs` | already carries the unload pin and `declaring_view_strings`; the latter becomes `pub(super)` |
| `llm-wiki/tickets.md`, `llm-wiki/build-test.md`, `join-cases-detail.md` | the record |

What `join_cases.rs` already has, used throughout (read it first): `build_batch(rows)` =
`prefixed(&synthetic(rows, 11), "b_")`; `probe_batch(rows, seed)`; `side(prefix)` — the
prefixed field list; `output_of(join_type)` — the pre-projection output fields per type;
`hash_join(join_type, null_equals_null, filtered, projection)` with keys `vec![(1, 1)]` — column 1
is `key: Int32` on both sides; `join(t)` = `hash_join(t, false, false, None)`; `script(build,
probe)`, `one_probe()`, `two_probes()`, `empty_build()`, `empty_probe()`; `gpu_refuses_with`,
`both_refuse_with`; the message constants `BUILD_COPY`, `PROBE_COPY`, `NO_BUILD`. `synthetic`'s
ordinals: `0 id Int64`, `1 key Int32`, `2 i32`, `3 i64`, `4 f64`, `5 s Utf8`, `6 d Date32`,
`7 b Boolean`; prefixed sides put the build's eight first, the probe's eight after, so a
projection ordinal `n ≥ 8` is the probe's `n − 8`.

---

### Task 1: Projection on every reachable type

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`

Matrix row 1. `Inner` with a projection exists (`an_inner_join_with_a_projection…`, find it);
this task adds `Right`, `RightSemi`, `RightAnti`, `LeftSemi`, `LeftAnti`, `LeftMark`. `Left` and
`Full` are not rows: #152 refuses their first probe batch, pinned already.

- [ ] **Step 1: One projection per type, chosen to reorder, drop the keys, and cross sides**

A projection is a list of ordinals into `output_of(join_type)`. Write it so it takes columns
from both sides where the type keeps both, reverses their order, and omits column 1 (the key)
on every side it keeps:

```rust
/// A projection over `output_of(join_type)` that reorders, drops every key, and takes
/// from both sides where the type keeps both: what the corpus does 634 times.
fn crossing_projection(join_type: JoinType) -> Vec<u32> {
    match join_type {
        // both sides: probe's s, build's f64, probe's id, build's d
        JoinType::Inner | JoinType::Right => vec![13, 4, 8, 6],
        // build side only: d, f64, id
        JoinType::LeftSemi | JoinType::LeftAnti => vec![6, 4, 0],
        // build side and the mark: mark, s, id
        JoinType::LeftMark => vec![8, 5, 0],
        // probe side only: b, i64, id
        JoinType::RightSemi | JoinType::RightAnti => vec![7, 3, 0],
        JoinType::Left | JoinType::Full => unreachable!("#152 refuses the first probe batch"),
    }
}
```

- [ ] **Step 2: The six cases, green-form**

```rust
operator_case! {
    GpuHashJoin,
    fn a_right_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        run_both(&node, two_probes()).same(Order::Any);
    }
}
```

The same shape for `RightSemi`, `RightAnti`, `LeftSemi`, `LeftAnti`, `LeftMark`, each named
`a_<type>_join_with_a_crossing_projection_agrees`, over `two_probes()` — two probe batches so
the per-batch join and the finish both run under the projection. `Right` over one probe batch
as well (`…over_one_probe_batch…`), since #152 refuses its second and the second case will pin
that under `BUILD_COPY` rather than prove the projection:

```rust
operator_case! {
    GpuHashJoin,
    fn a_right_join_over_one_probe_batch_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        run_both(&node, one_probe()).same(Order::Any);
    }
}
```

- [ ] **Step 3: The two empties with a projection**

```rust
// Row 1's empty shape: a zero-row build under a projection, on the swapping type and on the
// finishing type. Right owes every probe row padded; LeftAnti owes nothing.
operator_case! {
    GpuHashJoin,
    fn right_over_a_zero_row_build_with_a_projection_pads_every_probe_row() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        run_both(&node, empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_over_a_zero_row_build_with_a_projection_answers_nothing() {
        let node = hash_join(JoinType::LeftAnti, false, false, Some(crossing_projection(JoinType::LeftAnti)));
        run_both(&node, empty_build()).same(Order::Any);
    }
}
```

- [ ] **Step 4: Run the family on the device, read every failure**

Run the device cycle with `PCK_TEST_FILTER='tests::gpu_tests::join_cases'`. Expected: the
`Right` two-probe case refuses the second batch with `BUILD_COPY` (#152, known); every other
new case green or a schema/row difference that is a *new* finding — a wrong ordinal after the
side swap, a missing column in the `Narrow` finish project.

- [ ] **Step 5: Ticket and pin what diverged**

For the `Right` two-probe refusal: rename to
`bug_a_right_join_with_a_crossing_projection_refuses_its_second_probe_batch_on_the_device`,
`// #152` above it, body `gpu_refuses_with(&run_both(&node, two_probes()), BUILD_COPY)`. For
anything else: a ticket in `tickets.md` (Critical correctness, next free number — check the
highest on this branch *and* on `ENS-declared-schemas`, which holds #209; skip past both), then
the `bug_` form asserting the wrong slot with `assert_same` against a hand-written batch, the
ticket number in the comment above. Run again; commit.

```bash
git add peacockdb-core/src/tests/gpu_tests/join_cases.rs llm-wiki/tickets.md
git commit -m "join cases: a crossing projection on every reachable type"
```

---

### Task 2: `keyed` and `hash_join_keyed`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` (`declaring_view_strings` to `pub(super)`)

**Interfaces:**
- Produces: `enum Key { Composite, Int64, Utf8ViewDeclared, Date32 }`; `keyed(rows, seed, key) ->
  RecordBatch` — `synthetic` with column 1 retyped for the key (and column 2 kept `Int32` as the
  second key of `Composite`); `hash_join_keyed(join_type, key, projection) -> GpuHashJoin` whose
  leaves declare the keyed side schemas and whose `keys` are `[(1, 1)]`, or `[(1, 1), (2, 2)]`
  for `Composite`.

- [ ] **Step 1: The key enum and the keyed batch**

```rust
use datafusion::arrow::compute::cast;

/// The key types the corpus joins on that task 9 did not: a composite key, the `Int64` on
/// nearly every join, a string, a date. `Utf8ViewDeclared` retypes nothing in the batch —
/// the leaf *declares* `Utf8View` over `Utf8` data, the corpus's own situation, since
/// cuDF's `from_arrow` cannot upload a `Utf8View` array.
#[derive(Clone, Copy)]
enum Key {
    Composite,
    Int64,
    Utf8ViewDeclared,
    Date32,
}

impl Key {
    /// What column 1 is cast to in the batch. `Utf8` for the declared view: the data.
    fn data_type(self) -> DataType {
        match self {
            Key::Composite => DataType::Int32,
            Key::Int64 => DataType::Int64,
            Key::Utf8ViewDeclared => DataType::Utf8,
            Key::Date32 => DataType::Date32,
        }
    }

    /// What the leaf declares column 1 as.
    fn declared_type(self) -> DataType {
        match self {
            Key::Utf8ViewDeclared => DataType::Utf8View,
            other => other.data_type(),
        }
    }

    fn pairs(self) -> Vec<(u32, u32)> {
        match self {
            Key::Composite => vec![(1, 1), (2, 2)],
            _ => vec![(1, 1)],
        }
    }
}

/// `synthetic(rows, seed)` with its `key` column cast to the key's data type, under a field
/// of that type. `key` has seven values and nulls, so every side has matches, misses and a
/// null to leave out.
fn keyed(rows: usize, seed: u64, key: Key) -> RecordBatch {
    let batch = synthetic(rows, seed);
    let mut columns = batch.columns().to_vec();
    columns[1] = cast(&columns[1], &key.data_type()).expect("Int32 casts to every key type");
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 1 {
                Field::new("key", key.data_type(), true)
            } else {
                f.as_ref().clone()
            }
        })
        .collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), columns)
        .expect("the same columns under the keyed schema")
}
```

- [ ] **Step 2: The keyed join node**

`side(prefix)` reads `synthetic(0, 0)`'s fields; the keyed side declares column 1 as the key's
*declared* type:

```rust
/// `side(prefix)` with column 1 declared as the key's declared type.
fn keyed_side(prefix: &str, key: Key) -> Vec<Field> {
    side(prefix)
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 1 {
                Field::new(f.name(), key.declared_type(), true)
            } else {
                f
            }
        })
        .collect()
}

/// `hash_join` over keyed leaves. Same output rules as `output_of`, over the keyed fields.
fn hash_join_keyed(join_type: JoinType, key: Key, projection: Option<Vec<u32>>) -> GpuHashJoin {
    let b = keyed_side("b_", key);
    let p = keyed_side("p_", key);
    let fields: Vec<Field> = match join_type {
        JoinType::Inner => [b.clone(), p.clone()].concat(),
        JoinType::Right => [padded(b.clone()), p.clone()].concat(),
        JoinType::LeftAnti => b.clone(),
        other => unreachable!("{other:?} is not one of the three key-path types"),
    };
    let fields = match &projection {
        None => fields,
        Some(keep) => keep.iter().map(|i| fields[*i as usize].clone()).collect(),
    };
    let leaf = |fields: Vec<Field>, batches: BatchLayout| -> Box<dyn GpuNode> {
        Given::of(Schema::new(Arc::new(ArrowSchema::new(fields))), batches)
    };
    GpuHashJoin::new(
        leaf(b, BatchLayout::SingleBatch),
        leaf(p, BatchLayout::MultipleBatches),
        join_type,
        key.pairs(),
        None,
        Vec::new(),
        false,
        projection,
        Schema::new(Arc::new(ArrowSchema::new(fields))),
    )
}

fn keyed_script(key: Key) -> Script {
    script(
        Some(prefixed(&keyed(32, 11, key), "b_")),
        vec![prefixed(&keyed(48, 3, key), "p_")],
    )
}
```

- [ ] **Step 3: Make `declaring_view_strings` reachable**

In `harness_cases.rs`, change `fn declaring_view_strings` to `pub(super) fn
declaring_view_strings`. Nothing else moves; the spec names it as the shape to reuse.

- [ ] **Step 4: Compile-check without a device**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
```

Expected: builds, 0 warnings (an unused `Key` variant warns until Task 3 uses it — that is
fine for this step only; Task 3 commits with it used). Commit together with Task 3.

---

### Task 3: Key type × three code paths

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`

Matrix row 2: four keys × `Inner` (per-batch join), `Right` (cuDF swaps the sides, the indices
are swapped back), `LeftAnti` (the accumulated-keys finish). The `Utf8View`-declared cases
project the key away except one per path, which pins #183 at the join.

- [ ] **Step 1: The projection that drops every key**

```rust
/// Every column but the keys, both sides where kept: what a Utf8View-keyed case must
/// project to, so the join is what the comparison reads and not the export (#183).
fn without_keys(join_type: JoinType) -> Vec<u32> {
    let one_side = |base: u32| (0..8u32).filter(|i| *i != 1 && *i != 2).map(move |i| base + i);
    match join_type {
        JoinType::Inner | JoinType::Right => one_side(0).chain(one_side(8)).collect(),
        JoinType::LeftAnti => one_side(0).collect(),
        other => unreachable!("{other:?}"),
    }
}
```

- [ ] **Step 2: Twelve cases, named for key and path**

Composite, `Int64` and `Date32` compare whole outputs; `Utf8View` projects the keys away:

```rust
operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_composite_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Inner, Key::Composite, None), keyed_script(Key::Composite))
            .same(Order::Any);
    }
}
```

Repeat for `(Inner, Int64)`, `(Inner, Date32)`, `(Right, Composite)`, `(Right, Int64)`,
`(Right, Date32)`, `(LeftAnti, Composite)`, `(LeftAnti, Int64)`, `(LeftAnti, Date32)`, each
`a_<type>_join_on_a_<key>_key_agrees`. For `Utf8ViewDeclared`:

```rust
operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_declared_utf8view_key_agrees_past_the_key() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8ViewDeclared, Some(without_keys(JoinType::Inner)));
        run_both(&node, keyed_script(Key::Utf8ViewDeclared)).same(Order::Any);
    }
}
```

and the same for `Right` and `LeftAnti`.

- [ ] **Step 3: The three #183 pins at the join, one per path**

```rust
// #183 — the join keeps its declared-Utf8View key in the output, so the export refuses at
// the sink and the join is unobservable past it; the projected case above is the join's.
operator_case! {
    GpuHashJoin,
    fn bug_an_inner_join_keeping_a_declared_utf8view_key_is_refused_at_the_export() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8ViewDeclared, None);
        let why = run_both(&node, keyed_script(Key::Utf8ViewDeclared)).gpu_refuses();
        assert!(why.contains("declared vs exported"), "{why}");
        assert!(why.contains("b_key: Utf8View vs Utf8"), "{why}");
    }
}
```

Same for `Right` (`b_key` and `p_key` both, `Right` keeps both sides) and `LeftAnti` (`b_key`).
Read the exact clause in the run log first — the message is `{index} {name}: {declared} vs
{exported}` per column; assert the index too once seen.

- [ ] **Step 4: The empty shape**

```rust
// Row 2's empty shape: a declared-Utf8View key over zero rows crosses the boundary once.
operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_declared_utf8view_key_over_a_zero_row_probe_answers_zero_rows() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8ViewDeclared, Some(without_keys(JoinType::Inner)));
        let probe = prefixed(&keyed(0, 3, Key::Utf8ViewDeclared), "p_");
        run_both(&node, script(Some(prefixed(&keyed(32, 11, Key::Utf8ViewDeclared), "b_")), vec![probe]))
            .same(Order::Any);
    }
}
```

- [ ] **Step 5: Device cycle, tickets, pins, commit**

Expected known outcomes: `Right` cases refuse nothing over one probe batch (the script has
one). A key-type mismatch the device refuses (a `Date32` key, say) is a new ticket and a
`bug_` by message; a wrong row set on a composite key is a new ticket and a `bug_` by slot.
#45 (a string key cast) may name the string-key refusal if the message matches its text —
read the ticket before filing a new one.

```bash
git add peacockdb-core/src/tests/gpu_tests/join_cases.rs peacockdb-core/src/tests/gpu_tests/harness_cases.rs llm-wiki/tickets.md
git commit -m "join cases: composite, Int64, Date32 and declared-Utf8View keys on the three paths"
```

---

### Task 4: Residual combinations on `Inner`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`

Matrix row 3. `residual()` is `b_i64 < p_i64` over `filter_columns` `[build 3, probe 3]`.
Two more residuals are needed, on a string and on a decimal column; write them beside it:

- [ ] **Step 1: The string and decimal residuals**

```rust
/// `b_s = p_s` — a string comparison the AST cannot do, so the column path evaluates it.
fn string_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (
        Some(Expr::binary(Expr::column(0, "b_s"), BinaryOp::Eq, Expr::column(1, "p_s"), DataType::Boolean)),
        vec![
            JoinFilterColumn { side: JoinSide::Build, index: 5 },
            JoinFilterColumn { side: JoinSide::Probe, index: 5 },
        ],
    )
}

/// `CAST(b_i64 AS DECIMAL(20, 0)) < CAST(p_i64 AS DECIMAL(20, 0))` — a decimal operand,
/// which `is_ast_able` refuses, so the column path evaluates it.
fn decimal_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    let dec = |i: u32, name: &str| Expr::Cast {
        expr: Box::new(Expr::column(i, name)),
        target: DataType::Decimal128(20, 0),
    };
    (
        Some(Expr::binary(dec(0, "b_i64"), BinaryOp::Lt, dec(1, "p_i64"), DataType::Boolean)),
        vec![
            JoinFilterColumn { side: JoinSide::Build, index: 3 },
            JoinFilterColumn { side: JoinSide::Probe, index: 3 },
        ],
    )
}
```

`hash_join` takes `filtered: bool`; add a sibling that takes the residual itself:

```rust
fn hash_join_with(
    join_type: JoinType,
    null_equals_null: bool,
    residual: (Option<Expr>, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
) -> GpuHashJoin {
    // the body of `hash_join` with `residual` in place of the `filtered` match
}
```

Write it by copying `hash_join`'s body; then make `hash_join` call it with `residual()` or
`(None, Vec::new())`, so there is one body.

- [ ] **Step 2: Five cases**

```rust
operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_and_a_crossing_projection_agrees() {
        let node = hash_join_with(JoinType::Inner, false, residual(), Some(crossing_projection(JoinType::Inner)));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_under_null_equals_null_agrees() {
        run_both(&hash_join_with(JoinType::Inner, true, residual(), None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_over_two_probe_batches_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, residual(), None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_string_residual_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, string_residual(), None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_decimal_residual_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, decimal_residual(), None), one_probe()).same(Order::Any);
    }
}
```

- [ ] **Step 3: Device cycle, tickets, pins, commit**

Expected: the two-probe case refuses the second batch (#152, `BUILD_COPY`) — pin it as
`bug_an_inner_join_with_a_residual_refuses_its_second_probe_batch_on_the_device`. The string
and decimal residuals are new ground: a refusal names a new ticket; a row-set difference is a
new ticket and a slot pin.

```bash
git add peacockdb-core/src/tests/gpu_tests/join_cases.rs llm-wiki/tickets.md
git commit -m "join cases: residuals with a projection, under null_equals_null, over strings and decimals"
```

---

### Task 5: The non-AST nested loop

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/nested_cases.rs`
- Modify: `llm-wiki/tickets.md` (the `Left` refusal's ticket, before its `bug_`)

Matrix row 4. `nested(join_type, projection)` builds over `residual()` = `b_i64 < p_i64`, which
is AST-able. The cross-then-mask path (`join.cpp`, the `filter_columns`-ordered mask) needs a
predicate `is_ast_able` refuses; a decimal operand is the cheapest.

- [ ] **Step 1: A nested-loop builder that takes its predicate**

```rust
/// `CAST(b_i64 AS DECIMAL(20, 0)) > CAST(p_i64 AS DECIMAL(20, 0))`: a decimal operand is
/// what `is_ast_able` refuses, so this predicate takes the cross-then-mask path.
fn decimal_residual() -> (Expr, Vec<JoinFilterColumn>) {
    let dec = |i: u32, name: &str| Expr::Cast {
        expr: Box::new(Expr::column(i, name)),
        target: DataType::Decimal128(20, 0),
    };
    (
        Expr::binary(dec(0, "b_i64"), BinaryOp::Gt, dec(1, "p_i64"), DataType::Boolean),
        vec![
            JoinFilterColumn { side: JoinSide::Build, index: 3 },
            JoinFilterColumn { side: JoinSide::Probe, index: 3 },
        ],
    )
}

fn nested_with(
    join_type: NestedLoopJoinType,
    predicate: (Expr, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
) -> GpuNestedLoopJoin {
    // `nested`'s body with `predicate` in place of `residual()`
}
```

As in Task 4: copy `nested`'s body into `nested_with`, then make `nested` call it with
`residual()`.

- [ ] **Step 2: Four cases and two empties**

```rust
operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_agrees() {
        run_both(&nested_with(NestedLoopJoinType::Inner, decimal_residual(), None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_agrees() {
        let node = nested_with(NestedLoopJoinType::Inner, decimal_residual(), Some(vec![13, 4, 8]));
        run_both(&node, one_probe()).same(Order::Any);
    }
}
```

`Left` with and without a projection likewise (`a_left_nested_loop_join_with_a_decimal_predicate…`).
The empties, `Inner` only — `Left` will be a refusal whatever the input:

```rust
operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_build_answers_nothing() {
        run_both(&nested_with(NestedLoopJoinType::Inner, decimal_residual(), None), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_probe_answers_nothing() {
        run_both(&nested_with(NestedLoopJoinType::Inner, decimal_residual(), None), empty_probe()).same(Order::Any);
    }
}
```

- [ ] **Step 3: Device cycle; the `Left` ticket**

Expected: both `Left` cases refuse on the device with
`non-AST-able NestedLoopJoin filter is only supported for Inner joins` (`join.cpp:459`), while
the cpu answers. That has no ticket. File one in `tickets.md` under Critical correctness —
title along the lines of *a left nested-loop join with a predicate the AST cannot take is
refused on the device*, body: the planner admits it (`plan/join.rs` checks the batch layout
only), the C++ throws at the mask step, so a query with a decimal or string predicate on a
LEFT JOIN with no equi-key answers on the cpu and refuses on the device; pinned by the two
tests. Then rewrite both `Left` cases:

```rust
// #NNN — the planner admits a Left nested loop over any predicate; the device's
// cross-then-mask path is written for Inner alone and throws.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_left_nested_loop_join_with_a_decimal_predicate_is_refused_on_the_device() {
        let outcome = run_both(&nested_with(NestedLoopJoinType::Left, decimal_residual(), None), one_probe());
        gpu_refuses_with(&outcome, "non-AST-able NestedLoopJoin filter is only supported for Inner joins");
    }
}
```

The `Inner` cases: green, or a wrong row set is a new ticket (#190 is the cpu dropping a
projection; if the projected `Inner` case fails on the cpu with the projection dropped, that is
#190 and its existing pin's shape — `cpu_projected` in this file).

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/nested_cases.rs llm-wiki/tickets.md
git commit -m "nested-loop cases: the cross-then-mask path, Inner green and Left refused (#NNN)"
```

---

### Task 6: The `Utf8View` pins through the operators

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`, `accumulate_cases.rs`, `emit_cases.rs`

Matrix row 5. One per family that passes the column through unchanged — project (a column
copy), filter (every row passes), coalesce-all, emit on a string key. The unload's exists in
`harness_cases.rs`. Each declares `s: Utf8View` on its leaf via
`super::harness_cases::declaring_view_strings(&input().schema())` and asserts the export's
refusal names column 5.

- [ ] **Step 1: The project pin** (`exec_cases.rs`)

`project(exprs)` builds over `given()`; write the leaf by hand here:

```rust
// #183 — a column copy under a schema declaring Utf8View: the device exports Utf8 and the
// sink refuses, naming the column. The deliberate pin for the project family.
operator_case! {
    GpuProject,
    fn bug_a_projected_column_declared_utf8view_is_refused_at_the_export() {
        let declared = super::harness_cases::declaring_view_strings(&input().schema());
        let node = GpuProject::new(
            Given::of(declared.clone(), BatchLayout::MultipleBatches),
            vec![NamedExpr::new(Expr::column(5, "s"), "s")],
            columns(&[("s", DataType::Utf8View)]),
        );
        let why = run_both(&node, Script::Exec(vec![input()])).gpu_refuses();
        assert!(why.contains("(declared vs exported: 0 s: Utf8View vs Utf8)"), "{why}");
    }
}
```

- [ ] **Step 2: The filter pin** (`exec_cases.rs`)

`GpuFilter::new(leaf, predicate, projection, schema)`: leaf declaring the view, predicate
`gt(2, "i32", lit_i32(i32::MIN))` so every row passes, no projection, schema the declared one.
Assert `5 s: Utf8View vs Utf8`.

- [ ] **Step 3: The coalesce pin** (`accumulate_cases.rs`)

`GpuCoalesceAllBatches::new(Given::with_layout(declared, PartitionLayout::new(1)))` over
`Script::Accumulate(vec![synthetic(16, 1), synthetic(16, 2)])`; the export at done refuses.
Assert `5 s: Utf8View vs Utf8`.

- [ ] **Step 4: The emit pin** (`emit_cases.rs`)

`GpuEmitPartitions::new(Given::of(declared, MultipleBatches), vec![5], 4)` — the string
column as the key — over `Script::Emit(vec![input()])` (read the file's `emit` cases for the
script variant's exact spelling). Expected on the device: either the export's `5 s: Utf8View
vs Utf8` or the kernel's key-type refusal (`refused_key_type`, cuDF `type_id` for a string)
first — read the log, pin whichever fires, and say in the comment which one wins.

- [ ] **Step 5: Device cycle over all four families, commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/{exec,accumulate,emit}_cases.rs
git commit -m "the #183 pins: project, filter, coalesce-all and emit under a Utf8View declaration"
```

---

### Task 7: The record

**Files:**
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tasks/join-cases-detail.md`

- [ ] **Step 1: Counts and rows**

The `Operator harness` entry's count (`grep -n 'Operator harness |' llm-wiki/build-test.md`)
grows by the number of cases added; the gpu rung's `--lib -- gpu_tests::` figure and the grand
total move by the same number. Every new `bug_` test is a line in the detail file's table:
name, what it asserts, ticket — the known-wrong table in `build-test.md` arrives with
`declared-schemas`; until that merges the detail file is the register.

- [ ] **Step 2: The full family run**

```bash
PCK_RUN_CPP=0 PCK_TEST_FILTER='_cases' scripts/build-test-shadgpu.sh --run
```

Expected: every case module green, the kind guard green, counts as recorded. Paste the
`test result:` lines into the detail file.

- [ ] **Step 3: Commit**

```bash
git add llm-wiki/build-test.md llm-wiki/tasks/join-cases-detail.md
git commit -m "join cases: the record — counts, the bug_ register, the device run"
```
