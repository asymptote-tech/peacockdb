# The device cells a closed ticket still holds off are run

Kind: production

**This task closes no ticket.** Four tpch rows keep device cells off under tickets that are
closed: `aggregate_groupby`, `shuffle_additive` and `shuffle_stddev` under #183 (utf8-everywhere,
archived done), `filter_project` under #187 (decimal precision at export, archived done). The
archived #183 kept its tag on a row "only where the other four modes have no ticket yet"
(`corpus_cases.inc:53`: "a row keeping `183` has modes never run past the string class"). So 16
device cells — `tp1_rowgroup`, `tp4_single`, `tp4_rowgroup`, `tp4_sized` on each of the four — have
never been run since their tickets closed. Second of the join-rewrite chain, right after
duckdb-oracle: none of these rows has a join, so nothing later in the chain changes them, and each
cell meets DuckDB as it turns on.

The estimate (`reports/join-rewrite-cell-estimate.md` §6) finds no known blocker on them. The
chain's other `183` rows hold join cells (tpch nested-loop-join, q4, cross-join,
nested-loop-left-join, semi-join, anti-join, q15); #152 blocks those, and join-backend runs them.

## The work

1. Run the 16 cells on shad-gpu at their modes, each compared with the cpu's golden as its line
   says and with DuckDB.
2. A cell that passes is enabled. A cell that fails gets the ticket it fails on — an open one, or
   a new one filed with the query and mode that shows it — and stays off under it.
3. `183` and `187` are struck from the four rows' tags once no off cell is left without another
   ticket (`registry.rs`'s rule); the `corpus_cases.inc:53` comment is reworded or removed when no
   row keeps `183`.
4. `shuffle_stddev` runs with schema validation disabled on #225, as its line says; a cell that
   fails on schema alone keeps `225`.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc` | the four lines' device modes; the comment |
| `testdata/cost-registry.csv` | the four rows' cells and tags |
| `llm-wiki/tickets/`, `build-test.md` | any ticket a failure needs; counts |

Component-level API: none. No code change unless a failure is fixed here, which it is not: a
failure is a ticket.

## Restriction

The 16 cells. No fix to anything they show.

## Verification bar

- device: the 16 cells run, each enabled or carrying its ticket; their DuckDB cases green for
  the enabled ones.
- rust-only: the registry tests both ways.

## Device workflow

`build-test-shadgpu.sh`, one cycle under the corpus filter for the four rows.

## Completeness signoff (2026-10-09)

Solved under its constraints. All sixteen cells ran and all sixteen passed and met DuckDB, so
item 2 is vacuous and no ticket was owed; `183` came off three rows and `187` off the fourth,
which takes `187` out of the registry entirely. Seven join rows still keep `183` and are
join-backend's, so `corpus_cases.inc`'s comment stays under item 3's own condition. No code
changed, so the Restriction held by construction.

**Measured on nebius-gpu, an L40S at cuDF 25.02, not the shad-gpu this spec names** — the human's
host override routed it, shad-gpu being down throughout. Deferred by that override and by nothing
else: `--run-benchmarks` and the `bench_` cases, Nsight, any H200 timing, the sf40 pair, and
cuDF 26.02. The DuckDB half of the evidence is re-runnable without a device, by the 38
`duckdb_gpu_tpch_` cases against the committed `gpu-result.txt`.

One golden moved and it is accepted as float noise: `shuffle-stddev mode=tp1-single`, three lines
in their last digits, 8.2e-16 relative against tolerances of 1e-11. The completeness pass found
`architecture.md` claiming a plan run twice is byte-identical, which this contradicts and
`build-test.md` already denied; that sentence now names the exception. No bandaid applied.
