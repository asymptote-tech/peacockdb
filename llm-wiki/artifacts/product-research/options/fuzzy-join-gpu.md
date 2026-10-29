# fuzzy-join-gpu — GPU similarity joins in SQL for entity resolution, record linkage and dedup

## Verdict

- **What it is:** GPU SQL with Jaro-Winkler, Levenshtein, Jaccard and MinHash functions. It adds a fused "blocking join → score → threshold" operator that never writes the full pair set, plus connected components for clustering. It is sold first as a faster backend for Splink.
- **Who needs it:** demand.md rates it **thin** [D]. This pass found about 4 people who ask for GPU string similarity and about 6 who complain about CPU scale at 10M+ records. None of them has a stated budget. Splink, Zingg and dedupe have zero GPU issues [DM][IN].
- **Budget:** fits easily. About 0.8× engine on peacockdb and 0.45× on Sirius (estimate). The work is GPU-shaped, but a single CPU box already runs 1B records in under 10 minutes of scoring [IN].

## Who needs it badly

- **Rating: thin, taken from demand.md `## Ratings`, row `fuzzy-join-gpu`** [D]: "2 posters … Strong per-user gain (14 h → 4 min), but the users wrote it themselves. Splink has zero GPU issues."
- New evidence (it adds to the rating and does not change it). Every link was accessed 2026-10-29 [DM]:
  - **The two known posts are one person plus Alex Winters.** The 2024-10 NVIDIA forum ask matches GitHub user **teskuteyi**: same month, same RapidFuzz `fuzzy_merge` script.
    - On [cudf #17184](https://github.com/NVIDIA/cudf/issues/17184) (2024-10-25) he wrote: "currently runs for over 5 hours for 2Million rows". Maintainers pointed him to `edit_distance` and `jaccard_index`.
    - In 2025-05 he reported "memory errors when it comes to the cartesian product even with Dask-cuDF".
    - He then filed [cudf #18274](https://github.com/NVIDIA/cudf/issues/18274) (2025-03-14), asking for RapidFuzz-style Jaro-Winkler, Jaccard and Levenshtein. It is open with 0 comments.
  - **ynandal99** asked for Jaro-Winkler in cuDF on [cudf #6503](https://github.com/NVIDIA/cudf/issues/6503) (2024-07-30). That feature request has been open since 2020-10-12 with no PR [GF].
  - **dangraysf** found that cuDF's all-pairs `edit_distance_matrix` broke at about 100k strings ([cudf #15532](https://github.com/NVIDIA/cudf/issues/15532), 2024-04). The strings were amino acids, so this is adjacent to entity resolution, not part of it.
  - **Alex Winters** (u/axwinters, Penn State), [r/CUDA](https://www.reddit.com/r/CUDA/comments/1uy5az7/), 2026-07-16:
    - 1M×1M records, about 20B candidate pairs, 7 comparison columns.
    - "~14 hours on a distributed CPU platform; it's now ~4 minutes on my 4070".
    - "the one thing RAPIDS doesn't have is a string-similarity primitive".
    - "at 10M records you're back to needing smarter blocking."
    - His README says the CPU figure is about 16 h, *extrapolated*: the CPU run ran out of memory needing a 320 GB pair index ([crosswalk @9b801f70](https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/README.md)) [GF].
- CPU scale pain from people who do not mention GPU [DM][IN]:
  - **vfrank66** ([Splink #2209](https://github.com/moj-analytical-services/splink/discussions/2209), 2024-06-13): an 11M-record dedupe takes 3 h on an 8-node EMR cluster. DuckDB ran out of memory on an x2idn.32xlarge with 1.2 TB of RAM. The data has an SSN column. RobinL: "may imply the current recommendation to use DuckDB isn't quite right".
  - **flightster** ([Ask HN 40798668](https://news.ycombinator.com/item?id=40798668), 2024-06-26): fuzzy customer lookup by loan number, name and address over "100m-1000m rows". He asks for "a good database already pre-optimized for this kind of fuzzy lookup". This is an online lookup, not a batch join.
  - **vbvsahai** ([Splink #2876](https://github.com/moj-analytical-services/splink/discussions/2876), 2026-01-13): 45M records gave 450M edges. Clustering ran out of memory on 40 Spark workers with 512 GB each.
  - **guylissak** ([Splink #2173](https://github.com/moj-analytical-services/splink/discussions/2173), 2024-05-10): a daily dedupe of 500M rows on Databricks.
  - **domvwt** ([Splink #2886](https://github.com/moj-analytical-services/splink/issues/2886), 2026-01-15): Companies House "mega-blocks" at formation-agent addresses cause out-of-memory failures in EM.
  - **KilianLissSMRTR** ([zingg #688](https://github.com/zinggAI/zingg/issues/688), 2023-10-13): 46M customer rows; 40M would be "prohibitively expensive".
  - **u/vroemboem** ([r/databricks](https://www.reddit.com/r/databricks/comments/1svfzbt/), 2026-04-25): 10M company and person rows on DuckDB; "initial loads can take many hours".
- **Pattern:** at 10M+ records the pain is cluster cost, out-of-memory failures and blocking-rule tuning. It is framed as a Spark or DuckDB problem, not as a lack of compute.
- **Best first conversations:**
  - Robin Linacre (`RobinL`, Splink author). He is a gatekeeper, not a buyer, and he already filed "Compare performance of duckdb with datafusion" ([Splink #1063](https://github.com/moj-analytical-services/splink/issues/1063), 2023-09-12) [IN].
  - vfrank66.
  - The UK MoJ and ONS linkage teams ([MoJ blog, 2026-01-29](https://moj-analytical-services.github.io/splink/blog/2026/01/29/running-splink-in-production.html)).
  - Alex Winters, as a benchmark peer.
  - teskuteyi.

## How it would work

- **SQL surface** (DuckDB-compatible names, so that Splink SQL runs unchanged):
  - `jaro_winkler_similarity`, `jaro_similarity`, `levenshtein`, `damerau_levenshtein`, `jaccard`, `hamming`, with an optional `score_cutoff` argument ([DuckDB text functions](https://duckdb.org/docs/2.0/sql/functions/text.html)) [GF];
  - plus `minhash64(str, k, n)` for LSH blocking;
  - plus a `connected_components(edges)` table function.
  - DataFusion 45 already plans `levenshtein`, on the CPU (`datafusion-functions-45.0.0/src/string/levenshtein.rs`, local cargo registry).
- **Query shape:**
  ```
  SELECT l.id, r.id, jaro_winkler_similarity(l.name, r.name) AS jw, ...
  FROM people l JOIN people r ON l.dob = r.dob AND l.id < r.id
  WHERE jaro_winkler_similarity(l.name, r.name) > 0.9
  ```
- **Operator:** a new fused "similarity join" node. The planner recognises it as an equi-join followed by a residual that uses similarity functions. Then:
  1. Build a hash table on the blocking key (the existing join session [I]).
  2. Probe in output-bounded chunks, so a skewed "John Smith" block cannot explode one batch.
  3. Gather both strings per pair, padded or offset-indexed.
  4. Run one GPU thread per pair: Jaro-Winkler, or bit-parallel Myers Levenshtein for strings of 64 characters or fewer.
  5. Score with Fellegi-Sunter, using a lookup of weights per gamma level, and keep only pairs above the threshold.
  6. Emit the survivors.
  7. Several blocking rules form a UNION of joins with a dedup on (l.id, r.id).
- **LSH blocking:** `minhash64` → explode the bands → equi-join on (band, bucket). This is the NeMo Curator pattern on cuDF [GF].
- **Clustering:** connected components over the surviving edges, via cuGraph WCC (estimate).
- **Data path end to end:**
  1. Parquet holds the records: 10M–1B rows at about 100–200 B of match columns each (estimate).
  2. Load them once into VRAM, or into pinned host memory beyond VRAM.
  3. Blocking join and scoring stay on the device.
  4. Filtered pairs, 1–10% of candidates (estimate), go back device-to-host.
  5. Write them to Parquet.
- **Front door:** a Splink `DatabaseAPI` backend (Python) that sends Splink's generated SQL to peacockdb. On Sirius, Splink's existing DuckDB backend is the front door.

## The human's questions

- **Who does this today, and with what?**
  - **Splink** (UK MoJ) is the reference open-source tool. It is Fellegi-Sunter compiled to SQL for DuckDB, Spark, Athena or Postgres. It has had over 15M downloads with a team of "averaged two people" ([Linacre 2026-02-24](https://www.robinlinacre.com/splink_or_swim/)). Splink 5.0.0 shipped 2026-09-18 (repo @5b719296) [IN].
    - Users include ONS (the 2021 Census, about 58M records), MoJ, Companies House linkers and builders of master patient indexes [IN].
  - **Zingg:** AGPL, runs on Spark or Snowflake, and has an enterprise edition. Named customers are Fortnum & Mason, the CFL and others. It publishes no prices (repo @9d1a7f49) [IN].
  - **dedupe:** Python with active learning. It scales through Postgres; the largest user report is about 60M records (dedupe #749) [IN].
  - **AWS Entity Resolution:** $0.25 per 1,000 records processed, charged even when nothing matches. ML matching has no incremental mode ([pricing](https://aws.amazon.com/entity-resolution/pricing/)) [IN].
  - **Senzing:** $58,560/yr for 10M records and $1,497,000/yr for 1B ([pricing](https://senzing.com/pricing/)) [IN].
  - **Quantexa and Tamr:** this pass found no prices or contract sizes. tamr.com did not resolve. Treat their figures as unknown.
  - **Databricks:** Arc is "built on the open source project Splink" ([blog 2023-08-04](https://www.databricks.com/blog/2023/08/04/improving-public-sector-decision-making-simple-automated-record-linking.html)). `databrickslabs/arc` now returns 404 [IN]. No native Databricks ER product was found.
  - **rapidfuzz:** CPU, about 4.2k stars, with no GPU feature request in its tracker [IN][DM].
  - **cuDF:** nvtext has row-wise `edit_distance`, `jaccard_index(width)`, `minhash`/`minhash64` (permuted), normalisation and tokenisation. It has no Jaro or Jaro-Winkler, no all-pairs matrix at HEAD and no similarity join (cudf @785ee522, `cpp/include/nvtext/`) [GF].
  - **NVIDIA's own entity resolution:** the productised path is NeMo Curator fuzzy dedup of documents: MinHash over 24-character n-grams, LSH with 20×13 bands, connected components on RAFT ([docs](https://docs.nvidia.com/nemo/curator/latest/curate-text/process-data/deduplication/fuzzy.html)) [GF]. No NVIDIA record-linkage or Fellegi-Sunter example was found, and no Senzing or Quantexa partnership was verified.
  - **Other GPU tools:** Fast-ER (Caltech, CuPy, Jaro-Winkler) claims "over 35×" faster than the leading CPU tool, with no hardware stated ([docs](https://fast-er.readthedocs.io/en/latest/)). crosswalk is Winters' tool. HyperBlocker is an academic GPU blocking system claiming ≥6.8× ([arXiv 2410.04349](https://arxiv.org/abs/2410.04349)) [GF][DM].
- **What does it cost, and where does it hurt?**
  - On the CPU it is cheap:
    - 7M records and 1B comparisons took "just over 2 minutes" for under $1 on EC2 with DuckDB ([fast_deduplication](https://www.robinlinacre.com/fast_deduplication/)) [IN].
    - Splink 5 at 1B records and 10.04B scored pairs: `predict()` took 478–550 s on one 192-vCPU r8id.48xlarge ([Splink #3263](https://github.com/moj-analytical-services/splink/discussions/3263), 2026-09-02) [IN].
  - **Where it hurts:**
    - *Blocking-rule explosion.* Comparisons "rise quadratically". Some rules imply "trillions of record comparisons", and on a laptop the advice is to stay under about 20M comparisons ([Splink blocking docs](https://moj-analytical-services.github.io/splink/demos/tutorials/03_Blocking.html)). ONS rewrote every rule producing more than 10M comparisons; its 3B pairs took about 4 h [IN].
    - *Output and spill.* The 1B run held 556 GiB in memory and spilled about 1.5 TB. Parquet export then ran about 75× slower, with 75% of CPUs idle [IN].
    - *Clustering.* 450M edges ran out of memory on 40 Spark workers (#2876). Zingg sat 37 h in ConnectedComponents on a 30M-record run because null names formed one giant cluster ([zingg #1339](https://github.com/zinggAI/zingg/issues/1339)) [IN].
    - *Spark stacks.* Zingg at 70M records took 7 h on 128 cores ([zingg #446](https://github.com/zinggAI/zingg/issues/446)). Splink on Spark predicts 8.7× slower than on DuckDB [IN].
    - *Labels and data quality.* Maintainers mostly answer "fix your blocking" or "add training data" (zingg #1164) [IN].
  - **Managed pricing is where the money is** (derived, estimate): AWS ER costs $250k per pass over 1B records, against a few dollars of EC2 for Splink. But AWS and Senzing sell the whole entity model, incremental IDs and explainability, not join speed.
- **Is the work GPU-shaped?**
  - Mostly yes, per stage (estimate) [GF]:
    - **Blocking:** an equi-join that peacockdb already has. Its output size is the danger.
    - **Pairwise scoring:** compute-bound. Levenshtein at 32×32 is about 1k cell operations per pair, and Jaro-Winkler is similar. One thread per pair is ideal.
    - **Gather:** about 64–80 B per pair of random reads.
    - **Fellegi-Sunter scoring:** trivial arithmetic once fused.
    - **Connected components:** graph work, and cuGraph exists.
  - **Measured rates:**
    - crosswalk: about 8×10⁷ pairs/s end to end on a consumer 4070 Ti SUPER, using 1.4 GB of VRAM [GF].
    - Splink on DuckDB: about 8×10⁶ pairs/s on the 7M benchmark, and about 2×10⁷ pairs/s at 10B pairs on 192 vCPU [IN].
    - A fair GPU-over-CPU-box ratio is therefore about 4–10×, not the 79× crosswalk reports against Python (estimate).
  - **The risk is materialisation, not FLOPs.** Pair indices cost about 16 B per pair, so 1e10 pairs need 160 GB, more than one GPU. The operator must fuse probe, score and filter in chunks [GF].
- **What does the community evidence actually show?**
  - The rating stays thin. A wider search found about 4 distinct GPU askers in three years (teskuteyi, Winters, ynandal99, dangraysf) and about 6 CPU-scale complainers [DM].
  - The one repeated technical ask is "a string-similarity primitive in cuDF". NVIDIA has left it open for 6 years (#6503) [GF].
  - Buyers with budget (MoJ, ONS, CRM teams) do not ask for GPUs in public. They tune DuckDB or Spark, or they buy Senzing or AWS.
  - Gaps: Reddit outside r/dataengineering and r/databricks, the Discord/Slack servers and the KYC/MDM vendor forums were unreachable.

## Data path and cache

| Stage | Rate | Source |
|---|---|---|
| HBM read, H200 | 4,484 GB/s measured | [I] |
| PCIe Gen5 H2D, pinned / pageable | 47.3 / 10.6 GB/s (H200) | [I] |
| Parquet load on GPU (decode + materialise) | 6.8–8.7 GB/s per GPU | [I] |
| NVMe, 3×3.5 TB RAID0 (Splink 1B box) | ~6–10 GB/s write (estimate) | [IN] |
| NIC, 8× ConnectX-7 | 400 GB/s nominal | [LN] |
| Host RAM, 192-vCPU EC2 | 330–480 GB/s ([Linacre 2026-05-18](https://www.robinlinacre.com/optimising_duckdb_performance_large_ec2_instances/)) | [IN] |
| Scoring, GPU vs CPU box | ~8×10⁷ vs ~2×10⁷ pairs/s | [GF][IN] |

- **The thesis mostly does not hold here, because compute per byte is huge.**
  - 1B records at about 150 B is about 150 GB of input (estimate). That is about 20 s of Parquet load on one GPU, or about 3 s from pinned host memory.
  - The same input fans out to 1e10 pairs. Scoring them takes about 125 s at the crosswalk rate (estimate).
  - Load is therefore about 15% of the job, against more than 90% in the TPC-H PoC [E].
- **The new bottleneck is output.**
  - Splink keeps every scored pair by default.
  - 1e10 rows × about 50 B is about 500 GB. D2H takes about 11 s, but an NVMe write takes 50–80 s plus Parquet encoding (estimate).
  - A GPU does not fix this. A threshold applied inside the operator does, by cutting output 10–100× (estimate).
- **Cache needed:** little.
  - Input up to about 100M records (about 15 GB) fits in VRAM for the whole job, and every blocking rule re-reads it from VRAM.
  - Beyond VRAM, the input needs a pinned-host tier, because each of 6–42 blocking rules re-probes the same columns [IN].
  - There is no cross-query cache. Entity-resolution jobs are batch runs, not interactive queries.
  - Online lookups (flightster's case) would need a resident VRAM index. That is a different product.
- **Distributed:** not needed for an alpha. 1e10 pairs run on one GPU in minutes (estimate). The NIC never enters the picture.

## Build cost to alpha

Sizes are yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend, translator, hash join session, Parquet scan | reuse [I] | reuse (DuckDB planner and joins) [S] |
| Similarity scalar functions in the planner (Jaro-Winkler, Jaro, Levenshtein, Damerau, Jaccard, Hamming, cutoff) | adapt: DataFusion has `levenshtein`; register the rest as UDFs and translate them: 0.5–1k | reuse: DuckDB has them all [GF] |
| GPU kernels: Jaro-Winkler, bit-parallel Levenshtein, Damerau; Jaccard via nvtext | new: 1.5–3k | new: 1.5–3k (Sirius has no similarity code @b9e05ffc [GF]) |
| Fused, chunked "probe → gather → score → threshold" operator, with skew-bounded output | adapt the join session (1,036 lines) + node + wire: 3–5k | adapt the join operators: 2.5–4k |
| MinHash/LSH blocking (band explode, unnest) | adapt: nvtext `minhash64` + a new explode node: 1–2k | adapt: 1–1.5k |
| Connected components (cuGraph WCC) as a table function | new: 1–2k + cuGraph dependency | new: 1–2k |
| Fellegi-Sunter EM and u-estimation queries (aggregates over gamma levels) | reuse: plain GROUP BY [I] | reuse |
| Front door: Splink backend + GPU entry point | new: Splink `DatabaseAPI` (Python, outside yardstick) + Rust entry point: 1.5–2.5k [I] | reuse: Splink's DuckDB backend works as is |
| SQL gaps Splink hits (window functions #143, CTE shapes, list functions) | new: 2–4k (estimate; unverified which Splink queries need them) | reuse: DuckDB falls back to CPU |
| Spill and pinned-host tier for over-VRAM input and pair streams | new: 1.5–3k (no spill today [I]) | reuse: memory tiers / spill [S] |
| Parquet writer for the pair output | new: 0.5–1k | reuse (DuckDB COPY) |

- **Totals:** about 12.5–23.5k on peacockdb and about 6–11.5k on Sirius.
- **The hard part:** the fused operator under skew.
  - One mega-block (Companies House formation agents, null names) can produce more pairs than VRAM holds.
  - The operator must split a probe's output mid-block, keep the dedup across multiple blocking rules correct, and match DuckDB's Jaro-Winkler bit for bit, so that Splink's models agree.
- **Second hard part on peacockdb:** running Splink's whole generated SQL. On Sirius this comes free, but Sirius has lost to DuckDB on desk GPUs ([sirius #609](https://github.com/sirius-db/sirius/issues/609)) [D].

Total new work: 0.8× engine on peacockdb, 0.45× on Sirius (estimate)

## Adoption friction

- **Low if it hides behind Splink.**
  - Splink users already move between backends: DuckDB replaced Spark for most of them [IN].
  - A Sirius-based build needs only "install the extension". A peacockdb build needs a new Splink backend, which the Splink maintainers would have to accept or tolerate.
- **Medium otherwise.**
  - Teams on Zingg or Spark, or on RapidFuzz in pandas, must change tools.
  - Teams on AWS ER or Senzing buy a managed entity model; a SQL join does not replace it.
- **The hardware ask is awkward.** Most entity-resolution runs are monthly or one-off batches. Renting an H100 for 10 minutes is easy in the cloud but hard in government secure environments such as ONS or NHS TREs (estimate).
- **Correctness bar:** scores must match CPU Splink exactly, or users will not trust the matches. crosswalk shows this is possible: bit-exact with `jellyfish` [GF].

## Platform risk

- **NVIDIA:** low to medium.
  - NVIDIA has ignored Jaro-Winkler in cuDF for 6 years (#6503), and #18274 has no maintainer reply [GF].
  - But one nvtext PR would remove the kernel moat. NVIDIA already ships the LSH/MinHash/connected-components pipeline in NeMo Curator, for free [GF].
  - NVIDIA's strategy is to accelerate partners' engines, so a cuDF-backed Splink or Spark path is in character [N].
- **Free GPU tools already exist:** crosswalk, Fast-ER and HyperBlocker [GF][DM]. The kernel is not defensible. The defensible parts are the SQL integration, Splink compatibility and skew handling.
- **Host project:**
  - Splink's author pushes "one big CPU box + DuckDB" and publishes "under $1" benchmarks [IN]. That sets a low bar for what users will pay.
  - On Sirius, the risks are NVIDIA's control of Sirius [N] and Sirius adding string-similarity kernels itself.
- **Incumbents:** Senzing, Quantexa and AWS could add GPUs inside their managed products. None has said so (no source found).

## Funding story

- **What a design partner would say** (estimate):
  - "Our 500M-row nightly dedupe takes hours on Databricks. If it ran on Splink in 10 minutes on one GPU, with looser blocking and better recall, we'd use it." That is guylissak's or vfrank66's shape.
  - "My 7M job already costs $1" is the more common answer.
- **The pitch:** "brute-force-quality entity resolution at blocking-rule cost".
  - The GPU's 4–10× lets users loosen blocking and recover the recall that ONS and others give up by rewriting every rule over 10M comparisons [IN].
  - It ends in an open-source Splink accelerator with a paid tier for incremental, managed runs.
- **What the round would be raised on:** a benchmark against Splink-on-DuckDB at 1B records, plus one MoJ, ONS or CRM user quote. Today neither exists, and the per-record price war (AWS at $0.25/1k) is fought on the managed workflow, not on speed.
- **Weak spot:** the market argument is easy to make, but the brief says market-size arguments count for little. No named user with budget is in hand.

## Evidence against

- **CPU is already good enough** for most users. Splink on DuckDB does 7M records for under $1 and 1B records in about 9 minutes of `predict()` on one box [IN]. A GPU saves minutes on jobs that run monthly.
- **The real cliffs are not compute:**
  - output spill and I/O (75% of CPUs idle at 10B pairs);
  - clustering memory;
  - blocking-rule design;
  - labels and data quality [IN].
  - A GPU helps clustering and fused filtering, but not labelling or tuning, which maintainers name as the usual fix.
- **Nobody asks for it.** Zero GPU issues across Splink, Zingg, dedupe and RapidFuzz [IN][DM]. The GPU askers are a 2M-row pandas user and a researcher who already built his own tool. demand.md: "the users wrote it themselves" [D].
- **The kernel is commodity.** Three free GPU implementations exist, and NVIDIA could add Jaro-Winkler to nvtext in one release [GF].
- **Money sits with vendors that sell workflow** (Senzing $1.5M/yr at 1B, Quantexa, Tamr), not with join speed. A SQL operator competes with free open source.
- **Even Winters says** that at 10M records "you're back to needing smarter blocking" [DM].

## Ratings

- **Who needs it badly: weak.** demand.md rates it thin. About 4 GPU askers and about 6 CPU complainers, none with a stated budget or deadline [D][DM].
- **Build cost: strong.** 0.8× on peacockdb and 0.45× on Sirius, mostly additive. The hard part (a fused, skew-bounded operator) is contained.
- **Data path: strong.** Compute per input byte is very high, so load is about 15% of the job. Output volume is the only path issue, and an in-operator threshold fixes it.
- **Adoption friction: medium.** A Splink backend is a near-drop-in on Sirius. On peacockdb it needs Splink's acceptance and SQL-surface work. GPU access in secure government environments is hard.
- **Platform risk: medium.** NVIDIA neglects the kernel today, but free GPU entity-resolution tools exist and nvtext could close the gap in one PR.
- **Funding story: weak.** "Splink, but faster" sells against a $1 CPU job. The buyers with budget pay for workflow, not speed.

## Follow-up experiments

- Pairs per second for Jaro-Winkler and Levenshtein on an H100 versus DuckDB `jaro_winkler_similarity` on 64–192 cores, with the same strings and blocking. This settles the 4–10× estimate.
- Splink 5's 1B-record benchmark on one GPU with a fused threshold, end to end including the Parquet write. Does output I/O cap the gain at about 2×?
- Recall gained from looser blocking at fixed wall time: run the same budget on CPU and GPU on a labelled set (for example the ONS-style synthetic data or the NC voters set).
- Peak VRAM and runtime under a forced mega-block (one key with 1M rows), to check chunked probe output.
- Which Splink-generated queries DataFusion 45 and peacockdb refuse (window functions, list functions). This is a desk-plus-run check of Splink's SQL.
- cuGraph WCC on 450M–5B edges against DuckDB clustering in Splink.

## Sources

- [D] demand.md `## Ratings`, row fuzzy-join-gpu, and "Who already accelerates" (local, master 9d61cac8; accessed 2026-10-29)
- [I] inventory.md at master f25c250d, incl. `## Yardstick` and benchmark evidence (local; accessed 2026-10-29)
- [S] sirius.md `## Sirius components`, Sirius @ b9e05ffc (local; accessed 2026-10-29)
- [N] nvidia.md, stance and cuVS/NeMo rows (local; accessed 2026-10-29)
- [E] evidence-in-hand.md, TPC-H SF20 load vs compute (local; accessed 2026-10-29)
- [LN] options/lean.md data-path table (local; accessed 2026-10-29)
- [GF] Research notes, GPU fit (`../notes/gpu-fuzzy-joins-in-sql/gpu_fit.md`; [IN] and [DM] are `incumbents.md` and `demand.md` there). Clones: cudf @785ee522f5688f485fc0b1eabca7f8d6d2adea0e, crosswalk @9b801f70b25658a1a2cab39c5f6c52523851f655, sirius @b9e05ffc653c8ed031c789e5c1415ac1186e0619 (accessed 2026-10-29). Key URLs:
  - https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/nvtext (accessed 2026-10-29)
  - https://github.com/rapidsai/cudf/issues/6503 (accessed 2026-10-29)
  - https://github.com/rapidsai/cudf/issues/18274 (accessed 2026-10-29)
  - https://github.com/ajwinters/crosswalk/blob/9b801f70b25658a1a2cab39c5f6c52523851f655/README.md (accessed 2026-10-29)
  - https://fast-er.readthedocs.io/en/latest/ (accessed 2026-10-29)
  - https://docs.nvidia.com/nemo/curator/latest/curate-text/process-data/deduplication/fuzzy.html (accessed 2026-10-29)
  - https://duckdb.org/docs/2.0/sql/functions/text.html (accessed 2026-10-29)
  - https://raw.githubusercontent.com/NVIDIA/spark-rapids/main/docs/supported_ops.md (no Levenshtein row; accessed 2026-10-29)
- [IN] Research notes, incumbents. Repos: splink @5b719296d62bc48d55c1a63322fd60c0bfba4c2e, zingg @9d1a7f4951b12ba65d6ca7ad7d650d45afd9f77f, dedupe @3f61e79102910bd355e920a2df7e44c14c9cb247, ONS Splink-census-linkage @1de9772853b7b805b69649fb78226c5d3dc76219 (accessed 2026-10-29). Key URLs:
  - https://www.robinlinacre.com/fast_deduplication/ (accessed 2026-10-29)
  - https://github.com/moj-analytical-services/splink/discussions/3263 (accessed 2026-10-29)
  - https://www.robinlinacre.com/optimising_duckdb_performance_large_ec2_instances/ (accessed 2026-10-29)
  - https://www.robinlinacre.com/splink_or_swim/ (accessed 2026-10-29)
  - https://moj-analytical-services.github.io/splink/demos/tutorials/03_Blocking.html (accessed 2026-10-29)
  - https://github.com/Data-Linkage/Splink-census-linkage/blob/main/SplinkCaseStudy.pdf (accessed 2026-10-29)
  - https://aws.amazon.com/entity-resolution/pricing/ (accessed 2026-10-29)
  - https://senzing.com/pricing/ (accessed 2026-10-29)
  - https://www.zingg.ai/ (accessed 2026-10-29)
  - https://www.databricks.com/blog/2023/08/04/improving-public-sector-decision-making-simple-automated-record-linking.html (accessed 2026-10-29)
  - Splink #2876, #2173, #2886, #2638, #2573, #1011, #1063; zingg #446, #688, #1164, #1339; dedupe #390, #749 (GitHub; accessed 2026-10-29)
- [DM] Research notes, demand:
  - https://github.com/NVIDIA/cudf/issues/17184 (accessed 2026-10-29)
  - https://github.com/NVIDIA/cudf/issues/15532 (accessed 2026-10-29)
  - https://www.reddit.com/r/CUDA/comments/1uy5az7/ (accessed 2026-10-29)
  - https://github.com/moj-analytical-services/splink/discussions/2209 (accessed 2026-10-29)
  - https://news.ycombinator.com/item?id=40798668 (accessed 2026-10-29)
  - https://www.reddit.com/r/databricks/comments/1svfzbt/ (accessed 2026-10-29)
  - https://www.reddit.com/r/dataengineering/comments/1kp0ldl/ (accessed 2026-10-29)
  - https://arxiv.org/abs/2410.04349 (accessed 2026-10-29)
  - https://moj-analytical-services.github.io/splink/blog/2026/01/29/running-splink-in-production.html (accessed 2026-10-29)
  - https://www.federalreserve.gov/econres/feds/can-llms-improve-sanctions-screening-in-the-financial-system-evidence-from-a-fuzzy-matching-assessment.htm (accessed 2026-10-29)
- DataFusion 45 `levenshtein`: local cargo registry `datafusion-functions-45.0.0/src/string/levenshtein.rs` (accessed 2026-10-29)
