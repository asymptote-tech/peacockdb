// The benchmark corpus, one line per query, included only by
// `tests/peacock_gpu_benchmarks.rs` — which is what makes it a statement about timing
// rather than about coverage.
//
//   corpus_query_benchmark!(dataset, sf, query, modes)
//
// `modes` are the planning modes of `src/test_support/mod.rs`, or'd, or
// `none`. Separate from `corpus_cases.inc` because the two disagree about sf on purpose:
// correctness runs where a wrong answer is legible, timing where there is work to time.
// build-test.md's Corpus Benchmarks section carries the rest.

// ── TPC-H, sf40 ─────────────────────────────────────────────────────────────
// sf40 and not sf1: at sf1 a query is mostly the host prologue — 0..4us per call, below
// the clock's resolution — so its rows would describe the instrument. The dataset lives
// outside the repo; `build-test-shadgpu.sh` symlinks it in before the run.
corpus_query_benchmark!(tpch, 40, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
// q1 at every mode: a four-key aggregate beside q6's keyless one, a sort, and at four lanes a
// shuffle — the call kinds q6 and q19 do not make, each at five batch sizes.
corpus_query_benchmark!(tpch, 40, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
// One mode: when the tree was measured, q19's device cell in `corpus_cases.inc` named
// `tp1_single` alone. Its other modes are on the device now; timing them needs a run.
corpus_query_benchmark!(tpch, 40, q19, tp1_single);
// A second join beside q19's, at the one mode its device cell named then.
corpus_query_benchmark!(tpch, 40, q17, tp1_single);
// The rest of the device column in `corpus_cases.inc` as it stood when the tree was measured,
// each query at the modes it named then. That column has grown since, so the timed set is a
// subset of it. nested-loop-join reads region and nation, which do not grow with sf, so its
// rows describe the host prologue.
corpus_query_benchmark!(tpch, 40, shuffle_additive_avg, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized);
corpus_query_benchmark!(tpch, 40, aggregate_groupby, tp1_single);
corpus_query_benchmark!(tpch, 40, filter_project, tp1_single);
corpus_query_benchmark!(tpch, 40, shuffle_additive, tp1_single);
corpus_query_benchmark!(tpch, 40, shuffle_stddev, tp1_single);
corpus_query_benchmark!(tpch, 40, nested_loop_join, tp1_single);

// ── TPC-DS, sf40 ────────────────────────────────────────────────────────────
// Every tpcds query the device ran when the tree was measured, at tp1-single, the one mode its
// device cell named then.
corpus_query_benchmark!(tpcds, 40, q37, tp1_single);
corpus_query_benchmark!(tpcds, 40, q82, tp1_single);
corpus_query_benchmark!(tpcds, 40, q84, tp1_single);
corpus_query_benchmark!(tpcds, 40, q85, tp1_single);
