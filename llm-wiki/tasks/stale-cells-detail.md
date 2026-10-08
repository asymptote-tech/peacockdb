# stale-cells — run detail

Working notes. The spec is [`stale-cells.md`](stale-cells.md) (frozen); the plan is
[`stale-cells-impl.md`](stale-cells-impl.md).

## Blocked before it started (2026-10-08)

No branch exists. The task was never dispatched, because a fresh analyst's reading of it says
there is nothing a developer could do that would not be guessing at results.

- **Step 1 is the device cycle, and steps 2 and 3 are arithmetic over its 16 outcomes.** The one
  device-free edit in the plan is Task 1 Step 1 — flip the four lines' `gpu_modes` to `all_modes`
  and the four rows' five gpu cells to `enabled` — and that edit cannot be left in the tree:
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` fails on an absent
  `gpu-result.txt` wherever a cell is enabled, and committing it would assert that 16 cells are
  proven which have never run. That is the false-coverage shape a reviewer is told to hunt.
- **Hosts, probed 2026-10-08:** shad-gpu times out on :22 and :443 while DNS resolves and the host
  key is intact, so it is the host; `verda` resolves nowhere and no `VERDA_*` credentials exist
  here. This workstation builds and links the device targets against cuDF 25.02 at
  `~/data/miniforge3/envs/rapids-cuda-12.2` but has no card.

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

## What unblocks it

One `build-test-shadgpu.sh` cycle under a corpus filter for the four rows, on a shad-gpu that
answers. The same cycle that `duckdb-oracle` waits on can carry this task's 16 cells, so whoever
gets a card should run both rather than queueing them.
