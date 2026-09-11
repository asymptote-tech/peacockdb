# #183 — review of the proposal

Read at master 188c23ce (code identical to the proposal's c18e063a; the five commits between are
`llm-wiki/` only). Every `file:line` below was opened. DataFusion 45.0.0, datafusion-common
45.0.0, parquet 54.2.1 read from the cargo registry.

## 1. Verdict

**Needs changes.** The root cause and the two-line fix are right and verified end to end; one
scaffolding step (hashing types in `schema_digest`) reddens five enabled shad-gpu tests through
#187, and the proposal does not say where it lands relative to the chain that regenerates the
same goldens and plans a `bug_` test for this very class.

## 2. Findings

### F1 — `schema_digest` hashing types reddens five recipe-walk tests on shad-gpu (important)

Proposal 3b, third bullet: hash `field.data_type()` beside the name in
`tests/common/result_text.rs:92`. The risk section considers only CPU cells. It misses the
device path that does **not** go through the unload's declared-schema concat:
`tests/test_gpu_recipe_walk.rs:166-179` exports through `peacock_result_from_handle` and decodes
with `StreamReader`, no `concat_batches` against the sink schema, and then
`assert_walk_matches_datafusion` (`:582-607`) compares through `assert_results_match` →
`results_agree` → `schema_digest`. So #187's `Decimal128(38,s)` export reaches the digest there.
Five of the ten walk tests have a decimal at the sink and pass today only because the digest is
names-only: `a_filter_keeps_the_rows_its_predicate_names` (`:649`, `o_totalprice` (15,2)),
`each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them` (`:685`, sum (25,2)),
`an_average_finalizes_to_the_digits_the_oracle_computes` (`:706`, avg (19,6)),
`an_aggregate_whose_input_is_one_batch_finalizes_without_a_merge` (`:725`), and
`a_rollup_answers_with_every_grouping_set` (`:748`). All five go red on the device tier with
nothing in this fix being wrong. That is a regression in the enabled set, and the proposal's own
rule for a red ("a ticket rather than a revert") cannot apply: the ticket already exists (#187)
and an `#[ignore]` or a decimal whitelist is the shape `coding-style.md` forbids.

Correction: drop the type hash from this fix. Rewrite the doc at `result_text.rs:86-91` now, to
the reason that is true after #183: the corpus paths are guarded by the unload's exact-type
check (`gpu_backend/mod.rs:179-183`, and `declared_as` on the CPU); the walk's raw export is not,
and #187 is what a type hash would catch there. Hash types when #187 closes (add that line to
#187, which is where hacks-audit finding 12's "at close" then belongs). This also removes one
commit and one risk item from the plan.

### F2 — the fix is not placed against the board (important)

The proposal reads as a change landing on master alone. Two things on `ENS-drop-mode-name`
touch the same artifacts and one plans a test this fix must delete:

- `tasks/declared-schemas.md` step 1 regenerates all ten `.plans.txt` (`GpuUnload` gains
  `schema=[…]`), the same files this fix regenerates; whichever lands second takes a
  ten-golden conflict, mechanical (regenerate) but a `rebase needed` write on the board.
- `tasks/declared-schemas-impl.md:600-634` and `:711` plan `bug_a_declared_utf8view_is_exported_as_utf8`
  and a row for it in `build-test.md`'s new `bug_` table. If that lands first, this fix deletes
  both (the impl says so itself at `:626`, "Delete this test in the change that fixes #183"),
  and the proposal lists neither. If this lands first, row 1 of declared-schemas' query table
  is dropped, which the proposal does say.
- `tasks/test-layout.md` / `test-support.md` move `tests/common/result_text.rs`,
  `test_plan_goldens.rs` and the corpus harness into `src/`, so 3b/3c's paths move underneath.
- `tasks/refcounted-tables.md:237` names #183 as "which no task owns since casts.md was
  dropped" — this proposal is that owner and should say so; its own `.plans.txt` regen (if
  `Input` names change) is a third collision.

Correction: state the landing order — before task 7 (`sink-divergence-survey`, which then
measures a corpus without the string class) or as a task on the chain after 10 — and add the
conditional deletion of the `bug_` test and its `build-test.md` row to 3c/3d.

### F3 — one unused import the refusal leaves behind (minor)

3b's `as_declared` change deletes the only `cast(...)` call in `cpu_backend/source.rs`; the
import at `source.rs:12` (`use datafusion::arrow::compute::{cast, concat_batches}`) then warns,
and the definition of done is a clean build. The proposal names the import cleanup only for
`spark_partitioning.rs`. Also worth knowing: `RecordBatch::try_new` at `:137` already refuses a
type mismatch with expected/found/index, so the smallest correct form of 3b is to delete the
per-column loop and keep the `try_new` line; the three-fact message the proposal wants is
better for a reader but is the larger edit.

### F4 — two citation slips (minor)

- The "optimized for reading `StringView` … significantly faster" text is
  `ParquetFormat::force_view_types`'s doc at
  `datafusion-45.0.0/src/datasource/file_format/parquet.rs:238-247`, not
  `datafusion-common-45.0.0/src/config.rs:240-248` (that range is `sql_parser` options).
- "Three assertions flip" is four sites in three tests: `plan_text/tests.rs:151`,
  `schema_tests.rs:157`, `:286`, `:306`.

### F5 — comment-only residue not listed (minor)

`cpp/src/spark_hash_partition.cu:134` ("it sees Utf8View→Utf8") and `:177` describe the plan
type as Utf8View; `cpp/tests/gpu/test_plan_executor.cpp:222-228` (and three more nation schemas
in that file) build `fb::DataType_Utf8View` with a comment calling it nation's type. Nothing
behaves wrongly — the enum stays and the C++ folds the three tags — so no change is owed; the
proposal's "nothing in the C++ changes" stands. Listed so a reader grepping `Utf8View` after the
fix is not surprised.

### F6 — the fix is durable for DataFusion 45 only, and #23 asks for 46+ (minor)

Section 2's "in DataFusion 45 this option is the only source of Utf8View" is verified (no
`map_varchar_to_utf8view` / `map_string_types_to_utf8view` in datafusion-sql-45 or
datafusion-common-45). Later DataFusion adds a `sql_parser` option that types string literals
and `VARCHAR` as `Utf8View`; under it a `Utf8` column compared to a literal is cast to
`Utf8View` and the class returns on the expression side, where the proposed regression test
(source declared type = reader type) does not look. One sentence in #23, or in the ticket's
closing text, that the upgrade must set that option off too.

## 3. Claims verified

- The failure site and message: `gpu_backend/mod.rs:179-183` `concat_batches(&self.schema, …)`
  → arrow's "column types must match schema types, expected … but found … at column index N".
- The export: `cpp/src/gpu_executor.cpp:74` `cudf::to_arrow_schema`; cuDF's one string type.
- Root cause: `lib.rs:52` `ParquetFormat::default().with_enable_pruning(true)`;
  `ParquetFormat::infer_schema` (`parquet.rs:326-376`) applies `transform_schema_to_view` when
  `self.options.global.schema_force_view_types` — the format's own options, default `true`
  (`config.rs:428-430`); `register_parquet` → `to_listing_options` (`options.rs:569-589`) →
  `ParquetFormat::new().with_options(table_options.parquet)`; `default_table_options()`
  (`session_state.rs:821-824`) → `combine_with_session_config` (`config.rs:1397-1401`) copies
  `config.execution.parquet` into `parquet.global`. So the one config line reaches both
  `read_table` (with the proposed `with_options`) and `join_fixture.rs:90-92`'s
  `register_parquet`, which builds its context from `build_session_state`.
- Every session that plans or runs a corpus query goes through `build_session_state`:
  `tests/common/corpus.rs:333-336` (engine and oracle alike), `test_cpu_end_to_end.rs`,
  `test_plan_goldens.rs`, `test_gpu_recipe_walk.rs:538`, `test_gpu_abi.rs`, the CLI,
  `schema_tests.rs:30`, `plan_text/tests.rs:9`, `translator/tests.rs:24`,
  `memory_estimation.rs` tests. The bare `SessionContext::new()` sites
  (`parquet_meta.rs:260`, `cpu_backend/mod.rs:392,415`, test `task_ctx()`s) plan no corpus
  schema.
- `source.rs:61` reads with `ArrowReaderOptions::new()` (no schema hint), so the reader answers
  `Utf8`; `as_declared` (`:112-139`) casts to the declaration; its fast path compares fields
  including metadata and the relabel goes through `try_new`.
- `spark_partitioning.rs:53-71` casts `Utf8View`/`BinaryView` keys for comet; the contract
  table already scatters `Utf8` keys on both engines (`executor_cases.inc:12`,
  `test_gpu_executors/contract.rs:144-161`).
- `result_text.rs:85-100` hashes names only and its doc names this ticket.
- Pricing identical for the two types: `common.rs:35` structural `(rows+1)*4`;
  `array_content_size` (`:105-119`) is `offsets[rows]-offsets[0]` for `Utf8` and Σ valid lengths
  for `Utf8View`, equal when null slots carry no bytes; the parquet reader pads nulls
  zero-length and `filter`/`take`/`concat`/row-format emit do too; no corpus query uses
  `nullif`. `memory_estimation.rs:175-183` prices width through `logical_size_from_schema`, so
  `--- memory ---` does not move.
- Goldens: 425 / 3565 `Utf8View` in the two tp1-single plan files; the only `AS Utf8View` cast
  is q24's `CAST(upper(ca_country) AS Utf8View)`, twice per file, all five tpcds files; no `AS
  Utf8)` or `AS LargeUtf8)` anywhere; `recipe-payloads.txt` has 40 lines with `Utf8View`
  including `null::Utf8View` at `:3354`. Literal names in aggregate columns already render as
  `Utf8("…")` (2434 sites) and will not move.
- C++: `expr.cpp:87-89`, `:226-236`, `:331-333`, `:476-478` fold `Utf8`/`LargeUtf8`/`Utf8View`;
  no other C++ logic reads the tag (`scan.cpp` casts decimals only). `gpu_plan.fbs:28-35` keeps
  the member.
- Rust `Utf8View` mentions are exactly the proposal's list plus `wire/fb_text.rs:348` (a total
  renderer, keep): `serialize.rs:72-78,130`, `common.rs:35,115`, `plan_text/tests.rs:151`,
  `schema_tests.rs:157,286,306`, `spark_partitioning.rs:64`, `result_text.rs:89`. None in
  `tests/test_null_analysis.rs`, `test_layout_injection.rs`, `wire/tests.rs`, the exec model or
  the gtests' logic.
- Registry: 60 rows carry `183`; the ten without `152` are the ones named. Sink schemas from
  `tp1-single.plans.txt` give 29 rows with a `Decimal128(p<38, s)` (23 tpcds + tpch
  aggregate-groupby, anti-join, semi-join, shuffle-additive, q10, q18), 2 with an
  `extract` `Int32` (q7 `l_year`, q9 `o_year`), 29 candidates — the proposal's split exactly.
- Pruning: `rowgroup_prune.rs:113-121` builds the predicate over `config.file_schema`;
  parquet 54.2.1 `statistics.rs:494` has the `Utf8View` arm beside `Utf8`, so survivors should
  not move either way.
- `test_cpu_end_to_end.rs:176-186, 242-246` asserts `(name, type)` per query and mode.
- Minimum query `SELECT r_name FROM region`: plans as `GpuUnload` over one loader, reaches
  the unload, fails there, nothing upstream refuses; `region` is in `tpch.minimal`.
  `tpch/cross-join` is the smallest committed query carrying `183` alone.
- The device tier's `live_cpu` compare (`corpus_gpu.rs:159-163`) and golden compare (`:180-190`)
  are safe under a typed digest — both sides pass the unload — which is why F1 is about the
  walk and not the corpus.
- Alternatives: `casts.md` is archived "approach rejected" at `archive/archived-tasks.md:284`;
  the rejection reason (a cast at the unload builds the divergence into the plan) does not
  apply to removing the divergence at the source.

## 4. Corrected proposal — sections that change

### 3b. The scaffolding that goes with it (replaces the third bullet, adds one line to the first)

- `source.rs`: as proposed, and drop the `cast` import at `:12`. Smallest form: delete the
  per-column loop and keep `RecordBatch::try_new(declared.clone(), batch.columns().to_vec())`
  with the existing message; the three-fact message is a welcome larger edit.
- `spark_partitioning.rs`: as proposed.
- `tests/common/result_text.rs:85-100`: **doc only.** Names stay the digest. Rewrite the doc:
  names and not types because the walk (`test_gpu_recipe_walk.rs:166`) exports raw and the
  device's decimals come back at precision 38 (#187), while the corpus paths are already
  type-checked at the unload. Add to #187: "at close, `schema_digest` hashes
  `field.data_type()` too" (hacks-audit finding 12 moves there).

### 3c. Tests (one addition)

- If `declared-schemas` has landed: delete `bug_a_declared_utf8view_is_exported_as_utf8`
  (`wire/gpu_tests/`) and its row in `build-test.md`'s `bug_` table, in this change.

### 3d. Goldens, registry, corpus lines, wiki (one row changes, one is added)

| artifact | moves | why |
|---|---|---|
| `llm-wiki/tasks/tasks.md` | yes | this is a task with a board position: before task 7 of `ENS-drop-mode-name`, or after task 10; it regenerates the same ten `.plans.txt` as `declared-schemas` step 1 and `refcounted-tables` (if `Input` renames), so whichever lands second is `rebase needed(...)` with a regenerate, not a merge |
| `llm-wiki/tasks/active-tickets.md` #187 | one line | the deferred `schema_digest` type hash |

### 7. Risks and unknowns (one item removed, one added)

- Remove "`schema_digest` hashing types may redden a CPU cell": the hash is deferred.
- Add: a later DataFusion (#23's upgrade) types string literals as `Utf8View` through a
  `sql_parser` option; the upgrade must turn it off or the class returns on the expression
  side, where `a_source_declares_the_types_its_readers_produce` does not look.

## 5. Complexity

**M**, as proposed. The code is S (two lines plus three small edits and one test); the eleven
golden regenerations and the shad-gpu rollout over 60 rows make it M. F1 removes work rather
than adding it; F2 adds one board write and one conditional deletion. Of the 60 rows, 31 are
re-ticketed by reading (29 → #187, 2 → #191) and only the 29 candidates need a device run, in
about six batches.
