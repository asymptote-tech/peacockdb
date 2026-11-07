//! The CI jobs that build or run project code do it in the build image, through
//! `scripts/build-test.sh` (llm-wiki/build-test.md, "CI structure"). A bare `cargo` line in such
//! a job would build on the runner's own toolchain and pass there, so the image would stop being
//! what CI proves.

use crate::ci_coverage::fold_continuations;
use crate::repo_root;

/// (workflow, job): the jobs that run in the build image.
const IN_IMAGE_JOBS: &[(&str, &str)] = &[
    (".github/workflows/pipeline.yml", "cost-report"),
    (".github/workflows/pipeline.yml", "s3-datasets"),
];

/// (job, line prefix, reason): a line of a listed job that runs on the runner on purpose.
const RUNNER_SIDE: &[(&str, &str, &str)] = &[
    (
        "cost-report",
        "shellcheck ",
        "a linter over the Nebius scripts, running none of the project's code; the runner has it \
         and the build image does not",
    ),
    (
        "cost-report",
        "python3 \"scripts/env/tests/$n\"",
        "the step's on_runner files need what the image cannot give the runner's uid: the \
         en_US.UTF-8 locale (#309), or a user name, which bootstrap-vm.sh and provision-vm.sh \
         read as a VM's or laptop's own user; stdlib only, so the runner's python3 runs them",
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

/// The commands that build or run project code. A `testdata/` script is one too.
const PROJECT_COMMANDS: &[&str] = &["cargo", "ctest", "python3"];

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
        for off in OFF_IMAGE.iter().filter(|off| line.contains(*off)) {
            problems.push(format!(
                "{job}: `{}`: {off} is for a job outside the image",
                line.trim()
            ));
        }
    }
    let scripts: Vec<String> = run_scripts(&lines)
        .iter()
        .flat_map(|s| fold_continuations(s))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let exempt: Vec<&str> = runner_side
        .iter()
        .filter(|(j, ..)| *j == job)
        .map(|(_, prefix, _)| *prefix)
        .collect();
    for prefix in &exempt {
        if !scripts.iter().any(|l| l.starts_with(prefix)) {
            problems.push(format!(
                "{job}: RUNNER_SIDE's `{prefix}` matches no line: remove it"
            ));
        }
    }
    for line in scripts
        .iter()
        .filter(|l| !exempt.iter().any(|p| l.starts_with(p)))
    {
        problems.extend(
            line_problems(line)
                .into_iter()
                .map(|p| format!("{job}: `{line}`: {p}")),
        );
    }
    problems
}

/// One logical line's problems: a project command on the runner, or a wrapped one naming a
/// runner path.
fn line_problems(line: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for part in substitutions(line) {
        for words in simple_commands(&part) {
            let Some(at) = words.iter().position(|w| !is_keyword_or_assignment(w)) else {
                continue;
            };
            let command = words[at].as_str();
            if command == WRAPPER || command.strip_prefix("./") == Some(WRAPPER) {
                for word in &words[at + 1..] {
                    problems.extend(unmounted_names(word).into_iter().map(|name| {
                        format!("the wrapped command names ${name}, which the image does not mount: redirect the command's output to it on the runner instead")
                    }));
                    if word.starts_with('~') {
                        problems.push("the wrapped command names ~, the runner's home, which the image does not mount".to_string());
                    }
                }
            } else if PROJECT_COMMANDS.contains(&command)
                || command.trim_start_matches("./").starts_with("testdata/")
            {
                problems.push(format!(
                    "{command} runs on the runner: wrap it as `{WRAPPER} -- {command} …`, or list the line in RUNNER_SIDE with its reason"
                ));
            }
        }
    }
    problems
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

fn is_keyword_or_assignment(word: &str) -> bool {
    let assignment = word.split_once('=').is_some_and(|(name, _)| {
        let name = name.trim_end_matches('+');
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    assignment
        || matches!(
            word,
            "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "!" | "{" | "}" | "time"
        )
}

/// The job's lines, from its header to the next job's, comments included.
fn job_lines<'a>(workflow: &'a str, job: &str) -> Option<Vec<&'a str>> {
    let header = format!("  {job}:");
    let mut lines = workflow.lines().skip_while(|l| l.trim_end() != header);
    lines.next()?;
    Some(
        lines
            .take_while(|l| {
                let body = l.trim_start();
                body.is_empty() || body.starts_with('#') || l.len() - body.len() > 2
            })
            .collect(),
    )
}

/// The text of each `run:` of a job: an inline value, or a block scalar, dedented.
fn run_scripts(lines: &[&str]) -> Vec<String> {
    let mut scripts = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let body = line.trim_start().trim_start_matches("- ");
        let Some(value) = body.strip_prefix("run:") else {
            continue;
        };
        let value = value.trim();
        if !value.starts_with(['|', '>']) {
            scripts.push(value.to_string());
            continue;
        }
        let key_column = line.len() - body.len();
        let mut block = Vec::new();
        while let Some(l) = lines.get(i) {
            if !l.trim().is_empty() && l.len() - l.trim_start().len() <= key_column {
                break;
            }
            block.push(l.trim_start());
            i += 1;
        }
        scripts.push(block.join("\n"));
    }
    scripts
}

/// The line with each `$(…)` replaced by a word, then each substitution's own text: a command
/// inside one runs where the line runs.
fn substitutions(line: &str) -> Vec<String> {
    let mut outer = String::new();
    let mut inner = Vec::new();
    let mut chars = line.char_indices().peekable();
    let mut quoted = false;
    while let Some((at, c)) = chars.next() {
        if c == '\'' {
            quoted = !quoted;
        }
        if quoted || c != '$' || chars.peek().map(|(_, n)| *n) != Some('(') {
            outer.push(c);
            continue;
        }
        chars.next();
        let mut depth = 1;
        let start = at + 2;
        let mut end = line.len();
        for (i, n) in chars.by_ref() {
            depth += match n {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            if depth == 0 {
                end = i;
                break;
            }
        }
        outer.push('_');
        inner.extend(substitutions(&line[start..end]));
    }
    let mut all = vec![outer];
    all.extend(inner);
    all
}

/// The simple commands of a line, each its words up to its first redirection, quotes kept.
fn simple_commands(line: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    // After a redirection the words are its target and what the runner's shell does with it.
    let mut redirected = false;
    let mut chars = line.chars().peekable();
    let end_word = |word: &mut String, words: &mut Vec<String>, redirected: bool| {
        if !word.is_empty() && !redirected {
            words.push(std::mem::take(word));
        }
        word.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                word.push(c);
                for n in chars.by_ref() {
                    word.push(n);
                    if n == c {
                        break;
                    }
                }
            }
            '\\' => {
                word.push(c);
                word.extend(chars.next());
            }
            '#' if word.is_empty() => break,
            // An array assignment, `files=(a b)`, is one word: its elements run nothing.
            '(' if word.ends_with('=') => {
                word.push(c);
                for n in chars.by_ref() {
                    word.push(n);
                    if n == ')' {
                        break;
                    }
                }
            }
            ';' | '|' | '&' | '(' | ')' => {
                end_word(&mut word, &mut words, redirected);
                commands.push(std::mem::take(&mut words));
                redirected = false;
            }
            '<' | '>' => {
                if !word.chars().all(|d| d.is_ascii_digit()) {
                    end_word(&mut word, &mut words, redirected);
                }
                word.clear();
                while chars.next_if(|n| matches!(n, '>' | '&')).is_some() {}
                redirected = true;
            }
            c if c.is_whitespace() => end_word(&mut word, &mut words, redirected),
            c => word.push(c),
        }
    }
    end_word(&mut word, &mut words, redirected);
    commands.push(words);
    commands.retain(|w| !w.is_empty());
    commands
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
    }
    for (job, prefix, reason) in RUNNER_SIDE {
        assert!(
            !reason.is_empty(),
            "RUNNER_SIDE's `{prefix}` in {job} has no reason"
        );
        assert!(
            IN_IMAGE_JOBS.iter().any(|(_, j)| j == job),
            "RUNNER_SIDE names {job}, which IN_IMAGE_JOBS does not list"
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
        "scripts/build-test.sh --pass-env A -- python3 \"$f\" || status=1",
        "for f in a b; do scripts/build-test.sh -- python3 \"$f\"; done",
        "scripts/build-test.sh -- cargo run -p cost-report -- \\\n  --sha \"$REPORT_SHA\"",
    ] {
        accepted(&job_with(&[line]), &[]);
    }
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
