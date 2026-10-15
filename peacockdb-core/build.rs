use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let schema = workspace_root.join("flatbuffers/gpu_plan.fbs");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed={}", schema.display());

    // Use the vendored flatc binary built by the flatc-fork crate.
    let flatc = flatc_fork::flatc();

    let status = Command::new(flatc)
        .args(["--rust", "-o"])
        .arg(&out_dir)
        .arg(&schema)
        .status()
        .unwrap_or_else(|e| panic!("failed to run flatc: {e}"));

    assert!(status.success(), "flatc failed with {status}");

    copy_cudf_version_config(&out_dir);
}

/// cuDF's `version_config.hpp` into the build, so the test harness can stamp a recorded device
/// answer with the cuDF it was produced under rather than with the value an operator typed.
/// Copied rather than parsed here: one reader, `test_support::device_answer`, which has tests.
///
/// Empty where there is no cuDF to read. A rust-only build has none and must not start
/// depending on `CUDF_ROOT` — that would re-run this script and recompile the rust-only tree
/// whenever the variable moves. A from-source build (`CUDF_BUILD_FROM_SOURCE=1`) leaves the
/// header in the cmake tree, at a path this cannot know; the write path says so by name.
fn copy_cudf_version_config(out_dir: &Path) {
    let into = out_dir.join("cudf-version-config.h");
    let cudf_root = match env::var_os("CARGO_FEATURE_RUST_ONLY") {
        Some(_) => None,
        None => {
            println!("cargo:rerun-if-env-changed=CUDF_ROOT");
            env::var_os("CUDF_ROOT")
        }
    };
    let Some(root) = cudf_root else {
        std::fs::write(&into, "").expect("the empty cuDF version config");
        return;
    };
    let header = PathBuf::from(root).join("include/cudf/version_config.hpp");
    println!("cargo:rerun-if-changed={}", header.display());
    std::fs::copy(&header, &into)
        .unwrap_or_else(|e| panic!("cannot copy {}: {e}", header.display()));
}
