# A scan's limit is a limit node, and a limit's rows are counted once

Kind: production

**This task closes [#186](../tickets/corpus-coverage.md#t186)** (a limit pushed into the scan:
the cpu ignores it, the device refuses it) **and [#234](../tickets/corpus-coverage.md#t234)** (a
mid-plan limit is counted twice, by the driver and by its executor, and nothing compares them).
Third of chain K, after distinct-companions; the chain runs without a GPU.

## Why it happens

**#186.** DataFusion pushes a `LIMIT` into the scan (`DataSourceExec`'s `limit`) and, at tp1,
removes the limit node above it, so the scan's limit is the whole cut. `source()`
(`planner/translator/nodes.rs`) copies it onto `GpuLoadParquet.limit` and gives the scan one lane
(`lanes_for`), but each reader is left to apply it, per call. `CpuSource` never reads it:
`SELECT * FROM lineitem LIMIT 10` answers 6,001,215 rows on the cpu at the tp1 modes. `scan.cpp`
calls `set_num_rows(limit)` and then sets the call's row groups, which cuDF refuses together, so
every device mode fails its first read. And a per-call limit is right only for a scan of one batch,
which the rowgroup and sized modes do not map.

**#234.** For a mid-plan `GpuLimit`, the driver adds each consumed batch's rows to `rows_seen`
(`executor/driver/partitioned.rs`), and `settle_limit` reads it to stop pulling from below. Each
backend's `LimitStream` (`cpu_backend/accumulate.rs`, `gpu_backend/accumulate.rs`) keeps its own
`seen` and decides per batch whether to drop, forward or slice it. Two counts of one stream for
two decisions; they agree today, and a drift would be a LIMIT returning short or reading more than
it needs, with no test to notice.

## The work

1. **#186, a scan's limit is a limit node.** `source()` builds the `GpuLoadParquet` with no limit
   and, where DataFusion's scan carries one, puts a `GpuLimit` with `RowInterval { skip: 0, fetch:
   Some(n) }` directly above it. Where the scan's limit is the whole query's cut — the scan sits
   under the sink, as in `scan-limit` — the cut joins the unload's interval instead, since the
   validator refuses a `GpuLimit` whose only parent is the sink (`plan/validate.rs`). DataFusion
   pushes `skip + fetch` into the scan and keeps its own limit above for any skip, so the scan's cut
   never carries a skip. `lanes_for` keeps its limit arm: one lane, since rows from several
   lanes have no order a cut could follow. The scan's mapping is the mode's own: one batch, one per
   row group, or sized. The limit cuts the batch that straddles `n`, and once it is satisfied the
   driver's hold on its subtree (`settle_limit` → `scheduler.satisfy`) stops the scan. That alone
   bounds the read only where batches are small: at tp1-single, tp4-single and tp4-sized lineitem is
   one batch of all 49 row groups, so `LIMIT 10` would still read the file. So `source()` also
   trims a limited scan's survivors to the shortest prefix whose row counts (from the parquet
   metadata it already holds) reach `n`, before mapping: a small limit reads one row group in every
   mode.
2. **#186, the readers lose their limit.** `GpuLoadParquet.limit` goes, and with it the plan
   text's `limit=` on a source (`plan_text/node_text.rs`). The wire's `CudfScan.limit` is
   deprecated, no slot moved, as chain K deprecated `AggregateFuncNode.distinct`; the writer stops
   filling it, and `scan.cpp` drops `set_num_rows`. `CpuSource` needs nothing: it never read the
   limit.
3. **#234, one count.** The limit's executor keeps the only input count and every drop, forward
   and slice decision, as now. The driver stops counting the limit's input: it counts the rows the
   limit emits, which the lane outcome already carries, and `settle_limit` marks the limit satisfied
   when that reaches `fetch`. A limit with no `fetch` is never satisfied, and one with `fetch 0` is
   satisfied before any read, where today it pulls one batch first. The driver's number is
   then a consequence of the executor's decisions, not a second computation of them. The unload's
   interval, which only the driver decides, keeps counting its input as now.

## Corpus

`tpch/scan-limit` and `tpch/nested-limits` are the only corpus queries with a limited scan (three
at tp1-single). Their plan goldens at the five modes lose the source's `limit=`. scan-limit's cut
joins its unload's interval (`skip=0, fetch=10` at tp1, where it had none). nested-limits gains two
`GpuLimit`s, one over each limited scan: region's (`limit=23`) and part's, beside its existing one.
Their execution sections are regenerated. `mini.result.txt` does not change.
scan-limit's cpu cells at `tp1-single` and `tp1-rowgroup` turn on and `186` leaves their tags.
Their device cells, and nested-limits', stay off under [#281](../tickets/corpus-coverage.md#t281),
filed with this chain: chain K has no GPU.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/translator/nodes.rs`, `translator/mod.rs`, `plan/interval.rs` | `source()`: the limit node above the scan, or the unload's interval |
| `peacockdb-core/src/plan/source.rs`, `plan/mod.rs`, `plan_text/node_text.rs`, every `GpuLoadParquet` constructor's caller, `tests/rebuild.rs` | `GpuLoadParquet.limit` gone |
| `flatbuffers/gpu_plan.fbs`, `wire/node_writer.rs`, `wire/fb_text.rs`, `cpp/src/operators/scan.cpp` | `CudfScan.limit` deprecated; `set_num_rows` gone |
| `peacockdb-core/src/executor/driver/partitioned.rs`, `executor/driver/tests/` (the mock included) | a limit satisfied by what it emits |
| `peacockdb-core/src/tests/end_to_end/limits.rs` | the limit cases |
| `peacockdb-core/src/tests/gpu_tests/source_cases.rs` | the four #186 `bug_` cases |
| `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's sections for the two queries, `corpus_cases.inc`, `testdata/cost-registry.csv` | scan-limit, nested-limits |
| `llm-wiki/architecture.md` (a limited scan; the driver's limit), `build-test.md`, `tickets/` | as in the work; counts; #186 and #234 archived; #281 filed; #229's line on the scan's limit (`memory.md`) corrected |

Component-level API: the wire format, `CudfScan.limit` deprecated, no slot moved. No facade, trait
or ABI symbol change.

## Restriction

The scan's limit and the limit's count. The limit's handling of a zero-row batch stays as it is
([#214](../tickets/corpus-coverage.md#t214), whose fix is in the joins). The unload's interval does
not change.

## Tests

- Translator: `select n_nationkey from nation limit 3;` (tpch) plans a one-lane scan carrying no
  limit under an unload whose interval is `fetch 3`, at every mode; a limited scan below a join or
  a subquery plans a `GpuLimit` above it; a scan without a limit gains no node; a limited scan maps
  only the prefix of row groups that reaches `n`, at every mode.
- End-to-end on the cpu, the five modes: `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)`
  answers 3. Today it answers 25 at every mode: the limit sits in the scan under an aggregate, and
  no reader applies it.
- Driver (mock): a mid-plan limit is satisfied when the rows it emitted reach `fetch` and not
  before; an executor that emits fewer than its input's rows keeps the driver pulling; a limit with
  no `fetch` is never satisfied.
- End-to-end on the cpu, the five modes: `select * from lineitem limit 10` answers 10 rows, and
  `a_limit_slices_at_most_two_batches_and_stops_the_scan` holds over the scan's own mapping.
- The four `bug_` cases in `gpu_tests/source_cases.rs` lose their inputs with the scan's limit:
  they become one agreement case of the scan and three of a `GpuLimit` over the scan's batches,
  under names without `bug_`; built here, run on the next GPU run (#281).

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: the build against cuDF 25.02 locally, `ctest -L cpu`.
- device: none.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU. No device run, no GPU cycle. This task reaches `done` when every CI
job but the GPU tests is green; the GPU jobs are not waited on. Its `scan.cpp` edit and the device
cases are built, not run; #281 holds them.
