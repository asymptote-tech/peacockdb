
# Infrastructure tickets

Tests, CI, hosts, testdata etc

<a id="t263"></a>
### #263 — a tickets-only commit can turn the rust tier red while CI skips the pipeline
`ticket_is_open` (`test_support/duckdb_oracle.rs`) reads `llm-wiki/tickets/` at test run time, so
the open-ticket set is now a test input. `pipeline.yml`'s two skip layers both still class the
whole of `llm-wiki/` and every `.md` as inert: `paths-ignore` at the top, and the `changes` job's
`grep -qvE '(\.md$|^llm-wiki/)'`. The file's own comment states the condition this breaks — "a doc
file that ever becomes an input has to come off this list".

So a commit that only moves a ticket between `llm-wiki/tickets/` and
`llm-wiki/archive/archived-tickets.md` can break `--lib` and no run reports it. Two cases are live:
`an_archived_ticket_is_not_open_and_a_listed_one_is` asserts `ticket_is_open(205)` and
`ticket_is_open(251)` directly, and every `duckdb_divergent(<n>)` line goes red when `<n>` is
archived. #205 is on the archival path already — `corpus-coverage.md` names its fix. The helper's
post-merge protocol guarantees the shape: it archives task specs in a master-only commit carrying
nothing else. Master then goes red on the next unrelated code push, with the cause several commits
back.

Not [#252](#t252), though they share a cause. That one is a staged binary not receiving the
checkout on a remote CPU host; this one is CI declining to run at all.

**Two fixes, and the choice is a cost decision rather than a technical one.** Excepting
`llm-wiki/tickets/` is not expressible in `paths-ignore` — GitHub Actions has no negation there —
so the first shape is to drop the doc patterns from `paths-ignore` and let the `changes` job carry
the whole rule, where a grep can hold the exception. That costs a `changes` job per docs commit,
and the job currently answers "run everything" for any `push` event, so the push trigger needs the
same logic before it is safe. The second shape is to move the open-ticket set out of the docs tree,
which ends the class and also closes #252. Until one lands, `build-test.md`'s CI section says to
run `--lib` by hand after a tickets-only commit.

Found by the completeness analyst on the `duckdb-oracle` branch, 2026-10-08, which is the branch
that made the wiki an input.

<a id="t258"></a>
### #258 — the PR comment has about seven corpus rows of headroom left
`the_pr_comment_fits_under_the_body_cap` holds the cost widget's comment under GitHub's
65,536-byte body limit. pbench's 60 rows took it to 79,352, and three cuts in the pbench task
brought it back to **63,339** — the fully-enabled tick made one character wider, a 7-character sha
in the comment's query links (the archived page keeps the full one), and no `<sub>` on the three
mode cells. None of the three lost information.

That leaves **2,197 bytes, about six corpus rows**, and chain J already owes three of them: the
queries [#255](complete-coverage.md#t255) blocks. `int8-key-group` spent the fourth when pbench's
device cycle landed it. So the next task after those to add a corpus query goes
red here, and the lever left is a choice about what a PR comment is for rather than another byte
cut.

**The two options, measured in the pbench task:** take the Features column out of the comment
(about 9 KB, and the page still carries it), or collapse pbench in the comment to its summary line
with the rows on the page only. The first is the better trade on the argument that a feature matrix
does not change with the PR while a per-row pass or fail does — but it changes what every reviewer
sees in every comment from then on, which is why it is filed for a human rather than taken inside
the task that measured it.

<a id="t252"></a>
### #252 — six corpus cases read the checkout, which a remote CPU run never ships
`ticket_is_open` (`test_support/duckdb_oracle.rs`) resolves `llm-wiki/tickets/` through
`env!("CARGO_MANIFEST_DIR")` with no environment escape, so the four `duckdb_divergent` cases —
`duckdb_tpcds_q17`, `q58`, `q61`, `q66` — look for the ticket files at the build host's path.
`scripts/build-test.sh` ships binaries, goldens and data and never source, and `rust_only_targets`
stages `test_cpu_corpus` among them, so those four go red on verda and pass locally.

`testdata.rs` states the rule the other goldens follow: the compile-time path is the fallback and
`PEACOCK_TESTDATA_DIR` wins, because a binary is built on one host and run on another (#49).
`ticket_is_open` has no equivalent, and an escape variable nobody sets would be a no-op.

Six cases are in the class, not four. `all_modes_expands_to_the_five_in_either_position`
(`tests/test_cpu_corpus.rs`) reads `tests/common/corpus_cases.inc` the same way, through
`corpus::corpus_lines`, and panics in `macro_invocations`' `read_to_string`; and
`every_timed_case_is_enabled_on_a_device` (`tests/test_corpus_goldens/benchmark.rs`) already did
this before the oracle landed. `test_corpus_goldens` is staged too — `rust_only_targets`' second
axis greps only the top-level `tests/*.rs` for `repo_root`, so a `CARGO_MANIFEST_DIR` read inside
a submodule is invisible to it. `test_module_layout` and `test_ci_coverage` read the source tree
as well and are the two that genuinely never ship, excluded by that same rule at the top level.

**Fix proposed:** the push side, carrying both reads — `llm-wiki/tickets/` and
`peacockdb-core/tests/common/` — which is why this is a ticket and not a line in the task that
found it. Measured by reading `rust_only_targets` and its classifier, over
the duckdb-oracle branch; the four `duckdb_divergent` cases were found in that task's review round
3 and the other two in its completeness pass.

<a id="t178"></a>
### #178 — shad-gpu is shared, and a pool that cannot be built is a neighbour's fault
Each gtest main reserves a fixed byte budget (`kPoolBytes` beside its `main()`, listed in
`build-test.md`). Our own runs queue on the `shad-gpu` concurrency group. Work outside this repo
does not, so a stranger holding part of the card still fails us. **Tentatively closed**: it cannot
be proven closed from here.

The pool line says whose failure it is. `[rmm] pool of N GiB could not be built with M GiB free`
at the top of the log is a neighbour: date a line below naming the run and the binary, re-run the
job once, and do not debug it. A pool that *was* built and a test that then dies with `Maximum
pool size exceeded` is ours: the budget is too small, and a re-run buys nothing.

- 2026-09-12: CI run `34659896447` on PR #144 (`d41f223a`), `peacock_tpch_tests`: `pool of 69.0
  GiB could not be built with 14.9 GiB free` at 00:08 UTC; `peacock_tpchv_tests` four binaries
  later saw 103.0 GiB free, so a stranger held ~129 GiB for those minutes. Re-run once.
- bp-benchmarks, dispatch 5: not CI — a non-CI process held 62 GiB and 90–98 % of the card
  for six hours, and `peacock_gpu_benchmarks` measured q6 at six times its committed time
  beside it. Nothing measured beside a neighbour is published; the gate ran green meanwhile.

<a id="t176"></a>
### #176 — the CI coverage guard checks one direction only
Priority: low
`every_rust_test_target_is_named_by_ci` fails when a target exists that no workflow runs. Nothing
fails when a workflow names a target that does not exist.

That way round is not silent, but it is expensive and late: cargo errors inside the cuDF leg after
the C++ build and the dataset generation, so a typo or a step added ahead of its test file costs a
full run to discover. The case: a `--test test_cpu_end_to_end` step was added three commits
before the file, and both legs went red on it.

The converse is nearly free — `workspace_test_targets()` and the `--test` line parsing both exist,
so it is one assertion that every target pipeline.yml names is in the workspace set. The exemption
list already gets this treatment; the step lines do not.

<a id="t129"></a>
### #129 — The "26.02" CI leg builds against a 25.10a image; the GPU job has no fork guard
Two unrelated smells in `pipeline.yml`, both found auditing the CI section of
build-test.md. (a) The dataset-matrix matrix leg labelled `cudf: "26.02"` runs
`rapidsai/base:25.10a-cuda12-py3.12`, so the compile-only 26.02 coverage the wiki and
`#94` both rely on is actually 25.10a coverage; the label is the only place 26.02 appears.
Either bump the image or rename the leg — as it stands, "26.02 compiles" is a claim no job
makes. (b) `s3-datasets` explains its fork guard as mirroring "the GPU job's fork guard",
but `gpu-tests` has no job-level `if:` — on a fork PR `secrets.SHAD_GPU_SSH_KEY` is empty,
so Setup SSH writes an empty key and the job goes red on ssh instead of skipping. Moot
while the repo has no forks, which is exactly why it will bite later.

