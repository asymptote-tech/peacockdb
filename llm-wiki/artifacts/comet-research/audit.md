# Audit of comet-gpu.md

Auditor: fresh Explore subagent, 2026-11-04. Inputs: the report and the artifact folder only.

## Summary
- The arithmetic holds. `cost.md` regenerates byte for byte. My hand computation from the raw files gives the same four sf40 numbers: TPC-H 1.88 (sharing 2.38, device 0.79) and TPC-DS 2.14 (2.42, 0.88). Every number I traced from the report into the tables matched.
- One measurement defect changes the numbers. The S3 sequential "all" batch times also count the time spent dumping plans to disk: 10.7 s for TPC-H and 69.7 s for TPC-DS. This makes Comet look slower on S3, which favours the GPU. With it removed, TPC-DS sf40 falls from 2.14 to about 2.06 and TPC-DS 1 TB from 1.58 to about 1.44. The verdict class stays "conditional go".
- Other defects: a false claim about the sensitivity tables, a mislabelled data size ("1 TB" is about SF2,600 for TPC-DS), H200 decode rates applied to the H100 and tagged `published`, and a TPC-DS baseline that is modelled but presented as a concurrent run.
- Findings: 1 blocking, 4 should fix, 7 minor.
- Sources: 15 web sources drawn at random. 9 support the sentence that cites them, 6 partly support it, none fail to support it. Nothing rests on spark-plugin.md alone.

## Findings

1. **Blocking. The S3 sequential batch times include plan-dump time.**
   - **Location:**
     - Report "How far the NIC already limits Comet on CPU": "TPC-H sequential | ... | 84.7 s | 1.42-1.81", "TPC-DS sequential | 122.3 s | 265.6 s | 2.17" and "Sequential runs read at 0.32-0.49 of it".
     - `inputs.tsv` rows `s3_efficiency` (low), `s3_efficiency_sequential` and `cpu128_s3_concurrent_all_s.tpcds`.
     - The headline TPC-DS cells, every "sequential, all" ratio, and the settling-measurements row "TPC-DS 1 TB ... 165.9 s".
   - **What is wrong:**
     - In `scripts/comet_run.py:430-440`, `sequential()` starts the batch timer, then calls `query()`, which runs `dump_plan` after each query. The comment says "the dump is not timed", but it is inside the batch timer.
     - The two S3 sequential runs were the only timed runs that used `--dump-plans` (their `conf.txt` argv shows it).
   - **Evidence:** batch_ms minus the sum of the per-query times, from `measurements/comet/summary.tsv`:

     | run | uses `--dump-plans` | batch minus per-query sum |
     |---|---|--:|
     | tpch-s3-sequential-ball | yes | 84,734.9 − 74,058 = 10,677 ms |
     | tpcds-s3-sequential-ball | yes | 265,615 − 195,958 = 69,657 ms |
     | cal-tpch | yes | 9,953 ms |
     | cal-tpcds | yes | 62,242 ms |
     | local runs, same mode | no | 11-221 ms |

   - **Impact:**
     - With the dump removed, the S3 sequential batch times are about 74.1 s (TPC-H) and 196.2 s (TPC-DS).
     - The TPC-H ratio of S3 penalties becomes 10.43/20.84 = 0.50 instead of 0.331.
     - `cpu128_s3_concurrent_all_s.tpcds` becomes about 124.4 s [87.5-161.3] instead of 134.9 s [87.5-230.8].
     - `s3_efficiency_sequential` becomes 0.560 [0.431-0.892], and the low end of `s3_efficiency` becomes 0.560.
   - **Recomputed cells:** I re-ran `cost_model.py` on these corrected values, changed in memory only; no file was modified.

     | cell | report | corrected (about) |
     |---|---|---|
     | TPC-H sf40, faster baseline | 1.88 | 1.88 |
     | TPC-H sf200, faster baseline | 1.38 | 1.38 |
     | TPC-DS sf40, faster baseline | 2.14 [0.54-18.10] | 2.06 [0.55-13.33] |
     | TPC-DS 1 TB, faster baseline | 1.58 [0.06-37.40] | 1.44 [0.06-23.01] |
     | TPC-H sf40, sequential "all" | 2.94, device 1.24 | 2.78, device 1.17 |
     | TPC-DS sf40, sequential "all" | 3.51, device 1.45 | 3.02, device 1.25 |

     The S3 ÷ local ratios become 1.24-1.58 for TPC-H and 1.60 for TPC-DS, and sequential runs read at 0.43-0.56 of the best S3 rate. The error biases the CPU baseline against Comet.
   - **Fix:** use the per-query sums for the two S3 sequential "all" times, or rerun both without `--dump-plans`. Move `dump_plan` outside the batch timer. Regenerate `cost.md` and update every number above.

2. **Should fix. "Decode and GPU compute do not appear among the top inputs" is false.**
   - **Location:** report section 6, "What turns the conditional go into a go", the bullet beginning "Decode and GPU compute do not appear". The same sentence is in `design.md` section 1.
   - **What is wrong:** `cost.md` "Sensitivity of the headline cells" for **tpcds sf40** lists `gpu_speedup` as its seventh row: 1.72 at the low end, 2.14 at the high end. At the low end, the only cell whose mid is above 2 drops below 2.
   - **Evidence:** `cost.md`, tpcds sf40 table, row `gpu_speedup | 1.72 | 2.14 | modelled`. The bullet's own number, "1.72 and 1.88" for TPC-H sf40, is correct: I checked it, and that row ranks 11th, outside the top 8.
   - **Fix:** say that GPU speed does not matter for the TPC-H cells but can decide the TPC-DS sf40 cell.

3. **Should fix. "1 TB" is not SF1000, and the "overstates by about 2×" statement is wrong.**
   - **Location:**
     - Report verdict table: "TPC-DS | 1 TB (first size in the grid past host RAM)".
     - "By data size and by deployment": "so the 1 TB cell overstates the size by about 2×".
   - **What is wrong:**
     - The model's sizes are compressed bytes: `cost_model.py:459` sets "1 TB" to 1e12 B.
     - For TPC-DS that is 1e12 / 15,261,887,937 × 40, about **SF2,620**. For TPC-H it is about SF943.
     - The host-RAM crossing is 200 GB, about sf524. So the 1 TB cell is about **5×** past the crossing, not 2×.
     - Readers will take "TPC-DS 1 TB" to mean SF1000. The report itself quotes Comet's SF1000 results.
   - **Evidence:** `measurements/footers/sf40.tsv`, TPC-DS total 15,261,887,937 B; `cost_model.py` lines 457-459.
   - **Fix:** label the column "1,000 GB compressed (TPC-DS ≈SF2,600, TPC-H ≈SF940)" and change "about 2×" to "about 5×". Do the same for the 10 TB column.

4. **Should fix. `decode_rate` is a derived H200 figure, tagged `published` and applied to the H100.**
   - **Location:** report "The scalar inputs", row `decode_rate | 4.1 [3.35-18.8] | ... | published`; "Where the GPU stalls": "19 of 22 queries stall below 4.1 GB/s (32.8 Gbit/s)".
   - **What is wrong:**
     - The value is a median of ratios built from benchmark-minimal's **H200** load times over this task's footer bytes. The high end comes from **GB10's** chunked reader.
     - Under the report's own tag rules ("`modelled` means an assumption or a derived estimate"), this is `modelled`.
     - The model then uses it as the H100 node's decode rate. `data-path.md` line 20 says the H100 has about 70% of the H200's memory bandwidth.
     - So the stall threshold of 4.1 GB/s is an H200 number. On an H100 it would plausibly be about 2.9 GB/s (estimate).
     - `gpu_measured_scale` has the same problem: its high end of 1.73 is a derived H200 scaling but is tagged `published`.
   - **Evidence:** `inputs.tsv` rows 10 and 12; `data-path.md:20`.
   - **Fix:** retag both rows `modelled`, state the H200-to-H100 step, and widen the low end of `decode_rate` for H100 memory bandwidth. Decode is not binding at the NIC rate, so the headline numbers will not move, but the stall table will.

5. **Should fix. The TPC-DS headline baseline is a modelled time, presented as "Comet's concurrent run".**
   - **Location:** report "The verdict": "against the faster CPU baseline (Comet's concurrent run)".
   - **What is wrong:**
     - No TPC-DS concurrent S3 run exists. Both TPC-DS headline cells use the `modelled` input `cpu128_s3_concurrent_all_s.tpcds`.
     - The verdict mentions this only for the 1 TB row. The sf40 cell is the only one with a mid above 2, and it rests on the same input: 1.81 at its low end (`cost.md` tpcds sf40 sensitivity, row 3).
     - That input is also inflated by finding 1.
   - **Fix:** in the verdict, say that the TPC-DS baseline is a modelled concurrent S3 time.

6. **Minor. The "1.4×" explanation is wrong.**
   - **Location:** "The verdict": "because concurrent Comet is 1.4× faster than sequential Comet".
   - **What is wrong:** in the cells it explains, the modelled CPU times differ by 74.86/47.94 = 1.56× (TPC-H) and 191.48/116.59 = 1.64× (TPC-DS). The measured S3 ratio for TPC-H is 84.7/46.5 = 1.82×. The 1.4× figure is the RAM-resident ratio: 1.40 for TPC-DS and 1.48 for TPC-H.
   - **Fix:** quote the ratio that applies to these cells, or say it is the RAM-resident one.

7. **Minor. Some method differences are not named as departures.**
   - **Location:** "The verdict": "Two departures from Comet's benchmark config".
   - **What is wrong:** the sentence leaves out several differences from Recipe 3 and between runs:
     - Recipe 3 puts `SPARK_LOCAL_DIRS` on local disk; the runs put it on tmpfs (`scripts/comet_setup.sh:15-16,34,88`). This is stated in the "Local means RAM-resident" bullet, but not as a departure.
     - Only the TPC-H S3 sequential run had a `--warmup-query q1`. All other runs used `--warmup 0`.
     - Both S3 sequential runs dumped plans during the run (finding 1).
     - `spark.comet.explain.fallback.enabled` and `extendedExplainProviders` were on in every run.
   - **Evidence:** the argv and conf lines in `measurements/comet/*/conf.txt`.
   - **Fix:** list these with the two named departures.

8. **Minor. One-pass compares a cold S3 run with the faster of two local runs.**
   - **Location:** "The verdict": "The one-pass floor is below 1 ... 0.81 [0.26-11.94] for TPC-H".
   - **What is wrong:**
     - For each query, the local TPC-H time is the faster of a cold and a warm run (`cost_model.py:417`). The S3 time is one run in a fresh JVM.
     - So the per-query S3 penalty includes JVM warm-up beyond q1. Run 1 minus run 2 was 12.5 s over the whole pass.
     - This biases the result toward the GPU. The conclusion that one-pass is below 1 still holds.
   - **Fix:** state the asymmetry, or use the first local run for TPC-H one-pass.

9. **Minor. The GDS tier rate counts four GPUs as one.**
   - **Location:** "Exotic architectures": the row "GPUDirect Storage on VAST, WEKA, DDN; S3 over RDMA | 22-47" in the "GB/s to one GPU" column.
   - **What is wrong:** gds-report.pages.dev gives 26.4 GiB/s (28.3 GB/s) into one GPU. Its 43.4 GiB/s (46.6 GB/s) is four GPUs together. It has no WEKA or DDN numbers. `exotic.md:99` uses 46.6 as the per-GPU rate.
   - **Fix:** use 28.3 GB/s for one GPU. The verdict is unaffected, because these tiers are bound by the 16-core CPU side.

10. **Minor. Three citations carry only part of their claim.**
    - "Data on locally attached SSD": AWS `so.html` gives i4i.4xlarge's 9.375/25 Gbps network, but not its "2.8 GB/s" read. That figure comes from the i4i launch blog, `data-path.md` [S17]. Fix: cite that blog.
    - "GDS on cloud VMs runs in compatibility mode, a copy through host memory": the release notes support compatibility mode in cloud-provider environments, but the host-memory copy comes from `data-path.md` [S41]. Fix: cite [S41].
    - "They cost extra at low concurrency: always-share was 2.5× worse": in Johnson et al., 2.5× is an average gain over always-share, not a low-concurrency figure. Fix: drop "at low concurrency" or cite where it comes from.

11. **Minor. The q6 reader rate is rounded the wrong way.**
    - **Location:** "Where the GPU stalls": "peacockdb's reader today, about 3.8 GB/s compressed on q6".
    - **What is wrong:** `inputs.tsv` (`decode_rate` source) gives q6 as 1.54 GB in 398 ms = 3.87 GB/s, from the footers. The 3.8 figure comes from `data-path.md`'s 1.5 GB lead.
    - **Fix:** write "about 3.9 GB/s".

12. **Minor. A source note describes the abandoned tmpfs.**
    - **Location:** `inputs.tsv` row `cpu_ram_usable`: "Task 4's tmpfs was 252 GiB of 503".
    - **What is wrong:** 252 GiB is the default `/dev/shm` size. The report says `/dev/shm` was replaced by a dedicated tmpfs, which `scripts/comet_setup.sh:16` mounts with `size=320g`.
    - **Fix:** correct the source note.

## Checks

### Check 1. Sources
**Method.**
- I listed the 38 unique web URLs cited in the report body, everything before "## Sources", numbered by first appearance.
- I drew 15 with `random.seed(20261104); random.sample(range(1,39),15)`, which gave [1, 2, 3, 14, 15, 18, 19, 20, 21, 23, 24, 25, 28, 32, 37].
- I opened each with WebFetch. Outside the sample I also checked the GH200 price page; it supports the claim (Lambda $2.29, Vultr $1.99).

| # | report section | URL | verdict | note |
|---|---|---|---|---|
| 1 | Versions pinned | https://github.com/apache/datafusion-comet/releases/tag/1.2.0 | partly | Commit ab46419f, released 03 Nov, DataFusion 55.2.0 confirmed. arrow-rs 59.2.0, Arrow Java 18.3.0 and Spark 4.1.3 are not on the page; they come from comet-internals.md (Cargo.toml, pom.xml). The page lists Spark 4.1.4 among supported versions. |
| 2 | What a reader must know (NIC rates) | https://docs.nebius.com/compute/virtual-machines/types.md | supports | No NIC rate for 1gpu-16vcpu-200gb, cpu-d3 or L40S. It lists a "ConnectX-6 100 Gbps" adapter for the H100 type, as notes.md says. Preset is 16 vCPU, 200 GiB. |
| 3 | Scalar inputs, price_h100_node | https://nebius.com/prices | supports | HGX H100 $4.50 per GPU-hour, 16 vCPU, 200 GB. |
| 14 | Prior art, evidence against | https://github.com/NVIDIA/cudf-spark/issues/15847 | supports | "we don't see any usage of this feature"; hybrid execution removed in PR #15849. |
| 15 | Prior art, evidence against | https://perspectives.nvidia.com/cudf/faq/...6627a6/ | supports | "Engine teams adding GPU execution without forking mainline can use Velox". The page says it holds AI-generated content. |
| 18 | Getting more out of the wire | https://arxiv.org/html/2512.02189 | supports | B200 Snappy 117.24 GB/s output. "software-only decompression of ... the H100". |
| 19 | Getting more out of the wire | https://learn.microsoft.com/en-us/fabric/data-engineering/intelligent-cache | supports | "improving performance up to 60% on subsequent reads". On by default. |
| 20 | Caching in RAM | https://github.com/apache/spark/blob/master/.../SharedState.scala | supports | `val cacheManager: CacheManager` inside SharedState. Link is to master, not pinned. |
| 21 | Data on locally attached SSD | https://docs.aws.amazon.com/ec2/latest/instancetypes/so.html | partly | i4i.4xlarge network 9.375/25 Gbps (1.17 GB/s baseline) and 1×3750 GB NVMe confirmed. 2.8 GB/s not on the page (finding 10). |
| 23 | Data on locally attached SSD | https://docs.nvidia.com/gpudirect-storage/release-notes/index.html | partly | "cuFile APIs can be used in Cloud-service-providers environments in compatibility mode". Host-memory copy not stated (finding 10). |
| 24 | Data on locally attached SSD | https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instance-store-lifetime.html | supports | Data does not persist when the instance is stopped; blocks are erased. |
| 25 | Local SSD as a product option | https://hackernoon.com/avoiding-data-silos-in-presto-in-meta-from-raptor-to-raptorx | supports | Raptor replaced by RaptorX. The HackerNoon copy is dated 2022-08-30; the original Presto post is 2022-01-28, the date the report gives. |
| 28 | Local SSD as a product option | https://docs.databricks.com/aws/en/optimizations/disk-cache | supports | SSD workers "are enabled and configured for disk caching". |
| 32 | Exotic architectures | https://gds-report.pages.dev/ | partly | One L40S: VAST 26.4 GiB/s, local NVMe 24.3 GiB/s. 43.4 GiB/s is 4 GPUs. No WEKA or DDN (finding 9). |
| 37 | Batched query execution | https://www.vldb.org/conf/2007/papers/research/p351-johnson.pdf | partly | Model beats always-share "by 2.5x" on average. Not tied to low concurrency; QPipe and CJOIN figures not in this paper (finding 10). |

### Check 2. Arithmetic
**Method.**
- I worked from `inputs.tsv`, `measurements/comet/summary.tsv`, `measurements/comet/bytes.tsv`, `measurements/footers/sf40.tsv`, `plans/eligible.tsv` and `plans/columns.tsv`, with a `python3 -c` script of my own that follows `cost.md` "The model".
- I did not call `ratio` or `evaluate` for this check.
- I read `cost_model.py` to settle six points:
  - batch CPU times are the mean of runs (`load_inputs`, l.388);
  - per-query local times are the minimum of runs; driver_ms and scheduler_ms are the minimum, scan and shuffle bytes the last row (l.390-397);
  - the CPU `fixed` term is the sum of per-query overheads, capped at the batch time (l.229);
  - for 8×16, S3 bandwidth is min(8 × 1.0, 20) GB/s, and the NIC for shuffle is the same per-node rate (l.134-135);
  - the "faster" baseline is the elementwise minimum of the sequential and concurrent CPU times;
  - the sharing factor divides GPU time reading alone by GPU time with shared scans.

**TPC-H sf40, 8×16, batch "all"** (22 queries)
- Inputs:
  - Σ local per query 46.812 s; Σ scan 48.297 GB; footer union 9.102 GB; Σ shuffle 46.373 GB.
  - Σ driver 2.092 s; Σ scheduler 22.907 s; Σ overhead 7.819 s, of which a batch pays half, 3.909 s.
- GPU terms:
  - Compute: 3.818 s (measured for q1, q3, q6, q8; elsewhere work × eligible fraction / 10.1). cpu_part: 1.778 s (q2, q20). Shuffle: 46.373 / 4 = 11.593 s.
  - Wire rate: 1.25 × 0.892 = 1.115 GB/s. Shared: wire 8.163 s, decode 2.220 s. Alone: wire 43.316 s, decode 11.780 s.
  - GPU time shared 25.443 s, alone 60.596 s, so **sharing = 2.382**.
- CPU, concurrent: local 36.067 s (mean of two runs), S3 46.495 s. fixed 7.819 + work 31.073 (×1.1) + S3 penalty 3.978 (10.43 × (1.164/8)^0.5) + shuffle 5.073 = 47.941 s.
- CPU, sequential: local 53.230 s, S3 84.735 s, CPU time 74.860 s.
- Results:
  - Faster baseline: ratio **1.884**, device **0.791**. cost.md says 1.88, s2.38, d0.79. Match.
  - Sequential: ratio 2.942, device 1.235. cost.md says 2.94, d1.24. Match.

**TPC-DS sf40, 8×16, batch "all"** (82 queries)
- Inputs:
  - Σ local per query 122.135 s; Σ scan 98.390 GB; footer union 12.198 GB; Σ shuffle 56.505 GB.
  - Σ driver 13.325 s; Σ scheduler 103.366 s; Σ overhead 39.148 s, batch pays 19.574 s.
- GPU terms:
  - Compute 8.094 s; cpu_part 9.929 s; shuffle 14.126 s.
  - Shared: wire 10.940 s, decode 2.975 s. Alone: wire 88.242 s.
  - GPU time shared 54.569 s, alone 131.871 s, so **sharing = 2.417**.
- CPU, concurrent: local 87.495 s, S3 134.9 s (modelled). CPU time 116.592 s.
- CPU, sequential: local 122.322 s, S3 265.615 s. CPU time 191.478 s.
- Results:
  - Faster baseline: ratio **2.137**, device **0.884**. cost.md says 2.14, s2.42, d0.88. Match.
  - Sequential: ratio 3.509, device 1.452. cost.md says 3.51, d1.45. Match.

**"Where the GPU stalls" rows, recomputed**

| query | scan | decode | compute | stall rate | stall with fast decode | cost.md |
|---|---|---|---|---|---|---|
| q1 | 1.672 GB | 1.672 / 4.1 = 0.408 s | 0.2695 s (measured H100) | 1.672 / 0.408 = 4.10 GB/s, 32.8 Gbit/s; wire-bound (4.10 > 1.115) | 1.672 / max(0.089, 0.2695) = 6.20 GB/s | match |
| q8 | 5.193 GB | 1.267 s | 0.040 s | 4.10 GB/s | 18.80 GB/s | match |

I also checked q14, q15 and q16: 1.52, 2.88 and 1.55 GB/s, all matching.

**Units.**
- Gbit/s and GB/s: all conversions in the report check out (10-100 Gbit/s = 1.25-12.5 GB/s; 6.25-32 = 0.78-4; 3.125-16 = 0.39-2; 4.1 GB/s = 32.8 Gbit/s; 0.917 GB/s = 7.33 Gbit/s; 1.164 GB/s = 9.31 Gbit/s).
- GiB/s vs GB/s: one slip, in the GDS tier (finding 9).
- H200 vs H100: two slips (finding 4).

**Regeneration.** `python3 cost_model.py | diff - cost.md` printed nothing and returned rc=0. `cost.md` regenerates byte for byte.

### Check 3. Measured inputs
| input | file | result |
|---|---|---|
| `rate_cpu128_s3` 1.164 | `measurements/s3/summary.tsv` (4 threads; 16 threads 1.163) | matches |
| `s3_efficiency` 0.892 [0.49-1.0] | `summary.tsv`: 48.297 GB / 46.495 s / 1.164 = 0.892 | value matches. The low end of 0.49 rests on the inflated 84.73 s (finding 1) |
| `s3_efficiency_sequential` 0.490 [0.318-0.892] | 48.30 / 84.73 / 1.164 = 0.490; 98.39 / 265.62 / 1.164 = 0.318 | arithmetic matches; both inflated by plan-dump time (finding 1) |
| `cpu_run_spread` [0.881-1.119] | 59.569 and 46.891 s about a mean of 53.230 s; concurrent 0.939-1.061 | matches |
| `cores_cpu` 128 | `tpch-local-sequential-ball/conf.txt` `spark.executor.cores=128` | matches |
| `mem_cpu128_gb` 320 | same file: off-heap 256g + heap 64g | matches |
| `cpu_local_s.tpch` 46.812 / `.tpcds` 122.135 | sum of per-query minimums | matches |
| `cpu_s3_s.tpch` 74.058 / `.tpcds` 195.958 | sum of per-query S3 times | matches |
| `scan_bytes`, `shuffle_bytes` (both datasets) | `bytes.tsv` | 48.297 / 46.373 GB and 98.390 / 56.505 GB; match |
| `driver_s`, `scheduler_s` | `bytes.tsv` | 2.092 / 22.907 s and 13.325 / 103.366 s; match |
| `footer_bytes` | `footers/sf40.tsv` | 42,430,863,567 and 15,261,887,937 B; match |

`eligible_fraction` is correctly tagged `modelled`.

### Check 4. CPU baseline fairness
- **Config against Recipe 3** (`comet-internals.md` "Recipes" 3; `measurements/comet/*/conf.txt`):
  - These match Recipe 3's single-executor plan: one executor with 128 cores, driver 16g, heap 64g, memoryOverhead 48g, shuffle partitions 256, CometPlugin, CometShuffleManager, `Cast.allowIncompatible=true`, Profile credentials provider.
  - Named in the report: off-heap 256g instead of 320g (still 2 GB per core, Comet's ratio) and `spark.cleaner.periodicGC.interval=5min`.
  - Not named as departures: Spark's local dirs on tmpfs instead of local disk, the fallback-explain flags, the q1 warm-up in one run only, and plan dumping in the timed S3 runs (finding 7).
  - The `fs.s3a.vectored.read.*` keys appear in every run, local and S3, so they create no local/S3 difference.
- **Both modes:** TPC-H ran sequential and concurrent, local and S3. TPC-DS ran both modes locally but only sequential on S3; the report says the concurrent S3 run was not in the spec's matrix. Every `rc` is 0 and `skipped.tsv` has only its header. The nine configurations and two calibration passes are all present.
- **Scan parity:** no `fallbacks.tsv` has a scan row.
  - TPC-H: local (`plans/tpch/fallbacks.tsv`) and S3 are identical.
  - TPC-DS: they differ only at q31 (82 vs 84 ops), q39 (49 vs 48 ops) and q35 (an expression id), as the report says.
- **In-region:** the S3 runs set `fs.s3a.endpoint=https://storage.eu-west1.nebius.cloud:443`, and the VM is in eu-west1 (`s3/notes.md`). Confirmed.
- **"Local" description:** honest. Data and local dirs were on tmpfs (`comet_setup.sh`), the summary labels say "RAM-resident on tmpfs", and the network-disk rates are measured.
- **Unfair to the CPU:** the plan-dump time in the S3 sequential batches (finding 1, blocking) and the cold-vs-faster comparison in one-pass (finding 8). Both favour the GPU.

### Check 5. Presentation
- `decode_rate` and the high end of `gpu_measured_scale` are derived estimates but tagged `published` (finding 4).
- The TPC-DS concurrent baseline is modelled but presented as "Comet's concurrent run" (finding 5).
- `s3_efficiency` is a ratio of two measurements; tagging it `measured` is fair.
- No modelled per-query table is shown as measured.
- spark-plugin.md: `prior-art.md` "Method and versions" lists seven re-checked leads, which cover the report's five (CPU-only Comet, Gluten-cuDF 48/103, the open native-reader epic, the removed hybrid scan, the experimental cache). The hybrid-scan removal also holds on the web (#15847 above). `data-path.md`'s "NVIDIA's own reader rework is P0 (spark-plugin.md lead)" is not used in the report. I found no fact that rests on spark-plugin.md alone.

### Check 6. Consistency
- **Verdict table:** all four rows match `cost.md` "Headline cells". The bullets match it too: one-pass medians 0.81 and 0.83, worst 0.38 and 0.20, device factors 0.42-0.88, sharing factors 2.38-3.74, shared-CPU mids 1.76, 1.23, 1.85, 1.02.
- **Settling measurements:** I bisected the breakpoints in memory and they agree with `design.md`: 4.6 GB/s gives 2.003; 9.6 GB/s gives 2.003; scheduler share 0.32 gives 2.001; 165.9 s gives 2.000. The 165.9 s target is affected by finding 1.
- **"Batched and one-pass" table:** all 14 cells match `cost.md`'s tpch and tpcds 8x16 tables.
- **"By data size" table:** all 24 values and the fit labels match. The "1 TB" label is the problem (finding 3).
- **"Other tiers":**
  - GH200 4.09, 3.20, 3.74, 3.81 match.
  - "Faster feeds only reach 2.1-2.3" matches nic40, nic400, the GDS rows and nebius_shared_fs.
  - Local NVMe 1.49 and 1.47 match.
- **"Listed beside" table:** GETs 62,809 and 260,416, plus 2,182 for the q1 warm-up = 64,991, match `bytes.tsv`. Wire 48.3/9.1 and 98.4/12.2 GB match.
- **Batching table:** all wire, cut and per-query numbers match `cost.md` "Listed beside the ratio".
- **Experiment 1 table:** matches `s3/summary.tsv` and `s3/notes.md`. "15-19× slower" is 14.8× and 18.5×.
- **Experiment 2 table:** the times match `summary.tsv`, but the S3 sequential values are inflated (finding 1). The cache runs (53.3 and 36.1 s against 53.2 and 36.1 s), the 21% speed-up of the second run, no spill (`spill_bytes` is 0 in every row), shuffles of 46.4 and 56.5 GB, and the `footer-check.tsv` ratios all match.
- **Translatability:**
  - Operators 2,647 supported, 2,374 adaptable, 0 missing; expressions 26,944, of which 93.2% supported, 6.7% adaptable and 20 missing; 1,625 ANSI expressions (all from `plans/census.tsv` and the plan texts).
  - 96 of 104 queries fully on the GPU; the others keep 2-29% of operators on the CPU (`eligible.tsv`).
  - All match.
- **VM-hours table:**
  - cpu128 03:54:22-04:33:49 = 0.66 h, so 1.34 h of the 2 h cap is unused.
  - l40s by the log: 0.05 + 0.28 = 0.33 h. Wall time 0.38 h.
  - All match `vm-log.tsv`. The 03:13:39 stop time comes from the run state under tasks/, which I could not read; the log shows "found STOPPED" at 03:13:46.

## Sources
- Report: `/home/dmitry/workspace/peacockdb-alpha/llm-wiki/reports/comet-gpu.md`
- Artifact files (under `/home/dmitry/workspace/peacockdb-alpha/llm-wiki/artifacts/comet-research/`):
  - `cost.md`, `cost_model.py`, `inputs.tsv`, `design.md`
  - `comet-internals.md` (Versions, Recipes 1-5), `prior-art.md` (Method and versions), `data-path.md` and `exotic.md` (grep), `translatability.md` (grep)
  - `measurements/comet/summary.tsv`, `bytes.tsv`, `footer-check.tsv`, `skipped.tsv`, `calibration.tsv`
  - `measurements/comet/*/conf.txt`, `rc`, `status`, and `fallbacks.tsv` for both S3 sequential runs
  - `measurements/s3/summary.tsv`, `measurements/s3/notes.md`, `measurements/vm-log.tsv`, `measurements/footers/sf40.tsv`
  - `plans/eligible.tsv`, `plans/columns.tsv`, `plans/census.tsv`, `plans/tpch/fallbacks.tsv`, `plans/tpcds/fallbacks.tsv`, `plans/*/*.txt` (ANSI count)
  - `scripts/comet_run.py` (lines 400-525), `scripts/comet_setup.sh` (grep)
- Web:
  - https://github.com/apache/datafusion-comet/releases/tag/1.2.0 (accessed 2026-11-04)
  - https://docs.nebius.com/compute/virtual-machines/types.md (accessed 2026-11-04)
  - https://nebius.com/prices (accessed 2026-11-04)
  - https://github.com/NVIDIA/cudf-spark/issues/15847 (accessed 2026-11-04)
  - https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/ (accessed 2026-11-04)
  - https://arxiv.org/html/2512.02189 (accessed 2026-11-04)
  - https://learn.microsoft.com/en-us/fabric/data-engineering/intelligent-cache (accessed 2026-11-04)
  - https://github.com/apache/spark/blob/master/sql/core/src/main/scala/org/apache/spark/sql/internal/SharedState.scala (accessed 2026-11-04)
  - https://docs.aws.amazon.com/ec2/latest/instancetypes/so.html (accessed 2026-11-04)
  - https://docs.nvidia.com/gpudirect-storage/release-notes/index.html (accessed 2026-11-04)
  - https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instance-store-lifetime.html (accessed 2026-11-04)
  - https://hackernoon.com/avoiding-data-silos-in-presto-in-meta-from-raptor-to-raptorx (accessed 2026-11-04)
  - https://docs.databricks.com/aws/en/optimizations/disk-cache (accessed 2026-11-04)
  - https://gds-report.pages.dev/ (accessed 2026-11-04)
  - https://www.vldb.org/conf/2007/papers/research/p351-johnson.pdf (accessed 2026-11-04)
  - https://www.amcompute.com/gpu-cloud-prices/gh200 (accessed 2026-11-04)
