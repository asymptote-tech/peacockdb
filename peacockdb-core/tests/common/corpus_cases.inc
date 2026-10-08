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
// q1 and q17 run everywhere on both engines.
corpus_query!(tpch, 1, q1, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q14, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q19, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q12, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// THE DEVICE COLUMN. Every tpch line runs on the device wherever it runs on the cpu, but eight:
// q11 and q22 at tp4 modes where a keyless merge meets a lane with no arrival (#199), the rollup
// (#65), mixed-join (#168), nested-limits and scan-limit (#186), rollup-distinct and
// distinct-functions (#262). The joins' own blockers, #152's side copies and #220's batch
// splits, went with the join session.
corpus_query!(tpch, 1, q13, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q15, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q17, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q3, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q10, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 1, the five smallest tpch plans the rollout list carries.
// scan-limit's rows are not determined — an unordered LIMIT over lineitem — so its modes
// need not agree and the cpu oracle is the subset one; filter-project returns most of
// lineitem, which is over the result cap. Both take a live cpu run on the device for that.
// scan-limit keeps duckdb_exact even so, because that case compares two COMMITTED files; its
// device cells, which the same oracle would judge against a RECORDED answer, are off on #186
// anyway — `an_undetermined_lines_rows_are_compared_only_against_committed_files` holds both.
// scan-limit runs at the tp4 modes only: at the two tp1 modes the planner puts the limit in
// the scan alone and the cpu backend's reader does not apply it — six million rows for ten.
// filter-project, aggregate-groupby, cross-join and shuffle-additive run everywhere.
corpus_query!(tpch, 1, scan_limit, tp4_single | tp4_rowgroup | tp4_sized, none, duckdb_exact, data_fusion_subset, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, filter_project, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, aggregate_groupby, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, cross_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_additive, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 2, the five smallest joins. semi-join and anti-join return whole `orders` rows,
// which is over the result cap, so the device compares against a live cpu run.
// All five run everywhere on both engines.
corpus_query!(tpch, 1, hash_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, left_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, semi_join, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, anti_join, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 4, the small end of the tpch list emptied, and two firsts.
// shuffle-additive-avg runs everywhere on both engines.
// shuffle-stddev is the first corpus query to need the tolerance oracle, and the tell is which
// mode is exact: tp1-single matches DataFusion to the digit, and it is the mode with a single
// lane and so nothing to reassociate. The other four differ by ~3e-15 — the association order
// of the Welford merge rather than its arithmetic, since a wrong merge would not come right at
// one lane. shuffle-stddev and nested-loop-left-join run everywhere on both engines;
// nested-limits is off on #186 (two intervals on one path, scan-limit's refusal), and
// mixed-join's `none` means "cannot cross to a device" — #168's interval `ScalarValue`.
corpus_query!(tpch, 1, shuffle_additive_avg, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_stddev, all_modes, all_modes, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_disabled); // #225
corpus_query!(tpch, 1, mixed_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_left_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, nested_limits, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 5, the tpch list from here being aggregates over joins, and the cpu is where it
// showed. q22's nested-loop join projects, which the cpu applies since its joins became
// DataFusion streams. On the device the rollup is off on #65: its grouping id is INT32 where
// the plan says UInt8, and the schema hook refuses the first `GpuAggregate`. q22 is off at
// tp4-single on #199: that mode deals customer's two row groups to four lanes, and over the two
// empty ones the keyless avg's merge has an identity row on the cpu and none on the device.
corpus_query!(tpch, 1, rollup_over_join, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q4, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q16, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, q18, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q22, all_modes, tp1_single | tp1_rowgroup | tp4_rowgroup | tp4_sized, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 6, the rest of tpch bar q7, q8 and q2. q11 is the first corpus query whose
// nested-loop join projects, which the cpu applies; over the result cap, so DuckDB is
// compared by fingerprint and the device against a live cpu run. Its three tp4 cells are off on
// #199: GERMANY's one nation row lands in one of four lanes, so three lanes of the keyless sum
// above the join get nothing, as q22's do at tp4-single.
corpus_query!(tpch, 1, q20, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q5, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q9, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q11, all_modes, tp1_single | tp1_rowgroup, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, q21, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 7, the end of tpch: three queries rather than five, because these are its deepest
// plans and the regeneration cost has moved into the queries rather than their number.
// All three run everywhere on both engines.
corpus_query!(tpch, 1, q7, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q8, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, q2, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// distinct-companions (#62): a DISTINCT aggregate beside a companion DataFusion's own rewrite
// declines. rollup-distinct puts one under a grouping set, which no other corpus query does; its
// inner stage shuffles at tp4 on the keys alone, the grouping id dropped as rollup-over-join's is.
// Both are off on the device at every mode: #262, the lowering's device cells have never run —
// rollup-distinct on #65 as well. DataFusion 45 refuses stddev(DISTINCT) and answers a grouped
// decimal avg(DISTINCT) as the plain average, so DataFusion is no oracle for distinct-functions:
// DuckDB is, and end_to_end.rs's distinct_functions_answer_as_their_hand_lowered_form holds the
// answer too. The avg columns are decimals ours renders shorter than DuckDB's floats, so both
// lines' DuckDB compare is the approximate one.
corpus_query!(tpch, 1, rollup_distinct, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, distinct_functions, all_modes, none, duckdb_approx, data_fusion_disabled, golden_exact, schema_validation_enabled);

// --- tpcds --------------------------------------------------------------------
// THE DEVICE COLUMN. Every tpcds line runs on the device wherever it runs on the cpu, but
// twelve: q32 at the tp4 modes (#199), q78 and q2 (#60's `round`), q14, q18, q22, q77 and q80
// (#65's grouping id), q5 (#210's decimal literal), q17 and q39 (#225's Welford names), q28
// (#262). The joins' own blockers, #152's side copies and #220's batch splits, went with the
// join session.
corpus_query!(tpcds, 1, q41, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q42, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q3, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q43, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q52, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q55, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
// q96's `count(*)` is declared non-nullable and the state merge answers nullable, at the
// three tp4 modes only — the shuffle is what puts a merge under it. The two tp1 modes
// run, so this is a mode-scoped disablement rather than a query-scoped one.
corpus_query!(tpcds, 1, q96, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q15, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q37, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q82, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 3, tpcds, chosen for feature spread rather than size: q48 carries no feature code,
// q21 and q84 are top_n, q93 adds an outer join and q34 a correlated subquery; the three
// smaller candidates carry `avg` and run in batches 18 to 20.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q48, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q21, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q93, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q34, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q84, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 8, tpcds, chosen by expected executor calls rather than plan size: row groups scanned
// times joins, off the tp4-rowgroup plans; q32 and q26 score as low and run in batch 19.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q45, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q40, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q97, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q50, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q62, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 9, the next five by expected calls. All five run everywhere on both engines.
corpus_query!(tpcds, 1, q73, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q79, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q1, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q7, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q94, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 10, by expected calls, the `avg` queries left to batches 18 to 20.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q19, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q8, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q99, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q91, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q46, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 11, by expected calls. All five run everywhere on both engines, q66 among them:
// #55's divisor cast never showed on the device.
corpus_query!(tpcds, 1, q68, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q71, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q66, all_modes, all_modes, duckdb_divergent(251, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q76, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q16, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 12, by expected calls. q90 runs at the two tp1 modes and not the three tp4 ones:
// #199's keyless merge over an empty lane answering a NULL `count(*)` against a non-nullable
// declaration, the second query to reach it after q96 and mode-scoped for the same reason — the
// tp4 modes are the ones that leave a lane empty.
// q2 is off on the device on #60: every per-node section matches, and the answer differs in its
// seven `round(…, 2)` columns while their row count and sums agree to the printed digit.
corpus_query!(tpcds, 1, q25, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q29, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q90, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q2, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q59, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 13, by expected calls. q54's nested-loop join projects, which the cpu applies.
// On the device q78 is off on #60: five `ratio` cells, `round(…, 2)` over Float64, one ulp
// from DataFusion's and DuckDB's at every mode. q5 is off on #210: its `0.00` literals come back
// FLOAT64, and the schema hook refuses them ahead of #65. The other three run everywhere.
corpus_query!(tpcds, 1, q78, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q5, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q54, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q95, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q38, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 14, by expected calls. All five run everywhere on both engines.
corpus_query!(tpcds, 1, q87, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q11, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q83, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q61, all_modes, all_modes, duckdb_divergent(251, 2), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q10, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 15, the expensive end of the pool: four semi-joins and a rollup over an outer join.
// q77's Right outer gets no build batch on some tp4 lanes — its build side is a grouped
// aggregate over an Inner join whose empty scatter lane drops — and pads its probe rows over
// an empty build table, as the session does (#212). Its rollup does not meet #189: the shuffle
// hashes the user keys and never the grouping id. On the device q77 is off on #65: the schema
// hook refuses its grouping id at the first `GpuAggregate`. The other four run everywhere.
corpus_query!(tpcds, 1, q33, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q56, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q60, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q69, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q77, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 16, the top of the cost ranking. q80 is q77's shape — a rollup over an outer join —
// and it runs at every cpu mode; on the device it is off on #65, as q77 is. The other four run
// everywhere on both engines.
corpus_query!(tpcds, 1, q74, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q58, all_modes, all_modes, duckdb_divergent(251, 2, 4, 6), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q80, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q4, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q31, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 17, the four largest plans in the corpus — 324 to 447 nodes against 267 for the
// largest already enabled, and no overlap between the two sets. Their node KINDS are ordinary:
// two of the four sit below the corpus median of nine, so this batch varies plan size and
// nothing else. q88 runs at the two tp1 modes only — #199's NULL `count(*)` against a
// non-nullable declaration, the third query to reach it after tpcds q96 and q90.
// q64 runs at every cpu mode, under 0.9 GB and six seconds each (2026-09-28). The 13 GB it once
// took (#192) was arrow's Utf8View buffer lists multiplying along its join chain, gone with the
// strings declared Utf8 (#183). All four run on the device wherever they run on the cpu.
corpus_query!(tpcds, 1, q75, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q23, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q88, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q64, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 18, the last query outside `avg` and the first four inside it. Each is RUN rather than
// declared: a feature code is a selection filter, not evidence of an outcome. The four `avg`
// queries run at every cpu mode. On the device q14 is off on #65's grouping id; q6, q9 and q13
// run everywhere, q9's scalar subqueries over a declared `__rowmarker__` (#63).
// join_int runs everywhere on both engines.
corpus_query!(tpch, 1, join_int, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q6, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q9, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q13, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q14, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 19, the first seven of the thirteen `avg` queries, run rather than declared. All
// seven run at every cpu mode; q18 and q22 carry a rollup, which held them off the three tp4
// modes on #189's UInt8 grouping-set id at the shuffle until it stopped being hashed; q24's
// nested-loop join projects, which the cpu applies. On the device q18 and q22 are off on #65.
// q17's two tp1 cells meet #225's Welford names at the schema hook; its tp4 cells pass and stay
// off with the row, whose answer #205 (chain K) moves. q32 is off at the tp4 modes on #199: its
// keyless sum sits over an Inner join whose build lands in one lane of four. The rest run
// everywhere.
corpus_query!(tpcds, 1, q17, all_modes, none, duckdb_divergent(205), data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q18, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q22, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q24, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q26, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q30, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q32, all_modes, tp1_single | tp1_rowgroup, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// T19 batch 20, the last six, run rather than declared, and the rollout's end. All six run at
// every cpu mode; q39 takes the tolerance oracle, its `cov` differing from DataFusion's in the
// last digits at every mode — the Welford merge over batches, as shuffle-stddev found. On the
// device q39 meets #225's Welford names at the schema hook, below #57's CASE in the filter; the
// other five run everywhere.
corpus_query!(tpcds, 1, q35, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q39, all_modes, none, duckdb_approx, data_fusion_approximate, golden_approx_std, schema_validation_enabled);
corpus_query!(tpcds, 1, q65, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q81, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q85, all_modes, all_modes, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpcds, 1, q92, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

// distinct-companions (#62): q28 is six global `avg, count, count(DISTINCT ss_list_price)`
// blocks cross-joined, and was refused at every mode until the lowering. Its device cells are off
// on #262, the lowering's device cells never having run.
corpus_query!(tpcds, 1, q28, all_modes, none, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);

// --- pbench ------------------------------------------------------------------
// What tpch and tpcds lack: one query per demonstrable ticket of chain J, planned with the
// small-table rule off, so `fact` splits across four lanes at tp4 and the one-row-group tables
// leave three empty. llm-wiki/tasks/pbench.md.
//
// BOTH COLUMNS. Every line runs on both engines at every mode it plans at, but these. Refused
// at plan time: `cross-empty-build` and `outer-on-true-empty` over a file with no row group
// (#282), `in-is-null` (#250), `not-in-uncorrelated` at tp4 (#267). `scalar-subquery-cross` at
// tp4 meets #199 on both engines; on the device alone, `like-column-pattern` meets #246,
// `rollup-small-keys` #65 and `timestamp-s-key-group` #264. Three are absent, on #255.
corpus_query!(pbench, 1, anti_null_preserved_condition, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, bool_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, cross_empty_build, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, cross_projection, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_join, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal38_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, empty_side_left_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, exists_null_keys, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, exists_or_mark, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, finish_without_probe, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, full_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, full_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, in_is_null, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, indf_full_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, inner_join_hot_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, int8_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, left_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, left_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, like_column_pattern, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, mark_cross_residual, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_full, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_inner, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_anti, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_decimal, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_semi, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_mark, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_projection, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_anti, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_semi, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_exists_null_keys, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_in_correlated, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_in_uncorrelated, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb_divergent(205), data_fusion_exact, golden_exact, schema_validation_enabled); // zero rows, no batch: #205
corpus_query!(pbench, 1, not_in_under_or, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_not_in, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, not_or_not_in, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, outer_on_true_empty, none, none, duckdb_none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, probe_exists_residual, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, probe_not_exists_residual, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, right_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, right_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, rollup_small_keys, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, scalar_subquery_cross, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb_approx, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_anti, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_full, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_right, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_left, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_semi, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ms_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ns_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_s_key_group, all_modes, none, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled); // device: #264, a cast group key
corpus_query!(pbench, 1, timestamp_us_key_group, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(pbench, 1, ts_key_join, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_group, all_modes, all_modes, duckdb_fingerprint, data_fusion_exact, live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_join, all_modes, all_modes, duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled);

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
