# #60 — review of the proposal

Read at master `188c23ce`. Every cite below was opened. The five-row arithmetic and the walk
query's 25 rows were re-emulated independently in gawk (IEEE double, no `-M`), from the golden
and the two formulas as read from source — not from the proposal's table.

## 1. Verdict

**Needs changes.** The re-diagnosis is right and the fix is bit-identical to the oracle by
construction; what needs correcting is the blast-radius statement (q2 takes the same kernel
seven times per row), the record of the second defect (no `bug_` test), and three small doc
obligations the developer would hit.

## 2. Findings

### F1 — q2 also takes the `p > 0` branch; "only q78" is false — important

§3 Part A: "`round` appears in three corpus queries (`q2`, `q54`, `q78`) and only q78 takes the
`p > 0` branch." `testdata/tpcds-queries/q2.sql:46-52` is seven `round(x, 2)` calls, and the
plan carries them to the device as `round(CAST(sun_sales1@1 / sun_sales2@8 AS Float64), 2)`
(`tp1-single.plans.txt:1664`, seven columns). q2's device cells are off on `56 152`
(`cost-registry.csv:3`), so nothing is red today, but the fix changes q2's device answer as
well, and the June harness already had q2 disabled partly for `round` (`57bed016:test_gpu_executor.rs:313-319`).
Correction: Part A's scope sentence and the #60 rewrite in Part C name q2 as the second
consumer; the ticket body says the walk case is the pin for both, and that q2's `ratio`
argument is a Decimal128 quotient cast to Float64 — a cast whose device/oracle agreement this
fix does not prove (first-hour question for whoever enables q2's device cells; not this ticket).

### F2 — the DESC-NULLS defect gets a ticket but no `bug_` test — important

§7 finds a live wrong-answer defect (`sort.cpp:45-46`, `node_session.cpp:314-315`) and
defers it to its own ticket. coding-style.md "Building around a bug" prescribes three steps
for a bug found while doing something else: a ticket, **a test named `bug_<what it does wrong>`
asserting the wrong behaviour with the ticket number above it**, and no production change. The
proposal does the first and third. The pin site it names (`test_gpu_executors/exec.rs:80-124`)
is exactly where the `bug_` test goes, ~15 lines, and it runs in the same shad-gpu cycle.
Correction: Part B gains `bug_a_desc_key_puts_its_nulls_last` in `exec.rs` — six rows with one
NULL in column 1, `ColumnOrder { ascending: false, nulls_first: true }`, asserting the NULL comes
back **last** — with `// #<new ticket>` above it; it passes today and goes red when the two
sites are fixed. Verified the mechanism it pins: `null_compare` in the 25.02 headers returns
LESS for a null under `BEFORE` (`cudf/table/row_operators.cuh:101-111`), and both the
lexicographic comparator (`experimental/row_operators.cuh:645-647`) and the legacy one
`merge` uses (`row_operators.cuh:437`) flip the whole state for DESCENDING; the single-column
fast path swaps the null flags on `!ascending` (`sort_column_impl.cuh:66-70`); cuDF's own
Python sends `AFTER` for `asc ^ first` (`cudf/core/_internals/sorting.py:103-110`). Arrow's
`nulls_first` is positional (`arrow-ord-54.2.1/src/sort.rs:88-93`, `:434-438`), DataFusion
defaults `DESC` to `nulls_first = true` (`datafusion-sql-45.0.0/src/expr/order_by.rs:107`),
and the CPU relays that unchanged (`cpu_backend/mod.rs:522-527`). Neither device sort pin
carries a NULL (`exec.rs:80-124`, `executor_cases.inc:82-90` is `ascending: true, nulls_first:
false`), so nothing goes red today. The new ticket should also name `architecture.md:1044`
(cuDF options table, `cudf::null_order` row: "per key from the flat buffers's `asc` /
`nulls_first`"), which becomes false the day the mapping turns direction-dependent.

### F3 — `build-test.md`'s test count moves and the proposal does not say so — minor

Part B adds one `#[tokio::test]` to `test_gpu_recipe_walk.rs`. `build-test.md:19` (Recipe walk
row) says `N = 10`, which is the file's count today; the header's grand total (1569, Rust
1135) is the sum of the column. Shared rule: code and llm-wiki move in one commit. Correction:
`10 → 11`, `1569 → 1570`, `1135 → 1136` in Part B. (If F2's `bug_` test lands in `exec.rs`,
the "Executors on a device" row moves `31 → 32` and the totals by one more.)

### F4 — the DataFusion cite is the arm that does not run — minor

§2d cites `round.rs:145-149`, the `ColumnarValue::Scalar` arm. `RoundFunc::invoke_batch` goes
through `make_scalar_function` (`datafusion-functions-45.0.0/src/utils.rs:78-118`), which
expands every scalar argument to an array, so `args.len() == 2` lands in the
`ColumnarValue::Array` arm at `:160-171`. Same formula, `(value * 10.0_f64.powi(p)).round() /
10.0_f64.powi(p)`, so the conclusion is unchanged; the cite should point at the arm that runs.

### F5 — the optional corpus row touches sixteen goldens, not eleven — minor

§5/§8: "additive sections in eleven `tpcds.sf1` goldens". A registry row also gains a section
in each of the five `<mode>.plans.txt` (the meta test requires registry and plan goldens to
agree both ways; `tpch/filter-project` has one at `tp1-single.plans.txt`), so 5 plan + 5 cpu +
5 cost + 1 result = 16. No DuckDB cost file is needed — custom rows carry none
(`testdata/goldens/tpch.sf1/` has no `filter-project` cost oracle). Affects only the optional
follow-up's estimate.

### F6 — a code comment outside C++ names `cudf::round` as the kernel — minor

`scripts/exec_model/operators/expressions.py:237` documents `Round` as "`cudf::round` with
HALF_UP over float64 (`expr.cpp` ~L702)", and build-test.md says the exec-model's scalar
expressions are "pinned to what `expr.cpp` does". After Part A the `p > 0` kernel is
mul/round/div, which is in fact what the Python already computes
(`floor(|v|·scale + 0.5)/scale`, `:252-253`). Correction: one line in that docstring, in the
same commit. Also: `<cstdlib>` is already included (`expr.cpp:31`), so drop the hedge.

## 3. Claims verified

- **The arm**: `expr.cpp:707-722` is `cudf::round(fcol, places, HALF_UP)` for any `places`;
  the code is `57bed016:plan_executor.cpp:784-800` byte for byte (only `3f3e65e3` moved it,
  `965a8df9` touched no line of it). `<cudf/binaryop.hpp>` at `:7`.
- **cuDF's kernel**: `third_party/cudf/cpp/src/round/round.cu:108-117` `half_up_positive` =
  `modf` then `ip + round(fp·n)/n`; `:90-98` `half_up_zero` = `::round`; dispatch on the sign
  of `decimal_places` at `:323-327`; `n = std::pow(10, |p|)` at `:242` (100.0 exact). No
  fast-math flag in `cpp/cmake/Modules/ConfigureCUDA.cmake`; the AST `DIV` functor is a plain
  `lhs / rhs` (`ast/detail/operator_functor.cuh:81-89`). The vendored tree is 25.10a; the
  kernel is the 2020 one, as claimed.
- **DataFusion's kernel**: `(value * 10.0_f64.powi(p)).round() / 10.0_f64.powi(p)`, and the
  CPU backend relays `round` to that UDF (`cpu_backend/expr_physical.rs:110-121`, `:140-155`).
- **The arithmetic**: re-emulated over all 100 golden rows (`mini.result.txt:6448-6547`),
  reading `store_qty`/`other_chan_qty`: exactly 5 rows split, exactly the five named lines
  with exactly the doubles in the table; 43 rows with `ratio ≥ 1`; and every golden `ratio`
  cell parses to the DataFusion-formula double, so the golden is that formula's output. The
  walk query over `n_nationkey ∈ 0..24`: k=1 → `2.7800000000000002` vs `2.78`, k=2 →
  `5.5600000000000005` vs `5.56`, the other 23 equal.
- **Rendering**: `result_text.rs:39-44` → `ArrayFormatter` → `ryu` shortest round-trip
  (`arrow-cast-54.2.1/src/display.rs:434-451`), so one ulp is two different strings; the June
  harness used `pretty_format_batches` (`57bed016:test_gpu_executor.rs:16,55-56`).
- **Plan**: no anti join in q78's five plans; three `Right` + `IS NULL` branches and a `Left`
  at the top (`tp1-single.plans.txt:8413-8453`); the ratio expression at `:8410` is
  `round(CAST(ss_qty AS Float64) / CAST(__common_expr_1 AS Float64), 2)` and is AST-able
  (`is_ast_able`, `expr.cpp:439-447`), so the division is one IEEE op on the device too.
- **Sort prefix unique**: 100 distinct `(item, customer)` pairs over the 100 rows; structural
  (each CTE grouped on the triple, joined on all three).
- **Memory figures**: `tp1-single.plans.txt:8510,8518`; `doc-sentences-final.txt:891`.
- **Registry and corpus**: `cost-registry.csv:79` = `60 97 152`, all five gpu cells disabled;
  `corpus_cases.inc:217` gpu `none`, `golden_exact`; #97 at `archived-tickets.md:427`; the
  disabled-row-must-name-a-ticket rule at `tests/common/registry.rs:281`; `path_for` resolves
  archived numbers (`cost-report/src/main.rs:335-341,449`). Contents row: 17 open, #60 among
  them; next free number 201.
- **Walk anchors**: `PROJECT_OVER_FILTER` at `:628`, the sibling test at `:654`,
  `assert_walk_matches_datafusion(sql, knobs)` at `:582` with `None` tolerance (exact digest,
  `common/mod.rs:151-157`), `Driven::Handled` for `Scan`/`PlainProject` at `:772-778`, `Right`
  refused `:780-781`, `Sort` refused `:788`, `PROVEN` at `:798`, coverage list `:818-828`; the
  walk exports via `peacock_result_from_handle` (`:166-179`), not the unload concat. Float64
  IPC export already proven on a device (`exec.rs:152-188`).
- **Refusal path**: `gpu_backend/join.rs:293-299` `copy_of`; `corpus_gpu.rs:93` panics before
  the per-node compare; `ab58d9bc`'s message says q78 refused on the probe-copy half.
- **tp4 probe shape**: the web branch at tp4-single merges 4 lanes to 1 and emits into 4
  (`tp4-single.plans.txt:15394-15402`), so each probe lane sees four batches — #152 there.
- **Nothing enabled moves**: the six device cells are tpch q6 ×5 and q19 tp1-single, neither
  sorts nor rounds; no C++ test, executor test or walk test touches `round`; no code names #60.
- **Negative `p`**: `__powidf2(10, -2)` = `1/(10·10)` and the loop gives the same 0.01; the
  three ops then match. Not reached by any corpus query.
- **q78 absent from the exec-model corpus**: 71 plans, no `q78` anywhere under
  `scripts/exec_model/`.

## 4. Corrected proposal — only what changes

### 3. Localized fix

**Part A**, scope sentence: "`round` appears in three corpus queries: q54 takes the `p == 0`
kernel, which agrees; q78 (one column) and q2 (seven columns, `tp1-single.plans.txt:1664`) take
the `p > 0` kernel this replaces. q2's device cells are off on #56/#152, so the walk case is
the pin for both." Drop "`<cstdlib>` if not already reachable" — it is (`:31`).

**Part B**, add:

- `peacockdb-core/tests/test_gpu_executors/exec.rs`, beside `a_sort_orders_the_batch_it_was_given`:
  `bug_a_desc_key_puts_its_nulls_last` — the six rows with one NULL in column 1,
  `ColumnOrder { column: 1, ascending: false, nulls_first: true }`, asserting the NULL is the
  last row; `// #<new ticket>: sort.cpp:45 and node_session.cpp:314 send BEFORE whatever the
  direction; cuDF flips it for DESC` above it. Passes today; deleted in the change that fixes
  the two sites.
- `llm-wiki/build-test.md`: Recipe walk row `10 → 11`, Executors on a device row `31 → 32`,
  grand total `1569 → 1571`, Rust `1135 → 1137`.
- `scripts/exec_model/operators/expressions.py:237`: the docstring's first line names the
  three-step form for `p > 0` and `cudf::round` for `p == 0`.

**Part C**, the #60 rewrite names q2 as the second query on the kernel. The new ticket for the
DESC-NULLS defect (next free number, 201) names `architecture.md:1044` as the sentence its fix
must move, and the `bug_` test above as the record until then.

### 5. Minimum corpus query

Unchanged, except the optional custom row's footprint: sixteen goldens (five plan, five cpu,
five cost, one result), no DuckDB cost file.

### 7. Risks and unknowns

Add: q2's `round` argument is `CAST(Decimal128 / Decimal128 AS Float64)`; whether the device's
fixed-point → double cast lands on arrow's double is not proven by this change and is the next
question for q2's device cells, after #56 and #152.

## 5. Complexity

**S**, as proposed. The additions are one `bug_` test (~15 lines), three count edits and one
docstring line, all in the same shad-gpu cycle. The DESC-NULLS fix remains its own S.
