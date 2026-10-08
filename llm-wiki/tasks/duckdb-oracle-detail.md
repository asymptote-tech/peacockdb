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
