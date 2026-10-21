# Oracles that say what they compare, tests that never read Rust source, and a ticket triage

Kind: production

**This task closes [#252](../tickets/testinfra.md#t252)** (checkout reads remotely) **and
[#287](../tickets/corpus-coverage.md#t287)** (row order unchecked). Twelfth of chain K, after
round-tolerance; interval-days follows it and uses its grammar. It archives #251, #253, #254, #263
and #266 as well, which it makes moot rather than fixes.

## Why

- **Oracle names hide the comparison.** Every row oracle sorts both answers as text before
  comparing them: `duckdb_exact`, `data_fusion_exact`, `golden_exact` and their approximate twins.
  So "exact" does not mean exact, and no oracle checks the row order of an `ORDER BY` query.
- **#251's lines record a tolerance as a divergence.** q58, q61 and q66 differ from DuckDB only in
  the digits a decimal division truncates ([#288](../tickets/scalars.md#t288)): at most 1.0e-6
  relative (q58), measured 2026-10-20. Calling that a divergence keeps them under a ticket.
- **Tests read the checkout.** `ticket_is_open`, `corpus_lines` and `macro_invocations` read
  files at run time, so a staged test binary fails on a remote host (#252). A tickets-only
  commit can also turn tests red while CI skips them (#263).
- **Constants.** `chunk_bytes`'s 1 GiB is spelled three times by hand: `1 << 30` in
  `wire/mod.rs`, `DEFAULT_CHUNK_BYTES` in `gpu_backend/join.rs`, and `1ull << 30` in
  `join_residual.cpp`.

## The work

1. **One oracle grammar.** Every `corpus_query!` line takes:

       duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>", …)])
       data_fusion(sort, exact|approx)
       golden(sort|no_sort, exact|approx_std)

   - **The first argument says whether both answers are sorted before comparing.**
     `data_fusion` accepts `sort` only; it is written anyway, so every line reads the same.
     `golden(no_sort, …)` compares the device's live answer row for row against the cpu's
     committed section, which a `no_sort` line writes in engine order.
   - **`exact` and `approx` keep today's meaning.** `duckdb`'s `approx` is one unit in our last
     place for a decimal and 1e-11 relative for a float. `data_fusion`'s is 1e-12 relative.
     `golden`'s `approx_std` is `STDDEV_TOLERANCE`, 1e-11 relative on every numeric cell.
   - **`rel(<tol>, "<column>", …)`** is new, for a known numeric divergence. It takes no ticket.
     The named columns must agree within `tol`, relative to the larger magnitude, whether our
     cell is a float or a decimal. Every other column compares as `approx`.
   - **`date("<column>", …)`** is new, for a difference of type convention. DuckDB answers
     `date ± interval` as a timestamp where DataFusion answers a date. A named column's DuckDB
     cell compares by its date part.
   - **Columns are named as our result header prints them**, as string literals. Positions are
     refused. DuckDB's timestamp cells read `2024-02-29T00:00:00` (`duckdb_result.py`'s
     format), and `date` compares the part before the `T`.
   - **These keep their spellings:** `duckdb_fingerprint`, `duckdb_none`, `data_fusion_subset`,
     `data_fusion_disabled` and `live_cpu`. They are not settings of the two axes.
     `schema_validation_disabled(<modes>)` is untouched.
   - **`duckdb_divergent` is deleted.** Chain K's empty-sorts removed its two #205 lines, and
     item 3 turns #251's three into `rel`, so nothing uses it.
2. **`no_sort` compares row for row.** A line whose query's plan ends in a sort (under any limit
   or projection) declares `duckdb(no_sort, …)`, and `golden(no_sort, …)` where its gpu oracle
   is a golden. A guard test enforces both. A fingerprinted line is exempt, and so is a
   `live_cpu` one.
   - **No tie rule.** A comment at the comparison says it is safe because no corpus answer
     compared row for row has tied rows. Measured on 2026-10-20 with DuckDB 1.5.4: 90 lines are
     fully ordered, and the one query with ties, tpch q11, is a fingerprint.
   - **Writers keep order.** The writers of `duckdb-result.txt`, `*mini.result.txt` and
     `gpu-result.txt` keep the engine's row order for those queries and keep sorting the rest.
     Fingerprints are unchanged.
   - **What reaches the order check.**
     - The cpu: at the mode that authors its result section, on every run: its rendering must
       equal the committed section text for text, and that section meets DuckDB's row for row.
       The other four cpu modes are compared only through `data_fusion`, so they stay
       order-blind.
     - The device: at every mode, on every run, through `golden(no_sort, …)`; and against DuckDB
       as of the last recording of `gpu-result.txt`.
     - What else stays order-blind: `live_cpu`.
   - **A real order defect is filed, not fixed.** A cell that comes back in the wrong order is
     turned off under its ticket (#202 and #204 are the known ones), and its line keeps
     `no_sort`.
3. **The corpus moves to the grammar**, by a script, `scripts/rewrite_oracles.py`, which is
   deleted again at the end of the task. q58, q61 and q66 end in `ORDER BY`, so they become
   `duckdb(no_sort, rel(1e-5, …))`, with #251's columns by name.
4. **The registry gains a `duckdb_oracle` column, and the report shows it.**
   - The column comes before `cpu_oracle` and is held to the lines like the other two.
   - All three columns hold the line's full spelling, CSV-quoted where it has commas or quotes.
     Every reader of the CSV parses quoted fields before any cell is written quoted:
     `registry.rs`, cost-report's `Registry::load`, `ticket_widget.py` and `duckdb_result.py`.
   - `testdata/report.html` gets a DuckDB oracle column, placed immediately before the existing
     cpu oracle column: DuckDB, cpu, gpu. Like those, it shows a shortened keyword
     (`sort exact`, `no_sort approx`, `rel 1e-5`) with the full spelling and what it compares on
     hover, and for `rel` and `date`, the columns.
   - `scripts/ticket_widget.py`'s `ORACLE_HOW` moves to the grammar for all three oracles. Its
     fallback for a registry without oracle columns parses the new grammar, and a registry
     without `duckdb_oracle` still renders.
5. **No peacockdb-core test reads the checkout beyond testdata.**
   - `ticket_is_open` and its test `an_archived_ticket_is_not_open_and_a_listed_one_is` go.
   - `corpus_lines()`, `macro_invocations()` and `all_modes_expands_to_the_five_in_either_position` go.
   - `every_oracle_variant_is_named_by_some_line` and
     `every_duckdb_oracle_is_named_by_some_line` read the registry.
   - `every_timed_case_is_enabled_on_a_device` and
     `every_mask_entry_is_one_of_its_lines_device_modes` (`test_corpus_goldens/benchmark.rs`)
     stop parsing the includes at run time. That binary compiles `corpus_benchmark_cases.inc`
     and `corpus_cases.inc` in, through its own `corpus_query_benchmark!` and `corpus_query!`,
     each line submitting its modes and its schema-validation mask as text. The registry
     cannot serve the mask, since it holds only `enabled|disabled`.
   - `every_refusal_names_a_ticket_that_exists` moves from `planner/tests/plan_goldens.rs` to
     cost-report, which is never staged: `--lib` is staged and `llm-wiki/` is not. cost-report's
     `the_index_reads_the_anchors_of_every_wiki_file` and
     `the_widget_renders_a_pbench_section_beside_the_benchmarks` stay where they are.
6. **Source-reading tests move to `cost-report/tests/test_source_formatting.rs`.**
   - **What moves:** `test_module_layout` and its submodules (except the privacy probe below),
     `test_ci_coverage` with `runners.rs`, and `no_declaration_carries_a_block_left_behind_by_a_split`
     from `test_golden_format`.
   - **A new guard:** `test_source_formatting` gains
     `no_core_test_reads_the_checkout`. It fails on any `CARGO_MANIFEST_DIR` read in
     `peacockdb-core/src` or `peacockdb-core/tests` outside `test_support/testdata.rs` and
     `build.rs`.
   - **CI and staging:** CI runs it by name, `cargo test -p cost-report --test
     test_source_formatting`, since `test_ci_coverage` counts a target as run only when its CI
     line names it. `INTENTIONALLY_NOT_IN_CI` follows. `build-test.sh`'s `repo_root` classifier
     goes, since cost-report is never staged.
   - **The privacy probe stays in core.** `a_private_module_is_unreachable_from_outside_the_crate`
     moves to its own binary, `test_crate_privacy`: it reads no source, but it compiles snippets
     against the `peacockdb_core` rlib, and cost-report keeps depending on nothing. It is kept
     off remote staging by name.
   - `scripts/exec_model/tests/test_join_capability.py` stays as it is.
7. **`coding-style.md` says it.** A test does not read the checkout at run time beyond
   testdata, and a line list it needs is compiled in. cost-report, never staged, is the
   exception: its tests may read the ticket files, and its `test_source_formatting` may read
   source. The places that name `test_module_layout` follow the move.
8. **`chunk_bytes`'s 1 GiB lives in `gpu_plan.fbs`.**
   - **The schema:** `enum JoinLimits : ulong { DefaultChunkBytes = 1073741824 }`, with the
     field default `chunk_bytes: uint64 = 1073741824`. A test holds the two equal.
   - **The code:** the planner's cap (`wire/mod.rs`), the Rust executor and `join_residual.cpp`
     use the generated constant. No hand-written copy remains.
   - **Zero is refused.** A mode with no budget leaves the field out, and the session refuses 0
     with a `std::runtime_error` naming `chunk_bytes`, as its other refusals do.
   - **Every caller that passes 0 today** passes the default instead: the device test harnesses
     and the gtests.
   - The comments in `gpu_plan.fbs` and `architecture.md` follow.
9. **Audit `null_equals_null`.**
   - **The claim:** `gpu_plan.fbs` and `architecture.md` say "Every type honours
     `null_equals_null` on both engines".
   - **The check:** all nine join types, on the cpu backend and the device session. The answer
     per type and engine goes in the detail file, with the test that shows it. A type with no
     test showing it gets one.
   - **A failure is filed, not fixed.** A type or engine that does not honour it gets a ticket,
     and the claim names the exception.
10. **Ticket triage**, in `llm-wiki/tickets/`, `archive/archived-tickets.md` and the
    `tickets.md` index:
    - #251 (decimal quotient scale): archived, replaced by
      [#288](../tickets/scalars.md#t288) (decimal division digits), filed 2026-10-20.
    - #253 (DuckDB divergence record) and #254 (untestable subset oracle): archived as stale.
    - #263 (tickets-only CI skip): archived as won't fix, both halves. The second half's
      workaround (close and reopen the PR) is kept in the archived text.
    - #266 (archived ticket accepted): archived as stale, since `ticket_is_open` is gone.
    - #252 (checkout reads remotely): archived as done.
    - #287 (row order unchecked): archived as done, its text rewritten to what was built (item 2).
      The ticket's proposed monotonic key check was not taken.
    - #255 (nested column panic) moves to `scalars.md`. #264 (computed group key) moves to
      `complete-coverage.md`. Every link to either follows.
    - #258 (PR comment headroom): shortened to the problem and the two options.
    - #256, already archived as stale: a line saying it is a duplicate of #282 (scan with no row
      groups).

    `testdata/report.html` is regenerated in the same commit as any registry or ticket move.

## Scope

| path | change |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc` | every line on the grammar |
| `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs` | the macros parse the grammar; the dropped and moved tests; the order guard |
| `peacockdb-core/tests/test_corpus_goldens.rs`, `test_corpus_goldens/benchmark.rs` | the timed and mask checks off the registry |
| `peacockdb-core/src/test_support/{mod.rs,corpus.rs,duckdb_oracle.rs,device_answer.rs,corpus_gpu.rs,result_text.rs,registry.rs}` and their `tests` | oracle types, `no_sort`, `rel`, `date`, ordered writers; `ticket_is_open`, `corpus_lines`, `macro_invocations` and `Divergent` gone |
| `peacockdb-core/src/planner/tests/plan_goldens.rs` | the refusal-ticket test moved out |
| `peacockdb-core/tests/test_gpu_corpus.rs` (the `golden_exact` literal in `a_device_run_under_a_regeneration_writes_no_golden`) | the new spelling |
| `peacockdb-core/src/test_support/order_hazard.rs` | `plan_ends_in_sort`, reusing its plan walk |
| `peacockdb-core/tests/test_module_layout*`, `test_ci_coverage*`, `test_golden_format.rs`; new `test_crate_privacy.rs` | moved out |
| `cost-report/tests/test_source_formatting.rs` (+ modules), `cost-report/src/main.rs` | the moved tests; quoted-CSV parsing; the refusal-ticket test |
| `testdata/duckdb_result.py`, `testdata/test_duckdb_result.py`, `testdata/goldens/*/{duckdb-result.txt,*mini.result.txt,gpu-result.txt}` | row order kept for `no_sort` queries; quoted CSV |
| `testdata/cost-registry.csv`, `testdata/report.html`, `scripts/ticket_widget.py` | the `duckdb_oracle` column |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/{mod.rs,fb_text.rs,tests.rs}`, `executor/gpu_backend/{join.rs,backend.rs,exec.rs}`, `gpu_tests/`, `cpp/src/operators/{join_residual.cpp,join_residual.h,join_session.cpp}`, `cpp/tests/gpu/test_join_session.cpp` | `JoinLimits`; 0 refused; every 0 caller |
| `cpu_backend/tests/join.rs`, `cpp/tests/gpu/test_join_session.cpp` | the `null_equals_null` tests the audit adds |
| `scripts/build-test.sh`, `.github/workflows/pipeline.yml` | the moved binaries; the widget's Python test |
| `llm-wiki/coding-style.md`, `build-test.md`, `architecture.md`, `prompts.md`, `tickets.md`, `tickets/`, `archive/archived-tickets.md` | the rule, the grammar, row order, `chunk_bytes`, the audit, the triage, the moved binaries' names |

Component-level API: the wire format changes. `JoinLimits` is new in `gpu_plan.fbs`;
`CudfJoin.chunk_bytes` defaults to 1 GiB and 0 is refused. No facade, trait or C ABI symbol
changes.

## Restriction

- No engine behaviour changes except refusing `chunk_bytes = 0`.
- A `null_equals_null` defect is filed, not fixed.
- Wall-time benchmarks and sf40 are out.

## Verification

- The cpu corpus, `--lib`, `test_corpus_goldens` and cost-report tiers green, on verda when it
  is up.
- `gpu-result.txt` regenerated on nebius-gpu for the `no_sort` queries. `gpu_tests::` and the
  join-session gtests green there, since every `chunk_bytes = 0` caller changed. The final
  device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Negative cases:
  - `no_sort` fails on two swapped rows.
  - `rel` fails just outside its tolerance on a named column, and on an unnamed one at
    `approx`.
  - An unknown column name in `rel` or `date` fails by name.
  - `date` fails on a different day.
  - A zero `chunk_bytes` is refused by name.
- The order guard's predicate has unit tests: Limit over Sort, Projection over Limit over Sort,
  an unsorted query, and a line that declares `sort` over a sorted plan fails.
- `no_core_test_reads_the_checkout` passes, and fails on a doctored read.

## Signoff (2026-10-21)

Solved under its constraints. Every corpus line is on the grammar; a query whose plan ends in a
sort compares row for row, held by a guard whose two halves each go red alone, and both writers
keep one row order per line. The registry and report carry `duckdb_oracle`. No core test reads
the checkout beyond testdata; the source-reading tests run in cost-report's
`test_source_formatting`, the privacy probe in `test_crate_privacy`, CI names both.
`JoinLimits.DefaultChunkBytes` is the one 1 GiB and 0 is refused. `null_equals_null` holds on all
nine join types on both engines; no ticket filed. Triage done. The order check found #202 on tpcds
q34 and q71, whose device cells are off under it, as item 2 directs. Completeness: reviewer and
analyst each 1 important (the `duckdb_none` writer order, fixed); two `architecture.md` sentences
the branch wrote corrected. Shortcuts or bandaids: none.
