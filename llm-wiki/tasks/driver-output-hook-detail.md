# driver-output-hook — run record

Chain B, task 6, the last. Branch `ENS-driver-output-hook` off `ENS-device-schema-harness` at
`9375a52d`; PR targets `ENS-device-schema-harness`. Tasks 1–5 are `done` (PRs #158–#162 green)
awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: task 5's `test_support::schema_of(&GpuBatch)` (`pub(crate)`, `not(rust-only)`)
  and `device_divergence` are what the validator composes; `GpuBatch::executor()` exists. The
  registry has 26 enabled device cells over 14 queries; every one gets
  `schema_validation_enabled`. The join-batching ticket is #220; the next free ticket number is
  226. Tasks 1–4 cleared the known type classes, so the spec expects no cell to need
  `schema_validation_disabled`; one that does is a ticket, never a rewritten expectation.
- Routing: the developer works `driver-output-hook-impl.md` — the hook with its four mock
  tests red then green (rust-only), the validator and its cpu end-to-end test, the corpus
  argument, one shad-gpu cycle over the enabled corpus with validation on, the ad hoc trigger
  run recorded here and not committed, then the record.
- Restriction: the hook is the only production change; with `None` the driver does exactly
  what it did; no validation code outside `test-support`; `git diff executor/driver/` is the
  hook and nothing else.

## Developer notes

### The hook (rust-only, green)

- `OutputHook` is declared in `executor/mod.rs`, not `partitioned.rs` as the impl plan
  sketched: `test_support` returns it from `gpu_schema_validator`, and a subcomponent's
  private module has no crate-wide path (`pub use` is banned). `run_with_hook` sits beside
  `run` at all three levels; `partitioned::run` is `run_with_hook(.., None)`, and the field
  is set in `run_with_hook` rather than through a `with_hook` builder. `executor::run_with_hook`
  carries `#[cfg_attr(not(test), allow(dead_code))]`: its callers are the corpus harness and
  the end-to-end tier, so a release build has none.
- **The spec's host arm (`partitioned.rs:324`) is not hooked, and cannot be under the spec's
  type.** That arm queues the unload's `Held<CpuBatch>`; a hook typed `&<B as Backend>::Batch`
  takes a `CpuBatch` only when `B` is the CPU backend, and the driver is generic. The hook is
  called at the three sites that queue a `B::Batch` — the lane's device arm, the emitter's
  per-lane loop, the partition accumulator's loop — through one `offer(node, lane, &batch)`
  after `record_emitted`, before the push. The spec's sink skip in the validator is therefore
  unreachable; `held_to_declaration` panics by node name if a schema-less node ever offers
  a batch rather than carrying a branch for it. Nothing is lost for validation: the sink's
  host batches were to be skipped anyway, and `concat_batches` checks them.
- The refusal is `RunError::CallFailed("<node> lane <n>: the output hook refused a batch:
  <why>")`, the shape `call_failed` builds, so the corpus's `panic!("{what}: {e}")` prints it
  with the query and mode in front.
- The four mock tests are `executor/driver/tests/hook.rs`, over
  `unload(merge_sorted(merge(emit(project(source)))))` so one plan reaches every emitting
  category and the forwarder: pre-order unload 0, merge_sorted 1, merge 2, emit 3, project 4,
  source 5. Three source batches give 19 hook calls (3 + 3 + 3×4 + 1) — the mock's
  `RoundRobin` scatters every batch over all four lanes — and the forwarder's three moves and
  the unload's three host batches are in `report.emitted` and not in the count. The `None`
  case compares `Debug` renderings of the two reports.
- Proving commands: `cargo test --features rust-only -p peacockdb-core --lib --
  executor::driver` → `143 passed` (95 mock cases + 4 new, plus the scheduler's 44);
  `--test test_module_layout` → 17 passed.

### The validator and its cpu end-to-end test (rust-only, green)

- `test_support/schema_validation.rs`, declared `mod schema_validation;` ungated; the gpu
  flavour inside is `#[cfg(not(feature = "rust-only"))]`. Both facades in `test_support/mod.rs`
  are `pub(crate)` — they name `OutputHook`, `PlanIndex` and a backend, so bare `pub` would
  fail `no_test_support_signature_names_a_component_type`. `cpu_schema_validator` has
  `#[cfg_attr(not(test), allow(dead_code))]` (its only caller is `src/tests/end_to_end/`).
  `schema_of(&GpuBatch)` lost its `allow(dead_code)`: the corpus now reads it.
- The comparison is `device_divergence(&declared.fields, actual)` with `actual` from
  `schema_of(batch)` (gpu) or `device_schema_of(&batch.record_batch().schema())` (cpu), so a
  refusal reads in the sink's spelling: `0 revenue: Int16 vs DECIMAL128 scale 4` — the
  declared type in arrow's spelling, the held one in the device's.
- `src/tests/end_to_end/schema_validation.rs`: `tpch/q6` at every mode passes under the
  validator; the refusing case runs the planned tree as it is and hands the validator an
  index over the same tree rebuilt with one `GpuProject`'s first field retyped to `Int16`
  (`rebuild` from `tests/rebuild.rs`; same shape, so the node numbering matches). That is the
  only way to make the cpu validator fire: the CPU backend's `declared_as` would refuse a
  retyped plan before any hook saw a batch. Proving command: `--lib --
  tests::end_to_end::schema_validation` → 2 passed.

### The corpus switch (rust-only, green)

- Every one of the 120 `corpus_query!` rows ends `schema_validation_enabled` — the gpu-`none`
  rows too, so a later rollout enables a validated cell. The header comment in
  `corpus_cases.inc` names the argument. `gpu_case` gained a sixth `&str`, decoded by
  `corpus_gpu::schema_validation` (exhaustive, panics naming the row); the `PlanIndex` is
  built before the session opens and the hook is `validated.then(|| gpu_schema_validator(&index))`.
  The regeneration case in `test_gpu_corpus.rs` passes `"schema_validation_enabled"`.
- `--test test_cpu_corpus -- --list` → 550 cases; `-- registry` → 1 passed;
  `--test test_ci_coverage` → 8 passed. rustfmt was applied to the leaves only;
  `test_cpu_corpus.rs` was restored from HEAD after rustfmt reformatted hunks the task never
  touched, and the macro edit reapplied by hand.

### The device (shad-gpu, 2026-09-17)

- Cycle: `scripts/build-test-shadgpu.sh --build` (clean, no warnings), `--push-binaries
  --patch`, then `PCK_TEST_FILTER='gpu_' PCK_RUN_CPP=0 … --run` (nothing in C++ changed, so
  the C++ suites were skipped). Binaries executed: `peacockdb_core_gpu_lib` (the `gpu_tests::`
  rung, the scratch trigger inside it) and `test_gpu_corpus`. No `[rmm] pool` line in any run.
- **First run**: lib `test result: ok. 536 passed; 0 failed; 0 ignored; 0 measured; 591
  filtered out` (535 + the scratch case); corpus `test result: FAILED. 26 passed; 1 failed;
  0 ignored; 0 measured; 1 filtered out`. The one red cell, verbatim:

      tpch/shuffle-stddev at tp1-single on a device: call failed: GpuAggregate lane 0: the
      output hook refused a batch: 2 stddev(lineitem.l_quantity)$count: Int64 vs
      stddev(lineitem.l_quantity) INT64; 3 stddev(lineitem.l_quantity)$mean: Float64 vs
      stddev(lineitem.l_quantity) FLOAT64; … 13 var_pop(lineitem.l_quantity)$m2: Float64 vs
      var_pop(lineitem.l_quantity) FLOAT64

  Twelve findings, every one a name and never a type: the Welford state triple held under the
  aggregate's alias, which is [#225](../tickets.md#t225) as filed by task 5. The cell's values
  match (it was green before the switch and its result oracle still passes), so the row says
  `schema_validation_disabled); // #225`, the registry cell stays `enabled`, and #225 carries a
  dated line. No new ticket; 226 is still the next free number.
- **Second run**, after the row change (rebuild, push, `gpu_` again): lib `test result: ok.
  536 passed; 0 failed; 0 ignored; 0 measured; 591 filtered out; finished in 5.97s`; corpus
  `test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in
  8.04s` — the 26 enabled cells with validation on (25 validated, `shuffle-stddev` not) plus
  the registry guard; `gpu_tpch_shuffle_stddev_tp1_single ... ok`. **Third run**,
  `PCK_TEST_FILTER='a_device_run_under_a_regeneration'`, for the one corpus test the `gpu_`
  filter misses and whose call this task edited: `test result: ok. 1 passed; 0 failed; 0
  ignored; 0 measured; 27 filtered out`.
- **The ad hoc trigger** (scratch `src/tests/gpu_tests/schema_trigger_scratch.rs`, deleted
  afterwards, `gpu_tests/mod.rs` restored from HEAD): `SELECT CAST(l_quantity AS INT) AS y
  FROM lineitem WHERE l_orderkey < 50` planned at tp1-single, the tree rebuilt with the
  project declaring `y: Int64` over its `CAST(… AS Int32)` — the project recipe carries
  expressions and aliases, never the declared schema, so the device computes `Int32` — then
  `run_with_hook::<GpuBackend>(lying, device.ctx(), None, Some(gpu_schema_validator(&index)))`.
  Its output on the device, from the run log:

      GpuUnload
        GpuProject: exprs=[CAST(l_quantity@0 AS Int32) as y], lanes=1, batches=multiple, schema=[y:Int64]
          GpuFilter: predicate=l_orderkey@0 < 50, projection=[l_quantity@1], lanes=1, batches=multiple, schema=[l_quantity:Decimal128(15,2)]
            GpuLoadParquet: table=lineitem, projections=[l_orderkey@0, l_quantity@4], …, lanes=1, batches=multiple, schema=[l_orderkey:Int64, l_quantity:Decimal128(15,2)]
      [trigger] run_with_hook returned Err(CallFailed): GpuProject lane 0: the output hook refused a batch: 0 y: Int64 vs INT32
      ok

  Red at the project by name, with the field and both types; the sink never saw the batch.
- Nothing outside the spec's Scope table changed except `llm-wiki/tickets.md` (the dated line
  on #225, which the spec's own "a ticket" clause asks for).

## Reviewing — 2026-09-17

Dispatch 1 committed as `cb15e798` on `36bb3060`, pushed; PR #163 against
`ENS-device-schema-harness`, base verified. For the reviewer: three of the spec's four emission
sites are hooked — the lane's host arm queues the unload's `CpuBatch`, which a hook typed on
`B::Batch` cannot take for a generic `B`, and the sink was to be skipped anyway; `OutputHook`
lives in `executor/mod.rs` so `test_support` can name it; `tpch/shuffle-stddev` is
`schema_validation_disabled` on #225 (twelve name findings, no type finding, values match).
