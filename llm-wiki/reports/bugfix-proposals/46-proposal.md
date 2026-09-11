# #46 — q61 GPU: 'promotions' sum subtree returns the wrong value

Read-only analysis at master c18e063a. Paths relative to `/media/data/peacockdb`.

## 1. Issue

The ticket (`llm-wiki/tickets.md:118-122`) says the device answered q61's `promotions` as
2855378.83 where the CPU answers 2894907.87 (`testdata/goldens/tpcds.sf1/mini.result.txt:4704`),
with the cross join and `total` (5586124.26) correct, and asks for a node-by-node bisect of the
filtered-sum subtree.

What it disables today, corrected against the code: **nothing on its own.** `tpcds/q61`'s five
device cells are `disabled` on `testdata/cost-registry.csv:62` with tickets `46 152 187`, and
`corpus_cases.inc:232` declares `gpu_modes = none`. The batch comment (`corpus_cases.inc:223-228`)
attributes the batch's device refusals to #152/#183/#185 and never names #46. The wire-schema
task's device workflow (`llm-wiki/archive/archived-tasks.md:272-281`) lists q61 among the ten
queries carrying #187 — so q61 at `tp1-single` was run on shad-gpu during T19, executed its whole
tree, and was refused at the unload by #187's decimal widening (`Decimal128(17,2)` declared,
`(38,2)` exported). The other four modes stop earlier on #152: `store_sales` is 24 batches at
`tp1-rowgroup` and 4 lanes × 4 shuffled batches at the `tp4-*` modes, so every join above it sees
more than one probe batch and `build_copy` refuses the second.

So #46 is a legacy name on the wall behind q61's device cells, not the thing refusing them. It is
also why q61 is absent from the exec-model corpus (`00-tickets.md`, legacy table), which is a
Python hand-lowering and outside this proposal.

## 2. Root cause

**The defect was the null-propagating `LOGICAL_OR`, and it was fixed the day after the ticket was
filed. The ticket was never re-verified.**

Evidence, in order:

- The number reproduces to the cent. Over `testdata/tpcds.sf1` (DuckDB, read-only):
  q61's `promotions` subquery as written = **2894907.87**; the same subquery with the promotion
  filter changed to drop any row where one of the three channel columns is NULL — which is
  exactly what a null-propagating OR does to `(a='Y' OR b='Y') OR c='Y'` — = **2855378.83**.
  Three promotions are kept only by three-valued OR: `p_promo_sk` 15 (`Y,NULL,N`), 71
  (`Y,NULL,NULL`), 196 (`Y,NULL,N`). 158 promotions pass the SQL filter (the CPU golden's 158 at
  `tp1-single-mini.cpu.txt`, `GpuFilter` over `promotion`); 155 pass the null-propagating one.
  `total` has no promotion filter, so it was right — which is what the ticket observed.
- Timing. #46 was written in `2d07e908` (2026-06-10, Bucket D). `d3c92de5` (2026-06-11, Bucket
  H) changed both evaluators from `LOGICAL_AND/OR` to `NULL_LOGICAL_AND/OR`, its message naming
  "a disjunctive filter ... dropped rows it must keep", and re-enabled q7/q26 and twelve others.
  q61 was not on that list because its device test carried #46's comment, not Bucket H's, and
  nobody went back.
- The current code has no such path left. The promotion predicate crosses the wire as
  `(((p_channel_dmail@1 Eq 'Y') Or (p_channel_email@2 Eq 'Y')) Or (p_channel_tv@3 Eq 'Y'))`
  (`testdata/goldens/recipe-payloads.txt:3906`, literal type `Utf8View`,
  `wire/serialize.rs:72-76`). `execute_filter` (`cpp/src/operators/filter.cpp:22-29`) asks
  `is_ast_able` (`cpp/src/expr.cpp:403`): the `BinaryExprNode` arm at `:415-420` returns false on
  a string literal on either side, so the whole predicate takes `build_column` →
  `build_column_binary` (`:579`). There the op is `fb_to_binop` (`:498`), whose `And`/`Or` arms
  (`:513-514`) are `NULL_LOGICAL_AND/OR`; each `Eq` takes the column-scalar fast path (`:609`)
  as `cudf::binary_operation(strings, string_scalar, EQUAL, BOOL8)`, which yields NULL for a NULL
  string; the two ORs take the both-columns path (`:627`) under `NULL_LOGICAL_OR`, which is SQL
  three-valued logic (`NULL OR TRUE = TRUE`); `apply_boolean_mask` (`filter.cpp:29`) treats a
  NULL mask entry as false. The AST path's `fb_to_ast_op` (`:101-125`, arms `:118-119`) carries
  the same two operators with a comment naming q7/q15/q26/q79, so a predicate that stays AST-able
  (`d_year = 1998 AND d_moy = 11`) behaves the same way.
- Nothing else in the `promotions` subtree is promotions-only. Reading the `tp1-single` plan
  golden (`tpcds.sf1/tp1-single.plans.txt:6125-6181`): the branch is six Inner hash joins, a
  keyless `sum` partial (`aggregate.cpp:231-315`, the decimal arm at `:278-291` runs a
  one-constant-key groupby because `cudf::reduce` has no fixed_point sum), a merge over one state
  row, and a finalize rename. Every one of those operator arms is the same code the `total` branch
  runs over the same rows (`store_sales` joined to `store`, `date_dim`, `customer`,
  `customer_address`, `item`, all with `null_equals_null=false` → `UNEQUAL`, `join.cpp:86-90`).
  The only asymmetry besides the promotion filter and join is the `customer_address` join's
  orientation (probe side in `promotions`, build side in `total`), and an Inner join is symmetric.
  `ss_promo_sk` is nullable, and `UNEQUAL` matches a NULL key to nothing — the SQL answer.

So the mechanism the ticket asked to bisect for is gone, and what remains of #46 is a ticket that
outlived its fix.

**What is not verified by reading:** that the current device run of q61 produces 2894907.87. The
T19 run reached the unload and was refused there, so the value was computed and never compared.
The comparator on the device tier does compare values (`result_text.rs`: names plus rendered row
text, hacks-audit finding 12), so the `tp1-single` cell will settle it the moment #187 stops
refusing.

**A coverage gap this leaves.** The Bucket H fix was pinned on a device by
`gpu_case!(tpcds, 1, q7, full_table_tp1_standard, golden_exact)` and the q26 line; both were
deleted with the six legacy modes in `3c0750ee` (2026-09-08). q7 and q26 are out of today's corpus
on #163 (avg). tpch has no NULLs, so the six live device cells (tpch q6 ×5, q19 `tp1-single`)
cannot exercise `NULL_LOGICAL_OR`; the executor contract fixture (`common/executor_cases.inc`
`INPUT`) has no NULL; `test_gpu_recipe_walk.rs` is hardwired to tpch sf1; `cpp/tests/gpu/`
reads `tpch.minimal`. The Python model pins the rule (`scripts/exec_model/tests/test_operators.py:166`)
and the C++ does not. A refactor of `expr.cpp` could restore #46 with nothing going red.

## 3. Localized fix

No C++ or Rust logic changes. Two parts: close the ticket and re-pin the rule.

**Part A — close #46 (documentation and registry, three files).**

- `llm-wiki/tickets.md`: delete the `#46` entry (`:118-122`); in the Contents table (`:18`) drop
  `#46` from the Critical-correctness list and change its count 17 → 16.
- `llm-wiki/archive/archived-tickets.md`, under `## Done`: add `<a id="t46"></a>` with the
  ticket text and a closing paragraph: **Done 2026-09-11.** Fixed by `d3c92de5` (LOGICAL_OR →
  NULL_LOGICAL_OR on both evaluators) the day after filing; the recorded 2855378.83 is exactly
  q61's `promotions` with promotions 15, 71 and 196 dropped, the three that a null-propagating OR
  drops. The device pin (q7/q26 result cells) went with the legacy modes in `3c0750ee`;
  `tpcds/filter-or-nulls` (Part B) carries it now. q61's device cells stay off on #187 then #152.
- `testdata/cost-registry.csv:62`: tickets `46 152 187` → `152 187`. `registry.rs:263-283`
  only requires a disabled row to name at least one ticket; `TicketIndex::path_for`
  (`cost-report/src/main.rs:335`) resolves an archived number anyway, so either order of the two
  edits keeps the widget green.

**Part B — re-pin three-valued OR on a device with a custom tpcds corpus query.**

- `testdata/tpcds-queries/filter-or-nulls.sql`:

      SELECT p_promo_sk
      FROM promotion
      WHERE p_channel_dmail = 'Y' OR p_channel_email = 'Y' OR p_channel_tv = 'Y';

  158 rows, one Int64 column: no decimal and no string reaches the unload, so neither #187 nor
  #183 refuses it; no join, so no #152; 300 rows in one row group, so one batch at every mode;
  `promotion.parquet` is 20 KB, so one lane. The custom-query resolver already covers tpcds
  (`common/mod.rs:41`: `<root>/<dataset>-queries/<query>.sql`; `registry.rs:141` maps `_` to
  `-`), and custom queries carry no DuckDB oracle (`tpch/filter-project` has none; a missing
  `.duckdb_cost.txt` reads as `None`, `cost-report/src/main.rs:522,581`).
- `peacockdb-core/tests/common/corpus_cases.inc`: one line beside the tpcds block, cpu at all
  five modes, `data_fusion_exact`, `golden_exact`; gpu modes `none` until a shad-gpu run proves
  the cells, then all five (the rollout's rule: enable what ran green, name the ticket for what
  did not). A two-line comment above it: this is the shape #46 was, and the pin Bucket H lost.
- `testdata/cost-registry.csv`: one row `tpcds,1,filter_or_nulls,enabled×5,cpu enabled×5,gpu
  <as proven>,ok,,` — empty `features`, and a `tickets` column only if a gpu cell stays
  disabled.
- Goldens, all additive sections authored by the cpu tier under `UPDATE_CANONICAL=1`: a
  `== filter-or-nulls` section in each of the five `tpcds.sf1/<mode>.plans.txt`, the five
  `<mode>-mini.cpu.txt` and their derived `.cost.txt`, and one in `mini.result.txt`. No existing
  section moves. `recipe-payloads.txt` does not move: its membership is a cover over fb kinds and
  call shapes, and scan/filter/export are already covered.
- Nothing on the device side is written: the device tier reads the cpu's sections.

**CPU and GPU stay in agreement** because nothing in either engine changes: the CPU relays the
filter to DataFusion (Kleene OR), the device evaluates `NULL_LOGICAL_OR`, and the new cell is the
first thing in the tree that compares the two on a NULL-bearing disjunction.

**Deliberately not touched:** `cpp/src/expr.cpp`, `filter.cpp`, `aggregate.cpp`, any writer, the
q61 line or its cells, the exec-model corpus, `executor_cases.inc`'s fixture (adding a NULL row
there would move every case's expected answer on both engines for one rule).

**Hacks-audit scaffolding:** none exists for #46 — the audit found nothing shaped around it, and a
grep for `46` in code finds only the registry token. Finding 12 (the device comparator hashes
names, not types) is respected, not changed: a wrong *value* is still caught because the digest
includes rendered rows, which is what makes the new cell and a future q61 cell meaningful.

## 4. Alternatives rejected

- Bisect q61 node by node on a device, as the ticket asks: the number is already explained to
  the cent and the mechanism is gone; a bisect would find nothing and cost a device dispatch.
- Add a NULL row to `executor_cases.inc`'s `INPUT`: moves every contract case's expected answer
  on both engines to pin one rule.
- A gtest in `cpp/tests/gpu/test_plan_executor.cpp`: `tpch.minimal` has no NULLs, so the test
  would have to write its own parquet in C++ — more code than a five-line SQL file, and it pins
  the C++ alone rather than the two engines against each other.
- Extend `test_gpu_recipe_walk.rs` with a tpcds query: its `context()` is hardwired to tpch sf1;
  parameterizing it is scope the pin does not need.
- Wait for #187 and let q61's `tp1-single` cell be the pin: right for q61, but a six-join query is
  a poor pin for one filter rule, and it leaves the rule unpinned until #187 lands.
- Keep #46 open "until q61 runs": a ticket must state what the engine does wrong, and this one
  no longer does; the remaining question is q61's cell, which #187's workflow already owns.

## 5. Minimum corpus query

Against `testdata/tpcds.sf1`:

    SELECT p_promo_sk FROM promotion
    WHERE p_channel_dmail = 'Y' OR p_channel_email = 'Y' OR p_channel_tv = 'Y';

Modes: all five plan today (scan → filter with a fused `[0]` projection → unload; one lane and one
batch at each); it is refused nowhere on either backend. Backend: the device is the one that
matters. Correct answer: 158 rows. What the #46 mechanism would show: **155 rows, missing
`p_promo_sk` 15, 71 and 196**. Any other count is a different, new defect.

The sum-shaped variant, for when #187 is out of the way (it exports a `Decimal128` sum and is
refused at the unload today; its `tp4-*`/`tp1-rowgroup` cells also meet #152 on `store_sales`):

    SELECT sum(ss_ext_sales_price) FROM store_sales, promotion
    WHERE ss_promo_sk = p_promo_sk
      AND (p_channel_dmail = 'Y' OR p_channel_email = 'Y' OR p_channel_tv = 'Y');

Correct: 2702543999.35 over 1,448,276 rows; the #46 mechanism: 2650926187.55 over 1,420,745.

The single device run that settles q61 itself: `tpcds/q61` at `tp1-single` once #187 no longer
refuses the export, compared `golden_exact` — `promotions` must read 2894907.87; 2855378.83 would
mean the three-valued OR regressed; anything else is new.

## 6. Cells re-enabled

By Part A: none. q61 gpu × 5 stay off — `tp1-single` behind #187, the other four behind #152 —
and the wall behind them loses one name.

By Part B: five new cpu cells and, after one shad-gpu run, up to five new device cells for
`tpcds/filter-or-nulls`, which would take the device tier from six cells to eleven. The
`tp4-single` cell is the one to watch (see 7).

## 7. Risks and unknowns

- Not verified by reading: q61's actual `promotions` value on today's device. Bounded by the
  argument in 2 — every promotions-only operator arm is read and correct, every shared arm was
  already right for `total` — but it is a run, not a proof. #187's device workflow performs it.
- The corpus data at the time of #46 was generated by an older DuckDB (`build-test.md` notes
  dsdgen type drift between releases); the null placement in `promotion` could in principle have
  differed. The cent-exact match on today's data makes that immaterial to the diagnosis.
- Part B's `tp4-single` device cell: at `Off` batching a small table plans as four lanes with
  three empty (`tpch.sf1/tp4-single.plans.txt`, `cross-join`'s `region`), and an empty lane
  produces no batch. The driver carries that shape on the CPU (the drain injection); whether the
  device unload is happy with a lane that never calls it is what the run shows. If it is not,
  that cell stays off on a new ticket, and the other four still pin the rule.
- Part B's cpu cell is trivially green (DataFusion against DataFusion); its worth is the device
  cell, so Part B without a shad-gpu run adds goldens and no proof.

## 8. Complexity

**S.** Part A: three documentation/registry files, ~20 lines, no code. Part B: one 3-line SQL
file, one `corpus_query!` line, one registry row, and additive golden sections regenerated by the
rust-only cpu tier; one shad-gpu run to enable the device cells. No C ABI, FlatBuffers, wire or
declared-schema change. No existing golden section moves; `recipe-payloads.txt` untouched.
