//! The session option and the prefill through the Rust wrappers, on a device. The lib's
//! device rung runs `--test-threads=1`, which these need: the cache is the process's.
//!
//! With `PEACOCK_PINNED_HOST_CACHE_BYTES` set (the L40S list's cache-off run) the
//! environment fixes the capacity, so each case checks only that the environment won.

use std::path::{Path, PathBuf};
use std::ptr;

use datafusion::execution::context::SessionContext;
use peacockdb_ffi::raw::{PeacockExecutor, peacock_executor_create, peacock_executor_destroy};

use crate::executor::{
    PinnedCache, PinnedCacheSource, PinnedCacheState, PrefillError, apply_pinned_host_cache,
    prefill_on,
};

const MIB: u64 = 1 << 20;

/// Applies a fresh session's option, `SET` to `bytes` first unless it is `None`.
async fn applied(bytes: Option<u64>) -> PinnedCache {
    let ctx: SessionContext = crate::build_session_state(1);
    if let Some(bytes) = bytes {
        ctx.sql(&format!("SET peacock.pinned_host_cache_bytes = {bytes}"))
            .await
            .expect("SET takes a byte count");
    }
    apply_pinned_host_cache(ctx.state().config())
}

/// True, after checking it, when the environment fixes the capacity.
fn environment_won(cache: &PinnedCache) -> bool {
    if std::env::var_os("PEACOCK_PINNED_HOST_CACHE_BYTES").is_none() {
        return false;
    }
    assert_eq!(cache.source, PinnedCacheSource::Env, "{cache}");
    true
}

struct Executor(*mut PeacockExecutor);

impl Executor {
    fn new() -> Self {
        let mut executor: *mut PeacockExecutor = ptr::null_mut();
        assert_eq!(unsafe { peacock_executor_create(0, &mut executor) }, 0);
        Self(executor)
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        unsafe { peacock_executor_destroy(self.0) };
    }
}

fn temp_file(dir: &Path, name: &str, bytes: u64) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, vec![0x5Au8; bytes as usize]).expect("write the temp file");
    path
}

#[tokio::test]
async fn the_session_option_reaches_the_cache() {
    let off = applied(Some(0)).await;
    if environment_won(&off) {
        return;
    }
    assert_eq!(off.state, PinnedCacheState::Off, "{off}");
    assert_eq!(off.source, PinnedCacheSource::Session);
    assert_eq!(off.capacity_bytes, 0);

    let default = applied(None).await;
    assert_eq!(default.state, PinnedCacheState::On, "{default}");
    assert_eq!(default.source, PinnedCacheSource::Default);
    assert!(default.capacity_bytes > 0, "{default}");
}

#[tokio::test]
async fn prefill_reports_its_files_and_refuses_what_does_not_fit() {
    let dir = std::env::temp_dir().join(format!("peacock-prefill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the temp dir");
    let files = [temp_file(&dir, "a.bin", MIB), temp_file(&dir, "b.bin", MIB)];
    let executor = Executor::new();

    let default = applied(None).await;
    if environment_won(&default) {
        assert_eq!(prefill_on(executor.0, &files), Err(PrefillError::Off));
        return;
    }
    let first = prefill_on(executor.0, &files).expect("two MiB fit the default");
    assert_eq!(
        (first.files, first.bytes, first.needed),
        (2, 2 * MIB, 2 * MIB)
    );
    let again = prefill_on(executor.0, &files).expect("a repeat is idempotent");
    assert_eq!((again.files, again.bytes), (0, 0));

    let small = applied(Some(MIB)).await;
    assert_eq!(small.capacity_bytes, MIB);
    let refused = prefill_on(executor.0, &files);
    assert_eq!(
        refused,
        Err(PrefillError::DoesNotFit {
            needed: 2 * MIB,
            limit: MIB
        })
    );
    let after = crate::executor::pinned_host_cache();
    assert_eq!(
        after.resident_bytes, small.resident_bytes,
        "a refused prefill pins nothing"
    );

    applied(None).await;
    let missing = dir.join("missing.bin");
    match prefill_on(executor.0, &[files[0].clone(), missing.clone()]) {
        Err(PrefillError::Io { path, error }) => {
            assert_eq!(path, missing);
            assert!(!error.is_empty());
        }
        other => panic!("a missing file is an Io refusal naming it, not {other:?}"),
    }

    applied(Some(0)).await;
    assert_eq!(prefill_on(executor.0, &files), Err(PrefillError::Off));
    applied(None).await;
    std::fs::remove_dir_all(&dir).expect("remove the temp dir");
}
