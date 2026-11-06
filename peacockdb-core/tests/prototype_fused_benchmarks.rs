//! The fused batches on the device (query-fuser): per dataset, the first 3, 10, 20 and all
//! queries of the H100 `cases.csv`, read when the test runs, fused into one plan and timed at
//! sf40 tp1_single into `tp1_single.fused.txt`. A prototype's run, not a benchmark the CI knows:
//! no CI step builds or runs it. A run is `scripts/env/run-gpu.sh <vm options> run
//! prototype_fused_benchmarks --exact <test>` on the H100; build-test.md, "Benchmarks".
#![cfg(feature = "gpu")]

macro_rules! fused_benchmark {
    ($dataset:ident, $batch:ident) => {
        paste::paste! {
            #[tokio::test]
            async fn [<bench_fused_ $dataset _sf40_ $batch _tp1_single>]() {
                peacockdb_core::prototypes::benchmark_fused(
                    stringify!($dataset),
                    "40",
                    stringify!($batch),
                )
                .await;
            }
        }
    };
}

fused_benchmark!(tpch, first3);
fused_benchmark!(tpch, first10);
fused_benchmark!(tpch, first20);
fused_benchmark!(tpch, all);
fused_benchmark!(tpcds, first3);
fused_benchmark!(tpcds, first10);
fused_benchmark!(tpcds, first20);
fused_benchmark!(tpcds, all);
