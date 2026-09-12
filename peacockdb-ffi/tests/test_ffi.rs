#[cfg(not(feature = "rust-only"))]
mod ffi_tests {
    use peacockdb_ffi::raw::{
        PEACOCK_RMM_POOL_UNAVAILABLE, PeacockExecutor, PeacockRmmPoolInfo, peacock_executor_create,
        peacock_executor_destroy, peacock_install_rmm_pool,
    };
    use peacockdb_ffi::version;
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
}
