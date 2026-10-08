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
