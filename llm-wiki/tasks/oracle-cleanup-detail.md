# oracle-cleanup — run record

## Run log

- 2026-10-21: branched `ENS-oracle-cleanup` off `ENS-round-tolerance`. Developer dispatched.
  verda: no address, CPU tests local. GPU: nebius-gpu, tree `~/peacockdb-K`.
- Carried in from round-tolerance: #56 was retitled "(q2 fingerprinted answer)" and now proposes
  a `compare_sections` fallback for the device's over-cap recording; q2 is off on the device
  under it. The grammar rewrite must keep q2's line as it is (cells off).

## Progress (developer)

Plan tasks done in the working tree (the coordinator commits; nothing is committed per task):

- **Task 1, triage (wiki only): done.** #253, #254 archived under Stale; #255 moved to
  `scalars.md`, #264 to `complete-coverage.md`, every link followed; #258 shortened. #256 already
  carried "Stale 2026-10-15. A duplicate of #282" from chain J, so nothing was added there. The
  index in `tickets.md` is recomputed from the anchors by a scratch script (`/tmp/oc/index.py`,
  not in the tree). `report.html` was not stale on arrival.
- **Task 2, no core test reads the checkout for input: done.** `ticket_is_open`, `open_ticket`,
  `compare_sections`'s `tickets_open`, `macro_invocations(_in)`, `invocations_in`,
  `split_arguments`, `corpus_lines`, `CORPUS_CASES` and `all_modes_expands_…` are gone. The two
  benchmark checks read `TimedDeclaration` / `LineDeclaration`, submitted by macros defined in
  `test_corpus_goldens/benchmark.rs`, which `include!`s both `.inc` files (the macros live there
  rather than in `test_corpus_goldens.rs`: only that module reads them). The refusal-ticket test
  is `cost-report`'s `every_refusal_names_a_ticket_that_exists`, resolving through `TicketIndex`
  (anchors) where the core copy grepped `### #N `. #252 Done, #263 Stale (won't fix), #266 Stale.
  Red evidence: a doctored `corpus_query_benchmark!(tpch, 40, mixed_join, tp1_single)` and a
  `schema_validation_disabled(tp4_single)` on `mixed_join` (device `none`) fail both checks by
  name; restored from copies.
- **Task 3, `test_source_formatting`: done.** Files moved with plain `mv` (no git ops; git sees
  the renames): `test_module_layout{.rs,/}` and `test_ci_coverage{.rs,/runners.rs}` are now
  `cost-report/tests/test_source_formatting.rs` and `test_source_formatting/{ci_coverage,runners,
  near_miss,privacy,test_code,tree,visibility,walls,write_order}.rs`; the split-block guard is
  `split_blocks.rs`; the new guard is `checkout_reads.rs`. The probe is
  `peacockdb-core/tests/test_crate_privacy.rs`. `Exemption::NotRun` went with its only entry
  (`test_ci_coverage`, "this test"): an unused variant would be a new warning. CI: the cost-report
  job runs `cargo test -p cost-report --bin cost-report` (narrowed from the bare `-p cost-report`,
  so `test_source_formatting` is not run twice), then `--test test_source_formatting` by name, then
  `peacockdb-core --test test_crate_privacy`. `build-test.sh`: the `repo_root|.github/workflows`
  classifier is gone, `test_crate_privacy` excluded by name; the staged rust-only set is now
  `test_corpus_goldens test_cost_model test_cpu_corpus test_golden_format`. Counts: 32 in
  `test_source_formatting` (11 CI wiring + 21 layout/split/checkout), 41 in `test_golden_format`,
  1 in `test_crate_privacy`. Red: `a_doctored_read_of_the_checkout_is_found` failed against the stub
  scanner; `no_core_test_reads_the_checkout` failed naming `tests/test_cost_model.rs:217` when a
  `CARGO_MANIFEST_DIR` line was appended there (restored from a copy).
- **Task 4, quoted CSV and `duckdb_oracle`: done.** `registry::split_csv_line` (core), a private
  `split_csv_line` in cost-report, Python's `csv` in `ticket_widget.py` (`duckdb_result.py` gets
  its reader in Task 6). The column sits before `cpu_oracle`; `DECLARATION_COLUMNS` has four;
  `CsvRow.duckdb_oracle`; the registry decodes each row's DuckDB spelling through
  `DuckdbOracle::parse`. Lines and rows compare through `test_support::tight` (whitespace out
  outside quotes) — `pub`, since the test binaries call it. The widget renders the grammar
  directly (`oracle_view`), and still renders an old registry with no `duckdb_oracle` (a dash) and
  old keywords (prefix stripped). The plan's "group-row width off by one" was not real: the old
  formula matched the old 24 cells; it now counts the third oracle (25, checked against a row).
  `scripts/test_ticket_widget.py` (3 cases) is a new step in the cost-report job. Red: the core
  and cost-report split tests failed against a `split(',')` stub; the Python tests failed
  before `declarations` returned four fields. Pre-existing, not mine: `sha_links` unused warning
  in cost-report's test build (present at HEAD).
- **Task 5, the grammar: done (docs in build-test/architecture deferred to after Task 6, where
  the counts settle).** `RowOrder`, `Values`, `DuckdbOracle::{Compare, Fingerprint, None}` with
  `parse`, `row_order`, `facets`, `FACETS`; `CpuOracle::{SortExact, SortApprox, Subset,
  Disabled}`; `GpuResultMode::{Golden { order, approx_std }, LiveCpu}` (`ALL` has five). Every
  decode compares `tight` forms. `Divergent` and its tests are gone. The macros stringify each
  oracle whole outside the mode repetition and hand it on as `$x:expr` (new `cpu_cell_cases!` in
  the cpu binary). Subtle:
  - `rel`/`date` resolve names in OUR header; a name the header has twice is refused, not read
    at its first position (`rel_fails_on_a_name_the_header_has_twice`).
  - `date` cuts DuckDB's cell at `T` before pairing, so a time cannot reorder rows.
  - `rel` keys its columns as numbers for pairing, like `approx`'s non-exact columns.
  - The plan's `rel_fails_just_outside` numbers (1.0 vs 1.0000200001) are INSIDE a 2e-5 bound
    relative to the larger magnitude; the test uses 1.0000200005 and pins 1.00002 inside.
  - `date` fixtures are built by arrow's cast + display (the format `duckdb_result.py` mirrors).
  `scripts/rewrite_oracles.py` (deleted at the end) rewrote 192 lines (189 live + 3 commented
  pbench float lines) and copies the registry's three oracle cells from the lines; a second run
  changes 0. #251 archived (Stale); #56's spellings follow. Red: compile errors naming
  `RowOrder`/`Values`/`Golden`/`SortExact` before the types existed. Green at this point:
  `--lib` oracle tests, `test_cpu_corpus -- --skip cpu_` 1299 passed (all `duckdb_` cases incl.
  q58/q61/q66 under `rel`), `test_corpus_goldens` 28. `every_oracle_variant_is_named_by_some_line`
  is red until Task 6 writes the first `golden(no_sort, …)`.
- **Task 6, row order: done on the cpu side; `gpu-result.txt` waits for the device round.**
  - The guard (`every_line_whose_query_ends_in_a_sort_says_no_sort`) first listed 165 findings:
    84 lines (81 duckdb + 84 golden; q58/q61/q66 already had `duckdb(no_sort, rel…)`). All 84
    have a `golden(…)` gpu oracle, none `live_cpu`. After `rewrite_oracles.py --no-sort` with
    that list: 84 `duckdb(no_sort, …)`, 84 `golden(no_sort, …)` (82 exact, 2 approx_std); the
    other gpu oracles are 80 `golden(sort, exact)`, 4 `golden(sort, approx_std)`. Every
    `GpuResultMode::ALL` combination is named, so `every_oracle_variant…` is green again.
    The list is `/tmp/oc/nosort.txt` while this run lasts; it is also every registry row whose
    `duckdb_oracle` starts `duckdb(no_sort,`.
  - `duckdb_result.py` regenerated (all three datasets): 8 tpch and 21 tpcds sections moved,
    pbench none; every moved section is a `no_sort` query and its sorted lines are unchanged
    (checked by a scratch script against HEAD).
  - cpu: `PCK_UPDATE_SECTIONS=1 … --test test_cpu_corpus -- <cpu_ filter per no_sort query>
    --test-threads=2`: 420 passed. Only `mini.result.txt` moved (tpch 8, tpcds 21 sections, the
    same queries), only in row order; no `.cpu.txt`/`.cost.txt` moved.
  - Before the device round, `duckdb_gpu_` fails on exactly those 29 queries x 5 modes (145):
    their recordings are still sorted text. Expected; the round re-records the 84 `no_sort`
    queries' cells.
  - `golden(sort, …)` sorts the section's rows first (`sorted_rendering`), since a section may be
    written in engine order. `golden(no_sort, …)` compares row for row (`same_sequence`, exact as
    text, `approx_std` per cell). The "no tie rule" comment sits at both `same_sequence`s.
- **Task 7, `JoinLimits`: done in code; the gtests and `gpu_tests::` run in the device round.**
  `enum JoinLimits : ulong { DefaultChunkBytes = 1073741824 }`, field default the same,
  `wire::DEFAULT_CHUNK_BYTES = fb::JoinLimits::DefaultChunkBytes.0`; unbudgeted modes write the
  default (left out of the buffer), `fb_text` prints only a non-default value, so no plan golden
  moved (`--lib` 866 passed, 2 ignored pre-existing #182). `PairScratch` lost its own constant
  and `0 =>` arm; `JoinSession` throws naming `chunk_bytes` on 0; `chunks` lost its `budget == 0`
  line. Every 0 caller (wire tests, `gpu_tests/{backend,exec,mod,join}.rs`, `tests/gpu_tests/
  {device,script}.rs`, the gtest `JoinSpec` default, `nested`, the three `Form` tables and two
  loops) passes the default. Remaining `1 << 30`s, none the join's chunk:
  `gpu_tests/accumulate.rs` `NO_COMPACTION`, `gpu_tests/contract.rs:335`, `executor/mod.rs`'s
  `GIB` display constant, and the new assertion in `wire/tests.rs`. Red: 14 compile errors on
  `DEFAULT_CHUNK_BYTES` before it existed.
- **Device recording (nebius-gpu, `~/peacockdb-K`, cuDF 25.02):** `PCK_WRITE_GPU_RESULT=1
  test_gpu_corpus --test-threads=1 <gpu_ filter per no_sort query>`: 405 passed, 10 failed.
  The 10 are **tpcds q34 and q71 at all five modes: a real order defect, #202**, filed not fixed.
  q34's `c_preferred_cust_flag DESC NULLS FIRST` and q71's `ext_price DESC NULLS FIRST` carry
  NULLs; the cpu answers them first, the device last ("row 0, column 3: the golden ``, the
  device `Y`"; "row 0, column 0: the golden `5003001`, the device `6008005`" — the golden's row 0
  has a NULL `ext_price`). Both lines keep `no_sort`; their device cells are off (`none`,
  registry `disabled`, tickets `202`), their 10 `gpu-result.txt` sections removed, and #202 now
  names them (its "no corpus descending key carries a null" sentence was no longer true).
  Recording brought home with the `rsync` of `value-case-impl.md`. Moved sections: tpch 40
  (8 queries x 5), tpcds 105 (21 x 5, incl. the 10 removed), all `no_sort` and order-only, except
  q39 at the three tp4 modes, whose float cells moved in the last digit (GPU float noise; the
  line is `duckdb(no_sort, approx)` / `golden(no_sort, approx_std)`). q2's line also became
  `no_sort` (its plan ends in a sort); its device cells stay off under #56 as before.
- **Task 8, the `null_equals_null` audit: every cell honoured; the claim stays as written.**
  One NULL build key and one NULL probe key per type. cpu: the backend hands the flag to
  DataFusion's `HashJoinExec` for every keyed type (`cpu_backend/join.rs`). device: `JoinSession`
  maps it to `cudf::null_equality` for every type (`join_session.cpp`).

  | type | cpu `true` | cpu `false` | device `true` | device `false` |
  |---|---|---|---|---|
  | Inner | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | Left | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | Right | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | Full | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | LeftSemi | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | LeftAnti | honoured (A, B) | honoured (A, B) | honoured (C) | honoured (D) |
  | LeftMark | honoured (A) | honoured (A): mark `false` | honoured (C) | honoured (D): mark `false` |
  | RightSemi | honoured (A) | honoured (A) | honoured (C) | honoured (D) |
  | RightAnti | honoured (A) | honoured (A) | honoured (C) | honoured (D) |

  - (A) `cpu_null_keys_follow_null_equals_null_for_every_type`
    (`cpu_backend/tests/join/nulls.rs`), new: before it only LeftAnti had a cpu test. Shown red by
    hard-wiring `false` into `CpuJoin::hash` ("Inner under true"), restored.
  - (B) `a_null_key_matches_a_null_key_in_the_finish_pass_when_the_node_says_so` (existing).
  - (C) `JoinSession.NullEqualsNullMatchesNullKeysForEveryType` (existing; `true` only).
  - (D) `JoinSession.NullKeysMatchNothingForEveryTypeWhenNullsAreNotEqual`, new; expectations
    are the cpu's answers, so the two engines agree cell for cell. Green on the L40S.
  No ticket filed; `gpu_plan.fbs`'s field comment and `architecture.md`'s three sentences stand.
  New tickets: none (the next free number stays 290).
- **Device round 1 (final code): the L40S list, `gpu-host-test.sh run`, ALL rc=0.**
  `peacock_cpu_tests` 26, `peacock_gpu_tests` 2, `peacock_join_session_tests` 56 (the three new
  gtests among them), `peacock_plan_tests` 118, `peacockdb_core_gpu_lib -- gpu_tests::` 739,
  `test_gpu_corpus` 909, `peacock_gpu_benchmarks` 8 (`--skip bench_`), `test_node_timing` 1. No
  build warnings.
- **Red for `JoinSession.AZeroChunkBudgetIsRefusedByName`:** the refusal reversed on the VM only
  (`patch -R`), rebuilt, the test failed; restored by a sync and a rebuild, 56/56 again.
  **Gotcha:** `gpu-host-sync.sh` keeps the local file's mtime, which is older than the object the
  red build made, so the first "green" rebuild compiled nothing and the test still failed;
  `touch` the restored file before rebuilding. The L40S list was then run again on that build.
- **Closing.** CPU, final tree: `--lib` 867 passed / 2 ignored (#182), `test_cpu_corpus` 2228
  (`--test-threads=2`), `test_corpus_goldens` 28, `test_cost_model` 3, `test_golden_format` 41,
  `test_crate_privacy` 1, cost-report 43 + `test_source_formatting` 32, Python
  `test_duckdb_result.py` 22, `test_duckdb_cost.py`, `scripts/test_ticket_widget.py` 3. Device
  nebius-gpu stopped (soft, rc 0). The old-spellings `rg` leaves only deliberate hits: the
  negative parser tests (`duckdb_exact`, `duckdb_divergent(251, 2)`, `golden_exact` refused), the
  widget's old-registry case, and `coding-style.md`'s antipattern case naming `ticket_is_open`.
  `scripts/rewrite_oracles.py` is still in the tree: it is to go in a commit of its own after the
  one that adds it, which only the coordinator can make.
