# Strings are Utf8 from the leaf up

Kind: production

**This task closes [#183](active-tickets.md#t183)** — the sink declares `Utf8View` where the
device exports `Utf8` — by removing the declaration rather than converting the export. First of
chain B; the five tasks after it assume no view type exists in any plan.

## Why it happens

`Utf8View` enters a plan in exactly one place: DataFusion 45's parquet option
`schema_force_view_types`, default `true`, under which `ListingOptions::infer_schema`
(`peacockdb-core/src/lib.rs:72-82`) types every string column as `Utf8View` before an operator
exists. Everything downstream is conditional on that input: `string_coercion` picks `Utf8View`
only "if Utf8View is in any side" (`datafusion-expr-common/src/type_coercion/binary.rs:1148`),
every string function that can return `Utf8View` branches on `arg_types[0] == Utf8View`, and
the literal rewrite `serialize.rs:72-77` describes is the coercion following the column. cuDF has
one `STRING` layout, exports it as `utf8`, and cannot import a view array at all
(`from_arrow_host.cu:431`, `harness_cases.rs:117-121`). So the plan declares, from the leaf up, a
container the device never holds, and the two meet only at the sink — 76 queries and 204 columns
in [`reports/sink-divergence.md`](../reports/sink-divergence.md).

The corpus's strings enter through cuDF's parquet reader; the view type was never data.

## The work

1. **The option.** `build_session_state` (`lib.rs:38-48`) sets
   `config.options_mut().execution.parquet.schema_force_view_types = false` beside
   `target_partitions`, with a comment naming this task and the reason. After it every plan
   declares `Utf8` (or `LargeUtf8`, which the reader does not produce) and no coercion or
   function can bring a view back.
2. **The rule.** `plan/validate.rs` gains a structural rule run with the others: a node schema,
   a literal, a `Cast` target or a scalar function's return type of `Utf8View`, `BinaryView`,
   `ListView` or `LargeListView` is `PlanError::Invalid`, naming the node and column. Its unit
   test in `plan/validate/tests.rs` builds a node declaring `Utf8View` and asserts the refusal;
   a second asserts a `Cast` to `Utf8View` is refused. The rule is what keeps a DataFusion bump
   ([#23](../tickets.md#t23)'s note) from reintroducing the type silently.
3. **Drop every handling site**, since after 1 and 2 they are unreachable code that reads as
   support: `wire/serialize.rs` `Utf8View` literal arm (`:72-77`) and type arms (`:130-131`);
   `Utf8View` and `BinaryView` from `fb::DataType` in `flatbuffers/gpu_plan.fbs` (the wire
   moves; Rust and C++ ship together); `cpp/src/expr.cpp` arms at `:89`, `:228`, `:333`, `:336`,
   `:478`; `wire/fb_text.rs:348`; `cpu_backend/spark_partitioning.rs:64`'s cast;
   `common.rs:35,115` sizing arms; `test_support/result_text.rs:92`'s note;
   `tests/gpu_tests/harness_cases.rs`'s `declaring_view_strings` and its `bug_` pin for #183.
   join-cases and aggregate-cases retire their own view cases in their Task 8 before this
   task builds; if any survive, this task removes them and says so in the detail file.
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
| `flatbuffers/gpu_plan.fbs` | `Utf8View`, `BinaryView` removed from `DataType`; generated code on both sides |
| `cpp/src/expr.cpp` | view arms removed |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs`, `common.rs`, `test_support/result_text.rs` | the cast, the sizing arms, the note |
| `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` | `declaring_view_strings` and the #183 pin removed |
| `peacockdb-core/src/plan_text/tests.rs`, `planner/translator/schema_tests.rs` | re-asserted on `Utf8` |
| `testdata/goldens/**/*.plans.txt`, `recipe-payloads.txt` | regenerated |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md`, `tickets.md` | counts, #183 closed, #23 unchanged |

Component-level API: `fb::DataType` loses two values (wire). No ABI symbol changes. No facade
item changes.

## Restriction

No cast anywhere: the fix is that nothing needs one. No change to any string function, to
`from_arrow`, to the export. A `Utf8View` that survives the option is a finding — a ticket and
the rule's refusal — not a case for a cast.

## Registry

The survey's 76 queries carrying the string class, at `tp1_single`: each `gpu_*` cell whose sink
now passes and whose values match its golden is enabled with `183` struck from its row; each
that fails on values keeps `disabled` and gets a ticket naming what the values showed. Cells
that also carry the decimal class wait for the next task. The registry is what the device run
proves, nothing more.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, `test_golden_format`, the plan-golden regen, the
  two validation tests red before the rule and green after.
- device: the operator harness (`_cases`) green with the retired pins gone; the corpus rollout
  above at `tp1_single`.
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src cpp/src flatbuffers testdata/goldens`
  returns only the validation rule and its tests.

## Device workflow

`build-test-shadgpu.sh`. One cycle for the harness, one for the rollout.
