use std::env;
use std::path::PathBuf;
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
}
