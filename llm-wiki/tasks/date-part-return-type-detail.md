# date-part-return-type — run record

Chain B, task 4. Branch `ENS-date-part-return-type` off `ENS-aggregate-state-types` at
`121047e3`; PR targets `ENS-aggregate-state-types`. Tasks 1–3 are `done` (PRs #158, #159, #160
green) awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: the `bug_` pin the spec's item 2 says to write first already exists — master's
  chain D added `bug_a_year_extracted_from_a_date_is_exported_as_int16` to
  `tests/gpu_tests/exec_cases.rs`, and task 2 kept it — so the developer flips it rather than
  writes it; `tpch/q7` and `q9` already carry `191` from task 1's rollout. The join-batching
  ticket is #220. The export is told each decimal's precision (task 2) and every count is
  `Int64` (task 3), so a `tp1_single` cell for q7/q8/q9 that fails now fails on something new.
- Routing: the developer works `date-part-return-type-impl.md` — the three C++ cases red, the
  cast, green; the pin flipped; one shad-gpu cycle for the harness and the three rows and, for
  any that passes, its other four modes; item 3's neighbour survey into this file; the record.
- Restriction: `date_part` alone; no Rust-side cast; no other scalar arm; no golden moves.

## Developer notes

### Dispatch 1 — the C++ cases, red

shad-gpu at 0 MiB before each cycle; every `[rmm] pool on a discrete device` line a
reservation (1, 1, 69, 30 GiB), never `could not be built`. Each cycle is `--build` (warm, no
warnings), `--push-binaries --patch`, `--run`; the five C++ binaries run whole each time and
the two rust binaries under `PCK_TEST_FILTER`.

- Cycle 1, the spec's setup: a `CastExprNode(n_nationkey -> Date32)` under the `date_part`
  project. All three red, but one node too low — `[in CudfProject] CUDF failure at
  cast_ops.cu:428: Timestamps cannot be converted to numeric without converting it to a
  duration`. cuDF's `cast` makes no timestamp from a number in either direction, so the arm
  never ran. (The same refusal meets a user's `CAST(int AS DATE)` on the device where the cpu
  answers — the mirror of #218's text case; the cast arm is outside this task's survey, so it
  is noted here and not filed.) The wire has no duration type to route through, so the date
  is made from two `Date32` literals instead: `CASE WHEN n_nationkey < 10 THEN 1995-03-15
  ELSE 2003-11-28 END` in the lower project, which the CASE arm folds with `copy_if_else`
  over the `TIMESTAMP_DAYS` columns `build_scalar`'s `Date32` arm broadcasts. Rows 0 and 24
  then carry different years, months and days.
- Cycle 2, the made date: all three red at the arm — `test_plan_executor.cpp:1096: Failure
  … col.type().id() Which is: 4-byte object <02-00 00-00>` (`INT16`) against `INT32`, for
  `YEAR`, `MONTH` and `DAY`; `peacock_plan_tests` `[  PASSED  ] 41 tests. [  FAILED  ] 3
  tests`. The pin still green under `PCK_TEST_FILTER='extracted'`: `test result: ok. 1
  passed; 0 failed; … 957 filtered out`.

### Dispatch 1 — the cast, green

`expr.cpp`'s `date_part` arm keeps the component, reads `fb_to_type_id(sf->return_type())`,
refuses anything `cudf::is_integral_not_bool` denies (`date_part: return_type <name> is not
an integer type` — `Float64` is what DataFusion declares for `epoch`, a field the arm already
refuses by name, so the guard is reachable by a hand-built plan alone) and casts when the
component's type differs. `git clang-format` applied to the changed lines through `patch`.
The pin became `a_date_part_answers_in_its_declared_type`, `same(Order::AsEmitted)`.

- Cycle 3, `PCK_TEST_FILTER='_cases'`: `peacock_plan_tests` `[  PASSED  ] 44 tests` with
  `ProjectDatePartYearIsInt32`, `…Month…`, `…Day…` each `OK`; `peacock_cpu_tests` 12,
  `peacock_gpu_tests` 6, `peacock_tpch_tests` 4, `peacock_tpchv_tests` 4, all `PASSED`.
  `peacockdb_core_gpu_lib` `test result: ok. 330 passed; 0 failed` — 329 plus the #221 pin
  below; `a_date_part_answers_in_its_declared_type ... ok`. `test_gpu_corpus` `0 passed …
  28 filtered out` (nothing of its matches `_cases`). `==> GPU test run OK`.
- Local, rust-only: `--test test_module_layout` `test result: ok. 17 passed; 0 failed`;
  `--lib` `test result: ok. 564 passed; 0 failed; 2 ignored` (the lib count is unchanged; the
  harness case lives in the gpu rung).

### The neighbours — every other arm of `build_column_scalar_fn`, read against `return_type`

DataFusion 45's `return_type` per function (`datafusion-functions-45.0.0`), the cuDF type
the arm returns, and whether they can differ for an operand the wire can carry. The
validator refuses view types (task 1), so `Utf8View` never reaches an arm.

| arm | DataFusion declares | cuDF answers | verdict |
|---|---|---|---|
| `substr`/`substring` | the operand's string type (`Utf8` → `Utf8`, `LargeUtf8` → `LargeUtf8`) | `STRING` from `slice_strings` | match — `fb_to_type_id` maps both to `STRING` |
| `abs` | the operand's type, decimal included | the operand's type; `unary_operation(ABS)` keeps a fixed_point's scale | match |
| `round` | `Float32` for a `Float32` operand, `Float64` otherwise (its signature is exact on `Float32`/`Float64`, so a decimal or integer operand arrives under a planner cast) | `FLOAT64` always — the arm casts every operand to `FLOAT64` first | **mismatch on a `Float32` operand** — #221, pinned by `bug_a_round_over_float32_answers_float64_on_the_device`. No corpus query: tpcds q2, q54, q78 round decimals, which reach the arm as `FLOAT64` |
| `lower` / `upper` | the operand's string type | `STRING` | match |
| `concat` | `Utf8`, or `LargeUtf8` when any operand is | `STRING` | match |
| `coalesce` | the first non-null operand's type, every operand coerced to it by DataFusion (the casts are wire nodes) | the operands' common type through `copy_if_else`, which refuses two types | match; an operand arm that misreports would surface here as a `copy_if_else` refusal rather than a wrong type, and none does now that `date_part` is right |

Not the survey's question, seen on the way and not filed: `round(x, places)` refuses a
non-literal `places` on the device (`round: decimal places must be a literal`) where the cpu
answers it; `substr` the same for a non-literal start or length. Both are refusals of a
shape no corpus query writes; the coordinator may want tickets.

Outside the Scope table's wording, in a file it lists: `exec_cases.rs` gained the #221 pin
(the ticket has to name the case that reaches it, and a mismatch found is a `bug_` case by
coding-style's rule), and `dates_as_text()` became `input_with(name, type)` so the pin's
`Float32` column and #218's `Utf8` column are one helper rather than two copies.

### Rollout — three rows on the device at tp1_single

Cycle 4, `PCK_RUN_CPP=0 PCK_TEST_FILTER='gpu_tpch_q'`, the three rows enabled at
`gpu_tp1_single`: `test_gpu_corpus` `test result: FAILED. 12 passed; 3 failed` — the twelve
are q1, q6 at every mode and q17, q19 at `tp1_single`, unchanged. Each cell's outcome read
from the golden's node line at the reported line.

| verdict | queries | registry |
|---|---|---|
| the whole device plan runs; golden, `in_rows` at `GpuAggregateBatches` — q7 `in_rows=[[509]]` against `[[4]]`, q8 `[[4]]` against `[[2]]`, q9 `[[1416]]` against `[[175]]`, each at line 11 of its section. Attributed to #185 at first; the completeness reviewer read the joins beneath: the cpu's 8192-row splits feed the init aggregate ~224 times, the device's one batch once, so the merge's `in_rows` follows the join batching and the sections differ at 15/26/14 lines — #220's signature, not #185's | tpch q7 q8 q9 | `gpu_tp1_single` stays disabled; `220` added, `191` struck |
| #152's build-side copy, not re-run | tpch q7 q8 q9 at the other four modes | `152` kept |

No cell passes, so no row goes to its other four modes. `191` is struck from all three rows:
every disabled cell carries `185` or `152`. The differ names the first moved line only; the
sections differ at 15, 26 and 14 lines, and the rest is unread — every one of the three plans
joins five to seven times with the cpu's probe side in 8192-row batches, which is #220's shape,
but the record claims what was read. `corpus_cases.inc`'s batch-6 and batch-7 comments now
state #185 for q9 and for q7 and q8. `git status --short testdata/goldens` is empty.

#191 is closed in `active-tickets.md` in #183's and #187's form; #185 carries a dated line
with the three rows (58 carry it). #221 is filed in `tickets.md`'s Critical correctness
rather than `active-tickets.md`, which the spec named: that file's own header restricts it to
queries a rollout disabled, and #221 disables none — it is the shape of #217–#219, a device
wrong type pinned in `exec_cases.rs` with no corpus query behind it. The counter is 222.
`build-test.md`: Plan-executor 41 → 44, C++ 81 → 84; the harness 334 → 335, the gpu lib rung
389 → 390, gpu 417 → 418, Rust 1649 → 1650, grand total 2099 → 2103; #191 is out of the gpu
corpus paragraph's ticket list.

### Final cycle — the tree as handed over

Cycle 5, `PCK_TEST_FILTER='gpu_'`, every binary: `peacock_cpu_tests` `[  PASSED  ] 12`,
`peacock_gpu_tests` 6, `peacock_plan_tests` 44, `peacock_tpch_tests` 4, `peacock_tpchv_tests`
4; `peacockdb_core_gpu_lib` `test result: ok. 390 passed; 0 failed`; `test_gpu_corpus` `test
result: ok. 27 passed; 0 failed` — 26 cells plus
`the_registry_matches_the_gpu_corpus_in_both_directions`, the regen case filtered out. `==> GPU
test run OK`. Local: `test_cpu_corpus … registry` `1 passed`, `test_module_layout` `17 passed`,
`--lib` `564 passed; 2 ignored`. `git status --short testdata/goldens` empty; `git diff --stat
-- cpp/src` is `expr.cpp` alone, 7 insertions, 1 deletion.

Restriction check: no other scalar arm changed; no Rust production file changed (`git status`
lists `exec_cases.rs` and `corpus_cases.inc` on the Rust side, both test code); the planner's
`date_part` emission untouched.

## Reviewing — 2026-09-17

Dispatch 1 committed as `67eb167a` on `bfc66946`, pushed; PR #161 against
`ENS-aggregate-state-types`, base verified. Deviations on record for the reviewer: the spec's
gtest setup (a `CastExprNode` from `n_nationkey` to `Date32`) cannot work on cuDF, so the
date is made from `Date32` literals through a CASE; #221 went to `tickets.md` rather than
`active-tickets.md`, since it disables no cell; `exec_cases.rs` carries the #221 pin and a
generalised helper beyond the Scope row's wording.

## Completing — 2026-09-17

Review round 1: 0 blocking, 2 important, 3 nits; both importants markdown and taken here —
`architecture.md`'s "Every cast is explicit" counts four casts that stay in C++, `date_part`
joining the count's shape; and the three device refusals the survey noted but did not file are
tickets now, in #218's form: #222 (`round` with a column for `places`), #223 (`substr` with a
column for start or length), #224 (`CAST(int AS DATE)`), counter at 225. Nit taken: the date on
#185's line. Nits deferred, to ride with the next developer dispatch in this code and otherwise
dropped: no gtest reaches the non-integer `return_type` refusal (a `date_part` declared
`Float64` through `date_part_over_a_made_date`, matching `"date_part"` in `e.what()`); the four
wire types `fb_to_type_id` maps to `EMPTY` would meet cuDF's `Invalid type_id.` rather than the
arm's refusal, unreachable from the planner. The reviewer verified `round`'s DataFusion return
type for `Float32` (#221's verdict), the registry rule on q7/q8/q9 (`152` and `185` on each
row, so `191` struck), and that #185 is the right first ticket from the cpu golden's own
`in_rows` at that node.

## Completeness — analyst pass, 2026-09-17

Read as one change against the spec, `architecture.md` and `build-test.md`; the reviewer's list
unseen. 0 blocking, 0 important. What was checked, with the numbers re-derived from the tree:

- Scope: `git diff --stat` is the seven files the table names plus `architecture.md` (the
  coordinator's "Every cast is explicit" correction), `tasks.md` (board) and this file. Nothing
  under `testdata/goldens`, `flatbuffers/`, `cpp/include/` or production Rust moved; `cpp/src` is
  the `date_part` arm alone. The recorded deviations (CASE over `Date32` literals in the gtest,
  #221's pin and `input_with` in `exec_cases.rs`, #221 in `tickets.md`) are test code and
  ticket placement; none widens the change.
- Item 1: the arm reads `sf->return_type()`, refuses by `is_integral_not_bool` naming
  `date_part`, casts only when the component's type differs. Item 2: three gtests assert
  `INT32` and values at rows 0 and 24 (9204 = 1995-03-15, 12384 = 2003-11-28, checked by
  hand); the harness case is `same(Order::AsEmitted)` over `input()` declared `Int32`.
- Item 3: `build_column_scalar_fn` has eight arms — `date_part`, `substr`/`substring`, `abs`,
  `round`, `lower`, `upper`, `concat`, `coalesce` — and the survey table covers all seven
  others. Each verdict re-read against `datafusion-functions-45.0.0`'s `return_type` in the
  cargo registry: `round` is `Float32` for a `Float32` operand and `Float64` otherwise;
  `date_part` is `Float64` for `epoch` and `Int32` for every other field; `abs`, `substr`,
  `lower`, `concat`, `coalesce` answer in the operand's type. No second scalar dispatch exists:
  `build_expr` has no `ScalarFunctionExprNode` arm and `is_ast_able` returns false for one.
- Verification bar: red (cycle 2, `test_plan_executor.cpp:1096`, 41 passed 3 failed) and green
  (cycle 3, 44 passed, each case named) both quoted; the only code commit is `67eb167a`, so
  cycle 5's 390/27 is after the last code change. Rust-only `564 + 2 ignored = 566` matches
  the page's `--lib` figure.
- Registry: no gpu cell enabled on q7/q8/q9, so no other mode run, and the record says so; the
  first differing line named per row and the rest declared unread. Registry rule satisfied:
  every disabled cell carries `152` or `185`. `191` appears on no csv row.
- Counts from the tree: gtests 12+6+44+4+4+4+1+4+4+1 = 84; `gpu_tests/` operator cases 335
  (336 `operator_case!`/`#[test]` hits less the macro definition's own `#[test]` in
  `coverage.rs`); gpu rung 335+10+10+31+4 = 390, block 390+28 = 418; the page's N columns sum
  to 2103 and Rust = 2103−84−369 = 1650. 26 enabled gpu cells in the csv, as the page says;
  58 rows carry `185`, as #185's dated line says.
- Tickets: Contents table 33/16/27/23 matches the headings under each section; counter 225;
  #221–#224 each name a real site (`round`'s literal check, `substr`'s `lit_int`, the cast
  arm of `build_column`) and the refusal text in `expr.cpp`; #221's pin is in
  `gpu_tests/exec_cases.rs`, the device rung. Column-indexing counts unchanged on head
  (22 `->index()`, 49 `.column(`, 8 `column_names[`).
- `architecture.md`: no sentence falsified. "Every cast is explicit" now counts four C++
  casts and lists four; `extract_datetime_component` does answer `INT16` for every
  component. The `CudfProject` row, "What guards it", "cuDF options" and "Types are a plan
  fact" read as prose and stand. Noted, not owed here: #221 is a pre-existing violation of
  "No executor may change a type the plan did not ask it to", found rather than made by this
  branch, and the page cites no ticket beside that rule.
- Dropped as nits: #222 and #224 open with three problem lines against coding-style's two; the
  non-integer refusal has no gtest (reachable from a hand-built plan alone, since the arm
  refuses `epoch` by name first).

## Completeness pass — 2026-09-17

Two blind readings. **Analyst (what is missing): 0 blocking, 0 important** — every arm of the
scalar dispatch in the survey with a verdict matched against DataFusion 45's `return_type`;
red and green both quoted with their runs; counts recomputed; `architecture.md` falsified
nowhere. **Reviewer (what is wrong): 0 blocking, 2 important** — (1) q7, q8 and q9's next
ticket is #220, not #185: the cpu golden's `in_rows=[[509]]` at the merge is the init aggregate
run once per 8192-row join batch, the device's `[[4]]` once per its single batch, so the
difference follows the join batching beneath and the sections differ at 15/26/14 lines where
#185's signature is `in_rows` and nothing else — the coordinator moved the three rows to `220`
in the csv, #185 (55 rows), #220 (30 rows), #191's closing sentence, the batch-6/7 comments and
the rollout table above; no test reads a ticket's number, only its presence. (2) #222 and #224
stated their problem in three lines against the two the style allows — trimmed. Two nits left
dropped: the non-integer `return_type` refusal has no gtest (a hand-built plan is its only
reach), and `fb_to_type_id`'s `EMPTY` mappings would meet cuDF's own error before the arm's.

## Done — 2026-09-17

PR #161 against `ENS-aggregate-state-types`; CI runs 35187056570 and 35188709257 green on every
job. Awaiting the human's merge after tasks 1–3.
