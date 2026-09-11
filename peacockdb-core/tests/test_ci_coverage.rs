//! Asserts every integration-test target is actually NAMED by a CI workflow step.
//!
//! Why this exists. CI does not sweep Rust test targets as a set — pipeline.yml lists
//! each `cargo test ... --test <name>` by hand. So a new `tests/test_*.rs` is invisible
//! to CI until someone remembers to add it, and `cargo test` locally still runs it,
//! which makes the gap look like coverage (it has happened: a guard shipped able to go
//! red locally but not at the merge gate, and a C++ test binary was built and shipped
//! but never executed). The C++ side got a glob + ran-any assertion (gpu-tests job);
//! Rust cannot glob (targets are named in Cargo/CI), so this test is the equivalent
//! guard: it fails when a target exists that no workflow runs.
//!
//! If this fails you have two honest options — wire the target into pipeline.yml, or
//! add it to `INTENTIONALLY_NOT_IN_CI` below WITH a reason. Deleting the test to make
//! the failure go away recreates the exact hole it exists to close.

// A test target's child modules resolve against tests/ itself, and a file there would be
// another target. The path keeps them under a directory named for this one.
#[path = "test_ci_coverage/runners.rs"]
mod runners;

use std::collections::BTreeSet;

use runners::{
    GPU_RUN_STEP, GPU_STAGING_STEP, gpu_job_lib_rung, gpu_job_staged_by_name, gpu_job_staged_lib,
    gpu_job_staged_targets, is_rust_gpu_runner_invocation, lib_rung_of,
    rust_gpu_runner_invocations, staged_by_name_of,
};

/// Why a target is absent from the normal CI tiers.
///
/// An enum rather than a free-text reason plus a bool, because the two kinds differ in
/// what can be CHECKED: [`Exemption::GpuJob`] makes a claim about a committed workflow
/// array, so the claim is verified below; [`Exemption::NotRun`] asserts only that
/// nothing runs the target, which nothing can confirm.
#[derive(Debug)]
pub(crate) enum Exemption {
    /// Run on the GPU host by pipeline.yml's gpu-tests job, from a prebuilt binary
    /// rather than via cargo. VERIFIED: the target must appear in that job's
    /// `for t in …` staging array. Without that check, dropping a target from the
    /// array retires it silently while this exemption still excuses it — an execution
    /// mode disappearing with nothing red, which is the failure the pipeline comment
    /// warns about in prose but nothing enforced.
    GpuJob,
    /// Not run by any workflow, for the stated reason.
    NotRun(&'static str),
}

/// Targets deliberately absent from the CI tiers this guard sweeps.
pub(crate) const INTENTIONALLY_NOT_IN_CI: &[(&str, Exemption)] = &[
    ("test_gpu_corpus", Exemption::GpuJob),
    ("test_ci_coverage", Exemption::NotRun("this test")),
];

pub(crate) fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Join backslash-continued shell lines into one logical line.
///
/// A shell command in a workflow may be split across `\` continuations, and YAML makes
/// that idiomatic for long `cargo test` invocations. [`line_runs_target`] decides
/// `--no-run` per line, so an unfolded build invocation hands it continuation lines
/// that carry `--test` flags but not the `--no-run` that disqualifies them — they read
/// as run steps and the guard reports coverage that does not exist.
///
/// Folding here rather than requiring one physical line per invocation: continuations
/// are legitimate YAML and the next person will reintroduce them.
pub(crate) fn fold_continuations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        match trimmed.strip_suffix('\\') {
            // Keep a separator, or `--test a \` + `--test b` would fuse into one token.
            Some(head) => {
                acc.push_str(head);
                acc.push(' ');
            }
            None => {
                acc.push_str(trimmed);
                out.push(std::mem::take(&mut acc));
            }
        }
    }
    // A trailing continuation with no terminating line still has to be emitted.
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

/// Does this workflow line RUN the lib unit tests (`--lib`, not `--no-run`)?
///
/// The inline `#[cfg(test)]` modules are a target class this guard was blind to: every
/// other invocation in the workflows passes `--test`, which selects integration
/// targets ONLY, so the crate's own unit tests ran locally and never
/// at the merge gate. Being invisible to the guard AND to CI is the same hole one
/// level down — a target class nothing enumerates.
fn line_runs_lib_tests(line: &str) -> bool {
    if line.contains("--no-run") {
        return false;
    }
    // Word-boundary check, same reasoning as line_runs_target: `--library` or a longer
    // flag starting with `--lib` must not count.
    let Some(i) = line.find("--lib") else {
        return false;
    };
    let after_ok = line[i + "--lib".len()..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace);
    after_ok && line.contains("cargo test") && line.contains("-p peacockdb-core")
}

/// The `--features` value a cargo line passes; `None` is the default set.
pub(crate) fn cargo_features(line: &str) -> Option<&str> {
    let mut toks = line.split_whitespace();
    toks.find(|t| *t == "--features")?;
    toks.next()
}

/// The path filter a `cargo test … -- <filter>` line hands libtest: the first token after
/// `--` that is not a flag. `None` runs the binary whole.
fn lib_run_filter(line: &str) -> Option<&str> {
    let mut toks = line.split_whitespace();
    toks.find(|t| *t == "--")?;
    toks.find(|t| !t.starts_with('-'))
}

/// Does this line run the cpu rung — the lib whole, under `--features rust-only`?
///
/// Each rung is one `--lib` line, so [`line_runs_lib_tests`] alone cannot tell them apart:
/// with the cpu line deleted the ffi line still satisfied it, and the rung was gone with
/// nothing red. The rung is the feature set and the filter together.
fn line_runs_cpu_rung(line: &str) -> bool {
    line_runs_lib_tests(line)
        && cargo_features(line) == Some("rust-only")
        && lib_run_filter(line).is_none()
}

/// Does this line run the ffi rung — `--lib -- ffi_tests::` at default features?
fn line_runs_ffi_rung(line: &str) -> bool {
    line_runs_lib_tests(line)
        && cargo_features(line).is_none()
        && lib_run_filter(line) == Some("ffi_tests::")
}

/// Does this workflow line BUILD the `peacockdb` CLI?
///
/// The bin target is the same hole one level down again: it has no test target, so the
/// `--test` sweep cannot see it, and it is the only caller of the planner and the driver
/// from outside the crate. `cargo build` rather than `cargo test --no-run`, since the
/// crate has no tests for the latter to name.
fn line_builds_the_cli(line: &str) -> bool {
    let Some(i) = line.find("-p peacockdb") else {
        return false;
    };
    let after_ok = line[i + "-p peacockdb".len()..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace);
    after_ok && line.contains("cargo build")
}

pub(crate) const PIPELINE: &str = ".github/workflows/pipeline.yml";

/// Does this workflow line actually RUN `--test <name>`?
///
/// Matching is line-wise and deliberately strict, because a coverage guard that
/// reports FALSE coverage is worse than no guard at all. Two ways a naive
/// `workflows.contains("--test {name}")` lies:
///
///   - PREFIX COLLISION. `--test test_corpus` is a substring of
///     `--test test_corpus_goldens`, so the shorter name reads as covered by the
///     longer one's step. No two targets are a prefix pair today — the pair that made
///     this concrete went with the legacy tiers — so this is the guard holding a
///     property rather than fixing a live hole, and the next such pair inherits it.
///     Fixed by requiring a word boundary (whitespace or end-of-line) after the name.
///   - `--no-run` BLINDNESS. `cargo test --no-run ... --test X` BUILDS X without
///     running it. A target named only in such a step is "wired" while never
///     executing — precisely the built-but-never-run hole (peacock_tpchv_tests) that
///     this guard exists to close. Fixed by skipping those lines entirely.
///
/// `--no-run` is detected per LINE, so callers MUST pass lines that have already been
/// through [`fold_continuations`]. A build invocation split across `\` continuations
/// carries `--no-run` only on its first physical line, and the continuation lines then
/// read as genuine run steps — which is exactly how this guard silently weakened once
/// (five targets were counted as run by a build continuation).
fn line_runs_target(line: &str, name: &str) -> bool {
    if line.contains("--no-run") {
        return false;
    }
    let needle = format!("--test {name}");
    let mut from = 0;
    while let Some(i) = line[from..].find(&needle) {
        let end = from + i + needle.len();
        match line[end..].chars().next() {
            // end-of-line, or a separator -> a real, whole-name mention
            None => return true,
            Some(c) if c.is_whitespace() => return true,
            // otherwise this was a longer target name that merely starts with `name`
            _ => {}
        }
        from = end;
    }
    false
}

/// The matcher's own guard. Both cases below PASS under the naive
/// `workflows.contains("--test {name}")` this replaced, which is the point:
/// without these, a regression back to substring matching is invisible.
#[test]
fn line_matcher_rejects_both_false_coverage_modes() {
    // (1) prefix collision — a longer target name must not cover a shorter one.
    let only_misc = "          cargo test -p peacockdb-core --test test_corpus_goldens";
    assert!(
        !line_runs_target(only_misc, "test_corpus"),
        "prefix collision: `--test test_corpus_goldens` must NOT count as running \
         test_corpus"
    );
    assert!(line_runs_target(only_misc, "test_corpus_goldens"));

    // (2) --no-run blindness — building a target is not running it.
    let build_only =
        "          cargo test --no-run -p peacockdb-core --test test_corpus --test test_ffi";
    assert!(
        !line_runs_target(build_only, "test_corpus"),
        "--no-run builds without running; it must not count as CI coverage"
    );

    // A genuine run step still counts, including at end-of-line and mid-line.
    assert!(line_runs_target(
        "cargo test -p x --test test_corpus",
        "test_corpus"
    ));
    assert!(line_runs_target(
        "cargo test -p x --test test_corpus --test test_ffi",
        "test_corpus"
    ));

    // (3) LINE CONTINUATION — the mode that actually shipped. A --no-run build split
    // across `\` carries the flag only on its first physical line, so every target
    // named on a continuation looked like a run step. Five real targets were counted
    // that way; coverage survived only because each ALSO had a genuine run line, i.e.
    // the guard had stopped guarding while still reporting green.
    let build_continued = "          cargo test --no-run -p peacockdb-core --test test_a \\\n\
                           --test test_b --test test_c";
    let folded = fold_continuations(build_continued);
    assert_eq!(
        folded.len(),
        1,
        "the continuation must fold into ONE logical line: {folded:?}"
    );
    for t in ["test_a", "test_b", "test_c"] {
        assert!(
            !folded.iter().any(|l| line_runs_target(l, t)),
            "{t} is BUILT, not run — a continuation must not count as coverage"
        );
    }
    // CLI detection: the package name needs a word boundary, and a test invocation
    // naming the core crate is not a build of the bin.
    assert!(line_builds_the_cli(
        "          cargo build --features rust-only -p peacockdb"
    ));
    assert!(
        !line_builds_the_cli("          cargo build --features rust-only -p peacockdb-core"),
        "`-p peacockdb-core` must not read as a build of the `peacockdb` bin"
    );
    assert!(
        !line_builds_the_cli("          cargo test --no-run -p peacockdb"),
        "a test invocation is not the bin build; the crate has no test target"
    );

    // The rust GPU runner reader: it must find the line that runs the binary however that
    // line is prefixed, and must not mistake the loop's executable test for it — that line
    // carries no flags, so matching it would fail the assertion on a correct runner.
    assert!(is_rust_gpu_runner_invocation(
        r#"    env LD_LIBRARY_PATH="\$PATCHED_LD" "\$t" --nocapture --test-threads=1 > "\$tlog" 2>&1"#
    ));
    assert!(is_rust_gpu_runner_invocation(
        r#"    "\$t" --nocapture --test-threads=1 > "\$rlog" 2>&1"#
    ));
    assert!(
        !is_rust_gpu_runner_invocation(r#"    [ -x "\$t" ] || continue"#),
        "the loop's executable test is not the invocation"
    );
    assert!(
        !is_rust_gpu_runner_invocation(r#"    # "\$t" --test-threads=1 > "\$tlog""#),
        "a commented-out invocation is not one"
    );

    // --lib detection: a build is not a run, and the flag needs a word boundary.
    assert!(line_runs_lib_tests(
        "          cargo test --features rust-only -p peacockdb-core --lib"
    ));
    assert!(
        !line_runs_lib_tests(
            "          cargo test --no-run --features rust-only -p peacockdb-core --lib --test test_cpu_corpus"
        ),
        "--no-run builds the lib target without running it"
    );
    assert!(
        !line_runs_lib_tests("          cargo test -p peacockdb-core --test test_corpus"),
        "an integration-only invocation does not run the lib tests"
    );

    // The two lib rungs are both `--lib` runs, so each must fail the other's matcher or
    // deleting one line leaves the other satisfying both — which is how the cpu rung's
    // assertion was green with its line gone.
    let cpu = "          cargo test --features rust-only -p peacockdb-core --lib";
    let ffi = "          cargo test -p peacockdb-core --lib -- ffi_tests::";
    assert!(
        line_runs_cpu_rung(cpu) && !line_runs_ffi_rung(cpu),
        "the cpu rung line is only the cpu rung"
    );
    assert!(
        line_runs_ffi_rung(ffi) && !line_runs_cpu_rung(ffi),
        "the ffi rung line is only the ffi rung"
    );
    assert!(
        !line_runs_cpu_rung(
            "          cargo test --features rust-only -p peacockdb-core --lib -- planner::"
        ),
        "a filtered rust-only run is not the whole cpu rung"
    );
    assert!(
        !line_runs_cpu_rung("          cargo test -p peacockdb-core --lib"),
        "the lib whole at default features links the FFI — it is not the rust-only rung"
    );
    assert!(
        !line_runs_ffi_rung(
            "          cargo test --features rust-only -p peacockdb-core --lib -- ffi_tests::"
        ),
        "ffi_tests:: under rust-only selects nothing — the rung is the default feature set"
    );
    assert!(
        !line_runs_ffi_rung(
            "          cargo test -p peacockdb-core --lib -- --test-threads=1 ffi_tests::x"
        ),
        "a filter that merely starts with the rung's path is a narrower selection"
    );

    // The device rung's two readers. The staging step's loop stages `"$t"` and defines
    // `stage()`; neither is a file named outright. The run loop's `[ -x "$t" ]` test and
    // a commented-out assignment are not the rung line.
    let lib_line = r#"stage peacockdb_core_gpu_lib "$(cargo test --no-run -p peacockdb-core --lib \
                       --features gpu --message-format=json | resolve peacockdb_core lib)""#;
    let lib_line = &fold_continuations(lib_line)[0];
    assert_eq!(
        staged_by_name_of(lib_line).map(|(n, _)| n).as_deref(),
        Some("peacockdb_core_gpu_lib")
    );
    let loop_line =
        r#"stage "$t" "$(cargo test --no-run -p peacockdb-core --test "$t" --features gpu)""#;
    assert!(staged_by_name_of(loop_line).is_none());
    assert!(staged_by_name_of("stage() {").is_none());
    assert_eq!(
        lib_rung_of(r#"[ "\$tname" = peacockdb_core_gpu_lib ] && rung=gpu_tests::"#),
        Some((
            "peacockdb_core_gpu_lib".to_string(),
            "gpu_tests::".to_string()
        ))
    );
    assert!(lib_rung_of(r#"[ -x "\$t" ] || continue"#).is_none());
    assert!(
        lib_rung_of(r#"# [ "\$tname" = peacockdb_core_gpu_lib ] && rung=gpu_tests::"#).is_none()
    );

    // ...while a continued RUN step still counts, on any of its physical lines.
    let run_continued = "          cargo test -p peacockdb-core --test test_a \\\n\
                         --test test_b";
    let folded = fold_continuations(run_continued);
    for t in ["test_a", "test_b"] {
        assert!(folded.iter().any(|l| line_runs_target(l, t)), "{t} IS run");
    }
}

/// A step gated on a matrix leg names a leg the matrix declares.
///
/// The gate and the leg list are two spellings of one value in one file, and only one of them
/// is checked by anything: `every_rust_test_target_is_named_by_ci` asserts a target is NAMED
/// by a step, never that the step can RUN. Relabel a leg or drop one and the steps gated on
/// it stop running with that meta-test still green — and [#129](tickets.md#t129) records that
/// the leg labels already disagree with their images, so the label is the untrustworthy half
/// of a pair that seven steps now depend on.
#[test]
fn every_matrix_gated_step_names_a_leg_the_matrix_declares() {
    let text = std::fs::read_to_string(repo_root().join(PIPELINE)).expect("read pipeline.yml");
    let declared: BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- cudf: "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .collect();
    assert!(
        !declared.is_empty(),
        "pipeline.yml declares no `- cudf:` legs — the matrix was reshaped"
    );

    let mut gated = 0;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("if:") else {
            continue;
        };
        let Some(at) = rest.find("matrix.cudf == ") else {
            continue;
        };
        let value = rest[at + "matrix.cudf == ".len()..]
            .trim_start_matches('\'')
            .split('\'')
            .next()
            .unwrap_or("");
        assert!(
            declared.contains(value),
            "a step is gated on cudf {value:?} and the matrix declares {declared:?} — the steps \
             behind that gate silently stop running, and the coverage guard stays green"
        );
        gated += 1;
    }
    assert!(
        gated > 0,
        "no step is gated on a matrix leg — this test now proves nothing"
    );
}

/// Every integration-test target in the WORKSPACE, as `(crate, target)`.
///
/// Two scoping bugs this closes, both of which let a target escape the gate by being
/// somewhere or something the enumeration did not think to look for:
///   - ONE CRATE. This used to read `CARGO_MANIFEST_DIR/tests` only, so
///     peacockdb-ffi's targets were invisible. They happen to be wired, but a new one
///     would never have been noticed. Crates come from the workspace manifest, so
///     adding a member cannot silently shrink the guard's scope.
///   - THE `test_` PREFIX. This used to collect only stems starting with `test_`,
///     which made a naming convention load-bearing and unenforced: `tests/audit_foo.rs`
///     is a real cargo target and was invisible purely because of its name. Every
///     `tests/*.rs` counts now.
pub(crate) fn workspace_test_targets() -> BTreeSet<(String, String)> {
    let root = repo_root();
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("read workspace Cargo.toml");
    // Members are the authority; a hardcoded crate list here would be a second source
    // of truth and would drift exactly as the single-crate glob did.
    let members: Vec<String> = manifest
        .lines()
        .skip_while(|l| !l.starts_with("[workspace]"))
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .filter_map(|l| {
            l.trim()
                .trim_end_matches(',')
                .strip_prefix('"')?
                .strip_suffix('"')
                .map(str::to_string)
        })
        .collect();
    assert!(
        !members.is_empty(),
        "no [workspace] members parsed from Cargo.toml"
    );

    let mut targets = BTreeSet::new();
    for m in &members {
        let dir = root.join(m).join("tests");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // `common/` is a shared module dir, not a target; only *.rs files are.
            targets.insert((
                m.clone(),
                path.file_stem().unwrap().to_string_lossy().to_string(),
            ));
        }
    }
    targets
}

/// Every line of every workflow, folded. Every workflow rather than pipeline.yml alone:
/// a target named by any of them counts. Folded, not raw: see [`fold_continuations`] — a
/// build invocation split across `\` would otherwise contribute continuation lines that
/// look like run steps.
fn workflow_lines() -> Vec<String> {
    let wf_dir = repo_root().join(".github/workflows");
    let mut lines: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&wf_dir).expect("read .github/workflows/") {
        let path = entry.expect("dir entry").path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml") | Some("yaml")
        ) {
            let text = std::fs::read_to_string(&path).expect("read workflow");
            lines.extend(fold_continuations(&text));
        }
    }
    assert!(
        !lines.is_empty(),
        "no workflow files found under .github/workflows"
    );
    lines
}

#[test]
fn every_rust_test_target_is_named_by_ci() {
    let all = workspace_test_targets();
    let targets: BTreeSet<String> = all.iter().map(|(_, t)| t.clone()).collect();
    assert!(
        !targets.is_empty(),
        "found no tests/*.rs in any workspace crate — the enumeration is wrong, not the repo"
    );
    let workflow_lines = workflow_lines();

    let exempt: BTreeSet<&str> = INTENTIONALLY_NOT_IN_CI.iter().map(|(n, _)| *n).collect();

    let missing: Vec<&String> = targets
        .iter()
        .filter(|t| !exempt.contains(t.as_str()))
        // Must be RUN by some line: `--test <name>` at a word boundary, in a step
        // that is not `--no-run`. See line_runs_target for why both matter.
        .filter(|t| !workflow_lines.iter().any(|l| line_runs_target(l, t)))
        .collect();

    assert!(
        missing.is_empty(),
        "these test targets exist but NO CI workflow runs them, so they cannot go red at the \
         merge gate:\n{}\n\nWire each into .github/workflows/pipeline.yml, or add it to \
         INTENTIONALLY_NOT_IN_CI with a reason.",
        missing
            .iter()
            .map(|t| format!("  - {t}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // F5: a GpuJob exemption CLAIMS the gpu-tests job runs the target. Verify that
    // against the committed workflow text rather than trusting the claim. The array is
    // read from pipeline.yml, never copied here — a hardcoded list would be a second
    // source of truth and would drift exactly as this exemption did.
    let gpu_staging = gpu_job_staged_targets();
    let unstaged: Vec<&str> = INTENTIONALLY_NOT_IN_CI
        .iter()
        .filter(|(_, e)| matches!(e, Exemption::GpuJob))
        .map(|(n, _)| *n)
        .filter(|n| !gpu_staging.contains(*n))
        .collect();
    assert!(
        unstaged.is_empty(),
        "these targets are exempt on the grounds that the gpu-tests job runs them, but \
         they do NOT appear in its staging array in pipeline.yml: {unstaged:?}\n\
         Either add them back to that array or change their exemption — as it stands \
         they run nowhere and nothing would go red.\nArray currently names: {:?}",
        gpu_staging
    );

    // Keep the exemption list honest: an entry for a target that no longer exists is
    // stale and would silently excuse a future target that reuses the name. Report the
    // stated reason alongside, because that is what the reader has to judge — "is this
    // claim still true?" is answerable, "is `test_foo` still exempt?" is not.
    let stale: Vec<String> = INTENTIONALLY_NOT_IN_CI
        .iter()
        .filter(|(n, _)| !targets.contains(*n))
        .map(|(n, e)| match e {
            Exemption::GpuJob => format!("{n} (exempt as: run by the gpu-tests job)"),
            Exemption::NotRun(why) => format!("{n} (exempt as: {why})"),
        })
        .collect();
    assert!(
        stale.is_empty(),
        "INTENTIONALLY_NOT_IN_CI names targets that no longer exist — remove them:\n  {}",
        stale.join("\n  ")
    );
}

/// One CI line per rung, plus the CLI build. The ladder puts each rung's test modules in
/// the one lib binary, selected by feature set and path filter, so no `--test` sweep can
/// see them and nothing else says a rung stopped running. The device rung is not a command
/// line at all — shad-gpu runs prebuilt binaries — so it is read as the staged lib binary
/// plus the loop line that hands that one file `gpu_tests::`. Each assertion here is the
/// only thing between its rung and silence; each was watched red with its line deleted.
#[test]
fn each_rung_has_its_ci_line_and_the_cli_is_built() {
    let lines = workflow_lines();

    assert!(
        lines.iter().any(|l| line_runs_cpu_rung(l)),
        "no workflow line runs the cpu rung — the lib whole under --features rust-only. \
         Every in-crate test module that needs neither the FFI nor a device rides in it, \
         and every other cargo invocation passes --test. Add `cargo test --features \
         rust-only -p peacockdb-core --lib` to dataset-matrix."
    );
    assert!(
        lines.iter().any(|l| line_runs_ffi_rung(l)),
        "no workflow line runs the ffi rung — `--lib -- ffi_tests::` at default features. \
         The modules gated `not(feature = \"rust-only\")` compile only there and select \
         only by that path. Add `cargo test -p peacockdb-core --lib -- ffi_tests::` to \
         dataset-matrix."
    );

    let (staged, line) = gpu_job_staged_lib().unwrap_or_else(|| {
        panic!(
            "the `{GPU_STAGING_STEP}` step stages no lib binary: no `stage <name> \"$(cargo \
             test --no-run … --lib --features gpu …)\"` line. The device rung is the \
             `gpu_tests` modules of that one binary, so without it nothing runs them. \
             Lines naming a file outright: {:?}",
            gpu_job_staged_by_name()
                .iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
        )
    });
    assert!(
        line.contains("-p peacockdb-core"),
        "the staged lib is not peacockdb-core's: {line}"
    );
    let (keyed, rung) = gpu_job_lib_rung().unwrap_or_else(|| {
        panic!(
            "the `{GPU_RUN_STEP}` loop hands no file a rung: no `[ \"$tname\" = <name> ] && \
             rung=<filter>` line. The staged lib `{staged}` holds every rung, so unfiltered \
             it runs the cpu cases on the device and filtered by nothing it runs the device \
             cases nowhere — the zero-test guard sees the second, not the first."
        )
    });
    assert_eq!(
        (keyed.as_str(), rung.as_str()),
        (staged.as_str(), "gpu_tests::"),
        "the run loop keys the rung on a different file, or a different rung, from the lib \
         the staging step ships — `{staged}` is staged, `{keyed}` gets `{rung}`"
    );
    for inv in rust_gpu_runner_invocations(PIPELINE) {
        assert!(
            inv.contains("\\$rung"),
            "a rust invocation in the `{GPU_RUN_STEP}` loop does not pass $rung, so the \
             assignment above it reaches nothing: {inv}"
        );
    }

    assert!(
        lines.iter().any(|l| line_builds_the_cli(l)),
        "no workflow line builds the peacockdb CLI. It is the only caller of the planner \
         and the driver from outside the crate, and it has no test target, so nothing \
         else compiles it. Add `cargo build --features rust-only -p peacockdb` to \
         dataset-matrix."
    );
}
