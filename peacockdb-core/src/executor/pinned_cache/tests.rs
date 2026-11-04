//! The pinned host cache's Rust side without the FFI: the session option, the values the
//! benchmark's trailer and refusal print, and which files a plan's prefill names.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::executor::{
    PeacockOptions, PinnedCache, PinnedCacheSource, PinnedCacheState, PrefillError, plan_files,
};
use crate::plan::{GpuLoadParquet, GpuNode, RowGroupMeta, ScanMetadata, Schema};
use crate::planner;
use crate::test_support::{data_dir_for, mode_named, queries_dir_for};

fn option_of(ctx: &datafusion::execution::context::SessionContext) -> Option<u64> {
    ctx.state()
        .config()
        .options()
        .extensions
        .get::<PeacockOptions>()
        .expect("build_session_state registers the peacock options")
        .pinned_host_cache_bytes
}

#[test]
fn the_cache_option_is_unset_by_default() {
    assert_eq!(option_of(&crate::build_session_state(1)), None);
}

#[tokio::test]
async fn set_reads_back() {
    let ctx = crate::build_session_state(1);
    ctx.sql("SET peacock.pinned_host_cache_bytes = 0")
        .await
        .expect("SET takes a byte count");
    assert_eq!(option_of(&ctx), Some(0));
}

#[tokio::test]
async fn a_non_number_is_refused() {
    let ctx = crate::build_session_state(1);
    let refused = ctx
        .sql("SET peacock.pinned_host_cache_bytes = 'lots'")
        .await
        .expect_err("'lots' is not a byte count");
    let message = refused.to_string();
    assert!(
        message.contains("peacock.pinned_host_cache_bytes"),
        "the refusal has to name the key: {message}"
    );
    assert_eq!(
        option_of(&ctx),
        None,
        "a refused SET leaves the option unset"
    );
}

#[test]
fn pinned_cache_reads_as_one_trailer_value() {
    const GIB: u64 = 1 << 30;
    let on = PinnedCache {
        state: PinnedCacheState::On,
        source: PinnedCacheSource::Default,
        capacity_bytes: 1370 * GIB / 10,
        resident_bytes: 123 * GIB / 10,
        files: 8,
        fills: 8,
        fill_us: 812345,
        ..Default::default()
    };
    let off = PinnedCache {
        source: PinnedCacheSource::Env,
        ..Default::default()
    };
    let integrated = PinnedCache {
        state: PinnedCacheState::Integrated,
        ..Default::default()
    };
    let no_device = PinnedCache {
        state: PinnedCacheState::NoDevice,
        ..Default::default()
    };
    let shown = [on, off, integrated, no_device].map(|cache| cache.to_string());
    assert_eq!(
        shown,
        [
            "on cap=137.0GiB resident=12.3GiB files=8 fills=8 fill_us=812345 fallbacks=0 \
             evictions=0 reloads=0 source=default",
            "off source=env",
            "off integrated",
            "off no-device",
        ]
    );
    assert!(shown.iter().all(|s| !s.contains('\n')), "{shown:?}");
}

#[test]
fn a_refusal_reads_as_its_sizes() {
    let refusal = PrefillError::DoesNotFit {
        needed: 160_000_000_000,
        limit: 147_000_000_000,
    };
    assert_eq!(
        refusal.to_string(),
        "needs 160000000000 > limit 147000000000"
    );
    let io = PrefillError::Io {
        path: PathBuf::from("/data/lineitem.parquet"),
        error: "No such file or directory".to_string(),
    };
    assert_eq!(
        io.to_string(),
        "/data/lineitem.parquet: No such file or directory"
    );
    assert_eq!(
        PrefillError::Off.to_string(),
        "the pinned host cache is off"
    );
}

/// tpch sf1's `query` planned at tp1_single, as the corpus plans it.
async fn planned(query: &str) -> Box<dyn GpuNode> {
    let mode = mode_named("tp1_single");
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tables");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join(format!("{query}.sql")))
        .unwrap_or_else(|e| panic!("{query}.sql: {e}"));
    let physical = ctx
        .sql(&sql)
        .await
        .expect("DataFusion plans it")
        .create_physical_plan()
        .await
        .expect("a physical plan");
    let (tree, _memory) = planner::plan(&physical, mode.knobs_for("tpch"))
        .unwrap_or_else(|why| panic!("{query} at {}: {why}", mode.name));
    tree
}

/// The files as one spelling, so the comparison does not rest on how DataFusion writes a path.
fn canonical(files: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    files
        .iter()
        .map(|f| std::fs::canonicalize(f).unwrap_or_else(|e| panic!("{}: {e}", f.display())))
        .collect()
}

fn tpch_files(tables: &[&str]) -> BTreeSet<PathBuf> {
    let dir = data_dir_for("tpch", "1");
    canonical(
        &tables
            .iter()
            .map(|t| dir.join(format!("{t}.parquet")))
            .collect(),
    )
}

#[tokio::test]
async fn plan_files_of_q3_names_its_three_tables() {
    let q3 = planned("q3").await;
    let files = plan_files(&[q3.as_ref()]);
    assert_eq!(files.len(), 3, "{files:?}");
    assert_eq!(
        canonical(&files),
        tpch_files(&["customer", "lineitem", "orders"])
    );
}

#[tokio::test]
async fn plan_files_over_roots_is_the_union() {
    let (q3, q6) = (planned("q3").await, planned("q6").await);
    let files = plan_files(&[q3.as_ref(), q6.as_ref(), q3.as_ref()]);
    assert_eq!(files.len(), 3, "lineitem is named once: {files:?}");
    assert_eq!(
        canonical(&files),
        tpch_files(&["customer", "lineitem", "orders"])
    );
    assert_eq!(
        canonical(&plan_files(&[q6.as_ref()])),
        tpch_files(&["lineitem"])
    );
}

/// The device answers a scan of no column from the footer before any lease, so its file
/// would be pinned for nothing.
#[test]
fn plan_files_skips_a_rows_only_load() {
    let scan = ScanMetadata {
        file: "/customer.parquet".to_string(),
        groups: vec![RowGroupMeta {
            index: 0,
            rows: 10,
            bytes: 100,
        }],
        can_be_null: vec![],
    };
    let load = GpuLoadParquet::new(
        "customer".to_string(),
        vec![],
        vec![vec![vec![0]]],
        &scan,
        Schema::rowmarker(),
    );
    assert!(load.rows_only());
    assert_eq!(plan_files(&[&load]), BTreeSet::new());
}
