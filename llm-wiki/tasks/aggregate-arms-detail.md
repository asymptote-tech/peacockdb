# aggregate-arms — working record

## Standing facts

- Seventh of chain K (folded in from chain L), branch `ENS-aggregate-arms` off
  `ENS-keyless-identity`; its PR targets `ENS-keyless-identity`. Base master `ca4dedeb`; chain J's
  refcounted-scatter (`TableResult::owning`) and join-backend (`join.cpp` deleted, the join in
  `join_session.cpp`) are in it.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- Adding `rollup-stddev.sql` moves `testdata/test_duckdb_result.py`'s tpch pin (42 → 43). Run
  every cost-report CI step locally before reporting.
- #225's schema-hook masks beyond the spec's list: empty-sorts left `tpcds/q17` at
  `schema_validation_disabled(tp1_single | tp1_rowgroup)` on #225, and distinct-functions carries
  `disabled(tp4_single | tp4_rowgroup | tp4_sized)` on #225. With #225 closed, every such mask
  goes, and the hook runs at those cells.
- keyless-identity's done call is priced at zero bytes; not this task's.

## Dispatch log

### 2026-10-20 — `building`, developer dispatched
