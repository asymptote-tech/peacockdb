# One rule, checked wherever it is written down

Kind: production

**This task closes [#233](../tickets/corpus-coverage.md#t233)** (the plan validator does not
check that a pass-through node keeps its input's column count) **and
[#174](../tickets/corpus-coverage.md#t174)** (two clamps for one rule, and nothing compares them).
First of chain K, which runs beside chain J while the GPU host is down: everything here runs on
the cpu tier. No production behaviour changes; both tickets are a check that is missing.

## Why it happens

**#233.** `declared_width` (`plan/validate.rs`) checks a node's column count for the kinds that
compute a width, and for a Filter without a projection, against its input. The other pass-through
kinds — Sort, CoalesceAllBatches, AccumulateBatchesAndSort, Limit, MergePartitions,
EmitPartitions, MergeSortedPartitions — fall to its `_ => return Ok(())` arm and reach only
`types_across_the_edge`, which zips the node's fields with its input's. `zip` stops at the shorter
list, so a sort declaring three columns over five passes both checks. The planner cannot build
that tree; a test that rewrites a planned tree and calls `validate` can. (The ticket lists the
filter among them; `declared_width` already covers it.)

**#174.** A limit's row range is clamped twice: `RowRange::clamp` (`executor/mod.rs`, calling
`row_range::clamp` in `executor/row_range.rs`; `(offset, length)`) for the cpu, C++ `clamp_row_range` (`node_session.cpp`, `(begin, end)`) for
the device's export and slice. Each side has its own cases — 4 in `executor/row_range/tests.rs`,
10 in `cpp/tests/cpu/test_executor.cpp`'s four `ClampRowRange` tests, 14 in all — and no case
reaches both. A third copy is the driver's mock: `MockUnload::unload`
(`executor/driver/tests/mock.rs`) clamps with its own `min` arithmetic, so the driver's limit
tests check that copy and not the shipped rule.

## The work

1. **#233.** `types_across_the_edge` compares the node's field count with its input's before the
   zip. A mismatch is `PlanError::Invalid` naming the node and both counts, worded as
   `declared_width`'s message is.
2. **#174, the cases.** `testdata/fixtures/row-range-clamp.txt` holds every existing case, one
   per line: `offset length rows -> begin end`, `max` spelling `u64::MAX` (in `offset` and
   `length` only), `#` comments, grouped as to-the-end, past-the-end, empty, overflow. `begin
   end` is the expected half-open span. The fixtures README gains a paragraph for it, and its
   opening line, which says every fixture is read by two crates, is reworded.
3. **#174, Rust.** `row_range/tests.rs` reads the file through `test_support::testdata_root()`
   and checks, per line, `RowRange { offset, length }.clamp(rows)` mapped `(o, l) → (o, o + l)`
   against `(begin, end)`. Its literal cases go.
4. **#174, C++.** `peacock_cpu_tests` takes the `PEACOCK_TESTDATA_DIR` compile definition, read
   with the environment override as `peacock_plan_tests` does. `ClampRowRange` checks
   `clamp_row_range(offset, length, rows)` per line; all four tests' literal cases go.
5. **#174, the mock.** `MockUnload::unload` calls `rows.clamp(batch.rows as u64)`.
   `driver/tests/limit.rs`'s counts stay as they are.
6. Both readers fail on a malformed line, naming its number, and on a file with no case. Both
   accept the same lines: a number is digits only (no sign, no space), as Rust's `u64` parse
   takes it.
7. Every host that runs `peacock_cpu_tests` from a copied tree is handed the fixture. The GPU
   runs install every `peacock_*_tests` binary and run them by glob, `peacock_cpu_tests` included,
   from a testdata tree filled by hand-written lists: `build-test-shadgpu.sh` (beside its
   `cost-registry.csv` rsync) and `pipeline.yml`'s GPU job (its `rsync_retry` list) each gain
   `testdata/fixtures`. `build-test.sh`'s `sync_fixtures` already sweeps every tracked testdata
   file. The rust lib's row-range test runs on no GPU host: those run `gpu_tests::` only.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/plan/validate.rs`, `plan/validate/tests.rs` | #233's check and its two cases |
| `testdata/fixtures/row-range-clamp.txt`, `testdata/fixtures/README.md` | the case table |
| `peacockdb-core/src/executor/row_range/tests.rs` | reads the table |
| `peacockdb-core/src/executor/driver/tests/mock.rs` | the mock calls the clamp |
| `cpp/tests/cpu/test_executor.cpp`, `cpp/CMakeLists.txt` | reads the table; the compile definition |
| `scripts/build-test-shadgpu.sh`, `.github/workflows/pipeline.yml` | `testdata/fixtures` provisioned to the GPU host |
| `llm-wiki/build-test.md`, `llm-wiki/tickets/` | counts; both tickets archived on merge |

Component-level API: none. No facade, trait, ABI, wire or golden change.

## Restriction

The validator check, the shared table and the mock. Neither clamp's code changes; if the table
shows them disagreeing, that is a ticket, not a fix here.

## Tests

- `plan/validate/tests.rs`: a `GpuSort` over a two-column source whose declared schema the test
  overrides (its `kind` field, reachable from `plan`'s own test module; the constructor derives
  the schema) to one column is refused, naming both counts, and so is one overridden to three;
  the sort as built passes. Each fails before the change.
- The fourteen table lines pass on both sides. A deliberately wrong line, tried once by hand and
  not committed, fails both suites on that line.
- The driver's limit tests green unchanged.

## Verification bar

- rust-only: `--lib`, every target green.
- C++: `ctest --test-dir cpp/build -L cpu`, locally, `cpp/build` configured against cuDF 25.02
  (`scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`).
- device: none.

## No GPU

Chain K runs without a GPU, so it does not contend with chain J for nebius-gpu. No device run,
no GPU cycle. This task reaches `done` when every CI job but the GPU tests is green; the GPU jobs
are not waited on. Its edits to `build-test-shadgpu.sh` and to `pipeline.yml`'s GPU job are not
run here; the next GPU run after merge is the first to exercise them.

## Completeness signoff

Solved under its constraints. The validator refuses a pass-through width mismatch; both clamps
and the driver's mock now answer to `testdata/fixtures/row-range-clamp.txt`; the fourteen cases
are the exact union of the two sides' old literals, and the two clamps agree on every one, so
#174 produced no divergence ticket.

One shortcut, and it is the chain's: no GPU. The `build-test-shadgpu.sh` and `pipeline.yml`
GPU-job provisioning edits ship verified by reading and a local rsync rehearsal, not by a run on
the host. No other shortcut or bandaid.

Owed at merge: archive #233 and #174, including `tickets.md`'s corpus-coverage count 33 → 31.
