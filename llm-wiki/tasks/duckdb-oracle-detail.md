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
