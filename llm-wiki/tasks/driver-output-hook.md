# The driver lets a caller look at every batch a node emits

Kind: production

**Testing infrastructure; closes no ticket.** One production change — the driver accepts a hook
called on every emitted batch — and everything that uses it lives under test: a validator that
holds each device batch to its node's declared schema, driver unit tests over a mock, and a
corpus switch that turns the validator on for every cell already enabled. Sixth and last of
chain B; uses [`device-schema-harness.md`](device-schema-harness.md)'s comparator.

## Why

Between the scan and the sink nothing on the device compares a produced type to a declared one
([`reports/sink-divergence.md`](../reports/sink-divergence.md) §4: a divergence born at a
`GpuProject` rides through eight nodes and surfaces only if the column is projected out). The
harness now checks nodes one at a time; the corpus runs whole plans. A hook at the driver's one
emission site is what lets the corpus check every node of every enabled query at every mode, for
the price of one schema fetch per batch — and only when asked.

## The work

1. **The hook.** `executor/driver/partitioned.rs`: `Driver<'a, B>` holds
   `hook: Option<OutputHook<'a, B>>` where

       pub(crate) type OutputHook<'a, B> =
           Box<dyn FnMut(NodeId, usize, &<B as Backend>::Batch) -> Result<(), String> + 'a>;

   called at the two emission arms (`:311-313` device, `:322-323` host) after
   `record_emitted` and before the batch is queued or released. `Err(message)` becomes a
   `StepError` naming node, lane and the message — the run fails there as any call failure
   does. `driver::run`, `partitioned::run` and `single_partition`'s equivalent gain a
   `hook: Option<OutputHook>` parameter; every existing caller passes `None`. With `None`
   nothing changes: no allocation, no branch beyond the `Option`.
2. **The validator**, `cfg(test)`, in `test_support/schema_validation.rs`:
   `schema_validator<'a>(index: &'a PlanIndex) -> OutputHook<'a, GpuBackend>` — for each
   `GpuBatch` it calls `Device::schema_of(handle)` and `schema_divergence(node's declared
   schema, actual)`, returning the divergence string on mismatch. A CPU-backend flavour
   compares `batch.schema()` under the same projection, for the mock tests and symmetry;
   the CPU backend's own `declared_as` already holds every stage, so it is not installed in
   the cpu corpus.
3. **Driver unit tests**, `executor/driver/tests/hook.rs`, rust-only over `tests/mock.rs`:
   a `MockExec` whose output batch has one column retyped — the run fails at that node with the
   validator's message naming the column; the same plan with a matching mock — the hook is
   called once per emitted batch (counted) and the run passes; the same plan with `None` —
   passes, hook never constructed. A fourth: a hook that errors on the third batch fails the
   run at the third batch's node and lane.
4. **The corpus switch.** `corpus_query!` gains a trailing argument,
   `schema_validation_enabled | schema_validation_disabled`, in both `test_gpu_corpus.rs` and
   `test_cpu_corpus.rs` (the cpu macro ignores it); `gpu_case` installs `schema_validator`
   when enabled. `corpus_cases.inc`: every row whose gpu modes are not `none` says
   `schema_validation_enabled`. A cell that then fails on schema while its values match gets
   `schema_validation_disabled` with `// #NNN` on its line and a ticket — the registry cell
   stays `enabled`, so coverage does not move. Expected: none, since tasks 1–4 cleared the
   known classes; the switch is for the queries a later rollout enables.
5. **Verification on a device.** The enabled corpus at its enabled modes with validation on —
   CI's gpu job as it stands. Plus, once and not committed: an ad hoc corpus query whose
   hand-built plan declares a wrong type at one `GpuProject`, run to show the hook failing that
   node by name; the run's output goes into the detail file.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/executor/driver/partitioned.rs`, `mod.rs`, `single_partition.rs` | the hook |
| `peacockdb-core/src/executor/driver/tests/hook.rs` (new), `tests/mock.rs` | the mock tests; a retypable mock output |
| `peacockdb-core/src/test_support/schema_validation.rs` (new), `test_support/mod.rs` | the validator |
| `peacockdb-core/tests/test_gpu_corpus.rs`, `test_cpu_corpus.rs`, `tests/common/corpus_cases.inc` | the argument; every enabled row |
| `llm-wiki/architecture.md`, `build-test.md` | the driver section names the hook; counts |

Component-level API: `driver::run` and `partitioned::run` gain a parameter (`pub(crate)`);
`OutputHook` type. No wire change, no ABI change, no facade change.

## Restriction

The hook is the only production change. No validation code outside `cfg(test)`; no default
hook; no change to what the driver does with a batch beyond calling the hook first. The ad hoc
trigger query is not committed.

## Verification bar

- rust-only: `--lib` — the four mock tests, `test_module_layout`, `test_ci_coverage` (the new
  argument must not break the registry guard).
- device: the enabled corpus at `tp1_single` and `tp4_single` with validation on, green; the ad
  hoc trigger red at its node, recorded.
- `git diff` of `executor/driver/` shows the hook and nothing else.

## Device workflow

`build-test-shadgpu.sh`, the corpus binaries; one cycle plus the ad hoc run.
