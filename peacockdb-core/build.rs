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

    emit_build_profile(&out_dir);
}

/// Bake how this crate was compiled into it, for `peacock_gpu_benchmarks` to write
/// into every measurement record (`build_profile=`).
///
/// The harness refuses a non-release build, so this says WHICH release profile. Baked at
/// compile time because it is the one condition of a run the process cannot ask about.
///
/// Cargo hands a build script `OPT_LEVEL` but not the profile NAME — `PROFILE` collapses
/// every release-inheriting profile to "release". The profile directory carries it, and
/// `OUT_DIR` is `<target>/<profile-dir>/build/<pkg>-<hash>/out`, hence the fourth
/// ancestor. Unknown rather than a panic, a build script being the wrong place to fail.
fn emit_build_profile(out_dir: &std::path::Path) {
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let opt_level = env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=PEACOCK_BUILD_PROFILE={profile_dir}");
    println!("cargo:rustc-env=PEACOCK_BUILD_OPT_LEVEL={opt_level}");
}
