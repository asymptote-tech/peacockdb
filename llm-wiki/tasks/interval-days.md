# Date and timestamp arithmetic with an interval literal runs on the device

Kind: production

**This task closes [#168](../tickets/corpus-coverage.md#t168)** (interval literal on wire). Thirteenth
of chain K, after oracle-cleanup, whose oracle grammar its corpus lines use.

## Why

The wire's `ScalarValue` (`flatbuffers/gpu_plan.fbs`) has no interval type. A plan that adds an
interval literal to a column cannot cross to the device: `serialize_scalar_value`
(`wire/serialize.rs`) refuses it. Constant folding removes every other corpus interval. tpch
mixed-join's `o_orderdate + INTERVAL '90' DAY` does not fold, so its device cells are off at all
five modes. Only whole days are needed for mixed-join. This task carries the whole literal anyway,
because a month is an ordinary SQL interval and the device can do month arithmetic.

## The work

1. **An interval literal crosses the wire as three parts.** `DataType` gains
   `IntervalMonthDayNano`, appended last so no existing value's number moves. `ScalarValue` gains
   `interval_months: int32`, `interval_days: int32` and `interval_nanos: int64`.
   `IntervalYearMonth` and `IntervalDayTime` literals are written as the same type, converted to
   months, days and nanoseconds. A typed NULL interval crosses with `is_null`.
2. **The device answers `date or timestamp ± interval literal`** exactly as DataFusion does
   (arrow `add_month_day_nano`):
   - The order is months first, then days, then nanoseconds.
   - Months use `cudf::datetime::add_calendrical_months`. A day past the month's end clamps to
     its last day, so Jan 31 + 1 month is Feb 28, or Feb 29 in a leap year.
   - Days and nanoseconds are added as cuDF durations.
   - On a date, the sub-day part counts in whole days, truncated toward zero, as chrono does, so
     `date + 12 hours` is the same date.
   - On a timestamp, nanoseconds convert to the column's unit as arrow converts them.
   - Subtraction negates all three parts.
   - `interval + date` answers a date: the timestamp side gives the type, never the left
     operand.
3. **Where it runs.** An interval operand never takes the AST path. `fb_to_type_id` maps the
   new type to `EMPTY`, since cuDF has no interval type, and an `EMPTY` operand already sends a
   binary operation to the column path. `build_column_binary` gains the arm. A join residual
   reaches the same arm through `build_column`, and so does a hoisted keyless residual, which
   calls it on a zero-row table to learn the type, so the arm answers at zero rows.
   `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` gains a row that refuses the
   interval as an AST literal, by name.
4. **The refusals go.** `NOT_RUNNABLE` (`planner/tests/plan_goldens.rs`) becomes empty, and
   the uncrossable set with it. mixed-join's plan golden line becomes a plan.
   `a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where` (`wire/tests.rs`) takes a
   payload the wire still cannot carry, and names it.
5. **pbench gains queries over `fact`'s date and timestamps** (`f_dt`, `f_ts_ms`, `f_ts_us`,
   `f_ts_ns`). Between them they must reach:
   - months, with the month-end clamp and a leap day;
   - days, and a negative interval;
   - a mixed interval (`'1 month 2 days 3 hours'`) on a date and on a timestamp;
   - subtraction;
   - an interval in a filter's predicate, as an aggregate's argument, and in a join residual,
     keyed and keyless;
   - the months-then-days order, through dates where the two orders disagree (Jan 29 and 30).

   No interval expression is a group key, since the device refuses a computed group key
   (#264).

   **The SQL is the plain query.** DuckDB answers `date ± interval` as a `TIMESTAMP` (measured
   with 1.5.4), where DataFusion answers a date. That is declared on the corpus line with
   oracle-cleanup's `date("<column>")`, which compares DuckDB's cell by its date part. No query
   carries a cast for DuckDB's sake.

   **Timestamps are measured first.** DuckDB may answer `TIMESTAMP_MS` or `TIMESTAMP_NS ±
   interval` as a microsecond `TIMESTAMP`, where DataFusion keeps the column's unit. If the
   values agree and only the printed form differs, this task adds `timestamp("<column>")` to the
   grammar, next to `date`: DuckDB's cell compares as the same instant. The measurement goes in
   the detail file either way.

   **What stays out of pbench.** DuckDB floors a negative sub-day part on a date
   (`date '2024-03-10' − 12 hours` is 2024-03-09), where DataFusion truncates toward zero. That
   case is tested in the gtests only, so the DuckDB comparison is not asked about a difference
   of convention. The seconds unit is gtest-only too: parquet has no seconds unit, so `f_ts_s`
   reads as microseconds in DataFusion.

   **Where the edge cases come from.** `f_dt` spans 2024, so the month-end and leap-day cases
   come from it. The `f_ts_*` values lie in the first minutes of 2024-01-01. Changing `gen.sql`
   moves every pbench golden and is out of scope. Each query is a corpus line with cpu and
   device cells at all five modes, a registry row, and a `duckdb-result.txt` section.

## Corpus

`tpch/mixed-join`: its five device cells turn on. `168` leaves its row. The new pbench queries
land on, on both engines. All of them record `gpu-result.txt` sections at every mode.

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `DataType::IntervalMonthDayNano`; three `ScalarValue` fields |
| `peacockdb-core/src/wire/serialize.rs`, `wire/expr_writer.rs`, `wire/fb_text.rs`, `wire/tests.rs`, `wire/attach.rs`, `wire/mod.rs` | write the literal; the zoned and `Date64` refusal; its text; the refusal test; stale #168 text |
| `peacockdb-core/src/test_support/corpus_golden/tests.rs` | a fixture that used mixed-join's disabled cell moves to one that stays off |
| `cpp/src/expr.cpp`, `cpp/src/peacock/expr.h` | the type map, the literal, the AST refusal, the arithmetic arm |
| `cpp/tests/gpu/test_plan_executor.cpp` | the arithmetic gtests; the `Literals` walk row |
| `peacockdb-core/src/planner/tests/plan_goldens.rs`, `testdata/goldens/*/*.plans.txt` | `NOT_RUNNABLE` empty; mixed-join's plan |
| `peacockdb-core/src/test_support/duckdb_oracle.rs` and its tests, `scripts/ticket_widget.py` | `timestamp("<column>")`, only if the measurement asks for it |
| `testdata/pbench-queries/`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`, `testdata/report.html`, `testdata/goldens/{tpch,pbench}.sf1/*` | the new queries; mixed-join on |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/`, `archive/archived-tickets.md`, `tickets.md` | the wire type; counts; #168 archived |

Component-level API: the wire format. `DataType` gains `IntervalMonthDayNano` (appended), and
`ScalarValue` gains three fields. No C ABI symbol or facade change.

## Restriction

- Interval literals in date and timestamp `+` and `-`. These stay refused at plan time under
  [#249](../tickets/complete-coverage.md#t249) (wire nested/time types), by name:
  - an interval column;
  - interval-by-number arithmetic and interval comparisons;
  - an interval on a timestamp with a time zone, since arrow does months and days in the zone
    and the device has none;
  - an interval on a `Date64`, since the device holds it as a millisecond timestamp and would
    floor a sub-day part that DataFusion drops.
- No cpu golden moves except mixed-join's plan line and the new queries' sections.

## Verification

- Wire tests: each of the three DataFusion interval literal types round-trips into the three
  parts; a NULL interval has `is_null`.
- gtests over a hand-built date and timestamp column:
  - Jan 31 + 1 month in a leap and a non-leap year;
  - Mar 31 − 1 month;
  - `date + 36 hours` and `date − 12 hours`;
  - a mixed interval on each timestamp unit, seconds included;
  - `interval + date`.

  Each expected value is written from DataFusion's answer to the same SQL.
- The new pbench queries and mixed-join green at all five device modes on nebius-gpu, DuckDB's
  device comparison included, and on the cpu. The dispatched `GPU Tests (nebius-gpu)` CI job is
  green.

## Signoff (2026-10-21)

Solved under its constraints. An interval literal crosses the wire as months, days and
nanoseconds (`IntervalMonthDayNano` appended); the device adds months with the month-end clamp,
then days, then nanoseconds, as arrow does, in a new `interval.cpp`. Only a date or naive
timestamp ± a literal reaches the wire; every other interval use, a CASE or function argument
included, is refused at plan time naming #249. `NOT_RUNNABLE` is empty, tpch mixed-join is on at
all five device modes, six pbench queries cover the spec's list on both engines, and DuckDB's
timestamp answers compare through `date(…)` (no `timestamp(…)` facet needed: measured). #168
archived. Completeness: reviewer 0/0, analyst 0/0; no `architecture.md` sentence falsified.
Shortcuts or bandaids: none. `interval.cpp` is proven on cuDF 25.02; 26.02 only by CI's build leg.
