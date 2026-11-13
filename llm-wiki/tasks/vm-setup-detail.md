# vm-setup — run detail

Branch `ENS-vm-setup`, forked from `ENS-containers-only` at `7aeec989` and rebased onto its
rewritten head, `1eaefe39`, beside sirius-bench; its PR targets `ENS-containers-only`.

## Progress

- 2026-11-09: spec, plan and board entry committed (`fed95745`); Task 1 committed (`ad19c967`).
- 2026-11-09: Task 2 on dev, below. Dev's docker keeps the branch's 25.02 build and run images
  (inputs tags below) for Tasks 13, 14 and 16.
- Task 10: the runner, `follow`, `cancel`, `--init` and the card lock; below.
- Task 11: one tree per VM dir; below.
- Task 12: records and logs under a stamp, ship's stamp, `pull`, `pull --clear`, bench-each; below.
- Task 13: CI's `gpu-tests` names a fixed tree id and cleans up with `cancel`; below.
- Task 14: one table of build and test lines, each run as written; below.
- Task 14a: a missing `nebius.yaml` is cut on first use; below.
- Task 15: the branch's five images published; the table's 26.02 row run; below.
- Task 16: nebius-gpu by hand, steps 1 to 3; image pulls name a full disk and a lost VM; below.
- Task 17: a fresh verda VM, provisioned from this branch, its rust-only build and run green; a
  second developer, the login user, beside the first; below.

## Plain builds

On dev, 2026-11-09, at `ad19c967`, cuDF 25.02. Each `image.sh build` was followed at once by the
plain build of the same Dockerfile from the repository's root, with no build args.

| kind | `image.sh build <kind> 25.02` | plain `docker build -f docker/<kind>.Dockerfile .` | RUN/COPY steps, CACHED | sha256 of `RootFS.Layers` JSON, both images |
|---|---|---|---|---|
| build | 1659 s, `peacockdb-build:cudf-25.02-in-525f460c6dd5` | 2 s | 17 of 17 | `32ce27a87627f6daf76224fe89fd52d7b9cd53d6190a3ff03ece5c4c1e572d33` |
| run | 248 s, `peacockdb-run:cudf-25.02-in-febbd8e327d9` | 3 s | 8 of 8 | `a57a2838d613966a2de772f15f23285603cedb51ba4b4c71e61e14181416077c` |

- The run image's `peacockdb.driver-minimum-ptx` label reads `575.51.03`.
- `docker build -f docker/run.Dockerfile --build-arg CUDF=25.10 .` exits 1 at its final `FROM`:
  `ERROR: failed to build: failed to solve: run-25.10: failed to resolve source metadata for
  docker.io/library/run-25.10:latest: pull access denied, repository does not exist or may require
  authorization`.
- The plain images' tags were removed after the comparison.

## ssh through the helper

- `nebius.sh` has one ssh path, `vm_ssh_with <attempts>`, over `ssh_retry_up_to`: four for `ssh`,
  one for `rsh`, so `rsh` keeps the host-key repair and is never retried itself. install-watch's
  rsyncs use this script's `rsh` as their transport, with the options it was given.
- The repair runs only when the attempt returned had not authenticated, which `ssh_retry_up_to`
  reports through the caller's variable (a nameref, never a global). Grepping stderr alone would
  also match the remote command's own stderr: a command that printed `Host key verification
  failed` and exited 255 was re-keyed and run a second time. The helper's locals carry a `_sru_`
  prefix: a caller's variable named like one would resolve to the local instead, and named like the
  nameref itself would be circular.
- `start`'s wait checks its deadline between tries only. A check just before the deadline passes,
  then `sleep POLL` and one more full try: a CLI call, four attempts at `ConnectTimeout=15` and
  35 s of pauses. So the wait can end up to about 105 s past `GPU_HOST_START_TIMEOUT`.
- A soft `stop` on an unreachable VM now spends the probe's four attempts, about 95 s, before it
  stops on the API's state.
- Tests that pass on the code before this change, kept as guards on the rules the change must
  not break: a changed host key repaired without a retry, `rsh` one attempt, a session that
  authenticated not run twice. The wall-clock test goes red only once `vm_ssh` retries and the
  wait still counts polls (measured: 17.8 s against its 15 s bound).
- `gpu_host.py`'s `FAKE_NEBIUS_RSH_FAILS=<n>` counts every rsh the Host has made, across runs; a
  later run's drop is asked for as the rsh count so far plus one.

## The nebius CLI on a VM

- A machine that runs `test_bootstrap.py` may have `nebius` on PATH: a VM bootstrapped already, or
  the build image (0.12.293). So the fake machine's PATH leaves it out (`path_without`). In the
  image, with that line reverted, the install and the wrong-sha tests fail: the run skips the
  install. CI runs the file on the runner (`pipeline.yml`'s `on_runner`).
- `path_without` makes its directory with `mkdtemp`, so the machine's call and a test's own call
  share one temp dir.
- The skip test passes on the code before this change, kept as a guard: an install that ignores
  the version on PATH fails it.
- The download is removed by an EXIT trap, cleared once the binary is installed, before
  `machine_image_checked` sets its own. Measured: a TERM, a HUP, and a SIGINT to the process group
  (Ctrl-C) each run it. A SIGINT to bash alone waits for the foreground child.

## The two-profile cut

- `peacock_nebius_cut` reads `profile list` once, on the copy, before any change, and deletes the
  names from that list: update and activate do not change the set.
- The caller makes the copy and removes it on exit: the copy holds every profile until the end.
  bash 5.2 runs an EXIT trap on HUP, INT and TERM (measured: TERM, HUP and INT to the process
  group while the fake sleeps in `update`), so no signal trap is set; dropping the EXIT trap fails
  `test_a_signal_mid_cut_leaves_only_what_was_there`, dropping the TERM trap it once had did not.
- A TERM or HUP to bash alone leaves the CLI running, and a CLI that writes the config by its path
  afterwards recreated a copy beside `nebius.yaml` (measured red: `nebius.yaml.plT3Ji`). So the
  copy is `nebius.yaml` in `mktemp -d ~/.config/peacock/nebius-cut.XXXXXX`, removed whole: a late
  write then finds no directory, unless the CLI makes one, which is unknown for the real CLI. The fake's slow call reads its config before it sleeps and writes
  it after, and logs its pid, so the test waits for that CLI to end before it looks.
  install-watch's copy is in its temp dir, which its EXIT trap removes the same way.
- install-watch refuses an active profile whose name is not `--profile`'s shape, without showing
  it: `p` is logged and read as `<name>[=<pem>]`. `active_profile` asks and checks with tracing
  off, so `bash -x nebius.sh install-watch` does not trace the raw answer (measured red before).
  A missing active profile and a bad-shaped one are one refusal.
- The cut makes the copy empty under `umask 077` and then `cp`s onto it, which keeps the file's
  mode: a plain `cp` gave the copy the source's mode less the umask (measured 644) until the final
  `chmod`. In the cut rather than in `nebius-profiles.sh`, so install-watch's copy is 600 too.
- `peacock_nebius_profiles` and `peacock_nebius_version` turn tracing off for themselves
  (`local -; set +x`): under `bash -x` the parser's `[[ =~ ]]` traced each raw `profile list` line.
  `version`'s answer is used only in a shape like `0.12.293`, else `unknown`; the image's CLI
  prints exactly `0.12.293` (run under a temp HOME with no config).
- A failed `profile list` and one of another shape are one refusal, naming the CLI's version. The
  CLI's stderr is dropped there too: it can echo the config.
- `nebius_cli.py` prints FAKE-KEY-MATERIAL on stderr for every call, and on stdout too for one
  whose answer is not used (`update`, `activate`, `delete`, `unset`, an unknown profile); on
  `profile list`'s stdout it would fail the parser on every run. `fakes.py`'s nebius does the same
  for its profile, config and version calls. Measured red: dropping `profile list`'s stderr
  redirect (8 tests), the check's `config get` one (2), `activate`'s output redirect (the late
  refusal test), install-watch's `profile active` one (its no-`--profile` test).
- `FAKE_NEBIUS_SLOW=<verb>` and `FAKE_NEBIUS_FAIL=<verb>` drive the signal and late-refusal tests.
  A failing `mv` is not tested: making it fail (a read-only directory) also stops the copy's
  removal, so the test could not hold to "nothing left".
- `peacock_nebius_config_checked` is here, with five tests in `test_nebius_profiles.py`, so
  `provision-vm.sh` has only to call it.
- install-watch asks `profile active` of `NEBIUS_CONFIG` itself, through `--config`: `cli()` names
  no config yet, and the CLI's default may be another file than `GPU_HOST_NEBIUS_CONFIG`.
- install-watch catches the cut's stderr, its refusal alone, and puts it in its one failure line,
  to stderr and `nebius.log`, beside the CLI's path, the profile and any `--key-file`.
- `nebius-profiles.sh` runs on a laptop, so it carries `provision-vm.sh`'s bash 4.4 guard. CI's
  shellcheck line names it.

## What the VM holds

- `nebius.sh` checks its verb, then the config, then the CLI, before its state dir or any call: a
  usage error stays 64 whatever is missing, and a missing config or CLI is 1, never
  `peacock-config.sh`'s 2, which `nebius.sh` means as "busy".
- `cli()` puts `--config` after the command words, beside `--profile`: `fakes.py`'s nebius reads
  `compute instance …` from the front of its argv.
- `NEBIUS`, when set, is taken as given; `test_absent_cli_is_1_not_69` holds an unknown one to 1.
- `World.env()` names a HOME under the world: `nebius.sh` now reads `~/.config/peacock/` when
  nothing else is named, and a test must not take the machine's own GPU key.
- The CLI lookup tests build their PATH from links to every command but `nebius`: the build image
  has `/usr/local/bin/nebius`. `test_the_nebius_bin_cli_without_one_on_path` and
  `test_gpu_host_key_wins_over_the_peacock_key` pass on the code before this change, kept as
  guards: dropping the fallback or adding both keys fails them.
- The action's test runs its `run:` block over a curl that installs nothing and a CLI that writes
  `~/.nebius/config.yaml` and exits 9 on `--config`: the file `GITHUB_ENV` names is the one the
  CLI made with no config named.
- A laptop's `peacock.env` without a final newline now reaches the VM as it is: nothing is
  appended to it there.
- `nebius.sh`'s `-i ~/.config/peacock/gpu_ed25519` holds on dev too: with `-i`, ssh offers none of
  its default key files, only that key, an agent's and `~/.ssh/config`'s `IdentityFile` lines. So
  the GPU VMs' login user must accept `gpu_ed25519.pub`; `GPU_HOST_KEY` names another key.
- `test_a_missing_config_is_refused_naming_nebius_profiles` also pins an unknown verb beside an
  absent config to 64. Measured red with the config check moved above the verb check.

## Container paths

- The fakes' docker (`machine.py`'s `RUN_OPTIONS`, spliced into `gpu_host.py`'s too) refuses, exit
  126, a bind's `dst` or a `-w` under the fake docker's `HOME`, which stands for the image's
  `/home/peacock`. The fake machine's clone lies under its home, so with the steps at host paths
  every whole bootstrap run went red (11 tests), not only the two the plan names. On the GPU fake
  the stage lies outside the VM user's home, so `prepare` stayed green there and only the new
  test's argv assertions went red.
- The generator's output is found through the bind that holds `/peacock/stage/generate_testdata.sh`
  (the innermost, should binds nest); a path in no bind fails the fake.
- `test_a_tree_under_the_image_home_is_taken_for_uid_1000` passes on the code before this change,
  kept as a guard: a refusal of every uid but one, or of any tree under the home, fails it. The
  refusal test also runs a home the tree's path only begins with (`/home/peacockdb` against
  `/home/peacock`), which a prefix match without the slash would refuse.
- `build-test.sh` holds the worktree's git dir to the same rule as the tree: a worktree outside
  the image's home whose main clone lies in it would pass a check of the tree alone, and every git
  call in the container would then fail on its unreachable `.git`. Measured red before (exit 0).
- The fakes read `-v` binds as well as `--mount` ones, so run-gpu.sh's device runs are held to the
  same refusal; a `-v` source that is missing is not refused, as docker makes it.

## The run image by its ID

- `run_image_read` reads the ID and three labels with one `--format`, a line each. `$(…)` drops
  the trailing lines of empty labels, so the fields are read with a default: an empty PTX label,
  the last field, left `values[3]` unset and `set -u` killed the script (measured red).
- The inputs comparison is `machine_image_inputs_checked`, over a label already read;
  `machine_image_label_checked` is that over its own inspect. `prepare` keeps
  `machine_image_checked` for its pull's messages, so it compares the run image's inputs twice:
  once after the pull, once in `run_image_read`.
- `vm_cudf` takes the ID too, so its create and its stamp are of the image whose labels were read.
  It read `.Id` from the ref and then created from the ref, two reads with sf1 and sf40 between
  them and the check.
- The fake docker moves a tag right after its nth read from then on (world `moves`). That is how
  `test_run_runs_the_image_id_its_labels_were_read_from` goes red on the code before: its second
  read of the ref found the moved tag's inputs and refused the run. In
  `test_prepare_copies_cudf_from_the_image_id_it_read` the tag moves after prepare's second read
  (the pull's label check, then `run_image_read`), so a later read of the ref sees the moved one
  (measured red, on the `.image-id` assertion).
- `cards_checked` takes the query's stdout and stderr as its answer: a missing nvidia-smi is bash's
  `command not found`, exit 127, on stderr, which `$(…)` alone would leave out of the refusal
  (measured red without `2>&1`: the shell's text was on stderr, not on the refusal's line). The fake
  prints its failures (no devices, exit 6; no driver, 9; a driver/library mismatch, 18) on stdout. A
  failure is exit 2, "the driver query on the VM failed (exit N): <answer>": the code can also be
  `nebius.sh`'s 1 or ssh's 255, a VM gone between the image read and the query, so the refusal
  does not blame nvidia-smi. A bare `$(…)` had
  ended the script with nvidia-smi's own code and nothing said, which run-gpu.sh's callers read as
  its own codes (2 bad input, 4 restart). An empty answer with exit 0 is "lists no GPU", exit 2.
- `nebius.sh`'s 69 and 75 at that query are passed on, as at every other call; nvidia-smi has
  neither code. `test_nebius_sh_s_own_exit_at_the_driver_query_passes_on` passes on the code
  before, kept as a guard: dying on every failure turns it red (measured). The fake nebius's
  `FAKE_NEBIUS_RC_ON` narrows its refusal to the calls whose argv holds that text.
- `test_a_card_is_held_to_the_higher_of_the_two_minima` (26.02's labels: 575.57.08 above the PTX's
  575.51.03) passes on the code it was written against, so its red was measured on two mutations:
  the PTX minimum always taken past 9.x, and the minimum hard-coded as 25.02's. Both went red.
- The fake docker exits 99 on a `--format` it cannot render, a code nothing maps: 1 is "No such
  image", and 64, through `vm()`, reads as `nebius.sh` refusing its options.
- The fake `nvidia-smi` answers the one query and fails any other, so the new tests went red only
  in their setup at first. Their own red was measured with the fake briefly answering the old
  query too: each failed on its own assertion.
- Ported from `4e785104`'s `test_gpu.py`: `test_run_refuses_old_driver` as
  `test_every_card_s_driver_is_held_to_the_minimum` (its `--cudf 26.02` half dropped: the minimum
  is now the image's label, not a pin read by version);
  `test_a_card_past_compute_capability_9_needs_the_driver_that_compiles_its_ptx` as
  `test_a_card_past_9_needs_the_driver_that_compiles_its_ptx` and
  `test_a_card_below_compute_capability_8_is_refused`, which is exit 3 here, 2 there;
  `test_a_driver_answer_that_is_no_version_is_refused` under its name;
  `test_the_image_run_is_the_one_whose_label_was_read` as
  `test_run_runs_the_image_id_its_labels_were_read_from`.

## The runner

- The plan's runner signature carried an `<image id>`; the runner takes the whole `docker run`
  argument list after `--gpus all` instead (options, image, command), since a mode run and a staged
  binary put different things after the image. `vm_started` adds the mounts, user, hostname and the
  `peacock.dir` label to both.
- Container names are global to the VM's docker, and two users' dirs of one name (both clones named
  `peacockdb`) run containers of one name. So each container is labelled `peacock.dir=<its dir>`, and
  `vm_idle`, `status` and `cancel` take a `peacock-<i>` as this dir's only when the label is its path,
  or absent (a container from before the label). Without it a second user's `run` refused on the
  first user's going container, naming `cancel`, which would then have removed it. The card lock
  serialises the two runs, so the names never collide while both run.
- Another dir's container of this name that its run left behind (ended, or running with its runner
  killed) is refused, exit 2, naming its dir, and kept: `vm_name_free` refuses an ended one at a
  run's start (not `ship`'s, which runs no container: `vm_idle` alone holds ship),
  and the runner, once it holds the card, refuses any it finds, since that dir's live runner would
  have removed its own before letting the card go. Every `docker rm` of the runner, `vm_idle` and
  `cancel` is held to this dir's label. A refusal rather than a per-user name: `peacock-<i>` is what
  CI's cleanup and its guard name, and a leftover of another user's is that user's to remove. An
  ended container of the other dir in the instant between its client's end and its runner's `rm` is
  refused too; the call can be run again.
- Known limit: a runner and its client both killed `-9` leave the container running without the
  card lock, so another dir's run may join it on the card. This dir's next run refuses it (naming
  `cancel`), and another user's of the same name refuses it as above; a run of another name does not
  see it.
- `vm_idle` refuses a container with no runner only while it runs. One that ended is cleared, as
  `vm_not_running` did: a VM restart leaves the container exited (no `--rm`), and Review Focus 1
  needs `run` to start there. The ported container test uses a running one.
- `vm_started` closes `run/lock` on the runner's command line and again before it returns: the
  runner inherits every fd, and an attached call goes on to follow, so either holding it would block
  the next `run` until the run ended instead of refusing it. A call that waits for `run/lock` says so.
- The runner writes `run/state` itself, `<pid> <boot id> <what> <stamp> <start time>`, right after
  its TERM trap, and `vm_started` lets `run/lock` go only once that state is there. `cancel` takes
  `run/lock`, so it never finds a runner whose trap is not set yet: before, a cancel right after
  `run --detach` killed the runner by TERM's default action, with no status (measured red, through
  a slow first `docker info` in the runner). The start time is `/proc/<pid>/stat`'s field 22; with
  the boot id it tells the runner from a process that took its pid (measured red: a live `sleep`
  under a dead runner's state read as `waiting`).
- The runner's TERM trap only sets a flag, and `cancel` sends TERM every 0.2 s until the runner
  ends. Each TERM cuts short the wait in hand (the card's, the client's), and the flag decides what
  comes next, so a TERM at any point lands, the waiter's start and the client's included, and a
  client that ends by itself as the cancel comes keeps its status and, a staged binary's, its
  verdict. The client's exit goes to `run/exit` from a subshell, as the earlier branch did.
- `peacock_card_take`'s waiter shares the open file description, so the lock stays held after the
  waiter exits; its pid is in `PEACOCK_CARD_WAITER` for the runner to kill. It makes the file only
  when it is missing, and opens it read-only: on the VMs (Ubuntu, `fs.protected_regular=2`) an open
  with `O_CREAT` of another user's file in sticky `/run/lock` fails with EACCES, which `flock -n
  <file> true` hit in `nebius.sh stop`'s probe and read as held for every user but the file's maker.
  Two users making it at once: the loser's create is refused, and the file is there. A lock it
  cannot make or open is said in the log and is exit 1, never a run without it. Another uid's file
  cannot be made unprivileged, so the probe's red is the fake ssh's: it answers a create-open of
  `another's` free lock as held, as the kernel refuses it. `peacock_card_take`'s race has no test.
- `following` runs the VM call in the background and waits: bash runs a trap only once a
  foreground child ends, so a TERM to run-gpu.sh alone waited for the run's end (measured red). The
  job is started under `set -m`, turned off right after, so it leads a process group of its own,
  and the trap kills that group: a tree gathered with `pgrep` and then killed misses a process
  forked in between, such as ssh-retry's next attempt. Measured: no job notice is printed, `wait`
  gives the job's own code, and a killed group's `wait` 143. No test can catch that fork's window;
  the pid-only and group TERM tests bound the exit.
- `gpu_host.py`'s `__exit__` kills a runner's group only when the state's pid has the state's start
  time, so a pid another process took is never killed.
- `follow` tails with `-s 0.2`: `tail --pid` checks the pid at its sleep interval, 1 s by default,
  which every attached run then waited out.
- The fake docker's device run releases the world's lock while it holds and re-reads the world
  every 0.05 s; every world write, the tests' included, is a whole-file replace under that lock,
  since the fake `nvidia-smi` and the tests read without it. A held run gives up after 60 s.
- The fake `docker ps` renders `{{.State}}` and `{{.Label "…"}}`; it printed the state alone at
  first, which read every container as another dir's (four tests red after the change, root cause).
- `test_follow_and_status_with_no_run_name_status` passed on the code before at first: its
  "unknown verb follow" message listed `status`. It now also requires `no run`.
- `create_nsys_profile.sh` sources `peacock-config.sh` for `peacock_card_take` and the lock path,
  sent to the host as text and as a `%q` literal: the fake rsh, like ssh, forwards no environment.
- Red before the change: `test_run_gpu_runs.py` 20 of 21 (the 21st as above), `test_nebius.py`'s
  new test, and the four reshaped `test_run_gpu.py` tests.
- Second round, red before the fixes: `test_a_term_to_run_gpu_sh_alone_ends_the_following`,
  `test_a_runner_pid_another_process_took_is_not_going`,
  `test_a_cancel_as_the_runner_starts_still_cancels`,
  `test_another_dir_s_container_of_this_name_refuses_a_run_and_is_kept`, the `run/lock` message in
  `test_a_run_waits_while_its_dir_s_run_is_being_started`, and
  `test_soft_stop_stops_on_another_user_s_free_card_lock`. Third round:
  `test_ship_beside_another_dir_s_ended_container_of_this_name_ships_and_keeps_it` red before; the
  restart test writes the test process's real start time, so only the boot id differs, and went red
  (`running`, exit 5) with the boot-id check removed. `test_cancel_leaves_other_dirs_alone` now
  has the other user's container of this name on the card; it passed before, and went red with
  `vm_cancel`'s label check removed, as the other-dir test did with the runner's. The ship refusal
  added to `test_a_container_of_the_dir_still_on_the_vm_refuses_a_new_run` passed before: it restores
  an assertion the reshape dropped.
- The fake docker prints a device run's output before the world says it runs, so a test that waits
  for `running` and reads `status` finds the output in the log. `info_delays` slows each next
  `docker info` in turn; the dev side's image read makes the first.

### For Task 12

- Logs are already `logs/<what>-<stamp>.log`, and `test_pull_brings_the_results_the_record_and_the_log_home`
  is already reshaped to `bench-<stamp>.log`, so `test_each_run_logs_under_its_stamp` passes at its
  red step.
- `bench-each.sh` (near its line 175): a dropped ssh during `run bench` now exits 1 with the run going
  on. bench-each counts the case red and moves on; the next case's `run bench` is refused, exit 2,
  because the run still goes, and bench-each stops with "run-gpu.sh exited 2". Its rework in Task 12
  should follow the going run (or wait for it) before the next case.
- `pull --clear` claims the dir before anything: `tree_claimed` already comes first in `cmd_pull`.
  Add `("pull", "--clear")` to the verbs `test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears`
  and `test_a_copied_tree_is_refused_its_record_before_any_vm_call` refuse, and to the first one's
  check that nothing in the dir was touched.

### For Task 13

- If any `gpu-tests` run reached nebius-gpu after Task 11 and before Task 13, `~/peacock/ci/.tree`
  holds a random id and refuses `PEACOCK_TREE_ID=ci`, exit 2, naming `--dir`. Recovery, once, by hand
  with the VM up: `scripts/nebius.sh --instance-id computeinstance-e00tnrse7ayntnzcyt ssh 'rm
  ~/peacock/ci/.tree'`. The next call with `PEACOCK_TREE_ID=ci` claims the dir as one made before
  `.tree`, and says so.

## One tree per VM dir

- The tree's record is made on dev before the VM's claim, not after it succeeds. A call killed
  between a claim and a later write would leave the dir claimed by an id the tree no longer has.
  So a refused claim leaves the record; it is the tree's id all the same, and its next call with
  `--dir <name>` uses it. The earlier branch's "a refused name is not recorded" is dropped for this.
- Both the record and `.tree` are written whole to a temp file and linked into place (`ln`, which
  fails on an existing name). The plan's `set -C` makes the file empty and then writes it, so a call
  at the same moment could read it empty and take the dir as another tree's. Two first calls at once
  keep one id. No test races them.
- The claim is a call of its own, one ssh more per verb, rather than folded into each verb's first
  VM function: `run`'s first VM step is the image read, so a claim in `vm_run` would come after it.
- `status` and `follow` read the record, so a copied tree is refused them before any call, as every
  verb is. They make nothing, on dev or the VM, and a second tree may read another's run with them.
- `cancel` claims: a second clone of the same name must not cancel the first's run. Like every
  claiming verb it makes a missing dir, with its `.tree`.
- `PEACOCK_TREE_ID` set but empty is refused, not taken as unset: an expression in CI that expands
  empty would otherwise make a record in each fresh checkout and claim the dir by a random id.
- A claim of a dir from before `.tree` is said once on stdout, `claimed for this tree`.
- Until Task 13, CI's `gpu-tests` names no `PEACOCK_TREE_ID`: its first run claims `~/peacock/ci/` by
  a random id, and the next run's fresh checkout is refused. The job runs only on a push to master or
  a dispatch, and Task 13 comes before the merge. A dir once claimed stays claimed: a `gpu-tests` run
  between the two leaves `~/peacock/ci/.tree` holding a random id, which refuses Task 13's
  `PEACOCK_TREE_ID=ci` too ("### For Task 13").
- A record that cannot be made (`.run-gpu-tree` a dangling link or a directory, or a filesystem
  without hard links) is refused, exit 2, before any VM call; the link's failure alone cannot tell
  it from another call's record. The VM refuses an empty id too, rather than write an empty `.tree`
  that every such tree would share. Temp files of the record are `/.run-gpu-tree*` in `.gitignore`.
- The plan puts the docs in build-test.md's "The scripts"; `run-gpu.sh` is described in "The H100's
  device run", so the bullet is there, with the exit table's row 2.
- Red before the change: all six (`0 passed, 6 failed`). `test_status_and_follow_claim_nothing` fails
  only at its last line, the dir's `.tree`: what it guards is something not done. Each guard was
  then turned red by a change to the script: `status` claiming; `cancel` claiming nothing; `run`
  claiming after its run; an empty `PEACOCK_TREE_ID` taken as unset; the record's path unchecked.
- Second round, red before the fixes: `test_a_record_that_cannot_be_made_is_refused_before_any_vm_call`
  (the claim went out with an empty id; only the missing `prepare` stopped `ship`), and
  `test_the_vm_refuses_an_empty_tree_id` with the VM's check removed. `ship` added to the other
  tree's calls beside the owner's going run went red with `ship`'s claim moved after
  `vm_ship_ready`, which re-points the owner's dataset links.

## Records, ship's stamp and pull

- `.shipped` is merged, not replaced: `ship` sets the lines of what it sent and keeps the others,
  through `vm_stamped`, the same writer as a pull's. Replaced whole, a committed file deleted from
  git since the last ship (its VM copy stays: the testdata stream has no `--delete`) would lose its
  line, be listed with `-`, and come back into the tree as new. The plan's "writes `.shipped` whole,
  through a temp file" is read as atomic.
- The plan's `records-*.tsv` also matches `create_nsys_profile.sh`'s `calibration/records-metrics.tsv`,
  which the metrics pass writes on the VM and leaves there. A run's record is
  `records-<stamp>.tsv`, so `pull --clear`, pull's never-replace rule and bench-each's set-aside take
  `records-[0-9]*.tsv`. Without that, bench-each would set the metrics pass's rows aside as a case's
  record and they would reach the join. Red before: `test_bench_each_sets_aside_no_record_but_a_run_s`
  and the metrics lines of `test_pull_clear_removes_what_came_home_and_keeps_the_rest`.
- `--clear` asks the VM which records and logs it holds (`vm_clearable`, printed by
  `vm_pull_ready`, so no extra ssh), and sends the sha256 of only those copies home. Hashing every
  local log would grow with the cache, which nothing clears, and bench-each pulls once per case.
- `vm_pull_ready` takes `run/lock` first, then refuses `--clear` while a run goes, then makes the
  pulled dirs. Two earlier orders were wrong. Making the dirs first changed the dir before the
  refusal (the snapshot in `test_pull_clear_is_refused_while_a_run_goes`). Checking for a going run
  before taking the lock let a run that held `run/lock` while it started pass the check; its log,
  listed once the lock came, unchanged between the transfer and the clear (a run waiting for the
  card), was then removed while the runner wrote to it
  (`test_pull_clear_is_refused_by_a_run_that_starts_while_it_waits`, red: `removed 1`). Under the
  lock a run is either going or not yet started, and one started after the list is taken writes
  only names the list never held: its own log, and `records.tsv`. A `records.tsv` is kept under
  its stamp only while no run goes, so a plain `pull` beside a run leaves the run's record alone.
- `pull --clear` drops the `.shipped` line of each file it removes: otherwise the file grows two
  lines per bench-each case, for logs and records gone from the VM.
- `status` and `follow` of a run whose log `pull --clear` removed (every bench-each case ends with
  one) said `exited 0` and then failed on `tail`/`cat`, exit 1. They now say the log was cleared
  and where pull keeps it, and exit as the run's status (`vm_log_shown`).
- bench-each sets aside the records a pull brought before it acts on the pull's exit. A pull that
  kept another file changed on both sides (a `gpu-result.txt`) exits 1 with the case's record home
  and the VM's cleared; stopping first left that record in the tree, and the rerun counted it as
  there before and never set it aside, so the case's segment never reached the join.
- `vm_records_kept` takes the state's stamp whenever there is a state and no run goes. A
  `records-<that stamp>.tsv` already there would be overwritten, but a runner moves its record only
  after its container is gone, so a `records.tsv` with the same stamp beside it cannot arise.
- pull hashes the copy that came, not the listed sha256: a run may change the file between the list
  and the transfer, and the stamp must name the bytes now home.
- `in_pulled_set` is one predicate for both sides: dev filters the shipped paths with it, and it is
  sent to the VM among the helpers for `vm_pull_list`.
- bench-each, note (b) above: after `run bench` exits 1, `status` 5 means the run goes on, and
  `follow` takes it up until it ends; its exit is the case's. Without it the next `pull --clear` is
  refused and bench-each stops with 2. The fake run-gpu.sh's `drop` behaviour; red before as the
  verbs sequence.
- Step 5's split: `run-gpu.sh` reached 1084 lines, so the VM's side, whole, is
  `scripts/env/run-gpu-vm.sh` (589 lines; run-gpu.sh 520). It is sourced as
  `. "$root/scripts/env/run-gpu-vm.sh"`: shellcheck resolves `$root` from the repo root, as for the
  libraries, and `$here` not at all. `-x` follows the source for definitions only; a warning planted
  in the sourced file is not reported through `run-gpu.sh`, so CI's list names `run-gpu-vm.sh` too.
  Linted alone it has no `set -e`, so the file disables SC2164 with the reason, and `vm_docker`'s
  directive gains SC2032, which the dev side's `docker()` directive covered before. The only other
  edit is `on_vm`'s comment, "a function below" now "a function of run-gpu-vm.sh".
- Red before the change, `7 passed, 19 failed` over the records file: the 18 new tests but
  `test_each_run_logs_under_its_stamp`, which passed at red as noted above (logs were already
  stamped), and the two Task 11 tests with `pull --clear`. Also 6 of 15 bench-each tests, and the
  reshaped `test_pull_brings_the_results_the_record_and_the_log_home`. Some plan cases pass on rsync's
  `--update` with the plan's mtime alone, so each also takes the mtime that `--update` gets wrong:
  `test_pull_skips_a_shipped_file_a_checkout_touched` an hour on (passes on `--update`) and 2001
  (red); `test_pull_says_nothing_of_an_unchanged_identical_file` a local mtime of 2001, which
  `--update`'s transfer reset; `test_pull_takes_each_section_a_run_adds_to_a_shipped_file` a local
  mtime an hour on, as a checkout after the ship leaves it.
- Second round, red before the fixes: `test_pull_clear_is_refused_by_a_run_that_starts_while_it_waits`
  (exit 0, the going run's log removed), `test_status_and_follow_of_a_run_whose_log_was_cleared`
  (exit 1, `tail: cannot open`), the `.shipped` assertion added to
  `test_pull_clear_removes_what_came_home_and_keeps_the_rest`, and
  `test_bench_each_sets_aside_the_record_of_a_pull_that_kept_a_file` (no record in `<dir>`). The
  fake run-gpu.sh gains `conflict`: the next pull exits 1 naming a `gpu-result.txt`, after it has
  brought the records home and cleared them, as the real one does.

## CI's gpu-tests: a fixed tree id and cancel

- `PEACOCK_TREE_ID: ci` is in the job's `env:`, not a step's: `prepare`, `ship`, `run` and `cancel`
  all claim `~/peacock/ci/`, and a step without it would claim by a random id. No other line of the
  job may name it: a step's `env:`, an inline assignment, an `unset` or a `$GITHUB_ENV` line would
  give some calls another id.
- `cancel` claims first, with the same id, then ends the run. Its bounds in the cleanup, read from
  the code: a stopped VM is exit 1 after `current_ip`'s one CLI call (`GPU_HOST_CLI_TIMEOUT`, 120
  s); an unreachable one after ssh-retry's four attempts (`ConnectTimeout=15`, pauses 5, 10, 20 s);
  a session that dies after authenticating by `ServerAliveInterval=30` times 4; a going list by the
  runner's `docker stop -t 30`. Unbounded only on the VM: `vm_locked`'s wait for `run/lock` held by
  another call on the dir (CI's calls are one job at a time), and a docker that hangs.
- The step has no `timeout-minutes`: one would cut the soft stop along with the cancel. In a job
  that ran its course the job's 150-minute limit bounds the cleanup; in a cancelled job the bound is
  GitHub's short grace for `always()` steps after a cancel, not that limit. A limit, if one is ever
  wanted, goes as `timeout <s>` on the cancel alone; the guard reads past `timeout`.
- A `.tree` holding a random id ("### For Task 13") refuses `prepare`, the first claiming call, exit
  2: the job goes red there, `ship` and `run` are skipped, and it starts no run. The cleanup's
  `cancel` is refused the same way, echoed, and the soft stop is refused only if another party's run
  holds the card.
- The guard adds checks to the plan's list. The cancel is the last command before the first `||` of
  its line, and what follows that `||` is only `echo`, `true` or `:`: the job's shell is `bash -e`,
  and a failed cancel, or `|| exit 1`, `|| false`, `|| { …; exit 1; }` after it, would end the step
  before the soft stop. No `nebius.sh stop` comes before it, since a going run's card lock refuses
  the soft stop. The plan's `${{` check is the literal-id check (the set `run-gpu.sh` takes), and a
  cancel naming another dir is refused by the every-call `--dir` rule: with a branch of its own,
  removing either turned nothing red.
- The cleanup's `if:` comment said the VM had no lock; since Task 10 the soft stop counts the card's
  lock as busy, and the comment says so. So did build-test.md's "Contention is accepted" line, now
  that a hard stop alone can kill another's run.
- `gpu-tests` never pulls or clears, so since the runner (Task 10) `~/peacock/ci/logs/` keeps one
  `list-<stamp>.log` per run, where it kept one `list.log` before. build-test.md's "### Disk" says
  they are safe to remove; nothing prunes them.
- Red before the change: `0 passed; 1 failed`, naming no cancel and no `PEACOCK_TREE_ID`. Each rule
  then went red alone when disabled: the `always()` condition, the `||`, the cancel as the last
  command before it, the stop's order, the literal id, the every-call `--dir`, the containers named,
  and every cancel line read rather than the first. Second round, red before: the seven new refused
  cases (`|| exit 1`, `|| false`, `|| { echo …; exit 1; }`; a step's id over the job's, an inline
  id, `unset`, a `$GITHUB_ENV` line), the first of them `accepted`; with the `||` tail check off the
  three `||` cases are accepted, and with the other-lines check off the four id cases.
- `test_run_gpu.py::test_run_gpu_runs_cis_sequence_on_nebius_gpu` passed as soon as it took
  `PEACOCK_TREE_ID=ci` and the cleanup's `cancel`: the behaviour is Tasks 10 and 11's. Without the
  id on `prepare`, or on `cancel` alone, it fails, exit 2, "another tree's".
- The workflow: parsed with Python's yaml (the job's `env:` is `{'PEACOCK_TREE_ID': 'ci'}`); the
  cleanup's `run:` passes `bash -n`, and under `bash --noprofile --norc -eo pipefail`, the job's
  shell, with a cancel failing 1 and a stop failing 2, both are echoed and the step exits 0.

## Reshaped tests

- `test_install_watch_sources.py::test_install_watch_ships_files_that_exist` took every rsync
  argument under `scripts/` as a source. The transport, `-e`'s value, now names this tree's
  `nebius.sh` too, so the test skips that value; it still requires every shipped source to exist.
- `test_install_watch.py::test_install_watch_keeps_one_profile` held every profile call to `-c`
  naming a file called `config.yaml`. The cut names its copy with `--config`, as
  `config.yaml.XXXXXX` beside the file it becomes, so the test holds each call to `--config`
  naming a file under the test's temp dir; it still requires no call on the source.
- `test_install_watch_refuses_a_key_path_profile_without_a_key_file` is two:
  `test_install_watch_embeds_the_pem_a_key_path_names` and
  `test_install_watch_refuses_a_key_path_naming_no_file`. Such a profile now gets the file its path
  names embedded, and only a path naming no file is refused, before any ssh or rsync.
- `test_provision.py::test_provision_needs_bash_4_4` is `test_the_laptop_scripts_need_bash_4_4`: the
  same checks over `nebius-profiles.sh` too, each script read from the tree rather than the fake
  laptop's clone, which holds only `provision-vm.sh`.
- `test_provision.py::refused` held the laptop's `~/.config/peacock/` to the GPU key pair alone; it
  holds it to that and `nebius.yaml`, which every fake laptop now has. It still requires no ssh and
  nothing new kept.
- `test_provision_readies_a_vm`, `test_provision_normalises_the_env_file` and
  `test_provision_copies_the_named_gpu_key` expected a `GPU_SSH_KEY_FILE` line appended to the VM's
  `peacock.env`. Each now holds the VM's file to the laptop's bytes; the first also holds the VM's
  `nebius.yaml` to mode 600.
- `test_install_watch.py`'s `setup` wrote the default source as `~/.nebius/config.yaml`; the default
  is `~/.config/peacock/nebius.yaml`, so it writes that, with `~/.nebius/config.yaml` at mode 000.
  `test_install_watch_ships_one_profile_only` held every rsync argument clear of `~/.nebius`; it
  holds them clear of the whole HOME, which holds both.
- `test_bootstrap.py::test_bootstrap_fetches_the_registry_with_the_clone_read_only` held the clone's
  bind to its own path (`dst=<clone>`). The clone is now mounted at `/peacock/tree`, so the test
  holds it to `dst=/peacock/tree,readonly` and `-w /peacock/tree`; it still requires the clone
  read-only, the registry at the image's, and the caller's uid.
- `test_run_gpu.py`'s `test_run_passes_the_machines_hostname`, `test_run_uses_the_given_run_image`,
  `test_run_names_a_staged_binary` and `test_run_staged_narrows_by_the_filter` held the device
  run's image to the ref (`h.run_ref`, `GIVEN_RUN`). A run is now given the image's ID, so each holds
  it to `sha256:run1`, the world's ID for that ref; they still require the mode or the binary's
  arguments right after it.
- `test_prepare_readies_the_vm` and `test_prepare_pulls_the_given_images` held the copied
  `libcudf.so` to the ref the fake's `create` was given (`h.run_ref`, `GIVEN_RUN`). The copy is now
  made from the ID read with the labels, so each holds it to that image's ID, `sha256:run1`; they
  still require the copy to be of the image prepare checked.
- `test_prepare_refuses_old_driver` and `test_run_refuses_old_driver` set the world's `driver`;
  the fake `nvidia-smi` answers from `cards`, so each sets one card at that driver, compute
  capability 8.9. The assertions are unchanged.
- `test_run_gpu.py`'s `docker_runs()` (in `gpu_host.py`) meant a `docker run -d`; device runs are now
  attached under the runner, so it means a run with `--gpus`. `test_prepare_pulls_the_given_images`
  found the generator's run as one without `-d`; it is the one without `--gpus`.
- `test_run_refuses_a_run_in_progress` put a running container in the world; that is now a
  container no runner holds, refused naming `cancel` (`test_run_gpu_runs.py`). It starts a held run
  instead and holds `run` and `ship` to exit 2 naming `follow`, still with one device run.
- `test_detached_run_returns_while_running` set `run_state=running`; it holds the run with the
  hold file. It still requires the detach to return naming `status` (now `follow` too) and the run to
  be refused while the container runs, naming `follow` instead of the container.
- `test_status_reports_running_then_exit` changed the world's exit code under a running container;
  the run's code is now set before it is let go, and a second run gives `exited 0`. The log it reads
  is `logs/filter-<stamp>.log`, no longer `logs/filter.log`.
- `test_pull_brings_the_results_the_record_and_the_log_home` read `bench.log` at home; it reads the
  one `bench-<stamp>.log`. Task 12: it held the pulled `calibration/records.tsv` to exist; a record no
  run holds is kept under its run's stamp first, so it holds the one `records-<stamp>.tsv`, the bench
  run's stamp.
- `test_run_gpu_records.py::test_the_vm_refuses_an_empty_tree_id` read `vm_tree_claimed` from
  `run-gpu.sh`; the function is now in `run-gpu-vm.sh`, so it goes through `vm_function`, which reads
  both. The call and the assertions are unchanged.
- `test_a_copied_tree_is_refused_its_record_before_any_vm_call` and
  `test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears` gain `("pull", "--clear")`.
- `test_bench_each.py`'s fake run-gpu.sh: a run's record ends as `records-<stamp>.tsv`, as the runner
  keeps it; `pull` keeps a `records.tsv` no run holds, never replaces a local record that differs, and
  with `--clear` removes each VM record whose copy home is the same; `status` and `follow` answer a
  run left going by a `drop`.
- `test_bench_each_runs_one_process_per_query` held the calls to `run bench, pull` four times; it is
  `pull --clear` first, then `run bench, pull --clear` four times.
- `test_bench_each_passes_dir_to_run_gpu_alone` held every nebius.sh call to name `peacock/other/`:
  those were the record checks over ssh, which are gone. It holds every nebius.sh call to no `--dir`,
  every run-gpu.sh call to `--dir other`, and the results to land in `other`'s VM dir.
- `test_bench_each_moves_each_record_aside` held the VM's and the tree's `records.tsv` absent; it
  holds no record of either kind left on either side. The rest is unchanged.
- `test_bench_each_sets_aside_a_record_a_killed_run_left` also leaves a `records-<stamp>.tsv` of a run
  whose pull never came; both are set aside before the first case, each as attempt 1 of its case.
- `ci_gpu.rs::the_gpu_job_owns_its_dir_and_removes_its_container` is
  `the_gpu_job_owns_its_dir_and_cancels_its_run`. It required a cleanup step running `docker rm -f
  peacock-<dir>` under an `if:` that starts `always()`; it requires `run-gpu.sh … --dir <dir>
  cancel` there, forgiven by `||` and `echo`, `true` or `:` alone, before any stop, and a literal
  `PEACOCK_TREE_ID` in the job's `env:` and on no other line of the job. Its no-`if`, `success()`,
  commented-out and no-cleanup cases now hold the cancel. The `--dir` rules and "any `docker
  rm|kill|stop` names `peacock-<dir>` alone" are kept, with their cases beside the cancel; "a second
  docker rm" is dropped, the "another container" case once no rm is required.
- `test_run_gpu.py::test_run_gpu_runs_cis_sequence_on_nebius_gpu` modelled CI as three calls with no
  tree id. Each call now takes `PEACOCK_TREE_ID=ci`, a fourth, `cancel`, finds the run ended (exit
  0), the VM dir's `.tree` holds `ci`, and no `.run-gpu-tree` is made in the checkout. The dir,
  container and install assertions are unchanged.
- `test_bootstrap.py::test_bootstrap_leaves_the_shared_directory_writable_by_the_group` listed
  `<data dir>/cudf` and its commit's checkout among what bootstrap makes. Nothing makes them now;
  the data set's directory and its `tpch.sf1` remain.
- `test_provision.py::test_provision_refuses_a_missing_nebius_yaml_naming_nebius_profiles` removed
  `nebius.yaml` from a laptop whose `~/.nebius/config.yaml` is at mode 000. Such a laptop is now
  cut from, so the test also removes `~/.nebius/config.yaml`: a missing file with nothing to cut it
  from. The refusal it asserts is unchanged.

## Ported and dropped cases

From `4e785104`'s `test_gpu.py` and `test_gpu_locks.py`, for the runner (Task 10):

- `test_one_device_run_at_a_time_per_host` as `test_one_card_lock_for_every_dir_and_user`: a second
  dir and a second user, whose dir has the first one's name, wait; the fake docker fails a second
  device run on the card, so three green runs show they took turns.
- `test_a_dropped_session_leaves_one_run_that_a_rerun_follows` as
  `test_a_run_outlives_its_session_and_is_followed`: a rerun is refused naming `follow` rather than
  following, and `follow` follows. Its moved-tag half is Task 9's.
- `test_cancel_stops_the_instance_s_run_and_keeps_its_record` as
  `test_cancel_stops_the_run_and_keeps_what_an_ended_run_keeps`; the record half is Task 12's.
- `test_cancel_stops_a_container_its_client_is_still_making`, under its name (world `create_delay`).
- `test_cancel_removes_a_container_a_killed_runner_left`, under its name: a runner really killed, and
  a container that vanishes as `rm` comes (world `vanish_on_rm`).
- `test_cancel_leaves_other_instances_and_users_alone` as `test_cancel_leaves_other_dirs_alone`.
- `test_cancel_ends_a_run_still_waiting_for_the_card`, under its name.
- `test_a_run_that_ends_before_it_is_followed_gives_its_status`, under its name: the run's log and
  its code. Its mismatch half (another mode, environment or changed binaries) is dropped: `follow`
  here takes no mode, so it follows whatever run the dir has.
- `test_a_container_of_the_instance_still_on_the_host_refuses_a_new_run` as
  `test_a_container_of_the_dir_still_on_the_vm_refuses_a_new_run`, naming `cancel`.
- `test_the_shared_files_stay_group_writable` as `test_the_card_lock_is_mode_666`: the one shared
  file here is the lock, and it is open to every user rather than to a group.
- `test_a_run_whose_instance_is_busy_is_neither_started_nor_followed` as
  `test_a_run_waits_while_its_dir_s_run_is_being_started`: `run/lock` is held only around a start,
  so a second call waits for it rather than being refused, and starts once it is let go.
- Dropped, no such design here: the activity hold and its heartbeat
  (`test_a_hold_whose_caller_falls_silent_lets_go`,
  `test_a_command_longer_than_the_hold_s_timeout_keeps_it_by_its_heartbeat`,
  `test_a_gpu_sh_killed_alone_lets_go_of_its_hold`, `test_a_hold_lost_mid_command_is_named`,
  `test_a_hold_that_lapses_while_the_command_works_is_named`,
  `test_a_command_neither_waits_for_nor_leaves_its_heartbeat_s_sleep`,
  `test_the_portable_timeout_says_124_and_passes_the_command_s_own_status`); the ship's rsync servers
  holding the instance (`test_a_run_during_a_ship_is_busy`,
  `test_a_ship_killed_alone_keeps_its_locks_while_the_host_s_rsync_server_writes`,
  `test_a_half_open_ship_server_has_the_hold_s_timeout_and_holds_the_instance_until_it_ends`,
  `test_a_pull_s_server_of_the_instance_holds_no_ship`,
  `test_an_instance_s_hold_waits_on_no_server_of_another_instance`,
  `test_each_of_ship_s_two_servers_holds_the_instance_s_hold`,
  `test_ship_refuses_while_a_run_goes_on_and_changed_binaries_are_not_followed`'s binaries half);
  the lock held by no live run (`test_a_lock_held_without_a_live_unfinished_run_is_busy`: `run/lock`
  is a flock, let go with its holder); the install lock and the group join
  (`test_a_second_install_waits_and_says_so`,
  `test_a_second_user_s_first_prepare_joins_the_group_before_it_holds`); the power cases, which are
  `nebius.sh`'s here.

From `4e785104`'s `test_gpu_records.py`, for one tree per dir (Task 11):

- `test_a_tree_records_its_instance_and_later_commands_take_it` as
  `test_a_tree_records_its_id_and_its_dir_on_the_vm`. The record holds an id and the tree's path,
  not a dir: `--dir` names the dir on each call. So later calls keep one id, in a second dir too. Its
  half that each call names the record on stdout is dropped: there is no recorded name to say.
- `test_a_copied_tree_is_refused_the_record_it_carries` as
  `test_a_copied_tree_is_refused_its_record_before_any_vm_call`, over every verb, `status` and
  `follow` included.
- `test_a_second_tree_under_a_taken_name_is_refused_on_the_host` as
  `test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears`, with a going run of the
  owner's that the other tree's `cancel` and `run` leave alone. Its `pull --clear` is Task 12's; its
  "a refused name is not recorded" is dropped (above, "One tree per VM dir").
- Dropped, no such design here: `test_an_instance_other_than_the_tree_s_record_is_refused_before_any_ssh`
  (the record names no dir); `test_an_instance_and_a_tree_outside_one_never_share_a_dir` and
  `test_an_instance_neither_reads_nor_writes_the_tree_s_record` (no instance mode; a fixed
  `PEACOCK_TREE_ID` is the nearest, and `test_a_fixed_tree_id_owns_its_dir_and_writes_no_record` holds
  it to no file read or written, and to a dir of its own against a tree's id).
- The bench, record and pull cases are Task 12's.

From `4e785104`'s `test_gpu_records.py`, for records and pull (Task 12):

- `test_each_bench_run_keeps_its_own_record`, under its name. Its second half, a local edit with an
  old mtime named exit 1, is the stamp's opposite here: the VM's copy is unchanged since the pull,
  so the edit is kept without a word. Being named once the VM's copy changes is
  `test_pull_never_replaces_a_local_record_that_differs`.
- `test_a_killed_bench_s_record_comes_home`, under its name: the runner killed `-9`, the container it
  left removed by `cancel`, the record home under the run's stamp, not `-interrupted`, and the next
  bench not refused.
- `test_pull_clear_removes_the_host_s_records_that_are_home_and_keeps_the_rest` as
  `test_pull_clear_removes_what_came_home_and_keeps_the_rest`, logs included. A kept record is named
  by pull's line on stderr and the clear's `kept` line on stdout; a removal is counted, not named.
- `test_pull_clear_is_refused_while_the_instance_s_run_goes` as
  `test_pull_clear_is_refused_while_a_run_goes`, naming `follow`; a plain `pull` beside the run added.
- `test_the_host_s_clear_keeps_records_tsv_and_what_did_not_come_home` as
  `test_the_vm_s_clear_keeps_records_tsv_and_what_did_not_come_home`, over `vm_cleared` alone, logs
  included. `records.tsv` stays because no clear lists it, so no `kept records.tsv` line.
  `records.tsv` is not in `test_pull_clear_removes_what_came_home_and_keeps_the_rest`, as the plan
  has it: with no run going, the pull keeps it under the last run's stamp before any clear, and with
  a run going the clear is refused, so only the VM function meets one.
- `test_pull_brings_the_results_home_and_keeps_a_newer_local_file` as
  `test_pull_brings_the_results_home_and_keeps_an_edit_made_here`, reshaped to the stamp.
- Dropped: `test_the_host_s_clear_is_refused_while_the_instance_s_run_goes`. The refusal is the
  pull's, under `run/lock`, before any transfer, and the list of what may be cleared is taken under
  the same lock. A run started after that writes only names the list never held (its own log,
  `records.tsv`), and `vm_cleared` removes only listed files whose copy home has the same sha256, so
  a clear beside it takes nothing of it. The case of a run being started as the pull comes is
  `test_pull_clear_is_refused_by_a_run_that_starts_while_it_waits`.
- Dropped with `machine_cudf_source`: `test_bootstrap_keeps_one_cudf_checkout_a_commit` and
  `test_bootstrap_refuses_a_cudf_commit_it_cannot_fetch`, which pinned the machine's sparse checkout.
  What still matters is kept for the clone's own submodule: the rerun with the origin gone is in
  `test_bootstrap_checks_out_the_clone_s_third_party_cudf`, and a commit it cannot fetch, exit 2
  naming it, is `test_a_cudf_checkout_that_fails_is_named_after_every_other_step`. That one moves
  the origin away rather than pinning an unknown commit, and the refusal now comes after sf1, so
  "no docker call" is replaced by "sf1 was made, the tree left as it was". The fake cuDF origin's
  `README.md`, there to show the sparse set was kept, is dropped.

## The table, run as written

On dev, 2026-11-11, from this worktree at `60e99496` plus this change, in Task 2's image
`peacockdb-build:cudf-25.02-in-525f460c6dd5`. Dev has 12 cores and 14 GiB, so every line ran with
`RUST_TEST_THREADS=2` in the environment (*Memory constraints*), and `XDG_CACHE_HOME` named a
directory on the build disk, keeping ccache and the registry off the home disk. "in image" is
`scripts/build-test.sh -- bash -c '<the line>'`, run right after the host's. Wall times in seconds.

| line | host: exit, wall | in image: exit, wall |
|---|---|---|
| `scripts/build-test.sh --rust-only --build` | 0, 1087 (cold) | 0, 2 |
| `scripts/build-test.sh -- cargo test --features rust-only -p peacockdb-core --test test_cost_model -- every_node_kind_is_in_exactly_one_cost_category` | 0, 1 | 0, 1 |
| `scripts/build-test.sh --rust-only --run` | 0, 534 | 0, 531 |
| `PCK_TEST_FILTER=tpch_q6_ scripts/build-test.sh --rust-only --run` | 0, 2 (10 tests) | 0, 2 |
| `scripts/build-test.sh --build` | 0, 1635 (cold) | 0, 6 |
| `scripts/build-test.sh -- cargo test -p peacockdb-ffi --test test_ffi -- test_version_is_nonempty` | 127, 0; after the fix 0, 0 | 127, 1; after the fix 0, 1 |
| `scripts/build-test.sh -- ctest --test-dir cpp/build -R peacock_cpu_tests` (the table's first C++ line, the whole cpu suite) | 0, 1 | 0, 1 |
| `scripts/build-test.sh -- cpp/build/peacock_cpu_tests --gtest_filter=PeacockGpu.Version` (2026-11-12, after the loader-path fix; 1 test) | 0, 1 | 0, 1 |
| `scripts/build-test.sh --run` | 0, 528 | 0, 527 |
| `scripts/build-test.sh --gpu --build` | 0, 1994 (cold cargo; ninja had no work after the cpu build) | 0, 18 |
| `rm -rf cpp/build cpp/install && scripts/build-test.sh --cudf 26.02 --build` | 2026-11-11, before the image existed: `scripts/build-test.sh --cudf 26.02 --build` alone exits 2 at the image lookup, naming `image.sh build build 26.02` and the `images.yml` dispatch. 2026-11-12, at `aace33f5`: 0, 1686 (*The branch's images*, below) | `scripts/build-test.sh --cudf 26.02 --build` alone: 2, 1, refused (below) |

- The 26.02 row waited for the branch's 26.02 build image; until then its `rm` was not run, so the
  tree stayed on 25.02 for every other row.
- **The FFI test line, exit 127.** `-- <command>` ran the command before the
  `LD_LIBRARY_PATH=/opt/cudf/lib` export, which only `--run` made: `test_ffi` failed to load
  `libcudf.so`. `ctest` was not affected; its binaries carry the path. The export now comes before
  the command's `exec`, once, a caller's value after it. Red before:
  `test_a_command_has_cudf_s_libraries_on_the_loader_path` (an empty loader path in the command).
  `cargo test` adds the FFI's `OUT_DIR` itself, so `/opt/cudf/lib` is all a command needs.
- **`--cudf` in a shell in the image.** In the 25.02 image, with no `cpp/build`,
  `--cudf 26.02 --build` stamped `cpp/build` 26.02 and began a configure with gcc-12 against the
  image's 25.02 `/opt/cudf`, in `target-img-25.02` (measured, before the fix; the configure then
  failed on the missing submodule below). Inside the image a `--cudf` other than `PEACOCK_CUDF`,
  which the outer run sets, is now refused before anything, naming the host. Red before:
  `test_a_shell_in_the_image_refuses_another_cudf` (exit 0, the build run).
- **`third_party/cudf`.** This worktree had none: a worktree gets no submodule checkout. The cpu
  build stamped `cpp/build`, then cmake failed on `third_party/cudf/cmake/rapids_config.cmake`,
  exit 1. The image prefetches only what its own configure fetched; the tree's submodule is
  still read. A cpu or gpu shape without that file is now refused, exit 2, before any docker call,
  naming `git submodule update --init --depth 1 third_party/cudf`. That command was checked in a
  throwaway clone: 13 s, 77 MB. Red before:
  `test_a_cpp_build_without_third_party_cudf_is_refused` (exit 0, nothing refused). For the runs the
  worktree's `third_party/cudf` was filled with that checkout's files, leaving git's state alone.
- The worktree had no `tpch.sf1` or `tpcds.sf1`; copies from another clone of the same
  `generate_testdata.sh` were put in for the `--run` lines and removed after.
- Disk: a cold `--rust-only --build` leaves 14 GB in `target-img-25.02`, a cold cpu build 11 GB,
  `--gpu --build` 12 GB of debug and its release build beside them, and `cpp/install` 4.9 GB. The
  build disk had 22 GB free, so the target dir was removed between the rust-only, cpu and gpu
  rows, and the debug incremental cache during the gpu build's release half. Each row's in-image
  run followed its host run on the same target dir.
- The rust-only rows ran before the loader-path fix and were not run again (their target dir was
  gone, and its rebuild did not fit beside the gpu build). The cpu `--run` after the fix runs the
  same targets without the feature, green.
- Counts: `--rust-only --run` 66 + 3 + 2451 + 41 + `--lib` 1209 (2 ignored); `--run` adds
  `ctest -L cpu` (1) and `test_ffi` 4, and `--lib` 1214; the filter `tpch_q6_` ran 10 cases of
  `test_cpu_corpus` and none elsewhere.
- The C++ row was first `-- ctest --test-dir cpp/build -R peacock_cpu_tests`. Each C++ suite is one
  ctest entry for one gtest binary, so that ran all of `peacock_cpu_tests`. One test is the binary
  with `--gtest_filter`.
- `PEACOCK_CUDF` inside the image is set by the outer half only; the image has no ENV for it. A
  `docker run` of its own (the CPU-VM build) names none, and the first form of the `--cudf` guard
  refused every such run as "cuDF unknown". Only a named version that differs from `--cudf` is
  refused now. Red before: `test_inside_with_no_image_version_named_any_cudf_goes_through`. The
  CPU-VM build's recipe in build-test.md now passes `-e PEACOCK_CUDF=<v>` so the guard holds there.
- The command exemption from the `third_party/cudf` refusal is held by
  `test_a_cpp_build_without_third_party_cudf_is_refused`: `-- true` inside and outside, and
  `--rust-only --build` outside, go through. With the exemption deleted it fails.
- `bootstrap-vm.sh` checks out the clone's `third_party/cudf`, under a lock (where and when: the
  second round, below). Red before: `test_bootstrap_checks_out_the_clone_s_third_party_cudf` (no `rapids_config.cmake` in the
  clone). Without the lock, `test_two_bootstraps_at_once_generate_once_and_fetch_in_turn` failed 2
  runs in 3 on `could not lock config file .git/config`; with it, 3 in 3 passed. The fake machine's
  `~/.gitconfig` allows git's `file` transport, as its cuDF origin is a `file://` repository, which
  git refuses a submodule clone from by default.
- Second round, 2026-11-12. `bootstrap-vm.sh` checks out the submodule last, after
  `machine_data_set`: nothing before it reads the submodule's files (the image inputs read the
  index's gitlink). Red before: `test_a_cudf_checkout_that_fails_is_named_after_every_other_step`
  (no sf1). The lock lies in `git rev-parse --git-common-dir`, so a clone whose `.git` is a file
  works. Red before: `test_a_clone_whose_git_dir_lies_elsewhere_gets_its_third_party_cudf`
  (`Not a directory`, exit 1). `machine_cudf_source` and `CUDF_SOURCE` are gone;
  `machine_cudf_gitlink` stays for the refusal's message.
- Pinned by mutation, each with its line deleted and the file then restored: no `|| die`, and the
  failure test sees git's exit 1; no `flock`, and
  `test_the_cudf_checkout_waits_for_the_clone_s_lock` fails (it holds the lock with `fcntl.flock`,
  waits for bootstrap's `flock` child to hold an fd on that file, and checks `third_party/cudf` is
  still empty), as does the two-runs test this time; no `--depth 1`, and the shallow assertion in
  `test_bootstrap_checks_out_the_clone_s_third_party_cudf` fails.

## The branch's images

`images.yml`, dispatched on this branch at `aace33f5`, run `43258417105`: all five jobs green. Each
image is pushed under its inputs tag and the commit's tag:

| kind | cuDF | inputs tag | digest |
|---|---|---|---|
| build | 25.02 | `cudf-25.02-in-525f460c6dd5` | `sha256:9bfc69f3ef11…` |
| build | 25.10 | `cudf-25.10-in-9cac897c5671` | `sha256:8a57556a08a0…` |
| build | 26.02 | `cudf-26.02-in-722a431db732` | `sha256:75c59e21a0d0…` |
| run | 25.02 | `cudf-25.02-in-febbd8e327d9` | `sha256:0a00b74d74cf…` |
| run | 26.02 | `cudf-26.02-in-355b2cace775` | `sha256:6e4b6602781c…` |

- `:cudf-<v>` did not move: it still names master's `b50d9b2a` images of 2026-11-01, as only a
  dispatch on the default branch moves it. 25.10 is build-only (CI's second leg) and has no
  `:cudf-25.10`.
- The table's 26.02 row, on dev, 2026-11-12, from this worktree at `aace33f5` plus Task 14a's
  scripts: `rm -rf cpp/build cpp/install && scripts/build-test.sh --cudf 26.02 --build`, exit 0 in
  1686 s, `cpp/build` stamped 26.02. The published `cudf-26.02-in-722a431db732` could not be pulled
  to dev: its 4.3 GB layer was cut by an unexpected EOF about every ten minutes, and docker gave up
  after five retries (exit 125, 2823 s). The same inputs tag was built on dev with
  `scripts/env/image.sh build build 26.02` (exit 0, 1710 s) and used instead. `cpp/build`,
  `cpp/install` and `target-img-26.02` were removed after.
- CI green on the branch waits for the pipeline dispatch of Task 16.

## Task 14a

- Red before the code, each on the old refusal (`no …/nebius.yaml: make it with
  scripts/env/nebius-profiles.sh`): `test_a_missing_config_is_cut_from_the_cli_s_own_then_used` and
  `test_a_cut_that_fails_is_refused_naming_the_profile` (`test_nebius.py`, exit 1);
  `test_a_missing_nebius_yaml_is_cut_before_anything_is_written` and
  `test_a_nebius_yaml_cut_that_fails_is_refused_before_any_ssh` (`test_provision.py`, exit 2).
  `test_no_cli_config_refuses_a_missing_file_as_before`, `test_a_named_config_is_never_cut` and
  `test_an_existing_config_is_not_cut_again` pass on the code before, kept as guards. Pinned by
  mutation, the file restored after each: with `nebius.sh`'s `GPU_HOST_NEBIUS_CONFIG` condition
  dropped, the named-config test fails (a `nebius.yaml` made); with the function's exists check
  dropped, the not-again test fails.
- One function, `peacock_nebius_config_cut` in `peacock-config.sh`, for both callers. It runs
  `nebius-profiles.sh` beside the library (`${BASH_SOURCE[0]%/*}/../env/`) as a child, its stdout
  to stderr, and returns 2 on any failure; the child has said why. `nebius.sh` turns that into 1;
  in `provision-vm.sh` it is a statement, so `set -e` exits 2.
- `nebius.sh` calls it only when `GPU_HOST_NEBIUS_CONFIG` is unset or empty, the test
  `NEBIUS_CONFIG` is chosen by. `provision-vm.sh` reads no such variable: its file is always the
  default.
- The cut finds its CLI as `nebius-profiles.sh` does, PATH then `~/.nebius/bin`, never `NEBIUS`:
  the script is used as it is. In `test_nebius.py`, `NEBIUS` stays the world's fake and the cut's
  CLI is `nebius_cli.py`'s, first on PATH. A caller setting `NEBIUS` to a CLI off PATH, with no
  `GPU_HOST_NEBIUS_CONFIG`, gets a cut from PATH's CLI, or its no-CLI refusal.
- In `nebius.sh` the cut sits after the verb check, so a usage error stays 64, and before the
  CLI lookup. The header grew a line; the usage range is now `2,32p`, which
  `test_an_unknown_verb_prints_the_whole_usage` holds.
- In `provision-vm.sh` the cut is the last check but one, after the env file, git identity, GPU
  key and CLI, before `peacock_nebius_config_checked` and the PAT. A later refusal (a token gh
  cannot use) keeps the file it made, as a hand run would.
- The fake laptop's clone (`laptop.py`'s `LAPTOP_FILES`) holds `nebius-profiles.sh` too.
- A HUP to the process group while bash waited in a command substitution (`profile list`,
  `config get`) skipped the EXIT trap and left `nebius-cut.*`, the whole copy, behind.
  `test_a_signal_mid_cut_leaves_only_what_was_there` now signals at `update`, `list` and `get`:
  red in 3 runs of 3 before (HUP to the group, at `get` twice and `list` once), green in 3 of 3
  after `nebius-profiles.sh` gained a trap per HUP, INT and TERM. With them a signal to bash alone
  waits for the running CLI call, so no CLI outlives the dir.
- Those traps first ended with a plain `exit 129/130/143`. A bash caller reads a plain 130 as an
  interrupt the child handled and goes on: Ctrl-C mid-cut made `nebius.sh` exit 1 and a loop
  around it run its next pass, which cut again. Each trap now removes the dir, resets itself and
  EXIT, and kills its own process with the same signal. `cut_signalled` asserts the cut dies of
  the signal in all 15 cases: red before (`(SIGTERM, True, 'update', 143, '')`), green in 2 runs
  of 2 after. `test_ctrl_c_mid_cut_stops_a_bash_loop_around_nebius_sh`, a two-pass loop SIGINTed
  at `profile list`: red before (exit 0, the second pass cut the file), green after.
- `test_a_usage_error_cuts_nothing` (`nebius.sh bogus`) and `test_a_bad_argument_cuts_nothing`
  (`provision-vm.sh … --bogus`) pass on the code and are guards: with the cut moved above
  `verb=` in `nebius.sh`, or above the argument parsing in `provision-vm.sh`, each fails with a
  `nebius.yaml` made.
- Fixtures that made a temp dir and never removed it: `fakes.py`'s `World`,
  `test_nebius_profiles.py`'s `Laptop`, `test_idle_watch.py`'s and `test_idle_watch_kinds.py`'s
  `World`, and the latter's `test_load_install_reads_both_kinds`. Each now holds a
  `TemporaryDirectory`, removed with the object. `path_without`'s links always lie inside a
  fixture's own temp dir.
- `test_bootstrap.py` left two empty `tmp.*` files in `/tmp` per run: its `bootstrap()` helper's
  curl fails, and `bootstrap-vm.sh`'s gh block, which has no EXIT trap unlike the nebius one, exits
  under `set -e` with its `mktemp` tarball in place. `machine.py`'s `Machine` env now names a
  `TMPDIR` inside the machine, and the gh block has the nebius block's EXIT trap, reset after the
  install. `test_bootstrap_leaves_no_download_when_the_gh_install_fails`: a failed download red
  before (a `tmp.*` in the machine's tmpdir), and a tarball of another sha red alone with the trap
  removed; both green after. The script's other download, Claude Code's installer, is piped to
  bash and leaves no file.
- `test_a_bad_argument_cuts_nothing` also runs `provision-vm.sh` with no `--env` and no
  `peacock.env`: exit 2 naming it, nothing cut. With the cut moved to the top of the
  `if [ -n "$target" ]` block it fails with a `nebius.yaml` made.
- editor-eu and eu-west1 must be service-account profiles. `peacock_nebius_service_accounts_checked`
  asks `config get` for `auth-type`, `service-account-id` and `public-key-id` and takes a profile
  only when `auth-type` is exactly `service account` and both ids are set; a federation login, a
  token, an empty or unknown `auth-type` are refused, naming the profile and "The human's one-time
  setup". On the real CLI, measured by hand and not by these tests, a service account's profile
  answers `auth-type` with `service account`, and a key it lacks prints an empty line, exit 0. The
  `federation-endpoint` check of the first version is dropped: `auth-type` is the CLI's own answer
  of who the profile acts as. It never asks `private-key`, whose answer is the key, and it turns
  tracing off for itself, so `bash -x` shows none of the answers.
- `nebius-profiles.sh` now cuts to `cut.yaml` in its private dir, runs that check there, and only
  then moves the file into place; `peacock_nebius_cut` is unchanged, so install-watch's
  one-profile cut accepts what it did. `peacock_nebius_config_checked` ends with the same check.
- Red before the code, each a federation editor-eu taken with exit 0:
  `test_a_federation_profile_is_refused_naming_the_recipe` (`test_nebius_profiles.py`, for each of
  the two names), `test_a_federation_profile_is_refused_by_the_first_use_cut` (`test_nebius.py`,
  exit 1, nothing left) and `test_provision_refuses_a_federation_profile_naming_the_recipe`
  (`test_provision.py`, exit 2, no ssh). `test_only_a_service_account_with_both_ids_is_taken`
  refuses a token's profile, an empty and a `federation` `auth-type` with both ids, and a service
  account lacking either id; red on the endpoint-only rule (the empty `auth-type` taken). Each part
  of the rule pinned by mutation, the line restored after each: without the `auth-type` test the
  empty-`auth-type` case fails, without `service-account-id` the case lacking it, without
  `public-key-id` the case lacking that. `nebius_cli.py`'s `nebius_config` writes a kind
  (`service account`, `federation`, `token`) with its `auth-type`, an `auth_type` override and keys
  to leave out; `fakes.py`'s world answers `auth-type` as a service account.
- `peacock_nebius_cut` turns tracing off for itself too (`local -; set +x`): under `bash -x`
  it traced eu-west1's key path, endpoint and ids. `test_no_cli_answer_is_traced_under_bash_x`
  now holds the trace to none of those answers, not only to no key material: red before (the key
  path in the trace), green after.

## nebius-gpu by hand

### The run

On dev, 2026-11-13, from this worktree at `8c1bf1b8`, against nebius-gpu
(`computeinstance-e00tnrse7ayntnzcyt`, L40S, driver 580.173.02), with
`G="scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt"`. Wall times in
seconds.

| step | command | exit, wall |
|---|---|---|
| 1 | `scripts/nebius.sh status` | 0, 3: `STOPPED`; it cut `~/.config/peacock/nebius.yaml`, editor-eu (active) and eu-west1, 0 other profiles left out, mode 600 |
| 1 | `scripts/build-test.sh --gpu --build --no-release-bench` | 0, 39 (warm target dir and ccache) |
| 2 | `scripts/nebius.sh start` | 75, 996: the start operation failed `NotEnoughResources` after 318 s; `nebius.sh` waited out its 900 s |
| 2 | `scripts/nebius.sh start`, again | 0, 304 |
| 2 | `$G prepare --no-sf40` | 1, 245: the VM's disk full (below) |
| 2 | `$G prepare --no-sf40` | 255, 193: the VM preempted (below) |
| 2 | `scripts/nebius.sh start` | 0, 62 |
| 2 | `$G prepare --no-sf40` | 0, 535: both images pulled, sf1 generated |
| 2 | `$G ship` | 0, 27: four streams |
| 2 | `$G run list`, its local process killed 25 s in | the run went on, on the VM |
| 2 | `$G status` | 5: `running` and the log's tail |
| 2 | `$G follow`, ended by `timeout 30` | the log from its first line, `==> peacock_cpu_tests` |
| 2 | `$G cancel` | 0: `cancelled the list run of vmspec started 20261112T231038.910Z (exit 143)` |
| 2 | `$G status` | 1: `exited 143`, `run-gpu.sh: cancelled` last in the log |
| 3 | `PCK_TEST_FILTER=tpch_q6_ $G run filter` | 0, 38: `run-tests.sh filter: green`, `test_gpu_corpus` 5 passed |
| 3 | `$G pull --clear` | 0, 26: two logs home, the VM's `logs/` empty after |
| 3 | `scripts/nebius.sh stop` | 0, 44: `STOPPED` |

- The disk: 12 GB of 96 GB free, and the build image is 10 GB unpacked. An exited container of
  another dir and three images of older inputs, nothing else, were removed: 31 GB free after.
- The preemption: nebius-gpu is preemptible, stopped on preemption. The VM's journal has `Power
  key pressed short` mid-pull, and the API a `Stop Instance` operation a minute later; the idle
  watcher's last tick said `keep`.
- Both failures were named "a registry or network error … so try again"; the findings below fixed
  that.
- VM time: 01:28 to 01:48 MSK (preempted) and 01:59 to 02:14 MSK.
- No `.tree` was in `~/peacock/ci/` or the other dirs on the VM.
- Step 4: `gh workflow run pipeline.yml --ref ENS-vm-setup`, run `43330240922` on `8c1bf1b8`:
  every job green but `gpu-tests`, whose start found no L40S in its 30 minutes (two starts of 900
  s, exit 75); prepare, ship and the list were skipped, and the cleanup's `cancel` met the stopped
  VM (`not RUNNING; refusing to ssh`, `cancel failed (rc=1)`) as written. Again, run
  `43397318671` on `c6a807f4`: the console's own start of nebius-gpu was refused for high demand
  too, and the run was cancelled 17 minutes into `gpu-tests`' start; the cleanup ran and the VM
  stayed `STOPPED`. `cudf 25.02` failed in `test_results_name_version_threads_host`: the temp
  dir's name held `__`, which the results line's `tr -s ' \t' '_'` squeezes to `_`, so `db=` and
  the test's path differ. `gpu-tests` stays open until an L40S is free.

### Findings fixed in the scripts

- Two pull failures on nebius-gpu were both named "a registry or network error (above), so try
  again", from `machine_image_checked` in `scripts/lib/machine.sh`, which every image pull goes
  through: `run-gpu.sh prepare`'s build and run images, `check-build-image.sh` and
  `bootstrap-vm.sh`. One fix covers all of them.
- A full disk: docker said `failed to register layer: write …: no space left on device`, exit 1.
  Now refused, exit 2, as the disk being full, its space or its inodes (the same error), naming
  `scripts/nebius.sh <options> ssh 'docker system df; df -h; df -i'` where docker is a VM's, and
  the three commands alone in bootstrap-vm.sh, which runs on the machine itself: 2 is run-gpu.sh's
  code for a step refused until something is fixed, as an unpublished image is, and a retry fails
  the same way. `machine.py`'s `NO_SPACE` is that message.
- The VM lost mid-pull: nebius-gpu is preemptible, and the ssh carrying the pull ended `Connection
  to <ip> closed by remote host.`, exit 255. Any ssh can drop so, not only a pull's: sf1's
  generation in `prepare` runs for minutes. So the hint is given once, in the VM-call path:
  run-gpu.sh's `vm()`, which every `on_vm` call goes through, and check-build-image.sh's
  `docker()`. On 255 each says the ssh dropped, then `machine_vm_lost`: the VM may have been
  stopped; if `scripts/nebius.sh <options> status` does not say RUNNING, `start`, then the step
  again. The pull adds only "cannot pull <ref>: the ssh to the VM dropped (above)". The exit stays
  255, ssh's own, which both exit tables list as an ssh to the VM that dropped. Neither script
  starts or stops a VM. Not covered: ship's and pull's rsyncs, whose transport is retried by
  `sync_retry` and reports on its own.
- A VM stopped before the pull's ssh begins: `nebius.sh ssh` refuses, exit 1, `<id> is not RUNNING;
  refusing to ssh`, which also read as a registry error. Its exit cannot be told from a remote
  command's 1, but the pull's stderr is captured, so `machine_image_checked` finds that line there
  and names the VM not RUNNING with the same hint; the exit stays 1. Elsewhere nebius.sh's own line
  is the whole message, and nothing follows it that misreads it.
- Red before, each with the old message: `test_prepare_names_a_full_disk_on_the_vm` (exit 1, not 2)
  and `test_prepare_names_a_vm_lost_mid_pull` (`test_run_gpu.py`, each over the build image's
  pull, then the run image's; red at the first),
  `test_check_build_image_names_a_full_disk_on_the_vm` and
  `test_check_build_image_names_a_vm_lost_mid_pull` (`test_check_build_image.py`). The dropped ssh
  is the fake nebius.sh's `FAKE_NEBIUS_RC=255` on the pull's call alone. Green after, with the
  unpublished and other-failure cases unchanged.
- Then, red before the shared hint: `test_a_dropped_ssh_at_any_step_names_the_stopped_vm` (255 on
  `vm_data_set`, sf1's step: only "refused by the test"),
  `test_prepare_names_a_vm_stopped_before_the_pull` (the not-RUNNING line, then "a registry or
  network error"), and in `test_check_build_image.py`
  `test_check_build_image_names_a_stopped_vm_at_the_label_read_and_before_the_pull` (255 at the
  label's `docker image inspect`, then not RUNNING at the pull); the disk tests also failed on the
  `nebius.sh … ssh 'docker system df; df -h; df -i'` line. The fake nebius.sh's
  `FAKE_NEBIUS_RC_TEXT` gives its refusal nebius.sh's not-RUNNING line. Green after.
- A remote 255 read as a dropped ssh: ssh passes the command's status through, and nvidia-smi's
  "other error" is 255, so the driver query on every `prepare` and `run` would print the VM hint
  before its own failure. `vm()` now sends its command with a trailer, `s=$?; [ "$s" -ne 255 ] ||
  s=254; exit "$s"`, after a newline so a trailing comment cannot swallow it; `on_vm` goes through
  `vm()`, so 255 from `vm()` is ssh's alone. The driver query names nvidia-smi's 255 as
  `exit 254 (255 on the VM)`, and run-gpu.sh's exit table lists 254. Nothing read a remote 255:
  `vm_follow` maps a run's exit to 0, 2 or 1, and `following()`'s 255 means the ssh.
  check-build-image.sh's `docker()` is left as it is: it runs `docker pull` and
  `docker image inspect` alone, whose failures are 1 or 125, never 255.
  `test_a_driver_query_that_exits_255_is_not_a_dropped_ssh` (`test_run_gpu_vm.py`): red before
  (`the ssh to the VM dropped` first), green after.
- bootstrap-vm.sh's full-disk branch had no test:
  `test_bootstrap_names_a_full_disk_on_its_own_machine` asserts exit 2, "this machine's disk is
  full", `df -i` and no nebius.sh. Red with the branch
  removed (exit 1, "so try again"), green with it.
- machine.sh reads nebius.sh's exact words, "is not RUNNING; refusing to ssh". `test_nebius.py`'s
  `test_ssh_refused_when_api_does_not_vouch` now asserts them, naming machine.sh as their reader:
  red with the words changed in nebius.sh, green as they are.
- A dropped ssh while following said "run this step again", which an attached `run` refuses while
  its run goes on. `machine_vm_lost` takes the next step; `following()` sets `VM_NEXT` for its own
  call alone, in the background group it starts, to `follow it again: scripts/env/run-gpu.sh …
  --dir <i> follow`. `test_a_dropped_ssh_names_follow` asserts it and no "run this step again": red
  before, green after.

## A fresh VM

### The run

On dev, 2026-11-13, from this worktree, against a verda CPU VM: Ubuntu 26.04.1, 4 vCPU, 15 GiB,
a spot instance, with only Claude Code installed and logged in for its login user, docker and git
present. The developer's user is a new one, made by the run. Wall times in seconds.
`P="scripts/env/provision-vm.sh <login>@<address> --user <user> --ref ENS-vm-setup </dev/null"`.

| step | at | command | exit, wall |
|---|---|---|---|
| 1 | `3910b3c3` | `$P` | 2, 32: superpowers is not 7.0.0 at `bb92a777`; the marketplace installed 7.0.2 at `22f0b463` |
| 1 | `8a0d54e9` | `$P` | 1, 2: `install: No such file or directory` (uutils, below) |
| 1 | `7b7c9dad` | `$P` | 2, 8: the superpowers pin again, read from the VM's clone, still at `c6a807f4` (below) |
| 1 | `7b7c9dad` | `git pull --ff-only` in the VM's clone, by hand, then `$P` | 0, 291: `Claude Code is not logged in on <user>@<address>: run ssh -t …` |
| 2 | `7b7c9dad` | `$P` | 0, 22: every step skipped, the image `up to date` |
| 3 | `7b7c9dad` | as the developer: `scripts/nebius.sh --instance-id computeinstance-e00tnrse7ayntnzcyt --profile editor-eu status` | 0, under 1: `STOPPED` |
| 3 | `7b7c9dad` | as the developer: `scripts/build-test.sh --rust-only --build --run` | 1, 921: the disk full mid-build (below) |
| 3 | `7b7c9dad` | the same, the disk grown | 1, 50: every corpus test `register the tables: … NotFound` (below) |
| 1 | `f46a8e71` | `git pull --ff-only` in the VM's clone, by hand, then `$P` | 0, 19: the clone's `testdata/{tpch,tpcds}.sf1` linked to the machine's set |
| 3 | `f46a8e71` | as the developer: `scripts/build-test.sh --rust-only --build --run` | 0, 288: `build-test.sh rust-only --run: green` |

- Step 1 ends in the command that logs Claude Code in, not in a login: the run has no terminal.
- Step 3's `status` names nebius-gpu with editor-eu: nebius-cpu128 is deleted.
- The green run: `test_corpus_goldens` 66, `test_cost_model` 3, `test_cpu_corpus` 2451 (197 s),
  `test_golden_format` 41, `peacockdb-core --lib` 1209 and 2 ignored.
- The disk: 48 GB, 15 GB of it the OS's before the run. The build image is 15.5 GB unpacked and
  `target-img-25.02` reached 12 GB, incremental, when rustc failed `No space left on device`. The
  disk was grown to 100 GB (`growpart /dev/vda 1`, `resize2fs /dev/vda1`); after the green run
  `target-img-25.02` holds 14 GB and the root 45 GB of 96.

### A second developer: the login user

The same VM, the login user provisioned as the developer too: `--user <login>`, where Claude Code
2.1.304 was already installed and logged in, with superpowers 7.0.0 at `bb92a777` from an earlier
install.

| at | command | exit, wall |
|---|---|---|
| `f46a8e71` | `$P` with `--user <login>` | 2, 18: `plugin install` said `already installed` and changed nothing; the refusal said to bump the pins, which already matched the marketplace (below) |
| `f46a8e71` and the plugin change | the same | 0, 42: `marketplace update`, then `superpowers updated from 7.0.0 to 7.0.2`; `Claude Code is logged in on <login>@<address>`; the machine's sf1 set reused, the clone's links made |
| `661e8e61` | the same, the plugin change committed | 0, 20: every step skipped; `Claude Code is logged in on <login>@<address>` |
| | as the login user: `claude -p "Reply with the single word OK"` | 0, 3: `OK` |
| | both developers at once: `scripts/build-test.sh --rust-only --build --run` | 0, 596 (built target dir, tests only) and 0, 1541 (a cold build, then tests) |

- At once: one build image and one sf1 set, the set's lock taken by each; each developer's own
  clone, registry and `target-img-25.02` (14 GB each). Two containers at the peak, 5.6 GB of
  15.6 GB in use; the root 58 GB of 96 after. `test_cpu_corpus` took 417 s beside the cold build,
  197 s alone.

### Findings fixed in the scripts

- The superpowers pins: the marketplace had moved on to 7.0.2 at `22f0b463`, the same commit dev
  holds. `docker/pins.env` bumped (`8a0d54e9`); the two pins are the machine's, so no image's
  inputs change.
- uutils' `install`: Ubuntu 26.04's coreutils are uutils 0.10.0, whose `install` refuses a source
  that is not a regular file onto a destination that exists, exit 1, `install: No such file or
  directory`; onto a new file, or from a regular file onto an existing one, it works. The first run
  wrote `/etc/sudoers.d/peacock-<user>`, so every later run failed there. provision-vm.sh's sudoers
  file and run-gpu-vm.sh's `peacock-profiling.conf` now go through a `mktemp` file, removed on every
  path. The fakes' `install` refuses as uutils' does, and each fake VM has a `TMPDIR` of its own,
  empty after a rerun. `test_a_second_provision_rewrites_the_sudoers_file` and
  `test_a_second_prepare_rewrites_the_profiling_conf`: red before, green after; three existing
  rerun tests of `test_provision.py` went red with the fakes alone (`7b7c9dad`).
- sf1 outside the clone: bootstrap-vm.sh generates the set into the machine's data dir, and the
  tests read the clone's `testdata/<ds>`. bootstrap-vm.sh now links each to the machine's set,
  repointing a link into the data dir or a dangling one and keeping, and naming, anything else;
  build-test.sh mounts a link's target read-only where the link points, when it lies outside the
  tree, and refuses a dangling link, exit 2. `testdata/.gitignore`'s `/tpch.sf*` and `/tpcds.sf*`
  lose their trailing slash, so git ignores the links. Four tests in `test_bootstrap.py` and five in
  `test_build_test.py`; each of the mount loop's branches has a test that a mutation of it turns
  red (`f46a8e71`).

- An installed superpowers at another version: `plugin install` leaves an installed plugin as it
  is, so a machine with an earlier pin's plugin never reached the new one, and the refusal blamed
  the pins. bootstrap-vm.sh now refreshes the marketplace first (`marketplace add` when unknown,
  else `marketplace update`), then installs an absent plugin or updates an installed one, counting
  user-scope installs alone; each `claude` step that fails is named, exit 2; the refusals tell an
  install that recorded nothing, an update that changed nothing and a marketplace that gives
  another version apart, and only the last says to bump the pins. A record of another shape is
  refused by name. The fake `claude` models the plugin commands as 2.1.304 runs them, matching the
  run above; ten tests in `test_bootstrap.py`.
### Left as they are

- bootstrap-vm.sh never updates an existing clone, so a pin bumped in git reaches the VM only once
  the clone is pulled; the refusal names `docker/pins.env`, not the clone's copy of it.
- Neither bootstrap-vm.sh nor build-test.sh checks free space before it starts; CI's build-image
  action asks for 25 GB. A machine like this one needs about 50 GB free for the image, the
  incremental target dir and sf1.
