# Trainer-side SQL prep: GPU headroom and integration

All sources accessed 2026-10-29.

## Memory: is there spare HBM on training GPUs?

### Takeaway
LLM training is set up to fill HBM: parallelism and checkpointing are tuned until the model state fits. DLRM sharding planners pack embedding tables into HBM. Spare HBM is realistic only for small or lightweight models such as ResNet-18 or tabular nets. Even there, FusionFlow measured GPU prep (DALI) taking several GB of dynamic memory.

### Cited Findings
- Llama 3 405B: each H100 has 80GB HBM3. Its 4D parallelism "ensures each GPU's model parameters, optimizer states, gradients, and activations fit in its HBM", and 8K-token pretraining ran without activation checkpointing after memory optimizations — [Llama 3 Herd of Models](https://arxiv.org/html/2407.21783) (accessed 2026-10-29)
- NVIDIA's Llama 3.1 405B recipe needs at least 576 GPUs with at least 80GB each, because the model "will not fit" on less — [NGC DGXC benchmarking](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/dgxc-benchmarking/resources/llama31-405b-dgxc-benchmarking-a) (accessed 2026-10-29)
- FusionFlow measured DALI's memory use. A static workspace of about 400MB is 0.8–1.6% of a 24–48GB GPU and "comfortably fits within the memory space typically left available during DL training". Dynamic memory is larger: over 3.6GB for DefAugment and 7.3GB for RandAugment at batch 240. Prior work also reports DALI using "up to several GB". The paper says lost memory forces gradient accumulation or smaller batches — [FusionFlow, PVLDB 17(4)](https://www.vldb.org/pvldb/vol17/p863-kim.pdf) (accessed 2026-10-29)
- FusionFlow also says "as most DL training is notably memory and/or compute-intensive, spatial GPU sharing seems less frequently applicable". It falls back to temporal sharing, i.e. filling idle gaps between iterations, and reclaims free memory across the prep and training memory pools — [FusionFlow](https://www.vldb.org/pvldb/vol17/p863-kim.pdf) (accessed 2026-10-29)
- The TorchRec planner's default per-device topology is HBM_CAP = 32GB and DDR_CAP = 128GB. UVM/KV caching ratio defaults are 0.2, so tables that don't fit in HBM spill to host memory through UVM caching — [torchrec planner constants.py](https://raw.githubusercontent.com/meta-pytorch/torchrec/main/torchrec/distributed/planner/constants.py) (accessed 2026-10-29)
- RAP's profiling of DLRM training shows "a large amount of GPU resources are underutilized" because DLRM mixes compute-bound MLPs with memory-bound embedding lookups. The headroom is mostly SM and DRAM bandwidth over time, not free capacity — [RAP, ASPLOS'24](https://par.nsf.gov/servlets/purl/10538949) (accessed 2026-10-29)
- HBM sizes:
  - H100: 80GB HBM3 — [Llama 3 paper](https://arxiv.org/html/2407.21783) (accessed 2026-10-29)
  - H200: 141GB HBM3e at 4.8TB/s; MIG up to 7 instances of 18GB (SXM) or 16.5GB (NVL) — [NVIDIA H200](https://www.nvidia.com/en-us/data-center/h200/) (accessed 2026-10-29)
  - GB200 superchip (2 Blackwell GPUs + 1 Grace): 372GB HBM3E, so about 186GB per GPU, plus up to 480GB LPDDR5X at 512GB/s on the Grace CPU. An NVL72 rack has 13.4TB HBM and 17TB LPDDR5X — [NVIDIA GB200 NVL72](https://www.nvidia.com/en-us/data-center/gb200-nvl72/) (accessed 2026-10-29)
  - B200: 192GB per [gpuperhour](https://gpuperhour.com/compare/b200-vs-mi355x) vs 180GB per [Bacloud](https://www.bacloud.com/en/blog/203/nvidia-b200-vs-amd-instinct-mi355x-next-gen-ai-data-center-gpu-showdown.html). These sources conflict and both are aggregators (accessed 2026-10-29)
  - MI300X: 192GB; MI325X: 256GB; MI355X: 288GB at about 8TB/s — [computingpower-org (aggregator)](https://computingpower-org.pages.dev/hardware); [TechRadar MI355X](https://www.techradar.com/pro/amd-releases-details-of-288gb-mi355x-accelerator-80-percent-faster-than-mi325x-8tb-s-memory-bandwidth) (accessed 2026-10-29)

### Inferences
- In LLM and DLRM jobs the prep engine should plan on a fixed budget of about 1–8GB per GPU (estimate) and must not allocate dynamically. In LLM jobs that memory is taken from batch size. FusionFlow's measurements suggest DALI-style prep needs several GB.
- Tabular deep learning and small models probably have tens of GB free on 80–192GB parts (estimate; I found no measurement).
- On GB200/GH200, the Grace LPDDR5X (480GB per superchip) is a natural staging area for prepared batches. The GPU would read it over NVLink-C2C instead of PCIe (estimate; the page gives no C2C bandwidth figure).

### Gaps
- I found no published HBM-occupancy survey for tabular deep learning training.
- I did not fetch the TorchRec planner reservation default (HeuristicalStorageReservation %) because the docs page returned 404.
- I found no vendor datasheet for B200 or MI300X capacity, only aggregator sites.

## Time: MFU, idle SM time, and interference from co-located prep

### Takeaway
Even well-tuned LLM training reaches only about 38–43% MFU. That does not mean SMs are free, since the gap is mostly communication and memory stalls. Every co-location paper finds naive GPU prep slows compute-heavy training. Only prep that is resource-aware (scheduled into idle phases, kernels sized to the available resources) keeps the training slowdown to a few percent.

### Cited Findings
- Llama 3 405B BF16 MFU:
  - 43% on 8,192 H100s (TP8/PP16/DP64)
  - 41% on 16,384 H100s
  - 38% on 16,384 H100s with CP16 at 131K sequence length
  - Effective training time was above 90%, with 466 interruptions in 54 days
  - [Llama 3](https://arxiv.org/html/2407.21783) (accessed 2026-10-29)
- NVIDIA's FP8 Llama 3.1 405B recipe: 42.71% MFU on 576 H100s. The denominator is 1979 TFLOPS FP8, against 989 TFLOPS for BF16 — [NGC](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/dgxc-benchmarking/resources/llama31-405b-dgxc-benchmarking-a) (accessed 2026-10-29)
- RAP (ASPLOS'24) co-runs DLRM preprocessing with training on the same GPUs:
  - The preprocessing ops are FillNull, SigridHash, FirstX, Clamp, Bucketize, Ngram, MapId, Logit, Onehot.
  - Overlapping a large Ngram kernel (128 features) with MLP forward "can cause large increases in training latency".
  - RAP is 1.99× faster than sequential GPU preprocessing and 78.3× faster than CPU preprocessing frameworks. Its end-to-end throughput is only 3.24% below an ideal run with no preprocessing; the PNNL abstract says 2.04%.
  - At the latency turning point, average GPU/SM utilization was 77.6%/59.0% for the baseline vs 92.8%/80.3% for RAP.
  - [RAP PDF](https://par.nsf.gov/servlets/purl/10538949); [PNNL record](https://www.pnnl.gov/publications/rap-resource-aware-automated-gpu-sharing-multi-gpu-recommendation-model-training-and) (accessed 2026-10-29)
- RAP cites Meta's DSI study: input processing for DLRMs can use "significant power and compute ... even surpassing the power consumption of GPU trainers" and needs "a substantial number of dedicated CPU computing nodes" — [RAP](https://par.nsf.gov/servlets/purl/10538949), citing [Zhao et al. arXiv 2108.09373](https://arxiv.org/abs/2108.09373v3) (accessed 2026-10-29)
- MinatoLoader on DALI: "sharing the GPU between preprocessing and training leads to resource contention, which can interfere with the overall training performance, especially in compute-intensive workloads". A deeper DALI prefetch queue "increases GPU memory usage and preprocessing load, which can interfere with training". Its takeaway: "If possible, it is better to avoid data preprocessing on the GPU". With the default PyTorch loader, GPUs averaged 46% utilization, and the default loader can cause up to 76% GPU idleness — [MinatoLoader arXiv 2509.10712](https://arxiv.org/pdf/2509.10712) (accessed 2026-10-29)
- FusionFlow's gains depend on how heavy the model is. ResNet-18, which "leaves ample idle GPU cycles", gets 78.5–91.7% higher throughput than DALI-AoT. On ViT-Base the gain shrinks, and GPU-only DALI variants show "adverse effects on performance when the GPU is heavily utilized" — [FusionFlow](https://www.vldb.org/pvldb/vol17/p863-kim.pdf) (accessed 2026-10-29)
- Data stalls are large on the CPU side. DNNs spend up to 65% of epoch time in preprocessing and need 3–24 CPU cores per GPU. CoorDL cut training time by up to 5× vs DALI — [Mohan et al., VLDB'21, arXiv 2007.06775](https://arxiv.org/abs/2007.06775) (accessed 2026-10-29)
- In Google's fleet, 20% of tf.data jobs spend more than a third of their compute in the input pipeline. The tf.data paper argues for "disaggregating input data processing from model training" — [tf.data, arXiv 2101.12127](https://arxiv.org/abs/2101.12127) (accessed 2026-10-29)
- On co-locating whole training jobs: MPS beat naive collocation by up to 80% and MIG by up to 40% in throughput. MIG gives strict isolation but rigid partitions. Large models see little gain or crash from OOM — [Robroek et al., arXiv 2209.06018](https://arxiv.org/abs/2209.06018v3) (accessed 2026-10-29)

### Inferences
- The realistic slot for SQL prep is RAP-style. Small, fused kernels run on a low-priority CUDA stream in the same process, timed into the embedding/communication phases. MPS or MIG partitions would statically take SMs away from training (estimate).
- Prep cost should be measured as the drop in training throughput, as RAP does (2–3% target), not as prep latency.

### Gaps
- I did not find a paper quantifying SM-idle time inside LLM steps from a prep-co-location angle.
- I did not verify CUDA stream-priority semantics against NVIDIA docs.

## Alternatives: disaggregated prep vs co-location

### Takeaway
Industry practice at scale is disaggregation: Meta DPP and tf.data service. Co-location on trainer GPUs remains research territory (RAP, FusionFlow), alongside in-storage approaches (PreSto).

### Cited Findings
- Meta DPP is a separate preprocessing service that "scales to eliminate data stalls". The DSI fleet stores exabytes and serves tens of TB/s — [arXiv 2108.09373](https://arxiv.org/abs/2108.09373v3) (accessed 2026-10-29)
- PreSto (ISCA'24) moves recsys feature generation and normalization into in-storage processing. It reports 9.6× speedup, 4.3× better cost-efficiency and 11.3× better energy-efficiency than CPU preprocessing — [arXiv 2406.14571](https://arxiv.org/abs/2406.14571) (accessed 2026-10-29)
- FusionFlow needs 50–60% fewer CPUs than using remote scalable compute for similar speed — [FusionFlow](https://www.vldb.org/pvldb/vol17/p863-kim.pdf) (accessed 2026-10-29)

### Inferences
- The case for co-location rests on removing the CPU fleet that Meta and RAP describe as costly in power and compute. The case for disaggregation is zero interference and independent scaling.

### Gaps
- I did not fetch the tf.data service design doc or a GPU-side Meta DPP paper.

## Minimal integration with PyTorch/JAX

### Takeaway
The expected surface is a `torch.utils.data.IterableDataset`, or a plain iterator, that yields CUDA tensors through DLPack. It needs rank/worker sharding, `state_dict`/`load_state_dict` for resumption, and seeded shuffling.

### Cited Findings
- `torch.from_dlpack` returns a tensor that "will share the memory with the input tensor". In-place ops affect the source, and a capsule may be consumed only once. A `copy=None` default copies "only if necessary" — [PyTorch DLPack docs](https://docs.pytorch.org/docs/2.15/dlpack.html) (accessed 2026-10-29)
- StatefulDataLoader is a drop-in DataLoader with `state_dict()`/`load_state_dict()`:
  - Map-style datasets resume through sampler state.
  - IterableDatasets define per-worker `state_dict`/`load_state_dict`, which the loader aggregates.
  - Restore requires the same `num_workers`.
  - The example shards per worker with `get_worker_info()` and a seeded permutation.
  - [torchdata StatefulDataLoader tutorial](https://meta-pytorch.org/data/main/stateful_dataloader_tutorial.html) (accessed 2026-10-29)

### Inferences (not source-verified)
- GPU-resident output means `num_workers=0`, i.e. in-process. Using CUDA inside forked DataLoader workers is a known hazard (estimate).
- Stream sync can work two ways (estimate; check the DLPack and `__cuda_array_interface__` specs):
  - Producer side: run prep on a side stream, then call `torch.cuda.current_stream().wait_event(ev)` before handing the batch over.
  - Consumer side: let the `__dlpack__(stream=...)` protocol do the sync.
- Rank sharding means each DDP/FSDP rank reads disjoint shards keyed by (rank, world_size, epoch seed). JAX would use `jax.dlpack.from_dlpack`, or `jax.make_array_from_process_local_data` for multi-host (estimate; not fetched).

### Gaps
- I did not fetch the JAX dlpack or Grain docs, or the DLPack stream-semantics spec.

## Recsys feature/embedding prep: does it fit SQL?

### Takeaway
Mostly yes. Recsys per-feature ops are scalar or list UDFs, and offline training-set assembly is a point-in-time (as-of) join. Embedding lookup itself stays in the model (TorchRec), not in SQL.

### Cited Findings
- DLRM preprocessing ops are FillNull, SigridHash, FirstX (list truncation), Clamp, Bucketize, Ngram, MapId, Logit and Onehot — [RAP](https://par.nsf.gov/servlets/purl/10538949) (accessed 2026-10-29)
- Feast `get_historical_features` joins feature views onto an entity dataframe "in a point-in-time correct way". It looks back from each row's timestamp, bounded by the feature view's TTL — [Feast docs](https://docs.feast.dev/getting-started/concepts/point-in-time-joins) (accessed 2026-10-29)

### Inferences
- Mapping to SQL:
  - FillNull is COALESCE.
  - Clamp is LEAST/GREATEST.
  - Bucketize is a CASE or binary search.
  - Logit is a scalar expression.
  - SigridHash and MapId are hash UDFs.
  - FirstX and Ngram are list/array functions.
  - Point-in-time joins are ASOF JOIN with a TTL predicate.
- A SQL engine needs list-typed columns and an ASOF join to cover recsys prep (estimate).

### Gaps
- I did not find a source that quantifies the share of recsys prep time spent in joins vs per-feature transforms.
