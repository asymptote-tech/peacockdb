# A sort over zero-row batches answers a zero-row batch, and an empty answer keeps its columns

Kind: production

**This task closes [#205](../tickets/corpus-coverage.md#t205)** (the cpu's accumulating sort and
merge answer nothing over zero-row batches). Fourth of chain K, after limits; runs without a GPU.

## Why it happens

DataFusion's `SortExec` over zero rows yields no batch at all. `SortedRuns::mark_done_and_fetch`
and `CpuPartitionAccumulator::accumulate_and_fetch` (`cpu_backend/accumulate.rs`) hand that empty
answer to `coalesce_or_nothing`, which reads it as a lane that received nothing. So a
`GpuAccumulateBatchesAndSort` or `GpuMergeSortedPartitions` whose only batches have zero rows emits
no batch on the cpu, where the device emits one of zero rows; `CpuExec::exec` and the cpu coalesce
answer zero rows under the schema, so the two cpu paths disagree with each other as well.

An empty answer also loses its columns. `tpcds/q17` answers zero rows; its `mini.result.txt`
section is a bare `++`/`++`, with no names or types, so nothing checks them, and DuckDB's answer
(`duckdb-result.txt`, chain J's duckdb-oracle) prints the header ours lacks.

## The work

1. **The sort and the merge.** When every held batch has zero rows, `SortedRuns` and
   `CpuPartitionAccumulator` skip the sort and pass the held batches to `coalesce_or_nothing`,
   which concatenates them into one zero-row batch under the schema, as the device answers. A fetch
   has no rows to cut. A lane that received nothing still answers nothing.
2. **An empty answer keeps its schema.** A query whose root received nothing answers one
   zero-row batch under the sink's declared schema. The driver makes it, once, for both backends:
   the unload is called per batch and is never told that none came, and telling it would change
   its trait. The batch is the answer only: it is not recorded as an emitted batch, so the
   execution goldens move only where the rows did. DataFusion's own empty answer, which the corpus
   compares against (`test_support/corpus.rs`), gets its columns the same way, or every zero-row
   query's cpu cells would differ on the header alone. So an answer's schema never depends on how
   its rows ran out. q17's result section is regenerated with its header: tp4-sized writes it,
   where the scatter leaves every lane of the merge empty, which is this rule's case rather than
   1's.
3. **DuckDB.** If chain J's duckdb-oracle has merged when this task builds, q17's empty-answer
   divergence against `duckdb-result.txt` goes with 2, and its line in that task's divergence list
   is struck. If not, nothing here touches DuckDB, and the helper merging duckdb-oracle after this
   task strikes what names #205 there, since #205 is archived here: q17's `duckdb_divergent(205)`,
   the tests asserting #205 open (`duckdb_oracle/tests.rs:157-158`, `:270-278`, `:322-323`) and the
   empty-answer branch in `duckdb_oracle.rs:113-115`. If duckdb-oracle has not merged when this task
   lands, this list is also written into #235's empty-answer bullet.

## Corpus

`tpcds/q17` at `tp1-single`, the one enabled corpus query answering zero rows: its result section
gains its header. Its device cell at `tp1-single`, off on `205`, stays off under
[#281](../tickets/corpus-coverage.md#t281) (chain K has no GPU), and `205` leaves the row.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs` | the sort and merge over zero rows |
| `peacockdb-core/src/executor/driver/` | the empty answer under the sink's schema |
| `peacockdb-core/src/test_support/corpus.rs` | DataFusion's empty answer with its columns |
| `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` | the three `bug_` pins |
| `peacockdb-core/src/tests/end_to_end.rs` | the empty ordered query |
| `testdata/goldens/tpcds.sf1/*mini.result.txt`, q17's `tp1-single` and `tp1-rowgroup` `.cpu.txt` and `.cost.txt` sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | q17 |
| `llm-wiki/architecture.md` ("Zero-row batches change no answer"), `build-test.md`, `tickets/` | the break struck; counts; #205 archived |

Component-level API: `RunReport::batches` (`executor/mod.rs`, public) is never empty for a query
whose root received nothing; no signature changes.

## Restriction

The cpu's two accumulating sorts and the empty answer's schema. The limit's zero-row drop is #214;
a keyless aggregate over nothing is #199 (chain L).

## Tests

- The three `bug_` pins (`bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu`,
  `bug_a_fetch_over_zero_rows_is_nothing_on_the_cpu`,
  `bug_every_lane_a_zero_row_batch_is_nothing_on_the_cpu`, `gpu_tests/accumulate_cases.rs`)
  become agreement cases under names without `bug_`; built here, run on the next GPU run (#281).
- cpu-backend cases: one zero-row batch, two, and one per lane of a merge each answer one zero-row
  batch under the schema; a lane with no batch answers nothing; a fetch over zero rows answers zero
  rows.
- End-to-end on the cpu, the five modes: `select n_name from nation where n_nationkey +
  n_regionkey < 0 order by n_name;` (tpch) answers zero rows under one `n_name` column. Not
  `n_nationkey < 0`: its statistics prune nation's one row group, and a scan of nothing is refused
  until chain L fixes #282.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- device: none.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU. No device run, no GPU cycle. This task reaches `done` when every CI
job but the GPU tests is green; the GPU jobs are not waited on. The device cases are built, not
run; #281 holds them.

## Completeness signoff

Solved under its constraints. Both rules landed as written, every line of the Tests list exists
with the case the spec names — including the unpruned predicate — and q17's new result section is
byte-identical to the DuckDB oracle's, so #235's empty-answer divergence is closed in fact and not
only on paper. Two readings: the reviewer found nothing blocking or important, the analyst four
things, all of them prose the branch falsified or left unwritten, now applied. Shortcuts and
deviations, three: the device half — the three converted pins and one new mixed-merge case — is
built and not run under the chain's no-GPU rule, held by #281; those four cases assert only
cpu-device agreement, so none of them alone can tell "one zero-row batch on both" from "nothing on
both", which the cpu-side pins and a real run cover instead; and two review nits were traded
rather than fixed — `count_of`'s now-unreachable `expect`, and an end-to-end case that runs its
query twice at five modes because the harness hands no report back.
