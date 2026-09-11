# Review of the #46 proposal

Read-only at master 188c23ce. Arithmetic checked with the DuckDB CLI over `testdata/tpcds.sf1`
(read-only queries, no project code run).

## 1. Verdict

**Needs changes.** The diagnosis is right and proved to the cent — #46 is d3c92de5's bug, fixed
the day after filing — but two things a developer meets in the first hour are wrong: the route the
proposal names for verifying q61's device value does not exist as the tier is built, and Part B's
interim registry state is red on the rust-only tier.

## 2. Findings

### F1 — important: the "tp1-single cell will settle it" route is closed by the section assert

§2 ("the `tp1-single` cell will settle it the moment #187 stops refusing"), §5 ("the single device
run that settles q61 itself … compared `golden_exact`") and §7 ("#187's device workflow performs
it") all rest on the device tier comparing q61's *value*. It will not get there.

- `tests/common/corpus_gpu.rs:101-111`: `gpu_case` runs `assert_section` against the mode's
  `.cpu.txt` **before** `assert_result`, and `corpus_golden.rs:111-118` is a byte compare of the
  whole section, `batch_rows` lists included (`plan_text/run_text.rs:78-84`).
- q61's `tp1-single` section (`tpcds.sf1/tp1-single-mini.cpu.txt:2717+`) records DataFusion's
  chunked join output: the store join emits 336 batches of 8192, the item join `[[803,756]]`, the
  promotions `GpuAggregate` `batch_rows=[[1,1]]`, `GpuAggregateBatches in_rows=[[2]]`. The CPU join
  relay returns whatever DataFusion produced (`cpu_backend/join.rs:236-250`, `Vec<CpuBatch>`);
  the device join emits one batch per probe call (`gpu_backend/join.rs:164-180`).
- So once #187 stops refusing, q61 at `tp1-single` goes red at the first differing line — the
  `GpuAggregateBatches in_rows` line, rendered top-down before the joins — which is exactly the
  #185 shape q48/q93/q96 hit "where the device RUNS and the golden catches it"
  (`corpus_cases.inc:146-149`). The value compare never runs.

Aside, one line because it decides how the cell gets attributed: the #185 numbers read as this
same chunking rather than a merge miscounting — q96's `[[1]]` against 34 is one device batch
against 34 DataFusion chunks, and the device's `in_rows` is correct for what it consumed.

Correction: drop the three sentences; say that q61's device value can be confirmed today only
out of band (a hand-driven session reading the unload, the way `test_gpu_recipe_walk` drives a
plan), and that after #187 the cell will land on #185's shape, not on a value compare. The
closure argument does not need the cell — the cent-exact match is the proof — so the proposal
should say it rests on that and nothing else.

### F2 — important: Part B's interim registry state fails the rust-only tier

§3 Part B lands the row with gpu modes `none` "until a shad-gpu run proves the cells" and a
tickets column "only if a gpu cell stays disabled". Those two sentences contradict the loader:
`tests/common/registry.rs:274-283` asserts that a row with any `disabled` cell names at least one
ticket, and it runs wherever the CSV is loaded — `the_registry_matches_the_cpu_corpus_in_both_directions`
on dataset-matrix included. Five `disabled` gpu cells and an empty tickets column is a red test,
and there is no ticket that means "not run on a device yet".

Correction: the shad-gpu run is a step of the task, not an option. The developer runs the five
device cells (`build-test-shadgpu.sh`, `PCK_TEST_FILTER=filter-or-nulls`) before the registry
row is written, enables what ran green, and files a ticket for whatever refused — §7 already
expects `tp4-single` may. Without a device, the row cannot land at all.

### F3 — minor: §5's "one lane and one batch at each" is false at `tp4-single`

`promotion` at `Off` batching plans as four lanes with three empty — q61's own
`tp4-single.plans.txt` (`partition_groups=[[[0]],[],[],[]], lanes=4`, and `GpuFilter … lanes=4`),
the same shape as `cross-join`'s `region`. §7 says so; §5 says the opposite. Beyond the empty-lane
question §7 raises, this cell is the first **multi-lane device unload** in the tier at all: q6's
tp4 cells merge to one lane under the aggregate before the unload, and q19 runs only at
`tp1-single`. So the `tp4-single` device cell tests two new things, and §5 should say so rather
than "refused nowhere".

### F4 — minor: Part B pins one of the two evaluators d3c92de5 changed

The string OR is not AST-able (`expr.cpp:415-420` via the recursion at `:435`), so
`filter-or-nulls` exercises `fb_to_binop`'s `NULL_LOGICAL_OR` (`:514`) and never
`fb_to_ast_op`'s (`:119`) — the arm every non-string disjunction takes, e.g. q61's own
`(d_year = 1998) AND (d_moy = 11)` family. A refactor of the AST arm alone still goes green.

Correction: a second disjunction over nullable Int64 columns, which is AST-able (Int64 column
against an Int64 literal, `infer_expr_type` equal on both sides):

    SELECT p_promo_sk FROM promotion
    WHERE p_start_date_sk = 2450347 OR p_end_date_sk = 2450797;

Verified over the data: promotion 71 has `p_start_date_sk=2450347, p_end_date_sk=NULL` and 58
has `NULL, 2450797`, so three-valued OR keeps both and a null-propagating OR drops both. Either a
second `corpus_query!` line (`filter-or-nulls-ast`) or, less cleanly, an `AND` of the two
disjunctions in one query — the top-level `AND` is not AST-able, so `build_column_binary` hands
the int disjunction to `eval_ast_subtree` and the string one to the column path, pinning both in
one predicate at the cost of a muddier row set.

### F5 — minor: Part A's closing paragraph overclaims if a device cell stays off

"`tpcds/filter-or-nulls` (Part B) carries it now" is true only after the shad-gpu run enables at
least one device cell (F2). Write it after the run, naming the modes that carry it.

## 3. Claims verified

- Ticket text `tickets.md:118-122`; Contents row `:18` lists 17 with `#46`; count 17 → 16 is right.
- `cost-registry.csv:62` = q61, gpu ×5 disabled, tickets `46 152 187`. `corpus_cases.inc:232`
  gpu `none`; batch-14 comment `:223-228` names #152/#183/#185 and not #46.
  `archived-tasks.md:272-275` lists q61 among the ten #187 queries. `#46`/`t46` appear nowhere
  else outside the archive (grep over llm-wiki, tests, cost-report, scripts, cpp).
- `2d07e908` (2026-06-10) filed #46 and left q61's device line commented; `d3c92de5`
  (2026-06-11) changed `LOGICAL_AND/OR → NULL_LOGICAL_AND/OR` in both `fb_to_ast_op` and
  `fb_to_binop`, plus groupby `INCLUDE` and join `UNEQUAL`, and re-enabled 14 queries, q61 not
  among them. At `2d07e908`, `plan_executor.cpp:570-571` had `LOGICAL_OR` on the column path and
  `:474` routed a string literal off the AST, so the promotion predicate did take that path.
  q61's device line stayed commented through `b1375eb8`, `9c0979f3`, and was never re-enabled.
- Current `expr.cpp`: `fb_to_ast_op` `:101-125` with `NULL_LOGICAL_*` at `:118-119`;
  `is_ast_able` `:403`; `fb_to_binop` `:498` with `NULL_LOGICAL_*` at `:513-514`;
  `build_column_binary` `:579`, column-scalar path `:609`, both-columns path `:627`;
  `build_scalar` builds a `string_scalar` for `Utf8View` (`:476-480`). `filter.cpp:22-29`
  as described. The predicate crosses as `Utf8View` literals (`serialize.rs:72-76`,
  `recipe-payloads.txt:3906`).
- Arithmetic (DuckDB over `testdata/tpcds.sf1`): 300 promotions, 158 pass the SQL OR, 155 pass
  it with the three channel columns non-null; the three dropped are `p_promo_sk` 15 `(Y,NULL,N)`,
  71 `(Y,NULL,NULL)`, 196 `(Y,NULL,N)`. q61's promotions subquery = 2894907.87 as written and
  2855378.83 with the null-propagating filter; total = 5586124.26. Golden `mini.result.txt:4704`
  agrees. The sum-shaped variant: 2702543999.35 over 1,448,276 rows, null-propagating
  2650926187.55 over 1,420,745. Every number in the proposal is exact.
- CPU golden `tp1-single-mini.cpu.txt` q61: `GpuFilter` over `promotion` `output_rows=158`.
- `promotion.parquet`: 20,816 bytes, one row group; 5/5/6 NULLs in dmail/email/tv, none in
  `p_promo_sk`.
- The promotions subtree reading: six Inner joins, all `null_equals_null=false → UNEQUAL`
  (`join.cpp:86-90`); keyless decimal `sum` via a one-constant-key groupby
  (`aggregate.cpp:278-291`); the only asymmetry is the `customer_address` join's orientation.
  None of the other d3c92de5 changes (groupby `INCLUDE`, join `EQUAL`) can have contributed:
  no dimension key in q61 carries a NULL, and the aggregate has no keys.
- `3c0750ee` deleted `gpu_case!(tpcds, 1, q7, …)` and the q26 line. `test_gpu_recipe_walk.rs:536-542`
  registers tpch sf1 only; `executor_cases.inc` `INPUT` has no NULL; `cpp/tests/gpu/test_plan_executor.cpp`
  reads `tpch.minimal`; `scripts/exec_model/tests/test_operators.py:166` pins Kleene OR.
- Part B plumbing: `common/mod.rs:41` resolver, `registry.rs:140-142` `stem`, custom tpch queries
  have no `.duckdb_cost.txt` and `cost-report/src/main.rs:522,581` read a missing one as `None`;
  `gen_duckdb_cost.sh:129-154` enumerates `q1..q99` only, so a custom `.sql` is never profiled;
  `PAYLOAD_QUERIES` is a fixed list (`test_plan_goldens.rs:174`) and the cover test compares fb
  kinds and call shapes, which scan/filter/export already hold — `recipe-payloads.txt` stays.
  `TicketIndex::path_for` (`main.rs:335-345`) resolves archived numbers. `archived-tickets.md`'s
  `## Done` entries carry `<a id>` above the header, as the proposal writes it.
- Hacks-audit: nothing shaped around #46; finding 12 (names-only digest) is respected as stated.

## 4. Corrected proposal — sections that change

**§2, last two paragraphs.** Replace "the `tp1-single` cell will settle it the moment #187
stops refusing" with: the corpus tier cannot confirm q61's value on a device — `gpu_case`
asserts the cpu section before the result, and that section carries DataFusion's per-call
chunking at every join over `store_sales`, which the device's one-batch-per-probe join does not
reproduce (#185's shape). Closure rests on the cent-exact match and the code reading; a device
value for q61 is an out-of-band run if anyone wants it.

**§3 Part B.** Order the steps: SQL file; `corpus_query!` line with cpu ×5, gpu `none`; regen
the cpu-tier goldens locally; **run the five device cells on shad-gpu**; then write the registry
row with the gpu cells as proven and a ticket for any that refused. The row cannot land with
disabled cells and no ticket (`registry.rs:279`). Add the AST-path pin (F4) beside it, as a
second line or a second disjunct.

**§5.** `tp1-single`, `tp1-rowgroup`, `tp4-rowgroup`, `tp4-sized`: one lane, one batch.
`tp4-single`: four lanes, three empty, the `GpuUnload` at four lanes — the tier's first
multi-lane device unload. Delete "the single device run that settles q61 itself" or restate it
as an out-of-band run.

**§7.** Replace "#187's device workflow performs it" with "after #187, q61 at `tp1-single`
lands on #185's shape; the value is not compared". Add: Part B cannot land without the shad-gpu
run (F2).

## 5. Complexity

**S**, unchanged — but with the shad-gpu run as a required step rather than an option, and one
more cpu-tier query if the AST pin is taken. Still no code, no ABI, no wire change.
