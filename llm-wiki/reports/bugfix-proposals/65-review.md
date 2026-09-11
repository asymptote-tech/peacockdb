# #65 review — the device's `__grouping_id` is not DataFusion's

Read-only against master 188c23ce (the proposal cites c18e063a; the three commits between are
`llm-wiki/` only, so every code line cited is unchanged). Paths relative to `/media/data/peacockdb`;
DataFusion paths under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

## 1. Verdict

**Needs changes.** The root cause is right, the fix is the right shape and is as localized as it
says, and nothing enabled today — no cell, no golden, no pinning test — goes red. Two things a
developer meets in the first hour are missing: a third Python assertion on the old value that the
proposal says does not exist (CI's cost-report job is red at the first push), and the one pin that
reads the id through a real plan on a device is left optional. No blocking finding.

## 2. Findings

1. **A third pin on the wrong value, and the proposal says there is none.** Section 3 says
   "Nothing else reads the value: `test_end_to_end.py` … use `GROUPING_ID` as a column name and
   compare results after the projection." `scripts/exec_model/tests/test_end_to_end.py:310`
   (`test_a_rollup_carries_the_grouping_id_through_the_whole_sequence`) asserts
   `set(got[A.GROUPING_ID]) == {0, 2, 3}` before its projection step. That file runs in the
   cost-report job (`build-test.md`, "Exec-model prototype (Python)"), so the branch is red in CI
   until it is touched. Severity: **important** (the fix is one line, but the list of pins is what
   the developer works from). Correction: `:310` → `{0, 1, 3}`, beside the two `test_operators.py`
   edits. `grep -rn "GROUPING_ID" scripts/exec_model` shows no fourth reader of the value.

2. **The required pin never reads the id through a plan on a device.** The contract case pins the
   raw column at the unload, which is the C++ rule and is the right primary pin. But the ticket is
   about a query that *reads* the column, and the reading path — the finalize project copying a
   `UINT8` column (`expr.cpp:838-855`, bare `ColumnRef`), then `CAST(__grouping_id AS Int32)` on
   the column path (`expr.cpp:912-934`, `cudf::cast`) — is exercised only by the section-5 query,
   which section 3 lists as "Optional … contingent on the analyzer (risk 2)". Severity:
   **important**. Correction: make the recipe-walk addition required — one `const GROUPING: &str`
   beside `ROLLUP` (`test_gpu_recipe_walk.rs:640`), one `#[tokio::test]` calling
   `assert_walk_matches_datafusion(GROUPING, ONE_LANE)`, and the pair added to the list at
   `:820-830`. It is red before the fix by value (`results_agree` compares rendered rows,
   `tests/common/result_text.rs:128`; the device answers `g=2` on five rows). Risk 2 resolves at the
   first run rather than staying a risk: `ResolveGroupingFunction` rewrites any `grouping()` in an
   `Aggregate`'s `aggr_expr` (`resolve_grouping_function.rs:124-136`), which is where the SQL
   planner puts a select-list call; q70/q86 fail on a call that survived inside a window/sort
   expression (`tp1-single.plans.txt:7320`, `:9549`), and the section-5 query has neither. If
   DataFusion 45 still refuses it, the test is dropped and the refusal text goes in the archive
   note. Consequence the proposal does not list: `build-test.md:19`'s "Recipe walk on a device" row
   goes N 10 → 11 and the grand total at `:7` 1569 → 1570.
   The stronger alternative is a `corpus_query!` line over `nation` at all five cpu modes *and*
   all five gpu modes — nation is under `SMALL_TABLE_BYTES` so every mode is one lane and one
   batch (no merge, so #185 is not met; no join, so #152; no string, so #183; no decimal, so #187),
   which makes it the first device rollup cell in the corpus. That costs five plan-golden sections,
   five `.cpu.txt`/`.cost.txt` sections, a `.result.txt` section, a registry row, a
   `tpch-queries/*.sql` and a DuckDB cost profile (DuckDB 1.5.4 pinned) — an M, and a separate
   decision. The walk test is the S-sized form of the same pin.

3. **A second `architecture.md` sentence is falsified and not listed.** `:276`: "The gid is a real
   column: the expansion materializes an INT32 constant per set". After the fix it is a `UINT8`
   (`UINT16/32/64` above 8/16/32 keys). The proposal rewrites only `:284-286`. Two comments in the
   same state: `aggregate.cpp:329-335` ("tag rows with a distinct id, and concatenate" — the id is
   now DataFusion's, not merely distinct), and `executor_cases.inc:50` ("The answer as `k|v` rows,
   sorted — or `lane|k|v` where the shape is a scatter") gains a third form. Severity: minor.
   Correction: `:276` → "materializes DataFusion's `__grouping_id` per set — one unsigned constant
   at the width the key count picks —"; fold the `:329-335` sentence into the same rule; add
   "`k|v|__grouping_id|sum(v)` for the rollup" to the `expect` doc.

4. **"#65 was outside the audit's scope (`hacks-audit.md:8-9`)" is not what those lines say.**
   `:8-9` list the tickets excluded as already-known work; #65 is not among them. The audit did
   not *read* the grouping-set expansion — "`aggregate.cpp` beyond `execute_aggregate`'s first 200
   lines" is in its "What I did not read" (`:372`). So "nothing in production is shaped around it"
   is the proposer's own grep, not the audit's finding. I repeated the grep: `__grouping_id` in
   `cpp/` is `aggregate.cpp:191, 331, 332, 386` (comments) and `:431` (the name push) — the
   proposal's "`:431` only" undercounts the hits but the conclusion holds: no branch, flag or
   fixture avoids the value. Severity: minor. Correction: say "unread by the audit; grep confirms".

5. **`nkeys > 64` is silent on the device.** `group_id_array` (`aggregates/mod.rs:1268`) returns
   `not_impl_err` on the CPU; the proposed C++ folds into a wrapped `uint64_t` and answers. The
   proposal says "the CPU fails there first" — true for a corpus run, which the CPU authors, and
   untrue for the device alone. Severity: minor (65 grouping columns is not a query anyone writes).
   Correction, optional: one `throw std::runtime_error("grouping sets with more than 64 columns
   are not supported")` above the fold, mirroring DataFusion's own limit rather than inventing
   one — the "both engines refuse alike" argument the proposal makes for the hasher applies here
   too. If left out, say so in the helper's comment.

6. **`functools` is not imported in `aggregates.py`** (`:36-44` import `annotations`,
   `dataclass`, `numpy`, `pandas`, and two relative modules). Severity: minor. Correction: add the
   import, or write the fold as a two-line loop.

## 3. Claims verified

Opened and found true:

- `aggregate.cpp:384-398` folds `gid |= (1 << i)` into an `int32_t`; `:414-415` materialises it
  with `numeric_scalar<int32_t>`; `:431` names it. `group_id_array`
  (`datafusion-physical-plan-45.0.0/src/aggregates/mod.rs:1267-1285`) folds `(acc << 1) | is_null`
  over positions 0..n at `UInt8/16/32/64` by `len() <= 8/16/32`; `grouping_id_type`
  (`datafusion-expr-45.0.0/src/logical_plan/plan.rs:3223-3233`) picks the same widths; the bit
  convention doc at `:3239-3247`. A two-key rollup is 0, 1, 3 there and 0, 2, 3 here.
- `cpu_backend/mod.rs:344-348` builds `PhysicalGroupBy::new(keys, null_exprs, grouping_sets)`, so
  the CPU's id is DataFusion's; `check_state_layout` (`:466`) compares types only.
- `translator/aggregate.rs:261-266` takes the gid field from the partial's schema; `group_fields`
  (`aggregates/mod.rs:258-282`) pushes `Field::new(INTERNAL_GROUPING_ID, grouping_id_type(n),
  false)`; `:334-340` the merge groups on keys + id.
- The wire carries masks and placeholders and no output schema: `aggregate_writer.rs:41-64`,
  `:89` (`aggr_input_schema` is the input's), `gpu_plan.fbs:393-407`. `fb_text.rs` and
  `node_text.rs` print masks, never an id; `recipe-payloads.txt` has five `__grouping_id` hits,
  all names. No golden regenerates.
- The unload's type check: `gpu_backend/mod.rs:179` `concat_batches(&self.schema, …)` →
  `RecordBatch::try_new`, which refuses `Int32` against a declared `UInt8`. The export path
  (`gpu_executor.cpp:49-76`) widens only `DECIMAL32/64`.
- `build_scalar` (`expr.cpp:453-497`) has no `UInt8..UInt64` arm; `build_expr`'s AST literal arms
  (`:157-245`) have none either; `fb_to_binop` (`:498-521`) and `fb_to_ast_op` have no shift arms.
  `fb_to_type_id` (`:74-95`) does map the unsigned types, so a `CAST … AS Int32` over a `UINT8`
  column and `infer_expr_type` on one both work. A bare NULL literal short-circuits to
  `build_scalar` in `build_column` (`:830-835`) before the AST path, so the `Int64(None)`
  placeholder the new case sends is built invalid — #198's arm is not reached.
- `serialize.rs:92-99` writes any `None` scalar with `is_null` and its type.
- The hasher: `spark_hash_partition.cu:163-179` dispatches `STRING/INT32/INT64` and
  `CUDF_FAIL`s otherwise. #189 (`active-tickets.md:224-235`) is the CPU refusal on `UInt8`.
- Enabled device cells: `corpus_cases.inc:18` (`q6`, five modes) and `:25` (`q19`, `tp1_single`)
  are the only lines with gpu modes; neither is a rollup. `rollup_over_join` (`:97`) and `q5`
  (`:212`) are cpu tp1 only. The recipe walk's `ROLLUP` (`test_gpu_recipe_walk.rs:640, :748`)
  compares the root result after the projection (`:582-608`), so it stays green.
- `executor_cases.inc:107-111` `SumByKeyAndGroupingId` is a merge over a manufactured id; the
  device half (`contract.rs:156-200`) makes it with an `Int64` literal project; the CPU half
  (`test_cpu_executors.rs:245-282`) with a `UInt8Array`. Neither runs the expansion.
  `cpu_backend/tests/exec.rs:442` already runs a one-key grouping-set init through
  `CpuExec::aggregate` with a `Utf8(None)` placeholder, so the CPU half of the new case builds.
- `merged` / `merged_over` (`test_cpu_executors.rs:330`, `contract.rs:263-300`) stack init and
  merge exactly as the proposal describes; `Session::open` goes through `attach_recipes` and
  never `validate`, and `Schema::new` carries no annotations, so the test-side `Schema`s pass.
- The expected rows: string sort puts `NULL|…` before `a|…` (`N` < `a`) and `a|2` before
  `a|NULL` (`2` < `N`); `ScalarValue::to_string` prints a `None` as `NULL` on both engines. Set
  `[F,T]` masks `v` and sums it per `k`: 12 and 9; `[T,T]` is 21. The listed nine rows are right.
- Registry: `65` sits on rows 6, 15, 19, 23, 68, 71, 78, 81, 87 and nowhere else; every one is
  disabled or `na` for another reason and keeps another ticket, so `registry.rs:274-285`
  (non-empty tickets on a disabled row) stays green. Rows 71 and 87 carry `143`.
- `tickets.md:19` counts 14 in the section and lists 14; `:278-283` is the ticket; `:720` is the
  #144 cross-reference. `translator/tests.rs:653-655` carries the "#65 is about the id's ENCODING"
  sentence. `build-test.md:20` says "Eleven rows" and the table has eleven.
- Includes: `aggregate.cpp` has `scalar_factories.hpp` and `column_factories.hpp` (`:10-11`);
  `numeric_scalar<int32_t>` at `:288` and `:414`; `make_reduce_agg` at `:118`.
- q70 and q86 are refused at plan time on a surviving `grouping()` call (`tp1-single.plans.txt:
  7320`, `:9549`); `q86.sql` has `rank() OVER (PARTITION BY grouping(…)+grouping(…) …)`.
- `GROUPING(a, b)` in key order takes the cast shortcut (`resolve_grouping_function.rs:207-217`);
  the single-key form emits `bitwise_and` with a `u8`/`u16` literal and a shift (`:219-241`).
- `hash_key_ordinals` (`translator/aggregate.rs:57-63`) transcribes DataFusion's repartition
  keys, gid included — #189's territory, no shared line with this fix.

Not verifiable without a device (stated as the proposal states them, and standard cuDF): `make_
column_from_scalar` over `numeric_scalar<uint8_t>`, `groupby`/`concatenate` over `UINT8` keys, and
`to_arrow_host` exporting `UINT8` as Arrow `UInt8`.

## 4. Corrected proposal

Only the sections that change.

### 3. Localized fix — additions

- `scripts/exec_model/tests/test_end_to_end.py:310` — `{0, 2, 3}` → `{0, 1, 3}`. Three Python
  pins, not two. `aggregates.py` gains `import functools`, or the fold is written as a loop.
- The recipe-walk test is **required**, not optional: `const GROUPING` beside `ROLLUP`
  (`test_gpu_recipe_walk.rs:640`), a test `a_grouping_call_reads_the_id_datafusion_declared`
  calling `assert_walk_matches_datafusion(GROUPING, ONE_LANE)`, and `(GROUPING, ONE_LANE)` in the
  kinds list at `:820-830`. Red before the fix on five rows' `g`. It is the one pin on the
  reading path — finalize column copy then `cudf::cast` — that the contract case cannot reach.
- `llm-wiki/architecture.md:276` — "materializes an INT32 constant per set" → "materializes
  DataFusion's `__grouping_id` per set, one unsigned constant at the width the key count picks".
- `llm-wiki/build-test.md:19` — the walk row's N 10 → 11; `:7` grand total 1569 → 1570 (Rust
  1135 → 1136).
- `cpp/src/operators/aggregate.cpp:329-335` — "tag rows with a distinct id" → "tag rows with
  DataFusion's id".
- `executor_cases.inc:50` — the `expect` doc names the four-column rollup form.
- Optional, in the helper: `if (nkeys > 64) throw …` mirroring `group_id_array`'s refusal, so a
  device alone refuses where the CPU does; else the comment says the C++ wraps past 64.
- Hacks-audit paragraph: "#65 is not in the audit's exclusion list; the audit did not read the
  expansion (`hacks-audit.md:372`). A grep for `__grouping_id` under `cpp/` finds four comments
  and the name push at `aggregate.cpp:431` — nothing shaped around the value."

### 5. Minimum corpus query — addition

The query stands. Its vehicle is the recipe walk (required above). The corpus device tier *could*
carry it too — over `nation` every mode is one lane and one batch, so none of #152/#183/#185/#187
is met and it would be the first device rollup cell — at the cost of a full corpus line (five
plan-golden sections, five `.cpu.txt`/`.cost.txt` sections, a `.result.txt` section, a registry
row, a `.sql`, a DuckDB profile). That is a separate M-sized decision; the walk is the S-sized pin.

### 7. Risks and unknowns — changes

- Risk 2 narrows: the rewrite fires on `aggr_expr` of an `Aggregate`, which is where a
  select-list `GROUPING()` lands; q70/q86 die on a call inside a window/sort expression. The walk
  test settles it at the first run.
- New: `nkeys > 64` wraps silently on the device unless the throw is added.

### 8. Complexity — unchanged

**S.** The additions are one Python line, one walk test, four sentences. With a corpus line
instead of the walk test: M.

## 5. Complexity

**S**, agreeing with the proposal, with the walk test included. The proving run is unchanged: the
C++ build and `test_gpu_executors` + `test_gpu_recipe_walk` on shad-gpu, `test_cpu_executors` and
the exec-model pytest here.
