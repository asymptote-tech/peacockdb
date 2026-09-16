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
Review round 1 dispatched.

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
