//! One fused batch on the device: planned at sf40 tp1_single, fused under the hold rule and the
//! cap, its union prefilled, warmed, timed `MEASURED_RUNS` times (`FUSED_ALL_RUNS` for `all`),
//! every answer checked against its query run alone, and its section and record rows written.
//! The protocol is the tree benchmark's (`corpus_benchmark::measure`), over several roots.

use std::path::PathBuf;
use std::time::Instant;

use datafusion::arrow::array::RecordBatch;

use crate::executor::{
    FusedRun, NodeTiming, PrefillError, PrefillStats, Region, RmmPool, RunError,
    apply_pinned_host_cache, join_regions, nodes_as_recorded_of_roots, nvtx_range,
    pinned_host_cache, plan_files, set_node_timing, set_nvtx_ranges,
};
use crate::plan::{Batching, GpuNode};
use crate::plan_text::{render_timings_of_roots, timed_device_us, timed_load_us_of_roots};
use crate::prototypes::FUSED_ALL_RUNS;
use crate::test_support::{
    Capture, CaseOutcome, MEASURED_RUNS, MODES, RECORD_PATH_ENV, Regeneration, Resume, RunMeta,
    RunTotals, answer_on_the_device, assert_dataset_present, assert_fit_to_measure,
    assert_results_match, declared_steps, device_slug, install_bench_pool, measure_into,
    merge_section, plan_at, prefill_for_roots, provenance_of_this_run, record_rows, results_file,
    rounds_were_pinned, rows_match_the_recipes, run_roots_timed, run_section,
};
use crate::wire::{attach_recipes_for_roots, join_scratch_bytes};

use super::super::{
    CapDecision, DEVICE, Fused, HoldLedger, MergeHint, Outcome, PipelineHint, Query, batches, fuse,
    fused_results_file, is_out_of_memory, merge_hint, ordered_queries, pipeline_hint,
    pipeline_inputs, read_load_times, render_fused_section, settle,
};
use super::{baseline_for, batch_meta, case_range, declared_batches, holds_this_run};
use super::{load_times_path, out_of_memory_run, record_path_for, scan_seqs};
use super::{second_smallest_at, write_batch_record};

/// One timed run: its wall time, what it answered and reported, and the device's regions.
struct Run {
    total_us: u64,
    fused: FusedRun,
    regions: Vec<Region>,
}

pub(crate) async fn benchmark_fused(dataset: &str, sf: &str, batch: &str) {
    assert_eq!(
        sf, "40",
        "the fuser runs at sf40 only: its rules read sf40-baseline's files"
    );
    let what = format!("{dataset} {batch} fused at tp1-single on a device");
    let capture = Capture::from_env();
    let resume = Resume::from_env();
    let provenance = provenance_of_this_run(&what);
    let pool = install_bench_pool();
    let allocator = pool.to_string();
    set_node_timing(NodeTiming::Events);
    assert_fit_to_measure(&what);
    assert_dataset_present(dataset, sf, &what);
    let RmmPool::Pool { maximum_bytes, .. } = pool else {
        unreachable!("assert_fit_to_measure refused rmm's default resource")
    };

    let list = ordered_queries(dataset);
    let declared = declared_batches(&list);
    let file = fused_results_file(&device_slug(&provenance.device), dataset, sf);
    let names = match batches(&list).into_iter().find(|(name, _)| *name == batch) {
        Some((_, Ok(names))) => names,
        Some((_, Err(_))) => {
            let (_, marker) = declared
                .iter()
                .find(|(name, _)| name == batch)
                .expect("declared");
            let marker = marker
                .as_ref()
                .expect("a batch the list does not exceed is a marker");
            merge_section(&file, &declared, batch, marker, Regeneration::Sections);
            println!("{what}: {marker}");
            return;
        }
        None => panic!("{batch} is no batch: the batches are first3, first10, first20 and all"),
    };
    if resume == Resume::KeepMeasured
        && std::fs::read_to_string(&file)
            .is_ok_and(|text| holds_this_run(&text, batch, &provenance))
    {
        println!(
            "{what}: kept, {} holds its section from this run",
            file.display()
        );
        return;
    }

    let meta = batch_meta(dataset, sf, batch, &allocator, capture, &provenance);
    let env = std::env::var_os(RECORD_PATH_ENV).map(PathBuf::from);
    let record = record_path_for(capture, env.as_deref(), dataset, sf, batch);
    let write = |rows: &[String]| {
        if let Some(path) = &record {
            write_batch_record(path, rows, &meta)
        }
    };
    let measured = measure(dataset, sf, batch, &names, maximum_bytes, &meta, &what);
    // A captured run writes no section: its times were taken under the profiler.
    if capture != Capture::None {
        let (_, rows) = measured.await;
        write(&rows);
        return;
    }
    let rows = |rows: Vec<String>| write(&rows);
    match measure_into(
        &file,
        &declared,
        batch,
        &provenance,
        Resume::No,
        measured,
        rows,
    )
    .await
    {
        CaseOutcome::Measured => {}
        CaseOutcome::Held => unreachable!("a resumed batch is kept above"),
        CaseOutcome::Failed(reason) => {
            panic!("{what} failed, recorded in {}: {reason}", file.display())
        }
    }
}

/// The batch's section body and its record rows. Everything here runs inside `measure_into`, so
/// a panic in it, a refused prefill or a wrong answer, is the batch's recorded failure.
async fn measure(
    dataset: &str,
    sf: &str,
    batch: &str,
    names: &[String],
    pool_bytes: u64,
    meta: &RunMeta<'_>,
    what: &str,
) -> (String, Vec<String>) {
    let mode = &MODES[0];
    let mut planned = Vec::with_capacity(names.len());
    for name in names {
        planned.push(plan_at(dataset, sf, name, mode).await);
    }
    apply_pinned_host_cache(planned[0].0.state().config());
    let join_scratch = join_scratch_bytes(&mode.knobs_for(dataset));
    let trees: Vec<&dyn GpuNode> = planned.iter().map(|(_, tree)| tree.as_ref()).collect();
    let Prepared {
        fused,
        decision,
        merge,
        pipeline,
    } = prepare(dataset, sf, names, &trees, join_scratch, pool_bytes, what);
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let section = |outcome: Outcome<'_>| {
        render_fused_section(
            names, &fused, &decision, pool_bytes, &merge, &pipeline, outcome,
        )
    };

    let files: Vec<PathBuf> = plan_files(&roots).into_iter().collect();
    let prefill = match prefill_for_roots(&roots, join_scratch, &files, what) {
        Ok(stats) => stats,
        Err(PrefillError::Off) => PrefillStats::default(),
        Err(error) => panic!("prefill: {error}"),
    };
    eprintln!(
        "[pinned-cache] prefill {dataset} {batch}: files={} bytes={} us={} needed={} limit={}",
        prefill.files, prefill.bytes, prefill.us, prefill.needed, prefill.limit
    );
    let out_of_memory = |error: RunError| {
        let run = out_of_memory_run(meta.provenance);
        let reason = error.to_string();
        eprintln!("{what}: out of memory under cap {}: {reason}", decision.cap);
        (
            section(Outcome::OutOfMemory {
                reason: &reason,
                run: &run,
            }),
            Vec::new(),
        )
    };
    let cap = decision.cap;

    // Off across the warm-up and on for the measured runs under a capture, as the tree
    // benchmark does: the record holds the measured runs only.
    set_nvtx_ranges(false);
    let before = pinned_host_cache();
    match run_roots_timed(&roots, join_scratch, cap, what) {
        Ok(_) => {}
        Err(error) if is_out_of_memory(&error) => return out_of_memory(error),
        Err(error) => panic!("{what}, warm-up: {error}"),
    }
    set_nvtx_ranges(meta.capture != Capture::None);
    let in_case = nvtx_range(&case_range(dataset, sf, batch));
    let wanted = if batch == "all" {
        FUSED_ALL_RUNS
    } else {
        MEASURED_RUNS
    };
    let mut runs = Vec::with_capacity(wanted);
    for _ in 0..wanted {
        let started = Instant::now();
        match run_roots_timed(&roots, join_scratch, cap, what) {
            Ok((fused, regions)) => runs.push(Run {
                total_us: started.elapsed().as_micros() as u64,
                fused,
                regions,
            }),
            Err(error) if is_out_of_memory(&error) => return out_of_memory(error),
            Err(error) => panic!("{what}: {error}"),
        }
    }
    drop(in_case);
    // The answer checks below run each query alone, and their calls belong to no case.
    set_nvtx_ranges(false);
    let after = pinned_host_cache();
    rounds_were_pinned(&before, &after).unwrap_or_else(|moved| panic!("{moved}"));

    let times: Vec<u64> = runs.iter().map(|run| run.total_us).collect();
    let nodes = nodes_as_recorded_of_roots(&roots).unwrap_or_else(|e| panic!("{what}: {e}"));
    let recipes =
        attach_recipes_for_roots(&roots, join_scratch).unwrap_or_else(|e| panic!("{what}: {e}"));
    let declared = declared_steps(&recipes);
    let costed_of = |run: &Run| {
        join_regions(&run.fused.report, &run.regions)
            .unwrap_or_else(|refused| panic!("{what}: {refused}"))
    };
    let rows: Vec<String> = runs
        .iter()
        .enumerate()
        .flat_map(|(at, run)| {
            let rows = record_rows(&nodes, &run.fused.report, &costed_of(run), meta, at);
            rows_match_the_recipes(&rows, &declared)
                .unwrap_or_else(|e| panic!("{what}: the record disagrees with the plan: {e}"));
            rows
        })
        .collect();

    let chosen = runs.swap_remove(second_smallest_at(&times));
    let costed = costed_of(&chosen);
    let totals = RunTotals {
        run_us: chosen.total_us,
        device_us: timed_device_us(&costed),
        load_us: timed_load_us_of_roots(&roots, &costed),
        peak_bytes: chosen.fused.report.peak_bytes as u64,
    };
    check_answers(&trees, names, &chosen.fused.answers, join_scratch, what);
    let tree = render_timings_of_roots(&roots, &costed, &chosen.fused.report);
    let run = run_section(
        &totals,
        &times,
        meta.allocator,
        &after,
        &prefill,
        meta.provenance,
    );
    println!(
        "{what}: {}us of {times:?}, cap {cap}, load {}us, execute {}us",
        totals.run_us,
        totals.load_us,
        totals.device_us - totals.load_us
    );
    let outcome = Outcome::Measured {
        tree: &tree,
        measured_us: totals.device_us,
        run: &run,
    };
    (section(outcome), rows)
}

/// A batch fused under the hold rule and settled under its cap, with the two hints.
struct Prepared {
    fused: Fused,
    decision: CapDecision,
    merge: MergeHint,
    pipeline: PipelineHint,
}

/// `trees` fused and settled against the sf40 baseline and `loads.tsv`; a plan that is not the
/// baseline's, or does not fuse or settle, panics naming why.
fn prepare(
    dataset: &str,
    sf: &str,
    names: &[String],
    trees: &[&dyn GpuNode],
    join_scratch: u64,
    pool_bytes: u64,
    what: &str,
) -> Prepared {
    let mode = &MODES[0];
    let queries: Vec<Query<'_>> = names
        .iter()
        .zip(trees)
        .map(|(name, tree)| Query {
            name: name.clone(),
            tree: *tree,
        })
        .collect();
    let tree_file = results_file(DEVICE, dataset, sf, &mode.ident());
    let text = std::fs::read_to_string(&tree_file)
        .unwrap_or_else(|why| panic!("{}: the baseline: {why}", tree_file.display()));
    let baseline = baseline_for(&text, names);
    baseline
        .check(&queries)
        .unwrap_or_else(|why| panic!("{what}: the sf40 plans are not the baseline's: {why}"));
    let loads = read_load_times(&load_times_path(dataset, sf), dataset, sf, mode.name);
    let seqs: Vec<Vec<Option<u64>>> = trees
        .iter()
        .map(|tree| scan_seqs(*tree, join_scratch))
        .collect();
    let mut ledger = HoldLedger::new(pool_bytes, &baseline, &loads, &seqs, names);
    let fused = fuse(&queries, Batching::Off, &mut |fused, held| {
        ledger.choose(fused, held)
    })
    .unwrap_or_else(|why| panic!("{what}: does not fuse: {why}"));
    let (fused, decision) = settle(fused, &baseline, &mut ledger)
        .unwrap_or_else(|why| panic!("{what}: does not settle under its cap: {why}"));
    let merge = merge_hint(&fused, &baseline);
    let (load_us, needs) = pipeline_inputs(&fused, &baseline);
    let pipeline = pipeline_hint(&load_us, &needs);
    Prepared {
        fused,
        decision,
        merge,
        pipeline,
    }
}

/// Every root's answer against its query run alone on the same device; a mismatch panics.
fn check_answers(
    trees: &[&dyn GpuNode],
    names: &[String],
    answers: &[Vec<crate::executor::CpuBatch>],
    join_scratch: u64,
    what: &str,
) {
    for ((tree, name), answer) in trees.iter().zip(names).zip(answers) {
        let what = format!("{what}: {name}");
        let alone = answer_on_the_device(*tree, join_scratch, &what);
        let fused: Vec<RecordBatch> = answer
            .iter()
            .map(|batch| batch.record_batch().clone())
            .collect();
        assert_results_match(&alone, &fused, Some(1e-9), &what);
    }
}
