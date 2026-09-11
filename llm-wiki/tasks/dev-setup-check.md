# 1 — every workflow once, from dev

Kind: production

A throwaway task. Its code is two lines that mean nothing; its product is the record, in
[`dev-setup-check-detail.md`](dev-setup-check-detail.md), of what each documented workflow did
when driven from the host `dev` by the ensemble with nobody attached — coordinator, developer,
reviewer, analyst, the PR, CI. The branch is closed unmerged once the chain reports `done`, and
nothing from it is archived.

## Why this shape

`dev` is a fresh host, provisioned by hand to the shape `build-test.md` describes. The failures
that matter are the ones a real task would hit halfway through a dispatch and misreport: a script
assuming a path this host does not have, a tier needing a package nobody installed, a push needing
a key nobody authorised, a watchdog that cannot see the coordinator's exit. So the task drives every
workflow the "Local build workflows" table and the "Remote hosts" table name, exactly as written,
and writes down what happened. **Nothing is fixed on this branch.** A workflow that fails is
recorded with its first failure signature and the developer goes on to the next one; the fix is a
separate task on master. The two-line diff exists only so that CI sees code: `pipeline.yml` skips a
documentation-only PR, and a PR that never runs CI proves nothing about watching it.

## The diff

- `peacockdb-core/tests/test_golden_format.rs`: one more `#[test]` over `parse_node_line`, a
  string in and an assertion on what comes out, pinning a property the existing cases do not.
  Written first and seen red, then green — the developer's ordinary loop, so that it runs here.
- `cpp/tests/gpu/test_cudf.cpp`: one comment line above the first test, naming this task. It is a
  changed line in a file the GPU job compiles, and that is all it is.
- `llm-wiki/build-test.md`: the "Golden text format" row's count and the totals above the table
  move by one, because the row would otherwise be wrong.

## What the developer runs

In this order, every command under `timeout`, and each entry in the detail file carrying the
command, the host it ran on, wall time, outcome, and the first failure signature if it failed.
A failure ends that workflow, not the task.

1. **rust-only loop**, on dev, in `target/`: `cargo test --features rust-only -p peacockdb-core
   --test test_golden_format`, then `--test test_plan_goldens`.
2. **cost-report**: `cargo test -p cost-report`, and the two python tiers it rides with:
   `python3 -m pytest testdata/test_duckdb_cost.py` and `python3 -m pytest
   scripts/exec_model/tests/ --ignore=scripts/exec_model/tests/test_tpch_corpus.py`.
3. **C++ + staged Rust, cudf 26.02**, with dev as its own remote: `scripts/build-test.sh --host
   dev --local-cudf-root ~/miniforge3/envs/rapids-26.02 --build`, then `--push-binaries`, then
   `--run`. The remote defaults — `~/peacockdb`, `~/miniforge3/envs/rapids-26.02`, the
   `/media/data/peacockdb` symlink — are already what dev has.
4. **shad-gpu cycle, cudf 25.02**: `PCK_TEST_FILTER=q6 scripts/build-test-shadgpu.sh --build`,
   then `--push-binaries`, then `--patch --run` — three foreground calls, never one backgrounded
   chain. The filter scopes the rust binaries to one query; the C++ gate runs whole, because the
   script as written is what is under test.

The coordinator's half is not listed because it is not optional: branch, PR against master,
reviewer, completeness pass, CI green, `done`. That the coordinator did each of those from a
`claude -p` under the watchdog, without a terminal, is the other half of what this task proves.

## Constraints

- No file outside the three named above changes. No golden is regenerated. No dependency is added.
- A failing workflow is recorded, not repaired. Repairing it here would hide whether the host is
  ready for a task that does not know about the repair.
- The PR is never merged. The human closes it and deletes the branch.

## Scope

| file | change |
|---|---|
| `peacockdb-core/tests/test_golden_format.rs` | one `#[test]` |
| `cpp/tests/gpu/test_cudf.cpp` | one comment line |
| `llm-wiki/build-test.md` | two counts |

Component-level API: none.

## Done when

The four workflows above have each been run from dev and recorded in the detail file with their
outcome; the new case is green in the rust-only tier; the PR is open against master with CI green;
the reviewer and the completeness pass have run; and the board says `done`.

## Completeness signoff

Solved under its constraints: three files plus the record, no golden, no dependency, nothing
repaired. All four workflows were driven from dev by the ensemble unattended and recorded; the
coordinator's half — branch, PR #146 against master, reviewer, both completeness readings, CI
watched to green on the code commit — ran from `claude -p` under the watchdog. Two workflows did
not go green and are left as findings for master: the exec-model pytest timed out at 900 s
because the spec's `--ignore` is one file short, and every shad-gpu binary built on dev wants
`GLIBC_2.38` where the patch target is 2.35. Shortcuts or bandaids: none. Not driven, by the
spec's own list: `docker-build.sh`, `cost-report-preview.sh`, the shad-gpu detached path, verda.
