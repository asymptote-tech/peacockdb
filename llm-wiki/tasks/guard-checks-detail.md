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
