//! One benchmark case at one mode: plan it, run it, keep what it took.
//!
//! The timing counterpart of `corpus_gpu`, and deliberately not part of it: that module
//! holds a device run to what the cpu wrote, and this one compares nothing at all. Sharing
//! a module would put an assertion path and a measurement path behind one door.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::executor::{
    GpuBackend, Measurements, NodeTiming, Region, RmmPool, RunReport, install_rmm_pool,
    join_regions, node_measured, nodes_as_recorded, nvtx_range, run, set_node_timing,
    set_nvtx_ranges,
};
use crate::plan::GpuNode;
use crate::plan_text::{render_timings, timed_device_us};
use crate::wire::{attach_recipes, join_scratch_bytes};

use super::benchmark_section::{assert_dataset_present, measure_into};
use super::benchmark_trailer::run_section;
use super::corpus::plan_at;
use super::gpu_session::Session;
use super::provenance::{device_slug, of_this_run};
use super::record::{
    append_records, declared_steps, heading_mismatch, record_rows, rows_match_the_recipes,
};
use super::{
    BUILD, BenchmarkCase, Capture, CaseOutcome, MEASURED_RUNS, Mode, NOT_TIMED, RECORD_PATH_ENV,
    RESUME_ENV, Resume, RunMeta, SKIPPED, mode_named, stem, testdata_root,
};

/// The sections a `(dataset, sf, mode)` file is declared to hold: the queries timed at
/// this mode, and a marker for each one declared and timed at none.
///
/// Sorted numerically within the letter prefix, so `q2` precedes `q10`. `inventory` gives
/// no order of its own — it is link order — and a file whose sections move between builds
/// would diff against itself.
pub(crate) fn declared_for(dataset: &str, sf: &str, mode: &str) -> Vec<(String, Option<String>)> {
    let mut declared: Vec<(String, Option<String>)> = inventory::iter::<BenchmarkCase>
        .into_iter()
        .filter(|case| case.dataset == dataset && case.sf == sf)
        .filter(|case| case.mode == mode || case.mode == NOT_TIMED)
        .map(|case| {
            let marker = (case.mode == NOT_TIMED)
                .then(|| format!("{SKIPPED}declared with no mode to time it at"));
            (stem(case.query), marker)
        })
        .collect();
    declared.sort_by(|(a, _), (b, _)| query_order(a).cmp(&query_order(b)));
    declared.dedup_by(|(a, _), (b, _)| a == b);
    declared
}

/// `testdata/benchmark-results/<device>/<dataset>.sf<sf>/<mode>.benchmark.txt` — one file
/// per (device, dataset, mode), holding a section per query it timed. `device` is a slug.
///
/// One file rather than one per query because a mode's queries are read together, and a
/// directory of one-query files makes that a listing rather than a diff.
pub(crate) fn results_file(device: &str, dataset: &str, sf: &str, mode: &str) -> PathBuf {
    testdata_root()
        .join(format!("benchmark-results/{device}/{dataset}.sf{sf}"))
        .join(format!("{mode}.benchmark.txt"))
}

/// A query name as `(prefix, number, whole)`, so `q2` sorts before `q10` and a name with
/// no number in it still has a total order.
fn query_order(query: &str) -> (String, u32, String) {
    let digits = query.trim_start_matches(|c: char| !c.is_ascii_digit());
    let head = &query[..query.len() - digits.len()];
    let number: String = digits.chars().take_while(char::is_ascii_digit).collect();
    (
        head.to_string(),
        number.parse().unwrap_or(0),
        query.to_string(),
    )
}

/// The pool this binary reserves, the way each gtest binary declares its own `kPoolBytes`:
/// bytes it asks for, never a share of the device. Same dataset read in the same place as
/// `test_tpch.cpp`, whose measured sf40 peak is 67.42 GiB, and 69 is also the most that
/// lets two processes share the 139.7 GiB H200 (#178) — a gate job can land beside a
/// measurement. `PEACOCK_RMM_POOL_BYTES` overrides it for a sweep, and the sf40 run that
/// re-measures the tree is where this binary's own peak gets read.
const BENCH_POOL_BYTES: u64 = 69 << 30;

/// Discarded runs. The first execution pays for the page cache, CUDA module load and JIT,
/// and allocator growth — the host's recent history rather than the plan. The pool removes
/// most of the third before this runs, which is a reason to keep the warm-up: what is left
/// is the part that varies.
const BENCH_WARMUP_RUNS: usize = 1;

/// One measured execution.
struct Run {
    total_us: u64,
    report: RunReport,
    /// What the device answered with, drained before the session that recorded it closed.
    regions: Vec<Region>,
}

/// Time `query` at `mode`.
///
/// Takes the mode as its macro spelling (`tp4_sized`) rather than a `&Mode`, so a
/// case-list line and a call site read alike; `mode_named` resolves it and panics naming
/// the five when it is not one of them.
///
/// A plain run goes through `measure_into`: a case that panics is recorded in its section and
/// then fails, so the run goes on to the next, and under `PEACOCK_BENCH_RESUME=1` a section
/// this run's device, driver, CUDA, cuDF and host measured is kept and its case not run. A
/// captured run writes no section, so it measures every case and a panic is only a panic.
pub(crate) async fn benchmark_case(dataset: &str, sf: &str, query: &str, mode: &str) {
    let mode = mode_named(mode);
    let what = format!("{dataset}/{query} at {} on a device", mode.name);
    // Read before anything runs, so a misspelled value fails the case rather than the
    // publish at the end of it.
    let capture = Capture::from_env();
    let resume = Resume::from_env();
    let provenance = of_this_run(&what);

    // Conditions of a benchmark run, not choices: rmm's default makes every cuDF
    // intermediate a cudaMalloc/cudaFree round trip charged to the node that allocated it,
    // and a draining measurement reports a schedule the engine does not run. Installing
    // the pool first because rmm takes whatever resource is current when it allocates.
    let allocator = install_rmm_pool(BENCH_POOL_BYTES).to_string();
    set_node_timing(NodeTiming::Events);
    let meta = RunMeta {
        dataset,
        sf,
        query,
        mode: mode.name,
        allocator: &allocator,
        capture,
        provenance: &provenance,
    };
    // Outside the failure record, each of them: a run-level fault fails every case alike, and
    // recorded it would write one refusal into every section of the tree.
    assert_fit_to_measure(&what);
    assert_dataset_present(dataset, sf, &what);
    if let Some(mismatch) =
        std::env::var_os(RECORD_PATH_ENV).and_then(|path| heading_mismatch(Path::new(&path), &meta))
    {
        panic!("{what}: {mismatch}");
    }

    let case = measure(dataset, sf, query, mode, &meta, &what);
    if capture != Capture::None {
        let (_, rows) = case.await;
        append_records(&rows, &meta);
        return;
    }
    let ident = mode.ident();
    let file = results_file(&device_slug(&provenance.device), dataset, sf, &ident);
    let declared = declared_for(dataset, sf, &ident);
    // The rows before the section: the section is what a resumed run keeps, so a crash between
    // the two leaves the case to be measured again rather than kept with no rows.
    let append = |rows: Vec<String>| append_records(&rows, &meta);
    match measure_into(&file, &declared, query, &provenance, resume, case, append).await {
        CaseOutcome::Measured => {}
        CaseOutcome::Held => println!(
            "{what}: kept, {} holds its section from this device, driver, CUDA, cuDF and host \
             and {RESUME_ENV}=1",
            file.display()
        ),
        CaseOutcome::Failed(reason) => {
            panic!("{what} failed, recorded in {}: {reason}", file.display())
        }
    }
}

/// One case planned, warmed and run `MEASURED_RUNS` times: the section's body, and the
/// record's rows for the caller to append before the section is written.
///
/// Planning happens once, outside the runs: `plan_at` reads every file's parquet
/// metadata, and repeating that would time the catalog rather than the query. So
/// `total_us` here is execution only.
async fn measure(
    dataset: &str,
    sf: &str,
    query: &str,
    mode: &Mode,
    meta: &RunMeta<'_>,
    what: &str,
) -> (String, Vec<String>) {
    const _: () = assert!(MEASURED_RUNS >= 2, "a second minimum needs >= 2 runs");

    let (_ctx, tree) = plan_at(dataset, sf, query, mode).await;
    let join_scratch = join_scratch_bytes(&mode.knobs_for(dataset));

    // Off across the warm-up, and set rather than assumed: the switch is process-global
    // and this binary runs every case in one process, so a case that only turned it on
    // would leave later warm-ups ranging with no case range open — calls belonging to no
    // query. The capture's containment check catches exactly that, and did.
    set_nvtx_ranges(false);
    for _ in 0..BENCH_WARMUP_RUNS {
        run_once(tree.as_ref(), join_scratch, what);
    }
    // After the warm-up, not before: the warm-up is not written to the record, so ranging
    // it would leave the capture one execution longer than the file it joins against —
    // and the reader would have to know a Rust constant to allow for it.
    set_nvtx_ranges(meta.capture != Capture::None);
    // Around the measured runs, naming the case. A node range carries `seq`, and seq
    // numbering restarts per plan — q6 and q19 both open with `0.0 CudfScan` — so only
    // containment says which query a call was in. Held to the end of the function:
    // dropping it here would close the range before the runs it is meant to contain.
    let _case = nvtx_range(&format!("{dataset}.sf{sf} {query} {}", mode.name));

    let mut runs = Vec::with_capacity(MEASURED_RUNS);
    for _ in 0..MEASURED_RUNS {
        let started = Instant::now();
        let (report, regions) = run_once(tree.as_ref(), join_scratch, what);
        // Read after the run rather than inside it: `GpuUnload` copies the root off the
        // device, so the driver has returned only once the device is actually finished.
        // Under events nothing else would guarantee that — the node walk returns while
        // the stream may still be running.
        runs.push(Run {
            total_us: started.elapsed().as_micros() as u64,
            report,
            regions,
        });
    }

    let times: Vec<u64> = runs.iter().map(|run| run.total_us).collect();
    // Every run, not the chosen one: a spread of ten separates a call's cost from an
    // accident of scheduling, while the file beside it reports one run. And in the order
    // they ran, which `second_smallest` is about to destroy — a repeat of `call_index` 0
    // is where one execution's rows end, and sorting first interleaves them for good.
    let nodes = nodes_as_recorded(tree.as_ref()).unwrap_or_else(|e| panic!("{what}: {e}"));
    // Attached once more here rather than reached for through the session: `Session::open`
    // hands its plan to the driver and the driver consumes it. `attach_recipes` reads the
    // finished tree and builds a buffer, which is host work outside every measured run.
    let recipes =
        attach_recipes(tree.as_ref(), join_scratch).unwrap_or_else(|e| panic!("{what}: {e}"));
    let declared = declared_steps(&recipes);
    // One append for the case rather than one per run: the file is opened, its heading
    // checked and its lock taken each time, and ten of that says nothing ten times.
    let rows: Vec<String> = runs
        .iter()
        .enumerate()
        .flat_map(|(at, run)| {
            let rows = record_rows(&nodes, &run.report, &measured_of(run, what), meta, at);
            // Per run, before they are flattened: the check reads one execution's
            // `call_index` sequences, and ten concatenated executions repeat every one.
            rows_match_the_recipes(&rows, &declared)
                .unwrap_or_else(|e| panic!("{what}: the record disagrees with the plan: {e}"));
            rows
        })
        .collect();

    let chosen = second_smallest(runs);
    let costed = measured_of(&chosen, what);
    let body = format!(
        "{}{}",
        render_timings(tree.as_ref(), &costed),
        run_section(
            chosen.total_us,
            timed_device_us(&costed),
            &times,
            meta.allocator,
            meta.provenance
        )
    );

    let per_node: Vec<String> = (0..costed.nodes())
        .map(|node| match node_measured(&costed, node) {
            // Three states again: not measured, measured but addressing no seq (an unload
            // exports through a door that opens no region), and measured with regions.
            None => "unmeasured".to_string(),
            Some(t) if t.regions == 0 => "no regions".to_string(),
            Some(t) => format!("{}/{}", t.host_us, t.device_us),
        })
        .collect();
    // Printed until there is a file to write it to. Every time, not just the chosen one: a
    // second minimum says nothing about the spread it was picked out of.
    println!(
        "{what}: {}us of {times:?}, {} nodes, {} regions, per-node host/device {per_node:?}",
        chosen.total_us,
        chosen.report.emitted.len(),
        chosen.regions.len()
    );
    (body, rows)
}

/// One run's calls costed.
///
/// A mismatch in either direction means the two sides disagree about what ran — a defect
/// in the join, not a number to report around — so the panic is the report.
fn measured_of(run: &Run, what: &str) -> Measurements {
    join_regions(&run.report, &run.regions).unwrap_or_else(|refused| {
        panic!(
            "{what}: {refused} ({} regions came back)",
            run.regions.len()
        )
    })
}

/// The run worth reporting: second-smallest by end-to-end time.
///
/// The minimum is the run most likely to have caught a favourable scheduling accident,
/// and the rest are dragged up by whatever else the shared host was doing.
///
/// A whole run rather than a per-node minimum across runs: the latter gives a tree
/// belonging to no single execution, which can sum to less than any of them.
fn second_smallest(mut runs: Vec<Run>) -> Run {
    runs.sort_by_key(|run| run.total_us);
    runs.swap_remove(1)
}

/// The two conditions the harness refuses to measure without: rmm's pool, and a release
/// build. `install_rmm_pool` is idempotent, so asking it is asking what the resource is.
fn assert_fit_to_measure(what: &str) {
    assert!(
        matches!(install_rmm_pool(BENCH_POOL_BYTES), RmmPool::Pool { .. }),
        "{what} would measure over rmm's default resource, where every cuDF intermediate is \
         a cudaMalloc/cudaFree round trip charged to the node that allocated it — the \
         numbers would describe the allocator, not the plan"
    );
    // The same kind of statement: the record's `build=` line is a claim until something
    // refuses the build that would make it false. A plain `cargo test` compiles this at
    // opt-level 1, where the host prologue is a different quantity entirely.
    assert!(
        !cfg!(debug_assertions),
        "{what} would measure a debug build, which writes `build={BUILD}` about a host \
         prologue that is not release's; build it with \
         `scripts/build-test-shadgpu.sh --build-benchmarks`"
    );
}

/// One device session and one run over it.
///
/// The session is per run rather than per case: `attach_recipes` and `begin_plan` are what
/// a query costs on this side of the FFI, and holding one across runs would time the
/// second differently from the first.
///
/// No budget, so the accountant records without ever tripping — a benchmark that refuses
/// to finish reports nothing, and the mode's budget already sized the batches at plan time.
fn run_once(tree: &dyn GpuNode, join_scratch: u64, what: &str) -> (RunReport, Vec<Region>) {
    // In the measured path rather than beside the install, which is the whole point: the
    // install was once lost with the file that held it, and a check standing next to what
    // it guards goes the same way.
    assert_fit_to_measure(what);
    let mut session = Session::open(tree, join_scratch, what);
    let ctx = session.context();
    let report = run::<GpuBackend>(tree, &ctx, None).unwrap_or_else(|e| panic!("{what}: {e}"));

    // Checked on every run, not on the reported one: a leak surfaces later as a case
    // timing a device that is still holding batches, and by then it names the wrong query.
    assert_eq!(report.in_flight_bytes, 0, "{what} ended holding batches");
    assert_eq!(
        report.holds, report.releases,
        "{what} held {} batches and released {}",
        report.holds, report.releases
    );
    // Drained here rather than by the caller: the events die with the session, and the
    // session is this function's.
    let regions = session.regions(what);
    (report, regions)
}
