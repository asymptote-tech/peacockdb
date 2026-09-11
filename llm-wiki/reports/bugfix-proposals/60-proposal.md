# #60 — q78 GPU diverges in anti-join + top-N; possibly memory-borderline

Read at master `188c23ce`. Paths relative to `/media/data/peacockdb`. Nothing built or run. Every
number is read off a committed golden, the code at HEAD, the code at the filing commit
(`git show 57bed016:…`), the vendored cuDF, or the cargo registry sources of DataFusion 45 /
arrow 54. One thing is emulated rather than read: IEEE-double arithmetic in `awk` over the
committed result golden (§2d), which is a read of the golden, not a run of project code.

**Verdict up front.** q78's plan has no anti join, the join arms are the filing commit's
byte for byte, the sort is the filing commit's byte for byte, and the sort's prefix is unique
over the answer — so neither "anti-join" nor "top-N" can be where q78 diverges, then or now.
What the filing commit *did* add, in the same diff that filed the ticket, is `cudf::round(x, 2,
HALF_UP)` for q78's `ratio`, and cuDF's float rounding for `places > 0` is a different formula
from DataFusion's: it lands one ulp away on 5 of the 100 rows in q78's committed answer. The
exact comparator renders that as `2.78` against `2.7800000000000002`. That is a live defect in
the device's `round`, it is what the June harness most plausibly saw, and it is the one q78
mechanism that survives to today. Two things reading cannot close are named in §7, and the
smallest queries that settle each on a device are in §5.

## 1. Issue

`llm-wiki/tickets.md:129-133`: "3-CTE anti-join (`LEFT JOIN … IS NULL`) + multi-key DESC LIMIT
100. `round` is proven fine (q54). The isolated diff harness segfaults under GPU memory pressure
on a shared H200 — re-check on a free GPU." Filed by `57bed016` (2026-06-11), whose test file said
in place (`peacockdb-core/tests/test_gpu_executor.rs` at that commit, the `q78` comment): "scalar
fn `round` now executes (q54 confirms round is correct), but q78's result still diverges. The
divergence is NOT the rounding — it is upstream". The harness compared
`pretty_format_batches` text of the sorted rows, GPU against the peacock CPU executor, and the
per-cell diff was never obtained (the segfault the ticket records).

What it disables today, corrected against the code: **nothing on its own.**
`testdata/cost-registry.csv:79` carries `60 97 152` on q78 with every gpu cell `disabled`;
`corpus_cases.inc:217` declares `gpu_modes = none`. The batch-13 commit that enabled q78's cpu
cells says what the device did (`ab58d9bc`, 2026-08-27): "q78 was predicted #152 at every mode
by the batch-count mechanism and refused on the probe-copy half instead, which fires on join
type regardless of batch count." That refusal is `gpu_backend/join.rs:293-299` (`copy_of`) on
the first probe batch of the top `GpuHashJoin{Left}` (`tp1-single.plans.txt:8413`, recipe
`:8465` — `CudfProject{probe keys}, batch copy`), at all five modes. The corpus device test
panics on a refusal before its per-node compare (`tests/common/corpus_gpu.rs:93`), so the run
left no per-node evidence. #97 in the same cell is archived (`00-tickets.md`, "Not included").
#60 is also why q78 is absent from the exec-model corpus (`plans_tpcds.py:3-7`), a Python
hand-lowering outside this proposal.

## 2. Root cause

### 2a. There is no anti join, and the join arms have not moved

DataFusion 45 does not rewrite `LEFT JOIN … WHERE x IS NULL` into an anti join. Each CTE's
"anti-join" reaches the engine as a Left join that `JoinSelection` swapped, so the translator
meets a `GpuHashJoin{Right}` with the returns table as the build side and a `GpuFilter … IS NULL`
above it — `tp1-single.plans.txt:8424-8426` (web), `:8438-8440` (store), `:8451-8453` (catalog).
No `LeftAnti`/`RightAnti` node appears in any of q78's five plans. So `join.cpp`'s anti arm
(`cpp/src/operators/join.cpp:134-151`, `:170-187`, hardcoded `EQUAL`, #80/#59) is not on q78's
path at all. `git log -S'fj.anti_join(left_keys)'` shows that arm untouched since `12ad4999`
(2026-05-25) except for the file split `3f3e65e3`; it is the same code the June run executed,
and the June run did not execute it for q78 either.

The arms q78 does run are the filing commit's. A diff of `57bed016:cpp/src/plan_executor.cpp
:1498-1850` (`execute_hash_join`) against `join.cpp:41-400` shows exactly three substantive
changes: `null_equality` for the equi and semi joins now comes from the plan's
`null_equals_null` (`join.cpp:94-95`, `:284`) instead of a `constexpr UNEQUAL` — the same value
for every q78 join, since none is a set operation; the two `#else` anti arms spell `EQUAL`
explicitly where they took the cuDF default; and the comments. The `Right` arm is
`cudf::left_join(right_keys, left_keys, UNEQUAL)` with the pair swapped back and
`out_of_bounds_policy::NULLIFY` on the left (`join.cpp:295-302`, `:312-320`;
`57bed016:…:1744-1771`) then and now. The filing commit sat directly on `d3c92de5` (Bucket H),
which had already put `UNEQUAL` on the equi joins, `INCLUDE` on groupby and `NULL_LOGICAL_OR`
on both evaluators, so none of the three classes that explained #46 and #47 was still open when
#60 was measured.

### 2b. The top-N cannot decide anything for q78

`cpp/src/operators/sort.cpp:17-64` differs from `57bed016:…:1996-2043` (`execute_sort`) in the
signature and one error string, nothing else. The k-way merge above it
(`node_session.cpp:297-325`) is newer (`eabbb578`, 2026-07-07) and applies the SPM's `fetch`
after `cudf::merge`; its single-input fallback drops the fetch (#118), which is harmless here
because the per-batch `GpuSort` already carries `fetch=100` (`tp1-single.plans.txt:8409`), so at
most 100 rows enter the merge per batch.

More to the point, q78's ORDER BY is decided by its first three keys. `ss` is grouped by
`(d_year, ss_item_sk, ss_customer_sk)`; `ws` and `cs` are grouped by the same triple and
joined on all three, so each ss row matches at most one row of each, and every output row is
unique on `(ss_sold_year, ss_item_sk, ss_customer_sk)` — the committed answer has 0 duplicate
`(item, customer)` pairs over its 100 rows (`mini.result.txt:6448-6547`). The NULL-customer
groups cannot appear: `UNEQUAL` matches them to nothing, both `coalesce` are 0, the filter
drops them. So keys 4-10 (`ss_qty DESC nulls_first`, the decimals, `ratio`) never break a tie,
and a defect in a DESC key, in NULL ordering, or in Decimal/Float64 comparison cannot move a row
into or out of q78's top 100. (One such defect exists in this path for other queries — §7.)

### 2c. Memory is not borderline in today's engine

The plan model prices q78 at `accumulators=1119178460` under `budget=2147483648` at tp1-single
(`tp1-single.plans.txt:8510`), the largest single figure the top Left join's 465 MB
(`:8518`). The measured CPU peak was the corpus maximum at about 135.5 MB
(`llm-wiki/tasks/module-layout-baselines/doc-sentences-final.txt:891`). The corpus tier passes
no budget (#192), and the T19 device run got through every accumulator in q78's build subtree
and refused on a *copy*, not an allocation. The June segfault was the whole-table executor
materializing every node of three fact-table joins on a shared H200 — today that is
[#178](../../llm-wiki/tickets.md#t178)'s rule, a neighbour rather than a bug.

### 2d. The mechanism that fits: `round(x, 2)` is a different formula on the device

`ratio` reaches both engines as `round(CAST(ss_qty AS Float64) / CAST(__common_expr_1 AS
Float64), 2)` (`tp1-single.plans.txt:8410`; DataFusion was 45 at the filing commit too,
`57bed016:peacockdb-core/Cargo.toml:14`, so the June expression was this one). The division is
one IEEE operation over exact integers on both sides. The rounding is not the same operation:

- DataFusion: `(value * 10f64.powi(p)).round() / 10f64.powi(p)` — one multiply, one
  half-away-from-zero round, one divide
  (`~/.cargo/registry/src/*/datafusion-functions-45.0.0/src/math/round.rs:145-149`).
- cuDF, `rounding_method::HALF_UP`, floating, `p > 0`: `integer_part + round(fractional_part
  * n) / n` with `modf` first (`third_party/cudf/cpp/src/round/round.cu:108-117`,
  `half_up_positive`, unchanged since cuDF #6562 in 2020, so the same in 25.02 and 26.02).
  For `p == 0` it is `half_up_zero` = plain `round(e)` (`:90-98`), identical to
  `f64::round`.

`cpp/src/expr.cpp:703-722` calls `cudf::round(fcol, places, HALF_UP)` for any `places`. That
arm is what `57bed016` added (`57bed016:cpp/src/plan_executor.cpp:779-800`); `965a8df9` later
touched only its comment. So the call the ticket's own commit introduced is the one that
differs from the oracle, and "round is proven fine (q54)" proved the `p == 0` kernel only —
q54 is `round(revenue/50)` (`testdata/tpcds-queries/q54.sql:47`), where the two formulas
coincide.

Two roundings against one: cuDF's `ip + fl(k/100)` can land on the neighbour of DataFusion's
correctly rounded `(100·ip + k)/100` whenever the exact sum sits at or past a midpoint of the
result's grid — for `2.78`, `fl(0.78) = 0.78 + 0.24·2⁻⁵³` puts the sum exactly on the midpoint
and ties-to-even picks the upper double. Emulated in IEEE double over the committed answer
(`awk`, reading `store_qty`/`other_chan_qty` from `mini.result.txt:6448-6547`), **5 of the 100
rows split**, all with `ratio ≥ 1` (43 such rows):

| golden line | (store_qty, other_chan_qty) | DataFusion, the golden | cuDF's kernel |
|---|---|---|---|
| `:6470` | (25, 9) | `2.78` = 2.7799999999999998 | 2.7800000000000002 |
| `:6483` | (76, 56) | `1.36` = 1.3600000000000001 | 1.3599999999999999 |
| `:6517` | (38, 15) | `2.53` = 2.5299999999999998 | 2.5300000000000002 |
| `:6523` | (18, 11) | `1.64` = 1.6399999999999999 | 1.6400000000000001 |
| `:6532` | (42, 22) | `1.91` = 1.9099999999999999 | 1.9100000000000001 |

Rows with `ratio < 1` cannot split (`ip = 0`, one division on both sides). Both comparators
render a Float64 through arrow's `ArrayFormatter` with Rust's shortest round-trip `Display` —
the June harness via `pretty_format_batches`, today's device tier via
`tests/common/result_text.rs:39-44` under `golden_exact` (`corpus_cases.inc:217`) — so
`2.7799999999999998` prints `2.78` and `2.7800000000000002` prints `2.7800000000000002`, and
the row digests differ. Five rows one ulp apart in one column is exactly "the result still
diverges" from a harness that never showed which cells.

**Can the mechanism still exist? Yes, and only this one.** It is deterministic, data-shaped
(about one value in twenty at or above 1.0), unchanged since the filing commit, and it will
redden q78's device cell at every mode the moment #152 stops refusing the Left join — before
which #187 refuses the same cell at the unload, since the sink carries `Decimal128(17,2)` and
`(23,2)` columns (`tp1-single.plans.txt:8407`), the shape q61 and q77 were refused on.

**What reading cannot close**: that the June diff was *only* these cells (the June data came
from an unpinned DuckDB, but dsdgen row content is stable and the split rate does not depend on
which rows are in the top 100); and that cuDF's compiled kernels do plain IEEE arithmetic
(no fast-math contraction — the formula has a divide before its add, so there is no multiply-add
to fuse, but this is read from the source rather than measured). A single run settles both: §5.

## 3. Localized fix

Three parts. Part A is the engine change; B pins it on a device; C is the ticket and registry.
No CPU code moves: the CPU relays `round` to DataFusion, which is the oracle.

### Part A — `cpp/src/expr.cpp:703-722`, the `round` arm of `build_column_scalar_fn`

Keep the argument parsing and the cast to FLOAT64 (`:707-721`). Replace the single
`cudf::round(fcol->view(), places, HALF_UP)` at `:722` with:

```cpp
    if (places == 0)
      return cudf::round(fcol->view(), 0, cudf::rounding_method::HALF_UP);
    // DataFusion rounds as (x * 10^p).round() / 10^p. cudf::round's float kernel for
    // p > 0 is modf-based and lands one ulp off that on about one value in twenty at
    // or above 1.0 (#60), so the three steps are spelled out; the middle one is the
    // p == 0 kernel, which is round().
    double factor = 1.0;
    for (int i = 0; i < std::abs(places); ++i) factor *= 10.0;
    if (places < 0) factor = 1.0 / factor;
    cudf::numeric_scalar<double> f(factor);
    auto f64 = cudf::data_type{cudf::type_id::FLOAT64};
    auto scaled = cudf::binary_operation(fcol->view(), f, cudf::binary_operator::MUL, f64);
    auto rounded = cudf::round(scaled->view(), 0, cudf::rounding_method::HALF_UP);
    return cudf::binary_operation(rounded->view(), f, cudf::binary_operator::DIV, f64);
```

Why this is bit-identical to the oracle: `10f64.powi(p)` with a runtime `p` is libgcc's
`__powidf2`, repeated squaring — exact `10^p` for `p ≤ 22`, and `1/10^|p|` correctly rounded for
negative `p` — which the loop reproduces; the multiply and divide by a FLOAT64 scalar are
cuDF's compiled arithmetic path, one IEEE operation each; `cudf::round(·, 0, HALF_UP)` on
FLOAT64 is `generic_round` = CUDA `round()` = half away from zero = `f64::round`. Three kernel
launches instead of one, on a column the plan already priced; `round` appears in three corpus
queries (`q2`, `q54`, `q78`) and only q78 takes the `p > 0` branch. Update the comment at
`:703-706` to say the `p == 0` kernel is the one that agrees (four lines at most). `<cstdlib>`
for `std::abs` if not already reachable; `<cudf/binaryop.hpp>` is already included for
`build_column_binary`.

`git clang-format` on the changed lines only (coding-style.md).

### Part B — the device pin, `peacockdb-core/tests/test_gpu_recipe_walk.rs`

The walk is the right instrument: it drives the writer's recipe through the real C++ project
(`CudfProject` → `build_column` → this arm), exports without the unload's sink-schema concat, and
compares against DataFusion on the same SQL with the exact digest (`assert_results_match(…,
None, …)`, `tests/common/mod.rs:151-157`). `Scan` and `PlainProject` are already in `PROVEN`
(`:798`) and `Driven::Handled` (`:772-778`), so no arm and no claim moves.

- After `PROJECT_OVER_FILTER` (`:628`):

  ```rust
  const ROUND_PLACES: &str =
      "SELECT n_nationkey, round((CAST(n_nationkey AS DOUBLE) * 25.0) / 9.0, 2) AS r FROM nation";
  ```

  Parenthesized so the simplifier cannot fold `25.0 / 9.0`. Integer key and Float64 result:
  nothing for #183 or #187 to refuse under a driver either; one lane, one batch, no join.

- Beside `a_project_over_a_filter_evaluates_on_what_the_filter_left` (`:654`):

  ```rust
  /// The oracle rounds as (x·100).round()/100; cudf::round's float kernel is modf-based and
  /// one ulp off that on nationkey 1 (2.78) and 2 (5.56), which the exact compare renders
  /// as 2.7800000000000002. q78's `ratio` is the same call, and the shape #60 was filed on.
  #[tokio::test]
  async fn round_with_places_lands_on_the_oracles_double() {
      assert_walk_matches_datafusion(ROUND_PLACES, ONE_LANE).await;
  }
  ```

  Red before Part A on `k=1` (2.7777… → 2.7800000000000002 against 2.78) and `k=2` (5.5555… →
  5.5600000000000005 against 5.56), both from the same emulation as §2d; green after.

- The coverage list at `:818-828` gains `(ROUND_PLACES, ONE_LANE)`, so
  `the_kinds_a_device_has_run_are_the_kinds_this_file_claims` reads the set the tests run.

### Part C — ticket and registry

- `llm-wiki/tickets.md:129-133`: rewrite #60's body under the same anchor and number (the
  number is permanent; the mechanism was wrong). Fifteen lines at most; the two problem lines
  first: the device's `round(x, p > 0)` is one ulp off DataFusion's on about one value in
  twenty at or above 1.0, because cuDF's float kernel is `ip + round(fp·10^p)/10^p` where
  DataFusion computes `(x·10^p).round()/10^p`; q78's `ratio` has five such rows in its top
  100, which is the divergence recorded in June as "anti-join + top-N" — the plan has no anti
  join and the sort's prefix is unique. Then: `p = 0` is the plain `round(e)` kernel and
  agrees, which is what q54 proved. When Part A lands it moves to `archived-tickets.md` as
  Done with the walk case named; the Contents row count moves 17 → 16 then.
- `testdata/cost-registry.csv:79`: `60 97 152` → `152` once the ticket archives (`97` is
  archived already). `registry.rs` requires only that a disabled row name some ticket, and
  `TicketIndex::path_for` resolves archived numbers, so the widget stays green in either
  order. Adding `187` there is a prediction from the sink schema rather than an observed
  refusal; the helper's call, and it should be recorded as predicted if recorded at all.

### What this deliberately does not touch

`join.cpp`, `sort.cpp`, `node_session.cpp`, the wire, the plan, the driver, the goldens — none
carries #60 and none has moved in the way the ticket supposed. q78's `corpus_query!` line and its
comment (`corpus_cases.inc:209-217`) are #152's. The `p < 0` branch is written to the same rule
rather than refused, because refusing it would be a new behaviour for a case DataFusion answers;
no corpus query reaches it. The exec-model corpus (`plans_tpcds.py`) gains eligibility for q78
once the legacy ticket is gone, a Python lowering that is not this fix.

### How CPU and GPU stay in agreement

The CPU backend relays `round` to DataFusion and is the oracle; the device evaluates the same
wire expression through `expr.cpp`. After Part A the two compute the same three IEEE
operations on the same doubles, so they agree by construction rather than by tolerance, which is
the rule `architecture.md` states for every cast and finalize. The walk case is the first place
in the tree that compares the two on a `round` with places.

### hacks-audit

Nothing grew around #60: no code references it, and the audit's two readings found nothing
shaped by it. Respected, not changed: finding 12 (the device comparator hashes names and rendered
rows, not types — a one-ulp Float64 difference is still caught because the rendering differs,
which is what makes both the q78 cell and the walk case meaningful) and finding 10 (the walk
resolves a copy to the original handle; `ROUND_PLACES` has no join, so it does not lean on it).
Not the alternative of loosening the comparator, which would be exactly the scaffolding the
audit exists to find.

## 4. Alternatives rejected

- **Bisect q78 node by node on a device, as the ticket asks.** It cannot run past its last
  join (#152), and the arms the ticket suspects are the filing commit's — a bisect would
  reach the `ratio` project and find §2d.
- **Switch q78's `gpu_oracle` to `golden_approx`.** Hides a real engine disagreement behind a
  tolerance; the two engines are one engine, and this one is fixable to the bit.
- **Make the CPU match cuDF's formula.** The CPU is DataFusion; the oracle does not move to
  meet the executor.
- **`cudf::round` on the Decimal128 before the cast**, avoiding floats. The plan's expression
  is a Float64 division; the device may not change a type the plan did not ask for.
- **A gtest in `cpp/tests/gpu/test_plan_executor.cpp`.** Pins the C++ against a hand-built
  buffer and a hand-computed double; the walk pins it against the writer's buffer and
  DataFusion's double, which is the claim.
- **A custom corpus query for the pin instead of the walk.** Sound (Int64 + Float64, one
  batch, no join, no #183/#187/#152) and it would put five cpu cells and additive sections in
  eleven goldens behind it; the walk proves the same call with no golden moving. Worth doing
  later if the device tier wants a `round` cell; not needed to close the ticket.
- **Close #60 as stale on the join-arm history alone**, the way #46 and #47 closed. The arms
  are unchanged, but the ticket's divergence is real and still there; a ticket retired on an
  argument is the shape this one already had once.

## 5. Minimum corpus query

**The mechanism (§2d), tpch sf1 schema — the walk's `ROUND_PLACES`:**

```sql
SELECT n_nationkey, round((CAST(n_nationkey AS DOUBLE) * 25.0) / 9.0, 2) AS r FROM nation
```

Plans today at every mode (scan → project → unload; `nation` is one row group under
`SMALL_TABLE_BYTES`, so one lane and one batch everywhere); refused nowhere on either backend.
Backend: the device is the one that matters. Correct answer: 25 rows, `r` = DataFusion's
doubles, `2.78` and `5.56` among them. What today's device shows: `2.7800000000000002` at
`n_nationkey = 1` and `5.5600000000000005` at `2`, the other 23 rows equal. Any other row
differing is a new defect. On the device it runs as the walk at `ONE_LANE`, the tp1-single
shape, without waiting on any ticket.

**The shape the ticket named — "anti-join + top-N" — isolated, tpcds sf1 schema:**

```sql
SELECT ws_item_sk, ws_order_number
FROM web_sales LEFT JOIN web_returns
  ON wr_order_number = ws_order_number AND wr_item_sk = ws_item_sk
WHERE wr_order_number IS NULL
ORDER BY ws_item_sk DESC, ws_order_number DESC
LIMIT 100
```

This is one of q78's CTEs with the aggregate and the `date_dim` join removed and a top-N put
directly on it. It plans as q78's branch does — `GpuHashJoin{Right}` with `web_returns` as the
coalesced build side, `GpuFilter … IS NULL`, `GpuSort(fetch=100)`,
`GpuAccumulateBatchesAndSort(fetch=100)` — and its answer is total, since `(ws_item_sk,
ws_order_number)` is `web_sales`' key. Int64 only, so neither #183 nor #187 reaches the unload;
no `GpuAggregateBatches`, so not #185. At **tp1-single** `web_sales` is one batch
(`partition_groups=[[[0,1,2,3,4,5]]]`, as at `tp1-single.plans.txt:8429`), the Right join is
probe-local, and `build_copy` hands its only probe batch the original build handle — so it runs
on a device today with nothing in the way, and it is the first `Right` on a device under a
test (the walk refuses `Right`, `test_gpu_recipe_walk.rs:780-781`; 47-proposal §7). At
tp1-rowgroup the probe is six batches and at the tp4 modes each lane's probe is four scattered
batches (`tp4-single.plans.txt`, the web branch), so those four cells are #152.

What a single device run of it must show: 100 rows equal to DataFusion's under `golden_exact`,
and the cpu section's `in_rows`/`batch_rows` matched at every node. That is the ticket's stated
mechanism given the one run it never had; a mismatch there would be a new defect in the `Right`
arm or the `IS NULL` filter, not #60's rounding. Worth adding as a custom corpus row
(`testdata/tpcds-queries/anti-join-topn.sql`, cpu ×5, gpu tp1-single, the other four on #152) in
the same change or the next.

**q78 itself**, once #152 and then #187 stop refusing it: `gpu_tp1_single` under `golden_exact`
must differ from `mini.result.txt:6443-6547` in exactly the `ratio` cell of the five rows in §2d
before Part A and in nothing after it. Anything else — a row count other than 100, another
column, another row — is not this ticket.

## 6. Cells re-enabled

By Parts A-C: none. q78's five device cells stay off — every mode on #152 at the top Left
join's probe copy, and behind that #187 at the unload for the two decimal columns. What comes
back is the wall losing a name that was wrong, a device-proven `round`, and q78 becoming eligible
for the exec-model corpus. The optional `tpcds/anti-join-topn` row would add five cpu cells and
one device cell — the device tier's seventh — and the first `Right` join and the first
`ORDER BY … DESC LIMIT` on a device under a test.

## 7. Risks and unknowns

- **Not run.** The five-row split is an IEEE-double emulation of two formulas read from source,
  not a device measurement. What would make it wrong: cuDF's compiled binary-op or round kernels
  doing something other than plain IEEE double arithmetic (a fast-math build, a fused
  multiply-add where the formula has none), or CUDA `round()` not being half away from zero.
  Both are contrary to what the sources say. The walk case is the measurement, and its first
  shad-gpu run is what closes the ticket; if it is green *before* Part A, §2d is wrong and the
  ticket goes back to unexplained with §5's second query as the next step.
- **The June diff is unrecoverable.** Whether *only* `ratio` differed cannot be reconstructed;
  the arguments in §2a-c say nothing else could, on that executor, for this query.
- **Found while reading, not q78's, wants its own ticket: a `DESC` key's NULLs sort last on the
  device and first on the CPU.** `sort.cpp:45-46` and `node_session.cpp:314-315` map
  `nulls_first → null_order::BEFORE` regardless of direction. In libcudf `BEFORE` means "null
  compares less", and the lexicographic comparator then flips the whole comparison for a
  DESCENDING key (`row_operators.cuh:618-647` in the 25.02 headers under
  `/media/data/miniforge3/pkgs/libcudf-25.02.02-*/include/cudf/table/experimental/`; the
  single-column fast path `third_party/cudf/cpp/src/sort/sort_column_impl.cuh:62-74` swaps the
  null flags on `!ascending`; cuDF's own Python compensates with `asc ^ (null == "first")` in
  `…/site-packages/cudf/core/_internals/sorting.py:103-110`). Arrow's `nulls_first` is an output
  position independent of direction (`arrow-ord-54.2.1/src/sort.rs:88-93`, and `:434-438` says
  so), and DataFusion defaults `ORDER BY x DESC` to `nulls_first = true`
  (`datafusion-sql-45.0.0/src/expr/order_by.rs:107`). The CPU relays that
  (`cpu_backend/mod.rs:525`); the device inverts it. Inert for q78 (§2b), a silent wrong
  answer for any `ORDER BY <nullable> DESC [LIMIT n]` with NULLs in the column, at every mode.
  Fix: `BEFORE` when `nulls_first == asc`, `AFTER` otherwise, at both sites, with the reason on
  one of them. Pin: `test_gpu_executors/exec.rs:80-124` builds its own six rows — add a NULL and
  a `nulls_first: true, ascending: false` case; and in SQL, tpch sf1 (no NULLs in the data, so a
  CASE makes one): `SELECT CASE WHEN n_nationkey < 3 THEN NULL ELSE n_nationkey END AS k FROM
  nation ORDER BY k DESC LIMIT 5` — DataFusion: `NULL, NULL, NULL, 24, 23`; the device today:
  `24, 23, 22, 21, 20`. The walk refuses `Sort` (`:788`), so this one is an executor test or a
  corpus row, not a walk case. No existing device cell sorts (q6 ×5, q19 tp1-single), which is
  why nothing has gone red.
- **The T19 run's build subtree is unverified per node.** q78's three Right-join-with-IS-NULL
  branches and their aggregates ran on a device once and were never compared, because the
  refusal dropped the report. §5's second query is the smallest thing that compares them.
- **#187 is the next wall for q78** after #152, read from the sink schema rather than observed.
- **Row 79's `97`** is an archived ticket sitting in a live cell; it leaves with `60`.

## 8. Complexity

**S.** Part A: one arm of one C++ file, about twelve lines replacing one, no new include beyond
`<cstdlib>`; Part B: one const, one test, one list entry in one Rust test file, about fifteen
lines; Part C: a ticket body rewritten and one registry cell. No C ABI, FlatBuffers, wire-format
or declared-schema change; no CPU code; no golden regenerated (the walk writes none, no plan
moves). One shad-gpu cycle: red on the walk case before Part A, green after. The optional corpus
row in §5 is a separate S — one SQL file, one `corpus_query!` line, one registry row, additive
sections in eleven `tpcds.sf1` goldens from the rust-only cpu tier, and its device cell enabled
from the same shad-gpu run. The DESC-NULLS defect in §7 is its own ticket and its own S.
