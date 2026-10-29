# PG-Strom (HeteroDB): the incumbent GPU analytics extension for PostgreSQL

Method note: WebSearch budget was exhausted for this session, so no web searches were run. heterodb.com (en/ja) refused connections ("Socket is closed") from both WebFetch and curl. Sources are therefore: the GitHub API (gh), a clone of master at 29349cbfbc27ab7c12952f4880f810ae7d7b39b4 (the docs HTML lives in-repo under docs/, and the markdown sources under man/), the HeteroDB SWDC package index, and KaiGai Kohei's Speaker Deck/SlideShare transcripts. Everything was accessed 2026-10-29. Repo paths below are relative to github.com/heterodb/pg-strom at that SHA.

## Architecture

### Takeaway
PG-Strom is a single-vendor PostgreSQL extension. It uses the CustomScan API (KaiGai wrote that API himself) to replace Scan/Join/Agg nodes with GpuScan, GpuJoin, GpuPreAgg and, since v6.0, GpuSort. Since v5.0 a multi-threaded "GPU-Service" background worker owns the GPUs and runs a hand-written CUDA interpreter over SQL "pseudo-code"; RAPIDS/cuDF is not used. Heap pages are DMA'd raw to the GPU (format KDS_FORMAT_BLOCK), but only blocks that the visibility map marks all-visible. Development is now moving toward Arrow/Parquet files. The draft v6.2 notes remove GPU Cache and GPU-PostGIS.

### Cited Findings
**Execution model**
- KaiGai proposed the Custom-Scan API (scan/join/group-by path hooks) in 2014. PG-Strom began in 2012 as an FDW-based GPU scan that generated CUDA C++ at runtime. — [Speaker Deck, "日本発のOSS爆速DB PG-Strom 開発の歴史と展望", ODC 2025-09-06](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)
- In v5.0 (15-Dec-2023) the codebase was redesigned:
  - The process model moved from multi-process to a multi-threaded background worker called "PG-Strom GPU Service". Backends talk to it over IPC.
  - GPU code is no longer CUDA C++ generated per query and compiled by NVRTC. The GPU now interprets SQL-derived "pseudo-code" (opcodes such as LoadVars, MoveVars, Scan Quals and Group-By KeyHash, all visible in EXPLAIN VERBOSE). This saved about 150 ms of JIT time per query.
  - GPU Cache moved to CUDA managed memory, which allows overcommit.
  - The data layout was reworked for coalesced access.
  - GpuJoin extracts tuples only once.
  - Source: [docs/release_v5.0](https://heterodb.github.io/pg-strom/release_v5.0/) (accessed 2026-10-29; in-repo docs at 29349cbf)
- The GPU-Service returns results to backends over a UNIX domain socket. The v6.1 notes describe this as a cost that the "SELECT INTO Direct" mode avoids. — [docs/release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/) (accessed 2026-10-29)
- All GPU code is PG-Strom's own CUDA, about 75.8k lines of .c/.cu in src/: cuda_gpuscan.cu, cuda_gpujoin.cu, cuda_gpupreagg.cu, cuda_gpusort.cu and xpu_*.cu type libraries (numeric, text, time, JSON, PostGIS). `grep -i "cudf|rapids"` over src/ returns nothing. — [src/ @29349cbf](https://github.com/heterodb/pg-strom/tree/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src) (accessed 2026-10-29)
- Apache Arrow: PG-Strom had its own Arrow reader from v2.2 (2019). v6.1 switched to libarrow/libparquet. — [release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/) (accessed 2026-10-29)

**KDS formats and heap reads**
- src/xpu_common.h defines these KDS (kern_data_store) formats: ROW 'r' (heap tuples), HASH 'h' (inner hash table), BLOCK 'b' ("raw blocks for direct loading"), COLUMN 'c', ARROW 'a', PARQUET 'p' and FALLBACK 'f' (CPU fallback). — [src/xpu_common.h L732-738 @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src/xpu_common.h#L732) (accessed 2026-10-29)
- The GPU device code parses PostgreSQL HeapTupleHeader directly (t_infomask, HEAP_HASNULL, t_xmin) and does limited xmin checks against frozen and in-progress parallel xids. — [src/xpu_common.cu ~L140-222](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src/xpu_common.cu#L140), [src/cuda_gpuscan.cu L620-631 @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src/cuda_gpuscan.cu#L620) (accessed 2026-10-29)
- GPU-Direct reads of heap blocks happen only when `VM_ALL_VISIBLE` holds and the block is not dirty in shared buffers. A source comment says this is "too strict" because the GPU lacks commit-log MVCC checks. Direct reads also cannot cross 1 GB relation segments. — [src/relscan.c ~L620-645 @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src/relscan.c#L620) (accessed 2026-10-29)
- The docs say the same thing: "GPU routines … cannot run MVCC visibility checks per row", so only all-visible blocks go by P2P DMA, and users must VACUUM to build the visibility map. — [docs/ssd2gpu](https://heterodb.github.io/pg-strom/ssd2gpu/) (accessed 2026-10-29)

**GPU-Direct SQL**
- GPU-Direct SQL loads blocks from NVMe straight to GPU memory by P2P DMA over PCIe, using NVIDIA GPUDirect Storage (nvidia-fs, cuFile).
- It also works with NVMe-oF and NFS-over-RDMA, but the docs say these are "not sufficiently tested".
- Before v3.0 it used HeteroDB's own kernel module, nvme_strom, now deprecated.
- It is chosen only when:
  - the table sits on an NVMe device or an md-raid0 of NVMe (or a path set in `pg_strom.manual_optimal_gpus`), and
  - the table is larger than `pg_strom.gpudirect_threshold` (the docs give the default as 2 GB in one place and as "RAM + ⅓ shared_buffers" in another).
- Source: [docs/ssd2gpu](https://heterodb.github.io/pg-strom/ssd2gpu/) (accessed 2026-10-29)
- Multi-GPU: at startup PG-Strom computes PCIe distances between each GPU and each NVMe. A GPU selection policy chooses the GPUs: optimal (nearest), numa or system. EXPLAIN shows e.g. "GPU-Direct with 2 GPUs <0,1>". — [docs/ssd2gpu](https://heterodb.github.io/pg-strom/ssd2gpu/) (accessed 2026-10-29)
- The install guide requires replacing the inbox NVMe driver with NVIDIA DOCA (OFED), disabling IOMMU, and configuring PCI BAR1. — [docs/install](https://heterodb.github.io/pg-strom/install/) (accessed 2026-10-29)

**GPU Cache**
- Mechanism:
  - It keeps a copy of a table in GPU memory. The docs describe it as for "relatively small (~10GB)", frequently updated data such as vehicle or phone locations joined with GPU-PostGIS.
  - An AFTER ROW trigger, `pgstrom.gpucache_sync_trigger()`, copies INSERT/UPDATE/DELETE into a REDO Log Buffer in host shared memory, with no GPU call on the write path.
  - A background "GPU memory keeper" applies the log in batches on the GPU.
  - Any unapplied log is applied at query start, so reads are consistent.
  - The trigger must be ENABLE ALWAYS on replicas.
- Trigger options and defaults:

  | Option | Default |
  |---|---|
  | max_num_rows | 10,485,760 |
  | redo_buffer_size | 160m |
  | gpu_sync_interval | 5 s |
  | gpu_sync_threshold | 25% of the redo buffer |
  | gpu_device_id | 0 |

- `pg_strom.gpucache_auto_preload` builds caches at startup.
- Source: [docs/gpucache](https://heterodb.github.io/pg-strom/gpucache/) (accessed 2026-10-29)
- Draft v6.2 notes (man/release_v6.2.md, dated "20th-Jun-2026"):
  - **Removes GPU-Cache, DPU support, PostGIS support**, and the Pinned Inner Buffer for the case where partitions outnumber GPUs.
  - The stated reason: they "were rarely used in practice, yet they added disproportionate implementation complexity", and the Smart-SSD vendor behind DPU support "has discontinued its business".
  - Source: [man/release_v6.2.md L183-194 @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/man/release_v6.2.md) (accessed 2026-10-29)
  - These notes conflict with the code on master. Master still has src/gpu_cache.c and src/xpu_postgis.cu, and the latest master commit, 29349cbf (2026-10-28), fixes `pgstrom.gpucache_info()` JSON (issue #1065, filed 2026-10-01 by 0-kaz). — [commit 29349cbf](https://github.com/heterodb/pg-strom/commit/29349cbfbc27ab7c12952f4880f810ae7d7b39b4) (accessed 2026-10-29)

**Feature timeline by release**
- v6.0 (1-Apr-2025):
  - GPU-Sort (bitonic) plus row_number/rank/dense_rank window pushdown, only with `pg_strom.cpu_fallback=off`
  - Partitioned Pinned Inner Buffer (GpuScan output stays on the GPU as the join hash table)
  - Arrow_Fdw virtual columns from file names
  - GpuPreAgg final merge on the GPU
  - Source: [docs/release_v6.0](https://heterodb.github.io/pg-strom/release_v6.0/) (accessed 2026-10-29); also [Speaker Deck "PG-Strom v6.0 新機能とその先の未来", 2025-05-23](https://speakerdeck.com/kaigai/pg-strom-v6-dot-0-xin-ji-neng-tosonoxian-nowei-lai) (accessed 2026-10-29)
- v6.1 (18-Nov-2025):
  - Parquet read via Arrow_Fdw. Parquet is decompressed on the CPU by libparquet, so it cannot use GPU-Direct SQL.
  - SELECT INTO Direct: the GPU writes heap blocks straight to storage for CTAS into a new UNLOGGED heap table. On a 316 GB lineorder this took 27.2 s versus 63.8 s with the mode off.
  - Inner hash-table overhead cut from 48 to 24 B/row (an 85 GB inner table became 61 GB).
  - The `cpu_fallback` default changed to off.
  - Source: [docs/release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/) (accessed 2026-10-29)
- v6.2 (unreleased):
  - Parquet NVME Cache: decompressed Parquet column chunks are written asynchronously with O_DIRECT to local NVMe, per row group and per column. Defaults are 160 GB and 2 MB blocks, with LRU eviction. The cache resets on restart. Repeat reads then use GPU-Direct.
  - Roadmap items: direct S3 reads, a remote GPU-Service separated from the DB server ("v7.0?"), and PaaS use.
  - Source: [Speaker Deck "PG-StromのParquetを爆速化するParquet NVME Cache", 2026-05-29](https://speakerdeck.com/kaigai/pg-stromnoparquetwobao-su-hua-suru-parquet-nvme-cache) (accessed 2026-10-29); [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)

**Supported versions and hardware**

| Release | PostgreSQL | CUDA | GPU | CPU arch |
|---|---|---|---|---|
| v6.0 | ≥ 15 | ≥ 12.2 | CC ≥ 6.0 | x86_64 |
| v6.1 | ≥ 15 | ≥ 13 | CC ≥ 7.5 (Turing+) | x86_64 only |
| v6.2 draft | ≥ 16 | ≥ 13 | CC ≥ 7.5 | x86_64 |

- Recommended OS for v6.1: RHEL/Rocky 8/9/10. SWDC ships RPMs pg_strom-PG15…PG18-6.1-0 for el10. — [docs/install](https://heterodb.github.io/pg-strom/install/), [heterodb.github.io/swdc](https://heterodb.github.io/swdc/), [man/release_v6.2.md](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/man/release_v6.2.md) (accessed 2026-10-29)
- PostgreSQL 18 support was added in v6.1 (#919, #945). There is a branch `feat/pg19-varsize-datum-cast`. — [release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/); `gh api repos/heterodb/pg-strom/branches` (accessed 2026-10-29)
- src/Makefile has an aarch64 build flag (float2), but the docs list only x86_64. — [src/Makefile L53 @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/src/Makefile#L53) (accessed 2026-10-29)

### Inferences
- PG-Strom's speed on heap tables depends on all-visible pages, which means VACUUM-fresh, append-mostly tables. Hot OLTP tables fall back to the buffer-manager path. GPU Cache was the answer for hot tables, and it is now being removed. That leaves real-time GPU analytics on live heap tables largely unaddressed by the incumbent.
- The roadmap is moving toward Arrow/Parquet files, cloud object storage and a remote GPU service. It is converging with lakehouse engines rather than staying "accelerate my existing Postgres tables".
- The interpreter design (no cuDF) means every PG type and function must be ported by hand to xpu_*.cu. That keeps the coverage burden on a very small team (inference).

### Gaps
- I did not verify whether the GPU-Cache removal in the v6.2 draft has actually landed on master. The code is still present as of 29349cbf.
- I found no figures for GPU Cache refresh latency or maximum size beyond the docs' "~10GB" guidance.

## Published performance numbers

### Takeaway
HeteroDB's flagship number is about 40 GB/s NVMe→GPU scan, roughly 95% of the hardware limit, reaching about 1.7 billion rows/s on SSBM with Arrow_Fdw. That ran on a 2019-era 4×V100 / 16×NVMe box. Most numbers are SSBM (Star Schema Benchmark), self-run by the vendor and years old. I found no TPC-H/TPC-DS submissions and no independent benchmarks.

### Cited Findings
**2020 webinar results** ([Speaker Deck "Data processing more than billion rows per second", PostgreSQL Webinar Series 2020-10-06](https://speakerdeck.com/kaigai/data-processing-more-than-billion-rows-per-second-unleash-full-capability-of-gpu-and-nvme), accessed 2026-10-29)

| Setup | Workload | Result |
|---|---|---|
| 1U server: 1×Xeon Gold 6126T, 1×Tesla V100 16 GB, 3×Intel P4600 2 TB in md-raid0 (PG 11.2, PG-Strom 2.2devel) | SSB, ~353 GB | 7.3–7.9 GB/s with SSD-to-GPU Direct, against an 8.5 GB/s hardware limit. CPU/filesystem path: about 2.2–2.3 GB/s (about 3×). |
| Same 1U server | SF=1000, 879 GB, 6.0 B rows | PostgreSQL 11.5: about 15–16 M rows/s. PG-Strom on heap: about 52–55 M rows/s. Arrow_Fdw: about 138–500 M rows/s. |
| Supermicro SYS-4029GP-TRT: 4×Tesla V100, 16×Intel P4510 1 TB U.2 in two PCIe JBOFs (51.2 GB/s theoretical) | SSB SF=4000, 3.5 TB, 24 B rows | PostgreSQL: about 44–67 M rows/s. PG-Strom on heap: about 249–258 M rows/s. Arrow: about 658–1,719 M rows/s. About 40 GB/s read from NVMe, about 95% of the hardware limit. |

- The same deck's summary slide claims "100–120 GB/s effective" across 4 units (25–30 GB/s each, 8–10 GB/s physical each). "Effective" counts columnar bytes not read, so it is not physical bandwidth.

**2025 seminar results** ([Speaker Deck v6.0 deck, 2025-05-23](https://speakerdeck.com/kaigai/pg-strom-v6-dot-0-xin-ji-neng-tosonoxian-nowei-lai), accessed 2026-10-29)
- Arrow_Fdw with virtual columns, SSBM Q1.2 variant, on 2 GPUs whose model is not named:

  | Path | Time |
  |---|---|
  | Native PostgreSQL | 116,134 ms |
  | PG-Strom GPU-Direct on heap | 22,404 ms |
  | Arrow | 5,549 ms |
  | Arrow + virtual columns (file pruning) | 142.7 ms |

  The deck calls this "~817×". It is file pruning, not GPU compute. Observed Arrow I/O was about 15–20 GB/s.
- The 2025 SSBM SF=999 rig: Supermicro AS-2015CS-TNR, 1×EPYC 9254, 1×H100 80 GB PCIe, 8×Intel D7-P5510 3.84 TB. The throughput values are only in chart images, which the transcript does not include.

**Recent test rigs**
- 2026 rigs (Parquet NVME Cache deck): "saba" (1×H100 PCIe, 8×D7-P5510 + 4×KIOXIA CD8P-R, ConnectX-6 100 Gb) and "aji" (2×RTX PRO 6000 Blackwell Max-Q 96 GB). With the cache warm, Parquet on NFS is claimed to approach Arrow on local NVMe. No figure is given. — [Speaker Deck, 2026-05-29](https://speakerdeck.com/kaigai/pg-stromnoparquetwobao-su-hua-suru-parquet-nvme-cache) (accessed 2026-10-29)
- Multi-GPU heap scan example: CTAS over 316 GB lineorder, 2.4 B rows, on 2 GPUs took 27.2 s, which implies about 11.6 GB/s (estimate, computed). — [release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/) (accessed 2026-10-29)
- GPU-PostGIS: a 10 M-point × Tokyo-polygon join took 24.9 s on CPU and 1.15 s on GPU (21.5×). Hardware is not given. — [Speaker Deck, Univ. of Tsukuba lecture 2025-02-09](https://speakerdeck.com/kaigai/2025-02-09-ossji-pan-noshang-nishi-zhuang-surugpudetabesu) (accessed 2026-10-29)

### Inferences
- On heap tables the speedups are a modest 3–5× over vanilla PostgreSQL in vendor SSB runs. The headline 10–30× and "billion rows/s" figures require converting data to Arrow, which is effectively a separate columnar store.
- The bottleneck is I/O. PG-Strom's value proposition is NVMe bandwidth, which needs specific hardware topology (PCIe switches, DOCA drivers, IOMMU off). That is hard to get in the cloud, as KaiGai's own 2025 deck notes: an AWS g6e.48xlarge has 4 local SSDs against an estimated 32 needed.

### Gaps
- I found no published PG-Strom number on H100 or Blackwell in text form; the 2025–2026 charts are images. NVMe-oF numbers are absent, and NVMe-oF is listed only as "not sufficiently tested".
- I found no third-party or TPC-audited benchmarks.

## Licence and commercial edition

### Takeaway
The core is under the PostgreSQL License since v3.0 (2021); before that it was GPL v2. The commercially valuable features are gated behind the proprietary `heterodb-extra` module plus a per-GPU-UUID license file: multi-GPU, md-raid0 striping and NVMe-oF for GPU-Direct, and GiST on GPU. Without the module, GPU-Direct SQL falls back to "vfs" CPU emulation. No price is public.

### Cited Findings
- README: "Software is fully open source, distributed under PostgreSQL License." — [README.md @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/README.md) (accessed 2026-10-29)
- The switch from GPL happened in commit 21304d38 (2021-05-26): "PG-Strom v3.0 shall move to the PostgreSQL License, instead of the GNU Public Li[cense]". — [commit 21304d38](https://github.com/heterodb/pg-strom/commit/21304d38988fb2af24cc3a384ed59dffcc8c60ee) (accessed 2026-10-29). The 2020 webinar deck still says GPL-v2.0. — [Speaker Deck 2020-10-06](https://speakerdeck.com/kaigai/data-processing-more-than-billion-rows-per-second-unleash-full-capability-of-gpu-and-nvme) (accessed 2026-10-29)
- The GitHub API reports the licence as "NOASSERTION", because the LICENSE file is a custom PostgreSQL-style text with "HeteroDB,Inc" in place of "the University of California". — `gh api repos/heterodb/pg-strom`; [LICENSE @29349cbf](https://github.com/heterodb/pg-strom/blob/29349cbfbc27ab7c12952f4880f810ae7d7b39b4/LICENSE) (accessed 2026-10-29)
- heterodb-extra adds multi-GPU support, GPUDirect SQL, GiST on GPU and license management.
- Without an activated license these are restricted:
  - multiple GPUs
  - md-raid0 striping for GPUDirect SQL
  - NVMe-oF for GPUDirect SQL
  - GiST on GPU
- The license file is bound to GPU UUIDs (e.g. `"nr_gpus":1, "gpus":[{"uuid":…}]`), with trial serials "HDB-TRIAL", and is installed at /etc/heterodb.license.
- Source for the three bullets above: [docs/install](https://heterodb.github.io/pg-strom/install/) (accessed 2026-10-29)
- "Striping reads from multiple NVME-SSD volumes using md-raid0 or reading from remote disks over high-speed network requires the enterprise subscription provided by HeteroDB,Inc." Without heterodb-extra, `pg_strom.gpudirect_driver` can only be `vfs`, a CPU emulation of GPU-Direct. — [docs/ssd2gpu](https://heterodb.github.io/pg-strom/ssd2gpu/) (accessed 2026-10-29)
- In v6.1 heterodb-extra was made a "Recommends" dependency for OSS-only installs (#929). The current package is heterodb-extra-6.9-1 for el9/el10. — [release_v6.1](https://heterodb.github.io/pg-strom/release_v6.1/); [SWDC](https://heterodb.github.io/swdc/) (accessed 2026-10-29)
- Community support is "volunteer based … best effort and no SLA". Commercial support goes through HeteroDB. — [docs index](https://heterodb.github.io/pg-strom/) (accessed 2026-10-29)
- The business model is selling "PG-Strom Enterprise Edition" plus technical services. — [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)
- Issue #921 (a-hirota, 2025-05) reports "CUDA_ERROR_INVALID_VALUE occurs only after applying HeteroDB license". Issue #883 (2025-03) is about license activation. — [issues](https://github.com/heterodb/pg-strom/issues/921) (accessed 2026-10-29)

### Inferences
- In practice this is open-core. The free edition is single-GPU, single-SSD at best, which is exactly the ceiling of the published headline numbers. Anyone who wants the 40 GB/s configuration pays HeteroDB.
- Licensing is per GPU UUID (estimate of pricing unit, from the license-file format).

### Gaps
- Pricing: heterodb.com was unreachable and none of the decks state a price. No public price was found.

## HeteroDB as a business

### Takeaway
HeteroDB, Inc. (ヘテロDB株式会社) was founded 4 July 2017 in Shinagawa, Tokyo by KaiGai Kohei, an ex-NEC engineer and PostgreSQL contributor (SE-PostgreSQL, FDW, Custom-Scan). The company is essentially KaiGai plus one or two engineers. No funding, staff count or named customers appear in any source I could reach.

### Cited Findings
**Company and founder**
- Founded 2017-07-04, Kitashinagawa, Shinagawa-ku, Tokyo. Business: high-speed DB products and GPU/DB consulting. KaiGai is Chief Architect & CEO. — [Speaker Deck v6.0 deck](https://speakerdeck.com/kaigai/pg-strom-v6-dot-0-xin-ji-neng-tosonoxian-nowei-lai); [2020 webinar deck](https://speakerdeck.com/kaigai/data-processing-more-than-billion-rows-per-second-unleash-full-capability-of-gpu-and-nvme) (accessed 2026-10-29)
- KaiGai's background:
  - joined NEC's HPC division in 2003
  - proposed SE-PostgreSQL in 2007 (merged 2011)
  - SAP alliance assignment in Walldorf, 2013
  - started PG-Strom in 2012 while at NEC
  - founded HeteroDB in July 2017
  - Source: [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)
- His GitHub bio reads "Chief Architect & co-founder at HeteroDB, Inc." — `gh api users/kaigai` (accessed 2026-10-29)
- NEC appears only as his former employer. No current NEC partnership is described in the decks reviewed. — [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)

**Customers**
- Only one anonymised customer appears: a semiconductor fab of an "international top-brand company" holding up to 100 TB of logs in PostgreSQL. Target workloads are IoT/M2M logs in manufacturing, logistics and mobile. — [2020 webinar deck](https://speakerdeck.com/kaigai/data-processing-more-than-billion-rows-per-second-unleash-full-capability-of-gpu-and-nvme) (accessed 2026-10-29)
- The 2025 university lecture says only that PG-Strom is "in use by multiple users" and names none. — [Tsukuba deck 2025-02-09](https://speakerdeck.com/kaigai/2025-02-09-ossji-pan-noshang-nishi-zhuang-surugpudetabesu) (accessed 2026-10-29)
- The docs say "major portion of PG-Strom users are Japanese because of its development history". — [docs index](https://heterodb.github.io/pg-strom/) (accessed 2026-10-29)

**GitHub repository metrics**
- 1,408 stars, 176 forks, 69 open issues, created 2014-04-09, last push 2026-10-28. — `gh api repos/heterodb/pg-strom` (accessed 2026-10-29)
- Releases: GitHub has no Release objects, only tags.

  | Tag | Commit | Date |
  |---|---|---|
  | v6.1 | 4d12ef41 | 2025-11-17 |
  | v6.0 | 2441a6f7 | 2025-03-31 |
  | v5.2.2 | b319f49d | 2024-07-28 |
  | v5.1 | 0e7f29d0 | 2024-04-17 |
  | v5.0 | 96a43361 | 2023-12-16 |

  Latest release: v6.1, commit 4d12ef415759dc48cd4c1421565e9c694b7bd3f9. Makefile.common on master says VERSION 6.2, untagged. — `gh api repos/heterodb/pg-strom/git/refs/tags/*` (accessed 2026-10-29)
- Latest master commit: 29349cbfbc27ab7c12952f4880f810ae7d7b39b4, 2026-10-28, KaiGai Kohei, "pgstrom.gpucache_info() returned wrong JSON string (issue #1065)". — [commit](https://github.com/heterodb/pg-strom/commit/29349cbfbc27ab7c12952f4880f810ae7d7b39b4) (accessed 2026-10-29)
- Commits since 2025-01-01: 436 in total (273 in 2025, 163 in 2026 through Oct 28).

  | Author | Commits | Share |
  |---|---|---|
  | KaiGai Kohei | 372 | 85% |
  | 0-kaz | 61 | 14% |
  | Youhei Tooyama | 2 | <1% |
  | FairyFar | 1 | <1% |

  — `gh api repos/heterodb/pg-strom/commits?since=2025-01-01` (accessed 2026-10-29)
- All-time contributors: kaigai 4,700, 0-kaz 147, onlyindreams 75, then everyone else ≤ 4. — `gh api repos/heterodb/pg-strom/contributors` (accessed 2026-10-29)
- The project's stated policy: "We don't use pull request"; patches go on issues. — [docs index](https://heterodb.github.io/pg-strom/) (accessed 2026-10-29)
- There are 25 GitHub Discussions in total. — `gh api graphql` (accessed 2026-10-29)

### Inferences
- Bus factor is about 1. KaiGai writes about 85% of commits and 4,700 of about 4,940 all-time contributions. 0-kaz is a second engineer, probably HeteroDB staff (estimate: he files many Japanese-language issues and commits regression tests).
- Commercial traction looks small. Signs: no named customers in 9 years, mostly Japanese users, open-core gating on hardware-heavy configurations, and removal of features as "rarely used" (estimate).
- The 2026 cadence of about 16 commits/month shows the project is alive, but it is maintained by one person.

### Gaps
- Funding, investors, revenue, headcount, partner list and Japanese press coverage: heterodb.com was unreachable and WebSearch was unavailable, so none could be checked. Customer case studies on heterodb.com/ja could not be checked either.
- I could not confirm any NEC or other reseller partnership.

## Outside users and the issue tracker

### Takeaway
From 2024-01 to 2026-10, 302 issues were filed (PRs excluded). About 66% came from KaiGai or a handful of Japanese insiders. Outside reporters mostly hit build or install problems, CUDA errors, crashes and wrong-result bugs. The Apache Fineract user (#796) is one person evaluating PG-Strom under a core-banking app, not a production deployment.

### Cited Findings
- 2024–2026 issues by author:

  | Author | Issues | Who |
  |---|---|---|
  | kaigai | 122 | founder |
  | 0-kaz | 65 | second engineer |
  | ytooyama | 33 | Youhei Tooyama, Virtualtech Japan |
  | sakaik | 27 | |
  | qwebug | 19 | almost all "[GPU Logic Bug]" wrong-result reports, mid-2024; fuzzer-style |
  | a-hirota | 8 | Hirota Akio; Arrow/Parquet crashes, licence bug |
  | alefcs23 | 5 | |
  | devrimgunduz | 3 | PGDG RPM packager; PG18 support, CUDA 13 build, libarrow versions |
  | everyone else | 1–2 each | about 20 people |

  Issues per year: 135 in 2024, 106 in 2025, 61 in 2026 through October. — `gh api repos/heterodb/pg-strom/issues?state=all&since=2024-01-01` (accessed 2026-10-29)
- Notable outside reports:
  - #1062, 2026-09-30, by Andres Freund (PostgreSQL committer): "You may not create foreign tables with too many columns"
  - #1054 and #1055, 2026-08: performance regressions where PG-Strom overselects GpuJoin on selective indexed self-joins
  - #1041, 2026-06: GROUP BY hangs with the default compute_60 GPU arch
  - #1039, 2026-06: SIGSEGV in pgstromGpuCacheShutdown()
  - #1016, 2026-03: stuck spinlock in GpuJoinInnerPreload on a simple join
  - #951, 2025-10: PG16 CUDA errors in a parallel worker
  - #944 and #931: build failures on CUDA 12/13
  - #943 and #937, 2025-09: crashes on large Arrow files
  - Source: [github.com/heterodb/pg-strom/issues](https://github.com/heterodb/pg-strom/issues) (accessed 2026-10-29)
- Issue #796 (2024-06-25, IOhacker / "Victor"):
  - A Java/Liquibase migration of an Apache Fineract database (`fineract_default`, `m_import_document`, running alongside a mifosx container) failed. The error: `gpucache: max_num_rows = 10485760 consumes too much GPU device memory (main: 585.00MB, extra: 33.20GB)`, raised on an ALTER TABLE … ADD CONSTRAINT.
  - KaiGai said the 32 GB extra-buffer limit was obsolete and removed it in commit 5e7b418c07d963c1047bd57d1f4be5cb561e7da5. The user confirmed the fix after rebuilding from source. Closed 2024-07-02.
  - The user said "I am now testing and understanding the PG Strom and its technical benefits."
  - Source: [issue #796](https://github.com/heterodb/pg-strom/issues/796) (accessed 2026-10-29)

### Inferences
- The outside user base visible on GitHub is a trickle: about 20–25 distinct external reporters in nearly 3 years, mostly first-time installers. The share of crash and wrong-result reports (stuck spinlocks, SIGSEGV, hangs, a batch of 19 logic bugs) suggests correctness and robustness on arbitrary OLTP schemas is still a weak point.
- #796 shows that GPU Cache interacts badly with ordinary DDL and ORM migrations. That matches the later "rarely used" removal decision.

### Gaps
- I did not establish whether qwebug is an academic DBMS-testing group; no profile information was available. Repeated failures from the GitHub API (as of the last call) stopped further profile lookups for sakaik, qwebug and gdbpbt.

## NVIDIA relationship

### Takeaway
The relationship is light and dates from 2017. KaiGai won the Inception Award at GTC Japan 2017 and had a Top-5 poster at GTC 2017. The product depends on NVIDIA GPUDirect Storage (cuFile, nvidia-fs) and DOCA/OFED drivers. I found no evidence of a current co-marketing, partner-program or GTC-talk relationship in 2024–2026.

### Cited Findings
- GTC 2017 poster selected Top-5. Inception Award at GTC Japan 2017, "selected from 19 AI startups". — [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang); [Tsukuba deck](https://speakerdeck.com/kaigai/2025-02-09-ossji-pan-noshang-nishi-zhuang-surugpudetabesu); [v6.0 deck](https://speakerdeck.com/kaigai/pg-strom-v6-dot-0-xin-ji-neng-tosonoxian-nowei-lai) (accessed 2026-10-29)
- In 2021, CUDA 11.4 GPUDirect Storage made the custom nvme_strom driver unnecessary. — [ODC 2025 deck](https://speakerdeck.com/kaigai/ri-ben-fa-noossbao-su-db-pg-strom-kai-fa-noli-shi-tozhan-wang) (accessed 2026-10-29)
- From v3.0, GPU-Direct SQL uses NVIDIA GDS (nvidia-fs). — [docs/ssd2gpu](https://heterodb.github.io/pg-strom/ssd2gpu/) (accessed 2026-10-29)

### Inferences
- NVIDIA's own investment in the "SQL on GPU" space goes through RAPIDS/cuDF and Spark-RAPIDS, which PG-Strom does not use. PG-Strom is not part of NVIDIA's ecosystem push (estimate).

### Gaps
- I could not check whether HeteroDB remains an NVIDIA Inception member or has given GTC talks after 2017, because web search was unavailable.
