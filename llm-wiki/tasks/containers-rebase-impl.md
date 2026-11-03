# containers-rebase implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** branch `ENS-containers-master` on origin: today's master, then `02f6a4d8` carried from
`ENS-optimizer`, then containers' 16 commits replayed, each keeping rk00's authorship and date;
case list regenerated from master's registry; the H200's case list converted once to
`h200/cases.csv`, the per-device list format every chain L task reads; the agent-container
launcher and the embeddings' S3 path removed; green on the fork's gates; the four images built
from the fork by `images.yml` and pulled anonymously.

**Architecture:** git history surgery, no new behaviour. One cherry-pick (`-x`) and one
`rebase --empty=keep`; every conflict resolved by hand, amended into the commit it belongs to.
Work that is new (the launcher's and the S3 path's removal, regenerated case list, the CSV list
and its reader, doc recounts, hash references, detail file) goes in separate commits under the
developer's identity, after the 17 replayed ones.

**Tech stack:** git ≥ 2.34 (dev's), cargo, the containers build image (`docker/`,
`scripts/env/image.sh`), GitHub Actions `workflow_dispatch`, ghcr.io.

**Spec:** [`containers-rebase.md`](containers-rebase.md). Containers' own plan, for what each
replayed piece is: `git show origin/ENS-containers:llm-wiki/tasks/containers-impl.md`.

## Global constraints

- Work in the chain's own worktree on host `dev`; its own `target/`. The build-image runs use
  `CARGO_TARGET_DIR=$PWD/target-img-<v>` inside that worktree. **Never** a cargo target dir shared
  with another worktree (build scripts bake `CARGO_MANIFEST_DIR`).
- The primary checkout `/media/data/peacockdb` stays on master; nothing here checks a branch out there.
- Nothing may use `--reset-author`, `--ignore-date`, `commit --amend --reset-author`, a squash or
  `rebase --root`. `rerere.enabled=false` for the whole task (a recorded blind resolution from the
  trial must not replay).
- Carried from `ENS-optimizer`: `02f6a4d8` only. Never `69448594` (peacockdb-dphyp), `2bef3b81`
  (cost), `3625404b` (exec_model optimizer) or later optimizer commits.
- Containers' prose is re-applied to master's text, never master's text replaced by containers'
  file. Case counts in `build-test.md` are recounted, not merged.
- The H200 files are not re-measured. No device run in this task.
- Removed after the replay, in developer commits only: `up.sh`, `down.sh`, `scripts/lib/board.sh`,
  `test_up.py`, `test_up_branch.py`, `upload-embeddings.sh`, `embeddings-bucket.sh`, `--from-s3`.
  `scripts/lib/machine.sh` stays. Untouched: `docker/`, `image.sh`, `run-tests.sh`,
  `image-inputs.sh`, `images.yml`.
- PR 192 and `ENS-containers` are left untouched; the fork is a new branch and a new PR.
- Never open `~/.nebius/*` or any `*.pem`.

## Review focus

1. **A resolution that takes containers' whole file imports optimizer prose.** The trial
   resolved `build-test.md` with `--theirs` and brought 12 lines naming DPhyp/join-optimizer into
   the fork. → Task 1 Step 9: the added-lines grep is empty.
2. **`8437d6cd` ("containers: building") becomes empty** once the board is master's, and a
   default rebase drops it silently. → Task 1 Step 4 keeps it with `--allow-empty -C`; Step 8 counts
   17 rk00 commits.
3. **The case list hand-merged** instead of written from master's registry: it passes no check
   until someone regenerates and gets a diff. → Task 3 Step 2: a second regeneration is a no-op.
4. **The H200 tree touched by the move**, a merge picking master's flat copy over the newer one.
   → Task 3 Step 3: `git diff origin/ENS-containers ENS-containers-master --
   testdata/benchmark-results` is empty; after Task 4 it names only `h200/cases.inc` (deleted)
   and `h200/cases.csv` (added).
5. **An image published from another commit than the fork's tip, or not public.** → Task 6
   Steps 1–2: the `:cudf-<v>` images pull with an empty
   `DOCKER_CONFIG`.
6. **A converted H200 list that silently drops or reorders a case** — a sed that misses one
   line leaves `h200/` held to 13 cases and green. → Task 4 Step 5: the CSV's rows equal the old
   file's 14 invocations, in order; Step 6 reads them through `parse_device_cases`.
7. **A skip reason holding a comma** (an error message) splits into extra columns. → Task 4,
   `a_note_may_hold_commas`: the note is everything after the fifth comma.
8. **A removed refusal that still protected something**, or a provision rerun that no longer
   bootstraps once the marker is gone. → Task 2's decisions table, recorded per refusal;
   `test_provision_runs_bootstrap_on_every_run`, `test_provision_refuses_a_ref_the_vm_clone_is_not_on`.
9. **The origin fetch broken by the S3 removal**, its pins read through the deleted library.
   → Task 2 Step 1: `test_origin_*` and `test_the_committed_pin_names_each_artifact_once` stay
   and pass.

---

### Task 1: the fork — carry `02f6a4d8`, replay containers

**Files:** history only. Conflicted files, from a trial run on 2026-11-03 (resolutions there
were blind; expect these, possibly a few more on `build-test.md`):

| commit | file(s) | resolution |
|---|---|---|
| `02f6a4d8` (cherry-pick) | `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` (2 hunks: the `use` list at :9, the tests after `every_timed_case_is_enabled_on_a_device`) | keep master's `TimedDeclaration`/`LineDeclaration` inventories and its mask test; add 02f6a4d8's imports (`RECORD_PATH_ENV`, `append_records`, `macro_invocations`) and its tests |
| `462ed26b` spec | `llm-wiki/tasks/tasks.md` | HEAD's (master's board, chain L); containers' spec/impl/detail files come as they are |
| `8437d6cd` building | `llm-wiki/tasks/tasks.md` | HEAD's; commit becomes empty, kept |
| `f75cba7e` images' docs | `llm-wiki/build-test.md` | containers' "Images" section and `images.yml` CI line onto master's text |
| `199bde5f` embeddings | `scripts/exec_model/README.md` | the one-line change ("which no CI job fetches") onto master's line |
| `d2a0f88c` provenance | `peacockdb-core/src/test_support/device_answer.rs` | master's (`8f054f5c`, `272819bd`) changes plus containers' provenance |

Auto-merged in the trial and to be read once: `.github/workflows/pipeline.yml` (02f6a4d8's
comment hunk at :384, far from dphyp's at :693), `scripts/build-test-shadgpu.sh`,
`scripts/gpu-host-test.sh`, `scripts/build-test.sh`, `test_corpus_goldens.rs`,
`corpus_benchmark_cases.inc` (regenerated in Task 3 anyway).

**Why only `02f6a4d8`** (for the detail file): it is the second commit on `ENS-optimizer`; its
parent `69448594` shares one file with it, `pipeline.yml`, in a separate hunk. `2bef3b81` and
`3625404b` come after it, so it cannot use them. Of containers' files only
`scripts/calibration/tests/harness.py` was touched by `3625404b`, comment text only, and no
containers commit edits it. Containers' commits add no `dphyp`/optimizer reference outside their
own wiki records. The trial cherry-pick stopped once (above) and the replay five times.

**Interfaces:**
- Produces: local branch `ENS-containers-master` = `origin/master` + carried `02f6a4d8` + 16
  replayed commits, in containers' order; `carried` = the new hash of `02f6a4d8` (named in later
  tasks and the detail file).

- [ ] **Step 1: preconditions.** `git fetch origin`; `git cat-file -e
  origin/master:llm-wiki/tasks/containers-rebase.md` succeeds (the chain L specs and board are on
  master; if not, stop: `blocked(building)`, the human commits them). `git rev-list --count
  611b5dcd..origin/ENS-containers` = 16; `git merge-base origin/ENS-optimizer origin/ENS-containers`
  = `611b5dcd…`. Record both tips' hashes in the detail file.
- [ ] **Step 2: workspace.** In the chain workspace: `git config rerere.enabled false`;
  `git switch -c ENS-containers-master origin/master`.
- [ ] **Step 3: carry.** `git cherry-pick -x 02f6a4d8`; resolve `benchmark.rs` as the table says;
  `cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens --no-run` compiles;
  `git add` and `git cherry-pick --continue`. Tag it locally: `git tag fork-carried HEAD`.
- [ ] **Step 4: replay.** `git branch -f containers-replay origin/ENS-containers`;
  `git rebase --empty=keep --onto fork-carried 611b5dcd containers-replay`. At each stop:
  - `llm-wiki/tasks/tasks.md`: `git checkout --ours -- llm-wiki/tasks/tasks.md` (in a rebase,
    "ours" is the new base: master's board).
  - Other files: by hand, per the table. A Rust file resolved → `cargo check --features rust-only
    -p peacockdb-core --tests` before continuing.
  - If `git diff --cached --quiet` after `git add` (8437d6cd): `git commit --allow-empty -C
    REBASE_HEAD`, then `git rebase --continue`. `-C` keeps author, date and message.
  - Note each stop (commit, files, how resolved) for the detail file as you go.
- [ ] **Step 5:** `git branch -f ENS-containers-master containers-replay`;
  `git switch ENS-containers-master`; `git branch -D containers-replay`.
- [ ] **Step 6: a resolution found wrong later** (here or in Tasks 2-4) belongs in the commit it
  resolved, not in a fix-up commit, and `rebase -i` is unavailable: re-run Steps 2–5 from
  scratch (`git switch -C ENS-containers-master origin/master`) with the corrected resolution.
  Cheap: six stops. Before Task 1 is pushed only; after the push, a wrong resolution is fixed in
  a developer commit and named in the detail file.
- [ ] **Step 7: compile in the fork's rust-only shape.** `cargo test --features rust-only -p
  peacockdb-core --no-run` and `cargo test -p cost-report --no-run`. Expected: both finish.
- [ ] **Step 8: authorship check (Done-when 1).** Expected: no output from `diff`, and `17`.

```bash
old() { git log -1 --format='%s|%an|%ae|%ad' 02f6a4d8
        git log --reverse --format='%s|%an|%ae|%ad' 611b5dcd..origin/ENS-containers; }
new() { git log --reverse --author='00romankolesnik@gmail.com' --format='%s|%an|%ae|%ad' \
          origin/master..ENS-containers-master; }
diff <(old) <(new) && new | wc -l
# the mapping for the detail file, old → new, matched by subject:
join -t'|' <({ git log --format='%s|%h %an <%ae> %ad' 611b5dcd..origin/ENS-containers
                git log -1 --format='%s|%h %an <%ae> %ad' 02f6a4d8; } | sort) \
           <(git log --author='00romankolesnik@gmail.com' --format='%s|%h %an <%ae> %ad' \
               origin/master..ENS-containers-master | sort)
```
  (`join` prints 17 lines, subject|old|new; the two `%an <%ae> %ad` fields of each line are
  equal. The developer's own commits from Task 2 on are by `dmitry`, so the `--author` filter
  leaves them out.)
- [ ] **Step 9: no optimizer (Done-when 3).** Each prints nothing:

```bash
git diff --name-only origin/master ENS-containers-master | grep -E 'peacockdb-dphyp|join-optimizer|exec_model/(optimizer|plans/|run\.py)'
git diff --name-only origin/master ENS-containers-master -- scripts/exec_model Cargo.toml Cargo.lock | grep -v '^scripts/exec_model/README.md$'
git diff origin/master ENS-containers-master -- . ':!llm-wiki/tasks/containers*' | grep '^+' | grep -i -E 'dphyp|join.optimi|optimizer\.txt'
```
- [ ] **Step 10:** push: `git push -u origin ENS-containers-master` (a new branch; no force).

### Task 2: drop the launcher and the embeddings' S3 path (spec item 8)

Developer commits on top of Task 1's push, never amended into rk00's. Paths and lines are
containers' (`git show origin/ENS-containers:<path>`).

**Files:**
- Delete: `scripts/env/up.sh`, `scripts/env/down.sh`, `scripts/lib/board.sh`,
  `scripts/env/tests/test_up.py`, `scripts/env/tests/test_up_branch.py`
- Delete: `scripts/env/upload-embeddings.sh`, `scripts/lib/embeddings-bucket.sh`
- Modify: `scripts/env/tests/laptop.py` — `TREE` (:22-23) loses `up.sh`, `down.sh`, `board.sh`;
  `STUB_BOOTSTRAP` (:100-109) writes no `~/.config/peacock/machine` and, like the real one,
  exits 2 naming both branches when `--ref` differs from an existing clone's branch
- Modify: `scripts/env/tests/machine.py` — `TREE` (:26-27) as above; what only `test_up*.py`
  used goes: `BOARD`, `push_branch`, `up`, `down`, `instance_runs`, `other_user`/`OtherUser`,
  `flag_values`, `mounts`, `command`, and `GH_HELPER`/`IMAGE_PATH`/`REGISTRY` unless a kept test
  names them; the module docstring drops the board
- Modify: `scripts/env/bootstrap-vm.sh` — header (:2, :11), the marker (:135-136), the Claude
  Code block (:89-94), the root refusal's reason (:35), `safe_git` (:36-37), the closing message
  (:138-148)
- Modify: `scripts/env/provision-vm.sh` — header (:9-11); the GPU key is taken, not made: `--gpu-key <file>` (default `$PEACOCK_GPU_KEY`), the `ssh-keygen` at :66, :113-118 removed, a missing key refused naming `--gpu-key`; :158-161 copy the named key,
  the marker test (:141-151), the default-VM line (:173), the Claude login (:192-195)
- Modify: `scripts/lib/peacock-config.sh` — keeps `PEACOCK_GPU_KEY`; drops `PEACOCK_MARKER`,
  `PEACOCK_CLONE_IN_HOME`, `peacock_vm_target`, `peacock_forward`; header (:1-6)
- Modify: `scripts/lib/machine.sh` — header (:3-9), `MACHINE_INSTANCES` (:19), `machine_git`
  (:23-25, every call becomes `git`), `machine_cargo_fetch`'s comment (:83-86)
- Modify: `testdata/fetch_embeddings.sh` — no `--from-s3`, `SOURCE` or `s3_args`;
  `embeddings_pinned_sha` defined in it; the CI guard refuses `CI`, `GITHUB_ACTIONS` and
  `PEACOCK_NO_FETCH` alike, as on master
- Modify: `scripts/env/tests/test_embeddings.py`, `scripts/env/tests/test_s3.py`,
  `scripts/env/tests/test_provision.py`, `scripts/env/tests/test_bootstrap.py`
- Modify: `scripts/exec_model/README.md` — 199bde5f's line back to master's ("which no CI host
  may fetch")
- Modify: `llm-wiki/build-test.md` (containers' text) — "Datasets" (:872, the embeddings-bucket
  bullet :916-924), the "Images (Python)" tier row (:829: the S3, `up.sh`, `down.sh` and
  forwarding clauses and the `test_up*.py` links go), "Containers" (:1898-2118)
- Untouched, by the human's decision: `docker/`, `scripts/env/image.sh`, `docker/run-tests.sh`,
  `scripts/lib/image-inputs.sh` (its comment :7-8 still names `up.sh`; noted in the detail file),
  `.github/workflows/images.yml`, `testdata/embeddings.sha256`

**Decisions on the many-developer refusals** (the detail file's `## Refusals read`, one row
each: script, line, what it refuses, kept/dropped, why). The developer confirms each against the
code and records any the table misses:

| where | refusal or guard | decision | why |
|---|---|---|---|
| `provision-vm.sh` `--user`, `useradd`, sudoers when user ≠ login | a developer's own Linux user | keep | separates each developer's PAT, git identity and gh login on a shared VM |
| `provision-vm.sh` git identity check | no `user.name`/`user.email` | keep | the VM's commits carry it |
| `peacock_env_is_storage_only`, `peacock_env_has_storage` | a token or a missing key in `peacock.env` | keep | the file is passed between developers; bootstrap and fetch-sf40 read the keys |
| `provision-vm.sh` PAT check | not `github_pat_…`, or cannot read the repo | keep | the PAT sits on a shared VM; scoped to one repo |
| `provision-vm.sh` `test -e .config/peacock/machine` and its hooks-free `branch --show-current` | skips bootstrap; refuses `--ref` on another branch | drop | the marker goes; bootstrap always runs (a rerun skips its done steps) and refuses that `--ref` itself |
| `provision-vm.sh` GPU key, `GPU_SSH_KEY_FILE` | a missing `--gpu-key` file | **keep, taken not made** (human, 2026-11-03) | a VM it readies must reach the GPU hosts with a key they already accept |
| `provision-vm.sh` `vm` file, "default VM" line | — | keep the file, drop the line | `--new-pat` reads the file; only `up.sh`/`down.sh` had a default |
| `bootstrap-vm.sh` not root | root | keep, reason reworded | the clone, gh, Claude Code and the image's runs are the user's, as its uid:gid |
| `bootstrap-vm.sh` `--ref` on an existing clone | a clone on another branch | keep | the clone is never silently on another branch than asked |
| `bootstrap-vm.sh` group `peacock`, data dir 2775; `machine.sh` umask 002 and locks | — | keep | sf1 and the cuDF checkouts stay shared by the VM's developers |
| `bootstrap-vm.sh` `~/.bashrc` `CLAUDE_CONFIG_DIR` | — | drop `CLAUDE_CONFIG_DIR`, keep `DISABLE_AUTOUPDATER=1` | the shared directory was up.sh's mount; the pin check still needs no autoupdate |
| `bootstrap-vm.sh` the instance marker | — | drop | the human's decision |
| `machine_git`, `safe_git` (`core.hooksPath=/dev/null`, `core.fsmonitor=false`) | a hook planted in the clone's `.git` | drop | the threat was an instance writing `.git`; without instances only the developer writes it |
| `machine_image_checked` | an image of other inputs | keep | bootstrap's registry fetch and sf1 run in that image |
| `peacock_forward`'s "forwarded twice" and `--vm` refusals | — | drop | the forwarding itself goes |

**Interfaces:**
- Consumes: Task 1's pushed branch.
- Produces: `fetch_embeddings.sh [--sf N]` (origins only); `provision-vm.sh <login>@<address>
  [--env <file>] [--user <name>] [--name <vm>] [--ref <branch>] [--new-pat]`, `--name` recorded in
  `~/.config/peacock/vm` for `--new-pat`; `bootstrap-vm.sh [--ref <branch>] [--cudf <v>]`, ending
  with no marker. new-hosts Task 7 runs these two on `nebius-cpu128`.

- [ ] **Step 1: tests first.** `test_embeddings.py`: drop the six `test_from_s3_*`, the seven
  `test_upload_*`, `FAKE_AWS`, `obj`, `bucket`, `pinned_keys`, `copies`,
  `assert_every_call_names_the_endpoint`, `STORAGE`, `ENDPOINT`; `TREE` loses
  `embeddings-bucket.sh`, `s3.sh`, `upload-embeddings.sh`; rename
  `test_ci_refuses_the_origins_and_not_from_s3` → `test_ci_refuses_the_origins` (CI=1 and
  GITHUB_ACTIONS=1 each → rc 1, no curl call), `test_no_fetch_refuses_both_sources_before_any_call`
  → `test_no_fetch_refuses_before_any_call`; `test_an_interrupted_unzip_leaves_no_glove_text_a_rerun_accepts`
  runs over the origins (fake curl serving `glove.6B.zip`). `test_s3.py`: drop
  `test_the_embeddings_bucket_names_no_endpoint`, `test_the_embeddings_refuse_an_unset_endpoint_before_any_call`,
  `test_the_origins_need_no_endpoint` and the `--from-s3` half of `test_s3_endpoint_from_env`; the
  calibration tests stay; no import from `test_embeddings`. `test_provision.py`: add
  `test_provision_runs_bootstrap_on_every_run` (two runs → `bootstrap.log` two lines) and
`test_provision_copies_the_named_gpu_key` (`--gpu-key <fake key>`: the VM's `gpu_ed25519` equals it,
  mode 600, `GPU_SSH_KEY_FILE` in the VM's `peacock.env`, the key's text in no argv, no `ssh-keygen`
  call) and `test_provision_refuses_a_missing_gpu_key` (rc 2, stderr names `--gpu-key`, nothing
  written on the VM); `test_provision_readies_a_vm` passes `--gpu-key`; `test_provision_refuses_a_ref_the_vm_clone_is_not_on` keeps its two-names
  assertion, now from the stub bootstrap, and loses the hooks-free-git one; its and
  `test_provision_rerun_skips_what_is_done`'s `bootstrap.log` assertions take one line per run. `test_bootstrap.py`:
  add `test_bootstrap_leaves_no_marker` (no `~/.config/peacock/machine`, no `CLAUDE_CONFIG_DIR`
  in `~/.bashrc`, `DISABLE_AUTOUPDATER=1` there) and `test_bootstrap_names_no_launcher` (stdout and
  stderr hold no `up.sh`).
- [ ] **Step 2: run** `for f in scripts/env/tests/test_{embeddings,s3,provision,bootstrap}.py; do
  python3 "$f"; done` → the new tests FAIL, the rest pass.
- [ ] **Step 3: delete and trim** per Files and the decisions table. `provision-vm.sh` runs
  `bootstrap-vm.sh` on every run, `--ref` passed on. Its last `claude auth login` drops
  `CLAUDE_CONFIG_DIR=$HOME/.claude` and the credentials test reads `$HOME/.claude/.credentials.json`.
  `bootstrap-vm.sh`'s closing message ends at the env file, naming no next script.
- [ ] **Step 4: run** every `python3 scripts/env/tests/test_*.py` that needs no image → each
  `0 failed`; `shellcheck scripts/env/*.sh scripts/lib/*.sh testdata/fetch_embeddings.sh` → no new
  finding against `origin/ENS-containers`.
- [ ] **Step 5: commit** (developer identity) `containers-rebase: the agent-container launcher dropped; the VM scripts keep what still protects`
  (launcher, refusals, their tests) and, separately,
  `containers-rebase: the embeddings' S3 path dropped; the pinned origins stay`.
- [ ] **Step 6: docs** (`build-test.md`, one commit `containers-rebase: build-test.md without the launcher or the embeddings bucket`):
  - "Containers" becomes: one intro paragraph (a CPU VM, readied from the developer's machine by
    `provision-vm.sh`, which runs `bootstrap-vm.sh`; builds in the build image, the 128-core
    DuckDB runs); "Trust" keeps "Developers on one VM" and "`peacock.env` holds only the storage
    keys" (the GPU key stays personal, never forwarded) and the read-key-only rule, and loses the instance, hook,
    `up.sh` and known-limitation bullets; "A fresh VM" loses the per-chain `up.sh` paragraph, the
    marker, the default VM and the `CLAUDE_CONFIG_DIR` lines (the GPU key stays); "The env file" keeps
    `GPU_SSH_KEY_FILE`, loses `PROVIDER_CONFIG_DIR` and the "Every start needs the file" paragraph;
    "Instances" is deleted, but for its data-directory, third_party/cudf, registry and data
    bullets, which become "What a VM holds", reworded to what `bootstrap-vm.sh` fills.
  - "Datasets": the bucket sentence names `calibration-bucket.sh` alone; the embeddings-bucket
    bullet becomes one line: `testdata/embeddings.sha256`, committed, pins the four files
    `fetch_embeddings.sh` checks.
  - The tier row's count is left to Task 5's recount.
- [ ] **Step 7: grep.** Each prints nothing:

```bash
git grep -n -E 'up\.sh|down\.sh|board\.sh|upload-embeddings|from-s3|embeddings-bucket|EMBEDDINGS_BUCKET|config/peacock/machine|PEACOCK_MARKER|peacock_forward|peacock-instances' \
  -- . ':!llm-wiki/tasks' ':!llm-wiki/archive' ':!scripts/lib/image-inputs.sh'
git ls-files scripts/env/up.sh scripts/env/down.sh scripts/lib/board.sh scripts/env/upload-embeddings.sh \
  scripts/lib/embeddings-bucket.sh scripts/env/tests/test_up.py scripts/env/tests/test_up_branch.py
```
  Push.

### Task 3: the case list from master's registry, the H200 tree, the gates

**Files:**
- Modify (regenerated): `peacockdb-core/tests/common/corpus_benchmark_cases.inc`
- Modify: the places naming `02f6a4d8` as the H200 list's source:
  `llm-wiki/tickets/benchmarks.md` (#226 note), `llm-wiki/build-test.md` (three: "carrying the
  26-case list", "as it stood at", the record comment) → `<carried> (02f6a4d8 on ENS-optimizer)`,
  so they survive that branch's deletion. `h200/cases.inc:2` says it too; Task 4 replaces that
  file, and its header's sentence moves into build-test.md there.
- Test: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` (containers' tests, unchanged)

**Interfaces:**
- Consumes: Task 1's branch.
- Produces: `corpus_benchmark_cases.inc` = every tpch/tpcds `gpu_*` enabled cell of master's
  `testdata/cost-registry.csv` at sf40 (the list sf40-baseline and tp4-sized read);
  `h200/` held to its own list, `h200/cases.inc` until Task 4 converts it, through
  `PREDATES_LIST` and `PREDATES_PROVENANCE = ["h200"]` (benchmark.rs, containers' `:512`,
  `:625`) — the check's own exception form for "not measured on h200".

- [ ] **Step 1: failing test.** `cargo test --features rust-only -p peacockdb-core --test
  test_corpus_goldens -- the_benchmark_cases_follow_the_registry`. Expected: FAIL, "N cells
  enabled and not listed at sf40" with N ≈ 100 (branch 534 cells, master ≈ 630).
- [ ] **Step 2: regenerate.** Same command with `UPDATE_CANONICAL=1` → PASS. Count cells:
  `grep '^corpus_query_benchmark!' peacockdb-core/tests/common/corpus_benchmark_cases.inc | grep -o
  'tp[14]_[a-z]*' | wc -l`; record it. Run the regeneration a second time: `git status --short`
  shows no further change.
- [ ] **Step 3: the H200 tree.** `cargo test --features rust-only -p peacockdb-core --test
  test_corpus_goldens` → PASS, including `every_declared_case_has_a_section_in_the_tree` and
  `every_timed_case_is_enabled_on_a_device`. If the tree check fails on h200, `h200` is missing
  from `PREDATES_PROVENANCE` or `h200/cases.inc` was altered by a resolution: fix the
  resolution (Task 1 Step 6), never the list. Then `git diff origin/ENS-containers
  ENS-containers-master -- testdata/benchmark-results testdata/calibration/records.sha256` →
  empty (master never touched these since `623073da`; 02f6a4d8's copies are the newer of each
  pair, so the h200 tp1_single/tp4_sized files are 02f6a4d8's — name this in the detail file).
- [ ] **Step 4:** replace the `02f6a4d8` references (Files above); `git grep -n 02f6a4d8 --
  ':!llm-wiki/tasks/containers*'` → only the new phrasing. Commit (developer identity):
  `containers-rebase: the benchmark case list from master's registry; the H200 list's commit by its fork hash`.
- [ ] **Step 5: gates, rust-only (host `dev`, the worktree's `target/`).** sf1 present
  (`ls testdata/{tpch,tpcds,pbench}.sf1`; else `testdata/generate_testdata.sh` per build-test.md
  "Datasets"). Each passes:
  `cargo test --features rust-only -p peacockdb-core --lib`,
  `… --test test_cpu_corpus`, `… --test test_corpus_goldens`, `… --test test_cost_model`,
  `… --test test_golden_format`, `… --test test_crate_privacy`,
  `cargo build --features rust-only -p peacockdb`,
  `cargo test -p cost-report` (bin and `test_source_formatting`, incl. the CI coverage guard).
- [ ] **Step 6: the build image from the fork.** `git submodule update --init third_party/cudf`;
  from a clean tree, `scripts/env/image.sh build build 25.02` and `… build build 26.02`. Expected:
  both succeed and `docker image inspect ghcr.io/asymptote-tech/peacockdb-build:cudf-25.02-$(git
  rev-parse --short=12 HEAD)` exists. Record sizes.
- [ ] **Step 7: gates in the build image, every shape** (per cuDF version `<v>`), each
  in `docker run --rm --user "$(id -u):$(id -g)" -e HOME=$PWD/.home-img -v "$PWD:$PWD" -w "$PWD"
  -e CARGO_TARGET_DIR=$PWD/target-img-<v> ghcr.io/asymptote-tech/peacockdb-build:cudf-<v> bash -lc '<cmd>'`:
  FFI build and tier: `cargo test -p peacockdb-core --lib -- ffi_tests::` (4 pass) and
  `cargo test -p peacockdb-ffi --test test_ffi` (3 pass); gpu shape compiles:
  `cargo test --no-run -p peacockdb-core --features gpu --lib --tests`. A failure that also
  fails on `origin/ENS-containers` is recorded in the detail file, fixed only if a later chain L
  task needs it.
- [ ] **Step 8: script tests.** `for f in scripts/env/tests/test_*.py
  scripts/calibration/tests/test_*.py; do python3 "$f" || echo "FAIL $f"; done` → no `FAIL`.
  The image-dependent ones (`test_image_any_user`, `test_run_image`, `test_no_static_cudart`) need
  the run image: `scripts/env/image.sh build run <v>` first.
- [ ] **Step 9:** commit any gate fix separately, `containers-rebase: <what>`; push.

### Task 4: per-device case lists as CSV; the H200's converted

The human's decision: every per-device case list is `testdata/benchmark-results/<device>/cases.csv`.
Containers' mechanism carries it: `PREDATES_LIST` names the CSV, and the rule is the file's
presence, not the device's name. sf40-baseline writes the H100's on this reader.

**Files:**
- Create: `peacockdb-core/src/test_support/device_cases.rs`, `…/device_cases/tests.rs`
  (`mod device_cases;` and the `pub use` in `test_support/mod.rs`, no feature gate: rust-only
  reads it)
- Create: `testdata/benchmark-results/h200/cases.csv`; Delete: `testdata/benchmark-results/h200/cases.inc`
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` — `PREDATES_LIST` (:512),
  `missing_case_problem` and its doc (:563-600), `PREDATES_PROVENANCE`'s doc (:619-625)
- Docs: `llm-wiki/build-test.md` (:91, :1241, :2142, :2211 on containers' text: `h200/cases.inc`
  → `h200/cases.csv`; :2142's paragraph gains the format, one line per column, and the old
  header's sentence: the H200's cases as the hand-written list stood at `<carried>` (02f6a4d8 on
  ENS-optimizer), fixed since); `llm-wiki/tickets/benchmarks.md` (:26, :29, the same rename)

**Interfaces:**
- Produces in `test_support` (sf40-baseline, query-fuser, tp4-sized and bench-rows' report read
  device lists only through these):

```rust
pub const DEVICE_CASES_HEADER: &str = "dataset,sf,query,modes,status,note";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus { Ok, Skipped, Wrong }            // spelled `ok`, `skipped`, `wrong`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCase {
    pub dataset: String,
    pub sf: String,
    pub query: String,       // the main list's ident, `_` kept (`aggregate_groupby`)
    pub modes: Vec<String>,  // in the file's order; empty iff `Skipped`
    pub status: CaseStatus,
    pub note: String,        // `Skipped`: the reason; `Wrong`: `#NN`; `Ok`: empty
}
pub fn device_cases_path(device: &str) -> PathBuf;     // testdata/benchmark-results/<device>/cases.csv
pub fn parse_device_cases(text: &str) -> Vec<DeviceCase>;  // panics naming the line number and text
pub fn render_device_cases(cases: &[DeviceCase]) -> String; // the header, one row each, `\n` after every line
```
  The format: the first line is `DEVICE_CASES_HEADER` exactly. A row splits at its first five
  commas; `note` is the rest of the line and may hold commas (no quoting; a reason is one line).
  `modes` is `|`-joined with no spaces, each a planning-mode name of `MODES`. Refused: a
  `skipped` row with modes or without a note, an `ok` or `wrong` row without modes, a `wrong`
  note other than `#` and digits, an `ok` row with a note, an unknown status or mode, a
  `(dataset, sf, query)` twice. `parse_device_cases(&render_device_cases(c)) == c`.
- Produces in `benchmark.rs`:

```rust
/// The list a device tree is held to when the tree holds one (`<device>/cases.csv`); a tree
/// without is held to the whole main list.
const DEVICE_LIST: &str = "cases.csv";   // was PREDATES_LIST = "cases.inc"
/// `ok` and `wrong` rows, each mode → its `(dataset, sf, mode)` file → the query, `_` → `-`, as
/// `declared_files` keys them. A `skipped` row declares no section.
fn device_list_files(path: &Path) -> Files;
```
  `missing_case_problem`: `held_to` is `device_list_files(device.join(DEVICE_LIST))` when that
  file exists, else `files`. `PREDATES_PROVENANCE` keeps only the provenance exception; its doc
  drops "held to the list it was measured from".

- [ ] **Step 1: failing tests.** `device_cases/tests.rs`: `a_list_round_trips` (an `ok` row
  `tpch,40,q1,tp1_single|tp4_sized,ok,`, a `wrong` row with `#7`, a `skipped` row → render then
  parse is the input); `a_note_may_hold_commas` (`tpch,40,q9,,skipped,error: a, b` → note
  `"error: a, b"`); `a_skipped_row_has_no_modes_and_a_reason`; `a_wrong_row_names_its_ticket`
  (`#[should_panic(expected = "#")]` over note `7`); `an_ok_row_has_no_note`;
  `a_wrong_header_is_refused`; `an_unknown_mode_is_refused`; `a_case_twice_is_refused`.
  `benchmark.rs`: `a_tree_with_its_own_list_is_held_to_it` — `synthesized_tree("own-list",
  "h100-80gb-hbm3", Some("NVIDIA H100 80GB HBM3"))` plus a `cases.csv` with `q1` `ok` at
  `tp1_single` → `missing_case_problem(&tree, &declared_files(&case_list_path()))` is `Ok`; a row
  `q2` that the file lacks → `Err` naming `q2`; `q2` as `skipped` instead → `Ok`.
  `every_device_list_parses` — `parse_device_cases` over every `benchmark-results/*/cases.csv`.
  `a_new_device_tree_is_held_to_every_declared_case` stays as it is: its tree holds no list.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- device_cases` and
  `… --test test_corpus_goldens -- own_list device_list` → FAIL to compile.
- [ ] **Step 3: implement** `device_cases.rs` and the benchmark.rs change.
- [ ] **Step 4: convert the H200's list, once.**

```bash
{ echo 'dataset,sf,query,modes,status,note'
  git show origin/ENS-containers:testdata/benchmark-results/h200/cases.inc |
    sed -n 's/^corpus_query_benchmark!(\([a-z]*\), \([0-9]*\), \([a-z0-9_]*\), \(.*\));$/\1,\2,\3,\4,ok,/p' |
    sed 's/ | /|/g'; } > testdata/benchmark-results/h200/cases.csv
git rm -q testdata/benchmark-results/h200/cases.inc
```
  Expected: 15 lines (the header and 14 rows); `grep -c ',ok,$'` → 14.
- [ ] **Step 5: the conversion kept every case, in order.** `diff <(git show
  origin/ENS-containers:testdata/benchmark-results/h200/cases.inc | grep -o
  '^corpus_query_benchmark!([^)]*)' | sed 's/^corpus_query_benchmark!(//; s/)$//; s/ | /|/g; s/, /,/g')
  <(tail -n +2 testdata/benchmark-results/h200/cases.csv | sed 's/,ok,$//')` → no output.
- [ ] **Step 6: run** Step 2's commands → PASS; then the whole `test_corpus_goldens` and `--lib`
  (Task 3 Step 5's rust-only gates) → PASS, `every_declared_case_has_a_section_in_the_tree`
  holding `h200/` to the CSV. `git grep -n 'h200/cases.inc' -- ':!llm-wiki/tasks' ':!llm-wiki/archive'`
  → nothing.
- [ ] **Step 7: docs** (Files above) and commit (developer identity):
  `containers-rebase: per-device case lists as CSV; the H200's converted once`. Push.

### Task 5: the shared docs and the detail file

**Files:**
- Modify: `llm-wiki/build-test.md` — header grand total and tier headers recounted on the fork
  (Rust from `--list` per target as the page's rule says; Python adds the `scripts/env/tests`
  count after Task 2's deletions, no longer containers' 208, and the calibration additions); the "Corpus benchmarks" case count = Task 3 Step 2's.
- Modify: `llm-wiki/architecture.md` — the ABI symbol count: master's number + 1 (`device_info`),
  written in words on master's sentence (`:1117`).
- Modify: `llm-wiki/tickets/benchmarks.md`, `llm-wiki/tickets.md` — #226 closed as containers
  wrote it, on master's text.
- Create: `llm-wiki/tasks/containers-rebase-detail.md`

**Interfaces:**
- Produces: the detail file's sections `## Commit mapping` (table old → new, subject, author
  date), `## Conflicts` (Task 1's table, filled with what was done), `## Carried from
  ENS-optimizer`, `## Containers tasks, their new owners`, `## Refusals read`, `## Gates` (commands, counts, known
  failures shared with containers' tip), `## Images` (dispatched commit, run URL, tags, sizes).

- [ ] **Step 1:** recount; `cargo test -p cost-report --test test_source_formatting` → PASS
  (it reads the wiki's anchors and counts where it does).
- [ ] **Step 2:** write the detail file. "Containers tasks, their new owners", verbatim from the
  spec: task 4's publishing → this task (Task 6 below); tasks 6, 6a fresh-VM check →
  new-hosts, `provision-vm.sh` and `bootstrap-vm.sh` only, on `nebius-cpu128`; task 7 (the
  device runner, now `scripts/env/run-gpu.sh`, calling `scripts/nebius.sh`, no host adapters) →
  new-hosts; task 10 → this task; task 11 → sf40-baseline and tp4-sized, those two modes only.
  Dropped by the human: the launcher (`up.sh`, `down.sh`, `board.sh` and their tests, deleted in
  Task 2), task 12, task 5 step 3 and the embeddings' S3 path (deleted in Task 2). Then
  `## Refusals read`: Task 2's table as done.
- [ ] **Step 3:** Step 9 of Task 1 re-run on the new tip → nothing printed. Commit
  `containers-rebase: the wiki's counts on the fork; the record of the fork`; push.

### Task 6: the published images serve the fork

The four images on ghcr (`peacockdb-{build,run}:cudf-{25.02,26.02}`, also tagged `-b50d9b2a4ea0`)
were built on 2026-11-01 from containers' `b50d9b2a`. Every image input is the same on master, on
`ENS-containers` and at `b50d9b2a`: the `docker/` files, `pins.env`, `third_party/cudf` at `43505bb9`,
and flatbuffers `v25.12.19` (checked 2026-11-03). This task changes no `docker/` file. So the images
are expected to serve the fork as they are, and the task proves it rather than rebuilding them.

**Files:** none changed. Results in the detail file's `## Images`.

**Interfaces:**
- Produces: `ghcr.io/asymptote-tech/peacockdb-{build,run}:cudf-{25.02,26.02}`, public, whose
  `peacockdb.inputs` label equals the fork's own inputs. Later tasks pull `:cudf-<v>`, and the label
  check (`image_inputs_differ`) is what ties an image to a tree.

- [ ] **Step 1: anonymous pull** on `dev`, with no credentials:

```bash
export DOCKER_CONFIG=$(mktemp -d)
for k in build run; do for v in 25.02 26.02; do
  docker pull ghcr.io/asymptote-tech/peacockdb-$k:cudf-$v || echo "FAIL pull $k $v"
done; done
```
  Expected: no `FAIL`. A 401/denied means a package is private: the human sets it public; retry.
- [ ] **Step 2: the label matches the fork.** In the fork's worktree, for each of the four:
  `. scripts/lib/image-inputs.sh; image_inputs_differ "$(docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' <ref>)" . <k> <v>`
  → prints nothing. Record each digest and its empty diff.
- [ ] **Step 3: only if a diff is non-empty**, rebuild. `images.yml` is dispatchable only from the
  default branch, and master has none. Stop, `blocked(building)`, naming the differing inputs and
  asking the human to land `.github/workflows/images.yml` alone on master, as a separate commit
  (approved in advance). Then `gh workflow run images.yml --ref ENS-containers-master -f kind=all -f cudf=all`,
  `gh run watch <id> --exit-status` → four jobs green, and repeat Steps 1–2. Do not push to master.
- [ ] **Step 4:** record tags, digests, and whether a rebuild happened; commit the detail file; push.
