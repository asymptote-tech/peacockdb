//! Where the testdata tree is, and the one place that answers it.
//!
//! A test binary is built on one host and run on another — remote CPU runs ship binaries,
//! goldens and data but never source — so the compile-time path is the fallback and the
//! environment wins (#49). The checkout itself is named here too, once, for the
//! `UPDATE_CANONICAL` rewrites that run where the source is.

use std::path::PathBuf;

pub(crate) fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

/// `rel` under `peacockdb-core/` in the checkout the binary was built from. Only for
/// `UPDATE_CANONICAL` writes: a staged host never sets it, and nothing reads through it.
pub(crate) fn source_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}
