# duckdb-oracle — run detail

Working notes for this task. The spec is [`duckdb-oracle.md`](duckdb-oracle.md) (frozen); the
plan the developer works in is [`duckdb-oracle-impl.md`](duckdb-oracle-impl.md).

## Branch and PR

- Branch `ENS-duckdb-oracle`, forked off master at `38d5f2de` ("Chain J approved to build").
- First task of chain J, so its PR targets **master**.
- Workspace: `peacockdb-alpha` (`/home/dmitry/workspace/peacockdb-alpha`).

## Hosts, as probed 2026-10-08 before the first dispatch

- **verda: down.** `ssh verda` fails to resolve the hostname, so CPU tests run locally. Re-probe
  before each dispatch rather than trusting this line.
- **shad-gpu: down.** `ssh shad-gpu` (llm-gpu0h200.velkerr.ru:22) times out. So the device half of
  the verification bar cannot run yet: impl **Task 7 Step 3** (the `PCK_WRITE_GPU_RESULT=1` cycle
  and `--pull-results`) and with it the committed `gpu-result.txt` files are deferred, and so is
  Task 7 Step 4's commit of them. Everything else in the plan is rust-only and local.

## Round 1 dispatch (2026-10-08)

Scope: impl Tasks 1–6, Task 7 Steps 1, 1b, 2 and 3b, and Tasks 7b, 7c, 7d, 8 — the whole
rust-only verification bar. The device cycle is held back for a shad-gpu that answers.

Consequence to carry: with no device cycle, `gpu-result.txt` does not exist, so Task 7 Step 1b's
coverage guard (`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`) has no file to
read and the `duckdb_gpu_<dataset>_<query>_<mode>` cases have no sections. **Those guards are
written in their honest form and left red** — an absent file reads as "not regenerated", which is a
real gap, and a guard that passes over a missing file is a guard that cannot go red. They are the
one permitted red in round 1, and they go green in the same step that writes the file. The
developer reports them by name and does not weaken them to get a green suite.
