# product-research — run state

Executed inline by a helper session on master (`superpowers:executing-plans`), plan
[`product-research-impl.md`](product-research-impl.md). This file is the run's ledger.

- Scratchpad: `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad`
  (scratch venv with the Sheets client: `venv/`; external clones: `ext/`)
- Sirius clone: `ext/sirius` (depth 200), HEAD `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (2026-10-28)

## Tasks

- Task 0: complete
- Task 1: complete — yardstick 22,830 lines (57,659 test); 8 sheet CSVs (7 + 1 tabs, as the API reports); patch saved
- Task 2: complete — 10 named slugs present; 16 discovered pass
- Task 3: todo
- Task 4: todo
- Task 5: todo
- Task 6: todo

## Rulings

- Ruling: run on master, not in an isolated worktree — the spec and the human's standing rule
  put helper work on master — cost if wrong: none, nothing outside `llm-wiki/` changes.
- Ruling: this file is the ledger, not `.superpowers/sdd/` — the spec names `-detail.md` as run
  state — cost if wrong: none.
- Ruling: TDD does not apply (desk research, no code); each task's Verify step is its gate, and
  Task 6's audit is the final review — cost if wrong: none.

- Task 2: Ruling: start `sirius.md` and `nvidia.md` now, alongside discovery, rather than
  after the checkpoint — neither depends on the option list, and it saves a wave — cost if wrong:
  none, they are needed whatever is kept. `demand.md` still waits for the checkpoint.

## Checkpoint

## Subagent returns
- nvidia.md (2026-10-28): NVIDIA sells no DB but gives away GPU SQL engines its staff build
  (HeavyDB bought 2025, open-sourced 2026-10; Sirius, 7 of top 9 committers NVIDIA; GPU Presto
  with IBM); spark-rapids sold via AI Enterprise. "Denies competing": no public source. GQE
  (rapidsai/gqe): DataFusion→Substrait→libcudf PoC. ClickHouse GPU RFCs #116807, #124455 (Sirius
  offload) — both verified open on GitHub by the helper. Risk strong: clickhouse,
  lakehouse-iceberg, realtime-vram-cache, realtime-other, spark-plugin, Sirius base; medium:
  datafusion-plugin, distributed-pluggable, vector-sql; weak: streaming-flink, other-accelerators.
- discovery.md (2026-10-28): 29 options, 26 pass (10 named, 9 of them `pass (weak)` mostly
  for NVIDIA/host-project overlap; 16 discovered). Failed: robot-log-gpu, trace-analytics-gpu (not
  GPU-centred), flight-sql-gpu-endpoint (no need). Most promising discovered: asof-gpu,
  datafusion-log-gpu, trainer-side-sql-prep. No named user found for any; web search ran out
  partway; some 2026 facts rest on one GTC attendee write-up.
