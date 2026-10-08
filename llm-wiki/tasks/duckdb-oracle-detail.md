# duckdb-oracle — run detail

Working notes for this task. The spec is [`duckdb-oracle.md`](duckdb-oracle.md) (frozen); the
plan the developer works in is [`duckdb-oracle-impl.md`](duckdb-oracle-impl.md).

## Branch and PR

- Branch `ENS-duckdb-oracle`, forked off master at `38d5f2de` ("Chain J approved to build").
- First task of chain J, so its PR targets **master**.
- Workspace: `peacockdb-alpha` (`/home/dmitry/workspace/peacockdb-alpha`).

## Hosts, as probed 2026-10-08 before the first dispatch

- **verda: down.** `ssh verda` fails to resolve the hostname, so CPU tests run locally. Re-probe
  before each dispatch rather than trusting this line.
- **shad-gpu: down.** `ssh shad-gpu` (llm-gpu0h200.velkerr.ru:22) times out. So the device half of
  the verification bar cannot run yet: impl **Task 7 Step 3** (the `PCK_WRITE_GPU_RESULT=1` cycle
  and `--pull-results`) and with it the committed `gpu-result.txt` files are deferred, and so is
  Task 7 Step 4's commit of them. Everything else in the plan is rust-only and local.

## Round 1 dispatch (2026-10-08)

Scope: impl Tasks 1–6, Task 7 Steps 1, 1b, 2 and 3b, and Tasks 7b, 7c, 7d, 8 — the whole
rust-only verification bar. The device cycle is held back for a shad-gpu that answers.

Consequence to carry: with no device cycle, `gpu-result.txt` does not exist, so Task 7 Step 1b's
coverage guard (`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`) has no file to
read and the `duckdb_gpu_<dataset>_<query>_<mode>` cases have no sections. **Those guards are
written in their honest form and left red** — an absent file reads as "not regenerated", which is a
real gap, and a guard that passes over a missing file is a guard that cannot go red. They are the
one permitted red in round 1, and they go green in the same step that writes the file. The
developer reports them by name and does not weaken them to get a green suite.

## Round 1 result (2026-10-08)

Impl Tasks 1–6, Task 7 Steps 1/1b/2/3b, and Tasks 7b, 7c, 7d, 8 are done. The device cycle
(Task 7 Steps 3 and 4) is not, and nothing stands in for it.

### Case counts, before → after (rust-only)

| target | before | after |
|---|--:|--:|
| `--lib` | 602 | 636 |
| `test_cpu_corpus` | 555 | 704 |
| `test_golden_format` | 26 | 36 |
| `test_corpus_goldens` | 26 | 26 |
| `test_module_layout` | 17 | 17 |
| `test_ci_coverage` | 9 | 9 |
| `testdata/test_duckdb_result.py` | — | 9 |

`test_cpu_corpus`'s 149 new cases: 120 `duckdb_<ds>_<q>`, 26
`duckdb_gpu_<ds>_<q>_<mode>`, `every_duckdb_oracle_is_named_by_some_line`,
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`,
`all_modes_expands_to_the_five_in_either_position`. `build-test.md` line 25 said `--lib` 601
before this branch and measured 602 — it was stale by one; the new number is measured.

### The 27 red cases, and why

`testdata/goldens/{tpch,tpcds}.sf1/gpu-result.txt` does not exist, because no device cycle has
written it. So the 26 `duckdb_gpu_*` cases and
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other` fail, every one of them
with the same message: "… does not exist, so no device answer is recorded … Run a cycle with
PCK_WRITE_GPU_RESULT=1 and bring it home with --pull-results." Nothing else in the branch is
red. They go green in the step that writes the file and in no other, which is what makes them
a gap and not a nuisance: an absent file reads as "not regenerated since the cells moved",
which is exactly what the coverage guard exists to catch.

The 26 are tpch q1 and q6 and shuffle-additive-avg at all five modes; tpch
aggregate-groupby, filter-project, nested-loop-join, q17, q19, shuffle-additive,
shuffle-stddev at tp1-single; tpcds q37, q82, q84, q85 at tp1-single.

### Task 6: every line's oracle, from the first run

93 `duckdb_exact`, 15 `duckdb_approx`, 4 `duckdb_fingerprint`, 4 `duckdb_none`, 4
`duckdb_divergent`. The survey was taken by a throwaway case that tried each oracle in order
and wrote `/tmp/oracle-survey.txt`; it is deleted.

- **`duckdb_approx` (15)**: tpch q1, q8, q14, shuffle-additive-avg, shuffle-stddev; tpcds q7,
  q9, q13, q18, q26, q39, q59, q75, q85, q90. Each differs in rendered digits only — a
  decimal truncated at its scale against DuckDB's double (`25.522005` vs
  `25.522005853257337`), a trailing-zero difference (`86.250000` vs `86.25`), or a float's
  last digit from reassociation (`16.38077862639554` vs `…543`).
- **`duckdb_fingerprint` (4)**: tpch q16, anti-join, filter-project, semi-join. The engine's
  and DuckDB's fingerprints came out **byte for byte identical** — same `nonnull` counts, same
  `sum`/`min`/`max`, same SHA-256 — which is the strongest evidence the two writers agree.
- **`duckdb_none` (4)**: tpch q11 and q22, tpcds q24 and q54. Enabled at no cpu mode (#190),
  so our side has `skipped: not enabled at any mode` while DuckDB answers. Each moves to its
  variant in the task that turns its cells on.
- **`duckdb_divergent` (4)**, below.

**Nothing needed `duckdb_columns`.** No LIMIT window's cutoff tied: the only
`data_fusion_subset` line is tpch/scan-limit, whose section is `duckdb_exact`, and the
first run turned up no row pair that differed by a tie. The variant was not added, as the
spec's rule wants.

### The four divergences, and their tickets

- **tpcds q17 — `duckdb_divergent(205)`**, no positions. Both sides answer ZERO rows. Ours
  renders `++\n++`: the cpu emits no batch at all, so there is no schema to take a header
  from (#205), where DuckDB prints its fifteen column names. Not a wrong answer — a missing
  shape — and #205 is the open ticket for exactly that. Goes back to `duckdb_exact` when #205
  clears.
- **tpcds q58 (`2, 4, 6`), q61 (`2`), q66 (`20`–`31`) — `duckdb_divergent(251, …)`**. New
  ticket **#251**, filed in `corpus-coverage.md`'s Scalars section. One mechanism, two shapes:
  DataFusion cuts a decimal division at the scale it declared for the result rather than
  rounding, and the truncated value then feeds the rest of the expression.
  - `(x / y) * 100` multiplies the truncation by a hundred: q58's `ss_dev` is `103.719200`
    against `103.71926462058356`, q61's ratio `51.82319100` against `51.82319145188511` —
    **45 to 97 units in our last rendered place**.
  - `sum(x / y)` adds one truncation per row: q66's twelve `*_per_sq_foot` columns are **1.3
    to 1.8 units** short.
  - A quotient nothing consumes stays inside its scale, which is why tpch q1's `avg_qty` is
    `duckdb_approx` and not a divergence.
  The exact positions were read off the two goldens cell by cell; the per-column measurements
  are in #251.

### Deviations from the plan as written, and why

1. **The `skipped: … cap` marker is retired, not kept beside the fingerprint.** The plan
   wanted `is_over_cap` to read "a `SKIPPED` marker naming the cap, OR a fingerprint". But
   once BOTH writers fingerprint an over-cap section, nothing writes that marker again — so
   the marker arm would be defensive code for an unreachable input and `corpus::over_cap`
   would be dead. Instead: `over_cap` is deleted, and the one predicate is
   `section_holds_rows(section)` — false for a `skipped:` marker and false for a fingerprint,
   which is what all three of its callers actually ask. `is_fingerprint` is the narrower one
   the comparator needs. `an_over_cap_result_is_a_marker_and_not_a_deletion` became
   `an_over_cap_result_is_a_fingerprint_and_not_a_deletion` and now asserts the author line is
   LAST (the fingerprint keeps first position, as the marker did).
2. **`duckdb_case` is two functions, not one with an `Engine` enum.** `duckdb_case(dataset,
   sf, query, oracle)` and `duckdb_gpu_case(dataset, sf, query, mode, oracle)`. The device
   case needs the mode, which an `Engine::Device` unit variant cannot carry, and a
   lifetime-bearing `Engine<'a>` on the `test_support` surface buys nothing over two names.
3. **The device cases are per MODE**, `duckdb_gpu_<ds>_<q>_<mode>`, as the spec says — the
   plan's Step 1 sketch generated one per line. A small `duckdb_device_cases!` helper macro
   holds the `none`-versus-modes decision, so `corpus_query!` stays at four arms.
4. **An absent `gpu-result.txt` fails.** The plan's Task 7 Step 1 said an absent file or
   section is "nothing to compare" and the case returns. Left that way the one thing the
   file exists for — proving a device cell's answer was recorded — would pass vacuously
   forever. Both the per-cell case and the coverage guard panic naming the regeneration.
5. **The fingerprint's numbers are `{:.17e}` with a plain exponent**, not `{:e}`. Rust's
   `{:e}` prints `4e0` and Python's `'{:e}'` prints `4.000000e+00`; the two writers' text must
   be byte-identical, and `{:.17e}` with Python's `+00` padding stripped is the one form both
   produce. Verified over ten values including `5e-324` and `1e308` — all ten agree.
6. **`fingerprint_of` classifies columns from the RENDERED cells, not from the arrow type.**
   It has to: DuckDB has no arrow type to read, and the class is the only thing the two sides
   can agree on. That costs two passes over the rows (classify, then collect) and no extra
   memory.
7. **`CpuOracle::ALL` and `GpuResultMode::ALL` are the keyword tables**, with a
   `keyword()` beside each and `cpu_oracle_mode`/`gpu_result_mode` decoding THROUGH them.
   Without that the `ALL` consts have no production caller and need an `allow(dead_code)`;
   with it the accepted set in the panic message and the list the `ALL` test holds are one
   table rather than two spellings.
8. **`merge_mode_section` orders the file** by the registry's row order and then the mode
   sequence. The device cases run in whatever order libtest gives them, and a file in that
   order is a reordering to read on every pull home. Note the registry's rows are
   `cost-registry.csv`'s (q1 before q6), NOT `corpus_cases.inc`'s (which opens with q6).
9. **#235 is not archived.** Its device half has not run; archiving a ticket while 27 of its
   cases are red would put a closed number on an open gap. Its text records what landed and
   that one cycle closes it. #251 was filed and indexed.
10. **Tests live in `<module>/tests.rs`, not inline.** The plan sketched `#[cfg(test)] mod
    tests { … }` at the bottom of the new files; `coding-style.md` forbids that and
    `test_module_layout`'s `a_test_module_lives_in_its_own_file` enforces it.
11. **`sha2` became an optional dependency gated on `test-support`.** It was a
    dev-dependency only, which `src/test_support/` cannot reach — the same reason `inventory`
    is already optional there.
12. **The CI step globs `testdata/test_*.py`** instead of naming the new file. The
    exec-model step beside it already gives the reason: there is no meta guard over python
    files, so a hand-written list rots the way `test_ci_coverage` exists to prevent.

### What the next developer should know

- **Neither remote host answered.** `ssh verda` does not resolve; `ssh shad-gpu` times out.
  Everything here was run locally. **There is no cuDF on this workstation**
  (`~/data/miniforge3/envs/rapids` is absent), so `corpus_gpu.rs` and `test_gpu_corpus.rs`
  were **never type-checked** — `cargo check -p peacockdb-core --tests` fails in
  `peacockdb-ffi`'s build script for want of `CUDF_ROOT`. Both files were kept to small edits
  and `rustfmt` was used as a parse check on them; the first real compile is the next device
  or cudf-capable round. That is the biggest outstanding risk in this branch.
- To keep that risk down, the device helper's whole comparison was moved into a new UNGATED
  module, `test_support/device_answer.rs` (`GpuResultMode`, `gpu_result_mode`,
  `device_answer_matches`, and the lifted float-tolerant comparator). `corpus_gpu.rs` now
  reads the section and calls in; it holds the live-cpu run, the schema check and the
  `gpu-result.txt` write and nothing else that could be wrong about a comparison.
- **`every_result_section_names_the_mode_that_would_author_it_now`** already searched the
  whole body for a `mode=` line, so the fingerprint's trailing author line needed nothing.
  `each_result_section_was_written_by_the_mode_entitled_to_write_it` did not, and now
  discriminates on the author line rather than on the `skipped:` prefix — which made it
  stronger: an enabled query's section must name its author whether it holds rows or a
  fingerprint.
- **`the_root_emitted_the_rows_the_result_golden_holds`** still skips over-cap sections. It
  could now read `rows=<n>` out of the fingerprint and cover four more queries. Not done —
  out of this task's scope — and worth a look by whoever next touches that test.
- **`same_multiset` pairs rows by sorting a PROJECTION.** Under `duckdb_divergent` with
  positions the undeclared columns pair the rows and the named ones are then checked per
  column; if a named column's values are a permutation across rows, the column's multiset
  agrees and the line reads as "stopped diverging". None of the four lines is near that, but
  it is the comparator's one soft edge.
- **Regenerating the over-cap sections** is four cases, 6.4 seconds:
  `UPDATE_CANONICAL=1 PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core
  --test test_cpu_corpus -- --test-threads=1 cpu_tpch_q16_tp4_sized
  cpu_tpch_anti_join_tp4_sized cpu_tpch_filter_project_tp4_sized cpu_tpch_semi_join_tp4_sized`.
  The fingerprint renders every cell of a 2.4-million-row answer, so run it at one thread.
- **`duckdb_result.py` rewrites the whole file**; `--only` would truncate it to the named
  queries. A full run is ~8 minutes for both datasets and moved exactly the six over-cap
  sections (tpch q11, q16, anti-join, filter-project, semi-join; tpcds q98).
- **tpcds q98 is NOT a corpus line.** The spec names it among the five over-cap fingerprint
  sections, but it is one of the 18 queries only DuckDB answers, so no `duckdb_*` case reads
  it. Four lines take `duckdb_fingerprint`, not five. Its fingerprint is written and never
  compared.
- **The `goldens/` file counts in `build-test.md`** are unchanged because `gpu-result.txt` is
  not committed yet. They need +1 per dataset in the round that commits it.
- Two doc blocks in `test_cpu_corpus.rs` had been swapped by an earlier insertion — the
  documented "a doc comment reassigned by an insertion" antipattern. Repaired here, and the
  reconstruction is provable rather than a guess: one SENTENCE was cut across the two sites
  ("…so it needs no run — which is" at the end of the first, "what makes it catch the first
  `live_cpu` query BEFORE the rollout that needs it" opening the second), so the two halves
  pair uniquely. The pairing block now sits on
  `each_declarations_two_oracles_suit_each_other` and the device-cell block on
  `every_device_cell_has_a_cpu_cell_at_the_same_mode`, which is what each describes.

## Round 1 landed (2026-10-08)

Commit `3a6c5343`, pushed, **PR #167** against master, 2 commits. The whole rust-only bar is green
and the board reads `reviewing`; the developer's own report of what landed, the twelve deliberate
deviations and the oracle decisions are above this line, written by it.

**The device gap, which `reviewing` does not say.** The 27 `duckdb_gpu_*` cases and
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other` are red, on a
`testdata/goldens/{tpch,tpcds}.sf1/gpu-result.txt` that no device has written. shad-gpu did not
answer ssh at 00:25 or at 02:10 (`ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection
timed out`); verda does not resolve either. One `PCK_WRITE_GPU_RESULT=1` cycle through
`build-test-shadgpu.sh --all` plus `--pull-results` is the whole of what is left, and #235 stays
open until it runs. So this task cannot reach `completeness approved` on a host that does not
answer, whatever the review finds.

Note for whoever runs that cycle: the device binaries were **never type-checked** on this
workstation — there is no cuDF here (`~/data/miniforge3/envs/rapids` is absent), so
`corpus_gpu.rs` and `test_gpu_corpus.rs` were only parsed. Expect to fix compile errors there
before the cycle runs at all. That is why the device helper's comparison was moved into the
ungated `device_answer.rs`: it shrinks the never-compiled surface to the live-cpu run, the schema
check and the `gpu-result.txt` write.

## Analyst reading of the device obstacle (2026-10-08)

Probed from `peacockdb-alpha` on `ENS-duckdb-oracle`. Evidence, so a later run need not re-probe.

### shad-gpu is genuinely off the network — not a key, not a config, not our egress

| probe | result |
|---|---|
| `getent hosts llm-gpu0h200.velkerr.ru` | `89.169.176.82` — **DNS resolves** |
| `ssh -o ConnectTimeout=10 shad-gpu` | `connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out` |
| TCP connect to `89.169.176.82:22` | timeout |
| TCP connect to `89.169.176.82:443` | timeout — **the whole IP is dark, not just sshd** |
| TCP connect to `140.82.121.3:22` (github) | connects — our outbound 22 is not filtered |
| `ssh-keygen -F llm-gpu0h200.velkerr.ru` | found, two keys (ed25519 + ecdsa), `known_hosts` lines 10–11 |

The three local hypotheses are each excluded by the evidence, not by assumption:

- **Changed host key on reprovision** (build-test.md:1070) would fail *after* a TCP connect, with
  `REMOTE HOST IDENTIFICATION HAS CHANGED`. We never reach a TCP connect. `ssh-keygen -R` would
  change nothing.
- **ssh config / hostname drift**: `~/.ssh/config` carries `Host shad-gpu → llm-gpu0h200.velkerr.ru,
  User info`, which matches `scripts/lib/shadgpu-env.sh` (`REMOTE=shad-gpu`,
  `REMOTE_REPO=/home/info/peacockdb`) and `pipeline.yml`'s `GPU_HOST=info@llm-gpu0h200.velkerr.ru`.
  The tree and the config agree.
- **Proxy/VPN**: nothing in `scripts/` or `pipeline.yml` sets `ProxyJump`, `ProxyCommand` or any
  proxy env; CI reaches the same hostname directly. There is no tunnel to restore.

So: host down or dropped off its network. Human-side only.

### `ssh verda` is a different failure, and does not mean verda is down

`verda` has **no `Host` entry** in `~/.ssh/config` (only `shad-gpu` and `dev → localhost`), no
`/etc/ssh/ssh_config.d/*.conf`, no `Include`, nothing in `/etc/hosts`. `getent hosts verda` is
empty while `getent hosts github.com` answers, so DNS works and the name simply does not exist
here. `scripts/build-test.sh:22` says the host is **not hardcoded** and its usage example is
`--host dmitry@86.38.182.185` — verda is an ephemeral Verda/DataCrunch spot instance addressed by
IP (`scripts/list_verda_instances.sh` is how its IP is found, needing `VERDA_CLIENT_ID/SECRET`).
`ssh verda` can therefore never succeed on this box as configured, up or down.

**Drift to fix, in build-test.md:1041/1078 and prompts.md:160**: both tell an agent to run
`scripts/build-test.sh --host verda` and the coordinator to probe `ssh verda`, which presumes an
alias that does not exist. A probe of a name that cannot resolve reads as "verda is down" every
time, which is exactly what happened in Round 1. Either the human adds a `Host verda` stanza when
the instance is up, or the wiki says to get the IP from `list_verda_instances.sh`.

### This workstation DOES have cuDF — the Round 1 note checked the wrong env name

`~/data/miniforge3/envs/rapids` is indeed absent, but `~/data/miniforge3` is a symlink to
`~/miniforge3`, and that holds three envs:

| env | libcudf | role |
|---|---|---|
| `rapids-cuda-12.2` | **25.02.02** (`conda-meta/libcudf-25.02.02-cuda12_…`) | the root `scripts/lib/shadgpu-env.sh:13` hardcodes — **shad-gpu's own version** |
| `rapids-26.02` | 26.02.01 | 26.02 leg |
| `rapids-26.02-cu12` | 26.02.01 (no nvcc) | — |

All three carry `include/cudf`, `lib/libcudf.so` and `lib/cmake/cudf/cudf-config.cmake`;
`rapids-cuda-12.2` carries `bin/nvcc`. `gcc-12`/`g++-12` (what `cargo-cudf.sh` pins for that root)
are installed. `cargo check -p peacockdb-core --tests` fails only because the command set neither
`--features rust-only` nor `CUDF_ROOT` — `peacockdb-ffi/build.rs` panics with
`cudf not configured. Either: - Set CUDF_ROOT=…`. That is a missing variable, not a missing cuDF.

The name that is stale is **`rapids`**: `scripts/build-test.sh:37` (`LOCAL_CUDF_ROOT`),
`scripts/cargo-cudf.sh:23` (the `rapids)` gcc-14 arm) and `build-test.md:1005`'s one-off example all
spell the local 26.02 root `…/envs/rapids`, which no longer exists (it is `rapids-26.02`). That
spelling is what the Round 1 note checked.

Direct evidence the local 25.02 device build works on this box: this worktree still holds
`cpp/install/rust-tests/{test_gpu_corpus,test_node_timing,peacock_gpu_benchmarks,peacockdb_core_gpu_lib}`
dated 2026-09-22 02:59 — a `build-test-shadgpu.sh --build` ran here. Its `target-cudf-rapids-cuda-12.2`
has since been deleted (no such dir anywhere under `~`), so a repeat is a **cold** build.

**Constraint on doing it**: `/` is at 90% — 16 GiB free on the only volume, with `target/`
(rust-only) already 19 GiB. A cold `target-cudf-rapids-cuda-12.2` at opt-3 plus the C++/CUDA
install plausibly exceeds that. Free space first, or expect a disk-full failure rather than a
compile error.

### No other path to a committable `gpu-result.txt`

- **verda-gpu cannot produce it, for four independent reasons.** (1) It is cuDF **26.02**
  (build-test.md:1042) and the spec's step 4 fixes the committed file as 25.02's; (2)
  `testdata/.gitignore:28` ignores `/goldens/*/gpu-result-*.txt`, the file a correct 26.02 run
  writes; (3) `scripts/build-test.sh`'s remote gate forwards only `$LD_ENV`, `$TESTDATA_ENV` and
  `$UPDATE_CANON_ENV` — **`PCK_WRITE_GPU_RESULT` is not forwarded at all**, so that path cannot
  write the file today without a script change (only `build-test-shadgpu.sh:416` forwards it); (4)
  no `verda-gpu` host exists or resolves here either.
  **Trap worth naming**: the version suffix is chosen by the operator's value, not detected from
  the device — `corpus_gpu.rs:79-84` reads `PCK_WRITE_GPU_RESULT` and treats `"1"` as "the
  committed file" whatever card ran. A 26.02 run invoked with `=1` *would* write and commit 26.02's
  answers into 25.02's file and go green. Nothing in the tree catches that. It is forbidden by the
  spec, not by a test.
- **CI cannot produce it.** `pipeline.yml`'s `gpu-tests` job (:411) does not set
  `PCK_WRITE_GPU_RESULT` anywhere, rsyncs `testdata/goldens` **to** the host only, pulls nothing
  back, and ends with `rm -rf $REMOTE_DIR` (:681). `validate-large.yml` reaches shad-gpu but only
  validates datasets. `exec-model-corpus.yml` is `ubuntu-latest`, CPU. Every device path in the
  tree terminates at shad-gpu or verda-gpu; nothing else reaches a card.
- **Synthesising it from `duckdb-result.txt` or `mini.result.txt` would be fabrication**: the
  `duckdb_gpu_*` cases would then compare DuckDB with DuckDB and pass vacuously, which is the exact
  failure deviation 4 in Round 1 was written to prevent.

### The device files CAN be compiled before the cycle, and CI already does it

Both never-compiled files are gated on `#[cfg(not(feature = "rust-only"))]`
(`tests/test_gpu_corpus.rs:6`, `src/test_support/mod.rs:16`) — **not** on `feature = "gpu"`. So any
non-rust-only build type-checks them. Two consequences:

1. **`cpp-build-2502` already is that check.** `pipeline.yml:383-387` runs
   `cargo test --no-run -p peacockdb-core --test test_gpu_corpus --features gpu` (and the lib), in
   a `rapidsai/base:25.02` container on `ubuntu-latest` with `CUDF_ROOT=/opt/conda`. It compiles
   **and links**. A type error in `test_gpu_corpus.rs` or `corpus_gpu.rs` fails that job with no
   device involved. PR #167's CI run therefore already answers "do these two files compile?" —
   read that job's log before anyone waits on a host.
2. **Locally it is `./scripts/build-test-shadgpu.sh --build`** — phase-separated at
   `build-test-shadgpu.sh:193-228`, pure local (`scripts/build.sh` + `stage_cargo_test_binary …
   --features gpu`), **no ssh**; `--all` is `--build --push-binaries --patch --run` and only the
   last three need the host. Cheaper still, the same compile without the C++ install:
   `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core
   --test test_gpu_corpus --features gpu --no-run` (disk permitting — see above).

### What this means for how the task waits

- CI on PR #167 **will be red** regardless of the host: `dataset-matrix` runs
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus` (`pipeline.yml:293`),
  which is where all 27 cases live. `done` requires CI green, so the task cannot reach `done`.
- The count is **26 + 1**, not 27 + 1: `testdata/cost-registry.csv` has 26 cells at
  `enabled|skip` across the five `gpu_*` columns (tpch 22, tpcds 4), so 26 `duckdb_gpu_*` cases
  plus `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`.
- The coverage guard reads `gpu_result_golden(dataset, sf, **None**)`
  (`tests/test_cpu_corpus.rs:372`) — it has no `PCK_GPU_RESULT_VERSION` override, unlike
  `duckdb_gpu_case` (`test_support/duckdb_oracle.rs:331`). So a versioned file can exercise the
  comparison but can never satisfy the guard. That asymmetry is deliberate and correct.

## Round 2, recovered from a dispatch that died (2026-10-08)

The coordinator run that opened round 2 was terminated while the developer was still working,
and its window went with it. What survived is the work itself, uncommitted in the worktree, and
this record of it. **The reviewer's round-1 list did not survive** — it was never written here,
which is the mistake this section exists to not repeat. The status file carried only its shape:
one blocking finding and four important ones.

### The findings, reconstructed from the uncommitted diff

Reconstructed, not quoted — read them as what the work addresses, and let round 2's reviewer be
the judge of whether anything else on the original list is still open.

1. **Blocking, and CI confirmed it**: `duckdb_result.py` imported `duckdb` at module top level,
   so `test_duckdb_result.py` — which asks only about `cell` and `fingerprint` — died with
   `ModuleNotFoundError` in a CI job that installs no duckdb wheel. Both import sites are now
   deferred into `generate` and `main`, the way `duckdb_cost.py` defers `pyarrow.parquet`.
2. **The fingerprint classed its columns from the rendered cells, so a decimal left the hash.**
   `reads_as_inexact` called anything with a `.` approximate, which put every decimal column
   into a sum/min/max triple instead of the row hash — and a triple cannot tell two rows whose
   decimal cells are swapped from the right answer, which is the whole point of hashing an
   over-cap join. Now a column is approximate only if its declared type is a float:
   `is_approximate(&DataType)` on our side, `isinstance(value, float)` on DuckDB's. Six
   fingerprint sections were regenerated on both sides and the two writers still agree byte for
   byte (tpch anti-join, filter-project, semi-join carry the same `hash:` in
   `duckdb-result.txt` and `mini.result.txt`).
3. **The rendered side needed the other side's classes.** With the class no longer readable off a
   rendering, fingerprinting the under-cap side on its own would call one column approximate here
   and exact there. `compare_over_cap` now takes the classes from whichever side is fingerprinted
   and renders the other under them; `fingerprint_of_rendered` takes `&[bool]`.
4. **`assert_results_match` could only panic**, so the whole-answer oracles could not be shown
   failing without a run. Split: `results_match -> Result<(), String>` holds the comparison and
   `assert_results_match` panics on its `Err`.
5. **`a_device_run_under_a_regeneration_writes_no_golden` would have failed its own recording
   cycle.** It snapshotted `gpu-result.txt` and demanded it come back byte for byte, but
   `build-test-shadgpu.sh` exports `PCK_WRITE_GPU_RESULT` into every binary — so on the one run
   that writes the file the guard would go red for doing its job. The file now joins the snapshot
   only when that variable is unset.

### What is left before this task can move again

- **None of it is verified.** No test run reached this record, nothing is committed, and the
  reformatting a `rustfmt` pass will want has not happened. That is the next dispatch.
- The task stays at `reviewing`: round 2's reviewer has not seen this work.
- The device gap is unchanged. shad-gpu did not answer at 03:00 either
  (`connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`), and
  `list_verda_instances.sh` cannot even look for verda's IP here —
  `VERDA_CLIENT_ID is not set in the environment` — so CPU runs are local.
- **The last push was documentation only, so the whole pipeline skipped** and PR #167's checks
  read `skipping` across the board. The 677-passed/27-failed tally is from the `3a6c5343` push.
  The next push carries code and will run for real.

## Round 2 result (2026-10-08)

The dead dispatch's work is verified and complete. Every command below was run locally on
this workstation; neither remote host answered, so the device half is untouched.

### Case counts, measured

| target | round 1 | round 2 |
|---|--:|--:|
| `--lib` (rust-only) | 636 | 643 — 641 passed, 2 ignored |
| `test_cpu_corpus` | 704 | 704 — 677 passed, 27 failed |
| `test_golden_format` | 36 | 38 |
| `test_corpus_goldens` | 26 | 26 |
| `test_cost_model` | 3 | 3 |
| `test_module_layout` | 17 | 17 |
| `test_ci_coverage` | 9 | 9 |
| `testdata/test_duckdb_result.py` | 9 | 9 |
| `test_gpu_corpus` under `--features gpu` | never compiled | 28 cases, built and linked |

`--lib` gains 7 from the new `test_support/result_text/tests.rs`; `test_golden_format` gains 2
from the decimal-hash cases the dead dispatch wrote. The 27 red are the 26 `duckdb_gpu_*` and
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, unchanged, every one of
them carrying the "gpu-result.txt does not exist" message and nothing else.

### The device files compile — round 1's biggest risk is closed

`CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core
--test test_gpu_corpus --features gpu --no-run` exits 0 and links the binary. Cold it took 6m30s
and 7.9 GB in `target-cudf-rapids-cuda-12.2`; warm, 14 seconds. `cargo-cudf.sh check -p
peacockdb-core --tests --features gpu` covers the `lib test` target, which is where
`corpus_gpu.rs` and the `gpu_tests` modules live, and also exits 0. So `test_gpu_corpus.rs` and
`corpus_gpu.rs` have now been type-checked and linked on this box, and the 28 cases are in the
binary — `a_device_run_under_a_regeneration_writes_no_golden` among them. Nothing was run: there
is no card here.

**Disk, after that build: 7 GiB free on `/`.** `target/` is 19 GB and
`target-cudf-rapids-cuda-12.2` 7.9 GB. The cudf dir is the warm cache
`build-test-shadgpu.sh --build` shares, so it is kept rather than deleted; but a device cycle
also builds C++ into `cpp/build`, which is empty today, and 7 GiB may not hold it. Free space
before that cycle.

One warning the gpu shape shows and the rust-only shape cannot: `unused import: AsArray` in
`src/tests/gpu_tests/aggregate_dimension_cases.rs:9`. Pre-existing, outside this task's files,
left alone.

### The regeneration is reproducible, not hand-edited

Both writers were re-run and both produced the committed bytes exactly.

- Rust: the four over-cap cases under `UPDATE_CANONICAL=1 PCK_UPDATE_SECTIONS=1` at one thread,
  5.4 seconds. `mini.result.txt` came back byte for byte.
- Python: `python3 testdata/duckdb_result.py`, both datasets, duckdb 1.5.4, about 7 minutes.
  Both `duckdb-result.txt` came back byte for byte, no `failed:` section in either.

Seven fingerprint sections exist across the three files and all seven are consistent. tpch
`duckdb-result.txt` holds five — q11, q16, anti-join, filter-project, semi-join — and
`mini.result.txt` the four of them our side answers; the four shared sections carry the same
`hash:` on both writers. tpcds `duckdb-result.txt` holds q98 alone and tpcds
`mini.result.txt` holds none. Every triple that remains is a real float: q98's col 6
(`revenueratio`, a double). Every decimal lost its triple and joined the hash — q11 col 1,
anti-join and semi-join col 3, filter-project col 1, q98 cols 4 and 5. q16 never had one: its
four columns are two strings and two integers.

### Changed beyond what the dead dispatch left

1. **`rustfmt` on `fingerprint.rs` and `test_golden_format.rs`.** Two over-long lines, the
   `assert_eq!(row.len(), width, …)` and two `assert_eq!(compare_over_cap(…), Ok(()))`. No other
   touched file moved, so the diff stays readable.
2. **`fingerprint.rs`'s module doc was stale and now says what the code does.** It still claimed
   the class is read off the rendered cells and that there are two passes over the rows. Finding
   2 reversed both. **This supersedes deviation 6 of round 1**, which recorded the old rule.
3. **One clippy warning the new test introduced**, `cloned_ref_to_slice_refs` at
   `test_golden_format.rs:703`. Fixed by binding the batch into a one-element array. The three
   clippy warnings left in the touched files are all present at `3a6c5343`:
   `type_complexity` on `fingerprint`'s `each_row` argument, `cloned_ref_to_slice_refs` at
   `test_golden_format.rs:644`, and a constant assertion in `duckdb_oracle/tests.rs:123`.
4. **`test_support/result_text/tests.rs` is new, seven cases.** Finding 4 split
   `results_match` out of `assert_results_match` so a wrong answer could be shown failing
   without a run, but nothing drove the new `Err`. A split no test reaches buys nothing, so
   these drive it: the tolerant arm accepting a reassociated float and failing past its
   tolerance, on a row the oracle does not have, on a missing row and on a duplicated one; the
   exact arm failing on the drift the tolerant one accepts; and the panicking wrapper still
   naming its query. Proved by mutation — a `return Ok(())` at the top of `results_match` turns
   six of the seven red, and restoring it turns them green.

### The findings list, as round 2 leaves it

All five are answered and all five are now exercised by a case.

1. The deferred `duckdb` import is proven both ways. `testdata/test_duckdb_result.py` runs 9/9
   green with `duckdb` blocked from `sys.meta_path`, and `HEAD:testdata/duckdb_result.py`
   imported under the same block still raises `ModuleNotFoundError`. That is the CI red, and it
   is gone.
2. The decimal-in-the-hash change is proven by
   `a_decimal_column_is_hashed_and_a_swap_within_it_fails` and by the regenerated goldens
   agreeing across the two writers.
3. The rendered side under the other side's classes is proven by
   `a_rendered_side_is_fingerprinted_under_the_fingerprinted_sides_classes`.
4. The `results_match` split now has the seven cases above.
5. The recording-cycle guard reads `PCK_WRITE_GPU_RESULT` before it sets anything, and the file
   it edits compiles and links under `--features gpu`. It cannot be run without a card.

### What the next person needs

- **`compare_over_cap`'s `(false, false)` arm is unreachable from production.**
  `compare_sections` rejects "neither side fingerprinted" upstream with a message naming
  `duckdb_fingerprint`, so no corpus line can reach the arm. It is reachable from the public
  `test_support::compare_over_cap`, and the third assertion of
  `a_rendered_side_is_fingerprinted_under_the_fingerprinted_sides_classes` drives it. Left in
  place, named here rather than deleted — deviation 1 is the precedent for retiring a
  defensive arm, and it does not apply: this arm has a caller and a case.
- **The two writers class an all-null float column differently.** Ours reads the declared type,
  so a `Float64` column of nothing but nulls is approximate with an empty triple; DuckDB's reads
  `isinstance(value, float)` over the rows, finds no float and hashes the column instead.
  `compare_fingerprints` then fails naming the column — loudly, not silently — and no over-cap
  section has such a column today. The fix, if one is ever wanted, is `cursor.description`'s
  type codes in `fingerprint`'s signature. Not done: no line needs it.
- **`rendered_width` reads the header line, not the first data line**, so a zero-row answer
  still gives its width. The one degenerate case is tpcds q17's `++\n++`, which has no schema to
  take a header from (#205) and reads as width 1; it is `duckdb_divergent(205)` and never
  reaches the fingerprint path.
- The device gap is unchanged, and it is the whole of what is left. One
  `PCK_WRITE_GPU_RESULT=1` cycle through `build-test-shadgpu.sh --all` with `--pull-results`.
  #235 stays open until it runs, and `build-test.md`'s `goldens/` file counts need +1 per
  dataset in the round that commits `gpu-result.txt`.

## Review round 2 (2026-10-08)

A fresh reviewer over `git diff master...HEAD` at `b579007d`. **1 blocking, 5 important, 11 nits.**
It also checked the oracle positively, which is the part worth keeping: simulating
`compare_sections` in Python over the committed goldens, all 93 `duckdb_exact` lines pass as exact
multisets, all 15 `duckdb_approx` lines sit inside their tolerance (widest accepted gap 1.17e-5
relative, tpch q1 col 8), every `duckdb_divergent` column still differs past its tolerance while
every undeclared column of those lines agrees, and the only cases inspecting zero cells are the 4
`duckdb_none` lines plus tpcds q17. 58,650 cells compared. `test_duckdb_result.py`'s pinned hash
was recomputed from the documented algorithm by hand and matches.

### Blocking

1. **An empty `PCK_WRITE_GPU_RESULT` reads as "record a versioned file".**
   `corpus_gpu.rs:79-85` takes `Ok(asked)` and treats anything but `"1"` as a version suffix, but
   `build-test-shadgpu.sh:401,416` exports the variable unconditionally — so every ordinary
   `--run`/`--all` cycle writes `goldens/*/gpu-result-.txt`. Three things follow. The script's own
   comment at :415 and `build-test.md:1110-1112` say an empty value writes nothing, and both are
   false. `test_gpu_corpus.rs:85`'s new `var_os(...).is_none()` arm is unreachable on the only host
   that runs it. And `--pull-results`' `ls goldens/*/gpu-result*.txt` matches that file, so the
   `die "nothing came home"` guard at :733 cannot fire for the operator who forgot the variable —
   which is the one thing it exists to catch. The project's idiom for this is `cpp/src/expr.cpp:49`.

### Important

2. **`compare_fingerprints` passes vacuously on a NaN in a triple** (`fingerprint.rs:296`).
   `NaN > tol` is false, so a column whose `sum`/`min`/`max` is NaN on either side is not compared
   at all — and an approximate column is not in the hash either, so it goes wholly unchecked. This
   is reachable from our own writer: `triple_of` emits `min=nan max=nan` for a float column with no
   non-null value, and `number`/`float_of` round-trip `nan` deliberately. `cell_equal` handles NaN;
   this does not. The same shape, pre-existing, at `result_text.rs:306` and `device_answer.rs:187`.
3. **The two writers class a column from different evidence** (`fingerprint.rs:78` against
   `duckdb_result.py:88-91`): ours from the declared arrow type, DuckDB's from
   `isinstance(value, float)` over the rows. They agree unless no row carries a float, so an
   all-NULL double column is approximate here and exact there and the section can never pass —
   there is no `duckdb_divergent` on the fingerprint path. The same gap swallows the case the
   frozen spec names in step 3: "a decimal whose scale differs between the sides (a decimal on
   ours, a double on DuckDB's) is approximate". No section has either shape today.
4. **`build-test.md:686` said the over-cap fingerprint is "written and never asserted"**, which
   this branch's own `corpus.rs:462-467` falsifies — `assert_result_section` compares the committed
   section text, hash included, against a freshly computed one on every over-cap case, and that is
   the only thing pinning the Rust writer's hash to the goldens. Corrected by the coordinator.
5. **Nothing prunes `gpu-result.txt`** (`corpus_golden.rs:190-227`). `merged_cells` replaces or
   appends and never drops, so a device cell turned off keeps its section forever — while
   `test_cpu_corpus.rs:392-399` tells the reader to regenerate the file, which will not clear it.
   Only a hand edit will. `build-test.md:743-749` promises that turning a cell off and not
   regenerating fails here.
6. **tpch scan-limit declares its rows undetermined and then holds them to DuckDB exactly**
   (`corpus_cases.inc:45`). Its own comment says an unordered `LIMIT` over lineitem fixes no row
   set, which is why its cpu oracle is `data_fusion_subset`; `duckdb_exact` is the strictest oracle
   there is. It passes today as a fact about parquet read order and 4-way scan scheduling, not
   about the query. `each_declarations_two_oracles_suit_each_other` already derives `undetermined`
   from the cpu oracle and constrains the gpu one with it; it does not constrain the DuckDB one.

### Nits, and what became of them

Kept as cheap work in code the round is touching anyway: the comment-length caps
`coding-style.md` sets (`fingerprint.rs:1` 16 lines against 10, `corpus_gpu.rs:67` 11,
`test_cpu_corpus.rs:16` 12, `duckdb_oracle.rs:107` 5 against 4, `test_gpu_corpus.rs:79` 5);
`duckdb_oracle.rs:248`'s unchecked `row[column]`, so `duckdb_divergent(251, 99)` panics on an
index rather than naming the line; `duckdb_oracle/tests.rs:277` asserting `ticket_is_open(235)`,
the very ticket this task closes, so the case goes red the day it is archived; `judge`'s dead `at`
argument at `duckdb_oracle.rs:382`; and `macro_invocations` (`corpus.rs:575`) having dropped the
file path from its panics now that two case lists reach it.

Taken by the coordinator: the two wiki counts (`build-test.md:46` said the DuckDB tier is 149 of
the count and it is 148 — the third guard belongs to the declaration set) and
`architecture.md:1259`, whose heading named "the DuckDB oracle" when there are now two and that
section is the cost one.

Recorded and deliberately not fixed, because the fix costs more than the input is worth:
`fingerprint.rs:164` joins exact cells with `|` while `result_text.rs:341` splits rendered rows on
`|`, so one pipe inside a cell would both break the rendered parse and let `("a|b","c")` and
`("a","b|c")` hash alike — changing the separator moves every committed fingerprint, both writers
and the pinned Python hash, for an input no dataset in the tree contains. Likewise
`fingerprint.rs:155` counting `nonnull` as "the rendered cell is non-empty", which cannot tell an
empty string from NULL on either side — the same on both, so it compares correctly, and the
rendering is the only common ground the two engines have. And tpcds q66's twelve declared-divergent
columns clear their tolerance by only about 30%, so a small change on either side flips them to
"stopped diverging"; worth knowing when #251 is worked, not worth pre-empting.

## Round 3, recovered from a second dead dispatch (2026-10-08)

The run that dispatched the developer on review round 2's list died at 04:19, ten minutes after
the dispatch began (board commit `357c155b` at 04:06, status written 04:08, the developer's files
last written 04:18). Nothing reached this file, so the reconstruction below is read off the
uncommitted diff — 11 files, +179/−28 — and off which findings have no diff at all. Hosts
re-probed at 04:20: shad-gpu `connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed
out`, `verda` still resolves nowhere and no `VERDA_*` credentials are set. The ceiling is
unchanged.

### What the dead dispatch left, finding by finding

- **Blocking 1 (empty `PCK_WRITE_GPU_RESULT`) — done, and it looks right.** A three-armed
  `GpuRecording { No, Committed, Versioned(String) }` in `test_support/mod.rs`, read by
  `device_answer::gpu_recording_asked(Option<&str>)` so the rule is testable without an env
  var; `None | Some("")` is `No`. `corpus_gpu.rs:record_gpu_result` matches on it,
  `test_gpu_corpus.rs:85` replaces its unreachable `var_os(...).is_none()` with
  `gpu_recording() == GpuRecording::No`, and `build-test-shadgpu.sh:415`'s false comment now
  says empty is what a non-recording cycle sends. Case:
  `an_empty_recording_variable_is_not_a_version`.
- **Important 2 (NaN waved through a triple) — done at all three sites.**
  `result_text::nan_settles(a, b) -> Option<bool>` is the shared rule (`Some(true)` two NaNs,
  `Some(false)` exactly one, `None` defer to the caller's tolerance), called from
  `fingerprint.rs:296`, `result_text.rs:317` and `device_answer.rs:206`. Three cases, one per
  site: `a_nan_in_a_triple_is_compared_and_not_waved_through`,
  `the_tolerant_oracle_fails_on_a_nan_against_a_number`,
  `the_device_comparison_fails_on_a_nan_against_a_number`.
- **Important 3 (the two writers class a column from different evidence) — half-written and
  currently broken.** `testdata/test_duckdb_result.py` was rewritten for a three-argument
  `dr.fingerprint(names, types, rows)` and gained two cases
  (`test_a_decimal_is_hashed_and_a_double_is_summed`,
  `test_an_all_null_double_column_is_approximate_by_its_declaration`), but
  **`testdata/duckdb_result.py` itself was never touched** — its `fingerprint(names, rows)` still
  classes by `isinstance(value, float)` over the rows. Two further defects in what was written:
  the new cases say bare `TYPES` where the class attribute is `self.TYPES`, a `NameError`; and the
  all-null case expects `sum=0.00000000000000000e0 min=nan max=nan`, which the Python writer
  cannot produce today — `ordered[0]` raises `IndexError` on an empty column. So that file is
  red as it stands, and `fingerprint.rs`'s module doc still says `isinstance(value, float)`
  describes the other side.
- **Important 4 (`build-test.md:686`) — closed by the coordinator at `357c155b`.**
- **Important 5 (nothing prunes `gpu-result.txt`) — not started.** No diff in
  `corpus_golden.rs` or `test_cpu_corpus.rs`.
- **Important 6 (tpch scan-limit is `duckdb_exact` over an undetermined row set) — not
  started.** No diff in `corpus_cases.inc` or in `each_declarations_two_oracles_suit_each_other`.
- **The 11 nits — two done, the rest not started.** `corpus_gpu.rs:67` is now 9 doc lines
  (was 11) and `test_gpu_corpus.rs:79` is 4 body lines (was 5). Still over
  `coding-style.md`'s caps: `fingerprint.rs:1` at 16 against 10, `test_cpu_corpus.rs:16` at 12,
  `duckdb_oracle.rs:107` at 5 against 4. Untouched as well: `duckdb_oracle.rs:248`'s unchecked
  `row[column]`, `duckdb_oracle/tests.rs:277`'s `ticket_is_open(235)`, `judge`'s dead `at`
  argument at `duckdb_oracle.rs:382`, and `macro_invocations` (`corpus.rs:575`) having dropped
  the file path from its panics.

### Nothing in the uncommitted diff has been run

No `cargo` or `python3` output survived the dispatch, and `testdata/test_duckdb_result.py` is
provably red (the `NameError` above), so the whole diff is unverified. The coordinator did not
commit it: committing an unverified half-finished round is how a later run mistakes it for a
closed one.

## Round 3 result (2026-10-08)

Review round 2's list is closed. The dead dispatch's uncommitted diff was verified rather than
trusted, two defects in it were fixed, a third defect it could not have known about was found,
and the four untouched findings plus the nine remaining nits are done. Everything ran locally;
both hosts were re-probed at the end of the round and are still down — shad-gpu
`connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`, `verda` `Could not
resolve hostname`, zero `VERDA_*` in the environment. The device cycle is still the only thing
left in the task.

### Case counts, measured

| target | before this round | after | delta |
|---|---|---|---|
| `--lib` (rust-only) | 644 passed, 2 ignored | **646 passed, 0 failed, 2 ignored** | +2 |
| `test_cpu_corpus` | 704 cases | **705 cases: 678 passed, 27 failed, 0 ignored** | +1 |
| `test_golden_format` | 39 | **41 passed** | +2 |
| `test_corpus_goldens` | 26 | **26 passed** | — |
| `test_cost_model` | 3 | **3 passed** | — |
| `test_module_layout` | 17 | **17 passed** | — |
| `test_ci_coverage` | 9 | **9 passed** | — |
| `testdata/test_duckdb_result.py` | 11 (7 of them erroring) | **12, OK** | +1 |

`test_cpu_corpus`'s 27 failures are the standing device gap and nothing else: the 26
`duckdb_gpu_*` cases plus `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`,
each panicking with "…/gpu-result.txt does not exist, so no device answer is recorded… Run a
cycle with PCK_WRITE_GPU_RESULT=1 and bring it home with --pull-results" — 27 of 27 messages
checked, and `grep -v` over the failure list leaves 0 others. By family the file is 551 `cpu_`,
120 `duckdb_<ds>_<q>`, 26 `duckdb_gpu_*`, 8 guards.

Device targets type-check and link: `scripts/cargo-cudf.sh test --test test_gpu_corpus
--features gpu --no-run` links the executable, and `scripts/cargo-cudf.sh check --tests
--features gpu` finishes with one warning, `unused import: AsArray` at
`src/tests/gpu_tests/aggregate_dimension_cases.rs:9`, in a file this branch does not touch.

### What was wrong with the predecessor's diff

The reconstruction at the head of "Round 3, recovered from a second dead dispatch" was accurate
on every point. What it could not say, because nothing had been run:

1. **The three NaN cases and the recording case had never been proved red.** Each was reverted
   in turn and watched fail: `the_tolerant_oracle_fails_on_a_nan_against_a_number`,
   `the_device_comparison_fails_on_a_nan_against_a_number` and
   `an_empty_recording_variable_is_not_a_version` go red on `nan_settles`/`GpuRecording` reverted,
   and `a_nan_in_a_triple_is_compared_and_not_waved_through` goes red only when
   `fingerprint.rs`'s comparison is put back to its original `if (a - b).abs() > tol * …`. That
   last one matters: `!(x > t)` and `x <= t` are *not* the same for NaN, so the predecessor's
   flip of `>` to `<=` is half the fix and `nan_settles`'s `(true, true)` arm is the other half —
   without the arm, two NaNs (an all-NULL float column against itself) would start failing.
2. **`testdata/test_duckdb_result.py` was red**, as recorded: 7 errors of 11, `NameError: TYPES`.
   Fixed to `self.TYPES`.
3. **The all-NULL case expected the wrong bytes, and not for the reason the reconstruction
   gave.** It is not just that `ordered[0]` raises `IndexError`. The Rust writer emits
   `sum=-0.00000000000000000e0` for a float column with no value, because `Iterator::sum` for
   `f64` folds from `-0.0` (measured in isolation: `rustc` over an empty `Vec<f64>`). The Python
   writer folds from `0.0`. So even with the classes agreed, an all-NULL double column — or any
   float column whose values are all negative zero — would have disagreed on the sign of a zero
   and the section could never have passed. `triple_of` now folds from an explicit `0.0`, which
   is also what the Python side does, and `an_all_null_float_column_keeps_its_class_and_carries_an_absent_triple`
   pins the exact line both writers produce. No committed golden moves: the only approximate
   column in any committed fingerprint is tpcds q98 col 6, whose min is `+0.0` already, and the
   four tpch fingerprinted sections carry no triple at all.

### Finding 3, both halves

**The reachable half is closed.** `duckdb_result.py` classes from the declaration:
`fingerprint(names, types, rows)`, `types` being `cursor.description`'s second field, read at the
one call site. `is_approximate(declared)` strips any `(p,s)` suffix and looks the name up in
`APPROXIMATE_TYPES = {FLOAT, REAL, DOUBLE}` or `EXACT_TYPES`, and **raises** on anything in
neither — exhaustive rather than defaulted, per the antipattern about implicit behaviour switches.
The corpus's whole output-type set was enumerated first (`con.sql(q).types` over all 138 queries
of both datasets): BIGINT, INTEGER, HUGEINT, VARCHAR, DATE, DOUBLE, DECIMAL(15,2), DECIMAL(38,2),
DECIMAL(38,4), DECIMAL(38,6), DECIMAL(5,2), DECIMAL(7,2) — no TIMESTAMP, no BOOLEAN, no FLOAT.
Both `duckdb-result.txt` files were regenerated (`python3 testdata/duckdb_result.py`, ~7 min,
duckdb 1.5.4, both "wrote" lines present, 138 query lines, no `failed:`) and are **byte-identical**
to the committed ones: `git diff --stat testdata/goldens` and `git status --porcelain
testdata/goldens` are both empty.

**The residual half cannot be made to pass, and the spec's union rule cannot hold.** Each side's
`hash:` is one SHA-256 over the joined text of *that side's* exact columns, and it has to be one
hash rather than one per column because row *pairing* is what it exists to catch
(`rows_paired_differently_differ_in_the_hash_and_nowhere_else`). So the two sides must agree on
the exact column *set* at write time, and neither writer can see the other's declaration — ours
knows only the arrow type, the Python one only DuckDB's. A fingerprint no longer holds the rows,
so the hash cannot be recomputed under the other side's classes either; `compare_over_cap`'s
`under_the_classes_of` works only while one side is still rendered. The only rule that *would*
make the union hold is "approximate = any non-integer number", which moves decimals out of the
hash and gives up exactly the row-for-row checking step 3 asks for. So:

- `compare_fingerprints`' class-disagreement arm now names the column, which side called it
  which, and the remedy: "ours renders it exactly and DuckDB's approximately, so the two hash
  different columns and neither hash can be recomputed from a fingerprint. Class it alike on
  both sides — `is_approximate` here, `duckdb_result.py`'s there; duckdb_divergent does not
  reach this path." Case: `a_class_disagreement_names_the_column_and_the_remedy`, both
  directions, built from a real decimal batch against a real double batch.
- **For the coordinator:** the frozen spec's step 3 says "a decimal whose scale differs between
  the sides (a decimal on ours, a double on DuckDB's) is **approximate**". That is not
  achievable under two independent per-side writers and one row-pairing hash, and the branch
  implements the next best thing — an explicit, named, actionable failure. The spec is frozen,
  so this is a line for the signoff rather than an edit.
- A second shape this does *not* catch, recorded for whoever meets it: a decimal on BOTH sides
  at different scales. Both call it exact, both hash it, the rendered text differs, and the
  failure is an opaque hash mismatch. No section has that shape today and the spec does not name
  it.

`fingerprint.rs`'s module doc no longer says `isinstance(value, float)` describes the other side,
and is down from 16 lines to 10 — the two nits in one edit. Nothing load-bearing was dropped to
the wiki: the memory note moved nowhere (it is already on `fn fingerprint`) and the
rendered-side rule is already on `compare_over_cap` and `fingerprint_of_rendered`.

### Finding 5, decided: the writer prunes, keyed on the registry

`merged_cells` now drops a held section whose `(query, mode)` the registry no longer enables, and
keeps every section it does — so the hazard the dispatch named is answered by construction. A
filtered cycle (`PCK_TEST_FILTER`) restricts which cells *run*, not which cells are *enabled*, and
the rule reads the CSV that `merged_cells` already loaded for its ordering. `skip` counts as
enabled, matching the guard. `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`'s
"regenerate" is now true in both directions, so neither its message nor `build-test.md:742-750`
needed changing; the guard's doc gained a line saying the writer is what makes it true.
Case: `a_mode_keyed_merge_drops_a_cell_the_registry_no_longer_enables`, red before the fix with
the stale `== q12 mode=tp1-single` section still in the file.

### Finding 6, decided: `duckdb_exact` stays, with the reason written down

The line is not a latent flake, and nothing weaker in the five variants would still check
anything.

- `duckdb_<ds>_<q>` compares two **committed** files — `mini.result.txt`'s authority section
  against `duckdb-result.txt`'s — so it cannot vary run to run. Only a regeneration of either can
  move it.
- Both files hold `lineitem.parquet`'s first ten rows in file order. Measured: `select l_orderkey,
  l_linenumber from read_parquet('testdata/tpch.sf1/lineitem.parquet') limit 10` returns exactly
  the ten the committed sections carry.
- Weakening is unavailable, not merely unattractive. `duckdb_approx` still wants the same
  multiset. `duckdb_divergent` wants an open ticket — which a corpus-line property does not get —
  *and* wants the named columns to really differ, so it fails on agreement. `duckdb_fingerprint`
  wants an over-cap section and this one is ten rows. `duckdb_none` fails by construction while
  both sides answer ("duckdb_none over two sections that both exist").

So the reasoning is written where a reader meets it: four lines on the `corpus_cases.inc` block
(which stays at the ten-line comment cap — two older sentences about #186's device side were
folded into one to make room), a four-line comment at the `undetermined` derivation in
`each_declarations_two_oracles_suit_each_other`, and a new case,
`an_undetermined_lines_duckdb_oracle_is_still_one_that_compares_rows`, which holds every
`data_fusion_subset` line to a DuckDB oracle that compares rows and asserts there is still
exactly one such line — so a second one forces somebody to re-read this. Red-checked by flipping
scan-limit to `duckdb_none`.

### The nine nits

All done. Comment caps: `fingerprint.rs:1` 16→10, `test_cpu_corpus.rs:16` 12→9,
`duckdb_oracle.rs:107` 5→4. `duckdb_oracle.rs`'s unchecked `row[column]` is now a range check
ahead of the comparison, with `a_declared_position_past_the_last_column_names_the_line`
red-checked: without the guard it panics `index out of bounds: the len is 2 but the index is 99`,
with it the message names #251 and position 99. `duckdb_oracle/tests.rs`'s `ticket_is_open(235)`
is gone, with a comment saying why — #205 and #251 are what the corpus's divergent lines actually
name, and both stay asserted. `judge`'s `at` argument is gone: it was read only when `mode` was
`None`, and in that case it was always `"the cpu"`, which the body now says itself.
`macro_invocations` takes the **path** and reads the file, so all three panics and the
`assert_eq!` name it; `benchmark.rs`'s `read_cases` wrapper existed only to do that read and is
deleted.

### Deviations from the dispatch, and why

1. **The Python writer's empty-column sum is `+0.0`, and the Rust writer was changed to match.**
   The dispatch expected only the Python side to move for finding 3. The sign-of-zero disagreement
   above is a genuine second defect in the reachable half; closing it on the Python side instead
   (writing `-0.0`) would have put Rust's `Sum` identity into a committed file format, so the
   Rust side moved. No golden byte moves either way.
2. **A twelfth Python case, `test_a_column_type_neither_list_classes_is_refused`.** The dispatch
   did not ask for it. A declaration-based classifier with a silent default is the implicit-switch
   antipattern, and the raise needs a case or it is untested code.
3. **Clippy: five warnings in the touched files, not four.** `corpus.rs:500` (`DataFusion`
   prefix), `fingerprint.rs:132` (`type_complexity`), `benchmark.rs:108` (manual char comparison),
   `test_golden_format.rs:666` (`cloned_ref_to_slice_refs`, recorded at `:644` before this round
   added 82 lines above it), `duckdb_oracle/tests.rs:123` (constant assertion). All five were
   checked against `git diff -U0`'s hunks and none falls in a changed line, so all five are
   pre-existing; the round-2 record names three of them. One warning I did introduce,
   `collapsible_if` on the new range check, is fixed with a let chain.
4. **No wiki edit.** Four counts in `build-test.md` moved and are reported to the coordinator
   rather than changed here.

### Wiki lines the coordinator owns

- `build-test.md:25` — "1376 cases: `--lib` 643, `test_cpu_corpus` 704, …". `test_cpu_corpus` is
  **705**. `--lib` now reports 646 passed + 2 ignored; the `643` was already stale before this
  round, so the total wants recomputing rather than bumping.
- `build-test.md:46` — "the DuckDB tier, 148 of the count" is now **149**: 120
  `duckdb_<ds>_<q>` + 26 `duckdb_gpu_*` + `every_duckdb_oracle_is_named_by_some_line` +
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` +
  `an_undetermined_lines_duckdb_oracle_is_still_one_that_compares_rows`.
- `build-test.md:561` — the golden-format row's case count says 38; it is **41**.
- `build-test.md:566` — the Python row's case count says 9; it is **12**.
- `build-test.md:1114` — "Any other value (`PCK_WRITE_GPU_RESULT=26.02`) writes
  `gpu-result-<value>.txt`" should read **any other non-empty value**, now that empty reads as
  absent. The two script comments that said the same thing are fixed in this diff
  (`build-test-shadgpu.sh:113-115`, `:394-395`).
- Optional, `build-test.md:742-750`: the paragraph is true as written, and one clause would make
  it more useful — a cell turned off loses its `gpu-result.txt` section on the next recording
  cycle, because the writer keeps what the registry enables and drops what it does not.

### For the next person

- Nothing but the device cycle is outstanding. `PCK_WRITE_GPU_RESULT=1` through
  `build-test-shadgpu.sh --all` with `--pull-results`, then the 27 red cases go green or each
  failing section is a ticket.
- The device file's writer now prunes, so the **first** recording cycle after this branch lands
  will also clear any section for a cell that has since been turned off. Read
  `git diff testdata/goldens/*/gpu-result.txt` for *disappearances* as well as for moved answers.
- `testdata/duckdb_result.py`'s `EXACT_TYPES` is the list to extend when a corpus query first
  returns a type nobody listed — the regeneration raises rather than guessing, naming the type.

### The wiki counts, applied — and the DuckDB tier is 148, not 149

A researcher recomputed the page's arithmetic rather than bumping the four numbers, because
`build-test.md`'s grand total claims to be the sum of its own table rows and the four deltas do
not land on rows that exist.

**The tier count: the developer's 149 is wrong and the 148 already on the page is right.**
`test_cpu_corpus` holds eight non-expanded cases, and round 3's
`an_undetermined_lines_duckdb_oracle_is_still_one_that_compares_rows` is a declaration check, not
a DuckDB one — it iterates `CorpusDeclaration`, holds a `data_fusion_subset` line to an oracle
that compares rows, and reads no result file. The page's own prose arbitrates: the DuckDB tier's
enumeration is closed at "one case per line, one per cell, and two more cases", while the
declaration set is "checks that every declaration's oracles suit each other". So the tier stays
120 + 26 + 2 = 148, and the declaration set goes from three checks to five — review round 2's
total was right, though the guard it blamed was the wrong one.

**The page was 175 cases low, and 170 of that is this branch.** Its rows summed to 2505 against
a stated 2330; five cases of that predate the task (measured at the branch base `5a1eac45`), and
the three committed rounds grew the rows by 170 without touching the total. Four of round 3's
five new `--lib` cases had no row to land on, because the cpu block carried no row for
`test_support::duckdb_oracle::tests`, `::result_text::tests`, `::device_answer::tests` or
`::corpus::tests` — three of those modules are this task's own, so the omission is this task's to
close. Applied: the four module-unit rows added, `Golden merge` renamed and 1 → 3, `Recipes per
join type` 23 → 24 (pre-existing, unrelated), `Corpus, cpu` 703 → 704, the cpu header 1376 → 1382
(`--lib` 648, `test_cpu_corpus` 705), the golden-format row 38 → 41, the Python result-rendering
row 9 → 12, and the grand total 2330 → **2559** (Rust 2069, C++ 97, Python 393). The cpu block's
rows now sum to its header exactly, and the three block headers plus the "Everything else" table
sum to the grand total.

Also taken: `build-test.md:1116`'s "any other value" is now "any other non-empty value" with the
reason, and the `gpu-result.txt` regeneration paragraph gained a clause for the pruning the
writer now does.

## Review round 3 (2026-10-08)

A fresh reviewer over `git diff master...HEAD` at `bd245140`. **0 blocking, 4 important, 8 nits.**
Everything it reports was verified by running Python over the committed artifacts; it ran no
cargo.

**The positive half, which is the part worth keeping.** It reimplemented `compare_sections`,
`same_multiset`, `cell_equal`, `compare_over_cap`, `compare_fingerprints` and
`fingerprint_of_rendered` in Python and ran them over all four committed goldens. All 120 lines
pass under the oracle they declare, and the split is 93/15/4/4/4 as the page says. No line claims
*less* agreement than it has: every `duckdb_approx` and `duckdb_divergent` line has a cell that
really differs, and dropping any one named column from q58, q61 or q66 reddens the line. Four
mutation classes bite — a flipped digit reddens 93 of 93 `duckdb_exact` lines, a dropped row 108
of 108, a row-repairing swap 66 of 68 (the two misses are swaps between rows that differ only in
the swapped column, so the multiset is unchanged and nothing moved), and a flipped hash byte, a
`rows=` off by one or a `nonnull` off by one reddens all four over-cap sections. The tolerance is
a tolerance: 3 units in our last rendered place is red and 0.3 passes, on 12 of the 15
`duckdb_approx` lines (the other three are resolution-bound, in the safe direction). The four
over-cap fingerprints agree byte for byte across the two writers, hashes included, over tpch q16
(18,314 rows), semi-join (303,959), anti-join (1,196,041) and filter-project (2,402,187). No case
inspects zero cells; 58,650 rendered cells plus four fingerprints. No data cell in any golden
contains a `|`. And the spec's Restriction holds: master's `corpus_cases.inc` and HEAD's carry the
same 120 lines with identical modes, cpu oracle, gpu oracle and schema validation, and
`mini.result.txt`'s only change is four `skipped:` sections becoming fingerprints.

It also confirmed this round's recomputed counts independently, by summing every numeric table row
(2559) and every `#[test]` in the modules that gained rows, and says no line on `build-test.md` is
wrong.

### Important

1. **`tpch/scan-limit`'s `duckdb_exact` argument is about the cpu case only, and the device case
   is a latent red.** The argument written at `test_cpu_corpus.rs:220-248` is correct and is
   entirely about `duckdb_<q>`, which compares two committed files. `duckdb_gpu_case`
   (`duckdb_oracle.rs:322`) applies the same `duckdb_oracle` to the device's *recorded* answer,
   and for an unordered `LIMIT 10` over lineitem the device's ten rows need not be the cpu's. It
   cannot fire today only because `scan_limit`'s `gpu_modes` is `none`, and nothing in the branch
   ties those two facts together — `each_declarations_two_oracles_suit_each_other` constrains
   `gpu_oracle`, which `duckdb_gpu_case` does not read. The day #186 turns those cells on, three
   `duckdb_gpu_tpch_scan_limit_*` cases are a coin flip.
2. **`--pull-results`' "nothing came home" guard goes inert the moment `gpu-result.txt` is
   committed.** `build-test-shadgpu.sh:258` pushes `testdata/goldens/` to the host on every
   `--push-binaries`, which `--all` includes. So from the first commit of the file onward a
   non-recording cycle leaves the pushed committed copy on the host, the glob at :736 matches it,
   `pulled` becomes non-zero and the phase reports files having refreshed nothing. Nothing
   compares the file against its previous version by design, so the operator then reads an empty
   `git diff` as "no device answer moved" when it means "nothing was recorded". The guard needs
   freshness, not existence.
3. **The Rust writer's fingerprint hash is pinned nowhere, and a comment claims it is.**
   `test_golden_format.rs:554` asserts only `fp.contains("\nhash: ")`, while
   `test_duckdb_result.py:7-9` says the Python expectation is what that case pins on the Rust side
   "byte for byte". True of the four `rows=`/`col`/`sum` lines, false of the `hash:` line — the one
   that carries the row pairing. A Rust-side change to the separator, the sort or the join leaves
   both tests green and is caught only by the dataset-bearing corpus tier.
4. **The exact-column hash separates cells with `|`, and the same crate already rejected that
   choice for this exact reason.** `fingerprint.rs:158-159` and `duckdb_result.py:132` join cells
   with `|` and rows with `\n`; `result_text.rs:57-60` picks `\u{1}` instead and says why — with a
   separator that occurs in data, `("a|b","c")` and `("a","b|c")` hash alike, "two different
   answers agreeing, on the comparison that has no second opinion behind it". The fingerprint's
   hash is exactly such a comparison, since an approximate column is out of the hash as well.
   `fingerprint_of_rendered` would panic on its cell-count assertion rather than return a verdict.
   **Round 2 recorded this and declined it**; round 3 reverses that, and the reasons are new: the
   in-tree precedent two files away, the panic rather than a verdict, and 1.5M rows of
   `o_comment` hashed in anti-join and semi-join. The cost is bounded and measured — five hashes
   in two goldens, both writers, one pinned Python constant, and both regenerations are
   reproducible (Rust 5s, Python 7m).

### Nits

Comment caps, new overruns against the 10-line declaration cap: `test_cpu_corpus.rs:160-172` at
13 and `:220-230` at 11, `test_gpu_corpus.rs:60-70` at 11. `DuckdbOracle::ALL`
(`mod.rs:462`) is an array of `&'static str` where `CpuOracle::ALL` and `GpuResultMode::ALL` are
arrays of variants, so a sixth variant could be used by a line without appearing in `ALL`.
`duckdb_oracle.rs:301` turns a negative declared decimal scale into a tolerance of `10^|s|`
(`scale.max(0)` closes it; unreachable from the corpus). A zero-column header has no pipes, so
`split_cells` reads its width as 1 and tpcds q17 reports "1 columns against 15" when it has none
(`duckdb_oracle.rs:203-205`, `fingerprint.rs:93-97`) — message only. `fingerprint_of_rendered`
trims cells through `split_cells` while `fingerprint_of` takes them straight from
`ArrayFormatter`, so their equivalence case holds only for cells without surrounding whitespace —
worth a clause in the doc. `triple_of` sorts with `partial_cmp(..).unwrap_or(Equal)` and Python
uses `sorted()`, neither deterministic for a real NaN in an approximate column; untested on both
sides, and no committed section has an approximate column. `build-test.md:343`'s new row says
"the comparison every cpu corpus case runs", which is not true of `data_fusion_subset` —
coordinator's. `ticket_is_open` reads `llm-wiki/tickets/` through `CARGO_MANIFEST_DIR` with no
environment escape, against `testdata.rs:3-5`'s rule that the environment wins because a binary
is built on one host and run on another (#49) — the reviewer read this rather than testing it,
and `test_module_layout`, `test_ci_coverage` and `benchmark.rs` are already in that class, so it
is pre-existing in kind.

### For the signoff

- The reviewer **agrees with the branch** on the spec's step 3: a writer cannot know the other
  side's declared type, a fingerprint no longer holds the rows, and the reachable alternative
  (class every decimal approximate) is strictly weaker — it would take `o_totalprice` out of
  semi-join's and anti-join's row pairing and leave 1.5M rows to a float sum, against step 3's own
  "an all-integer over-cap join is then checked row for row". It also confirmed the shape does not
  occur: all four committed over-cap pairs have zero approximate columns, and tpch q11's DuckDB
  fingerprint, the one #190 will add, is all-exact too.
- **A consequence to name in the signoff, not a defect:** there is no way to record a *tolerated*
  over-cap divergence. `duckdb_fingerprint` takes no ticket, and a fingerprinted section under
  `duckdb_divergent` is routed to "declare duckdb_fingerprint". So the first genuine class
  disagreement forces a decision at both writers rather than a line edit.
- The 27 red cases are honest: both guards panic on an absent file with the regeneration recipe,
  the coverage guard compares both directions, an empty or `skipped:` body would still redden, and
  none of the 26 device cells carries `duckdb_none`. The registry-keyed pruning cannot be
  destroyed by a `PCK_TEST_FILTER`ed cycle.

## Round 4 result (2026-10-08)

Review round 3's list is closed: 4 important and 8 nits, of which two nits were the
coordinator's and one is argued down rather than done. Everything ran locally; both hosts were
re-probed and are still down — shad-gpu `connect to host llm-gpu0h200.velkerr.ru port 22:
Connection timed out`, `verda` `Could not resolve hostname`, zero `VERDA_*` in the environment.
The device cycle is still the only thing left in the task.

### Case counts, measured

| target | before this round | after | delta |
|---|---|---|---|
| `--lib` (rust-only) | 646 passed, 2 ignored | **648 passed, 0 failed, 2 ignored** | +2 |
| `test_cpu_corpus` | 705 cases | **705: 678 passed, 27 failed, 0 ignored** | — |
| `test_golden_format` | 41 | **43 passed** | +2 |
| `test_corpus_goldens` | 26 | **26 passed** | — |
| `test_cost_model` | 3 | **3 passed** | — |
| `test_module_layout` | 17 | **17 passed** | — |
| `test_ci_coverage` | 9 | **9 passed** | — |
| `testdata/test_duckdb_result.py` | 12 | **14, OK** | +2 |

`test_cpu_corpus`'s 27 failures are the standing device gap and nothing else: the 26
`duckdb_gpu_*` cases plus `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`.
**27 of 27** carry "…/gpu-result.txt does not exist, so no device answer is recorded for this
cell. Run a cycle with PCK_WRITE_GPU_RESULT=1 and bring it home with --pull-results", counted
over the whole log rather than its tail, and no failing thread is any other case. The case total
is unchanged because finding 1 renamed and extended a test rather than adding one.

Device targets type-check and link: `scripts/cargo-cudf.sh test --test test_gpu_corpus --features
gpu --no-run` links `test_gpu_corpus-5013741728a486dd`, and `scripts/cargo-cudf.sh check --tests
--features gpu` finishes with the one pre-existing warning, `unused import: AsArray` at
`src/tests/gpu_tests/aggregate_dimension_cases.rs:9`, in a file this branch does not touch.

### Finding 1 — the device side of an undetermined line

`an_undetermined_lines_duckdb_oracle_is_still_one_that_compares_rows` is now
`an_undetermined_lines_rows_are_compared_only_against_committed_files`, and holds every
`data_fusion_subset` line to two things rather than one: its `duckdb_oracle` compares rows, AND
no `gpu_*` cell of that line is enabled or `skip`. The second half is what ties the two facts
the reviewer found untied — `duckdb_gpu_case` applies the same oracle to the device's *recorded*
answer, and an unordered `LIMIT 10` over lineitem need not return the cpu's ten rows.

- **The registry, not the declaration**, for the reason `every_device_cell_has_a_cpu_cell_at_the_same_mode`
  gives and in the same shape: a `CorpusDeclaration` carries the oracles and not the modes. The
  `gpu_` columns are what `duckdb_device_cases!` expands from, and the gpu binary's
  `the_registry_matches_the_gpu_corpus_in_both_directions` is what holds the two together.
- **Red-checked**: `gpu_tp1_single` flipped to `enabled` for `tpch,1,scan_limit` in
  `testdata/cost-registry.csv`, the case goes red with "its rows are undetermined and its device
  cells at [\"tp1-single\"] are on, so duckdb_gpu_* holds the device's RECORDED answer to
  duckdb_exact — ten unordered rows that need not be the ten the cpu committed. Compare the row
  count alone on the device side of such a line, or leave the cells off." CSV restored, zero diff.
- **The written argument now names its case.** The doc says the `duckdb_<q>` case is the one the
  "two committed files" argument is about and that `duckdb_gpu_<q>_<mode>` has no such footing;
  the five-oracle "weakening is not available" enumeration moved into the body, at the assertion
  it is about, to keep the doc at the ten-line cap. `corpus_cases.inc`'s scan-limit block says the
  same in one sentence and names the test; it is back at exactly ten lines.
- **It closes the family, not the instance**: the loop is over every `data_fusion_subset`
  declaration, both halves apply to each, and `checked == 1` still forces a human to re-read when
  a second such line appears. The guard fires the day [#186] turns those cells on, which is the
  point — the remedy is in the message.

### Finding 2 — `--pull-results` requires freshness, not existence

`device_result_files <testdata dir> <marker>` in `scripts/lib/shadgpu-env.sh` classifies every
`goldens/*/gpu-result*.txt` as `fresh <rel>` or `stale <rel>` by whether it is newer than the
gate launch's run-id file, and answers `no-marker` where no gate has ever been launched. Shipped
into the remote script with `declare -f`, the way `passed_count` already is. `--pull-results`
pulls the fresh ones, names the stale ones on stdout, and dies when nothing is fresh.

`launch_remote` writes `$REMOTE_STATE/gate.id` before the binaries run, so a file the cycle wrote
is newer and the copy `--push-binaries` rsynced is older — both mtimes are the host's own clock
(`resilient_rsync -r`, no `-t`, dates the destination at write time), so there is no skew to
reason about.

**Verified without a host, three ways.** (1) `bash -n` on both scripts. (2) The remote block
rendered exactly as the heredoc sends it — `$(declare -f device_result_files)` expanded with the
real `$REMOTE_REPO` and `$phase_id` — written to a file and `bash -n`'d. (3) The predicate run
locally against four hand-made trees, with the committed copies dated before the marker and the
recording after it: a stale-only tree gives two `stale` lines, a half-recorded tree gives one
`stale` and one `fresh`, a tree with no marker gives `no-marker`, and an empty tree gives nothing.
The **red** half is on the record too: the old `ls goldens/*/gpu-result*.txt` predicate lists two
files on both the stale tree and the fresh one, so `pulled=2` either way and the guard cannot
fire. The consuming loop was then exercised over all five listing shapes (stale-only, mixed,
`no-marker`, empty, unparseable) with `pull_one` stubbed: `die` on four of them, rc=0 on the mixed
one, and the unparseable line refused by name.

**One hole named rather than closed**, in the comment: a `--push-binaries` between the run and the
pull re-dates the pushed copies. `--all` does both in one invocation and in the other order, so
reaching it takes two deliberate invocations in an order nothing recommends.

### Findings 3 and 4 — `\u{1}` in the hash, and the digest pinned on both sides

Done as one change, since finding 4 moves the digest finding 3 pins.

- `fingerprint.rs` gained `const CELL_SEPARATOR: char = '\u{1}'` with the reason on it, and
  `duckdb_result.py` `CELL_SEPARATOR = "\x01"` pointing at it. `result_text.rs:57-60`'s argument
  is the one quoted: a separator that occurs in data makes `("a|b","c")` and `("a","b|c")` hash
  alike.
- **Red first, and the collision was demonstrated rather than argued.** Before the change both
  writers hashed those two answers to the same digest,
  `ca5b0a56d0a3a76d5d48ae8b12cec10343c5c6675ffe2508b70d085b031aff4d` — the same value on both
  sides, which is incidental further evidence that the two writers are one algorithm. The cases
  are `two_answers_a_separator_in_a_cell_would_merge_hash_differently` (Rust) and
  `test_two_answers_a_separator_in_a_cell_would_merge_hash_differently` (Python).
- **The digest is pinned on the Rust side.**
  `an_exact_column_is_hashed_and_an_approximate_one_is_summed` now asserts the whole line,
  `hash: 55d02283b07cc29ea0d3abeea4a1938ba4843ad8966bf6ee14b61e8d4b9f18b3` =
  `sha256("1\u{1}a\u{1}\n2\u{1}b\u{1}")`, and the Python constant is the same text. Watched red at
  the old `a6005c86bd5307686acd561b309b2b6d5c670c935fe527ca4ad10023851dd239`, which is the value
  the reviewer computed — so the hashed text is understood the same way on both sides. The two
  comments that claimed the pin (`test_duckdb_result.py:4-9`, the `Fingerprint` docstring) are now
  true and say the hash line is included.

### The regeneration, reproducible and bounded

- **Rust**, `UPDATE_CANONICAL=1 PCK_UPDATE_SECTIONS=1 … --test-threads=1` over the four over-cap
  cases: 5.41 s, four `hash:` lines in `mini.result.txt` and nothing else.
- **Python**, `python3 testdata/duckdb_result.py`, duckdb 1.5.4, both datasets: both "wrote" lines
  present, 138 query lines, no `failed:` in either file.
- **`git diff --stat testdata/goldens` is 3 files, 10 insertions, 10 deletions, and every one of
  the 20 changed lines is a `hash:` line** — counted, not eyeballed: `git diff -U0` minus the
  `hash:` lines leaves 0.
- **The four shared sections still agree byte for byte across the two writers**, hashes included:
  q16, anti-join, filter-project, semi-join, compared as text after dropping the `mode=` line.
  The new hashes are `e30e20cb…`, `cad6a433…`, `db8ce653…`, `47e84761…`.
- **Ten hash lines moved, not five.** The dispatch said five; the files hold ten — tpch
  `duckdb-result.txt` 5 (q11, q16, anti-join, filter-project, semi-join), tpcds
  `duckdb-result.txt` 1 (q98), tpch `mini.result.txt` 4 — over six distinct digests, since the
  four shared sections carry the same value on both writers. Every fingerprint with at least one
  exact column and at least one row moves, and all ten have both.

### The nits

Seven taken, one argued down, two were the coordinator's.

- **Comment caps.** `test_cpu_corpus.rs`'s `each_declarations_two_oracles_suit_each_other` 13 → 10
  with all four of its ideas kept, the undetermined doc rewritten at 10, `test_gpu_corpus.rs:60`
  11 → 10. `corpus_cases.inc`'s scan-limit block is 10 after finding 1 added a sentence, and
  `duckdb_result.py`'s triple comment is 4 inside a body after the NaN rule went into it.
- **`DuckdbOracle::ALL` is now `[DuckdbOracle; 5]`**, variants like `CpuOracle::ALL` and
  `GpuResultMode::ALL`, and `parse` decodes THROUGH it rather than against a second hand-written
  match — so a variant missing from `ALL` is a variant no line can name. `Divergent`'s entry
  carries a placeholder ticket nothing reads, documented as such; the bare spelling still reaches
  `divergent(spelled, &[])` so "takes a ticket first" stays one message in one place, and the
  typo panic builds its accepted set from `ALL`. **Red-checked** by dropping `Self::None` from
  `ALL`: `parse("duckdb_none")` then panics "unknown duckdb_oracle 'duckdb_none' (expected
  duckdb_exact|duckdb_approx|duckdb_divergent|duckdb_fingerprint; …)". Worth recording for
  whoever adds a sixth: the comparator's own matches are exhaustive over `&DuckdbOracle`, so a new
  variant is already a compile error there (E0004) — what was missing was only the `ALL` half.
- **`scale.max(0)`** at the decimal tolerance, with
  `a_negative_decimal_scale_does_not_buy_a_wider_tolerance` red-checked: at scale −2 a cell off by
  50 passed before and fails now.
- **A zero-column answer reports no columns.** `rendered_width` moved out of `fingerprint.rs` into
  `result_text.rs` beside `split_cells`, which is the function whose pipeless-line rule causes it,
  and returns 0 for a header with no `|`. Both width readers now call it, so tpcds q17 says
  "0 columns against 15". Case: `a_zero_column_answer_reports_no_columns`, red before at
  "ours has 1 columns against 15 in DuckDB's".
- **`fingerprint_of_rendered`'s doc** gained the clause: it agrees with `fingerprint_of` only for
  cells with no surrounding whitespace of their own, the padding being the table's and not the
  value's.
- **`triple_of`'s NaN is now deterministic on both writers**, which was the choice rather than
  documenting the limitation. A NaN among the values makes the WHOLE triple NaN on both sides —
  no sort places a NaN (`partial_cmp(..).unwrap_or(Equal)` calls it equal to everything, Python's
  `sorted` leaves it where the rows put it), so min and max were a function of each side's row
  order and the two engines return the rows in different sequences. `nan_settles` then reads two
  NaN triples as the one absent value they are. Red-checked on both sides at once, and the red was
  the same on both: `min=1.00000000000000000e0` against `min=nan` for the same three values in two
  orders, with identical hashes. No committed section has an approximate NaN, so no golden moves.
- **`ticket_is_open`'s missing environment escape: not done, and the reason is not "cosmetic".**
  An escape nobody sets is a no-op, and the complete fix is provisioning — `scripts/build-test.sh`
  must push `llm-wiki/tickets/` and export the variable — which is outside this task's `## Scope`.
  The reviewer's "pre-existing in kind" is understated and the correction matters: it is
  pre-existing **in fact**. `rust_only_targets` (`scripts/build-test.sh:280`, run to confirm)
  stages `test_corpus_goldens`, `test_cost_model`, `test_cpu_corpus` and `test_golden_format` to
  verda, and excludes `test_module_layout`/`test_ci_coverage` by the `repo_root|\.github/workflows`
  rule at :310. On master, `test_golden_format` and `test_corpus_goldens/benchmark.rs` already read
  the checkout through `CARGO_MANIFEST_DIR` and are already staged, so a verda rust-only run
  already has this class in two binaries. This branch adds a third (`corpus_lines` and
  `ticket_is_open` in `test_cpu_corpus`) and does not change whether such a run is clean.
  **For the coordinator: this wants a ticket in `llm-wiki/tickets/testinfra.md`** — "rust-only test
  binaries read the checkout through `CARGO_MANIFEST_DIR`, so the remote CPU run cannot verify
  them", naming the four staged targets and the two-part fix (an escape per reader plus the push).
  Four cases of this branch's are in it: `duckdb_tpcds_q17`, `q58`, `q61`, `q66`, the only lines
  that call `ticket_is_open`.

### Deviations from the dispatch, and why

1. **Ten hash lines, not five** (above). The dispatch's expectation was a miscount; the measured
   diff is what the round delivers, and all of it is `hash:` lines.
2. **The undetermined test was renamed, not added to.** The reviewer offered "an assertion that a
   `data_fusion_subset` line declares `gpu_modes = none`, or a row-count-only comparison on the
   device side". Two separate tests would have repeated the under-reading the reviewer flagged
   twice, so both halves live in one case whose NAME states the whole property. The row-count-only
   device comparison is the remedy the failure message prescribes, for the round that enables
   those cells — writing it now would be an untested comparison for a case that cannot run.
3. **`cargo fmt -p peacockdb-core` formats the whole package**, and reformatted 18 files this
   branch does not touch. Reverted with `git checkout`, and the touched files were then formatted
   one by one with `rustfmt --edition 2024 <file>`. Worth knowing: the package is not
   rustfmt-clean, so the package-wide command is not safe in this tree.
4. **One clippy warning I introduced and fixed**: `needless_borrow` at
   `duckdb_oracle/tests.rs:151` in the new negative-scale case. The five warnings in touched files
   are the five round 3 proved pre-existing — `corpus.rs:500`, `fingerprint.rs:135`,
   `benchmark.rs:108`, `test_golden_format.rs:708`, `duckdb_oracle/tests.rs:124` — and each was
   re-checked against `git diff -U0`'s hunks this round: none falls on a line this round changed.
5. **No wiki edit.** Counts and two prose lines moved and are reported to the coordinator.

### Wiki lines the coordinator owns

- `build-test.md:7` — grand total **2559 → 2565**, Rust 2069 → 2073, Python 393 → 395.
- `build-test.md:25` — the cpu header **1382 → 1384**, `--lib` 648 → **650** (648 passed + 2
  ignored). `test_cpu_corpus` stays 705.
- `build-test.md:45` — "that an undetermined line's DuckDB oracle is still one that compares rows"
  is now **"that an undetermined line's rows are compared only against committed files"**: the
  test is renamed and holds the device cells off as well.
- `build-test.md:332` — the DuckDB-oracle module-unit row, 22 → **24**.
- `build-test.md:600` — the golden-format row, 41 → **43**.
- `build-test.md:605` — the Python result-rendering row, 12 → **14**. Its prose "the over-cap
  fingerprint must be the text `test_golden_format.rs` pins, byte for byte" is now true of the
  hash line too, which it was not before.
- `build-test.md:782-791` and `:1154`, optional but useful: `--pull-results` now requires a file
  to be NEWER than the gate launch, not merely present, because `--push-binaries` mirrors
  `testdata/goldens/` to the host; stale files are named and left, and a cycle that recorded
  nothing fails the phase.
- The tier count is unaffected: 120 + 26 + 2 = 148 still, and the declaration set stays at five
  checks — finding 1 renamed one of the five rather than adding a sixth.

### For the next person

- Nothing but the device cycle is outstanding. `PCK_WRITE_GPU_RESULT=1` through
  `build-test-shadgpu.sh --all`, then `--pull-results`, then the 27 red cases go green or each
  failing section is a ticket.
- **`--pull-results` is stricter now.** It needs a gate launch to have happened (it reads
  `$REMOTE_STATE/gate.id`) and it pulls only files newer than it. A stale file is reported and
  left in place rather than brought home, so the local committed copy is not overwritten by the
  copy the push put on the host.
- **Both writers' separator is `\u{1}`.** Any change to it, to the row sort, or to the join moves
  five digests in three goldens and must be made on both sides in one commit;
  `test_golden_format.rs`'s pinned literal and `test_duckdb_result.py`'s constant are the two
  places that go red, and they are the whole point of pinning it.
- The `duckdb_gpu_*` cases over a `data_fusion_subset` line are still unwritten work, not a
  solved problem: the guard refuses the cells, and the failure message says what to build instead.

## Completeness pass — the analyst's reading (2026-10-08)

A fresh analyst over the branch at `6c92f6b7`, asking what is missing. It walked the spec's eight
items, the Scope table, the Restriction and the Verification bar, and found items 1, 2, 5, 6 and 8
met, the Restriction holding, and the rust-only bar met including the two cases the bar names by
hand. Four things are not met.

1. **Nothing holds "the committed `gpu-result.txt` is cuDF 25.02's"** — the file carries no
   provenance at all. `merged_cells` rebuilds it from `== <query> mode=<mode>` sections and
   `ordered_sections` drops any preamble, so a 26.02 cycle run with `PCK_WRITE_GPU_RESULT=1`
   instead of `=26.02` writes the committed file and nothing can tell. Step 4's rule rests on the
   operator typing the right value. The analyst argues for fixing this **inside the task**, on a
   scheduling ground that is correct: the file does not exist yet, so stamping the version the
   writer ran under costs nothing now and costs another whole device cycle once the file is
   committed. Taken, and dispatched.
2. **Step 7 is two thirds met.** The step says the cpu helper's negative tests run under each
   `CpuOracle`; `CpuOracle::DataFusionSubset` routes to `assert_subset_of_unlimited`, which runs a
   live DataFusion query and takes no injectable answer, so nothing can hand it a wrong one. One
   corpus line uses it. Filed as **#254** rather than reopening the task, and named in the signoff.
3. **Step 3's decimal clause is not implemented and the substitute has no escape**, which the
   reviewer independently agreed is unachievable. The analyst's point is different and sharper:
   the consequence blocks the next two tasks and was recorded only as a sentence in a file that is
   deleted at merge. Filed as **#253** with the two other shapes that reach the same door.
4. **The panic text under `PCK_GPU_RESULT_VERSION` tells the reader to overwrite the committed
   file.** `duckdb_gpu_case` names the versioned path and then says "Run a cycle with
   `PCK_WRITE_GPU_RESULT=1`", which is the one thing a verify-26.02 developer must not do. One
   format string. Taken, and dispatched.

**What chain J will trip over**, which is the part of this reading nothing else produces:

- **`stale-cells`, the very next task, cannot express a device-only divergence** — the shape it
  exists to produce. One `duckdb_oracle` serves the cpu case and every device case, and both
  available values go red. #253's first half.
- **`join-backend` meets a fingerprint nobody has compared.** `duckdb-result.txt` already holds
  tpch q11 as `fingerprint: rows=27604`, both columns exact, no triple. When that task turns q11's
  cpu cells on, `mini.result.txt` becomes a fingerprint too and the whole comparison is one
  SHA-256 over 27,604 rendered `(ps_partkey, value)` rows — all or nothing, because
  `duckdb_fingerprint` takes no ticket. Nobody has measured our rendering of `value`, since no
  mode runs it. #253's second half.
- **`pbench` hits a hardcoded dataset list.** `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`
  iterates a literal `[("tpch", "1"), ("tpcds", "1")]` instead of deriving the datasets from the
  registry. pbench lands with its device cells off, so adding `("pbench", "1")` panics on a file
  the writer will never create, and not adding it leaves pbench uncovered until join-backend turns
  its cells on — which is exactly what this task is first in the chain to provide. Taken, and
  dispatched: derive the list, and settle what a dataset with no enabled device cell has.
  Separately `duckdb_result.py` pins `--dataset choices=["tpch","tpcds"]`, which pbench extends
  before it can have an oracle at all.
- **`verify-26.02` is in better shape than expected** — its impl plan already names
  `PCK_GPU_RESULT_VERSION=26.02` with a `duckdb_gpu_` filter, which keeps the coverage guard out
  of the run. It hits items 1 and 4 and nothing else.
- **A routing fact for every later dispatch**: #252 means any task that runs its CPU tier on verda
  sees `duckdb_tpcds_q17`, `q58`, `q61` and `q66` red for a reason unrelated to that task. Carry it
  into the dispatch rather than leaving it in a ticket file.

**`architecture.md`:** one sentence falsified, `:1271` — "**The DuckDB oracle** runs each query
twice" claimed a uniqueness round 2 removed when it renamed the heading at :1259 for the two
oracles the tree now has. Corrected to "**DuckDB's cost oracle**". The analyst offered `:1108` as
a second, at low confidence and flagging it as possibly growth; declined on that ground — the
sentence is not untrue, it merely names the weaker half of what would catch a column-order
defect.

Two bookkeeping notes from the same reading, neither a finding: the Scope table said "#235
archived" and #235 is instead kept open with its body cut back to the device cycle, which is
right given the ceiling and leaves the archival owed to whoever runs that cycle; and the spec's
optional sixth variant `duckdb_columns` was looked for and not needed — no `duckdb_divergent` line
is a LIMIT tie — which the spec asked the PR to say, so it is in the signoff.

## Round 5 result (2026-10-08)

The analyst's three items, all three done. Everything ran locally; both hosts stayed down, so
the device cycle is still the only thing outstanding in the task. No golden moved —
`git diff testdata/goldens` is empty, and `gpu-result.txt` still does not exist, which is the
whole scheduling reason item 1 was worth doing now.

### Case counts, measured

| target | round 4 | after | delta |
|---|---|---|---|
| `--lib` (rust-only) | 648 passed, 2 ignored | **657 passed, 0 failed, 2 ignored** | +9 |
| `test_cpu_corpus` | 705 cases | **706: 679 passed, 27 failed** | +1 |
| `test_golden_format` | 43 | **43 passed** | — |
| `test_corpus_goldens` | 26 | **26 passed** | — |
| `test_cost_model` | 3 | **3 passed** | — |
| `test_module_layout` | 17 | **17 passed** | — |
| `test_ci_coverage` | 9 | **9 passed** | — |
| `testdata/test_duckdb_result.py` | 14 | **14, OK** | — |

The +9 in `--lib`: `corpus_golden::tests` 3 → 9, `duckdb_oracle::tests` 24 → 26,
`device_answer::tests` 8 → 9. The +1 in `test_cpu_corpus` is the committed-file stamp guard.

**`test_cpu_corpus`'s 27 failures are the standing device gap and nothing else**, unchanged in
count and in text: the 26 `duckdb_gpu_*` cases plus
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, 27 of 27 carrying "does not
exist, so no device answer is recorded", counted over the whole log. The 26 read exactly as they
did in round 4 — `…/gpu-result.txt does not exist, so no device answer is recorded for this
cell. Run a cycle with PCK_WRITE_GPU_RESULT=1 and bring it home with --pull-results.` — because
the knob is interpolated and `PCK_GPU_RESULT_VERSION` is unset in an ordinary run. One word moved
in the 27th: the coverage guard's absent-file message now carries its dataset prefix
(`tpcds: …/gpu-result.txt does not exist…`), since the message comes back from a function and the
caller prefixes it the way the mismatch message was always prefixed.

Device targets type-check and link: `scripts/cargo-cudf.sh check --tests --features gpu` is rc=0
with the one pre-existing warning (`unused import: AsArray`,
`src/tests/gpu_tests/aggregate_dimension_cases.rs:9`), and
`scripts/cargo-cudf.sh test --test test_gpu_corpus --features gpu --no-run` links
`test_gpu_corpus-5013741728a486dd`. Not optional this round and it earned its keep: the gpu
target dir's `OUT_DIR/cudf-version-config.h` holds the real `CUDF_VERSION_MAJOR 25` /
`MINOR 2`, which is the end-to-end proof that a 25.02 build stamps `25.02` without a card.

### Item 1 — `gpu-result.txt` carries the cuDF it was recorded under

**Where it goes: a file-level provenance line, `cudf=<version>`, as the file's first line.**
The two reasons the per-section field lost. The rule the spec states is per FILE ("one version
per file"), and a per-section field makes a mixed file *expressible*, after which the guard has
to decide what a mixed file means; and the section bodies are exactly what the DuckDB comparison
reads, so a field inside one would have to be stripped back out again in the one place that must
not start guessing. `merged_cells` rewrites the whole file on every merge, so the line it writes
itself cannot go stale — which is the answer to "keeps `merged_cells` honest".

One consequence, and it is deliberate: **sections another cuDF recorded are dropped rather than
carried under this cuDF's stamp.** Without that the preamble would be a lie about every section
the merge did not write, which is worse than no stamp. So a 26.02 cycle that reaches the
committed file leaves a file that says 26.02 and holds 26.02's answers alone; the coverage guard
then reports the missing cells and the stamp guard reports the version.

**What it says: the cuDF the binary is LINKED against, read at build time.** The device's
`peacock_gpu_version()` was the dispatch's suggestion and it is not the cuDF version — it returns
the ENGINE's `"0.1.0"`, pinned by `cpp/tests/cpu/test_executor.cpp:9`. `peacock_cudf_version()`
is `verify-26.02`'s own step 2 and does not exist yet, and `libcudf.so` in the conda env carries
no version in its soname, so there is **no runtime source today**. The best reachable thing is
`$CUDF_ROOT/include/cudf/version_config.hpp`: `peacockdb-core/build.rs` copies it into `OUT_DIR`
and `device_answer::cudf_version_of_config` parses `MAJOR`/`MINOR` into `25.02` — the minor
padded to two digits, which is the one digit that matters and has a case.

**How the guard is weaker than a runtime read, stated plainly:** it is the cuDF the binary was
built against, not the one the loader bound. For this workflow they are the same thing —
`build-test-shadgpu.sh` builds locally against `rapids-cuda-12.2` and pushes binaries — but a
binary run against a different `libcudf.so` would stamp its build's version. **When
`peacock_cudf_version()` lands, `cudf_version()` should read it instead**; the doc on that
function says so at the site.

The rule is held in three places, one rule each:

- `cudf_a_path_promises(version)` — the cuDF a path's NAME promises: the suffix, or
  `COMMITTED_CUDF_VERSION` (`"25.02"`, declared once in `test_support/mod.rs`).
- **Read side**, `gpu_result_cudf_matches_path`: every reader of a `gpu-result` file checks the
  line against the path. `duckdb_gpu_case` does it right after the read, and
  `every_committed_gpu_result_file_carries_the_committed_cudfs_stamp` does it over the committed
  files. Both directions fail: 26.02's answers in `gpu-result.txt`, and 25.02's answers in a file
  named `gpu-result-26.02.txt`.
- **Write side**, `recording_cudf_suits_the_path`: `record_gpu_result` refuses before it writes.
  This is the half that takes the rule off the operator rather than detecting the breach
  afterwards — `PCK_WRITE_GPU_RESULT=1` on a 26.02 build now fails at the first recorded cell
  with "record with PCK_WRITE_GPU_RESULT=26.02 instead", so the committed file is never
  overwritten in the first place. The dispatch asked for stamp + test; this is the one addition
  beyond it, and the reason is that "the rule rests entirely on the operator typing the right
  value" is cured by checking the value, not by reading a diff later.

**Watched red, four ways.** (1) The parse: `cudf_version_of_config` red before it existed, then
green over the real 25.02 header, a 26.02 one, a `MINOR 10` one, the empty file a rust-only build
gets, and a header missing `MINOR`. (2) The writer: both merge cases red before the stamp
existed (the expected text begins `cudf=25.02\n`), and the drop-on-version-change case red
against a carried `== q1 mode=tp4-sized` section. (3) **The committed-file guard went red today
on a doctored file**, which is the thing the dispatch insisted on: a hand-written
`testdata/goldens/tpch.sf1/gpu-result.txt` holding `cudf=26.02` failed with
"recorded under cuDF 26.02 and read as cuDF 25.02's. Restore this file (git checkout) and record
26.02's answers with PCK_WRITE_GPU_RESULT=26.02 instead", and `duckdb_gpu_tpch_q6_tp1_single`
failed on the same line before it compared anything. The file was removed and
`git status testdata/` is clean. (4) Two `cudf=` lines panic rather than being read to the first
— the "reader that stops at the first match" shape, since a second line could contradict the
first unseen.

### Item 2 — the panic text names the value that records the file it read

`recording_knob(version)` is the only place `PCK_WRITE_GPU_RESULT=<value>` is spelled, and it
lives in `corpus_golden.rs` beside `gpu_result_golden` because the value and the path are one
choice. `duckdb_gpu_case`'s two messages interpolate it, as do the coverage guard's two and the
two mismatch messages. Red first: the function was written returning the bug (`=1` always) and
the case failed `left: "PCK_WRITE_GPU_RESULT=1", right: "PCK_WRITE_GPU_RESULT=26.02"`. Proven
end to end by hand, which is the part a unit test cannot show:
`PCK_GPU_RESULT_VERSION=26.02 cargo test … duckdb_gpu_tpch_q6` now says
`…/gpu-result-26.02.txt does not exist … Run a cycle with PCK_WRITE_GPU_RESULT=26.02`.

`build-test-shadgpu.sh`'s usage documents the read knob for the first time, and says the two
things a reader of that block needs: it is **not forwarded to the host** (`test_cpu_corpus` is
not staged there — `RUST_TESTS` is `test_gpu_corpus test_node_timing peacock_gpu_benchmarks`), so
it is set on the local cargo run after `--pull-results`; and it must name the value recorded
with. The write knob's entry gained the refusal. `bash -n` clean and the usage text rendered.

### Item 3 — the coverage guard's datasets come from the registry

`every_enabled_device_cell_has_its_gpu_result_section_and_no_other` loops
`registry_datasets()` — every `(dataset, sf)` `load_csv()` holds — and the both-ways comparison
moved into `duckdb_oracle::gpu_result_coverage(path, enabled, text)`, a function that takes the
file's text as `Option<&str>` so the degenerate case has a test at all.

**The degenerate case, settled:** a dataset with no enabled device cell has NO file, and that is
`Ok`. The reason is the writer — `merge_mode_section` runs only from a device case, so zero
enabled cells write zero sections — and it is how a dataset arrives: pbench lands with its
device cells off. A stray section for a cell nothing enables still fails ("not an enabled cell"),
so the pass is "no file and no cells", not "no file".

**Proven live against the real registry**, not only in the unit tests: with the four `tpcds`
`gpu_*` cells flipped to `disabled` in `testdata/cost-registry.csv`, the guard walked past tpcds
in silence and failed on tpch, which still has 22 enabled cells and no file. Before this round
the literal list made tpcds panic on a missing file whatever its enablement, which is exactly the
pbench failure the analyst predicted. CSV restored, `git checkout` zero diff.

`gpu_result_cells` in `test_support/mod.rs` had no caller left and is deleted;
`gpu_result_coverage` replaces it in the harness API.

**`testdata/duckdb_result.py`'s `--dataset choices` is NOT extended, and the one-word change is
not free.** `choices=["tpch","tpcds"]` is at `:230` and the default list is spelled a second time
at `:238`, so one word leaves them disagreeing; and `generate()` globs
`testdata/pbench-queries/*.sql`, which is empty until pbench's own task lands, so
`--dataset pbench` would write an EMPTY `duckdb-result.txt` rather than fail — a silent green
no-op in place of argparse's "invalid choice". That is a worse error than the one it replaces, so
pbench's task owns the choice, the queries and that guard together. Worth knowing before
somebody tries the one-liner.

### Deviations from the dispatch, and why

1. **`peacockdb-core/build.rs` is outside the spec's `## Scope` table.** The stamp needs a value
   from outside the Rust tree and there is no runtime source (above), so the build script copies
   one header into `OUT_DIR`. It copies rather than parses, so the rule stays in one tested
   function; it is skipped under `rust-only`, which must not start depending on `CUDF_ROOT` —
   that would re-run the script and recompile the rust-only tree whenever the variable moves;
   and a build with no `CUDF_ROOT` (`CUDF_BUILD_FROM_SOURCE=1` is a supported path in
   `scripts/build.sh`) writes an empty file rather than failing, with the write path naming
   `CUDF_ROOT` if it ever matters. No engine change, no component API change.
2. **The write-side refusal is more than the dispatch asked for.** Argued above: detecting the
   breach is not the same as not depending on the operator.
3. **The committed-file stamp guard passes on an absent file.** The coverage guard owns absence
   and already fails for it; two cases red for one reason would have made the 27 a 28 and bought
   nothing. The rule itself is exercised today by the rust-only cases over doctored text, and by
   the doctored real file above.
4. **`cargo fmt -p peacockdb-core` is still not safe in this tree** (round 4's finding). Each
   touched file was formatted with `rustfmt --edition 2024 <file>` and re-checked with
   `--check`.
5. **Wiki edited, not reported.** Round 4 handed its lines to the coordinator; this round the
   counts and the `gpu-result.txt` prose are in `build-test.md` already, since a verify-26.02
   developer reads that page and not this file. The edits are listed below.
6. Clippy: the five warnings in touched files are the five round 3 proved pre-existing
   (`corpus.rs:500`, `fingerprint.rs:135`, `benchmark.rs:108`, `test_golden_format.rs:708`, and
   `duckdb_oracle/tests.rs:124` → now `:126`, moved by two import lines, same
   `assert!(DUCKDB_FLOAT_TOLERANCE < 1e-10)`). No warning falls in any line this round wrote.

### `build-test.md`, edited here

Grand total 2565 → **2575**, Rust 2073 → **2083**. The cpu header 1384 → **1394**, `--lib` 650 →
**659**, `test_cpu_corpus` 705 → **706**. The corpus-cpu row 704 → **705**; the DuckDB tier 148 →
**149** and "Two more cases" → **three**, naming the stamp guard and the no-file dataset. Module
units: Golden merge 3 → **9**, DuckDB oracle 24 → **26**, Device answer comparison 8 → **9**,
each with its prose. The `gpu-result.txt` artifact row, the artifact diagram and the two
`PCK_WRITE_GPU_RESULT` paragraphs now carry the `cudf=` line, the refusal and the read knob.

### For the next person

- **Still only the device cycle.** `PCK_WRITE_GPU_RESULT=1` through
  `build-test-shadgpu.sh --all`, then `--pull-results`, then the 27 red cases go green or each
  failing section is a ticket. The cycle will now also write a `cudf=25.02` first line, and the
  28th case (the stamp guard) stops being vacuous the moment the file exists.
- **For a `verify-26.02` developer**, four things. `PCK_WRITE_GPU_RESULT=26.02` is now *enforced*
  rather than advised: `=1` on a 26.02 build panics at the first recorded cell and writes
  nothing. `gpu-result-26.02.txt` must carry `cudf=26.02` or every reader of it fails — it will,
  since the writer stamps what it is linked against. The `duckdb_gpu_` filter that task's impl
  plan already uses keeps the coverage guard out of the run, and the stamp guard too (it reads
  the committed file). And when `peacock_cudf_version()` lands, point
  `device_answer::cudf_version()` at it and delete the `build.rs` copy: that is the stronger
  source, and the doc at both sites says so.
- **For a `pbench` developer:** the coverage guard and the stamp guard both derive their datasets
  from `cost-registry.csv`, so a new dataset needs no edit in `test_cpu_corpus.rs`, and landing
  with every `gpu_*` cell off is fine — no `gpu-result.txt` is expected or wanted. What you do
  own is `duckdb_result.py`'s two dataset lists (`:230` and `:238`) and the empty-glob no-op
  behind them, per the note above.
- **If the committed file ever moves to another cuDF**, `COMMITTED_CUDF_VERSION` in
  `test_support/mod.rs` is the one line, and `testdata/.gitignore`'s comment is the one piece of
  prose beside it.

## Completeness pass — the reviewer's reading (2026-10-08)

A fresh reviewer over the branch at `6c92f6b7`, asking what is wrong, with no sight of the
analyst's list. **0 blocking, 4 important.** It verified rather than read: it reimplemented
`fingerprint.rs`'s algorithm independently in Python — declared-type classing, trimmed cells,
`\u{1}` per exact cell, byte-sorted rows joined with `\n`, `{:.17e}` with a plain exponent, the sum
in value order folded from `0.0` — ran it over DuckDB 1.5.4 and the committed sf1 parquet, and
reproduced **all six committed fingerprints byte for byte**, tpch q11 and tpcds q98 included. So
the cross-writer agreement is not a copied digest; it is what the algorithm produces from the data.
It also re-derived all 120 lines and re-ran each line's comparison: every `duckdb_approx` line
genuinely needs its tolerance (exact fails on all 15), every `duckdb_divergent` column really
differs — measured in last-place units, q58 at 92-97, q61 at 45, q66's twelve at 1.31-1.79 — and
19,892 golden data lines carry no `|` inside a cell. It found no coverage regression: no `#[test]`
is removed anywhere in the diff, and the one renamed case has strictly stronger assertions.

### Important

1. **`tickets.md` carried two numbers the branch falsified**, and one caused a collision: line 14
   said the next free number is 252 when #252 was taken, so the next agent to file would have
   reused it against the same paragraph's "Numbers are never reused"; line 20 said 113 open
   against its own table's 114. Both were right on master. Corrected, and recomputed after #253
   and #254: next free **255**, open **116**, which matches the anchors in every file.
2. **#252's enumeration was wrong in both directions**, so its remedy was incomplete. Two more
   cases are in the class: `all_modes_expands_to_the_five_in_either_position` reads
   `corpus_cases.inc` through `corpus_lines` the same way, and
   `every_timed_case_is_enabled_on_a_device` in `test_corpus_goldens/benchmark.rs` already did
   before this task — `rust_only_targets` stages that binary, because its second axis greps only
   the top-level `tests/*.rs` for `repo_root` and cannot see a read inside a submodule. Six cases,
   not four, and the fix has to carry `peacockdb-core/tests/common/` as well as
   `llm-wiki/tickets/`. Ticket corrected.
3. **`gpu_case` records the device's answer after an assertion against the cpu that panics first**
   — `corpus_golden::assert_section(&cpu_golden(…), query, &render_run(…))` at `corpus_gpu.rs:52`,
   twelve lines ahead of `record_gpu_result`. The spec's step 4 wants the section written before
   the device asserts against the cpu precisely so a divergence is recorded, and names #243's lane
   split as the case; `render_run` carries per-node lanes and batch lists, so that divergence
   aborts the case before anything is written and DuckDB never sees the answer. Latent — #243
   names no corpus query today — which is why three review rounds missed it. Handed to the
   developer.
4. **`build-test.md:992` named `over_cap`, which this branch deleted** (deviation 1 of round 1
   retired it for `section_holds_rows`). Corrected to name `duckdb_case` and `duckdb_gpu_case`,
   the two the branch actually added to that facade.

### What it checked and found clean

`build-test.md`'s arithmetic, all 79 rows summing to the grand total and each block header to its
own rows; the three readings of `PCK_WRITE_GPU_RESULT` agreeing across `device_answer.rs`, the gate
script and the page; `device_result_files`' shell, including that `declare -f`'s output is not
re-expanded inside the unquoted heredoc, that an unmatched glob is caught, and that the freshness
premise holds because the goldens push carries no `-t` so a pushed copy's mtime precedes the launch
marker — with the one hole, a `--push-binaries` between the run and the pull, named in the comment
rather than pretended away; `architecture.md`, whose only falsified sentence was the one already
corrected; the gitignore's handling of `gpu-result-26.02.txt` against the tracked committed file;
and that the regeneration was surgical, exactly four sections in one `mini.result.txt` and ten
`hash:` lines across three files.

### Nits it recorded and the pass drops

Three comment-cap overruns (`fingerprint.rs:187` at 12, `test_cpu_corpus.rs:216` at 11,
`test_golden_format.rs:554` at 5 in a body). One seam worth keeping in mind rather than fixing:
`duckdb_result.py`'s `EXACT_TYPES` lists `TIME`, `INTERVAL`, `BLOB` and `UUID` as hashable while
`cell()` falls through to `str(value)` for all four, which arrow-rs renders differently — the same
class step 5 fixed for timestamps. No corpus query returns any of them, the failure mode is a hash
mismatch rather than a false pass, and the exhaustive `is_approximate` raise is what keeps it from
going silent. Likewise a one-byte asymmetry in the cap threshold, Rust measuring the rendering
without its trailing newline and Python with it, whose only consequence is a red line.

### One question it could not settle, and the answer

It could not tell whether the page's `--lib` figure counts `#[ignore]`d cases, since the End-to-end
row's prose counts 29 "two of which are ignored". It does: round 5's measured `--lib` is 657 passed
plus 2 ignored and the page now says 659, so rows include ignored cases. The page is consistent.

### A process note, and it is mine

The reviewer found the working tree dirty under it mid-pass — #253 and #254 appeared while it was
reading — and re-read every wiki fact from `git show 6c92f6b7:` rather than from the tree, which is
the right instinct and should not have been necessary. A completeness reviewer reads a commit;
commit the wiki work before dispatching it, or hold it until the pass returns.

### Item 4, from the completeness reviewer — the record moved ahead of BOTH cpu comparisons

The finding is right and it was a real spec violation. `gpu_case` recorded after
`corpus_golden::assert_section(&cpu_golden(...))`, which is a comparison against the cpu, panics,
and is not covered by regeneration on the device side — so the divergence class step 4 names was
exactly the one that got nothing recorded. `render_run` carries per-node rows, bytes, lanes and
batch lists, so #243's lane split (NaN and -NaN in different lanes, an aggregate's group count
moving) and #220's batching split abort the case twelve lines before the writer. Latent today:
#243 names no tpch or tpcds corpus query.

**The fix.** `batches` is cloned and `record_gpu_result` called immediately after the two
accounting asserts, ahead of both comparisons. Three checks the coordinator asked for:

- **`batches` is still valid to clone there, and more so.** `report.batches` are the report's own
  `GpuBatch`es, alive to the end of the function with `session` still open, and
  `record_batch().clone()` is an Arrow clone of ref-counted arrays.
- **Nothing between the old and the new position could change what is written.** The removed
  region is `render_run(tree.as_ref(), &report)` — an immutable borrow of a pure renderer — and
  `assert_section`, which reads a file and panics. The recorded bytes are identical at either
  position; the only difference is whether a panic reaches the writer.
- **The accounting asserts stay first**, agreeing with the coordinator's expectation.
  `in_flight_bytes` and `holds`/`releases` compare the run against ITSELF, not against the cpu, so
  the spec's "before the device asserts against the cpu" does not reach them; and a report that
  held batches it never released is a broken run rather than an answer DuckDB can settle. The
  `gpu_case` doc now says that in one clause.

The comment no longer claims a generality it does not have: it names both comparisons it is
ahead of, says why they are the ones that matter, and points at the test.

**The test, and what it cannot do.** `tests/test_module_layout/write_order.rs` reads
`corpus_gpu.rs`, extracts `gpu_case`'s body with comments dropped (`tree::code_only`, which
exists for exactly this kind of claim) and asserts that `record_gpu_result(` precedes
`corpus_golden::assert_section(` and `assert_result(`, with each of the three required to appear
exactly once so a rename fails loudly instead of passing vacuously.

- **Watched red on the real bug**, not on a re-break: the guard was written before the fix,
  against the code as the reviewer found it, and failed with "gpu_case calls
  `corpus_golden::assert_section(` before `record_gpu_result(`, so a device answer the cpu
  rejects is never recorded".
- **Why that binary.** The honest alternative was `test_cpu_corpus`, where the device tier's
  other no-card rules live — but it is staged to verda and reads the checkout only through
  `CARGO_MANIFEST_DIR`, so a case there would have become a fifth name on #252's verda red list
  that every later dispatch has to carry. `test_module_layout` is never staged (the classifier in
  `scripts/build-test.sh` greps for `repo_root`, which that target carries deliberately), reads
  source by design, and its module doc already describes this kind of claim — "the claims that
  hold today because someone wrote the tree that way and nothing would notice if the next change
  did not". One clause added to that doc says one rule there is a task spec's rather than the
  style guide's. Verified mechanically that `test_cpu_corpus.rs` still contains no `repo_root`, so
  the verda staging set is unchanged.
- **The new file is a submodule, not a target**: `cargo test --test write_order` errors and lists
  the eight real targets, which is why `test_ci_coverage` has nothing new to name and still passes
  9/9. It follows the `#[path = "test_module_layout/<x>.rs"]` convention documented at the top of
  that binary, in alphabetical position beside the other six. The layout rules themselves read
  `src/` through `src_root()` and have no opinion about a file under `tests/`.
- **What this guard does NOT cover, and what the first real cycle must check.** It reads the order
  of three calls in one function body; it cannot see a panic reached through something called
  earlier, and nothing without a card can run `gpu_case`. So on the first recording cycle: if any
  device cell fails its `.cpu.txt` comparison, that cell must still have its `== <query>
  mode=<mode>` section in the file `--pull-results` brings home. A failing cell with no section is
  this ordering broken again, whatever the guard says.

**Measured after the reorder**, the whole bar again: `test_module_layout` **18 passed** (17 → 18),
`test_cpu_corpus` **706: 679 passed, 27 failed** — the same 26 `duckdb_gpu_*` plus the coverage
guard, 27 of 27 carrying "does not exist, so no device answer is recorded" — `--lib` **657
passed, 0 failed, 2 ignored**, `test_golden_format` **43**, `test_corpus_goldens` **26**,
`test_cost_model` **3**, `test_ci_coverage` **9**, `test_duckdb_result.py` **14, OK**. Device
targets under `--features gpu` against `rapids-cuda-12.2`: `check --tests` rc=0 with the one
pre-existing `AsArray` warning, and `test_gpu_corpus-5013741728a486dd` links — which matters more
this time, since `corpus_gpu.rs` compiles in no rust-only build. Clippy rc=0 with the same 26
warnings as before the round and none on a line it wrote. No golden moved.
`build-test.md`: grand total 2575 → **2576**, Rust 2083 → **2084**, the module-layout row 17 →
**18** with the new rule and its case named.

## Chain resumed on nebius-gpu (2026-10-08)

The control file said `rebase`, and master carried the human's host override into the chain.
`ENS-duckdb-oracle` is rebased onto `dbf44bcc` (one master commit: docker build retired, chain J
moved to nebius-gpu, #260 filed). Three conflicts, all mine:

- `llm-wiki/tickets.md` twice, the next-free-number line — master's 262 wins over the branch's
  252 and 255, since numbers are never reused. The contents table merged by union:
  system-hardening gains master's #260, testinfra keeps the branch's #252. The total was stale on
  both sides and is now 117.
- `scripts/build-test-shadgpu.sh`, the `--run-benchmarks` staging check — master's side, which
  retires the `scripts/docker-build.sh` hint and deletes the `/.dockerenv` guard the branch had
  added `PULL_RESULTS` to. Nothing was carried forward: the guard's subject no longer exists.
  `PULL_RESULTS` keeps its four other uses and `bash -n` passes. A code conflict is ordinarily a
  developer's, and this one is resolved here because the resolution is master's hunk verbatim with
  no new content; the developer proves it anyway, since the next build runs through this script.

The board reset the override asks for, in the same commit. `pbench` and `repartition-keys` are
written `rebase needed(building)` rather than plain `building`: their branches exist and still sit
on the old base, and that mark is the only thing that says so if this run dies before it reaches
them. The override's end state for both is `building`, which is what they return to once rebased
and re-proven.

`stale-cells` is unblocked to `approved to build`, by the override.

nebius-gpu answered before this dispatch: `computeinstance-e00tnrse7ayntnzcyt`, NVIDIA L40S,
46068 MiB, 0 MiB in use. Disk 86G of 96G used, 11G free — the override's cleanup paragraph is
live, not hypothetical.

## The device recording cycle, run on nebius-gpu (2026-10-08)

The one thing the task never did. `gpu-result.txt` now exists for both datasets and the 27 red
cases are green. Everything here ran on nebius-gpu's L40S under the host override; every CPU
number below was measured locally.

### What the cycle recorded

One cycle, the whole device corpus binary, nothing filtered:

```
rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ dmitry@89.169.109.150:peacockdb-J/
# on the host, in ~/peacockdb-J, after . ~/peacock-env.sh
./scripts/build-test-shadgpu.sh --build
PCK_WRITE_GPU_RESULT=1 \
  LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib \
  PEACOCK_TESTDATA_DIR=$PWD/testdata \
  ./cpp/install/rust-tests/test_gpu_corpus --test-threads=1
```

**28 passed, 0 failed, in 10.16s.** Two files written, pulled home by rsync rather than
`--pull-results` (see *Deviations*):

- `testdata/goldens/tpch.sf1/gpu-result.txt` — 28868 bytes, 219 lines, **22 sections**
- `testdata/goldens/tpcds.sf1/gpu-result.txt` — 3486 bytes, 41 lines, **4 sections**

26 sections for the 26 enabled device cells, no more and no fewer. Both open `cudf=25.02`, so the
provenance line round-tripped on its first real cycle and `COMMITTED_CUDF_VERSION` reads what the
binaries were linked against. Query order follows the registry, mode order follows the mode list.

**The fingerprint path is exercised, and it is the strongest single result here.**
`tpch/filter-project` is the one recorded cell on a `duckdb_fingerprint` line; its section is
`rows=2402187`, two `nonnull` counts and a SHA-256, and `duckdb_gpu_tpch_filter_project_tp1_single`
passed — the device's digest over 2.4M rows equals the one `duckdb_result.py` computed
independently. The other 25 cells sit on `duckdb_exact` (13) and `duckdb_approx` (12 — the
`q1` and `shuffle-additive-avg` families at five modes each, `shuffle-stddev` and `tpcds/q85`).
**No cell sits on `duckdb_none` or `duckdb_divergent`**, so not one of the 26 passed by declining
to compare.

### The 27 red cases, before and after

Measured on the same binary, same flags, before the pull and after it.

| | before | after |
|---|---|---|
| `test_cpu_corpus` | 679 passed, **27 failed** | **706 passed, 0 failed** |

All 27 failures carried the one message, counted: `grep -c "does not exist, so no device answer is
recorded"` → 27. **No divergence. No ticket is owed by this run** — not a wrong answer, not a
fingerprint mismatch, nothing to file in `corpus-coverage.md` or anywhere else.

### The two guards that were vacuous, shown red against the real file

Both had never seen a file. Each was doctored in place and restored (`cmp` against the backup
after, both `restored-ok`):

- **the cuDF stamp** — first line rewritten to `cudf=26.02`:
  `every_committed_gpu_result_file_carries_the_committed_cudfs_stamp` FAILED, and
  `duckdb_gpu_tpch_q6_tp1_single` FAILED with it at `duckdb_oracle.rs:349`. So the stamp guard is
  live, not vacuous, and every reader of the file checks it too.
- **the coverage guard** — the `== q6 mode=tp1-single` section deleted:
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` FAILED with
  `missing [("q6", "tp1-single")], not an enabled cell []`.

### Item 4's check, which no natural failure could give

The completeness reviewer's note said the ordering guard in `test_module_layout` reads three call
sites and cannot see a panic reached through something called earlier, so the first real cycle had
to check: a device cell that fails its `.cpu.txt` comparison must still have its section.

Every cell passed, so there was no natural instance. **One was constructed on the host**, in the
versioned sandbox so the committed file was never a candidate:

1. `tp1-single-mini.cpu.txt` line 224, `in_rows=[[114160]]` → `[[114161]]`, in the `q6` section;
2. `PCK_WRITE_GPU_RESULT=25.02 ... --exact gpu_tpch_q6_tp1_single`;
3. the case FAILED at `corpus_golden.rs:201` with ``  `q6` moved — line 9, column 23 ``;
4. `gpu-result-25.02.txt` existed anyway, 128 bytes, `cudf=25.02` and **one** `== q6
   mode=tp1-single` section.

The golden was restored (`cmp` clean) and the sandbox file deleted. The ordering holds in the
running binary and not only in the source the guard reads.

### The rest of the device set, same host, same build

Run after the recording cycle, sequentially, `--test-threads=1` on every Rust binary. The second
`test_gpu_corpus` ran **without** the recording variable and left both files untouched — same mtime, same
size, and `sha256sum` equal to the local copies — which is the ordinary gate shape.

| binary | result |
|---|--:|
| `cpp/install/bin/peacock_gpu_tests` | 4 passed |
| `cpp/install/bin/peacock_plan_tests` | 56 passed |
| `cpp/install/rust-tests/test_gpu_corpus` | 28 passed |
| `cpp/install/rust-tests/test_node_timing` | 1 passed |
| `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::` | 536 passed |
| `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered |

0 failed everywhere. That is `build-test.md`'s gpu block exactly: 536 + 28 + (8 + 3) + 1 = 576.

The build itself was clean: `./scripts/build-test-shadgpu.sh --build`, **0 warnings**, no error
line. That also proves the rebase's resolution of `scripts/build-test-shadgpu.sh` — master's hunk,
with the `/.dockerenv` guard deleted — since the whole build ran through it. Nothing misbehaved.

### The local bar, re-measured after the pull

Every number measured, `--features rust-only -p peacockdb-core`, `--test-threads=2`:

| target | result |
|---|--:|
| `test_module_layout` | 18 passed |
| `--lib` | 657 passed, 2 ignored |
| `test_cpu_corpus` | **706 passed, 0 failed** |
| `test_golden_format` | 43 passed |
| `test_corpus_goldens` | 26 passed |
| `test_cost_model` | 3 passed |
| `test_ci_coverage` | 9 passed |
| `python3 testdata/test_duckdb_result.py` | Ran 14 tests, OK |

Every count is the one `build-test.md` states, so **no wiki count moved and `build-test.md` needed
no edit**. The diff is two new files and nothing else: `git status --short` shows exactly
`testdata/goldens/tpch.sf1/gpu-result.txt` and `testdata/goldens/tpcds.sf1/gpu-result.txt`, both
untracked, neither matched by `git check-ignore`.

### Deferred, by the override

Each skipped on purpose, none blocking:

- **the sf40 pair**, `peacock_tpch_tests` and `peacock_tpchv_tests`. Out by the override, and
  unrunnable here twice over: `testdata/tpch.sf40` does not exist on nebius-gpu — sf40 lives only
  on shad-gpu — and `peacock_tpch_tests` reserves 69 GiB against the L40S's 46068 MiB. The staged
  binaries on the host are from an older build and stale.
- **`--run-benchmarks`** and the three `bench_` cases inside `peacock_gpu_benchmarks`.
- **Nsight captures** (`create_nsys_profile.sh`) and any H200 timing.

### Deviations from the dispatch, and why

- **`--pull-results` was not used**, by the override: it ssh-es to shad-gpu and gates on
  `$REMOTE_STATE/gate.id` there. Two plain `rsync` calls, one file each, brought the files home.
  `build-test.md`'s `--pull-results` paragraph is still the standing recipe for shad-gpu and was
  left alone; the override in `tasks.md` is what replaces it while shad-gpu is down.
- **`--run` was not used** either, same reason; each staged binary ran directly over ssh.
- **The C++ CPU tier (`ctest -L cpu`, 15 cases) was not re-run locally.** The change is two
  recorded data files and touches no code, and `peacock_cpu_tests` reads none of them.
- **The spec's completeness signoff is now false in one sentence** — "the device half never ran
  ... `gpu-result.txt` does not exist and its 26 cases plus the coverage guard are honestly red".
  Left unedited: the spec is frozen and its one later write is the signoff itself, so amending it
  is not a developer's write. The coordinator owns that line and #235's close.

### Host cleanup and a disk change that was not mine

- **What this run removed:** `~/miniforge3/bin/conda clean -a -y` only — 371 tarballs (4.55 GB), 1
  index cache, 41 packages. Free space 11G → 15G. Nothing else was deleted by this run; neither
  cuDF env nor `~/peacockdb-J/testdata` was touched.
- **Something else freed ~28 GB at 17:48**, between the build and the test sweep: `~/peacockdb`
  went 29G → 991M, losing `cpp/build`, `cpp/build26`, `cpp/install`, `target-cudf-rapids` and
  `target-cudf-rapids-cuda-12.2`. `who` showed an interactive login on pts/0 from 24.4.100.58 at
  17:48; this run never wrote outside `~/peacockdb-J` and `~/miniforge3`. Recorded because a
  `verify-26.02` developer will find the #260 debug build gone and should not read that as rot.
- **Disk at the end: 59G of 96G used, 37G free.** `~/peacockdb-J` is 22G, of which
  `target-cudf-rapids-cuda-12.2` is 17G and `cpp/` 4.8G.

### For the next person

- **Nothing on this task is outstanding.** The two recorded files are uncommitted in the working
  tree and are the whole of the change; they are new files, so a `git add -u` misses them.
- **The device answers agree with DuckDB on all 26 cells**, so this branch files no new ticket and
  the `duckdb_oracle` census in `build-test.md` (93 exact / 15 approx / 4 fingerprint / 4 none / 4
  divergent) is unchanged.
- **Re-recording is cheap**: ~10 s of device time once the build is warm. Any later task that
  turns a device cell on or off must re-run the cycle — the writer prunes a cell the registry no
  longer enables, so the coverage guard goes red until it does.
- **Float cells move between runs.** Nothing compares this file with its previous version, so a
  future `git diff` over it that moves only float digits is expected; a moved *section* is not.
