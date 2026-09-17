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

### Completeness fix — the refused batch is released (rust-only, green)

- The trace, on `a_hook_that_refuses_fails_the_run_at_that_node_and_lane`: the source holds
  its batch (1 held); the project's `hold_all` releases that input (1 released) and holds its
  output (2 held); `offer` refuses and `?` drops the `Held` — 2 held, 1 released. `Held` has
  no `Drop` and `release_in_flight` walks queues, so the refused batch, never queued, was
  never released. The emitter and accumulator sites are the same shape one batch at a time;
  the lane arm drops the whole `Vec<Held<_>>` from the refused one on.
- The choice: the spec's order stays — offer after `record_emitted`, before the queue.
  `offer` takes the `&Held` and on `Err` releases its bytes before building the `CallFailed`,
  the `single_partition::failed` idiom; the lane arm iterates with `while let` and on a
  refusal releases the rest of the vector before returning. The queue-first alternative
  would have moved the spec's wording and left `release_in_flight` reconciling a batch the
  report would then count as abandoned, which a refusal is not.
- The test is `hook.rs::a_refused_query_gives_back_everything_it_held`, modelled on
  `failure.rs::a_failed_query_gives_back_everything_it_held`, over
  `unload(merge_sorted(merge(emit(coalesce_all(source)))))` with
  `AccRule::EmitAtDone(2)`: four sites — source, coalesce (the lane arm with two on the
  vector), emit, merge_sorted — each stepped to its `Err`, `release_all`, `holds > 0` and
  `holds == releases`. It reaches the driver through `Driver::with_hook` in
  `partitioned/tests.rs`, the sanctioned `pub(crate)` test-only setter beside `hops`. A first
  draft used `sort` for the lane arm; `category_of` makes `GpuSort` an `Exec`, so it streamed
  one batch per batch and the vector remainder went unexercised — caught by removing the
  remainder release and watching the test stay green.
- Red, before the fix: `node 5: 1 held, 0 released`. With the fix and the remainder release
  removed: `node 4: 5 held, 4 released`. Green: the hook module `5 passed`;
  `executor::driver` `144 passed`; `--lib` `587 passed; 2 ignored` (589 listed, 100 in
  `executor::driver::tests`); `test_module_layout` `17 passed`; no warnings on a rebuild;
  `testdata/` untouched. No device run: the accountant is the mock driver's, and nothing the
  validator calls changed. `build-test.md` counts moved by one (99 → 100, 588 → 589,
  1161 → 1162, 1817 → 1818, 2270 → 2271).

## Reviewing — 2026-09-17

Dispatch 1 committed as `cb15e798` on `36bb3060`, pushed; PR #163 against
`ENS-device-schema-harness`, base verified. For the reviewer: three of the spec's four emission
sites are hooked — the lane's host arm queues the unload's `CpuBatch`, which a hook typed on
`B::Batch` cannot take for a generic `B`, and the sink was to be skipped anyway; `OutputHook`
lives in `executor/mod.rs` so `test_support` can name it; `tpch/shuffle-stddev` is
`schema_validation_disabled` on #225 (twelve name findings, no type finding, values match).

## Completing — 2026-09-17

Review round 1: 0 blocking, 0 important, 5 nits. The reviewer confirmed `git diff
executor/driver/` is the hook and nothing else, the `None` path one `match` with no allocation,
the host-arm deviation right on typing (an enum over both batch kinds would buy nothing: the
only consumer would skip `Host`, and `concat_batches` already checks those batches), the lint
gate right for every build shape, and the four mock tests, the end-to-end pair and the corpus
chain each able to go red. Nits taken here, comment and wiki: `corpus_cases.inc`'s header
within the ten-line cap; `run_with_hook`'s doc naming its three test callers; the driver
sentence in `architecture.md` split into short ones. Nits deferred, to ride with the next
developer in this code and otherwise dropped: `no_hook_is_the_run_as_it_was` pins only the
delegation (a concrete property of the report would hold the run too); `corpus_gpu.rs`'s
`schema_validation(...) -> bool` is a noun, not a claim.

## Completeness — analyst, what is missing — 2026-09-17

0 blocking, 2 important. Read as one change against the spec's Scope and work items, the
verification bar, `architecture.md` and `build-test.md`; the reviewer's list unseen.

### Important 1 — a hook's refusal drops the refused output still held

`partitioned.rs:409-413` (emitter), `:467-471` (accumulator), `:329-333` (lane arm). At each
site the batch is held before `offer` and queued after it, so a refusal returns with the batch
neither queued nor released: `acct.hold` counted, no `acct.release`, and `Driver::run`'s
error arm (`release_in_flight`) sees only queues. On the lane arm the whole `Vec<Held<_>>`
from `hold_all` is dropped from the refused one on. Every other failure path is deliberate
about this — `single_partition::failed` releases the input that went into the failed call;
`run_emitter` releases the input "whether or not the call came back"; `Driver::run`'s comment
says "held and released stay equal on every path out of here"; and
`failure.rs::a_failed_query_gives_back_everything_it_held` pins it for `FailAt::Exec` and
`FailAt::Emit` by stepping, `release_all`, then `hops`. The same test with a refusing hook at
any of the three sites would fail. Nothing on the device leaks — `Held<GpuBatch>`'s `Drop`
releases the handle — so this is the accountant's invariant and the wiki's sentence, not
memory. Fix: `offer` takes `&Held<B::Batch>` and on `Err` does `let _ =
self.acct.release(held.bytes)` (the `failed` idiom); the lane arm releases the rest of its
vector on a refusal; a fifth mock test in `hook.rs` mirroring
`a_failed_query_gives_back_everything_it_held` over the three sites, which needs a
`pub(crate) fn with_hook` in `partitioned/tests.rs` since `driver/tests/` cannot set the field.

### Important 2 — #164 says the per-node type check is unstarted

`tickets.md` #164, last sentence: "The third closure #135 named is unstarted and belongs here
too: a per-node type check in the GPU tiers, the only thing that would surface a wrong-order
subtree before the root." This branch is that check for the corpus tier — every enabled device
cell holds every node's batch to its declaration, names and `{type_id, scale}`, and a swapped
pair is a name finding at the node (which is exactly how `shuffle-stddev` went red) — and
task 5's harness is it for the operator tier. The two C++ items in #164 stand. One dated line.

### `architecture.md` — sentences the branch falsified

- `## Execution` → `### Traits`: "The driver therefore needs no teardown: it stops scheduling,
  and the failure site releases the batch it was handed, exactly where the successful path
  would have." — false for a hook refusal (Important 1). True again with the fix.
- `### The scheduling rule` (added by this branch): "A refusal ends the query as a failed call
  does." — ends it and reports as `CallFailed`, but unlike a failed call leaves the refused
  output held. True again with the fix.
- `## Node display`, "Types are a plan fact": "a project's expression is compared against
  nothing, and the C++ half is #164." — every enabled device cell now compares a project's
  output, per batch, against its declaration through the hook (the ad hoc trigger is that
  comparison going red at `GpuProject`); what is still compared against nothing is the
  expression's type at plan time. Reword to say so.

Read and not falsified: Traits' "A call can fail, and failing ends the query" (executor
methods; the driver paragraph covers the refusal), "A batch is one table's worth of rows",
`GpuUnload` as the one non-`B::Batch` output (the reason the host arm is unhooked); Memory
accounting's "Holds and releases are counted, not netted" and the run-time total; Early exit's
shared release path (a refusal's queued batches do go through it); every Determinism rule (the
hook is synchronous on the one thread and reorders nothing); "Every cast is explicit" (now
enforced by the corpus as well as visible in the golden).

### Checked and present

Scope table: every row touched, nothing outside it but `tickets.md` (#225's dated line, which
the spec asks for). Item 1: type as specified (in `executor/mod.rs`, recorded); five queue
pushes in the two drivers, three hooked, the host arm and the forwarder not, as recorded;
`run_with_hook` `pub(crate)` at three levels; `None` is one `match`. Item 2: validator reads
`node.kind().schema()` — the plan's declaration, nothing invented — and task 5's
`device_divergence`; cpu flavour present, not in the cpu corpus. Item 3: the four named tests.
Item 4: both macros; 120 rows changed only by the appended argument (diffed against the
parent); 13 of 14 device rows `enabled`, `shuffle-stddev` `disabled); // #225`; registry
`gpu_tp1_single` still `enabled`, `testdata/` untouched; #225 says the names differ, dated
line present. Item 5: `gpu_` run is all 26 enabled cells (27 passed of 28 in the binary, the
28th run separately) — the three `tp4_single` cells the corpus enables among them, so the bar's
`tp1_single` and `tp4_single` reading is met by the corpus as it stands; trigger's plan and
`Err` line recorded, no trace in the tree (`git status` clean, no file names it); e2e pair in
`src/tests/end_to_end/`, the refusing one red-able by construction. Counts: driver row 95 → 99
(99 `#[test]` counted), end-to-end 27 → 29 (+2), `--lib` 582 → 588 and header 2264 → 2270
(+6, arithmetic); `test_ci_coverage` needs nothing (no target; case names unchanged).
`build-test.md`'s corpus prose names the one unvalidated cell. Not in the record: a full
`--lib` run (only `executor::driver` and `tests::end_to_end::schema_validation` filters);
CI's rust-only job covers it before `done`.

## Completeness pass — 2026-09-17

Two blind readings converged. **Reviewer (what is wrong): 0 blocking, 1 important; analyst
(what is missing): 0 blocking, 2 important** — the same defect from both: a hook refusal
returned with the batch held and neither queued nor released, the lane arm's remainder with it,
so the accountant's holds and releases stopped reconciling on the one path this task added,
and no test reached it (`failure.rs`'s call-failure case is a different shape — input released,
output never held). A developer fixed it at the refusal site (`82e091c6`): `offer` takes the
`Held` and gives the hold back on `Err`, the lane arm releases the remainder, and a fifth mock
test steps a refusal at every site and asserts holds equal releases — red before, red again
with the remainder release removed. A scoped review of that commit found every `Err` path
reconciled once, the `None` path byte-for-byte the old order, and the test red-able at each
site. The analyst's second item: #164's closing sentence still named a per-node type check in
the GPU tiers as unstarted — dated. `architecture.md`: two sentences the defect had falsified
(Traits' failure contract, the driver paragraph) are true again with the fix; the driver
paragraph says the release outright; "Types are a plan fact" now says a project's expression
is compared against nothing at plan time, the device's product being held per batch by the
harness and the validator. The analyst's evidence trail is the section below.
