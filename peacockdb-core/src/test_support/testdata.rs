//! Where the testdata tree is, and the one place that answers it.
//!
//! A test binary is built on one host and run on another — remote CPU runs ship binaries,
//! goldens and data but never source — so the compile-time path is the fallback and the
//! environment wins (#49). The checkout itself is named here too, once, for the
//! `UPDATE_CANONICAL` rewrites that run where the source is, and only to be written: no path
//! into it leaves this file, so no test can read the source through it.

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

pub(crate) fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

/// Replaces `rel` under `peacockdb-core/` in the checkout the binary was built from with
/// `text`. Only for `UPDATE_CANONICAL`, which a staged host never sets.
pub(crate) fn rewrite_source_file(rel: &str, text: &str) {
    rewrite(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel), text)
}

/// `text` staged as `<path>.new` and renamed over `path`, so a reader sees the old file or the
/// new one, never a part.
fn rewrite(path: &Path, text: &str) {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".new");
    std::fs::write(&staged, text).unwrap_or_else(|e| panic!("staging {}: {e}", path.display()));
    std::fs::rename(&staged, path).unwrap_or_else(|e| panic!("publishing {}: {e}", path.display()));
}
