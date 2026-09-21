# The export is told the precision it cannot know

Kind: production

**This task closes [#187](active-tickets.md#t187)** — the device exports every decimal at
precision 38 whatever the plan declared — by handing the declared precision to the export. It
is also the chain's one wire and header rebuild: the `Utf8View`/`BinaryView` values the
previous task made dead leave the wire with their C++ arms, the dead `output_schema` fields go,
and `peacock_handle_schema`, which `device-schema-harness` needs, is declared here so the
header moves once. Second of chain B, after [`utf8-everywhere.md`](utf8-everywhere.md).

## Why it happens

cuDF's `data_type` is `{type_id, scale}` (`types.hpp`, "only `_fixed_point_scale` is stored"),
the same in 25.02 and 26.02. Precision is not a field of the column; on export
`to_arrow_schema` writes the width's maximum — 38 for DECIMAL128 — unless told otherwise, and
the way to tell it differs by version: 26.02's `column_metadata` has a `precision` field
(`rapids/include/cudf/interop.hpp:110`), 25.02's has `name` and `children_meta` only
(`rapids-cuda-12.2/include/cudf/interop.hpp:108-119`), and shad-gpu runs 25.02. So the label is
set where both versions agree: on the Arrow schema after `arrow::ImportSchema`, before the
batch is imported against it. `export_table_to_ipc` (`cpp/src/gpu_executor.cpp:51-80`) never
does that, and `peacock_result_from_handle(handle, offset, length)` is never told a
declaration. So `Decimal128(15, 2)` leaves as `decimal128(38, 2)`: same 16-byte values, same
scale, a different number in the schema message — 53 queries, 105 columns, twelve declared
precisions in the survey, scale preserved in every one.

Nothing narrows and nothing needs checking: DataFusion sizes every result precision to hold its
value, and a value that did not fit would be a wrong value, which the goldens catch. The label is
what is wrong, and the export is the only place that writes it.

## The work

1. **ABI.** `peacock_result_from_handle` gains two parameters:
   `const int32_t* decimal_precisions, uint64_t n_columns` — one entry per exported column,
   `0` meaning no declaration. `int32_t` because `column_metadata::precision` is
   `std::optional<int32_t>`. `cpp/include/peacock_gpu.h:203`, `gpu_executor.cpp:292`,
   `peacockdb-ffi/src/lib.rs:151`.
2. **Export.** `export_table_to_ipc` takes the array. After `arrow::ImportSchema` and before
   `ImportRecordBatch`, every column with a non-zero entry has its field's type replaced by
   `arrow::decimal128(precision, scale)` with the scale read off the cuDF column — the same
   buffers, only the label — so the stream's schema message says what the plan declared on
   25.02 and 26.02 alike. The DECIMAL32/64 widening loop (marked dead at `:57-58`) and its
   comment go. Two hard failures replace it, each a `runtime_error` naming the column: a
   fixed_point column whose `type_id != DECIMAL128`; a non-zero precision for a column that
   is not DECIMAL128. `n_columns` must equal the table's column count or the call fails.
   `export_table_to_ipc` stops being `static` and is declared in
   `cpp/src/plan_executor_internal.h`, so the C++ tests can hand it a table no C-API path can
   build — a DECIMAL64 column — and see the refusal.
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
5. **Wire and header, one rebuild.** `output_schema` leaves `PlanNode` and `CudfUnion` in
   `gpu_plan.fbs` (`:526`, `:608`); `wire/writer.rs:114,143` drop the `None`; `union.cpp:35-50`'s
   guarded block goes, and its comment's reason — branches are planned independently, so a
   column's cuDF type can differ per branch — moves to `architecture.md`'s union row (`:829`)
   beside the statement that the planner's per-branch cast projects are what align them;
   `test_plan_executor.cpp`'s `make_plan_node` loses its unused `schema` parameter;
   `node_session.cpp:275`'s comment stops naming `output_schema`. On the same rebuild the
   `Utf8View` and `BinaryView` values leave `fb::DataType`, `cpp/src/expr.cpp`'s five view arms
   (`:89`, `:228`, `:333`, `:336`, `:478`) go, and `test_plan_executor.cpp`'s 45
   `DataType_Utf8View` uses become `DataType_Utf8`. And the header gains one read-only
   function, for the harness two tasks on:

       int peacock_handle_schema(peacock_executor_t*, uint64_t handle,
                                 uint8_t** out_ipc, uint64_t* out_len);

   the Arrow IPC stream holding the handle's schema message alone (`to_arrow_schema` on its
   view, no rows), freed with `peacock_result_free`, 0 on success; extern in
   `peacockdb-ffi/src/lib.rs`; two gtest cases (a scan's handle reads back four typed fields
   and no batch; an unknown handle fails). Nothing in Rust calls it yet.

## Scope

| file | change |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `cpp/src/plan_executor_internal.h` | the two parameters; the label set on the imported schema; the two hard failures; widening removed; `export_table_to_ipc` exposed to tests; `peacock_handle_schema` |
| `peacockdb-ffi/src/lib.rs` | both extern signatures |
| `peacockdb-core/src/common.rs`, `executor/gpu_backend/mod.rs` | `declared_precisions` (feature-independent, unit-tested rust-only); `unload` builds and passes the array |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `fetch` takes the declaration |
| `peacockdb-core/src/plan/validate.rs`, `validate/tests.rs` | the decimal rule and its test |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/writer.rs`, `cpp/src/operators/union.cpp`, `cpp/src/node_session.cpp`, `cpp/src/expr.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `output_schema` gone; `Utf8View`/`BinaryView` gone from the wire and its C++ readers |
| `cpp/tests/gpu/test_plan_executor.cpp`, `test_cudf_nodes.cpp` | an export with a declared precision reads back at it; no declaration reads back at 38; a precision on a non-decimal is refused; `export_table_to_ipc` over a hand-built DECIMAL64 column fails naming it; the two `peacock_handle_schema` cases |
| `llm-wiki/architecture.md` | union row; "every cast is explicit" gains the sentence that the device type is `{type_id, scale}` and precision is a label the export is told |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | cells this task proves |
| `llm-wiki/build-test.md`, `active-tickets.md` | counts, #187 closed |

Component-level API: one ABI symbol's signature (`peacock_result_from_handle`) and one new
symbol (`peacock_handle_schema`); the wire loses two fields and two enum values. No facade item
changes.

## Restriction

No cast, no range check, no relabel on the Rust side after decode. The scale still comes from
the column and is still compared at the sink. Any other schema difference keeps today's
"declared vs exported" error.

## Registry

The survey's 53 decimal queries at `tp1_single`, the same rule as the task before: enabled
where sink and values pass; a ticket where values fail; `187` struck from a row only when no
disabled cell in it is left without a ticket (`registry.rs:229-240`). Cells that carried both
classes are decided here. "`Decimal128(38, 4)` agrees for the wrong reason" (survey §4) is a
real agreement after this.

## Verification bar

- rust-only: `--lib`, `test_module_layout`, the validation test red then green.
- C++: `ctest -L cpu` and the two new plan-executor cases on a device.
- device: the harness green (`Device::fetch` with declarations); the rollout at `tp1_single`.
- `grep -rn "output_schema" cpp peacockdb-core flatbuffers` returns nothing; `grep -rn
  "DECIMAL32\|DECIMAL64" cpp/src` returns `scan.cpp` alone; `grep -rn "Utf8View\|BinaryView"
  cpp flatbuffers` returns nothing.

## Device workflow

`build-test-shadgpu.sh`. Staged binaries from before this task are incompatible with the plan
after it — both sides rebuild together, as always.

## Completeness signoff — 2026-09-17

Solved under its constraints: the export is told one precision per column and writes it onto
the imported Arrow schema — same buffers, one label, no version branch; no cast, range check or
relabel on the Rust side; the widening is two refusals naming the column; validation refuses
any decimal but `Decimal128`; the wire lost `output_schema` and the two view values with their
C++ arms, `recipe-payloads.txt` standing since both sat at the tail; `peacock_handle_schema`
declared, externed and pinned by two gtests. 53 queries rolled out at `tp1-single`: five enabled,
the rest on #185, #220 or #163; no sink shows a decimal. Shortcuts or bandaids: `n_columns == 0`
with a null array declares nothing, where the spec's letter required the count to match; the
"`Decimal32`/`Decimal64` automatically" clause is a `Decimal256` match, arrow having neither
variant; `Arrow::arrow_shared` is linked into `peacock_plan_tests` for the read-back; the
DECIMAL64 refusal case sits in `test_plan_executor.cpp`, not the ungated `test_cudf_nodes.cpp`.
Outside scope, reported: `shadgpu-env.sh`'s artifact reader drains cargo's pipe. `done` waits on
CI, the 26.02 leg being the proof the new export compiles there.
