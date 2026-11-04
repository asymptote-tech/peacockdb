# comet-research — run state

## Coordinator

- 2026-11-04: run started on `ENS-comet-research`; state `building`. The developer is dispatched
  one plan task at a time (`comet-research-impl.md`); each checkpoint is committed by the
  coordinator with the plan's message.

## Tasks

- Task 0 (set up the run): done, committed.
- Task 1 (desk wave): todo
- Task 2 (experiment scripts): todo
- Task 3 (experiment 1, VM-to-S3 throughput): todo
- Task 4 (experiments 2 and 3, Comet on the CPU VM, plan dumps): todo
- Task 5 (analysis and the cost model): todo
- Task 6 (the report and its audit): todo

Plan drift (fixed by the coordinator, 2026-11-04): Global Constraints and Review Focus in
`comet-research-impl.md` were renumbered to match the headings, Task 0 to Task 6.

## Clones

Directory: `~/comet-research/ext/` on the local host (outside every worktree). Blobless clones
(`--filter=blob:none`, full history and tags), made 2026-11-04.

| Repo | Path | Ref | HEAD SHA |
|---|---|---|---|
| apache/datafusion-comet | `~/comet-research/ext/datafusion-comet` | tag `1.2.0` (detached) | `ab46419fcc99bcd34aa4d89f7b545d5d51694a88` |
| NVIDIA/cudf-spark (was NVIDIA/spark-rapids) | `~/comet-research/ext/cudf-spark` | `main` | `ce6349699ccb3bd527c223c66ba24bd708e67e18` |
| apache/gluten (was apache/incubator-gluten) | `~/comet-research/ext/gluten` | `main` | `d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f` |
| apache/auron | `~/comet-research/ext/auron` | `master` | `cdd73ecfafa52a96a2602235930dc309edb9c019` |

`NVIDIA/spark-rapids` and `apache/incubator-gluten` now redirect to the names above (GitHub
API, 2026-11-04).

### Comet pin

- Tag `1.2.0`, the latest release (published 2026-11-03; `1.1.0` was 2026-10-06). Tag commit
  `ab46419fcc99bcd34aa4d89f7b545d5d51694a88`, 2026-10-30, on `branch-1.2`.
- Spark (`pom.xml`): default build 4.1.3 (Scala 2.13, Java 17). Profiles: `spark-3.4` 3.4.3,
  `spark-3.5` 3.5.9, `spark-4.0` 4.0.4, `spark-4.1` 4.1.3, `spark-4.2` 4.2.0.
- Arrow Java (`pom.xml` `arrow.version`): 18.3.0.
- DataFusion (`native/Cargo.toml`): 55.2.0 (`datafusion-spark` 55.2.0 too).
- Arrow Rust / parquet (`native/Cargo.toml`): 59.2.0. `object_store` 0.13.2.

## VM hours

## Rulings
