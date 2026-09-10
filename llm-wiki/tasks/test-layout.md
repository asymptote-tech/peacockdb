# 3 — tests down the source tree

Kind: production

Third of four, after [`module-layout.md`](module-layout.md) and before
[`test-support.md`](test-support.md).

170 top-level items in `peacockdb-core/src` are `pub`. Eight are named by another crate; the other
108 are `pub` only because `peacockdb-core/tests/*.rs` are separate crates that see the library the
way crates.io would. This task moves the eleven targets that force them, plus the murmur gate, down
into `src/`, taking the surface from 108 test-driven items to **eight**. The last eight go in
[`test-support.md`](test-support.md).

Three things happen together, because none is worth a separate pass over the same files: the move,
the visibility sweep, and the separation of test code from production code.

## Why the demand is concentrated

Two files in `tests/common/` force 73 of the 108. `injection.rs` (822 lines) and `rebuild.rs` (623)
take a plan tree apart and put it back together, constructing every node kind and every backend
executor on the way. `join_fixture.rs` adds two more.

The other 2,488 lines of `tests/common/` — corpus, registry, golden text, result text, cost model,
mode table — need nine, and no target that keeps them touches the injector. That is what makes the
split clean rather than a judgement call.

Partial moves do not pay: 108 goes to 79 if only `common/` moves, 50 with the two executor tiers,
15 with three more, and 9 only when all eleven targets go. Do it once.

### Where a feature-gated helper pays, and where it does not

`#[cfg(test)]` cannot help an integration test at all: the library is compiled without `cfg(test)`
when cargo builds one, so a `cfg(test) pub` item is unreachable from `tests/`. That is the whole
reason these 108 exist. A **feature** can, and the mechanism costs nothing at the call site:

```toml
[features]
rust-only = ["peacockdb-ffi/rust-only"]
gpu = []                # device tests; not propagated to peacockdb-ffi, which has no device path
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

`gpu` and `rust-only` are mutually exclusive and a `compile_error!` says so. `gpu` does not
propagate to `peacockdb-ffi`: that crate is a C ABI binding with no device-conditional code, and
adding a feature there would be a knob nothing reads.

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so no CI step passes a flag and a plain build cannot see the module.

**It does not pay for the injector.** `test_gpu_executors` (46 items) constructs `GpuAccumulator`,
`GpuEmitter` and `GpuJoin` and calls their methods; `test_cpu_executors` (24) does the same on the
other backend; `test_null_analysis` (18) hand-builds plan nodes. A facade over those re-exposes the
same vocabulary under test-local names — indirection, not encapsulation. Those targets move
in-crate instead, which is what takes 108 to single digits.

**It does not pay for the corpus harness either, in this task.** `corpus.rs` and `corpus_gpu.rs`
force the eight items that survive here, and moving them behind a feature is
[`test-support.md`](test-support.md)'s whole subject. They stay in `tests/common/` until then.

## Test code is never in a production file

Added to `coding-style.md`.

- No test code in a file that also carries production code. A module's unit tests are a child
  module of their own in their own file: `validate.rs` with `validate/tests.rs`, which is already
  the pattern in `expr_physical`, `accounting`, `index`, `scheduler`, `single_partition` and
  `expr_writer`.
- A test-only helper in `src/` lives in a file or directory whose name carries `test`, so a reader
  can tell test-only code from production code by the path alone. `driver/mock.rs` and
  `driver/plans.rs` are `#[cfg(test)]` today and do not say so in their names; they become
  `driver/tests/mock.rs` and `driver/tests/plans.rs`.
- Every `mod tests` declaration carries exactly one of the two gates below.

Thirteen files hold inline `#[cfg(test)] mod tests { … }` and are split: `validate.rs` (23 cases),
`plan_text/mod.rs` (13), `expr_translate.rs` (13), `estimator.rs` (11), `partitioner.rs` (8),
`parquet_meta.rs` (6), `nodes/aggregate.rs` (4), `layout.rs` (4), `plan_text/expr_text.rs` (3),
`forwarder.rs` (3), `executor.rs` (2), `backend.rs` (1), and `config.rs` (2), which the previous
task deletes.

The mod.rs rule from `module-layout.md` applies to components and subcomponents, not to
implementation modules — an implementation module with unit tests is `foo.rs` beside `foo/tests.rs`.
Do not reach for `clippy::mod_module_files` to enforce the mod.rs rule: it cannot be scoped that
way and would reject exactly this pairing. The layout test can scope it, and has to exist anyway.

## The device guard becomes a feature

Today no `cfg` says "needs a device". `rust-only` says "no FFI linked", and `test_gpu_batch` is
`#![cfg(not(feature = "rust-only"))]` while running on a GPU-less runner. What actually keeps device
tests off CPU hosts is which binary CI runs where — and that mechanism disappears the moment those
tests are inside `--lib`.

Add a `gpu` feature. Three build shapes for the three contexts that already exist:

| Build | FFI linked | Device assumed | Runs |
|---|:-:|:-:|---|
| `--features rust-only` | no | no | dataset-matrix, CPU steps |
| default | yes | no | dataset-matrix, FFI steps |
| `--features gpu` | yes | yes | shad-gpu only |

Two spellings, one on every `mod tests`:

```rust
#[cfg(all(test, feature = "gpu"))] mod tests;        // device tests
#[cfg(all(test, not(feature = "gpu")))] mod tests;   // everything else
```

`--features gpu` therefore builds a lib test binary holding only device tests, so
`--test-threads=1` on shad-gpu does not drag 435 CPU unit cases through a serial run. A
`compile_error!` on `all(feature = "gpu", feature = "rust-only")` keeps the pair exclusive.

Two alternatives were considered and are not open. A runtime device check that skips is the shape
`build-test.md` already records shipping a hole — the binaries skip and exit 0, green having
verified nothing, which is why CI asserts sf40's presence itself. And `#[ignore]` already means
"disabled against a ticket" (#182), so reusing it for "needs a device" is one spelling for two
things.

## What moves into src/

| Lands in | From | | N |
|---|---|:-:|--:|
| `plan/tests/` | `test_batch_partitioned_injection` | cpu | 4 |
| `planner/tests/` | `test_planner_join_capability` | cpu | 13 |
| `planner/tests/` | `test_planner_join_refusals` | cpu | 10 |
| `planner/tests/` | `test_null_analysis` | cpu | 8 |
| `planner/tests/` | `test_batch_partitioned_plans` | cpu | 19 |
| `wire/tests/` | `test_gpu_recipe_walk` | **gpu** | 10 |
| `executor/tests/` | `test_gpu_batch` | cpu | 3 |
| `executor/cpu_backend/tests/` | `test_cpu_executors` | cpu | 1 |
| `executor/gpu_backend/tests/` | `test_gpu_executors` | **gpu** | 31 |
| `executor/gpu_backend/tests/` | `test_gpu_abi` | **gpu** | 4 |
| `executor/cpu_backend/tests/` | `test_murmur_conformance` | **gpu** | 10 |
| `src/tests/` | `test_cpu_batch_partitioned` | cpu | 26 |
| `src/tests/` | `common/{injection,rebuild,join_fixture}.rs` | — | 0 |
| | | | **139** |

55 of the 139 are device cases. The injector and the rebuilder land at crate level rather than in
`plan/` because they construct backend executors as well as plan nodes, so they sit above both.

`src/tests/` is a crate-level `#[cfg(test)] mod tests;` declared in `lib.rs` — a peer of the
components that can see all of them. It holds the end-to-end tier and the shared test support the
component tests reach through `crate::tests::…`. The layout injector and the tree rebuilder live
here, not in `plan/` or `wire/`: they construct plan nodes, wire recipes and backend executors
alike, so they sit above every component rather than inside one.

### What makes test-only actually test-only

Three layers, and only the first is discipline.

- **The path says so.** `src/tests/…`, `component/tests/…`, `module/tests.rs`. A reader can tell
  test code from production code without opening the file.
- **One `#[cfg(test)]` at the root of each test subtree, and the gate is transitive.** `src/tests/`
  is excluded from a non-test build entirely — its files are never compiled, so `mod.rs` and the
  files below it carry no attribute of their own. Production code that references anything inside
  is a hard error in the release build (`E0433: failed to resolve`), not a warning and not a
  lint. That is the guarantee: it is the compiler, not a convention.
- **The layout test checks the shape**, because two things the compiler is happy with would still
  be wrong: a `#[cfg(test)]` attribute anywhere other than on a `mod tests` declaration — which is
  how test code creeps back into a production file one item at a time — and a `mod tests` missing
  its `gpu` / `not(gpu)` gate.

## What stays a separate binary

| Target | Why it cannot fold in | | N |
|---|---|:-:|--:|
| `test_cpu_bp_corpus` | `inventory` collects per linked binary; the registry needs two | cpu | 448 |
| `test_gpu_bp_corpus` | the other half of that pair, and it writes env vars | **gpu** | 8 |
| `test_golden_format` | the format reader, over strings | cpu | 24 |
| `test_corpus_goldens` | committed sections against their own arithmetic | cpu | 20 |
| `test_ci_coverage` | reads the workflow yaml | cpu | 6 |
| `test_cost_model` | `.cost.txt` re-derived from `.cpu.txt` | cpu | 3 |
| | | | **509** |

None of the six names an item that would otherwise have to stay `pub` beyond what `corpus.rs` and
`corpus_gpu.rs` already force. The `inventory` constraint
survives untouched, which is the one that looked fatal: it collects per linked binary and the two
corpus binaries both stay.

## test_ci_coverage shrinks to about 300 lines

It keeps its job and loses most of its subject. From 720 lines:

- The target sweep now covers six targets, not eighteen, and `INTENTIONALLY_NOT_IN_CI` drops from
  six entries to two.
- The three GPU target lists become one staging list of two binaries plus the `--lib --features gpu`
  run.
- Its own matcher unit tests stay whole. They are the reason this guard can go red at all.

It gains one assertion, and it is the most important one in the file: **a workflow line must run
`cargo test --lib --features gpu` on shad-gpu.** Forty-five device cases move into `--lib` in this
task, and without that line they stop running with nothing saying so — a larger hole than any this
guard closes. The existing hand-written assertions, that some line runs `--lib` and some line builds
the CLI, get more load-bearing for the same reason: `--lib` now carries 129 cases from eleven former
targets.

## Renames

**`test_inc2_conformance` becomes `test_murmur_conformance`**, and that closes a documented
exception. `coding-style.md`'s Names section opens by admitting the name breaks its own rule —
"named after an increment, which the second bullet forbids" — and justifies keeping it because
renaming would move the staging array, the exemption list and two pages. This task moves all three
anyway. Delete that paragraph; the rule no longer needs an apology beside it.

Sixteen references in ten files: `scripts/build-test.sh` (5), `pipeline.yml` (2),
`build-test.md` (2), and one each in `build-test-shadgpu.sh`, `test_cpu_executors.rs`,
`test_ci_coverage.rs`, `executor_cases.inc`, `coding-style.md`, `architecture.md` and
`cpp/tests/gpu/test_cudf.cpp`.

It is also the lowest-level test in the suite and moves in-crate with the rest: it names nothing
from `peacockdb_core`, driving comet's `create_murmur3_hashes` and one FFI symbol over raw arrays.
It lands beside `spark_partitioning.rs`, the CPU half of the invariant it protects.

That placement surfaces a gap worth a ticket, not a fix here: the test re-derives `pmod` and the
seed-42 pre-fill locally rather than calling `rows_per_lane`, so it proves the kernel matches comet
while the code that actually places rows is only transitively covered. One rule, two copies.

One of the five references in `build-test.sh` is not a comment: line 309 is a literal
`peacockdb-core:test_inc2_conformance` in a hand-maintained list, and the comments around it record
that this is the one test file the suite derivation does not catch by pattern — it was silently
skipped once. Change the literal and leave the explanation, which is still true of whatever the
file is called.

`driver/mock.rs` and `driver/plans.rs` become `driver/tests/mock.rs` and `driver/tests/plans.rs`.
`translate/schema_tests.rs` already carries the word and stays.

## build-test.md

The test table is restructured in this task, not a later one. One standalone table for
`peacockdb-core`, in five visually separate sections, with cpu and gpu split inside each:

| Tier | cpu | gpu | total |
|---|--:|--:|--:|
| crate integration, external — the six binaries above | 501 | 8 | 509 |
| crate integration, internal — `src/tests/` | 26 | — | 26 |
| component — plan 63, planner 50, wire 31, executor 9, plan_text 13 | 156 | 10 | 166 |
| subcomponent — driver 90, cpu_backend 75, gpu_backend 35, translator 38, memory_estimation 11 | 204 | 45 | 249 |
| module unit | 133 | — | 133 |
| | 1,020 | 63 | 1,083 |

Assign each file by where its `mod tests` is declared, not by eye: `driver/accounting/tests.rs` is
a unit test of an implementation module, `driver/tests/` is the subcomponent's, and `nodes/tests/`
is `plan`'s. The three cross-checks are `--lib` at 519, `--lib --features gpu` at 55, and the six
binaries at 509.

Everything else — the C++ suites, the Python prototype and validators, `peacockdb-ffi`,
`cost-report` — moves to a second table. The two totals still have to add to the page's headline
figure, which is how the page is checked.

Two facts this makes visible that the current table cannot. Where the coverage actually sits:
`executor/driver` at 90 subcomponent cases and `plan` at 63 are the heaviest, and the corpus's 448
is one target rather than a tier. And the gpu column is 63 cases in four places — 45 inside `--lib`
under `--features gpu`, 18 in two binaries — which is the whole of what shad-gpu runs.

## The three GPU target lists, and the two scripts

Today five GPU targets are named in three places that `test_ci_coverage` asserts agree:
`build-test.sh`'s `gpu_runtime_targets()`, `build-test-shadgpu.sh`'s `RUST_TESTS` at line 30, and
`pipeline.yml`'s staging loop. After this task the list is **one target plus a lib build**, which is
a bigger change to those scripts than to the lists.

**`build-test-shadgpu.sh`**

- `RUST_TESTS=(test_gpu_bp_corpus)` — one entry.
- `stage_cargo_test_binary` resolves a built binary by matching `target.name` against a `--test`
  name in cargo's json. It needs a second form for the lib test target, whose json entry has
  `kind: ["lib"]` and `test: true` and whose `target.name` is the crate name. Stage it under an
  explicit filename — `peacockdb_core_gpu_lib` — because the run loop globs
  `cpp/install/rust-tests/*` and a bare crate name reads as ambiguous beside the target binaries.
- The build must pass `--features gpu`. Nothing else in the run phase changes: the loop already
  passes `--test-threads=1` to every staged binary, and the ran-any and `PASSED 0 tests` guards
  still apply.

**`build-test.sh`**

- `gpu_runtime_targets()` shrinks to `test_gpu_bp_corpus` plus the lib entry.
- **Ten lines of comment above it become obsolete and should go.** They explain that
  `test_inc2_conformance` is the only file gating per item rather than per file, which is why the
  membership test could not see it and why the list is written out rather than derived. After the
  move it is an in-crate `mod tests` gated on `feature = "gpu"` like every other device test, and
  the special case it documents no longer exists. Delete the explanation with the exception.
- `needs_cmake_targets()` derives from a file-level `#![cfg(not(feature = "rust-only"))]`, which
  still works and now finds one target. `--lib` is not a `--test` target, so nothing derives it —
  it must be named, in both the `--gpu` and the `--rust-only` paths, the way `pipeline.yml` names
  it today.
- The `--rust-only` path builds `--lib` without `gpu`; the `--gpu` path builds `--lib --features
  gpu`. These are different binaries with disjoint case sets, which is the point.
- The "derived suite must not be EMPTY" guard stays and gets closer to firing. With one derived
  target left it is one move away from being the thing that catches a mistake, so leave it.

**`pipeline.yml`** takes the same three-list change plus a `cargo test --lib --features gpu` step on
shad-gpu. **`test_ci_coverage`** then compares three one-entry lists and asserts that step exists —
which, as above, is the single assertion standing between 55 device cases and silently not running.

## The surface, after this task

Sixteen bare `pub` items remain, in six files: the CLI's eight, plus `GpuNode` and `validate`
(`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`, `GpuBackend` and
`GpuContext` (`executor/mod.rs`), and `render_run` (`plan_text/mod.rs`). All eight of those are
forced by `tests/common/corpus.rs` and `corpus_gpu.rs`, and [`test-support.md`](test-support.md)
removes them.

Five are lifted out of a subcomponent to get there, because a subcomponent is private: `run` and
`RunReport` out of `executor/driver`, `CpuBackend` out of `executor/cpu_backend`, `GpuBackend` and
`GpuContext` out of `executor/gpu_backend`. That is the rule doing its work — the wall forces
whatever must be public upward until it is all in six files.

## The wiki this moves

- **`build-test.md`'s test table is restructured here**, per the section above: one standalone
  `peacockdb-core` table in five sections with cpu and gpu split, everything else in a second. The
  two must add to the headline figure, which is how the page is checked.
- **`coding-style.md` gains the test-code rules** — no test code in a production file, a module's
  unit tests in a child module of their own, `test` in every test-only path, and one of the two
  `gpu` gates on every `mod tests`. It also loses the `test_inc2_conformance` exception paragraph
  that opens its Names section, since the rename closes it.
- **`coding-style.md`'s visibility section is amended, not rewritten**: the previous task states
  the rules, this one records what they settled at — eight unconditional `pub` items in three
  files, and the `test_support` rule that a feature-gated `pub` must have a signature free of
  engine types, or it is the surface under another name.
- **`architecture.md`** needs the Execution section's driver and accountant paths, the wire-format
  section's writer paths (now `wire/`), and the Rehash section's `spark_partitioning.rs` pointer,
  which moves into `executor/cpu_backend/` and whose conformance gate moves beside it.

## Validation

This task moves 8,450 lines of test code between compilation units. Nothing it touches may change
what the engine computes, and no case may be lost — the two risks are opposite in kind and are
checked differently.

### Baselines

1. `--list` for `--lib` and every integration target, reduced to **leaf names** — strip module
   paths, because that is the set the move must preserve while the paths necessarily change.
2. `sha256sum` over `testdata/goldens/`. No golden may move at all in this task.
3. The `pub`/`pub(crate)` item dump from the end of task 2.
4. Warning counts from clean builds in all three feature shapes.

### The invariant: the case set is preserved, the paths are not

A case that disappears here is silent. Nothing goes red — a target runs 30 tests instead of 31, and
no assertion knows it was meant to run 31.

- **Leaf-name set equality.** The union across `--lib`, `--lib --features gpu` and the six binaries
  must equal the baseline union exactly. Not the count — the set, so a case deleted and another
  duplicated cannot cancel out.
- **The counts split as predicted**, each a separate assertion: `--lib` lists **519**,
  `--lib --features gpu` lists **55** and nothing else, the six binaries list **509**. If
  `--lib --features gpu` lists a cpu case, a `mod tests` is missing its `not(feature = "gpu")` gate
  and that case is about to run serially on the GPU host.
- **Total unchanged at 1,083.** This task moves tests; it deletes none.

### Move in slices

One commit per target, ascending by how much it forces: `test_gpu_batch` (3 cases, 2 items) first
as the shape proof, then the four injector consumers with `injection`/`rebuild`/`join_fixture`, then
the executor tiers, then the rest. After each, the leaf-name set for that target must have moved
from its binary's list into the right module's list and nowhere else.

**Track the surface as it falls: 108 → 79 → 50 → 15 → 8.** A slice that does not move the number
moved the wrong thing.

### The device gate

- `cargo build --features gpu` and `--features rust-only` both succeed; the `compile_error!` fires
  when both are passed together, and that is checked by trying it.
- **Every `mod tests` carries exactly one gate.** The layout test asserts it by reading the tree;
  construct a declaration with neither and watch it go red.
- On shad-gpu, `cargo test --lib --features gpu -- --test-threads=1` runs 55 cases in roughly what
  the five staged binaries took. Materially longer means cpu cases leaked in.

### Test code is separated

- `git grep -n '#\[cfg(test)\]' -- peacockdb-core/src` returns only `mod tests` declarations.
  Anything else is test code in a production file, which is what this task exists to end.
- `git grep -n '#\[test\]' -- peacockdb-core/src` returns only paths containing `test`.
- The transitive gate is checked by construction: add a line in `src/tests/` referencing a private
  item, confirm `cargo build --release` still succeeds; add a line in `plan/` referencing
  `crate::tests::`, confirm it fails with `E0433`. Revert both.

### CI

`test_ci_coverage` shrinks here, so it is the one guard that must be shown red rather than merely
green. Delete the `--lib --features gpu` line from `pipeline.yml` and confirm it fails — that
assertion is all that stands between 55 device cases and silently not running. Do the same for the
`--lib` line and the CLI build.

The two scripts change with it, per the section above; verify `build-test.sh --gpu` and
`--rust-only` both still produce a non-empty derived suite, since that guard is now one move from
firing.

### Then build-test.md

Edited last, from the measured numbers rather than from this spec. Its two tables must add to the
headline figure, and that arithmetic is how the page is checked. A spec number and a measured number
that disagree mean the move is wrong, not the page.

## Done when

`peacockdb-core` exposes exactly the eight items in the table below, `test_support` exposes only
string and test-local signatures, and bare `pub` appears nowhere else in `src`; no
production file contains a `#[test]`; no `#[cfg(test)]` sits anywhere but on a `mod tests`
declaration; every `mod tests` carries one of the two gates; every test-only path in `src/` carries
`test` in its name; `test_ci_coverage` is near 300 lines and
asserts the `--features gpu` run; `build-test.md`'s two tables add to the headline; and the case
count is unchanged at 1,083 for the crate — this task moves tests, it does not delete any.
