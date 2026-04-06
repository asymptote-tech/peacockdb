#[cfg(not(feature = "rust-only"))]
mod ffi_tests {
    use peacockdb_ffi::raw::{
        peacock_executor_create, peacock_executor_destroy, PeacockExecutor,
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
        let ret = unsafe {
            peacock_executor_create(2 * 1024 * 1024 * 1024, &mut executor)
        };
        assert_eq!(ret, 0, "peacock_executor_create failed with code {ret}");
        assert!(!executor.is_null(), "executor pointer should be non-null");
        unsafe { peacock_executor_destroy(executor) };
    }
}
