//! The gpu-tests job's place on nebius-gpu. The VM is shared: a developer's clone of the
//! repository is `~/peacock/peacockdb/` there and its run the container `peacock-peacockdb`, the
//! names `run-gpu.sh` gives the CI checkout by default. So the job names a directory of its own,
//! every `run-gpu.sh` call the same, and a fixed tree id, since its checkout is new on every run;
//! and on every outcome its cleanup cancels that dir's run, before the soft stop.

use crate::ci_workflow::{commands, job_lines, job_steps, logical_lines, run_scripts, unquoted};
use crate::repo_root;
use crate::runners::GPU_RUN_STEP;

const RUN_GPU: &str = "scripts/env/run-gpu.sh";

/// The CI checkout's directory name, and so `run-gpu.sh`'s default `--dir`.
const CHECKOUT_DIR: &str = "peacockdb";

/// A step's `name:`, if it has one.
fn step_name<'a>(step: &[&'a str]) -> Option<&'a str> {
    step.iter()
        .find_map(|l| {
            l.trim_start()
                .trim_start_matches("- ")
                .strip_prefix("name:")
        })
        .map(str::trim)
}

/// The `run-gpu.sh` commands of a step's `run:` lines, each its words.
fn run_gpu_commands(step: &[&str]) -> Vec<Vec<String>> {
    run_scripts(step)
        .iter()
        .flat_map(|s| logical_lines(s))
        .flat_map(|l| commands(&l))
        .filter(|words| words.first().is_some_and(|w| w == RUN_GPU))
        .collect()
}

/// Every command of a step's `run:` lines, each its words, and after a `nebius.sh … ssh` the
/// commands its remote text runs on the VM.
fn step_commands(step: &[&str]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for words in run_scripts(step)
        .iter()
        .flat_map(|s| logical_lines(s))
        .flat_map(|l| commands(&l))
    {
        let remote = words
            .first()
            .filter(|w| w.ends_with("nebius.sh"))
            .and_then(|_| words.iter().position(|w| w == "ssh"));
        if let Some(at) = remote {
            let text: Vec<String> = words[at + 1..].iter().map(|w| unquoted(w)).collect();
            out.extend(commands(&text.join(" ")));
        }
        out.push(words);
    }
    out
}

/// A `docker rm`, `kill` or `stop` (`docker container …` too): its verb, the containers it names
/// and whether it forces. `None` for any other command.
fn container_stop(words: &[String]) -> Option<(String, Vec<String>, bool)> {
    let words: Vec<String> = words.iter().map(|w| unquoted(w)).collect();
    let mut rest = words.strip_prefix(&["docker".to_string()])?;
    if rest.first().is_some_and(|w| w == "container") {
        rest = &rest[1..];
    }
    let (verb, args) = rest.split_first()?;
    if !["rm", "kill", "stop"].contains(&verb.as_str()) {
        return None;
    }
    let (mut names, mut force) = (Vec::new(), false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" | "--force" => force = true,
            "-s" | "--signal" | "-t" | "--time" => {
                args.next();
            }
            option if option.starts_with('-') => {}
            name => names.push(name.to_string()),
        }
    }
    Some((verb.clone(), names, force))
}

fn dir_of(words: &[String]) -> Option<&str> {
    words
        .windows(2)
        .find(|pair| pair[0] == "--dir")
        .map(|pair| pair[1].as_str())
}

/// A script's verb: its first word past the options, each of which takes a value, as both
/// `run-gpu.sh`'s and `nebius.sh`'s do.
fn verb_of(words: &[String]) -> Option<String> {
    let mut rest = words.get(1..)?;
    while rest.first().is_some_and(|w| w.starts_with("--")) {
        rest = rest.get(2..)?;
    }
    rest.first().map(|w| unquoted(w))
}

/// A step's `if:`, if it has one.
fn step_condition<'a>(step: &[&'a str]) -> Option<&'a str> {
    step.iter()
        .find_map(|l| l.trim_start().trim_start_matches("- ").strip_prefix("if:"))
        .map(str::trim)
}

/// The line of the job's own `env:` that sets `name`, and its value as written.
fn job_env<'a>(lines: &[&'a str], name: &str) -> Option<(usize, &'a str)> {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let at = lines
        .iter()
        .position(|l| indent(l) == 4 && l.trim() == "env:")?;
    lines[at + 1..]
        .iter()
        .enumerate()
        .take_while(|(_, l)| {
            let body = l.trim_start();
            body.is_empty() || body.starts_with('#') || indent(l) > 4
        })
        .find_map(|(i, l)| Some((at + 1 + i, l.trim().strip_prefix(name)?.strip_prefix(':')?)))
        .map(|(i, value)| (i, value.trim()))
}

/// What is wrong with the job's tree id: none in the job's `env:`, not a literal id, which an
/// expression is not, or another line of the job naming it: a step's `env:`, an assignment, an
/// `unset` or a `$GITHUB_ENV` line would give some calls another id.
fn tree_id_problems(lines: &[&str]) -> Vec<String> {
    let Some((set, value)) = job_env(lines, "PEACOCK_TREE_ID") else {
        return vec![
            "gpu-tests: the job's env: sets no PEACOCK_TREE_ID. The checkout is new on every run, \
             so each would make a random id, and the next run would be refused its dir"
                .to_string(),
        ];
    };
    let mut problems: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|&(i, l)| {
            i != set && !l.trim_start().starts_with('#') && l.contains("PEACOCK_TREE_ID")
        })
        .map(|(_, l)| {
            format!(
                "gpu-tests: `{}` names PEACOCK_TREE_ID beside the job's env:, which alone sets \
                 it: a call with another id is refused ~/peacock/<dir>/",
                l.trim()
            )
        })
        .collect();
    let id = unquoted(value);
    let literal = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !literal {
        problems.push(format!(
            "gpu-tests: PEACOCK_TREE_ID is `{value}`, not a literal id: an expression that changes \
             between runs is refused the dir the last run claimed, and run-gpu.sh takes letters, \
             digits, '.', '_' and '-' alone"
        ));
    }
    problems
}

/// What is wrong with the cleanup: no `run-gpu.sh … cancel`, or one whose step's `if:` does not
/// start `always()`, whose failure ends the step (no `||` after it on its line, or one followed by
/// anything but `echo`, `true` or `:`), or that comes after a `nebius.sh stop`. A cancel of
/// another dir than `<dir>` is the caller's to refuse, as any call is.
fn cancel_problems(steps: &[Vec<&str>], dir: &str) -> Vec<String> {
    let is_cancel = |words: &Vec<String>| {
        words.first().is_some_and(|w| w == RUN_GPU) && verb_of(words).as_deref() == Some("cancel")
    };
    let is_stop = |words: &Vec<String>| {
        words.first().is_some_and(|w| w.ends_with("nebius.sh"))
            && verb_of(words).as_deref() == Some("stop")
    };
    // Each logical line of every `run:`, in the job's order, with its step.
    let lines: Vec<(&Vec<&str>, String)> = steps
        .iter()
        .flat_map(|step| {
            run_scripts(step)
                .iter()
                .flat_map(|s| logical_lines(s))
                .map(move |l| (step, l))
                .collect::<Vec<_>>()
        })
        .collect();
    let cancels: Vec<usize> = (0..lines.len())
        .filter(|&at| commands(&lines[at].1).iter().any(is_cancel))
        .collect();
    if cancels.is_empty() {
        return vec![format!(
            "gpu-tests: no step runs `{RUN_GPU} … --dir {dir} cancel`, so the run of a cancelled \
             job, or one whose ssh dropped, keeps the card, the idle watcher stays off, and the \
             next run's `run` is refused"
        )];
    }
    let mut problems = Vec::new();
    for at in cancels {
        let (step, line) = &lines[at];
        let condition = step_condition(step);
        if !condition.is_some_and(|c| c.starts_with("always()")) {
            problems.push(format!(
                "gpu-tests: the step running `{line}` runs on {condition:?}, not on \
                 `always() && …`: a cancelled job, or one whose ssh dropped, would leave its run \
                 on the VM"
            ));
        }
        // The first `||` of the line: what comes before it is the list a failure falls through,
        // and what follows must not fail the step itself, as `exit 1` or `false` would.
        let forgiven = line.split_once("||").is_some_and(|(head, tail)| {
            commands(head).last().is_some_and(is_cancel)
                && commands(tail)
                    .iter()
                    .all(|words| ["echo", "true", ":"].contains(&words[0].as_str()))
        });
        if !forgiven {
            problems.push(format!(
                "gpu-tests: `{line}` fails the step when the cancel fails, a VM stopped or out of \
                 reach, and the soft stop after it never runs: end it `|| echo …` alone"
            ));
        }
        if let Some((_, stop)) = lines[..at]
            .iter()
            .find(|(_, l)| commands(l).iter().any(is_stop))
        {
            problems.push(format!(
                "gpu-tests: `{stop}` comes before `{line}`: the run holds the card's lock, so \
                 the soft stop is refused and the VM stays up"
            ));
        }
    }
    problems
}

/// What is wrong with gpu-tests' directory, tree and run on the VM: the run step's `--dir`
/// missing or the checkout's, another `run-gpu.sh` call naming another, a `docker rm`, `kill` or
/// `stop` naming anything but `peacock-<dir>` alone, the cleanup's cancel (`cancel_problems`), or
/// the job's tree id (`tree_id_problems`).
fn gpu_job_problems(workflow: &str) -> Vec<String> {
    let Some(lines) = job_lines(workflow, "gpu-tests") else {
        return vec!["gpu-tests: no such job".to_string()];
    };
    let steps = job_steps(&lines);
    let Some(run_step) = steps.iter().find(|s| step_name(s) == Some(GPU_RUN_STEP)) else {
        return vec![format!("gpu-tests: no `{GPU_RUN_STEP}` step")];
    };
    let dir = run_gpu_commands(run_step)
        .iter()
        .find_map(|words| dir_of(words).map(str::to_string));
    let Some(dir) = dir.filter(|d| d != CHECKOUT_DIR) else {
        return vec![format!(
            "gpu-tests: the `{GPU_RUN_STEP}` step's {RUN_GPU} names no --dir of its own: the \
             default, {CHECKOUT_DIR}, is a developer's clone's on the VM too, and the cleanup \
             would cancel that clone's run"
        )];
    };
    let mut problems = Vec::new();
    for words in steps.iter().flat_map(|s| run_gpu_commands(s)) {
        if dir_of(&words) != Some(dir.as_str()) {
            problems.push(format!(
                "gpu-tests: `{}` does not name --dir {dir}, as `{GPU_RUN_STEP}` does",
                words.join(" ")
            ));
        }
    }
    let container = format!("peacock-{dir}");
    for words in steps.iter().flat_map(|s| step_commands(s)) {
        if let Some((_, names, _)) = container_stop(&words) {
            if names != [container.as_str()] {
                problems.push(format!(
                    "gpu-tests: `{}` names {names:?}, not {container} alone: the VM is \
                     shared, and any other container there is a developer's run",
                    words.join(" ")
                ));
            }
        }
    }
    problems.extend(cancel_problems(&steps, &dir));
    problems.extend(tree_id_problems(&lines));
    problems
}

const ALWAYS: &str = "        if: always() && steps.start.outcome == 'success'\n";
const STOP: &str = "scripts/nebius.sh stop || echo \"soft stop declined (rc=$?)\"";
const SHIP: &str = "--dir ci ship";
const RUN: &str = "--dir ci run list";

/// A job's `env:` setting `PEACOCK_TREE_ID` to `value`.
fn tree_id(value: &str) -> String {
    format!("    env:\n      PEACOCK_TREE_ID: {value}\n")
}

/// A gpu-tests job: its `env:` lines, a ship and a run step given their verbs, and a cleanup.
fn doctored_job(env: &str, ship: &str, run: &str, cleanup: &str) -> String {
    format!(
        "jobs:\n  gpu-tests:\n    runs-on: ubuntu-latest\n{env}    steps:\n      \
         - uses: actions/checkout@v4\n      - name: Ship\n        run: {RUN_GPU} \
         --instance-id x {ship}\n      - name: {GPU_RUN_STEP}\n        run: {RUN_GPU} \
         --instance-id x {run}\n{cleanup}  next-job:\n    runs-on: ubuntu-latest\n"
    )
}

/// The same with the `ci` tree id.
fn ci_job(ship: &str, run: &str, cleanup: &str) -> String {
    doctored_job(&tree_id("ci"), ship, run, cleanup)
}

/// A cleanup step under `condition`, its `run:` the lines.
fn cleanup_step(condition: &str, lines: &[&str]) -> String {
    format!(
        "      - name: Clean up\n{condition}        run: |\n          {}\n",
        lines.join("\n          ")
    )
}

fn cancel_line(dir: &str) -> String {
    format!("{RUN_GPU} --instance-id x --dir {dir} cancel")
}

fn forgiven_cancel(dir: &str) -> String {
    format!("{} || echo \"cancel failed (rc=$?)\"", cancel_line(dir))
}

/// The cleanup the job has, with `more` between the cancel and the stop.
fn ci_cleanup(more: &str) -> String {
    cleanup_step(ALWAYS, &[&forgiven_cancel("ci"), more, STOP])
}

fn rm_line(name: &str) -> String {
    format!("scripts/nebius.sh ssh 'docker rm -f {name}' || true")
}

/// Jobs the guard refuses, each named by what is wrong with it.
fn refused_jobs() -> Vec<(&'static str, String)> {
    let kill_elsewhere = format!(
        "{}      - name: Stop\n        run: \
         scripts/nebius.sh ssh 'docker kill peacock-peacockdb'\n",
        ci_cleanup("")
    );
    let tree_in_a_step = ci_cleanup("").replace(
        "        run: |",
        "        env:\n          PEACOCK_TREE_ID: ci\n        run: |",
    );
    let step_over = ci_cleanup("").replace(
        "        run: |",
        "        env:\n          PEACOCK_TREE_ID: ${{ github.run_id }}\n        run: |",
    );
    vec![
        ("no --dir", ci_job("ship", "run list", &ci_cleanup(""))),
        (
            "the checkout's dir",
            ci_job(
                "--dir peacockdb ship",
                "--dir peacockdb run list",
                &cleanup_step(ALWAYS, &[&forgiven_cancel("peacockdb"), STOP]),
            ),
        ),
        (
            "another dir",
            ci_job("--dir other ship", RUN, &ci_cleanup("")),
        ),
        (
            "another container",
            ci_job(SHIP, RUN, &ci_cleanup(&rm_line("peacock-peacockdb"))),
        ),
        (
            "a longer name",
            ci_job(SHIP, RUN, &ci_cleanup(&rm_line("peacock-ci-old"))),
        ),
        (
            "a name it starts",
            ci_job(SHIP, RUN, &ci_cleanup(&rm_line("peacock-cix"))),
        ),
        (
            "a second container",
            ci_job(
                SHIP,
                RUN,
                &ci_cleanup(&rm_line("peacock-ci peacock-peacockdb")),
            ),
        ),
        ("a kill elsewhere", ci_job(SHIP, RUN, &kill_elsewhere)),
        (
            "a stop",
            ci_job(
                SHIP,
                RUN,
                &ci_cleanup("scripts/nebius.sh ssh 'docker stop -t 5 peacock-x'"),
            ),
        ),
        (
            "by substitution",
            ci_job(
                SHIP,
                RUN,
                &ci_cleanup(&rm_line("peacock-ci $(docker ps -q)")),
            ),
        ),
        ("no cleanup", ci_job(SHIP, RUN, "")),
        (
            "no PEACOCK_TREE_ID",
            doctored_job("", SHIP, RUN, &ci_cleanup("")),
        ),
        (
            "PEACOCK_TREE_ID in a step alone",
            doctored_job("", SHIP, RUN, &tree_in_a_step),
        ),
        (
            "an id per run",
            doctored_job(&tree_id("${{ github.run_id }}"), SHIP, RUN, &ci_cleanup("")),
        ),
        (
            "an empty id",
            doctored_job(&tree_id("\"\""), SHIP, RUN, &ci_cleanup("")),
        ),
        (
            "an id run-gpu.sh refuses",
            doctored_job(&tree_id("ci/x"), SHIP, RUN, &ci_cleanup("")),
        ),
        ("a step's id over the job's", ci_job(SHIP, RUN, &step_over)),
        (
            "an id inline",
            ci_job(
                SHIP,
                RUN,
                &ci_cleanup(&format!(
                    "PEACOCK_TREE_ID=x {RUN_GPU} --instance-id x --dir ci status"
                )),
            ),
        ),
        (
            "an id unset",
            ci_job(SHIP, RUN, &ci_cleanup("unset PEACOCK_TREE_ID")),
        ),
        (
            "an id for the later steps",
            ci_job(
                SHIP,
                RUN,
                &ci_cleanup("echo PEACOCK_TREE_ID=x >> \"$GITHUB_ENV\""),
            ),
        ),
    ]
}

/// Cleanups the guard refuses, each named by what is wrong with it.
fn refused_cleanups() -> Vec<(&'static str, String)> {
    let cancel_step =
        |condition: &str, lines: &[&str]| ci_job(SHIP, RUN, &cleanup_step(condition, lines));
    vec![
        ("no cancel", cancel_step(ALWAYS, &[STOP])),
        (
            "a cancel of another dir",
            cancel_step(ALWAYS, &[&forgiven_cancel("other"), STOP]),
        ),
        (
            "a cancel under success()",
            cancel_step("        if: success()\n", &[&forgiven_cancel("ci"), STOP]),
        ),
        (
            "a cancel with no if",
            cancel_step("", &[&forgiven_cancel("ci"), STOP]),
        ),
        (
            "a cancel commented out",
            cancel_step(ALWAYS, &[&format!("# {}", forgiven_cancel("ci")), STOP]),
        ),
        (
            "a cancel that fails the step",
            cancel_step(ALWAYS, &[&cancel_line("ci"), STOP]),
        ),
        (
            "a cancel after another command's ||",
            cancel_step(ALWAYS, &[&format!("true || {}", cancel_line("ci")), STOP]),
        ),
        (
            "a cancel before another command's ||",
            cancel_step(
                ALWAYS,
                &[&format!("{}; true || echo", cancel_line("ci")), STOP],
            ),
        ),
        (
            "a second cancel under success()",
            ci_job(
                SHIP,
                RUN,
                &format!(
                    "{}{}",
                    ci_cleanup(""),
                    cleanup_step("        if: success()\n", &[&forgiven_cancel("ci")])
                ),
            ),
        ),
        (
            "a cancel after the stop",
            cancel_step(ALWAYS, &[STOP, &forgiven_cancel("ci")]),
        ),
        (
            "a cancel whose || exits",
            cancel_step(ALWAYS, &[&format!("{} || exit 1", cancel_line("ci")), STOP]),
        ),
        (
            "a cancel whose || is false",
            cancel_step(ALWAYS, &[&format!("{} || false", cancel_line("ci")), STOP]),
        ),
        (
            "a cancel whose || block exits",
            cancel_step(
                ALWAYS,
                &[
                    &format!("{} || {{ echo failed; exit 1; }}", cancel_line("ci")),
                    STOP,
                ],
            ),
        ),
    ]
}

/// A cancelled `gpu-tests` job, or one whose ssh dropped, leaves its run going on nebius-gpu,
/// which keeps the card busy and the idle watcher off; a cancel of `peacockdb` would end a
/// developer's run instead; and a checkout new on every run claims `~/peacock/<dir>/` by a new
/// random id each time. The job's calls name `--dir ci`, its `env:` alone sets a literal
/// `PEACOCK_TREE_ID`, and its cleanup cancels the run step's dir under an `if:` that starts
/// `always()`, forgiven by `echo`, `true` or `:` alone, before the soft stop. Nothing in the job
/// stops another container.
#[test]
fn the_gpu_job_owns_its_dir_and_cancels_its_run() {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let problems = gpu_job_problems(&text);
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    for (what, text) in [
        ("the cleanup", ci_job(SHIP, RUN, &ci_cleanup(""))),
        (
            "its own container removed",
            ci_job(SHIP, RUN, &ci_cleanup(&rm_line("peacock-ci"))),
        ),
        (
            "a quoted id",
            doctored_job(&tree_id("\"ci\""), SHIP, RUN, &ci_cleanup("")),
        ),
        (
            "a cancel forgiven by true",
            ci_job(
                SHIP,
                RUN,
                &cleanup_step(ALWAYS, &[&format!("{} || true", cancel_line("ci")), STOP]),
            ),
        ),
    ] {
        assert_eq!(
            gpu_job_problems(&text),
            Vec::<String>::new(),
            "{what}\n{text}"
        );
    }
    for (what, text) in refused_jobs().into_iter().chain(refused_cleanups()) {
        assert!(
            !gpu_job_problems(&text).is_empty(),
            "{what}: accepted\n{text}"
        );
    }
}
