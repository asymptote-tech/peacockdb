# guard-checks — run detail

What a restarted coordinator needs. Spec: [`guard-checks.md`](guard-checks.md); plan:
[`guard-checks-impl.md`](guard-checks-impl.md).

## Standing facts

- Chain K, first task, base master. Branch `ENS-guard-checks`, forked at `8806a3c3`.
- No GPU on this chain (board header). No device build, no device run. `done` once every CI
  job but the GPU tests is green; the GPU jobs are not waited on.
- PR targets master, since this is the first task of the chain.
- Verification is local: rust-only `--lib`, and `ctest --test-dir cpp/build -L cpu` with
  `cpp/build` configured against cuDF 25.02
  (`scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`).

## Dispatch log

### 2026-10-08 — round 1, developer

verda unreachable at dispatch (`Temporary failure in name resolution`), so the developer was
told to run everything locally. Board moved `approved to build` → `building` in the same commit
as this file.

### 2026-10-08 — round 1, developer result

All four plan tasks done; every work step in `guard-checks-impl.md` is ticked. The four commit
steps are annotated rather than ticked — the developer does no git operations.

**Files touched** (13 modified, 1 new; nothing committed):
`peacockdb-core/src/plan/validate.rs`, `plan/validate/tests.rs`,
`testdata/fixtures/row-range-clamp.txt` (new), `testdata/fixtures/README.md`,
`peacockdb-core/src/executor/row_range/tests.rs`, `executor/row_range.rs`,
`executor/mod.rs`, `executor/driver/tests/mock.rs`, `cpp/tests/cpu/test_executor.cpp`,
`cpp/CMakeLists.txt`, `scripts/build-test-shadgpu.sh`, `.github/workflows/pipeline.yml`,
`llm-wiki/build-test.md`.

#### What was subtle

- **`build-test.md`'s "Plan types" row is three modules, not one.** It links
  `plan::validate::tests` but its 38 was `plan::validate::tests` (30) + `plan::layout::tests`
  (4) + `plan::aggregate::tests` (4). The plan's +3 → 41 is right for that reason, not because
  `validate::tests` reached 41.
- **`build-test.md` had a second off-by-one the plan did not know about.** Reconciling every one
  of the 604 `--lib` cases against the cpu block's rows found `Recipes per join type` claiming 23
  where `wire::tests` has 24, and no other row covers `wire::tests`. Fixed here (a stale count is
  documentation, so `coding-style.md` says fix it where you find it, never file it). That is why
  the page lands at Rust 1860 / grand 2335 rather than the plan's predicted 1859 / 2334, and why
  `--lib` is 604 rather than 603. Every header is now exactly the sum of its rows, and the cpu
  block's `--lib` figure equals the binary's own `--list` total.
- **The mock's old copy really was blind to the shipped clamp, and the new one is not.** Proved
  both directions by hand (not committed): with the OLD mock arithmetic and `row_range::clamp`
  deliberately taking one row fewer, all 13 `driver::tests::limit` cases stayed green — the exact
  claim in #174. With the NEW mock and the same broken clamp, 7 of 13 go red. With the new mock
  and the correct clamp, 13 green. Without that second run the mock change is unfalsifiable: old
  and new agree on every input the driver tests feed.
- **Both clamp doc comments carried a sentence this task falsifies** — "the two answering
  differently would be a divergence no test of either one alone could see", in
  `executor/mod.rs`'s `RowRange::clamp` and in `executor/row_range.rs`'s `clamp` (the same five
  lines, duplicated). Replaced with a clause naming the shared table. Two files beyond the spec's
  scope table, no behaviour and no logic touched; flagged to the human rather than done silently.
  The C++ docs (`plan_executor.h`, `node_session.cpp`) make no such claim and were left alone.
- **`rustfmt` reorders one unrelated `use` in `driver/tests/mock.rs`.** `coding-style.md` says to
  run it over files you touch, so the reorder is kept; it is the one hunk in the diff that is not
  this task's.
- **The two clamps agree on all fourteen lines.** No disagreement to report, so no ticket.

#### Verification, with counts

- `cargo test --features rust-only -p peacockdb-core --lib` → **602 passed, 0 failed, 2 ignored**
  (604 listed; the 2 ignored are pre-existing).
- `cargo test --features rust-only -p peacockdb-core --test test_ci_coverage` → **9 passed, 0
  failed**. Green and still accurate: the change adds no Rust target.
- `cargo test --features rust-only -p peacockdb-core --test test_module_layout` → **17 passed, 0
  failed**.
- `ctest --test-dir cpp/build -L cpu` → **1/1 passed**; the binary itself is **12 passed** where
  15 were (4 `ClampRowRange` tests became 1).
- `cpp/build` configured against cuDF **25.02.02** via `scripts/build.sh --configure --build
  --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12`. Clean, no warnings.
- Clippy on the lib reports 10 warnings, all pre-existing and none on a line this task added.

#### Negative proofs (each by hand, reverted, none committed)

Each #233 case before the change: the "fewer" and "more" cases FAILED with
`expected an invalid plan naming … got Ok(())`; the passing case passed. Then, on the fixture,
four edits each tried once and reverted — a wrong `end`, a short line, `max` in the `rows`
column, and a file holding only a comment. All four fail **both** suites naming
`row-range-clamp.txt:12` (or "holds no case"), so neither reader can pass over zero cases.
`PEACOCK_TESTDATA_DIR=/nonexistent` fails with "cannot read" plus "holds no case".

#### The two GPU-host edits, which no run here exercises

Checked mechanically only, per the no-GPU constraint:
`bash -n` on `scripts/build-test-shadgpu.sh` and on `scripts/lib/shadgpu-env.sh` (exit 0);
`pipeline.yml` parsed with `yaml.safe_load` (7 jobs); the edited `run:` block extracted from the
parsed YAML and `bash -n`'d on its own (exit 0), which also proves the `\`-continued eight-argument
`rsync_retry` list survives YAML block-scalar rendering as one command. Both wrappers pass `"$@"`
straight to `rsync`, so the argument shape was checked by running both flag sets against a local
destination: each lands `<dest>/fixtures/row-range-clamp.txt`, which is the path the gtest builds
from `PEACOCK_TESTDATA_DIR`. `shellcheck` is not installed on this host.
`build-test.sh`'s `sync_fixtures` needed no edit — its `git ls-files --cached --others` sweep
already lists the new fixture.

### 2026-10-08 — round 1 green, PR #170 open

The developer reported green: rust-only `--lib` 602 passed, `test_ci_coverage` 9, `test_module_layout`
17, `ctest -L cpu` 12 (was 15 — four literal `ClampRowRange` tests became one table-driven test).
Both #233 cases fail before the change; four malformed-fixture edits each fail both suites naming
the line. The two clamps agree on all fourteen lines, so #174 produced no divergence ticket.

Committed as `d33a95c4`, pushed, PR #170 against master (base verified, 2 commits).

Two things to carry into review:

- Two files sit outside the spec's scope table: `executor/mod.rs` and `executor/row_range.rs`,
  comment-only. Each carried the sentence #174 quoted — that a divergence between the two clamps
  is one no test of either alone could see — which this task falsifies, so leaving it would break
  the code-comments-agree rule. Accepted.
- `build-test.md` carried a second stale count the plan did not predict: `Recipes per join type`
  said 23 where `wire::tests` has 24. Fixed in place, which is why the page lands at Rust 1860 /
  grand 2335 / `--lib` 604 rather than the plan's 1859 / 2334 / 603.

### 2026-10-08 — review round 1: 1 important, 2 nits, 0 blocking

**important — the verification bar was not fully met.** The spec's bar says rust-only "`--lib`,
every target green"; the round-1 evidence covers `--lib`, `test_ci_coverage` and
`test_module_layout` but not `test_cpu_corpus` (555), `test_corpus_goldens` (26) or
`test_cost_model` (3). `validate` is on the planner's hot path (`planner/pipeline.rs:38`, `:73`),
so every corpus query runs the new refusal. Not suspected breakage — all seven pass-through
constructors set `schema: input_schema(...)`, so the check cannot fire on a constructor-built
tree, and #233 itself records that no planned tree has the shape. Bar compliance only.

**nit** — `cpp/tests/cpu/test_executor.cpp:212-214`: the three-line comment that introduced the
four old `ClampRowRange` tests now sits above the inserted `namespace {` holding the file reader,
not above the test it describes.

**nit** — `testdata/fixtures/README.md:3-4`: the reworded opening left "The numbers are a real q6
section's shape, shortened." standing as a directory-wide claim when it is true of
`sectioned-cost.txt` alone.

The reviewer confirmed the three guards can all go red, re-implementing both fixture readers and
both clamps over the committed table and six mutations (every mutation reds both sides on the same
line number), enumerated the eight node kinds the #233 check reaches and found no remaining
pass-through gap, reasoned the `MockUnload` redirection has bite from the committed code, read both
GPU-host provisioning edits as correct, and summed every `build-test.md` header against its rows
(all four exact).

### 2026-10-08 — round 2, developer: round-1 findings addressed

All three findings closed. No production code changed in this round.

**important — the full rust-only bar.** Every target that compiles under `--features rust-only`
was run, not just the three the finding named. All nine compile (the device/ffi ones compile and
gate their cases off at runtime), so "every target green" is nine runs:

| target | result |
|---|---|
| `--lib` | 602 passed, 0 failed, 2 ignored (604 listed) |
| `test_cpu_corpus` | **555 passed, 0 failed** |
| `test_corpus_goldens` | **26 passed, 0 failed** |
| `test_cost_model` | **3 passed, 0 failed** |
| `test_golden_format` | **26 passed, 0 failed** |
| `test_ci_coverage` | 9 passed, 0 failed |
| `test_module_layout` | 17 passed, 0 failed |
| `test_gpu_corpus` | 0 cases under `rust-only` (gated to the device rung) |
| `test_node_timing` | 0 cases under `rust-only` (gated to the ffi rung) |
| `peacock_gpu_benchmarks` (`--skip bench_`, as CI) | 0 cases under `rust-only` |

The three corpus targets ran as one command with `-- --test-threads=2` per `build-test.md:989`,
on a 31 GiB host; 284s wall, exit 0, no OOM. The reviewer's prediction held — the new refusal
fires on no constructor-built tree, so all 555 corpus queries pass the hot-path `validate`
unchanged.

`test_golden_format` was worth adding to the list beyond the finding's three: it reads
`testdata/fixtures/sectioned-cost.txt`, so it is the one target outside `--lib` that this task's
`testdata/fixtures/` edits could have disturbed. It did not.

Two incidental confirmations from these counts: 555 + 26 + 3 matches the cpu block header's
per-binary figures exactly, and 1188 (cpu) + 26 + 9 + 17 = 1240 is every rust-only case in the
crate — so `build-test.md`'s external-binary numbers are now checked against code too, not only
its `--lib` rows.

**nit — the reassigned comment.** The three lines moved to immediately above
`TEST(ClampRowRange, AnswersEveryCaseInTheSharedTable)`. Worth naming the mechanism for the next
reader: nothing read as missing, because the anonymous `namespace {` the reader lives in needs no
comment of its own, so the block simply acquired a new owner. `coding-style.md`'s entry on this is
exact — an insertion is contiguous and shows only in a diff.

**nit — the README's opening.** The q6 sentence moved into `sectioned-cost.txt`'s own paragraph.
The opening is now "Committed samples of a format, each read by the code under test rather than by
a generator" — which also drops the "two readers read independently" claim the round-1 rewording
had merely narrowed: it is true of `sectioned-cost.txt` and of `row-range-clamp.txt`, but
`two-row-registry.csv` has the one reader `Registry::load`, so no two-reader phrasing covers all
three. What the three actually share is that the fixture is parsed by the production reader rather
than produced by one.

**Re-verified after both nit edits:** C++ rebuilt clean with no warnings,
`./cpp/build/peacock_cpu_tests` 12 passed, `ctest --test-dir cpp/build -L cpu` 1/1 passed,
`git clang-format --diff` reports no change, and `--lib` 602 passed / 0 failed / 2 ignored.

### 2026-10-08 — round 1 findings closed, state completing

Committed as `628a4298`. The full rust-only bar now stands in evidence: every one of the nine
targets in `peacockdb-core/tests/` compiles under `--features rust-only`, and all of them ran —
`test_cpu_corpus` 555, `test_corpus_goldens` 26, `test_cost_model` 3, `test_golden_format` 26,
`--lib` 602 (2 ignored), `test_ci_coverage` 9, `test_module_layout` 17, with `test_gpu_corpus`,
`test_node_timing` and `peacock_gpu_benchmarks` reporting zero cases because their rungs gate off.
The three corpus targets ran as one command under `-- --test-threads=2`, 284s, exit 0.
`test_golden_format` is the target worth remembering here: it reads
`testdata/fixtures/sectioned-cost.txt`, so it is the one binary outside `--lib` this task's
`testdata/fixtures/` edits could have disturbed.

Both nits closed; the C++ side re-verified after the comment move (`peacock_cpu_tests` 12 passed,
`ctest -L cpu` 1/1, `git clang-format --diff` clean) and `--lib` re-run at 602.

No blocking or important finding outstanding, so the task moves to the completeness pass.

### 2026-10-08 — completeness pass (analyst): what is missing

Read as one change against the spec's Scope / Restriction / Tests / Verification bar,
`architecture.md` and `build-test.md`. No build, no run; arithmetic and text analysis only.

**`architecture.md`: no sentence falsified.** The row-range convention under Interfaces
("A row range is `[offset, offset+length)`", l.962-965) is a statement about the ABI's
`slice_handle` / `result_from_handle` and never mentions the Rust twin or any test, so the
clause the branch replaced in `executor/mod.rs` and `executor/row_range.rs` has no counterpart
there; all fourteen table lines satisfy it. The page says nothing about `declared_width`,
`types_across_the_edge` or a pass-through column count. "Nothing checks that a child's column
*order* is what the plan assumed" (Column indexing → What guards it) stays true: the new check
compares declared counts, not emitted order. No edit owed.

**Verified complete.** All 14 lines are the exact union of the old literals — Rust's 4 are
lines 3, 4, 7, 10 and C++'s 10 are the other ten, nothing dropped and nothing to dedup; both
clamp rules recomputed over the table off-tree, 14/14 agree with the expected spans. The #233
check reaches every pass-through kind: the 7 the ticket names plus a projection-less Filter via
`types_across_the_edge`, `GpuUnion` / `GpuInterleave` via `union.rs`'s `check_branch_schemas`
(count, names and types, pre-existing), `GpuUnload` being a sink with no schema. Four paths run
`peacock_cpu_tests` and all four reach the fixture: pipeline.yml's dataset-matrix (`ctest -L cpu`
in the checkout, both cuDF legs), pipeline.yml's GPU job (rsync list + `PEACOCK_TESTDATA_DIR`),
`build-test-shadgpu.sh` (rsync + `PEACOCK_TESTDATA_DIR`), and `build-test.sh`'s plain cpu mode
(`sync_fixtures`' git sweep, read through the pre-existing `/media/data/peacockdb` symlink the
mode already needs). `build-test.md`'s headers recomputed from its rows independently: Rust 1860,
C++ 94, Python 381, grand 2335, cpu block 1188 = 604 + 555 + 26 + 3 — all exact, and 604 = 602
passed + 2 ignored. 12 `TEST(` in `test_executor.cpp`.

**Three items reported to the coordinator** (full text in the analyst's reply): `hacks-audit.md`
§6 and §7 still describe both fixes as open; the C++ clamp's own comment is the one twin comment
that does not name the shared table; two sentences the branch wrote overclaim
(`validate.rs`'s `_` arm, `fixtures/README.md`'s opening). Plus the merge bookkeeping list:
`tickets.md:24` (count 33, both IDs), `corpus-coverage.md:45-46` and the two bodies. No code
names #174 or #233, so archiving them breaks no refusal test.

### 2026-10-08 — completeness pass closed, all three findings applied

The reviewer's reading found 0 blocking and 0 important. The analyst's found 3 important, all
comment or markdown, so all three were applied directly rather than routed to a developer:

- `llm-wiki/reports/hacks-audit.md`: §6 and §7 cut, both fully fixed by this branch, and with them
  the "Tests that would not catch the bug they exist for" bullet about `driver/tests/limit.rs`,
  which the mock redirection falsifies. The header's prune line is now dated twice. Numbering gaps
  are the page's own convention — tickets cite the original numbers.
- `cpp/src/node_session.cpp`: `clamp_row_range`'s comment gained the clause its two Rust twins
  carry, naming the shared table. It was the side a reader edits and the only one that did not say
  where the cases live.
- `peacockdb-core/src/plan/validate.rs`: the `_` arm's comment named only
  `types_across_the_edge`, which is wrong for union and interleave — they reach that arm too and
  are checked by `union::check_branch_schemas`. Both halves now named.
- `testdata/fixtures/README.md`: the opening claimed each fixture is read by the code under test,
  which is false of `row-range-clamp.txt` — two test-local parsers read it and neither clamp sees
  the file. Reduced to what all three entries share.

Neither reading falsified a sentence in `architecture.md`. Its row-range convention bullet under
Interfaces is a statement about the ABI, not about how the rule is tested, and the page carries no
prose about `declared_width` or `types_across_the_edge`.

The signoff is appended to the spec. CI: the GPU job failed on run `37831124809` with
`ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out` — the host is
unreachable, not an rmm pool problem, so not #178 and not a defect in the branch's edits. Chain K
does not wait on that job. Every other job was green on that run.

### 2026-10-08 — done

CI run `37839490207` on head `d6c2176d`: Changed paths, both cuDF matrix legs, the 25.02 GPU
build, the cost report and the S3 metadata check all green; Deploy pages skipped as it is a
master-push job. `GPU Tests (remote)` failed on `ssh: connect to host
llm-gpu0h200.velkerr.ru port 22: Connection timed out`, which chain K does not wait on — the host
is unreachable rather than contended, so it is not #178 and not a defect in this branch.

The task is terminal for the ensemble. The human merges; the archive list is in the signoff.

## Rebase onto master 31c56bea (2026-10-09)

The human wrote `rebase` to `.claude/ensemble/K.control`. The branch moved from master
8806a3c3 to 31c56bea, which carries three commits:

- `64ced62e` plan_text: a null decimal prints as `NULL`, a join's projection names its own
  output (#236, #237) — code under `peacockdb-core/src/plan_text/`, a new planner test
  `planner/tests/join_projection_names.rs`, and the ten tpcds `tp*.plans.txt` /
  `tp*-mini.cpu.txt` goldens it moved.
- `0753f8c9` + `31c56bea` — chain L's specs and chain K's two new tasks, `limits` and
  `empty-sorts`, at `approved to build`, plus ticket and build-test edits.

So the rebase is **not** documentation-only and the task re-proves. This branch touches none
of the files 64ced62e moved, so the only conflict was `build-test.md`'s two case-count lines.
Resolved by summing both deltas against the common base 8806a3c3:

| | base | master's delta | this branch's delta | resolved |
|---|--:|--:|--:|--:|
| grand total | 2330 | +4 | +8 −3 (C++) | 2339 |
| Rust | 1852 | +4 | +8 | 1864 |
| C++ | 97 | 0 | −3 | 94 |
| cpu `--lib` | 601 | +4 | +3 | 608 |
| cpu block | 1185 | +4 | +3 | 1192 |

Arithmetic, not a count — the re-proving dispatch confirms it against
`scripts/case-inventory.sh rust-only`.

### Re-proving dispatch, 2026-10-09

verda does not resolve from this host (`ssh: Could not resolve hostname verda`), and chain K's
note says every build and run is local anyway, so the developer runs locally. The bar is the
same nine rust-only targets as round 2 above plus the C++ cpu side, re-run on the new base.
`--lib` should gain master's four new cases (`plan_text::tests` +2, `plan_text::expr_text::tests`
+1, `planner::tests::join_projection_names` +1) and so read 606 passed / 2 ignored / 608 listed;
a different number means the build-test.md arithmetic above is wrong and the developer corrects
it from `--list`.

### 2026-10-09 — re-proved on master 31c56bea

Everything local and CPU-only, as chain K requires: no device build, no device run, no
`--features gpu`, no remote host. verda still does not resolve from this host.

**Every target green, exit 0, no compiler warning on either side.**

| target | command | result |
|---|---|---|
| `test_cpu_corpus` | one `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus --test test_corpus_goldens --test test_cost_model -- --test-threads=2`, 234s wall | **555 passed, 0 failed** |
| `test_corpus_goldens` | same command | **26 passed, 0 failed** |
| `test_cost_model` | same command | **3 passed, 0 failed** |
| `--lib` | `cargo test --features rust-only -p peacockdb-core --lib`, 49s | 606 passed, 0 failed, 2 ignored (608 listed) |
| `test_golden_format` | own run | 26 passed, 0 failed |
| `test_ci_coverage` | own run | 9 passed, 0 failed |
| `test_module_layout` | own run | 17 passed, 0 failed |
| `test_gpu_corpus` | own run | 0 cases under `rust-only` (device rung gated off) |
| `test_node_timing` | own run | 0 cases under `rust-only` (ffi rung gated off) |
| `peacock_gpu_benchmarks` | own run, `-- --skip bench_` as CI | 0 cases under `rust-only` |

`--lib` is exactly the 606/2/608 the dispatch predicted: master's `64ced62e` added
`plan_text::tests` +2, `plan_text::expr_text::tests` +1 and
`planner::tests::join_projection_names` +1 on top of round 2's 602/2/604. 234s for the three
corpus targets against round 2's 284s.

C++, 25.02 (`scripts/build.sh --configure --build --cudf_ROOT
~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12`): configure reported
`Using host cudf: 25.02.02`, 20 build steps, zero warnings, exit 0.
`./cpp/build/peacock_cpu_tests` **12 tests from 7 suites, 12 passed**, including
`ClampRowRange.AnswersEveryCaseInTheSharedTable`; `ctest --test-dir cpp/build -L cpu` 1/1
passed. The rebase broke nothing: `64ced62e` touched `plan_text/`, a new planner test and ten
tpcds goldens, none of which this task reads.

Rust warning check is its own command, because the inventory build was warm and recompiled
almost nothing: `touch peacockdb-core/src/lib.rs` then `cargo check --features rust-only -p
peacockdb-core --all-targets` — exit 0, no warning.

#### `build-test.md`: the two resolved lines are right, one unrelated row was not

`scripts/case-inventory.sh rust-only` (the tool cannot see `peacock_gpu_benchmarks`, whose
file is not `test_*.rs`; it was run and listed separately) lists exactly what the cpu block
header claims: `--lib` **608**, `test_cpu_corpus` **555**, `test_corpus_goldens` **26**,
`test_cost_model` **3** — so `1192 cases` and every per-binary figure on line 25 are correct
as resolved, and no edit was owed there.

The rebase arithmetic above also lands on the right header *total*, but the header it lands on
was already one case high for a reason that predates this branch. Summing the N columns of both
tables and checking every countable row against the tree:

| row / block | page said | tree says |
|---|--:|--:|
| cpu block (4 binaries) | 1192 | 1192 ✓ |
| Golden text format (Rust) | 26 | 26 ✓ |
| CI wiring guard (Rust) | 9 | 9 ✓ |
| Module layout rules (Rust) | 17 | 17 ✓ |
| Cost-report renderer (Rust) | 37 | **36** |
| C++ CPU/FFI unit | 12 | 12 ✓ |
| DuckDB cost extraction (Python) | 41 | 41 ✓ |
| Exec-model three rows (Python) | 216+19+93 | 328 collected ✓ |
| Calibration scripts (Python) | 12 | 9 + 3 ✓ (`test_plot.py` needs matplotlib, absent here) |

`cost-report` lists 36 cases and `main.rs` carries 36 `#[test]`. Master's `ddf3ca2c`
(2026-09-29) deleted `a_rollout_ticket_links_into_the_rollout_file` with
`tasks/active-tickets.md` and edited `build-test.md` without touching that row, so the row has
been one high ever since, on master and on every branch off it. Corrected here to 36, and the
grand-total header with it: **2338 — Rust 1863, C++ 94, Python 381**, which is again exactly
the sum of the N columns. Nothing else in the page moved; a stale wiki number is not production
behaviour, so no ticket.

Worth recording for whoever next resolves this header: master's header has also been *below*
its own row sum for a while — 1852 against rows of 1857 at base 8806a3c3, 1856 against 1861 at
31c56bea. Round 2 of this task silently repaired that gap (header 1852 → 1860 against a row
delta of +3), which is why summing header deltas across the rebase happened to give a
row-consistent 1864 rather than compounding the error. Sum the rows, not the deltas.

Two blocks stay unverified here and are the only thing the header still rests on faith for:
the ffi block (7) needs an FFI cargo build and the gpu block (576) needs a device. Chain K
runs neither.

Non-golden files checked while in the page, all accurate: `testdata/goldens` 39 / 116 / 16 +
`recipe-payloads.txt`, and `duckdb-profiles` / `duckdb-dynfilters` 22 + 99 each.

### 2026-10-09 — done again on the rebased base

CI run [37879061329](https://github.com/asymptote-tech/peacockdb/actions/runs/37879061329) on
`71c1dac5`, the rebased code head. Every job green — changes, both dataset-matrix legs
(25.02 and 26.02), cpp-build-2502, cost-report, s3-datasets; deploy-pages skipped as it is a
master-push job.

GPU Tests failed at its rsync step: `ssh: connect to host llm-gpu0h200.velkerr.ru port 22:
Connection timed out`. The host is unreachable, so no pool was ever built — this is not
[#178](../tickets/testinfra.md#t178), which is a contended pool, and not a defect in this
branch. The same failure, from the same cause, as the run before the rebase. Chain K does not
wait on that job, so the task is `done`.

The two later commits on this branch are documentation only, so `paths-ignore` and the
`changes` job both skip them and no run was started for them. The code-carrying run is the
one above.

### 2026-10-09 — second rebase, onto master bc9b6e2f

The control file said `rebase`. master carried three commits over 31c56bea:

- `2ad302bf` join-backend's spec and a report — `llm-wiki/**` only;
- `f0a6ecbf` the cost gate fails only past `REGRESSION_FAIL_PCT` (10%) — `cost-report/src/main.rs`
  and `pipeline.yml`, two new cases;
- `bc9b6e2f` chain K's nebius-gpu note, the three specs' "No GPU" sections marked superseded,
  and the board reset that reopens distinct-companions, limits and empty-sorts.

So the rebase carried code, not documentation alone, and the task re-proves rather than keeping
`done`. What it carried is narrow — the cost-report binary and the cost gate's flag — but
`pipeline.yml` is a file both sides touched and `test_ci_coverage` reads it, which is the one
way master's change can reach this branch's tests.

One conflict, in `build-test.md`, and it is the test-count row this task already repaired once.
master went 37 → 38 for two new cost-report cases, starting from the stale 37; this branch had
corrected the same row to 36. The code is authoritative: `grep -c '#\[test\]'
cost-report/src/main.rs` is 38, so master's value stands and this branch's correction was to the
figure beneath it. The header follows the rows, not the deltas — re-summed both tables rather
than adding: first table 1775, the second table's Rust rows 90, so Rust 1865, C++ 94, Python 381,
grand total 2340.

Nothing else conflicted. `guard-checks.md`, `pipeline.yml`'s GPU job and
`build-test-shadgpu.sh` applied clean.
