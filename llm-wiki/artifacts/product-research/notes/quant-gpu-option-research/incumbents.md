# Incumbents for tick-data analytics (as-of joins, tick windows, backtests): what is used, what it costs, what people complain about

All URLs accessed 2026-10-29 unless noted. "(estimate)" marks my own inference. Vendor benchmarks are vendor-reported and unverified.

Method note: the session's WebSearch budget ran out partway through. DolphinDB pricing, QuestDB, ArcticDB, Timescale, NautilusTrader internals and GPU-backtester repos were therefore covered thinly or not at all (see Gaps). HN data comes from the hn.algolia.com API and Reddit data from the arctic-shift API.

## 1. kdb+/q and KDB-X: licence cost, free tier, complaints

### Takeaway
kdb+ list prices are not public. KX licenses per core (with an 8-core minimum reported in 2011) and also sells per-core-minute on-demand. Community figures are roughly $100k/year (2019) and "$500k on licences plus people" (2023). Since GA on 2025-11-17, KDB-X Community Edition is free for commercial use but capped at 16 GB RAM, 24 cores, 4 secondary threads per process and one instance, and KX explicitly forbids using GPU workflows to get around the 16 GB cap. The 2026 complaints are about talent and hiring rather than licence cost: teams at a large London hedge fund are porting q to Python with LLMs because q is hard to hire for, and LLMs are bad at q.

### Cited Findings
- **Pricing model:**
  - kdb+ on-demand is billed per core-minute, "instead of, or in combination with, the standard per core or enterprise models" (2018 announcement) — [Finextra](https://finextra.com/pressarticle/73245/kx-systems-releases-saas-version-of-kdb); [A-Team Insight](https://a-teaminsight.com/kx-adds-software-as-a-service-version-of-kdb-time-series-database-2)
  - Search summary of KX Software Usage Terms (Oct 2024): excess cores are charged at Order Form rates. No public per-core list price was found. — [kx.com](https://kx.com/?p=393917)
- **Reported figures (HN), oldest first:**
  - 2011-02-04: "check how much a license for kdb+ costs per core. Oh, and 'a standard purchase requires licensing at least 8 cores.'" — [HN 2180016](https://news.ycombinator.com/item?id=2180016)
  - 2018-04-08: "$10k isn't a lot (assuming that's right…)", comparing it with Oracle at about $50k/core and MS-SQL at about $7k/core. This is an unconfirmed per-core figure. — [HN 16786513](https://news.ycombinator.com/item?id=16786513)
  - 2019-05-21: "the license costs around $100,000/year (probably the most expensive software per kilobyte). Fintech can afford it, other industries can't." — [HN 19974192](https://news.ycombinator.com/item?id=19974192)
  - 2023-10-11: "Kdb with ease, but down 500k on licenses and the cost of people who can use it." — [HN 37850460](https://news.ycombinator.com/item?id=37850460)
  - 2024-01-08 (counterpoint): "Firms pay 10s of times more for things like DataDog than they do for a KDB+ license… the argument that KDB+ devs are expensive [is] laughable once I saw how much… we started to pay AWS/Python devs." — [HN 38916639](https://news.ycombinator.com/item?id=38916639)
  - 2026-02-18: the builder of a Rust streaming SQL DB says the options for real-time aggregation in finance were "kdb+ (expensive licensing) or Flink (JVM cluster)". — [HN 47057783](https://news.ycombinator.com/item?id=47057783)
- **Older free tiers:**
  - 2016: only the 32-bit version was free (4 GB limit). — [HN 11562579](https://news.ycombinator.com/item?id=11562579)
  - 2018: the 64-bit on-demand Personal Edition was free for non-commercial use, on up to 2 computers with 16 cores each. It could not run on cloud and needed an always-on internet connection. — [HN 18689693](https://news.ycombinator.com/item?id=18689693)
- **KDB-X Community Edition limits (official):**
  - Free "for personal and commercial projects".
  - Max 16 GB RAM for q, up to 24 cores across all instances, up to 4 secondary threads and 16 connections per q process, and a single physical or virtual instance.
  - Registering multiple licences to get around the caps is prohibited.
  - Installation on "commercially supported servers" is "strictly prohibited".
  - Using GPU workflows to bypass the 16 GB RAM limit is not permitted, including partitioning, staging or tranche-based loading.
  - Source: [code.kx.com usage restrictions](https://code.kx.com/licensing/usage-restrictions.html)
  - Another KX page reportedly says 8 connections instead of 16 (seen in a search summary only, not fetched). — [docs.kx.com usage restrictions](https://docs.kx.com/product/licensing/usage-restrictions.htm)
- **KDB-X timeline:** public preview in June 2025 with a `kc.lic` licence enforcing these limits, and GA on 2025-11-17. This comes from a search summary of the release notes. — [KDB-X release notes](https://code.kx.com/kdb-x/releases/release-notes-latest.html)
- **KX's own benchmark and the free tier:** KX's TAQ benchmark README says to use the `tiny` size (about 1 GB, 9.4M quotes) "when running the benchmark with KDB-X Community Edition, which enforces a memory limit". The full day of 2026-04-01 is 296 GB in memory and 2.86B quotes. — [KxSystems/NYSETAQBenchmarks README](https://github.com/KxSystems/NYSETAQBenchmarks)
- **Corporate:**
  - TA Associates acquired KX in July 2025.
  - KX merged with OneMarketData (OneTick) on 2025-09-15. The combined company is led by KX CEO Ashok Reddy, and deal terms were not disclosed.
  - Sources: [TA press release](https://ta.com/news/kx-and-onetick-merge-to-unite-capital-markets-data-analytics-ai-and-surveillance-on-one-platform); [BusinessWire](https://www.businesswire.com/news/home/20250915723209/en)
- **2026 complaints (Reddit r/quant):**
  - Thread "Some research teams are using Claude to move q/kdb+ code over to Python" (2026-06-27, 53 points, 34 comments) — [r/quant 1ugzkx7](https://old.reddit.com/r/quant/comments/1ugzkx7/some_research_teams_are_using_claude_to_move_qkdb/)
    - The OP is a QR at "a large hedge fund in London". They say teams are re-coding in Python because of "easier to hire people and AI is pretty awful at q", and that the reliability of the re-coded systems has not got better.
    - A comment says "most of the KDB bills are from the premium the few skilled QDs that exist demand". It adds that GPU and datacenter bills "can go up more than KDB bills". — [comment ou8t4nc](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ou8t4nc) (2026-06-28)
    - A comment says "kdb and kdb developers are good at entrenching themselves", which points to lock-in and people risk. — [comment ouaivvu](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ouaivvu) (2026-06-28)
    - Pro-kdb comments say that on "native compression and IPC, kdb is still better than Python on the backend by a long shot" ([ou5y22n](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ou5y22n)). Another says a linear regression over 1M rows is "4x slower in python than kdb" and that moving the same compute to Python "will become even more expensive" ([ou5wyfe](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ou5wyfe)). Both are dated 2026-06-27.
    - "Polars asof join handles the common cases well. Where it gets tricky is multi-asset alignment with different bar schedules and missing data windows. kdb handles that out of the box but in Python you usually end up building custom logic around the join." — [ovdmhot](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ovdmhot) (2026-07-03)
    - A comment describes q's syntax hazard: a lone `/` silently comments out the rest of a file, and "q loaded it without a murmur", producing wrong numbers in a TCA port. — [p3jry11](https://old.reddit.com/r/quant/comments/1ugzkx7/_/p3jry11) (2026-08-13)
  - "A Zero Copy ORM for KDB – Read & Write without a KDB licence" (2026-07-02) shows demand for getting at kdb data without a licence. — [r/quant 1ulwbwb](https://old.reddit.com/r/quant/comments/1ulwbwb/a_zero_copy_orm_for_kdb_read_write_without_a_kdb/)
  - In a 2026-07-29 thread on how quant firms serve research data, kdb is listed as one of the off-the-shelf column stores for pushing queries close to data. — [r/quant 1va7kq3](https://old.reddit.com/r/quant/comments/1va7kq3/_/p0kd84s)

### Inferences
- The free KDB-X tier is generous on cores (24) but not on memory (16 GB), which is about 5% of one full TAQ day. Anyone working with real tick volumes needs a commercial licence. A Parquet-native engine with no RAM cap competes directly with that gap. (estimate)
- The 2026 "migrate q to Python" trend creates a pull for Python/SQL front-ends that keep kdb-like primitives (aj, wj, xbar windows) at kdb-like speed. (estimate)
- With KX and OneTick merged, the two best-known commercial tick platforms are now one vendor. That could push some buyers to look for alternatives. (estimate)

### Gaps
- I found no authoritative 2025–2026 per-core list price. The figures above are anecdotal HN comments ($10k/core in 2018 is unconfirmed, and $100k/yr in 2019 has an unknown core count).
- KDB-X commercial pricing, and whether the GPU module costs extra, is not on any page I fetched.

## 2. KX's own GPU work: KDB-X GPU acceleration and the cuDF TAQ issue

### Takeaway
KX shipped a GPU module for KDB-X (blog dated 2026-04-02, "production-ready"). It covers as-of joins (`.gpu.aj`), sorts, binary search, a restricted qSQL subset and multi-GPU `peach`, with vendor-reported speedups of 4–5.5x on aj and 5–10x on sorts. KX's pages do not mention cuDF. Separately, NVIDIA opened an epic on 2026-10-15 to run KX's own NYSE TAQ benchmark fully on GPU in cudf-polars. As of that date, libcudf still has no merged as-of join primitive: the backward `cudf::asof_join` PR is still open.

### Cited Findings
- **The KX blog post** ("Introducing KDB-X GPU Acceleration", Nicholas Sansone, 2026-04-02) — [KX blog](https://kx.com/blog/introducing-kdb-x-gpu-acceleration/)
  - It calls the module a "new offering in the KDB-X family" and a "first-class, production-ready capability", loaded with `.gpu: use`kx.gpu`.
  - **Operations:**
    - `.gpu.aj` runs a parallel binary search across symbols.
    - Sorts: `.gpu.xasc`, `.gpu.iasc`, `.gpu.asc`, plus an on-disk sort pattern for splayed tables.
    - Other primitives: `.gpu.select`, `.gpu.bin`, `.gpu.append`.
    - Data movement: `.gpu.to`, `.gpu.xto`, `.gpu.from`.
    - Device management: `.gpu.ndev`, `.gpu.sdev`, `.gpu.mdev`.
    - Multi-GPU work runs through `peach`.
  - **Benchmarks (vendor):**
    - aj on an L4 vs a 48-core CPU at 1M rows: about 4x.
    - aj on an A100 at 10M rows: about 5.5x.
    - aj at 100M rows: GPU sub-second vs CPU over 4 s.
    - Context for the join figures: 19M trades joined to 194M quotes.
    - Sort of 10M ticks by `sym,time` on an A100: about 42 ms vs about 320 ms.
    - Sorts in general, from tens of millions of rows up: 5–10x.
    - Multi-GPU VaR on 5×H100 vs a 48-core CPU: about 0.27 s vs about 15.8 s (about 58x).
  - **Caveats it states:**
    - Payload columns stay on the CPU until results are materialized.
    - The join is weaker when only one table's keys are GPU-resident.
    - `.gpu.from` keeps only the sorted attribute.
    - For large on-disk sorts, disk I/O is the bottleneck, so batching is needed.
  - **What it leaves out:** no pricing or edition, and no mention of cuDF, RAPIDS or libcudf. The only NVIDIA reference is a nav link to a "KX-NVIDIA AIQ blueprint".
- **The KX docs page** — [code.kx.com GPU intro](https://code.kx.com/kdb-x/modules/gpu/introduction.html)
  - Supported operations: "a restricted set of qSQL queries", sorting of in-memory tables, and as-of joins with attributes.
  - `.gpu.bin` and `.gpu.aj` need the grouped attribute `` `g# ``.
  - Tables can have mixed residency, with some columns in VRAM (shown in q as `foreign`) and some on the host.
  - It does not mention cuDF. Access is by evaluation request (email devrel@kx.com).
- **The licence ties GPU use to the free-tier RAM cap:** "Using GPU workflows to bypass the 16GB RAM limit is not permitted." — [usage restrictions](https://code.kx.com/licensing/usage-restrictions.html)
- **NVIDIA/cudf issue #24634**, "Support KX NYSE TAQ benchmark in cudf-polars fully on GPU" — [GitHub](https://github.com/NVIDIA/cudf/issues/24634)
  - Opened 2026-10-15 by Peter Entschev (NVIDIA). It was OPEN with 0 comments on 2026-10-29.
  - **Goal:** every query in the KX NYSE TAQ benchmark runs fully on GPU and matches CPU Polars, both in memory and via lazy `pl.scan_parquet` over Parquet physically sorted by `time`. It should work on one or many GPUs and be "competitive with CPU Polars".
  - **Feature map:**
    - `join_asof`: by `sym` for q79/q80/q84, by `[sym, ex]` for q83, single-symbol for q81/q82, and time buckets for q62/q63. All are backward joins with exact matches allowed and no tolerance.
    - `group_by_dynamic` with 10m, 5m and 1m windows, including OHLC (q22–q33, q64–q67).
    - Fixed `rolling_mean(20)` with and without `.over("sym")` (q47–q50).
    - `cum_sum` and `shift` run-id grouping (q51–q55, q71/q72).
    - Grouped `corr` (q68/q69).
  - **Open items:**
    - The libcudf backward as-of primitive (#24180), its pylibcudf binding, and the cudf-polars translation.
    - Multi-GPU as-of join, by broadcasting the small right side or hash-shuffling on `by` keys, plus an order-aware path that carries the last right row per group across partitions.
    - Streaming ungrouped `cum_sum` and `shift`, and large-group `over()` rolling.
    - Reading categorical columns from Parquet. The benchmark currently reads `sym` as a string as a workaround.
    - Categorical filters, grouping and join keys.
    - Grouped `corr`.
  - **Done:** `dt.truncate`, streaming first/last, single- and multi-GPU streaming rolling, inferring scan ordering from Parquet footer statistics, and `set_sorted` metadata in streaming.
- **NVIDIA/cudf PR #24180**, "Add As-of join libcudf primitive with backward strategy" — [GitHub](https://github.com/NVIDIA/cudf/issues/24180)
  - Opened 2026-09-15 and still open.
  - It adds `cudf::asof_join` with a build-once/probe-many design.
  - It requires the right side sorted ascending by `(by…, on)` with contiguous groups and nulls first.
  - It supports only backward with `allow_exact_matches=true`.
- **The KX benchmark repo**, [KxSystems/NYSETAQBenchmarks](https://github.com/KxSystems/NYSETAQBenchmarks)
  - Created 2026-06-16, last push 2026-10-26, 14 stars.
  - It compares in-memory engines: KDB-X, KDB-X Python, Polars, Pandas, DuckDB and chDB, using Parquet for the non-kdb engines.
  - It uses public NYSE TAQ samples. Sizes run from tiny (1 GB, 9.4M quotes) to full (296 GB in memory, 187 GB on disk, 26,396 symbols, 2.86B quotes; date 2026-04-01).
  - KX names a public results site, benchmark.kx.com, which I did not fetch.

### Inferences
- In October 2026 there is no GPU engine that natively runs a backward as-of join from Parquet in the open-source RAPIDS stack. The libcudf primitive is unmerged, and multi-GPU and streaming as-of are only planned. That window is likely to close within one or two RAPIDS releases (estimate: by mid-2027), so a product would have to win on breadth (window joins, tick windows, backtest sweeps) and usability rather than as-of join alone. (estimate)
- KX's GPU module is tied to q and KDB-X licensing and is vendor-benchmarked only. Its 4–5.5x aj gain against 48 CPU cores is modest, which means a GPU aj alone is not a dramatic differentiator. (estimate)
- The KX TAQ query set (q1–q84) is becoming a de facto public benchmark that both KX and NVIDIA use. A new engine could run it to get comparable numbers. (estimate)

### Gaps
- I did not fetch benchmark.kx.com results, so there are no Polars, DuckDB or chDB vs KDB-X numbers here.
- I found no source on KDB-X GPU pricing or which editions include it, and no evidence for or against KX using cuDF internally.
- I did not fetch an NVIDIA–KX partnership page.

## 3. Other tick engines: OneTick, DolphinDB, ClickHouse, DuckDB, Polars, others

### Takeaway
OneTick is now part of KX. Its pricing is not public, and it reports 160+ clients. DolphinDB supports asof and window joins and has a free community version, but no public enterprise price list was found and I saw no GPU evidence. ClickHouse, DuckDB and Polars all have asof joins with restrictions: one inequality condition, sorted inputs, and in DuckDB's `USING` form only `>=`. None of them is GPU-native.

### Cited Findings
- **OneTick**
  - Owned by OneMarketData, which merged with KX on 2025-09-15. — [TA](https://ta.com/news/kx-and-onetick-merge-to-unite-capital-markets-data-analytics-ai-and-surveillance-on-one-platform)
  - A search summary reports 160+ clients across hedge funds, banks, asset managers, exchanges and regulators. It also describes OneTick Cloud with 200+ exchanges and up to 30 years of history. — [onetick.com company overview](https://www.onetick.com/company-overview)
  - Bridgewater and Tudor are named as quant funds that loaded tick data into OneTick (search summary). — [MarketsMedia](https://articles.marketsmedia.com/tick-data-powers-financial-markets)
  - The 2015 KOR Group deployment cited an "affordable annual subscription". — [Mondo Visione](https://mondovisione.com/news/kor-group-deploys-onetickcloud-tick-data-and-analytics-platform-2015811/)
  - No public prices were found.
- **DolphinDB**
  - Commercial, with a free community version. — [DB-Engines](https://db-engines.com/en/system/Brytlyt%3BDolphinDB%3BKdb)
  - Its AWS Marketplace listing has a community licence download and enterprise trials on request. — [AWS Marketplace](https://aws.amazon.com/marketplace/pp/prodview-6j7wqm5tobvko)
  - It has asof join and window join plus streaming join engines. — [DolphinDB join engine tutorial](https://github.stncp.dns-dynamic.net/dolphindb/Tutorials_EN/blob/master/join_engine.md)
  - A vendor article reports an asof join of 339 ms and a window join of 402 ms on tick data, with unspecified size. — [dev.to/dolphindb](https://dev.to/dolphindb/helpful-tools-for-quantgun-efficiently-calculate-transaction-costs-from-tick-data-58h4)
  - I found no GPU support.
- **ClickHouse ASOF JOIN** — [ClickHouse JOIN docs](https://clickhouse.com/docs/sql-reference/statements/select/join)
  - Allows "any number of equality conditions and exactly one closest match condition" using `>`, `>=`, `<` or `<=`.
  - The asof column must be an ordered sequence of type Int, UInt, Float, Date, DateTime or Decimal.
  - With the hash algorithm it "can't be the only column in the JOIN clause".
  - Supported only by the `hash` and `full_sorting_merge` algorithms. It is not supported in the Join table engine, partial_merge or block nested loop.
  - NaN in the asof column is unsupported, and the docs say to filter it out.
  - There is no window or range join.
  - Its embedded form, chDB, is one of the engines in KX's TAQ benchmark. — [NYSETAQBenchmarks](https://github.com/KxSystems/NYSETAQBenchmarks)
- **DuckDB ASOF JOIN** — [DuckDB docs](https://duckdb.org/docs/2.0/guides/sql_features/asof_join.html)
  - "At most one match from the right hand side", with inner and LEFT variants.
  - With `USING`, the last column is the inequality and it must be `>=`.
  - There is no window-join construct.
- **Polars `join_asof`** — [Polars docs](https://docs.pola.rs/api/python/stable/reference/dataframe/api/polars.DataFrame.join_asof.html)
  - Strategies are backward, forward and nearest. It also takes `tolerance`, `by`/`by_left`/`by_right` and `allow_exact_matches`.
  - Both frames must be sorted by `on` within `by` groups.
  - The in-memory engine can't check sortedness when `by` is used, and the streaming engine checks only the rows it processes.
  - `nearest` does not support string keys.
- **GPU Polars:** cudf-polars does not yet run `join_asof` on GPU. It falls back to CPU, per open items in [cudf #24634](https://github.com/NVIDIA/cudf/issues/24634).

### Inferences
- Every open-source SQL or dataframe engine has a point as-of join but no kdb-style window join (`wj`) primitive. The comment about "custom logic around the join" for multi-asset alignment ([ovdmhot](https://old.reddit.com/r/quant/comments/1ugzkx7/_/ovdmhot)) points to window joins and multi-schedule alignment as the unmet need. (estimate)

### Gaps
- No DolphinDB price or GPU information (WebSearch budget exhausted).
- QuestDB (ASOF/LT/SPLICE joins, open core), ArcticDB (Man Group, BSL-licensed with a paid commercial tier) and TimescaleDB were not researched in this session. Those parenthetical descriptions are from background knowledge and are uncited (estimate). Verify them before use.

## 4. Backtest and sweep tools: vectorbt / PRO, NautilusTrader, backtesting.py, GPU backtesters

### Takeaway
vectorbt PRO is a paid private repo, with listed starting prices of $25/month, $240/year or $500 lifetime. It is Numba-plus-Rust on CPU and the homepage does not mention a GPU. Its vendor benchmark is about 310,800 sweep results in under a minute on an Apple M3. The open-source options (NautilusTrader with 31k stars and LGPL, backtesting.py with 9.2k stars and AGPL, vectorbt OSS with 9.6k stars) are CPU-based. GPU backtesting in retail is mostly home-grown CuPy or CUDA.

### Cited Findings
- **vectorbt PRO**
  - Starting prices: $25/month; yearly "starts at $240" (shown down from $300); lifetime "starts at $500".
  - Every plan includes the private repo, docs and Discord.
  - Backends are Numba plus an optional ahead-of-time compiled Rust crate.
  - Rust simulator figures on an Apple M3: about 4.7B bars/s for low-level Rust in parallel, about 580M for mid-level and about 320M for high-level.
  - A sweep pipeline ran 6,475 parameter pairs × 24 splits = 310,800 results in under a minute on an M3.
  - No GPU is mentioned.
  - Source: [vectorbt.pro](https://vectorbt.pro/)
  - The FAQ adds: half of past monthly payments count toward lifetime (minimum $150), organization access is for internal non-commercial use, and commercial use is by email. — [vectorbt.pro FAQ](https://vectorbt.pro/faq)
- **Open-source CPU frameworks (GitHub API, 2026-10-29):**

  | Repo | Stars | Licence | Last push | Source |
  |---|---|---|---|---|
  | NautilusTrader (Rust/Python, event-driven) | 31,036 | LGPL-3.0 | 2026-10-29 | [github](https://github.com/nautechsystems/nautilus_trader) |
  | backtesting.py | 9,160 | AGPL-3.0 | 2026-08-05 | [github](https://github.com/kernc/backtesting.py) |
  | vectorbt (OSS) | 9,585 | non-standard licence ("NOASSERTION") | 2026-10-28 | [github](https://github.com/polakowo/vectorbt) |

  - The NautilusTrader Show HN (2025-08-06) got 208 points. — [HN 44810552](https://news.ycombinator.com/item?id=44810552)
- **Vectorized vs event-driven:**
  - "a vectorised approach can be ~1,000x faster than a event driven approach… the downside is you can't run a vectorised application live." — [r/algotrading comment](https://old.reddit.com/r/algotrading/comments/1x8tw1t/_/phggdvp) (2026-10-18)
  - "zipline reloaded - is event based. It is very slow for optimization. vectorbt - vector optimization" — [HN 39147798](https://news.ycombinator.com/item?id=39147798) (2024-01-26)
- **GPU backtesting in the wild:**
  - "vibecoding my own engine… using my GPU (CuPy)… VectorBT has been my reference as a benchmark" — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1wqrr91/_/pc6iswb) (2026-09-26)
  - "My own GPU based tick level data broker simulation engine for creating the best overfit parameter sets ever." — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1x27xu3/_/pf0z42a) (2026-10-10)
  - A retail user processes 500–750 MB/day of tick data per symbol on a 5090. — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1xf84n8/_/19ro0av5sl20z) (2026-10-26)
  - The argument for GPU offload: "Backtesting a regular single symbol model on CPU is often not cheap… multiply that by 50 correlated instruments, each having… 1500-3000 strikes+sides+expiries" — [HN 25830484](https://news.ycombinator.com/item?id=25830484) (2021-01-19)
  - Show HN: "Qantify – GPU-Accelerated Trading Library" (2025-11-13, 1 point). — [HN 45912809](https://news.ycombinator.com/item?id=45912809)
- **Retail complaints about tools:**
  - "tried… Backtesting.py, Backtrader… QuantConnect and vectorbt, but none of them feel truly complete. They're either too simple, overly complex…" — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1m50gaf/please_i_need_help_asap/) (2025-07-20)
  - On NinjaTrader's backtest and walk-forward: "consumed a huge amount of time, often crashed and regularly felt inflexible" — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1oyg59y/i_built_the_back_testing_engine_i_wanted_do_you/) (2025-11-16)
  - A user with "3 years of Databento L2 tick data" on MNQ/NQ: "17 strategies dead", which shows retail is doing tick-level research. — [r/algotrading](https://old.reddit.com/r/algotrading/comments/1spd5nf/6_months_full_time_on_algo_17_strategies_dead_on/) (2026-04-18)

### Inferences
- Retail willingness to pay for a backtest engine sits around $240–$500 per year or lifetime, which is vectorbt PRO's price point. Institutional pricing (kdb, OneTick) is quote-only and anecdotally in the $100k+/year range. That is a large gap, and a mid-market product could sit in it. (estimate)
- GPU parameter-sweep tooling is fragmented and home-grown (CuPy scripts). No packaged, Parquet-native GPU sweep engine stood out in what I found. (estimate, limited by search budget)

### Gaps
- The GitHub repository search for "gpu backtest" failed (API error), so there is no star-ranked list of GPU backtesters.
- I found no hard numbers on retail sweep runtimes (for example "my grid search takes N hours").
- There are no sourced pandas `merge_asof` slowness complaints. The HN search for this failed and the Reddit API timed out.
