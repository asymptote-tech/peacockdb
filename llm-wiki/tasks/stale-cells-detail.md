# stale-cells — run detail

Working notes. The spec is [`stale-cells.md`](stale-cells.md) (frozen); the plan is
[`stale-cells-impl.md`](stale-cells-impl.md).

## Blocked before it started, and unblocked the same day (2026-10-08)

No branch exists yet. The task was never dispatched, because a fresh analyst's reading of it said
there was nothing a developer could do that would not be guessing at results — every step needs a
card and no card answered. **That is no longer true**: the human's host override moved chain J to
nebius-gpu, the board here says `approved to build`, and `duckdb-oracle` has since run its device
cycle on that card. Dispatch this task; do not re-block it on shad-gpu.

The reasoning below is kept because its first half still holds and is the trap in this task.

- **Step 1 is the device cycle, and steps 2 and 3 are arithmetic over its 16 outcomes.** The one
  device-free edit in the plan is Task 1 Step 1 — flip the four lines' `gpu_modes` to `all_modes`
  and the four rows' five gpu cells to `enabled` — and that edit cannot be left in the tree:
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` fails on an absent
  `gpu-result.txt` wherever a cell is enabled, and committing it would assert that 16 cells are
  proven which have never run. That is the false-coverage shape a reviewer is told to hunt.
- **Hosts, as of the override.** shad-gpu is still down — it timed out on :22 and :443 with DNS
  resolving and the host key intact, so it is the host, and CI's `GPU Tests (remote)` job fails
  the same way. `verda` resolves nowhere. The card to use is **nebius-gpu**, `dmitry@89.169.109.150`,
  an L40S at cuDF 25.02; the override at the top of the chain J section of `tasks.md` carries the
  recipe, and `duckdb-oracle-detail.md` has a worked run of it. This workstation still builds and
  links the device targets but has no card, so the CPU half stays local.

## #253 does not block this task, which is worth recording because it looks as though it should

[#253](../tickets/corpus-coverage.md#t253) says the oracle cannot record a cell whose device answer
diverges from DuckDB while the cpu's matches. That is the outcome step 2 exists to produce, so the
question is live — and the answer is that the spec already prescribes a value that works. A failing
cell is `disabled` in the registry and dropped from the line's `gpu_modes`, and
`duckdb_device_cases!` emits a `duckdb_gpu_*` case only for the modes the line lists, so an off
cell has no device case to go red. The line's `duckdb_oracle` never moves. #253 bites only a task
that wants to keep a diverging cell on, and this spec says the opposite: "a cell that fails … stays
off under it". The cost here is coverage lost — a divergence recorded only as a ticket — not a red
tree.

Narrower still: `authoritative_mode` is the last enabled cpu mode, tp4_sized on all four rows, so
the cpu's own DuckDB comparison already runs at a tp4 mode and a split would have to be
mode-specific within tp4. #253's fingerprint half does reach one of these rows —
`filter_project` is `duckdb_fingerprint`, where no tolerance and no exemption exist at all.

## Two stale facts the plan carried, corrected

`stale-cells-impl.md` cited `corpus_cases.inc:43,44,46,70` for the four lines; they are at
**46, 47, 49, 73** (`filter_project`, `aggregate_groupby`, `shuffle_additive`, `shuffle_stddev`).
The registry rows 123/126/137/139 were right. The `183` comment is at **56**, not 53, and it sits
above the join batch, whose rows do keep `183` — tpch q4, q15, `anti_join`, `cross_join`,
`nested_loop_join`, `nested_loop_left_join` and `semi_join` are all still tagged with it. So the
plan's Task 3 checkbox about that comment is decidable today and the answer is **leave it**. Both
citations are corrected in the plan.

## What it needs now

One `build-test-shadgpu.sh --build` and a filtered device run for the four rows, on nebius-gpu
under the override — never `--run` or `--pull-results`, both of which ssh to shad-gpu.

The earlier note here said `duckdb-oracle`'s cycle could carry these 16 cells too. **It did not**,
and the chance has passed: that cycle recorded exactly the 26 cells enabled at the time, and the
completeness pass verified the recorded set has zero extras. So this task owns its own cycle, and
its first act is still the Task 1 Step 1 edit above — which is why the cycle and the enablement
have to land together rather than in either order.

## Dispatched, after the resequencing (2026-10-09)

Branch `ENS-stale-cells`, forked off `ENS-refcounted-scatter` at `35c0f04b`. Tasks 1–4 are `done`.

**This task moved.** It was task 2 and is now task 5, resequenced by the human in master's
`f0a6ecbf` so that it branches off refcounted-scatter rather than forking the chain off
duckdb-oracle. Nothing about its subject changed — none of its four rows has a join, so the three
tasks that overtook it cannot have touched them — but two things under it did move while it waited:
`tpch/q15` now reads `183 220` rather than `183`, and **#183 and #187 are both archived**, so the
spec's description of them as "closed" is now also true of where they live.

**One decision the coordinator owes it before it builds, and it is a refusal.**
[#253](../tickets/corpus-coverage.md#t253) says the DuckDB oracle cannot record a divergence found
on the device while the cpu agrees — one `duckdb_oracle` per line serves both, so `duckdb_exact`
fails the device case and `duckdb_divergent` fails the cpu case — and it names this task as the
first that can produce one, asking for the decision "before `stale-cells` builds rather than inside
it". The decision: **the harness does not change here.** The spec's Restriction is the 16 cells and
no fix to anything they show, and its step 2 already says what to do with a cell that cannot be
green — it stays off under a ticket. So a device-only divergence from DuckDB is a cell that stays
off under #253, recorded with its query and mode, and #253 gets the concrete instance it so far
lacks. That is cheaper and more honest than widening the harness inside a task whose whole point is
to measure, and it leaves #253 a decision for a task that can design it.
