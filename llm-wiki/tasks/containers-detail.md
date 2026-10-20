# containers — detail

## Known state

- The branch `ENS-containers` starts from `ENS-optimizer` (`611b5dcd`, PR #165, not yet merged);
  Task 10 moves it onto master once that merges.
- Facts the plan rests on, read on `611b5dcd`: CI builds in `rapidsai/base:25.02-cuda12.0-py3.12`
  and `rapidsai/base:25.10a-cuda12-py3.12` (the leg named 26.02, #129); the committed device
  answers came from nebius-gpu's `rapids-cuda-12.2` (libcudf 25.02.02, Arrow 19.0.1); its explicit
  specs exist only on that VM; `docker/cudf-build.Dockerfile` and `scripts/docker-build.sh` were in
  master from 2026-08-07 to `dbf44bcc` (2026-10-08), a build-only image retired when the build
  moved onto nebius-gpu natively.
- The benchmark today times 26 hand-listed cases (`corpus_benchmark_cases.inc`); the registry
  enables about 534 tpch and tpcds device cells. The H200 run of the 26 took 3,602 s, 96 % of it the
  six rowgroup cases. The benchmark pool is a fixed 69 GiB.
- Deferred to Task 11, the human's: the rowgroup modes for every query, and the source of tpcds
  sf40 (15 GB, in no bucket).
