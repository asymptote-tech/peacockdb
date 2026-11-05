//! What a benchmark run does with one case's section, with no device: a case that fails is
//! recorded in its file and the next case still runs, a resumed run keeps what the interrupted
//! run already measured and any other run measures it again, and the run-level refusals that
//! come before any case.

use std::path::PathBuf;

use peacockdb_core::test_support::{
    Capture, CaseOutcome, FAILED, Provenance, RESUME_ENV, Resume, RunMeta, assert_dataset_present,
    measure_into, ordered_sections, record_header, record_heading_mismatch,
};

use super::benchmark::device_tree_problem;

fn this_run() -> Provenance {
    Provenance {
        device: "NVIDIA H100 80GB HBM3".to_string(),
        driver: "580.95.05".to_string(),
        cuda: "12.9".to_string(),
        cudf: "25.02.02".to_string(),
        host: "gpu-host".to_string(),
    }
}

/// A timed section's body as the harness writes one: a node with its batches, the trailer's
/// seventeen keys, the five last.
fn timed(provenance: &Provenance, run_us: u64) -> String {
    format!(
        "GpuUnload\n  time_us=[[{run_us}]] total_us={run_us} batch_rows=[[1]] batch_bytes=[[8]]\n\
         --- run ---\nrun_us={run_us}\ndevice_us={run_us}\nload_us=0\nexecute_us={run_us}\n\
         peak_bytes=8\nruns=[{}]\nbuild=release\nallocator=rmm-pool\npinned_cache=off no-device\n\
         prefill_files=0\nprefill_bytes=0\nprefill_us=0\n\
         device={}\ndriver={}\ncuda={}\ncudf={}\nhost={}\n",
        vec![run_us.to_string(); 10].join(","),
        provenance.device,
        provenance.driver,
        provenance.cuda,
        provenance.cudf,
        provenance.host,
    )
}

/// One mode's file in a temp tree under the device's slug, and the tree's root.
fn scratch_file(case: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("peacock-run-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let path = root.join("h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt");
    (root, path)
}

fn declared(queries: &[&str]) -> Vec<(String, Option<String>)> {
    queries
        .iter()
        .map(|query| (query.to_string(), None))
        .collect()
}

fn section(path: &PathBuf, query: &str) -> String {
    let text = std::fs::read_to_string(path).expect("the mode's file");
    ordered_sections(&text)
        .into_iter()
        .find(|(name, _)| name == query)
        .unwrap_or_else(|| panic!("{} has no section for {query}", path.display()))
        .1
}

/// `measure_into` with nothing to append before the section.
async fn into(
    path: &PathBuf,
    declared: &[(String, Option<String>)],
    query: &str,
    run: &Provenance,
    resume: Resume,
    case: impl Future<Output = (String, ())>,
) -> CaseOutcome {
    measure_into(path, declared, query, run, resume, case, |()| {}).await
}

/// A case that measured, as `measure` hands one back: the body, and nothing beside it.
async fn measured(body: String) -> (String, ()) {
    (body, ())
}

/// A case that panics, as a refusal on the device does.
async fn refused(reason: &str) -> (String, ()) {
    panic!("{reason}")
}

/// A case a resumed run must not run again.
async fn not_again(query: &str) -> (String, ()) {
    panic!("{query} was measured again")
}

/// The first case panics: its section says so, on one line, under the run's provenance, and
/// the second case is measured into the same file after it.
#[tokio::test]
async fn a_failed_case_is_recorded_and_the_run_goes_on() {
    let (root, path) = scratch_file("failed");
    let run = this_run();
    let both = declared(&["q1", "q6"]);

    let failing = refused("no kernel for\n  this join");
    let first = into(&path, &both, "q1", &run, Resume::No, failing).await;
    let passing = measured(timed(&run, 7));
    let second = into(&path, &both, "q6", &run, Resume::No, passing).await;

    assert_eq!(
        first,
        CaseOutcome::Failed("no kernel for this join".to_string())
    );
    assert_eq!(second, CaseOutcome::Measured);
    assert_eq!(
        section(&path, "q1"),
        format!(
            "{FAILED}no kernel for this join\ndevice=NVIDIA H100 80GB HBM3\ndriver=580.95.05\n\
             cuda=12.9\ncudf=25.02.02\nhost=gpu-host\n"
        )
    );
    assert_eq!(section(&path, "q6"), timed(&run, 7));
    let tree = device_tree_problem(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        tree,
        Ok(()),
        "a failed case's record is a well-formed section of its tree"
    );
}

/// A run interrupted after q1, resumed as `PEACOCK_BENCH_RESUME=1` asks: q1's section is kept
/// byte for byte without running it, and the rest run — q6, measured under another cuDF,
/// q17, which failed, and q19, never reached.
#[tokio::test]
async fn test_partial_run_keeps_completed_cases() {
    let (root, path) = scratch_file("partial");
    let run = this_run();
    let other_cudf = Provenance {
        cudf: "26.02.01".to_string(),
        ..this_run()
    };
    let all = declared(&["q1", "q6", "q17", "q19"]);
    let first = Resume::No;
    into(&path, &all, "q1", &run, first, measured(timed(&run, 5))).await;
    into(
        &path,
        &all,
        "q6",
        &run,
        first,
        measured(timed(&other_cudf, 6)),
    )
    .await;
    into(&path, &all, "q17", &run, first, refused("out of memory")).await;
    let before = std::fs::read_to_string(&path).expect("the interrupted run's file");

    let resumed = Resume::KeepMeasured;
    let kept = into(&path, &all, "q1", &run, resumed, not_again("q1")).await;
    assert_eq!(kept, CaseOutcome::Held);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "holding q1 wrote nothing"
    );

    for (query, us) in [("q6", 60), ("q17", 170), ("q19", 190)] {
        let outcome = into(&path, &all, query, &run, resumed, measured(timed(&run, us)));
        assert_eq!(
            outcome.await,
            CaseOutcome::Measured,
            "{query} is measured again"
        );
        assert_eq!(section(&path, query), timed(&run, us));
    }
    assert_eq!(
        section(&path, "q1"),
        timed(&run, 5),
        "q1 is the interrupted run's"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Sections under this run's own five that lack what the harness writes now: an older binary's,
/// with neither batches nor the load (as the H100's tree held for q1, q6 and q8), and one short of
/// either half. A resumed run measures each again rather than keeping what the page and the
/// checks would then refuse.
#[tokio::test]
async fn a_section_short_of_the_batches_or_the_load_is_measured_again() {
    let run = this_run();
    let now = timed(&run, 5);
    let older_binary = format!(
        "GpuUnload\n  time_us=[[5]] total_us=5\n--- run ---\nrun_us=5\ndevice_us=5\n\
         runs=[{}]\nbuild=release\nallocator=rmm-pool\ndevice={}\ndriver={}\ncuda={}\n\
         cudf={}\nhost={}\n",
        vec!["5"; 10].join(","),
        run.device,
        run.driver,
        run.cuda,
        run.cudf,
        run.host,
    );
    let no_batches = now.replacen(" batch_rows=[[1]] batch_bytes=[[8]]", "", 1);
    let no_load = now.replacen("load_us=0\n", "", 1);
    for (case, body) in [
        ("older-binary", older_binary),
        ("no-batches", no_batches),
        ("no-load", no_load),
    ] {
        assert_ne!(body, now, "{case}: the mutation changed nothing");
        let (root, path) = scratch_file(case);
        let q1 = declared(&["q1"]);
        into(&path, &q1, "q1", &run, Resume::No, measured(body)).await;
        let outcome = into(
            &path,
            &q1,
            "q1",
            &run,
            Resume::KeepMeasured,
            measured(timed(&run, 9)),
        );
        assert_eq!(outcome.await, CaseOutcome::Measured, "{case} was kept");
        assert_eq!(section(&path, "q1"), timed(&run, 9), "{case}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The record's rows go in before the section: the section is what a resumed run keeps, so a
/// crash between the two must leave no section rather than a section with no rows.
#[tokio::test]
async fn the_rows_are_appended_before_the_section_is_written() {
    let (root, path) = scratch_file("order");
    let run = this_run();
    let one = declared(&["q1"]);
    let case = async { (timed(&this_run(), 5), vec!["a row".to_string()]) };
    let mut seen = None;
    let append = |rows: Vec<String>| seen = Some((rows, std::fs::read_to_string(&path).ok()));
    let outcome = measure_into(&path, &one, "q1", &run, Resume::No, case, append).await;

    let (rows, file_then) = seen.expect("the rows were handed over");
    let written = section(&path, "q1");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(outcome, CaseOutcome::Measured);
    assert_eq!(rows, ["a row"]);
    assert_eq!(
        file_then, None,
        "the section was written before the rows were appended"
    );
    assert_eq!(written, timed(&run, 5));
}

/// A measurement whose rows the record refuses is a failed case: its section says why, and a
/// resumed run measures it again rather than keep a section with no rows behind it.
#[tokio::test]
async fn a_case_whose_rows_the_record_refuses_is_recorded_as_failed() {
    let (root, path) = scratch_file("refused-rows");
    let run = this_run();
    let one = declared(&["q1"]);
    let case = async { (timed(&this_run(), 5), ()) };
    let outcome = measure_into(&path, &one, "q1", &run, Resume::No, case, |()| {
        panic!("the record was written by\n a different run")
    })
    .await;

    let written = section(&path, "q1");
    let _ = std::fs::remove_dir_all(&root);
    let reason = "the record was written by a different run";
    assert_eq!(outcome, CaseOutcome::Failed(reason.to_string()));
    assert!(
        written.starts_with(&format!("{FAILED}{reason}\n")),
        "{written}"
    );
}

/// Without the resume variable a section carrying this run's five is measured again: the five
/// do not name the binary, and a rebuild on the same host is a new measurement.
#[tokio::test]
async fn a_measured_section_is_measured_again_unless_the_run_resumes() {
    let (root, path) = scratch_file("remeasured");
    let run = this_run();
    let one = declared(&["q1"]);
    into(
        &path,
        &one,
        "q1",
        &run,
        Resume::No,
        measured(timed(&run, 5)),
    )
    .await;

    let again = into(
        &path,
        &one,
        "q1",
        &run,
        Resume::No,
        measured(timed(&run, 50)),
    );
    assert_eq!(again.await, CaseOutcome::Measured);
    assert_eq!(section(&path, "q1"), timed(&run, 50));
    let _ = std::fs::remove_dir_all(&root);
}

/// `PEACOCK_BENCH_RESUME` is unset, `0` or `1`; anything else is refused naming the two.
///
/// The one case in this binary that touches the variable, so setting it here races nothing.
#[test]
fn the_resume_variable_is_unset_zero_or_one() {
    let was = std::env::var_os(RESUME_ENV);
    let read = |value: Option<&str>| {
        // SAFETY: no other case in this binary reads or writes the variable.
        match value {
            Some(value) => unsafe { std::env::set_var(RESUME_ENV, value) },
            None => unsafe { std::env::remove_var(RESUME_ENV) },
        }
        std::panic::catch_unwind(Resume::from_env)
    };
    let unset = read(None);
    let zero = read(Some("0"));
    let one = read(Some("1"));
    let yes = read(Some("yes"));
    match was {
        Some(value) => unsafe { std::env::set_var(RESUME_ENV, value) },
        None => unsafe { std::env::remove_var(RESUME_ENV) },
    }
    assert_eq!(unset.ok(), Some(Resume::No));
    assert_eq!(zero.ok(), Some(Resume::No));
    assert_eq!(one.ok(), Some(Resume::KeepMeasured));
    let refused = yes.expect_err("`yes` was read as a resume setting");
    let message = refused
        .downcast_ref::<String>()
        .expect("a formatted refusal");
    assert!(
        message.starts_with(&format!("{RESUME_ENV}=yes is neither `1`")),
        "{message}"
    );
}

/// A dataset that is not there is the run's fault, refused before any case can record it.
#[test]
#[should_panic(expected = "no dataset at")]
fn a_missing_dataset_is_refused_before_any_case() {
    assert_dataset_present("tpch", "999", "tpch/q1 at tp1-single on a device");
}

/// A record another run wrote is refused before any case measures; an empty or missing one is
/// any run's, and one with this run's heading is this run's.
#[test]
fn a_record_another_run_wrote_is_refused_before_any_case() {
    let run = this_run();
    let meta = |allocator| RunMeta {
        dataset: "tpch",
        sf: "40",
        query: "q1",
        mode: "tp1-single",
        allocator,
        capture: Capture::None,
        provenance: &run,
    };
    let (root, _) = scratch_file("record");
    let path = root.join("records.tsv");
    assert_eq!(record_heading_mismatch(&path, &meta("rmm-pool")), None);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&path, format!("{}\n", record_header(&meta("rmm-pool")))).unwrap();

    let ours = record_heading_mismatch(&path, &meta("rmm-pool"));
    let theirs = record_heading_mismatch(&path, &meta("rmm-default"));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(ours, None);
    let theirs = theirs.expect("another allocator's record was taken as this run's");
    assert!(
        theirs.contains("was written by a different run"),
        "{theirs}"
    );
}
