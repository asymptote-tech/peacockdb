# guard-checks — run detail

What a restarted coordinator needs. Spec: [`guard-checks.md`](guard-checks.md); plan:
[`guard-checks-impl.md`](guard-checks-impl.md).

## Standing facts

- Chain K, first task, base master. Branch `ENS-guard-checks`, forked at `8806a3c3`.
- No GPU on this chain (board header). No device build, no device run. `done` once every CI
  job but the GPU tests is green; the GPU jobs are not waited on.
- PR targets master, since this is the first task of the chain.
- Verification is local: rust-only `--lib`, and `ctest --test-dir cpp/build -L cpu` with
  `cpp/build` configured against cuDF 25.02
  (`scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`).

## Dispatch log

### 2026-10-08 — round 1, developer

verda unreachable at dispatch (`Temporary failure in name resolution`), so the developer was
told to run everything locally. Board moved `approved to build` → `building` in the same commit
as this file.
