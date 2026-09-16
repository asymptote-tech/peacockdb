# Strings are Utf8 from the leaf up

Kind: production

**This task closes [#183](active-tickets.md#t183)** — the sink declares `Utf8View` where the
device exports `Utf8` — by removing the declaration rather than converting the export. First of
chain B; the five tasks after it assume no view type exists in any plan.

## Why it happens

`Utf8View` enters a plan through DataFusion 45's parquet option `schema_force_view_types`,
default `true`. The option that matters is not the session's: `ListingOptions::infer_schema`
reads the `TableParquetOptions` of the `ParquetFormat` it is given, and `lib.rs:71` builds that
format with `ParquetFormat::default()` (`datafusion/src/datasource/file_format/parquet.rs:249-250,
370`); the session config is never consulted for inference. Under it every string column is
`Utf8View` before an operator exists. Everything downstream follows the input: `string_coercion`
picks `Utf8View` only "if Utf8View is in any side"
(`datafusion-expr-common/src/type_coercion/binary.rs:1148`), every string function that can
return `Utf8View` branches on `arg_types[0] == Utf8View`, and the literal rewrite
`serialize.rs:72-77` describes is the coercion following the column. One producer is
unconditional besides the scan: `TypeSignature::Comparable` over all-`Null` arguments
(`greatest`/`least`, `type_coercion/functions.rs:567`) coerces to `Utf8View` — the corpus never
reaches it, and the rule below refuses it if it ever does. cuDF has one `STRING` layout, exports
it as `utf8`, and cannot import a view array at all (`from_arrow_host.cu:431`,
`harness_cases.rs:117-121`). So the plan declares, from the leaf up, a container the device
never holds, and the two meet only at the sink — 76 queries and 204 columns in
[`reports/sink-divergence.md`](../reports/sink-divergence.md).

The corpus's strings enter through cuDF's parquet reader; the view type was never data.

## The work

1. **The option.** `lib.rs:71`: `ParquetFormat::default().with_force_view_types(false)` (the
   existing `.with_enable_pruning(true)` chain), with a comment naming this task and the reason.
   Not the session config — inference never reads it. After it every plan declares `Utf8` (or
   `LargeUtf8`, which the reader does not produce) and no coercion or function can bring a view
   back.
2. **The rule.** `plan/validate.rs` gains a structural rule run with the others: a node schema,
   a literal, a `Cast` target, a binary's out type or a scalar function's return type of
   `Utf8View`, `BinaryView`, `ListView` or `LargeListView` is `PlanError::Invalid`, naming the
   node and column. Node schemas are checked in `walk`; expressions through
   `check_expr_types` in `plan/common.rs`, called at `check_column_refs`'s five node call sites
   (`exec_ops.rs:23,47`, `plan/mod.rs:588,592,596`) and at the join's residual filter, which
   goes through `collect_column_refs` (`join.rs:329`) and would otherwise escape. Unit tests in
   `plan/validate/tests.rs` (under `rooted(...)`, since `validate` refuses a non-sink root
   first): a node declaring `Utf8View`, a `Cast` to `Utf8View`, a residual filter comparing to a
   `Utf8View` literal. The rule is what keeps a DataFusion bump ([#23](../tickets.md#t23)'s
   note) from reintroducing the type silently.
3. **Drop the Rust handling sites**, since after 1 and 2 they are unreachable code that reads
   as support: `wire/serialize.rs` `Utf8View` literal arm (`:72-77`) and type arms
   (`:130-131`); `wire/fb_text.rs:348`; `cpu_backend/spark_partitioning.rs:64`'s cast;
   `common.rs:8`'s `BinaryViewArray` import and the sizing arms at `:35,115`;
   `test_support/result_text.rs:92`'s note; `executor/errors/tests.rs:31,42` re-asserted on
   `Utf8`; `tests/gpu_tests/harness_cases.rs`'s `declaring_view_strings` and its `bug_` pin
   for #183. The wire's `Utf8View`/`BinaryView` values, `cpp/src/expr.cpp`'s five arms and
   `cpp/tests/gpu/test_plan_executor.cpp`'s 45 uses of `DataType_Utf8View` are the next task's,
   which rebuilds the wire once for three reasons; until then those values are dead on the
   wire, which the rule guarantees. Chain E's join-cases and aggregate-cases retire their own
   view cases in their Task 8 before this chain starts; if any survive, this task removes them
   and says so in the detail file.
4. **Goldens.** Every plan golden's `schema=[…]` and every literal tag in `recipe-payloads.txt`
   move `Utf8View → Utf8`; regenerate rust-only and diff — the only change is that word.
   `plan_text/tests.rs:150` and `planner/translator/schema_tests.rs:156,285,305` assert the
   view type from real plans and are re-asserted on `Utf8`.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/lib.rs` | the option |
| `peacockdb-core/src/plan/validate.rs`, `validate/tests.rs` | the rule and its tests |
| `peacockdb-core/src/wire/serialize.rs`, `fb_text.rs` | view arms removed |
| `peacockdb-core/src/plan/common.rs`, `plan/join.rs` | `check_expr_types`; the residual filter checked |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs`, `common.rs`, `test_support/result_text.rs`, `executor/errors/tests.rs` | the cast, the import and sizing arms, the note, two tests re-asserted |
| `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` | `declaring_view_strings` and the #183 pin removed |
| `peacockdb-core/src/plan_text/tests.rs`, `planner/translator/schema_tests.rs` | re-asserted on `Utf8` |
| `testdata/goldens/**/*.plans.txt`, `recipe-payloads.txt` | regenerated |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md`, `tickets.md` | counts, #183 closed, #23 unchanged |

Component-level API: none. No wire change (the next task takes the two enum values), no ABI
symbol changes, no facade item changes.

## Restriction

No cast anywhere: the fix is that nothing needs one. No change to any string function, to
`from_arrow`, to the export. A `Utf8View` that survives the option is a finding — a ticket and
the rule's refusal — not a case for a cast.

## Registry

The survey's 76 queries carrying the string class, at `tp1_single`: each `gpu_*` cell whose sink
now passes and whose values match its golden is enabled; each that fails on values keeps
`disabled` and gets a ticket naming what the values showed. Cells that also carry the decimal
class wait for the next task. `183` is struck from a row only when no disabled cell in it is
left unexplained — `registry.rs:229-240` requires a ticket on any row with a disabled cell, so
the number stays until the other modes are enabled or another ticket names them. The registry
is what the device run proves, nothing more.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, `test_golden_format`, the plan-golden regen, the
  two validation tests red before the rule and green after.
- device: the operator harness (`_cases`) green with the retired pins gone; the corpus rollout
  above at `tp1_single`.
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src testdata/goldens` returns only
  `plan/common.rs`'s rule and `plan/validate/tests.rs`; `cpp/` and `flatbuffers/` are the next
  task's grep.

## Device workflow

`build-test-shadgpu.sh`. One cycle for the harness, one for the rollout.
