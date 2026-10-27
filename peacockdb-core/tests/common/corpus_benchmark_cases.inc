// The benchmark corpus, one line per query, timed by `tests/peacock_gpu_benchmarks.rs` — a
// statement about timing rather than about coverage. `test_corpus_goldens` compiles it in too,
// to check it.
//
//   corpus_query_benchmark!(dataset, sf, query, modes)
//
// Written from testdata/cost-registry.csv, never by hand: every tpch and tpcds row's enabled
// device cells (`gpu_*`), timed at sf40. `the_benchmark_cases_follow_the_registry` in
// `test_corpus_goldens` holds it to the registry and rewrites it under UPDATE_CANONICAL=1.
// sf40 and not sf1: at sf1 a query is mostly the host prologue, below the clock's resolution.
// build-test.md's Corpus Benchmarks section carries the rest.

// ── tpch, sf40 ──
corpus_query_benchmark!(tpch, 40, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q2, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q3, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q4, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q5, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q7, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q8, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q9, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q10, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q11, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q12, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q13, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q14, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q15, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q16, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q17, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q18, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q19, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q20, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q21, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q22, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, aggregate_groupby, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, anti_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, anti_join_reorder, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, cross_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, distinct_functions, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, filter_project, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, hash_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, join_int, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, left_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, left_join_fixed, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, left_join_reorder, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, mixed_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, nested_limits, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, nested_loop_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, nested_loop_left_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, rollup_distinct, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, rollup_grouping, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, rollup_over_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, scan_limit, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, semi_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, semi_join_reorder, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, shuffle_additive, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, shuffle_additive_avg, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, shuffle_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, rollup_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, global_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);

// ── tpcds, sf40 ──
corpus_query_benchmark!(tpcds, 40, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q2, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q3, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q4, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q5, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q7, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q8, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q9, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q10, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q11, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q13, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q14, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q15, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q16, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q17, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q18, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q19, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q21, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q22, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q23, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q24, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q25, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q26, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q28, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q29, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q30, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q31, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q32, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q33, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q34, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q35, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q37, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q38, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q39, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q40, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q41, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q42, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q43, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q45, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q46, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q48, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q50, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q52, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q54, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q55, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q56, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q58, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q59, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q60, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q61, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q62, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q64, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q65, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q66, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q68, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q69, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q71, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q73, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q74, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q75, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q76, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q77, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q78, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q79, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q80, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q81, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q82, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q83, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q84, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q85, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q87, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q88, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q90, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q91, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q92, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q93, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q94, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q95, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q96, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q97, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpcds, 40, q99, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
