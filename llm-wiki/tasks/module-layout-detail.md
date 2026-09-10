# module-layout — run detail

Branch `ENS-module-layout`, forked off `ENS-drop-mode-name` at 787c1e5c. PR targets
`ENS-drop-mode-name`, not master.

## Facts a restarted coordinator needs

- There is no `module-layout-impl.md`. This chain does not use one; the spec is the plan, and
  its "Where everything goes" table plus the per-commit order in "Validation" are what the
  developer works from.
- Task 1 (`drop-mode-name`) is `done`, PR #141 open against master, checks green. Its head is
  this branch's base.
- The spec's validation bar is the whole task: no golden may move after the quarantined
  `GpuHashJoin` commit, and the `--list` case inventory must come back byte-identical.

## Run log

### Round 1 — developer dispatched
Dispatched the developer with the spec as its working document. Nothing reported yet.
