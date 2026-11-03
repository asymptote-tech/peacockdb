# Fork containers onto master, without the join optimizer

Kind: production

**This task closes [#226](../tickets/benchmarks.md#t226) (benchmark provenance)**, which containers implemented and chain L now delivers.

Containers (PR 192, branch `ENS-containers`, chain C task 2, state `building`) is stacked on
`ENS-optimizer` (PR #165, join-optimizer, `done`, unmerged). Chain L needs containers' images,
bucket scripts, provenance and per-device tree, on master's code and master's registry. Master's
registry is the one that enables the 82 TPC-DS tp1_single cells; the branch has 71. The join
optimizer itself is not needed. This task makes `ENS-containers-rebase`: containers' own commits
replayed on today's master, green, as the base every later task of chain L branches from.

## What a trial rebase found (2026-11-02)

`git rebase --onto master ENS-optimizer ENS-containers` was run in a scratch clone, every conflict
resolved blindly, only to count them. It needed:

- **14 optimizer commits dropped and 16 containers commits replayed.** Master is 56 commits ahead
  of the optimizer's base. Those are chain K's device work and later.
- **5 stops, about 30 conflicted files.** By commit:
  1. the spec commit: `llm-wiki/tasks/tasks.md`;
  2. the images' docs: `llm-wiki/build-test.md`;
  3. the embeddings' S3 source: `scripts/exec_model/README.md`;
  4. **provenance**: `test_support/device_answer.rs`, `tests/test_corpus_goldens/benchmark.rs`,
     `scripts/calibration/{fetch_record.sh,nsys_hbm.py,record.py,split_record.py}` and their
     tests, `scripts/lib/calibration-bucket.sh`, and the four `h200/` result files;
  5. the registry-driven case list: `tests/common/corpus_benchmark_cases.inc`,
     `tests/peacock_gpu_benchmarks.rs`, `tests/test_corpus_goldens.rs`.

## Expected difficulties

1. **Containers is not independent of the optimizer branch.** Stop 4 edits files that
   `ENS-optimizer`'s first benchmark commit created or rewrote:
   `02f6a4d8 benchmarks: 26 device-enabled cases at sf40, the per-call record in S3 by its content`.
   That commit brought:
   - the S3 per-call record (`calibration-bucket.sh`, `fetch_record.sh`, `record.py`,
     `split_record.py`);
   - the `nsys_hbm.py` rework;
   - the H200 result files containers moved under `h200/`.

   Dropping it leaves containers' provenance commit editing code that is not there. The expected
   route is to carry `02f6a4d8` onto master first, then replay containers. The developer checks
   whether `02f6a4d8` itself depends on anything else of the optimizer's: `2bef3b81` (cost),
   `69448594` (the DPhyp crate) and `3625404b` (the optimizer). Any further commit carried is named
   in the detail file, with why. The join optimizer proper, `peacockdb-dphyp` and `exec_model`'s
   optimizer, must not come across.
2. **The registry moved under the case list.** `corpus_benchmark_cases.inc` is generated from
   `testdata/cost-registry.csv`. It must be regenerated from master's registry
   (`UPDATE_CANONICAL=1`, `the_benchmark_cases_follow_the_registry`), not hand-merged: 534 cells
   on the branch, about 630 on master. The H200 tree check then sees cases the H200 files never
   measured. Those must be declared as not yet measured on `h200`, in the check's own exception
   form, and the H200 files must not be re-measured.
2a. **The H200 list becomes CSV.** Containers holds `h200/` to `h200/cases.inc`, a
   `corpus_query_benchmark!` text nothing expands, through `PREDATES_LIST` (`benchmark.rs:512`).
   By the human's decision every per-device case list is
   `testdata/benchmark-results/<device>/cases.csv`, header `dataset,sf,query,modes,status,note`:
   `modes` `|`-joined and empty when skipped, `status` one of `ok`, `skipped`, `wrong`, `note` the
   skip reason or the `#NN` ticket. This task converts the H200's 14 lines once, each `ok` with an
   empty note, and the file is fixed from then on: nothing rewrites it. The mechanism stays
   containers' own, not a second one beside it: the constant names `cases.csv`, and the rule
   becomes **a device tree that holds a `cases.csv` is held to it; one without is held to the
   whole main list.** `PREDATES_PROVENANCE` keeps deciding only the provenance exception. One
   reader serves every device list, `test_support/device_cases.rs` (`DeviceCase`,
   `parse_device_cases`, `render_device_cases`); sf40-baseline adds the H100's file and its writer
   on it. The main list, `peacockdb-core/tests/common/corpus_benchmark_cases.inc`, stays in macro
   form.
3. **The two trees disagree on paths.** Master still has the old flat
   `testdata/benchmark-results/tpch.sf40/`. Containers moves results under `<device>/`. Master's
   files and the optimizer commit's files both land in `h200/`, and the newer of each pair wins,
   named in the detail file.
4. **Shared docs drifted.** `build-test.md`, `architecture.md`, `tickets.md` and the ticket files
   moved on master: chain K's counts, sections, tickets. Containers' prose is re-applied to
   master's text, never master's text to containers'. The case counts in `build-test.md` are
   recounted, not merged.
5. **The board.** Master has no chain C section; containers' copy of `tasks.md` has chain C's
   history. The fork's board is master's board plus chain L. **Chain L absorbs the containers
   task.** Its unfinished items go to their new owners, written in the detail file:
   - task 4's publishing (done-when 4, the images public on ghcr) is this task's;
   - tasks 6 and 6a, the fresh-VM check, go to new-hosts: `provision-vm.sh` and
     `bootstrap-vm.sh` only, on `nebius-cpu128`;
   - task 7 (the device runner, now `scripts/env/run-gpu.sh`) goes to new-hosts, without
     containers' host adapters: it calls `scripts/nebius.sh` directly;
   - task 10 is this task;
   - containers' own spec, plan and detail file, which the replay brings into `llm-wiki/tasks/`,
     are archived as a merged task's are;
   - task 11 (the 80 GB benchmark) goes to sf40-baseline and tp4-sized, those two modes only;
     other modes are dropped.

   Dropped by the human's decision, not deferred:
   - the agent-container launcher, `up.sh` and `down.sh`, with its live test from task 6 step 3
     (item 8);
   - task 12, the ensemble end to end on a scratch chain;
   - task 5 step 3, creating the `embeddings` bucket and uploading to it, and with it the
     embeddings' S3 path (item 8).
6. **Containers was mid-build.** Its tip is not known to be green. This task's bar is the fork's
   own gates, not containers' done-when:
   - the workspace builds in every shape;
   - the CPU and FFI tiers and `cost-report` pass. That includes master's rule that no core
     test reads the checkout (`no_core_test_reads_the_checkout`), which containers' case-list
     code breaks: the list is compiled in instead, in a commit of the developer's own;
   - `scripts/env/tests` pass;
   - the build image builds from the fork.

   A failure that containers' tip also has is recorded, and fixed only if a later chain L task
   needs that piece.
7. **PR 192 stays as it is.** The fork is a new branch and a new PR against master. What happens
   to PR 192 (retarget, close, or merge the optimizer first) is the human's call. The detail file
   lists the commit mapping, old to new, for that decision.
8. **Two pieces of containers go, by the human's decision.** They come across in the replay,
   since the 17 commits are replayed whole, and are removed after them in commits of the
   developer's own, never amended into rk00's.
   - **The agent-container launcher.** Deleted: `scripts/env/up.sh`, `scripts/env/down.sh`,
     `scripts/lib/board.sh`, `scripts/env/tests/test_up.py`, `scripts/env/tests/test_up_branch.py`.
     Their entries leave `scripts/env/tests/laptop.py` and `machine.py`, with the fakes only they
     used. `bootstrap-vm.sh` stops writing the instance marker (`$PEACOCK_MARKER`, "this machine
     runs instances") and stops setting up the shared Claude Code directory up.sh mounted
     (`CLAUDE_CONFIG_DIR` in `~/.bashrc`); no message names `up.sh`. `provision-vm.sh` stops
     recording the VM as "the default VM for up.sh and down.sh"; the `vm` file stays, for
     `--new-pat`. What only the launcher read goes with it: `peacock_forward`,
     `peacock_vm_target` and the marker in `scripts/lib/peacock-config.sh`, `MACHINE_INSTANCES` in
     `scripts/lib/machine.sh`. `scripts/lib/machine.sh` stays: `bootstrap-vm.sh` uses it.
     **The GPU-host key stays, and is one that already works** (the human's decision, 2026-11-03).
     `provision-vm.sh` no longer generates a key. It takes `--gpu-key <private key file>`, default
     `~/.config/peacock/gpu_ed25519`, and refuses, naming the flag, when that file is missing. It
     copies the key and its `.pub` over ssh's stdin to the VM's `~/.config/peacock/gpu_ed25519`
     (600), and writes `GPU_SSH_KEY_FILE` into the VM's `peacock.env`. On dev the key is
     `~/.ssh/id_ed25519`, which nebius-gpu and the H100 already accept, so nothing is authorized
     anew.
     `build-test.md`'s "Containers" shrinks to the VM and its two scripts, and "Instances" goes.
     The images are untouched: `docker/`, `image.sh`, `run-tests.sh`, `image-inputs.sh`,
     `images.yml`.
   - **The embeddings' S3 path.** Deleted: `scripts/env/upload-embeddings.sh`, the `--from-s3`
     branch of `testdata/fetch_embeddings.sh`, and `scripts/lib/embeddings-bucket.sh`: its one
     function the origin path still reads, `embeddings_pinned_sha`, moves into
     `fetch_embeddings.sh`. The S3 parts of `scripts/env/tests/test_embeddings.py` and
     `test_s3.py` go. The pinned origins and `testdata/embeddings.sha256` stay, and CI,
     GITHUB_ACTIONS and PEACOCK_NO_FETCH refuse the origins as on master.
   - **The many-developer refusals are read through** in `provision-vm.sh`, `bootstrap-vm.sh` and
     `scripts/lib/machine.sh`. One that still protects something stays; one that only guarded the
     launcher goes. Each refusal and its decision, with the reason, is listed in the detail file.

## Done when

1. `ENS-containers-rebase` exists on origin. It is based on master and holds containers' 16
   commits, rewritten, plus only the optimizer commits item 1 needs, each named.
   **Every replayed commit keeps its original author.** All 17 are rk00's
   (`rk00 <00romankolesnik@gmail.com>`), with the original author date. A rebase or cherry-pick
   does this by default; nothing may use `--reset-author`, `--ignore-date` or a squash that loses
   it. A conflict resolution is amended into the commit it belongs to, keeping that author. Work
   that is genuinely new, such as the regenerated case list, the board, or fixes the gates need,
   goes in separate commits under the developer's own identity. A test step checks it:
   `git log --format='%h %an <%ae> %ad'` over the replayed range against the original commits,
   matched by subject, with no difference.
2. The gates in item 6 pass on it. The case list is regenerated from master's registry, and the
   H200 tree check passes with its declared exceptions.
2a. `h200/cases.inc` is gone and `h200/cases.csv` holds its 14 cases in item 2a's format, read
   through `parse_device_cases`; the tree check holds a device tree to its `cases.csv` when it
   has one and to the main list otherwise, each branch tested; every page naming
   `h200/cases.inc` names the CSV.
3. `git diff master ENS-containers-rebase` holds no file of the join optimizer: no
   `peacockdb-dphyp`, no optimizer code in `exec_model`, no join-optimizer spec.
4. The build and run images for cuDF 25.02 and 26.02 on `ghcr.io/asymptote-tech/` serve the fork.
   They pull anonymously, and each one's `peacockdb.inputs` label equals the fork's own inputs.
   The published images were built from containers' `b50d9b2a`, whose inputs equal master's, and
   this task changes no `docker/` file, so no rebuild is expected. If a label differs, the images are
   rebuilt through `images.yml`, which then needs to land on master first in a separate commit
   (approved in advance). This is containers' done-when 4.
5. The detail file holds the commit mapping, the conflicts and how each was resolved, and the
   containers task ownership above.
6. Item 8 holds, in developer commits after the 17 replayed ones: none of its deleted files is on
   the fork; `git grep` over the fork outside `llm-wiki/tasks` and `llm-wiki/archive` finds no
   `up.sh`, `down.sh`, `board.sh`, `upload-embeddings`, `--from-s3`, `embeddings-bucket` or
   `config/peacock/machine`, but for `scripts/lib/image-inputs.sh`, which the human left
   untouched; `scripts/env/tests` pass; the detail file lists each refusal read and its decision.

## Scope

Containers' own files (`docker/`, `scripts/env/`, `scripts/lib/`, `.github/workflows/images.yml`,
the benchmark harness, `scripts/calibration/`, `testdata/benchmark-results/`), as containers' spec
lists them. Also whatever `02f6a4d8` touches, and the shared wiki pages. No new behaviour, but
for item 2a: `peacockdb-core/src/test_support/device_cases.rs` (new) and the list rule in
`peacockdb-core/tests/test_corpus_goldens/benchmark.rs`. Item 8 deletes and trims: the launcher's
five files, `upload-embeddings.sh`, `embeddings-bucket.sh`; trims `provision-vm.sh`,
`bootstrap-vm.sh`, `scripts/lib/{machine,peacock-config}.sh`, `fetch_embeddings.sh`, the tests in
`scripts/env/tests/`, and `build-test.md`.

Component-level API: none beyond what containers already adds.
