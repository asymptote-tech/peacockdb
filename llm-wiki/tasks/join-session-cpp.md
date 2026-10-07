# A join is a C++ session: built once, probed per batch, finished once

Kind: production

**This task closes no ticket on its own.** It builds the C++ half of
[#136](../tickets/joins.md#t136) (build-side match tracking when the probe side streams),
[#153](../tickets/joins.md#t153) (equi-join residual filter is applied after the outer gather),
[#160](../tickets/joins.md#t160) (nested-loop join supports Inner and Left only),
[#215](../tickets/joins.md#t215) (a left nested-loop join over a predicate the AST cannot take is
refused on the device) and [#63](../tickets/joins.md#t63) (a zero-column placeholder survives the
cross join and shifts every ordinal above it), and removes #154's ten `join.cpp` sites. Each of
those is reachable only through the planner and the Rust executors, so each closes in
join-backend, where a query or a harness case first shows it fixed. Seventh of the join-rewrite
chain.

The design is [`join-rewrite-design.md`](join-rewrite-design.md): §1 (ABI and FlatBuffers), §2 (the
core), §3 (per-case pseudocode naming every cuDF call), §5.6 (tests). This spec does not repeat it.

## The work

1. `flatbuffers/gpu_plan.fbs`: `CudfJoin` (§1.1), plus `chunk_bytes: uint64` (0 means 1 GiB),
   which join-backend writes from the planner's scratch budget. Pad and absent-side schemas may
   carry the `Timestamp*` `DataType` variants repartition-keys added. `CudfHashJoin`,
   `CudfNestedLoopJoin` and `CudfCrossJoin` stay until join-backend stops writing them.
2. `cpp/include/peacock_gpu.h`, `gpu_executor.cpp`, `node_session.cpp`: the four symbols (§1.2),
   join ids in their own map in `NodeSession::Impl`, freed by `end_plan` and the error path; one
   timing region per call.
3. `cpp/src/operators/join.cpp`, rewritten as the session: §3.0–§3.10, with the validation's
   corrections in place (the `hj` for a non-AST semi residual, `contains` not scatter, matches by
   range, the owned distinct-key table, `R IS TRUE`, the per-side residual steps, chunking by
   `inner_join_size` and `conditional_inner_join_size`). The semi family's cross conjuncts are
   ANDed at the cuDF AST level (`NULL_LOGICAL_AND` over `ExprContext`-kept operands, hoisted
   operands as `ast::column_reference`s), so several conjuncts make one `mixed_*` predicate;
   `hj` is built whenever the pairs path can be taken, and a pairs call with no `hj` throws by name
   rather than dereferencing null. Every pairs-producing path checks `inner_join_size` (or
   `conditional_inner_join_size`) against `size_type`'s max, chunking further or refusing by name,
   so 2³¹+k pairs never wrap. No rows-only arm: a zero-column side is
   refused as a planner bug (§4.1's explicit `__rowmarker__`). The old `execute_hash_join`,
   `execute_cross_join` and `execute_nested_loop_join` stay callable until join-backend switches,
   then go with the old tables.
4. The latent finish defect goes with the old code: today's Left/Full finish is a `LeftAnti` with
   NULL = NULL hardcoded, which would drop NULL-key build rows it should pad. The session has no
   finish join. A gtest pins the right answer.
5. `peacockdb-ffi`: the four symbols' declarations, so join-backend can call them.
6. The gtest binary `peacock_join_session_tests` is added to `install(TARGETS …)` and the
   `INSTALL_RPATH` list (`cpp/CMakeLists.txt:321-327`) — CI and shad-gpu run only installed
   `peacock_*_tests` — with a `main()` that installs the RMM pool and statistics adaptor, as
   `test_plan_executor.cpp:2114` does. One allocation check on a session probe covers #154's
   `join.cpp` sites (join-backend closes #154 whole).

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `CudfJoin` |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `cpp/src/node_session.cpp` | the symbols, the join map |
| `cpp/src/operators/join_session.{h,cpp}` (new) | the session — beside `join.cpp`, which keeps serving today's plans until join-backend deletes it |
| `cpp/src/peacock/expr.h`, `cpp/src/expr.cpp` | `JoinFilterColMap` as a span; the AST-level AND of cross conjuncts |
| `cpp/src/plan_executor.h`, `cpp/src/operators/dispatch.cpp` | the session's handle types; `CudfJoin` has no `execute_node` arm |
| `cpp/tests/gpu/test_join_session.cpp` (new), `cpp/CMakeLists.txt` | the matrix of §5.6 |
| `peacockdb-ffi/src/lib.rs` | declarations |
| `llm-wiki/architecture.md`, `build-test.md` | the C ABI's new symbols; counts |

Component-level API: four new C ABI symbols and one fbs table, both additive. No Rust behaviour
change: nothing calls the symbols until join-backend.

## Restriction

C++ and the ABI. No planner, wire-writer, executor or driver change; no golden moves; the corpus
does not change, since nothing reaches the session yet.

## Verification bar

- device: `test_join_session.cpp` green on 25.02 (shad-gpu), the shad-gpu log naming the installed
  binary; the CI's 25.10a leg compiles it.
  Every case of §5.6's matrix and named list, hand-counted.
- device: the existing gpu tier unchanged (the old paths still serve it).

## Device workflow

`build-test-shadgpu.sh`, a cycle per arm group (hash, residual, semi family, nested loop, cross and
empty sides), then one for the whole file.
