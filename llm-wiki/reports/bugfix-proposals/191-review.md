# Review of the #191 proposal

Read at master `188c23ce` (same commit the proposal read). Every file:line the proposal cites was
opened; nothing built or run.

## 1. Verdict

**Needs changes.** The two-line C++ fix and its regression test are right and stand as written;
what has to change is the closing half — §6 names #185 as q8's next cause, and at that cell #185
is unobservable, so the registry edit the proposal prescribes would mis-attribute the cell to a
ticket it does not reach.

## 2. Findings

### 2.1 — important — q8 at `tp1-single` does not land on #185; it lands on the unticketed batch-split wall, and the registry edit needs that ticket first

§6: "`tpch/q8` `gpu_tp1_single` sheds #191 and stops on the next cause, which by reading the CPU
golden is #185." §3e then says to drop `191` from the registry row and "add the cause's number".

Evidence, from the same golden the proposal read (`testdata/goldens/tpch.sf1/tp1-single-mini.cpu.txt:269-345`):

- The lineitem scan is one batch (`partition_groups=[[[0..48]]]`). The CPU's bottom hash join
  emitted it as `batch_rows=[[8192,8192,8192,8192,8192,2733]]` (`:328-330`), DataFusion's
  `batch_size` chunking, and every node above carries six batches: the top `GpuProject`
  `batch_rows=[[0,0,928,1577,98,0]]`, the `GpuAggregate` `batch_rows=[[0,0,1,2,1,0]]`,
  `output_rows=4` (`:281-284`).
- On the device every join answers one probe batch with one handle
  (`executor/gpu_backend/join.rs:164-180` — `out.extend(prior)`, one element), so one batch of
  2603 rows reaches `GpuAggregate`, which emits **one batch of 2 groups** (1995, 1996).
- `GpuAggregateBatches` therefore consumes 2 rows and emits 2. Its true `in_rows` is `[[2]]`;
  what #185 makes it report — its own output — is also `[[2]]`. The CPU golden says `[[4]]`. The
  line differs whether or not #185 exists, and #185 cannot be seen at this cell at all.
- `line_difference` (`tests/common/golden_text.rs:159-193`) reports the first differing line
  plus "(+N more lines)". The first differing line is the `GpuAggregateBatches` stats line
  (the eight lines above it — unload, accumulate-and-sort, sort, mkt_share project — agree, since
  they are all one batch of 2 rows on both engines). A reader who stops at that line attributes
  it to #185; the `+N more lines` are the six-versus-one batch lists at every node down to the
  bottom join.

The proposal half-sees this ("Behind that is a wall no ticket in `00-tickets.md` names") but
still puts #185 first and tells the registry edit to follow the device run's first line.
`grep -rn 8192 llm-wiki/` and a read of `active-tickets.md` confirm no ticket names the wall.
The same reading applies to `tpch/q3` and `q14` at `tp1-single` (`:83-117`, `:497-519`, both
with 8192-row join chunks on the CPU), so #185's "every `batch_rows` entry … agrees" is not
literally true of those two cells either — outside this ticket, but it is why "the next cause
is #185" must not be written down from the first line of the message.

What this changes in the proposal:

- §6: the cell's next cause is the batch-split disagreement, which wants a **new ticket** in
  `llm-wiki/tickets.md` (a production behaviour: the CPU oracle authors a golden the device
  cannot match at any node above a join whose per-call output exceeds 8192 rows). #185 is not
  named for this cell.
- §3e, registry: `testdata/cost-registry.csv:108` becomes `152 <new>`; the corpus comment at
  `corpus_cases.inc:116-121` names the new ticket for q8. **Ordering constraint the proposal
  does not state**: `cost-report/src/main.rs:442-460` exits 1 when a registry row names a
  ticket that resolves in none of `tickets.md`, `active-tickets.md`, `archived-tickets.md`. So
  the ticket is filed in the same commit as the registry edit, or before it — never after.
- §7's "if it is not [known], it wants a ticket after the device run confirms it" is right in
  spirit; the correction is that the ticket is a precondition of the registry edit, not an
  afterthought.

### 2.2 — minor — the `architecture.md:362` edit files the cast in the wrong paragraph, and the code comment cites the wrong precedent

"Two stay in C++ with a reason" (`architecture.md:362-366`) is the list of coercions **the plan
does not carry** — the loader's decimal width and hash-key normalization. This cast is to a type
the plan *does* carry: `ScalarFunctionExprNode.return_type` is written by `expr_writer.rs:167`
and the fbs doc (`gpu_plan.fbs:202-204`) already says the executor uses it. Adding it to both
that paragraph and the "What the Rust side puts in the flat buffers" table (`:1055`) has the page
saying one cast is both carried on the wire and not. Keep the two table rows the proposal
proposes (`## cuDF options`, the flat-buffers table); drop the `:362` edit.

Same slip in the §3a comment: "the same rule as the loader's decimal width". The loader
(`cpp/src/operators/scan.cpp:98-110`) reads no declared type — it widens every narrow decimal to
DECIMAL128 by a fixed rule. The precedent that actually matches is the `CastExprNode` arm in the
same function (`expr.cpp:912-935`), which casts to a wire-declared `target_type`. Suggested
comment (three lines, under the cap):

```cpp
    // cuDF answers every component in INT16 and takes no output type; the plan declared
    // what DataFusion typed the call, and the wire carries it as return_type. Casting to
    // it is the CastExprNode arm's rule applied to a kernel with a fixed width.
```

### 2.3 — minor — paths the developer will find moved, and prose the count edit leaves stale

- `test-layout.md` (state: approved to build, same chain) moves `tests/test_gpu_executors/` to
  `executor/gpu_backend/gpu_tests/` (`test-layout.md:180`) and renames `test_inc2_conformance`
  to `test_murmur_conformance` (`:354`). The proposal's paths are HEAD's; whichever lands
  second rebases across the other. Say so in the `-impl.md` so the developer does not read a
  missing file as a wrong cite.
- `build-test.md:21`'s "Executors on a device" row enumerates its exec cases by name
  ("filter, project, per-batch sort, …"). Moving the count 31 → 32 without adding "a scalar
  function's declared width" to that list leaves the prose one case short of its own number.
- If `declared-schemas.md` lands first, its `-impl.md` (`:694-711`) opens a `bug_` table in
  `build-test.md`; this fix then deletes a row there, not only the test.

### 2.4 — minor — the device run that reveals the next cause needs a step the proposal does not spell out

A disabled cell is not a test: `test_gpu_corpus.rs:16` expands only the gpu modes a
`corpus_query!` line declares, so nothing runs q8 on a device today. To see what q8 stops on
after the fix, the developer enables `tp1_single` on q8's gpu list locally (uncommitted), ships
with `build-test-shadgpu.sh`, and runs `test_gpu_corpus` under `PCK_TEST_FILTER` — the T19
protocol — then reverts the enable and records the result. First-hour knowledge; one sentence
in `-impl.md`.

## 3. Claims verified

Opened and found true:

- `cpp/src/expr.cpp:641-661` is the `date_part` arm; `:659-660` are the two lines; `:658` refuses
  every field but YEAR/MONTH/DAY/HOUR/MINUTE/SECOND, so `epoch` (the one `Float64` return) never
  reaches the cast. `fb_to_type_id` at `:74-94`, non-static, in scope; `Int32 → INT32`.
- `cudf::datetime::extract_datetime_component` returns int16 with no output-type parameter in
  both 25.02 (`rapids-cuda-12.2/include/cudf/datetime.hpp:247-262`) and 26.02
  (`rapids/include/cudf/datetime.hpp:58-62`); `extract_year` is absent from 26.02. `cudf::cast`
  is already used in this file (`:720`, `:934`), so no new include.
- `return_type()` has exactly one C++ reader, `infer_expr_type` at `:395-397` (grep over
  `cpp/src`). `is_ast_able:403-407` returns false for every `ScalarFunctionExprNode`;
  `build_column:909-910` dispatches to `build_column_scalar_fn`; `project.cpp:44-53` takes the
  column path for it.
- `planner/translator/expr.rs:111-121` builds `Expr::ScalarFunction` with `func.return_type()`;
  `plan/mod.rs:142-147`; `wire/expr_writer.rs:150-179` writes `return_type` and the decimal pair.
  DataFusion 45 `date_part::return_type_from_args` (`datetime/date_part.rs:142-161`): `Int32` for
  every field but epoch, `Float64` there.
- `gpu_backend/mod.rs:179-183`: `concat_batches(&self.schema, …)` wrapped as "the exported
  stream is not the sink's rows: {error}"; `produced()` at `:194-211` prices from the declared
  schema, and `CpuBatch::byte_size` (`cpu_batch.rs:16-21`) does the same, so no per-node byte
  could have moved. `gpu_executor.cpp:51-80` exports through `cudf::to_arrow_schema`, so INT16
  crosses as `Int16`.
- `spark_hash_partition.cu:141-148` casts INT8/INT16 to INT32 before hashing;
  `test_inc2_conformance.rs:175-188` is the INT16 case whose doc names `extract_year`.
- Aggregate keys are taken by ordinal with no type check (`aggregate.cpp:155-174`), so the INT16
  key flows through `#31 CudfAggregate{Partial}`, the merge and both projects (bare `ColumnRef`
  copies) to the unload unchanged.
- Registry `:108` `tpch/q8` tickets `152 191`; `:107` q7 and `:109` q9 carry `152 183`;
  `corpus_cases.inc` T19 batch-7 comment is at `:116-121` (the proposal says `:116-119`). Plan
  goldens: q7 `:1325`, q8 `:1413`, q9 `:1528` at `tp1-single`; q7 and q9 have a `Utf8View` sink
  column at a lower index than the year, so #183 is reported first. `date_part` appears in no
  tpcds plan golden and no tpcds `.sql`; no other Rust or C++ test drives it on a device.
- The regression test compiles against the harness as it stands: `schema_of` (`:273`), `source()`,
  `rows` (`:247`), `one_node` (`:283`, post-order index 1 is the project), `GpuProject::new`
  (`plan/mod.rs:466`, no expression validation at construction), `serialize.rs:68-82` writes both
  `Utf8` and `Date32` literals, `build_scalar:481-483` builds a `timestamp_scalar<timestamp_D>`,
  `build_column:830-835` broadcasts it. 9204 is 1995-03-15 (9131 days to 1995-01-01 plus 73).
  Today it panics inside `expect("the rows cross the boundary")`; after the fix it passes.
- `PAYLOAD_QUERIES` need not change: `call_shapes` (`test_plan_goldens.rs:450-493`) keys on
  recipe lines with seq digits stripped, not on expression kinds. Synthetic queries carry no
  `.duckdb_cost.txt` (`ls testdata/goldens/tpch.sf1/`). `generated.rs` is committed, so an fbs
  doc edit would regenerate it — the reason not to edit the doc holds.
- `executor/gpu_backend/` holds no `exports.rs`; the casts-branch scaffolding is not on master.
  `archived-tasks.md:284-287` records the rejection. `hacks-audit.md` item 12 (`schema_digest`,
  names only) is attributed to #183 with #191 behind it; this fix changes nothing there.
- The §5 query plans as scan → filter → project → unload; with `o_orderkey < 8` pruning orders to
  one row group its bytes are under `SMALL_TABLE_BYTES` (5 MiB, `planner/mod.rs:37`), so it
  drops to one lane at the tp4 modes too. Seven rows, deterministic, `golden_exact` is fine.
- Rejecting the generic cast at the end of `build_column_scalar_fn` is right for a reason
  beyond the decimal scale the proposal gives: `fb_to_type_id(Utf8View) == STRING` and
  `cudf::cast` has no STRING target (`expr.cpp:921-926`), so it would throw on every `substr`,
  `lower`, `upper`, `concat` over a `Utf8View` declaration.

## 4. Corrected proposal — only the sections that change

### 3a (comment only)

Replace the three-line comment with the one in finding 2.2. The two code lines are unchanged.

### 3e — pinning tests, goldens, registry rows, comments (replacing the registry, corpus-comment and architecture bullets)

- **`testdata/cost-registry.csv:108`** — after the device run: `152 191` becomes `152 <new>`,
  where `<new>` is the batch-split ticket below, filed in the same commit. Not #185.
- **`tests/common/corpus_cases.inc:116-121`** — "#191 for q8" becomes the new ticket, stated as
  what it is: the CPU oracle's hash join emits DataFusion's 8192-row chunks per call
  (`cpu_backend/join.rs:236-249` returns whatever `run_node` yields) where the device emits one
  handle per probe batch, so every `batch_rows` list above such a join disagrees, and the
  golden's first differing line is whichever node sits highest — for q8 the
  `GpuAggregateBatches` `in_rows`, which reads as #185 and is not.
- **`llm-wiki/tickets.md`** — one new ticket, ≤15 lines: the batch-split disagreement, with q8,
  q3 and q14 at `tp1-single` as the cells (`tp1-single-mini.cpu.txt:328-336`, `:98`, `:508`),
  and the note that #185 is unobservable wherever the device's one batch makes
  own-output equal true-input.
- **`llm-wiki/architecture.md`** — the two table rows only (`## cuDF options` at `:1034`;
  `### What the Rust side puts in the flat buffers` at `:1055`, the `return_type` row). No edit
  at `:362`: that paragraph lists casts the plan does not carry, and this one is carried.
- **`llm-wiki/build-test.md:21`** — 31 → 32, and "a scalar function's declared width" added to
  the parenthesised list of exec cases; totals `:7` 1569 → 1570, Rust 1135 → 1136.
- Everything else in the proposal's 3e stands (conformance-test doc reword, goldens unmoved,
  ticket to the archive once no cell carries `191`, the declared-schemas / operator-cases
  interplay — plus the `bug_` table row in `build-test.md` if declared-schemas lands first).

### 6 — cells re-enabled

- **From the existing corpus: none.** `tpch/q8` at `tp1-single` sheds #191 and stops on the
  batch-split wall — a new ticket, not #185, which cannot be observed at this cell (finding
  2.1). The four tp4/rowgroup modes stay on #152.
- q7 and q9 at `tp1-single`: as the proposal says, they no longer land on #191 after #183; they
  will land on the same batch-split wall (both have joins over the lineitem scan's single batch,
  `tp1-single-mini.cpu.txt:211-268`, `:345-400`).
- New cells if §5's query joins the corpus: five cpu, five gpu, all green — unchanged.

### 7 — risks and unknowns (one bullet replaced)

- The §6 prediction is from reading the CPU golden and the device join; the device run is what
  confirms it, and its message's `(+N more lines)` is where to read, not its first line. The
  batch-split wall is not known under any name in `llm-wiki/` (grep for `8192`, `chunk`,
  `batch_rows` over tickets); it is filed with the registry edit, not after it.

## 5. Complexity

**S for the fix, M with §5's corpus query.** The code is two lines and one ~30-line device
test, and the reasoning above changes no byte of it. What the proposal under-counts is the
closing half: a shad-gpu cycle for the regression test, a second targeted device run of q8 with
the cell temporarily enabled to read the next cause, one new ticket, and the registry/comment
edits keyed to it. Adding the corpus query is a five-mode CPU regeneration round (rust-only,
local) plus a device run to prove five new gpu cells before their `corpus_query!` line can
declare them — that is the M.
