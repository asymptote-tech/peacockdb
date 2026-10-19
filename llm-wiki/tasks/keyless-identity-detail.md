# keyless-identity — working record

## Standing facts

- Sixth of chain K (folded in from chain L), branch `ENS-keyless-identity` off `ENS-grouping-id`;
  its PR targets `ENS-grouping-id`. Base master `ca4dedeb`; empty-sorts (`Driver::answer`) and
  grouping-id are below it on the chain.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- grouping-id made the walk harness (`wire/gpu_tests/mod.rs`, `per_lane`) skip the at-done
  collapse on a lane with no batch, "as every device accumulator does". This task's
  `AtDoneIfNothingOut` makes that reason false: revisit it.
- Adding a query file moves pinned counts: `testdata/test_duckdb_result.py` pins tpch at 42 and
  pbench by a floor. Run the cost-report job's Python suites locally (grouping-id's lesson).

## Dispatch log

### 2026-10-19 — `building`, developer dispatched
