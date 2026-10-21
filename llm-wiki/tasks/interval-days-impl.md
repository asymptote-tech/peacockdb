# interval-days implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An interval literal crosses the wire as months, days and nanoseconds. The device answers
`date or timestamp ± interval literal` as DataFusion does. tpch mixed-join's device cells turn on,
and new pbench queries prove the rest (#168).

**Architecture:**
- **Wire:** `DataType::IntervalMonthDayNano`, appended to `gpu_plan.fbs`, and three `ScalarValue`
  fields. `serialize_scalar_value` writes all three DataFusion interval literal types into them.
- **Device:** `build_column_binary` (`cpp/src/expr.cpp`) gains one arm, `add_interval`. It applies
  months, then days, then nanoseconds to a `TIMESTAMP_*` column: months with
  `cudf::datetime::add_calendrical_months`, days and nanoseconds as `cudf::duration_scalar`s.
- **Routing:** the interval maps to `EMPTY` in `fb_to_type_id`, so it never takes the AST path.
- **Corpus:** six pbench queries. Their DuckDB lines declare oracle-cleanup's `date(…)` where
  DuckDB answers a timestamp. A `timestamp(…)` facet is added only if the measurement in Task 4
  shows the need.

**Tech stack:** FlatBuffers (Rust `flatc_fork`, C++ CMake), Rust wire and planner tests,
C++20/cuDF 25.02 (gtest), the corpus, DuckDB 1.5.4, nebius-gpu.

**Spec:** [`interval-days.md`](interval-days.md). Task 13 of chain K, after oracle-cleanup, whose
grammar (`duckdb(…, date("…"))`) and whose `chunk_bytes` default it builds on.

## Global Constraints

- **DataFusion's order and rules, for both `+` and `−`:**
  - months, then days, then nanoseconds;
  - a month past the month's end clamps to its last day;
  - on a date, the nanosecond part counts in whole days, truncated toward zero;
  - on a timestamp, the nanosecond part converts to the column's unit as arrow converts it;
  - `−` negates all three parts;
  - `interval + date` answers a date.
- **Expected values come from DataFusion,** never from cuDF's or DuckDB's answer. A gtest's
  expected value is what DataFusion 45 answers for the same SQL, recorded in the test's comment.
- **Out of scope:** interval columns, interval-by-number arithmetic, interval comparisons and
  zoned timestamps. They stay refused under #249 (wire nested/time types), by name.
- **No cpu golden moves** except mixed-join's plan line and the new queries' sections.
- **GPU:** nebius-gpu, `~/peacockdb-K`, through `value-case-impl.md`'s *Device cycle*, with
  `FIX=cpp/src/expr.cpp`.

## Review Focus

1. **A NULL interval literal** (`f_dt + CAST(NULL AS INTERVAL)`) answers an all-null column of
   the timestamp's type, not a refusal and not an unchanged column. Pinned by
   `IntervalArithmetic.ANullIntervalAnswersNulls`.
2. **A zero-row input,** which a hoisted keyless residual uses to learn the type. The arm answers
   a zero-row column of the right type. Pinned by `IntervalArithmetic.ZeroRowsKeepTheType`.
3. **A NULL date row** stays NULL through all three steps. `add_calendrical_months` keeps nulls;
   check the duration adds do. Pinned in `IntervalArithmetic.MonthsClampAtMonthEnd`, whose column
   holds a NULL.
4. **A negative sub-day part on a date, and a sub-unit part on a timestamp:** truncation toward
   zero on a date, flooring on a timestamp. These are the two places where cuDF's integer
   division would silently choose. Pinned by `IntervalArithmetic.SubDayOnADateTruncates` and
   `IntervalArithmetic.SubUnitOnATimestampFloors`.
5. **`interval + date` with the interval on the left.** The answer's type is the timestamp's,
   never the left operand's. Pinned by `IntervalArithmetic.IntervalOnTheLeft`.

---

### Task 1: The interval literal crosses the wire

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs`: `enum DataType` gains `IntervalMonthDayNano`, appended
  last, after `TimestampNanosecond`. `table ScalarValue` gains `interval_months: int32;`,
  `interval_days: int32;` and `interval_nanos: int64;`, appended after `decimal_scale`, so no
  existing field's slot moves and no payload digest churns.
- Modify: `peacockdb-core/src/wire/serialize.rs` (`serialize_scalar_value` only;
  `convert_data_type` is untouched)
- Modify: `peacockdb-core/src/wire/expr_writer.rs` (the refusal's ticket, ~35; the Binary arm's
  refusal for zoned timestamps and `Date64`)
- Modify: `peacockdb-core/src/wire/fb_text.rs` (`scalar_text` prints an interval literal as its
  three parts, not `0`)
- Test: `peacockdb-core/src/wire/tests.rs`; `wire/serialize/tests.rs`'s
  `bug_a_schema_holding_an_interval_is_refused_at_plan_time` stays, and stays green
- Modify (stale #168 text): `wire/attach.rs:24`, `wire/mod.rs:364`, `planner/tests/plan_goldens.rs`
  (~413, ~461, ~614), `wire/tests.rs:755`

**Interfaces:**
- Produces, Rust: `ScalarValue::IntervalMonthDayNano(Some(v))` → `(v.months, v.days, v.nanoseconds)`;
  `IntervalYearMonth(Some(m))` → `(m, 0, 0)`; `IntervalDayTime(Some(v))` →
  `(0, v.days, v.milliseconds as i64 * 1_000_000)`. A NULL of any of the three is
  `type = IntervalMonthDayNano, is_null = true`, written by explicit arms placed ahead of the
  generic null arm. `convert_data_type` does not learn `Interval`: it also serves schemas and
  cast targets, and an interval *column* must stay refused (#249).

- [ ] **Step 1: Write the failing wire tests:**
  - `each_interval_literal_type_crosses_as_three_parts`: the three types above, read back through
    the generated accessors.
  - `a_null_interval_crosses_as_a_typed_null`.
  - `a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where` takes a payload the wire
    still cannot carry, a `ScalarValue::Time64Nanosecond(Some(1))` literal in the filter. It
    asserts `scalar value`, `(#249)` and ` at #`.
  - `an_interval_on_a_zoned_timestamp_or_a_date64_is_refused`: `ts_utc + INTERVAL '1' DAY` over
    `Timestamp(Microsecond, Some("+01:00"))`, and the same over a `Date64` column. Each is
    refused at plan time naming `(#249)`. Arrow does months and days in the column's zone, and
    the device has no zone. On the device a `Date64` is a millisecond timestamp, which would
    floor a sub-day part where DataFusion drops it.
  - `a_schema_holding_an_interval_is_still_refused`: `bug_a_schema_holding_an_interval_is_refused_at_plan_time`
    keeps passing unchanged.
- [ ] **Step 2: Run them to see them fail.**
  `cargo test --features rust-only -p peacockdb-core --lib -- wire::tests`
- [ ] **Step 3: Implement.**
  - The fbs fields.
  - The three value arms and the three NULL arms in `serialize_scalar_value`.
  - In `expr_writer.rs`, the literal refusal names `#249`, and its comment says what the wire
    still lacks: Time, Duration, Struct and List literals.
  - The Binary arm refuses an interval literal operand whose other side is a zoned timestamp or
    a `Date64`, naming `(#249)`.
  - `fb_text`'s `scalar_text` prints `interval(<months>, <days>, <nanos>)`.
  - Every stale #168 sentence in the files listed says what is true now.
- [ ] **Step 4: Run** `--lib` (rust-only). Expected: PASS, with these exceptions:
  - mixed-join's plan golden sections still say `not runnable` until Task 3;
  - `every_query_that_cannot_cross_the_wire_is_declared_and_every_declaration_is_true` fails,
    because mixed-join now crosses.

  Note both in the detail file. Task 3 settles them. Do not commit a red tier: Tasks 1–3 are
  committed together if Task 3's goldens are not ready, otherwise each on its own.
- [ ] **Step 5: Commit** (with Task 3 if needed) `wire: an interval literal crosses as months, days and nanoseconds`

### Task 2: The device adds an interval

**Files:**
- Modify: `cpp/src/expr.cpp`: `fb_to_type_id` (the new type → `EMPTY`), `build_column_binary`
  (the arm), and a new `add_interval`.
- Modify: `cpp/src/peacock/expr.h` only if a helper must be visible to the host-only tests.
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (a new `IntervalArithmetic` suite; a new row in
  `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`).
- Test helper: `make_interval_literal(fbb, int32_t months, int32_t days, int64_t nanos)` and
  `make_null_interval_literal(fbb)`, beside `make_date32_literal`.

**Interfaces:**
- Produces: `static std::unique_ptr<cudf::column> add_interval(cudf::column_view ts, const
  fb::ScalarValue* interval, bool negate)` in `expr.cpp`.
- Consumes: `cudf::datetime::add_calendrical_months(column_view, scalar const& months)`, where
  `months` is an `INT32` `numeric_scalar`; and `cudf::binary_operation(column_view,
  duration_scalar<…>, ADD, ts.type())`.

- [ ] **Step 1: Write the failing gtests**, each over a hand-built column. Every expected value
  is DataFusion's, in a comment:
  - `IntervalArithmetic.MonthsClampAtMonthEnd`: `TIMESTAMP_DAYS` [2024-01-31, 2023-01-31,
    2024-03-31, NULL].
    - + 1 month → [2024-02-29, 2023-02-28, 2024-04-30, NULL].
    - − 1 month → [2023-12-31, 2022-12-31, 2024-02-29, NULL].
  - `IntervalArithmetic.MixedOnEachTimestampUnit`: one timestamp,
    `2024-01-30 23:00:00.000000123`, in each of the four units (each truncated to its unit), plus
    `1 month 2 days 3 hours`. Months first gives 2024-03-03 02:00; days first or nanoseconds
    first would give 2024-03-02 02:00, so this case pins the order.
  - `IntervalArithmetic.SubDayOnADateTruncates`: 2024-03-10.
    - + 36 hours → 2024-03-11;
    - + 12 hours → 2024-03-10;
    - − 12 hours → 2024-03-10;
    - − 36 hours → 2024-03-09.
  - `IntervalArithmetic.SubUnitOnATimestampFloors`: a `TIMESTAMP_MILLISECONDS` value plus and
    minus 1500 ns, at DataFusion's answers.
  - `IntervalArithmetic.IntervalOnTheLeft`: `INTERVAL '1' DAY + date` is `TIMESTAMP_DAYS`, one
    day later.
  - `IntervalArithmetic.ANullIntervalAnswersNulls`: every row is NULL, with the column's type.
  - `IntervalArithmetic.ZeroRowsKeepTheType`: zero rows in, zero rows of `TIMESTAMP_DAYS` out.
  - **The walk row:** `{fb::DataType_IntervalMonthDayNano, {}}`, meaning a bare NULL interval
    literal is refused by name. `ASSERT_EQ(cases.size(), …)` holds the list to the enum. This
    row passes in the red build too, because `build_scalar`'s default refusal names the type.
    It belongs to the green set only.
- [ ] **Step 2: Implement `add_interval`:**
  - **Months:** non-zero months → `add_calendrical_months` with ±months.
  - **Days:** non-zero days → `+ duration_scalar<duration_D>(±days)`.
  - **Nanoseconds:** non-zero nanoseconds →
    - on `TIMESTAMP_DAYS`, `+ duration_D(trunc(±nanos / 86'400'000'000'000))`;
    - otherwise `+ duration_scalar` in the column's unit of `floor(±nanos / unit_ns)`.
  - **NULL interval:** `is_null` → `cudf::make_column_from_scalar` of an invalid
    `timestamp_scalar` of `ts.type()`, at `ts.size()` rows.

  `build_column_binary` calls it, before any other arm, when the op is `Plus` with an interval
  literal on either side, or `Minus` with one on the right. Every other interval operand throws
  `interval arithmetic other than date/timestamp ± literal (#249)`.
- [ ] **Step 3: The device, red then green** (*Device cycle*).
  - **Red build:** `$R cpp/install/bin/peacock_plan_tests --gtest_filter='IntervalArithmetic.*'`
    fails on the missing arm, and its log shows 7 tests ran.
  - **Green build:** they pass, and so do `peacock_plan_tests --gtest_filter='Literals.*:Case*'` (the plan gtests' binary).
- [ ] **Step 4: Commit** `expr: date and timestamp ± an interval literal on the device`

### Task 3: mixed-join crosses and runs on the device

**Files:**
- Modify: `peacockdb-core/src/planner/tests/plan_goldens.rs`: `NOT_RUNNABLE` becomes `&[]`, typed
  as now, and the comment above it says the table is empty while every corpus plan crosses.
- Regenerate: `testdata/goldens/tpch.sf1/*.plans.txt` (only mixed-join's sections move), and
  `recipe-payloads.txt` only if mixed-join is in `PAYLOAD_QUERIES`.
- Modify: `corpus_cases.inc` (`tpch, 1, mixed_join`: `all_modes, all_modes`; its comment
  "cannot cross to a device" goes), and `testdata/cost-registry.csv` (five `gpu_*` cells
  `enabled`; `168` struck).
- Regenerate: `testdata/goldens/tpch.sf1/gpu-result.txt`, `testdata/report.html`.
- Modify: `peacockdb-core/src/test_support/corpus_golden/tests.rs`. Its
  `a_mode_keyed_merge_drops_a_cell_the_registry_no_longer_enables` uses `mixed-join
  mode=tp1-single` as its disabled cell and reads the live registry. Move it to a cell that
  stays off: pbench `timestamp-s-key-group` (#264).
- Modify: `peacockdb-core/tests/common/corpus_cases.inc:23-24` ("but one: mixed-join"), and
  `llm-wiki/build-test.md:658`.

- [ ] **Step 0: Move the fixture** first, and run `--lib` to see it pass before the cells
  move.
- [ ] **Step 1: Regenerate the plan goldens:**
  `UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens`.
  `git diff --stat testdata/goldens/` shows only mixed-join's sections in `tpch.sf1`. Read one:
  the `not runnable: … (#168)` line is gone, and the join carries its residual.
- [ ] **Step 2: Turn the cells on** and run `python3 scripts/ticket_widget.py`.
- [ ] **Step 3: The device.** Sync and build after the line changed: `test_gpu_corpus` compiles a
  device case only for a line that enables it. Then
  `$G ssh "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 tpch_mixed_join"`.
  Its log shows 5 tests ran, and all 5 pass. Then the `rsync`: five sections are added. A
  failing cell goes back off under a ticket, and its section is removed. Never
  `--pull-results`.
- [ ] **Step 4: Run locally** `--lib` and `--test test_cpu_corpus` (rust-only). Expected: PASS.
- [ ] **Step 5: Commit** `corpus: tpch mixed-join crosses and runs on the device`

### Task 4: pbench's interval queries

**Files:**
- Create: `testdata/pbench-queries/interval-month-clamp.sql`, `interval-days-filter.sql`,
  `interval-where.sql`, `interval-mixed-units.sql`, `interval-join-keyed.sql`,
  `interval-join-keyless.sql`
- Modify: `corpus_cases.inc` (six lines, pbench section), `testdata/cost-registry.csv` (six
  rows, all three oracle columns)
- Regenerate: `testdata/goldens/pbench.sf1/*.plans.txt`, `*-mini.cpu.txt`, `*-mini.cost.txt`,
  `mini.result.txt`, `duckdb-result.txt`, `gpu-result.txt`; `testdata/report.html`
- Modify, only if Step 4 asks for it: `peacockdb-core/src/test_support/duckdb_oracle.rs` and its
  tests, `mod.rs` (`timestamp(…)`), `scripts/ticket_widget.py` (`ORACLE_HOW`)

- [ ] **Step 1: The queries.** `f_dt` spans 2024-01-01..2024-12-30 and holds 2024-01-31,
  2024-02-29, 2024-03-31 and 2024-05-31. `f_ts_ms` lies in the first 400 ms of 2024-01-01, and
  `f_ts_ns` in the first 400 µs. No interval expression is a group key: the device refuses a
  computed group key (#264). Each query is small and its SQL is the plain query:

```sql
-- interval-month-clamp: months both ways over month ends and the leap day.
SELECT f_dt, f_dt + INTERVAL '1' MONTH AS next_month, f_dt - INTERVAL '1' MONTH AS prev_month
FROM fact WHERE f_dt IN (DATE '2024-01-31', DATE '2024-02-29', DATE '2024-03-31', DATE '2024-05-31')
GROUP BY f_dt ORDER BY f_dt;

-- interval-days-filter: days and a negative interval, as an aggregate's argument.
SELECT sum(CASE WHEN f_dt + INTERVAL '30' DAY > DATE '2024-12-01' THEN 1 ELSE 0 END) AS late,
       sum(CASE WHEN f_dt + INTERVAL '-45' DAY < DATE '2024-01-01' THEN 1 ELSE 0 END) AS early
FROM fact;

-- interval-where: an interval in a filter's predicate (DuckDB: 2475).
SELECT count(*) AS n FROM fact WHERE f_dt + INTERVAL '-45' DAY < DATE '2024-01-01';

-- interval-mixed-units: one mixed interval on a date and on three timestamp units, row by row.
-- f_dt holds Jan 29 and 30, where months-first and days-first disagree.
SELECT f_id, f_dt + INTERVAL '1 month 2 days 3 hours' AS dt,
       f_ts_ms + INTERVAL '1 month 2 days 3 hours' AS ms,
       f_ts_us - INTERVAL '1 month 2 days 3 hours' AS us,
       f_ts_ns + INTERVAL '1 month 2 days 3 hours' AS ns
FROM fact WHERE f_id < 400 ORDER BY f_id;

-- interval-join-keyed: an interval in a keyed join's residual.
SELECT count(*) AS n FROM fact f JOIN dim d
  ON f.f_k = d.d_k AND f.f_dt <= d.d_dt + INTERVAL '1' MONTH;

-- interval-join-keyless: an interval in a keyless join's condition, which is hoisted.
SELECT count(*) AS n FROM fact f JOIN dim d ON f.f_dt + INTERVAL '7' DAY < d.d_dt
WHERE f.f_id < 50 AND d.d_id < 50;
```

  If DataFusion folds or plans a query so that it no longer reaches the arm, rewrite it until
  its `tp1-single.plans.txt` section shows the interval in a device expression. Say why in the
  detail file.
- [ ] **Step 2: The lines and rows.**
  - The DuckDB oracle is `duckdb(<order>, exact, date("<date-valued columns>"))` for each query
    that outputs a date ± interval: `next_month` and `prev_month`, and `dt`.
  - The order is `no_sort` for those ending in `ORDER BY`. oracle-cleanup's guard says which.
  - The cpu oracle is `data_fusion(sort, exact)`, and the gpu oracle `golden(sort, exact)`. If a
    section is over the cap, it is `live_cpu`, as the pairing test demands.
  - Registry rows: plan, cpu and gpu cells `enabled`, `features` as the neighbouring rows spell
    a date and a join.
- [ ] **Step 3: Write the cpu sections and DuckDB's.**

```bash
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
UPDATE_CANONICAL=1 timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- interval_
[ -x /tmp/duckdb-1.5.4/bin/python ] || { python3 -m venv /tmp/duckdb-1.5.4 && /tmp/duckdb-1.5.4/bin/pip install duckdb==1.5.4; }
timeout 1800 /tmp/duckdb-1.5.4/bin/python testdata/duckdb_result.py --dataset pbench
git diff --stat testdata/
```

  Only the six queries' sections are added. Never pass `--only`: `duckdb_result.py` writes only
  the sections it ran.
- [ ] **Step 4: Measure DuckDB's timestamps.** The analyst's run with DuckDB 1.5.4 found that
  ms, us and ns ± interval all come back as microsecond `TIMESTAMP`, and that our printed text
  matched for these values. So the facet is probably not needed; confirm it here. Run `cargo test --features rust-only -p
  peacockdb-core --test test_cpu_corpus -- duckdb_pbench_interval`. Record in the detail file each
  timestamp column where ours and DuckDB's differ.
  - **Only the printed form differs** (unit or fraction digits, the instant equal): add
    `timestamp("<column>", …)` to the grammar beside `date`. DuckDB's cell then compares as the
    same instant: both parse to nanoseconds since the epoch and must be equal. Add tests
    `timestamp_compares_the_instant` and `timestamp_fails_on_another_instant`, a `FACETS` entry,
    and an `ORACLE_HOW` entry. Then name the columns on the line.
  - **The instants differ:** stop. It is a semantic difference, and it goes to the coordinator
    with both answers.
- [ ] **Step 5: The device.** Sync and build after the lines changed; never reuse an earlier
  build. Then
  `$G ssh "$R PCK_WRITE_GPU_RESULT=1 cpp/install/rust-tests/test_gpu_corpus --test-threads=1 interval_"`.
  Its log shows 30 tests ran, and all pass. Then the `rsync`: thirty sections are added to
  `pbench.sf1/gpu-result.txt`. A failing cell goes back off under a ticket, and its section is
  removed.
- [ ] **Step 6: Run locally:**
  - `--lib`, `--test test_cpu_corpus`, `--test test_corpus_goldens` and `--test test_cost_model`
    (rust-only);
  - `cargo test -p cost-report`;
  - `python3 scripts/ticket_widget.py`.

  Expected: PASS.
- [ ] **Step 7: Commit** `corpus: pbench's interval queries on both engines`

### Task 5: The wiki and the ticket

- [ ] **Step 1: `architecture.md`.** It has no wire type table. The new text goes where the
  wire's `ScalarValue` and the expression evaluator are described: find them with `grep -n
  'ScalarValue\|build_column' llm-wiki/architecture.md`. It names the three fields and how
  `add_interval` applies them, in the order and with the rules of the Global Constraints.
- [ ] **Step 2: `build-test.md`:** the test counts, the `IntervalArithmetic` suite in the C++
  coverage table, and the six queries in the pbench list.
- [ ] **Step 3: Archive #168 (interval literal on wire)** under Done, and update the index. If
  #249's text lists the interval literal, strike it: what remains is interval columns and the
  other types.
- [ ] **Step 4: Final device run:** the dispatched `GPU Tests (nebius-gpu)` CI job, green.
- [ ] **Step 5: Commit** `wiki: interval literals on the wire and the device; #168 archived`
