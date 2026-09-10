# peacockdb archived tickets

Tickets that are finished or that the tree outgrew. Numbers stay permanent and are never
reused, so a commit message or comment naming an old ticket still resolves — here.
`llm-wiki/tickets.md` holds only open work.

Three closing markers, and they say different things: **Done** is fixed, **Stale** is
overtaken by the tree, and **Obsolete** is a capability or a question deliberately dropped —
which a later reader needs to tell from an oversight. Stale and obsolete share a section.

**Numbers spent without a ticket.** A number withdrawn before it described anything real is
recorded here and nowhere else, so the counter never walks back over it:

- **#165** — filed and withdrawn 2026-08-20. It reported a dead widget link — `cost-registry.csv`
  naming #115 after #115 was archived — and there was no such link: the widget already follows a
  number to the file holding its anchor, with #115 as that test's own case. What was wrong was
  the note below, which predated that code and now says what holds. Named by commit 7b98a99.
- **#156** — filed and withdrawn 2026-08-19. It called the exec-model prototype's row-group
  chunking a defect against the engine; the prototype is a model, not a specification, so
  diverging from it where it is wrong is the outcome rather than a drift to reconcile
  (`scripts/exec_model/README.md`). Named by commit 4c89d91.

Archiving a ticket the registry names is safe: the cost widget resolves a number to
whichever of the two files holds its `<a id="tNN">` anchor (`TicketIndex::path_for` in
`cost-report/src/main.rs`), and refuses to render a link for a number in neither, failing
the report rather than emitting one that goes nowhere.

## Done

<a id="t187"></a>
### #187 — the device widens a decimal the plan declared narrow

`tpch/filter-project` at all five modes: `expected Decimal128(15, 2) but found Decimal128(38, 2)`
at the unload. The query is `SELECT l_orderkey, l_quantity FROM lineitem WHERE l_quantity > 30` —
no arithmetic anywhere, so this is the export or the filter widening the column rather than a scale
rule at a binop.

The CPU has a rule for this: `widened_decimal` (`cpu_backend.rs:499`) accepts a produced decimal
wider than the declared one at equal scale, and `declared_as` casts it back — over every exec
stage's output, not only a merged state, though its doc argues the merge that motivated it. The
device's unload has no such rule: `gpu_backend.rs:166` concats against the sink schema and arrow
refuses the type outright.

The value in question is the device's own export; DataFusion produces `(15,2)` throughout and the
CPU tier is green. So this is two rules for produced-against-declared, one per engine, disagreeing
on the same bytes — one tolerating and casting back, the other refusing. Whichever is right, one of
them is wrong. Neighbour of [#163](../tickets.md#t163) for that reason, where
[#183](../archive/archived-tickets.md#t183) is two representations of one value rather than two verdicts on it.

First plain decimal projection to reach a device in the corpus: q6's decimals are sums, whose
declared type is already wide, which is why twenty queries went past this and the twenty-first did
not.

Two sightings, and the pair narrows it: `filter-project`'s projected column declares `(15,2)` and
`hash-join`'s sum declares `(25,2)`, and both are found as `(38,2)`. Same scale, same 38, two
different declarations — so the export appears to produce one width rather than widening each
value by a step, which is a different fix from a scale rule and points at the export rather than at
anything upstream of it. Six device cells across T19's first two batches.

**Closed 2026-08-28** by `llm-wiki/tasks/wire-schema.md`, and the framing above is wrong twice over.

It was not two engine rules disagreeing. cuDF's decimal carries scale but not precision, so
`export_table_to_ipc` never had a precision to pass and `decimals_to_arrow` fell back to
`max_precision<__int128_t>()`, which is 38. The CPU's `widened_decimal` was never the other half of
a disagreement — it was the only side that had been told anything.

Nor was it a defect. 25.02's own header says so: "since the precision is not stored for them in
libcudf, decimals will be converted to an Arrow decimal128 which has the widest precision that cudf
supports". A documented default that nobody had overridden.

The fix tells the export what the plan declared. `PlanNode.output_schema` is filled for every fb
node by the builder that knows what its payload emits, `TableResult` carries a per-column precision
beside the names, and `export_table_to_ipc` rebuilds the imported Arrow schema with
`arrow::decimal128(p, s)` — Arrow's side rather than `cudf::column_metadata`, which gains a
`precision` field only in 26.02 and does not have one on the 25.02 leg that runs.

Thirty-nine cells: 5 green, 25 to #185, 9 to #195, and none to a type failure of any kind. The
observation the ticket got right is the one that pointed at the export: `(15,2)` and `(25,2)` both
arriving as `(38,2)` is one width being produced rather than each declaration widened by a step.

<a id="t183"></a>
### #183 — the device exports Utf8 where the sink declares Utf8View

`GpuUnload lane 0: the exported stream is not the sink's rows: column types must match schema
types, expected Utf8View`. The sink's schema comes from DataFusion, which uses `Utf8View`; the
device's IPC export produces `Utf8`. Same values, different arrow type.

Twelve of T18's device cases over eleven queries — tpcds q3 q15 q37 q42 q43 q52 q55 q82, tpch q10
q12 q15 — with `#183` on their gpu columns.

The same divergence bit the digest comparator one layer up, where hashing the column type reddened
eight legacy gpu cases whose rendered comparison had never looked at types. That one was a
comparison artefact and was fixed by hashing names; this one is the export genuinely disagreeing
with the schema the plan declared, and no comparison choice makes it go away.

**Closed 2026-08-28** by predicting the export type at plan time and casting this one divergence
at the unload — cuDF has a single string type, so the cast is the only place to absorb it
(`llm-wiki/tasks/casts.md`). No registry row cites #183; sixty did when the rollout started.

The 52 cells it was holding did not go green: two tpch queries did, at all five modes, plus
`tpcds/q84` at one, and the rest moved to the cause that refuses them next — 18 to #187, 2 to
#191, 14 to #185, 11 to #195, and 4 to #152. That is the ordering the spec predicted; what it got
wrong was expecting #152 to be the common landing place.
<a id="t194"></a>
### #194 — the cost gate's git baseline ignores which section it was asked for

`base_total` (`cost-report/src/main.rs:1623`) reads a base-side total two ways. The directory arm
passes `section` to `entry_total`; the git-ref arm — the one every PR uses — calls
`read_total_str` on the whole file and drops it. `read_total_str` returns the FIRST
`peacockdb_cost=` in the text, so every section of a per-mode `.cost.txt` is compared against
whichever query sorts first in that file.

Legacy goldens are one file per query, so `section` is `None` and the two arms agree. The
batch-partitioned per-mode files hold ~60 sections each, and there the gate is comparing unrelated
queries. On `tpcds.sf1/bp-tp1-single-mini.cost.txt` the reported deltas run -99.6% to +341.3%
against `q2`'s 737,382,823 — three below -90%, three above +200%, none of them a cost change.

Every bp row on PR #135's cost widget is this. The new-side totals are correct; only the baseline
is wrong, so nothing is mis-measured in the goldens themselves.

Fix: the git arm takes the same `entry_total(text, section)` path as the directory arm. A test that
the two arms agree on a multi-section file is what would have caught it.

**Done 2026-09-08, by commit ca5c34d2.** Both arms take `entry_total(text, section)`, and
`both_base_arms_read_the_same_section_of_a_multi_section_file` builds a two-section golden and
asserts the directory arm and the git arm return the same number for the section that is not
first. Archived here when the ticket was found open with the fix already in the tree.

<a id="t110"></a>
### #110 — Retire the all-at-once GPU executor
Remove `executors/all_at_once_gpu_executor.rs` → `peacock_execute` FFI (whole-plan, no
per-node stats), migrate or drop its 5 smoke tests plus the lifecycle test, and drop the
FFI symbols from `cpp/include/peacock_gpu.h` / `cpp/src/gpu_executor.cpp` (and
`execute_plan.cpp`). Its stated blocker — a common `Executor` trait — has landed, so this
is unblocked pending full_table/partitioned covering all needs.

**Done 2026-09-08.** The all-at-once executor, `peacock_execute`, `execute_plan.cpp` and
the recursive arm of `execute_node` are gone with the other five legacy modes; `execute_node`
is now only the resolver that hands an operator its next already-resident input.

<a id="t103"></a>
### #103 — GPU SIGSEGV: shuffle_stddev tp8-standard (Welford N-way merge)
`gpu_partitioned_tpch_sf1_shuffle_stddev_partitioned_tp8_standard` segfaults (139) or fails as a contained
`vector::reserve` Err on shad-gpu. Nondeterministic; reproduces at 12 GiB and 120 GiB, so
not budget-related. Inside `execute_instrumented`, upstream of golden compares; tp1 never
crashes. Suspect the 8-way Welford M2 merge (`cpp/src/operators/aggregate.cpp`).
**Fixed and the test is back**, 2026-08-21. The cause was `OutBuild` in
`cpp/src/operators/aggregate.cpp` carrying default initializers on its last fields only: the
Final-mergeable arm filled the struct by name and left `res` — an index into the results vector —
uninitialized, so it read whatever the stack held. That accounts for every symptom: two
signatures from one input, a budget that changed nothing, and tp1's immunity, since tp1 plans
`Single` and never enters that arm. Found when a new merge arm wrote the same shape and turned
the latent read into a segfault on its first call.

`gpu_case!(tpch, 1, shuffle_stddev, partitioned_tp8_standard, golden_approx_std)` is enabled
again after 20 consecutive green runs on shad-gpu, and `common/gpu_cases.inc` names this ticket
beside it: if it flakes again, reopen this rather than filing a new number.
<a id="t151"></a>
### #151 — the per-node benchmarks measured the engine with no RMM pool

`peacock_gpu_benchmarks` times the engine through the FFI, and the engine installs no device
resource ([#148](../tickets.md#t148)) — so every intermediate it allocated was a
`cudaMalloc`/`cudaFree` round trip, and a node's `time_us` included that. The C++ gtest binaries had stopped
measuring it, so the two families of numbers in the tree were taken under different
allocators, and the per-node records charged a node for the default resource — worst exactly
where its output is largest.

**Landed as the fallback, not the fix.** [#148](../tickets.md#t148) stays open on its own
terms: it also makes a shipping query faster, and it carries a second decision about
`gpu_memory_limit`. Instead:

- The sizing rule moved to `cpp/include/peacock/rmm_pool.hpp`, shared rather than copied —
  `multi_gpu.cpp` takes its 85/95 from the same constants. Header-only, because four gtest
  targets link `cudf::cudf` and gtest but not `peacock_gpu`.
- `peacock_install_rmm_pool` reports the OUTCOME, not success: a pool, or a pool that could
  not be built. Before it a failed reservation was indistinguishable from an ordinary run.
- It lives in `libpeacock_gpu.so` though no shipping query calls it. A test-only shim DSO
  would leave production untouched, but the build sets no `-fvisibility=hidden`, so rmm's
  current-device-resource state could be duplicated across DSOs — a green run that fixes
  nothing.
- Idempotency is in C++ alone, not a Rust `OnceLock`: one guard, on the side that owns the
  resource. Rebuilding would drop a resource live allocations still point into, and 127
  `#[test]`s in one process is the shape that finds it.
- Records carry `allocator=` beside `build_profile=` and `sync_floor_us=`, and the harness
  asserts both before it measures anything: a time taken without a pool, or from a
  non-release build, is an invalid comparison, so it is refused rather than written.

**Re-swept, all 127 records, every one faster**: median -9.6% on `total_us`. The move is not
uniform, which is the whole point — `GpuScanExec` -5.3% (parquet read, few intermediates)
against `GpuProjectExec` -53%, `GpuHashJoinExec` -51%, `GpuFilterExec` -46%,
`GpuAggregateExec` -30%. The old records were wrong about the SHAPE of a plan's cost, not
only its scale.

They were also stale for the smaller second reason this ticket carried — measured before the
scatter stopped opening a timed region of its own — so a part of that move is not the
allocator. Measured then at -2.5% median on repartition nodes and -0.6% on whole records, it
cannot account for -50% on joins. Both halves are now settled by one sweep.

**Done.** The pool is now the only way the benchmarks run — there is no switch to turn it
off and no outcome but a pool that the harness will record.

<a id="t173"></a>
### #173 — a collapse of nothing answers with a table that has no columns

`GpuCoalesceAllBatches` over a lane that received no batch returns a handle whose table carries
zero columns, so the export decodes a batch whose first column is out of bounds.
`cudf::concatenate` of an empty view list has no schema to preserve, and the node's declared one
is not reachable: `PlanNode.output_schema` exists on the wire but the recipe writer leaves it
`None` (`recipe/writer.rs:102`, `:131`), which `WRITER_DIFFERENCES` records with the reason that
nothing on the C++ side reads it.

Decided rather than filled in: an empty lane emits nothing, on both backends, so the arm is
unreachable from a correct driver and `node_session.cpp:302` throws on a zero-input collapse
instead of answering with a schemaless table — the same shape as `execute_one`'s
consumed-equals-provided check. Putting the schema on the wire for this one arm would move all 16
digests in the payload golden to serve a case that no longer happens.

Closes when the throw is in, a device test proves it goes red, the CPU executor emits nothing for
an empty lane as the GPU one does, and the `WRITER_DIFFERENCES` reason says the arm that would
have read a schema is now a refusal.

**Done 2026-08-25**, by 462e018: the zero-input collapse throws, a device test constructs it
through the ABI and shows it red, the CPU executor emits nothing for an empty lane as the GPU one
does, and `WRITER_DIFFERENCES` records that the arm which would have read a schema is now a
refusal.

<a id="t135"></a>
### #135 — Column ordinals are enforced only by cuDF's `at()` and the final result
Every column reference in the IR is an ordinal into the child's output table, and almost nothing
checks them. Two concrete gaps.

(a) `TableResult` is a `cudf::table` plus a name vector with no invariant that the two have the
same length, and the six sites indexing names use `operator[]` — so a short vector is undefined
behaviour, not an exception. `filter.cpp` ~L42 reads `fv.column(idx)` and
`input.column_names[idx]` in one iteration and only the first is checked; it happens to run
first. Assert `num_columns() == column_names.size()` where `TableResult` is built. (b) Nothing
checks a child produced its columns in the order the plan assumed. Per-node bytes come from the
plan's schema on both engines by design (`logical_size_from_schema`, single-sourced so they
cannot drift), so a node emitting the right count in the wrong order yields identical numbers
everywhere and surfaces only at the root; a per-node type check in the GPU tiers closes it. Also
there: `expr.cpp` ~L349 returns `type_id::EMPTY` for an out-of-range ColumnRef instead of
throwing, turning a bad ordinal into a confusing type error further along.

Closed 2026-08-19. The batch-partitioned planner now checks every column reference's name against the field at its position, so the class is caught at plan time for that mode; the C++ items above are carried by [#164](../tickets.md#t164).

<a id="t115"></a>
### #115 — q38/q76/q87 set-op & union-count divergences — retriage
q38 (INTERSECT ×3), q76 (UNION ALL + IS NULL filters + grouped `count(*)`) and q87 (EXCEPT ×2)
diverged on the GPU historically, and the ticket's instruction was to rerun on an H200, enable
what passes, and root-cause the rest.

**Done 2026-08-19.** All three rerun on shad-gpu in `golden_exact` — per-node rows and cost
against a freshly generated `.cpu.txt`, plus the whole result. None diverges in any way: not a
row count, not a value, not a per-node mismatch. q38 answers 107, q87 47298, and q76 matches
its 103-line grouped output exactly. Whatever the bucket-I triage saw was fixed somewhere
between it and today; q87's named suspect — anti/EXCEPT null handling overlapping
[#80](../tickets.md#t80) — was never the cause, since `EXCEPT` wants the `EQUAL` that anti
hardcodes.

**The three stay disabled in the legacy modes deliberately**, which is why this is closed
rather than acted on: they are `full_table_gpu=na` by choice now, not by defect, and the
registry keeps `115` in their `tickets` column so the widget still says which decision the
cells rest on. The batch-partitioned mode plans all three.

<a id="t77"></a>
### #77 — Cost report: publish per-SHA history on master
Pages deploy overwrites the latest report each run — no history, no trend. Deploy under
`/<sha>/`, keep `/index.html` as latest, add a lightweight index + `history.json`. Open:
retention policy, gh-pages branch vs deploy-pages artifact model.

**Done 2026-08-05.** Implemented and labelled as such in the code
(`cost-report/src/main.rs`, "page-per-sha Pages site (ticket #77)"): `<dir>/index.html` is
the latest report, `<dir>/<sha>/index.html` the same run addressable by commit,
`history.tsv` the newest-first manifest and `history.html` the rendered index, with the
report footer linking *latest · all reports*. The Pages deploy replaces the whole site, so
the CI step curls the prior manifest and each prior `<sha>/index.html` forward from the live
site before generating. Both open questions resolved: the deploy-pages artifact model (not a
gh-pages branch), and retention = whatever the manifest lists.

<a id="t27"></a>
### #27 — TPC-H q11/q22 scalar-threshold NLJ → broadcast filter
Likely stale: q11 and q22 now run full_table_gpu at tp1-standard and
nested-loop/cross-join operators exist. The 1×N broadcast-filter rewrite may still be a
perf win — verify, then close or rescope as an optimization.

**Done 2026-08-05.** The blocker is gone: tpch q11 and q22 both run
`full_table_tp1_standard` on the GPU (`test_gpu_full_table.rs` — q11 as `oracle` because its
result exceeds the golden size cap, q22 `golden_exact`), and both plans carry NestedLoopJoin
nodes that execute there. What remains is a pure optimization — rewriting the 1×N
scalar-threshold nested-loop join into a broadcast filter — and that belongs under the CBO
umbrella (#73) or runtime filters (#16), not as a standing blocker on two queries that
pass.

<a id="t18"></a>
### #18 — Add DuckDB cost estimates to canonical plan goldens
Appears delivered: `*.duckdb_cost.txt` goldens exist under `testdata/goldens/*.sf1/`,
generated by `gen_duckdb_cost.sh` / `duckdb_cost.py`. Verify the remaining checklist
items (regeneration wiring, diff display), then close.

**Done 2026-08-05.** The `*.duckdb_cost.txt` goldens exist for the whole corpus
(22 tpch + 99 tpcds under `testdata/goldens/*.sf1/`), `gen_duckdb_cost.sh` regenerates them
in two modes (`--gen` needs DuckDB 1.5.4; `--extract-only` rebuilds from the committed
profiles), and the cost report renders peacock Σout against duckdb Σout with the ratio
bucket. Nothing on the original checklist is outstanding.

<a id="t4"></a>
### #4 — Make the build process more understandable and transparent
Empty-body stub from project start. Largely answered by `llm-wiki/build-test.md`; fold
the rest into #13 or close.

**Done 2026-08-05.** Answered by `llm-wiki/build-test.md`, which now carries the local
build workflows and their separate cargo/C++ target dirs, what `rust-only` selects, the CI
job graph, and the remote hosts. The hermetic-build remainder it also gestured at is #13.

<a id="t3"></a>
### #3 — CI is too long
Empty-body stub from project start. Superseded by the tiered pipeline; close or re-file
with concrete targets.

**Done 2026-08-05.** Superseded by the tiered pipeline: cpp-cpu (two cuDF legs),
cpp-build-2502, gpu-tests and cost-report run as independent chains, the GPU leg ships
prebuilt binaries to shad-gpu rather than building there, and the golden/meta tier runs
under `rust-only` with no C++ at all. Re-file with concrete time targets if CI length
becomes a problem again.

## Stale or obsolete

<a id="t41"></a>
### #41 — Standing test for GpuUnion branch-type normalization cast
Nothing is unimplemented — the title says *test*. `execute_union` retypes every branch column to
the declared output type before `cudf::concatenate` (`union.cpp` ~L49), or it throws.

What is open is that nothing covers it. The implementation comment names the case it was
written for — tpcds q5, pairing a decimal measure against a `cast(0 AS decimal(7,2))` literal
that materializes as FLOAT64, plus cuDF's SUM drifting fixed_point scale per branch. That case
used to go red by luck, through a device tier that ran q5 and no longer exists; today no test
reaches the cast at all. Fix: a focused gtest building a two-branch union with FLOAT64 against
DECIMAL128, asserting the concatenate succeeds with the declared type. Cheap, and independent
of any corpus query.

**Obsolete 2026-09-08.** The kernel it asks to pin is unreachable. No recipe addresses a
union — `FbKind` has no variant for one, and the only `CudfUnion` the writer emits is
structural, holding branches together with no seq published for it — so `execute_union` and
the `cudf::cast` inside it are never called by this pipeline. What normalizes branch types now
is a per-branch `GpuProject` the planner inserts, and twelve corpus queries carry a union at
`bp-tp1-single` with tpcds q5 among them, answered against DataFusion on the cpu backend. A
gtest here would pin a path nothing takes; the device side of those twelve is the rollout's
gap, not this ticket's.

<a id="t32"></a>
### #32 — GPU window functions / PARTITION BY
The kernel gaps, which #143 sits on top of: this mode refuses a window query at plan time, so
nothing reaches them today. Whole-partition aggregate windows worked
(`cpp/src/operators/window.cpp`; q12/q51/q53/q63/q89 were green on a device before the modes
that ran them went). Remaining: `rank()`/`dense_rank()` (`StandardWindowExpr`) for
q36/q44/q47/q57/q67; q49 secondary blocker; q20 LIMIT-boundary NULL-ordering tiebreak; q98 OOM
on a shared GPU.

**Obsolete 2026-09-08.** Window functions are not a capability this engine is carrying: the
planner refuses a window query before any of these kernels is reached ([#143](#t143), obsolete
beside this), and the mode that once ran them on a device is deleted. The gaps are recorded
here for whoever revives the feature rather than tracked as work.

<a id="t143"></a>
### #143 — window functions in batch-partitioned mode
The planner refuses window queries at plan time, so a window query does not run at all —
the one capability that went with the retired modes rather than moving to this one, and
12 registry rows. Direction: a window is a per-partition op once the input is
hash-partitioned on the PARTITION BY keys;
whole-partition aggregate windows need a single batch (coalesce-all first), while
`BoundedWindowAggExec`-class frames could stream as a `BatchAccumulator`. The
rank/dense_rank gaps of #32 carry over unchanged.

**Obsolete 2026-09-08.** Window support is not planned. A window query is refused at plan
time, so thirteen TPC-DS queries do not run and their registry cells stay `na` — a decision
with a consequence, not an oversight, and the kernel-side gaps are in [#32](#t32) beside this.

<a id="t114"></a>
### #114 — plan_status is shape-validated but never truth-verified
`plan_status` (ok/fail) is shape-checked only; no test attempts to plan the `fail` rows.
When #23 lands and q27/q70/q72/q86 start planning, the widget keeps rendering `plan ✗`
with nothing going red. Fix: a permanent plan-attempt probe in `test_batch_partitioned_plans`
asserting each `fail` row still fails to physically plan (and `ok` rows plan) — that tier
already provisions parquet and already reads the registry. Accepted risk until then: stale ✗
cells after an upgrade.

**Obsolete 2026-09-08.** The probe it asked for exists, as a golden line rather than a test
of its own: each `<mode>.plans.txt` holds `refused by datafusion: …` per query,
`the_registry_matches_the_goldens_in_both_directions` maps that line to `na` in both
directions, and `load_csv` refuses an enabled cell on a `plan_status=fail` row. So when #23
lands and q27/q70/q72/q86 start planning, the golden moves first and the cells and the status
cannot stay as they are.

<a id="t132"></a>
### #132 — Two batch-size fields cross the IR and nobody writes or reads them
`CudfScan.batch_size` and `CudfCoalesceBatches.target_batch_size` are in the fbs, and
`grep -rn 'batch_size' cpp/src cpp/include` returns nothing. Neither is read: the scan reads
by row group (`set_row_groups`), so there is no batch to size, and `CudfCoalesceBatches` is
`execute_passthrough` in `dispatch.cpp`. Since the legacy planner went, neither is written
either — the recipe writer emits no coalesce-batches node at all and leaves `batch_size` at
its default — so they are two fields of wire-format surface with nobody on either end. The
wire format is deliberately frozen, so the decision is whether a batch bound should ever
cross it (a device would then have a memory lever it does not have) or the fields should go
in a commit that regenerates the payload digest deliberately. Until then, do not read them as
evidence that device execution is batch-bounded.

**Obsolete 2026-09-08.** With the legacy planner gone nothing writes these two either — the
recipe writer emits no coalesce-batches node at all and leaves `CudfScan.batch_size` at its
default — so they are inert fields of a frozen schema rather than a knob that misleads.
Dropping them would move every payload byte and the digest that pins them, for no reader's
benefit; a real batch bound, if a device ever takes one, is a new field designed for the mode
that wants it.

<a id="t96"></a>
### #96 — GPU real-8-way per-partition JOIN execution
Largely landed: the CPU oracle and the GPU map arm both run partitioned joins
per-partition (child0[p] ⋈ child1[p]); q17 green at tp8-standard plus
q3/q5/q7/q8/q9/q12/q13/q19. Before closing, confirm nothing remains beyond the
broadcast/CollectLeft and non-inner surfaces now tracked in #97.

**Stale 2026-09-08.** The real-8-way mode whose join arm this tracked is deleted, and #97,
which held what remained of it, is stale beside this. What a device does with a join now is the
batch-partitioned capability matrix, whose gaps carry their own tickets.

<a id="t131"></a>
### #131 — resident model never accounts for cross / nested-loop join build sides
`resident.rs::peak()` stacks a build side by matching `stat.node_name` against `"HashJoinExec" |
"CrossJoinExec" | "NestedLoopJoinExec"`, and two of the three never match.

`GpuCrossJoinExec` and `GpuNestedLoopJoinExec` are two of the five operators that do not strip,
so the name reaching the classifier keeps its `Gpu` prefix, matches nothing and falls into the
streaming arm: the build side contributes zero and never stacks with the probe. `HashJoinExec`
works only because its wrapper strips. So the resident-OOM enforcer under-estimates every plan
containing a cross or nested-loop join, in the direction that lets a query through that should
have tripped. Latent — the tight-budget set (`test_cpu_oom`: tpcds q78, tpch q7/q18) has no such
join — and unreachable by the existing unit tests, which construct names by hand
(`node("HashJoinExec", …)`) and so cannot see the mismatch: a guard that cannot go red. Fix:
classify on a type rather than a rendered name (`as_operator` plus the wrapped node's identity),
or normalize the prefix where the name is recorded, then add a case built from a real wrapped
plan.

**Stale 2026-09-08.** `resident.rs` and the streaming driver it hung off are deleted with
the legacy modes. The batch-partitioned mode accounts memory from its own model
(`batch_partitioned/estimator.rs` and the driver's accountant), which knows a join's build
side by node kind rather than by a rendered name.

<a id="t130"></a>
### #130 — `partition_topology()` is implemented 16 times and read by nobody
`Operator::partition_topology()` (`peacockdb-core/src/operators/operator.rs`) returns each
operator's partition behaviour — ScanEmit / Map / Collapse / KWayMerge / RepartitionHash /
Join — and has *no* callers: not in `src`, not in `tests`. The CPU backend and the C++ side
each re-derive the same
classification independently: the CPU partitioned backend by ad-hoc predicates
(`collapses_partitions`, `hash_repartition_of`, `partitioned_join_arity` in
`backend/cpu_node_executor.rs`) and the C++ side by `node_type()` switches in
`node_session.cpp`. That is the duplicated-rule antipattern coding-style.md names: three
copies of one fact, and the trait copy — the one a reader would trust, since it is the
declared interface — is the copy nothing can prove right, because no test can make it go
red. Either drive both off it (the CPU predicates become one match on the topology)
or delete it and stop implying an abstraction the code does not use. Found auditing the
operator tables for architecture.md.

**Stale 2026-09-08.** `Operator` and its `partition_topology` are deleted with the legacy
modes. The batch-partitioned nodes declare their lane behaviour in the layout the planner
fills and the validator checks, so the declaration has readers.

<a id="t133"></a>
### #133 — No tp1 plan golden: tp1 cost annotation is unpinned
Every `.plan.txt` in the tree is tp8 — 130 at `tp8-mini` plus `shuffle_additive` at
`tp8-standard` — so the plan tier never renders a tp1 plan. The tp1 node TREE is still pinned,
by the 110 `tp1-standard` `.cpu.txt` goldens, which carry each node's exprs, predicates and
projections. What is pinned nowhere is the plan-only annotation at tp1: `row_width`,
`subtree_max_row_bytes`, `estimate_input_bytes`, `estimate_output_bytes`, `estimate_cost`
appear only in `.plan.txt`. A change to the memory/cost model that moved those numbers at tp1
while leaving tp8 unchanged would go green — and tp1 is the shape the GPU full-table tier
runs, so it is not a corner. Fix: canonize a handful of tp1 `.plan.txt` goldens (the same
queries the tp1-standard cpu tier already uses), or teach the cpu golden to carry the
annotation. Same class as #114 — a cell nothing verifies. Found auditing tp1/tp8 planning
impact.

**Stale 2026-09-08.** There is no `.plan.txt` tier: the plan goldens are the five
batch-partitioned modes, and each renders the tree, the recipes and the memory model for
every query at that mode. The annotation this ticket wanted pinned at tp1 is pinned at each
of the five.

<a id="t126"></a>
### #126 — maybe_write_result_golden discards its removal result
`peacockdb-core/tests/common/mod.rs` (~L566): when a result exceeds
`RESULT_GOLDEN_MAX_BYTES` the golden is deleted so the GPU test falls back to the live
oracle, but `let _ = std::fs::remove_file(...)` discards the outcome and the message
prints unconditionally. A failed removal is therefore reported as "no golden" while the
stale golden is still on disk, and the message cannot distinguish "deleted a stale one"
from "there was nothing here". Narrow, but it is a regen path: the operator's next move
is to trust the log and commit. Same shape as #119. Check the result, and say which of
the two things happened.

**Stale 2026-09-08.** `maybe_write_result_golden` went with the legacy harness. The corpus
result golden is one file of sections written by the tier that authors it, with no size cap
and no removal path.

<a id="t97"></a>
### #97 — Semi/anti/outer joins at real-8-way
The widest tp8 blocker (~25 registry rows). Per-partition semi/anti/outer landed
(semi_join / anti_join / left_join green at tp8-standard). Remaining: (a) broadcast /
CollectLeft semi-anti-outer need a cross-probe-partition build-match bitmap reduction;
(b) the NOT-IN global "any build partition holds a NULL key" check (#80). Carriers:
tpch q4/q16/q18/q20/q21/q22 plus the tpcds outer/semi set.

**Stale 2026-09-08.** The real-8-way mode this described is deleted. The
batch-partitioned join capability matrix is what says which join shapes run on a device now,
and what it refuses it refuses by name with its own ticket.

<a id="t91"></a>
### #91 — Memory-aware GPU hash-repartition + resident-OOM in the partitioned executor
Real 8-way is only validated at generous budgets. (1) Repartition is concat-first
(`GpuCoalescePartitions` full concatenate, then scatter), spiking peak memory and
defeating the point of partitioning. (2) The resident-OOM enforcer
(`executors/stream.rs`) hangs off the streaming driver only, not ported to the CPU backend.
Payoff: a genuine tp8-mini (2 GiB real-8-way) device, inexpressible today.

**Stale 2026-09-08.** Both halves were about the legacy partitioned executor: the
concat-first repartition it emitted, and porting the streaming driver's enforcer to it. The
batch-partitioned mode scatters a batch at a time and carries its own accounting, and the
legacy executor is deleted.

<a id="t116"></a>
### #116 — Registry rows with no GPU coverage and no blocker
`hash_join` and `mixed_join` (full_table_gpu=na, no `gpu_full_table_test!` entry, no known blocker —
plain inner joins plus an aggregate; mixed_join adds a residual range filter) and
`join_int` (tp8-only oracle test, no tp1 row). Either add the missing GPU test rows or
mark the cells intentionally-na with a reason.

`nested_loop_left_join` is the fourth row and the one with a reason already: it was added for
T17's end-to-end tests, which run no device, and its GPU columns belong to T19's enablement
sweep.

**Stale 2026-09-08.** The columns it names — `full_table_gpu` and its neighbours — are
gone from the registry with the modes they described. What a query runs on a device now is a
`corpus_query!` line, and an unbacked cell fails the registry check in both directions.

<a id="t157"></a>
### #157 — legacy: the budget rule drops a CoalesceBatchesExec's fetch, and the wire cannot carry one
**Priority: low** — legacy planning only; the batch-partitioned model plans from scratch and
has no such node.

`gpu_rule.rs` ~L611 rebuilds the node as `CoalesceBatchesExec::new(input, batch_size)`, which
sets `fetch: None`, so a limit DataFusion pushed onto it is gone.

DataFusion's limit pushdown does park one there and removes the limit node once it has:
`SELECT count(*) FROM (SELECT * FROM nation WHERE n_regionkey > 1 LIMIT 3)` plans as an
aggregate over `CoalesceBatchesExec{target, fetch: 3}`. The GPU half could not carry it
regardless — `CudfCoalesceBatches` has only `target_batch_size` and the node is
`execute_passthrough`. No golden can show it either: the node's display prints estimates and
never a `fetch`, so corpus reachability is unknown rather than ruled out, and the corpus
limits that were checked survive as their own nodes. Fix is three parts — `with_fetch`
through the rebuild, an fbs field the C++ reads, and the fetch in the node display.

**Stale 2026-08-20.** Legacy planning only, and the legacy modes are not being fixed. The
three-part fix — `with_fetch` through the rebuild, an fbs field the C++ reads, and the fetch
in the node display — would extend the wire format for a planner the batch-partitioned mode
replaces, which plans from scratch and emits no such node. Whether the corpus reaches it was
never established either way.

<a id="t171"></a>
### #171 — shad-gpu's benchmark tree is in an instrumentation this repo does not use

Every record on the host carries per-node `setup_us`, `submit_us` and `device_us`; the committed
form is a single `time_us`. Someone's instrumentation change ran there and its output stayed.
Since `--pull-benchmarks` deletes nothing but overwrites everything it finds, any pull — including
one after a single filtered case — rewrites all 127 committed files into that other format, which
is what happened on 2026-08-21 and was reverted by hand.

Two ways out and they are not equivalent. Either the split instrumentation is wanted, in which
case it belongs in the repo with the goldens regenerated on purpose and the reader taught the new
lines; or it is not, in which case the host's tree should be cleared so the next pull cannot
resurrect it. Nobody has decided, and the cost of not deciding is paid by whoever pulls next
without reading `build-test.md`.

**Stale 2026-08-25.** Decided the other way round from the ticket's two options: shad-gpu is
scratch space for benchmark experiments, and the committed records in git are the canonical ones.
So neither the host's tree has to be cleared nor its instrumentation adopted — a pull brings back
whatever the host holds, and what to keep is the puller's to read before committing, which
`build-test.md` says at `--pull-benchmarks`.

<a id="t53"></a>
### #53 — Deterministic multi-partition CPU node-by-node execution
Largely superseded: `partitioned_cpu` produces deterministic tp8-standard `.cpu.txt`
goldens and `full_table_cpu` runs tp8-hinted plans single-partition. Residual: whether
recursive ftc_tp8 byte-accounting determinism still matters for any golden. Re-evaluate,
likely close.

**Stale 2026-08-05.** Largely superseded: `partitioned_cpu` produces deterministic
tp8-standard `.cpu.txt` goldens and `full_table_cpu` runs tp8-hinted plans single-partition.
Filed stale rather than done because the residual question — whether recursive ftc_tp8
byte-accounting determinism still matters for any golden — was never answered; it has simply
never failed.

<a id="t34"></a>
### #34 — Multi-partition table results in the GPU executor
The C++ executor models every node as one materialized `cudf::table`, so
GpuUnion/GpuInterleave must concatenate (peak = Σ inputs) and
Coalesce/Repartition/SortPreservingMerge are single-input pass-throughs. Introduce a real
multi-partition result type and move the concat up to GpuCoalescePartitions / final
collect. Partly overtaken by the multi-handle partitioned path — rescope to the
full-table executor before building.

**Stale 2026-08-05.** Overtaken in the part that mattered: the partitioned GPU path
keeps one `cudf::table` per output partition handle in the NodeSession registry, so a node
there is N tables, not one. The single-table constraint survives only in the full-table and
all-at-once paths — where it is why `GpuUnion`/`GpuInterleave` must concatenate — and #110
retires the all-at-once one. Rescope to the full-table executor if that path is still around
when peak memory there matters.

<a id="t29"></a>
### #29 — Track skipped TPC-DS GPU execution tests
Superseded: enablement now lives in `test_gpu_full_table.rs` / `test_gpu_partitioned.rs`
plus `testdata/cost-registry.csv`, and
each surviving bucket has its own ticket (#32, #62, #57, #55, #56, #63, #45, #46, #47,
#60, #115). Close.

**Stale 2026-08-05.** Superseded by structure rather than by work: enablement lives in
`test_gpu_full_table.rs` / `test_gpu_partitioned.rs` and `testdata/cost-registry.csv`, whose
inventory tests check both directions, so a skipped query cannot go untracked. Every
surviving bucket has its own ticket (#32, #45, #46, #47, #55, #56, #57, #60, #62, #63,
#115).
