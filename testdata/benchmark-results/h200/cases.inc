// The cases the committed H200 tree was measured from: the hand-written list's case lines as
// they stood at a5c852e4, before the list followed the registry. Compiled into
// test_corpus_goldens alone, by benchmark.rs's `h200_list`, for
// `every_declared_case_has_a_section_in_the_tree`, which holds `h200/` to it; no benchmark
// binary includes it. It goes with `PREDATES_PROVENANCE` once the H200 is run again (#226).
corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, q19, tp1_single);
corpus_query_benchmark!(tpch, 40, q17, tp1_single);
corpus_query_benchmark!(tpch, 40, shuffle_additive_avg, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, aggregate_groupby, tp1_single);
corpus_query_benchmark!(tpch, 40, filter_project, tp1_single);
corpus_query_benchmark!(tpch, 40, shuffle_additive, tp1_single);
corpus_query_benchmark!(tpch, 40, shuffle_stddev, tp1_single);
corpus_query_benchmark!(tpch, 40, nested_loop_join, tp1_single);
corpus_query_benchmark!(tpcds, 40, q37, tp1_single);
corpus_query_benchmark!(tpcds, 40, q82, tp1_single);
corpus_query_benchmark!(tpcds, 40, q84, tp1_single);
corpus_query_benchmark!(tpcds, 40, q85, tp1_single);
