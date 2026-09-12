# typed-nulls — run record

Spec: [`typed-nulls.md`](typed-nulls.md). Plan: [`typed-nulls-impl.md`](typed-nulls-impl.md).
Branch `ENS-typed-nulls` off `ENS-declared-schemas` at `b86805b3`; PR against it when reviewing.

### 2026-09-12 — building: plan task 1 dispatched

The gtest literal helpers first, since every later test builds a literal through them. The
developer also settles whether `PCK_TEST_FILTER` reaches the gtest binaries and records it here.
#198's amendment stands over the spec: the second pin of task 9 shows a bare typed null in a
select list is a column of zeros, so the spec's "a bare literal short-circuits to null" is false
and plan task 8's test asserts what the device does after the fix, not what the spec assumed.
