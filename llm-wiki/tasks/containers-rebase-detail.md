# containers-rebase — detail

## Run log

- 2026-11-03: coordinator start. Board `building`. Preconditions (plan Task 1 Step 1) hold:
  origin/master `dc90b51c` carries this plan (Step 7a present); `611b5dcd..origin/ENS-containers`
  = 16; merge-base of ENS-optimizer and ENS-containers = `611b5dcd`.
  Tips: `origin/ENS-containers` = `b71758e9`, `origin/ENS-optimizer` = `561411b4`.
- verda: not reachable (no `VERDA_CLIENT_ID`); CPU runs local on dev.
- Surgery state lives in `.claude/ensemble/L.surgery` until Task 1 Step 5 folds it in here.

## Surgery

Folded from `.claude/ensemble/L.surgery` at Task 1 Step 5; kept in step with it through Step 10. Done: Step 10 pushed; `L.surgery` deleted at Task 5.

```
# containers-rebase surgery state (coordinator)
fork-board = e06992b7e23f467e8e921eb25cb4ff5b4e1555d7  (ENS-containers-rebase: master + board commit e06992b7)
origin/master = dc90b51c94f0c644697c3ad050b7925106e5b60b
origin/ENS-containers = b71758e9a2922436a7b47b4a7d976c276587c5f8
origin/ENS-optimizer = 561411b41123568146e1bb13d99e968ca4ab04c7
rerere.enabled = false
step: 3 (cherry-pick -x 02f6a4d8 onto detached origin/master)
step 3 done: fork-carried = f3db66b1b12a65764b6e15d06827bcf40c2b339b (02f6a4d8 carried; conflict benchmark.rs resolved per table + macro_invocations_in in corpus.rs/mod.rs + 2 unit tests in corpus/tests.rs)
step: 4 (git branch -f containers-replay origin/ENS-containers; git rebase --empty=keep --onto fork-carried 611b5dcd containers-replay)
step 4: stops 462ed26b, 8437d6cd (empty, kept -C) done; at f75cba7e build-test.md
f75cba7e: build-test.md resolved by coordinator (master's text + Images and GPU host scripts rows + header totals +204 python, interim; recount in Task 5)
199bde5f: README.md = ours (master has neither line); build-test.md: header +20 image tests, Images row containers', Datasets embeddings paragraph + bucket bullet + calibration-record bullet (from optimizer base text, documents carried fetch_record.sh/records.sha256)
d2a0f88c: build-test.md resolved by coordinator (11 hunks: counts interim +delta, prose paragraph merged keeping master's mask sentences, Calibration + cuDF smoke rows containers', benchmark-flow/derived-text/record hunks containers'); Rust files -> developer
d2a0f88c: Rust resolved by developer (device_answer: cudf_release + master doc; benchmark.rs: use + Provenance/device_slug, body d2a0f88c's), cargo check green
cb8c3637: build-test.md resolved by coordinator (counts interim; golden table = master's + case-list row only, optimizer rows dropped; Corpus benchmarks section containers'); benchmark.rs -> developer (no compile check, Step 7a ports)
cb8c3637 done (developer kept master mask tests + CASE_LIST inside hunk; not compiled). stop at 615b60e8 build-test.md
615b60e8: build-test.md = containers' tpcds sf40 row + bullet
b50d9b2a: build-test.md header +48 image tests
b71758e9: build-test.md header +40 python
b71758e9 -> fc0796b9 had markers committed (script error); fixed and amended (author kept): header = master's + 208 image and launcher tests
step 5 done: ENS-containers-rebase = replay + board cherry-pick c21b90d9; containers-replay deleted. next: 7a (developer), 7, 8, 9, 10 push
```

Resolution notes beyond the plan's table:
- `02f6a4d8` carry: the developer also added two unit tests for `macro_invocations_in` in
  `test_support/corpus/tests.rs` (no containers commit touches that file).
- `cb8c3637`: HEAD's side of the body hunk also held master's mask tests (`1634e17c`) and the
  carry's `CASE_LIST`; both kept, cb8c3637's side taken whole after them.
- `build-test.md` stops were resolved by the coordinator, containers' prose onto master's text.
  Counts in the header and tier headers are interim deltas; Task 5 recounts. The optimizer-only
  golden rows (stats sidecar, cardinality, run.py, cost_report.html) that cb8c3637's side carried
  as context were dropped. The calibration-record bullet in "Datasets" (from the optimizer base's
  text, not a containers commit) was kept: it documents `fetch_record.sh` and `records.sha256`,
  which the carried `02f6a4d8` brings.
- `b71758e9`: a resolution script failed and the rebase continued with conflict markers in
  `build-test.md`'s header; caught before the branch moved, fixed and amended into the replayed
  commit (`--amend --no-edit`, rk00's author and date kept). No other replayed commit holds a
  marker (`git grep` over each).

Steps 7–9 (2026-11-03):
- Step 7a committed as a developer commit; Step 7 (`--no-run` for rust-only core and cost-report) green.
- Step 8: authorship diff empty; 17 rk00 commits with their original author dates.
- Step 9: commands 1 and 2 print nothing. Command 3 prints one line: `build-test.md`'s up.sh prose
  ("branches are named unlike their tasks (`ENS-optimizer` for `join-optimizer.md`)", from
  `b71758e9`). It is an example in launcher prose that Task 2 deletes; not amended into rk00's
  commit. Task 5 Step 3 re-runs the check on the final tip.
- `carried` (02f6a4d8 on ENS-optimizer) = `f3db66b1`.

## Refusals read

Task 2, confirmed against the code by the developer (line numbers are `origin/ENS-containers`'s).

| Script, line | Refuses | Decision | Why |
|---|---|---|---|
| `provision-vm.sh` :120–139 | `--user`, `useradd`, sudoers when user ≠ login | kept | separates each developer's PAT, git identity and gh login on a shared VM |
| `provision-vm.sh` :53–55 | no git `user.name`/`user.email` | kept | the VM's commits carry it |
| `peacock-config.sh` :45–76 | a token or a missing key in `peacock.env` | kept | the file is passed between developers; bootstrap and fetch-sf40 read the keys |
| `provision-vm.sh` :86–89 | no token, not `github_pat_…`, cannot read the repo | kept | the PAT sits on a shared VM; scoped to one repo |
| `provision-vm.sh` :141–151 | the marker test, its hooks-free branch read | dropped | bootstrap always runs and refuses a wrong `--ref` itself |
| `provision-vm.sh` | a missing `--gpu-key` file (and its `.pub`) | kept, taken not made | a VM must reach the GPU hosts with a key they already accept (human, 2026-11-03) |
| `provision-vm.sh` :173 | the "default VM" line | line dropped, `vm` file kept | `--new-pat` reads the file |
| `provision-vm.sh` :12–13 | bash older than 4.4 | kept | the script needs 4.4; macOS ships 3.2 |
| `provision-vm.sh` :23–39 | duplicate, valueless or unknown flag; no target; VM-only flags with `--new-pat` | kept, `--gpu-key` added | validation before any side effect |
| `provision-vm.sh` :41–47 | malformed target, user, VM or branch name | kept | the user name reaches `useradd` and sudoers |
| `provision-vm.sh` :59–63 | no `peacock.env` and no `--env`; `--env` missing | kept | the VM needs the storage keys |
| `provision-vm.sh` :66 | no `ssh-keygen` | dropped | no key is made any more |
| `provision-vm.sh` :71, :73 | no `gh`; origin not on GitHub | kept | the PAT check needs both |
| `provision-vm.sh` :107 | VM unreachable over ssh | kept | an ssh failure stops the run |
| `provision-vm.sh` :186 | gh on another VM refusing the new PAT | kept | the developer must know a VM holds the old token |
| `bootstrap-vm.sh` :35 | root | kept, reason reworded | the clone, gh, Claude Code and the image's runs are the developer's |
| `bootstrap-vm.sh` :38–40 | `--ref` on a clone on another branch | kept, names both branches | the clone is never silently on another branch |
| `bootstrap-vm.sh` :26–33, :41 | bad flags, bad `--cudf`; no sudo | kept | validation; installs need sudo |
| `bootstrap-vm.sh` :54, :73, :84, :101, :125 | unknown cuDF spec; unpinned value; gh sha256; Claude Code or superpowers off pin | kept | pinned installs |
| `bootstrap-vm.sh` :62–64; `machine.sh` umask 002, locks | group `peacock`, data dir 2775 | kept | sf1 and cuDF checkouts shared by the VM's developers |
| `bootstrap-vm.sh` :89–94 | `.bashrc` `CLAUDE_CONFIG_DIR` | dropped, `DISABLE_AUTOUPDATER=1` kept | the shared dir was up.sh's mount |
| `bootstrap-vm.sh` :135–136 | the instance marker | dropped | the human's decision |
| `machine.sh` :23–25 `machine_git`; `bootstrap-vm.sh` :36–37 `safe_git` | a hook planted in the clone's `.git` | dropped | without instances only the developer writes `.git` |
| `machine.sh` :40, :55 | no pinned cuDF commit or URL; commit not fetchable | kept | the FFI build needs the checkout |
| `machine.sh` :73, :78 `machine_image_checked` | image without a label; image of other inputs | kept | bootstrap's registry fetch and sf1 run in that image |
| `machine.sh` :120 | the generator made no dataset dir | kept | sf1 must appear whole |
| `peacock-config.sh` :83–93 `peacock_vm_target`; :105, :109 `peacock_forward` | VM file / line / name; forwarded twice, `--vm` | dropped | only the forwarding read them, and it goes |

Task 2 notes:
- Beyond the plan's Files: `llm-wiki/README.md`'s `up.sh` line back to master's; `MACHINE_CCACHE`
  (read only by up.sh) gone; `bootstrap-vm.sh` no longer sources `peacock-config.sh`.
- `test_up.py` covered code that stays: its two `provision-vm.sh` tests moved to
  `test_provision.py`, and seven `machine.sh` step tests were ported to `test_bootstrap.py`. The
  two-Linux-user concurrency case was not ported.
- `test_bootstrap_names_no_launcher` asserts the output names no script but `bootstrap-vm.sh`
  and `provision-vm.sh` (the plan's "no `up.sh`" would put `up.sh` into the grep's scope).
- shellcheck (0.11.0, not installed on dev; run from a throwaway install) has one new warning
  against `origin/ENS-containers`: SC2034 `PEACOCK_VMS appears unused` in `peacock-config.sh`
  (its reader there, `peacock_vm_target`, went; `provision-vm.sh` reads it). Left as is; the
  file already carries the same warning for three other variables.
- `scripts/lib/image-inputs.sh` keeps its `up.sh` comment (:7-8) and its hooks-free git, by the
  human's decision that the file stays untouched.
- No script reads `GPU_SSH_KEY_FILE` yet; only `provision-vm.sh` writes it (new-hosts' to read).
- A VM bootstrapped by containers keeps its old `CLAUDE_CONFIG_DIR` line; a rerun does not remove it.
- Recount for Task 5: `scripts/env/tests` 208 → 155 cases.
- dev's `/` was at 100% (819 MB free) after Task 2; the developer had pulled and then removed
  the run images. This worktree's `target/` is 57 GB.
- 2026-11-03: coordinator removed this worktree's `target-cudf-rapids-cuda-12.2` (31 GB, ignored, rebuildable) to free dev's disk for Task 3.

Task 3 notes:
- Step 1: `the_benchmark_cases_follow_the_registry` failed as planned: 116 cells enabled and not
  listed at sf40, 20 listed and not enabled (`anti_join_reorder`, `left_join_fixed`,
  `left_join_reorder`, `semi_join_reorder`, five modes each: the four named join queries
  `2bef3b81` added to the registry under containers' base `611b5dcd`; not carried, not on master).
- Step 2: regenerated from master's registry: 534 → 630 cells (534 + 116 − 20). A second
  `UPDATE_CANONICAL=1` run left the file byte-identical.
- Step 3: `test_corpus_goldens` 45/45. `git diff origin/ENS-containers` over
  `testdata/benchmark-results` and `testdata/calibration/records.sha256` is empty, for the branch
  and for the working tree. Master has no commit on those paths after `623073da`. The six
  `h200/` files are `02f6a4d8`'s flat files moved unchanged (`git diff 02f6a4d8 HEAD` shows them
  as renames with no line changed). Master's older flat `tpch.sf40/tp1_single` and `tp4_sized`
  are gone from the fork; 02f6a4d8's are the newer of each pair.
- Step 4: `02f6a4d8` → `f3db66b1 (02f6a4d8 on ENS-optimizer)` in `build-test.md` (three) and
  `tickets/benchmarks.md` (#226). `f3db66b1`'s case list, `h200/` tree, `records.sha256` and
  `create_nsys_profile.sh` equal `02f6a4d8`'s, so the sentences stay true. The grep still finds two
  old references: `tasks.md:107`, the board's chain-L entry, which is the coordinator's; and
  `h200/cases.inc:2`, which Task 4 replaces. Step 3's empty-diff check forbids editing it now.
- Step 5, on dev, the worktree's `target/`, `--test-threads=2`: `--lib` 905 passed, 2 ignored;
  `test_cpu_corpus` 2399; `test_corpus_goldens` 45; `test_cost_model` 3; `test_golden_format` 41;
  `test_crate_privacy` 1; `cargo build --features rust-only -p peacockdb` built;
  `cargo test -p cost-report`: the bin 47, `test_source_formatting` 32 (incl. `ci_coverage::*`
  and `no_core_test_reads_the_checkout`). No failure, no warning. For Task 5's recount:
  `build-test.md`'s cpu header says `test_corpus_goldens` 44, and the fork has 45.
- Steps 6–8, on dev, one cuDF version at a time for disk. The worktree's `target/` (25 GB,
  rust-only) was deleted first: dev had 7.2 GB free with one build image on disk. Each image was
  built from the clean tree at `19f57fb7` and carries both tags. dev's docker uses the containerd
  store, so `docker image ls` reports compressed content plus unpacked layers. The table gives
  `.Size` (compressed) and the `ls` figure. The unpacked size, the difference, matches
  build-test.md's table.

  | image | ID | `.Size` | `docker image ls` | build time |
  |---|---|---|---|---|
  | build 25.02 | `f05ac6dfae6b` | 5.19 GB | 15.4 GB | 4 min 27 s |
  | run 25.02 | `cb23e758511f` | 3.18 GB | 8.32 GB | 2 min 30 s |
  | build 26.02 | `e03d286d67b8` | 5.20 GB | 15.1 GB | 4 min 7 s |
  | run 26.02 | `11a7ae997d55` | 3.94 GB | 10.2 GB | 2 min 44 s |

  Revision label `19f57fb73e6b` on all four. Driver labels: 535.104.05 (25.02), 575.57.08 (26.02).
  A build image held about 24 GB of `/var/lib/containerd` while it was on disk.
- Step 7, both versions. The plan's literal commands: the gpu `--no-run` compile passes, with
  no warning. Both FFI tiers exit 127, because the test binaries cannot load `libcudf.so`. The
  build image sets no `LD_LIBRARY_PATH`, and the FFI install bundles Arrow's dependencies but
  not libcudf (`cpp/CMakeLists.txt`'s install(CODE)). CI's FFI step sets
  `LD_LIBRARY_PATH: /opt/conda/lib` (`pipeline.yml`). **Shared:** `origin/ENS-containers`
  (exported with `git archive`, the same 25.02 image, the same `third_party/cudf`) fails
  `ffi_tests::` the same way. The image inputs match: `git diff origin/ENS-containers` over
  `docker/`, `image.sh`, `image-inputs.sh` and the cudf gitlink is empty. With
  `-e LD_LIBRARY_PATH=/opt/cudf/lib` added, `ffi_tests::` passes 4/4 and `test_ffi` 3/3 on each
  version. Not fixed. For Task 5's docs: build-test.md's "The build image" should say that the
  FFI test binaries need `-e LD_LIBRARY_PATH=/opt/cudf/lib`.
- Step 8. Every file under `scripts/env/tests/` and `scripts/calibration/tests/` passes, with
  these conditions:
  - `test_plot.py` needs matplotlib, which dev lacks. It passed 7/7 in each build image.
  - `test_image_any_user.py` needs both build images on disk at once, and dev cannot hold them
    beside the run images. It passed by name per version, each with its own image present.
  - `test_run_image.py` 9/9 and `test_run_tests.py` 24/24 ran with both run images present.
    The 25.02 run image was saved and reloaded under the same ID.
  - `PEACOCK_STAGED_INSTALLS` was a tmpfs dir holding, per version, the FFI build's cmake
    install (`lib/`, `bin/`) and the four gpu test binaries, stripped, as
    `build-test-shadgpu.sh --build` stages them.
- After the runs: the images, build cache, `target-img-*`, `.home-img` and the staging are all
  removed. `.gitignore` gains `/target-img-*/` and `/.home-img/`; `git check-ignore` matches
  both, and `git status --short` showed only `.gitignore` while `target-img-26.02/` and
  `.home-img/` existed.

Task 4 notes:
- Step 2: both commands failed to compile on the missing items (`CaseStatus`, `DeviceCase`,
  `DEVICE_CASES_HEADER`, `parse_device_cases`, `render_device_cases`, `DEVICE_LIST`).
- After the implementation and before the conversion, `every_device_list_parses` and
  `every_declared_case_has_a_section_in_the_tree` failed as expected: with no `h200/cases.csv`,
  `h200/` fell to the main list and `tpcds.sf40/tp1_rowgroup` does not exist there.
- Mutation check: with the rule forced to ignore the device list, `a_tree_with_its_own_list_is_held_to_it`
  and the tree check both fail; restored byte for byte.
- Beyond the plan's tests: `an_ok_or_wrong_row_names_its_modes`, `an_unknown_status_is_refused`,
  `a_row_short_of_six_fields_is_refused`. Every refusal the Interfaces list has a test.
- Step 4: the fork's `h200/cases.inc` equalled `origin/ENS-containers`' before the conversion.
  `cases.csv` is 15 lines, 14 `,ok,`. Step 5's diff printed nothing; 14 invocations on each side.
- `device_cases_path` has no caller yet (sf40-baseline's writer). `DEVICE_LIST` in benchmark.rs
  spells `cases.csv` a second time, as the Interfaces fix it.
- For Task 5's recount: `--lib` gains 11 (`device_cases::tests`), `test_corpus_goldens` gains 2.

## Commit mapping

Old → new, matched by subject; every new commit keeps rk00's author and author date (Task 1 Step 8). Sorted by author date.

| old | new | author date | subject |
|---|---|---|---|
| `02f6a4d8` | `f3db66b1` | 2026-10-17 | benchmarks: 26 device-enabled cases at sf40, the per-call record in S3 by its content |
| `462ed26b` | `d0789eb2` | 2026-10-21 | containers: the spec, the plan and the record of the images and the hosts |
| `8437d6cd` | `49202378` | 2026-10-21 | containers: building |
| `b84a192e` | `c2c04772` | 2026-10-21 | containers: the conda specs of both cuDF environments and every pin, with their hashes |
| `7bc901f2` | `2d6bc6f3` | 2026-10-23 | containers: the build image, per cuDF version, offline and as any user |
| `f8694342` | `e3712fef` | 2026-10-23 | containers: logins, Claude Code and the data live on the machine |
| `c0c7b9cd` | `5df7f213` | 2026-10-24 | containers: the device-run image, per cuDF version |
| `f75cba7e` | `34cc024d` | 2026-10-24 | containers: the images' publishing workflow, and how to use the images |
| `0d9e1dc4` | `bb80e261` | 2026-10-26 | containers: the four images published on ghcr.io, public |
| `199bde5f` | `fde47edd` | 2026-10-27 | containers: the embeddings' S3 source, pinned by content, and the script that fills it |
| `cb8c3637` | `61ecd637` | 2026-10-27 | containers: the benchmark cases follow the registry; a failed case is recorded and the run goes on; a run cut short resumes on request |
| `d2a0f88c` | `602ff9f4` | 2026-10-27 | containers: each benchmark result and record names its device, driver, CUDA, cuDF and host; the tree is per device |
| `2976fd7e` | `3ad53d65` | 2026-10-28 | containers: the ensemble end to end on a scratch chain; an image refused when its inputs differ from the instance's tree; sf1 kept once per generator; tpcds sf40 from its bucket |
| `615b60e8` | `6445eec0` | 2026-11-01 | datasets: tpcds sf40 in its bucket, generated by the script and checked object by object; the upload and the S3 check know it |
| `804ce52a` | `3ff53850` | 2026-11-01 | containers: sf1 on the machines with the generator's default vectors, as CI; the embeddings bucket an archive on no run path; the S3 endpoint from the env file |
| `b50d9b2a` | `1f349b07` | 2026-11-01 | containers: a VM readied by one script, isolated instances started by another; an image refused when its inputs differ from the instance's tree; sf1 and the cuDF sources kept once per machine |
| `b71758e9` | `2e696eef` | 2026-11-02 | containers: one command readies a VM for a developer, one starts a chain; a chain that ran is resumed only on the branch named |

## Carried from ENS-optimizer

`02f6a4d8` only, as `f3db66b1` (`cherry-pick -x`). It is the second commit on `ENS-optimizer`; its
parent `69448594` (peacockdb-dphyp) shares only `pipeline.yml` with it, in a separate hunk.
`2bef3b81` (cost) and `3625404b` (exec_model optimizer) come after it, so it cannot use them. Of
containers' files only `scripts/calibration/tests/harness.py` was touched by `3625404b`, comment
text only, and no containers commit edits it. The four registry rows `2bef3b81` added
(`anti_join_reorder`, `left_join_fixed`, `left_join_reorder`, `semi_join_reorder`) were in
containers' old case list through its base; the regeneration from master's registry dropped them.
The six `h200/` result files are `02f6a4d8`'s, moved by `d2a0f88c` with no line changed; master
never touched them after `623073da`.

## Containers tasks, their new owners

- task 4's publishing (done-when 4, the images public on ghcr) → this task (plan Task 6);
- tasks 6 and 6a, the fresh-VM check → new-hosts: `provision-vm.sh` and `bootstrap-vm.sh` only,
  on `nebius-cpu128`;
- task 7 (the device runner, now `scripts/env/run-gpu.sh`, calling `scripts/nebius.sh`, no host
  adapters) → new-hosts;
- task 10 → this task;
- task 11 (the 80 GB benchmark) → sf40-baseline and tp4-sized, those two modes only; other modes
  dropped.

Dropped by the human's decision, not deferred: the agent-container launcher (`up.sh`, `down.sh`,
`board.sh` and their tests, deleted in Task 2), task 12 (the ensemble end to end on a scratch
chain), task 5 step 3 and the embeddings' S3 path (deleted in Task 2).

PR 192 and `ENS-containers` are left as they are; what happens to them is the human's call, and
the mapping above is for that decision.

## From containers' record

The full record and plan: `git show origin/ENS-containers:llm-wiki/tasks/containers-detail.md`
(and `…/containers-impl.md`). What is still open or not written down elsewhere:

- **The CUDA mix in the 25.02 build image.** nvcc 12.9 compiles everything (`CUDART_VERSION`
  12090). The shared CUDA libraries link from `/opt/cudf/lib` (12.2). nvcc also adds 12.9's
  `libcudadevrt.a` and `libcudart_static.a`. 36 runtime entry points exist in 12.9's static
  cudart and not in 12.2's `libcudart.so`, `cudaGetDriverEntryPointByVersion` among them. A call to
  one binds to a second, static runtime instead of failing to link. `scripts/env/no-static-cudart.sh`
  guards it. Open: compare nebius-gpu's native `cpp/build/build.ninja` and `CMakeCache.txt` with
  the image's.
- **Run image, never seen on a device (new-hosts):** the `dlopen`s of `libcuda.so.1`, nvrtc and
  cufile under `--gpus all`; `/proc/driver/nvidia/version` inside the container; `host=` under
  `--hostname`. Neither image has `/usr/share/zoneinfo`; tzdata is under
  `/opt/cudf/share/zoneinfo`. `libnvrtc.so` and `libnvJitLink.so`, which libcudf names
  unversioned, resolve in neither image. After a pull, the record must be moved aside on the host:
  `run-tests.sh bench` refuses a record with rows.
- **`nsys_hbm.py`'s device check** was never run on a real Nsight export. The fixture's
  `TARGET_INFO_GPU (id, name)` is a reading of nsys's schema.
- **Fresh VM, what the fakes cannot show (new-hosts):** `useradd`, `usermod`, `groupadd` and
  `install -m 440 /dev/stdin` as root on Ubuntu; sudo for the new user with no tty; `docker.io`'s
  postinst with a `docker` group made first; bootstrap run over ssh with stdin at EOF; sshd taking
  a key login for a user whose password is locked, under `UsePAM no`;
  `claude auth login` over `ssh -t` at the pinned version; `gh auth login --with-token
  --insecure-storage` reading the PAT from ssh's stdin; a second developer reusing the shared sf1
  and cuDF checkout, git accepting a checkout another uid owns; the FFI build from the sparse
  `third_party/cudf`.
- **Benchmark (sf40-baseline, tp4-sized):** a case that kills the process (segfault, abort, OOM)
  writes nothing, and every rerun reaches it again. Skip it by name in the device's `cases.csv`
  with the reason.
- **Drift containers could not fix:** `scripts/build-test-shadgpu.sh:683` and `:702` say "a failing
  case appends its rows before it panics". Since containers' Task 9 a failed case appends none,
  and a case's rows go in before its section.
- **ghcr.io:** only an organization member can create a package; an outside collaborator's push is
  denied (`permission_denied: create_package`). A new package is private. Making it public needs
  the organization's setting to allow public packages, and is done in the package's settings (no
  REST endpoint). ghcr.io caps a layer at 10 GB. A push that dies is rerun, and the layers already
  up are skipped.
- **tpcds sf40:** the copy the H200 results were measured on is on shad-gpu. Its sha256s are not yet
  compared with the bucket's (build-test.md has the sum; containers' record has each file's).

## Task 5 notes

- Recount (developer, 2026-11-04): 6795 = Rust 5863, C++ 233, Python 699. cpu 3367 (`--list`,
  rust-only); ffi 7 and gpu-rung lib/corpus/timing carried from master (no test file in them
  changed count; cannot build those shapes here); `peacock_gpu_benchmarks` 638 = 8 + 630 from
  source; Python `scripts/env/tests` 155 (collected), calibration 41 (34 collected + `test_plot`'s
  7 from source, no matplotlib on dev); C++ 232 + `PeacockGpu.DeviceInfoDescribesTheDevice`.
- `architecture.md` already says twenty-six ABI symbols after the replay; the header declares 26.
- **#226 left open.** Containers never closed it (the human kept it open in containers' round
  1), and it still lists open work: the H200 record move, two hazards, the first run with
  provenance (sf40-baseline's, on the H100). Closing it would misstate the engine's state, so the
  spec's "closes #226" is not carried out here; named in the signoff.
- `docker/run-tests.sh:2` and `scripts/env/image.sh:2` still cite `llm-wiki/tasks/containers.md`
  (now archived). Left: both files are untouched by decision, and `run-tests.sh` is an image input.
- Master drift seen, not edited: `build-test.md`'s `test_gpu_corpus` paragraph says "987 cells
  today. From tpch, 220 … From tpcds, 410".

## Images

Task 6, developer, 2026-11-04, on `dev`, at `2b471221`. Each image pulled anonymously
(`DOCKER_CONFIG` an empty `mktemp -d`), one at a time, then removed. Its `peacockdb.inputs` label
was checked against this worktree with `image_inputs_differ … . <k> <v>`. The digest is the
manifest's; dev's containerd store gives the same value as the image ID. Pulled size is
`docker image ls`'s unpacked size; the compressed content is in brackets.

| image | repo digest / image ID | pulled size | label diff |
|---|---|---|---|
| `peacockdb-build:cudf-25.02` | `sha256:2a0c84414baaab94e6e085fc2ad98a6d48aec3ea025fdfcae5c4ddb91f6bb786` | 15.4 GB (5.19 GB) | empty |
| `peacockdb-build:cudf-26.02` | `sha256:146fcbaa2f4197f884416f1eaca60ff2c8afc2f975143180f4f50b585b938e38` | 15.1 GB (5.20 GB) | empty |
| `peacockdb-run:cudf-25.02` | `sha256:10bf83bb1ba18ce20b4a267f109ecaf906019f3a47f190d88d84fa50656e9158` | 8.32 GB (3.18 GB) | empty |
| `peacockdb-run:cudf-26.02` | `sha256:bc3ff3712f2c2f61f5a5705f47403f19e6e514131d4c454c6171cd7c07b3a1ad` | 10.2 GB (3.94 GB) | empty |

All four are under `ghcr.io/asymptote-tech/`. **No rebuild**: Step 3 does not apply. Each
`:cudf-<v>` tag resolves to the same digest as its `:cudf-<v>-b50d9b2a4ea0` tag. The build images'
labels carry `third_party/cudf=43505bb975f46ce4e140fdaf55192611d6e231ac` and
`cpp/CMakeLists.txt:flatbuffers=v25.12.19`, the same as the fork. The check can fail when it
should. With the run 26.02 label, a changed `pins.env` hash printed `docker/pins.env`. A label
missing `docker/run-tests.sh` printed that input. Checking the label against the 25.02 tree
printed both specs and `docker/pins.env`.
- PR #193 opened against master (2026-11-04); board `reviewing`.

## Review round 1 (2026-11-04)

1 blocking, 2 important, 7 nits (nits not taken).
- B1: `test_the_data_directory_is_machine_wide_by_default` (old `test_up.py:538`) went with
  `test_up.py`; `machine.sh:15`'s default is untested → developer, port into `test_bootstrap.py`.
- I2: `source_file` is a public path into the checkout the `no_core_test_reads_the_checkout`
  guard cannot see → developer: make it write-only (`rewrite_source_file(rel, text)`).
- I3: #226 names `scripts/exec_model/optimizer/call_cost.py` and `exec-model-corpus.yml`'s record
  fetch, which exist only on `ENS-optimizer` → coordinator edits the ticket.
- Round 1 fixed: B1, I2 (developer, 379a56d5); I3 (coordinator, #226 loses the optimizer-only call_cost.py and exec-model-corpus.yml references).

## Review round 2 (2026-11-04)

B1, I2, I3 resolved; no new findings. Board `completing`.

## Completeness pass (2026-11-04)

Reviewer (what is wrong): 0 blocking, 0 important.
Analyst (what is missing): 0 blocking, 3 important; `architecture.md` has no false sentence;
five `build-test.md` passages left false by the carried `f3db66b1` (its docs were in
ENS-optimizer's `c5d12789`, not carried).
- A1: no CI test reads the provenance on a device (`provenance::of_this_run`, Rust
  `device_info()` reached only from the skipped `bench_` cases) → developer: a ninth harness test
  in `peacock_gpu_benchmarks.rs`, run under `--skip bench_` by every gpu-tests job.
- A2: open items from containers' record, owners named here:
  - the CUDA mix in the 25.02 build image (compare with nebius-gpu's native build) → new-hosts,
    which first runs the run image on a device;
  - `nsys_hbm.py`'s device check on a real Nsight export → sf40-baseline (the H100's Nsight pass);
  - the `tpcds-sf40` bucket's sha256s against shad-gpu's copy → sf40-baseline (it reads tpcds
    sf40 from the bucket); shad-gpu is down, so it waits on shad-gpu;
  - `scripts/build-test-shadgpu.sh:683`, `:702` "a failing case appends its rows before it
    panics" → fixed here (developer).
- A3: Done-when 2 on the tip → the developer re-runs the rust-only gates and
  `scripts/env/tests` on the tip; the PR's CI run is recorded below.
- build-test.md: :780 (one file per device, dataset and mode), :1232 (tpcds.sf40 too), :1251–1255
  (`--metrics` per case, `hbm-parts`, `--parts`), :1257 and :2201–2204 (`plot.py` takes one
  `--record` per dataset) → developer, from `git show c5d12789:llm-wiki/build-test.md`'s wording
  where it fits the fork.
- Round 1 fix: `test_bootstrap.py` ran 17 passed after `379a56d5` (developer's report).
- Completeness fixes (developer, 2026-11-04):
  - A1: `this_runs_provenance_names_its_device_driver_cuda_cudf_and_host` in
    `peacock_gpu_benchmarks.rs`, through two delegates in `test_support/mod.rs`
    (`provenance_of_this_run`, `cudf_release`). Red then green as a compile in the 25.02 build
    image (`cargo test --no-run -p peacockdb-core --features gpu --tests`): E0432 before the
    delegates, clean after. 639 tests, 9 under `--skip bench_`. Never run on a device; with none
    it stops at `of_this_run`'s "the CUDA runtime describes no device".
  - A2: `build-test-shadgpu.sh:683`, `:702` now say a failed case appends none of its rows.
  - build-test.md: the five passages, from c5d12789's wording. `create_nsys_profile.sh` reads
    pinned records only at `calibration/<dataset>.sf<sf>/`, the H200's level; a new device's
    fetched records sit under `<device>/`, so step 5 does not find them there yet.
  - `scripts/env/tests/test_run_tests.py` runs `docker run` on a run image that is not there,
    which pulls it (25.02, 8.3 GB; 26.02 timed out at 120 s). Remove it afterwards.
- Device CI: `pipeline.yml` dispatched 2026-11-04T01:51:56Z on `2b89446c`, run 41756935974. Waiting for `GPU Tests (nebius-gpu)`.
- Run 41756935974 failed at "Start nebius-gpu": no capacity (exit 75) from 2026-11-04T02:17Z for 30 min. GPU capacity clock starts 2026-11-04T02:17Z (10 hours → 12:17Z). The coordinator retries `gpu-host.sh start` every 15 min and re-dispatches once the VM is up.
