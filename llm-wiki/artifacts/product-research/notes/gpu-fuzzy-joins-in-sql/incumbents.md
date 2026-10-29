# Incumbent entity-resolution / record-linkage tools: costs, runtimes, pain points at 10M–1B records

Method note: the session's WebSearch budget was used up before this task started, so all evidence comes from direct WebFetch of known URLs and from the GitHub API (`gh`, GraphQL search over issues and discussions). Vendor topics that needed open web search (Quantexa, Tamr, Snowflake/Google native ER, KYC buyer studies) are therefore mostly **gaps**. All URLs are (accessed 2026-10-29).

Repo HEAD SHAs used for code and README claims (accessed 2026-10-29):
- moj-analytical-services/splink `5b719296d62bc48d55c1a63322fd60c0bfba4c2e`; latest release v5.0.0, 2026-09-18
- zinggAI/zingg `9d1a7f4951b12ba65d6ca7ad7d650d45afd9f77f`; latest release v0.7.0, 2026-08-10
- dedupeio/dedupe `3f61e79102910bd355e920a2df7e44c14c9cb247`
- J535D165/recordlinkage `b93d97641952f8c85106be5794ca93b1f1298fbc` (last push 2024-02-21)
- Data-Linkage/Splink-census-linkage (ONS) `1de9772853b7b805b69649fb78226c5d3dc76219`

## Q1. Who does ER today, and with what tools

### Takeaway
Open-source ER is led by Splink: a Fellegi-Sunter model that emits SQL and runs on DuckDB, Spark, Athena or Postgres. It is backed by UK government and was downloaded more than 15M times. Zingg (Spark/Snowflake, AGPL plus an enterprise edition) and dedupe (Python, active learning) follow it. The managed and commercial options price by record: AWS Entity Resolution charges $0.25 per 1k records, and a Senzing licence runs from about $58.6k/yr at 10M records to about $1.5M/yr at 1B. Databricks' own ER offering, Arc, is a wrapper around Splink, and its repo is no longer at its original GitHub location.

### Cited Findings
**Splink**
- Splink describes itself as "Capable of linking a million records on a laptop in around a minute" and able to run on "big-data backends like Spark for 100+ million records". It is based on Fellegi-Sunter — [Splink README @5b719296](https://github.com/moj-analytical-services/splink) (accessed 2026-10-29)
- The repo has about 2.5k stars and 281 forks. Its awards include Civil Service Awards 2025 (Innovation winner) and OpenUK Awards 2025 (Open data winner) — [Splink GitHub](https://github.com/moj-analytical-services/splink) (accessed 2026-10-29)
- Splink "would be downloaded over 15 million times". Its team "has averaged two people", and Robin Linacre started writing it in 2019. The same post cites the US Census Bureau saying that NSIs "had failed in possibly thirty attempts" to build such systems — [Linacre, "Splink or Swim", 2026-02-24](https://www.robinlinacre.com/splink_or_swim/) (accessed 2026-10-29)
- In 2023 Splink had "over 230,000 monthly downloads from PyPI". Databricks Arc is "a simple approach to data linking built on the open source project Splink". Arc auto-selects blocking rules and comparisons using Hyperopt and MLflow, and its largest evaluation dataset was 193,750 rows — [Databricks blog, 2023-08-04](https://www.databricks.com/blog/2023/08/04/improving-public-sector-decision-making-simple-automated-record-linking.html) (accessed 2026-10-29)
- `gh api repos/databrickslabs/arc` returns 404, so Arc has been moved or deleted from databrickslabs — GitHub API (accessed 2026-10-29). I found no evidence of any other Databricks-native ER product (see Gaps).
- ONS linked the 2021 England and Wales Census (about 58M records) to itself with Splink to find duplicate responses — [ONS Splink case study PDF, repo @1de97728](https://github.com/Data-Linkage/Splink-census-linkage/blob/main/SplinkCaseStudy.pdf) (accessed 2026-10-29)
- Splink's GitHub discussions show that its users include UK Companies House linkers ([#2886, domvwt, 2026-01-15](https://github.com/moj-analytical-services/splink/issues/2886)), MPI (master patient index) builders ([#2573, SR-splink, 2024-12-30](https://github.com/moj-analytical-services/splink/discussions/2573)), a 500M-row daily dedupe on Databricks ([#2173, guylissak, 2024-05-10](https://github.com/moj-analytical-services/splink/discussions/2173)) and a 45M-row dedupe on Dataproc ([#2876, vbvsahai, 2026-01-13](https://github.com/moj-analytical-services/splink/discussions/2876)). There is also community interest in Trino and Snowflake backends ([#3116](https://github.com/moj-analytical-services/splink/discussions/3116), [#2303](https://github.com/moj-analytical-services/splink/discussions/2303)) (all accessed 2026-10-29)

**Zingg**
- Zingg is AGPL v3 and runs on Spark. It reads from and writes to Snowflake, Databricks and others. It "learns a clustering/blocking model", and "Typical Zingg comparisons are 0.05-1% of the possible problem space" — [Zingg README @9d1a7f49](https://github.com/zinggAI/zingg) (accessed 2026-10-29)
- Zingg Enterprise adds incremental runs, a persistent ZINGG_ID, explainability and a "Native Snowflake run". Named customers are Fortnum & Mason, Redica Systems, Orthodox Union, DMG Events and the Canadian Football League. The site claims "1B+ Records resolved every month" but publishes no prices — [zingg.ai](https://www.zingg.ai/) (accessed 2026-10-29). The pricing page returned 404 — [zingg.ai/pricing](https://www.zingg.ai/pricing) (accessed 2026-10-29)

**dedupe**
- The dedupe repo has about 4.5k stars (GitHub API, [dedupeio/dedupe](https://github.com/dedupeio/dedupe), accessed 2026-10-29). Its scaling story is Postgres/MySQL examples. One user reported "successful with about 60million records using Postgres" — [dedupe #749, tendres, 2019-05-31](https://github.com/dedupeio/dedupe/issues/749) (accessed 2026-10-29)

**Other open source**
- RapidFuzz has about 4.2k stars, recordlinkage (Python) about 1.07k stars with its last push on 2024-02-21 (effectively dormant), and polars-ds about 657 stars — GitHub API: [rapidfuzz/RapidFuzz](https://github.com/rapidfuzz/RapidFuzz), [J535D165/recordlinkage](https://github.com/J535D165/recordlinkage), [abstractqqq/polars_ds_extension](https://github.com/abstractqqq/polars_ds_extension) (accessed 2026-10-29)

**AWS Entity Resolution**
- Rule-based or ML matching costs "$0.25 per 1,000 records processed". Data-service-provider matching costs "$0.10 per 1,000 records" plus the provider's subscription. You are "charged for all records processed … even those not resulting in matches". Incremental processing is billed for rule-based matching only: "Automated incremental processing is not available when using ML-powered matching". There is no free tier, and the price is the same in every region — [AWS ER pricing](https://aws.amazon.com/entity-resolution/pricing/) (accessed 2026-10-29)

**Senzing**
- Senzing prices by Data Source Records (DSRs) on an annual subscription: 10M DSRs is $58,560/yr and 1B DSRs is $1,497,000/yr. There are 51 volume tiers, but only these two prices are published. Without a licence it runs in evaluation mode, capped at 500 records. Senzing claims it is "proven in production at ~100B records" — [senzing.com/pricing](https://senzing.com/pricing/) (accessed 2026-10-29)

### Inferences
- Splink is the reference incumbent for "SQL-engine ER". It already treats ER as SQL generated for whichever engine you plug in, so a GPU SQL engine could become *a Splink backend* rather than a competitor (estimate).
- Per-record managed pricing is steep at scale. At $0.25/1k records, AWS ER costs $25k per pass over 100M records and $250k per pass over 1B. Senzing's 1B licence is about $1.5M/yr. Splink on DuckDB did 7M records for under $1 (Q2). The roughly 1000× cost gap between OSS on a large box and managed ER is the market's current shape (estimate, derived from the cited prices).

### Gaps
- **Quantexa and Tamr**: I found no contract sizes or pricing. tamr.com did not resolve (DNS failure), and with search exhausted I could not reach procurement records or analyst reports.
- **Snowflake and Google native ER**: Google's Enterprise Knowledge Graph pricing URL returned 404. I found no Snowflake-native ER product page; Snowflake ER appears to come via Zingg Enterprise and the marketplace (unverified).
- **NHS and other UK government Splink users**: no primary source fetched. The awards and the "used widely within government" claim are self-reported.
- **Zingg pricing**: not published.

## Q2. Published runtimes and costs

### Takeaway
On CPU, the leading open-source tool already reaches billion-row scale. Splink on DuckDB deduplicates 7M records (1B comparisons) in about 2 minutes for under $1. A 1B-record input that produces 10B scored pairs takes about 8–9 minutes of `predict()` on a single 192-vCPU EC2 box. After that, the bottleneck moves to spill and Parquet I/O, not similarity compute. Spark-based tools (Splink on Spark, Zingg) are 2–9× slower, and they take hours to tens of hours at 30–90M records.

### Cited Findings
**Splink benchmarks**
- 7M records were deduplicated in "just over 2 minutes, and at a cost of less than $1.00 using AWS EC2". The blocking rules "create 1bn comparisons in total" (about 100 comparisons per record), with Jaro-Winkler, Levenshtein and term-frequency adjustments. The full run took about 25 minutes on a c6g.2xlarge (8 vCPU, 16 GiB). On Spark, u-estimation took "about 60% longer" and predict took "8.7 times longer" than DuckDB, partly because of straggler tasks. The comparison point is fastLink, which took 400 min for 300k records on 8 cores — [Linacre, "Super-fast deduplication…", 2024-01-18](https://www.robinlinacre.com/fast_deduplication/) (accessed 2026-10-29)
- **Splink 5 at 1B rows** (the most important data point), run on an r8id.48xlarge (192 vCPU, 1.5 TiB RAM, 3×3.5 TB NVMe RAID0) with DuckDB 1.6.0.dev365 and Splink pinned at commit `7269e36cb510dcdb350846d55ab3b3b4da7250ba` (PR 3255):
  - Input was 1,000,000,000 rows with 6 blocking rules, producing 10,037,267,664 prediction rows.
  - `predict()` took 478–550 s, but the output "did not fit in memory": 556.5 GiB was held in memory and about 1.5 TB spilled.
  - Parquet export of the spilled table ran about 75× slower (about 1.5 GB/min) while about 75% of the CPUs sat idle, and it was cancelled after about 14 min.
  - A one-third chunk produced 1.115B predictions in 97.4 s and wrote 47 GB of zstd Parquet in 25 s.
  - On-disk DuckDB connections caused periods of near-0% CPU: predict_chunk took 214.6 s on disk versus 20.7 s in memory.
  - Source: [Splink discussion #3263, RobinL, 2026-09-02](https://github.com/moj-analytical-services/splink/discussions/3263) (accessed 2026-10-29)
- On large EC2 boxes "Do not assume more threads always gives better performance". The memory bandwidth of r8id.32xlarge/metal-48xl (330–480 GB/s) is comparable to an M4 Max (410–546 GB/s). A CPU-bound `sum(jaro_winkler_similarity(col1,col2))` over 800M rows does pin 100% CPU. "Throughput of writing to disk can be an important bottleneck" — [Linacre, "Optimising DuckDB performance on large EC2 instances", 2026-05-18](https://www.robinlinacre.com/optimising_duckdb_performance_large_ec2_instances/) (accessed 2026-10-29)

**ONS 2021 Census**
- The census had about 58M records. 42 blocking rules produced "approximately 3 billion candidate pairs", and the "Splink global model took around 4 hours". Rules with ">10 million comparisons and/or at least one block with more than 100 people were re-written". The training gold standard was a 3,219,099-record clerically reviewed sample (23,434 duplicates) — [ONS case study PDF @1de97728](https://github.com/Data-Linkage/Splink-census-linkage/blob/main/SplinkCaseStudy.pdf) (accessed 2026-10-29). The backend is not stated in the extracted text; the case study names Athena/Spark compatibility.

**Zingg official sizing**

| Records | Hardware | Runtime |
|---|---|---|
| 120k | 4 cores, 10 GB, local Spark | 5 min |
| 5M (NC voters) | 4 cores, 10 GB | ~4 h |
| 9M, 3 fields | m5.24xlarge (96 cores, 384 GB) | 45 min |
| 80M, 8–10 fields (community user) | 1 driver + 8 workers | under 2 h |

  Above about 50M records a "multi-worker cluster is strongly recommended" — [Zingg hardware sizing](https://docs.zingg.ai/zingg/reference/hardware-sizing-and-benchmarks.md) (accessed 2026-10-29)
- **Zingg at 70M**: "8 Workers, 448 GB Memory, 128 Cores. The matching ran for 7 hours", and the maintainer recommended "double or triple the size" for 200M records — [zingg #446, sonalgoyal, 2022-10-17](https://github.com/zinggAI/zingg/issues/446) (accessed 2026-10-29)
- **Zingg at 30M (synthetic)**: on Snowflake, training took 14 min and match took 11 h 43 min, with precision about 81% and recall about 31%. On local Spark the run "ran for about 37 hours" stuck in clustering and was killed. After the 692,727 blank-name records were dropped, it completed in about 17.5 h — [zingg #1339, Chandanydv1234, 2026-07-08/26](https://github.com/zinggAI/zingg/issues/1339) (accessed 2026-10-29)

**AWS ER and Senzing costs**
- AWS ER worked examples: 300k ML-matched records cost $75 and 1M provider records cost $100 plus the provider subscription — [AWS ER pricing](https://aws.amazon.com/entity-resolution/pricing/) (accessed 2026-10-29). I found no published AWS ER runtime, and the quotas page did not render.
- Senzing costs $58,560/yr at 10M and $1,497,000/yr at 1B — [senzing.com/pricing](https://senzing.com/pricing/) (accessed 2026-10-29)

### Inferences
- Splink's DuckDB numbers give about 1B Jaro-Winkler-heavy comparisons in about 2 min on one c6g box. The 1B-row/10B-pair run takes about 500 s of compute on 192 vCPU, which is roughly 20M scored pairs/s. A GPU engine would need to beat about 20M pairs/s per 192-core box *end to end, including output materialisation* to matter (estimate, derived).
- At 10B pairs the wall-clock bottleneck was spill, export and I/O, with CPUs 75% idle. Scoring is not the only cost: candidate-pair materialisation and output volume dominate. A GPU approach that fuses blocking, scoring and threshold filtering, so that sub-threshold pairs are never materialised, would attack the real cliff (estimate).
- Spark-based stacks (Zingg, Splink on Spark) are where the hours-long runtimes and big-cluster bills are. That is the more plausible displacement target (estimate).

### Gaps
- I found no published AWS ER throughput numbers and no Tamr or Quantexa runtime or contract figures.
- There is no third-party, independent benchmark comparing Splink, Zingg and dedupe on identical data at 100M+ records. The fastLink comparison is on different data, as Linacre notes himself.

## Q3. Where it hurts

### Takeaway
The pain is real, but it is mostly quadratic blow-up from loose or skewed blocking keys, plus clustering and memory failures at 45M–100M+ records. Raw similarity-function speed is not the main complaint. Maintainers almost always answer with "fix your blocking rules" or "your training data is too small", not "you need more compute".

### Cited Findings
**Splink: documented limits**
- Splink's docs say comparisons "rise quadratically". Blocking rules "are the most important determinant of the performance". They suggest "no more than about 20 million comparisons" on a laptop, though Splink "can handle a billion or more … on more powerful machines". Some rules "imply the creation of trillions of record comparisons", and "If your rules are too loose, your linkage job may fail". One example: 10,000 "London,J" records yield 49,995,000 comparisons — [Splink blocking tutorial](https://moj-analytical-services.github.io/splink/demos/tutorials/03_Blocking.html) (accessed 2026-10-29)
- "The primary driver of run time is the number of record pairs". The second driver is comparison complexity (number of levels, costly functions, TF adjustments). "On large datasets, several of the model-training and blocking-analysis steps can be slow to run exactly" — [Splink drivers of performance](https://moj-analytical-services.github.io/splink/topic_guides/performance/drivers_of_performance.html) (accessed 2026-10-29)

**Splink: user complaints**
- An 80k-record dedupe took 128 min on an M3 Pro. RobinL diagnosed `block_on("profession")` and `block_on("nationality")` as "creating too many comparisons". A second user, linking a 7.6M-row dataset on 8 cores/32 GB, could not run it in DuckDB without chunking, and some chunks hung in training — [#2638, Hamza55-22, 2025-02-26](https://github.com/moj-analytical-services/splink/discussions/2638) (accessed 2026-10-29)
- Linking 5M against 3M records hit an out-of-memory failure locally. Chunking lost matches across chunk boundaries — [#2573, SR-splink, 2024-12-30](https://github.com/moj-analytical-services/splink/discussions/2573) (accessed 2026-10-29)
- A 45M input produced 450M predicted edges. Clustering failed with Spark OOM on 40× n2d-highmem-64 workers (64 vCPU / 512 GB each). RobinL advised clustering in DuckDB instead — [#2876, vbvsahai, 2026-01-13](https://github.com/moj-analytical-services/splink/discussions/2876) (accessed 2026-10-29)
- A user with a 500M-row daily dedupe asked whether they could train on 1M rows. RobinL replied "500 million is a pretty big linkage … not theoretically valid to train on a sample" — [#2173, guylissak, 2024-05-10](https://github.com/moj-analytical-services/splink/discussions/2173) (accessed 2026-10-29)
- Companies House data has "mega-blocks" at formation-agent and virtual-office addresses, which "Cause out-of-memory errors" during EM. RobinL said making EM runtime "less sensitive to the blocking rule chosen is … an important feature" for Splink 5 — [#2886, domvwt, 2026-01-15](https://github.com/moj-analytical-services/splink/issues/2886) (accessed 2026-10-29)
- There is a feature request for a max block size to drop "John Smith blocks". As of 2026-07-09 the recommended workaround is a pre-computed boolean column — [#2732, RobinL, 2025-06-30](https://github.com/moj-analytical-services/splink/issues/2732) (accessed 2026-10-29)
- Other complaints:
  - [#2707 ".predict() Very Slow and Inconsistent on ~200k Records with Spark", akash-revsure, 2025-06-03](https://github.com/moj-analytical-services/splink/discussions/2707)
  - [#2666 negative total comparisons in cartesian product (int overflow), awt2118, 2025-04-02](https://github.com/moj-analytical-services/splink/issues/2666)
  - [#3066 reducing memory footprint / materialise_blocked_pairs, gringer, 2026-05-11](https://github.com/moj-analytical-services/splink/discussions/3066)
  - [#2752 DuckDB not releasing RAM, mattmarotta, 2025-08-13](https://github.com/moj-analytical-services/splink/discussions/2752)
  - [#1444 find_matches_to_new_records slow on Spark, richard-a-lott, 2023-07-17](https://github.com/moj-analytical-services/splink/discussions/1444)
  - [#1355 memory guidelines on EMR, jkginfinite, 2023-06-22](https://github.com/moj-analytical-services/splink/discussions/1355)
  - [#2539 Optimise speed of training u, RobinL, 2024-12-05](https://github.com/moj-analytical-services/splink/issues/2539)
  - (all accessed 2026-10-29)

**Zingg**
- With 46M rows (about 16M customers), the match phase got stuck on shuffle writes; jobs over about 78k records took 1–2 h instead of 5 min. The user said 40M would be "prohibitively expensive". The fix was reducing numPartitions — [zingg #688, KilianLissSMRTR, 2023-10-13](https://github.com/zinggAI/zingg/issues/688) (accessed 2026-10-29)
- An 88M-record run failed with "No space left on device" on 2–4-node clusters with 5–7 TB of disk per node. The maintainer said 64 matches and 78 non-matches of training data "would be too less to run such a high load" — [zingg #1164, robertohonores, 2025-05-30](https://github.com/zinggAI/zingg/issues/1164) (accessed 2026-10-29)
- A 30M-record run stalled in ConnectedComponents for 37 h because null names formed a giant cluster — [zingg #1339](https://github.com/zinggAI/zingg/issues/1339) (accessed 2026-10-29)

**dedupe**
- 122M rows (100 GB in MySQL) ran out of memory "while creating an inverted index" on a 30 GB AWS instance — [dedupe #390, lminer, 2015-07-02](https://github.com/dedupeio/dedupe/issues/390) (accessed 2026-10-29)
- Other scale threads:
  - [#749 ">200 million records", Sushant20, 2019-05-30](https://github.com/dedupeio/dedupe/issues/749)
  - [#1024 "core and memory benchmark for bigdata", sarbaniAi, 2022-05-20](https://github.com/dedupeio/dedupe/issues/1024)
  - [#965 disk-at-capacity with >500 GB free, zwarshavsky, 2022-02-18](https://github.com/dedupeio/dedupe/issues/965)
  - [#529 MemoryError, 34 comments, tendres, 2017-03-13](https://github.com/dedupeio/dedupe/issues/529)
  - [#1094 RecordLink inference too slow, QQSkill, 2022-09-08](https://github.com/dedupeio/dedupe/issues/1094)
  - (all accessed 2026-10-29)

### Inferences
- The failure points recur across all three tools:
  - quadratic pair blow-up in skewed blocks
  - graph clustering (connected components) at hundreds of millions of edges
  - spill, disk and shuffle volume

  Per-pair string-similarity CPU time is rarely the stated complaint (estimate, from the threads above).
- A GPU SQL engine's fuzzy-join value would need to come from either (a) making looser blocking or brute-force candidate generation affordable, which recovers recall lost to tight blocking (ONS rewrote every rule producing >10M comparisons), or (b) fast connected components on GPU. Faster Jaro-Winkler alone is not enough (estimate).

### Gaps
- I did not search HN threads on Splink (no web search budget).
- dedupe.io, the hosted SaaS, is not covered; I found no current pricing or status.

## Q4. Requests for GPU acceleration

### Takeaway
There are none. GraphQL searches of issues and discussions for "GPU", "cuda", "rapids" and "cudf" in Splink, Zingg and dedupe returned zero GPU requests. The incumbents' own roadmap is "single big CPU box + DuckDB".

### Cited Findings
- GitHub GraphQL search, `repo:<r> GPU|cuda|rapids|cudf` over ISSUE (issues and PRs) and DISCUSSION for moj-analytical-services/splink, zinggAI/zingg and dedupeio/dedupe, returned zero results, except one false positive matching "rapids": [Splink discussion #461 "Blocking with similarity functions", ivsanro1, 2022-05-12](https://github.com/moj-analytical-services/splink/discussions/461). That discussion is about fuzzy blocking, not GPUs (accessed 2026-10-29).
- The closest adjacent request is embedding-based similarity (cosine over array columns, for product titles), implemented through custom SQL. RobinL: "would be amazing if Splink could leverage embeddings effectively" — [Splink #1011, OlivierBinette, 2023-02-01, 30 comments](https://github.com/moj-analytical-services/splink/issues/1011) (accessed 2026-10-29)
- Splink's lead maintainer recommends DuckDB on a single large EC2 instance over Spark — [Linacre 2026-05-18](https://www.robinlinacre.com/optimising_duckdb_performance_large_ec2_instances/) (accessed 2026-10-29). Splink also already has a "Compare performance of duckdb with datafusion" issue: [#1063, 2023-09-12](https://github.com/moj-analytical-services/splink/issues/1063) (accessed 2026-10-29)

### Inferences
- I found no evidence that users are pulling for GPUs. Any GPU pitch would have to be vendor-led. The cheapest entry is a Splink-compatible SQL dialect, so Splink users get the GPU path for free, as they did for DuckDB (estimate).
- Embedding and vector similarity for fuzzy blocking is the one compute-heavy direction users ask about (#1011, #461). It maps naturally onto GPUs (estimate).

### Gaps
- I did not search RAPIDS, cuDF or NVIDIA forums for ER requests. I could not run a GitHub-wide repo search for GPU ER projects (the queries returned empty or errored).

## Q5. Typical buyer, and whether the bottleneck is compute or workflow

### Takeaway
The observed users are government statistics and justice bodies (census, MPI, Companies House), CRM/customer dedup (46M–500M rows), and B2B name matching. Most cited friction is workflow: choosing blocking rules, scarce labels, data quality (nulls, mega-blocks). Compute becomes the binding constraint only at hundreds of millions of records or billions of pairs, and even there the limit is I/O and memory rather than FLOPs.

### Cited Findings
- **Workflow and labelling**:
  - Zingg's maintainer attributes 88M-record failures to too little training data ("at least 5 times") — [zingg #1164](https://github.com/zinggAI/zingg/issues/1164).
  - A Zingg 30M run trained on only 52+52 labelled pairs reached about 31% recall — [zingg #1339](https://github.com/zinggAI/zingg/issues/1339).
  - ONS needed a 3.2M-record clerical-review gold standard — [ONS case study](https://github.com/Data-Linkage/Splink-census-linkage/blob/main/SplinkCaseStudy.pdf).
  - (all accessed 2026-10-29)
- **Data quality**:
  - Null names caused a 37 h clustering stall — [zingg #1339](https://github.com/zinggAI/zingg/issues/1339).
  - Formation-agent addresses create mega-blocks — [Splink #2886](https://github.com/moj-analytical-services/splink/issues/2886).
  - Same names with different DOB are matched wrongly — [Splink #2573](https://github.com/moj-analytical-services/splink/discussions/2573).
  - (all accessed 2026-10-29)
- **Buyer examples**:
  - CRM: 46M customer rows replacing a legacy process — [zingg #688](https://github.com/zinggAI/zingg/issues/688); about 200M customer records on Azure, with a cost estimate needed "for approval" — [zingg #446](https://github.com/zinggAI/zingg/issues/446).
  - Name matching: 85k external vs 300k internal names on Databricks — [zingg #1177, ojasch3, 2025-07-09](https://github.com/zinggAI/zingg/issues/1177).
  - Health: MPI matching — [Splink #2573](https://github.com/moj-analytical-services/splink/discussions/2573).
  - Zingg's customer logos are retail, events and sports leagues — [zingg.ai](https://www.zingg.ai/).
  - (all accessed 2026-10-29)
- **Compute**: Splink's 7M job costs under $1. The 1B-row job runs on one EC2 box, with I/O and spill as the cliff — [fast_deduplication](https://www.robinlinacre.com/fast_deduplication/), [#3263](https://github.com/moj-analytical-services/splink/discussions/3263) (accessed 2026-10-29)

### Inferences
- In the 10M–100M range, compute is cheap with the incumbents (Splink on DuckDB), so a GPU engine wins little there. In the 100M–1B+ range, and for repeated or incremental runs, KYC/AML-style low-latency screening, or brute-force/embedding candidate generation, GPU throughput could matter. Even there the buyer's main pain stays labelling and tuning (estimate).
- Commercial pricing (AWS $0.25/1k records; Senzing about $1.5M/yr at 1B) is charged for the whole workflow: entity model, incremental IDs, explainability. That is not raw join speed. A GPU SQL engine offering fuzzy joins would compete on the OSS and DIY side, not with Senzing or Quantexa (estimate).

### Gaps
- KYC/AML sanctions screening (e.g. latency and volume of name screening) and product-catalogue matching were not covered by any source I could reach without web search.
- I found no buyer survey quantifying compute versus labour cost in ER projects.
