# The device cells a closed ticket still holds off are run

Kind: production

**This task closes no ticket.** Four tpch rows keep device cells off under tickets that are
closed: `aggregate_groupby`, `shuffle_additive` and `shuffle_stddev` under #183 (utf8-everywhere,
archived done), `filter_project` under #187 (decimal precision at export, archived done). The
archived #183 kept its tag on a row "only where the other four modes have no ticket yet"
(`corpus_cases.inc:42`: "a row keeping `183` has modes never run past the string class"). So 16
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
   ticket (`registry.rs`'s rule); the `corpus_cases.inc:42` comment is reworded or removed when no
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
