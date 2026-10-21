# The device answers a value-form CASE

Kind: production

**This task closes [#57](../tickets/corpus-coverage.md#t57)** (value-form CASE). Ninth of chain K,
after welford-device.

## Why

`build_column_case` (`cpp/src/expr.cpp`) throws `value-form CASE not supported in column path` for
any `CASE x WHEN v THEN … END`. The throw rests on a false measurement: the gtest that saw a wrong
answer built every Int64 literal as 0 (`reports/corpus-fixes.md`, fix 7). tpcds q39 uses the form
twice per plan, and #57 is the only ticket left on its device cells.

## The work

1. **Lower the value form onto the search form.** Build the comparand column once. Each WHEN
   becomes `comparand = when` (`cudf::binary_operation`, `EQUAL`, `BOOL8`); a literal WHEN uses the
   scalar overload. The result feeds the existing `copy_if_else` fold unchanged.
2. **SQL's rules hold.** A NULL comparand or a NULL WHEN never matches, since `copy_if_else` reads
   a null condition as false. The first matching WHEN wins. No match and no ELSE answers NULL.
3. **Remove what guarded the refusal**: the throw and its comment, and the #57 clause in
   `PlanError::Unsupported`'s doc (`plan/mod.rs`).

## Corpus

`tpcds/q39`: its five device cells turn on. `57` leaves its registry row. Its `gpu-result.txt`
sections are recorded at every mode.

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp` | `build_column_case` |
| `cpp/tests/gpu/test_plan_executor.cpp` | the value-form gtest |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the `bug_` pin becomes an agreement case |
| `peacockdb-core/src/plan/mod.rs` | the doc clause |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/tpcds.sf1/gpu-result.txt` | q39 on |
| `llm-wiki/tickets/`, `archive/archived-tickets.md`, `tickets.md`, `build-test.md` | #57 archived; counts |

Component-level API: none. No wire, ABI or facade change.

## Restriction

The value form only. Nothing else in `build_column_case` changes. No cpu golden moves.

## Verification

- A gtest in `test_plan_executor.cpp`. Its comparand holds a NULL, one WHEN maps to NULL, and
  there are two WHENs where the first match must win. With and without ELSE.
- `bug_a_value_case_is_refused_on_the_device` becomes `a_value_case_agrees`, a cpu-vs-device
  agreement case with a string THEN, and a second with a NULL comparand row.
- q39 green at all five device modes on nebius-gpu, DuckDB's device comparison included. The final
  device run is the dispatched `GPU Tests (nebius-gpu)` CI job.

## Signoff (2026-10-21)

Solved under its constraints. The value form lowers onto the search form's `copy_if_else` fold;
NULL never matches and the first match wins, pinned by two gtests and two cpu-vs-device agreement
cases that were red with the fix reversed. q39 is on at all five device modes; #57 is archived.
Completeness: reviewer 0 blocking/0 important, analyst no omissions, no `architecture.md` sentence
falsified. Shortcuts or bandaids: none. Notes: q39's corpus cells prove no regression rather than
the new semantics (both its WHENs are literals); most of the `report.html` diff is catch-up from
earlier chain-K registry edits; this spec's `#t57` link now points at an archived ticket.
