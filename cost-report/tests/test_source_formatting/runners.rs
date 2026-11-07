//! The two GPU runners — CI's image path and `build-test-shadgpu.sh` — read for the staged
//! targets, the lib and its rung, and the single-tenant flag. CI's gpu-tests job runs the install
//! pipeline.yml's staging step builds (`build-test.sh --gpu --build`, which is
//! `scripts/env/stage-install.sh` in the build image) through `run-gpu.sh` on nebius-gpu, in the
//! run image, whose `docker/run-tests.sh list` is the run loop read here. `build-test.sh` holds
//! the GPU-runtime set the CPU modes subtract.

use std::collections::BTreeSet;

use crate::ci_coverage::{
    BENCHMARK_TARGET, INTENTIONALLY_NOT_IN_CI, PIPELINE, cargo_features, fold_continuations,
    workspace_test_targets,
};
use crate::repo_root;

const SHADGPU: &str = "scripts/build-test-shadgpu.sh";

const BUILD_TEST: &str = "scripts/build-test.sh";

/// Stages the install a device run ships, CI's included, in the build image.
pub(crate) const STAGE_INSTALL: &str = "scripts/env/stage-install.sh";

/// The run image's runner: its `list` is the L40S list CI runs on nebius-gpu.
pub(crate) const RUN_TESTS: &str = "docker/run-tests.sh";

/// Names the staged lib's crate and kind for `stage_cargo_lib_binary`.
const STAGE_CARGO: &str = "scripts/lib/stage-cargo.sh";

pub(crate) const GPU_STAGING_STEP: &str = "Build and stage rust GPU test binaries";

pub(crate) const GPU_RUN_STEP: &str = "Run the L40S list";

/// The run step's command: run-tests.sh's list on nebius-gpu, attached, its exit the run's.
const GPU_RUN: &str =
    "scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt run list";

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

/// The `--test` targets CI's gpu-tests job stages and runs: `stage-install.sh`'s one-line
/// `GPU_TESTS=( … )` array, which pipeline.yml's staging step builds through `build-test.sh --gpu
/// --build` ([`the_pipeline_stages_through_stage_install`]).
///
/// Parsed rather than duplicated: this is the array a [`Exemption::GpuJob`] entry
/// points at, so a copy here would defeat the check it exists to make.
///
/// [`Exemption::GpuJob`]: crate::ci_coverage::Exemption::GpuJob
pub(crate) fn gpu_job_staged_targets() -> BTreeSet<String> {
    one_line_array(&read(STAGE_INSTALL), "GPU_TESTS").expect(
        "stage-install.sh has no one-line `GPU_TESTS=( … )` array — the staging list was \
         renamed or reshaped, and every GpuJob exemption now rests on an array that does not exist",
    )
}

/// The words of a script's `<name>=( … )` array, written on one line; `None` when there is no
/// such line outside a comment, or its `)` is on another line.
fn one_line_array(text: &str, name: &str) -> Option<BTreeSet<String>> {
    let head = format!("{name}=(");
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with(&head))?;
    let words = line[head.len()..].split_once(')')?.0;
    Some(words.split_whitespace().map(str::to_string).collect())
}

/// The lib binary a staging script stages, as `(staged name, line)`: its first
/// `stage_cargo_lib_binary <name> <dir> … --features gpu` line outside a comment, the name a
/// literal or a `$VAR` the script assigns. `stage_cargo_lib_binary` is the crate's own unit-test
/// binary ([`the_staged_lib_is_the_crates_own_test_binary`]).
pub(crate) fn staged_lib_in(text: &str) -> Option<(String, String)> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("stage_cargo_lib_binary "))?;
    if cargo_features(line) != Some("gpu") {
        return None;
    }
    let name = line.split_whitespace().nth(1)?.trim_matches('"');
    let name = match name.strip_prefix('$') {
        Some(var) => assigned(text, var.trim_matches(['{', '}']))?,
        None => name.to_string(),
    };
    Some((name, line.to_string()))
}

/// The lib binary CI's gpu-tests job stages: [`staged_lib_in`] over `stage-install.sh`.
pub(crate) fn gpu_job_staged_lib() -> Option<(String, String)> {
    staged_lib_in(&read(STAGE_INSTALL))
}

/// The lib and its rung in run-tests.sh, as `(LIB_STAGED, LIB_RUNG)`. The run image runs prebuilt
/// binaries, never cargo, so the device rung is these two values and the loop line that appends
/// the rung under the name, not a command line.
pub(crate) fn run_tests_lib_rung() -> Option<(String, String)> {
    let text = read(RUN_TESTS);
    script_lib(&text, "LIB_STAGED", "LIB_RUNG")
}

/// The GPU test binaries `scripts/build-test-shadgpu.sh` stages, from its `RUST_TESTS`
/// array — what a developer's own run ships to the host.
pub(crate) fn shadgpu_staged_targets() -> BTreeSet<String> {
    one_line_array(&read(SHADGPU), "RUST_TESTS").expect(
        "build-test-shadgpu.sh has no one-line RUST_TESTS=( … ) array — the staging list was \
         renamed, and the check that it matches CI now rests on a list that does not exist",
    )
}

/// A script's lib binary on the GPU host, as `(staged name, rung)`: the first `<staged>=`
/// and `<rung>=` assignments found in `text`, quotes stripped. The lib is not a `--test`
/// target, so neither script lists it beside the targets — each names it in a pair of
/// variables that the run loop hands to `rung_args`.
fn script_lib(text: &str, staged: &str, rung: &str) -> Option<(String, String)> {
    Some((assigned(text, staged)?, assigned(text, rung)?))
}

/// The value of the first `<name>=` assignment in `text`, quotes stripped.
fn assigned(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.trim().strip_prefix(name)?.strip_prefix('=')?;
        Some(v.trim_matches(['"', '\'']).to_string())
    })
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// `build-test-shadgpu.sh`'s lib: `RUST_LIB_STAGED` and `RUST_LIB_RUNG`.
fn shadgpu_staged_lib() -> Option<(String, String)> {
    script_lib(&read(SHADGPU), "RUST_LIB_STAGED", "RUST_LIB_RUNG")
}

/// The body of run-tests.sh's rust loop, `for t in "${rust[@]}"` to its `done`, comments out.
fn run_tests_rust_loop(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|l| !l.contains("for t in \"${rust[@]}\""))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("done"))
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// A shell function's body, `<name>() {` to the `}` that closes it at the start of a line,
/// folded at continuations, comments out.
fn function_body(text: &str, name: &str) -> Vec<String> {
    let head = format!("{name}() {{");
    let body: Vec<&str> = text
        .lines()
        .skip_while(|l| !l.starts_with(&head))
        .skip(1)
        .take_while(|l| *l != "}")
        .collect();
    fold_continuations(&body.join("\n"))
        .into_iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

/// The lines every rust binary run-tests.sh runs takes its arguments from: each `args=(` line in
/// its rust loop and in `run_bench`. Each must hold `--test-threads=1`, and each `run_binary`
/// line there must pass `"${args[@]}"`. `None` when either block declares no `args=(`.
fn run_tests_rust_args(text: &str) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    for block in [run_tests_rust_loop(text), function_body(text, "run_bench")] {
        let declared: Vec<String> = block
            .iter()
            .filter(|l| l.contains("args=("))
            .map(|l| l.trim().to_string())
            .collect();
        if declared.is_empty() {
            return None;
        }
        lines.extend(declared);
    }
    Some(lines)
}

/// The `run_binary` lines of run-tests.sh's rust loop and `run_bench`, folded.
fn run_tests_rust_runs(text: &str) -> Vec<String> {
    run_tests_rust_loop(text)
        .into_iter()
        .chain(function_body(text, "run_bench"))
        .filter(|l| l.trim_start().starts_with("run_binary "))
        .map(|l| l.trim().to_string())
        .collect()
}

/// What run-tests.sh must hold for a failure to fail the run. It runs without `set -e` for a
/// binary, deliberately, so the only thing that can fail the run is what it folds into `rc`:
/// it ends `exit "$rc"`, every `run_binary` in `run_list` and `run_bench` is judged on the next
/// line, `judge` reads a segfault out of the log, and no library path is exported into its own
/// shell. Returns what is missing.
fn run_tests_cannot_report_success_over_a_failure(text: &str) -> Result<(), String> {
    let last = text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with('#'));
    if last != Some("exit \"$rc\"") {
        return Err(format!(
            "it does not end `exit \"$rc\"` but {last:?}, so what it folded into rc is discarded"
        ));
    }
    for name in ["run_list", "run_bench"] {
        let body = function_body(text, name);
        let runs: Vec<usize> = (0..body.len())
            .filter(|&i| body[i].trim_start().starts_with("run_binary "))
            .collect();
        if runs.is_empty() {
            return Err(format!("{name} runs no binary through run_binary"));
        }
        for i in runs {
            let next = body[i + 1..].iter().find(|l| !l.trim().is_empty());
            if !next.is_some_and(|l| l.trim_start().starts_with("judge ")) {
                return Err(format!(
                    "{name}: a binary's status is not judged on the next line: {}",
                    body[i].trim()
                ));
            }
        }
    }
    if !function_body(text, "judge")
        .iter()
        .any(|l| l.contains("grep") && l.contains("Segmentation fault") && l.contains("fail "))
    {
        return Err(
            "judge does not fail a log holding `Segmentation fault`: a binary that crashed \
             under a wrapper that exits 0 is green"
                .to_string(),
        );
    }
    if text.contains("export LD_LIBRARY_PATH") {
        return Err("it exports a library path into its own shell, whose coreutils then load it \
                    and can crash; apply it per command, through env"
            .to_string());
    }
    Ok(())
}

/// Does run-tests.sh's rung reach the lib `stage-install.sh` stages as `staged`? In its
/// `for t in "${rust[@]}"` loop the rung is appended to `args` under `[ "$name" = "$LIB_STAGED" ]`,
/// so the script's `LIB_STAGED` must be that name, and every `run_binary` line in the loop must
/// pass `"${args[@]}"`. Returns what is missing.
fn run_tests_rung_reaches_the_binary(text: &str, staged: &str) -> Result<(), String> {
    match assigned(text, "LIB_STAGED") {
        Some(name) if name == staged => {}
        other => {
            return Err(format!(
                "its LIB_STAGED is {other:?}, and {STAGE_INSTALL} stages the lib as {staged:?}: \
                 the rung goes to a name no binary has"
            ));
        }
    }
    let body = run_tests_rust_loop(text);
    if body.is_empty() {
        return Err("no `for t in \"${rust[@]}\"` loop with a body".to_string());
    }
    if !body.iter().any(|l| {
        l.contains("[ \"$name\" = \"$LIB_STAGED\" ]") && l.contains("args+=(\"$LIB_RUNG\")")
    }) {
        return Err(
            "no line appends $LIB_RUNG to args under [ \"$name\" = \"$LIB_STAGED\" ]".to_string(),
        );
    }
    let runs: Vec<&String> = body.iter().filter(|l| l.contains("run_binary ")).collect();
    if runs.is_empty() {
        return Err("the loop runs no binary through run_binary".to_string());
    }
    match runs.iter().find(|l| !l.contains("\"${args[@]}\"")) {
        Some(l) => Err(format!("a binary runs without its args: {}", l.trim())),
        None => Ok(()),
    }
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

/// Three committed lists name the GPU test binaries — stage-install.sh's `GPU_TESTS`, which CI
/// stages, build-test-shadgpu.sh's `RUST_TESTS`, and build-test.sh's `gpu_runtime_targets()` —
/// each read by a different runner, so a target in one and not the others is invisible in
/// exactly the direction that matters. The third is the dangerous one: the CPU modes are
/// built by SUBTRACTING it, whole-line, so an entry with the wrong crate or a stale target
/// name subtracts nothing and a file-gated GPU test RUNS on a host with no device.
///
/// Not checked: a target no workflow runs may be absent from `gpu_runtime_targets()` with
/// nothing red. No file says "needs a device" — that is what the hand-declared set is for.
#[test]
fn the_three_gpu_target_lists_agree() {
    let ci = gpu_job_staged_targets();
    let dev = shadgpu_staged_targets();
    let runtime = gpu_runtime_targets();

    assert_eq!(
        ci, dev,
        "{STAGE_INSTALL}'s GPU_TESTS, which CI stages, and {SHADGPU}'s RUST_TESTS name \
         different sets. A shad-gpu run then proves a different set from the merge gate's, in \
         whichever direction the lists disagree; {BUILD_TEST}'s gpu_runtime_targets() is the \
         third list, checked below."
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
        .zip(run_tests_lib_rung().map(|(_, r)| r));
    let dev_lib = shadgpu_staged_lib();
    assert!(
        ci_lib.is_some() && dev_lib.is_some(),
        "a runner no longer names the lib binary and its rung — {STAGE_INSTALL} + {RUN_TESTS} \
         {ci_lib:?}, {SHADGPU} {dev_lib:?}"
    );
    assert!(
        ci_lib == dev_lib,
        "the two runners disagree about the lib binary or its rung — {STAGE_INSTALL} + \
         {RUN_TESTS} {ci_lib:?}, {SHADGPU} {dev_lib:?}"
    );
    if let Err(why) = rung_reaches_the_binary(&read(SHADGPU), "RUST_LIB_STAGED", "RUST_LIB_RUNG") {
        panic!("{SHADGPU}: the lib's rung does not reach the binary — {why}");
    }
    let staged = ci_lib.map(|(n, _)| n).unwrap_or_default();
    if let Err(why) = run_tests_rung_reaches_the_binary(&read(RUN_TESTS), &staged) {
        panic!("{RUN_TESTS}: the lib's rung does not reach the binary — {why}");
    }
}

/// The run-tests.sh reader's own guard: it accepts the loop as written and refuses each way the
/// rung can stop reaching the lib — a commented-out rung, a run without the arguments, and a
/// `LIB_STAGED` naming another binary than the one staged.
#[test]
fn the_run_tests_reader_refuses_a_rung_that_cannot_reach_the_lib() {
    let script = |staged: &str, rung_line: &str, run_line: &str| {
        format!(
            "LIB_STAGED={staged}\nLIB_RUNG=gpu_tests::\n\
             for t in \"${{rust[@]}}\"; do\n\
             \x20 args=(--nocapture --test-threads=1)\n\
             \x20 {rung_line}\n\
             \x20 {run_line}\n\
             done\n"
        )
    };
    let rung = "if [ \"$name\" = \"$LIB_STAGED\" ]; then args+=(\"$LIB_RUNG\"); fi";
    let run = "run_binary \"$logs/$name.log\" \"$t\" \"${args[@]}\"";
    let lib = "peacockdb_core_gpu_lib";
    assert_eq!(
        run_tests_rung_reaches_the_binary(&script(lib, rung, run), lib),
        Ok(())
    );
    assert!(
        run_tests_rung_reaches_the_binary(&script(lib, &format!("# {rung}"), run), lib).is_err()
    );
    assert!(
        run_tests_rung_reaches_the_binary(
            &script(lib, rung, "run_binary \"$logs/$name.log\" \"$t\""),
            lib
        )
        .is_err()
    );
    assert!(run_tests_rung_reaches_the_binary(&script("other_lib", rung, run), lib).is_err());

    // The staging reader: the name a literal or a `$VAR` the script assigns, only `--features
    // gpu`, never a commented-out line.
    let staging = |line: &str| format!("LIB_STAGED={lib}\nTESTS=cpp/install/rust-tests\n{line}\n");
    let named = |line: &str| staged_lib_in(&staging(line)).map(|(n, _)| n);
    assert_eq!(
        named("stage_cargo_lib_binary \"$LIB_STAGED\" \"$TESTS\" --features gpu").as_deref(),
        Some(lib)
    );
    assert_eq!(
        named("stage_cargo_lib_binary other_lib \"$TESTS\" --features gpu").as_deref(),
        Some("other_lib")
    );
    assert_eq!(named("# stage_cargo_lib_binary \"$LIB_STAGED\" \"$TESTS\" --features gpu"), None);
    assert_eq!(named("stage_cargo_lib_binary \"$LIB_STAGED\" \"$TESTS\""), None);
    assert_eq!(named("stage_cargo_lib_binary \"$LIB_STAGED\" \"$TESTS\" --features rust-only"), None);
    assert_eq!(named("stage_cargo_lib_binary \"$UNSET\" \"$TESTS\" --features gpu"), None);
}

/// CI's staging step builds the install through `build-test.sh --gpu --build`, which is
/// `stage-install.sh` in the build image: so `stage-install.sh`'s lists are what CI stages, and
/// the guards that read them read CI.
#[test]
fn the_pipeline_stages_through_stage_install() {
    let step: Vec<String> = fold_continuations(&gpu_job_step(GPU_STAGING_STEP))
        .into_iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    assert!(
        step.iter().any(|l| l.contains("scripts/build-test.sh --gpu --build")),
        "the `{GPU_STAGING_STEP}` step does not build through `scripts/build-test.sh --gpu \
         --build`, so {STAGE_INSTALL}'s GPU_TESTS and lib are not what CI stages, and every \
         guard reading them reads another runner:\n{}",
        step.join("\n")
    );
    let build_test = read(BUILD_TEST);
    assert!(
        build_test
            .lines()
            .any(|l| !l.trim_start().starts_with('#') && l.contains("stage=(scripts/env/stage-install.sh")),
        "{BUILD_TEST} --gpu --build no longer runs {STAGE_INSTALL}"
    );
}

/// `stage_cargo_lib_binary`, which [`staged_lib_in`] reads, stages peacockdb-core's own
/// unit-test binary: the artifact of kind `lib` named `peacockdb_core`, built `--lib`.
#[test]
fn the_staged_lib_is_the_crates_own_test_binary() {
    let body: Vec<String> = function_body(&read(STAGE_CARGO), "stage_cargo_lib_binary");
    assert!(
        body.iter()
            .any(|l| l.trim_start().starts_with("stage_cargo_binary peacockdb_core lib ") && l.contains("--lib")),
        "{STAGE_CARGO}'s stage_cargo_lib_binary no longer stages peacockdb_core's lib test \
         binary, built --lib: {body:?}"
    );
    assert!(
        function_body(&read(STAGE_CARGO), "stage_cargo_binary")
            .iter()
            .any(|l| l.contains("cargo test --no-run -p peacockdb-core")),
        "{STAGE_CARGO}'s stage_cargo_binary no longer builds peacockdb-core's test binaries"
    );
}

/// The GPU step reports a failure, and has no way to report success over one.
///
/// run-tests.sh runs without `set -e` for a binary — one binary failing must not skip the rest —
/// so the only thing that can fail it is what it folds into `rc` itself. Ways that goes wrong:
/// a binary's status never judged, the run not exiting `rc`, the step not waiting for the run,
/// and a command dying of a signal, which leaves no status to fold at all. The last shipped:
/// the rust loop's `cat` and `grep` segfaulted under an exported glibc-2.35 once per target, so
/// five binaries ran with their output discarded and their zero-tests guard answering from a
/// crash, job green.
#[test]
fn the_gpu_step_cannot_report_success_over_a_failure() {
    let step: Vec<String> = gpu_job_step(GPU_RUN_STEP)
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(str::to_string)
        .collect();
    let step = step.join("\n");
    assert!(
        step.contains(GPU_RUN) && !step.contains("--detach"),
        "the `{GPU_RUN_STEP}` step does not run `{GPU_RUN}`, attached. run-gpu.sh's `run list` \
         follows the run and exits as it did; detached, it returns once the container is up, and \
         the step is green before a single binary has run:\n{step}"
    );
    assert!(
        !step.contains("|| true") && !step.contains("; true"),
        "the `{GPU_RUN_STEP}` step swallows the runner's status:\n{step}"
    );
    assert!(
        !step.contains("export LD_LIBRARY_PATH"),
        "the `{GPU_RUN_STEP}` step exports a library path"
    );
    if let Err(why) = run_tests_cannot_report_success_over_a_failure(&read(RUN_TESTS)) {
        panic!("{RUN_TESTS} can report success over a failure: {why}");
    }
}

/// The failure reader's own guard: run-tests.sh as committed passes, and each way it could hide
/// a failure, made in a copy, is refused.
#[test]
fn the_run_tests_failure_reader_refuses_each_way_to_hide_a_failure() {
    let text = read(RUN_TESTS);
    assert_eq!(run_tests_cannot_report_success_over_a_failure(&text), Ok(()));
    let judged = "    judge \"$name\" \"$logs/$name.log\" \"$status\"\n";
    assert!(text.contains(judged), "the fixture's judge line is not run-tests.sh's");
    for (what, broken) in [
        ("exit 0", text.replacen("exit \"$rc\"", "exit 0", 1)),
        ("a judge line dropped", text.replacen(judged, "", 1)),
        (
            "the bench unjudged",
            text.replacen(
                "  judge \"$BENCH_TARGET\"",
                "  echo \"$BENCH_TARGET\"",
                1,
            ),
        ),
        ("no segfault read", text.replace("Segmentation fault|", "")),
        (
            "an exported library path",
            text.replacen("set -euo pipefail\n", "set -euo pipefail\nexport LD_LIBRARY_PATH=$LIBS\n", 1),
        ),
    ] {
        assert_ne!(broken, text, "{what}: the fixture changed nothing");
        assert!(
            run_tests_cannot_report_success_over_a_failure(&broken).is_err(),
            "{what}: accepted"
        );
    }
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
/// build-test-shadgpu.sh writes the loop inside an ssh heredoc (`"\$t"`); a loop run on the host
/// itself spells it `"$t"`, and both spellings are the same command.
pub(crate) fn is_rust_gpu_runner_invocation(line: &str) -> bool {
    let line = line.trim_start().replace("\\$", "$");
    !line.starts_with('#') && line.contains("\"$t\"") && line.contains("> \"$")
}

/// Single-tenant GPU is one flag on both committed runners and nothing else. cuDF and
/// RMM share a process-wide pool, so `--test-threads=1` is what keeps the device to one test
/// at a time — and `test_gpu_corpus.rs` spends it further, setting environment variables
/// in an `unsafe` block whose safety argument is that flag. Dropping it from either runner
/// makes that argument false, and until this test existed nothing said so.
#[test]
fn both_gpu_runners_pass_test_threads_one() {
    let text = read(RUN_TESTS);
    let args = run_tests_rust_args(&text).unwrap_or_else(|| {
        panic!(
            "{RUN_TESTS}'s rust loop or run_bench declares no `args=(` — the runner was \
             reshaped, and the single-tenant GPU invariant now rests on lines this guard \
             cannot find"
        )
    });
    for line in &args {
        assert!(
            line.contains("--test-threads=1"),
            "{RUN_TESTS} gives a staged GPU binary arguments without --test-threads=1:\n  {line}\n\
             cuDF/RMM share one process-wide pool, so concurrent cases OOM the device, and \
             the env-var writes in test_gpu_corpus.rs are sound only while this flag holds."
        );
    }
    for line in run_tests_rust_runs(&text) {
        assert!(
            line.contains("\"${args[@]}\""),
            "{RUN_TESTS} runs a rust GPU binary without its arguments, --test-threads=1 among \
             them:\n  {line}"
        );
    }
    for line in rust_gpu_runner_invocations(SHADGPU) {
        assert!(
            line.contains("--test-threads=1"),
            "{SHADGPU} runs a staged GPU binary without --test-threads=1:\n  {line}\n\
             cuDF/RMM share one process-wide pool, so concurrent cases OOM the device, and \
             the env-var writes in test_gpu_corpus.rs are sound only while this flag holds."
        );
    }
}

/// The measurement binary is staged with the GPU targets and runs on every job, but never
/// its `bench_` cases: those are tens of minutes of device time and would write a benchmark
/// tree from a correctness run. That is `--skip bench_` in both committed runner loops and
/// nothing else, so a runner that lost it turns the gate into a measurement in silence.
#[test]
fn the_benchmark_binary_runs_without_its_cases_on_ci() {
    let lost = "does not skip the benchmark binary's bench_ cases. Every gate run would then \
                spend the measurement's device time and overwrite the committed tree with times \
                nobody asked for.";
    let text = read(RUN_TESTS);
    assert_eq!(
        assigned(&text, "BENCH_TARGET").as_deref(),
        Some(BENCHMARK_TARGET),
        "{RUN_TESTS}'s BENCH_TARGET is not {BENCHMARK_TARGET}"
    );
    assert!(
        run_tests_rust_loop(&text).iter().any(|l| {
            l.contains("[ \"$name\" = \"$BENCH_TARGET\" ]") && l.contains("args+=(--skip bench_)")
        }),
        "{RUN_TESTS}'s rust loop {lost}"
    );

    // build-test-shadgpu.sh names the target through `$BENCH_TARGET`; resolved, so one needle
    // reads it.
    let loop_body = rust_gpu_runner_loop(SHADGPU)
        .join("\n")
        .replace("$BENCH_TARGET", BENCHMARK_TARGET);
    assert!(
        loop_body.contains(&format!("{BENCHMARK_TARGET}) skip=\"--skip bench_\"")),
        "{SHADGPU}'s rust-tests loop {lost}"
    );
    for line in rust_gpu_runner_invocations(SHADGPU) {
        assert!(
            line.contains("$skip"),
            "{SHADGPU} runs a staged GPU binary without the per-binary skip:\n  {line}\n\
             The case above it sets it and this line is where it takes effect."
        );
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

    // run-tests.sh's shape: the flag is in `args=(`, which every run_binary passes. The same
    // three ways: (1) the flag dropped from args, the comment left; (2) the C++ loop's
    // run_binary, which takes no args, not read as a rust run; (3) a second `args=(` in the loop.
    let run_tests = |rust_args: &str, bench_args: &str| {
        format!(
            "run_list() {{\n\
             \x20 for t in \"${{cpp[@]}}\"; do\n\
             \x20   run_binary \"$logs/$name.log\" \"$t\"\n\
             \x20 done\n\
             \x20 for t in \"${{rust[@]}}\"; do\n\
             \x20   # --test-threads=1: cuDF/RMM share one process-wide pool.\n\
             \x20   {rust_args}\n\
             \x20   run_binary \"$logs/$name.log\" \"$t\" \"${{args[@]}}\"\n\
             \x20 done\n\
             }}\n\
             run_bench() {{\n\
             \x20 local stamp {bench_args}\n\
             \x20 run_binary \"$logs/b.log\" X=1 \\\n\
             \x20   \"$bin\" \"${{args[@]}}\"\n\
             }}\n"
        )
    };
    let all_flagged = |text: &str| {
        run_tests_rust_args(text).is_some_and(|a| a.iter().all(|l| l.contains("--test-threads=1")))
    };
    let flag = "args=(--nocapture --test-threads=1)";
    let ok = run_tests(flag, flag);
    assert!(all_flagged(&ok), "{ok}");
    assert_eq!(run_tests_rust_runs(&ok).len(), 2, "the loop's run and the folded bench run");
    let dropped = run_tests("args=(--nocapture)", flag);
    assert!(dropped.contains("--test-threads=1") && !all_flagged(&dropped));
    assert!(!all_flagged(&run_tests(flag, "args=(--nocapture)")), "the bench's own args");
    let retry = run_tests(&format!("{flag}\n    args=(--nocapture --ignored)"), flag);
    assert!(!all_flagged(&retry), "the second args= in the loop is read");
    assert!(
        run_tests_rust_args(&run_tests("x=1", flag)).is_none(),
        "a loop with no args= is not read as flagged"
    );
}
