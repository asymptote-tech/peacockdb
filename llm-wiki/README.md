# Running the ensemble

## Getting started on a VM

A chain runs in a container on a CPU VM, and its GPU steps on a GPU host. You drive both from your
own machine, the laptop. Reference: `build-test.md`, "Containers" and "GPU hosts".

**Ask for these first.**

| what | from |
|---|---|
| write access to `asymptote-tech/peacockdb` | a maintainer of the repository |
| your GitHub login on the repository variable `GPU_ALLOWED_ACTORS`; without it CI's GPU tests fail, and no task reaches `done` | a maintainer of the repository |
| approval of your fine-grained token, where the organization requires it | the organization's owner |
| `peacock.env`: the four values of the read-only storage key (`build-test.md`, "The env file") | whoever administers the cloud account |
| the CPU VM below, with your laptop's ssh key on its login user | whoever administers the cloud account |
| the GPU host below, made with your GPU key's `.pub` (step 1 prints it) as its login key; its address and login user | whoever administers the cloud account |
| for a GPU host the scripts start and stop: its instance id and its provider's CLI config (`build-test.md`, "GPU hosts") | whoever administers the cloud account |
| agreement to your first chain's PR and its CI run, whose GPU job runs on the shared nebius-gpu | a maintainer of the repository |
| republished images, when a commit moves their inputs (`build-test.md`, "Rebuilding and publishing"; needs `write:packages`) | a maintainer of the repository |
| a bump of `docker/pins.env`'s `SUPERPOWERS_VERSION` and `SUPERPOWERS_SHA` on the branch the VM clones, when the marketplace moves on | a maintainer of the repository |
| a Claude account Claude Code logs in with | you |

**The CPU VM.** Ubuntu 24.04, x86_64, 8 vCPU, 32 GB RAM, a disk of 100 GB or more: the build image is
10 GB, and each chain's `target/` takes tens. Its login user has passwordless sudo (`sudo -n true`
succeeds) and your ssh key. It has outbound internet access.

**The GPU host.** x86_64 with apt (Ubuntu 22.04 or 24.04), passwordless sudo, curl, gpg, systemctl
and `nvidia-smi`, and an address the VM reaches over ssh: a nebius host needs a public one.
An NVIDIA card of compute capability 8.0 or later. A driver of at least `DRIVER_MINIMUM_<v>` in
`docker/pins.env`: 535.104.05 for cuDF 25.02, 575.57.08 for 26.02, and 575.51.03 for a card past
capability 9.x. About 100 GB free on the root disk: the two images take 16 GB in Docker's
directory, and sf1 and sf40 take 60 GB under `/var/lib/peacock` (`PEACOCK_DATA_DIR` in
`/etc/environment` moves the data to another disk). `gpu.sh prepare` installs Docker, rsync and
the NVIDIA Container Toolkit itself.

**The laptop.** bash 4.4 or later (on macOS, `brew install bash`); ssh, ssh-keygen and tar; gh,
with no login needed, since it only checks the token you paste; git with `user.name` and
`user.email`, which become your commits' on the VM; a clone of this repository; for a GPU host the
scripts power, the nebius CLI, to stop it before a chain does (step 2). `provision-vm.sh`
checks bash, git's identity, gh when it asks for a token, ssh-keygen until your GPU key exists, and
the env file, before it writes anything. The VM clones and pushes to `asymptote-tech/peacockdb`
(`PEACOCK_REPO`), whatever your clone's origin; a branch you push from the laptop for a chain goes
there too: `origin` when that is your origin, or the remote that names it.

**Until `scripts/env/` is on master**, and only until then, three things differ: the laptop clone is
on `ENS-containers`; step 1 adds `--ref ENS-containers`, since the VM clones master otherwise; and
every chain starts as step 3's first chain does, on a branch of its own off `origin/ENS-containers`.

**The steps**, each from the clone on the laptop. Times are rough first runs. A dropped ssh is
never fatal: run the same command again, and it skips what is done. For `up.sh`, a rerun that
refuses with `chain <chain> is on this machine already (peacock-<you>-<chain> of <you>, running)`
means the chain is up: leave it. One that says `created` instead was cut off between creating the
container and starting it: `down.sh <chain>`, then the same `up.sh` again.

1. `scripts/env/provision-vm.sh <login>@<vm address> --env <the peacock.env you got>`. It prints a
   token recipe: make the token on GitHub and paste it. It prints your GPU key's `.pub`: send it to
   whoever makes your GPU host. It ends in Claude Code's login: open the URL it prints. 20 to 40
   minutes, most of it the 10 GB image and sf1. Worked: `<vm address> (<you>@<vm address>) is the
   default VM for up.sh and down.sh`, printed before the login, and then a rerun, which skips what
   is done, ending in `Claude Code is logged in on <you>@<vm address>`.
   - `<you>` is your Linux user on the VM: your laptop's user name, unless `--user` names another.
     A name Linux refuses, `first.last` say, is refused: pass `--user`. A second developer on the
     same VM runs this step from their own laptop with their own `--user`, once their ssh key is on
     the login user.
   - The VM clones the repository anonymously over HTTPS, before the token is there: that works
     because the repository is public.
   - Two refusals here are a maintainer's to fix (the table above). `superpowers is not <v> at
     <sha> … must be bumped` means the marketplace moved on: once the bump is on the branch the VM
     clones, pull the VM's clone (below) and run step 1 again. `… was built from other inputs than
     <the clone>` means the published build image is stale: once it is republished, run step 1
     again. This is the first check of the build image; nothing checks it sooner, and the run image
     is first checked at the chain's first `prepare`, which refuses it the same way.
   - The VM's address changed: run step 1 again with the new one, and the same `--name` if you gave
     one. It records the VM again, as the default, and skips the rest.
2. Once the GPU host is up: `scripts/env/provision-vm.sh --gpu-host <host> <user>@<gpu address>`,
   where `<host>` is a name you choose for it and `<user>` its login user. Add
   `--provider nebius --instance-id <id> --provider-config <file>` for a host the scripts start and
   stop. Worked: `… hosts/<host>.env names <user>@<gpu address>` and `hosts/ is on <you>@<vm
   address>`. A host made with your `.pub` says `takes the GPU key already`; on one that was not,
   ssh asks for the host's password once.
   - A host the scripts power must be running now, and stays up, and paid for, until something
     stops it. Stop it from the laptop: `scripts/env/gpu.sh <host> stop`, which takes your GPU key
     from `~/.config/peacock/` and needs the nebius CLI. A chain's GPU steps start and stop it after.
   - A host powered by hand whose address changed: run step 2 again with the new one. A powered
     host's address is asked of its provider each time.
3. `scripts/env/up.sh <chain> --gpu-host <host>`, where `<chain>` names a section
   `## Chain <chain> (base: <branch>)` of the board, `llm-wiki/tasks/tasks.md`, whose first task is
   `approved to build`. With one host recorded `--gpu-host` may be left out, and the chain takes that
   one; with several, `up.sh` refuses without it. A chain that ran before:
   `up.sh <chain> --branch <its branch> --gpu-host <host>`. A few minutes; longer when the image or
   the data are new. Worked: `chain <chain> is up: docker exec -it peacock-<you>-<chain> bash; …`.

   **Your first chain is a scratch one**, on a branch of your own that holds its board and its spec
   and is never merged:
   - `git checkout -b <yours> origin/master`;
   - in `llm-wiki/tasks/tasks.md`, a section `## Chain <c> (base: <yours>)`, where `<c>` is a letter
     that no branch's board uses yet (this lists the ones in use:
     `git fetch origin && git grep -h '^## Chain' $(git for-each-ref --format='%(refname:short)' refs/remotes/origin) -- llm-wiki/tasks/tasks.md | sort -u`),
     with one task, `` ### 1. [`<task>.md`](<task>.md) — closes no ticket — state: approved to build ``;
   - its spec, `llm-wiki/tasks/<task>.md`: what to build and why, and how it is shown done, as
     `llm-wiki/tasks/containers.md` is written. Keep the task small enough for one developer pass:
     a change to some code and its test, with one GPU step;
   - commit, push `<yours>` to the project repository, then
     `scripts/env/up.sh <c> --board origin/<yours> --gpu-host <host>`.

   Its PR, against `<yours>`, and the CI run its coordinator dispatches, with a GPU job on the
   shared nebius-gpu, are real: a maintainer of the repository agrees to them first. Afterwards
   close the PR unmerged, `down.sh <c>`, remove its worktree (below), and delete `<yours>` and
   `ENS-<task>` on origin. Later chains are defined on master ("Define a task", below).

   Three things fail late rather than at step 1, which checks only that the token reads the
   repository: a token without Contents or Pull requests write, and one whose approval by the
   organization is pending, fail the chain's first push or PR; a login missing from
   `GPU_ALLOWED_ACTORS` fails CI's `GPU Tests (nebius-gpu)` job, at the task's last state. The
   chain stalls on each, and `<chain>.status` and `<chain>.log` (below) say why.
4. The chain's GPU steps run `scripts/env/gpu.sh <host>` from inside the container. A host the
   scripts power needs nothing from you. One powered by hand must be up whenever the chain needs
   it. Nothing calls you: the chain stalls, and `<chain>.status` (below) says
   `cannot reach <host> … start it by hand`. Start the host, then `down.sh <chain>` and
   `up.sh <chain> --gpu-host <host>`. The first `prepare` pulls both images and 56 GB of sf40: an
   hour or more.

**Operating a chain.** `up.sh` and `down.sh` run from the laptop clone, which forwards them to the
VM, or on the VM as `~/peacockdb/scripts/env/…`. The rest is on the VM (`ssh <you>@<vm address>`):
- Its files are in `~/peacock-instances/<chain>/.claude/ensemble/`: `<chain>.status` says why it
  stopped, `<chain>.log` is everything it said, and `echo stop > <chain>.control` there reaches it
  (`pause` and `rebase` too, as below). `docker logs -f peacock-<you>-<chain>` is the watchdog.
- When the watchdog ends, the container stays `exited`, and `up.sh` refuses the chain until
  `down.sh <chain>`. Then `up.sh <chain> --gpu-host <host>` resumes it in the worktree it kept.
- A reboot stops the container and starts nothing. The worktree, the logins and the data survive:
  `down.sh <chain>`, then `up.sh <chain> --gpu-host <host>`.
- `up.sh` and `down.sh` run the VM's own `~/peacockdb`. Keep it current, hooks off:
  `git -c core.hooksPath=/dev/null -c core.fsmonitor=false -C ~/peacockdb pull --ff-only`.
  That follows the branch the clone is on; once `scripts/env/` is on master, move it there first:
  `git -c core.hooksPath=/dev/null -c core.fsmonitor=false -C ~/peacockdb switch master`.
  When `docker/pins.env` moves Claude Code or gh, run `~/peacockdb/scripts/env/bootstrap-vm.sh`
  again, with the `--ref` the clone is on, if any.
- After the chain's merge: `down.sh <chain>`, then
  `git -c core.hooksPath=/dev/null -C ~/peacockdb worktree remove ~/peacock-instances/<chain>`.

## On your own machine

- **Define a task**: run `claude` here on master and ask the `peacockdb-helper` agent for it.
  It brainstorms with you, writes `tasks/<task>.md` and a board entry at `new`, and commits.
  Move the entry to `approved to build` when you are happy with the spec.
- **Run a chain**: chains are lettered `A`, `B`, `C`, …; each runs in a workspace — a linked
  worktree named `alpha`, `beta`, `gamma`, … — and workspaces are reused from chain to chain.
  Check out the chain's first branch in a free workspace, then name the chain to the watchdog:

      git worktree add ../peacockdb-alpha <first-task-branch>      # once per workspace
      cd ../peacockdb-alpha && git checkout <first-task-branch>    # on reuse
      ../peacockdb/scripts/ensemble-watchdog.sh <chain>

  It runs unattended until every task reaches `done` — PR open, CI green — or until it stalls.
  On a VM, `scripts/env/up.sh <chain>` does all of it in a container: "Getting started on a VM", above.
- **Watch it, or steer it, from inside that workspace**, where its own `.claude/ensemble/`
  lives — status, control and log are per-workspace, never here. `<chain>.status` says why it
  stopped, `<chain>.log` is everything it said, and `echo rebase > .claude/ensemble/<chain>.control`
  reaches it (`pause` and `stop` too). A coordinator reads only its own branch, so `rebase` is
  also how tasks and board edits you made on master get to it. Add `--interactive` to type at it
  instead; unattended is the default.
  A run that hits a usage limit is not a stall: the watchdog waits it out and resumes by
  itself, backing off 5, 15, 30 then 60 minutes.
- **Merging is yours**: ask the helper. It merges oldest-first and archives the specs.
- **Free the workspace after a merge**: `rm .claude/ensemble/<chain>.*` inside it, and it is
  ready for the next chain — the build tree stays, which is the point of reusing it. To
  retire a workspace outright, `git worktree remove ../peacockdb-<name>`; an ignored
  `target/` does not block the removal and does not survive it. Removal is refused if a run left an
  untracked file behind. `git worktree prune` drops registrations whose
  directories are gone.
- The board is `tasks/tasks.md`; the instruction set every agent reads is `prompts.md`.
