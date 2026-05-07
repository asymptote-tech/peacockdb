#[cfg(not(feature = "rust-only"))]
mod gpu_executor_tests {
    use std::path::PathBuf;

    use peacockdb_core::gpu_executor::GpuExecutor;

    // Root of the testdata tree. PEACOCK_TESTDATA_DIR overrides the
    // compile-time path so the binary can be built on one machine and run
    // on another (e.g. shad-gpu), where CARGO_MANIFEST_DIR doesn't exist.
    fn testdata_root() -> PathBuf {
        if let Some(d) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
            return PathBuf::from(d);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
    }

    fn testdata_dir() -> PathBuf {
        testdata_root().join("tpch.sf1")
    }

    #[allow(dead_code)]
    fn queries_dir() -> PathBuf {
        testdata_root().join("tpch-queries")
    }

    const GPU_BUDGET: usize = 2 * 1024 * 1024 * 1024;

    fn total_rows(batches: &[arrow::record_batch::RecordBatch]) -> usize {
        batches.iter().map(|b| b.num_rows()).sum()
    }

    #[tokio::test]
    async fn test_gpu_scan_nation() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        let batches = exec.execute("SELECT * FROM nation").await.unwrap();
        println!("nation rows from GPU: {}", total_rows(&batches));
    }

    #[tokio::test]
    async fn test_gpu_filter_nation() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        let batches = exec
            .execute("SELECT n_name FROM nation WHERE CAST(n_nationkey AS BIGINT) > 5")
            .await
            .unwrap();
        println!("filtered nation rows from GPU: {}", total_rows(&batches));
    }

    #[tokio::test]
    async fn test_gpu_aggregate_count() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        // COUNT(*) alone triggers DataFusion's PlaceholderRowExec (metadata-only
        // row count, no scan). Use SUM to force a real GPU scan + aggregate.
        let batches = exec
            .execute("SELECT SUM(n_nationkey) FROM nation")
            .await
            .unwrap();
        println!("aggregate result rows from GPU: {}", total_rows(&batches));
    }

    #[tokio::test]
    async fn test_gpu_join_nation_region() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        let batches = exec
            .execute(
                "SELECT n.n_name, r.r_name \
                 FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey",
            )
            .await
            .unwrap();
        println!("join result rows from GPU: {}", total_rows(&batches));
    }

    #[tokio::test]
    async fn test_gpu_sort_nation() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        let batches = exec
            .execute("SELECT n_name FROM nation ORDER BY n_name ASC")
            .await
            .unwrap();
        println!("sorted nation rows from GPU: {}", total_rows(&batches));
    }

    #[tokio::test]
    async fn test_gpu_executor_create_destroy() {
        let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
        drop(exec);
    }
}
