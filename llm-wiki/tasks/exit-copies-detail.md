# exit-copies — run detail

## Dispatched (2026-10-09)

Branch `ENS-exit-copies`, forked off `ENS-stale-cells` at `acb1d82f`. Tasks 1–5 are `done`.

**What changed under this task's plan while it waited, and it is the one thing to read first.**
`exit-copies-impl.md` was written against a `TableResult` whose three vectors are public, and its
project and window steps assemble a handle by hand — `TableResult out;` then `out.owners.push_back`,
`out.columns.push_back`, `out.column_names.push_back`. refcounted-scatter's completeness pass found
that this would have quietly reopened [#164](../tickets/corpus-coverage.md#t164)'s name-count half,
which that task had just marked closed on `owning`'s check alone. So the check moved: **
`register_handle` is now the only path to a handle number and refuses a handle of no columns, or
whose names or owners do not number its columns.** Hand assembly through the public fields is still
the intended route and is why they are public — but the handle has to be complete before it is
registered. The plan carries a note saying so.

**What the spec expects, so a different outcome is a finding rather than a surprise.** Its §4
predicts 6 of the 11 sites lose their copy and 5 keep it — the four Welford struct members and one
merged-state child, whose shared form would cost more than it saves. It also allows any site to
keep its copy with a measured note. So the deliverable is eleven classed sites with reasons, not a
count.

**Deferred by the human host override**, which outranks the spec here: the Verification bar's sf40
q6 and q19 benchmark before and after cannot run — the sf40 dataset lives only on shad-gpu, which
has been down all run, and benchmark measurement is out of every task by the override. `expr.cpp`'s
`ColumnRef` arm is the site that bar exists to measure, worth about 46 of the 107 GB q19's lineitem
filter moves at sf40, so the saving will land unquantified at that scale. The RMM statistics
adaptor cases the Tests section asks for are gtest-scale and are **not** deferred; they are what
carries the claim instead.

## Round 1 — built (2026-10-09)

Developer round on `ENS-exit-copies` at `ad55df5b`. All tiers green, nothing deferred but the
sf40 benchmark the host override already took out. Seven new gtests, six copies removed, five
kept with reasons. The spec's §4 prediction held: 6 gone, 5 kept.

### The eleven sites, where they actually are

The spec's line numbers were written against an older file. Every `aggregate.cpp` site is one
line further down than the spec says, and `expr.cpp`'s arm is at **850, not 834**.

| site (pre-change) | kind | outcome |
|---|---|---|
| `expr.cpp` 850, `build_column`'s `ColumnRef` arm | a temporary that only needed a view | **gone** — `evaluate_column` returns `{nullptr, table.column(idx)}`; `build_column` is its owning wrapper (`.take()`) |
| `filter.cpp` 41 | ordinal subset of a fresh table | **gone** — `TableResult::owning(filtered)` then `.select(ordinals)` |
| `project.cpp` 44 | an input column kept in the output | **gone** — the `ColumnRef` arm pushes `input.owners[idx]` and `input.columns[idx]` |
| `window.cpp` 46 | an input column kept in the output | **gone** — `TableResult out = input;` then `out.with(col, name)` per window expr |
| `aggregate.cpp` 414 | a fresh grouping-set key table | **gone** — `for (auto& key : gk->release())`, row count read first |
| `aggregate.cpp` 760 | a fresh groupby key table | **gone** — `group_keys->release()`, after the stddev guard that reads its row count |
| `aggregate.cpp` 643, 645 (Merge arm) | the Welford state's mean and m2 | **kept** — `cudf::make_structs_column` owns its children, and these are the input's columns. A per-group state column at that |
| `aggregate.cpp` 679, 681 (Final arm) | the same two, in the Final arm | **kept** — same reason |
| `aggregate.cpp` 772 | a child of the merged MERGE_M2 struct | **kept** — up to three builds read one struct, a child each; releasing it for one strands the others, and it is per-group state |

The five kept copies are now at `aggregate.cpp` 647, 649, 685, 687 and 784, each with its reason
as a comment in place. The only other `std::make_unique<cudf::column>` left under `cpp/src/`
outside `join.cpp` is `EvaluatedColumn::take()`'s, which is the one sanctioned place a borrowed
column is copied — for a caller that keeps it past its input.

**The repeated-ordinal regression the spec names did run.** `corpus_cases.inc:139,294` put tpcds
q84 and q85 on the device at `tp1-single`, and `tp1-single.plans.txt` shows q84 projecting
`c_customer_id@0` twice in one `GpuProject` and q85 repeating two ordinals. So the 95-case corpus
run covered the shape, on top of the `AFiltersRepeatedProjectionOrdinalAnswersBothColumns` and
`AProjectOfColumnRefsAllocatesNothing` cases that pin it directly.

**The four Welford copies were examined for removal and the answer is no, deliberately.** A
hand-built `cudf::column_view{data_type{STRUCT}, rows, nullptr, nullptr, 0, 0, children}` would
borrow the two children instead, and cuDF accepts a struct view with null data. What stops it is
that `make_structs_column` takes owning children and these are the input's columns, owned
elsewhere — which is what the code comments at the sites say — and that the borrowed form is an
inconsistent shape rather than a measured win: `cnt` is a fresh cast at offset 0 beside two
borrowed children at offset k. The gain is two FLOAT64 per-group state columns. A correctness
risk on a path no test slices, for a small saving, is the wrong trade.

**The first version of that argument was wrong about cuDF, and the correction is the lesson.** It
said `structs_column_view::get_sliced_child` "adds the parent's offset to the child's". Read in
the vendored source rather than inferred from the header, it *substitutes* the parent's offset and
discards the child's — worse, same conclusion — and `group_merge_m2` never calls it at all: it
reads `values.child(0..2)` and `begin<T>()` directly, honouring each child's own offset and
ignoring the parent's. So the specific hazard first named does not arise at that call, and the
decision above stands on our own code instead. When a decision rests on a claim about a vendored
library, read the vendored body or argue from something in this tree.

### Three sites the spec's scope listed that needed nothing, and why

- **`sort.cpp`** is in the spec's scope table. It has no exit copy: its `ColumnRef` arm already
  pushes `tv.column(idx)` directly (`:37-39`) and only a computed key goes to `build_column`.
  Converting `owned_keys` to `EvaluatedColumn` would be a rename with no byte behind it. Left
  untouched. **This is drift in the spec, not in the code.**
- **`aggregate.cpp`'s `get_values_col` (:203) and `arg_col` (:453)** short-circuit `ColumnRef`
  the same way, so `build_column` there only ever sees a computed expression, which
  `evaluate_column` owns anyway.
- **`filter.cpp:27`'s mask** stays on `build_column`. A bare `ColumnRef` *is* AST-able —
  `cudf_ast_can_evaluate`'s `default: return true` — so the filter takes `cudf::compute_column`
  for `WHERE b` and never reaches the column path with one. A borrowed mask would need a
  predicate that is both a bare `ColumnRef` and AST-refused, and there is none. The plan's
  Review focus 4 was therefore unpinnable and says so now.

### What was converted in `expr.cpp`, and the rule

Today's `build_column` became `static make_column` with its `ColumnRef` arm deleted;
`evaluate_column` (`:917`) answers `ColumnRef` with a view and delegates everything else to it;
`build_column` is `evaluate_column(...).take()`. The trace line moved to `evaluate_column` so
both arms still trace, and `debug_sync("ColumnRef->copy")` went with the copy — nothing
asynchronous happens on that arm now.

The rule for a caller: a variable whose every use is `->view()`, `->type()`, `->size()` becomes
`evaluate_column`; one that is returned, moved or reassigned keeps `build_column`. Converted:
`build_column_binary`'s six `lcol`/`rcol`; `build_column_scalar_fn`'s `date_part` ts, `substr`
strcol, `abs`, `round`, `lower`, `upper`, `concat` args and `coalesce`'s loop variable;
`build_column_case`'s `last_then`, `cond` and `then`; `make_column`'s unary `arg`, LIKE `strcol`
and cast `inner`; `window.cpp`'s partition keys and argument. Kept on `build_column`:
`coalesce`'s and `case`'s `result` (reassigned), `project.cpp:52` and `join.cpp`'s two masks
(`join.cpp` is the next task's file and was not touched at all).

`round` changed shape rather than just its calls: it used to `std::move(col)` when the column was
already FLOAT64, which an `EvaluatedColumn` cannot do without owning, so the cast result is now a
separate `widened` and the call passes `widened ? widened->view() : col.view()`. Same two
outcomes, and a FLOAT64 bare `ColumnRef` no longer copies.

The string-to-string cast no-op returns `std::move(inner).take()`, so it still copies for a bare
`ColumnRef` exactly as before — the caller owns what it gets.

### The seven cases: red before, green after

`cpp/tests/gpu/test_plan_executor.cpp`, under the RMM statistics adaptor `main()` installs.
Every byte bound is a comparison against the same work run straight against cuDF **on the same
input** — the `partition_alone` pattern refcounted-scatter left — never a `rows * k` formula,
because cuDF's temporaries for one column type are not a multiple of another's. `kSlack` is
64 KiB and absorbs allocator rounding only; every copy forbidden here is a whole column.

| case | what it asserts | red (before) | green (after) |
|---|---|--:|--:|
| `ABareColumnRefIsBorrowedNotCopied` | `evaluate_column` allocates nothing, owns nothing, and its view is the input's own buffer | 32 B, `owned` non-null, pointer differed | 0 B, null, same pointer |
| `ANonAstPredicateDoesNotCopyTheColumnItReads` | the filter node ≤ (decimal compare + `apply_boolean_mask`) alone | 4 413 232 vs 2 431 792 | 2 431 792 vs 2 431 792 |
| `AFiltersProjectionCopiesNoColumnItKeeps` | the same filter with projection `{1,0}` ≤ the same filter with none | 8 050 880 vs 4 333 728 | 4 333 728 vs 4 333 728 |
| `AFiltersRepeatedProjectionOrdinalAnswersBothColumns` | projection `{1,1}` is two entries over **one** owner and both answer | owners differed (two copies) | one shared owner, equal strings at rows 0, 17, 1000 |
| `AProjectOfColumnRefsAllocatesNothing` | a project of three `ColumnRef`s allocates **0**, and the twice-projected column is one owner | 944 B, owners differed | 0 B, `owners[0] == owners[2]` |
| `AWindowPassesItsInputThroughUncopied` | the window node ≤ `grouped_rolling_window` alone, and `owners[0]` is the input's | 5 927 152 vs 3 914 992 | 3 914 992 vs 3 914 992 |
| `AnAggregateHandsGroupbysKeysOverUncopied` | the aggregate node ≤ cuDF's groupby plus the count widening | 6 873 232 vs 5 874 832 | 5 874 832 vs 5 874 832 |

All four byte bounds came out **exactly equal** after, to the byte: the operators now allocate
nothing of their own beyond what cuDF's own calls ask for. The savings, over one customer row
group (122 880 rows):

| site | bytes saved per call | what that is |
|---|--:|---|
| `expr.cpp`'s `ColumnRef` arm | 1 981 440 | one DECIMAL128 column + its null mask |
| `filter.cpp`'s projection | 3 717 152 | an INT64 column and a string column |
| `window.cpp` + its partition key | 2 012 160 | INT64 + INT32 passed through, and the key borrowed |
| `aggregate.cpp` 760 | 998 400 | exactly one INT64 key column |

The project case is nation (25 rows), where the three copies were 944 B — it asserts zero rather
than a proportion, which is the sharper claim.

The red run is the build before any production change, with `evaluate_column` stubbed as
`{build_column(expr, table), {}}` so the cases compile and fail on behaviour rather than on a
missing symbol: **7 of 7 failed**, each on the assertion it exists for.

### Tiers

| tier | command | result |
|---|---|---|
| C++ CPU | `/tmp/dkb-cppbuild/peacock_cpu_tests` (local) | 15 passed, 0 failed |
| rust-only | `cargo test --features rust-only -p peacockdb-core -- --test-threads=2` (local) | rc=0; 694+11+26+3+983+43+18 = 1778 cases, 0 failed, 2 ignored (#182's pair) |
| cost-report | `cargo test -p cost-report` (local) | 41 passed |
| ffi | `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::` (local) | 4 passed, 0 failed, against a `libpeacock_gpu.so` rebuilt from these sources (it exports `evaluate_column`) |
| device | `peacock_plan_tests` | **75 passed**, 0 failed (68 before, +7) |
| device | `peacock_gpu_tests` | 2 passed |
| device | `test_gpu_corpus --test-threads=1` | 95 passed, 0 failed, 21.73 s |
| device | `test_node_timing --test-threads=1` | 1 passed |
| device | `peacockdb_core_gpu_lib gpu_tests:: --test-threads=1` | 580 passed, 0 failed |
| device | `peacock_gpu_benchmarks --skip bench_ --test-threads=1` | 8 passed |

Device host: nebius-gpu `dmitry@89.169.109.150`, L40S, card idle, cuDF 25.02
(`~/data/miniforge3/envs/rapids-cuda-12.2`), sf1 testdata in `~/peacockdb-J/testdata`. One
`./scripts/build-test-shadgpu.sh --build` per cycle (≈4 min warm), then each staged binary run
directly with `LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib`
and `PEACOCK_TESTDATA_DIR=$PWD/testdata`. Four cycles: a baseline on the unmodified tree
(plan 68/68, gpu 2/2), the red cycle, the green cycle, and a re-run after the clang-format pass,
which reproduced every figure to the byte. Never `--run`, never `--pull-results`.

### Formatting

`git clang-format --diff HEAD` was run and applied **only to the code this change introduces** —
`evaluate_column`, `build_column`'s wrapper, `make_column`'s signature, the cast arm's one-line
`if`, the filter's ordinal loop, the window's `with` call, and the whole new test block. What is
left unapplied is a handful of pre-existing hand-wraps that clang-format would re-join because
the change touched the line to swap `->view()` for `.view()`. Re-joining them is churn with no
reader benefit, and `.clang-format`'s own header says the tree was never machine-formatted and
the config is not in CI for that reason: untouched files differ from it by 13 lines
(`sort.cpp`), 15 (`node_session.cpp`) and 128 (`join.cpp`). The device suites were re-run after
the pass.

**No golden moved.** `git status --porcelain testdata/goldens` is empty, and that is the direct
check the dispatch asked for: nothing was regenerated, and the 26 `test_corpus_goldens` cases and
983 `test_cpu_corpus` cases pass against the committed bytes. A C++-internal change cannot move a
plan golden anyway — plans are written by Rust — but the execution goldens are read by the device
corpus, and its 95 cases are green against them unchanged.

### Deferred, and why

- **The sf40 q6 and q19 benchmark before and after**, which the spec's Verification bar asks for.
  sf40 lives only on shad-gpu, which was down for the whole run, and the human host override
  takes benchmark measurement out of every task. So the saving at `expr.cpp`'s arm — the site
  that bar exists to quantify, ~46 of the 107 GB q19's lineitem filter moves at sf40 — lands
  **unquantified at sf40 scale**. No substitute guess is recorded. What carries the claim instead
  is the gtest table above, which measures the same saving at 122 880 rows and shows the operator
  allocating nothing of its own.
- `--run-benchmarks`, the `bench_` cases, Nsight and any H200 timing: out by the same override.

### Host notes for the next run here

- The `pgrep` trap from `stale-cells` still applies and was avoided: every remote phase was a
  detached `nohup bash /tmp/dkb-remote-*.sh` writing a one-line `rc` marker file, polled with
  `until ssh … test -f <rc>`.
- **`/tmp/dkb-cppbuild` is already configured against this worktree** (`CMAKE_HOME_DIRECTORY`
  points at `peacockdb-alpha/cpp`), so `cmake --build /tmp/dkb-cppbuild --target peacock_gpu
  peacock_plan_tests peacock_cpu_tests -j 8` is a 30-second compile check before paying for a
  device cycle. It caught nothing this round, which is the point — it would have.
- `cpp/build` here is still an empty root-owned directory; nothing was configured into it.
- **The ffi rung needs `LD_LIBRARY_PATH` or it exits 127 before running a test.**
  `build-test.md` says so and this round proved it again: without it the binary reports
  `libcudf.so: cannot open shared object file`, which reads like a broken build and is only a
  missing loader path. Prepend
  `target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-*/out/lib` (the newest of the three,
  by mtime) and `$CUDF_ROOT/lib`. No device is needed for that rung; this workstation has no
  GPU driver at all (`nvidia-smi` fails) and the four cases pass.
- **`--list` figures, for checking `build-test.md`.** `peacock_plan_tests --gtest_list_tests`
  counts **75** cases in 16 suites, of which `ExitCopies.` is the seven new ones. The gtest
  binaries have no registry case, so the row and the `--list` total are the same number here —
  unlike the Rust rows the page warns about. C++ total 107 → **114**; grand total 3023 →
  **3030** (Rust 2515 and Python 401 unchanged).

## Round 2 — review round 1 addressed (2026-10-09)

Four items taken: two important, two nits. Both important findings were in the test file; the
operators were not touched for correctness.

**Important 1 — the test no longer reaches a private header.** `test_plan_executor.cpp` had
`#include "peacock/expr.h"`, the only include of `cpp/src/peacock/` by any test in the tree (the
other seven `peacock/*` test includes are `rmm_pool.hpp` and `partitioning.hpp`, both under
`cpp/include/`, which is public). `EvaluatedColumn` and the `evaluate_column` declaration moved
to `cpp/src/plan_executor_internal.h` — the sanctioned route, whose four other entries each carry
a "why a test reaches this" note, and whose own header says it exists for exactly this — with
that note added, naming the case. `expr.h` includes it, so the operators still see both names
and nothing is declared twice; `expr.h`'s closing pointer now lists all four names that come from
there. The include was dropped, which also stops dragging `ExprContext`, `build_expr` and
`fb_to_type_id` into the test. The boundary did not move, so `architecture.md`'s Interfaces
section is still true as written.

The header gained `<cudf/column/column.hpp>` and `<memory>` for `EvaluatedColumn`'s body. Both
are host headers, so the "host-only CPU tests compile against it" property holds — proved by
`peacock_cpu_tests` building and passing 15/15 locally after the move.

**Important 2 — the non-AST case now pins its own premise.** `c_acctbal > decimal(0,2)` reaches
the column path only because `cudf_ast_can_evaluate` refuses a decimal operand, and that lived
only in a comment: loosen that arm and the node routes to `compute_column`, allocates *less*, and
`EXPECT_LE` stays green with nothing covering `expr.cpp`'s site. Added before the measurement:

    const fb::Expr* pred_expr = fb::GetGpuPlan(buf.data())->root()->node_as_CudfFilter()->predicate();
    ASSERT_FALSE(peacock::cudf_ast_can_evaluate(pred_expr, input.view()))

The predicate is re-read off the plan root rather than hoisted out of `acctbal_filter_plan`, so
the helper's signature is unchanged. `cudf_ast_can_evaluate` is already in
`plan_executor_internal.h`, which the file includes.

**The assertion was proved non-vacuous rather than assumed to be.** It cannot be checked on the
host — `infer_expr_type` needs the scanned table, and against an empty one the function returns
false for the wrong reason (`lt == EMPTY`), which would pass the assertion while proving nothing.
So it took a device cycle of its own in the `ASSERT_TRUE` form: **1 of 75 failed**, on
`Value of: peacock::cudf_ast_can_evaluate(pred_expr, input.view()) / Actual: false / Expected:
true`. That is the evidence that the call reaches the decimal arm and that `false` is its real
answer. Flipped to `ASSERT_FALSE` and green. Note from the same run: `ABareColumnRefIsBorrowedNotCopied`
passed there too, which is the proof the test reaches `evaluate_column` through its new header.

**Nit 5 — the plan's three contradictions about `filter.cpp:27`.** The superseded bullet is
deleted; Task 2 Step 3's sketch says `mask->view()`, which is what shipped; the
`AFilterOnABareBooleanColumnBorrowsItsMask` sketch is gone, replaced by one line saying it was
never written and pointing at Review focus 4. Also corrected while in there: the Interfaces block
said `// expr.h` and now names `plan_executor_internal.h` with the reason, the File structure
table's first row with it, and Step 2's red note, which described a compile failure where the
actual red was a behavioural one under the stub.

**Nit 6 — taken, not argued.** `filter.cpp` and `window.cpp` were 7-line in-body comment runs
against `coding-style.md`'s four. Both are 4 now: the `#154` half is one clause at each site. The
displaced "why" went to the operation it describes rather than into prose — a two-line doc above
`TableResult::select` in `table_result.cpp`, saying that an ordinal may repeat, that the two
entries share one owner, and that this is why a fused projection selects instead of moving a
column twice. That file's own header already claims the owner-per-column rule lives in one place,
so it belongs there, and a reader of `result.select(ordinals)` lands on it. The cap for a doc
above a declaration is ten lines, so it is comfortably inside. `project.cpp`'s runs were already
4 and 2 and were left alone.

### Re-run

The coordinator's bar — a clean build plus the whole `peacock_plan_tests` binary — is the right
level and I did not argue it: a header move and an added assertion cannot reach the corpus, and
nothing in the operators changed but comment text. Run anyway, because the header move changes
what `peacock_cpu_tests` compiles against:

| what | command | result |
|---|---|---|
| C++ build, local | `cmake --build /tmp/dkb-cppbuild --target peacock_gpu peacock_plan_tests peacock_cpu_tests -j 8` | rc=0, **no warnings** |
| C++ CPU, local | `/tmp/dkb-cppbuild/peacock_cpu_tests` | **15 passed, 0 failed** |
| device, the red cycle | `peacock_plan_tests` with `ASSERT_TRUE` | **74 passed, 1 failed** — the premise assertion, on `Actual: false` |
| device, green | `peacock_plan_tests` | **75 passed, 0 failed** |
| `--list` | `peacock_plan_tests --gtest_list_tests` | **75** cases, 16 suites — unchanged, no case added or lost |

The four printed figures came back identical to round 1, to the byte: non-AST predicate
2 431 792 = 2 431 792, filter projection 4 333 728 = 4 333 728, window 3 914 992 = 3 914 992,
aggregate 5 874 832 = 5 874 832. `build-test.md` needs no further change.

`git clang-format --diff HEAD` over the four files this round touched proposes nothing; the new
test lines were formatted over their range as before.

## Round 3 — completeness pass addressed (2026-10-09)

Two items taken. One is the real gap: **nothing on this branch pinned that anything is
*released*.** All seven earlier cases assert on `Allocated::total` or `== 0` plus owner
identity; none touched `net` or `peak`. Retention is the one failure mode sharing introduces,
and it was measured nowhere.

**Why no existing test could have caught it, which is the whole argument for the case.** An
operator that keeps an owner it should drop allocates **nothing new**. So every byte bound stays
green *to the byte*, the corpus stays green because answers do not change, and the driver never
notices because it prices from the plan schema. The only symptom is device residency at a scale
no test runs. The red run below is the proof: with both operators deliberately over-retaining,
**75 of 77 cases passed** — including all four byte bounds, at their exact round-1 figures.

### The two new cases, and why both

They are different claims, so both landed.

| case | claim | how it fails |
|---|---|---|
| `AFiltersUnselectedColumnIsFreedWithTheIntermediate` | a dropped column's bytes come back | a filter that keeps the intermediate's owners alongside the selection |
| `AProjectHoldsItsSharedColumnOnceAndDropsTheRest` | a shared column is held exactly once, and the unprojected columns go | anything that retains the input past the call — the handle not erased, the operator stashing it |

The filter case runs the same plan twice over the same row group, `{0,1}` then `{0}`, and bounds
the narrow call's `net` by the wide call's less the dropped column's own size — measured by
copying that very column, so no cuDF cost is a constant. The project case takes nation's four
columns to one and asserts `owners[0].use_count() == 1` and `net < 0`; `use_count` is the idiom
`Scatter.PartitionsShareTheirTableAndSurviveTheirSiblings` already uses, and the negative `net`
is `Slice.OwnsItsRowsSoTheBatchCanGo`'s.

The measurement is possible inside the region the cases already wrap, which was checked rather
than assumed: `execute_node` moves each input handle out of the registry and erases it
(`node_session.cpp:647-654`), and the `inputs` vector dies when `execute_one` returns — both
inside `allocated_by`. So the input batch's release is charged to the call.

### Watched red, by an injected over-retention

An allocation bound cannot produce this red, so it was driven by an edit: a function-local
`static std::vector<TableResult>` in each of `execute_filter` and `execute_project`, stashing the
intermediate and the input respectively. Reverted immediately after; `grep -rn leak_for_the_red
cpp/` is empty.

| | red (both operators over-retaining) | green (reverted) |
|---|---|---|
| filter: whole `net` / narrowed `net` | 0 / **0** | 0 / **−2 718 736** |
| filter: dropped column, measured | 2 718 752 | 2 718 752 |
| project: call `net` / holders | **0** / **2** | **−2 400** / **1** |
| project: scan `net` | 2 768 | 2 768 |
| whole binary | **75 passed, 2 failed** | **77 passed, 0 failed** |

The narrowed filter gives back 2 718 736 bytes against a dropped column of 2 718 752 — the
column and nothing else. The project call gives back 2 400 of the scan's 2 768, which is
nation's three unprojected columns. Both reds carried the right message:
`the unselected column was retained: 0 against 0` and `the three unprojected columns were
retained: net 0`.

### `table_result.cpp`'s file comment was false at the head

It claimed "every handle's table is built by one of these, so the owner-per-column rule and its
refusals live in one place". `project.cpp` is the first and only site in the tree that assembles
a handle's table field by field — it reorders and renames, which no constructor has the shape
for. The comment was true at `ENS-stale-cells` and this branch falsified it. It now says a
handle's table is built by one of the four **or** assembled through the public fields, naming
`project.cpp` and the reason, with `register_handle` as the single check either way. That
matters for the next task: `join-rewrite-design.md` §3.0's `emit` combines columns from two
gathered tables, which no single constructor can do, and the old wording pointed its author at
adding a fifth constructor instead of at the hand-assembly route.

### Re-run

The coordinator's bar — a clean build plus the whole `peacock_plan_tests` binary — was right and
nothing surprised me, so no corpus cycle was spent. The local C++ CPU run is in because the
build is shared.

| what | command | result |
|---|---|---|
| C++ build, local | `cmake --build /tmp/dkb-cppbuild --target peacock_gpu peacock_plan_tests peacock_cpu_tests -j 8` | rc=0, **no warnings** |
| C++ CPU, local | `/tmp/dkb-cppbuild/peacock_cpu_tests` | **15 passed, 0 failed** |
| device, red | `peacock_plan_tests`, both leaks injected | **75 passed, 2 failed** — only the two new cases |
| device, green | `peacock_plan_tests`, leaks reverted | **77 passed, 0 failed** |
| `--list` | `peacock_plan_tests --gtest_list_tests` | **77** cases, 16 suites |

The four byte figures are unchanged from round 1, to the byte, in both the red and the green run.

**`build-test.md` needs a count move: Plan-executor row 75 → 77, C++ total 114 → 116, grand
total 3030 → 3032.** Left to the coordinator, who moves the row, the tier heading and the header
together.

### CI, for the `done` transition (2026-10-09)

Run **37946551764** on `7d63d5ed`, which is the head and bears code, so the rule this chain learned
on PR #175 is satisfied without a close-and-reopen: cudf 25.02 pass, cudf 26.02 pass, GPU build
pass, Cost report pass, S3 check pass, Pages skipped, and `GPU Tests (remote)` red on `ssh: connect
to host llm-gpu0h200.velkerr.ru port 22: Connection timed out` — shad-gpu, exempted by name, its
work done on nebius-gpu above.

**The 26.02 leg is worth more than usual here.** Nothing on the device side of this task ran at
26.02 — nebius-gpu's 26.02 is red for reasons outside this chain — and the borrowed view through
`evaluate_column` is a lifetime change that a different cuDF could in principle treat differently.
The 26.02 leg builds and runs the cpu side of the whole tree against it, so `verify-26.02` (task 9)
inherits this change already compiled and exercised there.

`completeness approved` → `done`.
