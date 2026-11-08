//! The gpu-tests job's place on nebius-gpu. The VM is shared: a developer's clone of the
//! repository is `~/peacock/peacockdb/` there and its run the container `peacock-peacockdb`, the
//! names `run-gpu.sh` gives the CI checkout by default. So the job names a directory of its own,
//! every `run-gpu.sh` call the same, and its cleanup removes that container alone, on every outcome.

use crate::ci_workflow::{commands, job_lines, job_steps, logical_lines, run_scripts};
use crate::repo_root;
use crate::runners::GPU_RUN_STEP;

const RUN_GPU: &str = "scripts/env/run-gpu.sh";

/// The CI checkout's directory name, and so `run-gpu.sh`'s default `--dir`.
const CHECKOUT_DIR: &str = "peacockdb";

/// A step's `name:`, if it has one.
fn step_name<'a>(step: &[&'a str]) -> Option<&'a str> {
    step.iter()
        .find_map(|l| l.trim_start().trim_start_matches("- ").strip_prefix("name:"))
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

fn dir_of(words: &[String]) -> Option<&str> {
    words
        .windows(2)
        .find(|pair| pair[0] == "--dir")
        .map(|pair| pair[1].as_str())
}

/// What is wrong with gpu-tests' directory and container on the VM: the run step's `--dir`
/// missing or the checkout's, another `run-gpu.sh` call naming another, or no cleanup step
/// removing `peacock-<dir>` under an `if:` that starts `always()`.
fn gpu_container_problems(workflow: &str) -> Vec<String> {
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
             would remove that clone's run"
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
    let removes = |step: &&Vec<&str>| {
        step.iter().any(|l| {
            !l.trim_start().starts_with('#')
                && l.contains(&format!("docker rm -f {container}"))
                && !l.contains(&format!("docker rm -f {container}-"))
        })
    };
    let Some(step) = steps.iter().find(removes) else {
        problems.push(format!(
            "gpu-tests: no step runs `docker rm -f {container}`, so a cancelled run's container \
             keeps the card, and the next run's `run` refuses the busy name"
        ));
        return problems;
    };
    let condition = step
        .iter()
        .find_map(|l| l.trim_start().trim_start_matches("- ").strip_prefix("if:"))
        .map(str::trim);
    if !condition.is_some_and(|c| c.starts_with("always()")) {
        problems.push(format!(
            "gpu-tests: the step removing {container} runs on {condition:?}, not on \
             `always() && …`: a failed or cancelled run would leave its container on the VM"
        ));
    }
    problems
}

/// A cancelled or failed `gpu-tests` run leaves its container running on nebius-gpu, which keeps
/// the card busy and the idle watcher off; and a cleanup of `peacock-peacockdb` would remove a
/// developer's run instead. The job's calls name `--dir ci`, and the cleanup removes
/// `peacock-<dir>` of the run step, under an `if:` that starts `always()`.
#[test]
fn the_gpu_job_owns_its_dir_and_removes_its_container() {
    let text = std::fs::read_to_string(repo_root().join(".github/workflows/pipeline.yml"))
        .expect("read pipeline.yml");
    let problems = gpu_container_problems(&text);
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    let job = |ship: &str, run: &str, cleanup: &str| {
        format!(
            "jobs:\n  gpu-tests:\n    runs-on: ubuntu-latest\n    steps:\n      \
             - uses: actions/checkout@v4\n      - name: Ship\n        run: {RUN_GPU} \
             --instance-id x {ship}\n      - name: {GPU_RUN_STEP}\n        run: {RUN_GPU} \
             --instance-id x {run}\n{cleanup}  next-job:\n    runs-on: ubuntu-latest\n"
        )
    };
    let step = |condition: &str, run: &str| {
        format!("      - name: Clean up\n{condition}        run: |\n          {run}\n")
    };
    let always = "        if: always() && steps.start.outcome == 'success'\n";
    let rm = |name: &str| format!("scripts/nebius.sh ssh 'docker rm -f {name}' || true");
    let (ship, run) = ("--dir ci ship", "--dir ci run list");
    assert_eq!(
        gpu_container_problems(&job(ship, run, &step(always, &rm("peacock-ci")))),
        Vec::<String>::new()
    );
    for (what, text) in [
        ("no --dir", job("ship", "run list", &step(always, &rm("peacock-peacockdb")))),
        (
            "the checkout's dir",
            job("--dir peacockdb ship", "--dir peacockdb run list", &step(always, &rm("peacock-peacockdb"))),
        ),
        ("another dir", job("--dir other ship", run, &step(always, &rm("peacock-ci")))),
        ("another container", job(ship, run, &step(always, &rm("peacock-peacockdb")))),
        ("a longer name", job(ship, run, &step(always, &rm("peacock-ci-old")))),
        ("no if", job(ship, run, &step("", &rm("peacock-ci")))),
        ("not always()", job(ship, run, &step("        if: success()\n", &rm("peacock-ci")))),
        ("commented out", job(ship, run, &step(always, &format!("# {}", rm("peacock-ci"))))),
        ("no cleanup", job(ship, run, "")),
    ] {
        assert!(
            !gpu_container_problems(&text).is_empty(),
            "{what}: accepted\n{text}"
        );
    }
}
