# containers-only — run detail

Branch `ENS-containers-only`, forked from `ENS-tp4-sized` at `9058709c`; its PR targets
`ENS-tp4-sized`.

## Progress

- 2026-11-07T11:23Z: branched; board `building`; developer dispatched at Task 1 Step 1.

## Notes for a restart

- `images.yml` is on master (workflow "Images", id 409646666), from the merge of #193; Task 1
  Step 8's master check passes. A dispatch runs the ref's own copy of the file.
- dev's disk: 13 GB free on `/` at branch time; local images `peacockdb-build:cudf-25.02` (15.4 GB),
  `peacockdb-run:cudf-26.02` (10.2 GB), `peacockdb-run:cudf-25.02` (8.3 GB). The run images are
  re-pullable and may be removed to make room for Task 1 Step 6's 25.10 build.
- `testdata/calibration/hbm-parts/` in this worktree is an untracked leftover of tp4-sized: never
  stage it.
- verda: no address known (`list_verda_instances.sh` lacks `VERDA_CLIENT_ID`); treated as down.

## Counts before

(Task 3 Step 1, coordinator.)

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
