# vm-setup — run detail

Branch `ENS-vm-setup`, forked from `ENS-containers-only` at `7aeec989`; its PR targets
`ENS-containers-only`.

## Progress

- 2026-11-09: spec, plan and board entry committed (`fed95745`); Task 1 committed (`ad19c967`).
- 2026-11-09: Task 2 on dev, below. Dev's docker keeps the branch's 25.02 build and run images
  (inputs tags below) for Tasks 13, 14 and 16.

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
