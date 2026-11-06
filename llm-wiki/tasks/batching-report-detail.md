# batching-report — detail

Branch `ENS-batching-report`, forked from `ENS-query-fuser` at `413366c4` on 2026-11-06. PR targets
`ENS-query-fuser`. Plan: [`batching-report-impl.md`](batching-report-impl.md).

## Progress

- 2026-11-06: branched, board `building`. Verda unavailable (`VERDA_CLIENT_ID` unset); tests run
  locally. No VM needed. Developer dispatched at Task 1 Step 1.
- Inputs from query-fuser: the fused format and the merge/pipeline hints are documented in
  `query-fuser-detail.md` ("The fused format"). Its completeness pass notes that part of the fused
  load saving is deduplication inside one query (`shared=` shows `q14+q14+q14`), which a batch of
  one would also show; the merge hint counts cross-query groups only (since `7b1db00e`).

## Checkpoints

| step | commit | note |
|---|---|---|

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
