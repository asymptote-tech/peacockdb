//! The three GPU runners — pipeline.yml's gpu job, `build-test-shadgpu.sh` and `build-test.sh`
//! — read for the staged targets, the lib and its rung, and the single-tenant flag.

use std::collections::BTreeSet;

use crate::{
    INTENTIONALLY_NOT_IN_CI, PIPELINE, cargo_features, fold_continuations, repo_root,
    workspace_test_targets,
};

const SHADGPU: &str = "scripts/build-test-shadgpu.sh";

const BUILD_TEST: &str = "scripts/build-test.sh";

pub(crate) const GPU_STAGING_STEP: &str = "Build and stage rust GPU test binaries";

pub(crate) const GPU_RUN_STEP: &str = "Run GPU tests";

/// The body of one step of pipeline.yml's GPU jobs, by its `name:`.
fn gpu_job_step(name: &str) -> String {
    let text = std::fs::read_to_string(repo_root().join(PIPELINE)).expect("read pipeline.yml");
    let needle = format!("- name: {name}");
    let mut lines = text.lines().skip_while(|l| !l.contains(&needle));
    lines.next().unwrap_or_else(|| {
        panic!(
            "pipeline.yml has no `{name}` step — the GPU job was reshaped and the guards \
             that read it now read nothing"
        )
    });
    lines
        .take_while(|l| !l.trim_start().starts_with("- name:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `--test` targets pipeline.yml's gpu-tests job stages and runs, read out of the
/// staging step's `for t in <names>; do` array.
///
/// Parsed rather than duplicated: this is the array a [`Exemption::GpuJob`] entry
/// points at, so a copy here would defeat the check it exists to make.
pub(crate) fn gpu_job_staged_targets() -> BTreeSet<String> {
    let step = gpu_job_step(GPU_STAGING_STEP);
    let line = step
        .lines()
        .find(|l| l.trim_start().starts_with("for t in test_"))
        .expect(
            "pipeline.yml has no `for t in test_…; do` staging loop — the gpu-tests \
             staging step was reshaped, and every GpuJob exemption now rests \
             on an array that does not exist",
        );
    line.trim()
        .trim_start_matches("for t in ")
        .split(';')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The binaries the staging step names outright, as `(staged name, folded line)`: a
/// `stage <name> "$(cargo …)"` with a literal name, not the array loop's `stage "$t"`.
/// The lib is not a `--test` target, so it cannot ride in the array and is staged this
/// way — which is why the array reader above cannot see it.
pub(crate) fn gpu_job_staged_by_name() -> Vec<(String, String)> {
    fold_continuations(&gpu_job_step(GPU_STAGING_STEP))
        .iter()
        .filter_map(|l| staged_by_name_of(l))
        .collect()
}

/// Parse one folded staging line as `stage <literal name> …`, if it is one.
pub(crate) fn staged_by_name_of(line: &str) -> Option<(String, String)> {
    let mut toks = line.split_whitespace();
    (toks.next()? == "stage").then_some(())?;
    let name = toks.next()?;
    (!name.starts_with(['"', '$'])).then(|| (name.to_string(), line.trim().to_string()))
}

/// The lib binary the gpu-tests job stages, as `(staged name, line)`: the one `stage`
/// line naming a file outright whose cargo build is `--lib --features gpu`.
pub(crate) fn gpu_job_staged_lib() -> Option<(String, String)> {
    gpu_job_staged_by_name().into_iter().find(|(_, l)| {
        l.contains("--lib") && cargo_features(l) == Some("gpu") && l.contains("--no-run")
    })
}

/// The rung pipeline.yml's run loop hands one staged file, as `(staged name, filter)` —
/// the `[ "$tname" = <name> ] && rung=<filter>` line. shad-gpu runs prebuilt binaries,
/// never cargo, so the device rung is this line and not a command line.
pub(crate) fn gpu_job_lib_rung() -> Option<(String, String)> {
    rust_gpu_runner_loop(PIPELINE)
        .iter()
        .find_map(|l| lib_rung_of(l))
}

/// Parse one loop line as the rung assignment, if it is one.
pub(crate) fn lib_rung_of(line: &str) -> Option<(String, String)> {
    let line = line.trim_start();
    if line.starts_with('#') {
        return None;
    }
    let (cond, assign) = line.split_once("&&")?;
    let filter = assign.trim().strip_prefix("rung=")?;
    let name = cond.split_whitespace().rev().nth(1)?;
    Some((name.to_string(), filter.to_string()))
}

/// The GPU test binaries `scripts/build-test-shadgpu.sh` stages, from its `RUST_TESTS`
/// array — what a developer's own run ships to the host.
fn shadgpu_staged_targets() -> BTreeSet<String> {
    let text =
        std::fs::read_to_string(repo_root().join(SHADGPU)).expect("read build-test-shadgpu.sh");
    let start = text.find("RUST_TESTS=(").expect(
        "build-test-shadgpu.sh has no RUST_TESTS=( array — the staging list was renamed, \
         and the check that it matches CI now rests on a list that does not exist",
    ) + "RUST_TESTS=(".len();
    let end = start
        + text[start..]
            .find(')')
            .expect("unterminated RUST_TESTS array");
    text[start..end]
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// A script's lib binary on the GPU host, as `(staged name, rung)`: the first `<staged>=`
/// and `<rung>=` assignments found in `text`, quotes stripped. The lib is not a `--test`
/// target, so neither script lists it beside the targets — each names it in a pair of
/// variables that the run loop hands to `rung_args`.
fn script_lib(text: &str, staged: &str, rung: &str) -> Option<(String, String)> {
    let assigned = |name: &str| {
        text.lines().find_map(|l| {
            let v = l.trim().strip_prefix(name)?.strip_prefix('=')?;
            Some(v.trim_matches(['"', '\'']).to_string())
        })
    };
    Some((assigned(staged)?, assigned(rung)?))
}

/// `build-test-shadgpu.sh`'s lib: `RUST_LIB_STAGED` and `RUST_LIB_RUNG`.
fn shadgpu_staged_lib() -> Option<(String, String)> {
    let text =
        std::fs::read_to_string(repo_root().join(SHADGPU)).expect("read build-test-shadgpu.sh");
    script_lib(&text, "RUST_LIB_STAGED", "RUST_LIB_RUNG")
}

/// `build-test.sh`'s lib in `--gpu` mode: `LIB_STAGED` and `LIB_RUNG` inside the
/// `[ "$MODE" = "gpu" ]` branch that sets them, since the cpu modes set the same two
/// names to their own shapes.
fn build_test_gpu_lib() -> Option<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join(BUILD_TEST)).expect("read build-test.sh");
    let branch: String = text
        .lines()
        .skip_while(|l| !(l.contains("\"$MODE\" = \"gpu\"") && l.contains("then")))
        .skip(1)
        .take_while(
            |l| !matches!(l.trim(), s if s.starts_with("elif ") || s == "else" || s == "fi"),
        )
        .collect::<Vec<_>>()
        .join("\n");
    script_lib(&branch, "LIB_STAGED", "LIB_RUNG")
}

/// Does the lib's rung reach the binary in this script's run loop? The rung sits in a
/// variable, so its presence proves nothing: the loop must hand both variables to
/// `rung_args` and every invocation must carry what it returned. Returns what is missing.
fn rung_reaches_the_binary(text: &str, staged: &str, rung: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();
    // The loop is found from the inside out: the two scripts head it differently (`for t
    // in …rust-tests/*` and `for name in $RUST_TEST_NAMES`), and the feed line is the
    // one thing both loops must carry.
    let feed = lines
        .iter()
        .position(|l| {
            !l.trim_start().starts_with('#')
                && l.contains("rung_args \"\\$t\"")
                && l.contains(&format!("${staged}"))
                && l.contains(&format!("${rung}"))
        })
        .ok_or_else(|| format!("no line hands \"$t\", ${staged} and ${rung} to rung_args"))?;
    let head = lines[..feed]
        .iter()
        .rposition(|l| l.trim_start().starts_with("for "))
        .ok_or("the rung_args line is not inside a `for` loop")?;
    let loop_body: Vec<&str> = lines[head..]
        .iter()
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("done"))
        .copied()
        .collect();
    // `"$t"` followed by a flag is the binary run with its arguments; the `[ -x "$t" ]`
    // test and the feed line above both name `"$t"` and neither follows it with one.
    let runs: Vec<&&str> = loop_body
        .iter()
        .filter(|l| !l.trim_start().starts_with('#') && l.contains("\"\\$t\" --"))
        .collect();
    if runs.is_empty() {
        return Err("the loop runs no binary as `\"$t\" --…`".to_string());
    }
    match runs.iter().find(|l| !l.contains("\"\\${args[@]}\"")) {
        Some(l) => Err(format!(
            "an invocation runs without the arguments rung_args produced: {}",
            l.trim()
        )),
        None => Ok(()),
    }
}

/// The targets `scripts/build-test.sh` treats as needing a device at run time, from its
/// `gpu_runtime_targets()` heredoc, as `(crate, target)`. That set is SUBTRACTED from the
/// CPU modes and matched WHOLE against `<crate>:<target>`, so the crate is kept rather than
/// discarded: a line with the wrong crate subtracts nothing, and the target it names then
/// runs on a machine with no GPU.
fn gpu_runtime_targets() -> BTreeSet<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join(BUILD_TEST)).expect("read build-test.sh");
    let start = text.find("<<'GPUSET'").expect(
        "build-test.sh has no GPUSET heredoc — gpu_runtime_targets() was renamed or \
         reshaped, and both the mode ladder and this check depend on its contents",
    ) + "<<'GPUSET'".len();
    let end = start
        + text[start..]
            .find("\nGPUSET")
            .expect("unterminated GPUSET heredoc");
    text[start..end]
        .lines()
        .filter_map(|l| {
            l.trim()
                .split_once(':')
                .map(|(c, t)| (c.to_string(), t.to_string()))
        })
        .collect()
}

/// Three committed lists name the GPU test binaries — pipeline.yml's staging array,
/// build-test-shadgpu.sh's `RUST_TESTS`, and build-test.sh's `gpu_runtime_targets()` —
/// each read by a different runner, so a target in one and not the others is invisible in
/// exactly the direction that matters. The third is the dangerous one: the CPU modes are
/// built by SUBTRACTING it, whole-line, so an entry with the wrong crate or a stale target
/// name subtracts nothing and a file-gated GPU test RUNS on a host with no device.
///
/// Not checked: a target exempted as `NotRun` may be absent from `gpu_runtime_targets()`
/// with nothing red. No file says "needs a device" — that is what the hand-declared set is
/// for.
#[test]
fn the_three_gpu_target_lists_agree() {
    let ci = gpu_job_staged_targets();
    let dev = shadgpu_staged_targets();
    let runtime = gpu_runtime_targets();

    assert_eq!(
        ci, dev,
        "pipeline.yml's gpu-tests staging array and build-test-shadgpu.sh's RUST_TESTS \
         name different sets. A developer's run then proves a different set from the \
         merge gate's, in whichever direction the lists disagree."
    );

    // The heredoc is the only one of the three that names a crate, so it is the only one
    // that can be wrong about it — and it lists the same tests/ tree workspace_test_targets
    // reads, which is what makes a bad prefix or a renamed file checkable here at all.
    let on_disk = workspace_test_targets();
    let unmatched: Vec<String> = runtime.difference(&on_disk).map(qualified).collect();
    assert!(
        unmatched.is_empty(),
        "gpu_runtime_targets() names entries that no workspace test target matches: \
         {unmatched:?}\nbuild-test.sh matches those lines whole against `<crate>:<target>`, so \
         each subtracts nothing — either the crate is wrong or the test file was renamed, and \
         the target it means to hold back now runs on a CPU-only host."
    );

    // The other two lists are bare names, and every target they carry is peacockdb-core's,
    // so that is what a bare name means when set against the crate-qualified heredoc.
    let ci_qualified: BTreeSet<(String, String)> = ci
        .iter()
        .map(|t| ("peacockdb-core".to_string(), t.clone()))
        .collect();

    let unsubtracted: Vec<String> = ci_qualified.difference(&runtime).map(qualified).collect();
    assert!(
        unsubtracted.is_empty(),
        "CI stages these for the GPU host, but build-test.sh's gpu_runtime_targets() does \
         not name them: {unsubtracted:?}\nThat set is subtracted to build the CPU modes, \
         so each of these is currently both skipped by --gpu and RUN on a CPU-only host."
    );

    let exempt: BTreeSet<&str> = INTENTIONALLY_NOT_IN_CI.iter().map(|(n, _)| *n).collect();
    let orphaned: Vec<String> = runtime
        .difference(&ci_qualified)
        .filter(|(_, t)| !exempt.contains(t.as_str()))
        .map(qualified)
        .collect();
    assert!(
        orphaned.is_empty(),
        "gpu_runtime_targets() names these, no CI staging step runs them, and no \
         INTENTIONALLY_NOT_IN_CI entry says that is deliberate: {orphaned:?}"
    );

    // The lib is the one binary none of the three lists can hold — it is not a `--test`
    // target — so each runner names it and its rung separately, and those must agree too:
    // a runner staging the lib under another name runs it whole, cpu cases and all, on the
    // one serial host, and a runner with another rung runs the device cases nowhere.
    let ci_lib = gpu_job_staged_lib()
        .map(|(n, _)| n)
        .zip(gpu_job_lib_rung().map(|(_, r)| r));
    let dev_lib = shadgpu_staged_lib();
    let runtime_lib = build_test_gpu_lib();
    assert!(
        ci_lib.is_some() && dev_lib.is_some() && runtime_lib.is_some(),
        "a runner no longer names the lib binary and its rung — pipeline.yml {ci_lib:?}, \
         build-test-shadgpu.sh {dev_lib:?}, build-test.sh --gpu {runtime_lib:?}"
    );
    assert!(
        ci_lib == dev_lib && dev_lib == runtime_lib,
        "the three runners disagree about the lib binary or its rung — pipeline.yml \
         {ci_lib:?}, build-test-shadgpu.sh {dev_lib:?}, build-test.sh --gpu {runtime_lib:?}"
    );
    let runners = [
        (SHADGPU, "RUST_LIB_STAGED", "RUST_LIB_RUNG"),
        (BUILD_TEST, "LIB_STAGED", "LIB_RUNG"),
    ];
    for (rel, staged, rung) in runners {
        let text = std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e}"));
        if let Err(why) = rung_reaches_the_binary(&text, staged, rung) {
            panic!("{rel}: the lib's rung does not reach the binary — {why}");
        }
    }
}

/// The GPU step reports a failure, and has no way to report success over one.
///
/// It runs without `set -e` deliberately — one binary failing must not skip the rest — so the
/// only thing that can fail it is what it folds into `rc` itself. Three ways that goes wrong:
/// a binary's status never folded in, the block not exiting `rc`, and a command dying of a
/// signal, which leaves no status to fold at all. The third shipped: the rust loop's `cat` and
/// `grep` segfaulted under an exported glibc-2.35 once per target, so five binaries ran with
/// their output discarded and their zero-tests guard answering from a crash, job green.
#[test]
fn the_gpu_step_cannot_report_success_over_a_failure() {
    let step = gpu_job_step(GPU_RUN_STEP);

    assert!(
        step.contains("exit \\$rc"),
        "the GPU step does not end by exiting the status it accumulated, so everything it \
         recorded in rc is discarded and the step is green whatever ran"
    );
    for line in rust_gpu_runner_invocations(PIPELINE) {
        assert!(
            line.ends_with("|| rc=1"),
            "a staged rust GPU binary runs without folding its status into rc, so it cannot \
             fail the job: {line}"
        );
    }
    assert!(
        step.contains("[ \"\\$trc\" -eq 0 ] || rc=1"),
        "the C++ loop no longer folds each binary's status into rc"
    );
    assert!(
        step.contains("|| rc=$?"),
        "the ssh pipeline's own status is not captured into rc. Written `rc=\\$?`, the escape \
         a heredoc-shaped edit reaches for, it assigns the literal two characters and the \
         status of the whole remote run is lost"
    );
    assert!(
        step.contains("Segmentation fault") && step.contains("::error::"),
        "nothing in the GPU step notices a command dying of a signal. Without 'set -e' such a \
         death folds no status into rc, so the step is green having crashed — which is how the \
         rust loop printed nothing for five targets and said so nowhere"
    );
    assert!(
        !step.contains("export LD_LIBRARY_PATH"),
        "the GPU step exports the patched glibc into its shell. setup-glibc.sh prints the trap \
         in this same job's log: the host's own coreutils then load it and segfault. Apply it \
         per command (env LD_LIBRARY_PATH=… \"$t\" …) instead"
    );
}

/// The `<crate>:<target>` form build-test.sh matches on, for failure messages.
fn qualified((krate, target): &(String, String)) -> String {
    format!("{krate}:{target}")
}

/// Every line that runs a staged rust GPU binary, out of a committed runner.
///
/// All of them rather than the first, because a first-match reader is checking whichever
/// invocation happens to come first and excusing a retry or a variant added below it. Scoped
/// to the rust loop and read line-wise, since both shortcuts are false-green here: the C++
/// loop above it in each file uses the same `$t` and rightly passes no flag, and every rust
/// invocation carries a comment saying the flag is mandatory, so a file-wide `contains`
/// survives the edit that matters — the flag dropped from the command, the comment left.
pub(crate) fn rust_gpu_runner_invocations(rel: &str) -> Vec<String> {
    let found: Vec<String> = rust_gpu_runner_loop(rel)
        .iter()
        .filter(|l| is_rust_gpu_runner_invocation(l))
        .map(|l| l.trim().to_string())
        .collect();
    assert!(
        !found.is_empty(),
        "{rel}'s rust-tests loop runs no binary as `\"\\$t\" …` — the invocation was renamed or \
         moved out of the loop, and this guard now reads nothing"
    );
    found
}

/// The body of a runner's rust loop, from its `for t in …rust-tests/*` header to `done`.
fn rust_gpu_runner_loop(rel: &str) -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let mut after_header = text
        .lines()
        .skip_while(|l| !is_rust_gpu_runner_loop_header(l));
    after_header.next().unwrap_or_else(|| {
        panic!(
            "{rel} has no `for t in …rust-tests/*` loop — the runner was reshaped, and the \
             single-tenant GPU invariant now rests on a loop this guard cannot find"
        )
    });
    after_header
        .take_while(|l| !l.trim_start().starts_with("done"))
        .map(str::to_string)
        .collect()
}

fn is_rust_gpu_runner_loop_header(line: &str) -> bool {
    line.contains("for t in") && line.contains("rust-tests/")
}

/// Is this the line that executes the binary, rather than a comment or the `[ -x … ]` test?
///
/// Keyed on running `"$t"` AND redirecting to a log, not on the line starting with `"$t"`:
/// the patched glibc is applied per command (`env LD_LIBRARY_PATH=… "$t" …`) rather than
/// exported, and a reader anchored at the start of the line stops seeing the invocation the
/// moment anything precedes it — a guard silently reading nothing, not a red one.
pub(crate) fn is_rust_gpu_runner_invocation(line: &str) -> bool {
    let line = line.trim_start();
    !line.starts_with('#') && line.contains("\"\\$t\"") && line.contains("> \"\\$")
}

/// Single-tenant GPU is one flag on two committed runner lines and nothing else. cuDF and
/// RMM share a process-wide pool, so `--test-threads=1` is what keeps the device to one test
/// at a time — and `test_gpu_corpus.rs` spends it further, setting environment variables
/// in an `unsafe` block whose safety argument is that flag. Dropping it from either runner
/// makes that argument false, and until this test existed nothing said so.
#[test]
fn both_gpu_runners_pass_test_threads_one() {
    for rel in [PIPELINE, SHADGPU] {
        for line in rust_gpu_runner_invocations(rel) {
            assert!(
                line.contains("--test-threads=1"),
                "{rel} runs a staged GPU binary without --test-threads=1:\n  {line}\n\
                 cuDF/RMM share one process-wide pool, so concurrent cases OOM the device, and \
                 the env-var writes in test_gpu_corpus.rs are sound only while this flag holds."
            );
        }
    }
}

/// The reader's own guard, over the three ways it could report false coverage: a comment
/// standing in for the command, the C++ loop standing in for the rust one, and the first
/// invocation standing in for the rest.
#[test]
fn the_runner_reader_takes_every_rust_command_and_not_a_comment_or_the_cpp_loop() {
    let flagged: fn(&&str) -> bool = |l| l.contains("--test-threads=1");
    let read = |block: &str| -> Vec<String> {
        block
            .lines()
            .skip_while(|l| !is_rust_gpu_runner_loop_header(l))
            .take_while(|l| !l.trim_start().starts_with("done"))
            .filter(|l| is_rust_gpu_runner_invocation(l))
            .map(|l| l.trim().to_string())
            .collect()
    };

    // (1) the flag dropped from the command, its comment left above it.
    let dropped = "for t in $REMOTE_DIR/cpp/install/rust-tests/*; do\n\
                   \x20 # --test-threads=1: cuDF/RMM share one process-wide pool.\n\
                   \x20 \"\\$t\" --nocapture > \"\\$tlog\" 2>&1\n\
                   done";
    assert!(
        dropped.contains("--test-threads=1"),
        "a file-wide search is green on this"
    );
    assert!(
        !read(dropped).iter().any(|l| flagged(&l.as_str())),
        "the command has lost the flag"
    );

    // (2) the C++ loop first, sharing the same `$t`: taking the file's first invocation reads
    // that one and never reaches the rust loop. (3) a second invocation added inside the rust
    // loop — a retry — unflagged, which a reader that stops at the first would excuse.
    let both_loops = "for t in $REMOTE_DIR/cpp/install/bin/peacock_*_tests; do\n\
                      \x20 \"\\$t\" > \"\\$tlog\" 2>&1\n\
                      done\n\
                      for t in $REMOTE_DIR/cpp/install/rust-tests/*; do\n\
                      \x20 \"\\$t\" --nocapture --test-threads=1 > \"\\$tlog\" 2>&1\n\
                      \x20 \"\\$t\" --nocapture --ignored > \"\\$tlog\" 2>&1\n\
                      done";
    let first = both_loops
        .lines()
        .find(|l| is_rust_gpu_runner_invocation(l))
        .expect("a line");
    assert!(!flagged(&first), "the C++ invocation is the file's first");
    let rust = read(both_loops);
    assert_eq!(rust.len(), 2, "both rust invocations are read: {rust:?}");
    assert!(
        flagged(&rust[0].as_str()) && !flagged(&rust[1].as_str()),
        "the retry is unflagged"
    );
}
