// The corpus, one line per query, included by both corpus binaries — the cpu's macro expands
// `cpu_modes`, the device's `gpu_modes`, so one declaration covers both engines:
//
//   corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle,
//                 gpu_oracle, schema_validation)
//
// Modes are the five planning modes or'd together, `all_modes` for all five, or `none`; every
// argument is written, so a query's whole coverage reads off the line. `duckdb_oracle` says
// what DuckDB's answer is held to: duckdb_exact | duckdb_approx |
// duckdb_divergent(<ticket>[, <positions>]) | duckdb_fingerprint | duckdb_none. The last
// argument is the device run's schema validation; a cell red on schema alone says
// `schema_validation_disabled` with its ticket on the line.
// The first twenty are the smallest plans carrying no ticket; the rest of the corpus is T19.

// --- tpch ---------------------------------------------------------------------
corpus_query!(tpch, 1, q6, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// q1 runs everywhere on both engines; q17 at every cpu mode and at tp1-single on the
// device, where its other four modes are #152's build-side copy.
corpus_query!(tpch, 1, q1, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q14, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q19, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q12, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// THE DEVICE COLUMN. q6 runs at all five modes and q19 at tp1-single; every other device
// cell is off against a ticket. The causes: #152 refuses a join whose recipe copies a side
// the ABI cannot copy (by name rather than by crash), #95 is the hash scatter refusing a
// decimal key, #220 is the cpu splitting a join's output into more batches than the device, seen
// first at the aggregate merge's in_rows above it.
// T19 is where the rest are settled query by query.
corpus_query!(tpch, 1, q13, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q15, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q17, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q3, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q10, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 1, the five smallest tpch plans the rollout list carries.
// scan-limit's rows are not determined — an unordered LIMIT over lineitem — so its modes
// need not agree and the cpu oracle is the subset one; filter-project returns most of
// lineitem, which is over the result cap. Both take a live cpu run on the device for that.
// scan-limit keeps duckdb_exact even so, because that case compares two COMMITTED files; its
// device cells, which the same oracle would judge against a RECORDED answer, are off on #186
// anyway — `an_undetermined_lines_rows_are_compared_only_against_committed_files` holds both.
// scan-limit runs at the tp4 modes only: at the two tp1 modes the planner puts the limit in
// the scan alone and the cpu backend's reader does not apply it — six million rows for ten.
// The other four run at tp1-single, bar cross-join, off on #220's join batching.
corpus_query!(tpch, 1, scan_limit, tp4_single | tp4_rowgroup | tp4_sized, none, duckdb_exact, data_fusion_subset, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, filter_project, all_modes, tp1_single, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, aggregate_groupby, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, cross_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_additive, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 2, the five smallest joins. semi-join and anti-join return whole `orders` rows,
// which is over the result cap, so the device compares against a live cpu run.
// nested-loop-join runs at tp1-single. Off: #152's copy shapes for left-join (its probe
// batch) and hash-join (its build side) at four modes; at tp1-single hash-join is #220's
// in_rows at the merge, and semi-join and anti-join are #220's join batching at the unload.
// A row keeping `183` has modes never run past the string class.
corpus_query!(tpch, 1, hash_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, left_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, semi_join, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, anti_join, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_join, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 4, the small end of the tpch list emptied, and two firsts.
// shuffle-additive-avg runs everywhere on both engines.
// shuffle-stddev is the first corpus query to need the tolerance oracle, and the tell is which
// mode is exact: tp1-single matches DataFusion to the digit, and it is the mode with a single
// lane and so nothing to reassociate. The other four differ by ~3e-15 — the association order
// of the Welford merge rather than its arithmetic, since a wrong merge would not come right at
// one lane. shuffle-stddev runs at tp1-single on the device; nested-loop-left-join is off on
// #220's join batching, nested-limits on #186 (two intervals on one path, scan-limit's refusal),
// and mixed-join's `none` means "cannot cross to a device" — #168's interval `ScalarValue`.
corpus_query!(tpch, 1, shuffle_additive_avg, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_stddev, all_modes, tp1_single, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_disabled); // #225
corpus_query!(tpch, 1, mixed_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_left_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, nested_limits, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 5, the tpch list from here being aggregates over joins, and the cpu is where it
// showed. One of the five is cut before a device ever sees it: q22's nested-loop join projects
// and the cpu backend drops the projection — #190, at every mode.
// On the device every query here aggregates over a join: #152's build-side copy where the join
// sees several probe batches, and at tp1-single, which reaches past it, the golden's `in_rows`
// at the merge for q16, the rollup and q18, and q4's join itself — all #220's batching.
corpus_query!(tpch, 1, rollup_over_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q4, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q16, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, q18, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q22, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 6, the rest of tpch bar q7, q8 and q2. q11 is out at every mode on both engines:
// its nested-loop join carries a projection and the cpu backend passes `None` where DataFusion
// takes one, so the node declares two columns and the join answers three — #190. The first
// corpus query whose nested-loop join projects, which is why four batches went past it.
// tp1-single is the only mode whose join gets a single probe batch, so it is the only one past
// #152's build-side copy, and what it meets there is per query: #220's join batching for q20,
// q21 and q9, #220's `in_rows` at the merge for q5.
corpus_query!(tpch, 1, q20, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q5, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q9, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q11, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q21, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 7, the end of tpch: three queries rather than five, because these are its deepest
// plans and the regeneration cost has moved into the queries rather than their number.
// #152 at four modes each, and at tp1-single, the mode that always reaches the unload, all
// three run the whole device plan and meet #220's join batching — q7 and q8 at the merge
// above their joins, q2 at its min aggregate.
corpus_query!(tpch, 1, q7, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q8, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q2, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// --- tpcds --------------------------------------------------------------------
corpus_query!(tpcds, 1, q41, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q42, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q3, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q43, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q52, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q55, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// q96's `count(*)` is declared non-nullable and the state merge answers nullable, at the
// three tp4 modes only — the shuffle is what puts a merge under it. The two tp1 modes
// run, so this is a mode-scoped disablement rather than a query-scoped one.
corpus_query!(tpcds, 1, q96, tp1_single | tp1_rowgroup, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q15, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q37, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q82, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 3, tpcds, chosen for feature spread rather than size: q48 carries no feature code,
// q21 and q84 are top_n, q93 adds an outer join and q34 a correlated subquery; the three
// smaller candidates carry `avg` and run in batches 18 to 20.
// q84 runs at tp1-single. The rest: #152's build-side copy at the other modes, and at tp1-single
// the device RUNS and the golden catches it — #220 for all four. q93's merge reports in_rows 7169
// against the cpu's 7486: the cpu's join split its output into 36 batches, so the aggregate
// over it made 36 partials where the device made one.
corpus_query!(tpcds, 1, q48, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q21, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q93, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q34, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q84, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 8, tpcds, chosen by expected executor calls rather than plan size: row groups scanned
// times joins, off the tp4-rowgroup plans; q32 and q26 score as low and run in batch 19.
// #152 at four modes, and at tp1-single #220's batching for q45, q40,
// q50 and q62 — with q97 as the exception: its outer join copies the PROBE batch, which #152
// refuses at any batch count, so the one mode that usually gets far enough does not.
corpus_query!(tpcds, 1, q45, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q40, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q97, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q50, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q62, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 9, the next five by expected calls. q1 and q7 run at every cpu mode; on the device
// both are #220's `in_rows` at tp1-single, their other modes never run.
// The other three's fifteen device cells: #152's build copy at four modes, and at tp1-single
// #220's batching for q73, q79 and q94.
corpus_query!(tpcds, 1, q73, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q79, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q1, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q7, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q94, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 10, by expected calls, the `avg` queries left to batches 18 to 20.
// Twenty-five device cells: #152's build copy at four modes, and at tp1-single #220's batching
// for q19 and q91, #220's `in_rows` for q8, q46 and q99.
corpus_query!(tpcds, 1, q19, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q8, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q99, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q91, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q46, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 11, by expected calls. q71 is #152 at tp1-single too, where every other query so
// far has reached the unload: its probe side is three fact tables unioned, and a fact table
// streams in 8192-row batches whatever the mode. `single` is a scan's partitioning, not a
// promise of one batch — what decides #152's build half is the probe side's SIZE.
corpus_query!(tpcds, 1, q68, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q71, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q66, all_modes, none, duckdb_divergent(251, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q76, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q16, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 12, by expected calls. q90 runs at the two tp1 modes and not the three tp4 ones:
// #199's keyless merge over an empty lane answering a NULL `count(*)` against a non-nullable
// declaration, the second query to reach it after q96 and mode-scoped for the same reason — the
// tp4 modes are the ones that leave a lane empty.
// q2 is #152 at tp1-single as well, and it is the first query the mechanism rule predicted
// rather than explained: its probe side unions web_sales and catalog_sales, so the join sees
// several batches at every mode.
corpus_query!(tpcds, 1, q25, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q29, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q90, tp1_single | tp1_rowgroup, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q2, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q59, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 13, by expected calls. q54's nested-loop join projects, so #190 drops it at every
// mode and the node declares three columns against DataFusion's four.
// On the device, q38 is the third cell in the rollout where the device RAN and the golden caught
// it rather than the run refusing — #220 again, in_rows 11788 against the cpu's 12446 at the
// merge, the cpu's join having split its output.
corpus_query!(tpcds, 1, q78, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q5, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q54, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q95, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q38, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 14, by expected calls, and the batch-count rule LOST to the query written to test
// it. q11's eleven joins all copy their build side and four take 336 or 88 probe batches at
// tp1-single, where the rule says #152 must fire; the device reached the unload instead, and is
// off there on #220's join batching. What separates it from q71 and q2, which do refuse there,
// is not in these goldens — see #152. q87's RightAnti-against-#175 race resolved as neither:
// it ran, and the golden caught it on #220.
corpus_query!(tpcds, 1, q87, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q11, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q83, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q61, all_modes, none, duckdb_divergent(251, 2), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q10, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 15, the expensive end of the pool: four semi-joins and a rollup over an outer join.
// q77 runs at the two tp1 modes only: its Right outer's build side is a grouped aggregate over
// an Inner join whose empty scatter lane drops as it should, so the outer gets no build batch at
// all — #212, the route #175's fix does not reach, and all that holds its three tp4 cells. Its
// rollup does not meet #189: the shuffle hashes the user keys and never the grouping id.
corpus_query!(tpcds, 1, q33, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q56, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q60, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q69, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q77, tp1_single | tp1_rowgroup, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 16, the top of the cost ranking. q80 is q77's shape — a rollup over an outer join —
// and it runs at every cpu mode, where q77's three tp4 cells wait on #212's missing build batch:
// which ticket a query meets is a property of the query and not of the shape.
corpus_query!(tpcds, 1, q74, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q58, all_modes, none, duckdb_divergent(251, 2, 4, 6), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q80, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q4, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q31, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 17, the four largest plans in the corpus — 324 to 447 nodes against 267 for the
// largest already enabled, and no overlap between the two sets. Their node KINDS are ordinary:
// two of the four sit below the corpus median of nine, so this batch varies plan size and
// nothing else. q88 runs at the two tp1 modes only — #199's NULL `count(*)` against a
// non-nullable declaration, the third query to reach it after tpcds q96 and q90.
// q64 runs at every cpu mode, under 0.9 GB and six seconds each (2026-09-28). The 13 GB it once
// took (#192) was arrow's Utf8View buffer lists multiplying along its join chain, gone with the
// strings declared Utf8 (#183). Its device cells are off unrun, read off the plan: at four modes
// ten joins take several probe batches per lane (#152); at tp1-single each takes one, and the
// cpu's joins split their output into 8192-row batches (#220).
// Twelve device cells: #152 at every mode for q75 and at four for q23, whose tp1-single is off
// on #220's join batching; q88 at tp1-single is #220 again — the device ran and the golden
// caught it — and its tp1-rowgroup was never run past the string class, so the row's 152 stands.
corpus_query!(tpcds, 1, q75, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q23, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q88, tp1_single | tp1_rowgroup, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q64, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 18, the last query outside `avg` and the first four inside it. Each is RUN rather than
// declared: a feature code is a selection filter, not evidence of an outcome. The four `avg`
// queries run at every cpu mode. On the device at tp1-single, their other modes never run: q6 is
// #152's probe-batch copy; q9 fails in the CASE over its scalar subqueries, `copy_if_else`
// refusing two branch types (#63); q13 and q14 are #220's `in_rows`.
// join_int's five device cells are #152 at four modes and #220 at tp1-single, the sixth cell in
// the rollout the device ran and the golden caught.
corpus_query!(tpch, 1, join_int, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q6, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q9, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q13, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q14, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 19, the first seven of the thirteen `avg` queries, run rather than declared. Four run
// at every cpu mode; q18 and q22 carry a rollup, which held them off the three tp4 modes on
// #189's UInt8 grouping-set id at the shuffle until it stopped being hashed; q24's nested-loop
// join projects and the cpu backend drops it (#190), at every mode. On the device at tp1-single,
// the other modes never run:
// q17 answers zero rows, and the cpu's accumulating sort emits nothing where the device emits the
// zero-row batch (#205); q18 and q30 are #220's join batching; q22, q26 and q32 are #220's
// `in_rows`.
corpus_query!(tpcds, 1, q17, all_modes, none, duckdb_divergent(205), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q18, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q22, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q24, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q26, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q30, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q32, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 20, the last six, run rather than declared, and the rollout's end. All six run at
// every cpu mode; q39 takes the tolerance oracle, its `cov` differing from DataFusion's in the
// last digits at every mode — the Welford merge over batches, as shuffle-stddev found. On the
// device at tp1-single: q85 runs, its other four modes #152's build-side copy; q39 is #57's
// value-form CASE in the filter; q35 and q81 are #220's join batching; q65 and q92 are #220's
// `in_rows`.
corpus_query!(tpcds, 1, q35, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q39, all_modes, none, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_enabled);
corpus_query!(tpcds, 1, q65, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q81, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q85, all_modes, tp1_single, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q92, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// --- pbench ------------------------------------------------------------------
// The dataset whose data holds what tpch and tpcds lack: one query per demonstrable ticket of
// chain J, cells off where a ticket blocks them, each later task turning its own on. It plans
// with the small-table rule off, so `fact` splits across four lanes at tp4 and the
// one-row-group tables leave three empty. llm-wiki/tasks/pbench.md, and -detail.md beside it.
//
// Every runnable gpu cell was run once on an L40S under cuDF 25.02 and settled on what it
// measured: 30 are on, and an off cell carries the ticket it failed on. 157 could not be run —
// a gpu cell needs a cpu cell at its mode. Of the 29 rows with no cpu cell, 24 are refused by
// the planner, 21 of them citing their ticket on the golden's `refused:` line; the other 5 plan
// fine and the cpu backend refuses them at run time, on #190. Three queries are absent, on #255.
corpus_query!(pbench, 1, anti_null_preserved_condition, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, bool_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, cross_empty_build, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, cross_projection, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_join, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal38_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_side_left_join, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, exists_null_keys, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, exists_or_mark, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, finish_without_probe, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, full_join_null_keys, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, full_join_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, in_is_null, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, indf_full_join, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, inner_join_hot_keys, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, int8_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, left_join_null_keys, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, left_join_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, like_column_pattern, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, mark_cross_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_full, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_inner, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_anti, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_decimal, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_semi, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_mark, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_projection, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_anti, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_semi, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_exists_null_keys, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_in_correlated, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_in_uncorrelated, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_in_under_or, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_not_in, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_or_not_in, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, outer_on_true_empty, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, probe_exists_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, probe_not_exists_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, right_join_null_keys, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, right_join_residual, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, rollup_small_keys, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, scalar_subquery_cross, tp1_single | tp1_rowgroup, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_anti, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_full, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_right, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_left, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_semi, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ms_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ns_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_s_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled); // device: #264, a cast group key
corpus_query!(pbench, 1, timestamp_us_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, ts_key_join, all_modes, none, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_group, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_join, all_modes, tp1_single, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// #243 — the cpu keys -0.0 and NaN by their BITS, so the three float rows land with their SQL,
// their plan goldens and their DuckDB answers, and their lines commented out for the task that
// fixes it to enable. MEASURED: the cpu answers float64-key-group in 54 groups against DuckDB's
// 52. The oracle is duckdb_approx and not exact because arrow renders a NaN `NaN` and DuckDB
// renders it `nan`. Every cpu AND gpu cell is `na` rather than `disabled`: a commented line
// declares nothing, and a `disabled` cpu cell wants a `skipped:` section in the mode golden
// that no declaration writes. llm-wiki/tasks/pbench-detail.md carries the rest.
// corpus_query!(pbench, 1, float32_key_group, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
// corpus_query!(pbench, 1, float64_key_group, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
// corpus_query!(pbench, 1, float64_key_join, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
