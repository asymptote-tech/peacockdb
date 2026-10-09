# exit-copies — run detail

## Dispatched (2026-10-09)

Branch `ENS-exit-copies`, forked off `ENS-stale-cells` at `acb1d82f`. Tasks 1–5 are `done`.

**What changed under this task's plan while it waited, and it is the one thing to read first.**
`exit-copies-impl.md` was written against a `TableResult` whose three vectors are public, and its
project and window steps assemble a handle by hand — `TableResult out;` then `out.owners.push_back`,
`out.columns.push_back`, `out.column_names.push_back`. refcounted-scatter's completeness pass found
that this would have quietly reopened [#164](../tickets/corpus-coverage.md#t164)'s name-count half,
which that task had just marked closed on `owning`'s check alone. So the check moved: **
`register_handle` is now the only path to a handle number and refuses a handle of no columns, or
whose names or owners do not number its columns.** Hand assembly through the public fields is still
the intended route and is why they are public — but the handle has to be complete before it is
registered. The plan carries a note saying so.

**What the spec expects, so a different outcome is a finding rather than a surprise.** Its §4
predicts 6 of the 11 sites lose their copy and 5 keep it — the four Welford struct members and one
merged-state child, whose shared form would cost more than it saves. It also allows any site to
keep its copy with a measured note. So the deliverable is eleven classed sites with reasons, not a
count.

**Deferred by the human host override**, which outranks the spec here: the Verification bar's sf40
q6 and q19 benchmark before and after cannot run — the sf40 dataset lives only on shad-gpu, which
has been down all run, and benchmark measurement is out of every task by the override. `expr.cpp`'s
`ColumnRef` arm is the site that bar exists to measure, worth about 46 of the 107 GB q19's lineitem
filter moves at sf40, so the saving will land unquantified at that scale. The RMM statistics
adaptor cases the Tests section asks for are gtest-scale and are **not** deferred; they are what
carries the claim instead.
