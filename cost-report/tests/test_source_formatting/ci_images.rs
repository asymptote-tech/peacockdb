//! The CI jobs that build or run project code do it in the build image, through
//! `scripts/build-test.sh` (llm-wiki/build-test.md, "CI structure"). A bare `cargo` line in such
//! a job would build on the runner's own toolchain and pass there, so the image would stop being
//! what CI proves.

use crate::ci_workflow::{
    commands, job_lines, job_steps, logical_lines, run_scripts, unquoted, workflow_jobs,
};
use crate::repo_root;

/// (workflow, job): the jobs that run in the build image.
const IN_IMAGE_JOBS: &[(&str, &str)] = &[
    (".github/workflows/pipeline.yml", "cost-report"),
    (".github/workflows/pipeline.yml", "s3-datasets"),
    (".github/workflows/pipeline.yml", "dataset-matrix"),
    (".github/workflows/pipeline.yml", "cpp-build-2502"),
    (".github/workflows/exec-model-corpus.yml", "corpus"),
    (".github/workflows/exec-model-corpus.yml", "call-cost"),
    (".github/workflows/validate-large.yml", "validate-large"),
];

/// (workflow, job, reason): the jobs that run on the runner, each with why it needs no image.
/// Every job of every workflow is here or in IN_IMAGE_JOBS, so a new one cannot slip past both.
const HOST_JOBS: &[(&str, &str, &str)] = &[
    (
        ".github/workflows/pipeline.yml",
        "changes",
        "gh api only: it reads the pushed files' names, never the tree",
    ),
    (
        ".github/workflows/pipeline.yml",
        "deploy-pages",
        "publishes the Pages artifact cost-report made; checks out nothing",
    ),
    (
        ".github/workflows/pipeline.yml",
        "gpu-tests",
        "runs only the VM's door and its checks on the runner (RUNNER_SIDE), and the code in the \
         run image on the VM",
    ),
    (
        ".github/workflows/images.yml",
        "image",
        "builds, tests and pushes the images themselves, with docker",
    ),
    (
        ".github/workflows/nebius-watch.yml",
        "watch",
        "the idle checker: scripts/nebius/watch.sh, the nebius CLI and ssh, no project code",
    ),
];

const WORKFLOWS: &str = ".github/workflows";

/// (owner, command, reason): a command that a job, listed in either list, or a local action runs
/// on the runner on purpose. The command is named from its command word on, as far as needed.
const RUNNER_SIDE: &[(&str, &str, &str)] = &[
    (
        "cost-report",
        "shellcheck",
        "a linter over the Nebius scripts, running none of the project's code; the runner has it \
         and the build image does not",
    ),
    (
        "cost-report",
        "python3 \"scripts/env/tests/$n\"",
        "the step's on_runner files need what the image cannot give the runner's uid: a locale \
         that sorts unlike C, en_US.UTF-8 (#309), or a user name, which bootstrap-vm.sh and \
         provision-vm.sh read as a VM's or laptop's own user, and real ssh needs; stdlib only, \
         so the runner's python3 runs them",
    ),
    (
        "./.github/actions/build-image",
        "scripts/env/image-ref.sh",
        "finds the build image the job's later steps run in, with the runner's docker, before \
         there is an image to run anything in",
    ),
    (
        "gpu-tests",
        "scripts/nebius/check-actor.sh",
        "the actor check, before anything reaches the VM: bash over two strings",
    ),
    (
        "gpu-tests",
        "scripts/env/image-ref.sh",
        "finds the run and build images by the tree's inputs before the VM is paid for, reading \
         labels with the runner's docker",
    ),
    (
        "gpu-tests",
        "scripts/nebius.sh",
        "the VM's door: the nebius CLI and ssh, starting, reaching and stopping it",
    ),
    (
        "gpu-tests",
        "scripts/env/run-gpu.sh",
        "drives the VM through nebius.sh; the tests run there, in the run image",
    ),
    (
        "image",
        "scripts/env/image.sh",
        "builds and pushes the image with the runner's docker: there is no image to run it in",
    ),
    (
        "image",
        "scripts/env/image-ref.sh",
        "finds the run image the job just built, by the tree's inputs, with the runner's docker",
    ),
    (
        "image",
        "python3 scripts/env/tests/test_run_tests.py",
        "runs the run image the job just built with the runner's docker, over fake binaries; \
         stdlib only",
    ),
    (
        "watch",
        "scripts/nebius/watch.sh",
        "the idle checker: the nebius CLI and ssh, no build or test of project code",
    ),
];

const BUILD_IMAGE_ACTION: &str = "./.github/actions/build-image";

const WRAPPER: &str = "scripts/build-test.sh";

/// What only a job outside the image needs: the image holds the toolchains and the venv.
const OFF_IMAGE: &[&str] = &[
    "conda install",
    "pip install",
    "rapidsai/",
    "dtolnay/rust-toolchain",
];

/// The commands that build or run project code, besides python and the tree's own files.
const PROJECT_COMMANDS: &[&str] = &["cargo", "ctest", "cmake"];

/// The tree's directories of code: a file in one, run by its path, is project code.
const PROJECT_DIRS: &[&str] = &["scripts/", "benchmarks/", "testdata/", "docker/"];

/// Where a job's own composite actions live; a listed job's are held to the job's rules.
const LOCAL_ACTIONS: &str = "./.github/actions/";

/// The runner's own paths, which `build-test.sh` does not mount: a wrapped command writing one
/// writes into the container, and the runner never sees it. RUNNER_TEMP and the checkout are
/// mounted at their own paths.
fn is_unmounted(name: &str) -> bool {
    matches!(
        name,
        "HOME"
            | "GITHUB_OUTPUT"
            | "GITHUB_ENV"
            | "GITHUB_STEP_SUMMARY"
            | "GITHUB_PATH"
            | "GITHUB_STATE"
            | "GITHUB_EVENT_PATH"
    ) || (name.starts_with("RUNNER_") && name != "RUNNER_TEMP")
}

/// Every problem with one job of a workflow's text; empty when it runs in the image.
fn image_problems(workflow: &str, job: &str, runner_side: &[(&str, &str, &str)]) -> Vec<String> {
    let Some(lines) = job_lines(workflow, job) else {
        return vec![format!("{job}: no such job")];
    };
    let mut problems = Vec::new();
    let code: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let action = format!("uses: {BUILD_IMAGE_ACTION}");
    if !code
        .iter()
        .any(|l| l.trim_start().trim_start_matches("- ").trim_end() == action)
    {
        problems.push(format!(
            "{job}: no `{action}` step, which frees the disk and pulls the image"
        ));
    }
    for line in &code {
        if line.trim_start().starts_with("container:") {
            problems.push(format!("{job}: `{}`: the job runs on the runner, and its commands in the image (container: would pull 10 GB onto an unfreed disk)", line.trim()));
        }
    }
    problems.extend(step_problems(job, &lines, &exempt_for(job, runner_side)));
    problems
}

/// The local composite actions a job's steps use, by their `uses:` path.
fn local_actions(workflow: &str, job: &str) -> Vec<String> {
    job_lines(workflow, job)
        .unwrap_or_default()
        .iter()
        .filter_map(|l| {
            l.trim_start()
                .trim_start_matches("- ")
                .strip_prefix("uses: ")
        })
        .map(str::trim)
        .filter(|path| path.starts_with(LOCAL_ACTIONS))
        .map(str::to_string)
        .collect()
}

/// Every problem with a composite action's text: its steps run in a listed job.
fn action_problems(path: &str, text: &str, runner_side: &[(&str, &str, &str)]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    step_problems(path, &lines, &exempt_for(path, runner_side))
}

/// The commands RUNNER_SIDE lets a job or a local action run on the runner.
fn exempt_for<'a>(owner: &str, runner_side: &[(&str, &'a str, &str)]) -> Vec<&'a str> {
    runner_side
        .iter()
        .filter(|(o, ..)| *o == owner)
        .map(|(_, command, _)| *command)
        .collect()
}

/// The problems of a job's or an action's lines: anything only a job outside the image needs,
/// and each `run:` line but the exempt ones.
fn step_problems(owner: &str, lines: &[&str], exempt: &[&str]) -> Vec<String> {
    let mut problems = off_image_problems(owner, lines);
    problems.extend(run_line_problems(owner, lines, exempt));
    problems
}

/// Each line, comments aside, that installs what only a job outside the image needs.
fn off_image_problems(owner: &str, lines: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    let code = lines
        .iter()
        .copied()
        .filter(|l| !l.trim_start().starts_with('#'));
    for line in code {
        for off in OFF_IMAGE.iter().filter(|off| line.contains(*off)) {
            problems.push(format!(
                "{owner}: `{}`: {off} is for a job outside the image",
                line.trim()
            ));
        }
    }
    problems
}

/// The problems of each command the `run:` lines run but the exempt ones: a project command on the
/// runner, or a wrapped one naming a runner path. An exempt entry names a command from its command
/// word on, and one that names none is a problem too.
fn run_line_problems(owner: &str, lines: &[&str], exempt: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    let scripts: Vec<String> = run_scripts(lines)
        .iter()
        .flat_map(|s| logical_lines(s))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let mut used = vec![false; exempt.len()];
    for line in &scripts {
        for words in commands(line) {
            let text = words.join(" ");
            let named = exempt
                .iter()
                .position(|e| text == *e || text.starts_with(&format!("{e} ")));
            if let Some(at) = named {
                used[at] = true;
                continue;
            }
            problems.extend(
                command_problems(&words)
                    .into_iter()
                    .map(|p| format!("{owner}: `{line}`: {p}")),
            );
        }
    }
    for (entry, _) in exempt.iter().zip(used).filter(|(_, used)| !used) {
        problems.push(format!(
            "{owner}: RUNNER_SIDE's `{entry}` matches no command: remove it"
        ));
    }
    problems
}

/// One command's problems: a project command on the runner, or a wrapped one naming a runner
/// path.
fn command_problems(words: &[String]) -> Vec<String> {
    let command = unquoted(&words[0]);
    let path = command.trim_start_matches("./");
    if path != WRAPPER {
        if !is_project_command(path) {
            return Vec::new();
        }
        return vec![format!(
            "{command} runs on the runner: wrap it as `{WRAPPER} -- {command} …`, or list the \
             command in RUNNER_SIDE with its reason"
        )];
    }
    let mut problems = Vec::new();
    for word in &words[1..] {
        problems.extend(unmounted_names(word).into_iter().map(|name| {
            format!("the wrapped command names ${name}, which the image does not mount: redirect the command's output to it on the runner instead")
        }));
        if word.starts_with('~') {
            problems.push("the wrapped command names ~, the runner's home, which the image does not mount".to_string());
        }
    }
    problems
}

/// Whether a command word builds or runs project code: a toolchain the image holds, any python, or
/// a file of the tree's own code, run by its path.
fn is_project_command(path: &str) -> bool {
    let python = path
        .strip_prefix("python")
        .is_some_and(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'));
    python
        || PROJECT_COMMANDS.contains(&path)
        || PROJECT_DIRS.iter().any(|dir| path.starts_with(dir))
}

/// The `NAME`s of `$NAME` and `${NAME…}` in a word that are runner paths.
fn unmounted_names(word: &str) -> Vec<String> {
    let name_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    word.match_indices('$')
        .map(|(at, _)| word[at + 1..].trim_start_matches('{'))
        .map(|rest| rest[..rest.find(|c| !name_char(c)).unwrap_or(rest.len())].to_string())
        .filter(|name| is_unmounted(name))
        .collect()
}

#[test]
fn every_listed_job_runs_in_the_build_image() {
    let action = repo_root().join(BUILD_IMAGE_ACTION).join("action.yml");
    let mut problems = Vec::new();
    if !action.is_file() {
        problems.push(format!("{BUILD_IMAGE_ACTION}/action.yml is missing"));
    }
    for (workflow, job) in IN_IMAGE_JOBS {
        let text = std::fs::read_to_string(repo_root().join(workflow))
            .unwrap_or_else(|e| panic!("read {workflow}: {e}"));
        problems.extend(image_problems(&text, job, RUNNER_SIDE));
        for action in local_actions(&text, job) {
            let file = repo_root().join(&action).join("action.yml");
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
            problems.extend(action_problems(&action, &text, RUNNER_SIDE));
        }
    }
    for (owner, command, reason) in RUNNER_SIDE {
        assert!(
            !reason.is_empty(),
            "RUNNER_SIDE's `{command}` in {owner} has no reason"
        );
        let known = IN_IMAGE_JOBS.iter().any(|(_, j)| j == owner)
            || HOST_JOBS.iter().any(|(_, j, _)| j == owner)
            || owner.starts_with(LOCAL_ACTIONS);
        assert!(
            known,
            "RUNNER_SIDE names {owner}, which is no listed job and no local action"
        );
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A `cost-report`-shaped job, its steps the given `run:` lines.
fn job_with(run_lines: &[&str]) -> String {
    let mut text = String::from(
        "jobs:\n  cost-report:\n    runs-on: ubuntu-latest\n    steps:\n      \
         - uses: actions/checkout@v4\n      - uses: ./.github/actions/build-image\n        \
         with:\n          cudf: \"25.02\"\n",
    );
    for line in run_lines {
        text.push_str("      - run: |\n");
        for l in line.lines() {
            text.push_str(&format!("          {l}\n"));
        }
    }
    text.push_str(
        "  next-job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo build\n",
    );
    text
}

fn refused(text: &str, runner_side: &[(&str, &str, &str)], naming: &[&str]) {
    let problems = image_problems(text, "cost-report", runner_side);
    assert!(!problems.is_empty(), "accepted:\n{text}");
    let all = problems.join("\n");
    for name in naming {
        assert!(
            all.contains(name),
            "the refusal does not name `{name}`:\n{all}"
        );
    }
}

fn accepted(text: &str, runner_side: &[(&str, &str, &str)]) {
    let problems = image_problems(text, "cost-report", runner_side);
    assert!(
        problems.is_empty(),
        "refused:\n{}\n\n{text}",
        problems.join("\n")
    );
}

#[test]
fn a_bare_cargo_line_in_a_listed_job_is_refused() {
    let bare = job_with(&["cargo test -p cost-report"]);
    refused(&bare, &[], &["cost-report", "cargo test -p cost-report"]);
    accepted(
        &bare,
        &[("cost-report", "cargo test -p cost-report", "a reason")],
    );
    accepted(
        &job_with(&["scripts/build-test.sh -- cargo test -p cost-report"]),
        &[],
    );
    // Another job's bare line is not this job's.
    accepted(&job_with(&[]), &[]);
}

#[test]
fn a_project_command_anywhere_in_a_line_is_found() {
    for line in [
        "for f in a b; do python3 \"$f\" || status=1; done",
        "python3 -c \"import pandas\" 2>/dev/null || true",
        "if ! ctest --test-dir cpp/build; then exit 1; fi",
        "X=1 cargo build \\\n  --release",
        "v=\"$(python3 -c 'print(1)')\"",
        "testdata/generate_testdata.sh --sf 1",
        "scripts/build-test.sh -- true && cargo test",
    ] {
        refused(&job_with(&[line]), &[], &["cost-report"]);
    }
    for line in [
        "# python3 runs in the image",
        "echo \"== python3 $f\"",
        "files=(testdata/test_*.py)",
        "elsewhere=(scripts/a.py\n           scripts/b.py\n           scripts/c.py)",
        "scripts/build-test.sh --pass-env A -- python3 \"$f\" || status=1",
        "for f in a b; do scripts/build-test.sh -- python3 \"$f\"; done",
        "scripts/build-test.sh -- cargo run -p cost-report -- \\\n  --sha \"$REPORT_SHA\"",
    ] {
        accepted(&job_with(&[line]), &[]);
    }
}

/// A project command hides behind a word that runs the word after it, inside a shell's `-c` text
/// or as a shell's script, or as a script of the tree run by its path: each is found.
#[test]
fn a_project_command_behind_a_prefix_word_or_a_shell_is_found() {
    for (line, naming) in [
        ("timeout 600 cargo test", "cargo"),
        ("timeout -k 10 --signal=INT 600 ctest --test-dir cpp/build", "ctest"),
        ("env RUST_LOG=debug cargo test", "cargo"),
        ("env -u HOME -- cargo test", "cargo"),
        ("exec cargo test", "cargo"),
        ("nice -n 5 cargo build", "cargo"),
        ("command cargo test", "cargo"),
        ("sudo -E -u runner cargo test", "cargo"),
        ("ls | xargs -n 1 -I {} cargo test {}", "cargo"),
        ("timeout 60 env A=1 nice cargo test", "cargo"),
        ("bash -c 'cargo test'", "cargo"),
        ("sh -ec \"ctest --test-dir cpp/build\"", "ctest"),
        ("bash -euo pipefail -c 'x=1; cmake -B b'", "cmake"),
        ("bash scripts/env/stage-install.sh", "scripts/env/stage-install.sh"),
        ("sh -e testdata/generate_testdata.sh --sf 1", "testdata/generate_testdata.sh"),
        ("scripts/env/stage-install.sh", "scripts/env/stage-install.sh"),
        ("./scripts/case-inventory.sh", "scripts/case-inventory.sh"),
        ("benchmarks/run.sh", "benchmarks/run.sh"),
        ("docker/run-tests.sh list", "docker/run-tests.sh"),
        ("python scripts/x.py", "python"),
        ("python3.12 -m pytest", "python3.12"),
        ("v=$(timeout 5 python3 -c 'print(1)')", "python3"),
    ] {
        refused(&job_with(&[line]), &[], &[&format!("{naming} runs on the runner")]);
    }
    for line in [
        "timeout 600 scripts/build-test.sh -- cargo test",
        "scripts/build-test.sh -- bash -c 'cargo test && ctest'",
        "command -v cargo",
        "sudo rm -rf /usr/local/lib/android",
        "bash -c 'echo ready'",
        "env | sort",
        "echo scripts/env/stage-install.sh",
        "docker image prune --all --force",
    ] {
        accepted(&job_with(&[line]), &[]);
    }
}

/// A RUNNER_SIDE entry names a command, from its command word, wherever the line runs it: inside
/// `$(…)` or behind a prefix word too. A word that only starts with it is another command.
#[test]
fn a_runner_side_entry_names_a_command() {
    let side: &[(&str, &str, &str)] = &[("cost-report", "scripts/env/image-ref.sh", "a reason")];
    for line in [
        "ref=$(scripts/env/image-ref.sh build \"$CUDF\")",
        "timeout 60 scripts/env/image-ref.sh run 25.02",
    ] {
        refused(&job_with(&[line]), &[], &["scripts/env/image-ref.sh runs on the runner"]);
        accepted(&job_with(&[line]), side);
    }
    refused(&job_with(&["scripts/env/image-ref.shx run 25.02"]), side, &["image-ref.shx"]);
    refused(&job_with(&["true"]), side, &["matches no command"]);
}

#[test]
fn a_wrapped_command_writing_an_unmounted_path_is_refused() {
    let wrapped = |args: &str| {
        job_with(&[&format!(
            "scripts/build-test.sh -- cargo run -p cost-report -- {args}"
        )])
    };
    refused(&wrapped("--md \"$GITHUB_OUTPUT\""), &[], &["GITHUB_OUTPUT"]);
    refused(
        &wrapped("--md \"${GITHUB_STEP_SUMMARY}\""),
        &[],
        &["GITHUB_STEP_SUMMARY"],
    );
    refused(&wrapped("--md \"$HOME/r.md\""), &[], &["HOME"]);
    refused(&wrapped("--md ~/r.md"), &[], &["~"]);
    accepted(&wrapped("--md \"$RUNNER_TEMP/r.md\""), &[]);
    accepted(&wrapped("--md report/r.md >> \"$GITHUB_OUTPUT\""), &[]);
    accepted(
        &wrapped("--md \"$RUNNER_TEMP/r.md\" 2>&1 | tee -a \"$GITHUB_STEP_SUMMARY\""),
        &[],
    );
}

#[test]
fn a_listed_job_off_the_image_is_refused() {
    let without_action = job_with(&["scripts/build-test.sh -- true"]).replace(
        "      - uses: ./.github/actions/build-image\n",
        "      - uses: ./other\n",
    );
    refused(&without_action, &[], &["cost-report", BUILD_IMAGE_ACTION]);
    let in_container = job_with(&[]).replace(
        "    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout",
        "    runs-on: ubuntu-latest\n    container: rapidsai/base:25.02\n    steps:\n      - uses: actions/checkout",
    );
    refused(&in_container, &[], &["container:"]);
    for line in [
        "pip install --quiet pandas",
        "conda install -y gcc",
        "echo rapidsai/base",
    ] {
        refused(&job_with(&[line]), &[], &[line.split(' ').next().unwrap()]);
    }
    let rust = job_with(&[]).replace(
        "      - uses: actions/checkout@v4\n",
        "      - uses: actions/checkout@v4\n      - uses: dtolnay/rust-toolchain@stable\n",
    );
    refused(&rust, &[], &["dtolnay/rust-toolchain"]);
}

#[test]
fn a_local_action_a_listed_job_uses_is_held_to_the_job_rules() {
    let job = job_with(&[]).replace(
        "  next-job:\n",
        "      - uses: ./.github/actions/cpp-build\n        with:\n          cudf: \"25.02\"\n  next-job:\n",
    );
    assert_eq!(
        local_actions(&job, "cost-report"),
        [BUILD_IMAGE_ACTION, "./.github/actions/cpp-build"]
    );
    let action = |run: &str| {
        format!(
            "name: x\nruns:\n  using: composite\n  steps:\n    - name: Build\n      shell: bash\n      \
             run: |\n        {run}\n"
        )
    };
    for (run, naming) in [
        ("./scripts/build.sh --configure --build", "scripts/build.sh"),
        ("conda install -y gcc", "conda install"),
        (
            "python3 -c \"import zipfile\"",
            "python3 runs on the runner",
        ),
    ] {
        let problems = action_problems("./.github/actions/cpp-build", &action(run), &[]).join("\n");
        assert!(
            problems.contains("./.github/actions/cpp-build") && problems.contains(naming),
            "`{run}`: {problems}"
        );
    }
    for run in [
        "scripts/build-test.sh --pass-env LDFLAGS -- scripts/build.sh --configure --build",
        "gcc=$(sed -n 's/^GCC_25_02=//p' docker/pins.env)",
        "scripts/build-test.sh -- ccache -s",
    ] {
        let problems = action_problems("./.github/actions/cpp-build", &action(run), &[]);
        assert!(problems.is_empty(), "`{run}`: {}", problems.join("\n"));
    }
}

/// The job whose legs are the cuDF matrix.
const MATRIX_JOB: &str = "dataset-matrix";

/// Every problem with the matrix's legs: each must have a build image, which a leg names by its
/// `cudf:` alone, so the label is the image (#129 was a label ahead of its image).
fn leg_problems(workflow: &str, has_spec: impl Fn(&str) -> bool) -> Vec<String> {
    let Some(lines) = job_lines(workflow, MATRIX_JOB) else {
        return vec![format!("{MATRIX_JOB}: no such job")];
    };
    let mut problems = Vec::new();
    let legs: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.trim().strip_prefix("- cudf: "))
        .map(|v| v.trim().trim_matches('"'))
        .collect();
    if legs.is_empty() {
        problems.push(format!("{MATRIX_JOB}: no `- cudf:` legs"));
    }
    for leg in &legs {
        if !has_spec(leg) {
            problems.push(format!(
                "{MATRIX_JOB}: the leg cudf {leg} has no build image: docker/conda/cudf-{leg}.spec is missing"
            ));
        }
    }
    for line in lines.iter().filter(|l| l.trim().starts_with("image:")) {
        problems.push(format!(
            "{MATRIX_JOB}: `{}`: a leg's image is the build image of its cudf, not a key of its own",
            line.trim()
        ));
    }
    let action = format!("uses: {BUILD_IMAGE_ACTION}");
    let with_leg = lines
        .iter()
        .skip_while(|l| l.trim().trim_start_matches("- ") != action)
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("- "))
        .any(|l| l.trim() == "cudf: ${{ matrix.cudf }}");
    if !with_leg {
        problems.push(format!(
            "{MATRIX_JOB}: its `{action}` step does not take `cudf: ${{{{ matrix.cudf }}}}`, so a leg \
             would not run in its own image"
        ));
    }
    problems
}

#[test]
fn every_cudf_leg_has_a_build_image() {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let problems = leg_problems(&text, |leg| {
        repo_root()
            .join(format!("docker/conda/cudf-{leg}.spec"))
            .is_file()
    });
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_leg_off_its_build_image_is_refused() {
    let matrix = |legs: &str, cudf: &str| {
        format!(
            "jobs:\n  dataset-matrix:\n    strategy:\n      matrix:\n        include:\n{legs}    \
             runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      \
             - uses: ./.github/actions/build-image\n        with:\n          cudf: {cudf}\n      \
             - run: scripts/build-test.sh -- true\n  next-job:\n    runs-on: ubuntu-latest\n"
        )
    };
    let honest = "          - cudf: \"25.10\"\n          - cudf: \"25.02\"\n";
    let has_spec = |leg: &str| ["25.02", "25.10", "26.02"].contains(&leg);
    let problems = |text: &str| leg_problems(text, has_spec).join("\n");
    assert_eq!(problems(&matrix(honest, "${{ matrix.cudf }}")), "");
    let labelled =
        "          - cudf: \"26.02\"\n            image: rapidsai/base:25.10a-cuda12-py3.12\n";
    assert!(problems(&matrix(labelled, "${{ matrix.cudf }}")).contains("image: rapidsai/base"));
    let unknown = "          - cudf: \"9.99\"\n";
    assert!(
        problems(&matrix(unknown, "${{ matrix.cudf }}")).contains("docker/conda/cudf-9.99.spec")
    );
    assert!(problems(&matrix(honest, "\"25.02\"")).contains("matrix.cudf"));
    assert!(problems(&matrix("", "${{ matrix.cudf }}")).contains("no `- cudf:` legs"));
}

/// The key a job's Rust cache must carry: the target dir holds what the image built, against its
/// gcc and cuDF, which Swatinem's own key (rustc's version, the lockfiles) does not see.
const RUST_CACHE_KEY: &str = "key: ${{ env.PEACOCK_BUILD_IMAGE }}";

/// Each `Swatinem/rust-cache` step of a job without RUST_CACHE_KEY.
fn rust_cache_problems(workflow: &str, job: &str) -> Vec<String> {
    let lines = job_lines(workflow, job).unwrap_or_default();
    job_steps(&lines)
        .iter()
        .filter(|step| {
            step.iter().any(|l| {
                l.trim_start()
                    .trim_start_matches("- ")
                    .starts_with("uses: Swatinem/rust-cache@")
            })
        })
        .filter(|step| !step.iter().any(|l| l.trim() == RUST_CACHE_KEY))
        .map(|_| {
            format!(
                "{job}: a Swatinem/rust-cache step without `{RUST_CACHE_KEY}`: a new image would \
                 restore a target dir another image built"
            )
        })
        .collect()
}

#[test]
fn a_rust_cache_is_keyed_by_the_build_image() {
    let mut problems = Vec::new();
    for (workflow, job) in IN_IMAGE_JOBS {
        let text = std::fs::read_to_string(repo_root().join(workflow))
            .unwrap_or_else(|e| panic!("read {workflow}: {e}"));
        problems.extend(rust_cache_problems(&text, job));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    let cache = |key: &str| {
        job_with(&[]).replace(
            "  next-job:\n",
            &format!(
                "      - name: Cache Rust build\n        uses: Swatinem/rust-cache@v2\n        \
                 with:\n          shared-key: rust-x\n{key}  next-job:\n"
            ),
        )
    };
    let keyed = format!("          {RUST_CACHE_KEY}\n");
    assert_eq!(rust_cache_problems(&cache(&keyed), "cost-report"), Vec::<String>::new());
    for (what, key) in [
        ("no key", String::new()),
        ("commented out", format!("          # {RUST_CACHE_KEY}\n")),
        ("another key", "          key: ${{ github.sha }}\n".to_string()),
    ] {
        assert!(
            !rust_cache_problems(&cache(&key), "cost-report").is_empty(),
            "{what}: accepted"
        );
    }
}

/// Every problem with one workflow's jobs: a job in neither list or in both, a listed job it
/// does not have, a `container:` key or an off-image install anywhere, and a host job running a
/// project command on the runner.
fn accounting_problems(
    path: &str,
    workflow: &str,
    in_image: &[(&str, &str)],
    host: &[(&str, &str, &str)],
    runner_side: &[(&str, &str, &str)],
) -> Vec<String> {
    let jobs = workflow_jobs(workflow);
    let in_image: Vec<&str> = in_image
        .iter()
        .filter(|(w, _)| *w == path)
        .map(|(_, j)| *j)
        .collect();
    let host: Vec<&str> = host
        .iter()
        .filter(|(w, ..)| *w == path)
        .map(|(_, j, _)| *j)
        .collect();
    let mut problems = Vec::new();
    for job in &jobs {
        match (in_image.contains(job), host.contains(job)) {
            (false, false) => problems.push(format!(
                "{path}: job {job} is in neither IN_IMAGE_JOBS nor HOST_JOBS: run it in the build \
                 image, or list it in HOST_JOBS with why it needs none"
            )),
            (true, true) => problems.push(format!(
                "{path}: job {job} is in both IN_IMAGE_JOBS and HOST_JOBS"
            )),
            _ => {}
        }
    }
    for job in in_image.iter().chain(&host).filter(|j| !jobs.contains(j)) {
        problems.push(format!(
            "{path}: listed job {job} does not exist: remove it"
        ));
    }
    let lines: Vec<&str> = workflow.lines().collect();
    for line in lines
        .iter()
        .filter(|l| l.trim_start().starts_with("container:"))
    {
        problems.push(format!(
            "{path}: `{}`: a job runs on the runner, and its project commands in the image",
            line.trim()
        ));
    }
    problems.extend(off_image_problems(path, &lines));
    for job in host.iter().filter(|j| jobs.contains(j)) {
        let lines = job_lines(workflow, job).unwrap_or_default();
        problems.extend(run_line_problems(job, &lines, &exempt_for(job, runner_side)));
    }
    problems
}

#[test]
fn every_job_runs_in_the_image_or_says_why() {
    let mut problems = Vec::new();
    let dir = repo_root().join(WORKFLOWS);
    let mut paths: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {WORKFLOWS}: {e}"))
        .map(|e| {
            e.expect("a workflow entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".yml"))
        .map(|name| format!("{WORKFLOWS}/{name}"))
        .collect();
    paths.sort();
    for path in &paths {
        let text = std::fs::read_to_string(repo_root().join(path))
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        problems.extend(accounting_problems(path, &text, IN_IMAGE_JOBS, HOST_JOBS, RUNNER_SIDE));
    }
    for (workflow, job, reason) in HOST_JOBS {
        assert!(!reason.is_empty(), "HOST_JOBS' {job} has no reason");
        if !paths.iter().any(|p| p == workflow) {
            problems.push(format!("HOST_JOBS names {workflow}, which does not exist"));
        }
    }
    for entry in std::fs::read_dir(repo_root().join(LOCAL_ACTIONS)).expect("read the actions") {
        let file = entry.expect("an action entry").path().join("action.yml");
        let text = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let lines: Vec<&str> = text.lines().collect();
        problems.extend(off_image_problems(&file.display().to_string(), &lines));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_job_neither_in_the_image_nor_said_why_is_refused() {
    const WF: &str = "w.yml";
    let workflow = |b_steps: &str| {
        format!(
            "on:\n  workflow_dispatch:\n    inputs:\n      x:\n        default: ''\n  push:\n\n\
             jobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: scripts/build-test.sh -- true\n\n  \
             # a comment between jobs\n  b:\n    runs-on: ubuntu-latest\n    steps:\n{b_steps}"
        )
    };
    let quiet = workflow("      - run: scripts/nebius.sh start\n");
    assert_eq!(workflow_jobs(&quiet), ["a", "b"]);
    let side: &[(&str, &str, &str)] = &[("b", "scripts/nebius.sh", "the VM's door, no project code")];
    let problems = |text: &str, host: &[(&str, &str, &str)]| {
        accounting_problems(WF, text, &[(WF, "a")], host, side)
    };
    assert_eq!(problems(&quiet, &[(WF, "b", "why")]), Vec::<String>::new());
    // A host job's script of the tree runs on the runner too, so it needs its entry.
    let unlisted = accounting_problems(WF, &quiet, &[(WF, "a")], &[(WF, "b", "why")], &[]).join("\n");
    assert!(unlisted.contains("scripts/nebius.sh runs on the runner"), "{unlisted}");

    let unlisted = problems(&quiet, &[]).join("\n");
    assert!(unlisted.contains("job b is in neither"), "{unlisted}");
    assert!(
        !unlisted.contains("workflow_dispatch") && !unlisted.contains("push"),
        "{unlisted}"
    );
    let both = accounting_problems(WF, &quiet, &[(WF, "a"), (WF, "b")], &[(WF, "b", "why")], side);
    assert!(both.join("\n").contains("job b is in both"), "{both:?}");
    let stale = problems(&quiet, &[(WF, "b", "why"), (WF, "gone", "why")]);
    assert!(
        stale.join("\n").contains("listed job gone does not exist"),
        "{stale:?}"
    );
    // Another workflow's entries are not this one's.
    assert_eq!(
        problems(&quiet, &[(WF, "b", "why"), ("x.yml", "c", "why")]),
        Vec::<String>::new()
    );

    for (steps, naming) in [
        (
            "    container: ubuntu:24.04\n      - run: true\n",
            "`container: ubuntu:24.04`: a job runs",
        ),
        ("      - run: pip install --quiet pandas\n", "pip install"),
        ("      - run: conda install -y gcc\n", "conda install"),
        (
            "      - run: python3 scripts/x.py\n",
            "python3 runs on the runner",
        ),
        (
            "      - run: |\n          for f in a; do cargo test; done\n",
            "cargo runs on the runner",
        ),
    ] {
        let found = problems(&workflow(steps), &[(WF, "b", "why")]).join("\n");
        assert!(
            found.contains(naming),
            "`{steps}` not refused for {naming}: {found}"
        );
    }
    let commented =
        workflow("      # the pip install went: pandas is in the image\n      - run: true\n");
    assert_eq!(
        accounting_problems(WF, &commented, &[(WF, "a")], &[(WF, "b", "why")], &[]),
        Vec::<String>::new()
    );
}
