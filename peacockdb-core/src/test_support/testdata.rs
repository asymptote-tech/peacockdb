//! Where the testdata tree is, and the one place that answers it.
//!
//! A test binary is built on one host and run on another — remote CPU runs ship binaries,
//! goldens and data but never source — so the compile-time path is the fallback and the
//! environment wins (#49).

use std::path::PathBuf;

pub(crate) fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}
