# quant-gpu — as-of joins, tick windows and backtest sweeps on a resident GPU cache

## Verdict

- **What it is:** a single-box GPU engine over local tick and bar Parquet. It does as-of / point-in-time joins, time-range windows over ticks, and parameter-sweep backtests, which re-run the same queries many times against data held in VRAM.
- **Who needs it:** demand.md rates quant-gpu **medium**. Users exist, but they already solve the problem with cuDF/CuPy. KX ships a GPU as-of join, and NVIDIA is building the same pieces in cudf-polars right now [D][K1][C3].
- **Budget:** fits. It is about 0.75× the engine on peacockdb and 0.45× on Sirius (estimate). The hard part is the sweep runner, not the as-of join. The window this product exploits (no as-of join in open-source RAPIDS) will likely close within 6–9 months [C2][C3].

## Who needs it badly

- **Rating: medium, taken from demand.md `## Ratings`, row `quant-gpu`** [D]. R/U/C 5/4/0, 9 posters. "Users exist, but they already solve it with cuDF/CuPy."
- **Backtest sweeps (retail and small shops):**
  - u/RoundTableMaker runs cuDF/CuPy grid searches: "from 1/sec to 100k in 13 seconds" ([r/quant](https://www.reddit.com/r/quant/comments/1svrkka/comment/oians0g/), 2026-04) [R].
  - u/estimated1 runs tens of thousands of permutations on 5 s bars, which "can take awhile, even when running across 192 cores" ([r/algotrading](https://www.reddit.com/r/algotrading/comments/1ctidxy/), 2024-05) [R].
  - u/EdwardM290 does a Bollinger grid search "with cuDF" ([r/algotrading](https://www.reddit.com/r/algotrading/comments/1iq21bf/comment/mdiesoz/), 2025-02) [R].
  - u/Phunk_Nugget does GPU genetic modelling on CME ES ([r/quant](https://www.reddit.com/r/quant/comments/1jcgceh/), 2025-03) [R].
  - u/1StunnaV processes every S&P 500 trade, 500–750 MB/day, on an RTX 5090 ([r/algotrading](https://www.reddit.com/r/algotrading/comments/1xf84n8/), 2026-10) [R].
  - Others are hand-coding: "vibecoding my own engine… using my GPU (CuPy)" ([r/algotrading](https://old.reddit.com/r/algotrading/comments/1wqrr91/_/pc6iswb), 2026-09-26); "my own GPU based tick level data broker simulation engine" ([r/algotrading](https://old.reddit.com/r/algotrading/comments/1x27xu3/_/pf0z42a), 2026-10-10) [I].
- **As-of joins and windows:**
  - An as-of join request in cudf-polars ([cudf #20727](https://github.com/NVIDIA/cudf/issues/20727), 2025-11).
  - G-Research's fixed rolling windows request ([cudf #20307](https://github.com/NVIDIA/cudf/issues/20307), 2025-10). It was merged in 2026-07 [C1].
  - stelgic wants a timestamp merge of Arrow tables in libcudf C++ ([NVIDIA forum](https://forums.developer.nvidia.com/t/merge-arrow-cudf-datatable-by-timestamp-libcudf-c/269569), 2023-10) [DF].
- **Point-in-time joins in ML feature stores, where the loudest demand is:**
  - jamesblonde (Hopsworks) says Spark is "hideously slow" for left as-of joins ([HN 41075956](https://news.ycombinator.com/item?id=41075956), 2024-07-26), and uses DuckDB instead ([HN 39073149](https://news.ycombinator.com/item?id=39073149), 2024-01-20) [B].
  - Polars SQL lacks an as-of join ([HN 49983173](https://news.ycombinator.com/item?id=49983173), 2026-10-06). Others ask for a tolerance clause ([ClickHouse #109034](https://github.com/ClickHouse/ClickHouse/issues/109034), 2026-07) and a windowed join ([polars #24091](https://github.com/pola-rs/polars/issues/24091), 2025-08) [B].
- **Institutional pain is about q talent, not speed.** A London hedge-fund researcher says teams are porting q to Python because q is hard to hire for and "AI is pretty awful at q". A reply says Polars `join_asof` covers the common cases, but multi-asset alignment needs "custom logic around the join" ([r/quant 1ugzkx7](https://old.reddit.com/r/quant/comments/1ugzkx7/), 2026-06-27; [ovdmhot](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ovdmhot), 2026-07-03) [I].
- **Missing:** no poster attaches a budget or a deadline. No first-person thread says "our as-of join on N billion rows takes hours" [B].
- **Best first conversations:**
  1. u/RoundTableMaker and u/estimated1, the sweep users.
  2. BMLL, which publishes its as-of workloads [B].
  3. A small crypto market maker of the kind on QuestDB's customer page [B].
  4. jamesblonde/Hopsworks, for feature-store point-in-time joins.

## How it would work

- **Front doors:**
  - a Python API that returns Arrow / cuDF-on-device results;
  - SQL through DataFusion 45.
  - sqlparser 0.53, which DataFusion 45 uses, already parses `ASOF JOIN … MATCH_CONDITION` (`src/ast/query.rs:1808`). DataFusion 45's SQL planner has no as-of path: `grep -i asof` over `datafusion-sql-45.0.0/src` is empty. So the engine needs its own planner rule or table function [L].
- **New nodes:**
  - **AsOfJoin** (backward, forward or nearest; optional `by` keys and tolerance). Sort the right side by `(sym, ts)`, then `cudf::upper_bound − 1` over the multi-column table, check that the symbol matches, and gather with NULLIFY. This is the same algorithm as the open `cudf::asof_join` PR [C2]. Switch to that primitive once it merges.
  - **Window** with time-range frames, through `grouped_range_rolling_window` / `make_range_windows` [C4].
    - peacockdb already has the kernel (`cpp/src/operators/window.cpp`, 114 lines) and a `CudfWindow` wire kind with no writer (architecture.md "three of the thirteen wire kinds have no writer", #143).
  - **Time bucket** (`date_bin` / xbar) plus groupby, for OHLC bars.
  - **Window join** (kdb `wj`): none of ClickHouse, DuckDB or Polars has one [I].
- **Ordering:** read sort order from Parquet footers so that day-partitioned, time-sorted data never needs a global sort. This mirrors cudf-polars' `set_sorted` work [C3].
- **Resident cache:** columns decoded into VRAM, with pinned host RAM as the next tier. The key is (file, column, row group). Symbols are dictionary-encoded to int32 at load time.
- **Sweep runner:**
  - The user gives a strategy as a parameterised SQL / expression plan, plus a parameter grid.
  - The runner batches K parameter rows as a small table, and evaluates `(param_id, sym, ts)` windows and signals in one pass over cached columns. It reduces each batch to per-parameter P&L.
  - Raw-CUDA / Numba strategy kernels can be plugged in through device-array hand-off, as the STAC-A3 pack used Numba beside cuDF [C5].
- **Data path end to end:**
  - **Cold:** local NVMe Parquet (one file per day) → host → chunked GPU decode → VRAM cache.
  - **Hot:** sweep or join runs on HBM. Only aggregates such as P&L per parameter, or the joined result, return as Arrow.

## The human's questions

**1. What these users run today, what they pay, what they complain about**

- **kdb+/q (KX).**
  - Prices are not public. Reported figures: per core with an 8-core minimum ([HN 2180016](https://news.ycombinator.com/item?id=2180016), 2011); "around $100,000/year" ([HN 19974192](https://news.ycombinator.com/item?id=19974192), 2019); "500k on licenses and the cost of people" ([HN 37850460](https://news.ycombinator.com/item?id=37850460), 2023) [I].
  - KDB-X Community Edition (GA 2025-11-17) is free for commercial use but capped:
    - 16 GB RAM, 24 cores, one instance;
    - "using GPU workflows to bypass the 16GB RAM limit is not permitted" ([usage restrictions](https://code.kx.com/licensing/usage-restrictions.html)) [I].
  - One full TAQ day is 296 GB in memory, so KX's own benchmark tells free users to run the 1 GB `tiny` set ([NYSETAQBenchmarks](https://github.com/KxSystems/NYSETAQBenchmarks)) [I].
  - Complaints in 2026 are about q talent, lock-in ("kdb developers are good at entrenching themselves") and silent syntax traps. Pro-kdb replies cite compression, IPC and speed ([r/quant 1ugzkx7](https://old.reddit.com/r/quant/comments/1ugzkx7/), 2026-06/08) [I].
- **KX's GPU work.**
  - The KDB-X GPU module was announced as production-ready on 2026-04-02 ([KX](https://kx.com/blog/introducing-kdb-x-gpu-acceleration/)) [K1].
  - It covers `.gpu.aj`, sorts, `.gpu.bin`, a restricted qSQL `select`, and multi-GPU via `peach`.
  - Vendor numbers:
    - `aj` about 4× on an L4 and about 5.5× on an A100, against 48 CPU cores;
    - 10M rows in about 31 ms against about 172 ms;
    - VaR about 58× on 5×H100 [K1][K2].
  - The 2025 incubation post says it is built on CUDA, cuDF and GPUDirect Storage ([KX](https://kx.com/?p=14992)). The 2026 posts do not mention cuDF [K2]. No price is published.
  - KX merged with OneTick on 2025-09-15, under TA Associates ([TA](https://ta.com/news/kx-and-onetick-merge-to-unite-capital-markets-data-analytics-ai-and-surveillance-on-one-platform)) [I].
- **OneTick:** about 160 clients and no public price; it is now part of KX ([onetick.com](https://www.onetick.com/company-overview)) [I].
- **DolphinDB:**
  - has asof and window joins plus streaming join engines, and a free community edition ([tutorial](https://github.com/dolphindb/Tutorials_EN/blob/master/join_engine.md); [AWS](https://aws.amazon.com/marketplace/pp/prodview-6j7wqm5tobvko));
  - no price and no GPU found [I].
- **ClickHouse ASOF JOIN:** exactly one closest-match condition; only the `hash` and `full_sorting_merge` algorithms; no NaN; no window join ([docs](https://clickhouse.com/docs/sql-reference/statements/select/join)) [I].
- **DuckDB ASOF:** at most one match, and `>=` only in `USING` form ([docs](https://duckdb.org/docs/2.0/guides/sql_features/asof_join.html)) [I].
- **Polars `join_asof`:** backward, forward and nearest, with tolerance and `by`. It needs sorted input and cannot verify sortedness when `by` is used ([docs](https://docs.pola.rs/api/python/stable/reference/dataframe/api/polars.DataFrame.join_asof.html)) [I].
  - BMLL's as-of join of 54.9M trades to the order book took 15.8 s on 192 cores / 1.5 TB ([Polars × BMLL](https://pola.rs/posts/case-bmll/), 2026-08-25) [B].
- **cuDF/CuPy home-grown sweeps** are the GPU status quo; see the threads above.
  - The only audited GPU backtest is STAC-A3 on a DGX-2 in 2019, built with cuDF, Numba and Dask: 20M simulations against the prior record of 3,200 ([HPCwire](https://www.hpcwire.com/2019/05/13/nvidia-claims-6000x-speed-up-for-stock-trading-backtest-benchmark/)) [C5].
- **vectorbt PRO:**
  - $25/month, $240/year or $500 lifetime; CPU only (Numba plus Rust);
  - vendor figure: 310,800 sweep results in under a minute on an M3 ([vectorbt.pro](https://vectorbt.pro/)) [I].
  - Open-source CPU options: NautilusTrader (31k stars), backtesting.py (9.2k) and vectorbt (9.6k) [I].
- **The price gap:** retail pays $240–500 a year; institutions pay $100k+ a year (anecdotal). There is little evidence of anything in between (estimate) [I].

**2. Do as-of joins and tick windows map well to cuDF? Does cuDF have an as-of join?**

- **No cuDF release has an as-of join** (cudf `main` @ `785ee522`, 26.12 dev) [C1]:
  - `cpp/include/cudf/join/` has no asof header.
  - cudf-polars raises "asof joins not yet exposed" (`dsl/translate.py:830`).
  - cudf.pandas `merge_asof` falls back to CPU pandas.
- **NVIDIA is building one now:**
  - PR #24180 adds `cudf::asof_join`: backward only, pre-sorted input, build once and probe many. It is open, with changes requested on 2026-10-28 ([PR](https://github.com/NVIDIA/cudf/pull/24180)) [C2].
  - Story #24110 puts forward, nearest, tolerance and pandas `merge_asof` later, at P1–P3 ([issue](https://github.com/NVIDIA/cudf/issues/24110)) [C2].
- **peacockdb has no as-of join either.** inventory.md lists 19 node kinds with no as-of or window node ("No window node (#143)"). The window kernel exists in C++ but has no planner or wire writer (architecture.md, wire kinds) [P].
- **The mapping is good:**
  - Multi-column `cudf::upper_bound` (`search.hpp:104` @ `785ee522`) gives a by-key backward as-of in one call [C4].
  - Time-range windows are mature: `grouped_range_rolling_window`, `make_range_windows`, and per-row delta columns (`rolling.hpp:183,215,632–692`) [C4].
  - Cost: O(n log n) to sort the right side, plus O(m log n) to probe (estimate).
  - Symbols must be dictionary-encoded first. String compares dominate otherwise, as the Daft and pandas authors also found ([HN 47212583](https://news.ycombinator.com/item?id=47212583)) [B].
- **The poor fit is sweeps written as dataframe operations.** Each parameter set is another rolling pass. Sweeps map well to batching parameters into the plan, or to custom kernels over resident columns (estimate) [C5].

**3. How secretive and hard to reach is this buyer? How does a two-person team get a first design partner?**

- **Hard at the top.** Large funds build in-house ([Risk.net](https://www.risk.net/risk-management/5704661/quant-fund-manager-spurns-vendors-machine-learning-software), paywalled) [B].
- **Procurement** means the AITEC-AIMA vendor questionnaire, 14 sections on KY3P, plus SOC 2 ([FOW](https://news.fow.com/articles/3587548/ihs-markit-links-with-hedge-fund-groups-to-drive-due-diligence-standard); [Alpha Week](https://alpha-week.com/why-hedge-fund-technology-vendors-need-soc-report)) [B].
- **They do not post workloads.** The public threads are retail or ML [B].
- **What worked for others:**
  - ArcticDB was a spin-out from inside Man Group; Bloomberg is its only named outside customer ([Man](https://www.man.com/news-centre/man-group-brings-powerful-dataframe-database-product-arcticdb-to-market-with-bloomberg)) [B].
  - Databento's founder ran an HFT fund, and a trading-firm-linked investor came in 2024 ([MIT Sloan](https://mitsloan.mit.edu/ideas-made-to-matter/hedge-fund-success-then-a-pivot-purpose); [Databento](https://databento.com/blog/databento-10m-additional-funding-2024)) [B].
  - QuestDB went open-source-first and collected crypto and mid-tier market-making logos: Anti Capital, OKX, Hidden Road, Marex ([customers](https://questdb.io/customers/)) [B].
- **A path for two people (estimate):**
  1. Open-source the engine and publish results on KX's public TAQ query set (q1–q84), which NVIDIA also targets [I][C3].
  2. Post on r/algotrading and r/quant with a sweep demo on Databento or Tardis sample data.
  3. Pitch a 5–50-person crypto market maker or prop shop, and tick-data platforms (BMLL, Databento, Tardis) whose product is this workload.
  4. Ship on-prem or as a library, so no data leaves the client. That removes most of the procurement burden, but not the AITEC questionnaire.
- **The human has no trading-firm insider.** Every successful precedent had one (estimate).

**4. Data sizes**

- **NYSE Daily TAQ** (spec v4.0, gzip) [B]:
  - trades: 69M rows, 2.4 GB/day;
  - quotes: 1.9B rows, 38 GB/day;
  - NBBO: 330M rows, 11 GB/day.
  - Source: [NYSE](https://www.nyse.com/publicdocs/nyse/data/Daily_TAQ_Client_Spec_v4.0.pdf).
  - KX's full day is 296 GB in memory, 187 GB on disk and 2.86B quotes [I]. A year is about 10 TB gzip (estimate).
- **OPRA:**
  - SIAC projects about 16.1B messages/day for July 2026 ([OPRA](https://cdn.opraplan.com/documents/notices/OPRA_Capacity_Test%20Reminder_041126.pdf));
  - Databento says "several terabytes of data per day" at full granularity ([Databento](https://databento.com/catalog/opra/OPRA.PILLAR)) [B].
- **CME ES:**
  - MBP-10 about 25 GB/month (anecdotal, [NexusFi](https://nexusfi.com/a/data/databento-futures-market-data));
  - 10 years of ES ticks is about 2.2B rows ([QuantNet](https://quantnet.com/threads/handling-2-2b-rows-of-es-futures-data-transitioning-from-csv-to-apache-parquet-for-backtesting.64264/latest), unconfirmed) [B].
- **Crypto:** Binance BTCUSDT futures incremental book is about 238 MB/day compressed ([hftbacktest](https://hftbacktest.readthedocs.io/en/py-v2.1.0/tutorials/Data%20Preparation.html)) [B].
- **Retail:** 500–750 MB/day for all S&P 500 trades [R].
- **Shanghai Stock Exchange L2:** about 94 GB/day, and over 1.5 TB for 16 days [B].
- **Feature stores:** no published row counts. Typical inputs are 10⁸–10¹⁰ label rows (estimate) [B].
- **Shape:**
  - Retail and crypto bar sweeps fit in one GPU, often in a few GB.
  - One TAQ day of quotes, about 110–190 GB raw (estimate), fits only after column pruning or a 141–288 GB GPU.
  - Multi-day OPRA and a year of TAQ do not fit, and need day-by-day streaming.

## Data path and cache

- **Bandwidths, measured on peacockdb's H200 unless marked** [P]:

  | Stage | Rate | Time for 100 GB |
  |---|---|---|
  | 100 GbE NIC / S3 (spec, estimate) | ~10–12 GB/s | ~9 s |
  | NVMe Gen4 / Gen5 SSD (spec, estimate) | ~7 / ~14 GB/s | ~14 / ~7 s |
  | Parquet decode on GPU (`read_parquet`) | 6.8–8.7 GB/s | 11.5–14.7 s |
  | H2D pageable / pinned | 10.6 / 47.3 GB/s | 9.4 / 2.1 s |
  | HBM read | 4,484 GB/s | 0.02 s |

  - The engine's `read_parquet` is about 5× slower than the chunked reader on GB10 (#232) [P].
- **One-shot as-of join from cold Parquet: the thesis holds, and the GPU's edge is eaten.**
  - A TAQ-day join is decode-bound at about 8 GB/s. A 64-core CPU engine reading the same Parquet is in the same range (estimate).
  - Even with keys already in VRAM, KX gets only 4–5.5× over 48 cores on `aj` [K1].
  - peacockdb's own PoC loaded for longer than it computed on 11 of 12 queries [E].
- **Sweeps: the thesis holds, and the cache is what wins.**
  - A sweep reads the same 1–50 GB thousands of times. With a VRAM-resident cache, each pass costs HBM time (milliseconds), not decode time (seconds).
  - That is where STAC-A3's large ratios come from [C5].
- **Cache needed:**
  - VRAM for the hot (sym, ts, price, size) columns, decoded with dictionary symbols;
  - pinned host RAM as the next tier;
  - day-partitioned Parquet on local NVMe as the source.
  - Neither object storage nor NIC is on the hot path.
  - For data over VRAM, stream day by day: as-of joins partition naturally by day and symbol, with a carry-over of the last right row per symbol [C3].
- **peacockdb has none of this today:** no cache, pageable buffers, one file per table [P]. Sirius has `pin_table` (no eviction) and multi-file scans [S].

## Build cost to alpha

| Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
|---|---|---|
| SQL frontend, planner, base operators | reuse (DataFusion 45, 19 node kinds, cuDF ops) [P] | reuse (DuckDB parses ASOF JOIN natively) [S] |
| GPU entry point + Python API (Arrow / device arrays out) | new, 1.5–2.5k | reuse DuckDB Python, adapt device hand-off, 0.5–1k |
| Multi-file (per-day) tables, chunked reader, pinned buffers | adapt, 1–2k [P] | reuse [S] |
| Resident VRAM + pinned cache with eviction | new, 2–4k | reuse `pin_table`, adapt eviction, 0.5–1k [S] |
| Sort-order awareness from footers (no global sort) | adapt, 0.5–1k | adapt, 0.5–1k (estimate) |
| Dictionary-encoded symbols through join/window/groupby | new, 0.5–1k | reuse Simpatico dictionary, adapt 0.3–0.5k [S] |
| AsOfJoin node: planner rule, node, wire kind, `upper_bound` kernel; forward, nearest, tolerance | new, 1.5–2.5k | new GPU operator from DuckDB's ASOF plan, 2–3k (no as-of operator in sirius.md's table; estimate) |
| Window functions with time-range frames (#143; kernel exists) | new, 1.5–2.5k | wire up the unwired window, 1.5–2.5k (#1745) [S] |
| Time bucket + OHLC; window join (`wj`) | new, 1.5–2k | new, 1.5–2k |
| Sweep runner: parameter batching, plan reuse, per-parameter reduction, custom-kernel hook | new, 2–4k | new, 2–4k |
| Spill | out of alpha (stream by day; fail cleanly) | reuse cuCascade [S] |

Total new work: 0.75× engine on peacockdb, 0.45× on Sirius (estimate)

- **Arithmetic.** The peacockdb midpoints sum to about 17k against the 22,830-line yardstick. The Sirius midpoints sum to about 10.5k.
- **Sirius is plausible here.** DuckDB already parses and plans ASOF, and quants already use DuckDB [I].
  - The cost is control: about 92% NVIDIA commits, no versions, and the pin code is still changing [S].
  - Until Sirius has a GPU as-of operator, DuckDB runs ASOF on the CPU.
- **The hard part is the sweep runner.**
  - Real strategies are branchy and stateful: position, stops, fills. They don't reduce to SQL windows (discovery.md skeptics) [DS].
  - So the alpha must either restrict strategies to vectorisable signal → position → P&L, as vectorbt does [I], or host user CUDA/Numba kernels over cached columns.
  - The second hard part is making nearest/tolerance and multi-GPU as-of correct across day and partition boundaries [C3].

## Adoption friction

- **A new engine to adopt, not a plugin.**
  - Retail and small shops live in Python (pandas, Polars, vectorbt). They will adopt a pip-installable library with a Polars-like API more readily than a SQL engine (estimate) [D].
- **A free GPU route already exists and is improving.**
  - cudf-polars `join_asof` and the TAQ queries are in progress [C3].
  - Once it lands, `pl.scan_parquet(...).collect(engine="gpu")` gives users a GPU as-of join with no new tool.
- **Institutions sit on kdb and its q codebase.**
  - Migrating off it is happening, but toward Python [I].
  - A product would have to be the Python target of that migration, at kdb-like speed.
- **What eases adoption:**
  - Parquet in, Arrow out;
  - runs on one workstation GPU (an RTX 5090 is what retail owns) [R];
  - no RAM cap, unlike KDB-X Community [I];
  - results checked against Polars/DuckDB as an oracle.
- **Install pain is the biggest GPU complaint overall** (about 33 threads) [D]. A 20–40 GB RAPIDS dependency hurts retail.

## Platform risk

- **High, from two directions.**
  - **NVIDIA:**
    - cudf #24634 (opened 2026-10-15, "In Progress") targets every KX TAQ query fully on GPU in cudf-polars, on one or more GPUs: as-of join, `group_by_dynamic`, streaming ordered windows, categorical `sym` [C3].
    - `cudf::asof_join` is an approved-in-part open PR [C2].
    - Expect a GPU backward as-of join in RAPIDS around 26.12–27.02 (estimate).
    - NVIDIA gives away complete engines and staffs them; the risk is commoditisation [N].
  - **KX:** it ships GPU `aj`, sort and select inside KDB-X for its installed base, and now owns OneTick too [K1][I].
- **What is left after both ship (estimate):**
  - window joins;
  - tolerance/nearest at scale;
  - a sweep runner over a resident cache;
  - a non-q, non-Polars SQL surface.
  - Only the sweep runner is clearly outside NVIDIA's announced scope.
- **DuckDB + Sirius** could add a GPU ASOF operator upstream at any time (estimate) [S].

## Funding story

- **What a design partner would say, if it works (estimate):**
  - A crypto market maker: "Our nightly sweep over 6 months of L2 takes 8 hours on 192 cores. On one H100 with the data resident, it takes 10 minutes, and our researchers iterate 20× a day."
  - That is a measurable, repeatable claim. The STAC-A3 history makes it believable [C5].
- **What the round would be raised on:**
  - "GPU backtesting and tick research for the post-kdb, Python-first quant";
  - open-source core, paid on-prem support;
  - one or two named market makers or data vendors;
  - a public TAQ benchmark.
- **The weaknesses:**
  - ARPU is bimodal: retail at $240–500/yr, institutions with procurement walls [I][B].
  - The open-source as-of join will be free from NVIDIA [C3].
  - Investors have seen GPU-database failures: Voltron, BlazingSQL, MapD [N].
  - The founder has no trading-firm credibility, which every precedent (ArcticDB, Databento) had [B].

## Evidence against

- **The as-of join gap is closing in months, and for free.** NVIDIA is implementing exactly the KX TAQ feature list in cudf-polars [C2][C3]. KX already ships GPU `aj` [K1].
- **The GPU gain on as-of joins is modest.** KX, with keys in VRAM, reports 4–5.5× against 48 cores [K1]. From cold Parquet, decode at about 8 GB/s caps the gain [P].
- **Users already solve it themselves.** Sweep users wrote CuPy/cuDF scripts and are happy ("100k in 13 seconds") [R]. demand.md's rating rests on exactly this [D].
- **Sweeps are not queries.** Strategy logic is stateful and branchy. A SQL engine covers only the vectorisable part, which vectorbt already does fast on CPU (310,800 results in under a minute on an M3) [I][DS].
- **The buyers who would pay are the hardest to reach,** and they don't post their workloads [B]. Retail pays $240–500 a year [I].
- **Public demand for faster point-in-time joins comes from Spark feature stores, not quants** [B]. That is a different product: Spark/Feast integration, not tick windows.
- **Data at retail scale is small:** 500 MB–a few GB per day. Polars handles it [R][B].

## Ratings

- **Who needs it badly: medium.** This is demand.md's rating. 9 posters, but they self-serve with cuDF/CuPy, and none states a budget [D].
- **Build cost: strong.** About 0.75× on peacockdb and 0.45× on Sirius. The window kernel and the cuDF primitives already exist [P][C4].
- **Data path: medium.** Sweeps over a resident cache win big. One-shot as-of joins over cold Parquet are decode-bound and roughly break even with CPU [P][K1].
- **Adoption friction: medium.** Parquet and Python fit the users' habits. But it is a new engine, and cudf-polars will offer a zero-friction GPU route [C3].
- **Platform risk: weak.** NVIDIA's TAQ epic and `cudf::asof_join` PR, plus KX's GPU module, cover the core features [C2][C3][K1].
- **Funding story: weak.** A plausible sweep demo, but bimodal ARPU, closed institutional buyers, no insider founder, and a free NVIDIA alternative [B][I].

## Follow-up experiments

1. **Sweep on a resident cache vs vectorbt PRO / NumPy on 64 cores.** 6 months of 1 s bars for 500 symbols, 10k parameter sets, mean reversion. Measure wall time, and the batch size K where VRAM saturates.
2. **As-of join from cold Parquet.** One TAQ day (trades → quotes by sym), GPU `upper_bound` recipe vs Polars and DuckDB on 64 cores. Split time into read/decode/H2D/compute, to test whether decode eats the gain.
3. **Hot as-of join.** The same join with both sides in VRAM, against KX's ~31 ms per 10M rows, to set the ceiling [K1].
4. **`cudf::asof_join` (PR #24180) vs the `upper_bound` recipe**, once merged. If equal, peacockdb's operator is a thin wrapper.
5. **Run KX's TAQ q1–q84 suite** on cudf-polars nightly vs the prototype. This shows how much of the gap NVIDIA has already closed [I][C3].
6. **Ten conversations:** r/algotrading sweep users, two crypto market makers, BMLL. Ask for runtime, data size and what they would pay; settle ARPU before building.

## Sources

- [D] demand.md, `## Ratings` row quant-gpu, and `## First conversations` (local; accessed 2026-10-29)
- [DS] discovery.md, rows asof-gpu and backtest-sweep-gpu (local; accessed 2026-10-29)
- [R] sources/reddit.md (local; accessed 2026-10-29), and the Reddit links cited inline (accessed 2026-10-28)
- [DF] sources/discord-forums.md (local; accessed 2026-10-29)
- [P] inventory.md at master `f25c250d` (sections "Node vocabulary", "Cross-cutting capabilities", "Benchmark evidence", "Yardstick"), and llm-wiki/architecture.md (wire kinds; `CudfWindow` has no writer). `cpp/src/operators/window.cpp`, 114 lines, at `f36a68ec` (local; accessed 2026-10-29)
- [S] sirius.md, `## Sirius components` and the cache sections, Sirius @ b9e05ffc (local; accessed 2026-10-29)
- [N] nvidia.md (local; accessed 2026-10-29)
- [E] evidence-in-hand.md (local; accessed 2026-10-29)
- [L] sqlparser 0.53.0 `src/ast/query.rs:1808` (`JoinOperator::AsOf`) and datafusion-sql 45.0.0 (no asof), from the cargo registry as locked in Cargo.lock (local; accessed 2026-10-29)
- [C1] cudf @ 785ee522f5688f485fc0b1eabca7f8d6d2adea0e: https://github.com/NVIDIA/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e ; https://github.com/NVIDIA/cudf/issues/20727 ; https://github.com/NVIDIA/cudf/issues/20307 (accessed 2026-10-29)
- [C2] https://github.com/NVIDIA/cudf/pull/24180 ; https://github.com/NVIDIA/cudf/issues/24110 (accessed 2026-10-29)
- [C3] https://github.com/NVIDIA/cudf/issues/24634 (accessed 2026-10-29)
- [C4] https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/search.hpp ; https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/rolling.hpp (accessed 2026-10-29)
- [C5] STAC-A3 on DGX-2: https://stacresearch.com/node/44695 ; https://www.hpcwire.com/2019/05/13/nvidia-claims-6000x-speed-up-for-stock-trading-backtest-benchmark/ ; STAC-M3 kdb 4.1 (CPU only): https://stacresearch.com/news/stac-m3-benchmark-results-kx-kdb-4-1-on-supermicro-micron-intel/ (accessed 2026-10-29)
- [K1] https://kx.com/blog/introducing-kdb-x-gpu-acceleration/ ; https://code.kx.com/kdb-x/modules/gpu/introduction.html (accessed 2026-10-29)
- [K2] https://kx.com/?p=14992 ; https://kx.com/blog/whats-new-in-kdb-x-gpu-acceleration-nested-columns-profiling-and-performance-tuning/ (accessed 2026-10-29)
- [I] Incumbents (accessed 2026-10-29): https://code.kx.com/licensing/usage-restrictions.html ; https://github.com/KxSystems/NYSETAQBenchmarks ; https://news.ycombinator.com/item?id=2180016 ; https://news.ycombinator.com/item?id=19974192 ; https://news.ycombinator.com/item?id=37850460 ; https://old.reddit.com/r/quant/comments/1ugzkx7/ ; https://ta.com/news/kx-and-onetick-merge-to-unite-capital-markets-data-analytics-ai-and-surveillance-on-one-platform ; https://www.onetick.com/company-overview ; https://aws.amazon.com/marketplace/pp/prodview-6j7wqm5tobvko ; https://clickhouse.com/docs/sql-reference/statements/select/join ; https://duckdb.org/docs/2.0/guides/sql_features/asof_join.html ; https://docs.pola.rs/api/python/stable/reference/dataframe/api/polars.DataFrame.join_asof.html ; https://vectorbt.pro/ ; https://github.com/nautechsystems/nautilus_trader ; https://github.com/kernc/backtesting.py ; https://github.com/polakowo/vectorbt
- [B] Buyers and sizes (accessed 2026-10-29): https://www.nyse.com/publicdocs/nyse/data/Daily_TAQ_Client_Spec_v4.0.pdf ; https://databento.com/catalog/opra/OPRA.PILLAR ; https://pola.rs/posts/case-bmll/ ; https://questdb.io/customers/ ; https://www.man.com/news-centre/man-group-brings-powerful-dataframe-database-product-arcticdb-to-market-with-bloomberg ; https://databento.com/blog/databento-10m-additional-funding-2024 ; https://news.fow.com/articles/3587548/ihs-markit-links-with-hedge-fund-groups-to-drive-due-diligence-standard ; https://news.ycombinator.com/item?id=41075956 ; https://news.ycombinator.com/item?id=39073149 ; https://news.ycombinator.com/item?id=49983173 ; https://github.com/ClickHouse/ClickHouse/issues/109034 ; https://github.com/pola-rs/polars/issues/24091 ; https://github.com/pandas-dev/pandas/pull/66121
