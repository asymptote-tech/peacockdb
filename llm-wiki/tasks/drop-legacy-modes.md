# Drop the legacy execution modes

Delete the six legacy execution modes — CPU full-table at tp1 and tp8, CPU partitioned,
GPU all-at-once, GPU full-table, GPU partitioned — with the planning they rest on, the
tests, the goldens, the benchmark records and the widget columns that describe them. What
stays is the batch-partitioned planner and its two backends.

## What goes

- **Rust**: `executors/` (five mode classes, the node-by-node driver, the streaming
  driver, the resident enforcer), `operators/` (the 16 `Gpu*Exec` wrappers and their
  serializers), `gpu_rule.rs` (both physical optimizer rules), `plan_serializer.rs`,
  `resident.rs`, `cpu_executor.rs`, `gpu_executor.rs`, `node_executor.rs`, and
  `CpuExecutor` in `lib.rs`.
- **C++**: `execute_plan.cpp` and the `peacock_execute` ABI entry point. `execute_node`
  stops being a recursive driver and becomes what it always was on the node path: the
  resolver that hands an operator its next already-resident input.
- **Tests**: the fourteen legacy targets and the harness that only they used
  (`common/exec_mode.rs`, `common/benchmark.rs`, `common/gpu_cases.inc`).
- **Goldens**: every `.plan.txt`, every `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`
  and `plan_bytes.sha256`; `testdata/benchmark-results/` with the harness that wrote it.
- **Widget**: the legacy table, the six legacy registry columns, and the second PR
  comment the four tables needed.

## What is kept, and where it moved

The batch-partitioned side reached into the legacy tree in four places, so each moves
rather than dies: the per-node DataFusion runner (`cpu_backend/single_node.rs`), the three
Arrow-to-wire helpers the recipe writers share (`recipe/wire.rs`), `parquet_table_name`
(`parquet_meta.rs`), and the small-table threshold, which two test files spelled
separately and is now `plan::SMALL_TABLE_BYTES`.

## Coverage this removes and does not replace

Two guards were defined as "the recipe writer against the legacy writer" and cannot
survive it: `every_field_the_legacy_writer_sets_is_set_here_or_declared_a_difference` and
the seven expression cases in `recipe/expr_writer/tests.rs`. What they added over the rest
was a second, independent producer of the same bytes; the payload digest still pins what
this writer writes. Say so in the PR rather than quietly.

The C++ operator gtests keep their coverage: `test_plan_executor.cpp` drives its
hand-built plans through `NodeSession` node by node, the way the driver does.

## Verification bar

CPU: the rust-only suite and `ctest -L cpu` locally, `cargo test -p cost-report`.
GPU: the five staged targets on shad-gpu. Both green before the PR.
