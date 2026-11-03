# containers-rebase — detail

## Run log

- 2026-11-03: coordinator start. Board `building`. Preconditions (plan Task 1 Step 1) hold:
  origin/master `dc90b51c` carries this plan (Step 7a present); `611b5dcd..origin/ENS-containers`
  = 16; merge-base of ENS-optimizer and ENS-containers = `611b5dcd`.
  Tips: `origin/ENS-containers` = `b71758e9`, `origin/ENS-optimizer` = `561411b4`.
- verda: not reachable (no `VERDA_CLIENT_ID`); CPU runs local on dev.
- Surgery state lives in `.claude/ensemble/L.surgery` until Task 1 Step 5 folds it in here.

## Surgery

Folded from `.claude/ensemble/L.surgery` at Task 1 Step 5; kept in step with it through Step 10.

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
