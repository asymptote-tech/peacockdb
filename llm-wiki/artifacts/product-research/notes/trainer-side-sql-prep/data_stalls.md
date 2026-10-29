# Training data stalls / input-pipeline bottleneck (2026): how big, for whom

All sources accessed 2026-10-29.

## Q1: Recommender systems (DLRM-style): Meta DSI/DPP and others

### Takeaway
RecSys is where the problem is most severe and best documented. At Meta, storage plus online preprocessing used more power than the GPU trainers, and each 8-GPU trainer needed 9–55 CPU preprocessing servers. The work is per-feature transform and decode work on a mini-batch, not SQL. Joins and filters run upstream in Spark ETL.

### Cited Findings
- Meta (Zhao et al., ISCA 2022): "storage and online preprocessing can already consume more power than the actual GPU trainers themselves in Meta's datacenters". The exact per-model split is only in Fig. 1, which the text does not give numerically. — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- Same paper: the DSI pipeline "is becoming the dominating factor that constrains the overall training performance and capacity". It stores exabytes and serves tens of TB/s. — [arXiv abs](https://arxiv.org/abs/2108.09373) (accessed 2026-10-29)
- DPP workers needed per 8-GPU trainer node (Table 9): RM1 24.16, RM2 9.44, RM3 55.22, i.e. "between 9 and 55 servers per trainer node". — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- With preprocessing on the trainer's own CPUs (RM1), "56% of GPU cycles were spent stalled waiting for training data", with trainer CPU at 92% and memory bandwidth at 54%. Loading alone, without transforms, takes up to 40% of trainer CPU cycles and 55% of memory bandwidth. — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- Ingestion bandwidth per 8-GPU node: RM1 16.5 GB/s, RM2 4.69 GB/s, RM3 12.0 GB/s. They projected the online-preprocessing requirement to grow 3.5× within two years. Co-designed optimizations cut DSI power by 2.59×. — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- Shape of the work:
  - Jobs read only 9–11% of the stored features and 21–37% of the stored bytes in the partitions they use.
  - Row filtering is by date partition.
  - Online transforms "are localized to each mini-batch, not across many rows". Examples: Bucketize, SigridHash, NGram, BoxCox, FirstX, IdListTransform.
  - Transform cycles split roughly 75% feature generation, 20% sparse normalization, 5% dense normalization.
  - Joins, filtering and labeling run in offline Spark/streaming ETL.
  — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- RecD (Meta/Stanford, MLSys 2023): sessions average about 16.5 samples, and feature values are duplicated across 81.6–89.4% of them. Deduplicated IKJT batches gave up to about 2.48× training throughput, 1.79× preprocessing throughput and 3.71× storage efficiency. — [arXiv 2211.05239](https://arxiv.org/pdf/2211.05239); [Stanford MAST](https://mast.stanford.edu/pubs/recd_deduplication_for_end_to_end_deep_learning_recommendation_model_training_infrastructure) (accessed 2026-10-29)
- InTune (RecSys 2023): this is Netflix-cluster DLRM pipelines, not Meta DPP. An RL agent splits trainer CPUs across pipeline stages and gets up to 2.29× ingestion throughput vs. tf.data AUTOTUNE. Existing tools gave "sub-optimal performance, frequent crashes" or needed cluster re-organization. — [arXiv 2308.08500](https://arxiv.org/abs/2308.08500) (accessed 2026-10-29)
- Pinterest, from ZenML summaries of Ray Summit talks (secondary source):
  - Preprocessing inside PyTorch jobs ("last mile data processing") gave poor GPU utilization.
  - Ray Data with heterogeneous CPU nodes gave 45% higher training throughput vs. native PyTorch dataloaders, 25% cost savings and >90% GPU utilization.
  - The home-feed ranker reached 880k examples/s while the model could do about 5M/s when compute-bound.
  - Sparse CSR gave up to 2×; a compact batch format cut unpickling 5×.
  — [ZenML 1](https://www.zenml.io/mlops-database/pinterest-ml-platform-evolution-with-ray-talks-deep-dives-ray-based-last-mile-ml-data-processing-to-accelerate-dataset-i); [ZenML 2](https://www.zenml.io/mlops-database/pinterest-ml-platform-evolution-with-ray-talks-deep-dives-ray-based-distributed-data-loading-for-recommender-model-train) (accessed 2026-10-29)
- Uber (arXiv, Apr 2026): Petastorm + Parquet distributed training ran at 10–15% GPU utilization. The causes were network I/O and CPU-bound transforms "e.g., PyArrow to NumPy". Worker-level transform pushdown and local-disk caching raised utilization above 60% and cut training from 22 h to 3 h. The Uber blog version adds about 80% compute-cost reduction. — [arXiv 2604.21275](https://arxiv.org/abs/2604.21275); [Uber blog](https://www.uber.com/in/hi/blog/accelerating-deep-learning/) (accessed 2026-10-29)

### Inferences
- The RecSys bottleneck is Parquet/columnar decode, row↔tensor conversion (Arrow→NumPy/tensors, jagged sparse features) and per-feature hashing/bucketizing on wide rows (1–1.5k features). Projection matters (about 10% of columns used). Relational joins at train time are rare. They are done upstream (estimate, from Meta's description).
- Projection, predicate pushdown, dedup and vectorized columnar transforms address this. Join engines do not.

### Gaps
- Meta's exact power split per model (Fig. 1) isn't in the extracted text. No public 2024–26 Meta update to the DSI numbers was found.

## Q2: General / fleet-wide: Google tf.data, Mohan, Cachew, Pecan, newer loaders

### Takeaway
Across Google's fleet, 30% of total compute time goes to input processing, and 20% of jobs spend more than one third of their compute there. The median job spends about 10%. Data stalls are a heavy-tailed problem, not a universal one.

### Cited Findings
- Murray et al., tf.data (VLDB 2021), covering millions of jobs over one month in 2020:
  - "20% of jobs spend more than a third of their compute time ingesting data".
  - "30% of the total compute time is spent ingesting data".
  - The median job spends 10% in the input pipeline.
  - For about 75% of jobs the pipeline reduces data volume. Filters drop more than 10% of bytes in about 23% of jobs, and more than half in 8%.
  - 10% of pipelines account for 77% of executions and 72% of input-pipeline compute. About 75% of pipelines run more than once an hour, yet only 19% of jobs use `cache`.
  — [arXiv 2101.12127](https://arxiv.org/abs/2101.12127) (PDF text, Fig. 1, §5) (accessed 2026-10-29)
- tf.data service (Audibert et al., SoCC 2023):
  - About 20% of Google accelerator usage benefits from distributed (disaggregated) data processing.
  - Input-bound jobs averaged 31.7× throughput and 26.2× cost reduction. Per model: M1 11.7×, M2 110.3×, M3 2.9×, ResNet50 2.57×.
  - The top 10 CPU-heavy jobs used up to 25× more CPU cores than the host had.
  - Cloud TPU hosts have 96 cores per accelerator.
  — [arXiv 2210.14826](https://arxiv.org/html/2210.14826v3) (accessed 2026-10-29)
- Mohan et al. (VLDB 2021):
  - DNNs need 3–24 CPU cores per GPU for preprocessing and spend up to 65% of epoch time in preprocessing, "primarily on redundant decoding".
  - The OS page cache thrashes.
  - The CoorDL loader gives up to 5× speedup on one server.
  — [arXiv 2007.06775](https://arxiv.org/pdf/2007.06775) (Table 1) (accessed 2026-10-29)
- Cachew (ATC 2022): autoscaling and autocaching give up to 4.1× lower training time vs. Kubernetes scaling. — [USENIX](https://www.usenix.org/conference/atc22/presentation/graur) (accessed 2026-10-29)
- Pecan (ATC 2024): reordering transforms and hybrid placement cut preprocessing cost by 87% on average and total training cost by up to 60% vs. disaggregated preprocessing, and by 55% on average vs. colocated. — [USENIX](https://www.usenix.org/conference/atc24/presentation/graur) (accessed 2026-10-29)
- MinatoLoader (arXiv Sep 2025):
  - Uneven per-sample preprocessing time causes head-of-line blocking; PyTorch's default loader can leave GPUs up to 76% idle.
  - On 4×A100: up to 7.5× (3.6× average) faster than PyTorch DataLoader and Pecan, and 2.2× average vs. DALI.
  - GPU utilization went from 46.4% to about 90%.
  — [arXiv 2509.10712](https://arxiv.org/html/2509.10712v2) (accessed 2026-10-29)
- Seneca (arXiv Nov 2025): 45.23% makespan reduction vs. PyTorch. It names image, video, audio and recommendation models as stall-prone. — [arXiv 2511.13724](https://arxiv.org/html/2511.13724v1) (accessed 2026-10-29)

### Inferences
- The Google 2020 numbers are 6 years old. GPUs have since grown faster per host CPU core (H100/B200 vs. V100), so the CPU:accelerator imbalance has likely grown (estimate).

### Gaps
- No 2024–26 fleet-wide input-bound statistics from Google, Meta or any cloud were found.
- Lotus and FastFlow numbers were not retrieved (tool budget).

## Q3: Tabular (GBDT / tabular DL)

### Takeaway
There is no academic measurement of this. Community threads show tabular DL on Parquet routinely running at 7–10% GPU utilization because of PyArrow row iteration on the CPU. GBDT (XGBoost/LightGBM) loads data into memory once (DMatrix), so per-step stalls don't apply. Its cost is the one-off load/ETL (estimate).

### Cited Findings
- PyTorch forum, 2025-10-31:
  - 200M-row numeric Parquet dataset (120 GB), read with PyArrow per worker → about 7% GPU utilization and "severe CPU bottleneck".
  - Prefetch exhausts 64 GB RAM.
  - No replies; mentions NVTabular/Dask, and that the torch Parquet dataset package is deprecated.
  — [discuss.pytorch.org](https://discuss.pytorch.org/t/parquet-data-loading-strategy/223768) (accessed 2026-10-29)
- PyTorch forum, 2024-11-19: HF Datasets streaming 4,000 Parquet/Snappy shards from GCS gave about 10% GPU utilization at 8 workers; more workers ran out of memory. No replies. — [discuss.pytorch.org](https://discuss.pytorch.org/t/low-gpu-utilization-10-with-hf-datasets-streaming-on-vertex-with-pytorch-trainer/213157) (accessed 2026-10-29)

### Inferences
- The tabular pain is "Parquet → batched tensors" with per-row Python overhead. It is decode/convert-shaped, not join-shaped (estimate).

### Gaps
- No papers quantify stalls for XGBoost or tabular-DL training specifically.

## Q4: LLM pretraining vs. multimodal/video

### Takeaway
Text LLM pretraining is rarely input-bound: it reads pre-tokenized, memory-mapped token buffers. Video and multimodal training is heavily decode-bound: preprocessing can take 2–6.5× the GPU time.

### Cited Findings
- Megatron/NeMo and Apertus (2026) store pre-tokenized binary files with an index and read them via mmap. — [NeMo docs](https://docs.nvidia.com/nemo-framework/user-guide/25.09/data/index.html); [arXiv 2604.12973](https://arxiv.org/pdf/2604.12973) (accessed 2026-10-29)
- A 2025 student seminar report on Megatron finds mmap reduces loader time and batch size drives loader overhead. This is weak evidence. — [HPS report](https://hps.vi4io.org/_media/teaching/autumn_term_2025/stud/hpcsa/karim-elezabawy.pdf) (accessed 2026-10-29)
- SAND (KAIST):
  - Video preprocessing takes 2.2–6.5× longer than GPU training time.
  - SAND reports 6.0× GPU utilization and 75.9% less training time (emulated).
  — [SAND PDF](https://ina.kaist.ac.kr/assets/bibliography/SAND.pdf) (accessed 2026-10-29)
- Meta's PyTorch Conf 2024 talk "Blobs to Clips" covers GPU starvation from video decode/sampling and a new decoding library (TorchCodec). — [sched](https://pytorch2024.sched.com/event/1fHn5/blobs-to-clips-efficient-end-to-end-video-data-loading-andrew-ho-ahmad-sharif-gokul-gunasekaran-meta-kaiyue-yang-meta-platforms-inc) (accessed 2026-10-29)
- An Oracle blog on Nemotron multimodal reports about 12% H100 utilization from CPU decode and clip assembly, rising to about 50% after fixes. This comes from a search snippet only; the page returned 403. — [Oracle blog](https://blogs.oracle.com/ai-and-datascience/multimodal-data-processing-nvidia-nemotron-model) (accessed 2026-10-29)
- D2E (arXiv Oct 2025) calls I/O and data-pipeline bottlenecks "critical limitations" in large-scale video training and uses adaptive batch decoding. — [arXiv 2510.05684](https://arxiv.org/pdf/2510.05684) (accessed 2026-10-29)

### Inferences
- LLM data cost lives offline (dedup, filtering, tokenization), not in the trainer loop. Vision and video stalls are JPEG/video-decode and augment-shaped.

### Gaps
- No published LLM run quantifies loader stall as a share of MFU. "Negligible" is folklore here, not a measured result.

## Q5: SQL-shaped or decode-shaped?

### Takeaway
At train time the bottleneck is mostly decode, convert and per-feature transform. SQL-shaped work (joins, aggregations, label joins) sits upstream in Spark ETL. The SQL-like operations that do reach the trainer are projection (about 10% of features) and partition/row filters.

### Cited Findings
- Meta: online transforms are per-mini-batch and per-feature; joins and labels are offline. — [arXiv 2108.09373](https://arxiv.org/html/2108.09373v4) (accessed 2026-10-29)
- Google: filters discard more than 10% of bytes in 23% of jobs; maps change volume by an order of magnitude in 13%. — [arXiv 2101.12127](https://arxiv.org/abs/2101.12127) (accessed 2026-10-29)
- Uber: PyArrow→NumPy conversion was named as a CPU bottleneck. — [arXiv 2604.21275](https://arxiv.org/abs/2604.21275) (accessed 2026-10-29)
- Mohan: preprocessing time is dominated by redundant decoding. — [arXiv 2007.06775](https://arxiv.org/pdf/2007.06775) (accessed 2026-10-29)

### Inferences
- A trainer-side SQL engine would win mainly on projection, filter and type conversion through vectorized columnar decode, and on cross-job reuse (Google: 75% of pipelines re-run hourly; Meta RecD dedup). Join execution itself is not the bottleneck (estimate).

### Gaps
- No 2024–26 Reddit or HN threads with dates were collected (tool budget). The community evidence is limited to the two PyTorch forum threads.
