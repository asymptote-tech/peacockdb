# repartition-keys — run detail

Working notes. The spec is [`repartition-keys.md`](repartition-keys.md) (frozen); the plan the
developer works in is [`repartition-keys-impl.md`](repartition-keys-impl.md).

## Branch and PR

- Branch `ENS-repartition-keys`, forked off `ENS-pbench` at `badda3d5`.
- Task 4 of chain J, so its PR targets **`ENS-pbench`**.
- Task 2 (`stale-cells`) is blocked and has no branch, so the chain's branch order is
  `master → ENS-duckdb-oracle → ENS-pbench → ENS-repartition-keys`. A resequence for the human at
  merge, not a conflict: disjoint lines and rows.

## Hosts, probed 2026-10-08

- **shad-gpu: down** all day — :22 and :443 both time out while DNS resolves and the host key is
  intact. **verda: unlocatable** — resolves nowhere, no `VERDA_*` credentials. This box builds and
  links the device targets against cuDF 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2` and has
  no card. Under 7 GiB free on `/`.

## Why this task is dispatched partially, and what the partial is

An analyst read the task against the device absence. Two of the spec's three verification bars are
rust-only and reachable today, so the earlier blanket block over tasks 4-9 was wrong for this one.
**Four plan tasks are device-free, in this order:**

1. **Task 2 — #189's grouping-id drop.** Planner only: `drop_grouping_id`, a planner test, three
   unit tests, a plan-golden regeneration.
2. **Task 5b — the four wire timestamp types**, minus its gtest: the fbs append,
   `convert_data_type`, `fb_text`, the `fb_to_type_id` arms compiled but not run, and
   `timestamp-s-key-group` leaving `NOT_RUNNABLE`. Both later C++ tasks consume these values.
3. **Task 5c — #249's plan-time refusal, with step 4 deleted.** See the deviation below.
4. **Task 9 step 1 — the 18 cpu cells**: tpch `rollup-over-join` and tpcds q5, q18, q22, q80
   (15), plus pbench's `rollup-small-keys` (3).

**The order matters and is not the plan's.** Task 1 — the murmur gate, #201 — is **not** rust-only,
which is the opposite of what it looks like: `cpu_backend/mod.rs:708` declares
`#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`, so every case in `murmur_conformance.rs` runs
in the device rung only, and `build-test.md` says so. #201's proof *is* the seed-mutation red-green
cycle on that gate, so rewriting the one gate that proves cpu and device agree, and landing it
unrun, would be #201 over again. Task 1 waits for a card.

## Three fences. Nothing in the partial may assert a cell that was not proven

- **No Task 10.** Archiving #95, #189, #201, #206 or #240 is the one write here that would lie —
  #189 included, since 6 cells still want comet's unsigned arm.
- **No Task 9 step 2, and no gpu cell enabled at all.**
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` makes this self-policing: an
  enabled device cell with no recorded answer goes red.
- **No cpu-side lane-rule change** — not Task 6's decimal cast, not Task 4's NaN canonicalisation,
  not Task 6b's unsigned widening. Each is one half of a two-engine rule whose only proof is a
  device gate; landing the cpu half alone moves cpu goldens and leaves "the engines agree"
  asserted by nothing. `registry.rs` is too weak to catch it — it wants only *some* ticket on a row
  with off cells — so the guard is the dispatch, not a test.

## The deviation the partial requires

**Task 5c step 4 is deleted, not deferred.** It adds `struct-key-join`, `struct-through-join` and
`interval-through-join` to `NOT_RUNNABLE`, and those three query files do not exist — they are held
behind [#255](../tickets/complete-coverage.md#t255), the planner panicking on a Struct or Interval
column instead of refusing. `every_query_that_cannot_cross_the_wire_is_declared_and_every_declaration_is_true`
(`plan_goldens.rs:495`) asserts each declaration carries its line in all five modes, so declaring
an absent query goes red. Deleting it costs nothing measurable: no existing query puts an
unnameable type in a serialized schema, and #249's own text already assigns those declarations to
the #255 task. **The spec's Scope line about the three #249 queries is therefore false**, and the
spec is frozen with one write left, so it is recorded here and in the signoff rather than edited.

## Two things already settled, so the round need not re-derive them

- **[#253](../tickets/corpus-coverage.md#t253) is decided** — a side on `duckdb_divergent`, an
  optional ticket and column list on `duckdb_fingerprint`, with the two rejected alternatives
  named. It gates only the device half, so nothing in this partial turns on it. `uint-key-group`'s
  cpu side is settled too: its line is already `duckdb_fingerprint` with tp1 enabled and green, so
  the tp4 cpu answer is the same rows.
- **#189 reaches 18 of its 24 cells**, and the plan is already right about it — Task 9 step 1
  enables exactly those 18 and the unsigned half is Task 6b, inside this task. The ticket's fix
  paragraph was corrected to say so.

## One healthy red to expect

tpcds q14 has `rollup` in its features and its cpu tp4 cells are enabled today. If #189's drop
changes its lanes, its committed sections fail. That is a real finding, not something to clear with
`UPDATE_CANONICAL` — the plan expects only six queries to move.

## Carried forward

- **27 rust-only cases are red on the base and belong to task 1**: the 26 `duckdb_gpu_*` tpch and
  tpcds cases plus the coverage guard, on a `gpu-result.txt` no device wrote. The count must stay
  27.
- **Run the rust-only loop as plain `cargo test --features rust-only` into `./target`**, never
  through `scripts/cargo-cudf.sh`, which redirects to a cuDF target dir and would recompile the
  DataFusion stack with under 7 GiB free.
- **[#259](../tickets/corpus-coverage.md#t259)** records that pbench's device cells have never been
  run and that `int8-key-group` never landed; this task's own key-type rows are part of what that
  cycle will cover.
