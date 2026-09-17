# utf8-everywhere — run record

Chain B, task 1. Branch `ENS-utf8-everywhere` off master at `76f17db2`; PR targets `master`.

## Dispatch 1 — 2026-09-16

- Hosts: **verda down** (`Could not resolve hostname`), so the rust-only proofs run locally;
  the sf1 symlinks in this worktree resolve. **shad-gpu up**, 0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from chain E's runs; `cpp/build26` absent, so
  `--build` re-runs cmake there (minutes, not the cold hour).
- Pre-dispatch grep: `grep -rn "utf8view\|Utf8View\|declaring_view" peacockdb-core/src/tests`
  finds only `harness_cases.rs` (`declaring_view_strings`, the #183 pin) — chain E's Task 8
  retirements are in master, no survivor outside the spec's list. The nine files
  `grep -rln "Utf8View\|BinaryView" peacockdb-core/src` names are exactly the spec's scope
  table; `peacockdb-core/tests/common/corpus_cases.inc:30,48` mention the type in comments
  that the rollout step rewrites.
- Routing: the developer works `utf8-everywhere-impl.md` task by task — the option and the
  rule (red then green, local rust-only), the deletions, the goldens regen, then one shad-gpu
  cycle over `_cases` and one over `gpu_` for the rollout (foreground `--build`,
  `--push-binaries --patch`, `--run`, each under a timeout), then the record.
- The spec's restriction is the one to hold: no cast anywhere; a view type that survives the
  option is a ticket and the rule's refusal, never a conversion. Every golden diff line is
  `Utf8View → Utf8` and nothing else.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Reviewing — 2026-09-16

Committed as `2902c5f7`, `dee2462c`, `3305fd0b`, `1bff7269` (the plan's four commits) on
`e8f396d3`; pushed; PR #158 against master, base verified, 6 commits. The coordinator's own
golden check agreed with the developer's: the residual beyond the word swap is the 18
`sha256=` pairs in `recipe-payloads.txt` and q24's residual filter in the five tpcds files.
Review round 1: 0 blocking, 2 important, 5 nits. Important: (1) `cpu_backend/source.rs`
`as_declared` keeps a `cast` arm whose stated reason was the view type, and its doc comment now
states a false fact — a handling site the spec's identifier grep missed; (2) the deleted #183
pin was the only device-side assertion of the unload's `declared vs exported` refusal
(`gpu_backend/mod.rs:241-247`). Nits: `null_exprs` and the aggregate `intermediate` schema are
outside the rule (guarded transitively); `check_expr_types` re-walks each subtree once per
ancestor; the `corpus_cases.inc` comment above `q13` is 11 lines and narrates history;
`src/common.rs:89,44-52` comments name deleted arms and a nonexistent function; #183 over the
30-line cap. The coordinator trimmed #183 and #215 (`971f2c6e`); the rest go to the developer.

## Restart — 2026-09-16, round 1 fixes in flight

A fresh coordinator found the board at `reviewing`, `b070a616` pushed, and the round-1 fix
dispatch dead with its predecessor: twelve files modified in the working tree, uncommitted,
nothing proven, and only the "sites outside the spec's list" note below reached this file.
Redispatched a developer to audit the tree against the round-1 list, finish it, and prove it.
Hosts: **verda down** (name resolution), **shad-gpu up**, 0 MiB held.

**PR #158 is `CONFLICTING` against master and has never had a CI run** — GitHub cannot build
the merge ref, so the `pull_request` event never fires. Master moved by chain D's `empty-build`
(`76f17db2..491f1afc`); both sides touched `corpus_cases.inc`, `cost-registry.csv`,
`build-test.md`, `tickets.md`, `tasks.md`. A rebase is the human's call through the control
file; until it lands, `completeness approved` cannot reach `done`.

## Completing — 2026-09-16

Round 1's fixes committed as `83396f59`, pushed. Review round 2: 0 blocking, 0 important, 2
nits; all six round-1 findings closed. The reviewer verified `build-test.md`'s counts against
the tree (lib 541, executors 65, plan types 36, harness 232, total 1846 — the header's
off-by-one against its own sum, 1845, predates the fork) and read `architecture.md`'s owning
sections: nothing falsified. Nits: #183 at 17 lines against the 15 cap, trimmed here; the
board prose for this task says the option lives in `build_session_state` and the scan is "the
only producer", where the code sets it on the `ParquetFormat` in `read_table` and the spec
names `greatest`/`least` as a second — the prose is master's side, so it is left for the human
or helper to amend at the rebase.

## Completeness pass — 2026-09-17

Two blind readings. **Reviewer (what is wrong): 0 blocking, 1 important** — the T19 batch
comments in `corpus_cases.inc` still gave #183 as the cause beneath them and said "No device
cell" above three lines this branch enabled at `tp1_single`. Comment-only, so the coordinator
rewrote every block to the csv's current state, verified no non-comment line moved and every
block at or under ten lines. **Analyst (what is missing): 0 blocking, 2 important** — (1)
`build_session_state` leaves the session config's `schema_force_view_types` at its default
`true`, and DataFusion's `register_parquet` reads that config rather than `read_table`'s
`ParquetFormat`, so `src/tests/join_fixture.rs`'s `big` still infers `pad` as `Utf8View`;
nothing selects `pad` today, so nothing is red, but it is a door the spec's Restriction says
must not exist — to the developer: the one assignment in `build_session_state`, a red-then-green
test at that door, and a clause on `read_table`'s comment; (2) #185, #187 and #191 carried
pre-rollout cell counts — the coordinator added one dated rollout line each, with the row
counts read from the csv (22, 50, 3; the analyst's 21 and 47 were undercounts). `architecture.md`:
none falsified, both readers agree. The analyst's evidence trail is the last section of this file.

## Completeness approved — 2026-09-17

The session-config door closed in `096afbf1` (one line, one red-then-green test, `--lib` 542,
goldens unmoved); a scoped reviewer read that commit alone: 0 blocking, 0 important, both
settings load-bearing on disjoint paths, the guard shown to go red. Signoff appended to the spec.
`done` needs CI green, and CI cannot run while PR #158 conflicts with master — waiting on the
human's `rebase`. The chain carries on to `decimal-precision-at-export` meanwhile.

## Developer notes

### Dispatch 1 — what was done, in the plan's order

**Task 1, the option and the rule.** Three tests in `plan/validate/tests.rs` written first and
run red (`got Ok(())` on all three); then `is_view_type` and `check_expr_types` in
`plan/common.rs`, `no_view_types` in `validate.rs`'s `walk`, `check_expr_types` at the top of
`check_column_refs` (which covers the five node call sites) and once more at the top of
`join.rs`'s `check_filter_columns`, the residual's own path. Then `with_force_view_types(false)`
on the `ParquetFormat` in `lib.rs`. With the option flipped the plan goldens went red on every
string column and on nothing else — the option took, and the rule refused nothing in the corpus.

**Task 2, the deletions.** Exactly the spec's list, nothing outside it:
`wire/serialize.rs` (the `Utf8View` literal arm, the two type arms), `wire/fb_text.rs:348`,
`cpu_backend/spark_partitioning.rs` (the two cast arms and the imports they needed; the
function's doc went with them, since it only described the cast), `common.rs` (the two view
imports, the structural arm's view variants, the two content arms, and one comment word),
`test_support/result_text.rs:92` (the sentence now names #187's decimal instead),
`executor/errors/tests.rs` (`l_year: Int32 vs Int16`, #191's pair, keeps the two-clause
point), `tests/gpu_tests/harness_cases.rs` (`declaring_view_strings`, the #183 pin, and the
three imports only they used). No exhaustiveness arm was needed anywhere. Chain E's Task 8 left
no survivor: the grep over `peacockdb-core/src/tests` was clean after the harness edit.

**Task 3, goldens.** `plan_text/tests.rs:150` and `schema_tests.rs:156,285,305` re-asserted
on `Utf8`. Regenerated with `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1` over
`planner::tests::plan_goldens`: **11 files**, 11642 lines each way. Every line is the word
swap except two things the spec did not anticipate, both consequences of the same option and
neither a finding against the code:
- `recipe-payloads.txt`: 18 `sha256=` lines moved, because the payload bytes carry the type
  tag (`Utf8View` → `Utf8` in every scan schema and string literal) and the digest is over the
  bytes.
- `tpcds/q24`, 2 lines in each of the 5 tpcds mode files: the residual
  `c_birth_country@probe:12 != CAST(upper(ca_country@build:3) AS Utf8View)` is now
  `c_birth_country@probe:12 != upper(ca_country@build:3)`. DataFusion 45's `upper` returns
  `Utf8` for a `Utf8View` argument (`utf8_to_str_type`), and `string_coercion` then cast it
  back to the column's view type; with both sides `Utf8` there is nothing to coerce. That cast's
  target was itself a view type, so it is the one expression in the corpus the rule would have
  refused had the option not removed it.
  Checked mechanically: minus lines with `Utf8View→Utf8` substituted, against plus lines,
  differ in exactly those 28 lines (18 digests, 10 q24) and nothing else.

**Task 4, the device.** Three cycles on shad-gpu, none with the `[rmm]` neighbour line:
- harness, `PCK_TEST_FILTER='_cases'`: `test result: ok. 228 passed; 0 failed` (lib);
  `test_gpu_corpus` matched 0 and the gate accepted it.
- rollout, all 76 at `tp1_single`, `PCK_TEST_FILTER='gpu_'`: lib `286 passed; 0 failed`;
  corpus `9 passed; 74 failed` — 73 cells plus the registry test, red by construction while the
  csv was untouched. The table is below.
- final, registry and corpus edits applied: lib `286 passed`; corpus `10 passed; 0 failed`
  (nine cells and `the_registry_matches_the_gpu_corpus_in_both_directions`).

**No sink showed a string.** Every one of the 73 red cells failed on something the string
class had hidden: the decimal class (#187), the year (#191), `GpuAggregateBatches`'s `in_rows`
(#185), or a class no ticket named — the cpu's joins answering DataFusion's batch stream
(8192-row splits and one empty batch per unmatched probe batch) where the device answers one
table per call, so `output_bytes` and the batch lists differ by the per-batch overhead. That is
[#215](active-tickets.md#t215), opened here; the attribution was checked on `tpch/q15`
(`[[8192,1808]]` vs one batch, 946045 vs 946033) and `tpch/q4` (18 empty batches × 4 bytes =
the 72-byte gap at the join and at the aggregate over it) and `tpch/cross-join` (5 batches of
25 vs one of 125, 92 bytes). Four cells ran the whole device plan clean and failed only because
their cpu cell is off on #163, so the golden section says `skipped`.

**Registry rule applied.** For every one of the 76 rows: the ticket the run proved for
`tp1_single` is added; `183` is struck only where `152` is also on the row, because the T19
comments in `corpus_cases.inc` record #152 as the other four modes' cause for those rows.
Ten rows keep `183` beside the new ticket, their other four modes never having been run past
the string class: aggregate-groupby, anti-join, cross-join, nested-loop-join,
nested-loop-left-join, semi-join, shuffle-additive, shuffle-stddev, tpch q4, tpch q15. The
#163 rows whose sink showed a decimal gained `187` as well; their device cell cannot be enabled
until the cpu's is.

### Rollout table — 76 queries at `tp1_single`

| verdict | queries | registry |
|---|---|---|
| green, enabled | tpcds q84; tpch nested-loop-join, shuffle-stddev | `gpu_tp1_single` enabled; q84 drops 183 |
| sink, decimal (#187) | tpcds q3 q7 q8 q15 q18 q19 q24 q25 q26 q30 q37 q40 q42 q43 q45 q46 q52 q55 q56 q58 q59 q60 q65 q68 q76 q79 q80 q81 q82 q85 q91; tpch aggregate-groupby anti-join q1 q2 q10 q18 q22 semi-join shuffle-additive shuffle-additive-avg | `187` added where absent |
| sink, `Int32` vs `Int16` (#191) | tpch q7 q9 | `191` added |
| golden, `in_rows` at `GpuAggregateBatches` (#185) | tpcds q21 q31 q34 q50 q62 q66 q73 q83 q99; tpch q5 q12 q16 rollup-over-join | `185` added |
| golden, batching at a join (#215, new) | tpcds q4 q10 q11 q23 q29 q69 q74; tpch cross-join nested-loop-left-join q4 q15 q20 q21 | `215` added |
| device clean, no cpu cell (#163) | tpcds q1 q17 q22 q35 | unchanged |

The raw first-difference line per cell is in the run log the coordinator can regenerate with
the rollout filter; the categories above were read from `declared vs exported`, the golden's
node line at the reported line number, and the `skipped` sentinel.

### Proving commands, rust-only (local, verda down)

- `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2`:
  `test result: ok. 536 passed; 0 failed; 2 ignored`
- `--test test_module_layout`: `17 passed`; `--test test_golden_format`: `26 passed`;
  `--test test_cost_model`: `3 passed`; `--test test_corpus_goldens`: `20 passed`
- `--test test_cpu_corpus --test test_ci_coverage`: `448 passed` and `8 passed`
- `--test test_cpu_corpus -- registry`: `1 passed` after the csv edit
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src testdata/goldens` → `plan/common.rs`
  (the rule) and `plan/validate/tests.rs` only.

### Things the next developer should know

- `rustfmt` on `lib.rs` follows every `mod` and reformats eight unrelated files; they were
  restored from `HEAD`. Name the leaves.
- The impl plan's Task 5 mentions a `bug_` table in `build-test.md` that loses the pin; no such
  table exists, so the harness row's count moved (232 → 231) and nothing else.
- `build-test.md`'s "Plan types" row is `validate/tests` + `layout/tests` + `aggregate/tests`
  (23 + 4 + 4 = 31, now 34), not the one file it links.
- #23's note in `tickets.md` still describes the option flip as the fix for #183, which is now
  true of the bump rather than of the present; left as the spec asked (`#23 unchanged`).

### Round 1 fixes — what was done

The dead dispatch had already edited every file the six findings name; nothing was proven.
The audit found each finding's change complete in the tree, and this dispatch added the doc
fix on `as_declared`, the red runs, the proofs and this record. Per finding:

1. **`cpu_backend/source.rs` `as_declared`** (important). The `cast` arm and the false doc
   are gone; the relabel to the declared schema stays and a column the file holds in another
   type is `RecordBatch::try_new`'s refusal — `"the scan's batch is not its schema: column
   types must match schema types, expected Int32 but found Int64 at column index 1"`. It names
   both types and the position, not the field name; the doc says so. The grep missed this site
   because its reason was prose: the doc said the reader's string "reaches the rest of the plan
   as a view type", and the cast was there to make that true. Test
   `a_column_the_file_holds_in_another_type_is_refused_rather_than_cast` in
   `cpu_backend/tests/source.rs`, run red against HEAD's `source.rs` (panicked at "Int64 in the
   file and Int32 declared, and the read went through"), then green. The full cpu corpus is the
   proof that nothing needed the cast.
2. **The device pin** (important). `harness_cases.rs` gains
   `the_sink_names_a_column_whose_exported_type_is_not_the_declared_one`: a `GpuUnload`
   declaring `s` as `LargeUtf8` over a batch uploaded as `Utf8`, asserting the refusal message
   and `(declared vs exported: 5 s: LargeUtf8 vs Utf8)`. `LargeUtf8` is on the wire
   (`serialize.rs:117`) and is not a view type, so the rule admits it and the device exports its
   one string layout as `Utf8`. Not a `bug_` case: the refusal is the right answer, so
   `script.rs`'s `gpu_refuses` doc now says it serves both. No red run on the device — the state
   that would fail it is before `6ad5857d`; the assertion on the exact message is what shows it
   is not passing vacuously.
3. **`null_exprs` and the intermediate schema** (nit). `AggregateBody::validate` runs
   `check_expr_types` over `null_exprs`; `no_view_types` in `validate.rs` checks the
   intermediate of `GpuAggregate` and `GpuAggregateBatches` through `try_as_node_ref` as
   `intermediate column N`. Two tests in `validate/tests.rs`, both run red against HEAD's
   `plan/mod.rs` and `validate.rs` (`got Ok(())`), then green.
4. **`check_expr_types` re-walk** (nit). `check_column_refs` now calls it once at the top and
   recurses through a private `column_refs_in_range`; `join.rs`'s residual path was already a
   single call. No behaviour change, no test.
5. **The `q13` comment** (nit). Eight lines, current state only: the four causes are #152,
   #184, #185 and the string class, whose cells are off on #187, #191, #185, #215.
6. **`src/common.rs` comments** (nit). The `array_content_size` doc no longer names View
   layouts; the `type_structural_size` fallback comment is four lines and names no
   `assert_type_accountable`.

**Proofs, local (verda down), `--test-threads=2`:**
- `--lib`: `test result: ok. 539 passed; 0 failed; 2 ignored`
- `test_ci_coverage` `8 passed`; `test_corpus_goldens` `20 passed`; `test_cost_model`
  `3 passed`; `test_golden_format` `26 passed`; `test_module_layout` `17 passed`
- `test_cpu_corpus`: `test result: ok. 448 passed; 0 failed`
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src testdata/goldens` → `plan/common.rs`
  (2) and `plan/validate/tests.rs` (11) only. No golden moved: no change here alters plan
  shape.
- `rustfmt --check` clean on every touched leaf and on `plan/mod.rs` itself.

**Device, shad-gpu, `PCK_TEST_FILTER='_cases'`**, `--build` (warm caches, minutes), then
`--push-binaries --patch`, then `--run`, each foreground under a timeout; no `[rmm] … could not
be built` line, pools reserved as declared. Two rust binaries ran: `peacockdb_core_gpu_lib`
`test result: ok. 229 passed; 0 failed; 0 ignored; 0 measured; 602 filtered out` (the new pin
`... ok` among them; 228 before it); `test_gpu_corpus` `0 passed; 0 failed; 11 filtered out`.
The five C++ binaries ran unfiltered as the gate always does.

**`build-test.md`** checked against `--list`: `--lib` 541 (539 + 2 ignored); CPU backend
executors 65 (`executor::cpu_backend::tests::` less `contract`); Plan types 36 (28 + 4 + 4);
gpu rung 287, of which `tests::gpu_tests::` 232 — the harness row; grand total 1846. The
harness row's prose names the new sink case.

### Completeness fix — the session config door

The analyst's item 1. `build_session_state` now sets
`execution.parquet.schema_force_view_types = false` on the session config, so DataFusion's own
`register_parquet` — which builds its `ParquetFormat` from that config alone — agrees with
`read_table`, whose format keeps its own `with_force_view_types(false)`. `read_table`'s comment
gained the clause saying so; both comments sit under the four-line body cap.

- Test: `planner::tests::join_capability::a_table_registered_through_the_session_config_declares_its_strings_utf8`
  — the fixture's `SELECT pad FROM big`, planned by DataFusion through `register_parquet`, asserts
  `pad` is `DataType::Utf8`. Red before the line (`left: Utf8View, right: Utf8`), green after.
  It names `Utf8` only, so the Verification bar's grep is unchanged.
- `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2`:
  `test result: ok. 540 passed; 0 failed; 2 ignored`; `git status testdata/` empty — no golden moved.
- `--test test_module_layout`: `17 passed`; `--test test_ci_coverage`: `8 passed` — a new test in
  an existing module, no CI change.
- `grep -rn "Utf8View\|BinaryView" peacockdb-core/src testdata/goldens` → `plan/common.rs` and
  `plan/validate/tests.rs` only.
- `build-test.md`: `--lib` 541 → 542 and the join-capability row 13 → 14, both read off `--list`;
  cpu 1012 → 1013, Rust 1403 → 1404, grand total 1846 → 1847.
- `rustfmt` was run on `join_capability.rs` alone; `lib.rs` was checked with `--check` and the
  chain split by hand to its output, since formatting `lib.rs` reformats eight other files.

## Completeness pass, analyst — 2026-09-17

The branch read as one change against the spec's four items, Scope, Restriction, Registry and
Verification bar, `architecture.md`'s owning sections and `build-test.md`'s counts. 0 blocking,
2 important.

1. **`build_session_state` leaves the option at its default, and DataFusion's own registration
   reads it** (important; spec item 1 and the Restriction). `read_table` sets
   `with_force_view_types(false)` on its own `ParquetFormat`, the production path.
   `SessionContext::register_parquet` builds `ParquetFormat::new().with_options(table_options.parquet)`,
   and `parquet.global` there is a copy of `config.execution.parquet`
   (`datafusion-45.0.0/src/execution/context/parquet.rs:50`,
   `datafusion-common-45.0.0/src/config.rs:1398-1401`), whose `schema_force_view_types` defaults to
   `true` (`config.rs:430`). `src/tests/join_fixture.rs:89-97` takes that path from
   `build_session_state` over `big`, whose `pad` column is `Utf8` in the file and `Utf8View` in the
   inferred schema. No test selects `pad` today, so nothing is red; `SELECT pad FROM big` through
   `planned` would meet the rule's refusal. The spec's own words make that a finding: "a `Utf8View`
   that survives the option". Fix: one line in `build_session_state`,
   `config.options_mut().execution.parquet.schema_force_view_types = false;`, a test at the door —
   the physical plan of `SELECT pad FROM big` declares `pad` as `Utf8` — red before and green after,
   and a clause on `read_table`'s comment saying the other registration path reads the session config
   alone. The same line makes #23's note (`tickets.md:424-426`) and the board prose true as written.
   The spec's "not the session config" is right about `read_table` and says nothing about
   `register_parquet`. The spec-sanctioned alternative is a ticket and no code.
2. **#185, #187 and #191 keep their pre-rollout cell inventories** (important; Registry, "gets a
   ticket naming what the values showed", and the shared rule that wiki content agrees with the
   tree). The csv carries `185` on 21 rows, `187` on 47 and `191` on 3; the tickets still say "Eight
   device cells", "Six device cells" and "One cell, `tpch/q8`". The rollout's lists live only in this
   file, which is deleted at archive, and the #183 note carries counts alone. Fix: one dated line per
   ticket in `active-tickets.md` naming the rollout's cells at `tp1-single`, from the table above.

**`architecture.md`: none falsified.** Read: Planning with Modes and knobs and The row-group mapping;
Grouping sets; Every cast is explicit; Traits; Memory accounting; Determinism rules; The wire format
and its three subsections; Interfaces; Rehash and the comet hash; Column indexing; cuDF options with
the IPC export row and What the Rust side puts in the flat buffers; Node display, Types are a plan
fact; Cost model. The page names no string type. The sentences nearest the change stay true: "No
executor may change a type the plan did not ask it to" (two executor-side casts were removed, none
added); "Statement order is the wire format … Regenerating it to silence a red defeats its purpose"
(the 18 digests moved because the payload bytes carry the type tag, re-derived below); the IPC export
row says nothing of strings.

**Checked and consistent.** Every golden diff line is the word swap except the 18 digest pairs and
q24's two lines in each tpcds file (minus lines with the substitution against plus lines, per file).
The csv against the rollout table row by row: 71 rows changed = 76 − `tpch/q2`, which already carried
187, − the four #163 rows; all 60 rows carrying `183` at the fork changed; ten keep it, as the spec
allows; no disabled cell lacks a ticket. The rule reaches every `Expr` field in `plan/mod.rs` — filter
predicate, project list, group keys, `null_exprs`, aggregate args, finalize, both joins' residual
through `check_filter_columns` — and both schemas; `AggCall.outputs` types never reach the wire (names
and widths only); `validate` runs at `planner/pipeline.rs:38,73` before any recipe is written.
`build-test.md`'s deltas (+6 cpu, +3 gpu, +9 header) match the tree. No `pub` item added. The four
files outside the Scope table — `cpu_backend/source.rs`, its test, `plan/mod.rs`, `gpu_tests/script.rs`
— are round-1 fixes inside item 3's intent and the Restriction, recorded above. Nothing assigned to
this task is left for the wire task; `cpp/`, `flatbuffers/`, python and the workflows carry no
reference to the deleted names. PR #158 is `CONFLICTING` with no checks, as recorded.

## Rebase needed — 2026-09-17

The control file said `rebase`. Task 2 (`ENS-decimal-precision-at-export`, PR #159, at
`reviewing`) is marked `rebase needed(reviewing)` on its own branch. This branch rebases onto
master (`491f1afc`, chain D's `empty-build` merged and archived) first; the conflicts expected
are `tasks.md`, `tickets.md`, `build-test.md` (mine) and `corpus_cases.inc`, `cost-registry.csv`
(a developer's). `completeness approved` is restored only after the developer re-runs the
proving commands green on the new base; then CI, then `done`, then task 2's turn.
