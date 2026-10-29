# Quant/trading buyers and tick-data sizes (desk research, accessed 2026-10-29)

Research note: the session-wide WebSearch budget ran out partway through. Several sub-questions (Deephaven, Beacon, Pyth, STAC, crypto market makers, Tardis/Binance totals, Tecton scale, conference channels) could not be searched and are listed under Gaps. Every URL below was accessed 2026-10-29.

## Q1. Data sizes in tick-data analytics

### Takeaway
One day of US equities TAQ is about 2.0B rows and about 52 GB gzip (quotes dominate: 1.9B rows, 38 GB). US options (OPRA) is one to two orders of magnitude larger: SIAC projects about 16B messages/day for mid-2026, and Databento says to expect "several terabytes per day" at full granularity. So "one day of one asset class" is roughly 10⁹–10¹⁰ rows, which is the size of the as-of join problem at a single-node or GPU scale. A multi-year backtest is 10¹²+ rows.

### Cited Findings
**NYSE Daily TAQ (official spec v4.0, "typical", gzip-compressed, pipe-delimited)** — [NYSE Daily TAQ Client Spec v4.0](https://www.nyse.com/publicdocs/nyse/data/Daily_TAQ_Client_Spec_v4.0.pdf)
- Trades: 2.4 GB compressed, 69 MM records/day.
- Quotes (26 files, split alphabetically by symbol): 38 GB compressed, 1.9 BB records/day.
- NBBO: 11 GB compressed, 330 MM records/day.
- Master 675 KB (11,700 symbols). Trade/Quote LULD 88/110 MB (8 MM each). CTA/UTP Admin 95/55 MB.
- The files are published after the close (trades 8:30 PM ET, quotes 9:00 PM ET). History starts 1/1/1993 (NBBO from 9/10/2003). The sizes are "estimates based on recent trading volumes."
- Earlier spec versions (v3.x) listed quotes at 17 GB / ~682M records and trades at 649 MB. Volume has roughly tripled since then — [Daily TAQ Client Spec v3.3](https://www.nyse.com/publicdocs/nyse/data/Daily_TAQ_Client_Spec_v3.3.pdf) (search-snippet figures; I did not open the PDF itself).

**OPRA (US equity options)**
- SIAC capacity notice (March 2026) projects total messages/day of about 16.1B for July 2026, 17.0B for January 2027 and 17.9B for July 2027 — [OPRA Capacity notice](https://cdn.opraplan.com/documents/notices/OPRA_Capacity_Test%20Reminder_041126.pdf) (search summary; PDF not opened directly).
- Databento: after the February 2024 migration to 96 lines, OPRA capacity went from 400B to 1T transactions/day, and peak microbursts exceed 40M messages/sec — [Databento OPRA glossary](https://databento.com/microstructure/opra); [Databento "Beyond 40 Gbps"](https://databento.com/blog/beyond-40-gbps-processing-opra-in-real-time).
- Databento also cites ">200 billion regional quotes and NBBO updates per day across 96 inbound channels" — [Databento Quant Night London](https://databento.com/blog/building-at-scale-opra-hft-and-infrastructure-insights-from-quant-night-london). This **conflicts** with SIAC's ~16B/day. The likely cause is different counting units (capacity, or per-venue quote updates vs. published messages), but I have not verified that.
- Databento OPRA.PILLAR catalog: "anticipate several terabytes of data per day" for the full dataset at CMBP-1. It lists 1.6M+ symbols and 12 schemas. Pricing: usage-based, or Standard $199/mo, Plus $1,500/mo, Unlimited $4,000/mo (annual) — [Databento OPRA.PILLAR](https://databento.com/catalog/opra/OPRA.PILLAR).
- Polygon/Massive options flat files: the daily quotes file is "~120 GB compressed" (third-party tool page, not official) — [polygon-options-puller on PyPI](https://pypi.org/project/polygon-options-puller/). Flat files come with paid plans at no extra charge via S3 — same source.

**CME futures**
- Databento GLBX.MDP3 is built from the MBOFD feed (every order event, L3). Historical data costs from $0.50/GB — [Databento GLBX.MDP3 docs](https://databento.com/docs/datasets/glbx-mdp3).
- Forum/third-party estimate: ES MBP-10 is ~25 GB/month and ES trades ~2 GB/month — [NexusFi Databento article](https://nexusfi.com/a/data/databento-futures-market-data) (anecdotal).
- Example user: 10 years (2016–now) of ES tick data is ~2.2 billion rows. Moving from CSV to Parquet with Polars gave a reported 22× disk reduction — [QuantNet thread](https://quantnet.com/threads/handling-2-2b-rows-of-es-futures-data-transitioning-from-csv-to-apache-parquet-for-backtesting.64264/latest) (seen via search snippet; the page returned 403, so I could not confirm the handle or date).

**Crypto**
- Tardis.dev covers 40,000+ instruments, with incremental L2 updates and snapshots as CSV — [Tardis FAQ](https://docs.tardis.dev/faq/general); [Tardis CSV docs](https://docs.tardis.dev/downloadable-csv-files).
- One day of Binance futures BTCUSDT incremental book is ~238 MB compressed (hftbacktest tutorial sample) — [hftbacktest Data Preparation](https://hftbacktest.readthedocs.io/en/py-v2.1.0/tutorials/Data%20Preparation.html).
- The "2000+ TB total" figure for Tardis comes from an unofficial listing page and is unverified — [web-5gv tardis listing](https://web-5gv.pages.dev/resources/tardis-dev).

**Other single-node scale datapoints (vendor case study, 2026-08-25)** — [Polars × BMLL case study](https://pola.rs/posts/case-bmll/)
- "Whole-market tick data: tens to hundreds of GB per trading day."
- Shanghai SE, one day (2025-02-17): 54.9M trades (~17 GB) plus 136.4M L2 rows (~77 GB). A Polars as-of join of trades to book took 15.8 s on a 192-core / 1.5 TB RAM machine.
- 16 days of SSE: ~268 GB trades plus 1.2 TB L2 (>1.5 TB).
- Loading one day of US equity trades took 4.3 s in Polars vs. ~3.5 min in pandas.
- BMLL estimates dedicated data infrastructure at $300–600k/yr and cloud batch compute at $200–400k/yr for a $100B AUM manager (illustrative).

**Energy/commodity trader ingest** — Energetech ingests >10M msgs/min at peak, ~140 GB/day, into QuestDB — [casestudies.com Energetech/QuestDB](https://www.casestudies.com/company/questdb/case-study/energetech-and-questdb-partner-for-efficient-market-data-management).

**Feature-store point-in-time joins**
- Chronon (Airbnb) describes its backfills as "scalable for large time windows, resilient to highly skewed data, point-in-time accurate," and runs them on Spark. The README gives no row counts — [airbnb/chronon GitHub](https://github.com/airbnb/chronon); [Airbnb Chronon blog](https://medium.com/airbnb-engineering/chronon-a-declarative-feature-engineering-framework-b7b8ce796e04).
- Feast: per a 2026-08 third-party guide, slowness in `get_historical_features` comes from the offline engine's PIT join or from materializing into pandas. The entity dataframe sets both the join cardinality and the time range scanned — [oneuptime Feast guide](https://oneuptime.com/blog/post/2026-08-15-feast-large-historical-feature-query/view). Feast also added entity-less retrieval for Spark/Ray — [Feast blog](https://feast.dev/blog/entity-less-historical-features-retrieval).

### Inferences
- (estimate) Raw, uncompressed TAQ quotes at ~1.9B rows × ~60–100 B/row in columnar form come to ~110–190 GB/day. One trading day of TAQ fits on a single 80–192 GB GPU only after column pruning or symbol partitioning. A year (~252 days) is ~500B quote rows and ~10 TB gzip.
- (estimate) OPRA at ~16B msgs/day × ~40–64 B is ~0.6–1 TB/day of binary top-of-book. Databento's "several TB/day" probably includes per-venue CMBP plus overhead. Either way, OPRA is the only US dataset where one day clearly exceeds single-GPU memory.
- (estimate) Crypto per-instrument data is small (~100s of MB/day for a top perp). The pain is breadth across thousands of instruments and many venues, not single-day depth.
- Feature-store PIT joins are usually ML-entity scale (10⁸–10¹⁰ label rows), not tick scale, and I found no published row counts.

### Gaps
- No official Polygon/Massive equities flat-file sizes, and no official CME MBO bytes/day. Databento's `metadata.get_cost` would give an exact figure.
- No official Tardis total TB or per-exchange totals, and no Binance-wide L2/L3 daily size.
- No Tecton/Chronon/Feast published training-set row counts.
- I could not reconcile Databento's 200B/day OPRA figure with SIAC's ~16B/day.

## Q2. How hard quant/trading buyers are to reach, and how a two-person team could land a design partner

### Takeaway
The evidence is thin and partly anecdotal. Top quant firms lean toward building in-house and are wedded to kdb+ or homegrown stacks. The infra vendors that did win trading customers were either spun out of a fund (ArcticDB/Man Group, Databento's ex-HFT founder) or open-source-first, with logos accumulating over years (QuestDB). The most reachable buyers look like small or mid-size prop and crypto market makers, plus data-platform vendors (BMLL, Databento-type) whose own product is processing tick data.

### Cited Findings
- **ArcticDB** was built inside Man Group over about a decade, open-sourced, and commercialized in 2023. Bloomberg signed a multi-year open-source development and BQuant integration agreement (Feb 2023) and is the only named outside customer I found. The model is an open-source core plus a paid enterprise tier — [Man Group press release](https://www.man.com/news-centre/man-group-brings-powerful-dataframe-database-product-arcticdb-to-market-with-bloomberg); [Bloomberg press](https://www.bloomberg.com/company/press/man-group-brings-powerful-dataframe-database-product-arcticdb-to-market-with-bloomberg); [The TRADE](https://www.thetradenews.com/man-group-and-bloomberg-to-bring-data-and-front-office-research-solution-to-market/).
- **QuestDB** customers include Anti Capital, a prop/HFT market maker (press release, Sept 2025): full order book data and real-time fill reconciliation — [HPCwire/BigDATAwire](https://www.hpcwire.com/bigdatawire/this-just-in/questdb-selected-by-anti-capital-to-streamline-real-time-market-data-analysis/). OKX ingests HFT market data and fills. Logos on the customer page include Brevan Howard, Hidden Road, Marex and One Trading. QuestDB claims use at "tier-1 banks, exchanges, market makers, hedge funds" — [QuestDB customers](https://questdb.io/customers/). Takeaway: crypto venues and crypto/mid-tier MMs are among the named logos.
- **Databento** founder Christina Qi previously co-founded the HFT fund Domeyard (closed after 2020) and started Databento out of data pain she experienced there — [MIT Sloan](https://mitsloan.mit.edu/ideas-made-to-matter/hedge-fund-success-then-a-pivot-purpose); [Entrepreneur](https://www.entrepreneur.com/building-a-business/she-grew-up-on-food-stamps-how-she-built-a-million-dollar-company). Its 2024 round included Belvedere Strategic Capital, a trading-firm-affiliated investor — [Databento blog](https://databento.com/blog/databento-10m-additional-funding-2024). In July 2026 it raised a $97M Series B (NEA). Qi says 90% of revenue comes from large financial institutions and AI labs — [SiliconANGLE](https://siliconangle.com/2026/07/09/market-data-platform-startup-databento-closes-97m-round-drawing-300m-investor-demand/); [AI Weekly](https://aiweekly.co/alerts/databento-raises-97m-to-chip-away-at-bloombergs-terminal-moat). Founding year conflicts: 2019 per the [M&P podcast](https://newsroom.siliconslopes.com/podcast/databento-affordable-raw-market-data/amp), 2020 per Entrepreneur. Pattern: a founder with trading-firm credibility, plus a trading-firm-linked investor.
- **Build vs. buy:** A quant fund CEO (Aston Capital, Isaac Lieberman) argued that firms should build ML processes in-house rather than buy vendor software — [Risk.net](https://www.risk.net/risk-management/5704661/quant-fund-manager-spurns-vendors-machine-learning-software) (paywalled; summary only). A TimeStored blog post (run by a kdb vendor) says major kdb+ users "had to invest heavily to build their own platform," and argues Python plus DuckDB/Polars is displacing kdb+ for local analysis — [TimeStored "The Future of kdb+?"](https://timestored.com/b/the-future-of-kdb) (seen via search snippet; the page fetch timed out). Exegy, a vendor with an interest in the answer, claims one in-house feed handler plus infrastructure can cost up to $1.8M — [Crowdfund Insider on Exegy research](https://www.crowdfundinsider.com/2025/09/252885-exegy-research-quantifies-tech-costs-compares-in-house-vendor-solutions/).
- **HN practitioner signal on kdb alternatives:** "nhourcard" (2024-08-03) lists ClickHouse/QuestDB (ASOF JOIN) for tick storage and DuckDB/Polars for local analysis — [HN 41145708](https://news.ycombinator.com/item?id=41145708).
- **Procurement/security:** Alternative-asset managers use the AITEC-AIMA vendor cyber due-diligence questionnaire (14 sections: access control, network security, incident response, software practices), hosted on IHS Markit KY3P. AIMA ODD guidelines extend to "vendor technology." SOC 2 reports are a standard ask — [IHS Markit/FOW](https://news.fow.com/articles/3587548/ihs-markit-links-with-hedge-fund-groups-to-drive-due-diligence-standard); [Alpha Week on SOC reports](https://alpha-week.com/why-hedge-fund-technology-vendors-need-soc-report); [DataArt joins AITEC standard](https://www.dataart.com/company/newsroom/press-release/dataart-joins-aitec-security-due-diligence-standard).
- **Data vendors as buyers:** BMLL, which sells L1–L3 history across 100+ venues, publicly documents its as-of join workloads and its single-node performance needs — [Polars × BMLL](https://pola.rs/posts/case-bmll/).

### Inferences
- (estimate) A two-person startup's most realistic first design partners:
  - (i) A crypto market maker or prop shop with 5–50 people. QuestDB's logo list shows these firms do adopt outside infrastructure.
  - (ii) Tick-data vendors or platforms (BMLL-, Databento- or Tardis-type) whose product is the workload.
  - (iii) Academic or market-microstructure groups that use TAQ.
- (estimate) Large multi-strats and HFTs are unlikely first customers, because of build-in-house culture and the AITEC/SOC 2 procurement burden. The pattern that worked (ArcticDB, Databento) needed insider credibility or a fund sponsor.
- (estimate) A product that installs on-prem or as a library, with no data egress, removes most of the procurement questions. SOC 2 matters less when nothing leaves the client's environment, but the AITEC questionnaire still applies. Open source plus a benchmark on public data (TAQ, Tardis samples) is the cheapest credibility path, following the QuestDB and ArcticDB model.

### Gaps
- Not researched (search budget exhausted):
  - Deephaven (Walleye origin), Beacon Platform, Pyth and Thalesians as case studies.
  - STAC Summit / STAC-M3 as a channel, KX meetups, QuantMinds.
  - r/algotrading, r/quant, Elite Trader and QuantConnect channel evidence.
  - Polars/DuckDB Discord activity.
  - Specific crypto-MM hiring or open-source signals.
- No survey data on how often quant firms buy from startups.

## Q3. First-person public posts about slow as-of / PIT joins and backtests over tick data (2023–2026)

### Takeaway
Public complaints exist but are scattered, and most come from ML/feature-engineering people (Spark is slow for left as-of joins) more than from quants. Quants mostly discuss the I/O and storage move to Parquet/Polars/DuckDB. I found no Reddit thread specifically on `merge_asof` or `join_asof` being too slow at billions of rows.

### Cited Findings (HN items found via the Algolia API, comments since 2023)
- **jamesblonde** (Hopsworks), 2024-07-26: uses ASOF LEFT JOIN to build ML training data and says Spark is "hideously slow" for left as-of joins — [HN 41075956](https://news.ycombinator.com/item?id=41075956).
- **jamesblonde**, 2024-01-20: uses DuckDB for left as-of joins because most vendors' Spark implementations are slow — [HN 39073149](https://news.ycombinator.com/item?id=39073149).
- **faizshah**, 2024-12-02: as-of style queries take minutes on row stores vs. seconds on column stores — [HN 42300716](https://news.ycombinator.com/item?id=42300716).
- **mousematrix**, 2025-03-28: out-of-core as-of join on Trino/DuckDB — [HN 43500570](https://news.ycombinator.com/item?id=43500570).
- **geysersam**, 2025-09-03: DuckDB's Python relational API lacks as-of joins, and unions in loops may go O(N²) — [HN 49548554](https://news.ycombinator.com/item?id=49548554).
- **ryzhyk** (Feldera), 2024-10-25: streaming joins can need unbounded buffers, and a specialized as-of join avoids this — [HN 41950005](https://news.ycombinator.com/item?id=41950005).
- **chrisaycock** (author of pandas `merge_asof`), 2026-03-01/02: `merge_asof` was inspired by kdb `aj`. A searchsorted approach underperformed; categorizing the keys first worked far better — [HN 47211443](https://news.ycombinator.com/item?id=47211443); [HN 47212583](https://news.ycombinator.com/item?id=47212583).
- **raoulj / orlp**, 2026-10-06: the Polars SQL interface lacks asof_join, and a Polars maintainer asks for an issue — [HN 49983173](https://news.ycombinator.com/item?id=49983173); [HN 49983322](https://news.ycombinator.com/item?id=49983322).
- **zX41ZdbW** (ClickHouse), 2024-08-03: ClickHouse added ASOF JOIN in 2019 — [HN 41144025](https://news.ycombinator.com/item?id=41144025).

### Cited Findings (GitHub, via issue search API)
- pandas PR "PERF: merge_asof with by" by jbrockmendel, 2026-07-02. Grouped `merge_asof` performance is still being worked on — [pandas #66121](https://github.com/pandas-dev/pandas/pull/66121).
- Polars feature request "join_rolling – Ergonomic & Performant Time-Series Windowed Join" by Dung2005qk, 2025-08-17 — [polars #24091](https://github.com/pola-rs/polars/issues/24091).
- ClickHouse "Add a TOLERANCE clause to ASOF JOIN" by ayakovlev-clickhouse, 2026-07-01 — [ClickHouse #109034](https://github.com/ClickHouse/ClickHouse/issues/109034).

### Cited Findings (vendor/engine posts)
- Daft blog "Scaling As-Of Joins" (May 2026): sort plus two-pointer cost O(M log M + N log N), and the bottleneck was multi-key string comparison during sorting. Hash bucketing, binary search and parallelism made it 6× faster with half the memory vs. pandas — [Daft blog](https://www.daft.ai/blog/scaling-asof-joins) (search-snippet figures; the page fetch timed out).
- ES futures, 2.2B tick rows, CSV moved to Parquet via Polars with 22× less disk — [QuantNet](https://quantnet.com/threads/handling-2-2b-rows-of-es-futures-data-transitioning-from-csv-to-apache-parquet-for-backtesting.64264/latest). Also mirrored from Reddit — [sentinel-team snapshot](https://reddit.sentinel-team.org/posts/1t9x8ea/snapshots/2026-05-16T06%3A14%3A02.121185Z). Handle and date not confirmed.

### Inferences
- The loudest public demand for faster as-of/PIT joins comes from ML feature-engineering at Spark scale (Hopsworks, Feast users), not from quants. Quants who post publicly are mostly retail or small-shop users at 10⁸–10⁹ rows, where Polars/DuckDB already cope (BMLL: 54.9M-row as-of join in 15.8 s).
- (estimate) The performance gap that matters is at 10¹⁰+ rows: multi-day OPRA, a year of TAQ, or parameter sweeps that repeat the join many times. Those workloads belong to firms that don't post publicly.

### Gaps
- The Reddit search APIs failed:
  - arctic-shift returned errors or empty results.
  - hn.algolia via curl was blocked; WebFetch worked.
- So there are no verified r/algotrading or r/quant first-person posts with handles and dates.
- No first-person posts quantifying backtest sweep times over tick data.
