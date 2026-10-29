# Existing tools for feeding training GPUs with prepared tabular/feature data (as of 2026-10-29)

All URLs accessed 2026-10-29. GitHub activity figures come from `gh api repos/<r>` and `/commits` / `/releases/latest`, queried 2026-10-29.

## Q1. Per-tool landscape: GPU transforms, SQL/joins, tensor handoff, maintenance, limitations

### Takeaway
No maintained tool does all of the following: SQL with joins on the training GPU, then a zero-copy (DLPack) handoff to the trainer's tensors. The pieces exist separately. cuDF/cudf-polars and Sirius do GPU SQL-ish relational work. cuDF's `to_dlpack` and cudf-polars' experimental `persisted_to_torch` hand off tensors. DALI, Ray Data and Daft do GPU transforms or scheduling, but have no GPU SQL. NVIDIA's purpose-built tabular loader (NVTabular/Merlin) is effectively frozen.

### Cited Findings

**NVIDIA DALI**
- Actively maintained: v2.4.0 released 2026-10-26; repo pushed 2026-10-29. The release notes list no deprecations and add GCS reader, dynamic-mode arithmetic fixes, CUDA 13.4U1 and S3 webdataset index — [DALI v2.4.0 release](https://github.com/NVIDIA/DALI/releases/tag/v2.4.0) (accessed 2026-10-29)
- GPU operators target media (image/video/audio decode and augment). The v2.4.0 notes mention no tabular reader, only a NumPy reader fix. There is no SQL or join support — [DALI v2.4.0 release](https://github.com/NVIDIA/DALI/releases/tag/v2.4.0) (accessed 2026-10-29)
- PyTorch handoff: `DALIGenericIterator` returns torch tensors that "are still owned by DALI. They are valid till the next iterator call", so the user must copy them to keep them. `feed_ndarray` *copies* a DALI tensor into a torch tensor (CPU or GPU) — [DALI PyTorch plugin API](https://docs.nvidia.com/deeplearning/dali/archives/dali_1_24_0/user-guide/docs/plugins/pytorch_plugin_api.html) (accessed 2026-10-29)

**NVTabular / Merlin / Merlin Dataloader**
- NVTabular: last release v23.08.00 (2023-08-29). Recent commits are only security/serialization fixes: 5dd11f4a33 "fix for loading" (2025-10-23), 08e06339fb (2026-03-12), 1d1bbca785 "remove serialization.py" (2026-05-22) — [NVTabular commits](https://github.com/NVIDIA-Merlin/NVTabular/commits) (accessed 2026-10-29)
- The 2025 fixes correspond to CVE-2025-33214 (CVSS 8.8, Workflow deserialization), patched per NVIDIA's Dec 2025 bulletin citing commits 5dd11f4 / 876f19e — [secondary report](https://www.redhotcyber.com/?p=145028) (accessed 2026-10-29). This is secondary; the NVIDIA bulletin itself was not fetched.
- Merlin meta-repo: last release v24.06.00 (2024-06-14). Its only commit since 2024-07-22 is 37eee9a295 "Update privacy compliance and cookie consent" (2026-07-21) — [Merlin commits](https://github.com/NVIDIA-Merlin/Merlin/commits) (accessed 2026-10-29)
- Merlin dataloader: last release v23.08.00. Last commit 1441a12e5c is from 2023-10-17, and the repo was last pushed 2024-04-16 — [dataloader](https://github.com/NVIDIA-Merlin/dataloader) (accessed 2026-10-29). Its torch backend uses DLPack (`to_dlpack` appears in `merlin/dataloader/torch.py`), per GitHub code search.
- None of the repos is archived, and their READMEs contain no deprecation notice (grep of READMEs, 2026-10-29). The NGC `nvtabular` container is marked deprecated — [NGC](https://catalog.ngc.nvidia.com/orgs/nvidia/containers/nvtabular) (accessed 2026-10-29)
- NVTabular did GPU ETL with cuDF/Dask-cuDF (Categorify, joins via JoinExternal), but had no SQL frontend.

**cuDF + DLPack, cudf.pandas**
- Very active: the repo moved to `NVIDIA/cudf`; latest release v26.10.02 (2026-10-20, SHA 2cd6c6c087) — [NVIDIA/cudf](https://github.com/NVIDIA/cudf/releases) (accessed 2026-10-29)
- `Frame.to_dlpack` at v26.10.02 (python/cudf/cudf/core/frame.py:806):
  - It raises `NotImplementedError("non-numeric data is not yet supported")`.
  - It casts all columns to a common dtype.
  - Its output is column-major (Fortran order), so the docstring says "transpose the output" for row-major.
  - Source: [frame.py@v26.10.02](https://github.com/NVIDIA/cudf/blob/v26.10.02/python/cudf/cudf/core/frame.py#L806) (accessed 2026-10-29)
- cudf.pandas runs supported ops on the GPU and falls back to CPU pandas otherwise — [NVIDIA cuDF](https://developer.nvidia.com/cudf) (accessed 2026-10-29). There is no SQL API: dask-sql (below) is the RAPIDS SQL path and is unmaintained.

**cudf-polars (Polars GPU engine) and `to_torch`**
- If the GPU engine does not support a query plan, it "transparently falls back to the standard Polars engine and runs on the CPU" — [cudf_polars docs](https://docs.nvidia.com/cudf/latest/cudf_polars/) (accessed 2026-10-29)
- Polars' own `DataFrame.to_torch` is labelled "unstable" and returns a CPU tensor: the result is collected to a host DataFrame first — [polars to_torch](https://docs.pola.rs/api/python/dev/reference/dataframe/api/polars.DataFrame.to_torch.html) (accessed 2026-10-29)
- New and experimental: `cudf_polars.engine.torch_interop.persisted_to_torch` plus `SPMDEngine.from_torch_distributed()` run *inside* a torchrun job.
  - Columns become "zero-copy views of that GPU memory".
  - Columns must be numeric or bool with no nulls.
  - All ranks "must issue the same queries in the same order".
  - Each rank's result must fit in GPU memory.
  - The tensors keep engine memory alive, so the docs say to `.clone()` the ones you keep.
  - Users must call `use_gpu()`, not `torch.cuda.set_device`.
  - The docs say: "This is experimental and may change without notice."
  - Source: [cudf_polars PyTorch](https://docs.nvidia.com/cudf/latest/cudf_polars/pytorch/) (accessed 2026-10-29)

**Ray Data**
- Ray 2.59.0 released 2026-10-02 — [ray releases](https://github.com/ray-project/ray/releases) (accessed 2026-10-29)
- Ray 2.58 (blog dated 2026-08-25) added `map_batches(batch_format="cudf")` and an experimental GPU shuffle via RapidsMPF (repartition, groupby).
  - It has no GPU joins or SQL.
  - "Data still moves between host and device across stages"; fusing cuDF stages to stay on the GPU is only planned.
  - Source: [Anyscale: GPU-native operators in Ray Data](https://anyscale.com/blog/gpu-native-operators-in-ray-data) (accessed 2026-10-29)
- `iter_torch_batches` yields `Dict[str, torch.Tensor]`. The docs say a batch size is required when using GPUs, and that specifying both num_cpus and num_gpus is experimental — [map_batches](https://docs.ray.io/en/latest/data/api/doc/ray.data.Dataset.map_batches.html), [Working with PyTorch](https://ray.readthedocs.io/en/master/data/working-with-pytorch.html) (accessed 2026-10-29)

**Daft**
- Active: v0.7.26 released 2026-10-06 — [Daft releases](https://github.com/Eventual-Inc/Daft/releases) (accessed 2026-10-29)
- Daft's GPU support is resource scheduling for UDFs (`@daft.cls(gpus=N)`, used for model inference). The docs describe no GPU execution of relational operators — [Daft UDF docs](https://docs.daft.ai/en/stable/custom-code/udfs/) (accessed 2026-10-29). Daft has a SQL frontend, but it runs on CPU (Rust).

**PyTorch DataLoader + DuckDB**
- No dedicated integration was found. The pattern is DuckDB → Arrow/NumPy on CPU → torch → `.cuda()`, so it always crosses the host. (No authoritative source was found; see Gaps.)

**torchdata**
- DataPipes and DataLoader2 were deprecated in 0.8.0 (July 2024). v0.9 was the last release that includes them, and 0.10 removed them. The replacement is `torchdata.nodes` (composable iterators, threads or processes, state_dict) — [torchdata status](https://pytorch.org/data/beta/what_is_torchdata_nodes.html), [PyPI](https://pypi.org/project/torchdata/) (accessed 2026-10-29)
- The repo moved to `meta-pytorch/data`. Last release v0.11.0 (2025-02-21); last commit ee79a399d3 (2026-09-16). It is a CPU-side pipeline with no SQL — [meta-pytorch/data](https://github.com/meta-pytorch/data) (accessed 2026-10-29)

**Petastorm**
- Alive in maintenance mode: v0.13.1 released 2026-01-02. Commit f47d304130 (2025-12-19) added a local disk cache — [petastorm](https://github.com/uber/petastorm/commits) (accessed 2026-10-29)
- It reads Parquet into CPU numpy/torch. Databricks now documents Mosaic Streaming as its Spark→PyTorch path (see Spark below).

**WebDataset**
- Tags v0.3.2 (commit f3285808f3, 2025-02-10) and v0.2.107. Last commit e0953f9bba (2026-02-06) only touched type annotations — [webdataset](https://github.com/webdataset/webdataset) (accessed 2026-10-29)
- It reads tar shards for media samples, on CPU, with no SQL.

**Meta DPP / TorchArrow / TorchRec**
- DPP is a disaggregated online preprocessing service that scales to hundreds of nodes. Meta reports it "eliminat[es] data stalls that can reduce training throughput by 56%". It is internal and not open source — [ISCA'22, arXiv 2108.09373](https://arxiv.org/abs/2108.09373) (accessed 2026-10-29)
- TorchArrow is archived on GitHub. Its last push was 2024-03-29 and its only release v0.1.0 (2022-07-13) — [pytorch/torcharrow](https://github.com/pytorch/torcharrow) (accessed 2026-10-29)
- TorchRec is active (v1.9.2-rc3, 2026-09-30) — [meta-pytorch/torchrec](https://github.com/meta-pytorch/torchrec) (accessed 2026-10-29). It consumes KeyedJaggedTensor batches; it does not do upstream SQL.

**Mosaic StreamingDataset**
- Last release v0.13.0 (2025-07-15); last commit d99bf9c7cd (2026-06-25), a dependency update — [mosaicml/streaming](https://github.com/mosaicml/streaming) (accessed 2026-10-29)
- On Databricks, data is preprocessed in Spark, written to MDS with `dataframe_to_mds`, then read with `StreamingDataset`. It is preinstalled in DBR 15.2 ML+ — [Databricks docs](https://docs.databricks.com/aws/en/machine-learning/load-data/streaming) (accessed 2026-10-29)
- This is an offline materialization step, not SQL at training time.

**Hugging Face datasets**
- Active: 5.1.0 released 2026-10-05 — [huggingface/datasets](https://github.com/huggingface/datasets/releases) (accessed 2026-10-29)
- Arrow-backed and CPU-side; torch formatting produces CPU tensors. (General knowledge; no per-claim docs were fetched.)

**Lance / LanceDB**
- Very active: v14.0.2 released 2026-10-29 — [lance-format/lance](https://github.com/lance-format/lance/releases) (accessed 2026-10-29)
- `lance.torch.data.LanceDataset` subclasses `IterableDataset` and converts to `torch.Tensor`. The docs say "lance is not fork-safe" and recommend spawn or `get_safe_loader` — [Lance PyTorch](https://lancedb.github.io/lance/integrations/pytorch.html) (accessed 2026-10-29)
- A LanceDB `Table` implements the torch Dataset contract — [LanceDB torch](https://docs.lancedb.com/training/torch) (accessed 2026-10-29)
- Reads happen on CPU; filtering and SQL run in the CPU engine (DataFusion).

**Spark → torch (spark-rapids, Databricks)**
- The Databricks-recommended route is Spark → MDS → Mosaic StreamingDataset, with TorchDistributor support — [Databricks docs](https://docs.databricks.com/aws/en/machine-learning/load-data/streaming) (accessed 2026-10-29)
- Spark-RAPIDS can accelerate the ETL, but the data is materialized to files before training; it is not handed to the trainer's GPU.

**Theseus (Voltron Data)**
- Proprietary GPU SQL engine. AMD Instinct support was announced June 2025 — [The Register](https://theregister.com/2025/06/18/voltron_amd_sql) (accessed 2026-10-29)
- No public GitHub repo (`voltrondata/theseus` returns 404), and no documented torch/DLPack handoff was found.

**Sirius (DuckDB GPU extension)**
- Very active: commits on 2026-10-29, e.g. 73ed20b1c8 "run INNER ANY joins on the GPU - limited feature", 0b183b832e "stop dropping the SQL FILTER clause in GPU aggregates". There are no GitHub releases — [sirius-db/sirius](https://github.com/sirius-db/sirius/commits) (accessed 2026-10-29)
- Built on cuDF/RMM via Substrait plans. Unsupported operators "fall back silently to CPU". Two execution paths:
  - `gpu_execution`: out-of-core, with spilling, over Parquet.
  - `gpu_processing`: in-GPU-memory, over DuckDB storage.
  - Requires CUDA ≥ 13.0 and compute capability ≥ 7.0.
  - Source: [sirius README](https://github.com/sirius-db/sirius) (accessed 2026-10-29)
- Claims 5× over DuckDB on TPC-H SF1000 on a GB300 DGX Station — [sirius README](https://github.com/sirius-db/sirius); research paper [arXiv 2508.04701](https://arxiv.org/html/2508.04701v2) (accessed 2026-10-29)
- No documented torch or DLPack output path was found.

**BlazingSQL**
- Dead: last release v21.08.00 (2021-08-16); last push 2022-09-16 — [BlazingDB/blazingsql](https://github.com/BlazingDB/blazingsql) (accessed 2026-10-29)

**dask-sql / Dask-cuDF**
- rapidsai/dask-sql is archived (last push 2022-01-05).
- The dask-contrib/dask-sql README was changed in commit f186de3f9f (2024-06-14) to say "Dask-SQL is currently not in active maintenance" (see issue #1344).
- Commit 775b56fb8f (2024-08-29) "Drop GPU CI files from repo"; last release 2024.5.0.
- Source: [dask-sql](https://github.com/dask-contrib/dask-sql) (accessed 2026-10-29)
- Dask-cuDF ships inside the cudf repo (v26.10.02). It has no SQL frontend.

### Inferences
- "SQL on the training GPU, then hand tensors via DLPack" is not offered as a product today. The closest pieces:
  - cudf-polars `persisted_to_torch`: experimental, SPMD lockstep, numeric and no-null columns only, Polars DSL rather than SQL text. Polars' `SQLContext` could likely feed it, but that is unverified.
  - Sirius: GPU SQL with growing join support, but no tensor export.
  - cuDF `to_dlpack`: numeric only, single-dtype cast, column-major.
- Every handoff path makes the same restrictions: numeric, non-null, homogeneous dtype. Categorical/string features and nulls need encoding upstream. NVTabular used to do that encoding and is now frozen.
- The GPU-native tools (cudf-polars, Sirius) silently fall back to CPU for unsupported plans. That is a performance-predictability gap for a training loop.
- Ray Data's GPU path still crosses PCIe between stages, by its own blog's admission.
- The industry pattern (Databricks, Mosaic) is offline materialization to shards, not live joins at training time.

### Gaps
- DALI ≥ 2.x DLPack and `__dlpack__` support for torch/JAX was not verified from current docs; the DALI evidence above is from the 1.24 archive.
- NVIDIA "GPU-accelerated data loading for Grace Hopper": no primary source was retrieved.
- No official Merlin EOL announcement was found. "Effectively unmaintained" is inferred from commit and release history only.
- Theseus's status after mid-2025 is unknown; no 2026 sources were found.
- Daft's "Working with GPUs" page was not fetched. No evidence was found of GPU-executed relational operators in Daft.
- No authoritative DuckDB→torch DataLoader integration doc was found, and no DuckDB DLPack export was confirmed.
- JAX handoff (jax.dlpack) for cudf-polars or Sirius: not documented.
