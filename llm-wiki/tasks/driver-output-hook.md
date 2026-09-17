# The driver lets a caller look at every batch a node emits

Kind: production

**Testing infrastructure; closes no ticket.** One production change — the driver accepts a hook
called on every emitted batch — and everything that uses it lives under the `test-support`
feature: a validator that holds each device batch to its node's declared schema, driver unit
tests over a mock, and a corpus switch that turns the validator on for every cell already
enabled. Sixth and last of chain B; uses [`device-schema-harness.md`](device-schema-harness.md)'s
comparator and reader.

## Why

Between the scan and the sink nothing on the device compares a produced type to a declared one
([`reports/sink-divergence.md`](../reports/sink-divergence.md) §4: a divergence born at a
`GpuProject` rides through eight nodes and surfaces only if the column is projected out). The
harness now checks nodes one at a time; the corpus runs whole plans. A hook at the driver's
emission sites is what lets the corpus check every node of every enabled query at every mode,
for the price of one schema fetch per batch — and only when asked.

## The work

1. **The hook.** `executor/driver/partitioned.rs`: `Driver<'a, B>` holds
   `hook: Option<OutputHook<'a, B>>` where

       pub(crate) type OutputHook<'a, B> =
           Box<dyn FnMut(usize /*node*/, usize /*lane*/, &<B as Backend>::Batch) -> Result<(), String> + 'a>;

   called wherever a node's *own* output is queued: the lane's device and host arms
   (`:314`, `:324`), the emitter's per-lane outputs (`:391`), and the partition accumulator's
   (`:448`) — after `record_emitted`, before the queue or release. The forwarder at `:495`
   moves a child's batch between queues and is not an emission; it is left alone. `Err(message)`
   becomes a `StepError` naming node, lane and the message — the run fails there as a call
   failure does. `driver::run_with_hook` and `partitioned::run_with_hook` are added beside
   `run` (which calls them with `None`), and `executor::run_with_hook` is the `pub(crate)`
   facade — `pub` would need a `SURFACE` entry (`test_module_layout/visibility.rs:37`) and no
   caller outside the crate exists. With `None` nothing changes: no allocation, no branch
   beyond the `Option`. `single_partition.rs` has no `run` of its own and is untouched.
2. **The validator**, in `test_support/schema_validation.rs` under the `test-support` feature
   and `#[cfg(not(feature = "rust-only"))]` (gated like `corpus_gpu`; `cfg(test)` would hide
   it from `tests/test_gpu_corpus.rs`, which links the library without it):
   `gpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, GpuBackend>` — for
   each `GpuBatch` it calls `device_schema::schema_of(&batch)` (the batch carries its executor
   and handle; no device object is needed) and `device_divergence(node's declared schema,
   actual)`, returning the divergence string on mismatch; a sink's host batches are skipped —
   the sink node has no schema and `concat_batches` already checks them. A
   `cpu_schema_validator` compares `batch.schema()` under the same projection, for symmetry and
   the end-to-end test; the CPU backend's own `declared_as` already holds every stage, so it is
   not installed in the cpu corpus.
3. **Driver unit tests**, `executor/driver/tests/hook.rs`, rust-only over `tests/mock.rs`
   (whose `MockBatch` is rows and bytes, no schema — so the hook under test is a closure, not
   the validator): a hook that refuses at a chosen node and lane — the run fails there with
   the hook's message; the same plan with an accepting, counting hook — called once per
   emitted batch and the run passes; the same plan with `None` — the report equals `run`'s;
   a hook that refuses the third batch — the failure names the third batch's node and lane.
4. **The corpus switch.** `corpus_query!` gains a trailing argument,
   `schema_validation_enabled | schema_validation_disabled`, in both `test_gpu_corpus.rs` and
   `test_cpu_corpus.rs` (the cpu macro ignores it); `gpu_case` installs `gpu_schema_validator`
   when enabled. `corpus_cases.inc`: every row whose gpu modes are not `none` says
   `schema_validation_enabled`. A cell that then fails on schema while its values match gets
   `schema_validation_disabled` with `// #NNN` on its line and a ticket — the registry cell
   stays `enabled`, so coverage does not move. Expected: none, since tasks 1–4 cleared the
   known classes; the switch is for the queries a later rollout enables.
5. **Verification on a device.** The enabled corpus at its enabled modes with validation on —
   CI's gpu job as it stands. Plus, once and not committed: an ad hoc device case whose
   hand-built plan declares a wrong type at one `GpuProject`, run through `run_with_hook` with
   the validator to show the hook failing that node by name; the run's output goes into the
   detail file. The `cpu_schema_validator` gets one committed end-to-end test in
   `src/tests/end_to_end/`: a small query passes under it, and a hook comparing against a
   schema with one field retyped fails naming the field.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/executor/driver/partitioned.rs`, `driver/mod.rs`, `executor/mod.rs` | the hook; `run_with_hook` beside `run` |
| `peacockdb-core/src/executor/driver/tests/hook.rs` (new) | the four mock tests |
| `peacockdb-core/src/test_support/schema_validation.rs` (new), `test_support/mod.rs`, `src/tests/end_to_end/` | the validator; its cpu end-to-end test |
| `peacockdb-core/tests/test_gpu_corpus.rs`, `test_cpu_corpus.rs`, `tests/common/corpus_cases.inc` | the argument; every enabled row |
| `llm-wiki/architecture.md`, `build-test.md` | the driver section names the hook; counts |

Component-level API: `run_with_hook` beside `run` at three levels (`pub(crate)`); the
`OutputHook` type. `run` itself is unchanged. No wire change, no ABI change, no facade change.

## Restriction

The hook is the only production change. No validation code outside the `test-support`
feature; no default hook; no change to what the driver does with a batch beyond calling the
hook first. The ad hoc trigger case is not committed.

## Verification bar

- rust-only: `--lib` — the four mock tests and the cpu end-to-end test; `test_module_layout`;
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --list` (the
  macro expands with its new argument) and `-- registry` (the registry guard, which reads the
  csv, not the macro).
- device: the enabled corpus at `tp1_single` and `tp4_single` with validation on, green; the ad
  hoc trigger red at its node, recorded.
- `git diff` of `executor/driver/` shows the hook and nothing else.

## Device workflow

`build-test-shadgpu.sh`, the corpus binaries; one cycle plus the ad hoc run.

## Completeness signoff — 2026-09-17

Solved under its constraints: `run_with_hook` beside `run` at three levels, `pub(crate)`, the
one production change; with `None` the driver does exactly what it did; a refusal ends the
query as a failed call does and releases the refused batch where it was refused, pinned by a
mock test over every site; the validator under `test-support` holds each device batch to its
node's declared schema through task 5's reader and comparator, with four mock tests and the
cpu twin's end-to-end pair; every enabled device cell runs with validation on and is green,
the ad hoc trigger red at its `GpuProject` by name and not committed. Shortcuts or bandaids:
none. Deviations, on record: three of the spec's four emission sites are hooked — the unload's
host batches are a `CpuBatch` a hook on `B::Batch` cannot take, and were to be skipped;
`OutputHook` lives in `executor/mod.rs` so `test_support` can name it; `tpch/shuffle-stddev`
runs with validation off on #225, its registry cell untouched. `done` waits on CI; with it the
chain is complete.
