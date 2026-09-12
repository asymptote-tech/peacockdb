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
