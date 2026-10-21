# interval-days — run record

## Run log

- 2026-10-21: branched `ENS-interval-days` off `ENS-oracle-cleanup`. Developer dispatched.
  verda: no address, CPU tests local. GPU: nebius-gpu, tree `~/peacockdb-K`. oracle-cleanup's
  grammar is in place: `duckdb(sort|no_sort, …, date("<column>"))`; `date` is not in `FACETS`
  yet (deliberately, for this task). An ORDER BY query must declare `no_sort` (order guard).
