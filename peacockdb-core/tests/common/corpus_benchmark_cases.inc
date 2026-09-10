// The benchmark corpus, one line per query, included only by
// `tests/peacock_gpu_benchmarks.rs` — which is what makes it a statement about timing
// rather than about coverage.
//
//   corpus_query_benchmark!(dataset, sf, query, modes)
//
// `modes` are the batch-partitioned planning modes of `common/bp_mode.rs`, or'd, or
// `none`. Separate from `corpus_cases.inc` because the two disagree about sf on purpose:
// correctness runs where a wrong answer is legible, timing where there is work to time.
// build-test.md's Corpus Benchmarks section carries the rest.

// ── TPC-H, sf40 ─────────────────────────────────────────────────────────────
// sf40 and not sf1: at sf1 a query is mostly the host prologue — 0..4us per call, below
// the clock's resolution — so its rows would describe the instrument. The dataset lives
// outside the repo; `build-test-shadgpu.sh` symlinks it in before the run.
corpus_query_benchmark!(tpch, 40, q6, bp_tp1_single | bp_tp4_sized);
// One mode, not two: q19's device cell in `corpus_cases.inc` names `bp_tp1_single` alone,
// and naming a mode a query is not enabled at buys a plan-time failure rather than a
// number. Which ticket holds the other four is recorded beside that cell, not restated
// here.
corpus_query_benchmark!(tpch, 40, q19, bp_tp1_single);
