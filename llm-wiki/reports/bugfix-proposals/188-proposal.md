# #188 — the device refuses a read with row groups and a limit together

Read at master c18e063a. Paths relative to `/media/data/peacockdb`. Nothing built or run; the
cuDF rule below was read in the cuDF source at `/home/dmitry/cudf/cpp/src/io/functions.cpp`
and the 25.02 header, and the parquet-crate rule in `~/.cargo/registry/src/*/parquet-54.2.1`.

## 1. Issue

`GpuLoadParquet` carries a limit DataFusion pushed into the scan (`plan/mod.rs:893`), the
recipe writer puts it on the wire as `CudfScan.limit` (`wire/node_writer.rs:93`), and the C++
scan sets it as `parquet_reader_options::set_num_rows` (`cpp/src/operators/scan.cpp:62`) and
then sets the row groups the call named (`scan.cpp:78`). cuDF refuses that pair, so **every**
`execute_scan_rowgroups` on a limited scan fails at run time — the first call of the run,
before anything else on the device is reached. Its twin, #186: `CpuSource::new`
(`executor/cpu_backend/source.rs:37-78`) never reads `node.limit`, so the same plan on the
CPU answers the whole table wherever no interval sits above the scan.

The two are one plan shape seen from each backend, and the ticket says so: a fix for either
has to decide what a limit in the scan means. This proposal decides it — the loader honours
it, on both backends — and closes both.

What it disables, corrected against the code (00-tickets.md's row is right about #188's
cells but not about what re-enabling them needs):

- **cpu**: `tpch/scan_limit` × tp1-single, tp1-rowgroup (#186; `corpus_cases.inc:44-51`,
  registry line 135 `186 188`). These two come back with this fix.
- **gpu**: `tpch/scan_limit` × tp4-single, tp4-rowgroup, tp4-sized and `tpch/nested_limits`
  × 5 (`corpus_cases.inc:49-50, 78-79`; registry lines 131, 135). The #188 refusal goes at
  all eight, but none of the eight passes afterwards: scan-limit is `SELECT * FROM lineitem`
  — four `Utf8View` columns (#183) and four `Decimal128(15,2)` (#187) at the unload; and
  nested-limits' `GpuCrossJoin` answers one probe call with five batches on the CPU
  (`tp1-single-mini.cpu.txt` nested-limits: `batch_rows=[[23,23,23,23,23]] abandoned=[92]`,
  DataFusion's `CrossJoinExec` emitting one batch per build row) and one table on the device,
  so the byte-exact section compare (`tests/common/corpus_golden.rs:111`) fails on the join
  and unload lines. That last wall has no ticket; §6 and §7 name it.

Only two corpus queries carry a limited scan (three loader lines per tpch plan golden, none
in tpcds), so the blast radius of any change here is those two.

## 2. Root cause

Device side, in order:

1. `translator/nodes.rs:392-411` `source()` builds the loader with `config.limit` and one lane
   (`lanes_for`, `:374-391`) but whatever batching the mode has: the tp1-rowgroup golden for
   scan-limit shows 49 batches, nested-limits' `part` two. architecture.md:374 says "one lane
   and one batch"; the code plans one lane.
2. `wire/attach.rs:100-114` publishes one call per batch, `execute_scan_rowgroups(#seq, row
   groups)`; `node_writer.rs:93` writes `limit: node.limit.unwrap_or(0)`.
3. `gpu_backend/source.rs:63-91` `read_next` calls the symbol once per mapping entry with that
   entry's row groups. `node_session.cpp:472-482` refuses an empty list, so a list is always
   named.
4. `scan.cpp:61-63` — `if (scan->limit() > 0) opts.set_num_rows(limit)`, legal at that point
   because `_row_groups` is still empty; then `:77-78` `opts.set_row_groups({rgs})` with a
   non-empty list. cuDF `functions.cpp:805-812`:

       if ((!row_groups.empty()) and ((_skip_rows != 0) or _num_rows.has_value()))
         CUDF_FAIL("row_groups can't be set along with skip_rows and num_rows");

   The 25.02 header declares the same three setters (`parquet.hpp:219, 299, 306`), and the
   message in the ticket is this one. The exclusivity is cuDF's contract, not a version quirk:
   `num_rows`/`skip_rows` address the file's rows from its start, `row_groups` a subset of
   groups, and the reader has no meaning for both at once.
5. `node_session.cpp:490-501` wraps it as `execute_scan_rowgroups: seq N reading row groups
   [...]: CUDF failure ...`; `gpu_backend/source.rs:79-84` turns it into a `BackendError`; the
   driver fails the run naming `GpuLoadParquet lane 0`.

CPU side: `CpuSource` (`cpu_backend/source.rs`) stores file, metadata, projection and the
lane's mapping and never looks at `node.limit`; `read_next` reads whole row groups
(`:85-91`). At the tp4 modes DataFusion leaves a `GlobalLimitExec` above a four-partition scan,
`translate()` (`translator/mod.rs:84-92`) puts its interval on the unload, and the driver's
range/early-exit trims — so those cells pass. At tp1 the scan is one partition, DataFusion
absorbs the fetch into it and erases the limit node (skip = 0), `limit_interval` finds nothing,
and nothing above the loader ever counts.

Why the intended design does not hold today: architecture.md's rule rests on "one batch" so
that `set_num_rows` per call is the whole answer. The planner never made it one batch, and
cuDF never allowed `set_num_rows` beside the row groups every call names. Both halves of the
sentence at `architecture.md:374-378` are false of the code.

## 3. Localized fix

**Decision: a scan's limit is honoured by the loader, per lane, across its batches** —
what `architecture.md:181` already promises. Each call is capped at `limit` on the device
(exact stats), the executor on both backends counts what its lane emitted, trims the batch
that crosses the bound, and reads no further mapping entry. No plan shape moves, no plan-tree
line moves, no ABI symbol or fbs field is added or changed.

### C++ — `cpp/src/operators/scan.cpp`

- Delete `:61-63` (`set_num_rows`).
- After `read_parquet` and after the `PEACOCK_LOG_SCAN_ROWS` block (`:81-91`, so the log still
  reports rows decoded), before the decimal widening (`:97-105`, fewer rows to cast):

      // cuDF refuses `num_rows` beside `row_groups`, and every read here names its groups,
      // so the limit is applied to the table read rather than to the read. A cap per call:
      // the count across a lane's calls is the executor's (`gpu_backend/source.rs`).
      const uint64_t limit = scan->limit();
      if (limit > 0 && static_cast<uint64_t>(result.tbl->num_rows()) > limit) {
        result.tbl = std::make_unique<cudf::table>(
            cudf::slice(result.tbl->view(), {0, static_cast<cudf::size_type>(limit)}).front());
      }

  `#include <cudf/copying.hpp>`. `cudf::table(table_view)` is an owning copy, so the decoded
  row group is released when `result.tbl` is replaced. `NodeStats` are computed by the two
  callers from the returned view (`node_session.cpp:238-242, 502-505`), so rows and
  `varlen_content_bytes` are exact for the capped table — which is what keeps the device's
  `batch_bytes` equal to the CPU's for the first batch, the only one any corpus query trims.
- The `batches`-map arm of `execute_node` (`node_session.cpp:225-247`) goes through the same
  function and gets the same cap per entry. Nothing writes `batches`.
- `0` keeps meaning "no cap" (`gpu_plan.fbs:336` says so). hacks-audit finding 2 (`Some(0)`
  and `None` are one wire value): the count is decided by the executor's `Option`, below, so a
  `LIMIT 0` pushed into a scan — which DataFusion plans as an empty relation, never as a scan
  — would still answer no rows. Say so in one line at `node_writer.rs:93`; the field's doc
  gains "applied to the table read, per call; the count across calls is the executor's".

### Rust — `executor/cpu_backend/source.rs`

- `CpuSource` gains `remaining: Option<u64>` ("rows this lane may still emit under the node's
  limit; `None` where the scan carries none"), set in `new` from `node.limit`.
- `read_next` (`:80-114`): after `.with_projection(...)` (`:89`), `if let Some(remaining) =
  self.remaining { builder = builder.with_limit(remaining as usize) }`. parquet 54's
  `with_limit` composes with `with_row_groups` and is applied over the selected groups'
  rows (`arrow_reader/mod.rs:214-226, 625, 840-870`), so the reader itself returns the first
  `remaining` rows of this batch's groups and decodes no page past them. After `as_declared`:

      if let Some(remaining) = &mut self.remaining {
          *remaining -= batch.num_rows() as u64;
          // The lane owes nothing more: the mapping's later entries are never read.
          if *remaining == 0 { self.batches.clear(); }
      }

  No slice on the CPU: the reader's limit is the trim.

### Rust — `executor/gpu_backend/source.rs`, `gpu_backend/mod.rs`, `gpu_backend/accumulate.rs`

- `GpuSource` (`mod.rs:43-48`) gains `remaining: Option<u64>`, set in `new` from `node.limit`.
- `new` (`:32-41`) accepts the recipe below: `[scan]` when `node.limit.is_none()`, `[scan,
  slice]` when `Some`, where `slice.symbol == SliceHandle`, `when == PerStraddlingBatch`,
  `inputs == [PriorOutput, RowRange]`; any other shape refused as today.
- `read_next` (`:63-91`): after `produced(...)`:

      let mut rows = stats.rows;
      if let Some(remaining) = &mut self.remaining {
          if rows > *remaining {
              // Only a batch after the first can cross the bound: the scan call itself
              // capped this one at the node's limit, and `remaining` is smaller only once
              // an earlier batch was emitted.
              batch = sliced(self.executor, batch, RowRange { offset: 0, length: *remaining }, &self.schema)?;
              rows = *remaining;
          }
          *remaining -= rows;
          if *remaining == 0 { self.batches.clear(); }
      }

- `sliced(executor, batch, range, schema) -> Result<GpuBatch, BackendError>` is
  `LimitStream::accumulate_and_fetch`'s slice (`accumulate.rs:421-461`) moved into a
  `pub(super)` function in `gpu_backend/mod.rs` and called from both sites, so the
  `slice_handle` call, its error text and the pricing comment exist once. The pricing is
  hacks-audit finding 1's approximation (`logical_size_from_schema(schema, rows, 0)`: no
  string bytes); the move states it once instead of twice and does not fix it — the fix is
  `slice_handle` reporting `NodeStats`, an ABI change this task does not make. For the loader
  it is reachable only when `limit` exceeds the first batch's rows, which no corpus query does.

### Rust — `wire/attach.rs:100-114`

The loader's recipe names the call it may make, as `GpuLimit`'s does (`:341-352`):

    let mut calls = vec![Call::seq(seq, FbKind::Scan, vec![Input::RowGroups], CallPattern::PerBatch)];
    if load.limit.is_some() {
        // The batch that crosses the bound is cut to what remains of the limit; the calls
        // before it are whole and the mapping's later entries are never read.
        calls.push(Call::bare(AbiSymbol::SliceHandle, vec![Input::PriorOutput, Input::RowRange], CallPattern::PerStraddlingBatch));
    }

Renders (`wire/recipes.rs:171-193`) as `GpuLoadParquet: calling_lanes=1, per batch:
execute_scan_rowgroups(#0 CudfScan, row groups); per straddling batch: slice_handle(prior
output, row range)`. Three loader lines per tpch plan golden move, nothing else in them.

### Rust — `plan/source.rs`, `plan/mod.rs`

- `validate_schemas_and_partitions` (`plan/source.rs:21-48`): `if self.limit.is_some() &&
  self.partition_groups.len() != 1` → `PlanError::Invalid("{table}: a pushed-down limit is
  honoured per lane, so a limited scan reads one lane — the planner gives it one
  (`lanes_for`)")`. The planner decides (`nodes.rs:379-381`); the node checks, which is the
  `validate.rs` pattern. Check first that `tests/common/rebuild.rs:394` `source(Some(7))` —
  two lanes with a limit — is only rebuilt and debug-compared, never validated (grep says so;
  `test_layout_injection.rs:241` validates `merge_over_sorted()`, which uses `source(None)`).
- Doc at `plan/mod.rs:893`: "... Honoured per lane: the loader emits at most this many rows
  and reads no batch past the one that met it."

### What it deliberately does not touch

- The planner: `lanes_for`, `source`, `limit_interval`, `translate`. No node is added or
  removed; every plan-tree line, `partition_groups`, `--- memory ---` section stays
  byte-identical.
- The driver: `settle_limit`/`rows_seen` count arriving rows for interval owners; a source has
  no arrivals, and making it an interval owner would trip `limit_positions`
  (`validate.rs:39-53`) for the tp1 shape and add a third counter of one stream
  (hacks-audit finding 8). The loader drains its own work list instead.
- The fbs, the C ABI, the wire bytes of any unlimited scan (the writer's statement order is
  unchanged; `recipe-payloads.txt`'s existing digests do not move).
- `execute_scan`'s frozen no-row-groups path: it now caps after the read too (one mechanism);
  no plan reaches it.

### CPU and GPU stay one engine

Same rows per batch and same batch count per lane, by the same arithmetic: batch k emits
`min(n_k, remaining_k)` rows, `remaining` falls by that, the lane stops at zero. The CPU meets
it at the reader (`with_limit(remaining)`); the device at the scan call (cap at `limit`) plus
`slice_handle` for a later straddler. `output_rows`, `batch_rows`, `batch_bytes` in the
`.cpu.txt` section agree for every corpus loader after the fix (the bytes are schema-derived
plus exact varlen on both, the first batch being C++-capped).

### Pinning tests / goldens / registry / comments that move

- `tests/common/corpus_cases.inc:51`: scan_limit cpu modes → all five; gpu stays `none`.
  Comments `:44-50`: drop the #186 sentence; #188 → "#183 and #187 at the unload". `:78-79`:
  nested-limits' device reason → the cross-join per-call batch shape (new ticket, §6).
- `testdata/cost-registry.csv:135`: `cpu_tp1_single`, `cpu_tp1_rowgroup` → `enabled`; tickets
  `186 188` → `183 187`. `:131`: `188` → the new ticket's number.
- `testdata/goldens/tpch.sf1/<mode>.plans.txt` ×5: the three loader recipe lines (rust-only
  regen, `UPDATE_CANONICAL=1 ... --test test_plan_goldens`).
- `tests/test_plan_goldens.rs:174` `PAYLOAD_QUERIES`: add `("tpch", "nested-limits")` with
  the reason "the only recipe with a slice on a source, and the only payload carrying
  `CudfScan.limit`" — `the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce`
  (`:506`) demands it. `recipe-payloads.txt` gains one section under
  `PEACOCK_REWRITE_RECIPE_BYTES=1` with the fixed `/tmp` symlink; no existing digest moves.
- `testdata/goldens/tpch.sf1/<mode>-mini.cpu.txt` and `.cost.txt` ×5: scan-limit (loader
  `output_rows` 6001215/122880 → 10, `batch_bytes` → 1828, unload `in_rows` → 10; the two tp1
  sections authored for the first time, `early_exit=none` there) and nested-limits (`part`
  loader 200000/122880 → 28, `GpuLimit in_rows` → 28; `early_exit=GpuUnload@6,GpuLimit@3`
  stays — 28 ≥ 5+23). `mini.result.txt` unchanged in content (same first rows).
- `tests/test_corpus_goldens.rs:290` `a_loaders_batches_line_up_with_the_row_groups_that_made_
  them`: a loader whose line carries `limit=` emits a prefix of its mapping whatever the
  marker (`emitted <= count`), and Σ `batch_rows` ≤ that limit. Today the rule demands
  `emitted == count` under `early_exit=none`, which scan-limit at tp1-rowgroup (1 of 49) would
  fail.
- `src/planner/translator/tests.rs:604`: stays as is; its comment becomes literally true.
- `test_cpu_end_to_end.rs:426` `a_limit_slices_at_most_two_batches_and_stops_the_scan`: passes
  unchanged (`pulled == 2`, `most_offered > 2`, `satisfied` non-empty); its comment "the scan
  under it never reads a second one whatever the batching" now holds for two reasons.
- New tests, red before the fix:
  - `executor/cpu_backend/tests/source.rs` (fixture: six rows in three row groups of two):
    `a_lane_stops_at_its_limit_and_reads_no_further_row_group` — mapping `[[[0],[1,2]]]`,
    limit 3 → batches `[[1,2],[3]]` then `Exhausted`; and limit 2 → `[[1,2]]` (one batch, the
    bound met on a boundary). Today both return all six.
  - `tests/test_gpu_executors/` (same six-row fixture, `mapped(...)` + a live session): the
    twin, driven through `GpuSource`; asserts the same two batch lists, and `batch_bytes` of
    the 3-row second batch equals the CPU's (Int64 only, so exact on both).
  - `cpp/tests/gpu/test_plan_executor.cpp` `ScanRowGroups`: `ALimitCapsWhatOneCallReturns` —
    `customer_scan_plan` with `limit = 3` (6th arg of `CreateCudfScan`), `execute_scan_rowgroups
    (0, {0})` → 3 rows, `stats.rows == 3`; `{1}` → 3 rows as well (the cap is per call).
    Today: throws the cuDF message.
  - `tests/test_gpu_abi.rs`: `a_scan_carrying_a_limit_reads_its_row_groups` — plan
    `SELECT c_custkey FROM customer LIMIT 3` at `KNOBS` (the #186/#188 shape,
    `Unload(LoadParquet(limit=3))`), one `execute_scan_rowgroups` over `[0]` → 3 rows. The
    minimum reproduction on the boundary the driver crosses.
- `build-test.md`: N for Lib unit, Executors on a device, Per-call ABI, Plan-executor (C++),
  Corpus cpu (447 → 449) and the grand total; row 42's "37 queries / thirteen on #163" drift
  while there.
- `architecture.md`: `:374-378` (the bold rule and the `set_num_rows` sentence — rewrite to
  the rule above); `:763` (add `slice_handle` on the batch that crosses a pushed-down limit);
  `:814` and `:1043` (`set_num_rows(limit)` → "the table read capped at `limit` rows");
  `:1073` (`parquet_reader_options::set_num_rows` → "a slice of the table read to its first
  `limit` rows, per call"). `:181` becomes true as written.
- `tasks/active-tickets.md`: #186 and #188 close together.

### hacks-audit scaffolding

- Finding 2 (`Some(0)`/`None`): touched and decided as above; the wire keeps `0 = none`, the
  executor's `Option` is the authority.
- Finding 1 (`slice_handle` priced with no string bytes): not fixed; the shared `sliced()`
  puts the approximation in one place instead of two. Reachable from a loader only for a
  limit larger than its first batch.
- "GpuLoadParquet.limit is covered on neither backend" (tests section): closed by the twin
  source tests and the re-enabled tp1 cells.
- Finding 8 (a stream counted twice): not added to — the loader is the only counter of its
  own output.
- Finding 4 (`limit_interval`'s dead coalesce arm) and the `nodes.rs:85` live one: untouched;
  not on this path.

## 4. Alternatives rejected

- **Planner emits the interval as a node (`GpuLimit{0,n}` over a limit-free loader), the
  loader never sees a limit.** At tp4 DataFusion already leaves the interval on the root and
  the bound in the scan, so the wrapper would sit directly under the sink — `limit_positions`
  refuses that shape — and dropping it only where an interval owner is directly above needs
  the translator to look upward; where DataFusion erases a limit under a join (a limited
  subquery with no offset at tp1) the wrapper is needed mid-plan anyway. New nodes at all five
  modes for both queries, renumbered seqs, moved memory sections, for the same statement the
  loader can make about itself.
- **Planner forces one batch for a limited scan (architecture.md's own sentence).** Reads the
  whole table into one device table for `LIMIT 10` at every mode, moves `partition_groups`
  and memory at both rowgroup modes, and makes `a_limit_slices_at_most_two_batches_and_stops_
  the_scan`'s `most_offered > 2` false — the test says a one-batch plan "had nothing to stop".
  The invariant would also hold only by planner discipline, unchecked for injected plans.
- **Keep `set_num_rows` when no row groups are named, cap otherwise.** Two mechanisms for one
  field, the first reachable only from a hand-built plan.
- **Drop `limit` from the wire; trim only in Rust via `slice_handle`.** The slice reports no
  stats, so every string-bearing limited scan would be priced without its strings on the
  device and its `batch_bytes` would disagree with the cpu-authored section — finding 1's hole
  moved onto the common path.
- **Driver-owned satisfaction (`row_interval()` on the loader).** Trips `limit_positions` for
  the tp1 shape, needs `rows_seen` to mean produced rows for one category, and adds a third
  counter of the same stream beside the executor's trim.
- **Planner trims the mapping to the row-group prefix that covers the limit.** Right, and the
  real answer to reading 371 MB for ten rows, but a mapping-policy change moving plan goldens
  and memory sections; an optimization above this fix, its own task.
- **Leave the loader's recipe at one call.** Saves ten recipe lines and one payload section;
  leaves the executor making a per-call ABI symbol its recipe does not name, which no other
  executor does.

## 5. Minimum corpus query

    SELECT c_custkey FROM customer LIMIT 3

tpch sf1 (customer: 150,000 rows in two row groups, 122,880 + 27,120; one Int64 column, so no
#183/#187 at the unload). Plans at all five modes: tp1-single/tp1-rowgroup as
`GpuUnload` over `GpuLoadParquet: ... limit=3, lanes=1` with no interval; the three tp4 modes
as `GpuUnload: skip=0, fetch=3` over the same loader. Refused nowhere at plan time; the
recipe crosses.

- CPU, tp1-single and tp1-rowgroup: returns 150,000 rows (#186). CPU, tp4 modes: 3 rows.
- GPU, every mode: the first call fails — `execute_scan_rowgroups(#0, [0]):
  NodeSession::execute_scan_rowgroups: seq 0 reading row groups [0]: CUDF failure at
  .../functions.cpp:808: row_groups can't be set along with skip_rows and num_rows` — surfaced
  by `gpu_backend/source.rs:79-84` and failed by the driver at `GpuLoadParquet lane 0`.

The later-straddler arm: `SELECT c_custkey FROM customer LIMIT 122883` at tp1-rowgroup or
tp4-rowgroup (mapping `[[[0],[1]]]`): after the fix, batch 1 is 122,880 rows whole, batch 2 is
3 rows — cut by the reader on the CPU, by `slice_handle` on the device. Today: 150,000 rows on
the CPU at tp1, the same refusal on the device.

Worth adding as a `corpus_query!` line (`data_fusion_subset`, `live_cpu`, both engines at all
five modes): it would be the first device cell with a pushed-down limit, since neither
existing limited query can pass on a device behind its next wall.

## 6. Cells re-enabled

- **Back on**: `tpch/scan_limit` cpu × tp1-single, tp1-rowgroup (registry line 135). Two cells,
  #186's.
- **Refusal removed, cell still off**:
  - `tpch/scan_limit` gpu × 5 → #183 (`Utf8View`) and #187 (`Decimal128(15,2)`) at the unload;
    registry tickets `186 188` → `183 187`. Also latent there: `mini.result.txt:1320` holds a
    frozen scan-limit section while its `gpu_oracle` is `live_cpu`, and
    `assert_oracle_suits_the_golden` (`corpus_gpu.rs`) refuses that pairing — to be resolved
    when those cells are enabled, not here.
  - `tpch/nested_limits` gpu × 5 → the cross join's per-call output is five batches on the CPU
    (`cpu_backend/join.rs:111-121, 236-251`: `CrossJoinExec` output returned as DataFusion
    chunked it, one batch per build row) and one table on the device (`cudf::cross_join`), so
    `GpuCrossJoin batch_rows`, `abandoned` and the unload's `in_rows` differ and the section
    compare fails. Needs a ticket: "a CPU join returns a probe call's output as DataFusion
    chunked it, the device as one table" — the smallest fix is a `concat_batches` in
    `CpuJoin::probe_and_fetch`'s `declared(joined)` for the `one_call` family (cross,
    nested-loop, and hash joins whose output DataFusion splits at `batch_size`), which also
    brings the CPU to architecture.md's "no executor may return more than one batch per call
    per output lane". Moves `cross-join` and `nested-limits` cpu goldens. Not #188's.
- **Unchanged**: everything else; no tpcds plan carries a limited scan.

## 7. Risks and unknowns

- `cudf::slice` + `cudf::table(table_view)` after a `read_parquet` in the 25.02 build on
  shad-gpu: standard calls, not exercised here; the gtest is what proves them.
- parquet 54 `with_limit` composing with `with_row_groups` was read in the crate source, not
  run. If it misbehaves, the fallback is `concat` then `RecordBatch::slice(0, remaining)`
  (the `LimitStream` CPU form) — same rows, same batch boundaries, one line more.
- The device still decodes every row group a limited batch names before capping: 371 MB
  transient for scan-limit at `Off` batching, unpriced (`GpuSource::scratch_bytes` is 0, as
  for every source). Pre-existing on the CPU at tp4-single; the mapping-prefix optimization in
  §4 is the remedy.
- The device `sliced()` arm on a loader prices strings at zero (finding 1); reachable only for
  a limit larger than the first batch, so no corpus cell, but a hand-written case with strings
  would show a `batch_bytes` disagreement between engines.
- `GpuSource::new`'s recipe check and `test_gpu_recipe_walk.rs:308` (`let [call] = ...` on a
  scan) — the walk plans no limit, so it is unaffected; if a limited shape is ever added there,
  its `source()` arm must accept the trailing slice.
- The new validation rule against `tests/common/rebuild.rs:394`'s two-lane limited fixture:
  believed never validated; confirm by running `test_layout_injection` before keeping it.
- Enabling scan-limit at tp1 on the CPU makes `data_fusion_subset`'s containment stream six
  million lineitem rows per cell (`corpus.rs:155-159` says so); two more such cells in CI's
  dataset-matrix leg.
- Result stability: scan-limit's rows are the first ten of row group 0 on both backends before
  and after; `live_cpu`/subset oracles do not care, and the frozen section's rows do not move.

## 8. Complexity

**M.** Code ~60 lines across `scan.cpp`, `cpu_backend/source.rs`, `gpu_backend/{source,mod,
accumulate}.rs`, `wire/attach.rs`, `plan/source.rs`; tests ~120 lines across four tiers (CPU
unit, device executors, per-call ABI, gtest) plus two small test-rule edits. No frozen surface
moves: no ABI symbol, no fbs field, no wire bytes of any existing payload. Goldens regenerated:
five tpch plan goldens (recipe lines only), five `.cpu.txt` + five `.cost.txt` sections for two
queries, one added `recipe-payloads.txt` section (a byte-golden write, so it takes
`PEACOCK_REWRITE_RECIPE_BYTES=1` deliberately). Two registry rows, one corpus line and its
comments, six architecture sentences, and two tickets closed. The reason it is not S: two
backends, the C++ and four golden files must move in one commit for the tiers to stay green
together, and the corpus regen needs the sf1 dataset.
