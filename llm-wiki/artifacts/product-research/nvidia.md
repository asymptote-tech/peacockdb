# NVIDIA — platform risk for a GPU SQL product

NVIDIA does not sell a database or a hosted SQL service. It does give away complete GPU SQL engines, and it staffs them.
It owns HeavyDB and open-sourced all of it in October 2026 [14].
Its engineers are the majority of the top committers to Sirius [18].
It sells the Spark plugin with paid support through NVIDIA AI Enterprise [7].
It published GQE, a DataFusion → Substrait → libcudf engine, in August 2026 [35].
For a small startup the risk is commoditization by free, NVIDIA-staffed engines, not a competing NVIDIA product SKU.
The claim "NVIDIA denies competing" has no public source. The claim "NVIDIA backs Sirius" is true, and understates NVIDIA's role.
Citations in brackets point to the Sources list. All were accessed 2026-10-28.

## Stance on competing

**Public statements.**
- NVIDIA's message is "accelerate every data platform". Huang at GTC 2026: "We address every single data processing platform" and "The libraries is the crown jewels of our company" [2].
- The GTC 2026 blog says cuDF accelerates Spark, Presto, DuckDB, Polars and Velox. It names EDB, NetApp, Snowflake, Starburst and VAST as platforms that support cuDF/cuVS. That section mentions neither HeavyDB nor Sirius [1].
- Huang in IBM's release: "Together with IBM, we are bringing CUDA GPU acceleration directly into the data layer." IBM owns the engine there [3].
- NVIDIA's data-processing page sells cuDF as "drop-in accelerators" for the customer's existing stack [4].
- NVIDIA's cuDF FAQ tells engine builders to add GPU through Velox "without forking" [63].
- In cloud, NVIDIA has said it does not compete with partners [65][81]. Huang has said NVIDIA "doesn't build clouds" [78]. Neither statement is about databases.

**Does "denies competing" have a public source?** No.
- No source has NVIDIA saying it will not ship a database. No source has "not a database company" in any form [1][2][4].
- The nearest thing is partner-first positioning. That is marketing, not a commitment.
- At GTC 2026 an NVIDIA session called Sirius "NVIDIA's GPU-native analytics engine" (secondhand, attendee notes [5]).
- NVIDIA's developer blog calls Sirius a UW-Madison project "with support from NVIDIA engineers" [6]. Both framings come from NVIDIA staff.

**Job postings.**
- One 2026 posting was verified: Senior C++/CUDA engineer, Analytics and Data Intelligence (ADI) group [11].
- Duties: UcxExchange in Velox (30%), multi-node Presto GPU (30%), Presto/Velox/cuDF projects (40%), incl. operator completeness and CPU fallback [11].
- That is full-engine work, but on Presto, which NVIDIA does not own. It does not mention DuckDB, Sirius or HeavyDB [11].
- Hiring volume is unknown. NVIDIA's Workday was not searched.

**Acquisitions and people.**
- NVIDIA acquired HEAVY.AI (MapD/OmniSci) in 2025. No price, date or NVIDIA press release exists [12][13].
- Pavlo calls it a "sort-of acquihire" [13].
- HeavyDB docs now live on docs.nvidia.com [10].
- The heavydb README says "NVIDIA is now releasing the entire platform as open source", including former enterprise features. That covers the Immerse UI, the renderer and HeavyConnect. Licence is Apache-2.0 [14].
- Support is "best effort" with no guaranteed response times [14].
- Its October 2026 commits come from nvidia.com addresses [14].
- DBDB still marks HeavyDB "abandoned" [15]. The October 2026 README is newer, so it wins.
- Todd Mostak (HEAVY.AI founder) is now Sr. Director of Engineering at NVIDIA (secondhand [5]).
- Voltron Data (Theseus) died in 2025 after layoffs [13][69][70]. No NVIDIA acquisition of it is on record.
- Its people moved to NVIDIA:
  - Josh Patterson (Voltron CEO) is NVIDIA VP Solutions Architecture [21].
  - Rodrigo Aramburu co-authored NVIDIA's Sirius blog [6][79].
  - Felipe Aramburu is presented as an NVIDIA Distinguished Solutions Architect (secondhand [5]).
  - Malpica, Brobbel and Ocsa list NVIDIA on their GitHub profiles [18].
- GTC speakers presented Theseus as the research engine behind cuCascade (secondhand [5]). cuCascade now sits in the NVIDIA GitHub org [19].
- Sirius, CIDR 2026 paper: first author Yogatama, with Patterson and Kimball, all NVIDIA. PI is Xiangyao Yu (UW-Madison) [16][17].
- It acknowledges "NVIDIA Data Science Cloud Credits" [16]. No NSF grant is listed [16].
- DuckDB's docs list Sirius under `core_extensions`, marked "coming soon" [20].

**Commercial offerings.**
- The RAPIDS Accelerator (Spark plugin) "is exclusively available with NVIDIA AI Enterprise", with paid support and dated production branches [7][8].
- The code itself is Apache-2.0 on GitHub [27]. NVIDIA therefore already makes money from GPU SQL acceleration, but as an accelerator plus support. Spark owns the engine.
- The NVIDIA AI Data Platform is a storage reference design. Partners build it: Dell, NetApp, VAST, WEKA and others [64][1].
- STX is "a foundation for partners building a data engine" [9].
- Dell's AI Data Platform ships the cuDF/cuVS engines. NVIDIA does not [1].
- No NVIDIA-hosted SQL service was found on DGX Cloud. NVIDIA pulled DGX Cloud back to mostly internal use in 2025 [66].
- Sirius and HeavyDB are not in AI Enterprise today (not found, open question).

**Verdict.**
- A paid, NVIDIA-branded GPU database or SQL service is unlikely (estimate). It would upset IBM, Snowflake, Starburst, Oracle, Dell and the clouds [1].
- A free, complete, NVIDIA-staffed GPU SQL engine is not a risk. It already exists three times over: HeavyDB [14], Sirius [18], and Presto/Velox-cuDF [30].
- A small startup should fear price compression and loss of the engine as a moat. NVIDIA's product does not need to exist for that to happen.
- The safe ground is what NVIDIA leaves to partners (estimate): managed service, catalog and lakehouse integration, multi-tenant serving, CPU/GPU routing, and support outside AI Enterprise.

## Open-source projects in this space

Activity is from GitHub as of 2026-10-28. Commit counts are since 2026-04-01. "nvidia.com" counts are a lower bound, because many NVIDIA staff commit from gmail or noreply addresses.

| Project | What it does | Activity | Licence | Helps | Threatens |
|---|---|---|---|---|---|
| cuDF / libcudf [22] | GPU DataFrame and C++ columnar kernels. Repo moved to NVIDIA/cudf | v26.10.02 (2026-10-20). 2,065 commits, 112 authors, 637 from nvidia.com | Apache-2.0 | all directions (the executor base) | none directly. It is a component |
| libcudf_streaming [23] | Chunked Parquet, partition/shuffle, bloom filter, approx-distinct over rapidsmpf | new 2026-06 (PR #22747) | Apache-2.0 | datafusion-plugin, lakehouse-iceberg, distributed-pluggable, streaming-flink | none |
| libcudf experimental readers [24] | hybrid_scan (filter-then-materialize) and Delta-style deletion vectors | in main | Apache-2.0 | lakehouse-iceberg (fixes "local Parquet only" together with KvikIO) | lowers the bar for rivals too |
| rapidsmpf [26] | Multi-GPU streaming and shuffle primitives | v26.10.00. 253 commits, 32 authors | Apache-2.0 | distributed-pluggable, datafusion-plugin | none |
| cudf-polars [25][77] | Polars GPU engine. Streaming executor on one GPU, or Ray/Dask/SPMD multi-node | in cuDF monorepo. Open beta | Apache-2.0 | none for SQL | datafusion-plugin, lakehouse-iceberg (Python buyers) |
| cudf.pandas [22] | Zero-code pandas accelerator | in cuDF monorepo | Apache-2.0 | none | none for SQL |
| dask-cudf [22] | Distributed cuDF on Dask | in cuDF monorepo | Apache-2.0 | none | distributed-pluggable (weakly) |
| cudf-spark (ex-spark-rapids) [27][28] | NVIDIA's Spark plugin. Iceberg (async S3 byte-range reader), Delta 2.0–4.3 + Databricks, local-disk file cache, cache serializer, UCX shuffle | v26.08.1 (2026-08-28). 961 commits, 459 from nvidia.com | Apache-2.0 code, CC-BY docs. Supported build sold via AI Enterprise [7] | none outside JVM | spark-plugin, lakehouse-iceberg |
| Velox cuDF backend [29][30][80] | GPU operators under Velox. Used by Presto C++ GPU (nightlies since 2026-06 [31]) and by Gluten GPU (experimental, SF1 [32]) | 212 commits, 49 authors (NVIDIA, IBM, Meta) | Apache-2.0 | distributed-pluggable (UCX exchange ideas) | spark-plugin, lakehouse-iceberg, distributed-pluggable |
| Sirius [18] | GPU engine behind DuckDB via Substrait. Built on cuDF, cuVS, cuCascade | no semver releases, rolling `stable` tag (2026-10-02). 935 commits, 38 authors, 160 from nvidia.com | Apache-2.0 | on-sirius | clickhouse, realtime-other, realtime-vram-cache, distributed-pluggable |
| Sirius StarRocks / Doris backends [34][33] | StarRocks: 107 commits since 2026-05-28. Doris: a Rust BE that joins an unmodified Doris FE | experimental | Apache-2.0 | on-sirius | realtime-other |
| GQE [35][36][37] | NVIDIA's PoC SQL engine: DataFusion 52.3 → Substrait → C++ GPU executor, Arrow Flight SQL, NVSHMEM multi-GPU, nvCOMP-compressed in-memory tables | one public commit (2026-08-10, nvidia.com). 62 stars | Apache-2.0, but needs proprietary nvCOMP/NVSHMEM/CUPTI | datafusion-plugin, as a blueprint | datafusion-plugin, realtime-vram-cache |
| HeavyDB [14] | Full GPU SQL database, UI and renderer. NVIDIA-owned | 10.1 on master. nvidia.com commits Oct 2026. 3,068 stars | Apache-2.0 | none | realtime-vram-cache, realtime-other |
| cuCascade [19] | Tiered GPU/pinned-host/disk memory, reservations, cuDF spill | created 2025-12-15. "Experimental". 63 stars | Apache-2.0 | realtime-vram-cache, datafusion-plugin, clickhouse | none |
| cuVS [38][39] | GPU vector search. C/C++/Python/Rust/Go/Java bindings | v26.10.00. 492 commits. In Milvus, Elastic, OpenSearch, Oracle 26ai, Kinetica, Faiss | Apache-2.0 | vector-sql | vector-sql (the feature everyone already has) |
| RMM [40] | GPU memory resources | v26.10.00. 182 commits | Apache-2.0 | all | none |
| KvikIO / GDS [41] | cuFile/GDS bindings, plus S3, HTTP and WebHDFS remote reads | v26.10.00. 124 commits | Apache-2.0 | lakehouse-iceberg, realtime-vram-cache | none |
| nvCOMP [42] | GPU compression | repo archived 2026-07-13. Binaries only | proprietary NVIDIA licence | realtime-vram-cache (compressed VRAM) | lock-in: closed code on the hot path |
| cuCollections / CCCL [43][44] | GPU hash maps; Thrust/CUB/libcu++ | 42 commits (cuco). CCCL released 2026-10-28 | Apache-2.0 (with LLVM exception for libcu++) | all (custom kernels) | none |
| Morpheus / NeMo Curator [45][46] | Security streaming / LLM data curation | Morpheus last release 2025-09. Curator v1.4.0 (2026-10-09) | Apache-2.0 | none | none |
| hipDF (AMD) [47][48] | ROCm port of cuDF 25.10. Python-only, early access | AMD-run | Apache-2.0 (derived) | other-accelerators | other-accelerators (AMD fills the layer itself). Community ROCm ports of cuCascade/cuCollections exist [74] |

**GQE and peacockdb.**
- The two are near twins: a Rust DataFusion planner over a C++ libcudf executor [36].
- Differences:
  - GQE uses DataFusion 52.3. peacockdb uses DataFusion 45.
  - GQE ships plans as Substrait (`from_substrait.hpp`). peacockdb uses its own FlatBuffers plan over FFI (`flatbuffers/gpu_plan.fbs` in this repo).
  - GQE adds Flight SQL serving, NVSHMEM multi-GPU, partition pruning, a join hash-map cache, a custom chunked Parquet reader and nvCOMP-compressed in-memory tables [35].
- GQE claims all 22 TPC-H SF1000 queries in 9.15 s on one GPU [35].
- GQE is a PoC with one code drop [35]. Still, "DataFusion + libcudf" is not a moat on its own. NVIDIA has already built and published it.

**Platform-control risk from the libraries themselves.**
- RAPIDS releases every two months [22]. It dropped all CUDA 11 support in 25.08 [71]. No libcudf API-stability policy was found.
- An outside engine absorbs breakage after the fact. NVIDIA-staffed engines (Sirius, Velox-cuDF) get coordinated changes (estimate).

## Partners and ecosystem

**Big platforms (cuDF inside the engine the customer already runs).**
- **IBM watsonx.data:** Presto C++ on Velox + cuDF. IBM reports Nestlé ran 5x faster at 83% lower cost (vendor claim) [3][59][76].
  - NVIDIA built the UCX exchange (secondhand [5]). The open-source GPU Presto images are pre-release nightlies [31].
- **Starburst:** cuDF at the operator level in its proprietary Trino build. Scan, filter, aggregation, TopN and join are done. GPU exchange is in progress. Not GA [58].
  - Starburst reports TPC-H 4.6x and ClickBench 3.4x on L4/GB202 (vendor claim) [58].
- **Databricks / EMR / Dataproc / Oracle AIDP / Cloudera:** all run NVIDIA's own Spark plugin [54][55][56][57][82].
  - On Databricks it still requires Photon off, and one GPU per node [54].
  - Databricks' 2024 promise of GPU Photon has no 2025–2026 confirmation [53].
  - Project Aether migrates EMR Spark jobs to GPU; access is by application [55].
- **Snowflake:** cuDF/cuML in Snowflake ML containers only. Not in its SQL engine [52].
- **Oracle:** the Spark plugin in AIDP, plus cuVS for vector-index builds in AI Database 26ai [57][1]. No GPU SQL in Oracle Database.
- **Dell / VAST / NetApp / WEKA (AI Data Platform):**
  - Storage partners ship the engines. Dell bundles a cuDF Spark engine and a cuVS vector DB [1][64].
  - VAST is a listed Sirius partner [18].

**Engines that host GPU execution themselves.**
- **ClickHouse Inc.:**
  - RFC #116807 (2026-08-27, a ClickHouse org member): "As a first step, we would like to build ClickHouse on top of Sirius" [49].
  - RFC #124455 (2026-10-07): offload Substrait subtrees to a Sirius step, with a GPU column cache keyed by (part, column, version) [50].
  - cuDF and cuCascade are merged into ClickHouse `contrib/`. GPU aggregation, join and MergeTree-reader PRs are open [51].
  - NVIDIA's own page lists ClickHouse only for the Vera CPU, with no joint GPU announcement [4]. So this occupation is ClickHouse-led, on NVIDIA components.
- **DuckDB:** Sirius is listed as a coming core extension [20]. Mühleisen called it "the blessed way" to run DuckDB on GPUs (secondhand [5]).
- **Polars:** GPU engine powered by cuDF, open beta [77].
- **Gluten:** cuDF backend, experimental. TPC-DS SF1 fails with CPU fallback disabled [75][32].

**Inception-era GPU DB vendors that faded.**
- Kinetica has only an archived NVIDIA partner page. At GTC 2026 it appeared for Vera CPU gains (secondhand) [60][5].
- SQream was an Inception partner in 2024 [61]. Brytlyt became Inception Premier in 2023 [62].
- None of the three appeared in the GTC 2026 GPU SQL sessions (secondhand [5]).
- HEAVY.AI was absorbed [12]. Voltron died [13].

**What this says.**
- NVIDIA wants third parties of two kinds (estimate from [4][63]):
  - owners of an installed engine base (IBM, Starburst, Cloudera, ClickHouse)
  - builders of open, reusable parts (Sirius)
- A new proprietary standalone GPU database fits neither kind.
- NVIDIA absorbs promising GPU-DB IP into open libraries rather than funding GPU DB companies: HEAVY.AI, and the Theseus ideas in cuCascade [12][19].
- Room NVIDIA leaves (estimate): engines whose vendor has not moved (Databend, DataFusion-based systems, Flink SQL), and operational layers (managed service, multi-tenant serving).

## Per direction

- clickhouse — strong — ClickHouse Inc. is building GPU execution in-tree on cuDF + cuCascade, with Sirius offload as step one (RFCs #116807, #124455; cuDF in contrib). evidence_against.md rated this weak because NVIDIA itself shows ClickHouse only on Vera CPU. But the host vendor is occupying the slot on NVIDIA-staffed parts, so a third-party plugin would race upstream — https://github.com/ClickHouse/ClickHouse/issues/124455 (accessed 2026-10-28)
- lakehouse-iceberg — strong — NVIDIA's own cudf-spark already reads Iceberg and Delta. IBM Presto/Velox-cuDF is in production. Starburst has Trino-cuDF in development. libcudf deletion-vector readers lower the bar for everyone — https://github.com/NVIDIA/cudf-spark (accessed 2026-10-28)
- realtime-vram-cache — strong — NVIDIA owns HeavyDB, the original GPU in-memory real-time SQL database, and open-sourced all of it in Oct 2026. GQE has nvCOMP-compressed in-memory tables. Sirius pins tables in VRAM. HeavyDB is only "best effort" maintained, which is the one opening — https://github.com/heavyai/heavydb (accessed 2026-10-28)
- streaming-flink — weak — no NVIDIA or Flink GPU work was found. Alibaba–NVIDIA Flink talk is snippet-only [73]. libcudf_streaming/rapidsmpf help as components — https://github.com/apache/flink/issues?q=GPU (accessed 2026-10-28)
- datafusion-plugin — medium — no NVIDIA or upstream DataFusion GPU work. But NVIDIA's GQE is exactly DataFusion → Substrait → libcudf, a PoC published as one code drop. That proves the design is no moat and could be promoted or forked at any time (estimate) — https://github.com/rapidsai/gqe (accessed 2026-10-28)
- realtime-other — strong — Sirius ships experimental StarRocks (107 commits) and Doris backends. The Doris backend joins an unmodified Doris FE. The CIDR paper shows 12.5x with Doris. evidence_against.md rated this medium without weighing these Sirius backends — https://github.com/sirius-db/sirius/blob/main/experimental/doris/README.md (accessed 2026-10-28)
- distributed-pluggable — medium — no Databend GPU work (issue #15606 closed). But NVIDIA supplies the distributed parts itself (rapidsmpf, Velox UCX exchange, cuCascade), and any Velox-based engine gets GPU for free. A third party is an integrator on NVIDIA parts — https://github.com/facebookincubator/velox/tree/main/velox/experimental/cudf (accessed 2026-10-28)
- spark-plugin — strong — head-on with NVIDIA's own supported product. It is sold via AI Enterprise, runs on every managed Spark, has Aether migration, and has Gluten-cuDF as a second path — https://catalog.ngc.nvidia.com/orgs/nvidia/-/resources/spark-rapids-pb25h2/- (accessed 2026-10-28)
- other-accelerators — weak — NVIDIA cannot compete on AMD or Cerebras hardware. The real rival is AMD's own hipDF. The CUDA EULA translation-layer ban [72] only bites binary shims, not source ports of Apache-2.0 cuDF — https://github.com/AMD-Ecosystem/hipDF (accessed 2026-10-28)
- vector-sql — medium — NVIDIA sells no vector-SQL engine. But cuVS is already inside Milvus, Elastic, OpenSearch, Oracle 26ai and Kinetica, so "GPU vector search" is a checkbox, not a differentiator. evidence_against.md says strong; this rates it medium because the threat comes from partners, not NVIDIA, and a fused vector+join+aggregate GPU plan is not clearly taken. It turns strong if Sirius exposes cuVS through SQL (unverified) — https://github.com/NVIDIA/cuvs/blob/main/fern/pages/integrations.md (accessed 2026-10-28)
- on-sirius (building on Sirius itself) — strong — aligned with NVIDIA, but with no moat. 7 of the top 9 committers are NVIDIA staff by GitHub profile or GTC listing. NVIDIA controls the roadmap and cuCascade. ClickHouse, Doris and StarRocks integrate with upstream directly, so a startup layer competes with its own maintainers — https://github.com/sirius-db/sirius (accessed 2026-10-28)

## Evidence against

**Sourced facts.**
- NVIDIA owns a full GPU SQL database and gave all of it away under Apache-2.0, enterprise features included. October 2026 commits are from nvidia.com [14][10].
- Sirius is NVIDIA-staffed:
  - 160 of 935 commits since April come from nvidia.com addresses (lower bound) [18].
  - 54 of the last 200 commits do [18].
  - The first author of the CIDR paper is NVIDIA staff [16].
  - The memory layer is in the NVIDIA org [19].
  - An NVIDIA GTC session called it "NVIDIA's GPU-native analytics engine" (secondhand [5]).
- GQE: NVIDIA itself wrote a DataFusion → Substrait → GPU SQL engine with Flight SQL serving [35][36].
- cudf-spark is a sold product: "exclusively available with NVIDIA AI Enterprise", with support SLAs [7]. NVIDIA rebranded it "NVIDIA cuDF plugin for Apache Spark" [27].
- The BlazingSQL and Voltron engine builders now work at NVIDIA [21][6][18]. Theseus ideas became NVIDIA's cuCascade (secondhand [5]; [19]).
- NVIDIA's ADI group hires for full-engine work (multi-node, CPU fallback, operator completeness) [11].
- Precedents for moving up the stack:
  - DGX Cloud Lepton put NVIDIA in competition with CoreWeave and Nebius, while NVIDIA said it does not compete [65][81].
  - CentML ceased operations after an acqui-hire [67].
  - The 2025 deals were framed as a "push toward full-stack AI control" [68].
  - NVIDIA retreated from DGX Cloud once hyperscalers pushed back [66]. A small startup has no such leverage.
- nvCOMP went closed: the repo was archived and binaries ship under an NVIDIA licence. libcudf, GQE and ClickHouse's GPU work depend on it for compressed paths [42].

**Inference.**
- "NVIDIA does not compete in GPU SQL" was already false in October 2026 at the engine layer. It holds only for product SKUs (estimate).
- Commoditize-the-complement: free HeavyDB, Sirius and Presto-GPU cap what a startup can charge for a GPU SQL server (estimate).
- GQE could be promoted from PoC, or folded into Sirius, with little warning (estimate).
- An AI Enterprise SKU covering Sirius or HeavyDB would make NVIDIA the vendor of record for GPU DuckDB. It would directly undercut a support business (estimate; no evidence yet).
- The Voltron → NVIDIA move looks like a talent migration. No deal is on record (estimate).

**Counter-evidence.**
- NVIDIA open-sourced HeavyDB instead of selling it, and has no hosted query service [14][66].
- NVIDIA's revenue here comes from GPUs and AI Enterprise. Its partner lists are long and include the big data vendors [1][7].

## Follow-up experiments

- Clone sirius-db/sirius and run `git shortlog -se` over the last 12 months. Map the top 20 authors to employers. This turns "NVIDIA-staffed" into a number. If more than 60% are NVIDIA, on-sirius is an NVIDIA product in all but name (1 hour).
- Read the GQE source: `from_substrait.hpp`, the Parquet reader, the in-memory table code. Diff it against peacockdb's operator coverage. Check whether GQE's executor can sit behind peacockdb's planner, or vice versa (half a day).
- Watch the AI Enterprise product support matrix and NGC for Sirius, HeavyDB or Presto-GPU entries. Any entry moves the stance verdict from "free competitor" to "sold competitor" (monthly, 10 minutes).
- Track ClickHouse PRs #119502, #121061 and #127604 and RFC #124455. If they merge into a release, clickhouse is closed for good. If they stall, there is room to help ClickHouse users (monthly).
- Check whether Sirius exposes cuVS through SQL functions today: grep the repo and its docs. That decides vector-sql between medium and strong (30 minutes).
- Search NVIDIA Workday for "Sirius", "DuckDB", "HeavyDB", "DataFusion" and "Iceberg". Count the open roles per engine. This closes the hiring-volume gap (30 minutes).
- Check DuckDB release notes for the date Sirius ships as a core extension. Check MotherDuck for GPU plans (15 minutes).
- Search VeloDB/SelectDB, CelerData and Databend for "GPU". If they adopt Sirius officially, realtime-other is closed; if they reject it, it reopens (30 minutes).

## Sources

1. https://blogs.nvidia.com/blog/gtc-2026-news/ (accessed 2026-10-28)
2. https://www.constellationr.com/insights/news/nvidia-gtc-2026-were-software-company-too (accessed 2026-10-28)
3. https://newsroom.ibm.com/2026-03-16-ibm-and-nvidia-announce-expanded-collaboration-at-gtc-2026-to-advance-ai-for-the-enterprise (accessed 2026-10-28)
4. https://www.nvidia.com/en-us/solutions/data-processing/ (accessed 2026-10-28)
5. https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html — third-party GTC notes, secondhand (accessed 2026-10-28)
6. https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/ (accessed 2026-10-28)
7. https://catalog.ngc.nvidia.com/orgs/nvidia/-/resources/spark-rapids-pb25h2/- (accessed 2026-10-28)
8. https://docs.nvidia.com/ai-enterprise/deployment/spark-rapids-accelerator/latest/overview.html (accessed 2026-10-28)
9. https://www.nvidia.com/en-gb/data-center/ai-storage/stx/ (accessed 2026-10-28)
10. https://docs.nvidia.com/heavyai/overview (accessed 2026-10-28)
11. https://jobs.anitab.org/companies/nvidia/jobs/90093660-senior-software-engineer-c-and-cuda-analytics-and-data-intelligence (accessed 2026-10-28)
12. https://en.wikipedia.org/wiki/HEAVY.AI (accessed 2026-10-28)
13. https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html (accessed 2026-10-28)
14. https://github.com/heavyai/heavydb (accessed 2026-10-28)
15. https://dbdb.io/db/heavydb (accessed 2026-10-28)
16. https://arxiv.org/html/2508.04701v3 (accessed 2026-10-28)
17. https://vldb.org/cidrdb/papers/2026/p12-yogatama.pdf (accessed 2026-10-28)
18. https://github.com/sirius-db/sirius (accessed 2026-10-28)
19. https://github.com/NVIDIA/cuCascade (accessed 2026-10-28)
20. https://duckdb.org/docs/2.0/core_extensions/sirius (accessed 2026-10-28)
21. https://techarena.ai/people/josh-patterson (accessed 2026-10-28)
22. https://github.com/NVIDIA/cudf (accessed 2026-10-28)
23. https://github.com/rapidsai/cudf/pull/22747 (accessed 2026-10-28)
24. https://github.com/NVIDIA/cudf/tree/main/cpp/include/cudf/io/experimental (accessed 2026-10-28)
25. https://github.com/NVIDIA/cudf/blob/main/docs/cudf/source/cudf_polars/engines.md (accessed 2026-10-28)
26. https://github.com/rapidsai/rapidsmpf (accessed 2026-10-28)
27. https://github.com/NVIDIA/cudf-spark (accessed 2026-10-28)
28. https://github.com/NVIDIA/cudf-spark/blob/main/docs/additional-functionality/advanced_configs.md (accessed 2026-10-28)
29. https://github.com/facebookincubator/velox/tree/main/velox/experimental/cudf (accessed 2026-10-28)
30. https://developer.nvidia.com/blog/accelerating-large-scale-data-analytics-with-gpu-native-velox-and-nvidia-cudf (accessed 2026-10-28)
31. https://prestodb.io/blog/2026/06/24/gpu-accelerated-presto-c/ — via search summary, page returned 403 (accessed 2026-10-28)
32. https://github.com/apache/gluten/blob/main/docs/backends/velox/get-started/VeloxGPU.md (accessed 2026-10-28)
33. https://github.com/sirius-db/sirius/blob/main/experimental/doris/README.md (accessed 2026-10-28)
34. https://github.com/sirius-db/sirius/tree/main/experimental/starrocks (accessed 2026-10-28)
35. https://github.com/rapidsai/gqe (accessed 2026-10-28)
36. https://github.com/rapidsai/gqe/tree/main/rust (accessed 2026-10-28)
37. https://github.com/rapidsai/gqe/blob/main/LICENSE (accessed 2026-10-28)
38. https://github.com/NVIDIA/cuvs (accessed 2026-10-28)
39. https://github.com/NVIDIA/cuvs/blob/main/fern/pages/integrations.md (accessed 2026-10-28)
40. https://github.com/rapidsai/rmm (accessed 2026-10-28)
41. https://github.com/rapidsai/kvikio (accessed 2026-10-28)
42. https://github.com/NVIDIA/nvcomp (accessed 2026-10-28)
43. https://github.com/NVIDIA/cuCollections (accessed 2026-10-28)
44. https://github.com/NVIDIA/cccl/blob/main/LICENSE (accessed 2026-10-28)
45. https://github.com/nv-morpheus/Morpheus (accessed 2026-10-28)
46. https://github.com/NVIDIA-NeMo/Curator (accessed 2026-10-28)
47. https://github.com/AMD-Ecosystem/hipDF (accessed 2026-10-28)
48. https://rocm.docs.amd.com/projects/hipDF/en/docs-25.05/index.html (accessed 2026-10-28)
49. https://github.com/ClickHouse/ClickHouse/issues/116807 (accessed 2026-10-28)
50. https://github.com/ClickHouse/ClickHouse/issues/124455 (accessed 2026-10-28)
51. https://github.com/ClickHouse/ClickHouse/pulls?q=%5BGPU%5D (accessed 2026-10-28)
52. https://www.snowflake.com/en/news/press-releases/snowflake-supercharges-machine-learning-for-enterprises-with-native-integration-of-nvidia-cuda-x-libraries/ (accessed 2026-10-28)
53. https://www.databricks.com/company/newsroom/press-releases/databricks-and-nvidia-strengthen-partnership-accelerate-enterprise (accessed 2026-10-28)
54. https://docs.nvidia.com/cudf-spark/26.06/getting-started/databricks.html (accessed 2026-10-28)
55. https://developer.nvidia.com/blog/migrate-apache-spark-workloads-to-gpus-at-scale-on-amazon-emr-with-project-aether/ (accessed 2026-10-28)
56. https://cloud.google.com/dataproc-serverless/docs/guides/gpus-serverless (accessed 2026-10-28)
57. https://blogs.nvidia.com/blog/nvidia-oracle-accelerate-enterprise-ai-data-processing/ (accessed 2026-10-28)
58. https://www.starburst.io/blog/gpu-accelerated-sql-analytics-how-starburst-and-nvidia-deliver-industry-benchmark-speedups-on-gpu-infrastructure/ (accessed 2026-10-28)
59. https://www.ciodive.com/news/ibm-nvidia-gtc-partnership-ai-data-management-gpus/814877/ (accessed 2026-10-28)
60. https://archive.kinetica.com/partner/nvidia/ (accessed 2026-10-28)
61. https://sqream.com/event/gtc-nvidia-event-2024-sqream-sponsorship/ (accessed 2026-10-28)
62. https://prnewswire.co.uk/news-releases/brytlyt-becomes-nvidia-inception-premier-partner-301734551.html (accessed 2026-10-28)
63. https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/ (accessed 2026-10-28)
64. https://blogs.nvidia.com/blog/nvidia-certified-enterprise-storage (accessed 2026-10-28)
65. https://www.tipranks.com/news/nvidias-nvda-new-cloud-market-push-puts-partners-on-edge (accessed 2026-10-28)
66. https://www.tomshardware.com/tech-industry/nvidia-steps-back-from-dgx-cloud (accessed 2026-10-28)
67. https://techcrunch.com/snippet/2983711/nvidia-reportedly-buys-an-ai-startup/ (accessed 2026-10-28)
68. https://www.hpcwire.com/aiwire/2025/04/01/nvidias-strategic-acquisitions-signal-push-toward-full-stack-ai-control/ (accessed 2026-10-28)
69. https://www.theinformation.com/briefings/ai-startup-voltron-data-switches-ceos-lays-off-staff (accessed 2026-10-28)
70. https://notice.co/c/voltrondata (accessed 2026-10-28)
71. https://docs.rapids.ai/notices/rsn0048/ (accessed 2026-10-28)
72. https://www.tomshardware.com/pc-components/gpus/nvidia-bans-using-translation-layers-for-cuda-software-to-run-on-other-chips-new-restriction-apparently-targets-zluda-and-some-chinese-gpu-makers (accessed 2026-10-28)
73. https://www.alizila.com/alibaba-cloud-pushes-open-source-apache-flink-toward-agentic-streaming-for-ai/ — snippet only, page returned 403 (accessed 2026-10-28)
74. https://github.com/mycomputercrashsalot/cuda2rocm (accessed 2026-10-28)
75. https://github.com/apache/gluten/issues/13122 (accessed 2026-10-28)
76. https://www.itjungle.com/2026/03/23/ibm-taps-nvidia-gpus-for-ai-turbocharged-data-mart/ (accessed 2026-10-28)
77. https://docs.nvidia.com/cudf/latest/cudf_polars/ (accessed 2026-10-28)
78. https://lexfridman.com/jensen-huang-transcript/ (accessed 2026-10-28)
79. https://www.linkedin.com/in/roaramburu/ (accessed 2026-10-28)
80. https://github.com/rapidsai/velox (accessed 2026-10-28)
81. https://techstrong.ai/features/nvidia-in-the-ai-cloud-cooperation-and-competition/ (accessed 2026-10-28)
82. https://www.datacentre.solutions/news/23622-cloudera-adds-native-nvidia-gpu-acceleration-to-apache-spark-for-faster-ai-data-prep (accessed 2026-10-28)
