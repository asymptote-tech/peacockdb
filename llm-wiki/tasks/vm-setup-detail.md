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

## Reshaped tests

- `test_install_watch_sources.py::test_install_watch_ships_files_that_exist` took every rsync
  argument under `scripts/` as a source. The transport, `-e`'s value, now names this tree's
  `nebius.sh` too, so the test skips that value; it still requires every shipped source to exist.
