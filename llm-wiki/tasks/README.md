# Task specs

Four files per task, with four different write disciplines. Confusing them is what made the
old single-spec model accumulate.

- `<task>.md` — the spec: what the task is, why this shape, what the constraints are. Frozen
  once the human finalizes it, with exactly one later write: the completeness signoff appended
  at the end, at most ten lines, saying whether the task was solved under its constraints and
  naming every shortcut or bandaid applied, or "none". A `Kind:` line under the title declares
  the task `production` or `prototype`; a prototype gets a branch but no PR and no reviewer,
  because its code is throw-away.
- `<task>-impl.md` — the implementation plan, written in the second planning phase and worked
  by the developer. Keeping it out of the spec is what lets the spec freeze.
- `<task>-detail.md` — everything a run accumulates: developer handoff notes, review findings
  and how each was resolved, research answers, prototype findings. A restarted coordinator
  recovers from this file.
- `tasks.md` — the board. Written only when a state changes.

At archive time only the spec survives, moved into `../archive/archived-tasks.md` with its
signoff. The board entry goes, and `-impl.md` and `-detail.md` are deleted outright: they were
scaffolding for a task that is now in the history.

Keep specs short: goal, scope — the code and the component-level API expected to change —
constraints, verification bar. Long-lived design facts belong in
`architecture.md` and `build-test.md`.
