#[cfg(not(feature = "rust-only"))]
mod gpu_executor_tests {
    use std::path::PathBuf;

    use peacockdb_core::gpu_executor::GpuExecutor;

        fn testdata_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.sf1")
    }

    fn queries_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries")
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
