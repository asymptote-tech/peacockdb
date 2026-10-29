# trainer-side-sql-prep — SQL feature prep on the training GPU, handed to PyTorch/JAX as device tensors

## Verdict

- **What it is:** a Python library that runs SQL over Parquet (projection, filters, feature transforms, as-of joins, sampling) on the training GPU and yields CUDA tensors to PyTorch/JAX through DLPack, with no host round trip per batch.
- **Who needs it:** demand.md rates it **weak**: 5 posters with small convenience asks, and "nobody says SQL is what they lack" [D]. The real stall pain is at recsys and video shops (Meta, Uber, Pinterest), and they have built their own pipelines.
- **Budget:** fits. About 0.8× engine on peacockdb, 0.7× on Sirius (estimate). But NVIDIA's cudf-polars already ships an experimental version of exactly this handoff [CP], and the train-time cost is mostly decoding and type conversion, not SQL.

## Who needs it badly

- **Rating: weak, taken from demand.md `## Ratings`, row `trainer-side-sql-prep`** [D]. R/U/C 4/4/0 · 5 posters. "Small convenience asks. Nobody says SQL is what they lack."
- Threads already in demand.md [D]:
  - Embeddings stored in Parquet must pass through the CPU to reach cuML: "This breaks the GPU-native pipeline" ([cudf #19497](https://github.com/NVIDIA/cudf/issues/19497), 2025-07).
  - **kipukun** needs a columnar-file-to-GPU path for XGBoost inference ([HN 45988948](https://news.ycombinator.com/item?id=45988948), 2025-11).
  - CSV straight to GPU for PyTorch: "loading and copy is the most time-wasting operation" ([PyTorch forum](https://discuss.pytorch.org/t/cudf-for-python-3-11/189225), 2023-10).
  - A GPU dataframe type inside PyTorch ([forum](https://discuss.pytorch.org/t/dataframe-kindo-of-object-in-pytorch/202686), 2024-05).
  - "need something like SiriusDB to manipulate and prepare data in-place before the AI algorithms get to work" ([HN 46479821](https://news.ycombinator.com/item?id=46479821), 2026-01).
- Evidence added here (not a re-rating):
  - An unnamed poster has a 200M-row, 120 GB numeric Parquet set read through PyArrow. They get about 7% GPU utilization and a "severe CPU bottleneck", and prefetch exhausts 64 GB of RAM. The thread has no replies ([forum 223768](https://discuss.pytorch.org/t/parquet-data-loading-strategy/223768), 2025-10-31) [R1].
  - HF Datasets streaming 4,000 Parquet shards from GCS runs at about 10% GPU utilization ([forum 213157](https://discuss.pytorch.org/t/low-gpu-utilization-10-with-hf-datasets-streaming-on-vertex-with-pytorch-trainer/213157), 2024-11-19) [R1].
  - **Uber** (April 2026): Parquet + Petastorm training ran GPUs at 10–15%, with "PyArrow to NumPy" a named CPU cost. In-house fixes raised utilization above 60% and cut a job from 22 h to 3 h ([arXiv 2604.21275](https://arxiv.org/abs/2604.21275)) [R1].
  - **Pinterest** moved last-mile processing to Ray Data CPU nodes: 45% more throughput and above 90% GPU utilization. This comes from secondary talk summaries ([ZenML](https://www.zenml.io/mlops-database/pinterest-ml-platform-evolution-with-ray-talks-deep-dives-ray-based-distributed-data-loading-for-recommender-model-train)) [R1].
- **Pattern:** the posters with pain are tabular-DL users on Parquet who want "Parquet → tensors fast". The firms with big pain (Uber, Pinterest, Meta) fixed it with CPU fleets or in-house loaders. None of them asks for SQL.
- **Best first conversations:**
  - the Uber authors of arXiv 2604.21275, since Petastorm, their own tool, is in maintenance [R2];
  - the forum 223768 poster;
  - kipukun.

## How it would work

- **Front door:** `pip install`, then use it in place of a DataLoader:
  ```
  ds = peacock.TrainSet(sql="SELECT coalesce(age,0) AS age, bucketize(price,[10,100]) AS pb, label
                             FROM 'events/*.parquet' e ASOF JOIN 'users/*.parquet' u USING (uid, ts)
                             WHERE day >= '2026-09-01'",
                        batch_rows=65536, shuffle_seed=7, rank=r, world=w, vram_budget="4GB")
  for batch in ds:            # dict[str, torch.Tensor] on cuda:r via torch.from_dlpack
      loss = model(batch).backward()
  ```
- **In-process:** the library runs in the trainer's Python process (`num_workers=0`), so CUDA is never used inside forked DataLoader workers (estimate).
  - It shares the CUDA context, on its own low-priority stream.
  - Memory comes from a fixed RMM pool sized by `vram_budget`, or from PyTorch's caching allocator.
  - Before handing over a batch, it records an event that the training stream waits on.
- **Planner:** DataFusion parses SQL, peacockdb translates it to its node set, and the C++/cuDF executor runs it batch by batch [I].
  - Batch sizing already derives from a memory budget [I].
- **Sharding:** each rank reads disjoint row groups keyed by (rank, world, epoch seed). Row-group-to-lane mapping already exists at plan time [I].
- **Resume:** `state_dict()` is (epoch, seed, next row group, offset), compatible with torchdata's StatefulDataLoader [R3].
- **Output:** numeric, non-null, typed columns. Strings and categoricals are dictionary-encoded to int ids. List columns become offsets + values (jagged tensors, as TorchRec's KeyedJaggedTensor expects).
- **Data path, end to end:**
  - Parquet on NVMe or object store → pinned host buffer → H2D.
  - On the GPU: decode → filter/project/transform/join → DLPack → `torch.from_dlpack`, which shares memory with no copy [R3].
  - From epoch 2 on: a decoded, projected column cache in pinned host RAM, then H2D only (see `## Data path and cache`).

## The human's questions

**1. How big is the data-stall problem today, and for whom?**
- **Recsys: large and well measured, at hyperscalers.**
  - Meta (ISCA'22): storage plus online preprocessing "can already consume more power than the actual GPU trainers".
  - Each 8-GPU trainer needed 9–55 preprocessing servers.
  - With preprocessing on the trainer's own CPUs, 56% of GPU cycles stalled.
  - Ingest per 8-GPU node is 4.7–16.5 GB/s.
  - Source: [M].
  - Uber 10–15% → >60% utilization, Pinterest >90% after Ray; RecD dedup gives 2.48× training throughput [R1].
- **Fleet-wide:** in Google 2020 data, 30% of total compute went to input processing [T].
  - 20% of jobs spent more than a third of their compute there; the median job spent 10%.
  - About 75% of pipelines rerun every hour, yet only 19% use caching.
  - Heavy-tailed: a problem for some jobs, not universal.
- **Generic DNNs:** 3–24 CPU cores per GPU are needed, and up to 65% of epoch time goes to preprocessing, "primarily on redundant decoding" [MO].
  - MinatoLoader raised utilization from 46% to about 90% [MI].
- **Tabular:** no paper measures it. Forum posts report 7–10% utilization on Parquet + PyArrow [R1].
  - GBDT (XGBoost) loads once into a DMatrix, so it has no per-step stall; its cost is the one-off load (estimate).
- **Multimodal/video:** decode-bound. Preprocessing takes 2.2–6.5× the GPU training time [SA].
  - That is JPEG/video decode, which DALI and TorchCodec target, not SQL.
- **Text LLM pretraining:** rarely input-bound. Data is pre-tokenized and memory-mapped [NE].
  - No published run measures loader stall, so "negligible" is folklore.
- **Is it SQL-shaped?** Mostly not.
  - At Meta, joins, filtering and labeling run upstream in Spark. Online transforms are per-feature and per-mini-batch (Bucketize, SigridHash, NGram) [M].
  - The SQL-like wins are projection (jobs read 9–11% of features), partition filters and cross-job reuse [M][T].
  - The rest is decode and Arrow→tensor conversion [R1].

**2. What already exists, and where does each fall short?** Repo activity checked 2026-10-29 [R2].

| Tool | GPU transforms | SQL/joins | Handoff | Shortfall |
|---|---|---|---|---|
| NVIDIA DALI v2.4.0 (2026-10-26) | yes, media | no | DALI-owned tensors valid until the next call; `feed_ndarray` copies | image/video/audio only; no tabular, no joins |
| NVTabular / Merlin | yes (cuDF ETL, Categorify) | joins via JoinExternal, no SQL | DLPack | effectively frozen: last release v23.08.00, only CVE fixes since; Merlin dataloader last commit 2023-10; NGC container deprecated |
| cuDF + DLPack v26.10.02 | yes | pandas-like API, no SQL | `to_dlpack`: numeric only, one common dtype, column-major (`frame.py:806`) | no SQL; user writes the glue |
| cudf-polars `persisted_to_torch` | yes | Polars DSL (SQLContext unverified) | zero-copy views | experimental; every rank must run the same queries in the same order; numeric non-null only; silent CPU fallback [CP] |
| Ray Data 2.58–2.59 | `batch_format="cudf"`, experimental GPU shuffle | no GPU joins | `iter_torch_batches` | "data still moves between host and device across stages" [RY] |
| Daft v0.7.26 | GPU only schedules UDFs | SQL on CPU | CPU | no GPU relational execution |
| DataLoader + DuckDB | no | yes, CPU | Arrow/NumPy → `.cuda()` | always crosses the host; CPU-bound decode |
| torchdata 0.11 (`nodes`) | no | no | CPU | DataPipes removed in 0.10; only orchestration |
| Petastorm v0.13.1 | no | no | CPU NumPy | maintenance mode; Uber's own 10–15% utilization case |
| WebDataset | no | no | CPU | tar shards for media; last commit 2026-02, type annotations only |
| Meta DPP / TorchArrow / TorchRec | DPP on CPU fleet | internal | KJT | DPP not open source; TorchArrow archived 2024; TorchRec takes batches, it does no prep |
| Mosaic Streaming, HF datasets, Lance | no | Lance: DataFusion on CPU | CPU | offline materialization (Databricks: Spark → MDS → StreamingDataset) |
| Sirius | yes | yes, growing (GPU INNER ANY joins, 2026-10-29) | none documented | results go back to DuckDB on the host; silent CPU fallback; no releases |

- **The gap:** no maintained tool runs SQL with joins on the training GPU and hands over zero-copy tensors. The nearest is NVIDIA's own experimental cudf-polars path [CP].

**3. Do training GPUs have spare memory and time for SQL?**
- **Memory: usually no, for big models.**
  - Llama 3's parallelism is tuned so that parameters, optimizer state, gradients and activations "fit in its HBM" on 80 GB H100s [L3].
  - TorchRec's planner packs embedding tables into HBM (default 32 GB HBM per device in its topology) and spills to UVM [TR].
  - GPU-side prep is not small: DALI measured about 400 MB static plus 3.6–7.3 GB dynamic. That memory is taken from batch size [FF].
- **Time: some, but naive sharing hurts.**
  - Llama 3 MFU is 38–43%, but the gap is communication and memory stalls, not free SMs [L3].
  - FusionFlow: "spatial GPU sharing seems less frequently applicable" [FF].
  - MinatoLoader: "If possible, it is better to avoid data preprocessing on the GPU" [MI].
  - RAP (ASPLOS'24) is the positive case. Resource-aware DLRM preprocessing co-run on the training GPUs costs only 2–3% of training throughput. That is 1.99× faster than sequential GPU prep and 78.3× faster than CPU frameworks [RAP].
  - But a large unscheduled kernel next to MLP forward "can cause large increases in training latency" [RAP].
- **Who has headroom:** tabular deep learning and small models on 80–192 GB parts probably have tens of GB free (estimate; no measurement found). Those are exactly the weak-demand posters.
- **Implication:** the engine must run under a hard VRAM budget of about 1–8 GB with no dynamic growth (estimate). It must also shrink its kernels into the idle phases, RAP-style.
  - peacockdb has budgeted batch sizing but no RMM pool and no spill; `gpu_memory_limit` is ignored today (#148) [I].

**4. What is the minimal integration with a training loop?**
- An iterable that yields `dict[str, torch.Tensor]` on the rank's device, created with `torch.from_dlpack`, which shares memory [R3].
- It needs:
  - (a) rank/world sharding;
  - (b) a seeded shuffle at row-group granularity plus an in-buffer shuffle;
  - (c) `state_dict`/`load_state_dict` for resumption, as torchdata's StatefulDataLoader expects [R3];
  - (d) stream sync: an event recorded on the prep stream, waited on by the training stream (estimate);
  - (e) a fixed VRAM budget.
- JAX would take the same capsules via `jax.dlpack.from_dlpack` (estimate; not checked against the docs).
- Out of scope for the alpha: multi-host data loading coordination beyond static sharding, and JAX's global arrays.

## Data path and cache

| Stage | Rate | Source |
|---|---|---|
| Meta trainer ingest need | 4.7–16.5 GB/s per 8-GPU node, i.e. 0.6–2 GB/s per GPU | [M] |
| HBM read, H200 | 4,484 GB/s measured | [I] |
| PCIe H2D pinned / pageable, H200 | 47.3 / 10.6 GB/s | [I] |
| Parquet decode on GPU (engine today) | 6.8–8.7 GB/s per GPU; page decode + Snappy is 80% of kernel time on q11v | [I] |
| GB10 chunked reader vs `read_parquet` | 61 vs 340 ms for q6 | [I] |
| GB200 Grace LPDDR5X | up to 480 GB per superchip, 512 GB/s | [GB] |
| NVMe, DGX-class, 8 drives | ~50 GB/s total (estimate) | [LN] |

- **Bandwidth is not the limit; GPU time is.**
  - Per-GPU ingest needs (0.6–2 GB/s) fit easily inside PCIe (47 GB/s) and inside the GPU's decode rate.
  - But decoding 2 GB/s at 7 GB/s uses about 25–30% of the training GPU's time (estimate). That is the very stall the product claims to remove, moved onto the GPU.
  - Projection (9–11% of features read [M]) shrinks the decode to a few percent (estimate).
- **The thesis holds, in a training form.**
  - Without a cache, every epoch re-reads and re-decodes, and NIC or decode eats the gain. Google: 75% of pipelines rerun hourly, yet only 19% cache [T].
  - A VRAM cache does not fit, because training owns the HBM (question 3).
  - The cache to build is **decoded, projected columns in pinned host RAM** (or Grace LPDDR5X on GB200). Epochs 2+ then cost only H2D at 47 GB/s plus transforms. Uber's own fix was a local-disk cache [R1].
- **Engine today has none of this:** no cache, pageable host buffers, slow `read_parquet` (#149, #232) [I].

## Build cost to alpha

Sizes are in yardstick lines; the unit is 22,830 [I]. Every figure is an estimate.

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend, translator, node set, cuDF operators | reuse [I] | reuse (DuckDB planner) [S] |
| Python package and in-process entry point (PyO3, iterator) | new: 1.5–2.5k; the GPU has no entry point outside tests today [I] | adapt the DuckDB Python extension: 0.5–1k |
| GPU-resident result → DLPack (typed columns, dictionary ids, jagged lists) | new: 1–2k | new: results return to DuckDB on the host today; keep them on the GPU: 2–4k |
| Co-existence with PyTorch: fixed RMM budget, shared context, low-priority stream, events | adapt: RMM pool missing (#148): 1–2k | adapt: Sirius memory tiers assume they own the GPU: 1–2k |
| Feature functions on GPU (hash, bucketize, clamp, list FirstX/NGram, casts) | new: 2–4k | new: DuckDB has them on the CPU, and Sirius falls back silently: 2–4k |
| List-typed columns through plan and wire | adapt: 1–2k | adapt: 0.5–1k |
| As-of (point-in-time) join | new: 1.5–3k; no window node today (#143) | new on GPU: 1.5–3k |
| Sharding, seeded shuffle, sampling, `state_dict` resume | new: 1.5–2.5k | new: 1.5–2.5k |
| Fill path: chunked reader, pinned buffers, multi-file, object store | adapt: 1.5–3k (#149, #232) | reuse (kvikio, io_uring, S3) [S] |
| Decoded-column cache in pinned host RAM | new: 2–3k | adapt the pin cache and memory tiers: 0.5–1k |
| Tests (DLPack round trip, resume, determinism) | adapt the corpus harness: ~1k | adapt: ~1k |
| **Total** | **~14–25k** | **~11–20k** |

Total new work: 0.8× engine on peacockdb, 0.7× on Sirius (estimate)

- **The hard part:** running inside someone else's GPU process without hurting the trainer.
  - That means a hard VRAM cap with no growth, kernels small enough to overlap with training (RAP needed a scheduler to keep the cost at 2–3% [RAP]), and deterministic resume.
  - The engine today fails the query when over budget and has no retry (#142) [I].
- **On Sirius:** the cost is close to peacockdb's, because GPU result export and in-process co-existence are new there too. Sirius churns by ≈50–60k lines a week, so a fork is out [S].

## Adoption friction

- **Low in form:** a `pip` library that replaces a DataLoader, with no new system to run. This is the best-shaped plugin of any option.
- **High in practice:**
  - Users write Python transforms (pandas, Polars, torch), not SQL. demand.md: nobody says SQL is what they lack [D].
  - The firms with real stalls own their pipelines (Meta DPP, Uber, Pinterest on Ray) and solve them with CPU fleets that do not touch the trainer [R1][R2].
  - ML engineers will not risk an OOM in a multi-day run to save loader time. MinatoLoader's advice is to keep prep off the GPU [MI].
  - Silent CPU fallback (Sirius, cudf-polars) makes step time unpredictable; peacockdb refuses unknown plans, which is better here [I][R2].
- cuDF's Python-version lag also blocked posters [D]. A Rust/C++ wheel would need to bundle libcudf (estimate).

## Platform risk

- **High.**
- **NVIDIA** ships cudf-polars `persisted_to_torch`: zero-copy GPU tensors inside a torchrun job, marked experimental [CP].
  - cudf-polars is in the cuDF monorepo, open beta, Apache-2.0 [N].
  - One promotion from experimental removes this option's main gap, for free.
  - NVIDIA also froze Merlin/NVTabular [R2], so it may not prioritise tabular training. That cuts both ways.
- **Ray** is adding GPU-native operators and plans to fuse cuDF stages on the GPU [RY].
- **PyTorch/Meta** own the loader API (torchdata `nodes`) and the recsys stack (TorchRec) [R2].
- **DALI** is active and owns media decode [R2].

## Funding story

- **What a design partner would say** (estimate): "Our GPUs sit at 10–15% on Parquet. But we fixed it with a local cache and transform pushdown on CPU (Uber), or with Ray CPU nodes (Pinterest). Putting prep on our training GPUs would cost us batch size."
- A tabular-DL user would say: "I just want Parquet to tensors fast. I don't write SQL."
- **What a round would be raised on:** "the data layer for GPU training", priced against Meta's finding that data ingestion costs more power than the trainers [M].
  - That is a big story, but the alpha's evidence would be a benchmark, not a user. The investor's first question is why NVIDIA's free cudf-polars path does not already do it.

## Evidence against

- **The pain is not SQL-shaped.** Train-time cost is decode, Arrow→tensor conversion and per-feature transforms. Joins live upstream in Spark [M][R1]. A SQL engine adds little there that a fast columnar loader does not.
- **The training GPU is the wrong place.** HBM is full in LLM and DLRM jobs [L3][TR]. Co-located prep slows compute-heavy training unless it is carefully scheduled [MI][FF]. Industry chose disaggregation (DPP, tf.data service) [M][T].
- **Decoding on the GPU moves the stall rather than removing it:** about 25–30% of GPU time at Meta's ingest rates before projection (estimate, from [I][M]).
- **Weak demand:** 5 posters, all convenience asks [D]. The firms in pain have fixed it in-house.
- **NVIDIA ships the handoff** (cudf-polars → torch) [CP]. It had the purpose-built tool (NVTabular) and let it lapse, a hint that the market is small [R2].
- **The headroom is with small models,** whose owners have the mildest pain.

## Ratings

- **Who needs it badly: weak.** demand.md rates it weak: 5 posters, no one asks for SQL [D].
- **Build cost: strong.** About 0.8× on peacockdb, which fits the budget. The parts are mostly additive to the existing engine.
- **Data path: medium.** Bandwidth is ample, but GPU decode steals trainer time. It needs a pinned-host-RAM decoded-column cache, since there is no room for one in VRAM.
- **Adoption friction: medium.** It is a drop-in Python iterator, but users want Python transforms, not SQL, and fear OOMs in long runs.
- **Platform risk: weak.** NVIDIA's cudf-polars already has experimental zero-copy torch export; Ray is moving to GPU-native stages.
- **Funding story: weak.** No named design partner with this pain unsolved; the story is a benchmark against a free NVIDIA path.

## Follow-up experiments

1. **Interference:** run a DLRM (TorchRec) step and a tabular MLP step on an H100. Co-run peacockdb prep under a 4 GB cap on a low-priority stream, and measure the drop in training throughput. Target: under 5%, against RAP's 2–3%.
2. **Decode cost per GPU:** decode a Criteo-like 1k-column Parquet set at 10% projection with the chunked reader. Measure GPU-ms per GB and compare with CPU PyArrow → NumPy at 32 cores.
3. **Cache payoff:** epoch-2 throughput from a pinned-host-RAM decoded cache vs re-decoding, on H100 and GH200 (C2C).
4. **The NVIDIA baseline:** cudf-polars `persisted_to_torch` on the same pipeline. If it is within 1.5×, this option has no product.
5. **Ask the forum 223768 poster and the Uber authors** whether SQL would replace their transforms. That is a question, not a measurement, but nothing else settles demand.

## Sources

- [D] demand.md `## Ratings`, row trainer-side-sql-prep (local, master f36a68ec; accessed 2026-10-29)
- [I] inventory.md at master f25c250d, incl. its benchmark evidence (local; accessed 2026-10-29); [S] sirius.md, Sirius @ b9e05ffc (local; accessed 2026-10-29); [N] nvidia.md rows cudf-polars [25][77] (local; accessed 2026-10-29); [LN] options/lean.md data-path table (local; accessed 2026-10-29)
- [M] Zhao et al., Meta DSI, ISCA 2022: https://arxiv.org/html/2108.09373v4 (accessed 2026-10-29)
- [T] Murray et al., tf.data, VLDB 2021: https://arxiv.org/abs/2101.12127 (accessed 2026-10-29); tf.data service: https://arxiv.org/html/2210.14826v3 (accessed 2026-10-29)
- [MO] Mohan et al., data stalls, VLDB 2021: https://arxiv.org/pdf/2007.06775 (accessed 2026-10-29)
- [MI] MinatoLoader: https://arxiv.org/pdf/2509.10712 (accessed 2026-10-29)
- [SA] SAND (KAIST): https://ina.kaist.ac.kr/assets/bibliography/SAND.pdf (accessed 2026-10-29)
- [NE] NeMo data docs: https://docs.nvidia.com/nemo-framework/user-guide/25.09/data/index.html (accessed 2026-10-29)
- [RAP] RAP, ASPLOS'24: https://par.nsf.gov/servlets/purl/10538949 and https://www.pnnl.gov/publications/rap-resource-aware-automated-gpu-sharing-multi-gpu-recommendation-model-training-and (accessed 2026-10-29)
- [FF] FusionFlow, PVLDB 17(4): https://www.vldb.org/pvldb/vol17/p863-kim.pdf (accessed 2026-10-29)
- [L3] Llama 3 Herd of Models: https://arxiv.org/html/2407.21783 (accessed 2026-10-29)
- [TR] TorchRec planner constants: https://raw.githubusercontent.com/meta-pytorch/torchrec/main/torchrec/distributed/planner/constants.py (accessed 2026-10-29)
- [GB] NVIDIA GB200 NVL72: https://www.nvidia.com/en-us/data-center/gb200-nvl72/ (accessed 2026-10-29)
- [CP] cudf-polars PyTorch interop: https://docs.nvidia.com/cudf/latest/cudf_polars/pytorch/ (accessed 2026-10-29)
- [RY] Anyscale, GPU-native operators in Ray Data: https://anyscale.com/blog/gpu-native-operators-in-ray-data (accessed 2026-10-29)
- [R1] community and industry stall reports: https://discuss.pytorch.org/t/parquet-data-loading-strategy/223768 ; https://discuss.pytorch.org/t/low-gpu-utilization-10-with-hf-datasets-streaming-on-vertex-with-pytorch-trainer/213157 ; Uber https://arxiv.org/abs/2604.21275 and https://www.uber.com/in/hi/blog/accelerating-deep-learning/ ; Pinterest (secondary) https://www.zenml.io/mlops-database/pinterest-ml-platform-evolution-with-ray-talks-deep-dives-ray-based-distributed-data-loading-for-recommender-model-train ; RecD https://arxiv.org/pdf/2211.05239 (all accessed 2026-10-29)
- [R2] tool status (checked with `gh api`, 2026-10-29):
  - DALI and Merlin: https://github.com/NVIDIA/DALI/releases/tag/v2.4.0 ; https://docs.nvidia.com/deeplearning/dali/archives/dali_1_24_0/user-guide/docs/plugins/pytorch_plugin_api.html ; https://github.com/NVIDIA-Merlin/NVTabular/commits (5dd11f4a33, 1d1bbca785) ; https://github.com/NVIDIA-Merlin/dataloader (1441a12e5c) ; https://catalog.ngc.nvidia.com/orgs/nvidia/containers/nvtabular
  - cuDF: https://github.com/NVIDIA/cudf/blob/v26.10.02/python/cudf/cudf/core/frame.py#L806 (release v26.10.02, 2cd6c6c087)
  - Ray and Daft: https://github.com/ray-project/ray/releases ; https://docs.daft.ai/en/stable/custom-code/udfs/
  - PyTorch ecosystem: https://pytorch.org/data/beta/what_is_torchdata_nodes.html ; https://github.com/uber/petastorm/commits (f47d304130) ; https://github.com/webdataset/webdataset (e0953f9bba) ; https://github.com/pytorch/torcharrow ; https://github.com/meta-pytorch/torchrec
  - Streaming, Lance and Sirius: https://docs.databricks.com/aws/en/machine-learning/load-data/streaming ; https://lancedb.github.io/lance/integrations/pytorch.html ; https://github.com/sirius-db/sirius/commits (73ed20b1c8)
  - All accessed 2026-10-29.
- [R3] PyTorch DLPack: https://docs.pytorch.org/docs/2.15/dlpack.html ; StatefulDataLoader: https://meta-pytorch.org/data/main/stateful_dataloader_tutorial.html (accessed 2026-10-29)
