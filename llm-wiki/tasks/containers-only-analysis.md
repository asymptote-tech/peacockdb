# containers-only — completeness analysis (analyst)

Branch `ENS-containers-only` at `fda7c4e8`, read as one change against `ENS-tp4-sized`
(74 files). Spec, plan and detail file read in full. Image-free evidence only: no build, no image,
no VM. CI logs read: runs 42445243365 (round 2 PR run) and 42440753811 (the last device run).

## 1. What is missing (blocking: none; important: six)

### 1.1 `ticket_widget.py` is still a host cargo build (spec items 3 and 6)

`scripts/ticket_widget.py:607` runs `cargo run -p cost-report` on the host, into `target/`. The
wiki calls it stdlib-only and host-runnable (build-test.md:2017, :2059), and prompts.md:45 tells
every developer to run `python3 scripts/ticket_widget.py` on each registry change, "needs nothing
else". A CPU VM gets no host cargo from `bootstrap-vm.sh`, so the command fails there. On dev it
works only because the retired host toolchain is still installed. The detail file's "Wiki commands
run" counts this host run as run on the image path.

The same shape: `scripts/exec_model/optimizer/dphyp.py:17`'s refusal names
`CARGO_TARGET_DIR=/build/peacock/rust-only-target cargo build --release -p peacockdb-dphyp`, a host
build into a shared target dir outside the tree, which build-test.sh would refuse. The README's
DPhyp section says the refusal names the image command.

Fix: the three wiki lines say `scripts/build-test.sh -- python3 scripts/ticket_widget.py`, and
`ticket_widget.py` drops off the stdlib list. dphyp.py's `BUILD` string becomes
`scripts/build-test.sh -- cargo build --release -p peacockdb-dphyp` (code: the developer).

### 1.2 Post-merge hand-offs are recorded only in the detail file, which archiving deletes

None of these is in the spec, a ticket or build-test.md as a step. Each belongs in the signoff:

- **images.yml on master.** `:cudf-25.10` does not exist, and every `:cudf-<v>` still names
  master's pre-merge inputs. CI does not need them: it finds images by the inputs tag. The wiki's
  examples (`docker pull …:cudf-25.02`, the run-image `docker run`) and "`:cudf-<v>` is the latest
  published from master" do. Dispatch `gh workflow run images.yml -f kind=all -f cudf=all` on master
  after the merge. Wait for every open branch that reads `:cudf-<v>` to rebase first: before
  round 1's fix 7, run-gpu.sh, check-build-image.sh and bootstrap-vm.sh label-checked the moving
  tag, so moving it refuses them. The dispatch is also needed at once if master's image inputs at
  the merge differ from the branch's: another merge touching `docker/`, the flatbuffers tag in
  `cpp/CMakeLists.txt` or `third_party/cudf`. Otherwise master's CI fails at "Find the build
  image". Today they match.
- **nebius-gpu's old state.** Remove these once no branch's CI runs `nebius-test.sh`: all of chain
  L merged, and chain J's override rewritten. `~/peacockdb-ci/` (0.75 GB), `~/miniforge3` (17 GB),
  `~/peacock-env.sh`, and this chain's stale `~/peacockdb-peacockdb-beta/cpp/install` (4.9 GB, the
  old conda install). build-test.md "Disk" names `~/peacockdb-ci/`, conditionally. It leaves the
  envs as "the human's call" without naming them.
- **Other chains' plans and the board.** `tasks.md:12-17`, chain J's host override, routes the L40S
  through `nebius-sync.sh` and `nebius-test.sh`. `verify-26.02-impl.md` (chain J, on hold) builds
  with `scripts/cargo-cudf.sh`, `cpp/build26` and conda envs. Both go stale at the merge. The board
  prose is the helper's.
- **Dev worktrees.** In this worktree, `target/` (11 GB) and `target-cudf-rapids-cuda-12.2` (25 GB)
  are dead weight. dev has 23 GB free. Each worktree's host `cpp/build` and `cpp/install` must go
  before its first `build-test.sh --build`, which refuses an unstamped configure. The build image
  takes 15 GB in `docker images`. Keep `target-cudf*` only where shad-gpu builds.

### 1.3 No ticket: nebius-gpu's disk fills with images and sf1 sets

CI's `prepare` pulls a new build and run image pair, about 15 GB, for each change of image inputs.
It generates a new sf1 set, about 0.8 GB, for each change of generator inputs. Nothing prunes
either, and the VM is 96 GB, shared by the chains. About 17 GB was free at the last reading
(Task 9's stop). #309's own fix changes the build image's inputs. After about two such changes,
`gpu-tests` goes red at `prepare` on a full disk, and the wiki's 10 GB rule binds developers, not
CI. File it in `testinfra.md`, with the remedy: prune images whose label matches no tree's inputs,
or let `prepare` remove the VM's older images of the same kind and version.

### 1.4 verda and verda-gpu: unproven, and unprovisioned

verda was down for the whole task, so `build-test.sh` never ran there. Nothing says how verda gets
Docker now that `init.sh` and `create_verda_instance.sh` are gone. `bootstrap-vm.sh` could, but the
wiki does not say so. verda-gpu also needs the NVIDIA Container Toolkit. Its 2gpu tests
(build-test.md:15, rows 1258-1260, "Wall-time C++ suites") have a build recipe on the image path
(26.02, proven on nebius-cpu128) and no run recipe: `run-gpu.sh` reaches Nebius VMs only.
prompts.md:190-196 still routes a dispatch's CPU tests to verda. Spec item 4 allowed the scripts to
be "reduced to installing Docker and the NVIDIA toolkit where `bootstrap-vm.sh` / `run-gpu.sh
prepare` do not already". verda-gpu is such a place. At least one wiki line is owed: how verda
gets Docker, and how verda-gpu runs the 2gpu binaries (the 26.02 run image with
`--entrypoint`, as "A binary run by hand" says).

### 1.5 The new 25.10 build image has no any-user test, and no by-hand image test ran

`test_image_any_user.py` checks the 25.02 and 26.02 build images only: a uid with no passwd entry,
`--network none`, offline configure, sf1 generation. The branch added 25.10 without a case.
Task 1 changed `docker/build.Dockerfile`, so all three build images were republished. Neither
`test_image_any_user.py` nor `test_run_image.py` was run on them, by the detail file's record. CI
exercises 25.02 and 25.10 as uid 1001 with the network on, which is partial evidence. Add
`test_the_25_10_image_runs_as_any_user`, and run both files once on the published images.

### 1.6 Spec constraint: two files fail in the image, and no ticket names them

"A test that fails in the image and passed outside it is a finding: fix the image or ticket it."
`test_bootstrap.py` and `test_provision.py` run on the runner because the runner's uid has no
passwd entry in the image (`id -un`). #309 names only the locale pair. Name them in #309, or in a
ticket of their own. The same gap hits any developer whose uid is not 1000.

### Lower, noted only

- On a `bootstrap-vm.sh` clone, `build-test.sh --build` (cpu, gpu) needs `third_party/cudf`
  checked out, and `--run` needs sf1 inside the tree. The clone has neither: the machine's copies
  under `/var/lib/peacock` are outside build-test.sh's mount. build-test.md:2008's "or a CPU VM
  alike" needs that line.
- `test_run_tests.py` runs only in `images.yml`, so an edit to that test file alone runs in no CI.
- CI's testdata on the VM is no longer mirrored (ci-ship's delete case was dropped on purpose). A
  file deleted from the tree stays in `~/peacock/<i>/testdata`.

## 2. Falsified sentences

### architecture.md

None. It names no build path, host, image or CI job. Its build-test.md cross-references
("Fused batches", the multi-GPU note, the exec_model pointer) still resolve.

### build-test.md

| line | sentence (quoted) | correction |
|---|---|---|
| 7 | "**Grand total: 8245 test cases — Rust 6319, C++ 286, Python 1640.**" (and "The header is the sum of the N columns") | The tables sum to **8268: Rust 6323, C++ 286, Python 1659**. Round 1 and 2 moved CI wiring to 34 and Images to 335 after the header was last set. |
| 1248 | "With the images built: each image as a uid it does not know with no network" | "the 25.02 and 26.02 build images as a uid…", or add the 25.10 case (1.5) |
| 1362-1363 | "in CI the runner's own with the `S3_*` secrets" | "in CI the build image's, the `S3_*` secrets passed by name (exec-model-corpus.yml's call-cost job)" |
| 2017 | cost-report row: "checked-in report: `python3 scripts/ticket_widget.py` → `testdata/report.html`" | `scripts/build-test.sh -- python3 scripts/ticket_widget.py`: it runs `cargo run -p cost-report` |
| 2058-2060 | "A script of the standard library alone runs with the host's `python3` as well: `ticket_widget.py`, …" | drop `ticket_widget.py`: it shells out to cargo |
| 1556 | "until a full `python3 scripts/exec_model/run.py` writes them again" | `scripts/build-test.sh --pass-env PEACOCK_DPHYP_LIB -- python3 scripts/exec_model/run.py` |

Pre-existing drift, not this branch's: build-test.md:2816 "A standard runner has 14 GB", and the
build-image action's comment, say the same. Run 42445243365's 25.10 leg logged `/dev/root 145G`,
86 GB available before freeing and 117 GB after. "Free the disk" costs about 3 minutes per job.

### prompts.md

| line | sentence | correction |
|---|---|---|
| 45 | "`python3 scripts/ticket_widget.py`, which writes it and needs nothing else" | `scripts/build-test.sh -- python3 scripts/ticket_widget.py` |
| 402-403 | "If a host is lost mid-step: `status`, `start` (…), sync, rerun the step." | no sync step remains on the L40S or the H100: "`status`, `start` (…), `ship` again, rerun the step" (build-test.md:2237 already dropped "sync") |

### Elsewhere

- `llm-wiki/tasks/tasks.md:12-17`, chain J's override: "nebius-gpu through `scripts/nebius.sh`,
  `nebius-sync.sh` and `nebius-test.sh`". The L40S is `run-gpu.sh` now (the helper's to rewrite).
- `llm-wiki/tickets/benchmarks.md:33` (#226): "both scripts' `--pull-benchmarks` rsync it
  additively". Only `build-test-shadgpu.sh` has the flag now.
- `llm-wiki/tickets/system-hardening.md:174-187` (#244, open, blocks verify-26.02): "The build is
  `scripts/build-test.sh --gpu --build`" and the reproduce recipe over `cpp/build26/install` and a
  conda env. The flag now builds in the image into `cpp/install`, and `cpp/build26` is no longer
  made. Rewrite the recipe for the 26.02 images, or mark it as the pre-image record.
- `llm-wiki/tasks/verify-26.02-impl.md` (11-25, 104-157, 205-289): `cargo-cudf.sh`, `cpp/build26`
  and "build-test.sh's verda build". `verify-26.02.md:6`'s `#t129` link dangles (known).
- `scripts/exec_model/README.md`, DPhyp: "every file that calls DPhyp refuses to run without it,
  naming the build:" followed by the image command. The refusal names a host build (1.1).
- Code comments:
  - `scripts/build-test.sh:215-216`: "Kept in step with build-test-shadgpu.sh's RUST_TESTS and
    pipeline.yml's gpu-tests staging array". The guard reads `stage-install.sh`'s `GPU_TESTS` now
    (runners.rs).
  - `testdata/gen_reconstruction_report.py:14-15`, which this branch wrote: "nothing ships them to
    a test host". `run-gpu.sh ship` sends every committed `testdata/` file, `testdata/reports/`
    included (run-gpu.sh:464).
  - Pre-existing: `docker/run-tests.sh:7` "by scripts/gpu-host-test.sh's rules" (a run-image
    input, frozen for this task), and `scripts/lib/image-inputs.sh:7`'s `up.sh`, which does not
    exist.

## 3. Shortcuts, waivers and what was not run

1. **#309**: `test_fetch_sf40.py` and `test_image.py` run on the runner's python, because the image
   has no `en_US.UTF-8`.
2. **`test_bootstrap.py` and `test_provision.py` run on the runner**: no passwd entry for the
   runner's uid. No ticket names them (1.6).
3. `shellcheck` stays on the runner, as a linter of the scripts. Justified in the step.
4. **The by-hand image tests were not run on this branch's images**: `test_image_any_user.py`,
   with no 25.10 case, and `test_run_image.py`. `test_run_tests.py` runs only on an `images.yml`
   publication.
5. **The `call-cost` job has never run in CI** in the image. Only a no-filter dispatch runs it.
   It was proven runner-shaped on dev.
6. **`validate-large.yml` was never dispatched.** Its ssh half needs shad-gpu, which is down. Its
   S3 step was proven on dev only.
7. **verda: nothing run**, since it was down (1.4).
8. **Multi-GPU** was built in the 26.02 image on nebius-cpu128. It was never run on two GPUs, and
   no recipe exists for running it.
9. Spec item 6, wiki commands not run on the image path: `test_stats_embeddings.py`, the README's
   `fetch_record.sh` and `test_call_cost_measured.py`, the whole-corpus `run.py --jobs 16`, the
   sf40 `duckdb_result.py --answers`, the Images section's run-image commands (`docker run --gpus`,
   the driver-label inspect), `image.sh push` and `docker login` by hand, `create_nsys_profile.sh
   --metrics`, `publish_record.sh`, `join_records.py` and `split_record.py`. `ticket_widget.py` was
   run as a host cargo build (1.1).
10. The H100's `bench-each.sh`, Nsight (`--trace` only), the fused batch and `run bench` ran one
    case each, at round 1's finding 8.
11. **The last device run, 42440753811 at `f4515fb4`, predates `ff9dbf5b` and `fda7c4e8`.** Those
    changed the pull path `gpu-tests`' `prepare` uses (`machine.sh`'s `machine_image_checked`,
    `run-gpu.sh`). The signoff should cite the `completeness approved` dispatch, not that run.
12. CI's VM testdata is additive, not mirrored: a dropped ci-ship case.
13. Counts were held against a stand-in baseline: the parent's last full workflow_dispatch,
    because its last PR run was documentation-only.
