# welford-device — working record

## Standing facts

- Eighth and last of chain K (folded in from chain L), branch `ENS-welford-device` off
  `ENS-aggregate-arms`; its PR targets `ENS-aggregate-arms`. Base master `ca4dedeb`.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- Adding `global-stddev.sql` moves `testdata/test_duckdb_result.py`'s tpch pin (43 → 44). Run
  every cost-report CI step locally before reporting.
- Spec drift from the chain below: aggregate-arms left one `MERGE_M2` site, `merge_m2_input`
  (`aggregate.cpp`), with `constexpr kMergeM2CountType = INT32` not yet version-gated — item 4
  gates that constant. The keyless path is `keyless_aggregate`; its `stddev` reduction uses
  `func->ddof()`. The finalize's refusal message is now `ColumnRef stddev(…)$m2@2 is past the 1
  columns its input has`, not the spec's `ColumnRef index 2 out of range`. chain J's
  verify-26.02 has not run (`new`, on hold), so #94 is archived here.
- keyless-identity's init done call (`AtDoneIfNothingOut`) is the only zero-row keyless call.

## Dispatch log

### 2026-10-20 — `building`, developer dispatched
