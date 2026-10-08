# distinct-companions — run detail

What a restarted coordinator needs. Spec: [`distinct-companions.md`](distinct-companions.md);
plan: [`distinct-companions-impl.md`](distinct-companions-impl.md).

## Standing facts

- Chain K, second task. Branch `ENS-distinct-companions`, forked off `ENS-guard-checks` at
  `6175d5fb`. Its PR targets `ENS-guard-checks`, not master — guard-checks is `done` and awaiting
  the human's merge, so the parent branch exists on the remote already.
- No GPU on this chain. No device build beyond compiling, no device run. `done` once every CI job
  but the GPU tests is green.
- Verification bar: rust-only `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`;
  the ffi rung through `scripts/cargo-cudf.sh`; C++ `cpp/build` against cuDF 25.02 plus
  `ctest -L cpu`. No device run.

## Both of the spec's conditionals resolve to "has not merged"

The spec defers two things to whichever chain lands second. Checked on this branch at
`6175d5fb`:

- **chain J's duckdb-oracle has not merged.** `corpus_query!` (`tests/test_cpu_corpus.rs:22`)
  still takes `(dataset, sf, query, cpu_modes, gpu_modes, cpu_oracle, gpu_oracle,
  schema_validation)` and nothing under `src/test_support/` mentions duckdb. So the three corpus
  lines carry no `duckdb-result.txt` section and no `duckdb_none`, and `distinct-functions`' cpu
  oracle is the new `data_fusion_disabled` with no DuckDB oracle beside it.
- **repartition-keys has not merged.** No `drop_grouping_id` anywhere in `peacockdb-core/src`. So
  `rollup-distinct`'s tp4 cpu cells stay off on #189 and `189` stays in its registry row.

`data_fusion_disabled` does not exist yet; this task adds it. #261 and #262 already have ticket
bodies (`complete-coverage.md`, `corpus-coverage.md`), filed with the spec.

## Dispatch log

### 2026-10-08 — round 1, developer

verda unreachable (`Temporary failure in name resolution`), so everything runs locally. Board
moved `approved to build` → `building` in the same commit as this file.
