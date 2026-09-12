# declared-schemas — run detail

Spec: [`declared-schemas.md`](declared-schemas.md). Plan: [`declared-schemas-impl.md`](declared-schemas-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 10. Branch `ENS-declared-schemas`, forked at `c4337459`, the tip
of task 9's branch. **Its PR targets `ENS-operator-cases`.**

## How this task is dispatched

The plan's seven tasks, one dispatch each, the coordinator committing after each: 1 the boundary
node declares (a production change: `NodeKind::Sink` → `Exporter { schema }`, and all ten
`.plans.txt` goldens move — the diff to review); 2 `Call::output_schema`, declared nowhere; 3 the
six arms declare; 4 section B of the payload golden; 5 the harness shared with the walk; 6 the
queries, one named test each, on shad-gpu; 7 the statement of what was measured and the pages.

## Hosts at dispatch (2026-09-12)

Ubuntu 24.04 / glibc 2.39 box; verda's hostname does not resolve; shad-gpu up with a neighbour
at 37 GiB of 143.7; `cpp/build`, `cpp/install` and `target-cudf-rapids-cuda-12.2` warm from task
9's cycle; `target/` warm for `rust-only`; `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.
The device recipe: `scripts/build-test-shadgpu.sh --build`, `--push-binaries`, `--patch`, then
`PCK_RUN_CPP=0 PCK_TEST_FILTER=<module path> … --run-detached` and `--run-status`; an empty
`PCK_TEST_FILTER` is the rung whole (285 on `peacockdb_core_gpu_lib` at the fork, `test_gpu_corpus`
8). Golden regeneration is cpu-side: `UPDATE_CANONICAL=1` for the plan goldens, and
`PEACOCK_REWRITE_RECIPE_BYTES=1` beside it for `recipe-payloads.txt`, with the fixed `/tmp`
testdata symlink `build-test.md` describes.

## What the spec says, and what the tree has

The spec was written before tasks 4-9 ran. What holds and what moved:

- **Prerequisites hold.** `wire/gpu_tests/` exists (task 4's recipe walk lives there) and
  `planner/tests/plan_goldens.rs` owns the goldens. `wire` and `plan_text` carry no bare `pub`
  since task 6 and `SURFACE` refuses a new one; `Call::output_schema` and the renderer are
  `pub(crate)`. `NodeKind::Sink` has six references in `src/` today; the spec's line numbers are
  stale, find them by name.
- **The survey's verdict on the query list** (`reports/sink-divergence.md`, on the prototype branch
  `ENS-sink-divergence-survey` only): keep rows 1, 2 and 12 — `Utf8View → Utf8`, a narrow decimal,
  `Int32 → Int16` are the three classes that reach the sink and all of them; row 2 may name any
  narrow decimal; row 3 stays with the survey as its evidence; rows 7 and 9 stay because the sink
  is blind to names and order; row 8's nullability note should say `try_new` refuses null values
  under a non-nullable field — the flag is the limitation, not the data; no row is dropped, the
  identity rows are unmeasured; the class worth a harness most is #163's count state, which is
  `declared-schemas-derived.md`'s aggregate arm, not this task's six.
- **The operator harness (tasks 8-9) already pins many classes at operator level** — #187 on a
  scan, a sum and arithmetic; #191 as `extract(year)`; #203 a cast to text; #202 descending
  nulls. Those cases declare what the test writes (`Utf8`, not the planner's `Utf8View`), so
  they do not measure the planner's declaration against the device; this task does, per call of a
  planned query, which is a different instrument. A class both catch is fine; say so.
- **The spec's "first task in the chain that writes any `bug_` tests" is false**: task 9 wrote 76,
  counted inside `build-test.md`'s harness row (230) and the grand total (1825). The spec asks
  for a `bug_` table of its own, excluded from the grand total, with a sentence saying why. Plan
  task 7 builds that table for every `bug_` test in the tree — the 76 and this task's — and moves
  the 76 out of the coverage counts; the coordinator owns the page and will do the arithmetic.
- **Tickets** take the next free number after #208 (`tickets.md`'s header counter is 209).

## Run log

### 2026-09-12 — plan task 1 dispatched: the boundary node declares

Board set to `building`.

### 2026-09-12 — plan task 1 done: the boundary node declares

`NodeKind::Sink` is `NodeKind::Exporter { schema }`. `layout()` still answers `None` for it;
`schema()` answers `Some` for every kind. `GpuUnload::new` takes the schema from
`input.kind().schema()` at construction and clones it into the kind.

**References changed, by name not line number** (the tree had nine, not the spec's eight — the
rebuilder's line is under `src/tests/rebuild.rs`, not `tests/common/`):

- `plan/mod.rs` — the variant with its doc line, the `NodeKind` doc comment (it said a sink has
  neither), both accessors, `GpuUnload::new`.
- `plan/validate.rs` — three `matches!` arms (`validate`, `limit_positions`, `structural`); four
  `.expect` messages reworded: "a sink has an input" → "an unload has an input" (the `first()` is
  what can be `None`), and "a sink cannot be an input" / "not a sink" → "every kind declares a
  schema" (×3). The prose "the sink" in `limit_positions`' error text and the `is_sink` local stay:
  they name the role, not the variant. The other 26 `is not a sink`-style expects in files this
  task did not touch are left as they are (`wire/attach.rs`, `wire/recipes.rs`, `plan/union.rs`,
  `plan/common.rs`, `executor/{cpu,gpu}_backend/*`, `planner/translator/*`, `src/tests/injection.rs`,
  `planner/translator/schema_tests.rs`).
- `executor/tests.rs` — the stub backend's `executors_for` arm.
- `src/tests/rebuild.rs` — **not a doc line only.** `BY_CONSTRUCTION` exempted `GpuUnload.kind`
  from `fields_with_one_value` as "nothing in it for a rebuild to drop". With a schema in the kind,
  the two unload fixtures (`source(None)` over `columns()`, `other_source()` over `other_columns()`)
  give it two values, and the guard's other direction went red on it — `GpuUnload.kind varies and
  is listed as constant` in `a_node_rebuilt_over_its_own_children_is_the_node_it_was`. The exemption
  is gone (`[&str; 0]`, mechanism kept); no fixture changed. Correct behaviour of the guard, and the
  fixtures already varied what the rebuild now has to carry.
- `plan/tests/mod.rs` — new `an_unload_declares_the_columns_its_input_hands_it`, red at the fork
  (`schema()` was `None`), green after. 530 → 531.
- `llm-wiki/architecture.md` "Execution" — one sentence said "a sink structurally has neither";
  rewritten to say every kind declares a schema and the exporter alone has no layout, taken from its
  input. Shared rule (code and wiki agree per commit); the coordinator may drop it if task 7 owns
  the page.

**Golden diff.** Exactly the ten `.plans.txt` (5 tpch, 5 tpcds); 600 lines changed, 600 `-`/`+`
pairs, `recipe-payloads.txt` and every `.cpu.txt`/`.cost.txt`/`.result.txt` untouched, no
`PEACOCK_REWRITE_RECIPE_BYTES`. Every changed line is a plan-section `GpuUnload` line and only
that: `GpuUnload` → `GpuUnload: schema=[…]` (no interval) or `GpuUnload: skip=…, fetch=…` gains
`, schema=[…]`. 81 lines per tpcds file, 39 per tpch file. Checked mechanically: for all 600, the
schema text on the unload equals the schema text on the line beneath it (its child, one level
deeper) — the constructor is getting the schema it should. The `--- memory ---` and recipe
sections' `GpuUnload` lines carry no schema and did not move.
`git diff testdata/goldens | grep '^[-+]' | grep -v '^[-+][-+]' | grep -v GpuUnload` is empty.

**Results, all fresh after the last edit (rustfmt reshaping):**

- `cargo test --features rust-only -p peacockdb-core --lib` — 531 passed, 0 failed, 2 ignored.
- `--test test_module_layout` 17 / `--test test_corpus_goldens` 20 / `--test test_cost_model` 3,
  all green.
- `cargo build --features rust-only -p peacockdb-core` and `-p peacockdb` (the CLI) — 0 warnings.
- `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` —
  recompiled core, 0 warnings.
- `scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | grep -v test_support | wc -l` = 46;
  `bare_pub_is_the_surface_and_nothing_else` passes — a variant field on the listed `pub enum
  NodeKind` is not a new item, and the guard reports nothing.
- rustfmt-check clean on the four leaf files; `plan/mod.rs` checked with its `mod` lines masked
  (rustfmt follows them), its only remaining diff the pre-existing one at HEAD.
- Comment caps: longest new comment is 2 lines above a declaration.

Nothing under `cpp/`, no cast, no refusal, no declaration corrected, exporter and wire untouched.
No git mutation; the commit is the coordinator's.

### 2026-09-12 — plan tasks 2-3 done: a call carries its schema, and the six arms declare

**Task 2, `wire/mod.rs`.** `Call` gains `pub(crate) output_schema: Option<Schema>` and a
builder `declaring(self, &Schema) -> Self`; `Call::seq` and `Call::bare` set `None`, so every
existing construction site compiled untouched. **`Eq` cost nothing**: `Call` and `Recipe` drop it
(`plan::Schema` is `PartialEq` only), and nothing in the crate needed it — no `HashSet`/`BTreeSet`
of either, no `Eq` bound, and the build under `rust-only`, the gpu test binary and the CLI came
up with 0 errors and 0 warnings. No key carried. `Writer::push` untouched; the wire does not read
the field.

**Task 3, `wire/attach.rs`, the six sites.** `scan` declares `node.kind().schema()` (already in
hand for the payload); `filter` and `project` declare `node.kind().schema()` — **not** `inputs[0]`,
since a filter carries a projection and a project's output is its own; confirmed the translator
sets both from the DataFusion operator's own `schema()` (`planner/translator/nodes.rs`, the
`FilterExec` and `ProjectionExec` arms). `sort` and `coalesce_all_batches` declare `inputs[0]`
(columns kept, rows reordered/trimmed/concatenated); `unload` declares `node.kind().schema()`,
which plan task 1 gave it. `coalesce_all_batches` and `unload` lose their `_inputs`/`_node`
underscore where the argument became used; `filter` and `project` keep `_inputs`, which they do
not read. The plan's `debug_assert_eq!(declared, inputs[0])` in `unload` is not there: the
constructor is the only source of the exporter's schema and `plan/tests` pins it equal to the
input's, so a second derivation here would be the duplicated rule `coding-style.md` warns about.
The other nine recipe-bearing arms (`aggregate`, `accumulate_and_sort`, `aggregate_batches`,
`merge_sorted_partitions`, `emit_partitions`, `hash_join`, `cross_join`, `nested_loop_join`,
`limit`) still answer `None`; the three forwarders have no recipe.

**Firing-role reading.** For the six, every firing of a call produces one schema: `scan`
`PerBatch` over one seq (every row-group batch of every lane carries the source's columns),
`filter`/`project`/`sort` `PerBatch`, `coalesce_all_batches` once `AtDone`, `unload` `PerHandle`.
Among the arms task 2 of the spec owns, the only places a firing's shape changes are already on
separate calls: `aggregate` carries init (`PerBatch`, state) and its finalize project (a second
call); `aggregate_batches` carries concat + merge (`PerCompaction`, state) and the finalize
(`AtDone`, output) as three calls; `accumulate_and_sort`'s per-batch sort and at-done merge both
carry the input's columns. Nothing read contradicts one schema per call.

**Tests, `wire/tests/declarations.rs`** — `wire/tests.rs` was 942 lines and the cap is 1000, so
it became `wire/tests/mod.rs` (unchanged bar a `mod declarations;` and one header sentence) with
the new cases beside it, the `plan/tests/` shape. Six per-arm tests over hand-built nodes through
`attach_recipes` (the real walk): scan over the rebuilder's `source(None)`; filter **with a
projection narrowing `[k, v]` to `[v]`**, so declaring `inputs[0]` goes red; project narrowing
the same way; sort with a fetch; coalesce-all; unload. All six were red with `None` before the
arms changed. One more, `no_call_outside_the_six_arms_declares_anything_yet`, walks every fixture
of `rebuild::every_kind()` in the recipe walk's order: the six's calls equal their node's schema,
every other call is `None`. Two Welford fixtures are refused by the writer (`m2` on its own has
no name on the wire), so an `Err` fixture is skipped **and the set of kinds that reached a recipe
is asserted equal to the fifteen** — the skip cannot widen silently. Proved live by temporarily
making `limit` declare: red on `GpuLimit`, then restored. Wire tests 23 → 30.

**The split's cost.** `TEST_ONLY_ITEMS` (`test_module_layout/test_code.rs`) names
`has_finish_pass`'s caller by path and `cfg_test_appears_only_on_a_test_module` went red on the
move; the entry and the three doc comments it checks (`executor/mod.rs`,
`executor/cpu_backend/mod.rs`, `executor/cpu_backend/join/tests.rs`) now say `wire/tests/mod.rs`,
as do `build-test.md`'s row link and `coding-style.md`'s mention. `build-test.md`'s counts moved
with the cases: `--lib` 532 → 540 (task 1's +1 and this +7), cpu block 1003 → 1011, Rust 1389 →
1397, grand total 1825 → 1833; the plan-rules row 31 → 32 and the recipes row 23 → 30 with a
clause for the declarations.

**Results, all fresh after the last edit:**

- `cargo test --features rust-only -p peacockdb-core --lib` — 538 passed, 0 failed, 2 ignored;
  `wire::` 30 ok; `planner::tests::plan_goldens` 19 ok, including
  `the_payload_golden_carries_what_each_call_hands_the_executor` with section A untouched:
  `git status testdata/` clean, `PEACOCK_REWRITE_RECIPE_BYTES` unset.
- `--test test_module_layout` 17 passed.
- `cargo build --features rust-only -p peacockdb-core` and `-p peacockdb` — 0 warnings.
- gpu `--no-run` via `scripts/cargo-cudf.sh` — recompiled core, 0 warnings.
- Surface 46; `bare_pub_is_the_surface_and_nothing_else` green — everything added is `pub(crate)`.
- rustfmt-check clean on `attach.rs`, `declarations.rs`, `test_code.rs`, `join/tests.rs`; the four
  `mod.rs` checked with `mod` lines masked, only HEAD's pre-existing diffs remain.
- Comment caps: longest new comment is 8 lines above a test, 4 on a field.

No C++, no wire change, no cast or refusal; no git mutation — `wire/tests.rs` → `wire/tests/mod.rs`
is a working-tree move for the coordinator to stage (git will read it as a rename).

### 2026-09-12 — plan task 4 done: section B of the payload golden

**The renderer**: `plan_text/declared.rs`, `pub(crate) fn render_declared_schemas(root, plan)`
through `plan_text/mod.rs`'s facade. Post-order numbering with the node's own line printed
before its children, the order `wire::recipes::render_recipe_node` prints in, so the two
sections of one query read side by side. It reaches `node_text::schema_text` (now `pub(crate)`
inside the component, still unreachable from outside `plan_text`) — the one that prints
`Decimal128(15,2)` — and `wire::RecipePlan`, a sibling's `pub(crate)` item. Nothing in `wire`
changed; `Writer` is untouched.

**The section's shape**, per query, after section A and under a header naming its source:

```
-- declared (rust, pre-serialization) --
GpuUnload:
  result_from_handle: schema=[l_returnflag:Utf8View, l_linestatus:Utf8View, std_samp_qty:Float64, …]
  GpuProject:
    #9 CudfProject: schema=[l_returnflag:Utf8View, …]
    GpuAggregateBatches:
      #6 CudfCoalescePartitions: undeclared
      #7 CudfAggregate{Merge}: undeclared
      #8 CudfProject{finalize}: undeclared
      GpuEmitPartitions:
        #5 CudfRepartition{Hash, 1→4}: undeclared
        GpuCoalesceAllBatches:
          #4 CudfCoalescePartitions: schema=[…, stddev(lineitem.l_quantity)$count:UInt64, …]
          GpuMergePartitions: no calls
            …
                GpuLoadParquet:
                  #0 CudfScan: schema=[l_quantity:Decimal128(15,2), l_returnflag:Utf8View, l_linestatus:Utf8View]
```

(`tpch shuffle-stddev`.) One line per call under its node: `#seq Kind` for a call with a seq,
the ABI symbol for a bare one; `schema=[…]` where an arm declared, `undeclared` where none has
(spelled out so an absent line cannot pass for a declaration, and greppable: 696 `undeclared`,
534 declared, 154 `no calls` across the twenty queries), `no calls` on a node with no recipe.
Every `Decimal128` in section B carries its digits (147 lines); section A's 37 bare
`Decimal128` are `fb_text::schema_text`'s and did not move.

**How the golden test holds it.** `the_payload_golden_carries_what_each_call_hands_the_executor`
appends section B to the same `text` after `render_plan_recipes`, so a stale B is a section
difference under no variable, exactly as a stale A is (seen red: "20 of 20 queries differ"
before the regen). Under `UPDATE_CANONICAL=1` alone the gate now compares `declared_of(canonical)`
against `declared_of(text)` beside `digests_of` and refuses — seen red with "a declared schema
moved" before the regen and the golden untouched on disk. Only `UPDATE_CANONICAL=1` **and**
`PEACOCK_REWRITE_RECIPE_BYTES=1` rewrite it. `declared_of` reads every line from the header to
the next `== `, keyed by query, so a B section can never be taken for A's (`digests_of` keys on
`sha256=`, which B never emits).

**Regeneration command**, exactly:
`UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens`.
Nothing set for the symlink: the test itself re-points `/tmp/peacock-plan-bytes-root` at this
worktree's `testdata` by atomic rename each run (`point_canonical_root`), and the added lines
carry no host path (`grep /home/` over the `+` lines: 0). The ten `.plans.txt` were regenerated
in the same run and did not move.

**Tests** (`plan_text/tests.rs`, 13 → 16): `the_declared_section_prints_one_line_per_call_under_its_node`
pins the exact text of unload → limit → merge-partitions → source (declared, `undeclared`, `no
calls`, `#0 CudfScan` all in one tree); `the_declared_section_keeps_a_decimals_precision_and_scale`
plans `SELECT c_acctbal FROM customer` over the minimal testdata and finds
`Decimal128(15,2)` on the exporter's line; `the_two_payload_sections_number_the_same_nodes`
renders both sections for a sort+limit, a group-by and a join and asserts the node lines, with
their indentation, are identical — the plan's 1b. All three red on an empty renderer first.
`rendered` in that file was split into `planned` + `render_plan` so the new cases share the
planning helper; `translate`'s registered caller stays `plan_text/tests.rs`.

**Ticket decision on `fb_text::schema_text`'s bare `Decimal128`: no ticket.** The rule in
`prompts.md` gives a ticket to production behaviour only, and this is a golden renderer's text
in a test artefact. It is also not a coverage hole: the `sha256=` digest pins the bytes, so a
precision that moved on the wire goes red through the digest even though the text would not
show it. That makes it cosmetic — fix it when in that code — and fixing it here would move 37
section-A lines the coordinator ruled out of scope. Recorded here for the task that puts schemas
on the wire: before section A can check section B, `fb_text::schema_text` has to print the
digits, or the comparison is blind to precision.

**Page**: `build-test.md` describes the golden's two sections, the gate on B, the plan-text row
(16, with the declared cases named), and the counts: `--lib` 543, cpu 1014, Rust 1400, grand
total 1836.

**Results, all fresh after the last edit:**

- `git diff testdata/goldens/recipe-payloads.txt`: 2465 insertions, 0 deletions, 0 `sha256=`
  moved, 20 headers added, no other golden moved; `PEACOCK_REWRITE_RECIPE_BYTES` unset in the
  proving runs.
- `cargo test --features rust-only -p peacockdb-core --lib` — 541 passed, 0 failed, 2 ignored
  (538 + 3); `planner::tests::plan_goldens` 19; `wire::` 30; `plan_text::` 19 (16 in `tests`, 3
  in `expr_text`).
- `--test test_corpus_goldens` 20; `--test test_module_layout` 17.
- `cargo build --features rust-only -p peacockdb-core` and `-p peacockdb` — 0 warnings (forced
  recompile); gpu `--no-run` — recompiled core, 0 warnings.
- Surface 46; the renderer and `schema_text` are `pub(crate)`.
- rustfmt-check clean on `declared.rs`, `node_text.rs`, `plan_text/tests.rs`, `plan_goldens.rs`;
  `plan_text/mod.rs` masked shows only HEAD's diff. Caps: file header 7, above-declaration 4,
  in-body 4.

No C++, no wire change, no fix. No git mutation; `plan_text/declared.rs` is new and unstaged.

### 2026-09-12 — plan task 5 dispatched: the harness, shared with the walk, in a fresh window

Plan tasks 1-4 committed (`685f3da0`, `c5629cd9`, and the head above); the developer that carried
them hands over here. What it settled: every item `pub(crate)`; `wire/tests/` is a directory now
(`mod.rs` plus `declarations.rs`); section B is `render_declared_schemas` in `plan_text/declared.rs`,
appended per query under `-- declared (rust, pre-serialization) --` with `undeclared` spelled out
for the nine arms this task does not declare; regeneration is `UPDATE_CANONICAL=1
PEACOCK_REWRITE_RECIPE_BYTES=1 … -- planner::tests::plan_goldens` and the test re-points the
`/tmp` root itself; the device rung is 285 at the fork.

### 2026-09-12 — plan task 5 done: the harness, shared with the walk

**Layout.** The driver moved out of `wire/gpu_tests/mod.rs` into `wire/gpu_tests/walk.rs`
(`mod walk;` in the test module's facade); `mod.rs` keeps the walk's own assertions — the
knobs, `trail`, `assert_walk_matches_datafusion`, the nine queries, ten cases, `Driven`/`PROVEN`
— and nothing in what they assert moved. `walk.rs` holds `Session`, `At`, `Walk`, `route`,
`phases`, `context`, `plan_recipes`, `walk`, and the four cases of the driver's own. Everything
shared is `pub(crate)`: `ONE_LANE`/`TWO_LANES` in `mod.rs`; in `walk.rs` `context`,
`plan_recipes(sql, knobs) -> (Box<dyn GpuNode>, RecipePlan)`, `walk(sql, knobs) -> Walked`,
`Walked { batches, calls, firings }`, `Firing`, `Column`, `columns`.

**The harness's signature**, for plan task 6:

```rust
pub(crate) struct Firing {
    pub(crate) symbol: AbiSymbol,
    pub(crate) target: Option<(Seq, FbKind)>,   // None for the bare result_from_handle
    pub(crate) declared: SchemaRef,             // Call::output_schema's fields
    pub(crate) exported: Result<SchemaRef, String>, // raw, or why the export refused
}
impl Firing {
    pub(crate) fn label(&self) -> String;       // "#3 CudfScan" / "result_from_handle"
    pub(crate) fn declared_vs_exported(&self) -> Option<(Vec<Column>, Vec<Column>)>;
}
pub(crate) struct Column { pub(crate) name: String, pub(crate) data_type: DataType }
pub(crate) fn columns(schema: &ArrowSchema) -> Vec<Column>;
```

One `Firing` per handle a declared call produced (a repartition would give one per output
lane), in the order made, plus one per handle the sink exported. `Walk::measure` is the one
skip: a call whose `output_schema` is `None` fires and is not measured — the arms
`declared-schemas-derived.md` takes — so no case needs a guard. `columns` is the comparison by
name and type: nullability is not carried, and a decimal reads at `DECIMAL128_MAX_PRECISION`
(or 256's) on both sides, so only its scale can differ; `Firing::declared` and `::exported`
are the raw schemas for a test that wants the exporter's 38 or its `has_nulls()` flag. The
export is the production `peacock_result_from_handle` over `0..u64::MAX`, schema read off
`StreamReader::try_new` before any batch (`Session::exported_schema`); the sink's export reads
the batches too (`Session::export`). No C++ change, no new symbol.

**What the walk needed.** `Session::export` became `Result` and the sink records a `Firing`
from it; `Session::scan`/`execute` are unchanged and still assert `rc == 0` — a call the
device refuses (query 11's cast to text, #45, will be one) panics at `execute`, which is the
spec's "ignored test naming the gap", not a `Firing`. A refused *export* at any node is the
`Firing`'s `Err`; at the sink it also leaves `batches` short, which the walk's oracle compare
reports as "exported no rows". Every declared intermediate is exported on every walk — no mode
switch: the wire rung went from 10 to 14 cases in 9.95 s, so the extra host copies (the
lineitem scans at two lanes are the largest) did not earn a flag.

**The zero-row question.** `assert!(total_rows > 0)` lives in `assert_walk_matches_datafusion`
(`mod.rs`), the walk's oracle helper, not in the driver — its comment says why (two empty
results compare equal having compared nothing), and it stays. Task 6's cases call `walk`
directly and never go through it. **But the spec's query 6 does not plan**:
`SELECT n_name FROM nation WHERE n_nationkey < 0` is refused at
`scan_mapping/partition.rs` — row-group pruning leaves no survivor and the planner returns
`Invalid("no surviving row groups: …")`. Seen on the device first, then on the CPU through
the CLI at `testdata/tpch.sf1` (`n_nationkey + 100 < 0`, `n_name = 'NOWHERE'` and
`n_nationkey % 7 = 9` all plan and answer zero rows; `< 0` alone refuses). **Ticket #209**
filed (Critical correctness; counter → 210). Task 6's query 6 should take the arithmetic
form; `a_query_selecting_no_rows_is_walked_and_measured` drives it and shows three firings
(`#0 CudfScan`, the filter, `result_from_handle`) each exporting a schema of the declared
arity over zero rows.

**Driver cases** (`walk.rs`, all red first — the API did not exist, then the two device ones
red on wrong expectations of mine: `SUM_BY_FLAG` at two lanes has no plain project above the
aggregate, and the first zero-row SQL was the refused one): `columns_set_precision_and_nullability_aside`
(pure; runs on the device rung because the module is gpu-gated),
`every_firing_of_a_declared_call_is_measured_and_no_undeclared_one_is` (`SUM_BY_FLAG`,
`TWO_LANES`: 2 scans + 1 coalesce-all + 2 exports = 5 firings, the aggregates/repartition/
finalize fired and are absent), `a_query_selecting_no_rows_is_walked_and_measured`,
`a_refused_export_is_returned_rather_than_panicking` (`exported_schema(u64::MAX)` on an open
session → `Err` naming `unknown handle`).

**Results, all fresh after the last edit:**

- `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` —
  0 warnings; `--list gpu_tests::` 289 (285 at the fork + 4, `wire::gpu_tests::walk::*`).
- shad-gpu (neighbour at 37 GiB, pool built): `--build`, `--push-binaries`, `--patch` rc 0;
  `PCK_RUN_CPP=0 PCK_TEST_FILTER=wire::gpu_tests --run-detached` → run
  `20260912T094522-341316`, `peacockdb_core_gpu_lib` 14 passed 0 failed (the walk's ten
  and the driver's four), `test_gpu_corpus` 0 run (filtered). Empty filter → run
  `20260912T094558-341378`, exit 0: `peacockdb_core_gpu_lib` 289 passed 0 failed 26.96 s;
  `test_gpu_corpus` 8 passed 7.94 s. (Earlier runs `…T093954-338627` and `…T094257-339819`
  were the two red rounds above.)
- `cargo test --features rust-only -p peacockdb-core --lib` — 541 passed, 0 failed, 2 ignored;
  `--test test_module_layout` 17.
- rustfmt-check clean on `walk.rs` and `gpu_tests/mod.rs`; surface 46; comment caps: longest
  in-body 2, above a declaration 7.
- `build-test.md`: gpu block 293 → 297, `gpu_tests::` 285 → 289, Rust 1400 → 1404, grand total
  1836 → 1840; a "Recipe walk driver" row (4) under the walk's.

Nothing under `cpp/`, no fix, no wire change. No git mutation: `walk.rs` is new and unstaged,
`mod.rs`, `tickets.md`, `build-test.md` and this file modified.

### 2026-09-12 — plan task 6 done: the queries, one named test each

**Files.** `wire/gpu_tests/declared.rs` (new; 14 cases, one ignored) declared in
`gpu_tests/mod.rs`; `wire/tests/refusals.rs` (new; 2 cases at the rust rung) declared in
`wire/tests/mod.rs`; `#187` corrected in `active-tickets.md`; `build-test.md` rows and counts;
`-impl.md` names row 15's query. Nothing under `cpp/`, no fix, no wire change, no new `pub`.

**The list against the survey.** Rows 1, 2 and 12 kept as the three classes the survey saw at the
sink; row 3 kept as the survey's evidence; 7 and 9 kept because the sink is blind to names and
order; 8's note says the flag is the limitation; no row dropped. Query 6 takes `#209`'s open form
(`n_nationkey + 100 < 0`), the test says why. Query 10 needs aliases — DataFusion refuses two
casts of one column under one generated name. This parquet's `l_linenumber` is `Int64`, so
query 5 pins `Int64` only and `Int32` identity comes from `n_nationkey` in 7, 9 and 10. Rows
13/14 need no parquet: `arrow_cast(l_shipdate, 'Date64')` gives the planner a `Date64` at a
project and `CAST(l_shipdate AS TIMESTAMP)` a `Timestamp(Nanosecond, None)` — both plan on the
CPU (CLI) and the wire decides.

**Where the plan-time refusals live and why.** Row 14 and the interval are `attach_recipes`
refusals with no device: the rung rule says a module declares the lowest rung it needs, so they
are `wire/tests/refusals.rs`, planned over tpch sf1 at the catalog's knobs (`--lib` 541 → 543
passed). Both refuse cleanly, `PlanError::Unsupported`, verbatim: `unsupported: unsupported Arrow
data type: Timestamp(Nanosecond, None) at #2` and `unsupported: unsupported scalar value:
IntervalMonthDayNano("IntervalMonthDayNano { months: 0, days: 1, nanoseconds: 0 }") (#168) at
#2`. One thing seen on the way, not measured: `serialize_schema` maps an unsupported field type
to `fb::DataType::Null` (`unwrap_or`), so a `Timestamp` that reached a schema without passing
through a cast expression would cross as `Null` silently; no query here does, and the cast arm
refuses first. Recorded for #200.

**Row 15's query** (named in `-impl.md`): `SELECT o_orderkey, o_totalprice FROM orders WHERE
o_totalprice > 500000` at `tp1-rowgroup` — eleven of thirteen row groups survive, three calls
fire eleven times each, every firing's exported schema equals its call's first. The mode's
coalesce-all shapes (anti/semi join, cross join — rendered on the CPU) put the several batches on
the probe side, which the walk refuses (#152), so the claim is on the per-batch calls.

**The catalog, per query** (`walk.rs` firings; `?` marks a nullable field; every declared field
is nullable because the parquet columns are `optional`; the device lines are the dump of run
`…T095817` reproduced by hand on the host):

| # | query (tp1-single unless said) | test | verdict | declared → exported, verbatim |
|---|---|---|---|---|
| 1 | `SELECT n_name FROM nation` | `bug_a_declared_utf8view_is_exported_as_utf8` | `bug_` #183 | `#0 CudfScan: [n_name:Utf8View?] → [n_name:Utf8]`; `result_from_handle` the same — enters at the scan |
| 2 | `SELECT l_extendedprice FROM lineitem WHERE l_orderkey = 1` | `a_narrow_decimal_exports_at_the_exporters_default_precision` | limitation, #187 corrected | `#0 CudfScan: [l_orderkey:Int64?, l_extendedprice:Decimal128(15, 2)?] → [l_orderkey:Int64, l_extendedprice:Decimal128(38, 2)]`; `#1 CudfFilter` and the export `(15, 2)? → (38, 2)` |
| 3 | `… CAST(l_extendedprice AS DECIMAL(38,4)) …` | `a_decimal_declared_at_max_precision_agrees_for_the_exporters_reason` | green, for the wrong reason | `#2 CudfProject: [lineitem.l_extendedprice:Decimal128(38, 4)?] → [… Decimal128(38, 4)]` |
| 4 | `SELECT l_shipdate FROM lineitem WHERE l_orderkey = 1` | `a_date32_survives_the_crossing` | green | `[l_shipdate:Date32?] → [l_shipdate:Date32]` at all three |
| 5 | `SELECT l_orderkey, l_linenumber FROM lineitem WHERE l_orderkey = 1` | `an_int64_survives_the_crossing` | green | `[l_orderkey:Int64?, l_linenumber:Int64?] → [l_orderkey:Int64, l_linenumber:Int64]` |
| 6 | `SELECT n_name FROM nation WHERE n_nationkey + 100 < 0` | `a_zero_row_batch_types_its_string_column_as_a_populated_one` | green | `#1 CudfFilter: [n_name:Utf8View?] → [n_name:Utf8]`, zero rows; the sink's exported schema equals query 1's |
| 7 | `SELECT n_name AS label, n_nationkey AS id FROM nation` | `column_names_survive_the_crossing` | green | `#1 CudfProject: [label:Utf8View?, id:Int32?] → [label:Utf8, id:Int32]` |
| 8 | `SELECT n_nationkey, CASE WHEN n_nationkey > 10 THEN n_name END AS maybe FROM nation` | `nullability_is_read_off_the_data_rather_than_the_declaration` | limitation | `#1 CudfProject: [n_nationkey:Int32?, maybe:Utf8View?] → [n_nationkey:Int32, maybe:Utf8?]` — the key's flag is off because no key is null |
| 9 | `SELECT n_regionkey, n_nationkey FROM nation` | `column_order_and_arity_survive_the_crossing` | green | `#0 CudfScan: [n_nationkey:Int32?, n_regionkey:Int32?] → same order`; `#1 CudfProject: [n_regionkey:Int32?, n_nationkey:Int32?] → [n_regionkey:Int32, n_nationkey:Int32]` |
| 10 | `SELECT CAST(n_nationkey AS BIGINT) AS wide, CAST(n_nationkey AS DOUBLE) AS real FROM nation` | `fixed_width_cast_targets_survive_the_crossing` | green | `#1 CudfProject: [wide:Int64?, real:Float64?] → [wide:Int64, real:Float64]` |
| 11 | `SELECT CAST(n_nationkey AS VARCHAR) FROM nation` | `a_cast_to_text_cannot_be_measured_until_the_device_answers_it` | **ignored**, #203 | run by hand with `--ignored`: `execute_node(#1, [1] handles) failed: [in CudfProject] cast to STRING from a non-string type not supported in column path` — no handle reaches the export |
| 12 | `SELECT extract(year FROM o_orderdate) FROM orders` | `bug_an_extracted_year_declared_int32_is_exported_as_int16` | `bug_` #191 | `#0 CudfScan: [o_orderdate:Date32?] → [o_orderdate:Date32]`; `#1 CudfProject: [date_part(Utf8("YEAR"),orders.o_orderdate):Int32?] → [… :Int16]`; the export the same — enters at the project |
| 13 | `SELECT arrow_cast(l_shipdate, 'Date64') FROM lineitem WHERE l_orderkey = 1` | `bug_a_date64_is_exported_as_a_millisecond_timestamp` | `bug_` #200 | `#2 CudfProject: [arrow_cast(lineitem.l_shipdate,Utf8("Date64")):Date64?] → [… :Timestamp(Millisecond, None)]`; the export the same |
| 14 | `SELECT CAST(l_shipdate AS TIMESTAMP) FROM lineitem WHERE l_orderkey = 1` | `wire::tests::refusals::a_cast_to_timestamp_is_refused_naming_the_type` | green, rust rung | `Unsupported("unsupported Arrow data type: Timestamp(Nanosecond, None) at #2")` |
| 15 | `SELECT o_orderkey, o_totalprice FROM orders WHERE o_totalprice > 500000`, tp1-rowgroup | `the_firings_of_one_call_export_one_schema` | green | `#0 CudfScan` ×11, `#1 CudfFilter` ×11, `result_from_handle` ×11, each `[o_orderkey:Int64?, o_totalprice:Decimal128(15, 2)?] → [o_orderkey:Int64, o_totalprice:Decimal128(38, 2)]` |
| — | `SELECT l_shipdate + interval '1 day' FROM lineitem WHERE l_orderkey = 1` | `wire::tests::refusals::an_interval_literal_is_refused_naming_168` | green, rust rung | `Unsupported("unsupported scalar value: IntervalMonthDayNano(…) (#168) at #2")` |

**Which exporter.** Every line above is the production `peacock_result_from_handle` — the only
exporter this task uses; the thin one was cut by the spec.

**Untested by construction.** A sink column of `Binary`, `Null` or `Float16`: cuDF maps the class
to `EMPTY`, no corpus column declares one and nothing refuses them at planning time. Named, not
measured. `UInt8`/`UInt64`, `ORDER BY`'s sort, `Dictionary`, `Int8`, `Float32` and list types
are out of this task's six arms or have no source, as the spec says.

**Tickets.** No new ticket: every divergence had one (#183, #191, #200), the gap had one (#203),
the zero-row refusal is #209 from task 5. #187's text corrected in place (a missing argument,
not two verdicts) with the catalog's test named. The `bug_` tests here are three; the
coordinator's `bug_` table (plan task 7) takes them out of the coverage counts — for now they sit
inside the "Schema catalog" row (14) as task 9's do in the harness row.

**On TDD.** Every case was red first at compile (`declared.rs` did not exist); the device then
answered all thirteen green on the first run — the tickets' predictions held, including the two
the spec called unexercised (#200's cast and the CASE's nullability). The one thing the device
corrected was the count in row 15's comment (eleven, not thirteen — the predicate prunes two row
groups), fixed before the final cycle.

**Results, all fresh after the last code edit** (the last edit after the device runs is a
`//!` header trim in `gpu_tests/mod.rs`; gpu `--no-run` after it: 0 warnings):

- `scripts/cargo-cudf.sh test … --features gpu --no-run` 0 warnings; `--list gpu_tests::` 303
  (289 + 14).
- shad-gpu: `--build`/`--push-binaries`/`--patch` rc 0. `PCK_TEST_FILTER=wire::gpu_tests::declared`
  → run `20260912T100132-348891`: `peacockdb_core_gpu_lib` 13 passed 0 failed 1 ignored (3.22 s).
  Empty filter → run `20260912T100141-348929`, exit 0: `peacockdb_core_gpu_lib` 302 passed
  0 failed 1 ignored (30.27 s); `test_gpu_corpus` 8 passed (8.07 s). First catalog run
  `20260912T095817-345847` was the same 13/0/1; the dump binary (scratch, removed) ran by hand.
- `cargo test --features rust-only -p peacockdb-core --lib` 543 passed, 0 failed, 2 ignored
  (541 + the two refusals); `--test test_module_layout` 17.
- rustfmt-check clean on `declared.rs`, `refusals.rs`, `walk.rs`, `gpu_tests/mod.rs`; surface 46;
  caps: in-body ≤ 2, declaration/header ≤ 10.
- `build-test.md`: cpu `--lib` 543 → 545, cpu block 1014 → 1016; gpu `gpu_tests::` 289 → 303,
  block 297 → 311; Rust 1404 → 1420; grand total 1840 → 1856; rows "Wire refusals at plan time" (2)
  and "Schema catalog" (14).

No git mutation: `declared.rs` and `refusals.rs` are new and unstaged.

### 2026-09-12 — plan task 7 done: the statement of what was measured

Docs only; `git diff --stat` touches `llm-wiki/` alone. No build, no device run.

**The measurement table**, one row per (node kind, call), complete over `attach.rs`'s
eighteen `NodeRef` arms. "Measured" means a catalog case (`wire/gpu_tests/declared.rs`) compared
the firing's exported schema with its declaration; "exported" means the driver's own case
(`wire/gpu_tests/walk.rs`) read the export and asserted only that it answered.

| node kind | call (pattern) | declared | measured by | verdict |
|---|---|---|---|---|
| `GpuLoadParquet` | `CudfScan` (per batch) | yes | queries 1–13, 15 | agrees on `Date32`, `Int32`, `Int64`, names, order, arity; **`bug_a_declared_utf8view_is_exported_as_utf8` (#183)** enters here; decimals at 38 (#187, the exporter's) |
| `GpuFilter` | `CudfFilter` (per batch) | yes | queries 2–6, 13, 15 | agrees; inherits #183 (query 6) and the precision default |
| `GpuProject` | `CudfProject` (per batch) | yes | queries 3, 7–10, 12, 13 | agrees on casts to `Int64`/`Float64`, `Decimal128(38,4)`, names, order; **`bug_an_extracted_year_declared_int32_is_exported_as_int16` (#191)** and **`bug_a_date64_is_exported_as_a_millisecond_timestamp` (#200)** enter here; nullability read off the data (query 8, the exporter's) |
| `GpuSort` | `CudfSort` (per batch) | yes | **nobody** | **declared and never measured**: every `ORDER BY` plans `GpuAccumulateBatchesAndSort` (task 2's arm) and no shape reaches a bare `GpuSort` |
| `GpuCoalesceAllBatches` | `CudfCoalescePartitions` (at done) | yes | exported only — `every_firing_of_a_declared_call_is_measured_and_no_undeclared_one_is` | **declared, exported, held to no claim**: at `tp1-rowgroup` the coalesce-all shapes (anti/semi/cross join) put the several batches on the probe side, which the walk refuses (#152) |
| `GpuUnload` | `result_from_handle` (per handle) | yes | every query | carries what entered below it: #183, #191, #200; precision and nullability the exporter's |
| `GpuAggregate` | `CudfAggregate{Partial}` (per batch), `CudfProject{finalize}` (per batch, self-finalizing) | no | — | `declared-schemas-derived.md` |
| `GpuAccumulateBatchesAndSort` | `CudfSort` (per batch), `CudfSortPreservingMerge` (at done) | no | — | derived task |
| `GpuAggregateBatches` | `CudfCoalescePartitions`, `CudfAggregate{Merge}` (per compaction), `CudfProject{finalize}` (at done) | no | — | derived task |
| `GpuMergeSortedPartitions` | `CudfSortPreservingMerge` (at done) | no | — | derived task |
| `GpuEmitPartitions` | `CudfRepartition` (per batch) | no | — | derived task |
| `GpuHashJoin` | `CudfHashJoin` (per probe batch); with a finish pass `CudfProject{probe keys}`, `CudfCoalescePartitions`, `CudfHashJoin`, `CudfProject{null pad}`/`{narrow}` (at done) | no | — | derived task |
| `GpuCrossJoin` | `CudfCrossJoin` (per probe batch) | no | — | derived task |
| `GpuNestedLoopJoin` | `CudfNestedLoopJoin` (per probe batch) | no | — | derived task |
| `GpuLimit` | `slice_handle` (per straddling batch) | no | — | derived task |
| `GpuMergePartitions`, `GpuUnion`, `GpuInterleave` | none | — | — | no recipe; nothing to declare |

Two rows matter: `CudfSort`, declared by task 3 and checked by nothing on a device, and the
coalesce-all, exported but never compared. Both are claims the catalog does not back, and this
table is the only place that says so.

**`build-test.md`.** New `### Known-wrong behaviour` under Test categories, below the second
table: the why sentence, the runtime, `**Total: 79.**`, one row per `bug_` test — test (linked),
asserts (read off each test's comment and assertion), ticket (anchor form the page uses), runs
(shad-gpu, all of them). **79 confirmed by grep** over the whole repo: `fn bug_` under
`peacockdb-core/src` — accumulate 5, aggregate 7, emit 3, exec 7, join 42, nested 7, source 5,
`wire/gpu_tests/declared.rs` 3; nothing under `peacockdb-core/tests`, `cpp/` (the only `bug_`
hits there are `debug_`), or the Python. Arithmetic as decided: Operator harness 230 → 154,
Schema catalog 14 → 11, grand total 1856 → 1777, Rust 1420 → 1341; block headers keep their
`--list` figures (cpu `--lib` 545, gpu `gpu_tests::` 303) and the header paragraph gains the
second reason a `--list` total exceeds the rows, plus the `bug_` total beside the grand total
with the one sentence. **Checked by hand:** the 67 N cells of the two tables sum to 1777 —
447 + 1 + 20 + 3 + 26 + 14 + 32 + 23 + 5 + 4 + 13 + 8 + 10 + 2 + 2 + 2 + 2 + 2 + 4 + 4 + 1 + 30 +
2 + 16 + 1 + 90 + 64 + 1 + 29 + 9 + 43 + 5 + 13 + 31 + 11 + 27 + 3 + 16 + 2 + 3 + 7 + 1 + 154 +
10 + 4 + 11 + 10 + 31 + 4 + 26 + 8 + 17 + 37 + 41 + 216 + 19 + 93 + 12 + 6 + 27 + 4 + 4 + 4 + 1 +
4 + 4 + 1 = 1777 — and 1777 − 67 (C++) − 369 (Python) = 1341 Rust.

**The runtime.** `$CUDF_ROOT/include/cudf/version_config.hpp` on the build host: `25.2.2`
(`CUDF_VERSION_MAJOR 25`, `MINOR 2`, `PATCH 2`). `cpp/install/lib` ships no `libcudf`;
`build-test-shadgpu.sh` puts `$HOME/miniforge3/envs/rapids-cuda-12.2/lib` on the run's
`LD_LIBRARY_PATH`, and that env on shad-gpu carries the same `version_config.hpp`, `25.2.2`.
So the device binary links shad-gpu's own libcudf 25.02.02 — the plan's 25.02, at patch 2.
Named in the `bug_` section's preamble.

**`architecture.md`.** The "Types are a plan fact" paragraph (Node display) said the declared
schema "checks nothing"; corrected to "the golden checks nothing", and one paragraph added
beneath it: six calls declare, the payload golden prints them, the catalog holds scan, filter,
project and unload to the declaration, `CudfSort` is declared and unreached, the coalesce-all
exported and unclaimed, the three `bug_` classes named. Nothing else on the page spoke of
per-call declarations.

**Tickets.** None new; no finding required one. **Nullability, for the record** (the plan's
self-review item): arrow 54's `RecordBatch::try_new` refuses null values under a non-nullable
field, so the direction the engine could get wrong is caught; the direction it cannot report —
a nullable declaration over a batch with no null — is the exporter's `has_nulls()` flag, and
query 8 records it as a limitation rather than a ticket.

Pages touched: `build-test.md`, `architecture.md`, this file. The completeness signoff in the
spec is not written; it follows review.
