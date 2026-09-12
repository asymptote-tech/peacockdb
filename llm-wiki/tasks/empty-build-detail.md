# empty-build — run record

Spec: [`empty-build.md`](empty-build.md). Plan: [`empty-build-impl.md`](empty-build-impl.md).
Branch `ENS-empty-build` off `ENS-typed-nulls`; PR against it when reviewing.

### 2026-09-12 — building: plan tasks 1 and 2 dispatched

The index's derived field and the conditional drop first, with their tests at the rust rung.
#175's handoff from task 9 stands over the spec: the three `without_build` pins in
`gpu_tests/join_cases.rs` (`bug_right_with_no_build_batch_is_refused_on_both` and its Full and
RightAnti siblings) are reached by no driver, so whichever of §2's two shapes wins, they are
retargeted or deleted by hand in the change that fixes the cause, and the known-wrong table moves
with them.
