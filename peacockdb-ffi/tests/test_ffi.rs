#[cfg(not(feature = "rust-only"))]
mod ffi_tests {
    use peacockdb_ffi::raw::{
        PEACOCK_PINNED_CACHE_DEFAULT_BYTES, PEACOCK_PINNED_CACHE_NO_DEVICE,
        PEACOCK_PREFILL_BAD_ARGUMENT, PEACOCK_PREFILL_OFF, PEACOCK_RMM_POOL_UNAVAILABLE,
        PeacockDeviceInfo, PeacockExecutor, PeacockPinnedCacheStats, PeacockPrefillStats,
        PeacockRmmPoolInfo, peacock_device_info, peacock_executor_create, peacock_executor_destroy,
        peacock_install_rmm_pool,
        peacock_pinned_cache_prefill, peacock_pinned_host_cache_stats,
        peacock_set_pinned_host_cache_bytes,
    };
    use peacockdb_ffi::version;
    use std::ffi::CString;
    use std::ptr;

    #[test]
    fn test_version_is_nonempty() {
        let v = version();
        assert!(!v.is_empty(), "version string should not be empty");
        println!("peacock_gpu version: {v}");
    }

    #[test]
    fn test_executor_lifecycle() {
        let mut executor: *mut PeacockExecutor = ptr::null_mut();
        let ret = unsafe { peacock_executor_create(2 * 1024 * 1024 * 1024, &mut executor) };
        assert_eq!(ret, 0, "peacock_executor_create failed with code {ret}");
        assert!(!executor.is_null(), "executor pointer should be non-null");
        unsafe { peacock_executor_destroy(executor) };
    }

    /// A zero-byte request is not a pool: rmm builds one and then fails every allocation
    /// in it with "Maximum pool size exceeded", so INSTALLED would hand the caller a
    /// device resource that cannot serve anything.
    #[test]
    fn test_install_rmm_pool_rejects_a_zero_request() {
        let mut info = PeacockRmmPoolInfo::default();
        let ret = unsafe { peacock_install_rmm_pool(0, &mut info) };
        assert_eq!(ret, 0, "peacock_install_rmm_pool failed with code {ret}");
        assert_eq!(
            info.state, PEACOCK_RMM_POOL_UNAVAILABLE,
            "a zero-byte request reported state {}",
            info.state
        );
    }

    /// The ffi rung has no device, which is CI's dataset-matrix and a container without
    /// `--gpus`: the cache is off, a prefill says so, and nothing throws across the ABI. On a
    /// host with a device only the cache's state and the argument refusals are checked here.
    /// One case, so libtest's other threads share no cache state with it.
    #[test]
    fn pinned_host_cache_is_off_without_a_device() {
        let mut stats = PeacockPinnedCacheStats::default();
        assert_eq!(unsafe { peacock_pinned_host_cache_stats(&mut stats) }, 0);
        let mut info = PeacockDeviceInfo {
            name: [0; 256],
            runtime: 0,
        };
        if unsafe { peacock_device_info(&mut info) } == 0 {
            assert_ne!(stats.state, PEACOCK_PINNED_CACHE_NO_DEVICE, "a device answered");
            assert_ne!(unsafe { peacock_pinned_host_cache_stats(ptr::null_mut()) }, 0);
            return;
        }
        assert_eq!(stats.state, PEACOCK_PINNED_CACHE_NO_DEVICE);
        assert_eq!(stats.capacity_bytes, 0);

        let rc = unsafe {
            peacock_set_pinned_host_cache_bytes(PEACOCK_PINNED_CACHE_DEFAULT_BYTES, &mut stats)
        };
        assert_eq!(rc, 0);
        assert_eq!(stats.state, PEACOCK_PINNED_CACHE_NO_DEVICE);
        assert_eq!(stats.capacity_bytes, 0);

        let file = std::env::temp_dir().join(format!("peacock-ffi-{}.bin", std::process::id()));
        std::fs::write(&file, [7u8; 4096]).expect("write the temp file");
        let path = CString::new(file.to_str().expect("a UTF-8 temp path")).expect("no NUL");
        let paths = [path.as_ptr()];
        let mut executor: *mut PeacockExecutor = ptr::null_mut();
        assert_eq!(unsafe { peacock_executor_create(0, &mut executor) }, 0);
        let mut prefill = PeacockPrefillStats::default();
        let rc = unsafe { peacock_pinned_cache_prefill(executor, paths.as_ptr(), 1, &mut prefill) };
        assert_eq!(rc, PEACOCK_PREFILL_OFF);
        assert_eq!(prefill.files, 0);

        assert_ne!(
            unsafe { peacock_pinned_host_cache_stats(ptr::null_mut()) },
            0
        );
        let rc =
            unsafe { peacock_pinned_cache_prefill(executor, paths.as_ptr(), 1, ptr::null_mut()) };
        assert_eq!(rc, PEACOCK_PREFILL_BAD_ARGUMENT);
        unsafe { peacock_executor_destroy(executor) };
        std::fs::remove_file(&file).expect("remove the temp file");
    }
}
