# Audit of reports/product-direction.md (2026-10-29)

Line numbers refer to `reports/product-direction.md` as of commit `cedf5731` plus the working tree.
External checks were made on 2026-10-29 with `gh api`, the HN Firebase API and WebFetch.

## 1. Fifteen sentences against their support

| # | Line | Claim | Support checked | Verdict |
|--:|--:|---|---|---|
| 1 | 24–25 | "13 named posters with large shuffle-and-join bills, and BI serving over billions of rows" | demand.md row spark-plugin; spark-plugin.md:9–26 | **Partly.** There are 13 posters, but demand.md:26 says they are "mostly trying it out", and spark-plugin.md:26 says "most are evaluating". Only dogman123 states a shuffle/join bill, and only Lin does BI serving. |
| 2 | 29 | Reader still Scala/JVM in 26.10-rc | spark-plugin.md:57–64 (cudf-spark `49d3be94`) | Supported (the artifact's grep; not re-run). |
| 3 | 34 | "NVIDIA opened a top-priority effort to replace the reader (#16073)" | gh: #16073 "Productize Wild cuDF-Spark scale-up capabilities", label `epic`, opened 2026-09-22 | **Partly.** The epic is a scale-up milestone (TPC-H SF30000 on 32 B200s). Native Parquet (#16087) and GDS (#16088) are two of about 15 child items. It is not an effort to replace the reader. Line 203 ("P0 epic #16073 for a native reader") has the same flaw. |
| 4 | 41 | DuckDB PR #28022, open | gh: "Add Sirius as a core extension", open, 2026-10-28 | Supported. |
| 5 | 121–122 | Stock DataFusion output gives silent wrong answers: LIMIT n → LIMIT 0 (sirius #1963) | gh #1963; sirius.md:104 | **Partly.** The bug is real, open and silent. But #1963 was filed against the StarRocks translator (Rust `substrait` crate emits only `count_expr`). The link to DataFusion 45 is sirius.md's inference, and the issue does not test it. |
| 6 | 138–141 | 1,244 commits in 12 months; ~92% of human commits NVIDIA; no releases; old engine deleted 2026-09-30 | sirius.md:31, 132, 134 | Supported. (sirius.md:4 says "92% of lines", not commits. That is an internal inconsistency, but the report uses the commit figure.) |
| 7 | 156–157 | Light GPU-tier compression sped up the suite ~20%; host-tier compression cost ~7% | notes/vram-cache-encoding-datapath.md:55–61 | **Partly.** The −19.7% row is "dimensions on GPU raw, lineitem + orders on GPU compressed" against an all-host-raw baseline. It measures moving data to the GPU tier, not compression on that tier. The +7% host figure is right. |
| 8 | 217 | NVIDIA porting KX TAQ to cudf-polars (cudf #24634); `cudf::asof_join` PR open (#24180) | gh: both open, titles match | Supported. |
| 9 | 229 | ~16 H100-hours per 8 TB vs ~6,000 for one classifier pass | llm-corpus-sql.md:112–113 | Supported. |
| 10 | 245 | Sirius SF1000 on GB300: 18–21 s from Grace memory, ~7 s from HBM | grace-superchip-analytics.md:57 | Supported. |
| 11 | 252 | Sirius merged GPU Iceberg support with deletes on 2026-09-15 | gh PR #1425 merged 2026-09-15; lakehouse-iceberg.md:120 | **Partly.** Only position deletes and v3 DVs are covered; equality deletes fall back to DuckDB. |
| 12 | 335–336 | GTC 2026 session called Sirius "NVIDIA's GPU-native analytics engine", per attendee notes | nvidia.md [5]; WebFetch of the notes | Supported. The phrase is in the S81873 session overview, quoted by the attendee. |
| 13 | 367–368 | ClickHouse's CTO: "a small subset of production workloads in a narrow combination of conditions" | HN 46482420 (zX41ZdbW) | Supported. The subject is "GPU databases can run", not "GPU wins", which is a minor paraphrase. |
| 14 | 402–403 | dogman123 runs EMR joins of 150B+ rows and finds Glue too costly | HN 43966388 | Supported ("cost prohibitive"). |
| 15 | 406–407 | Andy Gong wants inputs cached in GPU memory, "which Spark refused" | cudf-spark discussion #13435 | **Partly.** He asked whether this is supported, and an NVIDIA spark-rapids maintainer confirmed it is not. Nothing was refused by Spark. |

Tally: **9 supported, 6 partly supported, 0 unsupported, 0 contradicted.**

Further sentences checked outside the sample:
- **Line 191–193, "applied where users have budgets": contradicted.** Line 47 says "No option has a user with a stated budget". spark-plugin.md:26 says "none of these posters states a budget".
- **Line 104, "rows 7–14 are ordered by the same tie-breaks": contradicted** by the mechanical check in section 4.
- **Line 203–204, "Independent attempts missed their targets or ran slower than CPU (cudf-spark #11992)": partly.** #11992 missed a cost target. The slower-than-CPU runs are GKE (HackerNoon) and spark-rapids #8082 (spark-plugin.md:92–95).
- **Line 186, "revenue far short of a staff of about 120": partly.** lean.md:140 has a missed revenue target (headline only, paywalled) and half of ~120 staff cut. "Far short" is not sourced.
- **Line 178–179, "The engines being adopted are the ones inside a host": overstated.** lean.md:109 says Sirius has no named production users, and the plugin evidence is "thin too".
- Sentences with no support in any artifact: none found, beyond the contradictions above.
- Spec gap: most claims carry no per-claim source link or date. The report relies on its blanket note (lines 5–7) and on the artifacts.

## 2. Was the lean judged like the others?

- **Template: yes.** options/lean.md has the same sections as the other 13 option files: Verdict, Who needs it, How it would work, Questions, Data path, Build cost, Adoption, Platform risk, Funding, Evidence against, Ratings, Follow-ups and Sources.
- **Ratings: yes.** The lean has six ratings and a table row (#3). Its ranking follows the rule (section 4).
- **Not restated as true:** part 3 (Voltron) is rejected, and parts 1 and 2 are qualified.
- **Not singled out for harsher treatment: no sign of that.**
- **The report's summary is softer than the lean file.** It drops three of the lean file's strongest negatives:
  - part 1: "Multi-GPU is not needed for capacity" (one H200 or B300 holds 100 GB compressed);
  - part 1: "The cost per query is roughly a wash… sells latency, not cost" (lean.md:95–96);
  - part 2: "no plugin vendor has shown it can charge" (lean.md:131).
- Line 191–193 then rescues the lean's cache thesis into the recommendation with an unsupported budget claim (see section 1). No other option gets a salvage paragraph.
- **Borderline: the demand rating.** The lean gets medium for "about 6 posters… soft asks". trainer-side-sql-prep gets weak for 5 posters' "small convenience asks", and four of the lean's six posters are also gpu-bi-dashboards' evidence. demand.md defines only strong and thin, so medium against weak is a judgement call. If the lean were weak, it would leave the top six and quant-gpu would rank 3rd.
- **Verdict:** judged by the same template and rules, but summarised more kindly than its own file.

## 3. Do the "strong" demand ratings have three or more independent first-person threads?

Rule (demand.md:107): strong needs at least three independent first-person threads with a painful need.

- **spark-plugin: passes.**
  - The row cites dogman123 (HN 43966388, first-person, "struggling", "cost prohibitive"), MaxNevermind (#11992, own benchmark, missed target) and the Databricks executor-kill poster (forum 121862, own job stuck).
  - spark-plugin.md:14–24 adds ByteDance #16321, Levesque #16460, Lin #11064 and the GKE author.
  - Caveat: most are evaluations, and none states a budget.
- **workstation-gpu-sql: passes.**
  - The row cites u/tecedu (team blocked, "willing to pay"), the 20 GB-image poster (forum 297463, first-person) and jingnanquan (sirius #609, own benchmarks on GB10/5090/A6000).
  - #609 is a performance question more than pain. workstation-gpu-sql.md lists many more install-pain threads (demand.md: about 33 install threads).
- No strong rating rests on fewer than three threads.

## 4. Applying the ranking rule to the table

- **Step 1:** no row exceeds 2× on both bases (max 1.6× / 1.3×). Nothing is out. Matches.
- **Step 2:**
  - strong: spark-plugin, workstation;
  - medium: lean, quant, gpu-bi, llm-corpus;
  - weak: datafusion, trainer, grace, lakehouse, other-acc, postgres;
  - thin: fuzzy, router.
  - Matches.
- **Strong tier:** friction is medium against weak, so spark-plugin is 1 and workstation 2. Matches.
- **Medium tier:**
  - friction: lean and quant medium; gpu-bi and llm weak;
  - data path: lean strong beats quant medium; gpu-bi strong beats llm medium;
  - so 3 lean, 4 quant, 5 gpu-bi, 6 llm-corpus. **Matches the stated order 1–6, recommendation and runner-up.**
- **Weak tier (rows 7–12): does not match.**
  - Friction is medium for datafusion, trainer, grace and lakehouse, and weak for other-acc and postgres.
  - Within the medium-friction group, grace has a strong data path, so it ranks above datafusion and trainer (medium), which rank above lakehouse (weak).
  - datafusion and trainer tie on every tie-break (data path, funding, platform risk). The rule leaves them unordered.
  - other-acc ranks above postgres on platform risk (medium against weak).
  - Mechanical order: grace, {datafusion = trainer}, lakehouse, other-acc, postgres. The table has grace at row 9.
- **Thin tier:** fuzzy (data path strong) ranks above router (medium). Matches.

Ratings that disagree:
- **grace-superchip-analytics, demand.** demand.md:123 rates it weak but says "No first-person post asks… Thin on direct evidence". With 0 posters, demand.md's own rule (fewer than three posters means thin) makes it thin. As thin, it ranks between fuzzy and router: friction ties, data path strong beats router's medium, and platform risk is weak against fuzzy's medium.
- **grace option file.** Its Ratings section says "demand.md has no row", which is stale.
- **fuzzy-join-gpu and gpu-cpu-router option files** say "Who needs it badly: weak" while demand.md says thin. The table correctly uses demand.md, but the option files disagree.
- All other table cells match their option files' Ratings sections exactly.

## Fixes

1. **Line 191–193.** Replace "applied where users have budgets and where NVIDIA has not built it." with "applied where the largest budgets in the space sit (though no user has stated one) and where NVIDIA has not built it."
2. **Line 104.** Replace "Within the weak and thin tiers, rows 7–14 are ordered by the same tie-breaks." with the mechanical order and a note: "Ordered by the same tie-breaks: grace-superchip-analytics first (strong data path), then datafusion-plugin and trainer-side-sql-prep (tied on every tie-break), lakehouse-iceberg, other-accelerators, postgres-gpu-analytics; then fuzzy-join-gpu and gpu-cpu-router." Reorder rows 7–12 to match. If fix 3 is applied, move grace into the thin tier between fuzzy and router instead.
3. **Table row 9 Need, and demand.md:123.** Change "weak" to "thin" for grace-superchip-analytics. demand.md's own rule gives thin for 0 posters, and its reasoning already says "Thin on direct evidence".
4. **Line 24–25.** Replace "13 named posters with large shuffle-and-join bills, and BI serving over billions of rows." with "13 posters, most of them evaluating. One (dogman123) has a large shuffle-and-join bill, and one (Lin) serves BI over a 9.6B-row table."
5. **Line 33–34.** Replace "NVIDIA opened a top-priority effort to replace the reader (#16073)." with "NVIDIA's P0 scale-up epic #16073 includes a native Parquet reader (#16087) and GDS (#16088) among its items." Apply the same correction at line 203: "opened P0 epic #16073 for a native reader" becomes "opened P0 scale-up epic #16073, whose items include a native reader".
6. **Lean, part 1 (after line 171).** Add: "Multi-GPU is not needed for capacity: one H200 or B300 holds 100 GB once compressed; extra GPUs buy latency only, and cost per query is roughly a wash against a 128-core DuckDB box *(estimate)*."
7. **Lean, part 2 (line 178–182).** Replace "The engines being adopted are the ones inside a host" with "The 2026 momentum is inside host engines (though Sirius has no named production users)". Add the bullet: "No plugin vendor has shown it can charge; BlazingSQL queried in place and still went unpaid."
8. **Line 155–157.** Replace "its result that light compression on the GPU tier sped up its whole suite by about 20%" with "its result that moving hot tables to the GPU tier, lightly compressed, sped up its whole suite by about 20% against host-pinned raw data".
9. **Line 121–122.** Replace "Stock DataFusion output already gives silent wrong answers: `LIMIT n` becomes `LIMIT 0` (sirius #1963)." with "Substrait from current producers gives silent wrong answers: `LIMIT n` becomes `LIMIT 0` (sirius #1963, filed for StarRocks; DataFusion 45's output is affected by inference)."
10. **Line 203–204.** Replace "Independent attempts missed their targets or ran slower than CPU (cudf-spark #11992)." with "Independent attempts missed their targets (cudf-spark #11992) or ran slower than CPU (a GKE write-up, spark-rapids #8082)."
11. **Line 252.** Replace "Sirius merged GPU Iceberg support with deletes on 2026-09-15." with "Sirius merged GPU Iceberg scans with position deletes and v3 deletion vectors on 2026-09-15 (equality deletes fall back to DuckDB)."
12. **Line 406–407.** Replace "which Spark refused (cudf-spark #13435)" with "which spark-rapids does not support, as NVIDIA confirmed (cudf-spark #13435)".
13. **Line 186.** Replace "revenue far short of a staff of about 120" with "a missed revenue target against a staff of about 120 (half were cut in 2024)".
14. **Option files.** Fix grace-superchip-analytics.md "demand.md has no row", and align fuzzy-join-gpu.md and gpu-cpu-router.md "Who needs it badly: weak" with demand.md's thin.
