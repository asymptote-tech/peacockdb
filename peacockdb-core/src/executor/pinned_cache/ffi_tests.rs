//! The wrappers over the cache's three symbols where the FFI is linked and no device is,
//! which is this rung by definition: the C++ answer arrives as `NoDevice` and `Off`.

use std::ptr;

use peacockdb_ffi::raw::{PeacockExecutor, peacock_executor_create, peacock_executor_destroy};

use crate::executor::{
    PinnedCacheSource, PinnedCacheState, PrefillError, apply_pinned_host_cache, pinned_host_cache,
    prefill_on,
};

#[tokio::test]
async fn the_cache_is_off_without_a_device() {
    let cache = pinned_host_cache();
    assert_eq!(cache.state, PinnedCacheState::NoDevice);
    assert_eq!(cache.capacity_bytes, 0);
    assert_eq!(cache.to_string(), "off no-device");

    let ctx = crate::build_session_state(1);
    ctx.sql("SET peacock.pinned_host_cache_bytes = 1048576")
        .await
        .expect("SET takes a byte count");
    let applied = apply_pinned_host_cache(ctx.state().config());
    assert_eq!(applied.state, PinnedCacheState::NoDevice);
    assert_eq!(applied.source, PinnedCacheSource::Session);
    assert_eq!(
        applied.capacity_bytes, 0,
        "no device pins nothing, whatever was set"
    );

    let file = std::env::temp_dir().join(format!("peacock-lib-ffi-{}.bin", std::process::id()));
    std::fs::write(&file, [7u8; 4096]).expect("write the temp file");
    let mut executor: *mut PeacockExecutor = ptr::null_mut();
    assert_eq!(unsafe { peacock_executor_create(0, &mut executor) }, 0);
    let prefilled = prefill_on(executor, std::slice::from_ref(&file));
    unsafe { peacock_executor_destroy(executor) };
    std::fs::remove_file(&file).expect("remove the temp file");
    assert_eq!(prefilled, Err(PrefillError::Off));
}
