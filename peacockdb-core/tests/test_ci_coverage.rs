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

use std::collections::BTreeSet;

/// Why a target is absent from the normal CI tiers.
///
/// An enum rather than a free-text reason plus a bool, because the two kinds differ in
/// what can be CHECKED: [`Exemption::GpuJob`] makes a claim about a committed workflow
/// array, so the claim is verified below; [`Exemption::NotRun`] asserts only that
/// nothing runs the target, which nothing can confirm.
#[derive(Debug)]
enum Exemption {
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
const INTENTIONALLY_NOT_IN_CI: &[(&str, Exemption)] = &[
    ("test_inc2_conformance", Exemption::GpuJob),
    ("test_gpu_abi", Exemption::GpuJob),
    ("test_gpu_recipe_walk", Exemption::GpuJob),
    ("test_gpu_executors", Exemption::GpuJob),
    ("test_gpu_corpus", Exemption::GpuJob),
    ("test_ci_coverage", Exemption::NotRun("this test")),
];

fn repo_root() -> std::path::PathBuf {
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
fn fold_continuations(text: &str) -> Vec<String> {
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
    let Some(i) = line.find("--lib") else { return false };
    let after_ok = line[i + "--lib".len()..].chars().next().is_none_or(char::is_whitespace);
    after_ok && line.contains("cargo test") && line.contains("-p peacockdb-core")
}

/// Does this workflow line BUILD the `peacockdb` CLI?
///
/// The bin target is the same hole one level down again: it has no test target, so the
/// `--test` sweep cannot see it, and it is the only caller of the planner and the driver
/// from outside the crate. `cargo build` rather than `cargo test --no-run`, since the
/// crate has no tests for the latter to name.
fn line_builds_the_cli(line: &str) -> bool {
    let Some(i) = line.find("-p peacockdb") else { return false };
    let after_ok =
        line[i + "-p peacockdb".len()..].chars().next().is_none_or(char::is_whitespace);
    after_ok && line.contains("cargo build")
}

/// The test targets pipeline.yml's gpu-tests job stages and runs, read out of the
/// committed workflow (`for t in <names>; do`).
///
/// Parsed rather than duplicated: this is the array a [`Exemption::GpuJob`] entry
/// points at, so a copy here would defeat the check it exists to make.
fn gpu_job_staged_targets() -> BTreeSet<String> {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("for t in test_"))
        .expect(
            "pipeline.yml has no `for t in test_…; do` staging loop — the gpu-tests \
             staging step was renamed or removed, and every GpuJob exemption now rests \
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

/// The GPU test binaries `scripts/build-test-shadgpu.sh` stages, from its `RUST_TESTS`
/// array — what a developer's own run ships to the host.
fn shadgpu_staged_targets() -> BTreeSet<String> {
    let text = std::fs::read_to_string(repo_root().join("scripts/build-test-shadgpu.sh"))
        .expect("read build-test-shadgpu.sh");
    let start = text.find("RUST_TESTS=(").expect(
        "build-test-shadgpu.sh has no RUST_TESTS=( array — the staging list was renamed, \
         and the check that it matches CI now rests on a list that does not exist",
    ) + "RUST_TESTS=(".len();
    let end = start + text[start..].find(')').expect("unterminated RUST_TESTS array");
    text[start..end].split_whitespace().map(str::to_string).collect()
}

/// The targets `scripts/build-test.sh` treats as needing a device at run time, from its
/// `gpu_runtime_targets()` heredoc, as `(crate, target)`. That set is SUBTRACTED from the
/// CPU modes and matched WHOLE against `<crate>:<target>`, so the crate is kept rather than
/// discarded: a line with the wrong crate subtracts nothing, and the target it names then
/// runs on a machine with no GPU.
fn gpu_runtime_targets() -> BTreeSet<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join("scripts/build-test.sh"))
        .expect("read build-test.sh");
    let start = text.find("<<'GPUSET'").expect(
        "build-test.sh has no GPUSET heredoc — gpu_runtime_targets() was renamed or \
         reshaped, and both the mode ladder and this check depend on its contents",
    ) + "<<'GPUSET'".len();
    let end = start + text[start..].find("\nGPUSET").expect("unterminated GPUSET heredoc");
    text[start..end]
        .lines()
        .filter_map(|l| l.trim().split_once(':').map(|(c, t)| (c.to_string(), t.to_string())))
        .collect()
}

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
    assert!(line_runs_target("cargo test -p x --test test_corpus", "test_corpus"));
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
    assert_eq!(folded.len(), 1, "the continuation must fold into ONE logical line: {folded:?}");
    for t in ["test_a", "test_b", "test_c"] {
        assert!(
            !folded.iter().any(|l| line_runs_target(l, t)),
            "{t} is BUILT, not run — a continuation must not count as coverage"
        );
    }
    // CLI detection: the package name needs a word boundary, and a test invocation
    // naming the core crate is not a build of the bin.
    assert!(line_builds_the_cli("          cargo build --features rust-only -p peacockdb"));
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
    assert!(is_rust_gpu_runner_invocation(r#"    "\$t" --nocapture --test-threads=1 > "\$rlog" 2>&1"#));
    assert!(
        !is_rust_gpu_runner_invocation(r#"    [ -x "\$t" ] || continue"#),
        "the loop's executable test is not the invocation"
    );
    assert!(
        !is_rust_gpu_runner_invocation(r#"    # "\$t" --test-threads=1 > "\$tlog""#),
        "a commented-out invocation is not one"
    );

    // --lib detection: a build is not a run, and the flag needs a word boundary.
    assert!(line_runs_lib_tests("          cargo test --features rust-only -p peacockdb-core --lib"));
    assert!(!line_runs_lib_tests(
        "          cargo test --no-run --features rust-only -p peacockdb-core --lib --test test_cpu_executors"
    ), "--no-run builds the lib target without running it");
    assert!(!line_runs_lib_tests("          cargo test -p peacockdb-core --test test_corpus"),
            "an integration-only invocation does not run the lib tests");

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
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let declared: BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- cudf: "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .collect();
    assert!(!declared.is_empty(), "pipeline.yml declares no `- cudf:` legs — the matrix was reshaped");

    let mut gated = 0;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("if:") else { continue };
        let Some(at) = rest.find("matrix.cudf == ") else { continue };
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
    assert!(gated > 0, "no step is gated on a matrix leg — this test now proves nothing");
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
fn workspace_test_targets() -> BTreeSet<(String, String)> {
    let root = repo_root();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("read workspace Cargo.toml");
    // Members are the authority; a hardcoded crate list here would be a second source
    // of truth and would drift exactly as the single-crate glob did.
    let members: Vec<String> = manifest
        .lines()
        .skip_while(|l| !l.starts_with("[workspace]"))
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .filter_map(|l| l.trim().trim_end_matches(',').strip_prefix('"')?.strip_suffix('"').map(str::to_string))
        .collect();
    assert!(!members.is_empty(), "no [workspace] members parsed from Cargo.toml");

    let mut targets = BTreeSet::new();
    for m in &members {
        let dir = root.join(m).join("tests");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // `common/` is a shared module dir, not a target; only *.rs files are.
            targets.insert((m.clone(), path.file_stem().unwrap().to_string_lossy().to_string()));
        }
    }
    targets
}

#[test]
fn every_rust_test_target_is_named_by_ci() {
    let all = workspace_test_targets();
    let targets: BTreeSet<String> = all.iter().map(|(_, t)| t.clone()).collect();
    assert!(!targets.is_empty(), "found no tests/*.rs in any workspace crate — the enumeration is wrong, not the repo");

    // Read every workflow, not just pipeline.yml: a target named by any of them counts.
    let wf_dir = repo_root().join(".github/workflows");
    let mut workflow_lines: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&wf_dir).expect("read .github/workflows/") {
        let path = entry.expect("dir entry").path();
        if matches!(path.extension().and_then(|e| e.to_str()), Some("yml") | Some("yaml")) {
            let text = std::fs::read_to_string(&path).expect("read workflow");
            // Folded, NOT raw: see fold_continuations — a build invocation split
            // across `\` would otherwise contribute continuation lines that look
            // like run steps.
            workflow_lines.extend(fold_continuations(&text));
        }
    }
    assert!(!workflow_lines.is_empty(), "no workflow files found under .github/workflows");

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
        missing.iter().map(|t| format!("  - {t}")).collect::<Vec<_>>().join("\n")
    );

    // The lib unit tests are not an integration target, so the sweep above cannot see
    // them. Assert the run line exists directly, or deleting it silently un-gates the
    // inline #[cfg(test)] modules exactly as it did before this check.
    assert!(
        workflow_lines.iter().any(|l| line_runs_lib_tests(l)),
        "no workflow line runs the peacockdb-core LIB unit tests. Every other cargo \
         invocation passes --test, which selects integration targets only, so the \
         inline #[cfg(test)] modules — 435 cases across every component — would run \
         locally and never at the merge gate. Add `cargo test --features rust-only \
         -p peacockdb-core --lib` to the CPU tier."
    );

    // The CLI has no test target at all, so neither the sweep above nor the --lib check
    // reaches it. Asserted the same way and for the same reason.
    assert!(
        workflow_lines.iter().any(|l| line_builds_the_cli(l)),
        "no workflow line builds the peacockdb CLI. It is the only caller of \
         plan and the driver from outside the crate, and it has no test \
         target, so nothing else compiles it. Add `cargo build --features rust-only \
         -p peacockdb` to the CPU tier."
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

/// Three committed lists name the GPU test binaries — pipeline.yml's staging array,
/// build-test-shadgpu.sh's `RUST_TESTS`, and build-test.sh's `gpu_runtime_targets()` —
/// each read by a different runner, so a target in one and not the others is invisible in
/// exactly the direction that matters. The third is the dangerous one: the CPU modes are
/// built by SUBTRACTING it, whole-line, so an entry with the wrong crate or a stale target
/// name subtracts nothing and a file-gated GPU test RUNS on a host with no device.
///
/// Not checked: a target exempted as `NotRun` may be absent from `gpu_runtime_targets()`
/// with nothing red. No file says "needs a device" — that is what the hand-declared set is
/// for, and `test_gpu_executor_misc` is that shape today.
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
    let ci_qualified: BTreeSet<(String, String)> =
        ci.iter().map(|t| ("peacockdb-core".to_string(), t.clone())).collect();

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
}

/// The body of the `Run GPU tests` step, out of the committed workflow.
fn gpu_test_step() -> String {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let mut lines = text.lines().skip_while(|l| !l.contains("- name: Run GPU tests"));
    lines.next().unwrap_or_else(|| {
        panic!(
            "pipeline.yml has no `Run GPU tests` step — the GPU job was reshaped and the \
             guards below now read nothing"
        )
    });
    lines
        .take_while(|l| !l.trim_start().starts_with("- name:"))
        .collect::<Vec<_>>()
        .join("\n")
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
    let step = gpu_test_step();

    assert!(
        step.contains("exit \\$rc"),
        "the GPU step does not end by exiting the status it accumulated, so everything it \
         recorded in rc is discarded and the step is green whatever ran"
    );
    for line in rust_gpu_runner_invocations(".github/workflows/pipeline.yml") {
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
fn rust_gpu_runner_invocations(rel: &str) -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let mut after_header = text.lines().skip_while(|l| !is_rust_gpu_runner_loop_header(l));
    after_header.next().unwrap_or_else(|| {
        panic!(
            "{rel} has no `for t in …rust-tests/*` loop — the runner was reshaped, and the \
             single-tenant GPU invariant now rests on a loop this guard cannot find"
        )
    });
    let found: Vec<String> = after_header
        .take_while(|l| !l.trim_start().starts_with("done"))
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

fn is_rust_gpu_runner_loop_header(line: &str) -> bool {
    line.contains("for t in") && line.contains("rust-tests/")
}

/// Is this the line that executes the binary, rather than a comment or the `[ -x … ]` test?
///
/// Keyed on running `"$t"` AND redirecting to a log, not on the line starting with `"$t"`:
/// the patched glibc is applied per command (`env LD_LIBRARY_PATH=… "$t" …`) rather than
/// exported, and a reader anchored at the start of the line stops seeing the invocation the
/// moment anything precedes it — a guard silently reading nothing, not a red one.
fn is_rust_gpu_runner_invocation(line: &str) -> bool {
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
    for rel in [".github/workflows/pipeline.yml", "scripts/build-test-shadgpu.sh"] {
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
    assert!(dropped.contains("--test-threads=1"), "a file-wide search is green on this");
    assert!(!read(dropped).iter().any(|l| flagged(&l.as_str())), "the command has lost the flag");

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
    let first = both_loops.lines().find(|l| is_rust_gpu_runner_invocation(l)).expect("a line");
    assert!(!flagged(&first), "the C++ invocation is the file's first");
    let rust = read(both_loops);
    assert_eq!(rust.len(), 2, "both rust invocations are read: {rust:?}");
    assert!(flagged(&rust[0].as_str()) && !flagged(&rust[1].as_str()), "the retry is unflagged");
}
