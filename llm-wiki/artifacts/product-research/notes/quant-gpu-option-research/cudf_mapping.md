# Mapping as-of joins, tick windows and parameter-sweep backtests onto cuDF/libcudf (GPU)

Code snapshot: NVIDIA/cudf `main` @ `785ee522f5688f485fc0b1eabca7f8d6d2adea0e` (committed 2026-10-29), VERSION file `26.12.00`. This is a shared blobless sparse clone at `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext/cudf`; `python/cudf/cudf/{core,pandas}`, `python/cudf_polars/cudf_polars` and `python/pylibcudf/pylibcudf` were added to its sparse checkout. All URLs were accessed 2026-10-29. Code links point at that SHA, e.g. https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/rolling.hpp

The WebSearch budget ran out partway through. Searches for RAPIDS/NVIDIA backtesting blog posts and newer STAC-A3 results were not run (see Gaps).

## 1. Does libcudf or cudf Python have an as-of join today?

### Takeaway
No released or `main` cuDF ships an as-of join as of 2026-10-29. The first libcudf primitive, `cudf::asof_join` (backward strategy only), is in open PR #24180 and received change requests on 2026-10-28. The cudf-polars `join_asof` request (#20727) was closed only because it moved to story #24110, which is still open. Fixed-size rolling windows in cudf-polars (#20307) were merged in July 2026. The KX TAQ benchmark issue (#24634) is open and in progress, with `join_asof` as its first blocker.

### Cited Findings
- At SHA 785ee522, `git grep -i asof` over `cpp/include` finds nothing, so libcudf has no public as-of API. Over `python/` it finds only:
  - two pandas-testsuite xfail entries for `DataFrame.asof` / `Series.asof`, both marked "TODO: Add a reason for failure", in `python/cudf/cudf/pandas/scripts/pandas-testing-plugin.py:1277,2912`;
  - one cudf-polars guard, `if how[0] != "IEJoin":  # pragma: no cover; asof joins not yet exposed` followed by `raise NotImplementedError(f"Unsupported join type {how}")`, at `python/cudf_polars/cudf_polars/dsl/translate.py:830`.
  - Source: [cudf @785ee522](https://github.com/NVIDIA/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e)
- `cpp/include/cudf/join/` at that SHA contains `conditional_join, direct_join, distinct_hash_join, filtered_join, hash_join, join, key_remapping, mark_join, mixed_join, sort_merge_join, streaming_hash_join`. There is no asof header. — [cudf @785ee522 cpp/include/cudf/join](https://github.com/NVIDIA/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/join)
- **cudf.pandas fallback.** cudf has no `merge_asof`, so the module accelerator wraps `pandas.merge_asof` as `_FunctionProxy(fast_attr, slow_attr)` with an `_Unusable` fast side (`python/cudf/cudf/pandas/module_accelerator.py` `_wrap_attribute`, ~L271–330 @785ee522). A call therefore runs CPU pandas, after converting any GPU-resident proxy frames to host. The conversion step is inferred from the fast/slow proxy design, not traced line by line (estimate). — [module_accelerator.py @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/python/cudf/cudf/pandas/module_accelerator.py)
- **#20727 "[FEA] Support `join_asof` in cudf-polars"**
  - Opened 2025-11-25 by taozuoqiao; closed 2026-09-10 by rjzamora "as completed" but in fact moved to #24110. No implementation PR is attached.
  - On 2026-02-25, wence- said libcudf needs new work first and suggested adapting sort-merge join matching.
  - The same day, shrshi confirmed as-of joins are on the libcudf roadmap: extend the sort-merge merge step, doing forward/backward first and nearest later.
  - wence- also noted that Polars' mix of equality (`by`) and as-of keys needs sorting within groups.
  - Source: [issue #20727](https://github.com/NVIDIA/cudf/issues/20727)
- **#24110 "[Story][FEA] Support as-of joins in libcudf, pylibcudf, cuDF Python, and cudf-polars"**
  - Opened 2026-09-10 by rjzamora. Still open, assigned to shrshi and rjzamora, project status "Todo".
  - v1 scope is the BACKWARD strategy only (last right row with `right_on <= left_on` within the group), covering ungrouped, single-key and multi-key grouped joins.
  - Ordered key types: int, Date, Datetime, Duration, Time. Group keys: strings and fixed-width types.
  - Inputs must be pre-sorted by `(by..., on)` with nulls first. The output is a right-side gather map with `cudf::JoinNoMatch` where nothing matches.
  - Follow-ups: P0 streaming/multi-GPU; P1 `allow_exact_matches=False` and forward; P2 calendar tolerances; P3 nearest and the cuDF Python `merge_asof`.
  - rjzamora noted that real time-ordered data has interleaved groups and asked for `tolerance` in v1, to keep cross-chunk boundary exchange cheap.
  - Source: [issue #24110](https://github.com/NVIDIA/cudf/issues/24110)
- **PR #24180 "Add As-of join libcudf primitive with backward strategy"**
  - By shrshi. Open and unmerged. rjzamora approved it for cudf-polars use; bdice and mhaseeb123 requested changes on 2026-10-28.
  - New API `cudf::asof_join` in `cpp/include/cudf/join/asof_join.hpp`, implemented in `cpp/src/join/asof_join.cu`.
  - Design is build-once/probe-many, like `hash_join`. The right side must be sorted by `(right_by..., right_on)` with contiguous groups.
  - Group offsets are built with a row-equality flag plus CUB `DeviceSelect::Flagged`. Each probe binary-searches the group, then the ordered key. It started with `thrust::lower_bound`; a later commit says "Use upper_bound for backward as-of join probing".
  - Reviewers asked for left/right index pairs instead of a single map, and for a strategy-templated design.
  - No benchmark numbers are posted.
  - Source: [PR #24180](https://github.com/NVIDIA/cudf/pull/24180)
- **#20307 "[FEA] Support rolling expressions with fixed-size windows in cudf-polars"**
  - Opened 2025-10-19 by adamreeve, after `rolling_sum(window_size=250)` failed with `NotImplementedError: rolling_rsum` on cudf-polars-cu12 25.10.0.
  - Closed 2026-07-20 by PR #21964 "Add fixed-size rolling window support to cudf-polars", merged by vyasr (commit b9db6f4 per the page).
  - Covered expressions: sum, mean, min, max, std, var, median, quantile, skew, kurtosis.
  - First release containing it: estimate 26.08, inferred from the July 2026 merge date.
  - Source: [issue #20307](https://github.com/NVIDIA/cudf/issues/20307)
- **#24634 "Support KX NYSE TAQ benchmark in cudf-polars fully on GPU"**
  - Opened 2026-10-15 by pentschev. Open; moved to "In Progress" on 2026-10-21.
  - Goal: every TAQ query fully on GPU, both in memory and with `scan_parquet` over time-sorted Parquet (with `set_sorted("time")`, no global sort), on one or more GPUs.
  - All TAQ as-of queries are backward, exact-match and have no tolerance.
  - Story checklist (none ticked):
    - `join_asof` (#24110 / PR #24180)
    - `group_by_dynamic` (#24734; related PRs #24522, #24701, #24653, #24591, #24892, #24511, #24900, #24702)
    - streaming ordered windows (#22032, #22033, #23593; PRs #22308, #24589, #24603)
    - order-aware streaming infrastructure (#22128)
    - Categorical `sym` (#18908, PR #24430)
    - grouped `corr` (#22908, PR #24905)
  - Multi-GPU as-of plan: broadcast a small right side, otherwise hash-shuffle on `by`.
  - The issue body has no performance numbers.
  - Source: [issue #24634](https://github.com/NVIDIA/cudf/issues/24634)

### Inferences
- An as-of join released in libcudf/pylibcudf is plausible in 26.12 or 27.02 at the earliest (estimate). It would be backward-only and require pre-sorted input. Forward, nearest and tolerance, and pandas `merge_asof` in cuDF Python, come later (P1–P3).
- Today cudf.pandas users running `merge_asof` silently take the CPU path.
- NVIDIA's own priority order, through the TAQ benchmark, matches a tick-analytics product: as-of join, time-bucket groupby, ordered streaming windows, then categorical symbols.

### Gaps
- No public benchmark for PR #24180. The commit "Explain benchmark-based as-of group selection choice" exists, but its content was not visible.
- Not confirmed which tagged release first contains PR #21964.

## 2. Building an as-of join from existing libcudf primitives

### Takeaway
Yes, this can be done today. Sort both sides by `(sym, ts)`, then call `cudf::upper_bound(haystack=right[(sym,ts)], needles=left[(sym,ts)]) − 1` on the multi-column table. That gives the backward match per symbol in one call. Then check that the symbol at the matched index equals the left symbol, and gather with NULLIFY. This is essentially the algorithm of PR #24180.

### Cited Findings
- `cudf::lower_bound` / `cudf::upper_bound` (`cpp/include/cudf/search.hpp` L62, L104 @785ee522) take a `table_view` haystack and needles plus per-column `order` and `null_order`. They search lexicographically across multiple columns; the doc example is a 3-column haystack. That is exactly the (key, ts) composite search a by-key as-of join needs. — [search.hpp @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/search.hpp)
- `cudf::sort_merge_join` is a class that preprocesses the right table once (`cpp/include/cudf/join/sort_merge_join.hpp` L40–45). Maintainers named it as the base to extend for as-of joins. — [sort_merge_join.hpp @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/join/sort_merge_join.hpp); [#20727](https://github.com/NVIDIA/cudf/issues/20727)
- `conditional_join.hpp` and `mixed_join.hpp` exist; mixed join means equality keys plus an AST inequality. cudf-polars uses an "IEJoin" path built on them (`translate.py` ~L830). Inequality joins return all matching pairs, not only the nearest, so an as-of join on top of them needs a groupby-max reduction afterwards. That costs O(matches) and can blow up (estimate). — [cudf join headers @785ee522](https://github.com/NVIDIA/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/join)
- wence- suggested reusing the rolling window-boundary utilities, i.e. the range-window machinery of section 3, for as-of matching. — [#20727](https://github.com/NVIDIA/cudf/issues/20727)

### Inferences
- **Recipe (estimate):**
  ```
  R = sort(right by [sym, ts])                      // cudf::sort_by_key
  i = upper_bound(R[sym,ts], L[sym,ts]) − 1         // cudf::upper_bound, nulls_before
  ok = i ≥ 0 ∧ R.sym[i] == L.sym                    // gather + binary_operation EQUAL
  out = gather(R.payload, i, NULLIFY) masked by ok  // cudf::gather / copy_if_else
  ```
- Variants (estimate):
  - `allow_exact_matches=False`: use `lower_bound − 1`.
  - Forward: use `lower_bound` and check `< n` and the symbol matches.
  - Tolerance: compare `L.ts − R.ts[i] ≤ tol`.
  - Nearest: take both and pick the closer.
- Cost (estimate): one sort of the right side, O(n log n), plus a binary search per left row, O(m log n). Sorting the left side is optional; sorted needles improve cache locality.
- Strings as `sym` make lexicographic compares slow. Dictionary-encode or categorical-encode `sym` to int32 first.

### Gaps
- No published libcudf as-of benchmark found. The only GPU as-of numbers found are KX's (section 5).

## 3. Range/time-based rolling windows in libcudf

### Takeaway
libcudf has mature time-range windows.
- `grouped_range_rolling_window` (since well before 25.x).
- The newer `make_range_windows` plus `range_window_type` variant API (`unbounded`, `current_row`, `bounded_closed`, `bounded_open`).
- At 26.12-dev, per-row delta columns (`bounded_closed_column`, `bounded_open_column`).

cudf-polars already uses these for `rolling(index_column=...)`, `over` and a restricted `group_by_dynamic`.

### Cited Findings
- **`cpp/include/cudf/rolling.hpp` @785ee522:**
  - `range_window_type = std::variant<unbounded, current_row, bounded_closed, bounded_open, bounded_closed_column, bounded_open_column>` (L183). The `*_column` variants carry a per-row delta column, so the window width can vary per row.
  - `make_range_windows(group_keys, orderby, order, null_order, preceding, following, stream, mr)` (L215) returns preceding/following columns to pass to `rolling_window`. `orderby` must be sorted, groupwise if there are group keys.
  - `grouped_rolling_window` has 4 overloads (L438–510). `grouped_range_rolling_window` has 3 overloads (L632, 659, 692); two take `range_window_type` and return a table, i.e. multiple requests share one window computation.
  - Source: [rolling.hpp @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/rolling.hpp)
- The 25.04 release notes list "Remove deprecated rolling window functionality (#17993)" and the removal of an unused grouped range rolling API (#18313). This marks the rolling-API rework around 25.04. — [cudf v25.04.00 breaking changes](https://newreleases.io/project/github/rapidsai/cudf/release/v25.04.00)
- The 26.08 docs list `make_range_windows`, and the libcudf rolling API docs are current for 26.10. — [docs.nvidia.com cudf 26.08 rolling](https://docs.nvidia.com/cudf/26.08/libcudf/api_docs/aggregation_rolling/); [26.10](https://docs.nvidia.com/cudf/26.10/libcudf/api_docs/aggregation_rolling/)
- **cudf-polars usage @785ee522:**
  - `plc.rolling.make_range_windows` at `dsl/expressions/rolling.py:726`.
  - `plc.rolling.grouped_range_rolling_window` at `rolling.py:226,835` and `ir.py:2454` (IR class `Rolling`, L2296).
  - `group_by_dynamic` is rewritten as a grouped truncation (`dsl/utils/groupby_dynamic.py:68`). It raises NotImplementedError for non-fixed durations, `period != every`, nonzero offset, non-default `closed`/`label`, boundary columns and non-default `start_by` (L30–50).
  - Source: [cudf_polars @785ee522](https://github.com/NVIDIA/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/python/cudf_polars/cudf_polars/dsl)

### Inferences
- Tick windows such as "trailing 5 min VWAP per sym" map directly to `grouped_range_rolling_window` on a `(sym, ts)`-sorted table. OHLC bars map to truncate plus groupby.
- The weak spots are streaming/multi-GPU ordered windows (#22032, still open) and the `group_by_dynamic` options listed above.
- The per-row delta columns could express as-of tolerance windows (estimate).

### Gaps
- Not verified which release introduced `make_range_windows` (estimate: 25.02–25.04) or `bounded_*_column` (estimate: 26.10–26.12), because the clone is shallow with no history.

## 4. Published GPU as-of / backtesting implementations and benchmarks

### Takeaway
The only audited GPU backtesting result is STAC-A3 on a DGX-2 from 2019, using cuDF, Numba and Dask. It is a per-instrument parameter sweep of a mean-reversion strategy, exactly the "parameter-sweep backtest" shape. No STAC-M3 (tick database) result on GPU was found; the latest kdb+ STAC-M3 run (Oct 2025) is CPU-only. No academic paper dedicated to GPU as-of joins was found.

### Cited Findings
- **STAC-A3 on NVIDIA DGX-2 (May 2019)**
  - System under test: NVIDIA's "STAC-A3 Pack for Python with RAPIDS (Rev A)" with Dask, Numba and cuDF, on a DGX-2 with 16× V100.
  - Result: 6,250× as many simulations on a 50-instrument basket in 60 minutes as the previous record (STAC-A3.β1.SWEEP.MAX60), i.e. 20 million vs 3,200.
  - 10,000 simulations on 48 instruments took under 6 minutes, about the same time as 1,000 simulations.
  - The only spec implemented was a per-instrument parameter sweep of a simple mean-reversion algorithm.
  - Sources: [STAC report](https://stacresearch.com/node/44695); [insideHPC](https://insidehpc.com/2019/05/nvidia-dgx-2-delivers-record-performance-on-stac-a3-benchmark/); [HPCwire](https://www.hpcwire.com/2019/05/13/nvidia-claims-6000x-speed-up-for-stock-trading-backtest-benchmark/)
- **STAC-M3, latest kdb+ result (2025-10-27):** kdb+ 4.1 on 6× Supermicro nodes with 2× Xeon 6767P each. NVIDIA parts are networking only (ConnectX-7, NDR IB); there are no GPUs. — [STAC news](https://stacresearch.com/news/stac-m3-benchmark-results-kx-kdb-4-1-on-supermicro-micron-intel/); [KDB250929](https://docs.stacresearch.com/kdb250929)
- **Papers:**
  - General GPU join studies cover sort-merge vs hash (arXiv 2312.00720), and He et al. (2008) cover GPU join primitives. Neither treats as-of joins.
  - Kinetica ships a GPU ASOF join as a commercial feature.
  - Sources: [arXiv 2312.00720](https://arxiv.org/html/2312.00720v1); [Kinetica](https://www.kinetica.com/features/time-series-database)

### Inferences
- A parameter sweep maps to cuDF poorly as dataframe ops, since each parameter set would need a separate rolling pass. It maps well as a custom kernel (Numba/CUDA) over GPU-resident columns, with one thread or block per (instrument, parameter) pair. That matches how the STAC-A3 pack used Numba alongside cuDF (estimate).

### Gaps
- Web search budget ran out before these could be searched:
  - NVIDIA developer or RAPIDS blog posts on cuDF backtesting or financial time series after 2019;
  - newer STAC-A3 results (e.g. on H100/GB200);
  - any STAC-M3 run on GPUs.
- The STAC-A3 configuration and code are behind STAC's subscription.

## 5. KDB-X GPU module: what it accelerates, and does it use cuDF?

### Takeaway
The KDB-X GPU module offloads as-of joins, sorts, binary search, selects/grouped aggregations and data movement to NVIDIA GPUs. Speedups for as-of joins and sorts are 4–10×. On cuDF the sources conflict:
- KX's Nov 2025 incubation blog says it is built on CUDA kernels, cuDF and GPUDirect Storage.
- The Apr 2026 launch blog and the Jun 2026 v2.0 blog do not mention cuDF.

### Cited Findings
- **Nov 2025 incubation blog (2025-11-06, Ryan Siegler, KX DevRel)**
  - Status then: "has not yet reached general availability".
  - API: `.gpu.aj`, `.gpu.xasc`, `.gpu.iasc`, `.gpu.gather`, `.gpu.tableToGPU`/`tableFromGPU`, `.gpu.loadKdbTable`/`saveKdbTable`.
  - Underlying stack named as NVIDIA CUDA kernels, the cuDF GPU dataframe library and GPUDirect Storage (cuFile).
  - Algorithm: keys are copied to device once, and threads run quote lookups in parallel across symbols.
  - As-of join numbers: 1M rows on L4 took about 48 ms vs about 196 ms on a 48-core CPU (~4×). 10M rows with 5 symbols on A100 80GB took about 31 ms vs about 172 ms (~5.5×). 100M rows ran under 1 s vs over 4 s.
  - Sort of 10M rows by sym,time on A100: about 42 ms vs about 320 ms (~8×).
  - 5-day VaR on 5× H100: about 0.27 s vs about 15.8 s (~58×).
  - Source: [KX blog](https://kx.com/?p=14992)
- **Launch blog (2026-04-02)**
  - Describes GPU Acceleration as a new KDB-X offering.
  - Operations: `.gpu.aj`, `.gpu.xasc`/`iasc`/`asc`, on-disk sort (keys sorted on the GPU, reordering on the CPU), `.gpu.bin`, `.gpu.select`, `.gpu.append`, `.gpu.to`/`from`.
  - Same benchmark numbers as the Nov 2025 blog.
  - No mention of cuDF, RAPIDS, CUB or Thrust.
  - Source: [Introducing KDB-X GPU Acceleration](https://kx.com/blog/introducing-kdb-x-gpu-acceleration/)
- **v2.0 blog (2026-06-08)**
  - Adds nested columns in `.gpu.select` (`select y by x`), `mavg` in grouped selects, Nsight/NVTX profiling, and a memory-release threshold ("up to 40%" runtime cut).
  - No mention of cuDF.
  - Source: [What's new in KDB-X GPU](https://kx.com/blog/whats-new-in-kdb-x-gpu-acceleration-nested-columns-profiling-and-performance-tuning/)
- **Docs:**
  - Installation needs a GPU-enabled KDB-X license and the CUDA Toolkit on non-containerized hosts; examples use CUDA 13.1 containers (via search snippet). — [code.kx.com quickstart](https://code.kx.com/kdb-x/modules/gpu/quickstart/kdb-env.html)
  - code.kx.com/kdb-x/modules/gpu/ returned HTTP 403 to the fetcher.

### Inferences
- cuDF is a dependency or influence according to KX's 2025 post. The later silence may mean custom kernels replaced it, or that it is simply not marketed (estimate).
- KX's as-of numbers (~31 ms for 10M rows on A100) give a reference point for a libcudf `upper_bound`-based as-of join, which should be of the same order (estimate).
- KX positions the GPU as accelerating a few heavy operators (aj, sort, VaR) inside a CPU engine, not as a full GPU engine.

### Gaps
- The code.kx.com GPU module reference page could not be fetched (403), so the module's own documentation could not confirm cuDF use or list all functions.
