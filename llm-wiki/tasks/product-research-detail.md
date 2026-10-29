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
- Task 3: complete — human kept the nine suggested options, and added four on 2026-10-29 (thirteen) on 2026-10-28 (see Checkpoint)
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

Decided by the human on 2026-10-28, from the rewritten `discovery.md`. Ranking rule unchanged
(plan Task 3): over 2× the yardstick on both bases is out; then demand strength; ties by adoption
friction, data path, funding story, platform risk.

**Kept — thirteen deep runs, slug of the option file first:**

1. `lean` — the human's hypothesis, given to its researcher as "a candidate product's premises".
2. `workstation-gpu-sql` — merged with `dataframe-gpu`.
3. `spark-plugin`.
4. `gpu-bi-dashboards` — merged with `realtime-vram-cache` and `gpu-resident-results`.
5. `other-accelerators`.
6. `trainer-side-sql-prep`.
7. `quant-gpu` — `asof-gpu` merged with `backtest-sweep-gpu`.
8. `datafusion-plugin` — merged with `datafusion-log-gpu`.
9. `lakehouse-iceberg` — merged with `gpu-scan-tier`.
10. `postgres-gpu-analytics` — added by the human on 2026-10-29.
11. `llm-corpus-sql` — added by the human on 2026-10-29.
12. `fuzzy-join-gpu` — added by the human on 2026-10-29.
13. `gpu-cpu-router` — added by the human on 2026-10-29.

(The helper's message said "ten"; the list it gave, and the human approved, has nine.) Each merged option's file answers the questions of every
option merged into it, named ones verbatim from the spec.

**Cut at the checkpoint, with the reason the report lists:**

- `clickhouse` (named): ClickHouse is building GPU execution itself, on Sirius (RFCs #116807,
  #124455, 11+ PRs).
- `realtime-other` (named): Doris has merged a Sirius backend; StarRocks has an experimental one.
- `distributed-pluggable` (named): the engines are adding GPU through Sirius or cuDF themselves.
- `streaming-flink` (named): no community evidence at all (0/0/0); Flink's GPU work is inference.
- `vector-sql` (named): thin evidence (2/0/1, no named poster).
- Discovered passes not kept: `flow-hunt-gpu`, `single-cell-gpu`, `in-db-gpu-inference`,
  `edge-jetson`, `geo-join-gpu`, `gpu-sql-migration-qualifier`, `ran-sim-datalake` — evidence of two named
  posters or fewer each, below the kept options.

## Resume notes (for a new session)

- Next is Task 4. Wave 1 is partly done: `sirius.md` and `nvidia.md` exist. `demand.md` is still
  to write; it now starts from the four digests in `artifacts/product-research/sources/` as its
  corpus and fills gaps (Discord unreachable; Reddit partial) rather than searching from zero.
- Then the option wave: the thirteen kept options above, at most five at a time, each given the paths
  of `sirius.md`, `nvidia.md`, `demand.md`, `discovery.md` and the digests.
- Ruling: researchers may go past the deep-research skill's ~15-call cap per researcher when a
  question needs it (it is the skill's own guidance, not a quota) — cost if wrong: time.
- The session scratchpad is per-session and will be gone. Re-clone Sirius at the SHA above into
  the new scratchpad, and recreate the Sheets venv only if a sheet must be re-read (the CSVs are
  committed). The paste-in context for prompts is the spec's Why, Budget and How to judge
  sections — never the hypothesis section.
- PullPush (Reddit) blocks agents after ~45 requests; Answer Overflow and linen.dev (Discord)
  return 429 bot checkpoints; the Stack Exchange quota is ~300/day per IP.

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
- sirius.md (2026-10-28): no own optimizer (DuckDB's; heuristics only); no automatic cross-query
  cache (explicit `pin_table`, never evicted; raw-bytes cache off by default); spilling GPU→pinned
  host→disk via cuCascade, young, unmeasured; Substrait in via library call, re-planned by DuckDB,
  DataFusion 45 output gives wrong answers (LIMIT→0, #1963); smallest unit a whole plan or a
  Fragment, no standalone libsirius. Fork: out (1,244 commits/12 mo, ~92% NVIDIA, no releases;
  ~7.8× peacockdb's size with cuCascade). Only route: pinned dependency plus upstream work.
- Task 2: Ruling (human request, 2026-10-28): redo discovery with community sources scraped —
  the first pass used deep-research, whose researchers stop at ~15 tool calls, and did not search
  Reddit, Stack Overflow, Discord or several engines' issue trackers. Four scrape subagents
  (GitHub + Apache JIRA; Reddit via PullPush; HN + Stack Exchange; Discord indexes + Discourse and
  vendor forums) write classified digests to `artifacts/product-research/sources/`, raw dumps
  stay in the scratchpad; then discovery.md is rewritten from them and the checkpoint is re-run.
  The digests double as `demand.md`'s corpus. Cost if wrong: a few hours of agent time.
- sources/hn-stackexchange.md: 133 relevant (89 HN, 44 SE): 49 complaint, 28 user, 27 vendor,
  14 request. Signals: big Spark shuffle/join cloud bills (crowded: spark-rapids, ParaQuery,
  Gluten, Comet); GPU idle on mixed job sizes, routing layer wanted; happy GPU users are
  single-node pandas/Polars, plus log/netflow security niches. SE quota left 236.
- sources/discord-forums.md: Discord/Slack unreadable (Answer Overflow and linen.dev 429 bot
  checkpoints; community.heavy.ai refuses). 27 Discourse + 3 Khoros forums: 61 relevant (6
  request, 8 user, 19 complaint, 21 vendor). Signals: Spark+RAPIDS on 2× DGX Spark nightly 380M
  rows (only clear success); new:ran-sim-datalake (Aerial Data Lake → CPU ClickHouse inserts
  stall); top complaint is RAPIDS install pain (20–40 GB images, Python lag, no Windows, ARM64).
- sources/issue-trackers.md: 100 relevant threads (32 complaint, 30 request, 18 vendor, 15 user);
  ~250 more listed in bulk families; JIRA 787 tickets, almost no GPU-SQL demand. Signals:
  incumbents adding GPU themselves, mostly via Sirius (ClickHouse RFC + 11 PRs; Doris backend;
  Sirius proposed as DuckDB core extension, PR 2026-10-28; MatrixOne, Paimon, PolarDB; Presto/Velox
  cuDF); first-person complaints say GPU loses to a good CPU engine on IO, transfer and silent
  CPU fallback; keep-data-on-GPU and BI-from-GPU demand explicit but thin; Microsoft Fabric GPU
  acceleration with warm cache in early access. One cited spark-rapids complaint is the human's
  own account (`alephonea`), marked, not a lead.
- sources/reddit.md: 110 relevant (15 request, 18 user, 26 complaint, 31 vendor); scrape cut
  short (PullPush blocked agents after ~45 requests; Arctic Shift flaky; several subreddits never
  completed) — counts indicative. Signals: u/fmoralesh 1.2 TB logs on cuDF/V100 + 11 TB S3
  Parquet (datafusion-log-gpu, gpu-scan-tier); single-machine setup pain (Windows, WSL2, old GPUs;
  "willing to pay" for Windows cuDF) → workstation-gpu-sql; new: quant backtest sweeps, single-cell
  genomics. No Reddit report of Spark RAPIDS in production.
- discovery.md rewritten (2026-10-28) from the four digests: 41 options, 30 pass (10 named, 20
  discovered), 11 fail (9 (a), 2+1 (c)). Strongest evidence (R/U/C · posters):
  workstation-gpu-sql 7/11/38·22, spark-plugin 4/7/20·13, dataframe-gpu 2/10/17·7,
  other-accelerators 12/3/2·5, gpu-bi-dashboards 4/5/3·5. Changed: storage-oem-engine,
  gov-airgapped, ab-metrics-gpu, idle-gpu-batch, telco-xdr-gpu → fail (a); lakehouse-iceberg →
  pass (weak); datafusion-plugin → pass. Borderline passes: ran-sim-datalake, gpu-cpu-router.
- demand.md (2026-10-29): strong — workstation-gpu-sql, spark-plugin; medium — lean,
  gpu-bi-dashboards, quant-gpu, llm-corpus-sql; weak — other-accelerators, trainer-side-sql-prep,
  datafusion-plugin, lakehouse-iceberg, postgres-gpu-analytics, clickhouse, realtime-other,
  distributed-pluggable; thin — fuzzy-join-gpu, gpu-cpu-router, streaming-flink, vector-sql. No
  user asked for the lean's "~100 GB in a few hundred ms". Leads: Graphistry (results on GPU, cudf
  #23039), Superset over 9.6B-row Delta via Spark Thrift + RAPIDS (cudf-spark #11064), UC Berkeley
  DuckDB data server GPU backend (mcp-data-server #227). Discord/Slack 429 again.
- options/lean.md: cache premise holds on NVLink boxes only with data already in VRAM, fails
  cold, partly on PCIe-only; plugin premise mostly holds but DuckDB already has Sirius
  `pin_table`; Voltron premise false (Theseus read Parquet in place, sold embedded via partners;
  causes: revenue shortfall vs ~120 staff, late launch). Cost 1.1× peacockdb, 0.3× Sirius.
  Ratings: need medium · cost medium · data path strong · friction medium · platform weak ·
  funding weak.
- options/lakehouse-iceberg.md: only as a GPU Iceberg engine with a three-tier cache (VRAM
  decoded, pinned RAM lightly compressed, NVMe encoded pages); drop the decode-tier-for-any-engine
  variant (3–5× bytes over Flight). Cold S3 NIC-bound; fix #232 and Snappy first (7–9 GB/s decode
  < 100 Gbps NIC). Sirius already has GPU Iceberg with deletes (2026-09-15); also cudf-spark,
  Presto-GPU, Starburst Torin, Bodo. Cost 0.6–1.1× peacockdb, 0.3–0.5× Sirius. Ratings: need weak
  · cost medium · data path weak · friction medium · platform weak · funding weak.
- options/gpu-bi-dashboards.md: GPU SQL server, hot tables in VRAM/pinned RAM, live appends,
  Flight SQL, in-process device API. Cost 0.9× peacockdb, 0.35× Sirius. Ratings: need medium ·
  cost medium · data path strong · friction weak · platform weak · funding weak. Encoded vs
  compressed: both ~2.5–3× capacity; nvCOMP/Parquet pages ~0.5–0.7 s per 80 GB touch, bit-packed/
  dictionary ~50–140 ms. TODO Task 5: correct demand.md on heavydb #819 (SIGSEGV from
  `select_ipc_gpu` returning 0 rows, not only a SIGSEGV report).
- notes/vram-cache-encoding-datapath.md: a late sub-researcher's notes from the gpu-bi-dashboards
  run (cache/ingestion patterns in HeavyDB, PG-Strom, Kinetica, Sirius, GQE; nvCOMP vs lightweight
  encodings; cuDF dictionary support; bandwidth table; GPU result hand-off), copied from the
  scratchpad because they answer the human's VRAM-cache question in depth. Cited from the report.
- options/spark-plugin.md: fits only at the top of the budget, read-only SQL alpha; best design
  a GPU engine behind Comet's plan serializer, native Parquet reader, cross-query decoded-column
  cache. Q1 reader issue NOT fixed in 26.10-rc (cudf-spark `49d3be94`; P0 epic #16073 for a native
  HybridScanReader + GDS, one PR merged). Q2: every headline saving is NVIDIA-published;
  independent attempts missed targets. Q3: no GPU decoded-column cache (compressed Parquet on JVM
  heap, re-decoded per query). Cost 1.6× peacockdb, 1.3× Sirius. Ratings: need strong · cost weak
  · data path medium · friction medium · platform weak · funding medium.
