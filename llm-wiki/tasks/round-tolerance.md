# q78 and q2 on the device, compared within a float tolerance

Kind: production

**This task closes [#60](../tickets/corpus-coverage.md#t60)** (round() off by one ulp), as won't
fix, **and [#56](../tickets/corpus-coverage.md#t56)** (q2 CASE in partial sum), once q2 is green.
Eleventh of chain K, after decimal-literal.

## Why

The device's `round(x, p > 0)` is `cudf::round(·, p, HALF_UP)`, which rounds in two steps. So
about one value in twenty at or above 1.0 is one ulp from DataFusion's answer. In q78, 5 `ratio`
cells of 100 rows come back as `2.7800000000000002` against `2.78`: a gap of 4.4e-16, about
1.6e-16 relative. A bit-identical fix needs either three cuDF passes and two temporary columns,
or a custom kernel. Neither is worth one ulp. So the engine keeps `cudf::round`, and the device's
comparison allows the gap.

#56 asks for proof that q2's `sum(CASE WHEN <string equality> …)` is right across a plan's merges.
q2's per-node sections already match the cpu's, and its answer differs only through `round`. A
green q2 is that proof.

## The work

1. **Loosen the device comparison for q78 and q2.**
   - `gpu_oracle` becomes `golden_approx_std`: 1e-11 relative on every numeric cell, the device
     comparison that already exists.
   - `duckdb_oracle` becomes `duckdb_approx`: 1e-11 relative on a float, one last-place unit on a
     decimal. The cpu's `data_fusion_exact` stays, since the cpu is DataFusion's own kernel.
2. **Say it in the code.** The comment at `round` in `cpp/src/expr.cpp` names the two-step
   rounding, the one-ulp gap against DataFusion, and that the corpus compares it within 1e-11.
3. **No engine change.**

## Corpus

`tpcds/q78` and `tpcds/q2`: their five device cells each turn on. `60` leaves both rows and `56`
leaves q2's. Their `gpu-result.txt` sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | the comment at `round` |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q78 and q2 on, under the looser oracles |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #60 archived as won't fix, #56 as done; counts |

Component-level API: none.

## Restriction

No engine change. Only q78's and q2's lines change oracle. If q2 is red for a reason other than
`round`, #56 stays open with the measurement, and q2's cells stay off under it. A device cell further from
the golden than 1e-11 relative (a half-way value rounding to another last digit) is not this
task's either: that query's cells stay off and #60 stays open with the measurement.

## Verification

- q78 and q2 green at all five device modes on nebius-gpu, DuckDB's device comparison included;
  the dispatched `GPU Tests (nebius-gpu)` CI job green.
- The detail file records each device cell that differs from the golden, with the largest
  relative gap, so the archived #60 says how big the gap was.

## Signoff (2026-10-21)

Partly solved. q78 is on at all five device modes under `golden_approx_std` and `duckdb_approx`:
5 of its 1000 cells sit one ulp from the golden, the largest 1.76e-16 relative. #60 is archived as
won't fix with that measurement, and the comment at `round` names the gap. No engine change.
Not met: q2's half of Verification and closing #56. q2's device answer is right, but `round`'s
ulp widens its rendering past the 256 KiB result cap, the recording becomes a fingerprint, and
`duckdb_approx` refuses it. That is a scope gap, not the Restriction's case: the fix is in
`test_support/duckdb_oracle.rs`, outside Scope, and #56 now carries it. The board's "closes #56"
is the human's wording and is stale. Shortcuts: the cpu-side DuckDB check on q78's decimals now
allows one last-place unit, as the spec chose. Completeness: reviewer 0/0, analyst 1 important
(this scope gap); no `architecture.md` sentence falsified.
