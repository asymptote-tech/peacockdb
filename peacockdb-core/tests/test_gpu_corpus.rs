//! The corpus on a device: every query in [`corpus_cases.inc`](common/corpus_cases.inc) at
//! every mode its `gpu_modes` declares.
//!
//! The same declaration list the cpu binary reads, so the two engines' coverage is one line
//! per query. This binary never writes a golden — see `test_support`'s `corpus_gpu.rs`.
#![cfg(not(feature = "rust-only"))]

use peacockdb_core::test_support::{
    GpuRecording, RegistryEntry, assert_registry_matches_csv, cost_golden, cpu_golden, gpu_case,
    gpu_recording, gpu_result_golden, result_golden,
};

/// One test and one registration per enabled gpu mode of a declaration, given its oracles and
/// schema validation already rendered. A helper because the mask does not repeat per mode: used
/// inside the repetition over `$gpu`, a mask metavariable would have to repeat with it.
macro_rules! gpu_cases {
    ($dataset:ident, $sf:expr, $query:ident, $($gpu:ident)|+, $gpu_oracle:expr, $duck:expr, $validation:expr) => {
        $(
            paste::paste! {
                #[tokio::test]
                async fn [<gpu_ $dataset _ $query _ $gpu>]() {
                    gpu_case(
                        stringify!($dataset),
                        stringify!($sf),
                        &stringify!($query).replace('_', "-"),
                        stringify!($gpu),
                        $gpu_oracle,
                        $duck,
                        $validation,
                    )
                    .await;
                }
            }
            inventory::submit! {
                RegistryEntry {
                    kind: "gpu",
                    dataset: stringify!($dataset),
                    sf: stringify!($sf),
                    query: stringify!($query),
                    device: stringify!($gpu),
                    state: "enabled",
                }
            }
        )+
    };
}

/// The device's reading of a declaration: nothing at all for `none`, and otherwise the
/// cases above. The cpu arguments and the DuckDB oracle are consumed and dropped, which is
/// what makes one list serve both binaries — the DuckDB comparison is a rust-only case in the
/// cpu binary. The last argument says whether the run holds every batch to its node's
/// declared schema, at every mode or at all but the ones a `schema_validation_disabled(…)`
/// mask names; `gpu_case` decodes it. Two arms carry it because a mask is optional, and
/// `macro_rules` has no way to render an absent one.
macro_rules! corpus_query {
    ($dataset:ident, $sf:expr, $query:ident, all_modes, $($rest:tt)*) => {
        corpus_query!($dataset, $sf, $query,
            tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, all_modes, $($rest:tt)*) => {
        corpus_query!($dataset, $sf, $query, $($cpu)|+,
            tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, none, $($rest:tt)*) => {};
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:tt)*))?, $cpu_oracle:ident $(($($cpu_arg:tt)*))?,
     $gpu_oracle:ident $(($($gpu_arg:tt)*))?, $validation:ident ($($vmode:ident)|+)) => {
        gpu_cases!(
            $dataset, $sf, $query, $($gpu)|+, stringify!($gpu_oracle $(($($gpu_arg)*))?),
            stringify!($duck $(($($duck_arg)*))?), stringify!($validation($($vmode)|+))
        );
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:tt)*))?, $cpu_oracle:ident $(($($cpu_arg:tt)*))?,
     $gpu_oracle:ident $(($($gpu_arg:tt)*))?, $validation:ident) => {
        gpu_cases!(
            $dataset, $sf, $query, $($gpu)|+, stringify!($gpu_oracle $(($($gpu_arg)*))?),
            stringify!($duck $(($($duck_arg)*))?), stringify!($validation)
        );
    };
}

include!("common/corpus_cases.inc");

/// The device does not write a CPU golden, asserted on the real path rather than left to
/// the fact that nothing on it happens to call the write.
///
/// This binary links the write path through `test_support` exactly like the cpu one, and the
/// whole tier rests on the device being held to what the cpu wrote: a device that can author
/// its own golden proves nothing against it. So both regeneration variables are set, one real
/// device case runs, and the three cpu files it could have touched come back byte for byte.
///
/// Setting the environment is safe here and only here: the gpu job runs this binary with
/// `--test-threads=1`, since cuDF and RMM share one process-wide pool.
#[test]
fn a_device_run_under_a_regeneration_writes_no_golden() {
    let (dataset, sf, query, mode) = ("tpch", "1", "q6", "tp1_single");
    let mut files = vec![
        cpu_golden(dataset, sf, "tp1-single"),
        cost_golden(dataset, sf, "tp1-single"),
        result_golden(dataset, sf),
    ];
    // `gpu-result.txt` is the one file a device run MAY write, so it joins the snapshot only
    // on a cycle that is not recording — the same `GpuRecording` predicate the writer reads,
    // and not `var_os`: the gate script exports the variable into every binary, so on the
    // only host that runs this `var_os` is always `Some`.
    let recorded = gpu_result_golden(dataset, sf, None);
    if gpu_recording() == GpuRecording::No && recorded.exists() {
        files.push(recorded);
    }
    let before: Vec<Vec<u8>> = files
        .iter()
        .map(|path| std::fs::read(path).expect("a committed golden"))
        .collect();
    // SAFETY: `--test-threads=1` on this binary, which the gpu job and the shadgpu script
    // both pass and `test_source_formatting` holds them to.
    unsafe {
        std::env::set_var("UPDATE_CANONICAL", "1");
        std::env::set_var("PCK_UPDATE_SECTIONS", "1");
    }
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(gpu_case(
                dataset,
                sf,
                query,
                mode,
                "golden(sort, exact)",
                "duckdb(sort, exact)",
                "schema_validation_enabled",
            ));
    }));
    unsafe {
        std::env::remove_var("UPDATE_CANONICAL");
        std::env::remove_var("PCK_UPDATE_SECTIONS");
    }
    for (path, was) in files.iter().zip(before) {
        assert_eq!(
            std::fs::read(path).expect("the golden"),
            was,
            "the device wrote {} under a regeneration",
            path.display()
        );
    }
    assert!(ran.is_ok(), "the case itself failed, so it proved nothing");
}

/// The five `gpu_` columns against what this binary declares, in both directions. It
/// runs only on the gpu host, because that is where these registrations are linked — so a
/// gpu-column drift does not go red on the cpu leg, which is a consequence to know rather
/// than a gap to close: `inventory` collects per linked binary.
#[test]
fn the_registry_matches_the_gpu_corpus_in_both_directions() {
    assert_registry_matches_csv(&[
        "gpu_tp1_single",
        "gpu_tp1_rowgroup",
        "gpu_tp4_single",
        "gpu_tp4_rowgroup",
        "gpu_tp4_sized",
    ]);
}
