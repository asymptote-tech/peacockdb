# aggregate-cases — run record

Chain `ENS-join-cases`, task 2. Branch `ENS-aggregate-cases` off `ENS-join-cases`; PR targets
`ENS-join-cases`.

## Handoff from join-cases — 2026-09-16

The cpu executors refuse `Utf8` data under a `Utf8View` declaration everywhere but the unload
(`cpu_backend/mod.rs` `declared_as` → `try_new`, which the aggregate path reaches at `mod.rs`
and `accumulate.rs`). So the spec's `Utf8View` group-key row cannot be read with `.same()`:
join-cases read such cases on the device under the declaration against the cpu on `Utf8` over
the same strings, asserting the cpu's refusal so a harness that closes the gap turns them red
(`device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key` in
`gpu_tests/join_dimension_cases.rs`; the gap itself in `join-cases-detail.md`). Take the same
reading here unless a `run_both` that casts the cpu's upload to the leaf's declared types has
landed. The #183 pin asserts the exported type in the slot, as the four operator pins do.

Superseded on 2026-09-16: the amended specs (master `c6b7f63c`) declare no view type, the
`Utf8View` group-key row is gone, and join-cases' Task 8 deleted the device-only helper and
the four operator pins named above. The harness gap itself is still true and still recorded in
`join-cases-detail.md`; nothing here reads a case through it any more.

## Dispatch 1 — 2026-09-16

- Branch `ENS-aggregate-cases` forked from `ENS-join-cases` at `6703c08f` (task 1 done, PR
  #155 green). PR targets `ENS-join-cases`.
- Hosts at dispatch: **verda down** for us (`Permission denied (publickey)`, as for task 1), so
  the rust-only check runs locally with the main checkout's sf1 linked in and unlinked after.
  **shad-gpu up**, a neighbour holding ~60 GiB of 144; the gpu lib binary's 1 GiB fits.
- Routing: the developer works `aggregate-cases-impl.md` task by task, one shad-gpu device
  cycle per family, foreground calls (`--build`, `--push-binaries --patch`, `--run`), from
  this worktree's own `target-cudf-rapids-cuda-12.2` (warm from task 1). Handoff run: the whole
  `gpu_tests::` rung, 330 on the base.
- The known-wrong table `build-test.md` is asked for does not exist on this base; the
  `bug_` register goes in this file, as task 1 did.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, harness gaps)

### 2026-09-16, the cases written before the first device cycle

- Plan drift, the key projected away: `finalize_columns` (`plan/aggregate.rs`) projects
  every group key through by position and `AggregateBody::finalize` says a key "is not
  finalized and is not here", so an aggregate's finalize cannot drop a key and the plan's
  `a_sum_grouped_on_a_declared_utf8view_key_agrees_past_the_key` cannot be built. Under the
  handoff's reading the key leaves on the harness side instead: the oracle helper in
  `aggregate_dimension_cases.rs` drops the key column from both engines' slots before
  `assert_same`, so the aggregate is what the comparison reads and the exported type is read
  by the pin alone. Five cases take it — the init and the merge on `s`, and on `(key, s)`,
  the spec's "two columns (`Int32` and `Utf8View`)" — with a plain `(key, b)` pair beside
  them so the two-column path is also read on both engines directly.
- Plan drift, `count(1)`: DataFusion rewrites `count(*)` to `count(Int64(1))` (every golden's
  `count(1) as count(*)`), so the spec's `count(*)` and `count(1)` are one IR shape and get one
  case per path. `count(col)` over nulls grouped and global is task 9's
  `a_grouped_sum_min_max_count_agree` (`count(i32)`) and the global `count(s)`; the case added
  is the shape those do not answer — `count(key)` grouped on `key`, whose null group counts
  nothing.
- Plan drift, the keyless `Count` merge: no plan merges a count by `Count` — the translator's
  own test asserts the merge is `Sum` — so the keyless count merge is `Sum` over `count(*)`,
  as `a_count_merges_by_sum` reads it grouped.
- The dispersion finalize is `plan::finalize` over `resolve("stddev")` / `resolve("var")`,
  the planner's own expression, rather than the plan's hand copy: the goldens render it as
  `CASE WHEN (CAST(count AS Float64) - 1) <= 0 THEN NULL ELSE sqrt(m2 / (CAST(count AS
  Float64) - 1)) END`, the count cast up before the subtraction, which the hand copy had
  the other way round.
- #56's shape is a CASE over a *string equality* inside the sum, not over a boolean column,
  so the CASE case takes `s = 'beta'`.
- The nullable descending merge key is `key DESC NULLS FIRST` — DataFusion's default for
  `DESC`, the corpus's 24 merges — beside the ascending nulls-first and nulls-last forms,
  since #202's mapping shows only on a descending key with nulls.
- `aggregate_dimension_cases.rs` is the sibling file, as `join_dimension_cases.rs` was:
  `aggregate_cases.rs` keeps task 9's cases and the builders, now `pub(crate)`, with the
  parameterised siblings beside the originals (`state_by`/`init_by`, `state_cut`,
  `merge_by`, `welford_state_by`, `welford_merge_by`, `welford_answered_by`,
  `same_within_welford`). `exec_cases.rs` (936 lines) and `accumulate_cases.rs` (508) take
  their rows in place.

### The device cycles

Four cycles, each `--build`, `--push-binaries --patch`, `--run` in the foreground under
`timeout 590`, from this worktree's warm `target-cudf-rapids-cuda-12.2` (a relink, ~30 s).
Logs under `/tmp/aggregate-cases/` on dev for this session only. No `[rmm]` line in any run.

1. `tests::gpu_tests::aggregate`: 44 passed, 9 failed — the five declared-`Utf8View` key
   cases and the pin (the cpu panics, below), the global Welford init and keyless merge (one
   column where three were expected), the two global finalizes (the device's refusal).
2. `tests::gpu_tests::` (the operator harness alone, after the `Utf8View` cases became `Utf8`
   ones and a probe test printed the device's global Welford answers): 332 passed, 11 failed
   — the four global Welford cases, `fetch 0`, text to date, `date_part`, `ILIKE`, and both
   `DESC NULLS FIRST` merges.
3. `tests::gpu_tests::`: 341 passed, 1 failed — the keyless merge pin asserted one slot
   where a merge answers three.
4. `tests::gpu_tests::`: `342 passed; 0 failed`. Then the handoff run over the rung,
   `PCK_TEST_FILTER='gpu_tests::'`: `test result: ok. 397 passed; 0 failed; 0 ignored;
   0 measured; 538 filtered out; finished in 18.64s`. `test_gpu_corpus` ran 0 tests under
   that filter, as every cycle; nothing here touches it.

The operator harness is 342 (275 + 67); the rung is 397 (330 + 67); the page's counts move by
67. The spec said roughly 45; the 67 are one per item the matrix names, none answering a
question another answers — the matrix's own rows sum to that many once each key type is read
at the init and the merge and each sort shape on both accumulating nodes.

### Harness gap: the cpu's aggregate panics on a declared `Utf8View` key over `Utf8` data

The handoff's premise — the cpu refuses at `declared_as` → `try_new` — holds for the join,
the filter, the project and the accumulators, which is why join-cases could read the device
against the cpu's refusal. The aggregate never reaches its output check: DataFusion's
group-by consumes the key first, and its `Utf8View` group values assert on the array's type —
`datafusion-physical-expr-common-45.0.0/src/binary_view_map.rs:213`, `assertion failed:
matches!(values.data_type(), DataType::Utf8View)` on a single string key, and arrow's
`cast.rs:808: byte view array` on `(key, s)`. A panic, not an `Err`, and `run_both` drives
the cpu before it opens the device, so the device's half is never run and nothing can be
read off it: not the oracle comparison, not the #183 pin the dispatch asked for.

Consequences:

- The matrix's `Utf8View` group-key items — the init and the merge on `s`, on `(key, s)`,
  and the pin — have no case. `Utf8` over the same strings is what the harness can read, and
  it gets its own four green cases (`…grouped_on_a_string…`, `…on_an_int32_and_a_string…`,
  init and merge). The spec's constraint "every `Utf8View` group-key case but the one pin
  projects the key out" is moot, and would have been unbuildable anyway (`finalize_columns`
  above).
- No ticket: production data reaches the cpu's aggregate through DataFusion's parquet reader
  under the declared schema, and every cpu producer's output is held to its declaration by
  `declared_as`, so no cpu operator can hand a `Utf8` array to a `Utf8View`-keyed aggregate.
  This is the harness feeding one type under a declaration of another, the join-cases gap in
  its panicking form.
- Closing it is a harness change outside "no new mechanism": a `run_both` that drives the
  device when the cpu panics (or a device-only driver), or an upload that casts to the leaf's
  declared types, which the device cannot take. Either lets the five cases and the pin be
  written as the spec asked.

### Findings, one per divergence

- **#216 (new)** — the device's keyless aggregate has no Welford arm. `aggregate.cpp`'s
  `key_cols.empty()` path reduces every `stddev` name with `make_std_aggregation`, phase
  regardless: the global init answers one `Float64` column `stddev(f64)` holding the sample
  stddev (150.095… where the cpu's triple is count 60, mean 20.954…, m2 1329186.4…); the
  keyless merge reduces over the state's first column, the count, and answers 0.0 over
  partials of count 1; the finalize project above the merge then refuses, `ColumnRef index 2
  out of range (cols=1)`. Four pins. The grouped path is right on all of it: the grouped
  stddev and var finalizes agree to `WELFORD_RELATIVE`, with the planner's own expression.
- **#217 (new)** — `GpuSort` with `fetch 0` keeps every row on the device: `sort.cpp` slices
  under `fetch() > 0`, the wire's -1 meaning none. The cpu slices to zero rows.
- **#218 (new)** — a `Utf8` → `Date32` cast is refused on the device: `cudf::cast` on a
  string column, "Column type must be numeric or chrono or decimal32/64/128". The mirror
  of #203.
- **#219 (new)** — `ILIKE` is a case-sensitive `LIKE` on the device: the wire carries
  `case_insensitive`, `expr.cpp`'s LIKE arm reads `negated` alone.
- **#202, second site** — the merge in `node_session.cpp` on `key DESC NULLS FIRST`. Over
  runs dealt round-robin, each carrying a null key, the device answers *duplicated and
  dropped* rows (ids 18, 39 and 45 three times each; 14, 17, 19, 22, 32, 43 gone) — cuDF's
  merge precondition is broken for its comparator, so the output is not a permutation and no
  expected batch can state it. The pins use forty-four rows in eleven runs, which puts every
  null key (rows 10, 21, 32, 43) in the last run alone: each run is then sorted under either
  null order, the device answers a permutation, and the wrong one is exactly the rows ordered
  with the nulls last. Added to the ticket's text.
- **#191** — `date_part('year', d)` comes back `Int16` where the plan declares `Int32`, the
  values the cpu's. Pinned at the project, and the ticket now names the pin.
- **#203** — a `Date32` → `Utf8` cast is the same refusal as the integer's; the pin cites it.
- **#187** — a cast to `Decimal128(20, 0)` is exported at precision 38, the values the cpu's.
- **#163** — nothing new: the keyless Welford count never reaches the export (#216 is in the
  way), so the global count pins the plan expected are #216's instead.

Findings the matrix's right column allowed for and did not land: #56's shape,
`sum(CASE WHEN s = 'beta' THEN i64 ELSE 0 END)` grouped, agrees — the aggregate's argument
goes through `build_column`, and CASE takes the column path there, so the ticket's "AST for a
string comparand" is not what runs today; the ticket may be closeable by a query-level check
on q2, which this task does not make. #180's shape (a merged `count(*)` declared
non-nullable) is not reachable here: `columns` declares every field nullable. #199's
zero-row neighbour, a keyless sum merge over two zero-row arrivals, agrees (both answer
the identity row). #55 was not reached: the decimal cast case lands on #187 before any
divisor cast. `round(f64, 1)` agrees bit for bit; `concat` over a null agrees (both treat it
as empty); `substr`, `coalesce`, `lower`, `NOT LIKE`, the typed-NULL CASE branch and the
string literal column all agree; every `GpuSort` shape but `fetch 0` agrees; every ascending
nullable and two-key merge agrees on both nodes, and four lanes do.

### The `bug_` register (the known-wrong table does not exist on this base)

| case | file | asserts | ticket |
|---|---|---|---|
| `bug_a_global_welford_init_answers_a_finished_stddev_on_the_device` | aggregate_dimension_cases | one `Float64` column `stddev(f64)`, its value `sqrt(m2 / (count - 1))` of the cpu's state to `WELFORD_RELATIVE` | #216 |
| `bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device` | aggregate_dimension_cases | the done slot is one `Float64` column holding 0.0; the cpu merges the triple | #216 |
| `bug_a_global_stddev_finalize_is_refused_on_the_device` | aggregate_dimension_cases | `ColumnRef index 2 out of range (cols=1)` | #216 |
| `bug_a_global_var_finalize_is_refused_on_the_device` | aggregate_dimension_cases | the same | #216 |
| `bug_a_cast_to_decimal_is_exported_at_precision_38` | exec_cases | the cpu's values at `Decimal128(38, 0)` | #187 |
| `bug_a_date_cast_to_text_is_refused_on_the_device` | exec_cases | `cast to STRING from a non-string type not supported in column path` | #203 |
| `bug_a_text_cast_to_date_is_refused_on_the_device` | exec_cases | `Column type must be numeric or chrono or decimal32/64/128` | #218 |
| `bug_a_year_extracted_from_a_date_is_exported_as_int16` | exec_cases | the cpu's years at `Int16` | #191 |
| `bug_ilike_is_case_sensitive_on_the_device` | exec_cases | false on every word, null where `s` is | #219 |
| `bug_a_fetch_of_zero_keeps_every_row_on_the_device` | exec_cases | cpu zero rows, device the whole batch | #217 |
| `bug_an_accumulating_sort_on_a_descending_key_nulls_first_puts_them_last_on_the_device` | accumulate_cases | cpu the rows nulls first, device the rows nulls last, eleven runs | #202 |
| `bug_a_merge_on_a_descending_key_nulls_first_puts_them_last_on_the_device` | accumulate_cases | the same through eleven lanes | #202 |
| `bug_a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_duplicates_and_drops_rows_on_the_device` | accumulate_cases | three lanes of 48 rows dealt round-robin: the 48 ids the device answered, three of them thrice and six never (round 1) | #202 |
| `bug_a_sum_grouped_on_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device` | aggregate_dimension_cases | the device alone through `run_gpu`: slot 0 column 0 is `Utf8` (round 1) | #183 |

### The rust-only proof

With the main checkout's sf1 linked in (`testdata/{tpch,tpcds}.sf1`) and unlinked after —
`git status` clean of them: `timeout 1800 cargo test --features rust-only -p peacockdb-core
--lib -- --test-threads=2` → `test result: ok. 533 passed; 0 failed; 2 ignored; 0 measured;
0 filtered out; finished in 192.43s`, unchanged. `--test test_module_layout` → `17 passed;
0 failed`. The gpu lib binary compiles with no warning (`scripts/cargo-cudf.sh test -p
peacockdb-core --lib --features gpu --no-run`).

### For the reviewer

- The `Utf8View` group-key row is the one matrix item with no case, for the harness reason
  above; the dispatch's demand for a #183 pin at the aggregate cannot be met without a
  harness change. The coordinator's call.
- `build-test.md`'s harness row says so in two sentences; the counts move by 67 (275 → 342,
  330 → 397, 338 → 405, 1873 → 1940, Rust 1437 → 1504).
- `exec_cases.rs` is 936 lines after this task, under the rule but close; the next row of
  project cases wants a sibling file.
- The 11-run merge pins are the one place a fixture's shape is chosen for the device's
  comparator rather than the plan's; the doc comment on `null_keys_in_one_run` says why, and
  the garbage the ordinary shape produces is in #202's text.

## Coordinator — 2026-09-16, to reviewing

Committed as `601c308f`, PR #156 against `ENS-join-cases` (base verified, 2 commits). Reviewer
round 1 dispatched. For the reviewer and the analyst: the `Utf8View` group-key row (and its
#183 pin) has no case — the developer reports DataFusion's group-by panics on `Utf8` data under
the declaration before `run_both` reads the device, so the join-cases reading does not carry
over; recorded above as a harness gap. Whether that row can still be answered inside the task's
constraints is the first question of the review. 67 cases against the spec's "roughly 45".

## Review round 1 — 2026-09-16

Reviewer: 67 cases counted, counts and tickets within caps, the refactored builders reduce to
the same nodes and expectations, the panic claim verified from DataFusion's sources. Findings,
routed to the developer (1–4, 7) and the coordinator (5, 6):

1. blocking — the `Utf8View` group-key row and its #183 pin are absent, and a reading exists
   inside the constraints: the device half of `run_both` is `drive::<GpuBackend>` over
   `Device`, both already in the harness; only `drive`'s private visibility stands between a
   case and the device alone. Shape: `pub(crate) fn drive` (or a `run_gpu` that `run_both`
   delegates to), a case-local `device_alone(node, script)`, the five cases read as
   `join_dimension_cases.rs` reads its declared-key cases — the device on the `Utf8View` key
   against `run_both(<same node on Utf8>).cpu`, column 0 dropped from both slots — and the pin
   as `exported_type` of the device's column 0. Coordinator's ruling: making an existing
   harness function `pub(crate)` is not a new mechanism; a `catch_unwind` alone answers nothing.
2. important — `a_sum_grouped_on_an_int64_agrees` groups on unique `id` (64 singleton groups),
   and the `Date32` init and merge fold only their nulls: an aggregate that never folds two equal
   non-null keys passes. Feed every key twice (a second `synthetic` with the same ids;
   `cut(1), cut(1)` for the date merge).
3. nit — three cases answer answered questions: the `(key, b)` init and merge (the `(key, s)`
   pair reads the two-column path `.same()`), and the keyless count merge (a `Sum` over `id`
   with a count's name). Drop them; say in the arm's comment that a count merges by `Sum`.
4. nit — #202's amendment ("duplicated and dropped rows" over runs each carrying a null) has no
   pin; and `null_keys_in_one_run`'s doc gives the accumulating sort a reason that is the
   partition merge's alone (the device re-sorts each batch there).
5. nit — `build-test.md`'s harness prose: "group keys of every type the corpus uses" false
   while the `Utf8View` key has no case; "the project expressions and casts with no case"
   describes the past. Coordinator's, after round 2.
6. nit — #218 sits under "Blockers for disabled coverage" though it blocks nothing. Coordinator's.
7. nit — `same_within_welford(.., keys: usize, ..)` reads only `keys == 1`; a `bool` or an assert.

### Review round 1, addressed

1. The `Utf8View` group-key row. `run_gpu(node, &script)` in `gpu_tests/script.rs` is the
   device half of `run_both`, which now delegates to it. The oracle helper in
   `aggregate_dimension_cases.rs` runs the declared node through `run_gpu`, asserts under
   `catch_unwind` that `run_both` on the same node still panics (the cpu's group-by; a harness
   that closes the gap turns the case red with "read this case with .same()"), runs the same
   node on `Utf8` through `run_both` for the cpu's answer, and drops the `Utf8View` key's
   column from both engines' slots before `assert_same` — column 0 on the single key, column 1
   on `(key, s)`, so `key` and the sum are what the two-column case compares. Four cases, the
   init and the merge on `s` and on `(key, s)`, all green on the device; the #183 pin reads
   the device's slot 0 column 0 through `run_gpu` and is `Utf8`. The harness-gap section
   above stands as the account of why the cpu is not read; its "has no case" consequences no
   longer hold, and `build-test.md`'s "has no case" sentence is the coordinator's to drop.
2. Keys folded: the date and `id` inits take `input_twice()` — `input()` and `synthetic(64, 2)`
   concatenated, every `id` and nearly every date twice with a different `i64` — and the date
   merge takes `cut(1), cut(1)`. Green on both.
3. Dropped: the `(key, b)` init and merge and the keyless count merge; the merge-arm comment
   says a count merges by `Sum`, so the keyless sum case is its merge.
4. The 3-run shape pinned as `bug_a_merge_on_a_descending_key_nulls_first_over_runs_each_
   carrying_a_null_duplicates_and_drops_rows_on_the_device`: the 48 ids the device answered
   in cycle 2, as a `take` over `synthetic(48, 1)`; deterministic — green on the first run.
   `null_keys_in_one_run`'s doc now gives the accumulating sort its own reason (the device
   re-sorts each batch before the merge, so any run shape holds; it takes the same runs for
   the same expected batch).
7. `same_within_welford` takes `keyed: bool`.

Proof: `--build`, `--push-binaries --patch` (once retried: `Connection closed by
89.169.176.82 port 22` mid-rsync, the flaky link, not the host), `PCK_TEST_FILTER='gpu_tests::'
… --run` → `test result: ok. 400 passed; 0 failed; 0 ignored; 0 measured; 538 filtered out;
finished in 19.20s` — 397 − 3 + 5 + 1. Locally, sf1 linked in and out: rust-only `--lib` →
`533 passed; 0 failed; 2 ignored`; `--test test_module_layout` → `17 passed`. The gpu lib
compiles with no warning. `build-test.md` counts moved by +3 (345, 400, 408, 1943, Rust
1507). `aggregate_dimension_cases.rs` is 612 lines, `accumulate_cases.rs` 535.

### Review round 2, addressed

`input_twice()` is `input()` concatenated with itself — every `id` and every date twice, every
sum the row's doubled — where round 1's `synthetic(64, 2)` half drew fresh dates and folded
only the null group; the doc line says so. One device cycle (`--build`, `--push-binaries
--patch`, `--run`, foreground, `timeout 590` each), `PCK_TEST_FILTER='gpu_tests::'`: `test
result: ok. 400 passed; 0 failed; 0 ignored; 0 measured; 538 filtered out; finished in 3.77s`,
unchanged. Nothing outside the gpu rung moved; counts unchanged.

## Review round 2 — 2026-09-16

All round-1 findings closed as stated; one important remained — `input_twice` drew its second
batch from seed 2, so the date init still folded only nulls — fixed as `input()` twice and
re-proven on the device (400/400). #202's pin list gained the fourth pin (coordinator). Nothing
important outstanding: to `completing`.

## Completeness pass — 2026-09-16

Reviewer (what is wrong): 0 blocking, 2 important — #216 stated two consequences its own pins
contradict (the merge answers 0, the finalize above it indexes past; every shape is a refusal,
none reaches the unload), and `aggregate_cases.rs`'s Welford comment said the global init adds
nothing. Analyst (what is missing): 0 blocking, 4 important — #202's amendment stated as
production damage a shape no plan reaches (every run the merge sees was sorted by the same
mapping; the pin's content is that the two sites move together), the pin's comment likewise;
the spec's "4 lanes" for `GpuAccumulateBatchesAndSort` has no case and no line saying why;
#183's pin count and #187's pin list were stale; #56 carried nothing from the case that runs its
shape green. All six applied by the coordinator as markdown and comment-only edits.

`GpuAccumulateBatchesAndSort` at four lanes has no case on purpose: the node is lane-scoped and
one lane's stream by definition, so lane 0 of a four-lane input is the same call sequence as
one lane (`Script::lane()` is 0 for `Accumulate`). The four-lane case is the partition merge's.

`architecture.md` corrected on the analyst's list: "The aggregate sequence" (a node with no
finalize list emits state — except the device's keyless Welford, #216), "From node to seqs"
(every aggregate merges as state — with that one exception), the `CudfAggregate.mode` row, and
the three `fetch` sentences that now say the device's sort reads 0 as none (#217).

For the merger: PR #155 (`ENS-join-cases`) first, then #156, each retargeted to master; the
`bug_` registers of both tasks live only in the two detail files, which the archive deletes —
carry them into `build-test.md`'s known-wrong table when `declared-schemas` brings it, or
before archiving.

## Done — 2026-09-16

CI green on the head `8fb6f550`: both dataset-matrix legs, the 25.02 GPU build, GPU tests on
shad-gpu, cost report, S3 check all success. The chain's last task; nothing left to dispatch.
