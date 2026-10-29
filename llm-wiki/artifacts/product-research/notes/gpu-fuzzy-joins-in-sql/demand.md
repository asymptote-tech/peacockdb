# GPU fuzzy joins in SQL — first-person demand evidence

All URLs accessed 2026-10-29. Window: 2023-10 to 2026-10.

Method: arctic-shift Reddit API (posts across r/dataengineering, r/databricks; the other subreddits returned mostly HTTP 429 "Too many requests" or timeouts, so r/datascience, r/CUDA, r/Python, r/learnpython, r/statistics, r/SQL, r/snowflake coverage is **incomplete**). HN Algolia API (8 queries plus comment trees of 11 threads). GitHub search API (note: `rapidsai/cudf` is now `NVIDIA/cudf`, and searching under the old name fails silently with a 422). Splink GitHub Discussions via GraphQL. Web search for Stack Overflow, the industry side and vendors.

## Q1: Who are the named first-person posters with scale or speed pain, or asking for GPU?

### Takeaway
I found about 6 credible first-person posts beyond the two already known. Only 3 explicitly ask for GPU: the two known ones plus two cudf GitHub issues. **One new finding:** the 2024-10 NVIDIA-forum asker appears to be the same person as GitHub user teskuteyi, who filed two cudf issues. That makes it one recurring requester, not two. The rest complain about CPU scale (10M to 1B rows) without mentioning GPU.

### Cited Findings

**Explicit GPU asks**
- **u/axwinters (Alex Winters), r/CUDA, 2026-07-16.** He works at a social science research institute and ported its record-linkage library to RAPIDS: cuDF for blocking, CuPy for Fellegi-Sunter, and a hand-written Numba Jaro-Winkler kernel. He says "the one thing RAPIDS doesn't have is a string-similarity primitive". Scale: 1M × 1M records, about 20B candidate pairs after blocking, 7 comparison columns. The job "historically took ~14 hours on a distributed CPU platform; it's now ~4 minutes on my 4070", at about 84M pairs/sec. He also notes that "at 10M records you're back to needing smarter blocking". — [r/CUDA post](https://www.reddit.com/r/CUDA/comments/1uy5az7/) (accessed 2026-10-29); repo [github.com/ajwinters/crosswalk](https://github.com/ajwinters/crosswalk) (accessed 2026-10-29)
- **teskuteyi, NVIDIA/cudf #17184 "String Similarity Library/ Algorithm", 2024-10-25.** Quote: "trying to be able to run this anonymized script using cuDF or cuDF.pandas with GPU acceleration. currently runs for over 5 hours for 2Million rows of data". Uses: pandas + RapidFuzz `process.extract` with `partial_ratio`. NVIDIA maintainers (bdice, davidwendt, VibhuJawa) pointed to `edit_distance` and `jaccard_index`. On 2025-05-11 the user then reported "memory errors when it comes to the cartesian product even with Dask-cuDF". The issue was closed. — [cudf #17184](https://github.com/NVIDIA/cudf/issues/17184) (accessed 2026-10-29)
- **teskuteyi again, NVIDIA/cudf #18274 "[FEA] RapidFuzz/ Fuzzy Matching out of the box functionality", 2025-03-14.** Quote: "I wish I could use cuDF to do fuzzy matching just like RapidFuzz but with Jaro-WInkler, Jaccard, Levenshtein…". Still open, with 0 comments. — [cudf #18274](https://github.com/NVIDIA/cudf/issues/18274) (accessed 2026-10-29)
- **ynandal99, comment on NVIDIA/cudf #6503 (the Jaro-Winkler FEA, open since 2020), 2024-07-30.** Quote: "was wondering if you ended up doing any workaround for JW similarity in cudf? It'd be cool to have this distance in cudf". No scale stated. — [cudf #6503](https://github.com/NVIDIA/cudf/issues/6503) (accessed 2026-10-29)
- **dangraysf, NVIDIA/cudf #15532, 2024-04-15.** Uses `str.edit_distance_matrix` with dask_cudf on amino-acid strings, not names, and hits a CUDA max-string error at about 100k rows. His workaround loops over 250k sequences. This shows the existing all-pairs primitive breaks at modest scale. It is adjacent to the topic: bio sequences, not ER. — [cudf #15532](https://github.com/NVIDIA/cudf/issues/15532) (accessed 2026-10-29)

**CPU scale pain, no GPU mention**
- **vfrank66, Splink Discussion #2209 "Performance between Spark and Duckdb", 2024-06-13.**
  - Scale: an 11M-record dedupe "Training and prediction occurs in 3 hours" on AWS EMR (c6.8xlarge master, plus 3× c6gd.8xlarge and 5× c6gd.12xlarge core nodes).
  - On DuckDB, an x2idn.32xlarge with 1.2 TB of RAM ran out of memory after 2 hours, even on a 500k subset. He had "started at 16 mil records but that was not going to run successfully".
  - The data includes an SSN column, which suggests US finance or health.
  - Splink's author RobinL replied that this "may imply the current recommendation to use DuckDB isn't quite right".
  - — [splink discussion #2209](https://github.com/moj-analytical-services/splink/discussions/2209) (accessed 2026-10-29)
- **meharc, Splink Discussion #2119 "predict() is super slow even for a small dataset!", 2024-04-01.** Links 2 × 1.5M-row datasets in PySpark on 128 cores. Training blocking produces "500 million comparisons, which cannot be further reduced". RobinL diagnosed the slow predict as Spark misconfiguration. — [splink discussion #2119](https://github.com/moj-analytical-services/splink/discussions/2119) (accessed 2026-10-29)
- **flightster, Ask HN "What's the best way to join on numerous fields with fuzzy matching?", 2024-06-26.** Customer lookup over name, address, phone, email, DOB and loan number. Quote: "the data is in the 100m-1000m rows size, so this has to run reasonably quickly… Would like to pre-load the big table in RAM… is there some kind of good database already pre-optimized for this kind of fuzzy lookup?" The thread got 1 reply (Placekey). — [HN 40798668](https://news.ycombinator.com/item?id=40798668) (accessed 2026-10-29)
- **u/incremental_load, r/dataengineering, 2025-05-17.** Detects name and address changes in a 10M-record historical US provider extract against an Excel update that arrives by email. Asks for "the most efficient and scalable approach" in Pandas, PySpark or SQL. — [reddit 1kp0ldl](https://www.reddit.com/r/dataengineering/comments/1kp0ldl/) (accessed 2026-10-29)
- **u/vroemboem, r/databricks, 2026-04-25.** Builds a company and shareholder data product. Initial load is about 10M company and people rows plus 50M PDF pages, with about 10k new rows a day. Runs entity resolution in DuckDB over Postgres (260 GB in total). Quote: "doing initial loads can take many hours". — [reddit 1svfzbt](https://www.reddit.com/r/databricks/comments/1svfzbt/) (accessed 2026-10-29)
- **u/Queasy_Teaching_1809, r/dataengineering, 2025-04-10.** Customer-contact dedupe in MS SQL Server that "would need to be a task that runs daily". No scale given. — [reddit 1jw6m8p](https://www.reddit.com/r/dataengineering/comments/1jw6m8p/) (accessed 2026-10-29)
- **Alteryx Community, "fuzzy matching slooooowwwwww could use advice".** Per the search snippet, the user matched 3,300 records against 72M rows and it was still running after about a day. I did not verify the post date or handle, so it may fall outside the window. — [Alteryx community](https://community.alteryx.com/discussion/834176/fuzzy-matching-slooooowwwwww-could-use-advice) (accessed 2026-10-29)
- **Stack Overflow "how to apply entity name matching rapidfuzz at scale" (q 75673229).** Matches 150k records against 8M. The suggested answer was LSH. The question predates 2023-11 (it appears in a 2023-11 dump), so it is likely before the window. — [SO mirror](https://kiwix.lema.org/stackoverflow.com_en_all_2023-11/questions/75673229/how-to-apply-entity-name-matching-rapidfuzz-at-scale) (accessed 2026-10-29)

**Checks that came back empty or thin**
- Splink GitHub issues: 0 hits each for "gpu" and "cuda" (GitHub search, `repo:moj-analytical-services/splink`). This confirms the earlier pass. — [search](https://github.com/moj-analytical-services/splink/issues?q=gpu) (accessed 2026-10-29)
- zinggAI/zingg and dedupeio/dedupe: 0 issues mentioning "gpu". — [zingg search](https://github.com/zinggAI/zingg/issues?q=gpu), [dedupe search](https://github.com/dedupeio/dedupe/issues?q=gpu) (accessed 2026-10-29)
- RapidFuzz: 1 hit each for "gpu" and "cuda", both incidental and from 2021–22. There is no GPU feature request. — [RapidFuzz issues](https://github.com/rapidfuzz/RapidFuzz/issues?q=gpu) (accessed 2026-10-29)
- HN, 8 queries since 2023-10: Splink launch threads (Splink 4 in 2024-07, Splink 5 in 2026-09, and others) drew almost no comments, and none raised GPU or scale complaints. — [Splink 5 thread](https://news.ycombinator.com/item?id=49891072), [Splink 4 thread](https://news.ycombinator.com/item?id=41055367) (accessed 2026-10-29)

**Supply side, not demand (do not count as demand)**
- ryantwolf (NVIDIA) promoted NeMo Curator GPU fuzzy dedup (MinHash, for LLM corpora), 2024-07-05. — [HN 40884295](https://news.ycombinator.com/item?id=40884295) (accessed 2026-10-29)
- HyperBlocker, a GPU rule-based ER blocking system (academic). It claims to be at least 6.8× faster than CPU distributed systems and 9.1× faster than GPU ER solvers. It was posted to HN on 2025-03-10 with no discussion. — [arXiv 2410.04349](https://arxiv.org/abs/2410.04349), [HN 43326745](https://news.ycombinator.com/item?id=43326745) (accessed 2026-10-29)
- Fast-ER (Caltech, CuPy-based) claims more than 35× over the leading CPU implementation. A JORS paper appeared in 2026. — [fast-er docs](https://fast-er.readthedocs.io) (accessed 2026-10-29)
- A vendor fuzzy-match API post on dev.to says "By the time I hit 100,000 rows, RapidFuzz was taking over 20 minutes". It is marketing. — [dev.to](https://dev.to/siyana_hristova_900e581ee/i-built-a-fuzzy-matching-engine-thats-300x-faster-than-rapidfuzz-on-1m-records-1o00) (accessed 2026-10-29)
- u/_bsc_, "Would you use an API for large-scale fuzzy matching / dedupe?", r/dataengineering, 2025-11-23. The post was removed. — [reddit 1p4ncud](https://www.reddit.com/r/dataengineering/comments/1p4ncud/) (accessed 2026-10-29)
- Other posters are tool makers, not buyers. u/sonalg (Zingg founder) appears repeatedly in r/dataengineering and r/databricks, and u/RobinL is the Splink author. — [reddit 1sdarme](https://www.reddit.com/r/dataengineering/comments/1sdarme/) (accessed 2026-10-29)

### Inferences
- The cudf "string similarity primitive" gap is the one concrete, repeated technical ask. Three independent people raised it: teskuteyi (on the NVIDIA forum and twice on GitHub), axwinters, and ynandal99. FEA #6503 has been open for 6 years and #18274 has zero maintainer response. NVIDIA steers users toward `jaccard_index` rather than JW or Levenshtein joins.
- The forum post and the cudf issues share the same 2024-10 timing and the same RapidFuzz `fuzzy_merge` pattern, so they are very likely one person (teskuteyi). The two "known posts" therefore count as one known person plus axwinters.
- Pain at the 10M+ scale shows up as cluster cost and wall-clock time (vfrank66, flightster, vroemboem, incremental_load). These users frame it as Spark or DuckDB tuning, or as "what database is built for this". None of them mention GPU. Demand is latent, not articulated.
- Even GPU advocates such as axwinters say blocking, not raw speed, dominates at 10M records. A GPU fuzzy join therefore needs candidate generation (blocking or LSH) built in, not just a fast all-pairs kernel.

### Gaps
- Reddit coverage outside r/dataengineering and r/databricks failed because arctic-shift returned 429s and timeouts. r/datascience, r/Python, r/learnpython and r/statistics are unsearched. Comment-level Reddit search also failed.
- Stack Overflow results from the window: I found nothing dated 2023-10 or later with a GPU ask. The SO results I found are pre-window mirrors.
- I could not verify the date or handle of the Alteryx 72M-row thread.

## Q2: What does the industry side show (KYC/AML screening, MDM, company blogs moving ER to GPU)?

### Takeaway
I found no first-person company blog moving ER or record linkage to GPU, whether from a fintech, a census bureau or a health linkage unit. Sanctions-screening discussion centres on accuracy and LLM latency, not on GPU fuzzy-match throughput.

### Cited Findings
- A Federal Reserve FEDS working paper (2025-092) on sanctions screening proposes tiered fuzzy-match → LLM escalation. It says inference latency and API limits push high-throughput uses such as payments toward tiered approaches. The paper does not discuss GPU fuzzy matching. — [Federal Reserve FEDS 2025-092](https://www.federalreserve.gov/econres/feds/can-llms-improve-sanctions-screening-in-the-financial-system-evidence-from-a-fuzzy-matching-assessment.htm) (accessed 2026-10-29)
- A UK Ministry of Justice blog says its production Splink datasets range from tens to hundreds of millions of records. It frames the work as CPU (DuckDB or Spark), with no GPU. — [Running Splink in Production at MoJ](https://moj-analytical-services.github.io/splink/blog/2026/01/29/running-splink-in-production.html) (accessed 2026-10-29)
- Splink's own benchmark reports deduplicating 7M records in just over 2 minutes for under $1 on EC2 with DuckDB. This weakens the GPU cost case at that scale. — [fast_deduplication](https://robinl.github.io/fast_deduplication/) (accessed 2026-10-29)
- An HN thread on name matching in sanctions screening (2025-07, 2025-08) is framed around accuracy, not speed. — [HN 44433978](https://news.ycombinator.com/item?id=44433978) (accessed 2026-10-29)

### Inferences
- Large-scale CPU ER users (MoJ, and vfrank66's EMR cluster) are where cost pain plausibly sits. The best-known tool's author publishes "cheap on CPU" benchmarks, though, which undercuts the GPU pitch below about 10M records.

### Gaps
- I found no AML or KYC vendor complaining publicly about fuzzy-match throughput, and no MDM practitioner forum evidence (Informatica or Reltio communities were not searched).
- I found no census bureau or health data linkage unit GPU write-up in the window.

## Q3: Which posters are possible design partners (budget, recurring job, ≥10M records)?

### Takeaway
Two or three are plausible, and none is strong. The best fits are vfrank66 (11M records, a costly EMR cluster) and flightster (100M–1B rows, lending, an online lookup). After them come axwinters (a research institute that has already built its own GPU tool, so a peer or competitor rather than a buyer) and vroemboem (10M rows, a solo founder with little budget).

### Cited Findings
- **vfrank66:**
  - Scale: 11–16M records.
  - Budget: an 8-node EMR cluster and attempts on an x2idn.32xlarge.
  - Whether the job is recurring: not stated.
  - Fit: strongest on scale and budget.
  - — [splink #2209](https://github.com/moj-analytical-services/splink/discussions/2209) (accessed 2026-10-29)
- **flightster:**
  - Scale: 100M–1B rows.
  - Workload: customer lookup with loan numbers, so financial services. It is an online API, which is more a fuzzy-index lookup than a batch join.
  - Fit: strongest on scale. The handle is anonymous and the post is from 2024.
  - — [HN 40798668](https://news.ycombinator.com/item?id=40798668) (accessed 2026-10-29)
- **axwinters:**
  - Scale: 1M×1M records, recurring administrative-data linkage at Penn State.
  - Fit: has already solved the problem in-house with a GPU. Better as a reference user, benchmark collaborator or contributor than as a buyer.
  - — [r/CUDA](https://www.reddit.com/r/CUDA/comments/1uy5az7/) (accessed 2026-10-29)
- **teskuteyi:**
  - Scale: 2M rows, 5-hour RapidFuzz runs.
  - Fit: a persistent requester (3 posts over 2024–25) and already a GPU user, but the scale is below 10M.
  - — [cudf #17184](https://github.com/NVIDIA/cudf/issues/17184), [cudf #18274](https://github.com/NVIDIA/cudf/issues/18274) (accessed 2026-10-29)
- **vroemboem:**
  - Scale: 10M rows.
  - Workload: daily incremental, a startup-style data product.
  - Fit: initial loads take hours, but the budget is likely small.
  - — [reddit 1svfzbt](https://www.reddit.com/r/databricks/comments/1svfzbt/) (accessed 2026-10-29)
- **incremental_load:**
  - Scale: 10M records.
  - Workload: recurring (extracts arrive by email). The tooling is unsophisticated and the user did not ask about GPU.
  - — [reddit 1kp0ldl](https://www.reddit.com/r/dataengineering/comments/1kp0ldl/) (accessed 2026-10-29)

### Inferences
- **Honest bottom line:** explicit GPU fuzzy-join demand is thin. Over 3 years I found about 4 distinct people asking for GPU (teskuteyi, axwinters, ynandal99, dangraysf), plus about 6 CPU-scale complainers at 10M+ records. Buyers with budget do not voice it in public. They tune Spark or DuckDB, or they buy MDM or screening vendors.

### Gaps
- None of the posters disclosed an employer or budget, apart from axwinters (Penn State). Design-partner fit is inferred, not confirmed.
