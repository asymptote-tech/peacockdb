# `date_part` answers in the type the wire names

Kind: production

**This task closes [#191](active-tickets.md#t191)** — `extract(year …)` is `Int32` in DataFusion
and on the CPU, INT16 from cuDF, and the device hands the INT16 up unchanged. Fourth of chain B.

## Why it happens

`expr.cpp:641-660`'s `date_part` arm returns `cudf::datetime::extract_datetime_component`'s
column as it comes — INT16 for every field — although the wire already says what to produce:
`ScalarFunctionExprNode.return_type` (`flatbuffers/gpu_plan.fbs:204`, "used by the
column-producing executor to allocate the result column"). In `tpch/q8` the year is born
`Int16` at the bottom `GpuProject`, rides as a group key through eight nodes that all declare
`Int32` and none of which look, and is refused at the sink because the query projects it. A
query that grouped by year without outputting it would run the whole plan unseen.

This is the opposite of [#187](active-tickets.md#t187): there the device *widens* a label it
cannot hold; here it *narrows* a value it was told the width of. And unlike
[#163](../tickets.md#t163) the declaration is right — `extract` is `Int32` to the user — so the
producer is fixed, not the label.

## The work

1. **Fix.** The `date_part` arm ends with `cudf::cast(component, cudf::data_type{fb_to_type_id(
   node->return_type())})` when the component's type differs from the declared one. All six
   fields go through the same line. A `return_type` that is not an integer type is a
   `runtime_error` naming the function — the wire promised an allocation type, not a request.
2. **Tests, three levels.**
   - `cpp/tests/gpu/test_plan_executor.cpp`: `tpch.minimal` has no date column, so a
     `CudfProject` casts `nation`'s `n_nationkey` to `Date32` first; above it a `CudfProject`
     with one `ScalarFunctionExprNode {name: "date_part", args: ["YEAR", col 0], return_type:
     Int32}`, asserting the output column's `type().id() == INT32` and the values; the same
     shape for `MONTH` and `DAY`. Red before the fix.
   - `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`: a `GpuProject` over the file's
     `input()` — `synthetic::schema()` carries `d: Date32` at index 6 — with `date_part(YEAR,
     d) as y` declared `Int32`, `run_both(...).same(Order::AsEmitted)`. Written first as the
     `bug_` pin whose refusal names `y: Int32 vs Int16`, then flipped to the green case by the
     fix. The green case's name says what it proves; the pin's name goes with the ticket.
   - The corpus: `tpch/q7`, `q8`, `q9` at `tp1_single`. `q7` and `q9` gain `191` in the registry
     first — the survey (§3) showed the class on them without the ticket.
3. **The neighbours, looked at and not fixed.** Every other arm of `expr.cpp`'s scalar dispatch
   is read against its `return_type`; a mismatch found is a ticket with the query or case that
   reaches it, not a second fix in this task. The detail file lists each arm and its verdict.

## Scope

| file | change |
|---|---|
| `cpp/src/expr.cpp` | the cast at the end of the `date_part` arm |
| `cpp/tests/gpu/test_plan_executor.cpp` | three cases |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the pin, then the green case |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | `q7`, `q9` gain `191`; the three rows' `tp1_single` gpu cells |
| `llm-wiki/tasks/active-tickets.md`, `tickets.md`, `build-test.md` | #191 closed; any neighbour tickets; counts |

Component-level API: none. No wire change, no ABI change, no Rust production code.

## Restriction

`date_part` alone. No change to how the planner emits the function, no Rust-side cast, no
change to any other scalar arm. No golden moves: the plan already declares `Int32`.

## Registry

`tpch/q7`, `q8`, `q9` at `tp1_single`; the other modes for those that pass. Each is enabled
where values match, or keeps its next ticket; `191` is struck from a row only when no disabled
cell in it is left without a ticket (`registry.rs:229-240`).

## Verification bar

- C++: the three plan-executor cases red before, green after, on a device.
- device: the harness with the case green and the pin gone; the three rows.
- rust-only: `--lib` and `test_module_layout` unchanged.

## Device workflow

`build-test-shadgpu.sh`, one cycle.

## Completeness signoff — 2026-09-17

Solved under its constraints: the `date_part` arm alone changed, casting cuDF's INT16
component to the `return_type` the wire names and refusing a non-integer one by name; three
plan-executor cases red before and green after on the device; the harness pin flipped to the
positive case; no Rust-side cast, no other arm, no golden moved. `tpch/q7`, `q8`, `q9` at
`tp1-single` now run the whole device plan and stop on the cpu's join batching beneath the
merge (#220); `191` is on no row. The neighbour survey read every other arm and found one
mismatch, `round` over `Float32` (#221, pinned), and three refusals of shapes the cpu answers
(#222–#224). Shortcuts or bandaids: none. Deviations, each on record: the spec's gtest date
came from `Date32` literals through a CASE, since cuDF makes no date from an integer (#224);
#221 sits in `tickets.md`, disabling no cell. `done` waits on CI.
