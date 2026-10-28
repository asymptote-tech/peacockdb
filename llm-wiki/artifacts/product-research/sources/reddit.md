# Reddit: who wants GPU SQL / dataframes, and for what

## Method

- All links (accessed 2026-10-28). Window 2023-10-01 to 2026-10-28; no older hits kept.
- reddit.com's own API refuses this host (403). Used two public archives instead. Scripts are in SCRATCH/scrape/reddit/ (pp.py, as.py, extract.py, score.py, curated.py). Raw pages are in SCRATCH/raw/reddit/.
- **PullPush** (api.pullpush.io), searched site-wide with no subreddit filter. 50 pages, 38 queries. Submission queries: cudf, "cudf.pandas", "cudf polars", "spark rapids", "rapids accelerator", "rapids cudf", "polars gpu", "duckdb gpu", "gpu databases", "gpu sql", "gpu analytics", "gpu join", "gpu parquet", "gpu etl", "gpu accelerated sql", heavydb, omnisci, heavy.ai, kinetica, sqream, blazingsql, "pg-strom", pgstrom, brytlyt, "sirius" duckdb, "voltron data", theseus gpu, "clickhouse gpu", "postgres gpu", "postgresql gpu". Comment queries: cudf, "cudf.pandas", "cudf polars", "spark rapids", "rapids accelerator", "rapids cudf", "polars gpu", "duckdb gpu".
- PullPush refused us after about 45 requests, even at one request every 2–4 s. It returned HTTP 429 with the message "This website does not provide free scraping resources for agents". The 429 persisted through 10+ minutes of backoff. The phrase queries "gpu database" and "gpu dataframe" returned 429 every time, even while other queries still worked. I stopped PullPush at that point. These were never fetched: the per-subreddit plan, all remaining site-wide comment searches, and the queries "gpu olap", "gpu query", "velox gpu", "datafusion gpu", "gpu pandas", "nvidia rapids", "gpu spark", "gpu data processing", "gpu-accelerated database".
- **Arctic Shift** (arctic-shift.photon-reddit.com) was used as the fallback. It only allows full-text search inside one subreddit, not across the whole site. 81 pages over 79 queries:
  - Submissions matching `gpu` in dataengineering, databasedevelopment, Database, DuckDB, Clickhouse, apachespark, dataengineeringjobs, PostgreSQL, BusinessIntelligence, PowerBI, snowflake, bigquery, quant, algotrading, observability, SQL, Polars, bioinformatics and dataanalysis.
  - Comments matching `gpu` in DuckDB, Clickhouse and apachespark.
  - Small probes in CUDA (sql, parquet, dataframe) and MachineLearning (cudf).
  - 52 full comment threads fetched by link_id, plus 13 posts looked up by id.
- Arctic Shift often answered "Timeout. Maybe slow down a bit" (HTTP 422). It went down twice with Cloudflare 530/1033 errors (13:11–13:16, and again from about 13:55).
- These Arctic Shift searches failed after retries:
  - Comment search in dataengineering, Database, databasedevelopment, PostgreSQL, quant, algotrading, gis and datascience.
  - Submission search in datascience, databricks, gis, HPC, LocalLLaMA, nvidia and the rest of MachineLearning/CUDA.
  - All of rust, Python, sysadmin, devops, homelab, networking, cybersecurity, AskNetsec and kubernetes. These were reached only through the site-wide PullPush hits.
- Coverage gaps:
  - PullPush data was current, with posts up to 2026-10-28.
  - Arctic Shift lags or drops some posts. For example, it returned only 3 r/DuckDB and 3 r/Database posts mentioning "gpu" in three years.
  - Removed and deleted posts appear as stubs.
  - Without site-wide comment search beyond 8 terms, first-person mentions buried in big subreddits are under-sampled.
- Funnel: 2,890 unique items fetched (1,703 posts, 1,187 comments). A keyword filter kept 1,099 that pair a GPU term with a data term, or name a GPU engine. I ranked those and read the bodies of about 600. **110 were judged relevant** and hand-classified. Noise came mostly from "Theseus", "Kinetica", "Rapids" and "Sirius" used as ordinary words, job ads, stock-picking subreddits and gaming hardware posts.

## Counts

Slug × kind for the 110 relevant hits.

| slug | request | user | complaint | vendor | other | total |
|---|---|---|---|---|---|---|
| workstation-gpu-sql | 1 | 5 | 14 | 4 | 5 | 29 |
| other |  |  | 4 | 4 | 3 | 11 |
| other-accelerators | 4 |  | 2 | 1 | 3 | 10 |
| spark-plugin | 2 | 2 | 2 | 4 |  | 10 |
| new:backtest-sweep-gpu | 2 | 3 |  |  | 2 | 7 |
| new:single-cell-gpu | 1 | 2 | 2 |  | 2 | 7 |
| distributed-pluggable | 1 |  |  | 3 |  | 4 |
| trainer-side-sql-prep |  | 2 |  | 1 | 1 | 4 |
| geo-join-gpu |  |  |  | 2 | 1 | 3 |
| gpu-bi-dashboards |  | 1 |  | 1 |  | 2 |
| gpu-scan-tier |  |  | 2 |  |  | 2 |
| lakehouse-iceberg |  |  |  | 1 | 1 | 2 |
| llm-corpus-sql | 1 |  |  | 1 |  | 2 |
| telco-xdr-gpu |  |  |  | 2 |  | 2 |
| vector-sql |  |  |  | 2 |  | 2 |
| asof-gpu |  | 1 |  |  |  | 1 |
| clickhouse |  |  |  | 1 |  | 1 |
| datafusion-log-gpu |  | 1 |  |  |  | 1 |
| datafusion-plugin |  |  |  | 1 |  | 1 |
| flow-hunt-gpu | 1 |  |  |  |  | 1 |
| new:duckdb-gpu-backend |  |  |  | 1 |  | 1 |
| new:gpu-data-editor |  |  |  |  | 1 | 1 |
| new:gpu-record-linkage |  | 1 |  |  |  | 1 |
| new:in-db-gpu-inference | 1 |  |  |  |  | 1 |
| new:r-front-end | 1 |  |  |  |  | 1 |
| new:zk-proof-sql |  |  |  | 1 |  | 1 |
| postgres-gpu-analytics |  |  |  |  | 1 | 1 |
| robot-log-gpu |  |  |  | 1 |  | 1 |
| **all** | 15 | 18 | 26 | 31 | 20 | 110 |

`other` slug = generic GPU-database debate or vendor news that fits no single option.

## Strongest threads

Ranked: first-person concrete workloads first (rows 1–24), then weaker first-person, skeptics and the most relevant projects.

| date | system | slug | kind | use case | quote | link |
|---|---|---|---|---|---|---|
| 2025-05-27 | cuDF on Tesla V100 32GB | datafusion-log-gpu | user | u/fmoralesh (r/dataengineering): 1.2 TB process logs, GROUP BY for reports; old DB crashed at 25 GB | "I have around 1.2 TB of LOGs from certain process, and I need to do some "group by" operation to get some information for reporting." | https://www.reddit.com/r/dataengineering/comments/1kw9ku2/comment/mugm06f/ |
| 2025-06-12 | ClickHouse JSONExtractString; dask_cudf | gpu-scan-tier | complaint | u/fmoralesh (r/dataengineering): 11 TB Parquet on S3 with a nested-JSON column; 5 TB took 6.5 h | "it is extremely slow given the amount of data I'm trying to process." | https://www.reddit.com/r/dataengineering/comments/1la1lcw/ |
| 2025-06-13 | dask_cudf | gpu-scan-tier | complaint | u/fmoralesh (r/dataengineering): same 11 TB nested-JSON Parquet job | "I tried with DASK\_CUDF (not Polars), but it is not practical to process that amount of data." | https://www.reddit.com/r/dataengineering/comments/1la1lcw/comment/mxkx44z/ |
| 2024-12-14 | cuDF on RTX A5000 12GB (Dell Precision laptop) | workstation-gpu-sql | user | u/Wackome (r/mac): policy simulations; wants same speed on Apple M4 | "The GPU acceleration speeds up my simulations by about 70x." | https://www.reddit.com/r/mac/comments/1hdxh51/ |
| 2024-09-18 | cuDF / Polars GPU on Windows | workstation-gpu-sql | request | u/tecedu (r/Python): team blocked by cuDF being Linux-only | "at this point we are willing to pay someone to port over cudf to work with windows." | https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/ |
| 2024-11-25 | cuDF on in-house GPU boxes | workstation-gpu-sql | user | u/tecedu (r/gadgets): team runs GPU-first data jobs, CPUs only orchestrate | "For us we have a couple of GPUs setup with CPUs just being orchestrators." | https://www.reddit.com/r/gadgets/comments/1gywo4t/comment/lyyif4w/ |
| 2025-11-22 | Spark RAPIDS on Azure Databricks (already on Photon) | spark-plugin | request | u/PerfectAmbassador197 (r/dataengineering): PySpark/Spark SQL ETL, large tables, heavy joins and aggregations | "Codebase is mostly pyspark/spark SQL with processing on large tables with heavy joins and aggregations." | https://www.reddit.com/r/dataengineering/comments/1p44fyv/ |
| 2025-10-14 | DuckDB -> wants R + CUDA on a few workstations, no Docker/K8s | distributed-pluggable | request | u/No_Mongoose6172 (r/dataengineering): research statistics outgrew one DuckDB node | "Previously I've been using duckdb, but using a single node is no longer possible due to the increasing amount of data we have to analyse." | https://www.reddit.com/r/dataengineering/comments/1o6bnir/ |
| 2026-07-16 | hand-written CUDA Jaro-Winkler kernel | new:gpu-record-linkage | user | u/axwinters (r/CUDA): record linkage across administrative datasets (fuzzy name joins) | "I work at a social science research institute where we maintain a record linkage library." | https://www.reddit.com/r/CUDA/comments/1uy5az7/ |
| 2024-05-16 | Python on 32-core Threadripper / 192-core AWS | new:backtest-sweep-gpu | request | u/estimated1 (r/algotrading): 5 s OHLC bars, tens of thousands of parameter permutations | "When I do long (6month+) tests with tens of thousands of parameter permutations this can take awhile, even when running across 192 cores." | https://www.reddit.com/r/algotrading/comments/1ctidxy/ |
| 2025-02-18 | cuDF | new:backtest-sweep-gpu | user | u/EdwardM290 (r/algotrading): Bollinger mean-reversion on 1-min OHLC, grid search | "i just used grid search with cuDF (which is exactly like pandas)" | https://www.reddit.com/r/algotrading/comments/1iq21bf/comment/mdiesoz/ |
| 2026-04-26 | CuPy, cuDF, Dask | new:backtest-sweep-gpu | user | u/RoundTableMaker (r/quant): backtest grid searches on financial data | "Takes grid searches down from 1/sec to 100k in 13 seconds." | https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/ |
| 2026-10-25 | home app on RTX 5090 | asof-gpu | user | u/1StunnaV (r/algotrading): every S&P 500 trade, 500-750 MB/day, building a backtest DB | "does the calculations on my 5090 GPU and creates the json files and database for the gui." | https://www.reddit.com/r/algotrading/comments/1xf84n8/ |
| 2025-11-14 | rapids_singlecell replacing R/Seurat | new:single-cell-gpu | user | u/supermag2 (r/bioinformatics): scRNA-seq datasets of hundreds of thousands of cells | "datasets are getting increasingly bigger with time so R suffers quite a bit with this" | https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/ |
| 2025-11-15 | rapids_singlecell vs scanpy | new:single-cell-gpu | user | u/Shot-Rutabaga-72 (r/bioinformatics): 50k cells on a 4-year-old lab server | "I ran 50k cells with CPU (scanpy) vs GPU (rsc) on our 4 year old server." | https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/nowyn62/ |
| 2025-11-15 | Seurat on 128 GB workstation | new:single-cell-gpu | request | u/KMcAndre (r/bioinformatics): CosMx spatial data, hundreds of thousands to millions of cells | "6K genes hundreds of thousands of cells if not millions" | https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/nowvcwv/ |
| 2024-07-13 | RAPIDS before Power BI | gpu-bi-dashboards | user | u/Nokita_is_Back (r/PowerBI): preprocessing outside Power BI | "I go with gpu/rapids for anything matrix related." | https://www.reddit.com/r/PowerBI/comments/1e23ace/ |
| 2024-01-10 | ClickHouse + Python UDFs | new:in-db-gpu-inference | request | u/onefanwu (r/Clickhouse): BART summarisation over documents stored in ClickHouse | "I'm exploring ways to perform efficient AI model inference within ClickHouse" | https://www.reddit.com/r/Clickhouse/comments/19334wv/ |
| 2025-10-07 | tables-with-transactions pipeline | llm-corpus-sql | request | u/Fabulous_Pollution10 (r/dataengineering): fine-tuning data pipelines for LLM startups | "The orchestrator can’t “see” the data, so batching is poor, retries are clumsy, and my GPUs sit idle." | https://www.reddit.com/r/dataengineering/comments/1o0kyd0/ |
| 2025-10-03 | cuDF on 2x GTX 1080 Ti | flow-hunt-gpu | request | u/Puzzleheaded-Life46 (r/learnmachinelearning): intrusion-detection dataset, 'massive' | "im trying to build an enviroment for and IDS using cuda drivers cudf nvidiatoolkit etc cause my dataset is massive." | https://www.reddit.com/r/learnmachinelearning/comments/1nwsne4/ |
| 2024-02-20 | cuDF + DALI | trainer-side-sql-prep | user | u/thrileyreid (r/pcmasterrace): 27 GB CSVs that exhaust 32 GB RAM | "ive exhausted it many times just to load the csv files which are 27gbs large sometimes" | https://www.reddit.com/r/pcmasterrace/comments/1avakqc/comment/kr9w4nc/ |
| 2026-01-15 | DuckDB internals + Apple Metal (MPS) | other-accelerators | request | u/Impressive_Run8512 (r/DuckDB): custom gpu_mean-style DuckDB functions | "Basically I'm trying to create functions like `gpu_mean`, etc for a very specific use case." | https://www.reddit.com/r/DuckDB/comments/1qdrwxi/ |
| 2026-01-15 | Metal; looked at Sirius | other-accelerators | other | u/Impressive_Run8512 (r/DuckDB): same thread: why not Sirius | "I've looked at Sirius before, but doesn't seem like they're doing anything." | https://www.reddit.com/r/DuckDB/comments/1qdrwxi/comment/nzsar8s/ |
| 2025-03-16 | custom GPU genetic modelling | new:backtest-sweep-gpu | user | u/Phunk_Nugget (r/quant): CME ES futures research | "I have a custom, genetic, GPU based modelling approach and highly optimized data storage and processing." | https://www.reddit.com/r/quant/comments/1jcgceh/ |
| 2024-05-23 | Spark RAPIDS on a local server | spark-plugin | user | u/tuliosarmento (r/apachespark): first tests; asks whether VRAM adds to RAM | "I'm running my first tests in my local server with rapids acceleration (GPU oriented)." | https://www.reddit.com/r/apachespark/comments/1cz54v0/ |
| 2024-10-08 | PySpark + RAPIDS, CUDA 12.6, Spark 3.4 | spark-plugin | complaint | u/pyamy (r/apachespark): local GPU Spark job hangs | "it gets stuck on stage 0, the same file runs very well with cpu." | https://www.reddit.com/r/apachespark/comments/1fyq8cq/ |
| 2025-01-31 | cuDF on RTX 4050 6 GB laptop | workstation-gpu-sql | complaint | u/a_steel_heart_ (r/CUDA): 1 GB CSV, df.corr() runs out of memory | "when i try to to df.corr() i get Call to cuMemcpyDtoH results in CUDA_ERROR_OUT_OF_MEMORY" | https://www.reddit.com/r/CUDA/comments/1iena2p/ |
| 2024-09-17 | cuDF / Polars GPU | workstation-gpu-sql | complaint | u/EtienneT (r/datascience): data larger than VRAM | "Last time I tried, I could not get cuDF to work with larger than memory GPU datasets." | https://www.reddit.com/r/datascience/comments/1fj0nmt/comment/lnl3cw9/ |
| 2024-12-27 | cuDF | workstation-gpu-sql | complaint | u/yeochinschadanheze (r/dataengineering): ETL with strings | "At least cudf seems to not useful for etl purpose, it does not even offer str object." | https://www.reddit.com/r/dataengineering/comments/1h74caw/comment/m40s7kx/ |
| 2024-07-10 | Spark on Mac Pro M2 Ultra | other-accelerators | request | u/Individual-Divide817 (r/apachespark): single-box Spark on Apple GPU | "I’m planning to set up Apache Spark on my new Mac Pro M2 Ultra and want to leverage its GPUs" | https://www.reddit.com/r/apachespark/comments/1dzlu3b/ |
| 2023-11-11 | cuDF from R via Arrow | new:r-front-end | request | u/jcstay123 (r/rprogramming): R user wants cuDF speed | "I have started to use Cudf in python and honestly it's incredibly fast. Now I would much rather work in R." | https://www.reddit.com/r/rprogramming/comments/17snq57/ |
| 2023-12-06 | cuDF on RTX 3080 Ti | workstation-gpu-sql | user | u/thegratefulshread (r/macbookpro): ~50 GB local analysis vs a Mac | "My mac is even slow with my mongodb query script for around 50 gigs of data." | https://www.reddit.com/r/macbookpro/comments/18cczma/comment/kcaebgz/ |
| 2024-05-12 | PySpark UDFs rewritten in PyTorch | spark-plugin | user | u/Appropriate_Ant_4629 (r/apachespark): GPU-heavy UDFs inside Spark ETL | "Some of our UDFs benefit from GPU acceleration - so those were re-written in Python/PyTorch." | https://www.reddit.com/r/apachespark/comments/1cq4h53/comment/l3pm73y/ |
| 2025-06-14 | GPU table renderer (Coco Alemana) | new:gpu-data-editor | other | u/Impressive_Run8512 (r/dataengineering): interactive editing of 100M-row tables | "We accidentally built an insanely fast rendering mechanism for large tables." | https://www.reddit.com/r/dataengineering/comments/1lb13ep/ |
| 2026-01-07 | GPU SQLite on Apple Silicon (Metal) | other-accelerators | other | u/sado361 (r/programming): test of the PCIe-copy argument; 369 upvotes, 68 comments | "I built a GPU-accelerated SQLite on Apple Silicon to test a theory about the PCIe bottleneck." | https://www.reddit.com/r/programming/comments/1q6dj59/ |
| 2026-07-30 | Voltron Theseus, GPU DBs | other | other | u/Small_Management_302 (r/databasedevelopment): after Voltron's failure, do GPU DBs have a future? | "Why did Voltron Data Company go bankrupt? Has the Theseus GPU database also failed?" | https://www.reddit.com/r/databasedevelopment/comments/1vafuoh/ |
| 2026-08-01 | GPU coprocessors | other | complaint | u/Suspicious_Web6431 (r/databasedevelopment): workloads too rare or too small | "it doesn't make economic sense to run analytical workloads on a coprocessors because either the workload is too infrequent or it is not big enough." | https://www.reddit.com/r/databasedevelopment/comments/1vafuoh/comment/p128rmi/ |
| 2026-07-01 | NVIDIA GQE / Sirius | other | complaint | u/quantum_pretzel (r/databasedevelopment): needs costly interconnect to beat CPU end to end | "with a fast enough interconnect to make them outperform CPU systems in end to end latencies." | https://www.reddit.com/r/databasedevelopment/comments/1ukefwg/comment/ouvmxdc/ |
| 2026-10-28 | Algeon: DataFusion + cuGraph/cuVS/cuML table functions | datafusion-plugin | vendor | u/diaper151 (r/dataengineering): SQL over GPU graph/vector/ML | "GPU analytics used to require ad-hoc ETL pipelines, Python glue code, and imperative operation calls." | https://www.reddit.com/r/dataengineering/comments/1xhqvo3/ |
| 2026-02-18 | cuDF + NIM on GKE (H.E.I.M.D.A.L.L) | robot-log-gpu | vendor | u/IllustratorAlive8644 (r/LocalLLaMA): natural-language queries over fleet telemetry | "Managing telemetry from hundreds or thousands of autonomous vehicles or robots means dealing with terabytes of logs." | https://www.reddit.com/r/LocalLLaMA/comments/1r7viid/ |

## New directions

- **new:backtest-sweep-gpu**: parameter-sweep backtests over OHLC or tick data. This is the most repeated first-person GPU-dataframe workload in this sample (7 hits).
  - Users run grid searches with cuDF/CuPy ([mdiesoz](https://www.reddit.com/r/algotrading/comments/1iq21bf/comment/mdiesoz/), [oians0g](https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/), [1jcgceh](https://www.reddit.com/r/quant/comments/1jcgceh/)).
  - Others ask whether they can ([1ctidxy](https://www.reddit.com/r/algotrading/comments/1ctidxy/), 29 comments; [1kx8wjv](https://www.reddit.com/r/algotrading/comments/1kx8wjv/)).
  - The shape is the same few queries over one dataset, run thousands of times with different parameters. That is closer to batched SQL than to one big query. Skeptics note that the code is branchy and sequential ([l4ejf6t](https://www.reddit.com/r/algotrading/comments/1ctidxy/comment/l4ejf6t/)).
- **new:single-cell-gpu**: scRNA-seq and spatial omics tables of 10^5–10^6 cells, moving from R/Seurat to rapids_singlecell.
  - The post reports 15 min → 1 min ([1ox6vu2](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/), 25 comments; [nowyn62](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/nowyn62/)).
  - A CosMx user with millions of cells on a 128 GB workstation is interested ([nowvcwv](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/nowvcwv/)).
  - Pushback: batch jobs don't need the speed, and GPU memory is small ([novurzn](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/novurzn/), [nox8u66](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/comment/nox8u66/)).
  - This is matrix and graph work, not SQL. It fits a "GPU tables for scientists" angle only loosely.
- **new:gpu-record-linkage**: fuzzy joins (Jaro-Winkler) across administrative datasets at a research institute. The author wrote a CUDA kernel ([1uy5az7](https://www.reddit.com/r/CUDA/comments/1uy5az7/)). A string-similarity join operator in SQL would serve this directly.
- **new:in-db-gpu-inference**: running a GPU model (BART summarisation) over rows stored in ClickHouse. Today this is only possible through Python UDFs ([19334wv](https://www.reddit.com/r/Clickhouse/comments/19334wv/)). Related projects:
  - Algeon exposes cuGraph/cuVS/cuML as DataFusion table functions ([1xhqvo3](https://www.reddit.com/r/dataengineering/comments/1xhqvo3/)). It is a near neighbour of a DataFusion-based GPU engine, and either a collaborator or a competitor.
  - Spark users rewrite GPU UDFs in PyTorch ([l3pm73y](https://www.reddit.com/r/apachespark/comments/1cq4h53/comment/l3pm73y/)).
- **new:windows-native / workstation install**: cuDF being Linux-only and hard to install is the largest complaint cluster (14 complaints under workstation-gpu-sql).
  - Examples: Windows ([lnrwn9g](https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnrwn9g/), [lnmciqq](https://www.reddit.com/r/Python/comments/1fj0kfi/comment/lnmciqq/)), WSL2, Pascal GPUs dropped, conda solving, and a PyInstaller .exe ([1uywd7o](https://www.reddit.com/r/CUDA/comments/1uywd7o/)).
  - A single static binary that reads local Parquet with no CUDA toolkit setup would answer this directly.
- **new:gpu-data-editor**: a desktop data editor with GPU table rendering, whose author is also trying to push DuckDB intermediates onto Apple Metal ([1lb13ep](https://www.reddit.com/r/dataengineering/comments/1lb13ep/), [1qdrwxi](https://www.reddit.com/r/DuckDB/comments/1qdrwxi/)).
- **new:r-front-end**: R users want cuDF speed without leaving R ([17snq57](https://www.reddit.com/r/rprogramming/comments/17snq57/), [1o6bnir](https://www.reddit.com/r/dataengineering/comments/1o6bnir/), [1ox6vu2](https://www.reddit.com/r/bioinformatics/comments/1ox6vu2/)). A SQL engine reachable from R through ADBC/DBI would reach them.
- **new:duckdb-gpu-backend**: Kinetica markets itself as "The GPU Backend for DuckDB" ([1qcr6cy](https://www.reddit.com/r/u_KineticaDB/comments/1qcr6cy/)). Users ask for the same thing and dismiss Sirius as inactive ([nzsar8s](https://www.reddit.com/r/DuckDB/comments/1qdrwxi/comment/nzsar8s/)).
- **new:zk-proof-sql**: Space and Time runs Proof-of-SQL zero-knowledge proofs on a GPU ([1dew2xn](https://www.reddit.com/r/PostgreSQL/comments/1dew2xn/)). Vendor only; noted for completeness.

## Leads

- **u/fmoralesh** (r/dataengineering, 2025-05/06). This is the strongest single need in the sample.
  - Has 1.2 TB of process logs and needs GROUP BY reports. Their current DB crashes at 25 GB.
  - Already uses cuDF on a Tesla V100 32 GB.
  - Then 11 TB of S3 Parquet with a nested-JSON column. ClickHouse JSONExtractString is too slow, 5 TB takes 6.5 h, and dask_cudf was "not practical".
  - Fits datafusion-log-gpu and gpu-scan-tier (Parquet + JSON extraction on one GPU box).
- **u/tecedu** (r/Python, r/hardware, r/gadgets, 2024).
  - Their team runs data jobs on GPU boxes with CPUs as orchestrators, and chose NVIDIA for cuDF and CuPy.
  - They are "willing to pay someone to port over cudf to work with windows". A paying team blocked by platform support.
- **u/Wackome** (r/mac, 2024-12). Policy analyst whose simulation pipelines run 70× faster with cuDF on an A5000 laptop. Wants the same on Apple M4. Fits workstation-gpu-sql and other-accelerators.
- **u/No_Mongoose6172** (r/dataengineering, 2025-10). Research group whose statistics outgrew single-node DuckDB. Wants a cluster of a few workstations with R and CUDA, without Docker/K8s.
- **u/PerfectAmbassador197** (r/dataengineering + 5 cross-posts, 2025-11). Evaluating Spark RAPIDS on Azure Databricks for join-heavy PySpark/SQL ETL. Already on Photon, wants measured speedups and costs. Fits spark-plugin / gpu-sql-migration-qualifier.
- **u/axwinters** (r/CUDA, 2026-07). Social-science institute that hand-ported record linkage to CUDA. A concrete fuzzy-join workload.
- **u/Impressive_Run8512** (Coco Alemana, r/DuckDB, r/nvidia, r/dataengineering, 2025–26). Building GPU paths into DuckDB-style tools on Metal. Has studied cuDF internals.
- **u/estimated1, u/EdwardM290, u/RoundTableMaker, u/Phunk_Nugget, u/1StunnaV** (r/algotrading, r/quant). Backtest sweeps over bar or tick data. EdwardM290 published a cuDF backtester on GitHub; 1StunnaV computes all S&P 500 trades on an RTX 5090.
- **u/supermag2, u/Shot-Rutabaga-72, u/KMcAndre** (r/bioinformatics, 2025-11). Single-cell and spatial datasets moving to GPU.
- **u/onefanwu** (r/Clickhouse, 2024-01). GPU model inference over rows stored in ClickHouse.
- **u/Fabulous_Pollution10** (r/dataengineering, 2025-10). Builds fine-tuning data pipelines for LLM startups and says "my GPUs sit idle". Fits llm-corpus-sql.
- **u/diaper151** (Algeon, 2026-10-28). Builds a DataFusion + RAPIDS SQL engine. Worth a conversation as a peer.
- **u/Nokita_is_Back** (r/PowerBI). Uses RAPIDS for preprocessing before Power BI.

## What this source says

- Reddit has few first-person GPU-SQL users. Most GPU-data posts are vendor launches (Polars GPU, Theseus, Kinetica, HeavyDB, SQream, Bodo, ParaQuery, GQE), NVIDIA staff replies, or install trouble.
- Concrete pain clusters on single machines, not clusters:
  - one person with a V100 and terabytes of logs;
  - analysts on GPU laptops and workstations;
  - backtest sweeps;
  - single-cell tables.
  - This favours workstation-gpu-sql and datafusion-log-gpu over distributed products.
- The biggest complaint cluster is setup, not speed: Linux-only cuDF, WSL2 failures, Pascal GPUs dropped, conda solving, packaging. Larger-than-VRAM failures come second. Polars' new streaming GPU backend now targets that second problem.
- Skeptics are loud and consistent, especially in r/databasedevelopment after Voltron's collapse (2026-07). Their points:
  - PCIe copies eat the gain;
  - most DE work is I/O-bound;
  - the hardware is too expensive;
  - risk-averse buyers won't swap databases.
  - The few positive remarks point to string/regex and geospatial work, and to warehouses over rebuildable data.
- Spark RAPIDS shows up as evaluation questions and local-setup failures. No Reddit poster reported running it in production.
- Apple Silicon and AMD demand is real but small: Metal GPU SQLite, MLX TPC-H, Spark on M2 Ultra, "AMD users out of luck?".
- Weak or absent here: telco, observability, cybersecurity, GIS, Postgres-GPU, ClickHouse-GPU and lakehouse users. The only hits there are vendor posts, partly because the subreddit searches for those communities failed (see Method).
- Treat the counts as indicative only. The sample was cut short by PullPush's agent block and Arctic Shift outages. Site-wide comment search covered only 8 terms.
