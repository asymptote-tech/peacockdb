# Driver output hook implementation plan

**Goal:** The driver accepts an optional hook called on every emitted batch; under test, a
validator holds each device batch to its node's declared schema, four mock-driver tests prove
the hook, and `corpus_query!` gains `schema_validation_enabled | schema_validation_disabled`,
on for every enabled cell.

**Architecture:** One field on `Driver`, one parameter on `run_with_hook` (a sibling of `run`
so no existing caller moves), one call at each of the four sites where a node's own output is
queued. Everything that *uses* the hook is under the `test-support` feature (`test_support`)
or `cfg(test)` (`driver/tests`).

**Tech stack:** Rust; rust-only for the driver work; the device corpus for the rollout.

**Spec:** [`driver-output-hook.md`](driver-output-hook.md) — frozen.

## Global constraints

- `git diff peacockdb-core/src/executor/driver/` outside `tests/` shows the hook and nothing
  else. With `None` no allocation and no branch beyond the `Option`.
- No validation code outside the `test-support` feature; `cfg(test)` would hide it from the
  corpus binary. The ad hoc trigger case is never committed.
- Registry cells do not move: a schema failure with matching values is
  `schema_validation_disabled` + `// #NNN` on the `corpus_query!` line + a ticket, cell still
  `enabled`.
- `rustfmt`; commits at most 10 lines; the device run foreground.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/executor/driver/partitioned.rs` | `OutputHook`, the field, `Driver::with_hook`, the four call sites (`:314`, `:324`, `:391`, `:448`) |
| `peacockdb-core/src/executor/driver/mod.rs`, `executor/mod.rs` | `run_with_hook` beside `run`, `pub(crate)` at every level |
| `peacockdb-core/src/executor/driver/tests/hook.rs` (new) | the four tests, over closures — `MockBatch` has no schema and stays so |
| `peacockdb-core/src/test_support/schema_validation.rs` (new), `test_support/mod.rs`, `src/tests/end_to_end/` | `gpu_schema_validator`, `cpu_schema_validator`; the cpu end-to-end test |
| `peacockdb-core/src/test_support/corpus_gpu.rs` | `gpu_case` takes the switch |
| `peacockdb-core/tests/test_gpu_corpus.rs`, `test_cpu_corpus.rs`, `tests/common/corpus_cases.inc` | the argument; every row |

---

### Task 1: The hook

**Files:**
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs:36-100,300-330,385-395,444-450`, `driver/mod.rs:24-30`, `executor/mod.rs:633-640`
- Create: `peacockdb-core/src/executor/driver/tests/hook.rs`

**Interfaces:**
- Produces:

```rust
pub(crate) type OutputHook<'a, B> =
    Box<dyn FnMut(usize /*node*/, usize /*lane*/, &<B as Backend>::Batch) -> Result<(), String> + 'a>;
pub(crate) fn run_with_hook<B: Backend>(root: &dyn GpuNode, ctx: &B::Context, budget: Option<usize>,
                                        hook: Option<OutputHook<'_, B>>) -> Result<RunReport, RunError>;
pub(crate) fn run_with_hook<B: Backend>(…)  // executor/mod.rs, same shape; pub(crate), not pub —
                                           // a pub item needs a SURFACE entry and no outside caller exists
```
  `run` stays as it is and calls `run_with_hook(…, None)`.

- [ ] **Step 1: Failing tests.** `driver/tests/hook.rs`, over `mock.rs`'s plans (use the
  smallest exec plan `plans.rs` builds — scan → project → unload):

```rust
#[test]
fn a_hook_that_refuses_fails_the_run_at_that_node_and_lane() {
    let plan = two_node_plan();                                    // the file's builder
    let hook: OutputHook<Mock> = Box::new(|node, lane, _| if node == 1 && lane == 0 {
        Err("column 0 x: Int32 vs INT16".into()) } else { Ok(()) });
    let err = run_with_hook::<Mock>(plan.as_ref(), &ctx(), None, Some(hook)).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("GpuProject") && text.contains("lane 0") && text.contains("Int32 vs INT16"), "{text}");
}
#[test]
fn a_hook_is_called_once_per_emitted_batch() {
    let mut seen = Vec::new();
    let hook: OutputHook<Mock> = Box::new(|node, lane, b| { seen.push((node, lane, b.num_rows())); Ok(()) });
    let report = run_with_hook::<Mock>(plan.as_ref(), &ctx(), None, Some(hook)).unwrap();
    assert_eq!(seen.len(), report.batches_emitted_total());     // or the report's equivalent count
}
#[test]
fn no_hook_is_the_run_as_it_was() { /* run and run_with_hook(None) give equal reports */ }
#[test]
fn a_hook_refusing_the_third_batch_names_the_third_batchs_node() { /* counter in the closure */ }
```

  The error must carry the node's name and the lane: route it through `StepError` the way a
  call failure does (`self.call_failed(node, lane, …)`, `:373`), with the hook's string as the
  message.

- [ ] **Step 2: Red** — `run_with_hook` and `OutputHook` do not exist.
- [ ] **Step 3: Implement.** `partitioned.rs`: the type alias at the top; `hook: Option<
  OutputHook<'a, B>>` on `Driver`; `Driver::new(..)` leaves it `None` and `pub(crate) fn
  with_hook(mut self, hook) -> Self`; at the four sites where a node's own output is queued —
  the lane's device arm (`:314`) and host arm (`:324`), the emitter's per-lane loop (`:391`),
  the partition accumulator's loop (`:448`) — after `record_emitted`, before the push/release
  (the forwarder at `:495` moves a child's batch and is not an emission):

```rust
if let Some(hook) = self.hook.as_mut() {
    hook(node, lane, &batch).map_err(|why| self.hook_refused(node, lane, why))?;
}
```

  (`hook_refused` builds the same `StepError` shape as `call_failed`, message `"{node name}
  lane {lane}: the output hook refused a batch: {why}"`.) `partitioned::run` becomes
  `run_with_hook(root, ctx, budget, hook)` = `Driver::new(..)?.with_hook_opt(hook).run(..)`,
  `run` calls it with `None`; `driver/mod.rs` and `executor/mod.rs` forward both. Borrow
  check: the hook is called with `&batch` before the batch moves into the queue — order the
  statements so.
- [ ] **Step 4: Green** — `cargo test --features rust-only -p peacockdb-core --lib --
  executor::driver::tests`; the whole `--lib` too (nothing else may move).
- [ ] **Step 5:** `git diff --stat peacockdb-core/src/executor/` — only the files above; the
  diff of `partitioned.rs` is the alias, the field, `with_hook`, four `if let Some(hook)` blocks
  and `hook_refused`.
- [ ] **Step 6: Commit.** `git commit -m "the driver takes an output hook, called once per emitted batch"`.

### Task 2: The validator

**Files:**
- Create: `peacockdb-core/src/test_support/schema_validation.rs` — declared in `test_support/mod.rs` exactly as `corpus_gpu` is: under the `test-support` feature, the gpu flavour additionally `#[cfg(not(feature = "rust-only"))]`. Not `cfg(test)`: `tests/test_gpu_corpus.rs` links the library without it.

**Interfaces:**
- Produces: `pub fn gpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, GpuBackend>` (no device argument — `device_schema::schema_of(&GpuBatch)` reads the batch's own executor and handle); `pub fn cpu_schema_validator<'a>(index: &'a PlanIndex<'a>) -> OutputHook<'a, CpuBackend>`.

- [ ] **Step 1: Failing test** (rust-only, the CPU flavour over the mock is not possible —
  the mock is its own backend — so the CPU flavour is tested through an end-to-end run):
  in `src/tests/end_to_end/`, a small query through `run_with_hook::<CpuBackend>` with
  `cpu_schema_validator` — passes; then the same with a hook that compares against a schema
  with one field retyped — fails naming the field. The gpu flavour's test is Task 3's device
  run.
- [ ] **Step 2: Implement.** Both close over the index; per batch: if
  `index.nodes[node].kind().schema()` is `None` (the sink) return `Ok(())` — its
  `concat_batches` already checks; else `device_divergence(declared, &actual)` where `actual`
  is `device_schema::schema_of(batch)` (gpu) or `device_schema_of(&batch.schema())` (cpu) —
  both from the previous task's `test_support::device_schema`; `Some(text)` → `Err(text)`.
- [ ] **Step 3: Green**; commit `"schema_validator: every device batch held to its node's declaration, under test only"`.

### Task 3: The corpus switch

**Files:**
- Modify: `peacockdb-core/tests/test_gpu_corpus.rs:15-45`, `test_cpu_corpus.rs:21-…`, `peacockdb-core/src/test_support/corpus_gpu.rs:89-100`, `peacockdb-core/tests/common/corpus_cases.inc`

- [ ] **Step 1:** Both macros gain a trailing `$validation:ident`; the cpu macro ignores it;
  the gpu macro passes `stringify!($validation)` to `gpu_case`, which matches
  `"schema_validation_enabled" | "schema_validation_disabled"` (anything else panics naming
  the row) and calls `run_with_hook::<GpuBackend>(tree.as_ref(), &ctx, None, enabled.then(||
  gpu_schema_validator(&index)))` in place of `run` (`corpus_gpu.rs:96`). The `PlanIndex` is
  built once from the tree with `driver::build_index` before the run; `run_with_hook` reaches
  `corpus_gpu.rs` as `crate::executor::run_with_hook` — `pub(crate)` suffices, the corpus
  helper is in the crate.
- [ ] **Step 2:** `corpus_cases.inc`: every `corpus_query!` line gains
  `schema_validation_enabled` as its last argument (`sed` over the file; the rows with gpu
  `none` too, for uniformity — the macro's `none` arm takes it and ignores it).
- [ ] **Step 3:** rust-only: `cargo test --features rust-only -p peacockdb-core --test
  test_cpu_corpus -- --list` (the macro expands with its new argument) and `-- registry`
  (the registry guard, unaffected but proven) green; `test_module_layout` green.
- [ ] **Step 4: Commit.** `git commit -m "corpus_query! says whether a run is schema-validated; every row says yes"`.

### Task 4: The device

- [ ] **Step 1: The enabled corpus** — `PCK_TEST_FILTER='gpu_'` at all enabled modes (CI's
  gpu job's shape). Expected: every enabled cell green with validation on. A cell red on the
  hook's message with matching values: `schema_validation_disabled`, `// #NNN` on its line,
  a ticket naming node, column and the divergence; the registry untouched. Record every such
  cell in the detail file — expected none.
- [ ] **Step 2: The ad hoc trigger** — not committed: a hand-built plan in a scratch device
  case (`Given` leaf → `GpuProject` declaring `y: Int64` over a `Cast` to `Int32` → unload) run
  through `run_with_hook::<GpuBackend>` with `gpu_schema_validator`; assert the run fails at
  the project naming `y: Int64 vs INT32`. Run it on the device, paste the failure into the
  detail file, `git checkout` the scratch file away. The committed counterpart is the cpu
  end-to-end test from Task 2, which proves the same wiring rust-only.
- [ ] **Step 3:** `architecture.md`'s driver section: one paragraph — the hook, its contract,
  that production passes `None`. `build-test.md`: the four mock tests, the corpus's new
  argument, counts.
- [ ] **Step 4: Commit.** `git commit -m "every enabled corpus cell runs schema-validated on the device"`.

### Task 5: The record

- [ ] Detail file: the mock tests, the corpus run's summary, the ad hoc trigger's output, any
  cell disabled and why. `git commit -m "driver-output-hook: the record"`.
