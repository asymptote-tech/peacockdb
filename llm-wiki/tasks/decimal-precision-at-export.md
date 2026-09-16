# The export is told the precision it cannot know

Kind: production

**This task closes [#187](active-tickets.md#t187)** — the device exports every decimal at
precision 38 whatever the plan declared — by handing the declared precision to the export, and
it removes the wire's dead `output_schema` on the same rebuild. Second of chain B, after
[`utf8-everywhere.md`](utf8-everywhere.md).

## Why it happens

cuDF's `data_type` is `{type_id, scale}` (`types.hpp`, "only `_fixed_point_scale` is stored"),
the same in 25.02 and 26.02. Precision is not a field of the column; on export
`to_arrow_schema` takes it from `column_metadata::precision` and, when that is unset, from the
width's maximum — 38 for DECIMAL128 (`to_arrow_schema.cpp:95`). `export_table_to_ipc`
(`cpp/src/gpu_executor.cpp:54-56`) fills `column_metadata` with the name alone, and
`peacock_result_from_handle(handle, offset, length)` is never told a declaration. So
`Decimal128(15, 2)` leaves as `decimal128(38, 2)`: same 16-byte values, same scale, a different
number in the schema message — 53 queries, 105 columns, twelve declared precisions in the
survey, scale preserved in every one.

Nothing narrows and nothing needs checking: DataFusion sizes every result precision to hold its
value, and a value that did not fit would be a wrong value, which the goldens catch. The label is
what is wrong, and the export is the only place that writes it.

## The work

1. **ABI.** `peacock_result_from_handle` gains two parameters:
   `const int32_t* decimal_precisions, uint64_t n_columns` — one entry per exported column,
   `0` meaning no declaration. `int32_t` because `column_metadata::precision` is
   `std::optional<int32_t>`. `cpp/include/peacock_gpu.h:203`, `gpu_executor.cpp:292`,
   `peacockdb-ffi/src/lib.rs:151`.
2. **Export.** `export_table_to_ipc` takes the array and sets `column_metadata{name, precision}`
   for every non-zero entry. The DECIMAL32/64 widening loop (marked dead at `:57-58`) and its
   comment go. Two hard failures replace it, each a `runtime_error` naming the column: a
   fixed_point column whose `type_id != DECIMAL128`; a non-zero precision given for a column
   that is not DECIMAL128. `n_columns` must equal the table's column count or the call fails.
3. **Callers.** `GpuExport::unload` (`executor/gpu_backend/mod.rs:207-250`) builds the array
   from `self.schema` — `Decimal128(p, _)` → `p`, else `0` — and passes it; its
   `concat_batches` check is unchanged and starts passing. `Device::fetch`
   (`tests/gpu_tests/device.rs:85-112`) takes a `&Schema` for the declaration and builds the
   same array, or passes zeros when the caller has none.
4. **The rule.** `plan/validate.rs`: every decimal in a node schema, literal, `Cast` target or
   scalar return type is `Decimal128`; `Decimal256` is refused now, `Decimal32`/`Decimal64`
   automatically if arrow ever adds them. Unit test with a `Decimal256` column. `expr.cpp`'s
   `fb_to_type_id` maps `Decimal128` alone; no DECIMAL32/64 arm anywhere in `cpp/src` outside
   the loader's widening (`scan.cpp:98-108`), which stays because cuDF's reader is where 64 is
   born.
5. **Wire cleanup, same rebuild.** `output_schema` leaves `PlanNode` and `CudfUnion` in
   `gpu_plan.fbs` (`:526`, `:608`); `wire/writer.rs:114,143` drop the `None`; `union.cpp:35-50`'s
   guarded block goes, and its comment's reason — branches are planned independently, so a
   column's cuDF type can differ per branch — moves to `architecture.md`'s union row (`:829`)
   beside the statement that the planner's per-branch cast projects are what align them;
   `test_plan_executor.cpp`'s `make_plan_node` loses its unused `schema` parameter.

## Scope

| file | change |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | the two parameters; precision into metadata; the two hard failures; widening removed |
| `peacockdb-ffi/src/lib.rs` | extern signature |
| `peacockdb-core/src/executor/gpu_backend/mod.rs` | `unload` builds and passes the array |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `fetch` takes the declaration |
| `peacockdb-core/src/plan/validate.rs`, `validate/tests.rs` | the decimal rule and its test |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/writer.rs`, `cpp/src/operators/union.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `output_schema` gone |
| `cpp/tests/gpu/test_plan_executor.cpp` | an export with a declared precision reads back at it; an export of a DECIMAL64 table fails naming the column |
| `llm-wiki/architecture.md` | union row; "every cast is explicit" gains the sentence that the device type is `{type_id, scale}` and precision is a label the export is told |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md` | counts, #187 closed |

Component-level API: one ABI symbol's signature (`peacock_result_from_handle`); the wire loses
two fields. No facade item changes.

## Restriction

No cast, no range check, no relabel on the Rust side after decode. The scale still comes from
the column and is still compared at the sink. Any other schema difference keeps today's
"declared vs exported" error.

## Registry

The survey's 53 decimal queries at `tp1_single`, the same rule as the task before: enabled with
`187` struck where sink and values pass; a ticket where values fail. Cells that carried both
classes are decided here. "`Decimal128(38, 4)` agrees for the wrong reason" (survey §4) is a
real agreement after this.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, the validation test red then green.
- C++: `ctest -L cpu` and the two new plan-executor cases on a device.
- device: the harness green (`Device::fetch` with declarations); the rollout at `tp1_single`.
- `grep -rn "output_schema" cpp peacockdb-core flatbuffers` returns nothing; `grep -rn
  "DECIMAL32\|DECIMAL64" cpp/src` returns `scan.cpp` alone.

## Device workflow

`build-test-shadgpu.sh`. Staged binaries from before this task are incompatible with the plan
after it — both sides rebuild together, as always.
