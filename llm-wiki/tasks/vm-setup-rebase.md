# vm-setup on master, Sirius in a script of its own on the runner

Kind: production

vm-setup (PR #205, done) cannot merge. It branched from chain L's containers-only before chain L
was squashed and merged, and it and sirius-bench rebuilt `scripts/env/run-gpu.sh` in different
directions:

- **vm-setup** moved the VM's side into `scripts/env/run-gpu-vm.sh`. Every run, a staged binary's
  too, goes through a runner on the VM: one card lock for every user of the VM, `follow` and
  `cancel`, logs and records under a timestamp, `pull` judged by content stamps with `--clear`,
  the image run by its ID, and every card's driver checked.
- **sirius-bench**, now on master, added the `sirius` verb, `prepare --sirius`, the free-disk
  check, a driver check per image and the goldens directory for pull, all against the old script:
  a `docker run -d` of its own, outside any lock.

This task lands vm-setup on master. Sirius moves out of `run-gpu.sh` into a script of its own,
`scripts/env/run-sirius.sh`, whose runs go through vm-setup's runner. It replaces PR #205.
`ENS-vm-setup`, rk00's branch, is left as it is.

## Done when

1. **vm-setup's change, replayed onto master.** The first code commit on `ENS-vm-setup-rebase` is
   the three-way merge of vm-setup's tip `e0612b23` onto master, with `1eaefe39` as the base: the
   containers-only tip vm-setup branched from (`git merge-recursive 1eaefe39 -- HEAD e0612b23`).
   Both commits are reachable from `origin/ENS-vm-setup`. The replay conflicts in `run-gpu.sh`,
   `scripts/env/tests/gpu_host.py`, `test_pins.py`, `test_run_gpu.py`, `build-test.md`,
   `tickets.md`, `tickets/testinfra.md` and `tasks.md`. At the task's end, everything vm-setup
   changed survives, and so does everything master changed. vm-setup's spec, plan and detail file come with the replay.
   - **The replay is Roman's commit.** Its author is vm-setup's, `rk00
     <00romankolesnik@gmail.com>` (`git commit --author`). Its message names `e0612b23` and lists
     vm-setup's 35 commit subjects. It carries vm-setup's work and the conflict resolutions alone.
     Where Sirius and vm-setup collide in `run-gpu.sh` and its tests, it takes vm-setup's side.
     Sirius comes back in the next commits, under item 2.
   - Everything this task adds is in later commits of its own, never amended into Roman's.
   - Before the merge, the branch is squashed to two commits: Roman's replay, his authorship kept,
     then the port.
2. **Sirius has its own script, `scripts/env/run-sirius.sh`.** `run-gpu.sh` loses the `sirius`
   verb, `prepare --sirius` and every Sirius line. It keeps vm-setup's verbs, which serve any run
   on a dir, Sirius's too: `status`, `follow`, `cancel` and `pull`.
   - `run-sirius.sh` takes `run-gpu.sh`'s VM options (`--instance-id`, `--profile`, `--dir`) and
     two verbs. `prepare` is today's `prepare --sirius`, after `run-gpu.sh prepare`. `run
     [--detach] -- <docker/sirius-bench.py arguments>` is today's `sirius`, with its argument
     checks.
   - The code both scripts need moves into a library that both source: the VM calls and syncs,
     the dir's claim, the stamps, the image read and the card check. It is not copied. The VM's
     side of the runner stays in `run-gpu-vm.sh`. If Sirius needs anything there, it is a
     generic hook, never a Sirius branch.
3. **A Sirius run goes through the runner.** It is a runner kind, `sirius`.
   - It takes the card lock, so it waits for another run on the card, and another run waits for
     it. Its log is `logs/sirius-<stamp>.log`.
   - Its exit stays as today: 0 or 2 as the runner's, else 1. The staged-binary judgment
     (segfault, `running 0 tests`) does not apply to it.
   - The container keeps today's flags: the shipped seccomp profile, memlock unlimited, the VM's
     sf40 data read-only at its own path as `--data`, the shipped testdata as `--out`, and the
     VM's hostname.
   - A dir never shipped can still run Sirius, as today. A mount of a missing install must not
     leave a root-owned directory behind.
4. **The Sirius image is checked the way the run image is.** Its ID and labels are read in one
   call, and the run gets the ID, so a tag moved in between is never run. Its inputs label is held
   to this tree. Every card is held to the image's `peacockdb.driver-minimum` and to compute
   capability 8.0 or later; a refusal exits 3. The Sirius image has no
   `peacockdb.driver-minimum-ptx` label, so the PTX rule does not apply to it. `prepare` keeps
   its free-disk check before the pull, then checks the cards against the Sirius image.
5. **A Sirius run's files come home through the stamps.** `run` stamps the files it sends with
   this tree's sha256: the query directories, the H100 results tree, and, for `--list` or
   `--resume`, the answers database. `--update` stays: the VM keeps a file newer than this tree's
   copy. `run-gpu.sh pull` then brings home what the run changed. It names a conflict only for a
   file changed on both sides. `pull --clear` clears a Sirius run's log as it clears any run's.
6. **`bench-sirius.sh` calls `run-sirius.sh run`, then `run-gpu.sh pull`.** Its own options and
   exit codes stay as they are, and `test_bench_sirius.py` follows.
7. **Tests.**
   - Master's Sirius cases in `test_run_gpu.py` move to a `test_run_sirius.py` on vm-setup's fake
     VM.
   - New cases cover:
     - a Sirius run waiting on the card lock for a running run, and the reverse;
     - `cancel` and `follow` of a Sirius run;
     - a pull of a Sirius run's results and answers database through the stamps, and a conflict;
     - the Sirius image run by ID when its tag moves;
     - a card below the Sirius minimum, refused with exit 3;
     - `run-gpu.sh` refusing `sirius` and `--sirius` as unknown.
   - Every Python test passes under `scripts/env/tests/`, `scripts/nebius/tests/` and
     `scripts/calibration/tests/`. The PR's CPU CI gates are green.
8. **Docs agree.** The usage of both scripts and `build-test.md` ("Sirius on the H100", and the
   run-gpu sections vm-setup changed) say where Sirius now lives. `build-test.md`'s test counts
   are recounted once on the result. The tickets index holds vm-setup's tickets and #313
   (`duckdb_minimal.sh` fix) beside master's #311 (Sirius planning time).
9. **The proof is offline.** No VM is started. Three things stay deferred, recorded in the
   detail file:
   - a Sirius run on the H100 through the runner;
   - vm-setup's by-hand nebius-gpu items;
   - CI's `gpu-tests` through the runner, which vm-setup left unproven when the L40S had no
     capacity.
10. **A new PR against master supersedes #205.** It names #205, and its text lists the deferred
    items. #205 is closed when the new PR merges.

## Approaches

- **Sirius in a script of its own, on the shared runner** (picked; the human wants Sirius out of
  `run-gpu.sh`). The card lock then covers Sirius, and `follow`, `cancel` and the stamps serve it
  for free. `run-gpu.sh` stays about peacockdb.
- **Sirius as a verb of `run-gpu.sh`, in the runner.** Rejected by the human: Sirius logic stays
  out of `run-gpu.sh`.
- **Sirius on its own `docker run`, outside the runner.** Rejected: a Sirius run would skip the
  card lock and share the card with a peacockdb run, the failure vm-setup's lock exists to
  prevent.
- **Rebase vm-setup's 35 commits one by one.** Rejected: nearly every commit edits
  `build-test.md`'s test counts, which sirius-bench also changed, so each would conflict again.
  One three-way replay resolves each conflict once. The branch is squashed before the merge
  anyway.

## Scope

| path | change |
|---|---|
| everything vm-setup changed (its spec's Scope) | replayed from `e0612b23` |
| `scripts/env/run-sirius.sh` (new), a shared library under `scripts/lib/` (new) | items 2-5 |
| `scripts/env/run-gpu.sh`, `scripts/env/run-gpu-vm.sh` | Sirius out; the shared code moved into the library; a generic runner hook if needed |
| `scripts/env/bench-sirius.sh` | item 6 |
| `scripts/env/tests/` (`gpu_host.py`, `test_run_gpu*.py`, `test_run_sirius.py` (new), `test_pins.py`, `test_bench_sirius.py`) | the merge, and item 7 |
| `llm-wiki/build-test.md`, `llm-wiki/tickets.md`, `llm-wiki/tickets/testinfra.md` | item 8 |

Component-level API: none in the engine. On the command line, `run-gpu.sh sirius` and `run-gpu.sh
prepare --sirius` become `run-sirius.sh run` and `run-sirius.sh prepare`.
## Constraints

- No engine change, and no test is weakened.
- The Sirius image's hashed inputs are not edited: `docker/sirius.Dockerfile`,
  `sirius.Dockerfile.dockerignore`, `sirius-venv.lock`, `sirius-fetch.sh`, `sirius-config.yaml`,
  `sirius-bench.py` and `sirius-LICENSE`. An edit there changes the image's tag and leaves the
  published image stale. `image_inputs` for `sirius` must give the same value before and after.
- No VM is started, and no workflow is dispatched.
- vm-setup's own constraints hold: never open a Nebius credential file, and secrets travel on
  ssh's stdin alone.
- `ENS-vm-setup` is never pushed to.
