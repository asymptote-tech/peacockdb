# #191 — the device exports Int16 for an extracted year the plan declared Int32

Read at master `188c23ce`. Paths relative to `/media/data/peacockdb`. Nothing built or run.

## 1. Issue

The plan declares `extract(year FROM d)` — DataFusion's `date_part('YEAR', d)` — as `Int32`, and
the device answers `Int16`. The type leaves the device unchanged through the aggregate, the sort and
the export, and the unload refuses it: `GpuExport::unload` concatenates the decoded IPC batches
against the sink's declared schema (`peacockdb-core/src/executor/gpu_backend/mod.rs:179-183`) and
arrow's `concat_batches` demands exact type equality — `the exported stream is not the sink's
rows: … expected Int32 but found Int16 at column index 0`.

Where the wrong type is made: `cpp/src/expr.cpp:659-660`, the `date_part` arm of
`build_column_scalar_fn`:

```cpp
    auto ts = build_column(args->Get(1), table);
    return cudf::datetime::extract_datetime_component(ts->view(), comp);
```

`cudf::datetime::extract_datetime_component` answers every component in `INT16`, has no output-type
parameter, and the arm returns it as is. The wire already carries the declared type beside it —
`ScalarFunctionExprNode.return_type` (`flatbuffers/gpu_plan.fbs:198-214`, doc: "Output type, used by
the column-producing executor to allocate the result column") — and this arm never reads it.

**Cells disabled.** `00-tickets.md` is right: the gpu column of `tpch/q8` at `tp1-single` and nothing
else today (`testdata/cost-registry.csv:108`, tickets `152 191`; `tests/common/corpus_cases.inc:113-121`).
The other four q8 modes stop earlier on #152. Two more corpus queries carry the same expression and
will reach it once #183 clears: `tpch/q7` (`date_part(YEAR, l_shipdate) as l_year`,
`goldens/tpch.sf1/tp1-single.plans.txt:1325`) and `tpch/q9` (`o_year`, `:1528`), both refused today at
a lower column index on `Utf8View` (registry `:107`, `:109`, tickets `152 183`). No tpcds plan carries
`date_part` (grep over `goldens/tpcds.sf1/tp1-single.plans.txt`). The CPU tier is green on all three:
DataFusion's `date_part` produces `Int32` itself.

Not #163's class: the declared type here is right, and the producing expression on the device
disagrees with it. Not #187's either, as the ticket says: that one is a missing argument to the
exporter; this one is a kernel with a fixed width.

## 2. Root cause

Trace of `o_year` in `tpch/q8` at `tp1-single` (plan at `goldens/tpch.sf1/tp1-single.plans.txt:1406-1450`):

1. **Planner.** `planner/translator/expr.rs:111-121` meets DataFusion's `ScalarFunctionExpr` and
   builds `Expr::ScalarFunction { name: "date_part", args, return_type: func.return_type().clone(),
   nullable }` (`plan/mod.rs:142-147`). DataFusion 45's `date_part` declares `Int32` for every
   field but `epoch` (`datafusion-functions-45.0.0/src/datetime/date_part.rs:142-161`), so the
   `GpuProject` at `:1413` declares `o_year:Int32`, and every node above it inherits that.
2. **Wire.** `wire/expr_writer.rs:150-179` writes the node with `return_type: Int32`. Byte-exact,
   nothing missing.
3. **C++ routing.** `is_ast_able` (`cpp/src/expr.cpp:403-406`) says no to every
   `ScalarFunctionExprNode`, so the whole expression takes the column path;
   `build_column` (`:909-910`) dispatches to `build_column_scalar_fn`. `infer_expr_type`
   (`:395-397`) — the one reader of `return_type()` in the tree — reports the declared `INT32`
   for AST-routing purposes, which is a claim about the column the arm below does not honour.
4. **The arm.** `expr.cpp:641-661` decodes the field, builds the timestamp column and returns
   `extract_datetime_component(ts, comp)` directly. cuDF fixes that kernel's output at `INT16`
   for every component (25.02 and 26.02 alike; `extract_year` was the same width before its
   removal, `cpp/tests/gpu/test_tpch.cpp:699`). No cast, no read of `return_type()`.
5. **Downstream, silently.** The aggregate groups on the `INT16` key (`#31 CudfAggregate{Partial}`),
   the merge and finalize pass it through, the sort orders it, and at tp4 modes the scatter
   would hash it: `spark_hash_partition.cu:141-148` widens `INT8/16` to `INT32` before hashing
   because Spark does, so lane placement already agrees with the CPU's `Int32` hash — which is
   what `gpu_spark_partition_ids_int16_match_comet_live` (`tests/test_inc2_conformance.rs:175-188`)
   was written to prove, and the comment there names `extract_year` as the reason an `INT16` key
   exists at all.
6. **Export.** `gpu_executor.cpp`'s IPC export maps `INT16` to arrow `Int16`. `GpuExport::unload`
   (`gpu_backend/mod.rs:179`) concats against the declared `Int32` schema and refuses. Bytes were
   never wrong: `logical_size_from_schema` prices both engines from the declared schema
   (`gpu_backend/mod.rs:194-211`), so no per-node golden could have seen it. The first place the
   type is compared is the unload — which is also why it is "newly reached, not new".

Twin on the CPU: `cpu_backend/expr_physical.rs:110-120, 143-157` rebuilds
`ScalarFunctionExpr::new(name, udf, args, return_type)` and DataFusion evaluates it, producing
`Int32`; `declared_as` (`cpu_backend/mod.rs:239`) then has nothing to relabel. The CPU agrees with
the declaration by construction, so — as `declared-schemas.md` warns — its agreement is not evidence
about what the type *should* be. What decides it is the architecture's rule: the declared type is
DataFusion's, the wire carries it, and no executor may change a type the plan did not ask for. Here
the plan asked for `Int32` and the device produced something else.

## 3. Localized fix

One arm in one C++ file, plus a regression test and the comments that name the old behaviour.

### 3a. `cpp/src/expr.cpp`, `build_column_scalar_fn`, the `date_part` arm (`:659-660`)

Replace the two lines with:

```cpp
    auto ts = build_column(args->Get(1), table);
    auto component = cudf::datetime::extract_datetime_component(ts->view(), comp);
    // cuDF answers every component in INT16 and takes no output type; the plan declared
    // what DataFusion typed the call. Honouring it here is the same rule as the loader's
    // decimal width: the kernel's natural width is not the declaration's.
    return cudf::cast(component->view(),
                      cudf::data_type{fb_to_type_id(sf->return_type())});
```

- `fb_to_type_id` is already in scope (`expr.cpp:73-93`); `Int32 → INT32`. No decimal scale
  is involved, so a bare `data_type{id}` is correct for every type `date_part` can declare
  (`Int32`; `Float64` only for `epoch`, which the arm already refuses at `:658`).
- Unconditional rather than `if (type differs)`: today's DataFusion always declares `Int32`, so
  the branch would never be false, and `coding-style.md` forbids the guard for the impossible
  case. `INT16 → INT32` is lossless and null-preserving (`cudf::cast` carries the mask).
- The comment is three lines, inside a function body, under the four-line cap.
- After the change `infer_expr_type`'s `ScalarFunctionExprNode` arm (`:395-397`) states a fact
  rather than an intention; no edit needed there.

### 3b. Regression test — red before, green after

`peacockdb-core/tests/test_gpu_executors/exec.rs`, beside
`a_project_evaluates_its_expressions_under_the_names_it_declares` (`:46`):

```rust
/// A year is a kernel whose natural width is not the declared one: cuDF extracts every
/// datetime component as Int16 and DataFusion types the call Int32. The export against the
/// declared schema is where the two met (#191), so that is what this asserts through.
#[test]
fn a_date_part_answers_in_the_type_the_plan_declares() {
    let out = schema_of(&[("year", DataType::Int32)]);
    let node = GpuProject::new(
        source(),
        vec![NamedExpr::new(
            Expr::ScalarFunction {
                name: "date_part".to_string(),
                args: vec![
                    Expr::Literal(ScalarValue::Utf8(Some("YEAR".to_string()))),
                    // 1995-03-15 as days since the epoch, so the answer is written down.
                    Expr::Literal(ScalarValue::Date32(Some(9204))),
                ],
                return_type: DataType::Int32,
                nullable: true,
            },
            "year",
        )],
        Schema::new(Arc::new(out.clone())),
    );
    let answer = one_node(Box::new(node), &out);
    assert_eq!(answer.record_batch().schema().field(0).data_type(), &DataType::Int32);
    assert_eq!(
        rows(&answer).into_iter().map(|row| row[0].clone()).collect::<Vec<ScalarValue>>(),
        vec![ScalarValue::Int32(Some(1995)); 6]
    );
}
```

Why this shape: the fixture has no date column and is shared with the CPU contract
(`tests/common/executor_cases.inc`), so the date is a broadcast literal — `build_scalar` has a
`Date32` arm (`expr.cpp:481-483`) and `build_column` broadcasts a literal
(`:830-834`). `one_node` (`test_gpu_executors.rs:283`) runs the recipe on a device and exports
through `GpuExport::unload` against `out`, so today it fails inside `expect("the rows cross the
boundary")` with the ticket's exact message; after 3a it passes. The developer should confirm the
day count for 1995-03-15 (9204 = 25 years incl. 6 leap days + 73) or pick a date and compute it
once; the value is the assertion.

Optional second pin, closer to the fix: a `PlanExecutor` gtest in
`cpp/tests/gpu/test_plan_executor.cpp` projecting `date_part('YEAR', CAST(r_regionkey AS Date32))`
over `tpch.minimal/region` and asserting `type().id() == INT32` and 1970 at every row. Two new
helpers are needed there (a string literal, a `ScalarFunctionExprNode`); build the literal with
`fb::ScalarValueBuilder` and named setters, since the positional `CreateScalarValue` trap
`typed-nulls.md` records (`is_null` at position 2) would otherwise build the field name as an
empty string. One test is enough; the Rust one reproduces the corpus failure.

### 3c. What it does not touch

- No Rust production code, no planner or wire change, no fbs change, no new ABI symbol.
- Not the unload: `concat_batches` against the declared schema stays the check it is; the
  human rejected casting there (`archive/archived-tasks.md:284-287`, casts.md).
- Not a general "cast every scalar function to `return_type`" at the end of
  `build_column_scalar_fn` — see Alternatives.
- Not `infer_expr_type`, `is_ast_able`, `spark_hash_partition.cu`, `gpu_executor.cpp`.
- Not the `.fbs` doc comment on `return_type`: it becomes true for the one arm that needed it,
  and editing it would regenerate `wire/generated.rs` and `gpu_plan_generated.h` for prose.

### 3d. How CPU and GPU stay one engine

Both produce the type DataFusion declared. The CPU because DataFusion computes it
(`expr_physical.rs:151`); the device because the column path now casts to the `return_type` the
recipe carries, which is the same `DataType` the CPU's `ScalarFunctionExpr` was built with. Above
the project, both group, merge, sort, hash and export an `Int32`. Lane placement does not move:
Spark's murmur3 hashes a short as an int, so the pre-fix `INT16` key hashed to the same lane the
CPU's `Int32` did (`spark_hash_partition.cu:141-148`, proved by the conformance case). No plan
text, recipe payload, or per-node byte moves, since all three are computed from the declared
schema.

### 3e. Pinning tests, goldens, registry rows, comments that must change with it

- **`tests/test_inc2_conformance.rs:175-178`** — the `INT16` conformance case's doc gives
  `cudf::extract_year emits INT16` as the reason a year-grouped query hashes an `INT16` key. After
  the fix no engine path makes an `INT16` from a date. Keep the test (a parquet `smallint`
  column still arrives as `Int16`, and the kernel arm is real), reword the motivation to that.
  Leaving it is the "comment names a cause that no longer exists" drift `coding-style.md` forbids.
- **`tests/common/corpus_cases.inc:116-119`** — the T19 batch-7 comment names "#191 for q8".
  Rewrite to name what q8 at `tp1-single` stops on after the device run (see §6).
- **`testdata/cost-registry.csv:108`** — drop `191` from `tpch/q8`'s tickets after the device run
  that shows the next cause; add the cause's number. A cell moves onto its next ticket, it does
  not turn green (§6).
- **Goldens** — none move. `*.plans.txt` unchanged (plan text is IR, not cuDF); `*-mini.cpu.txt`,
  `.cost.txt`, `.result.txt` are CPU-authored and the CPU did not change; `recipe-payloads.txt`
  unchanged (wire bytes identical). If §5's query is added to the corpus, new sections appear and
  no existing byte moves.
- **`llm-wiki/architecture.md`**:
  - `:362` "Two stay in C++ with a reason." becomes three, with the third: the datetime
    component's width — cuDF answers `extract` in `INT16` and takes no output type, so the
    column path casts to the `return_type` the expression declares, as the loader honours the
    decimal width it declares.
  - `## cuDF options` table (`:1034`): one row — `date_part` output type · `expr.cpp` ·
    `cudf::cast` to `ScalarFunctionExprNode.return_type` · the default would hand the unload an
    `Int16` for a declared `Int32`, refused at the sink.
  - `### What the Rust side puts in the flat buffers` (`:1055`): one row —
    `ScalarFunctionExprNode.return_type` · `expr_writer.rs` · `ScalarFunctionExpr::return_type()`
    · the `date_part` column's type, and `infer_expr_type`'s answer for AST routing. Today the
    field has a writer and, for the column it names, no consumer — #132's shape.
- **`llm-wiki/build-test.md`** — "Executors on a device (Rust)" `:21` gains a case (31 → 32),
  grand total `:7` 1569 → 1570, Rust 1135 → 1136. Corpus rows move only if §5's query is added.
- **`llm-wiki/tasks/active-tickets.md:178-195`** — #191 moves to `archive/archived-tickets.md`
  once no registry cell carries `191`.
- **`tasks/declared-schemas.md:219`**, query 12, expects `Int32 → Int16` as a `bug_` test. If
  this lands first, that row becomes a green assertion rather than a `bug_` test, and the
  `-impl.md` should say so; if declared-schemas lands first, this fix deletes the `bug_` test it
  wrote, as the rule requires. `operator-cases.md:29`'s "a scalar function" `GpuProject` case
  goes green rather than red under the harness.

### 3f. Hacks-audit scaffolding

Nothing in `reports/hacks-audit.md` stands because of #191 specifically. Item 12 of "What the
known bugs are propping up" (`result_text.rs:92`, the names-only `schema_digest`) attributes to
#183 "with #187 and #191 behind it": this fix does not let the digest hash types again while
#183's `Utf8View → Utf8` remains, so the fix respects it and changes nothing there; when #183
closes and the digest hashes types, q8's `o_year` is what this fix makes pass that check. No
`exports.rs`, no cast-reason enum, no whitelist survives on master from the rejected casts branch
(`ls executor/gpu_backend/` confirms), so there is nothing to remove. `cpp/tests/gpu/tpch_golden.hpp:432`'s
`INT16` arm in `int64_at` belongs to the bare-cuDF sf40 suite, which calls
`extract_datetime_component` directly and never crosses the wire — not this engine's path, leave it.

## 4. Alternatives rejected

- **Cast at the unload to the declared schema** — the casts.md approach the human rejected on
  2026-09-10 ("builds the divergence into the plan"); also blankets the one check that surfaced
  #187 and #191.
- **Planner emits `Expr::Cast(Int32)` around `date_part`** — Rust modelling a cuDF kernel's width,
  the "predict in Rust what C++ does" instrument declared wrong twice in `declared-schemas.md`;
  moves 15 plan goldens for a no-op cast on the CPU.
- **Recipe writer wraps the wire node in a `CastExprNode`** — same model, hidden from the plan text,
  and moves `recipe-payloads.txt` digests for a fix that changes no plan.
- **Generic cast to `return_type` at the end of `build_column_scalar_fn`** — hides the next kernel
  whose type is wrong for a reason that is not width (the class the operator harness exists to
  catch), and is wrong for decimals unless it also reads `return_decimal_scale`; `abs` over a
  decimal would be rescaled to scale 0. Narrow now, widen only if the catalog shows a second arm.
- **Declare `Int16` in the plan** — makes the CPU narrow DataFusion's `Int32` and abandons "the
  declared type is DataFusion's coercion", the rule the whole type story rests on.
- **Replace the kernel** — no cuDF datetime API answers in `INT32`; `extract_year` was `INT16` too
  and is gone in 26.02.

## 5. Minimum corpus query

```sql
SELECT extract(year FROM o_orderdate) AS o_year FROM orders WHERE o_orderkey < 8
```

tpch sf1. Seven rows (order keys 1–7 exist). Plans today at all five modes — scan, filter,
project, unload; no join, no shuffle, so neither #152 nor #183 nor #187 is in the way — and the
CPU passes at all five. On a device today it is refused at every mode by `GpuExport::unload`
(`gpu_backend/mod.rs:179`): `GpuUnload lane 0: the exported stream is not the sink's rows: column
types must match schema types, expected Int32 but found Int16 at column index 0`. After the fix it
passes at all five on both backends.

Worth adding to the corpus as `testdata/tpch-queries/extract-year.sql` with a `corpus_query!`
line declaring all five cpu and all five gpu modes, `data_fusion_exact, golden_exact`: it is the
only way this fix produces a green device cell (§6), and it would be the first device cell with a
scalar function in it. Cost: one sql file, one corpus line, one registry row, new sections in
the five `.plans.txt`, five `.cpu.txt` + `.cost.txt`, one `.result.txt` — all CPU-authored under
`UPDATE_CANONICAL=1`, no existing byte moves, no DuckDB oracle needed (synthetic queries carry
none; `filter-project` has no `.duckdb_cost.txt`). `PAYLOAD_QUERIES` need not change: the cover is
over node kinds and call shapes, not expression kinds.

## 6. Cells re-enabled

- **From the existing corpus: none turn green.** `tpch/q8` `gpu_tp1_single` sheds #191 and stops on
  the next cause, which by reading the CPU golden is #185: `tp1-single-mini.cpu.txt:279-282` shows
  `GpuAggregateBatches` with `in_rows=[[4]]` over a `GpuAggregate` that emitted six batches
  (`batch_rows=[[0,0,1,2,1,0]]`), and the device reports that node's own output (2). Behind that
  is a wall no ticket in `00-tickets.md` names: the CPU's hash join emits DataFusion's 8192-row
  chunks (`tp1-single-mini.cpu.txt:328-336`, `batch_rows=[[8192,8192,8192,8192,8192,2733]]`;
  `cpu_backend/join.rs:236-249` returns whatever `run_node` yields) where the device emits one
  handle per probe batch, so the section's `batch_rows` lists disagree at every join whose
  per-call output exceeds 8192 rows, and every node above them. `assert_section`
  (`tests/common/corpus_golden.rs:111`) compares the whole section byte for byte; the first
  differing line is the aggregate's, which is why the rollout recorded #185 and not this. Same for
  `tpch/q3` and `q14` at `tp1-single`. Not this ticket's to fix; named so the registry edit after
  the device run does not attribute it to #185 by default.
- **Walls removed for others**: `tpch/q7` and `tpch/q9` at `tp1-single` would have landed on #191
  after #183; they no longer will. At the four tp4/rowgroup modes of q7/q8/q9, #152 stays first.
- **New cells, if §5's query joins the corpus**: `tpch/extract_year` × 5 cpu + 5 gpu, all green.
- **Stays off, other tickets**: everything else on the q8 row (#152 at four modes).

## 7. Risks and unknowns

- **Not run on a device.** `cudf::cast(INT16 → INT32)` on a column with a null mask is routine, but
  the assertion that q8 then reaches the section compare, and what that compare says, is a device
  run's to make. §6's prediction of #185-then-batch-split is from reading the CPU golden.
- **The Rust test's `Date32` literal path** rests on `build_scalar`'s `Date32` arm and
  `make_column_from_scalar` over a `timestamp_scalar<timestamp_D>` — both read, neither exercised
  by an existing device test with a date. If it trips, the gtest form with
  `CAST(r_regionkey AS Date32)` is the fallback (`cudf::cast` numeric → timestamp is supported).
- **26.02** only needs to compile: `extract_datetime_component` and `cudf::cast` exist in both.
- **Value agreement for q8's `mkt_share`** (a `sum/sum` decimal divide with declared `(38,8)`) is
  untested on a device for this query; the recipe walk's `avg` case passes the same shape, so it is
  expected to match, but it is a separate question from the type.
- **Whether the 8192-chunk disagreement is already known** under some name I did not find: grep of
  `llm-wiki/` for `8192` finds only a benchmark table. If it is not, it wants a ticket after the
  device run confirms it, not before.

## 8. Complexity

**S.** Two production lines and a four-line comment in `cpp/src/expr.cpp`; one ~30-line device
test; a comment reword in `test_inc2_conformance.rs`; one `corpus_cases.inc` comment and one
registry cell after a device run; three short edits to `architecture.md` and two counts in
`build-test.md`; the ticket to the archive at close. No frozen surface changes — no C ABI symbol,
no `.fbs` field, no wire byte, no declared-schema contract. No golden regenerates; adding §5's query
creates new sections only and would move the size to S+ for the regeneration round.
