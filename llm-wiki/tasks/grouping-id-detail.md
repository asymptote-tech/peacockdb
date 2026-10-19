# grouping-id — working record

## Standing facts

- Fifth of chain K (folded in from chain L), branch `ENS-grouping-id` off `ENS-empty-sorts`
  (`ad2bd77b`); its PR targets `ENS-empty-sorts`. The chain's base is master `ca4dedeb`, which
  carries chain J (#152, #220, #189 closed) and the DISTINCT lowering.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable: no `VERDA_CLIENT_ID`). The final device run is the dispatched `GPU Tests
  (nebius-gpu)` CI job.
- Pulling `gpu-result.txt` home from nebius-gpu: rsync it back through `gpu-host.sh rsh` before
  the next sync, which overwrites the VM's copy (learned on empty-sorts).
- q17's tp1 device cells are masked on #225, set by empty-sorts; not this task's.

## Dispatch log

### 2026-10-19 — `building`, developer dispatched

nebius-gpu started for the dispatch. Developer told: spec and `grouping-id-impl.md`, red build
first, corpus run filtered to the spec's rows and `rollup-grouping`.
