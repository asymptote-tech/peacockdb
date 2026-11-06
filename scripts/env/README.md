# Deploying peacockdb

How to run the engine's chains, device runs and benchmarks on machines of your own, and where every
input comes from, for the people who set them up. This page is the walkthrough and the inventory;
[`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "Images", "Containers" and "GPU hosts", is
the reference it links to for detail. Every script named here has `--help`.

## Overview

**The machines.**

- **Your own machine**, the laptop: a clone of this repository and `~/.config/peacock/`, which holds
  your keys and the hosts you recorded. Every path starts here.
- **A CPU VM**: runs a chain of agents, one container per chain, in the build image.
- **A GPU host**: any machine with an NVIDIA card and driver, of any provider. It runs the device
  tiers and the benchmark in the run image, from a tree built elsewhere.
- **The images**, on `ghcr.io/asymptote-tech/`: `peacockdb-build` and `peacockdb-run`, one pair per
  cuDF version. Pulling needs no login.
- **The storage buckets**: `tpch-sf40` and `tpcds-sf40`, the sf40 data sets, read with the
  read-only storage key in `peacock.env`.
- **GitHub**: the repository a chain clones and pushes to, its CI, and its board.

```
   laptop: a clone, ~/.config/peacock/
     |                       |                        |
     | provision-vm.sh,      | build-tree.sh,         | image.sh push, or
     | up.sh, down.sh        | then gpu.sh (path B)   | images.yml (path C)
     v                       |                        v
   CPU VM <------------------|----- docker pull -- ghcr.io/asymptote-tech/
     ~/peacockdb             |                       peacockdb-build:cudf-<v>
     a container a chain,    |                       peacockdb-run:cudf-<v>
     in the build image      |                        |
     |                       v                        | docker pull
     +--- gpu.sh (path A) -> GPU host <---------------+
                               ~/peacock/<instance>/: the tree's binaries and results
                               <data dir>: sf1, sf40, the locks
                               a run, in the run image
                                 ^
                                 | sf40, on a first prepare: the keys on ssh's stdin
                               buckets tpch-sf40, tpcds-sf40
```

**The three paths.**

- **A. A chain on a CPU VM.** Agents build and test in a container on the VM. Their GPU steps run
  `gpu.sh` from inside the container on a GPU host. A person readies the VM and the host once, then
  starts and stops chains.
- **B. A device run or a benchmark from your own machine**, no VM: build the tree in the build image
  locally, then run it on a GPU host with `gpu.sh`.
- **C. Rebuilding and publishing the images**, when a commit changes what they are built from. A
  maintainer of the repository does it.

## Inputs: what comes from where

### Secrets and configuration

On the laptop each lives in `~/.config/peacock/`, mode 700, each file mode 600; `hosts/` is mode 700
too. On a VM the same directory is the VM user's.

| input | what it holds | who provides it | where it is kept | who reads it | where it travels |
|---|---|---|---|---|---|
| `peacock.env` | the four storage key lines: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL`, `AWS_REGION` ([`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "The env file"); the read key | whoever administers the cloud account | the laptop, copied in by `provision-vm.sh --env <file>`, or put there by hand | `provision-vm.sh`; `up.sh`, the VM's copy, for `--env-file`; `gpu.sh prepare` outside an instance, when the host lacks sf40, never sourcing it | to the VM on ssh's stdin, where it gains `GPU_SSH_KEY_FILE`; to an instance as `--env-file`; to a GPU host only its four values, on ssh's stdin, for the sf40 download, never as a file |
| `github-pat` | a fine-grained PAT for this repository (`github_pat_…`) | you make it on GitHub, from the recipe `provision-vm.sh` prints; the organization's owner approves it where the organization requires it | the laptop | `provision-vm.sh` | to the VM on ssh's stdin, into `gh auth login`, which keeps it in `~/.config/gh/hosts.yml`; that directory is mounted read-write in the user's instances. Never in a `peacock.env`, never on a GPU host |
| `gpu_ed25519`, `.pub` | the key to GPU hosts | `provision-vm.sh` makes it when missing; `provision-vm.sh --gpu-key` makes it alone | the laptop | `gpu.sh` and the host adapters, when `GPU_SSH_KEY_FILE` is unset; `provision-vm.sh --gpu-host` | the pair to the VM on ssh's stdin; in an instance mounted read-only at its VM path, which `GPU_SSH_KEY_FILE` names. A GPU host gets the `.pub` alone, in `authorized_keys` |
| `vm` | a line a VM: `<name> <user>@<address>`; the last one is the default | `provision-vm.sh` writes it | the laptop | `up.sh` and `down.sh` on the laptop, which forward to that VM; `provision-vm.sh --new-pat` and `--gpu-host` | nowhere |
| `hosts/<name>.env` | a GPU host: `PROVIDER`, `HOST`, `USER`, and `INSTANCE_ID` where its provider needs it, nothing else | `provision-vm.sh --gpu-host` writes it, from what whoever created the host tells you | the laptop | `gpu.sh`, the adapters in `scripts/env/host/`, `up.sh --gpu-host` | `hosts/` is copied to every VM in `vm`, and mounted read-only in every instance. Never to a GPU host |
| `hosts/<name>/config.yaml` | the provider CLI's profile for a host the scripts power: a service account's private key | whoever administers the cloud account, privately ([`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "A nebius host's config") | the laptop, copied by `provision-vm.sh --gpu-host … --provider-config <file>` | the provider's CLI alone, through the adapter's `--config <path>`. No script opens it, and nobody should | as `hosts/` |
| `machine` | the marker: this machine runs instances | `bootstrap-vm.sh`, as its last step | the VM | `up.sh` and `down.sh`, which run locally where it is and forward to the VM where it is not; `provision-vm.sh` | nowhere |
| your ssh key on the VM's login user | your laptop's ssh key | whoever creates the VM puts it there | the login user's `authorized_keys` | ssh | `provision-vm.sh` copies that `authorized_keys` to your own user on the VM |
| git identity | `user.name` and `user.email` | you, in the laptop's git | the laptop's git config | `provision-vm.sh` | to the VM's `~/.gitconfig`, on ssh's stdin; mounted read-only in an instance. A chain's commits carry it |
| the Claude login | your Claude account's login | you | the VM's `~/.claude` (`CLAUDE_CONFIG_DIR`) | Claude Code | `provision-vm.sh` ends in `claude auth login` over `ssh -t`; the directory is mounted read-write in your instances |
| `GPU_ALLOWED_ACTORS` | the GitHub logins CI's GPU job runs for | a maintainer of the repository | a repository variable | CI's `GPU Tests (nebius-gpu)` job | nowhere; without your login there, no task of yours reaches `done` |

The rules:

- The modes above. `provision-vm.sh` sets them, and refuses a `peacock.env` holding any other key,
  naming the key, never its value.
- No script prints a key's or a token's value, and neither should you: not into a log, a ticket, a
  commit or a chat.
- `peacock.env` holds the read key. The storage writer key is never on a VM, a GPU host or in an
  instance: an instance can push and dispatch CI, and a VM is shared by its developers
  ([`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "Trust").
- The PAT is fine-grained, for this repository alone. A classic token, or the OAuth token plain
  `gh auth login` makes, is refused by `provision-vm.sh` and `up.sh`.
- `peacock.env` may be passed between developers. The PAT and the GPU key are personal.

### Images

| image | holds | built from |
|---|---|---|
| `peacockdb-build:cudf-<v>` | every build mode, cuDF in `/opt/cudf`, nvcc, gcc, rust, DuckDB and its extensions, the Python venv, gh, aws and nebius | `docker/build.Dockerfile` |
| `peacockdb-run:cudf-<v>` | cuDF's runtime and `run-tests.sh`, which runs `list`, `filter` or `bench`; nothing to build with | `docker/run.Dockerfile` |

- `<v>` is a cuDF version with a spec in `docker/conda/`: 25.02, the default, or 26.02. Each image
  has two tags: `:cudf-<v>`, the latest published, and `:cudf-<v>-<commit>`, the commit it was built
  from.
- **The label check.** Each image carries `peacockdb.inputs`: the hashes of the `docker/` files it is
  built from, of the `docker/pins.env` lines it declares, and for the build image the
  `third_party/cudf` commit. `bootstrap-vm.sh`, `up.sh`, `build-tree.sh` and `gpu.sh prepare` and `run` compare it
  with the tree, and refuse an image built from other inputs, naming each, or one with no label. The
  fix is a republished image, path C.
- `docker/pins.env` pins every version the images and the VM take: the compilers, the tools, Claude
  Code and its plugin, gh, and `DRIVER_MINIMUM_<v>`, the oldest driver each image runs on.

Detail: [`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "Images".

### Data and caches

| what | where it comes from | where it is kept |
|---|---|---|
| the cuDF source, the cmake files the FFI build's configure reads | `third_party/cudf`, the commit the tree pins | on a VM, one checkout a commit, `<data dir>/cudf/<commit>/`, fetched by `bootstrap-vm.sh` and `up.sh`; on the laptop, the clone's own submodule or `build-tree.sh --cudf-source <checkout>` |
| cargo's registry | `cargo fetch --locked` in the build image | `~/.cache/peacock/cargo-registry`, mounted at `/opt/rust/cargo/registry`; `build-tree.sh --registry` names another |
| ccache | the builds | `~/.cache/peacock/ccache`; `build-tree.sh --ccache` names another |
| sf1, `tpch.sf1` and `tpcds.sf1` | generated by `testdata/generate_testdata.sh` in the build image, offline | `<data dir>/<hash>/`, the hash over the generator, the DuckDB pin and its arguments; on a VM and on a GPU host, never fetched |
| sf40, `tpch.sf40` and `tpcds.sf40` | fetched from the buckets `tpch-sf40` and `tpcds-sf40` in the build image, each file checked against the bucket's `manifest.json`, and `SHA256SUMS` written | a GPU host's `<data dir>/sf40/`, fetched by the first `gpu.sh prepare` that finds a set missing or broken |

**The data directory** is `/var/lib/peacock`, or `PEACOCK_DATA_DIR`. On a VM `bootstrap-vm.sh` makes
it, group `peacock`; on a GPU host `gpu.sh prepare` makes it, group `docker`; both setgid, mode 2775,
shared by the machine's users. On a GPU host another directory is named by `PEACOCK_DATA_DIR` in
`/etc/environment`, and must exist: it is never made.

## Path A: a chain on a CPU VM

A chain runs in a container on a CPU VM, and its GPU steps on a GPU host. You drive both from the
laptop. Reference: [`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "Containers" and "GPU hosts".

**Ask for these first.**

| what | from |
|---|---|
| write access to `asymptote-tech/peacockdb` | a maintainer of the repository |
| your GitHub login on the repository variable `GPU_ALLOWED_ACTORS`; without it CI's GPU tests fail, and no task reaches `done` | a maintainer of the repository |
| approval of your fine-grained token, where the organization requires it | the organization's owner |
| `peacock.env`: the four values of the read-only storage key (`llm-wiki/build-test.md`, "The env file") | whoever administers the cloud account |
| the CPU VM below, with your laptop's ssh key on its login user | whoever administers the cloud account |
| the GPU host below, made with your GPU key's `.pub` (step 1 prints it, as does `provision-vm.sh --gpu-key` alone) as its login key; its address and login user | whoever administers the cloud account |
| for a GPU host the scripts start and stop: its instance id and its provider's CLI config (`llm-wiki/build-test.md`, "GPU hosts") | whoever administers the cloud account |
| agreement to your first chain's PR and its CI run, whose GPU job runs on the shared nebius-gpu | a maintainer of the repository |
| republished images, when a commit moves their inputs (`llm-wiki/build-test.md`, "Rebuilding and publishing"; needs `write:packages`) | a maintainer of the repository |
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
`git fetch origin && git ls-tree origin/master scripts/env` prints nothing until then.

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
   `ENS-<task>` on origin. Later chains are defined on master ([`llm-wiki/README.md`](../../llm-wiki/README.md), "Define a task").

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
  (`pause` and `rebase` too, as [`llm-wiki/README.md`](../../llm-wiki/README.md) describes). `docker logs -f peacock-<you>-<chain>` is the watchdog.
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

### The chain's GPU steps

Inside the container the agents run each GPU step themselves, with `stage-tree.sh` and `gpu.sh`, on
the host `up.sh --gpu-host` named: [`llm-wiki/prompts.md`](../../llm-wiki/prompts.md), "Developer",
is their sequence. The instance names its tree, its image and its keys, so `--instance`, `--image`,
`--cudf` and `--env` are refused there.

## Path B: a device run or a benchmark from your own machine

No VM: the tree is built on the laptop, in the build image, and run on a GPU host.

**What it needs.**
- The GPU host of path A ("The GPU host", above), reachable over ssh from the laptop rather than
  from a VM. From whoever creates it: its address and login user, and for a host the scripts power,
  its instance id and its provider CLI's config.
- On the laptop: bash 4.4 or later, ssh, ssh-keygen, tar and rsync; docker and flock, for the build;
  `peacock.env` (above), for the first `prepare` on a host without sf40.
- For a host the scripts power, its provider's CLI on PATH: the nebius CLI for a nebius host, which
  `scripts/env/host/nebius.sh` drives with the host's `config.yaml`. Without it every command for
  that host is refused. Path A's instances have it in the build image; on the laptop, install it.
- A clone of this repository with `third_party/cudf` checked out at the commit it pins
  (`git submodule update --init third_party/cudf`), or another checkout of that commit for
  `--cudf-source`.

**macOS.** `gpu.sh`, the adapters and `provision-vm.sh` run on a macOS laptop with a bash from brew:
`scripts/lib/portable.sh` stands in for `sha256sum` and `timeout`, and they take macOS's rsync, paste
and sort. `build-tree.sh` needs docker and flock, which macOS lacks: `brew install flock`. The build
image is x86_64. On Apple Silicon docker runs it under emulation, which nothing in this repository
tests; a Linux x86_64 machine with docker is the known-good place to build.

**The instance name.** `--instance <name>`, given once, names the tree's dir on the host,
`~/peacock/<name>/`. It must be unique among everyone who uses that host's login, path A's instances
included, which take `<user>-<chain>`: `<you>-<purpose>`, `dev1-h100-bench` say. The tree records the
name in `.gpu-test/.instance`, with an id of its own and its path, and the host keeps that id in
`~/peacock/<name>/.tree`. So a second tree under a taken name is refused on the host before it ships,
runs, pulls or clears anything, and a copy of a tree is refused its record: remove `.gpu-test/.instance`
in the copy and give it a name of its own. A tree moved rather than copied keeps its instance: put
its new path on the record's third line.

**The steps**, each from the clone. A dropped ssh is never fatal: run the same command again. `run`
follows a run that is still going, and the others skip or redo what is done.

1. `scripts/env/provision-vm.sh --gpu-key`. It makes `~/.config/peacock/gpu_ed25519` when it is
   missing and prints its `.pub`, and does nothing else. Worked: `The key to GPU hosts,
   …/gpu_ed25519.pub: put it on a GPU host when you create one.`, then the `.pub` line. Send that line
   to whoever creates the host, as its login key.
2. `scripts/env/build-tree.sh --bench`, before the host is paid for. It pulls the build image, about
   10 GB, runs `cargo fetch` and then `stage-tree.sh --bench` in it, and leaves `cpp/install`. The
   first build is long, with no figure on record; the registry and ccache keep later ones shorter.
   `--cudf <v>` or `--image <tag>` picks the image, `--cudf-source <dir>` another cuDF checkout, and
   `--registry` and `--ccache` other caches than `~/.cache/peacock/`. Worked:
   `staged <tree>/cpp/install: gpu.sh <host> ship sends it`.
3. Once the host is up: `scripts/env/provision-vm.sh --gpu-host <host> <user>@<address>`, with
   `--provider nebius --instance-id <id> --provider-config <file>` for a host the scripts power.
   Worked: `…/hosts/<host>.env names <user>@<address>: gpu.sh <host> prepare readies it`, after
   `takes the GPU key already` on a host made with your `.pub`.
4. `scripts/env/gpu.sh <host> prepare --instance <name> [--env <file>] [--cudf <v>]`, once. It
   checks the driver, installs what is missing, pulls both images and checks their labels, generates
   sf1, and fetches sf40 with the keys in `~/.config/peacock/peacock.env`, or `--env`'s file, when
   the host lacks it. Worked: `<host> is ready for <name>: gpu.sh <host> ship`. The first one on a
   host pulls the images and 56 GB of sf40: an hour or more. From here on each command takes the
   recorded name and says so in one line: `instance <name>, as .gpu-test/.instance records for this
   tree`.
5. `scripts/env/gpu.sh <host> ship`. Worked: `shipped to <host>:~/peacock/<name>`. Ship again after
   every build.
6. `scripts/env/gpu.sh <host> run list`, the device tiers, as CI's L40S list. Worked: exit 0 and
   `==> run-tests.sh list: green`. `run filter --filter <f>` runs the list narrowed by a test filter.
7. `scripts/env/gpu.sh <host> run bench`, which times all 390 cases, 130 queries at `tp1_single`,
   `tp4_single` and `tp4_sized`, in one run; the row-group modes are not timed. Or one mode a run,
   `run bench --filter tp1_single` and so on, 130 cases each: shorter runs, so a lost host costs less.
   Worked: its last line, `==> bench outcome: 130 cases ran: 130 passed, 0 failed` for one mode.
   Each run's record is kept under its own start time, `records-<start time>.tsv`, so runs in a row
   lose nothing and one `pull` after the last brings every record home.
8. `scripts/env/gpu.sh <host> pull`: `testdata/benchmark-results/<device>/`, every record into
   `testdata/calibration/`, and the logs into `.gpu-test/<host>/logs/`. Worked: `pulled from <host>:
   …`. `pull --clear` instead does the same pull, then removes from the host each `records-*.tsv`
   whose copy is now home, byte for byte, a line a record. Clear once you no longer want the host's
   copies: before a new series of runs, say. The local `records-*.tsv` are never touched by it.
   Publishing a record to the calibration bucket is outside this page: `llm-wiki/build-test.md`,
   "Corpus benchmarks", covers it.
9. As needed: `scripts/env/gpu.sh <host> cancel` stops the tree's run and keeps its record;
   `gpu.sh <host> start` and `stop` power a host the scripts power, and `stop --force` skips its
   checks.

Give `--cudf` or `--image` to every `prepare` and `run` when it is not 25.02: the tree records its
instance, not its image.

**`pull` exits 1** when it kept a local file rather than take the host's: a record that differs from
the host's, or another file newer here. It names each. The rest of the pull is home, and
`pull --clear` has still removed the records whose copies are home and the same; the one that
differs stays on the host.

**`gpu.sh`'s exit codes**, every command: 0 done; 1 a tool failed, or the run failed; 2 refused, with
one line saying why; 3 the host's driver is older than the image needs. `run` exits with the run's
own 0, 1 or 2. **`build-tree.sh`'s**: 0 staged; 1 a step in the build image failed; 2 refused: a bad
argument, a missing tool or directory, or an image of other inputs; 130 interrupted by INT or TERM,
its container killed.

**The bench outcome.** A bench ends with `==> bench outcome: <n> cases ran: <p> passed, <f> failed`,
the failed ones named, and exits 1 when one failed. A run whose harness summed nothing up says
`the run broke (exit <s>) after <n> cases began`; a cancelled one `cancelled (exit <s>) after <n>
cases began`. Either way what it measured is in `testdata/benchmark-results/` after `pull`
([`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "GPU hosts", "A bench's outcome").

**A host the scripts power** is started by each command and soft-stopped when it ends, unless the
card or another command is busy: a detached run keeps it up until the run ends. A host powered by
hand is yours to start and stop.

## Path C: rebuilding and publishing the images

When a commit changes what an image is built from, `docker/` or the `third_party/cudf` commit, every
label check against that commit refuses the published image until it is published again. A maintainer
of the repository does it, from a clean tree:

    gh workflow run images.yml -f kind=<build|run|all> -f cudf=<v|all> [--ref <branch>]

or by hand, `scripts/env/image.sh build <build|run> <v>`, then `scripts/env/image.sh push <build|run>
<v>`, with docker logged in to ghcr.io with `write:packages`. Detail:
[`llm-wiki/build-test.md`](../../llm-wiki/build-test.md), "Rebuilding and publishing".

## The scripts

| script | runs on | does | reads | writes |
|---|---|---|---|---|
| `scripts/env/provision-vm.sh <login>@<address>` | the laptop | readies a VM for you: your user and groups, `bootstrap-vm.sh`, the secrets on ssh's stdin, git's identity, Claude's login; `--new-pat` replaces the PAT on every VM | `--env`'s file, `~/.config/peacock/`, the laptop's git config | the laptop's `peacock.env`, `github-pat`, `gpu_ed25519`, `vm`; the VM's `~/.config/peacock/`, gh's login, `~/.gitconfig` |
| `provision-vm.sh --gpu-host <name> <user>@<address>` | the laptop | records a GPU host and puts the GPU key on it, its password typed once where the key does not log in yet | `gpu_ed25519.pub`, `--provider-config`'s file | `hosts/<name>.env`, `hosts/<name>/config.yaml`, `hosts/` on every VM |
| `provision-vm.sh --gpu-key` | the laptop | makes the GPU key when missing, its `.pub` again from the key when that alone is lost, and prints the `.pub` | `gpu_ed25519` | `gpu_ed25519` and `.pub` |
| `scripts/env/bootstrap-vm.sh` | the VM, as a user with sudo; `provision-vm.sh` runs it | installs Docker, gh and Claude Code with its plugin, makes the data directory, clones the repository, fetches the cuDF checkout, checks the build image's label, fills cargo's registry, generates sf1 | `docker/pins.env` | `~/peacockdb`, the data directory, the marker `machine` |
| `scripts/env/up.sh <chain>` | the VM, or the laptop, which forwards it there | checks the logins and the image, makes the chain's worktree, fetches crates and generates sf1 when missing, starts the container `peacock-<user>-<chain>` with its watchdog | `peacock.env`, `~/.config/gh`, the board, `hosts/` | the worktree, the instance's home, the container |
| `scripts/env/down.sh <chain>` | the VM, or the laptop | stops and removes the chain's container, keeping its worktree | the containers' labels | nothing |
| `scripts/env/stage-tree.sh [--bench]` | the build image: an instance | builds the device rung and the C++ suites, and stages them, stripped | the tree | `cpp/install` |
| `scripts/env/build-tree.sh [--bench]` | any machine with docker and flock, outside an instance | `cargo fetch`, then `stage-tree.sh`, in the build image | the tree, the cuDF checkout, the registry, ccache | `cpp/install`, `cpp/build`, `target/`, the caches |
| `scripts/env/gpu.sh <host> <command>` | an instance, the laptop or a VM | `prepare`, `ship`, `run`, `pull`, `cancel`, `start`, `stop` | `hosts/<host>.env`, the GPU key, `peacock.env` for `prepare` outside an instance, `.gpu-test/.instance` | the host; at home `testdata/benchmark-results/`, `testdata/calibration/records-*.tsv`, `.gpu-test/` |
| `scripts/env/on-gpu-host.sh` | the GPU host; `gpu.sh` ships and runs it | the host's side: install, data, the detached run, locks, cancel, clearing records | the instance's dir, the data directory | `~/peacock/<instance>/`, the data directory |
| `scripts/env/host/ssh.sh`, `nebius.sh` | wherever `gpu.sh` runs | a provider's `start`, `stop`, `status`, `ip`, `powers`, `ssh <cmd>` | `hosts/<name>.env`; `config.yaml` by its path alone | nothing |
| `scripts/env/image.sh` | a maintainer's machine, or `images.yml` | builds and publishes an image | `docker/` | the images on ghcr.io |
| `docker/run-tests.sh` | the run image, as its entrypoint | `list`, `filter`, `bench` | `/peacock/install`, `/peacock/testdata` | `benchmark-results/`, `calibration/records.tsv` |

`scripts/lib/` holds what they share: `peacock-config.sh`, the paths in `~/.config/peacock/` and the
env file's checks; `host.sh`, a host file and the ssh to it; `machine.sh`, a VM's layout and its steps
in the build image; `image-inputs.sh` and `data-inputs.sh`, the label's and sf1's hashes; `s3.sh`, the
storage's endpoint and region; `portable.sh`, what a macOS laptop lacks. `scripts/env/` holds three
more, none of them part of deploying: `no-static-cudart.sh`, `spec_hashes.py` and
`upload-embeddings.sh`.

## Where things live

**On your own machine:**
- `~/.config/peacock/`: `peacock.env`, `github-pat`, `gpu_ed25519` and `.pub`, `vm`, `hosts/`
  (above, "Secrets and configuration").
- `~/.cache/peacock/cargo-registry` and `~/.cache/peacock/ccache`, `build-tree.sh`'s caches.
- In the tree: `cpp/install`, the staged build; `.gpu-test/.instance`, the tree's instance;
  `.gpu-test/<host>/logs/`, the runs' logs and statuses `pull` brought; `testdata/benchmark-results/`
  and `testdata/calibration/records-*.tsv`, what the runs measured. `.gpu-test/` is git-ignored.

**On a VM**, under your user:
- `~/peacockdb`, the clone `up.sh` and `down.sh` run from.
- `~/peacock-instances/<chain>/`, a chain's worktree, and `~/peacock-instances/.home/<chain>/`, its
  home; `.claude/ensemble/` in the worktree holds the chain's status, log and control file.
- `~/.config/peacock/`: `peacock.env` with `GPU_SSH_KEY_FILE`, `gpu_ed25519` and `.pub`, `hosts/`,
  the marker `machine`. `~/.config/gh/`, `~/.claude/`, `~/.gitconfig`.
- `~/.cache/peacock/cargo-registry` and `~/.cache/peacock/ccache`.
- The machine's data directory: `<hash>/tpch.sf1` and `tpcds.sf1`, `cudf/<commit>/`.

**In an instance**, the container `peacock-<user>-<chain>` of the build image:
- The worktree and the clone's `.git` at their VM paths, read-write; the instance's home at `$HOME`.
- `peacock.env` as `--env-file`; `PEACOCK_INSTANCE=<user>-<chain>`, `PEACOCK_IMAGE_TAG`, and
  `PEACOCK_GPU_HOST` when `up.sh --gpu-host` named one.
- Read-only: `hosts/`, the GPU key, the cuDF checkout, cargo's registry, sf1, `~/.gitconfig`, Claude
  Code's install. Read-write: `~/.claude`, `~/.config/gh` and ccache, shared by your instances.

**On a GPU host**, under its login user:
- `~/peacock/<instance>/`: `.tree`, the id of the tree that owns it; the shipped `cpp/install`,
  `scripts/` and fixtures; `testdata/`, where a run writes `benchmark-results/` and
  `calibration/records-*.tsv`; `logs/<mode>-<start time>.log`, each with its `.status`; `run/`, the
  instance's run state.
- The data directory: `<hash>/` with sf1, `sf40/`, `runs/` with each instance's run lock, and the
  host's `.activity.lock` and `.device.lock`.
- A run's container, `peacock-run-<user>-<instance>`, while the run goes.

**In a run's container**, the run image: `cpp/install` read-only at `/peacock/install`, the
instance's `testdata/` read-write at `/peacock/testdata`, the data sets read-only over it, the host's
name as its hostname.

## Who does what

**Only a person**, since each needs a login, a password, a payment or an agreement:
- asks for what path A's table lists, and creates or has created the VM and the GPU host;
- makes the PAT and pastes it, and logs Claude Code in: `provision-vm.sh <login>@<address>` and
  `--new-pat`;
- types a GPU host's password, when `provision-vm.sh --gpu-host` meets a host the key does not log
  in to yet;
- starts and stops chains with `up.sh` and `down.sh`, and steers them through their control file;
- agrees to a first chain's PR and its CI run, merges, and, as a maintainer, publishes the images.

**An agent outside an instance** may take path B whole: `provision-vm.sh --gpu-key`;
`provision-vm.sh --gpu-host` on a host made with the GPU key's `.pub`, where no password is asked;
`build-tree.sh`; and every `gpu.sh` command. It runs nothing else on this page.

**An agent in an instance** builds with `stage-tree.sh`, runs the GPU steps with `gpu.sh`, commits
and pushes its branch, opens its PRs and dispatches CI. It has no password and no storage writer key,
and the instance sets its `--instance`, `--image` and `--env`.

## Common refusals

| it says | what to do |
|---|---|
| `… was built from other inputs than …` | the published image is stale: path C, then the same command again |
| `… has no peacockdb.inputs label: publish it again with scripts/env/image.sh` | path C |
| `superpowers is not <v> at <sha> …` | a maintainer bumps `SUPERPOWERS_VERSION` and `SUPERPOWERS_SHA` in `docker/pins.env`; then path A's step 1 again |
| `no …/peacock.env, to read … from` (`gpu.sh prepare`) | put the storage keys there, or pass `--env <file>` |
| `this tree is instance <a>, as .gpu-test/.instance records, not <b>` | use `<a>`, or remove `.gpu-test/.instance` to move the tree to `<b>` |
| `.gpu-test/.instance was written for <path>, not this tree: …` | a copy: remove `.gpu-test/.instance` and give `--instance <a new name>`; a moved tree: put its path on the record's third line |
| `~/peacock/<name> on <host> is another tree's: …` | another tree, maybe another developer's, has that name on the host: give this tree another. If it is this tree's own dir and the record was lost (`git clean -x` removes `.gpu-test/`), remove the host's mark: `scripts/env/host/<provider>.sh <host> ssh 'rm ~/peacock/<name>/.tree'`, then `--instance <name>` again |
| `not in an instance: --instance <name> names …` | the tree has no record yet: give `--instance <name>` |
| `the GPU key does not log in to <host> …` | `provision-vm.sh --gpu-host <host> <user>@<address>`, or have the key added to the host's `authorized_keys` |
| `cannot reach <host> … start it by hand` | start the host, then the same command again |
| `the driver on <host> is <v>, older than <w>, …` (exit 3) | update the host's driver |
| `<tree>/cpp/build was configured at …: remove cpp/build` | remove `cpp/build`, then `build-tree.sh` again |
| `no … libpeacock_gpu.so: build and stage the binaries first` | `build-tree.sh`, or `stage-tree.sh` in an instance |
| `… gpu.sh <host> prepare first`, `… prepare fetches it`, `… prepare pulls …` | `gpu.sh <host> prepare` |
| `a <mode> run of <instance> started <stamp> is still going …` | wait for it, follow it with the same `run` again, or `cancel` it |
| `the <mode> run of <instance> … is still going on <host>: pull without --clear, or once it ends` | `pull` now, and `pull --clear` after the run |
| `the pull itself completed; <host> cleared nothing` | a run started between the pull and the clear: `pull --clear` again once it ends |
| `the container peacock-run-… of an earlier run … is still on …` | `gpu.sh <host> cancel` removes it |
| `--filter <f> and PCK_TEST_FILTER=<g> differ: give one` | unset `PCK_TEST_FILTER`, or drop `--filter` |
| `not stopping <host>: …` (exit 2) | something still uses the host; `stop --force` only when it names it |
