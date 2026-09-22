
# Scalars tickets

Tickets related to scalars and functions. These tickets are tied to a milestone of full SQL functionality for MVP.

<a id="t224"></a>
### #224 — the device cannot cast an integer to a date

`CAST(i AS DATE)` over an integer column answers on the cpu and is refused on the device:
`Timestamps cannot be converted to numeric without converting it to a duration`.

Arrow reads the integer as days. The cast arm of `build_column` (`expr.cpp`) hands every
numeric-to-chrono cast to `cudf::cast`,
which routes none of them; an integer source needs a duration in days first, and the wire
carries no duration type to route it through. The mirror of #218, where the source is text. No
corpus query casts an integer to a date; `date-part-return-type`'s gtests met it building a
date from `n_nationkey` and made the date from literals instead. No pin yet.

<a id="t223"></a>
### #223 — `substr` with a column for its start or length is refused on the device

`substr(s, start, len)` with a column `start` or `len` answers on the cpu and is refused on the
device: `substr: position/length must be literals`.

The `substr` arm of `build_column_scalar_fn` (`expr.cpp`) reads both from literals, handing
`cudf::strings::slice_strings` two scalars; the per-row form is that function's column
overload. No corpus query reaches it. Seen by `date-part-return-type`'s neighbour survey; no pin
yet.

<a id="t218"></a>
### #218 — the device cannot cast text to a date

`CAST(d AS DATE)` over a `Utf8` column answers on the cpu and is refused on the device:
`cudf::cast` throws "Column type must be numeric or chrono or decimal32/64/128".

The cast arm of `build_column` (`expr.cpp`) hands every non-string target to `cudf::cast`,
which parses no strings; a text source needs `cudf::strings::to_timestamps` with the format
DataFusion accepts, or `to_integers`/`to_floats` for the numeric targets, chosen by the input's
type. The mirror of #203, where the target is the string. No corpus query casts text to a date.
Pinned by `bug_a_text_cast_to_date_is_refused_on_the_device` (`gpu_tests/exec_cases.rs`).

<a id="t222"></a>
### #222 — `round(x, places)` with a column for `places` is refused on the device

`round(x, n)` with a column `n` answers on the cpu and the device refuses it: `round: decimal
places must be a literal`.

DataFusion's signature takes `Int64` for the places, column or literal. The `round` arm of
`build_column_scalar_fn` (`expr.cpp`) reads `places` from a literal alone,
since `cudf::round` takes one scale for the whole column; a per-row scale is one `cudf::round`
per distinct value gathered back, or a refusal the planner makes at plan time so both engines
agree. No corpus query rounds by a column. Seen by `date-part-return-type`'s neighbour survey;
no pin yet.

<a id="t221"></a>
### #221 — `round` over a `Float32` column answers `Float64` on the device

`round(x)` with `x: Float32` is `Float32` on the cpu — DataFusion's signature takes `Float32`
exactly and declares it — and `FLOAT64` on the device, so the sink refuses the column.

The `round` arm of `build_column_scalar_fn` (`expr.cpp`) casts every operand to `FLOAT64`
before `cudf::round`, whatever `return_type` the wire carries, and hands the double up. A
`Float32` operand is the one case where the declaration differs: a decimal or integer operand
arrives under a planner cast to `Float64`. The fix is a `cudf::cast` back to the wire's
`return_type` when it differs, as `date_part`'s arm does. No corpus query: tpcds q2, q54 and q78
round decimals. Pinned on `ENS-date-part-return-type` (PR #161) by
`bug_a_round_over_float32_answers_float64_on_the_device` (`gpu_tests/exec_cases.rs`).

<a id="t219"></a>
### #219 — `ILIKE` is case-sensitive on the device

`s ILIKE 'B%'` answers true for `beta` on the cpu and false on the device: the pattern is
matched as `LIKE 'B%'`.

The wire carries `case_insensitive` on every `LikeExprNode` (`expr_writer.rs`), and the LIKE
arm of `build_column` (`expr.cpp`) reads `negated` alone before calling `cudf::strings::like`,
which has no case-insensitive form. The fix is a `to_lower` on both the column and the pattern
when the flag is set, or `cudf::strings::contains_re` with the `IGNORE_CASE` flag. A wrong row
count under `WHERE … ILIKE`, and a wrong column in a select list; no corpus query writes
`ILIKE`. Pinned by `bug_ilike_is_case_sensitive_on_the_device` (`gpu_tests/exec_cases.rs`).

<a id="t200"></a>
### #200 — a Date64 comes back as a type the wire cannot name

`fb_to_type_id` maps `Date64` to `TIMESTAMP_MILLISECONDS`, and `to_arrow_schema` maps that back to
`Timestamp(ms, None)`. So a column declared `Date64` is exported as a timestamp.

`gpu_plan.fbs` has no `Timestamp` in its `DataType` enum — `Date32` and `Date64` and nothing else in
that family — so the type the device hands back cannot be expressed on the wire at all. Nothing
casts it and nothing refuses it: a plan carrying a `Date64` at the sink dies at `concat_batches`
with `expected Date64 but found Timestamp(Millisecond, None)`.

No corpus column declares a `Date64`, so no cell is disabled against this and it was missed by a
rollout over sixty queries. A user reaching one gets the failure with no ticket to read.

The same gap seen from the other side: a query *producing* a `Timestamp` cannot be serialized, since
`convert_data_type` has no arm for it. That refusal has never been exercised, and whether it is clean
or a panic is unverified.

**Corpus query:** none — every corpus date is `Date32` (`o_orderdate`, `l_shipdate`, …), and
tpch q7, q8 and q9's `date_part` answers an integer, not a date. Simplest:
`select arrow_cast(o_orderdate, 'Date64') from orders limit 1;` (tpch) — speculative: whether our
planner accepts `arrow_cast` is unchecked.

