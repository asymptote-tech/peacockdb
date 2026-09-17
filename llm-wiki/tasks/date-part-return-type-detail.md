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
| the whole device plan runs; golden, `in_rows` at `GpuAggregateBatches` — the node's own output where the cpu records what it consumed (#185): q7 `in_rows=[[509]]` against `[[4]]`, q8 `[[4]]` against `[[2]]`, q9 `[[1416]]` against `[[175]]`, each at line 11 of its section | tpch q7 q8 q9 | `gpu_tp1_single` stays disabled; `185` added, `191` struck |
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
