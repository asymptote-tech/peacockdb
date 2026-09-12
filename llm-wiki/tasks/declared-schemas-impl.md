# Declared schemas implementation plan

**Goal:** Declare what each of six recipe calls produces, render the declarations into a new section
of `recipe-payloads.txt`, measure them on a device through the export that already exists, and
record every disagreement as a named `bug_` test — fixing nothing.

**Architecture:** `Call` gains an `Option<Schema>` filled only where the declaration already exists.
A new section of the payload golden renders it Rust-side, before serialization, so the wire never
changes. The measurement uses the export that already exists — **no C++ change and no new ABI
symbol** — which means precision and nullability are named as limitations rather than measured.
Each query gets its own named device test.

**Tech stack:** Rust, C++17/cuDF 25.02, flatbuffers. Device tests on `shad-gpu`.

**Spec:** [`declared-schemas.md`](declared-schemas.md) — frozen. Read it in full before Task 1; the
"Why this shape, and where it measures" section is the part that decides whether a later finding is
a bug or an artefact of the instrument.

## Global constraints

- **This task fixes nothing.** No cast, no refusal, no correction to a declaration, no change to the
  production exporter. A one-line fix is a ticket.
- Everything is in-crate: `Call::output_schema` is `pub(crate)`, the renderer is `pub(crate)` in
  `plan_text`, the tests are `#[cfg(all(test, feature = "gpu"))]` under `wire/gpu_tests/`. **No new
  `pub` anywhere** — after `visibility.md` there are none in `wire` or `plan_text` to join.
- `test_support` is untouched. Its rule forbids a `pub` naming a type from `wire` or `plan_text`, and
  the harness signature is nothing else.
- **One `bug_` test per (divergence class, node kind)**, never per node instance, never a shared
  table of expected divergences.
- A test name is a claim (`coding-style.md`, Names). `bug_` is a prefix on a claim about wrong
  behaviour, with its ticket in a comment above it.
- Commit messages at most 10 lines including the subject.

## Ordering constraint

This task lands **at the end of the ENS-drop-mode-name chain**, after `module-layout`, `test-layout`,
`test-support` and `visibility`. Every path below is the post-refactor path. Before starting, confirm
the tree matches:

```bash
ls peacockdb-core/src/wire/gpu_tests/ peacockdb-core/src/planner/tests/
git log --oneline -1
```

If `wire/gpu_tests/` does not exist, `test-layout.md` has not landed and this task is not ready. Stop
and report that, rather than creating the directory.

## Device cycle

```bash
./scripts/build-test-shadgpu.sh --build --push-binaries --patch --run
```

The device rung is selected by path filter on the staged `--lib --features gpu` binary:
`-- --test-threads=1 gpu_tests::`. Golden regeneration is CPU-side and needs **both**
`UPDATE_CANONICAL=1` and `PEACOCK_REWRITE_RECIPE_BYTES=1`.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/mod.rs` | `NodeKind::Exporter { schema }`, the rename and the accessor |
| `peacockdb-core/src/plan/validate.rs` | three `matches!` patterns follow the rename |
| `peacockdb-core/src/wire/mod.rs` | `Call::output_schema`, and the constructors that set it |
| `peacockdb-core/src/wire/attach.rs` | the six arms declare |
| `peacockdb-core/src/plan_text/mod.rs` | one `pub(crate) fn` rendering section B |
| `peacockdb-core/src/planner/tests/…` | the golden test gains section B |
| `peacockdb-core/src/wire/gpu_tests/schema_conformance.rs` | one named test per query |

---

### Task 1: The boundary node declares

`GpuUnload` is the one arm with no declaration, because a sink carries no schema — and it is the
boundary where every divergence has surfaced. Fix that first; the rest of the task depends on it.

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs:249,257,264,970`
- Modify: `peacockdb-core/src/plan/validate.rs:25,48,301`
- Modify: `peacockdb-core/src/executor/tests.rs:135`
- Modify: `peacockdb-core/tests/common/rebuild.rs:573` (doc line)

**Interfaces:**
- Produces: `NodeKind::Exporter { schema: Schema }`. `kind().schema()` returns `Some` for an unload
  from here on; `kind().layout()` still returns `None`. Tasks 3 and 5 rely on both.

- [ ] **Step 1: Find every reference before changing any**

```bash
grep -rn 'NodeKind::Sink\|Self::Sink' peacockdb-core/src peacockdb-core/tests --include=*.rs
```

Expected: eight hits in four files. If there are more, the tree has moved since the spec was written
— list them in `declared-schemas-detail.md` before continuing.

- [ ] **Step 2: Change the variant and the two accessors**

```rust
pub enum NodeKind {
    Source { layout: PartitionLayout, schema: Schema },
    Intermediate { layout: PartitionLayout, schema: Schema },
    /// The boundary: rows leave the device here. It declares the columns that cross, and
    /// has no partition layout because nothing downstream is partitioned.
    Exporter { schema: Schema },
}
```

```rust
    pub fn layout(&self) -> Option<&PartitionLayout> {
        match self {
            Self::Source { layout, .. } | Self::Intermediate { layout, .. } => Some(layout),
            Self::Exporter { .. } => None,
        }
    }

    pub fn schema(&self) -> Option<&Schema> {
        match self {
            Self::Source { schema, .. }
            | Self::Intermediate { schema, .. }
            | Self::Exporter { schema } => Some(schema),
        }
    }
}
```

`schema()` keeps its `Option` even though every arm now answers `Some`. Narrowing it to `&Schema` is
a separate change touching every caller, and it is not this task's.

- [ ] **Step 3: Give `GpuUnload::new` its schema**

```rust
impl GpuUnload {
    pub fn new(input: Box<dyn GpuNode>, interval: Option<RowInterval>) -> Self {
        let schema = input
            .kind()
            .schema()
            .expect("an unload's input declares the columns that cross")
            .clone();
        Self { kind: NodeKind::Exporter { schema }, interval, input }
    }
}
```

- [ ] **Step 4: Follow the patterns, and reword the messages that lie**

`validate.rs:25`, `:48`, `:301` and `executor/tests.rs:135` become `NodeKind::Exporter { .. }`. Then:

```bash
grep -rn 'is not a sink\|cannot be a sink\|not a sink' peacockdb-core/src --include=*.rs
```

Eight `.expect("… is not a sink")` messages stay *true* — they are about a child, which is never the
exporter — but they read as if `schema()` could still be `None` for one. Reword the ones in files
this task already touches; leave the rest, and say so in the report.

- [ ] **Step 5: Regenerate the plan goldens and review the diff**

```bash
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core planner::tests
git diff --stat testdata/goldens/
```

Expected: ten `.plans.txt` files, each `GpuUnload` line gaining `schema=[…]` from
`plan_text/node_text.rs:30`, which renders a schema wherever `kind().schema()` is `Some`.

**Read one file's diff in full.** The schema on each unload must equal its child's — if any differs,
`GpuUnload::new` is not getting the schema you think it is.

- [ ] **Step 6: Run the rust-only suite and commit**

```bash
cargo test --features rust-only -p peacockdb-core
git add peacockdb-core/src peacockdb-core/tests testdata/goldens
git commit -m "the boundary node declares, like every other

NodeKind::Sink carried neither layout nor schema, which made the export the
one call with nothing to compare against — at the boundary where every
divergence has surfaced. It is Exporter { schema } now, taken from its
input at construction. Ten .plans.txt gain a schema on the GpuUnload line."
```

---

### Task 2: A schema on each call, declared nowhere yet

The field first, empty, so that Task 3's six arms are a diff about declarations rather than about
plumbing.

**Files:**
- Modify: `peacockdb-core/src/wire/mod.rs:216-250`

**Interfaces:**
- Produces: `Call { symbol, target, inputs, when, output_schema: Option<Schema> }`;
  `Call::seq(seq, kind, inputs, when)` and `Call::bare(symbol, inputs, when)` unchanged in signature,
  both setting `output_schema: None`; a builder-style `Call::declaring(self, schema: &Schema) -> Self`.

- [ ] **Step 1: Add the field and the setter**

```rust
pub(crate) struct Call {
    pub(crate) symbol: AbiSymbol,
    /// `None` for the two symbols whose arguments are runtime row counts.
    pub(crate) target: Option<(Seq, FbKind)>,
    pub(crate) inputs: Vec<Input>,
    pub(crate) when: CallPattern,
    /// The schema of the table one firing of this call produces. `None` means no arm has
    /// declared it yet — the join and aggregate families, which `declared-schemas-derived.md`
    /// takes. It never means "this call produces nothing"; a call that produces no table
    /// declares an empty schema and says so.
    pub(crate) output_schema: Option<Schema>,
}
```

```rust
    /// The schema this call's firing produces. Separate from the constructors so that an
    /// arm which has not worked out its declaration reads as undeclared rather than as
    /// having passed something.
    pub(crate) fn declaring(mut self, schema: &Schema) -> Self {
        self.output_schema = Some(schema.clone());
        self
    }
```

- [ ] **Step 2: Deal with `Eq`**

`Call` derives `Eq`, and `plan::Schema` is `PartialEq` only. Build and find out:

```bash
cargo build --features rust-only -p peacockdb-core 2>&1 | grep -A5 'Eq'
```

If it breaks, drop `Eq` from `Call`'s derive and from `Recipe`'s if it propagates, then:

```bash
cargo build --features rust-only -p peacockdb-core 2>&1 | tail -30
```

**If anything actually needed `Eq` on a `Call`, stop.** Carry a key instead of the schema and record
the decision in `declared-schemas-detail.md` — a `HashSet<Call>` somewhere would be a design this
plan has not accounted for.

- [ ] **Step 3: Build green, commit**

```bash
cargo test --features rust-only -p peacockdb-core
git add peacockdb-core/src/wire/mod.rs
git commit -m "a call carries the schema its firing produces

The field only; no arm declares yet. None means undeclared, not empty --
a call producing no table declares an empty schema and says so."
```

---

### Task 3: The six arms declare

**Files:**
- Modify: `peacockdb-core/src/wire/attach.rs:100,115,129,145,228,355`

**Interfaces:**
- Consumes: `Call::declaring` from Task 2, `kind().schema()` from Task 1.

- [ ] **Step 1: Declare each, and unprefix the inputs that become used**

Four of the six ignore their arguments today. `filter`, `project` and `coalesce_all_batches` take
`_inputs`; `unload` takes `_node` and `_inputs`. Each becomes used.

```rust
fn scan(load: &GpuLoadParquet, node: &dyn GpuNode, writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    let output = node.kind().schema().expect("a source declares its columns");
    let seq = writer.node(0, |b, _| node_writer::scan(b, load, output))?;
    Ok(Some(Recipe::of(vec![
        Call::seq(seq, FbKind::Scan, vec![Input::RowGroups], CallPattern::PerBatch)
            .declaring(output),
    ])))
}

fn filter(node: &GpuFilter, _inputs: &[&Schema], writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    // NOT inputs[0]. GpuFilter carries a `projection` -- DataFusion's filter projects as
    // well as filtering -- so a filter's output is its own schema and can be narrower
    // than its input. The field's own doc says dropping it "would leave this node
    // declaring its child's columns while emitting fewer".
    let output = node.kind().schema().expect("a filter is not the exporter");
    let seq = writer.node(1, |b, kids| node_writer::filter(b, node, kids))?;
    Ok(Some(Recipe::of(vec![
        Call::seq(seq, FbKind::Filter, vec![Input::Batch], CallPattern::PerBatch)
            .declaring(output),
    ])))
}

fn project(node: &GpuProject, _inputs: &[&Schema], writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    // A project's output is its own schema, not its input's -- that is what it is for.
    let output = node.kind().schema().expect("a project is not the exporter");
    let seq = writer.node(1, |b, kids| node_writer::project(b, node, kids))?;
    Ok(Some(Recipe::of(vec![
        Call::seq(seq, FbKind::PlainProject, vec![Input::Batch], CallPattern::PerBatch)
            .declaring(output),
    ])))
}

fn sort(node: &GpuSort, inputs: &[&Schema], writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    // A sort reorders rows and keeps columns. `fetch` trims rows, not columns.
    let input = inputs[0];
    let seq = writer.node(1, |b, kids| node_writer::sort(b, node, input, kids))?;
    Ok(Some(Recipe::of(vec![
        Call::seq(seq, FbKind::Sort, vec![Input::Batch], CallPattern::PerBatch)
            .declaring(input),
    ])))
}

fn coalesce_all_batches(_node: &GpuCoalesceAllBatches, inputs: &[&Schema], writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    // A concat of the lane's batches: same columns, more rows.
    let output = inputs[0];
    let seq = writer.node(1, |b, kids| Ok(node_writer::coalesce_partitions(b, kids)))?;
    Ok(Some(Recipe::of(vec![
        Call::seq(seq, FbKind::CoalescePartitions, vec![Input::LaneBatches],
                  CallPattern::AtDone)
            .declaring(output),
    ])))
}

fn unload(node: &GpuUnload, inputs: &[&Schema], _writer: &mut Writer)
    -> Result<Option<Recipe>, PlanError> {
    // The exporter declares what crosses, which Task 1 gave it. Assert it against the
    // input rather than reading only one: two derivations of one fact disagreeing is the
    // first thing this catalog should catch, and it costs nothing to check here.
    let declared = node.kind().schema().expect("an exporter declares its columns");
    debug_assert_eq!(declared, inputs[0], "the exporter's schema is not its input's");
    Ok(Some(Recipe::of(vec![
        Call::bare(AbiSymbol::ResultFromHandle,
                   vec![Input::Batch, Input::RowRange], CallPattern::PerHandle)
            .declaring(declared),
    ])))
}
```

- [ ] **Step 2: Confirm `project` really needs its own schema and not its input's**

```bash
grep -rn 'Intermediate { layout' peacockdb-core/src/planner/translator/ | head
```

A project's `NodeKind::Intermediate` schema is set at translation from the DataFusion operator's
output. If it is not, `project` is declaring the wrong thing and everything downstream inherits it —
check before moving on.

- [ ] **Step 3: Run rust-only, commit**

```bash
cargo test --features rust-only -p peacockdb-core
git add peacockdb-core/src/wire/attach.rs
git commit -m "the six arms declare what their call produces

scan, filter, project, sort, coalesce-all and the exporter, each from a
schema already in hand -- nothing derived, nothing invented, so every
later disagreement is between two facts that both predate this."
```

---

### Task 4: Section B of the payload golden

**Files:**
- Modify: `peacockdb-core/src/plan_text/mod.rs`
- Create: rendering in `peacockdb-core/src/plan_text/node_text.rs` or a sibling
- Modify: `peacockdb-core/src/planner/tests/…` (the file owning `recipe-payloads.txt`)

**Interfaces:**
- Produces: `pub(crate) fn render_declared_schemas(root: &dyn GpuNode, plan: &RecipePlan) -> String`.

- [ ] **Step 1: Write the renderer**

Walk the tree in the same order `render_plan_recipes` does, and for each node print one line per
call. Reuse `node_text`'s `schema_text` and `type_text`.

It lives in `plan_text`, not beside `render_plan_recipes` in `wire/recipes.rs`. Putting it there
would mean promoting `node_text::schema_text` out of its module, and then every call site faces the
choice between the Rust renderer and the wire renderer — which is the trap this section exists to
close. `plan_text` reaching `wire::RecipePlan` is a sibling component reading a `pub(crate)` item,
which the layout rules allow.

```rust
/// The schema each call declares, read off the `Call` data before `Writer` serializes
/// anything. A different source from the payload section, which is read back out of the
/// finished buffer -- which is the point: when a later task puts schemas on the wire, the
/// two sections become each other's check.
///
/// Renders through `schema_text`, which prints `Decimal128(15,2)`. Never
/// `wire::fb_text::schema_text`, which formats the fb enum with `{:?}` and prints a bare
/// `Decimal128`, dropping precision and scale. Same name, two modules, and the wrong one
/// loses the digits with nothing going red.
pub(crate) fn render_declared_schemas(root: &dyn GpuNode, plan: &RecipePlan) -> String {
    let mut text = String::new();
    let mut position = 0usize;
    render_declared_node(root, 0, &mut position, plan, &mut text);
    text
}

/// Post-order, children first, one position per node -- the same numbering
/// `wire::recipes::render_recipe_node` walks. The two must agree, which is what
/// `the_two_payload_sections_number_the_same_nodes` asserts; nothing in the types says so.
fn render_declared_node(
    node: &dyn GpuNode,
    depth: usize,
    position: &mut usize,
    plan: &RecipePlan,
    text: &mut String,
) {
    let mut children = String::new();
    for child in node.children() {
        render_declared_node(child, depth + 1, position, plan, &mut children);
    }
    let at = *position;
    *position += 1;

    let indent = "  ".repeat(depth);
    match plan.get(at) {
        // A node that attaches no recipe makes no call, so it has no schema to declare
        // and that is not a gap. Distinguished from `undeclared` below, which is one.
        None => {
            let _ = writeln!(text, "{indent}{}: no calls", node.name());
        }
        Some(recipe) => {
            let _ = writeln!(text, "{indent}{}:", node.name());
            let inner = format!("{indent}  ");
            for call in &recipe.calls {
                // A bare call has no seq -- the exporter's `result_from_handle` is one --
                // so it is named by its symbol instead. Omitting it would hide the one
                // call at the boundary every divergence has surfaced at.
                let addressed = match call.target {
                    Some((seq, kind)) => format!("#{seq} {kind}"),
                    None => format!("{:?}", call.symbol),
                };
                match &call.output_schema {
                    Some(schema) => {
                        let _ = writeln!(text, "{inner}{addressed}: {}", schema_text(schema));
                    }
                    // Spelled out rather than omitted: an absent line and a call nothing
                    // declared would read the same, which is the invisible-absence shape
                    // `coding-style.md` records twice. It also makes the count of what
                    // this task did not declare greppable.
                    None => {
                        let _ = writeln!(text, "{inner}{addressed}: undeclared");
                    }
                }
            }
        }
    }
    text.push_str(&children);
}
```

Note the ordering of the two renderers' output differs by design: `render_recipe_node` writes its
children *before* its own line into the caller's buffer, and this writes its own line first. Match
whichever reads better in the golden — but match `render_recipe_node` if in doubt, so the two
sections of one query can be read side by side.

- [ ] **Step 1b: Assert the two renderers number the same nodes**

The numbering is duplicated, so it can drift. One test, in `plan_text`'s own tests:

```rust
/// Both payload sections walk the tree post-order and index `RecipePlan` by position.
/// Nothing in the types ties the two walks together, so a node kind whose `children()`
/// order changes would silently give the two sections different plans.
#[test]
fn the_two_payload_sections_number_the_same_nodes() {
    // For each corpus plan: collect (position, node.name()) from both walks and assert
    // the sequences are equal. Cheap, CPU-only, and it fails the moment a traversal
    // moves.
}
```

- [ ] **Step 2: Append the section to the golden, headed so its source is unambiguous**

The golden's existing per-query blocks stay exactly as they are. The new section is appended per
query, under a header naming where it comes from:

```
-- declared (rust, pre-serialization) --
```

- [ ] **Step 3: Regenerate and check section A did not move**

```bash
UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 \
  cargo test --features rust-only -p peacockdb-core planner::tests
git diff testdata/goldens/recipe-payloads.txt | grep -c '^[-+]sha256='
```

**Expected: 0.** A single moved `sha256=` means the wire changed, which this task forbids — find it
before continuing.

- [ ] **Step 4: Read the new section for the six kinds and for a decimal**

Confirm a `Decimal128` renders with precision and scale. If it prints bare, the wrong `schema_text`
was used.

- [ ] **Step 5: Commit**

```bash
git add peacockdb-core/src testdata/goldens/recipe-payloads.txt
git commit -m "the payload golden carries what each call declares

A second section, rendered from the Call data before serialization, so the
wire does not change and no sha256 moves. When a later task puts schemas
on the wire, the two sections become each other's check."
```

---

### Task 5: The harness, shared

**Files:**
- Modify: `peacockdb-core/src/wire/gpu_tests/` — the walk that `test-layout.md` moved there
- Create: `peacockdb-core/src/wire/gpu_tests/schema_conformance.rs`

- [ ] **Step 1: Make the walk's driving reusable**

Extract the drive loop — `begin_plan`, the calls each recipe names, handles threaded — as
`pub(crate)` within `wire::gpu_tests`, leaving the walk's own assertions in the walk. Shared, not
copied: one list and one driver, two sets of assertions.

- [ ] **Step 2: Remove the harness's assumption that a query has rows**

```bash
grep -rn 'total_rows > 0' peacockdb-core/src/wire/gpu_tests/
```

Query 6 is a filter matching nothing. An `assert!(total_rows > 0)` refuses it outright, and the
assumption is the harness's rather than the engine's.

- [ ] **Step 3: Add the per-firing export and comparison**

```rust
/// The schema the device produced for one firing, read off the IPC stream the thin
/// exporter wrote.
///
/// `StreamReader::try_new` parses the IPC schema message before any batch, so this
/// answers even when the firing produced no rows -- which query 6 needs, and which a
/// `batch.schema()` on the first batch could not give.
///
/// Exports through the thin path (`peacock_result_from_handle` with declared fields), so
/// a decimal that does not fit its declared precision fails here rather than arriving
/// silently widened. The error is the finding, so it is returned rather than unwrapped.
pub(crate) fn exported_schema(
    executor: *mut PeacockExecutor,
    handle: u64,
) -> Result<ArrowSchema, String> {
    let mut ipc: *mut u8 = std::ptr::null_mut();
    let mut len: u64 = 0;
    // The export that already exists. No declared fields are passed and no symbol is
    // added: what this reads is what the device produced under the production path,
    // which is the thing the catalog is measuring.
    let rc = unsafe {
        peacock_result_from_handle(executor, handle, 0, u64::MAX, &mut ipc, &mut len)
    };
    if rc != 0 {
        return Err(format!("the export refused handle {handle}: rc={rc}"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
    let schema = StreamReader::try_new(std::io::Cursor::new(bytes), None)
        .map(|stream| stream.schema())
        .map_err(|error| format!("decoding the exported IPC stream: {error}"));
    unsafe { peacock_result_free(ipc) };
    schema.map(|s| s.as_ref().clone())
}
```

**Confirm `peacock_result_from_handle`'s real signature before writing this** — the row range
arguments above are from the header and may not be what it takes. It is the existing symbol either
way; nothing new is added.

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/wire/gpu_tests/
git commit -m "the walk's driver is shared, and reads what a firing produced

One list and one driver, two sets of assertions -- the walk keeps its own.
The rows-greater-than-zero assumption goes: it was the harness's, not the
engine's, and it refuses a filter that matches nothing."
```

---

### Task 6: The queries, one named test each

**Files:**
- Modify: `peacockdb-core/src/wire/gpu_tests/schema_conformance.rs`

- [ ] **Step 1: Write query 1 as the pattern the other twelve follow**

```rust
/// cuDF has one string type, so a declared `Utf8View` comes back `Utf8`. It enters at the
/// scan and every node above it inherits, which is why this is one test for the class
/// rather than one per node — a test per node would bury one cause under a dozen.
///
/// [#183](../../../../llm-wiki/tasks/active-tickets.md#t183). Measured through the thin
/// exporter, which does not touch string types: this is cuDF's answer, not ours.
#[test]
fn bug_a_declared_utf8view_is_exported_as_utf8() {
    let plan = plan_query("SELECT n_name FROM nation", Mode::Tp1Single);
    let seen = drive_and_collect(&plan);

    // Every firing whose call declared a Utf8View, and what the device handed back for
    // it. Collected rather than asserted one at a time so the failure names how many
    // firings diverged, not just the first.
    let strings: Vec<_> = seen
        .iter()
        .flat_map(|firing| firing.declared_vs_exported())
        .filter(|(declared, _)| *declared == DataType::Utf8View)
        .collect();

    assert!(
        !strings.is_empty(),
        "no call declared a Utf8View, so this query no longer reaches #183 — the plan \
         changed, not the device"
    );
    for (_, exported) in strings {
        // The wrong behaviour, asserted. Delete this test in the change that fixes #183.
        assert_eq!(exported, DataType::Utf8);
    }
}
```

`plan_query`, `drive_and_collect` and `Firing::declared_vs_exported` are Task 6's, and the twelve
below use nothing else. The `assert!(!strings.is_empty(), …)` line is not decoration: without it a
plan change that stops producing a `Utf8View` turns this into a test that asserts nothing and still
passes — the failure mode a `bug_` test is most prone to, because nobody looks at one that is green.

**No generic assert-every-call loop.** A general "every firing matches its declaration" cannot be
green beside this test, and every way around that is a whitelist or building around a bug.

- [ ] **Step 1b: Reconcile the list with the survey first**

```bash
sed -n '1,80p' llm-wiki/reports/sink-divergence.md
```

`sink-divergence-survey.md` runs immediately before this task and measured the whole corpus. Compare
its class table against the spec's thirteen queries: drop a query whose class the survey shows does
not occur, add one for a class nobody predicted, and record both decisions in
`declared-schemas-detail.md`. The spec requires this and it is the cheapest step in the task.

If the report does not exist, the survey has not run — **stop and say so** rather than proceeding on
the ticket-derived list.

- [ ] **Step 2: Write the remaining twelve, each at the mode the spec's table names**

Queries 2–13 of the spec. Each is its own `#[test]`, `bug_`-prefixed only where its assertion records
something wrong. The mode is stated per query; query 13 is `tp1-rowgroup` because `tp1-single` is
`OneBatchPerLane` and cannot produce two firings of one call.

Row 15's query, at `tp1-rowgroup`: `SELECT o_orderkey, o_totalprice FROM orders WHERE o_totalprice
> 500000` — orders is thirteen row groups, eleven survive the predicate, and the scan, the filter and
the export each fire once per batch. The coalesce-all shapes that mode plans (the anti and semi joins,
the cross join) put the several batches on the probe side and the walk refuses a multi-batch probe
(#152), so the claim is measured on the three per-batch calls instead.

- [ ] **Step 3: Write the two that never reach a device**

The interval query is a plan-time refusal — `attach_recipes` returns `Err`, so driving it would
panic. It is a test in the same file with no device. The `EMPTY` class has no corpus source; record
that it is untested by construction rather than omitting it.

- [ ] **Step 4: Run the device cycle and let it tell you**

Every divergence becomes a `bug_` test asserting the **observed** value with its ticket in a comment.
Where no ticket exists, file one. Where a ticket's text the catalog contradicts — #187 is framed as
two engines disagreeing when it is a missing argument — correct it.

**Record which exporter each `bug_` test used.** The two differ by construction, and a finding that
does not say which one produced it is the confusion that misfiled #187 in the first place.

- [ ] **Step 5: Commit, one commit per batch of about five queries**

---

### Task 7: The statement of what was measured

The catalog's actual product, and the thing the earlier attempt never produced.

**Files:**
- Modify: `llm-wiki/build-test.md`
- Modify: `llm-wiki/tasks/declared-schemas.md` — the completeness signoff, appended once
- Modify: `llm-wiki/architecture.md`, `llm-wiki/tickets.md` as findings require

- [ ] **Step 1: Write the table**

One row per (node kind, call): declared or not; measured or not; and if measured, agreeing or
carrying a named `bug_` test. **Declared and never measured is the row that matters** — it is a claim
nothing checked, and without this table it is indistinguishable from a checked one.

- [ ] **Step 1b: Open the `bug_` section in `build-test.md`**

This task writes the chain's first `bug_` tests, so it builds the table they go in. Under
`## Test categories`, below the categories table:

```markdown
### Known-wrong behaviour

`bug_` tests, one per (divergence class, node kind), each asserting what the engine does wrong today
and each deleted by the change that fixes it (`coding-style.md`, "Building around a bug"). **Not part
of the grand total above.** Every row there counts coverage; these count defects, and summing the two
would make the number rise when a bug is found.

**Total: N.**

| Test | Asserts | Ticket | Runs |
|---|---|---|---|
| `bug_a_declared_utf8view_is_exported_as_utf8` | a declared `Utf8View` is exported `Utf8` | #183 | shad-gpu |
```

Two properties to keep as it grows: the ticket column is what makes it a worklist rather than a list
of curiosities, and a row that survives its ticket closing means the fix did not do what it claimed.

- [ ] **Step 2: Record the runtime**

cuDF 25.02, named in the table. #94 is version-specific by its own text, and on a version bump a
`bug_` test going green reads as "fixed, delete it" when it may mean "this runtime differs".

- [ ] **Step 3: Square the wiki, run the full suite, commit**

---

## Self-review against the spec

- **§1 the boundary node declares** — Task 1, including the ten goldens.
- **§2 a schema on each call** — Tasks 2 and 3; the `Eq` hazard is Task 2 step 2, with a stop
  condition rather than a guess.
- **§3 the thin exporter** — Task 5, with the two-path difference asserted rather than assumed.
- **§4 section B** — Task 4, including the zero-moved-`sha256` check that proves the wire did not
  change.
- **§5 one named test per query** — Tasks 6 and 7.
- **§6 the queries** — Task 7, at the modes the spec's table names, including the two that never
  reach a device.
- **§7 catalogued not adjudicated** — Task 7 step 4, and the per-(class, node kind) rule is a global
  constraint rather than a step, because it applies to every test written.
- **API changes: none** — no step adds a `pub`.
- **Not covered by any task, and deliberately:** the spec's note that `nullability` may not be
  validated by this arrow version. It is a question for Task 7's query 8, and if the answer is that
  nothing validates it, that is a finding and a ticket rather than a fix.
