# User demand for a GPU-centred peacockdb product

## Method

- Corpus: the four digests in `sources/` (issue trackers, Reddit, HN + Stack Exchange, forums), `discovery.md`, `nvidia.md`, `sirius.md` and `evidence-in-hand.md`. Together that is 404 judged hits, 2023-10 to 2026-10.
- Gap filling on 2026-10-29: about 60 GitHub API calls, 20 HN Algolia calls, 12 Arctic Shift calls and 6 web searches. Stack Exchange was not queried again; its quota is untouched.
- Verified at the source: 23 GitHub issues, 5 cudf-spark discussions (GraphQL), 6 HN comments and 7 Reddit posts. These are the threads the options lean on most. Corrections:
  - sirius #1760 was opened by Sirius contributor `ran-yuan-rui`. Aaron Spiegel (Qbeast) and Chen Junjie (Tencent) only commented on it.
  - heavydb #819 is a SIGSEGV from `select_ipc_gpu`, the GPU-memory result hand-off, which returns 0 rows; reported by `anirudh-here-com`. The "GPU shared memory is required" quote is in a comment (corrected 2026-10-29 from the gpu-bi-dashboards research).
  - Boettiger's 23.5 s (GPU) vs 5.2 s (CPU) result is not like-for-like. The runs were months apart, on a data model that had drifted. His own record says so.
  - The Fabric GPU claim rested on one third-party repo issue. It now has primary sources: a Microsoft blog and the CoddSpeed paper.
- New searches covered: lean/cache asks, HeavyDB outside users, Sirius outside users, ClickHouse GPU issues, NeMo Curator dedup users, KX/kdb GPU, Splink and record linkage, PG-Strom, the DataFusion GPU projects, and per-query GPU/CPU routing.
- Still unreachable:
  - **Discord and Slack.** Answer Overflow and linen.dev returned 429 again on 2026-10-29 (one try each). DuckDB, Polars, DataFusion, RAPIDS, Sirius and Iceberg chat is still missing.
  - **Reddit comment search.** Arctic Shift returned 422 on every comment search and on half the post searches. PullPush was not retried, since it blocks agents.
  - **Vendor forums.** The HeavyAI, Kinetica and SQream user forums are gone, so there are no stories from their paying users.
- Counts are hand tallies from the digests, so treat them as a ranking only. "Posters" means distinct named first-person handles. It is a lower bound.

## Who already accelerates, and how

- **Single box with Python dataframes.** This is the largest happy group: about 17 posters across sources.
  - What they run: cuDF, cudf.pandas, cudf-polars or dask_cudf on a consumer or desk GPU.
  - xs83 converted 500 GB on a 6 GB GTX 1060 faster than a 20-node cluster ([HN](https://news.ycombinator.com/item?id=40560169)).
  - A policy analyst got 70× on an A5000 laptop ([r/mac](https://www.reddit.com/r/mac/comments/1hdxh51/)).
  - tecedu's team uses GPU boxes with "CPUs just being orchestrators" ([r/gadgets](https://www.reddit.com/r/gadgets/comments/1gywo4t/comment/lyyif4w/)).
- **Spark RAPIDS.** About 13 posters, mostly trying it out rather than running it.
  - Confirmed in production: ByteDance on Kubernetes with L20s ([#16321](https://github.com/NVIDIA/cudf-spark/issues/16321)), Jeff Levesque's cached-frame jobs ([#16460](https://github.com/NVIDIA/cudf-spark/issues/16460)), and his nightly 380M-row batch on 2× DGX Spark ([forum](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791)).
  - One BI-serving setup: Yu-Ting Lin runs Superset → Spark Thrift Server → RAPIDS over a 9.6B-row Delta table ([#11064](https://github.com/NVIDIA/cudf-spark/issues/11064)).
- **GPU databases.** HeavyDB still has a few production users:
  - anirudh-here-com depends on GPU-memory result hand-off and carries custom patches ([#819](https://github.com/heavyai/heavydb/issues/819));
  - a free-edition user hit the 3-session cap ([#836](https://github.com/heavyai/heavydb/issues/836)).
  - Only about 10 outside users filed HeavyDB issues after 2024, besides one fuzzer.
  - A core-banking team tests PG-Strom GPU-Cache ([pg-strom #796](https://github.com/heterodb/pg-strom/issues/796)).
- **Hand-written CUDA in niche shops.**
  - Alex Winters (Penn State) ported record linkage to cuDF plus a Numba Jaro-Winkler kernel. A 1M×1M job went from about 14 h on a CPU cluster to about 4 min on an RTX 4070 ([r/CUDA](https://www.reddit.com/r/CUDA/comments/1uy5az7/); [crosswalk](https://github.com/ajwinters/crosswalk)).
  - Quants run cuDF/CuPy grid searches: "from 1/sec to 100k in 13 seconds" ([r/quant](https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/)).
- **Graph and log analytics.**
  - Graphistry runs Cypher/GFQL as cuDF joins. Louie Meyerov says a customer's log-analytics setup ran SSD arrays → GDS → 8×A100 ([HN](https://news.ycombinator.com/item?id=47109102); [cudf #23039](https://github.com/NVIDIA/cudf/issues/23039)).
  - fmoralesh processes 1.2 TB of logs with cuDF on a V100 ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/)).
- **LLM data curation.** People run NeMo Curator fuzzy dedup on 5–8 GPU nodes. Five outside users report OOMs and crashes ([#471](https://github.com/NVIDIA-NeMo/Curator/issues/471), [#1228](https://github.com/NVIDIA-NeMo/Curator/issues/1228)).
- **What worked:**
  - one node;
  - data that fits in VRAM or streams in chunks;
  - heavy compute per byte (string similarity, grid sweeps, graph joins);
  - Python APIs, not SQL.
- **What did not work:** remote storage reads, small queries, and anything where the CPU engine was DuckDB.

## What people ask for

Request counts are summed across the four digests, plus the additions found on 2026-10-29.

| use case | requests | typical ask | example |
|---|---|---|---|
| Non-NVIDIA backends (AMD, Apple) | 12 | MI300 for memory; Metal aggregates in DuckDB | [sirius #53](https://github.com/sirius-db/sirius/issues/53) |
| Easy install: Windows, small images, a single binary | 7 | "willing to pay someone to port over cudf to work with windows" | [r/Python](https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/) |
| Keep data or results on the GPU between steps | 5 | cache inputs across queries; keep `collect()` results on the GPU | [#13435](https://github.com/NVIDIA/cudf-spark/discussions/13435), [cudf #23039](https://github.com/NVIDIA/cudf/issues/23039) |
| Spark speed and cost | 4 | joins of 150B+ rows; Glue is "cost prohibitive" | [HN](https://news.ycombinator.com/item?id=43966388) |
| Columnar file → GPU tensors for training | 4 | CSV or Parquet straight to PyTorch/XGBoost | [HN](https://news.ycombinator.com/item?id=45988948) |
| Time series: as-of joins, rolling windows, sweeps | 5 | `join_asof` in cudf-polars; G-Research rolling windows | [cudf #20727](https://github.com/NVIDIA/cudf/issues/20727) |
| Table formats (Iceberg/Paimon) on GPU | 3 | file skipping on append-only Iceberg | [sirius #1760](https://github.com/sirius-db/sirius/issues/1760) |
| GPU backend for a DataFusion app | 1 user, 4 builders | Beacon wants libcudf-rs | [beacon #231](https://github.com/maris-development/beacon/issues/231) |
| Open multi-node GPU DB | 2 | 5×4 L20 servers, no paid cluster licence | [heavydb #842](https://github.com/heavyai/heavydb/issues/842) |
| String-similarity joins | 2 | GPU rapidfuzz | [NVIDIA forum](https://forums.developer.nvidia.com/t/string-similarity-package-in-cudf-for-fuzzy-merge/308933) |
| Route each job to GPU or CPU | 1 | "a scheduling layer that estimates the size of the job" | [HN](https://news.ycombinator.com/item?id=46480338) |
| Postgres → GPU | 3 | SQL Server/Postgres tables straight into VRAM | [SO](https://stackoverflow.com/q/77960181) |

- Nobody, in any source, asks for a named latency target such as "100 GB in a few hundred ms". The closest asks are Lin's Superset serving and andygongyb's cross-query cache.

## What goes wrong

Counts are distinct first-person threads, hand-tallied (estimate).

- **Install and packaging, about 33 threads.** This is the biggest group.
  - Linux-only cuDF and WSL2 failures.
  - Python 3.11/3.12 lag ([PyTorch forum](https://discuss.pytorch.org/t/cudf-for-python-3-11/189225)).
  - 20–40 GB images ([forum](https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463), [Jupyter](https://discourse.jupyter.org/t/minimal-image-compatible-with-nvidia-rapids/34116)).
  - ARM confusion on DGX Spark ([forum](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629)).
  - An air-gapped Databricks cluster that cannot fetch the jar ([Databricks](https://community.databricks.com/t5/data-governance/cannot-set-spark-plugins-com-nvidia-spark-sqlplugin-config/td-p/125570)).
- **IO, transfer and orchestration beat the GPU gain, about 10 threads.**
  - Sirius was slower than DuckDB on all 22 TPC-H queries on GB10, RTX 5090 and A6000 ([#609](https://github.com/sirius-db/sirius/issues/609)).
  - DataFusion+cuDF is "bottlenecked by … host/device transfer" ([libcudf-rs #49](https://github.com/gabotechs/libcudf-rs/issues/49)).
  - HDFS held 8×H100 at 15% ([#11431](https://github.com/NVIDIA/cudf-spark/discussions/11431)).
  - Databend is "IO (S3)-bound" ([#15606](https://github.com/databendlabs/databend/issues/15606)).
  - A small Polars join took 0.1 s on CPU and 7 s on GPU ([polars #20696](https://github.com/pola-rs/polars/issues/20696)).
  - The human's own PoC spent more time loading than computing on 11 of 12 SF20 queries ([evidence](evidence-in-hand.md)).
- **VRAM ceilings and OOM, about 10 threads.**
  - "Limited by VRAM is a huge constraint" ([HN](https://news.ycombinator.com/item?id=40559986)).
  - A GB10 hangs on OOM ([forum](https://forums.developer.nvidia.com/t/device-hangs-under-load/375016)).
  - NeMo dedup OOMs on 5×A100 ([#471](https://github.com/NVIDIA-NeMo/Curator/issues/471)).
  - Spark joins kill executors ([Databricks](https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862)).
- **Silent CPU fallback, 5 threads.** Users measured the CPU without knowing it ([sirius #2174](https://github.com/sirius-db/sirius/issues/2174); [mcp-data-server #227](https://github.com/boettiger-lab/mcp-data-server/issues/227); [cudf #14500](https://github.com/NVIDIA/cudf/issues/14500); [polars #28842](https://github.com/pola-rs/polars/issues/28842)).
- **Cost and utilisation, about 7 threads.**
  - A missed 50% cost-cut target ([#11992](https://github.com/NVIDIA/cudf-spark/discussions/11992)).
  - A paid GPU idles on mixed job sizes ([HN](https://news.ycombinator.com/item?id=46480338)).
  - Pricing tricks in benchmarks ([HN](https://news.ycombinator.com/item?id=46484962)).
  - "Kinetica … if you can afford the price" ([HN](https://news.ycombinator.com/item?id=44340460)).
  - Workloads "too infrequent or … not big enough" ([r/databasedevelopment](https://www.reddit.com/r/databasedevelopment/comments/1vafuoh/comment/p128rmi/)).
- **Operational breakage, about 8 threads.**
  - A kernel update made RAPIDS 1.5–2.4× slower.
  - Thrift Server multi-session bugs ([#9966](https://github.com/NVIDIA/cudf-spark/discussions/9966)).
  - Sirius pins go stale after UPDATE ([sirius #106](https://github.com/sirius-db/sirius/issues/106)).
  - PG-Strom crash reports, and a HeavyDB SIGSEGV.
  - NeMo Curator examples fail out of the box ([#1292](https://github.com/NVIDIA-NeMo/Curator/issues/1292)).

## Ratings

Rules: **strong** needs at least three independent first-person threads with a painful need. **Thin** means fewer than three first-person posters. Posters are distinct named first-person handles.

| slug | rating | evidence (counts · posters · best links) | reasoning |
|---|---|---|---|
| lean | medium | About 6 posters want hot data or results kept on the GPU or in RAM: andygongyb ([#13435](https://github.com/NVIDIA/cudf-spark/discussions/13435), 2025-09), Yu-Ting Lin ([#11064](https://github.com/NVIDIA/cudf-spark/issues/11064), 2024-06), anirudh-here-com ([heavydb #819](https://github.com/heavyai/heavydb/issues/819), 2023-12), jieguolove, trungdoanle1101 and IOhacker. None states a 100 GB / sub-second target. | The cache thesis matches the human's own SF20 data, but users ask for it softly. Sirius `pin_table`, PG-Strom GPU-Cache, Fabric's warm cache and KDB-X already offer it inside host databases. |
| workstation-gpu-sql (+dataframe-gpu) | strong | R/U/C 9/21/55 · about 29 posters. tecedu "willing to pay" ([r/Python](https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/), 2024-09); 20 GB images ([forum](https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463), 2024-06); Sirius slower on desk GPUs ([#609](https://github.com/sirius-db/sirius/issues/609), 2026-04) | The need is real and loud. But it is for dataframe APIs and easy installs, not SQL, and Sirius is entering DuckDB core ([#28022](https://github.com/duckdb/duckdb/pull/28022), 2026-10-28). |
| spark-plugin | strong | R/U/C 4/7/20 · 13 posters. dogman123 ([HN](https://news.ycombinator.com/item?id=43966388), 2025-05); MaxNevermind ([#11992](https://github.com/NVIDIA/cudf-spark/discussions/11992), 2025-01); Databricks join OOM ([forum](https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862), 2025-06) | The most painful and best-funded need, but NVIDIA sells this exact product. Spark compatibility is the bulk of the work. |
| gpu-bi-dashboards (+realtime-vram-cache, gpu-resident-results) | medium | R/U/C 7/11/5 · about 9 posters. Lin's Superset over 9.6B rows ([#11064](https://github.com/NVIDIA/cudf-spark/issues/11064), 2024-06); Graphistry results on the GPU ([cudf #23039](https://github.com/NVIDIA/cudf/issues/23039), 2026-06); the 5×4 L20 owner ([heavydb #842](https://github.com/heavyai/heavydb/issues/842), 2024-12) | Three or more first-person threads exist, but urgency is unclear. Fabric ships GPU query acceleration ([Microsoft](https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701), 2026-06), and HeavyDB is now free. |
| other-accelerators | weak | R/U/C 12/3/2 · 5 posters. Hot Aisle ([sirius #53](https://github.com/sirius-db/sirius/issues/53), 2025-06); Coco Alemana on Metal ([r/DuckDB](https://www.reddit.com/r/DuckDB/comments/1qdrwxi/), 2026-01); DuckDB beat MLX on M4 ([HN](https://news.ycombinator.com/item?id=47059629), 2026-02) | Many asks, but no workload or budget attached. On Apple, the CPU engine wins. |
| trainer-side-sql-prep | weak | R/U/C 4/4/0 · 5 posters. Embeddings to cuML ([cudf #19497](https://github.com/NVIDIA/cudf/issues/19497), 2025-07); columnar file to XGBoost ([HN](https://news.ycombinator.com/item?id=45988948), 2025-11); CSV to PyTorch ([forum](https://discuss.pytorch.org/t/cudf-for-python-3-11/189225), 2023-10) | Small convenience asks. Nobody says SQL is what they lack. |
| quant-gpu (asof-gpu + backtest-sweep-gpu) | medium | R/U/C 5/4/0 · 9 posters. RoundTableMaker ([r/quant](https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/), 2026-04); estimated1 ([r/algotrading](https://www.reddit.com/r/algotrading/comments/1ctidxy/), 2024-05); G-Research rolling windows ([cudf #20307](https://github.com/NVIDIA/cudf/issues/20307), 2025-10) | Users exist, but they already solve it with cuDF/CuPy. KX ships a GPU as-of join (`.gpu.aj`) ([KX](https://kx.com/blog/introducing-kdb-x-gpu-acceleration/)), and NVIDIA targets the KX TAQ benchmark ([cudf #24634](https://github.com/NVIDIA/cudf/issues/24634), 2026-10). |
| datafusion-plugin (+datafusion-log-gpu) | weak | 1 user ask, 4 builders. Beacon ([#231](https://github.com/maris-development/beacon/issues/231), 2026-06, 0 comments); Datadog's profiling ([libcudf-rs #49](https://github.com/gabotechs/libcudf-rs/issues/49), 2026-05); Algeon ([datafusion #26633](https://github.com/apache/datafusion/issues/26633), 2026-10). The log demand (fmoralesh, Graphistry) does not involve DataFusion. | Peers, not buyers. Algeon, datafusion-arrowmetal and GQE fill the slot already. |
| lakehouse-iceberg (+gpu-scan-tier) | weak | R/U/C 3/2/5 · 5 posters. Qbeast/Tencent comments ([sirius #1760](https://github.com/sirius-db/sirius/issues/1760), 2026-09); fmoralesh's 11 TB nested JSON ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1la1lcw/), 2025-06); HDFS-bound H100s ([#11431](https://github.com/NVIDIA/cudf-spark/discussions/11431), 2024-09) | Remote reads cap the GPU, and the askers are vendors. Sirius, Paimon and cudf-spark are building it themselves. |
| postgres-gpu-analytics | weak | R/U/C 3/1/4 · 3 posters. Fineract on PG-Strom ([#796](https://github.com/heterodb/pg-strom/issues/796), 2024-06); SQL Server → VRAM ([SO](https://stackoverflow.com/q/77960181), 2024-02); Xavier edge ([SO](https://stackoverflow.com/q/77300203), 2023-10) | The askers are curious rather than in pain. The 2025–26 PG-Strom tracker is mostly its maintainers (estimate). |
| llm-corpus-sql | medium | About 8 posters hit GPU-curation pain: NeMo users chenrui17 ([#471](https://github.com/NVIDIA-NeMo/Curator/issues/471), 2025-01) and jmcmanus15 ([#1228](https://github.com/NVIDIA-NeMo/Curator/issues/1228), 2025-11); pham_hung2 ([forum](https://forums.developer.nvidia.com/t/cudf-spill-on-demand-couldnt-find-memory-device-to-spill-spillmanager-device-memory-limit-spilled-unspilled/332484), 2025-05); Fabulous_Pollution10 ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1o0kyd0/), 2025-10) | The pain is real, but it lives inside NVIDIA's free tool. Several builders choose CPU dedup tools instead, and no one asks for SQL. |
| fuzzy-join-gpu | thin | 2 posters. Alex Winters ([r/CUDA](https://www.reddit.com/r/CUDA/comments/1uy5az7/), 2026-07); a cudf.pandas rapidfuzz ask ([forum](https://forums.developer.nvidia.com/t/string-similarity-package-in-cudf-for-fuzzy-merge/308933), 2024-10). Fast-ER already exists ([docs](https://fast-er.readthedocs.io)). | Strong per-user gain (14 h → 4 min), but the users wrote it themselves. Splink has zero GPU issues. |
| grace-superchip-analytics | weak | Added after the checkpoint (2026-10-29). No first-person post asks for SQL on Grace Hopper or Grace Blackwell in any digest. The nearest evidence is the lean and gpu-bi-dashboards rows (hot data in RAM or on the GPU) and one nightly Spark + RAPIDS batch on two DGX Spark units (NVIDIA forum, jeff1evesque; see `sources/discord-forums.md`). | A hardware target that strengthens other options, not a demand of its own; see `options/grace-superchip-analytics.md`. Thin on direct evidence. |
| gpu-cpu-router | thin | 1 poster: necubi ([HN](https://news.ycombinator.com/item?id=46480338), 2026-01). Presto and Velox add a per-query GPU switch themselves ([presto #28678](https://github.com/prestodb/presto/pull/28678), 2026-10). | It answers the top objection, but only one person asks for it, and engines build it in. |
| clickhouse | weak | 0 user asks. RFC by staff, with 8 reactions, all from members ([#116807](https://github.com/ClickHouse/ClickHouse/issues/116807), 2026-08); CTO skeptical ([HN](https://news.ycombinator.com/item?id=46482420), 2026-01) | The vendor is filling the slot. Users are silent. |
| realtime-other (Doris/StarRocks/VeloDB) | weak | 0 user asks; Kinetica price complaint only. Doris PMC builds a Sirius backend ([sirius #2025](https://github.com/sirius-db/sirius/issues/2025), 2026-10) | Vendor-driven. Zero StarRocks GPU issues. |
| distributed-pluggable (Databend) | weak | 2 posters: No_Mongoose6172 ([r/dataengineering](https://www.reddit.com/r/dataengineering/comments/1o6bnir/), 2025-10) and jieguolove. Databend declined GPU ([#15606](https://github.com/databendlabs/databend/issues/15606), 2024-05) | Sirius is going multi-host itself ([sirius #2372](https://github.com/sirius-db/sirius/issues/2372), 2026-10). |
| streaming-flink | thin | 0 first-person hits in all sources; Flink GPU work is inference ([FLIP-577](https://cwiki.apache.org/confluence/spaces/FLINK/pages/421957275/FLIP-577+AI-Native+Flink+%E2%80%94+An+Umbrella+Proposal+for+Multimodal+Data+Processing)) | No demand found. |
| vector-sql | thin | 0 named users. Lance and Milvus GPU work is vendor work; cuVS is already everywhere ([cuVS](https://github.com/NVIDIA/cuvs/blob/main/fern/pages/integrations.md)) | GPU vector search is a checkbox feature. |

## First conversations

Listed in order of how well the need fits a GPU SQL product.

1. **Louie Meyerov, Graphistry** (`lmeyerov`).
   - Need: keep cudf-polars results on the GPU for GFQL/Cypher.
   - His customers run log analytics on 8×A100, and Graphistry is a revenue business.
   - Fits: gpu-bi-dashboards and lean.
   - [cudf #23039](https://github.com/NVIDIA/cudf/issues/23039), [HN](https://news.ycombinator.com/item?id=47109102).
2. **Yu-Ting Lin** (`LIN-Yu-Ting`).
   - Need: Superset over a 9.6B-row Delta table through Spark Thrift + RAPIDS. Blocked by DISTINCT OOM and session bugs.
   - This is the closest match to lean's shape.
   - Fits: lean and gpu-bi-dashboards.
   - [#11064](https://github.com/NVIDIA/cudf-spark/issues/11064), [#9966](https://github.com/NVIDIA/cudf-spark/discussions/9966).
3. **Carl Boettiger, UC Berkeley** (`cboettig`).
   - Need: H3 joins over Parquet on MinIO, with an optional GPU backend planned. He keeps a public GPU-vs-DuckDB record.
   - He also runs GPU JupyterHub images for researchers.
   - Fits: workstation-gpu-sql and lakehouse.
   - [#227](https://github.com/boettiger-lab/mcp-data-server/issues/227).
4. **Andy Gong** (`andygongyb`).
   - Need: cache inputs in GPU memory across consecutive queries. Spark said no.
   - Fits: lean.
   - [#13435](https://github.com/NVIDIA/cudf-spark/discussions/13435).
5. **`jieguolove`.**
   - Owns 5 servers × 4 L20 and wants an open GPU cluster DB.
   - Fits: lean and gpu-bi-dashboards.
   - [heavydb #842](https://github.com/heavyai/heavydb/issues/842).
6. **`anirudh-here-com`.**
   - Production HeavyDB 6.4 with custom patches. Needs GPU-memory result hand-off.
   - Fits: lean and gpu-resident-results.
   - [heavydb #819](https://github.com/heavyai/heavydb/issues/819).
7. **Alex Winters, Penn State** (`ajwinters`, u/axwinters).
   - Record linkage, with 20B candidate pairs per 1M×1M job.
   - Fits: fuzzy-join-gpu.
   - [r/CUDA](https://www.reddit.com/r/CUDA/comments/1uy5az7/).
8. **u/fmoralesh.**
   - 1.2 TB of logs and 11 TB of S3 Parquet with nested JSON. Already uses a V100.
   - Fits: workstation-gpu-sql and gpu-scan-tier.
   - [post](https://www.reddit.com/r/dataengineering/comments/1la1lcw/).
9. **Maksim Konstantinov** (`MaxNevermind`).
   - An ETL cost benchmark on AWS that missed its 50% cost-cut target. He knows his costs.
   - Fits: spark-plugin and gpu-cpu-router.
   - [#11992](https://github.com/NVIDIA/cudf-spark/discussions/11992).
10. **dogman123 (HN).**
    - EMR joins of 150B+ rows; Glue is too costly.
    - Fits: spark-plugin.
    - [HN](https://news.ycombinator.com/item?id=43966388).
11. **Aaron Spiegel, Field CTO at Qbeast** (`spiegela`).
    - Building TPC-H Iceberg tables on S3 to test GPU pruning.
    - Fits: lakehouse-iceberg.
    - [sirius #1760](https://github.com/sirius-db/sirius/issues/1760).
12. **Jeff Levesque** (`jeff1evesque`).
    - Spark + RAPIDS on 2× DGX Spark, nightly 380M rows.
    - Fits: workstation-gpu-sql and spark-plugin.
    - [forum](https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791).
13. **u/RoundTableMaker, u/EdwardM290, u/estimated1.**
    - Backtest sweeps.
    - Fits: quant-gpu.
    - [r/quant](https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/).
14. **NeMo Curator users `chenrui17`, `jmcmanus15`, and `pham_hung2`.**
    - GPU dedup at scale fails with OOMs and crashes.
    - Fits: llm-corpus-sql.
    - [#471](https://github.com/NVIDIA-NeMo/Curator/issues/471).
15. **Robin Kooyman, MARIS Beacon** (`robinskil`).
    - A climate data server that wants an optional GPU backend.
    - Fits: datafusion-plugin.
    - [beacon #231](https://github.com/maris-development/beacon/issues/231).
16. **Peers to ask, not to sell to:**
    - Gene Bordegaray (Datadog) and gabotechs, building libcudf-rs;
    - Morris (`morristai`), building Algeon;
    - Ignacio Van Droogenbroeck (Basekick Arc), who wants GPU but runs a CPU-only fleet ([arc #260](https://github.com/Basekick-Labs/arc/issues/260)).
- **User groups:**
  - DGX Spark owners on the NVIDIA forum;
  - r/algotrading and r/quant;
  - r/bioinformatics single-cell users;
  - Splink users (Robin Linacre, `RobinL`).
  - The RAPIDS Slack and the DuckDB/Polars Discord are unreachable from here; join them by hand.

## Evidence against

- **The numbers are small.** About 60 named first-person posters across 404 judged hits in three years. No poster has a budget and a deadline. Most first-person posts are evaluations.
- **GPU loses to a tuned CPU at the sizes people have:**
  - Sirius was slower than DuckDB on desk GPUs (#609).
  - Boettiger's GPU path lost to DuckDB by about 4×.
  - DuckDB beat MLX on every TPC-H query.
  - Under the human's scaling, Voltron was about 1.3× one CPU box (estimate, [evidence](evidence-in-hand.md)).
- **CPU engines keep improving.** DuckDB 2.0 now saturates 128–192 vCPUs ([HN](https://news.ycombinator.com/item?id=50181617)).
- **Moving the data costs more than the compute.** The PoC spent more time loading than computing on 11 of 12 queries. Every remote-storage case is IO-bound.
- **Insiders say the niche is narrow.** ClickHouse's CTO: "a small subset of production workloads in a narrow combination of conditions." After Voltron's collapse, r/databasedevelopment calls coprocessor analytics uneconomic.
- **History.** Fortune 500 users of BlazingSQL did not pay ([HN](https://news.ycombinator.com/item?id=40079813)). A GPU-DB startup dropped GPU to make things "simply work" ([HN](https://news.ycombinator.com/item?id=39872559)).
- **The free supply is large.** NVIDIA gives away Sirius, HeavyDB, GQE, cudf-polars and NeMo Curator. Fabric, KX, ClickHouse and Doris add GPU in-house. Where users do feel pain, a free vendor tool usually exists already.
- **The happiest users do not want SQL.** They use pandas/Polars or hand-written CUDA.

## Sources

Corpus links (accessed 2026-10-28) unless marked 2026-10-29.

- [sources/issue-trackers.md](sources/issue-trackers.md), [sources/reddit.md](sources/reddit.md), [sources/hn-stackexchange.md](sources/hn-stackexchange.md), [sources/discord-forums.md](sources/discord-forums.md), [discovery.md](discovery.md), [nvidia.md](nvidia.md), [sirius.md](sirius.md), [evidence-in-hand.md](evidence-in-hand.md) (accessed 2026-10-28)
- https://github.com/NVIDIA/cudf-spark/issues/11064, https://github.com/NVIDIA/cudf-spark/discussions/9966, https://github.com/NVIDIA/cudf-spark/discussions/13435, https://github.com/NVIDIA/cudf-spark/discussions/11992, https://github.com/NVIDIA/cudf-spark/discussions/11431 (accessed 2026-10-29)
- https://github.com/NVIDIA/cudf-spark/issues/16321, https://github.com/NVIDIA/cudf-spark/issues/16460 (accessed 2026-10-28)
- https://github.com/sirius-db/sirius/issues/609, https://github.com/sirius-db/sirius/issues/2174, https://github.com/sirius-db/sirius/issues/1760, https://github.com/sirius-db/sirius/issues/106, https://github.com/sirius-db/sirius/issues/53, https://github.com/sirius-db/sirius/issues/2025, https://github.com/sirius-db/sirius/issues/2372 (accessed 2026-10-29)
- https://github.com/duckdb/duckdb/pull/28022 (accessed 2026-10-29)
- https://github.com/gabotechs/libcudf-rs/issues/49 (accessed 2026-10-29)
- https://github.com/maris-development/beacon/issues/231 (accessed 2026-10-29)
- https://github.com/apache/datafusion/issues/26633, https://github.com/apache/datafusion/pull/26004 (accessed 2026-10-29)
- https://github.com/algeon-dev/algeon (accessed 2026-10-29)
- https://github.com/rapidsai/gqe (accessed 2026-10-29)
- https://github.com/NVIDIA/cudf/issues/23039, https://github.com/NVIDIA/cudf/issues/19497, https://github.com/NVIDIA/cudf/issues/20727, https://github.com/NVIDIA/cudf/issues/20307, https://github.com/NVIDIA/cudf/issues/24634 (accessed 2026-10-29)
- https://github.com/NVIDIA/cudf/issues/14500 (accessed 2026-10-28)
- https://github.com/pola-rs/polars/issues/20696, https://github.com/pola-rs/polars/issues/28842 (accessed 2026-10-28)
- https://github.com/heavyai/heavydb/issues/819, https://github.com/heavyai/heavydb/issues/842, https://github.com/heavyai/heavydb/issues/836 (accessed 2026-10-29)
- https://github.com/heterodb/pg-strom/issues/796 (accessed 2026-10-29)
- https://github.com/ClickHouse/ClickHouse/issues/116807 (accessed 2026-10-29)
- https://github.com/databendlabs/databend/issues/15606 (accessed 2026-10-28)
- https://github.com/Basekick-Labs/arc/issues/260 (accessed 2026-10-29)
- https://github.com/boettiger-lab/mcp-data-server/issues/227, https://github.com/boettiger-lab/mcp-data-server/blob/main/benchmarks/gpu-vs-cpu.md (accessed 2026-10-29)
- https://github.com/prestodb/presto/pull/28678 (accessed 2026-10-29)
- https://github.com/NVIDIA-NeMo/Curator/issues/471, https://github.com/NVIDIA-NeMo/Curator/issues/1228, https://github.com/NVIDIA-NeMo/Curator/issues/1292 (accessed 2026-10-29)
- https://github.com/ajwinters/crosswalk (accessed 2026-10-29)
- https://github.com/NVIDIA/cuvs/blob/main/fern/pages/integrations.md (accessed 2026-10-28)
- https://cwiki.apache.org/confluence/spaces/FLINK/pages/421957275/FLIP-577+AI-Native+Flink+%E2%80%94+An+Umbrella+Proposal+for+Multimodal+Data+Processing (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=43966388 (accessed 2026-10-29)
- https://news.ycombinator.com/item?id=46480338 (accessed 2026-10-29)
- https://news.ycombinator.com/item?id=47109102 (accessed 2026-10-29)
- https://news.ycombinator.com/item?id=46482420 (accessed 2026-10-29)
- https://news.ycombinator.com/item?id=40560169 (accessed 2026-10-29)
- https://news.ycombinator.com/item?id=40559986 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=45988948 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=47059629 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=46484962 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=44340460 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=40079813 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=39872559 (accessed 2026-10-28)
- https://news.ycombinator.com/item?id=50181617 (accessed 2026-10-29)
- https://stackoverflow.com/q/77960181 (accessed 2026-10-28)
- https://stackoverflow.com/q/77300203 (accessed 2026-10-28)
- https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/ (accessed 2026-10-28)
- https://www.reddit.com/r/gadgets/comments/1gywo4t/comment/lyyif4w/ (accessed 2026-10-28)
- https://www.reddit.com/r/mac/comments/1hdxh51/ (accessed 2026-10-28)
- https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/ (accessed 2026-10-28)
- https://www.reddit.com/r/dataengineering/comments/1la1lcw/ (accessed 2026-10-29)
- https://www.reddit.com/r/dataengineering/comments/1o0kyd0/ (accessed 2026-10-29)
- https://www.reddit.com/r/dataengineering/comments/1o6bnir/ (accessed 2026-10-29)
- https://www.reddit.com/r/CUDA/comments/1uy5az7/ (accessed 2026-10-29)
- https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/ (accessed 2026-10-28)
- https://www.reddit.com/r/algotrading/comments/1ctidxy/ (accessed 2026-10-28)
- https://www.reddit.com/r/DuckDB/comments/1qdrwxi/ (accessed 2026-10-28)
- https://www.reddit.com/r/databasedevelopment/comments/1vafuoh/comment/p128rmi/ (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/kernel-7-0-0-1019-nvidia-makes-apache-spark-rapids-gpu-work-1-5-2-4x-slower-on-dgx-spark-6-17-0-1032-with-the-same-driver-580-178-04-is-fine/386791 (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/can-the-rapids-image-go-on-a-diet-20gb-image/297463 (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629 (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/device-hangs-under-load/375016 (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/string-similarity-package-in-cudf-for-fuzzy-merge/308933 (accessed 2026-10-28)
- https://forums.developer.nvidia.com/t/cudf-spill-on-demand-couldnt-find-memory-device-to-spill-spillmanager-device-memory-limit-spilled-unspilled/332484 (accessed 2026-10-28)
- https://discourse.jupyter.org/t/minimal-image-compatible-with-nvidia-rapids/34116 (accessed 2026-10-28)
- https://discuss.pytorch.org/t/cudf-for-python-3-11/189225 (accessed 2026-10-28)
- https://community.databricks.com/t5/data-engineering/executors-getting-killed-while-scaling-spark-jobs-on-gpu-using/td-p/121862 (accessed 2026-10-28)
- https://community.databricks.com/t5/data-governance/cannot-set-spark-plugins-com-nvidia-spark-sqlplugin-config/td-p/125570 (accessed 2026-10-28)
- https://community.fabric.microsoft.com/t5/Fabric-Updates-Blog/A-new-analytics-frontier-GPU-accelerated-Fabric-Data-Warehouse/bc-p/5192701 (accessed 2026-10-29)
- https://www.microsoft.com/en-us/research/publication/coddspeed-hardware-accelerated-query-processing-in-microsoft-fabric/ (accessed 2026-10-29)
- https://kx.com/blog/introducing-kdb-x-gpu-acceleration/ (accessed 2026-10-29)
- https://code.kx.com/kdb-x/modules/gpu/introduction.html (accessed 2026-10-29)
- https://fast-er.readthedocs.io (accessed 2026-10-29)
- https://www.answeroverflow.com and https://linen.dev: HTTP 429, not readable (accessed 2026-10-29)
