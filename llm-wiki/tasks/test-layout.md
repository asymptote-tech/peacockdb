# 4 — tests down the source tree

Kind: production

Fourth of five, after [`module-layout.md`](module-layout.md) and
[`rmm-pool-budget.md`](rmm-pool-budget.md), and before [`test-support.md`](test-support.md).

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
```

`gpu` and `rust-only` are mutually exclusive and a `compile_error!` says so. `gpu` does not
propagate to `peacockdb-ffi`: that crate is a C ABI binding with no device-conditional code, and
adding a feature there would be a knob nothing reads.

**The `test-support` feature is declared here, because here is where a helper first has two
audiences.** Nine of the eleven moving targets read `tests/common/` — `golden_text`, `registry`,
`mode`, `memory_limit` and seven `mod.rs` helpers between them — and the seven binaries that stay
read the same files from outside the crate. An in-crate `#[cfg(test)]` module cannot see
`tests/common/`, and duplicating a helper on both sides of the boundary guarantees the drift.

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so no CI step passes a flag and a plain build cannot see the module. `src/test_support/` is then
the one place a helper with two audiences lives: `crate::test_support::…` from inside,
`peacockdb_core::test_support::…` from the binaries.

**A helper moves when a moving target needs it, and not before.** `golden_text.rs`, `registry.rs`,
`mode.rs`, `memory_limit.rs` and the shared `mod.rs` helpers move; `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` are read only by binaries that stay, so they stay too, and
`corpus.rs`/`corpus_gpu.rs` are [`test-support.md`](test-support.md)'s whole subject.

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
- Every test module declares the lowest rung it needs, per the ladder below, and its name carries
  that rung — `tests`, `ffi_tests`, `gpu_tests` — so a CI line can select one rung by path.

A dozen or so files hold inline `#[cfg(test)] mod tests { … }` and are split. **Do not work from
a list written here**: task 2 renamed and moved most of them and deleted `config.rs` outright, so
the count and the paths are both stale. Derive the set at the time of the move — the layout test's
own rule names them — and record what you found.

The mod.rs rule from `module-layout.md` applies to components and subcomponents, not to
implementation modules — an implementation module with unit tests is `foo.rs` beside `foo/tests.rs`.
Do not reach for `clippy::mod_module_files` to enforce the mod.rs rule: it cannot be scoped that
way and would reject exactly this pairing. The layout test can scope it, and has to exist anyway.

## The device guard becomes a feature

Today no `cfg` says "needs a device". `rust-only` says "no FFI linked", and `test_gpu_batch` is
`#![cfg(not(feature = "rust-only"))]` while running on a GPU-less runner. What actually keeps device
tests off CPU hosts is which binary CI runs where — and that mechanism disappears the moment those
tests are inside `--lib`.

Add a `gpu` feature. The three build shapes are a **ladder**: each rung adds a capability and
keeps everything below it.

| Build | FFI linked | Device assumed | Runs |
|---|:-:|:-:|---|
| `--features rust-only` | no | no | dataset-matrix, CPU steps |
| default | yes | no | dataset-matrix, FFI steps |
| `--features gpu` | yes | yes | shad-gpu only |

So a test declares the **lowest rung it needs**, and nothing declares what it excludes:

```rust
#[cfg(test)] mod tests;                                       // pure Rust
#[cfg(all(test, not(feature = "rust-only")))] mod ffi_tests;  // needs the FFI linked
#[cfg(all(test, feature = "gpu"))] mod gpu_tests;             // needs a device
```

Two conditions, both positive in meaning, and the sets nest rather than partition: the FFI set is
a subset of what a default build compiles, and a `gpu` build compiles all three. A reader answers
"which builds run this" from one attribute. `rust-only` is the only spelling available for the
middle rung — cargo features are additive and `rust-only` is subtractive, so there is no positive
`ffi` feature to write and inventing one would mean every ordinary build had to ask for the FFI
by name. A `compile_error!` on `all(feature = "gpu", feature = "rust-only")` keeps the ends of
the ladder exclusive.

**A module's name carries its rung whenever the rung is above the floor**, and that is what makes
a rung selectable. Cumulative shapes are the point of the ladder, but a CI line that runs a whole
shape re-runs every rung beneath it: unfiltered, the device host would drag the CPU unit cases
through `--test-threads=1` on the one serial resource in the suite, and the default-features line
would re-run what the `rust-only` line just ran.

| Rung | Module | Selected by |
|---|---|---|
| pure Rust | `tests` | nothing — it is the floor |
| FFI linked | `ffi_tests` | `-- ffi_tests::` |
| device | `gpu_tests` | `-- --test-threads=1 gpu_tests::` |

Those are path filters, not name filters, so they cannot suffer the trap `build-test.md` records
for filters that name a query. The layout test asserts name and gate imply each other in both
directions at both rungs: an `ffi_tests` module carries the `not(rust-only)` gate and a `gpu_tests`
module the `gpu` gate, and no module carries either gate without the matching name. Each CI line
then lists exactly its own rung, which is what makes "one line per rung" an accounting rather than
a slogan.

Two alternatives were considered and are not open. A runtime device check that skips is the shape
`build-test.md` already records shipping a hole — the binaries skip and exit 0, green having
verified nothing, which is why CI asserts sf40's presence itself. And `#[ignore]` already means
"disabled against a ticket" (#182), so reusing it for "needs a device" is one spelling for two
things.

## What moves into src/

Target names are the ones tasks 1 and 2 left behind, not the ones this spec was first written
against. The rung column is the ladder above: `rust` needs nothing, `ffi` needs the FFI linked,
`gpu` needs a device and lands in a `gpu_tests` module.

| Lands in | From | rung | N |
|---|---|:-:|--:|
| `plan/tests/` | `test_layout_injection` | rust | 4 |
| `planner/tests/` | `test_planner_join_capability` | rust | 13 |
| `planner/tests/` | `test_planner_join_refusals` | rust | 10 |
| `planner/tests/` | `test_null_analysis` | rust | 8 |
| `planner/tests/` | `test_plan_goldens` | rust | 19 |
| `wire/gpu_tests/` | `test_gpu_recipe_walk` | **gpu** | 10 |
| `executor/ffi_tests/` | `test_gpu_batch` | **ffi** | 3 |
| `executor/cpu_backend/tests/` | `test_cpu_executors` | rust | 1 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_executors` | **gpu** | 31 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_abi` | **gpu** | 4 |
| `executor/cpu_backend/gpu_tests/` | `test_murmur_conformance` | **gpu** | 10 |
| `src/tests/` | `test_cpu_end_to_end` | rust | 26 |
| `src/tests/` | `common/{injection,rebuild,join_fixture}.rs` | — | 0 |
| `src/test_support/` | `common/{golden_text,registry,mode,memory_limit}.rs` and the shared `mod.rs` helpers | — | 0 |
| | | | **139** |

`test_gpu_executors` is already a directory of five modules — `accumulate`, `backend`, `contract`,
`exec`, `join` — and moves as one, keeping that shape under `gpu_tests/`. `test_gpu_batch` is the
only occupant of the middle rung, which is why it is the shape proof the slices start with.

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
- **The layout test checks the shape**, because three things the compiler is happy with would
  still be wrong: a `#[cfg(test)]` attribute anywhere other than on a test-module declaration —
  which is how test code creeps back into a production file one item at a time; a name and a gate
  that disagree at either rung — `ffi_tests` without `not(rust-only)`, `gpu_tests` without `gpu`,
  or either gate on a module named `tests` — since the runs select by path and a mismatch either
  loses a case or drags it onto the wrong host; `driver/partitioned.rs` carries four
  item-level `#[cfg(test)]` attributes today (lines 670, 675, 680, 685); they are the first
  thing the first rule finds, and they move into the module with the tests that use them.

### Testdata paths move with the tests, and #49 is in the way

`tests/common/mod.rs` honours `PEACOCK_TESTDATA_DIR`, overriding the compile-time root "so a binary
built on one host can run on another". Every target that says `mod common` inherits that, including
the ones staged to shad-gpu. Nothing in `src/` does: seven sites bake the path instead —
`env!("CARGO_MANIFEST_DIR").join("../testdata/tpch.minimal")` in `estimator.rs` (×3),
`parquet_meta.rs`, `plan_text/mod.rs`, `translate/tests.rs` and `translate/schema_tests.rs`. That is
the residual [#49](../tickets.md#t49) names.

Moving eleven targets in-crate walks straight into it: they lose `testdata_root()` and land beside
the seven that do it the unportable way, and the device ones then run on shad-gpu **from a binary
built on another host**, which is the case the variable exists for.

So this task closes that residual, and `test_support` is where the root belongs rather than
`src/tests/` — the moved targets, the crate's own unit tests and the seven binaries that stay all
need it, which is the same two-audience argument the feature exists for. `test_support/testdata.rs`
honours `PEACOCK_TESTDATA_DIR` with the same compile-time fallback; `tests/common/mod.rs`'s
`testdata_root()` becomes a call to it rather than a second implementation, the seven `src` sites
call it too, and #49 closes with the sweep. Two spellings of one rule is what that ticket is
about, so any second copy re-files it one layer down.

### The exemption expires here, one slice at a time

`coding-style.md` says it outright: nine subcomponent paths under `executor/` are declared
`pub mod` rather than `mod` because test crates reach them, 60 bare `pub` items sit behind those
nine — 75 counting the two subcomponent facades the layout test skips — and "`test-layout.md` moves those test files into `src/`, and the whole exemption expires
with them". This task is where that happens, and it is not a consequence — it is work.

`PUB_MODULES` in `test_module_layout.rs` holds nine entries, each naming the test files that force
it, and **it is checked both ways**: an entry whose named files no longer force it is reported, and
a `pub mod` that is not in the register is a violation. So a slice that moves a forcing file has
exactly one green path — drop the entries that move invalidates **and** demote the `pub mod` they
sanctioned, in the same commit. Leaving either half for later is a red build, not a tidy-up.

Which slice closes what follows from the register's own `forced_by` lists: the injector trio takes
`cpu_backend/join` and `cpu_backend/source`, `test_cpu_executors` takes the rest of the
`cpu_backend` group, and `test_gpu_executors` and its child files take all four `gpu_backend`
entries. Task 2 recorded the trade this creates and left it deliberately: if
`test_gpu_executors.rs` alone stops naming `executor/gpu_backend`, the forward half goes red and
the three child-naming files cannot re-justify the entry. That is why the target moves whole.

**One piece of the next task comes forward, and only one.** `wire/tests.rs` names
`executor::cpu_backend::join::CpuJoin`, so raising the `cpu_backend` wall is an `E0603` on that
line unless `CpuJoin` is declared in `executor/mod.rs` first. That is one type and one delegation,
taken in the slice that raises the wall. The remaining hoist — the other thirteen backend types and
55 inherent methods — stays [`test-support.md`](test-support.md)'s, because nothing here forces it.

`CROSS_COMPONENT_REACHES`'s single entry is that same reach and dies with it. The register itself
is deleted in task 4, not here: this task empties it, and an empty register is still a register.

## What stays a separate binary

| Target | Why it cannot fold in | rung | N |
|---|---|:-:|--:|
| `test_cpu_corpus` | `inventory` collects per linked binary; the registry needs two | rust | 448 |
| `test_gpu_corpus` | the other half of that pair, and it writes env vars | **gpu** | 8 |
| `test_golden_format` | the format reader, over strings | rust | 24 |
| `test_corpus_goldens` | committed sections against their own arithmetic | rust | 20 |
| `test_ci_coverage` | reads the workflow yaml | rust | 6 |
| `test_cost_model` | `.cost.txt` re-derived from `.cpu.txt` | rust | 3 |
| `test_module_layout` | reads the tree, as `test_ci_coverage` reads the yaml | rust | 35 |

`test_module_layout` is task 2's, not this task's to write — **this task extends it** with the
rules below rather than inventing a layout test. Where this spec says "the layout test", it means
that target.

None of the seven names an item that would otherwise have to stay `pub` beyond what `corpus.rs` and
`corpus_gpu.rs` already force. The `inventory` constraint
survives untouched, which is the one that looked fatal: it collects per linked binary and the two
corpus binaries both stay.

## test_ci_coverage shrinks to about 300 lines

It keeps its job and loses most of its subject. From 720 lines:

- The target sweep now covers seven targets, not nineteen, and `INTENTIONALLY_NOT_IN_CI` drops from
  six entries to two.
- The three GPU target lists become one staging list of one binary plus the `--lib --features gpu`
  run.
- Its own matcher unit tests stay whole. They are the reason this guard can go red at all.

It gains assertions, and they are the most important ones in the file, because the ladder puts one
CI line under each rung and nothing else says a rung stopped running. **Four lines must exist**:
`--lib` under `--features rust-only`, `--lib` at default features, `--lib --features gpu --
--test-threads=1 gpu_tests::` on shad-gpu, and the CLI build. Each names its rung's filter, so each
lists exactly its own rung: the device line carries the 55 cases that move into `--lib` in this
task, and the default-features line the three of the middle rung, under `-- ffi_tests::`. Each gets the red-watch below — delete
the line, confirm the guard fails — because each is the only thing standing between a rung and
silence.

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
skipped once. **Delete that literal rather than renaming it.** The target stops existing: it becomes
an in-crate `gpu_tests` module, and this task's own list leaves `RUST_TESTS` with one entry. A
renamed literal would name a `--test` target that is not there, which is the failure
[#176](../tickets.md#t176) describes — cargo errors late, inside the cuDF leg. The ten lines of
explanation go with it, as below. Re-count the sixteen references before editing: tasks 1 and 2
moved several of these files.

`driver/mock.rs` and `driver/plans.rs` become `driver/tests/mock.rs` and `driver/tests/plans.rs`.
`translate/schema_tests.rs` already carries the word and stays.

## build-test.md

The test table is restructured in this task, not a later one, and the axis changes. Today one
table carries every language and every kind of test; it becomes two.

**The first table is the Rust tests against production code** — `peacockdb-core` and
`peacockdb-ffi` — **blocked by rung**, because the rung is what decides where a case can run and
each block's total is one CI line's case count. A bolded header row opens each block; within a
block the rows are grouped by tier, then by category; and the Why, Examples and N columns survive
as they are.

    **cpu — `--features rust-only`: no FFI, no device**
      crate integration, external
      crate integration, internal
      component · subcomponent · module unit
    **ffi — default features: FFI linked, no device**
      component
    **gpu — `--features gpu`: shad-gpu only**
      crate integration, external · component · subcomponent

The `Runs` column goes. It said which CI job runs a row, and once the repo guards move to the
second table there is no variation left inside a rung: cpu and ffi are dataset-matrix, gpu is
shad-gpu, and the block header says so once instead of every row repeating it.

**The ffi block has two rows and five cases** — `test_gpu_batch`'s three and `peacockdb-ffi`'s
`test_ffi` — and the table should show that rather than pad it. A thin rung is a fact about this
engine: almost nothing needs the FFI linked and no device.

**The second table is everything else**: the C++ suites, the Python prototype and validators,
`cost-report`, and the repo guards — `test_ci_coverage`, which reads the workflow yaml,
`test_module_layout`, which reads the source tree, and `test_golden_format`, which tests the
harness's own format reader. None of the three runs engine code, and grouping them by what they
guard is more use than filing them by a rung they do not have.

`test_corpus_goldens` stays in the *first* table, cpu rung, crate integration external. It runs no
engine code either, but its subject is the engine's output and it goes red when the rendering
drifts, which is the distinction that matters.

Assign each file by where its test module is declared, not by eye: `driver/accounting/tests.rs` is
a unit test of an implementation module, `driver/tests/` is the subcomponent's, and `nodes/tests/`
is `plan`'s. The cross-checks are one per rung plus the binaries, and **every figure in this table
is pre-task-2**: it predates `test_module_layout`'s 35 cases and five target renames, so rebuild it
from the baselines. The arithmetic that must hold is the page's, not this spec's — the two tables
add to the headline figure.

The two tables still have to add to the page's headline figure, which is how the page is checked.
Task 2 reported a four-case discrepancy there and fixed it in its completeness commit, so the
arithmetic is sound entering this task; keep it sound rather than re-deriving it.

Two facts the new shape makes visible that the current table cannot. The gpu block is what
shad-gpu runs, entire — **55** inside `--lib` selected by `gpu_tests::`, plus **8** in one binary —
so the cost of the serial host is one number a reader can find. And the coverage distribution
survives inside the cpu block, where `executor/driver` and `plan` are the heaviest subcomponent
and component rows and the corpus's 448 is one target rather than a tier.

## The three GPU target lists, and the two scripts

Today five GPU targets are named in three places that `test_ci_coverage` asserts agree:
`build-test.sh`'s `gpu_runtime_targets()`, `build-test-shadgpu.sh`'s `RUST_TESTS` at line 30, and
`pipeline.yml`'s staging loop. After this task the list is **one target plus a lib build**, which is
a bigger change to those scripts than to the lists.

**`build-test-shadgpu.sh`**

- `RUST_TESTS=(test_gpu_corpus)` — one entry.
- `stage_cargo_test_binary` resolves a built binary by matching `target.name` against a `--test`
  name in cargo's json. It needs a second form for the lib test target, whose json entry has
  `kind: ["lib"]` and `test: true` and whose `target.name` is the crate name. Stage it under an
  explicit filename — `peacockdb_core_gpu_lib` — because the run loop globs
  `cpp/install/rust-tests/*` and a bare crate name reads as ambiguous beside the target binaries.
- The build must pass `--features gpu`, and **the lib binary alone takes the `gpu_tests::`
  argument**: under the ladder it holds every rung, so an unfiltered run would put the CPU unit
  cases through `--test-threads=1` on the one serial host. The loop passes arguments to every
  staged binary alike, so this is a per-binary argument the loop does not have today.
- **The `PASSED 0 tests` guard becomes load-bearing.** A path filter that matches nothing runs no
  cases and exits 0 — a rename of the `gpu_tests` convention would be invisible without it. It is
  already there, and the spec's point is that it is now what catches this, not an incidental check.

**`build-test.sh`**

- `gpu_runtime_targets()` shrinks to `test_gpu_corpus` plus the lib entry.
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
  gpu`. Under the ladder these are not disjoint — the second is a superset — and the filter is what
  makes the runs disjoint. Say so where the lists are written, or the next reader reads the
  supersetting as a bug.
- The "derived suite must not be EMPTY" guard stays and gets closer to firing. With one derived
  target left it is one move away from being the thing that catches a mistake, so leave it.

**`pipeline.yml`** takes the same three-list change, plus two steps. The device one is
`cargo test --lib --features gpu -- --test-threads=1 gpu_tests::` on shad-gpu. The other is the
middle rung: `cargo test -p peacockdb-core --lib -- ffi_tests::` at default features on
dataset-matrix, which
**replaces the `--test test_gpu_batch` step it retires** — the job already compiles that feature
shape for `test_gpu_batch` and `peacockdb-ffi --test test_ffi`, so this is a swap, not a second
compile of the DataFusion stack, and the cache-thrash rule in `build-test.md` is not in play.
**`test_ci_coverage`** then compares three one-entry lists and asserts the four lines above.

## The surface, after this task

**Two counts, and only one of them is this task's.** Bare `pub` in `src/` is a raw number in the
hundreds — 249 measured after task 2 — and it stays there, because most of those items are `pub`
for no reason anyone can name and demoting them is [`test-support.md`](test-support.md)'s subject.
What this task ends is `pub` **that an external consumer forces**, and that count reaches eight.

Sixteen items are `pub` for a reason at the end of this task, in six files: the CLI's eight, plus `GpuNode` and `validate`
(`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`, `GpuBackend` and
`GpuContext` (`executor/mod.rs`), and `render_run` (`plan_text/mod.rs`). All eight of those are
forced by `tests/common/corpus.rs` and `corpus_gpu.rs`, and [`test-support.md`](test-support.md)
removes them.

Five of them were lifted out of a subcomponent by task 2 and must stay lifted: `run` and
`RunReport` from `executor/driver`, `CpuBackend` from `executor/cpu_backend`, `GpuBackend` and
`GpuContext` from `executor/gpu_backend`, all declared in `executor/mod.rs` today. That is the rule
doing its work — the wall forces whatever must be public upward — and this task must not push any
of them back down.

## The wiki this moves

- **`build-test.md`'s test table is restructured here**, per the section above: the Rust tests
  against production code in one table blocked by rung — cpu, ffi, gpu — grouped by tier and then
  category inside each block, with Why, Examples and N kept and `Runs` dropped; the C++ suites,
  the Python sets, `cost-report` and the three repo guards in a second. The two must add to the
  headline figure, and the four-case discrepancy that sum has today is closed here.
- **`coding-style.md` gains the test-code rules** — no test code in a production file, a module's
  unit tests in a child module of their own, `test` in every test-only path, and the rung ladder:
  a test module declares the lowest build shape it needs and its name says which — `tests`,
  `ffi_tests`, `gpu_tests` — with name and gate implying each other at both rungs above the floor. It also loses the `test_inc2_conformance` exception paragraph that opens its Names
  section, since the rename closes it.
- **`coding-style.md`'s visibility section is amended, not rewritten**: the previous task states
  the rules, this one records what this task settled — the exemption register empty, `pub mod` down
  to six, and eight items still `pub` because a test crate forces them, which
  [`test-support.md`](test-support.md) then removes along with the rest of the raw count. The
  `test_support` signature rule belongs to [`test-support.md`](test-support.md), which is where
  the corpus facade makes it load-bearing; the feature itself arrives here.
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
3. The `pub`/`pub(crate)` item dump from the end of task 2 — **it is a script, not a memory**:
   `visibility-dump.py`, whose output at the end of that task is `visibility-items-final.txt`.
   Re-run it here rather than inventing a second count, and state which of its rows the ladder
   below counts: bare `pub` only, test-gated items excluded. `case-inventory.sh` and
   `compare-inventory.sh` beside it are the leaf-name tooling for (1). All three move out of
   `module-layout-baselines/` into `scripts/` in this task's first slice: they are checks in this
   task and in task 4, so they outlive the task that wrote them.
4. Warning counts from clean builds in all three feature shapes.

**Every figure in this spec predates task 2** — which renamed five targets, added
`test_module_layout` and its 35 cases, and moved the tree. Take the baselines first and work from
them. Where a measured number and a number written here disagree, the measurement is right and the
sentence is stale; say so in the detail file rather than bending the move to fit the page.

### The invariant: the case set is preserved, the paths are not

A case that disappears here is silent. Nothing goes red — a target runs 30 tests instead of 31, and
no assertion knows it was meant to run 31.

- **Leaf-name set equality.** The union across the three lib shapes and the seven binaries must
  equal the baseline union exactly. Not the count — the set, so a case deleted and another
  duplicated cannot cancel out. The lib shapes nest, so the union is taken after dedup; that is a
  consequence of the ladder and not a smell.
- **The rungs are what the counts must show**, each a separate assertion: `--lib` under
  `rust-only` lists the pure-Rust set; `--lib` at default features filtered by `ffi_tests::` lists
  **exactly the three** middle-rung cases; `--lib --features gpu` filtered by `gpu_tests::` lists
  the device set **and nothing else**. If either filtered run selects a case from a lower rung, a
  name and a gate disagree and the case is about to run on the wrong host.
- **The total is unchanged.** This task moves tests; it deletes none. Take the figure from the
  baseline rather than from this spec, which was written before task 2 added 35 cases.

### Move in slices

**A slice is a dispatch, not just a commit.** Each one ends by appending its state to
`test-layout-detail.md` — what moved, what the ladder number is now, what is unproven — and handing
back; the next slice starts in a fresh window from the baselines and that file. Task 2 was the
lesson: the same shape, five components with a commit each, run as one dispatch that spent eight
hours and died on a context limit with the work unreported.

**Slice 0 comes before any move**, because nothing can move until it exists: the `gpu` feature with
its `compile_error!`, `src/tests/testdata.rs` with the seven existing sites converted to it, and
`test_module_layout` extended with the rules above. It moves no test and its proof is that the
three build shapes still compile and the suite is unchanged.

Then one commit per target, ascending by how much it forces: `test_gpu_batch` (3 cases, 2 items)
first as the shape proof — it is also the only middle-rung target, so it proves the rung and its CI
swap together — then the four injector consumers with `injection`/`rebuild`/`join_fixture`, then the
executor tiers, then the rest. After each, the leaf-name set for that target must have moved from
its binary's list into the right module's list and nowhere else.

**Two ladders, and every slice moves one of them.** The surface as it falls — the spec's figures
were 108 → 79 → 50 → 15 → 8 and are re-derived from the baselines — and the register beside it,
**9 entries → 0**, with the `pub mod` count falling 15 → 6 as each is demoted. A slice that moves
neither moved the wrong thing.

### The device gate

- `cargo build --features gpu` and `--features rust-only` both succeed; the `compile_error!` fires
  when both are passed together, and that is checked by trying it.
- **Name and gate agree in both directions.** The layout test asserts it by reading the tree:
  construct a `gpu_tests` module without the gate, and a `gpu`-gated module not called `gpu_tests`,
  and watch each go red.
- On shad-gpu, `cargo test --lib --features gpu -- --test-threads=1 gpu_tests::` runs the device set
  in roughly what the five staged binaries took. Materially longer means the filter is selecting
  more than it should; `PASSED 0 tests` means it is selecting nothing.

### Test code is separated

- `git grep -n '#\[cfg(test)\]' -- peacockdb-core/src` returns only test-module declarations —
  `mod tests` and `mod gpu_tests`. Anything else is test code in a production file, which is what
  this task exists to end, and `driver/partitioned.rs` lines 670-685 are where it starts.
- `git grep -n '#\[test\]' -- peacockdb-core/src` returns only paths containing `test`.
- The transitive gate is checked by construction: add a line in `src/tests/` referencing a private
  item, confirm `cargo build --release` still succeeds; add a line in `plan/` referencing
  `crate::tests::`, confirm it fails with `E0433`. Revert both.

### CI

`test_ci_coverage` shrinks here, so it is the one guard that must be shown red rather than merely
green. Delete each of the four asserted lines from `pipeline.yml` in turn and confirm the guard
fails on each: the device line, the default-features `--lib` line, the `rust-only` `--lib` line and
the CLI build. One rung per line, and the assertion is all that stands between a rung and silently
not running.

The two scripts change with it, per the section above; verify `build-test.sh --gpu` and
`--rust-only` both still produce a non-empty derived suite, since that guard is now one move from
firing.

### Then build-test.md

Edited last, from the measured numbers rather than from this spec. Its two tables must add to the
headline figure, and that arithmetic is how the page is checked. A spec number and a measured number
that disagree mean the move is wrong, not the page.

## Done when

exactly eight items in `peacockdb-core` are `pub` because a test crate forces them — `GpuNode` and
`validate` (`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`,
`GpuBackend` and `GpuContext` (`executor/mod.rs`) and `render_run` (`plan_text/mod.rs`), all eight
forced by `corpus.rs` and `corpus_gpu.rs` and removed by [`test-support.md`](test-support.md);
`PUB_MODULES` is empty and `pub mod` is down from 15 to six, counted by `visibility-dump.py`; the
raw bare-`pub` count is whatever task 4 inherits and is not this task's claim. No
production file contains a `#[test]`; no `#[cfg(test)]` sits anywhere but on a test-module
declaration; every test module declares the lowest rung it needs and is named for it, with name
and gate implying each other at both rungs above the floor; every test-only path in `src/` carries `test` in its name;
`crate::test_support::testdata::root()` is the only testdata root anywhere, called by the crate's
unit tests, the moved targets and `tests/common/mod.rs` alike, and [#49](../tickets.md#t49) closes
with it; a plain `cargo build` cannot name `test_support`; `test_ci_coverage` is near 300 lines and asserts one CI
line per rung plus the CLI build; `build-test.md`'s two tables add to the headline; and the leaf-name
set is the one the baselines recorded — this task moves tests, it does not delete any.
