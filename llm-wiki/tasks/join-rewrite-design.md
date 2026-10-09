# Join rewrite — design

Draft under discussion with the human; uncommitted until the chain's specs are final. Shared by
`join-session-cpp` and `join-backend`, which point here rather than repeat it. After the merge
its lasting parts move into `architecture.md` (Joins) and this file goes to the archive.

Tickets: [`joins.md`](../tickets/joins.md) in full, umbrella
[#155](../tickets/joins.md#t155). Every non-join node keeps the recipe architecture; joins leave it.

## 1. ABI and FlatBuffers — approved

### 1.1 `CudfJoin`

One table replaces `CudfHashJoin`, `CudfNestedLoopJoin` and `CudfCrossJoin`. It is a leaf in the
plan: no child stubs, since its inputs arrive through the session calls, not through `execute_node`.

```
table CudfJoin {
  join_type: JoinType;                  // the nine of DataFusion 45; no RightMark (#241)
  keys: [JoinKey];                      // ColumnRef pairs; empty = nested loop or cross
  filter: Expr;                         // residual; refs index the filter's own schema
  filter_columns: [JoinFilterColumn];   // filter schema ordinal -> (side, column)
  null_equals_null: bool = false;       // honoured by every type; no hardcoded EQUAL
  build_schema: Schema;                 // an empty or missing build side, and pad types
  probe_schema: Schema;                 // pad types for unmatched build rows
  projection: [uint32];                 // over [build..., probe...] or the finish output; every type
}
```

### 1.2 Symbols

```
int  peacock_join_build (exec, uint64_t seq, uint64_t build /*0 = no batch*/,
                         uint64_t* out_join, PeacockNodeStats* out_stats);
int  peacock_join_probe (exec, uint64_t join, uint64_t probe,
                         uint64_t* out_handle /*0 = this type answers at finish*/, PeacockNodeStats* out_stats);
int  peacock_join_finish(exec, uint64_t join,
                         uint64_t* out_handle /*0 = this type has no finish*/, PeacockNodeStats* out_stats);
void peacock_join_release(exec, uint64_t join);
```

- `build` and `probe` are consumed. `refcounted-scatter` (#145) lands first, so a handle is a
  `TableResult` of one owner per column plus views: a build side that came out of a scatter reaches the
  session as a view of the scatter's table, with no copy. The session owns the build table, the distinct-key table a
  `distinct_hash_join` views (it must outlive the object), the cuDF join object and the
  build-matched column. Every output is a fresh handle.
- `join_probe` answers exactly one table, possibly of zero rows — the one-batch rule — except for
  LeftSemi, LeftAnti and LeftMark, which answer only at finish and return handle 0 per probe.
- `join_finish` answers one table, possibly of zero rows, for the types that finish (Left, Full,
  LeftSemi, LeftAnti, LeftMark), and handle 0 for the rest.
- Join ids live in their own map in `NodeSession::Impl`; `end_plan` and the error path free them
  as they free handles.
- Each call opens one timing region, `(seq, partition 0, call_index)`, as a one-output
  `execute_node` does; the driver knows which lane made which call.
- **Broadcast foundation (#140).** A session is not tied to a lane: any lane may probe it, and
  `finish` is called once, after the last probe. Calls on one session are serialized. A later
  broadcast task is planner and driver work only.
- Memory: the Rust side estimates what a session holds, as every executor is priced today.

### 1.3 Validation (`plan/join.rs`, `PlanError::Invalid`)

- As today: equal lane counts, `ByHash` on own keys when multi-lane, build `SingleBatch`.

## 2. The join core — approved

Each probe call: **match** (the pairs, or a per-row yes/no), **track** (the build rows matched so
far), **emit** (per type). Build side = DataFusion's left input.

| type | probe call emits | session keeps | finish emits |
|---|---|---|---|
| Inner | the pairs, gathered | – | – |
| Left | the pairs | build matched | unmatched build rows + NULL probe columns |
| Right | the pairs + unmatched probe rows with NULL build columns | – | – |
| Full | as Right | build matched | as Left |
| LeftSemi / LeftAnti | nothing (handle 0) | build matched | `build[matched]` / `build[¬matched]` |
| LeftMark | nothing (handle 0) | build matched | build + `mark` |
| RightSemi / RightAnti | `probe[matched]` / `probe[¬matched]` | – | – |

Matchers — rows, never pairs, where the answer is a yes/no per row:

| case | matcher | materialized |
|---|---|---|
| Inner / Left / Right / Full | `hash_join` over build keys, built once | the pairs = the output |
| … with a residual | index pairs → gather the filter's columns only → mask → compact → gather output | indices + filter columns of the key matches |
| RightSemi / RightAnti | `distinct_hash_join` over the build's distinct keys, built once | O(probe batch) |
| LeftSemi / LeftAnti / LeftMark | the `hash_join` probed with the batch's distinct keys | ≤ \|build\| |
| semi family with a residual | `mixed_left_semi_join` | O(rows); the build is rescanned per batch |
| nested-loop semi family | `conditional_left_semi/anti_join` | O(rows) |

A condition the cuDF AST cannot take: hoist its one-side pieces into columns; split the
conjuncts and run the conditional join on the AST-able ones; a chunked cross product only when no
conjunct is AST-able.

## 3. Per-case pseudocode — approved

C++ in `cpp/src/operators/join.cpp` (rewritten) and `node_session.cpp` (the four symbols).
`B` = build table, `P` = one probe batch, `Bk`/`Pk` = their key columns (views),
`cmp` = `null_equals_null ? EQUAL : UNEQUAL`. Defaults (`stream`, `mr`) omitted.

### 3.0 Helpers

```
constexpr size_type kNoMatch = std::numeric_limits<size_type>::min();   // 25.02 has no public name

idx_view(uvector v)          = column_view{data_type{INT32}, v.size(), v.data(), nullptr, 0}
gather_rows(T, idx, policy)  = cudf::gather(T, idx_view(idx), policy)   // NULLIFY where idx may hold kNoMatch
bools(n, v)                  = cudf::make_column_from_scalar(numeric_scalar<bool>(v), n)
set_true(col, idx)           = binary_operation(col, cudf::contains(idx_view(idx), sequence(|col|, 0)), LOGICAL_OR, BOOL8)
                               // contains, not scatter: idx repeats a row once per match, and cuDF leaves a
                               // scatter map with duplicate indices undefined; idx never holds an out-of-range value
in_range(idx, n)             = idx >= 0 AND idx < n, as BOOL8 (two binary_operations)
                               // the unmatched value: only distinct_hash_join promises INT32_MIN; hash_join::left_join
                               // and conditional_left_join say "an unspecified out-of-bounds value"
negate(col)                  = cudf::unary_operation(col, unary_operator::NOT)
null_column(dtype, n)        = s = cudf::make_default_constructed_scalar(dtype); s->set_valid_async(false);
                               cudf::make_column_from_scalar(*s, n)
null_table(schema, n)        = one null_column(to_cudf(field), n) per field   // to_cudf: fb DataType + scale
empty_table(schema)          = null_table(schema, 0)
distinct_keys(K)             = K' = (cmp == UNEQUAL) ? cudf::drop_nulls(K, all_cols(K)) : K
                               cudf::distinct(K', all_cols(K'), KEEP_ANY, cmp, nan_equality::ALL_EQUAL)
residual_mask(Bt, Pt, bi, pi) =
    // the filter's columns only, gathered through the pair indices
    cols = for each JoinFilterColumn f: gather_rows(side(f) == Build ? Bt.select({f.index}) : Pt.select({f.index}),
                                                    side(f) == Build ? bi : pi, DONT_CHECK).col0
    build_column(filter, table_view(cols))          // the column path: AST-able or not
keep(bi, pi, mask)           = cudf::apply_boolean_mask(table_view{{idx_view(bi), idx_view(pi)}}, mask)
                               // a NULL mask entry drops the pair, as SQL wants
emit(cols_build, cols_probe) = apply `projection` over [cols_build..., cols_probe...] when the field is
                               present — an absent projection keeps every column, a present empty one keeps
                               none; the plan never asks for zero kept columns (4.1's placeholder)
```

No side and no output has zero columns: the planner makes `__rowmarker__` an explicit column (4.1),
so the session has no rows-only arm and refuses a zero-column side as a planner bug. `null_table` is the only maker of a padded column, and it builds no literal, so the typed-null
defects of the expression path (#198, fixed; #211, open) cannot reach a pad. A padded column must
match what the matched rows carry; the node-level schema tests (`join_schema_cases.rs`) hold that
per case.

### 3.1 `join_build(seq, build)`

```
desc = node(seq).as<CudfJoin>()
B    = build != 0 ? take(build) : empty_table(desc.build_schema)          // #212: no batch is an empty build
s    = new Session{desc, B}
if |B| == 0: s.empty_build = true; return s                                // 3.8 answers every type

if desc.filter: filter' = hoist(filter, Build, B); s.B = B with the hoisted columns    // 3.6; keyed joins too
// the residual's conjuncts, split once (3.4's last rule):
//   cross_filter  — read both sides
//   build_only    — read the build alone;  probe_only — read the probe alone
// for RightSemi/RightAnti the build is not emitted, so build_only filters it here, before any key work;
// for the build-side semi family it is the preserved-side R, evaluated over B at finish (3.3)
if type in {RightSemi, RightAnti} and build_only: B = apply_boolean_mask(B, build_only IS TRUE over B)
if desc.keys non-empty:
    Bk = B.select(build_key_ordinals)
    semi_family = type in {LeftSemi, LeftAnti, LeftMark, RightSemi, RightAnti}
    if type in {RightSemi, RightAnti} and !cross_filter:
        s.Bd  = distinct_keys(Bk)                                          // owned: dhj views it
        if |s.Bd| == 0: s.empty_build = true; return s                     // every key NULL under UNEQUAL
        s.dhj = distinct_hash_join(s.Bd, cmp)                              // built once
    elif semi_family and cross_filter AST-able:
        s.Bk = Bk                                                          // mixed_* per probe, 3.4; the cross
                                                                           // conjuncts ANDed at the cuDF AST level
                                                                           // (NULL_LOGICAL_AND), one expression
    if keys non-empty and the pairs path can be taken (not RightSemi/RightAnti-without-cross_filter,
       not semi_family-with-AST-cross_filter):
        s.hj = hash_join(Bk, cmp)                                          // built once; the portable ctor
    // a pairs-path call with s.hj null throws by name — never a null dereference
if type in {Left, Full, LeftSemi, LeftAnti, LeftMark}:
    s.matched = bools(|B|, false)
```

### 3.2 Equi-joins without a residual — `join_probe`

```
Pk = P.select(probe_key_ordinals)
Inner:    [pi, bi] = hj.inner_join(Pk)
          return emit(gather_rows(B, bi, DONT_CHECK), gather_rows(P, pi, DONT_CHECK))
Left:     [pi, bi] = hj.inner_join(Pk)
          s.matched = set_true(s.matched, bi)
          return emit(gather_rows(B, bi, DONT_CHECK), gather_rows(P, pi, DONT_CHECK))
Right:    [pi, bi] = hj.left_join(Pk)                    // probe is cuDF's left: every probe row once
          return emit(gather_rows(B, bi, NULLIFY), gather_rows(P, pi, DONT_CHECK))
Full:     [pi, bi] = hj.left_join(Pk)
          bi_hit   = apply_boolean_mask(table_view{{idx_view(bi)}}, in_range(bi, |B|))
          s.matched = set_true(s.matched, bi_hit)
          return emit(gather_rows(B, bi, NULLIFY), gather_rows(P, pi, DONT_CHECK))
          // never hj.full_join: per batch it re-emits every unmatched build row
LeftSemi, LeftAnti, LeftMark:
          [_, bi] = hj.inner_join(distinct_keys(Pk))      // each build row at most once
          s.matched = set_true(s.matched, bi)
          return handle 0
RightSemi, RightAnti:
          bi  = dhj.left_join(Pk)                        // one entry per probe row, kNoMatch if none
          hit = in_range(bi, |s.Bd|)
          mask = type == RightSemi ? hit : negate(hit)
          return emit(-, cudf::apply_boolean_mask(P, mask))
```

Unmatched entries are found by range, never by comparing with `kNoMatch`, and never reach
`set_true`. `probe_only` conjuncts of a build-side semi family filter `P` before `Pk` is taken.

### 3.3 `join_finish` — the build-preserving types

```
Left, Full:  U = cudf::apply_boolean_mask(B, negate(s.matched))
             return emit(U.columns, null_table(desc.probe_schema, |U|).columns)
m = build_only ? s.matched AND (build_only IS TRUE over B) : s.matched      // 3.4's last rule
LeftSemi:    return emit(cudf::apply_boolean_mask(B, m), -)
LeftAnti:    return emit(cudf::apply_boolean_mask(B, negate(m)), -)
LeftMark:    return emit(B.columns + [m as "mark"], -)                    // B.with(mark): shares B's column
                                                                           // owners, owns the mark; no copy
others:      return handle 0
```

No probe batch at all (#173) needs no arm: `matched` is still all false, so LeftAnti answers the
build, Left and Full the build padded, LeftSemi zero rows, LeftMark the build with `mark = false`.
Projection applies on every path (hacks-audit finding 11).

### 3.4 Equi-joins with a residual

Pairs path, for Inner, Left, Right, Full:
```
[pi, bi]   = hj.inner_join(Pk)
[bi', pi'] = keep(bi, pi, residual_mask(B, P, bi, pi))
Inner:  return emit(gather_rows(B, bi', DONT_CHECK), gather_rows(P, pi', DONT_CHECK))
Left:   s.matched = set_true(s.matched, bi'); same emit as Inner          // #153: unmatched decided after the filter
Right:  hitP = set_true(bools(|P|, false), pi')
        UP   = cudf::apply_boolean_mask(P, negate(hitP))
        return emit(concat(gather_rows(B, bi', DONT_CHECK), null_table(build_schema, |UP|)),
                    concat(gather_rows(P, pi', DONT_CHECK), UP))
Full:   Right's emit, plus s.matched = set_true(s.matched, bi')
```
The residual is evaluated once per key match, over the filter's columns only; full rows are
gathered for the survivors.

Memory is bounded by chunking the probe batch, as 3.6 does. Before matching,
`n = hj.inner_join_size(Pk)` (both versions) prices the pairs at `n × (8 + filter row bytes)`, and
any range whose `n` exceeds `size_type`'s max is chunked down further — every pairs-producing path
checks this, so 2³¹+k pairs never wrap to k rows (3.9's refusal names the join if one row range
alone exceeds it);
over the scratch budget, `P` is cut into row ranges of about equal share and each range runs the
path above, the outputs concatenated into the one table the call returns. Every per-row outcome
(a probe row's match, a build row's `matched` bit) is decided within one range, so chunking never
changes an answer.

Semi family, row-wise (no pairs). The cross residual must be AST-able after hoisting (3.6); if it
is not, these types take the pairs path above over the `s.hj` 3.1 built for them, chunked the same way, and derive their rows from
`bi'` (LeftSemi/LeftAnti/LeftMark: `set_true(matched, bi')`) or `pi'` (RightSemi/RightAnti:
`set_true(bools(|P|, false), pi')` as the mask). Every key match is evaluated either way —
`mixed_*` does the same inside its kernel — so the extra cost is only the stored indices and
filter columns, and the chunking bounds that.
```
LeftSemi, LeftAnti, LeftMark:
    bi = cudf::mixed_left_semi_join(Bk, Pk, B_cond, P_cond, ast(filter), cmp)   // build rows matched by this batch
    s.matched = set_true(s.matched, bi)
    return handle 0
RightSemi:  pi = cudf::mixed_left_semi_join(Pk, Bk, P_cond, B_cond, ast(filter, sides swapped), cmp)
            return emit(-, gather_rows(P, pi, DONT_CHECK))
RightAnti:  pi = cudf::mixed_left_semi_join(…same…)                          // then the complement:
            return emit(-, apply_boolean_mask(P, negate(set_true(bools(|P|, false), pi))))
            // not mixed_left_anti_join: its header says a NULL predicate drops the row, and NOT EXISTS keeps it
```
`B_cond`/`P_cond` are the tables the AST's `LEFT`/`RIGHT` refs index. `mixed_*` hashes its right
table per call: the probe batch for the build-side types, the build for RightSemi/RightAnti. Both
versions have it; the 26.02-only `filtered_join` and `filter_join_indices` are not used, so the
whole file compiles one way on 25.02, the 25.10a CI image and 26.02.

A residual's conjuncts that read only the **preserved** side of a semi, anti or mark join — the
side whose rows it emits — never reach the matcher. Evaluated per row of that side (`R`), they
combine with the key match bit: semi keeps `matched ∧ R`, anti keeps `¬(matched ∧ R)`, mark is
`matched ∧ R`, where `R` is `R IS TRUE` (`replace_nulls(R, false)`): a NULL condition is no match,
so `NOT EXISTS (… AND b.v > 5)` keeps a row whose `b.v` is NULL and its mark is `false`, not NULL.
For the build-side types `R` is evaluated over `B` at finish (3.3); for RightSemi/RightAnti over
each probe batch. Conjuncts reading only the other side filter it before matching (DataFusion
usually pushes those below the join already). Only conjuncts reading both sides need `mixed_*`.
So `NOT EXISTS (S AND x IS NULL)` from 3.5 stays on the row-wise matchers of 3.2.

### 3.5 `NOT IN` — a planner rewrite, nothing in the executor

DataFusion 45 plans `x NOT IN (S)` as the anti (or, inside an `OR`, mark) join of `NOT EXISTS (S
AND y = x)`, and answers wrong when `x` or `y` is NULL (#80): measured, a correlated `NOT IN` gives
7 rows where SQL gives 3, an uncorrelated one 5 where SQL gives 2. A logical `OptimizerRule` of
ours, registered in `build_session_state` ahead of DataFusion's `decorrelate_predicate_subquery`,
rewrites a `NOT IN` that sits on the **AND/OR spine of a filter** — reached from the `Filter`
root through `AND` and `OR` alone — whose `x` or `y` can be NULL in the data. On that spine a
NULL and a `false` drop a row alike, so the two-valued rewrite below is sound there and nowhere
else. **First the rule puts the filter's predicate in negation normal form**: every `NOT` is
pushed down to a leaf through `AND` and `OR` by De Morgan, and double negations cancel — both hold
in SQL's three-valued logic, so the predicate means exactly what it meant. A `NOT` reaching an
`IN`/`EXISTS` leaf flips its `negated` flag: `NOT (x NOT IN S)` becomes the positive `x IN S`,
`NOT (x IN S)` becomes `x NOT IN S`, `NOT (a OR x NOT IN S)` becomes `NOT a AND x IN S`. (DataFusion
decorrelates `Not(InSubquery{negated: false})` as an anti join with `NOT EXISTS` semantics,
`decorrelate_predicate_subquery.rs:190-216`, and its simplifier never folds the `NOT` in —
`negate_clause` has no `InSubquery` arm — so without this pass those forms are wrong.) After it,
every `IN`/`NOT IN` that was under connectives and `NOT`s alone is a spine leaf. The rule visits every `Filter`, including
those inside expression subqueries — `transform_down_with_subqueries`, or `apply_order:
Some(TopDown)` matching `Filter` nodes — since a `NOT IN` nested in an `EXISTS` is otherwise
decorrelated before it is seen (`optimizer.rs:386-392`).

**Where a NULL `IN` result is read, a nullable `IN`/`NOT IN` stays refused.** Inside a filter, an
`IN` under any operator but `AND`, `OR` and `NOT` — `IS [NOT] NULL`, `IS [NOT] TRUE/FALSE/UNKNOWN`,
a comparison (`(x IN S) = false`), `COALESCE`, a function argument — reads SQL's NULL answer as a
value, and a mark join never yields NULL (`join_type.rs:56-69`). A projection or a `CASE` never
reaches the rule: DataFusion 45 decorrelates subqueries only in a filter (#247). The refusal fires
only where the data says `x` or `y` can be NULL; otherwise `IN` is two-valued and plans. The refusal names [#250](../tickets/joins.md#t250). So
`planner/nulls.rs`'s refusal is **narrowed, not deleted**: it moves to `planner/nullability.rs`
beside `can_be_null` and refuses a nullable `IN`/`NOT IN` subquery off the spine, by name. The
three-valued form (`CASE WHEN EXISTS(S AND y = x) THEN true WHEN x IS NULL AND <S non-empty> OR
EXISTS(S AND y IS NULL) THEN NULL ELSE false END`) is [#250](../tickets/joins.md#t250)'s fix.

The rewrite fires on such a `NOT IN` whose `x` or `y` can be NULL in the data. Every corpus column is declared nullable, so the
declared schema decides nothing: the rule traces `x` and `y` through projections, filters and
aliases to a `TableScan` column, and reads that column's row-group null counts from the parquet
footers with the reader `can_be_null` uses (`scan_mapping/parquet_meta.rs`), shared — opened from
the `ListingTableUrl`'s `url.as_str()` through `Url::to_file_path()`, since `url.prefix()` is an
object-store path with no leading `/` (`listing/url.rs:140-156`); a `SubqueryAlias` (`FROM orders
o`) recurses into its input. A column it cannot trace, or a footer without the statistic, counts
as possibly-NULL. Where neither side can be
NULL, `NOT IN` means `NOT EXISTS` and is left alone — so tpch q16 and anti-join, whose keys hold no
NULL, keep their plans and their lanes; pbench, whose keys do, shows the rewrite:

```
correlated:    NOT EXISTS (S AND y = x) AND NOT EXISTS (S AND y IS NULL) AND NOT EXISTS (S AND x IS NULL)
uncorrelated:  NOT EXISTS (S AND y = x) AND (SELECT count(*) FROM S WHERE y IS NULL) = 0
                                        AND (x IS NOT NULL OR (SELECT count(*) FROM S) = 0)
```

DataFusion then decorrelates these itself (measured on 45, all four forms equal DuckDB):
- correlated: three anti joins — or three mark joins under an `OR` — keyed on the correlation
  columns, hashed into lanes; the third carries `x IS NULL`, a condition on the preserved side
  alone (3.4's last rule);
- uncorrelated: one anti join on `x = y`, hashed; and a cross join and a `Right` nested loop
  against one-row counts, which put the outer side in one lane — a one-row side is what a
  broadcast (#140) would later serve. DataFusion 45 cannot plan an uncorrelated `NOT EXISTS`,
  hence the counts. The counts are `count(lit(1i32))`, not `Int64(1)`: over an unfiltered `S`,
  DataFusion's `AggregateStatistics` answers a `count(*)` (`COUNT_STAR_EXPANSION`, Int64(1)) from
  the table's statistics with a `PlaceholderRowExec` (`aggregate_statistics.rs:45-90`,
  `count.rs:321-349`), which the translator refuses (`nodes.rs:166`, #158). The rule says so in a
  comment, and a planner test plans the uncorrelated rewrite over an unfiltered `S` with no
  `PlaceholderRowExec`.

The cpu answers correctly too, since DataFusion executes the rewritten plan, so the two engines
agree and the DuckDB oracle confirms. No flag reaches the wire, no executor rule.
`planner/nulls.rs`'s refusal of anti and mark joins over nullable keys becomes the narrower one
above: every join type honours `null_equals_null`, which is `NOT EXISTS`'s semantics, so only the
off-spine `IN`/`NOT IN` forms stay refused. Planner tests, each answer checked against DuckDB:
`NOT (x IN S)` (folded, then rewritten), `w = 0 OR NOT (x IN S)` (the same), `NOT (x NOT IN S)`
(folded to a positive `IN`, answered as a semi join), `NOT (w = 0 OR x NOT IN S)` (De Morgan, then
`IN`), a `NOT IN` inside an `EXISTS` (rewritten), and `(x IN S) IS NULL` (refused). pbench shows the
folds at query level: `not-not-in` and `not-or-not-in`; and the refusal: `in-is-null`. A positive `IN` needs no rewrite — NULL and
false drop a row alike. `IN` as a projected value does not plan on DataFusion 45 at all.

### 3.6 Nested loop — no keys, a condition

A predicate-free nested loop of any type but Inner arrives here with the literal `true` as its
condition (4.1); it is AST-able, so it runs the first arm below.

```
hoist(filter, side, T): for each maximal sub-expression that reads only `side` and that
    cudf_ast_can_evaluate refuses: append build_column(sub, T) to T, replace sub by a ColumnRef
    to it. Build side once at join_build; probe side per batch.
split(filter') = conjuncts that are AST-able (A) and the rest (R)

A non-empty, R empty — row-wise where the type allows:
    Inner:  [bi, pi] = cudf::conditional_inner_join(B', P', ast(A))
    Left:   same, then s.matched = set_true(s.matched, bi)               // never conditional_left_join on a stream
    Right:  [pi, bi] = cudf::conditional_left_join(P', B', ast(A, sides swapped))   // NULLIFY bi
    Full:   as 3.4's pairs path from conditional_inner_join: hitP, the unmatched probe rows, s.matched
    LeftSemi/LeftAnti/LeftMark: bi = cudf::conditional_left_semi_join(B', P', ast(A)); s.matched = set_true(…)
    RightSemi: pi = cudf::conditional_left_semi_join(P', B', ast(A, swapped))
    RightAnti: pi = cudf::conditional_left_semi_join(P', B', ast(A, swapped)), then the complement
               (the anti forms' headers drop a row whose predicate is NULL; NOT EXISTS keeps it)
    then emit as in 3.2

A non-empty, R non-empty — candidates, then the rest, chunked like 3.4 by
conditional_inner_join_size(B', P', ast(A)) (both versions):
    [bi, pi] = cudf::conditional_inner_join(B', P', ast(A))
    [bi', pi'] = keep(bi, pi, residual_mask over R)
    derive per type exactly as 3.4's pairs path

A empty — chunked cross product of indices, never of rows:
    c = max(1, budget_rows / max(|B|, 1))
    for each probe chunk [i, i+c):
        pi = cudf::repeat(table{sequence(n_c, i)}, |B|).col0           // probe-major; row order is free here
        bi = cudf::tile(table{sequence(|B|, 0)}, n_c).col0
        [bi', pi'] += keep(bi, pi, residual_mask over R)
    derive per type as in 3.4; one table out per call
```
This lifts #160 (nested loop beyond Inner and Left) and #215 (a Left nested loop over a non-AST
predicate). `budget_rows` comes from the executor's scratch budget; the chunk loop is inside one
call, so the one-batch rule holds.

### 3.7 Cross — no keys, no condition (Inner only)

Every other predicate-free type is a 3.6 nested loop over `true` (D6 of the validation).

```
nb, np = |B|, |P|
if nb * np > size_type max: throw "cross join of nb × np rows exceeds one table"     // the one overflow check
out = nb == 0 || np == 0 ? empty table of [build..., probe...]
    :                      cudf::cross_join(B, P)          // neither side has zero columns (4.1)
return emit(out split at B.num_columns)                    // #207: the projection drops placeholders
```
A zero-row side answers one zero-row table on both engines (#208 is the cpu's half).

### 3.8 An empty build side

`join_build` with `|B| == 0` sets `s.empty_build`; no cuDF join object is made.
```
probe:  Inner, Left, RightSemi:  empty table of the output schema
        LeftSemi, LeftAnti, LeftMark: handle 0
        Right, Full:    return emit(null_table(build_schema, |P|), P)
        RightAnti:      return emit(-, P)               (every row)
finish: Left, Full, LeftSemi, LeftAnti, LeftMark: empty table of the output schema
                        (LeftAnti over an empty build is empty; LeftMark has no rows to mark)
```
The driver no longer refuses a lane with no build batch: it calls `join_build(build = 0)`.
`without_build` and `feeds_owing_build` go.

### 3.9 Output and stats

The output table moves the gathered columns in, without a deep copy. `PeacockNodeStats.rows`
and `varlen_content_bytes` are read off it as `execute_node` does. A table that would exceed
`size_type` rows is a refusal naming the join, not a silent wrap.

### 3.10 cuDF version

Headers: `#if __has_include(<cudf/join/join.hpp>)` includes `join/join.hpp`, `hash_join.hpp`,
`distinct_hash_join.hpp`, `conditional_join.hpp`, `mixed_join.hpp`; else `<cudf/join.hpp>`.
Constructors: `hash_join(Bk, cmp)` and `distinct_hash_join(keys, cmp)` — the forms that compile
on 25.02, 25.10a and 26.02. No call differs by version.

Header layout is detected with `__has_include`, as `join.cpp` and the gtests already do, since
the layout is what changed. A later fast path that only a version has (`filtered_join`,
`filter_join_indices`) would test `<cudf/version_config.hpp>`'s `CUDF_VERSION_MAJOR` and
`CUDF_VERSION_MINOR` — present in 25.02 and 26.02 alike, used nowhere in the tree today.

## 4. Planner, Rust executors, driver — approved

### 4.1 Planner (`planner/translator/nodes.rs`, `plan/join.rs`)

- The three plan nodes stay — `GpuHashJoin`, `GpuNestedLoopJoin`, `GpuCrossJoin` — so plan text
  reads as today; all three serialize to one `CudfJoin`. `GpuNestedLoopJoin.join_type` widens from
  `{Inner, Left}` to DataFusion's nine.
- The capability matrix becomes two facts per type: every probe streams; `needs_finish` is
  Left, Full, LeftSemi, LeftAnti, LeftMark. `answers_in_one_call` and the probe-side
  `GpuCoalesceAllBatches` go: the probe side is never coalesced.
- Refusals lifted: outer with a residual (#153), RightSemi/RightAnti with a residual (#159),
  nested loop beyond Inner and Left (#160). `planner/nulls.rs`'s refusal of anti and mark joins
  over keys NULL on both sides is narrowed to 3.5's off-spine `IN`/`NOT IN` forms; it and the
  file's `can_be_null` analysis move to `planner/nullability.rs`, which #137 below also reads.
- Two refusals stay as they are, both unreachable from SQL and pinned only by construction: a
  join key that is not a bare column (DataFusion projects `ON t.k + 1 = b.k` below the join, so
  the key it hashes is a column; `join_capability.rs:553`), and a join filter reading the mark
  column (`JoinSide::None` exists only in a LeftMark's output, never in a filter's inputs;
  `translator/common.rs:94`).
- The `NOT IN` rewrite (3.5) is a logical `OptimizerRule` in `planner/`, registered by
  `build_session_state` (`lib.rs`) ahead of `decorrelate_predicate_subquery`.
- #137: under `null_equals_null = false`, a side whose unmatched rows are never emitted gets
  `GpuFilter(<key> IS NOT NULL)` under its `GpuEmitPartitions`. Decided on the translated side —
  a side whose top node is a `GpuEmitPartitions` — not on DataFusion's `RepartitionExec`, which
  DataFusion always wraps in `CoalesceBatchesExec` (`coalesce_batches.rs:57-80`). That side, per type: Inner both;
  Left probe; Right build; Full none; LeftSemi, RightSemi both; LeftAnti probe; RightAnti build;
  LeftMark probe. Only a shuffled side — the skew is a shuffle's — and only where `can_be_null`
  says the key can be NULL. tpch's keys hold none, so its plans do not move; tpcds's foreign keys
  do (`ss_sold_date_sk` holds 129,850 NULLs at sf1, all of them on one lane today), so about 73
  tpcds plans gain the filter at the three tp4 modes, with their cpu, cost and memory goldens.
  Answers do not move. Accepted.
- **`__rowmarker__` is explicit (#63).** It is today's `__rowcount__` (`project.cpp`'s placeholder), renamed: the column marks rows, it counts nothing. `project.cpp` and every test or golden naming the old spelling move with it. Where DataFusion has a zero-column node (19
  `GpuProject: exprs=[]` in tpcds, under `count(*)` and q9's cross joins), the translator plans a
  project of one literal column, `Int8 0 AS __rowmarker__`, declared in its schema. A join above
  such a side drops the placeholder through its projection; a join whose kept columns would be
  none keeps one placeholder instead. A scan that projects no column (tpch nested-limits:
  `GpuLoadParquet … projections=[] schema=[]`) declares `__rowmarker__` itself, and both engines
  produce it from the row count they read — a project cannot go below a scan. The wire says so
  explicitly: `CudfScan` gains `rows_only: bool`, since an empty projection reads every column
  today (`scan.cpp:45-53`). Plan validation
  refuses a zero-column schema anywhere. The
  cpu computes the same literal, so both engines hold the table the plan declares, and
  `project.cpp`'s empty-projection arm becomes a refusal. Cost: a cross join materializes the
  1-byte column before its projection drops it.
- **A side DataFusion folds to nothing** (review row 8). `PropagateEmptyRelation` keeps a Left
  join over an `EmptyExec` (`SELECT * FROM tiny t LEFT JOIN (SELECT * FROM dim WHERE false) d ON
  t.t_k = d.d_k`), and the translator has no arm for it today, so the query is refused. A new
  leaf, `GpuEmpty{schema}`, one lane, emits no batch on either engine and calls nothing on the
  device (its recipe is driver-routed, as `GpuMergePartitions`' is). A join over it takes
  `set_build(None)` on that side, or a probe stream with no batch — the shapes 3.8 and 3.3 already
  answer.
- **`IS [NOT] DISTINCT FROM`** (review row 9). DataFusion 45 extracts only `=` as a join key, so
  `ON d.d_k IS NOT DISTINCT FROM t.t_k` is a keyless nested loop whose filter carries the
  operator, and the device's AST throws on it (`expr.cpp:103-127`). Two changes: the device maps
  `IsNotDistinctFrom` to cuDF's AST `NULL_EQUAL` and `IsDistinctFrom` to `NOT(NULL_EQUAL)` (25.02
  and 26.02 both have it), and the column path to `binary_operator::NULL_EQUALS` /
  `NULL_NOT_EQUALS`, so the nested loop answers on any lane; and the translator promotes a keyless
  nested loop whose filter's conjuncts include `a IS NOT DISTINCT FROM b` (one column from each
  side) to a hash join on those pairs with `null_equals_null = true`, the other conjuncts its
  residual — only when every key pair would be such a conjunct, since `null_equals_null` is one
  flag for all keys. The hash join shuffles into lanes where the nested loop ran in one.
- Keyless joins keep both sides in one lane (`check_join_inputs`). A predicate-free
  `NestedLoopJoinExec` becomes `GpuCrossJoin` only when it is Inner; any other type becomes
  `GpuNestedLoopJoin` with the literal `true` as its filter (3.6) — today's arm makes every
  predicate-free nested loop a cross join, which answers nothing where a Left, Right or Full
  over an empty side owes padded rows (`tiny LEFT JOIN empty ON true`: 8 rows, not 0). Both
  arms merge their probe, where the cross arm today does not.

### 4.2 Wire (`wire/`)

- The wire still writes the join: `wire/join.rs` puts one `fb::CudfJoin` node into the per-query plan buffer through the same writer every node uses, so it has a `seq`, and the session's calls name that `seq` (`peacock_join_build(exec, seq, build)`); the session reads its description from the plan, `node(seq).as<CudfJoin>()`. What goes is the join's call choreography — the chained `execute_node` calls and the extra nodes they needed. Non-join nodes keep their recipes whole.
- `wire/join.rs`'s recipes go: no `ProjectRole::{ProbeKeys, NullPad, Narrow}`, no
  `Input::{BuildSideCopy, BatchCopy, AccumulatedKeys}`, no `CallPattern::AtDone` for joins. A join
  writes one `CudfJoin` with both side schemas; `attach.rs` dispatches it without a recipe.
- `CudfJoin.chunk_bytes` (join-session-cpp's field; 0 means 1 GiB) is written from the planner's
  scratch budget, so the session's chunking follows the mode's budget and the accountant prices it.
- Timestamps in a `CudfJoin` schema use the fbs `Timestamp*` variants repartition-keys adds. A type
  `convert_data_type` cannot map is a `PlanError` in every schema from repartition-keys on (#249) —
  the plan golden says "not runnable" — never the `Null` that `serialize_schema` writes silently
  today (`serialize.rs:136`), which the session would only refuse at run time, on a lane with no
  build or probe batch.
- The plan goldens' recipe line for a join (`per probe batch: execute_node(#4 CudfHashJoin{…},
  build copy, batch)`) becomes the session's: `join_build(#4), per probe batch: join_probe, at
  done: join_finish` — the last only for the finishing types. `recipe-payloads.txt` loses its joins.

### 4.3 GPU executor (`executor/gpu_backend/`)

```
JoinExecutor::set_build(self, Option<GpuBatch>) -> CallResult<GpuProbingJoin>
    peacock_join_build(seq, build.map(consume).unwrap_or(0)) -> join id
ProbingJoin::probe_and_fetch(&mut self, GpuBatch) -> CallResult<Option<GpuBatch>>
    peacock_join_probe(join, consume(batch)) -> handle or 0
ProbingJoin::finish_and_fetch(self) -> CallResult<Option<GpuBatch>>
    peacock_join_finish(join) -> handle or 0
Drop for GpuProbingJoin: peacock_join_release(join)
```
- `without_build` goes: no build batch is `set_build(None)`, and the session answers (3.8).
  `ProbingJoin::owes_nothing()` — an empty build under Inner, Left, LeftSemi, LeftAnti, LeftMark
  or RightSemi — lets the driver drop the lane's probe batches without a call.
- `copy_of`, `build_copy`, `finish_without_keys`, the `per_probe`/`at_done` call lists go
  (hacks-audit P9, finding 11).
- `ProbingJoin` returns `Option`, not `Vec`: the one-batch rule in the type (#220's device side).
- Accounting: resident = build bytes + the cuDF join object (about 16 B per build row: a
  `static_multiset` of 8-byte slots at load factor 0.5, whatever the key width; the same for
  `distinct_hash_join` over the distinct keys, plus that table) + one byte per build row for
  `matched`. Scratch per probe = the batch + its output estimate + the transients: the batch's
  `distinct_keys`, `8 + the residual's per-row bytes` per key-match pair (priced by
  `inner_join_size`), and for RightSemi/RightAnti with a cross residual the build's hash that
  `mixed_*` makes per call. **An earlier version of this line said 8 bytes a pair**, which is the
  index maps alone; join-session-cpp measured the executor's real bound and put the formula in
  `gpu_plan.fbs`'s `chunk_bytes` comment, where the second term is the sum over the filter's
  columns of their fixed width, 16 for a variable-width one. A planner converting its scratch
  budget by the old figure under-prices by the residual's width, so the accountant and the session
  would disagree about the same call.
- `peacockdb-ffi`: the four symbols.

### 4.4 CPU executor (`executor/cpu_backend/join.rs`)

One DataFusion join stream per lane, alive from `set_build` to `finish_and_fetch` — the device's
session in DataFusion's terms. Measured on DataFusion 45 (scratch probe `stream-probe`): all nine
types, hash with and without a residual, nested loop with a condition and cross; every per-call
and finish count equals a hand count.

```
set_build(build: Option<RecordBatch>):
    left  = MemoryExec([build or RecordBatch::new_empty(build schema)])      // #212: no batch, empty build
    right = ChannelExec(rx)                                                  // one partition, fed per call
    plan  = HashJoinExec(left, right, on, filter, type, projection, CollectLeft, null_equals_null)
          | NestedLoopJoinExec(left, right, filter, type, projection)        // #190: the projection passed
          | ProjectionExec(CrossJoinExec(left, right), projection)            // #207
    stream = plan.execute(0, ctx)
probe_and_fetch(batch):
    tx.send(batch); chunks = poll stream.next() until Pending
    LeftSemi, LeftAnti, LeftMark: return None   // DataFusion yields a zero-row chunk per probe batch
                                                // for these; the device answers handle 0, and the
                                                // harness tells no batch from a zero-row one
    others: return one batch: concat_batches(chunks), new_empty(schema) when none   // #220, #208
finish_and_fetch():
    drop(tx); chunks = poll stream.next() until end
    return concat as above, for the finishing types; nothing otherwise
```
- The build is hashed once, where today every probe call rebuilds it.
- DataFusion's own join streams already have the device's shape — per-probe output, then the
  finish's emission — so the per-call join, the key project, the finish join and the pad project
  of today's `Calls` all go, and with them the accumulated keys.
- Every type, with or without a residual, is DataFusion's own operator (#153, #159, #160).
- "Poll until `Pending`" drains one probe batch only because the build is already in memory and
  the stream is one partition with nothing spawned beneath it; both hold by construction here.
- `owes_nothing()` reads the type and the empty build, as the device's does.

### 4.5 Driver (`executor/driver/`)

- `LaneCall::NoBuild` calls `set_build(None)`; `without_build`'s refusal path goes. When
  `owes_nothing()`, the lane drains its probe batches without calls, as `Draining` does today.
- `feeds_owing_build` (`index.rs`) and the scatter-drop exception (`partitioned.rs`) go: a
  zero-row scatter output is dropped everywhere, and a lane that got no build batch still has
  its session.
- The mock's `JoinRule.empty_build_owes_its_probe` (hacks-audit P8) goes.
- The probe-subtree hold (`scheduler.rs`) stays.

### 4.6 `scripts/exec_model`

`operators/recipe_join.py` and its recipe tests retire; the pandas oracle (`operators/joins.py`)
and `join_types.py`'s capability model follow 4.1. The two guard tests that grep `join.cpp` for
join types and cuDF calls are updated to the new file.

## 5. Testing

Counts are from master `fc3b0b55`. Every task keeps `build-test.md`'s counts and rows current.

### 5.0 Rules

- **A ticketed refusal is pinned by a `bug_` test.** Every test that asserts a refusal (or a
  wrong answer) tied to an open ticket is named `bug_…` and cites the ticket, so it goes red the
  day the ticket is fixed and is flipped then, not deleted: #243, #245, #246, #247, #249 and #250
  each get one at the lowest layer that shows them; a refusal no SQL reaches, with no ticket (a
  non-column key, a filter on the mark), is an ordinary test.
- **A pin flips, it is not deleted.** A `bug_` pin keeps its script and becomes the positive case
  (`bug_a_left_join_refuses_its_first_probe_batch_on_the_device` → `a_left_join_over_one_probe_batch_agrees`).
  A pin whose shape no longer exists (a refusal message, a recipe copy) is replaced by the case
  that asserts the new behaviour, named in the PR.
- **Red first where reachable.** A new case for a defect is written failing before its fix; a case
  for a path that does not exist yet (the session's arms) is written against the new symbol.
- **Lowest layer that can see it.** Arms and cuDF calls: gtests. Plan shape: planner tests and
  plan goldens. Lane protocol: driver tests over the mock. Both backends agreeing: the operator
  harness. Answers against SQL: corpus and pbench cells under the DuckDB oracle.
- **Hand-counted expectations** in gtests: tables of a few rows, every answer written out, NULLs
  and duplicates included.

### 5.1 duckdb-oracle

`duckdb-oracle.md` holds the detail. `duckdb_oracle` is the first oracle argument of every
`corpus_query!` line, explicit on each: `duckdb_exact`, `duckdb_approx`, `duckdb_divergent(<ticket>, <positions>)`,
`duckdb_fingerprint` (sections over the 256 KB cap), `duckdb_none` — and `duckdb_columns(<positions>)`
only if the first run finds a tied LIMIT window that needs it;
`DuckdbOracle::ALL` and its test; comparator cases per variant; `all_modes` as mode sugar.

### 5.2 pbench

- The generator is seeded and pinned to the DuckDB version `duckdb_result.py` uses; a test
  regenerates into a temp dir and compares row content (not parquet bytes) with the committed
  files.
- Every query lands registered, plan and result goldens written, cpu cells on where the cpu is
  right, every device cell off with its ticket, `duckdb_exact` unless a ticket says otherwise and
  `duckdb_none` where the cpu does not answer.
- `cost-report`'s widget renders the pbench section; its existing render test gains pbench.
- Queries include the Left/Full NULL-key pair (device cells off on #152), one per demonstrable
  in-scope ticket, and the shapes that collapse a join to one lane; the list is the pbench spec's.
- pbench plans with the small-table threshold at 0 (a per-dataset `small_table_bytes`; tpch and
  tpcds keep 5 MB), so `fact` splits across lanes at tp4 and the one-row-group tables leave three
  lanes empty. A test that the override reaches pbench's plans and no other dataset's.

### 5.3 repartition-keys

- #201 first: `murmur_conformance.rs` calls production `rows_per_lane` and compares lane by lane;
  its local `cpu_partition_ids` and `pmod` go, and `pmod_handles_negative_hashes` tests the
  production `pmod`.
- Then new live gates, each red before its kernel arm: Float32, Float64 (with -0.0, +0.0, NaN),
  Boolean, Timestamp in all four units, Decimal128 (15,2) and (38,4) and a decimal composite,
  plus the gaps the survey found: Int8, a zero-row input, an all-NULL key; and UInt8, UInt16,
  UInt32, UInt64 (review row 11): one rule on both engines — u8/u16 cast to i32, u32 to i64, u64
  reinterpreted as i64 bits — before comet on the cpu and in the kernel's normalizing switch.
- `emit_cases.rs`: the #206 (float, boolean) and #95 (decimal) pins flip. The operator harness
  builds its batches in memory (`tests/synthetic.rs`, a seeded splitmix64 generator; `emit_cases.rs`'s
  `with_key(ArrayRef)`), not from parquet, so every key type gets a case of its own there, both
  backends placing every row in the same lane: Int8, Int16, Int32, Int64, Float32, Float64 (each
  with -0.0, 0.0, NaN, -NaN and NULL), Boolean, Date32, Timestamp in all four units, Decimal128 at p ≤ 18
  and p > 18, Utf8, the four unsigned widths, and a composite of mixed types; `emit_schema_cases.rs` the same types held as
  declared. `synthetic.rs` gains `key_types(rows, seed)`, one column per hashable type with those
  special values, so the join cases below reuse it.
- #243 (open, not fixed in this chain): two `bug_` pins in the operator harness, cpu against
  device over keys holding `-0.0`, `0.0`, NaN and `-NaN` — a float-keyed `GpuAggregate` (the cpu
  six groups, the device four) and a float-keyed Inner `GpuHashJoin` over one probe batch (the cpu
  without the `-0.0`/`0.0` and `NaN`/`-NaN` pairs). Each asserts the divergence, so it goes red
  when #243 is fixed. pbench's float rows are commented out on #243 (`pbench.md`).
- #189: a planner test that a rollup's tp4 shuffle hashes no grouping id; the 15 cpu cells of
  tpch rollup_over_join and tpcds q5, q18, q22, q80 turn on (gpu cells stay on other tickets).
- Every NaN float key is canonicalized before hashing on both engines, so `NaN` and `-NaN` share
  a lane: a live gate holds both. The fbs `DataType` gains the four timestamp variants; a cast to
  `Timestamp(Second)` serializes and renders, and pbench's `timestamp-s-key-group` is runnable.
- Decimals hash the 16 bytes of the unscaled value on both engines (the cpu casts to
  `Decimal128(38, s)` before comet): the decimal gates prove it against `rows_per_lane`. No wire
  change, so no payload golden moves.

### 5.4 refcounted-scatter

- gtests: a scatter's N handles share its column owners (release N−1, read the survivor); the N outputs
  concatenate back to the input; `out_stats` per partition unchanged; a child of two handles is
  refused (#197). Peak is checked with an RMM statistics adaptor: input + one partitioned table
  during the call, the partitioned table after it.
- No golden moves; benchmark timings for p1..N−1 drop, which the PR notes.

### 5.5 exit-copies

- One gtest per operator family (aggregate, filter, project, window, expr's `ColumnRef`) under an
  RMM statistics adaptor: bytes allocated by the call equal the output's, not twice it. Every
  existing test stays green unchanged — answers do not move.

### 5.6 join-session-cpp — `cpp/tests/gpu/test_join_session.cpp`, through the four symbols

The matrix, each cell a hand-counted case:

| dimension | values |
|---|---|
| type | the nine |
| condition | keys only; keys + cross residual (AST); keys + cross residual (non-AST after hoisting); keys + preserved-side-only residual (semi family); no keys + AST; no keys + AST and non-AST conjuncts; no keys + nothing AST-able; cross |
| keys | NULL on both sides under UNEQUAL and EQUAL; duplicates on both sides (many-to-many); a composite key with a NULL in the second column |
| build | no batch; zero rows; rows |
| probe | no batch; one; three with a zero-row batch between |
| projection | none; crossing sides; zero kept columns (`__rowmarker__`) |

Not the full product: every type × condition × {no batch, zero rows, rows} build, and each other
dimension against every type at least once. Named cases besides:
- Left and Full with NULL keys on both sides under UNEQUAL: the padded rows (the latent finish
  defect).
- LeftSemi over many-to-many keys: each build row once; RightSemi with duplicate build keys.
- Full never re-emits an unmatched build row across three probe batches (no `full_join`).
- The pairs path and the cross chunk loop forced to chunk by a small budget hook: same answer as
  unchunked.
- Cross: a zero-row side, the `size_type` overflow refusal, a zero-column side refused.
- Padded column types: Int32, Int64, Utf8, Date32, Decimal128 at two scales, Boolean — each pad
  equal in type and scale to the matched rows.
- Lifecycle: release without finish frees everything; `end_plan` frees a live session; an
  unknown join id is refused; a probe after finish is refused.
- From the validation: a keyed LeftSemi whose cross residual is not AST-able (a decimal
  comparison, as `wire/gpu_tests/mod.rs:674` `SEMI_JOIN`); LeftAnti and LeftMark whose
  preserved-side condition is NULL keep the row and mark `false`; RightSemi/RightAnti over a build
  whose keys are all NULL under UNEQUAL; a NULL probe key against `hash_join(Bk, cmp)` (which
  `nullable_join` the two-argument constructor assumes is unspecified — verify it does not throw);
  a present empty projection against an absent one; a predicate-free Left, Right and Full nested
  loop over an empty side; LeftAnti and LeftMark with a residual, and a Left nested loop, over no
  probe batch (`architecture.md`'s "Zero-row batches change no answer" breaks).
- `join.cpp`'s #154 sites: one allocation check as in 5.5.
- The binary is in `install(TARGETS)` and the rpath list (`cpp/CMakeLists.txt:321-327`), since CI
  and shad-gpu run only installed `peacock_*_tests`; its `main()` installs the RMM pool, as
  `test_plan_executor.cpp:2114` does. A semi join with several cross conjuncts (their AND built at
  the cuDF AST level, `NULL_LOGICAL_AND`); a pairs path over `size_type` refused by name; one
  allocation check on a session probe (#154's `join.cpp` sites).
CI compiles the file on both legs (25.02, 25.10a); verify-26.02 runs it on 26.02.

### 5.7 join-backend

Pins flipping (56 join pins): #152 30, #59 10, #173 4, #190 4, #212 3, #207 2, #208 2, #215 2
(`nested_cases.rs:438` counts for #190 and #215). The NO_BUILD, PROBE_COPY, BUILD_COPY and NO_KEYS
message consts go with them.

New harness cases (`join_cases.rs`, `join_dimension_cases.rs`, `nested_cases.rs`, both backends,
in-memory batches): every join shape pbench shows at the query level also gets a node-level
case, so a failure is found at the lowest layer first —
- NULL keys on both sides for all nine types, under the SQL default and `null_equals_null`
  (Left and Full are missing today);
- many-to-many keys for the semi family; a preserved-side condition that is NULL (D3);
- each key type of `key_types` as a join key, Inner and LeftSemi at least — floats holding -0.0
  and NaN excepted: there the cpu keys by bits and the device by value (#243), pinned by
  repartition-keys' `bug_` cases, not asserted equal here;
- lanes with no build batch and with no probe batch for every type (the sparse `tiny` shapes);
- a predicate-free Left, Right and Full nested loop over an empty side; a keyed semi with a
  non-AST cross residual;
and, from the tickets:
Left, Right, Full with a residual (#153); RightSemi, RightAnti with a residual (#159); the nested
loop's seven other types (#160); a Left nested loop over a non-AST predicate (#215); a cross join
whose projection keeps no column (#63); Left and Full with NULL keys on both sides; three probe
batches for every type; an empty build under each `owes_nothing` type ends the lane without a
probe call.

Planner:
- `join_refusals.rs`: the five join refusals become "plans and answers" tests.
- `join_capability.rs`: the matrix is rewritten — every type streams, `needs_finish` for Left,
  Full and the build-side semi family, no probe-side `GpuCoalesceAllBatches`; the nulls.rs tests
  (:309, :376) flip to planning, except the off-spine `IN`/`NOT IN` forms 3.5 still refuses; `translator/tests.rs:542`, `plan/tests/joins.rs:159, :190` follow.
- `null_analysis.rs` moves with `planner/nullability.rs`, unchanged.
- The `NOT IN` rewrite: the four forms of the scratch probe (correlated and uncorrelated, top
  level and under `OR`, over `o(w,x)`, `s(z,y)` with NULLs) as planner tests asserting the plan
  shape, and their cpu answers asserted equal to DuckDB's (3 and 2 rows).
- The rewrite is skipped where the data holds no NULL: tpch q16 and anti-join plan as today;
  pbench's `NOT IN` queries plan the rewrite.
- #137: per type, the `IS NOT NULL` filter on exactly the side 4.1 names, only on a shuffled
  side, and not at all where `can_be_null` says the key holds none. tpch's plans do not move;
  about 73 tpcds plans gain the filter at the tp4 modes, with their cpu, cost and memory goldens,
  regenerated and reviewed as one diff; pbench's show it.
- `plan/tests/joins.rs:253` (a streaming Left nested loop names its fix) flips; 
  `memory_estimation/tests.rs:217` (accumulated keys) becomes the 4.3 pricing.
- A predicate-free non-Inner nested loop plans as `GpuNestedLoopJoin` over `true`, Inner as
  `GpuCrossJoin`.
- An `EmptyExec` plans as `GpuEmpty`, and a Left join over it pads every row on both engines;
  `IS NOT DISTINCT FROM` as a join condition plans as a hash join with `null_equals_null = true`
  (and stays a nested loop, answering through `NULL_EQUAL`, when a `=` key sits beside it); a
  harness case per arm of `NULL_EQUAL` (both NULL, one NULL, equal, unequal) on the AST and the
  column path.
- `__rowmarker__`: a zero-column project plans as the one-literal project; a cross join of two
  such sides keeps one placeholder; validation refuses a zero-column schema. tpcds's 19 such nodes
  move in every mode's plan golden and in `recipe-payloads.txt`.
- **The estimate.** `llm-wiki/reports/join-rewrite-cell-estimate.md` (the analyst's prediction,
  committed with the chain's specs) says which corpus cells each task flips and which then meet
  another issue. join-backend's completeness record compares it with the cells that actually
  turned on: each miss, either way, gets a line with its cause, and a cell that met an issue the
  estimate did not name gets its ticket.

Wire and goldens: `wire/tests.rs`'s 12 join-recipe tests are replaced by `CudfJoin`
serialization tests (one per plan node, schemas and projection carried);
`wire/gpu_tests/mod.rs`'s join walker and `driven()` list follow; plan goldens' join recipe
lines (75 per tpch mode, 656 per tpcds mode) become the session line; `recipe-payloads.txt` loses
its 134 join recipe lines and gains `CudfJoin` payloads.

Executors and driver:
- `cpu_backend/tests/join.rs` asserts the stream shape: one batch per call (#220), the finish
  emission, the projection on nested loop and cross (#190, #207), one zero-row batch over a
  zero-row build (#208); its `Calls`-split tests go.
- `gpu_backend/gpu_tests/join.rs`: the session lifecycle, `Drop` releasing, `owes_nothing`.
- The harness: `tests/gpu_tests/script.rs:265-281` calls `set_build(None)` for no build and takes
  `Option` slots; `join_dimension_cases.rs`'s `crossing_projection` gains real Left and Full arms;
  `nested_with` gives Left a multi-batch probe.
- `cpp/tests/gpu/test_plan_executor.cpp`'s four `CudfHashJoin` tests (:375, :427, :609, :1054)
  move to the session symbols, since `CudfJoin` has no `execute_node` path.
- `wire/gpu_tests/mod.rs`'s `SEMI_JOIN` (:674) stays, as the device's regression for the
  non-AST semi residual.
- Driver: `empty_build_owes_its_probe` goes from the mock and its 8 `flow.rs` tests become
  `set_build(None)` tests; `index/tests.rs:178` (`feeds_owing_build`) goes;
  `single_partition/tests.rs:300` expects `set_build(None)`. `dimensions.rs:125-221` is rewritten:
  its helper reads `feeds_owing_build` and asserts an owing join never reaches `NoBuild`, both of
  which 4.5 reverses; its two queries (tpcds q93 Right, tpch anti-join over an empty build) stay,
  now asserting the lane reaches `set_build(None)` and answers like the oracle.
- `exec_model`: `recipe_join.py` and its four recipe-only tests go; the refusal pins and the
  #153 defect pin flip; the two `join.cpp` guards read the new file's calls.

Corpus: of the 87 rows whose only tickets are in this chain, every cell turns on as its last
ticket closes — up to 23 cpu and 428 gpu cells — each run on shad-gpu at its mode and compared
with DuckDB. The 20 rows that also carry an out-of-chain ticket (#55, #56, #57, #65, #191, #199)
keep those cells off and drop the in-chain tickets from their tags. **#183 is not one of them**:
`stale-cells` ran the sixteen cells it still held off and every one passed, so a row carrying
`183` alone has no live blocker and its cells are join-backend's to run — which is what
`join-backend.md`'s own Registry section already says. pbench's join cells
turn on with them.

### 5.8 verify-26.02

On shad-gpu's 26.02 environment (a temporary host only as the fallback): every tier `build-test.md` lists, with its counts recorded beside the
25.02 run's; fixes land with their own red case; then the corpus benchmark, on 26.02 and 25.02,
for the cells this chain turned on that it times — the tpch sf40 cases (tpcds and pbench have no
sf40 data) — written under `benchmark-results/cudf-<version>/`.

## 6. Join shapes after chain J

The chain-J review (2026-10-07) enumerated 29 join shapes; their state once the chain lands,
with the decisions taken since. Tables from pbench.

- **Fixed in the chain:**
  - `NOT (x IN S)`, alone and under `OR`, and a `NOT IN` inside a subquery (rows 1, 2, 5): the
    rewrite works the AND/OR spine of every `Filter`, `Not(InSubquery)` normalized (§3.5);
  - more than 2³¹ key-match pairs in a call (14): refused by name (`fits_or_throw`);
  - the semi family with several cross conjuncts (27): ANDed at the AST level, `hj` guarded;
  - a side DataFusion folds to `EmptyExec` (8): `GpuEmpty` (§4.1), pbench `empty-side-left-join`;
  - `IS [NOT] DISTINCT FROM` as a join condition (9): `NULL_EQUAL` on the device and a null-equal
    hash key in the plan (§4.1), pbench `indf-full-join`;
  - unsigned keys across a shuffle (11): one widening rule on both engines (repartition-keys),
    pbench `uint-key-group`, `uint-key-join`;
  - float keys' lane split (part of 6): every NaN canonicalized before hashing (repartition-keys);
  - timestamp-keyed joins and LeftMark with a cross residual (28, 29): now tested, pbench
    `ts-key-join`, `mark-cross-residual`;
  - `NOT IN` under `NOT` (3): the rule's negation normal form folds `NOT (x NOT IN S)` to
    `x IN S`, a semi join, pbench `not-not-in` and `not-or-not-in`.
- **Refused at plan time, safely** (never a wrong answer):
  - `IN` read as a boolean (`IS NULL`, `= false`) over nullable data (4): the narrowed refusal,
    [#250](../tickets/joins.md#t250), pbench `in-is-null`;
  - a `count(*)` DataFusion answers from statistics as a join side (7): #158;
  - a time, interval or nested column carried through a join (10): an unmapped wire type is a
    `PlanError`, never a `Null` pad — [#249](../tickets/complete-coverage.md#t249), pbench
    `interval-through-join`, `struct-through-join`;
  - a key that is not a bare column, a filter reading the mark (23), SortMergeJoin and
    SymmetricHashJoin (22): unreachable from SQL under this engine's configuration.
- **Ticketed, open:** float keys' equality on the cpu (6) — [#243](../tickets/joins.md#t243); a
  nested-type key across a shuffle (12) — [#245](../tickets/joins.md#t245), pbench
  `struct-key-join`; `LIKE` against a column (13) — [#246](../tickets/joins.md#t246), pbench
  `like-column-pattern`; `CollectLeft` merged to one lane, a Full join's NULL-key skew, a keyless
  condition outside the hoistable form (24, 25, 26) — [#248](../tickets/performance.md#t248), with
  [#140](../tickets/optimizer.md#t140) for the broadcast; a wire type for time, interval and nested
  columns (10) — [#249](../tickets/complete-coverage.md#t249); `IN` read as a value (4) —
  [#250](../tickets/joins.md#t250).
- **DataFusion 45's own limits** (15–21) — [#247](../tickets/df-upgrade.md#t247): uncorrelated
  `EXISTS`, `IN` as a projected value, a tuple `IN`, `ANY`/`ALL`, `LATERAL`, a correlation under a
  `LIMIT`/union/window, a scalar subquery returning several rows.
