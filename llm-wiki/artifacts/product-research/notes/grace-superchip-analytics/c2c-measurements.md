# NVLink-C2C on Grace superchips: measured numbers for analytics (accessed 2026-10-29)

Method note: the session-wide WebSearch budget ran out after one search. Everything below comes from directly fetched primary sources (all accessed 2026-10-29). Coverage of GB200/GB300 measurements and of DB-specific GH200 join papers is therefore thin. See the Gaps sections.

## 1. Measured H2D copy and GPU-reads-host bandwidth (GH200 / GB200 / GB300 / GB10)

### Takeaway
On GH200, a GPU kernel streaming from Grace LPDDR5X gets about 350-420 GB/s, which is 78-93% of the 450 GB/s/direction link and roughly 10x below HBM (3.4-4 TB/s). Copy-engine H2D gets about 375 GB/s pinned. System-allocated (malloc) memory is copied through a slower host-based path. With 4 KiB pages, GPU reads of malloc memory collapse to about 219 GiB/s. The reverse direction, Grace reading HBM over C2C, is much worse: 130 GB/s read and 168 GB/s write, with about 800 ns latency.

### Cited Findings
- GH200 (Schieffer et al., KTH). Comm|Scope measured NVLink-C2C H2D at **375 GB/s** and D2H at **297 GB/s** (theoretical 450). STREAM measured CPU LPDDR5X at **486 GB/s** (peak 500) and GPU HBM3 at **3.4 TB/s** (peak 4). Access granularity is 64 B on the CPU and 128 B on the GPU. — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
- GH200, Quad GH200 on CSCS Alps (Fusco, ..., Schulthess, Hoefler). Hopper reading local Grace DDR reaches **~93% of 450 GB/s (≈418 GB/s, derived)**, and writing reaches ~84% (≈378 GB/s). Grace reading HBM reaches only ~53% (≈239 GB/s derived) and writing ~64%. Paths that cross both C2C and NVLink to a peer superchip never exceed **60%** of theoretical. — [arXiv 2408.11556](https://arxiv.org/html/2408.11556v2)
- In that paper, cudaMemcpy uses "the slower host-based implementation … for all system allocated memory regardless of physical placement". Use cudaMallocHost (pinned) memory to get the copy-engine path. The figures for each source/destination combination are given only in Fig. 5. — [arXiv 2408.11556](https://arxiv.org/html/2408.11556v2)
- DDR→DDR and HBM→HBM copies through the other chip cross the link twice, so their bound is about 250 GB/s. DDR→HBM bound is about 450 GB/s. The measured DDR→DDR copy is about half of DDR read bandwidth. — [arXiv 2408.11556](https://arxiv.org/html/2408.11556v2)
- GH200 nvbandwidth (HPI, Werner/Weisgut/Rabl, HCDS'25): GPU SM/CE access to CPU memory reached a maximum of **350 GB/s** (78% of 450). — [HPI PDF](https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf)
- GH200 (Verda). With a 500 MiB async memcpy, H2D was **135 GiB/s** (H200 PCIe: 57) and round-trip 65 GiB/s, so small or medium copies are far from the link limit. A kernel doing in-place read/write over 1 GiB measured:
  - malloc with 4 KiB pages, data left in CPU DDR: **218.8 GiB/s**
  - malloc with 64 KiB pages, migrated to HBM: 2112 GiB/s
  - cudaMallocManaged: 2192 GiB/s
  - cudaMalloc: **2390 GiB/s**
  — [Verda blog](https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip)
- Verda cache effects for GPU access to LPDDR5X-resident memory: cold **131 GiB/s**, warm **248 GiB/s**, and **0.22 GiB/s** after cache eviction (pathological). — [Verda blog](https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip)
- Already known from the brief (not re-derived): dnhkng nvbandwidth GH200 H2D ~377-380 GB/s; Sirius cites ~370 GB/s on GB300; peacockdb GB10 measured cudaMemcpy ~59 GB/s, kernel reading mapped host 205-219 GB/s vs 233-239 GB/s from device memory.
- Sirius/CIDR'26 table lists GH200 CPU↔GPU at "∼400 GB/s" against 3,000 GB/s HBM, and says the "Hopper GPU can access host memory at more than 400 GB/s, which is more than the maximum bandwidth between the CPU and main memory". The same paper prices GH200 at $1.5/h (Lambda; $3.2/h in the text) vs $7.344/h AWS. — [arXiv 2508.04701](https://arxiv.org/pdf/2508.04701)
- GB200 spec: 1 Grace + 2 Blackwell per superchip. Each Grace has up to 480 GB LPDDR5X at **up to 512 GB/s**. Each GPU has about 186 GB HBM3E at 8 TB/s (derived from 372 GB / 16 TB/s per superchip). The NVL72 has 36 Grace and 72 GPUs, 17 TB LPDDR5X and 14 TB/s aggregate. The page gives no C2C bandwidth figure. — [NVIDIA GB200 NVL72](https://www.nvidia.com/en-us/data-center/gb200-nvl72/)

### Inferences
- Under GB200, the two Blackwell GPUs share one Grace's ≤512 GB/s LPDDR5X. If both GPUs stream from host at once, each probably gets ≤~250 GB/s regardless of the C2C rate (estimate).
- The host-to-HBM ratio is about 10x on GH200 (≈400 vs 3,400-4,000 GB/s). On GB200 it is about 16-20x per GPU (≈400 vs 8,000 GB/s, estimate). Host-resident scans are therefore link-bound, but the link is still about 3x PCIe Gen5 x16.
- GB10's link (59 GB/s memcpy) is a different class: its kernel-reads-host rate is ~90% of device, because both live in the same LPDDR5X.

### Gaps
- I found no measured GB200/GB300 nvbandwidth or STREAM numbers from a primary source; the search budget was exhausted. The Sirius ~370 GB/s GB300 figure is the only data point.
- The exact pinned vs pageable cudaMemcpy GB/s on GH200 appears only in a figure (2408.11556, Fig. 5).

## 2. Coherent access vs copies vs migration; pitfalls

### Takeaway
On hardware-coherent Grace systems, GPU kernels can dereference malloc pointers through ATS. For streaming scans this costs about 10x vs HBM, plus page-table/TLB costs that depend on the page size. Access-counter migration (threshold 256 accesses, CUDA 12.4+) helps only data that is re-read many times. GPU-side first touch of system memory is expensive. Random GPU access over small pages is the worst case because GPU TLB misses are expensive.

### Cited Findings
- Access-counter migration applies only to hardware-coherent systems and works on system-allocated memory from CUDA 12.4. Enable it with `cudaMemAdviseSetAccessedBy`. `SetPreferredLocation=host` prevents migration. File-backed (mmap) memory never migrates by access. cudaMallocManaged defaults to fault-and-migrate. — [CUDA Programming Guide, Unified Memory](https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/unified-memory.html)
- The GPU uses the CPU's page table entries. Small pages over large areas cause many TLB misses, and GPU TLB misses cost more than CPU ones, so random GPU access over small pages is especially costly. Huge pages may not survive migration or GPU first touch. Hints have a host-side cost. — [CUDA Programming Guide](https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/unified-memory.html)
- Host-native atomics: NVLink-connected hardware-coherent devices report `cudaDevAttrHostNativeAtomicSupported=1`, and their atomics to CPU memory need no faults. — [CUDA Programming Guide](https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/unified-memory.html)
- Access-counter notification threshold defaults to **256** accesses. In SRAD with 64 KB pages, iterations 2-4 are a transition, and from iteration 5 the run beats managed memory as C2C reads drop to near zero. No GPU→CPU migration was observed. — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
- First touch: `cudaHostRegister` pre-population adds about **300 ms** in SRAD. GPU-side first touch of system memory is markedly slower than with managed memory, because the CPU must populate page-table entries. — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
- 64 KB vs 4 KB pages:
  - Allocation and free are **4.6-38x faster (avg 15.9x)** with 64 KB pages.
  - Compute is often **1.1-2.1x faster with 4 KB** pages (Rodinia).
  - Qiskit with system memory gains up to 4x from 64 KB pages.
  - Under oversubscription the effect varies: +58% migration speed in one case, nearly 3x slower compute in another.
  — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
- Under oversubscription, managed memory slows sharply. At ~130% it did not migrate at all and read over C2C at low bandwidth. System memory degrades more gracefully. — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
- System-allocated memory needs 64 KiB pages for the driver to migrate malloc data. Data left in 4 KiB-page DDR gets about 219 GiB/s. — [Verda blog](https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip)
- Choosing an allocation type (Alps study):
  - cudaMallocManaged is best for repeated GPU access but expensive for sparse or strided access.
  - ATS/system memory suits irregular, fine-grained access.
  - cudaMallocHost is needed for async copies.
  - The system page size is 64 KB, which makes strided managed access costly.
  — [arXiv 2408.11556](https://arxiv.org/html/2408.11556v2)
- Reverse direction, Grace CPU reading HBM over C2C (HPI):
  - Sequential read max **130 GB/s** and random read 128 GB/s (64 B accesses; 4 KiB random reads give only 80 GB/s).
  - Sequential write 163 GB/s and random write **168 GB/s**.
  - Grace reading its local LPDDR5X: 362 GB/s sequential read, 345 GB/s random, 370 GB/s write.
  - Idle latency: **807 ns HBM vs 220 ns local**.
  - Interleaving pages across CPU and GPU memory helps: 362-370 GB/s peak at 2:1-3:1 ratios, a 1.29-1.68x speedup.
  — [HPI PDF](https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf)
- Chips and Cheese also measured Grace→HBM latency of "nearly 800 ns" even with 2 MB pages, 592 ns above LPDDR5X (>200 ns). Single-core Grace DRAM bandwidth is 21 GB/s. — [Chips and Cheese](https://chipsandcheese.com/p/grace-hopper-nvidias-halfway-apu)
- NVIDIA says coherency removes the need for pinning (`cudaHostRegister`) "when huge pages are used". In SPECaccel 404.lbm, unified memory is 22% slower, mostly because the CPU reads a 5 GB result array that sits in GPU memory. — [NVIDIA blog](https://developer.nvidia.com/blog/simplifying-gpu-programming-for-hpc-with-the-nvidia-grace-hopper-superchip/)

### Inferences
- Hash-join build tables should stay in HBM and the probe side should stream. Random GPU probes into host memory pay both C2C latency (estimate ~800 ns+ round trip) and ATS/TLB misses. Expect far below the ~400 GB/s sequential rate (estimate; no direct DB measurement found).
- Avoid CPU-side reads of GPU-resident results: Grace→HBM gets only ~130 GB/s.

### Gaps
- I found no published GPU-random-read-of-host-memory bandwidth for GH200 (e.g., 8 B gathers).

## 3. Database / analytics papers

### Takeaway
I could verify only Sirius (CIDR'26, GH200) and the HPI index study (HCDS'25). The Lutz et al. NVLink 2.0 papers (Pump Up the Volume, SIGMOD'20; Triton Join, SIGMOD'22) returned 403 at ACM DL, so I could not extract their numbers. I found no reliable source for a dedicated "GH200 hash join" paper; the single search surfaced none.

### Cited Findings
- Sirius (Yogatama, …, Patterson, Kimball, McKinney, Xiangyao Yu; CIDR'26) is a drop-in GPU engine for DuckDB/Doris via Substrait on GH200. It reports **8.3x (TPC-H) and 7.4x (ClickBench) better cost efficiency** with DuckDB. Data is cached in device or pinned host memory. Spilling to pinned memory and disk was "planned" in the paper. — [arXiv 2508.04701](https://arxiv.org/pdf/2508.04701)
- The current Sirius repo has out-of-core tiered GPU/host/disk execution. `pin_table(tier='host')` is for tables larger than GPU memory. Defaults are 95% of GPU memory and **8 GiB pinned host per NUMA node**, and it uses O_DIRECT plus io_uring. — [Sirius GitHub](https://github.com/sirius-db/sirius)
- HPI HCDS'25 simulated a hybrid tree index (FPTree-like) with inner nodes in CPU DRAM and leaves in GPU HBM, accessed by the CPU. They conclude that C2C-attached memory is viable as secondary memory. — [HPI PDF](https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf)
- NVIDIA reports Spark RAPIDS 24.06 on 16 GH200 ran NDS 10 TB in 6 min vs 42 min on 16 x86 CPUs (Q67 36x). SF100 took 40 min, versus an equivalent 344-CPU cluster. HEAVY.AI on GH200 ($4.32/h) vs 8xA100 ($13.89/h) gave 5x speed and 16x cost at SF100, and 2x speed and 6x cost at SF200, where data was offloaded to Grace memory over C2C. All of this is vendor-reported. — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gh200-superchip-delivers-breakthrough-energy-efficiency-and-node-consolidation-for-apache-spark/)

### Gaps
- I did not retrieve the Lutz/Breß/Zeuch/Rabl/Markl NVLink 2.0 join numbers (ACM 403).
- I did not retrieve the cuDF/cudf.pandas/Polars GPU on GH200 unified-memory blogs (search exhausted).

## 4. NVIDIA guidance, NUMA and multi-superchip placement

### Takeaway
Treat each superchip as its own unit: two NUMA nodes per superchip (Grace DDR, GPU HBM), first-touch placement, and explicit numactl/libnuma binding. Cross-superchip traffic tops out at ≤60% of theoretical. On multi-GPU or multi-node systems, EGM exposes host memory to remote GPUs over NVLink.

### Cited Findings
- On Quad GH200 (Alps), Grace CPUs are NUMA nodes 0-3 and Hopper HBM is NUMA nodes 4, 12, 20 and 28. malloc/mmap are first-touch, so place memory with numactl/numa_alloc_onnode. Saturating C2C drops Grace→HBM writes to 17%. The 500 GB/s DDR is shared between CPU and GPU consumers. — [arXiv 2408.11556](https://arxiv.org/html/2408.11556v2)
- Grace tuning guide specs:
  - GH200: CPU memory up to 500 GB/s; GH200 NVL2 up to 1 TB/s.
  - Grace C1 with 480 GB: 384 GB/s.
  - Grace and GPU memory appear as two NUMA nodes joined by C2C.
  — [NVIDIA Grace tuning guide](https://docs.nvidia.com/grace-performance-tuning-guide.pdf)
- Grace memory specs conflict across sources:
  - 384 GB/s for 480 GB LPDDR5X-6400 — [Chips and Cheese](https://chipsandcheese.com/p/grace-hopper-nvidias-halfway-apu); [HPI](https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf)
  - 546 GB/s — [Verda](https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip)
  - 500 GB/s — [tuning guide](https://docs.nvidia.com/grace-performance-tuning-guide.pdf)
  - Measured STREAM is 486 GB/s on the KTH node — [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1)
  - HPI measured 362-370 GB/s — [HPI](https://hpi.de/oldsite/fileadmin/user_upload/fachgebiete/rabl/publications/2025/hcds25-Werner-GraceHopper.pdf)
  - The memory SKU differs between systems.
- EGM (Extended GPU Memory) lets any GPU reach CPU-attached memory over C2C or NVLink. Allocate with `cuMemCreate(CU_MEM_LOCATION_TYPE_HOST_NUMA)` or `cudaMemPoolCreate(cudaMemLocationTypeHostNuma)`. Find the closest host node with `CU_DEVICE_ATTRIBUTE_HOST_NUMA_ID`. Limit GPUs with CUDA_VISIBLE_DEVICES, not cgroups. EGM uses 2 MB pages. Remote-socket EGM can be faster because it routes over NVLink. — [CUDA Guide EGM](https://docs.nvidia.com/cuda/cuda-programming-guide/04-special-topics/extended-gpu-memory.html)

### Inferences (engine design)
- Hot columns and hash tables belong in HBM via cudaMalloc or a pool. Stream cold columns from pinned host memory or 64K-page malloc memory with large sequential reads, in copy chunks well above 500 MiB or as direct kernel reads, to approach 350-420 GB/s.
- Don't route cudaMemcpy through pageable or system-allocated buffers.
- Pre-fault host buffers on the CPU instead of GPU first touch.
- Pin worker threads and buffers to the GPU's local Grace NUMA node.
- Overlap transfers with compute, since the link is about 1/10 of HBM.
- Leave Grace DDR bandwidth headroom: CPU-side work competes with GPU reads for the same 500 GB/s.

### Gaps
- I found no measured NVL72 per-GPU host bandwidth when both GPUs on one GB200 share a Grace.
- GB300 C2C and LPDDR measurements are unverified beyond the Sirius ~370 GB/s cited in the brief.
