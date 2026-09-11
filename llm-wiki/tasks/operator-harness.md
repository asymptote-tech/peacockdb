# One call sequence, two backends: the operator harness

Kind: production

**Closes no ticket and fixes nothing.** The task is a harness and the cases that prove it; what
it finds wrong it reports — a ticket and a `bug_` test — and leaves as it found it. Its two
production edits are mechanism the harness needs, not repairs, and neither changes what any
query answers.

Eighth in the chain, after [`sink-divergence-survey.md`](sink-divergence-survey.md) — a prototype
whose branch is never merged, so this one forks off `visibility`. It lands after
[`test-layout.md`](test-layout.md) on purpose: the harness is crate-level, needing both backends
and a device, and `src/tests/gpu_tests/` is the place that task creates for exactly that shape.
Writing it against `tests/*.rs` first would mean writing it twice.

## Why

Nothing today runs one operator on both backends over the same input and compares the two.
`executor_cases.inc` is eleven rows each side proves against a hand-written answer, and
`test_gpu_executors` asserts the device against answers the test wrote. A per-node divergence
therefore surfaces only at the root of a corpus query, where architecture.md's column-indexing
section says the per-node numbers cannot see it. The corpus is also numeric-aggregate heavy
([#195](../tickets.md#t195)), so most operator shapes have no query reaching them at all.

The harness makes the comparison one call: a hand-built node, a script of batches the test
wrote, both backends, the outputs compared exactly. This task builds it and proves it on the
operators whose recipe has no seq, so nothing in the FlatBuffer can hide a wrong helper.
[`operator-cases.md`](operator-cases.md) then runs everything else through it.

## What it is

**An upload.** `peacock_handle_from_arrow(executor, schema, array, out_handle)`: an Arrow C-data
import through `cudf::from_arrow` — the murmur3 hook at `gpu_executor.cpp:340` already does this
— adopted into the live session's registry as a handle, `NodeSession::adopt(TableResult)`. The
registry lives in the session, so `begin_plan` comes first; the seqless three load a plan of one
stub node, which is a shape the C++ already accepts under any forwarder's parent. The symbol is
test-only and says so where it counts: the header comment, no `AbiSymbol` names it, no recipe
can, and the extern sits in `peacockdb-ffi` beside `peacock_spark_partition_ids`, the test-only
hook already there — architecture.md's count of the ABI becomes seventeen, the conformance
group two. The `GpuBatch` a test wraps the handle in is the existing constructor. The fetch
side exists — `GpuExport` is what `GpuUnload` runs.

**A stub leaf, and the recipe code unchanged but for one arm.** `Given` — the leaf the cpu
backend's tests already have, declaring a schema and a layout and nothing else — moves up to
`src/tests/` and is shared. In `wire/attach.rs` a node outside the registry (`try_as_node_ref`
answers `None`, the case that function exists for) emits the writer's stub — the empty
`CudfScan` it already fills a forwarder's parent slot with, made reachable from `attach.rs` —
and no recipe. The stub is what keeps the plan rooted: the writer refuses a plan with no node,
and so does the C++, so a leaf that emitted nothing under an unload or a limit would leave
nothing to load. `attach_recipes(operator over Given leaves)` is then the operator's recipe, at
the last post-order, and its bytes are what `begin_plan` loads. No second recipe writer, no plan.

**A synthetic batch.** `synthetic(rows, seed)`: one fixed schema — Int32, Int64, Float64, Utf8,
Date32, Boolean, a key column with duplicates, a unique id — nulls in every column, floats
dyadic so sums compare exactly, deterministic from the seed. Zero rows is a legal argument and
a case in its own right. Decimals are a second fixture, `decimals(rows, seed)`, and not a column
of the first: the device exports every decimal at precision 38 whatever was declared
([#187](active-tickets.md#t187), open and owned by no task), so a decimal in every batch
would make every case that ticket's `bug_` test instead of the decimal cases alone.

**A comparator.** `assert_same(cpu, gpu, Order)`: slot by slot — one slot per call, and per
lane for the emitter, since output timing is a function of the call sequence on both backends
and a flattened multiset would pass a scatter that put a row in the wrong lane. A slot both
sides left empty — a call that produced no batch, which a limit outside its interval and a
one-call join's finish legitimately do — is equal; one side empty is a named difference, and it
is not the same thing as a zero-row batch. Within a slot, column names and data types, then
values, exact, after sorting by every column; `Order::AsEmitted` for the sorts, whose synthetic
keys carry no ties because neither engine's sort is stable. Nullability is not compared: the
device reports it from the data rather than the declaration, and the engine's own schema check
(`plan/validate.rs`) ignores it for the same reason. `CallStats` are not compared: the byte
formula is shared and scratch is measured. A type divergence fails — it is the
#183/#187/#191 shape, and a `bug_` test is where it belongs, not a cast in the comparator. The
comparator ships with its own red cases: a differing value, a differing type, a differing row
count, each shown to fail.

**One driver per category, generic over `B: Backend`.** `run_both(node, Script) -> Outcome`,
where `Script` names the call sequence in RecordBatches — `Exec(batches)`,
`Accumulate(batches)`, `Lanes(per-lane events)`, `Emit(batches)`, `Join { build, probe }`,
`Unload { batch, range }`, `Source` — and the harness refuses a script whose shape is not the
node's category. Executors come from `Backend::executors_for` with the post-order the tree
gives, so what is exercised is the trait, not a constructor. `Outcome` carries each side's
`Result`: a one-sided `Err` is the failure a `bug_` test then pins by message.

## Cases in this task

The three operators whose recipe carries no seq, plus the helper round trip:

- upload then fetch: whole, a row range, a range past the end, zero rows;
- `GpuUnload` through `executors_for`: whole, ranged, clamped, a zero-row batch, a range over a
  zero-row batch;
- `GpuLimit`: an interval inside one batch, one straddling two, batches entirely outside it,
  skip only, a stream of several batches, a zero-row batch inside a stream, a stream of nothing
  but zero-row batches, and an interval no batch reaches.

Empty inputs are separate cases everywhere, here and in task 9, one per shape — a zero-row
batch, no batch at all, one side or one lane empty — because the frozen surface cannot make a
table out of nothing ([#173](../tickets.md#t173)) and each shape reaches that limit by a
different route.

And a guard: every `NodeRef` kind is named by at least one case, in both directions, with the
three forwarders as the listed exclusions — they have no executor and belong to the driver,
which is tested elsewhere. Each case declares its kind in a registry the guard reads; the guard
never reads source text. Until task 9 fills the registry, the guard also carries the list of
kinds still without a case, checked both ways like the exclusions, and task 9 empties and
deletes it. It is what makes "comprehensive" a red test rather than a claim.

## Scope

Code expected to change:

- `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`: `peacock_handle_from_arrow`;
  `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp`: `NodeSession::adopt`.
- `peacockdb-ffi/src/lib.rs`: the extern, one entry.
- `peacockdb-core/src/wire/attach.rs`: the `try_as_node_ref` arm in `emit`;
  `peacockdb-core/src/wire/writer.rs`: the stub made reachable from that arm.
- `peacockdb-core/src/tests/`: `Given` lifted from `executor/cpu_backend/tests/` (whose own copy
  goes and whose call sites rename), `synthetic` and `decimals`, the comparator with its red
  cases, the kind registry and its guard.
- `peacockdb-core/src/tests/gpu_tests/`: `Device`, `Script`, `Outcome`, `run_both`, and this
  task's cases.
- `llm-wiki/build-test.md`: one row; `llm-wiki/architecture.md`: the ABI count.
- Nothing in `.github/workflows/`, `plan/`, `planner/`, `executor/`, or the wire format.

Component-level API expected to change:

- The C ABI: one additive symbol, test-only. `NodeSession`, the de facto C++ interface: `adopt`.
- `wire::attach_recipes`: accepts a leaf outside the registry, which emits a stub node and no
  recipe. Its signature and every other `wire/mod.rs` item are unchanged; `Writer` gains one
  `pub(super)` method inside `wire/`.
- `crate::tests` and `crate::tests::gpu_tests`, test modules rather than components: the items
  above are new. No component facade gains or loses an item.

## Constraints

- **No fix, anywhere.** Not in an operator, not in the C++, and not in the harness by casting or
  filtering a divergence away. A divergence or a one-sided failure is a ticket — an existing
  one where the defect is the same, a new one otherwise — and a `bug_` test asserting the wrong
  behaviour with the ticket above it. A ticket whose defect a case here no longer reproduces
  stays open: a ticket closes when its corpus cells run, not when a fix is visible in the code
  or a hand-built case passes. The detail file notes it, and nothing else moves.
- Synthetic data only. No sf1, no `testdata/`; the GPU job needs no dataset for any of this.
- Exact comparison. No tolerance argument exists.
- No driver and no forwarder: the harness calls executors, never `run`.
- Production code moves in two places only: the additive test-only symbol, and the one
  `attach.rs` arm with the writer method it calls. No production behaviour changes.
- Rung discipline as `test-layout.md` set it: the leaf, the batch and the comparator compile
  under `rust-only` in `src/tests/`; everything touching a device sits under
  `src/tests/gpu_tests/` behind `feature = "gpu"`. No new CI line — the lib binary's
  `gpu_tests::` filter already reaches it, and `test_ci_coverage` says so or goes red.

## Verification bar

- The comparator's red cases fail for the reason each names.
- Round trip, unload and limit green on shad-gpu; the kind guard green with the three
  exclusions and the pending list, and shown red once with a kind removed from it.
- `cargo test --features rust-only -p peacockdb-core --lib` still compiles and passes: the
  rust-rung modules carry no device type.
- `test_ci_coverage` green without a workflow edit.
- `build-test.md` gains one row for the harness, in its table's terms.
