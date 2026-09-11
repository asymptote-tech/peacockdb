# Digest B — #183, #187, #191, #192, #184, #168, #23

Read at master 188c23ce, read-only. Every contested file:line below was opened; where a review
contradicted its proposal I say which side the code supports. Paths relative to
`/media/data/peacockdb`; crate cites are the cargo-registry copies of DataFusion 45.0.0 / arrow 54.x.

Context for #183 and #187: commit 188c23ce archived `casts.md` (PR #136, "predicting the export
type at plan time and casting at the unload builds the divergence into the plan; the operator
harness reports it instead", `archive/archived-tasks.md:286`) and `wire-schema.md` (PR #137, "the
divergence it removed on the wire is now to be found by the operator harness and fixed at its
source, not carried as a per-column width", `:161`). The harness tasks (`tasks/operator-harness.md`,
`operator-cases.md`) fix nothing and pin every divergence as a `bug_` test; a later task fixes it.

## #183 — the device exports `Utf8` where the sink declares `Utf8View`

- **Status after research:** live, and misdiagnosed in the ticket text — real cause: the plan
  inherits `Utf8View` from a DataFusion *parquet-reader option* this engine never runs, not from
  any DataFusion type rule; both readers the engine does run (arrow-rs, cuDF) answer `Utf8`.
- **Issue:** `GpuExport::unload` concatenates the decoded IPC batches against the sink's declared
  schema (`executor/gpu_backend/mod.rs:179-183`); `concat_batches` demands exact types and refuses
  "expected Utf8View but found Utf8". Keeps off 60 registry rows' gpu cells (the tp1-single cell
  wherever #152 does not refuse first; all five modes for the ten rows without `152`); 29 of the
  60 also carry a `Decimal128(p<38)` sink column and would land on #187, two (tpch q7, q9) on #191.
- **Root cause:** `lib.rs:52` registers tables through `ParquetFormat::default()`, whose
  `schema_force_view_types` defaults to `true` (`datafusion-common-45.0.0/src/config.rs:430`), so
  `infer_schema` runs `transform_schema_to_view` (`datafusion-45.0.0/src/datasource/file_format/parquet.rs:370`)
  and every string field is declared `Utf8View`. The CPU backend reads `Utf8` and hides the
  difference with a per-batch cast (`cpu_backend/source.rs:112-139`, `as_declared`); cuDF has one
  string type (`cpp/src/expr.cpp:87-89`) and exports `Utf8` (`gpu_executor.cpp:74`).
- **Fix (as corrected by the review):** `lib.rs::build_session_state` sets
  `config.options_mut().execution.parquet.schema_force_view_types = false`; `read_table` builds
  `ParquetFormat::default().with_options(ctx.state().default_table_options().parquet)` so the one
  rule reaches `register_parquet` fixtures too. Scaffolding removed: `as_declared`'s cast arm
  becomes a refusal (drop the `cast` import at `source.rs:12`); the `Utf8View`/`BinaryView` arms of
  `spark_partitioning.rs:53-71 hash_keys` go. **Not** the review-corrected part: `result_text.rs:92
  schema_digest` stays names-only (doc rewritten) — see contested point. Regression test in
  `planner/translator/schema_tests.rs` (declared source types == reader's file types); four
  assertions flip `Utf8View → Utf8` (`plan_text/tests.rs:151`, `schema_tests.rs:157,286,306`).
  Frozen surfaces: none (same fb enum, different value; no ABI, no fbs, no declared-schema
  contract). Goldens: all ten `.plans.txt` (schema lists, plus q24's one `CAST(... AS Utf8View)`),
  `recipe-payloads.txt` bytes and digests under both regen variables; `.cpu.txt`/`.cost.txt`/
  `.result.txt` expected byte-identical (pricing is the same for both types, `common.rs:35,115`).
  Registry: 31 rows re-ticketed by reading (29 → `187`, 2 → `191`), 29 candidates need a shad-gpu
  run. `build-test.md:44` blocker list; if `declared-schemas` has landed, delete
  `bug_a_declared_utf8view_is_exported_as_utf8` and its `build-test.md` row.
- **Rejected approach?** A different one. `casts.md` predicted the export type and cast at the
  unload; this removes the divergence at its declaration, casts nowhere, and deletes the CPU's
  hidden cast — the "fixed at its source" the archive note asks for. Caveat (review F6, verified):
  it is a DataFusion-45 fact; later versions add a parser option typing VARCHAR/literals as
  `Utf8View`, which #23's upgrade path must also switch off.
- **Contested points:** (1) Proposal hashes `field.data_type()` in `schema_digest`; review F1 says
  that reddens five enabled shad-gpu recipe-walk tests. **Code supports the review**:
  `test_gpu_recipe_walk.rs:166-179` exports raw through `peacock_result_from_handle` (no concat
  against the sink schema) and compares via `assert_results_match → results_agree → schema_digest`
  (`tests/common/mod.rs:145`, `result_text.rs:128`); FILTER (`o_totalprice` (15,2)), SUM_BY_FLAG,
  AVG_BY_FLAG, MAX_OF_SUMS and ROLLUP (`:627-641`) all carry a decimal at the sink that the device
  exports at 38. Keep names-only. (2) Review says the type hash moves to "#187's close" — **that is
  also wrong as stated**: #187's fix sits at `GpuExport::unload`, which the walk never passes, so
  the walk's decimals still come back at 38 after #187. A typed digest needs the walk's `export`
  relabelled through the same rule, or a walk that goes through the unload. Unowned today.
- **Minimum corpus query:** `SELECT r_name FROM region`, tp1-single, gpu. Plans as `GpuUnload` over
  one loader, reaches the unload, refused there; no device vehicle for ad hoc SQL, so the committed
  stand-in is `tpch/cross-join` (off at all five gpu modes on `183` alone). Not in testdata as such.
- **Cells re-enabled / next wall:** 29 rows move to #187, 2 to #191 by reading; 29 candidates
  (10 without `152` at five modes, 19 at tp1-single) — next walls #185, the unticketed batch-split
  divergence (below), #55/#59/#80 on individual rows.
- **Overlaps and dependencies:** dissolves #192 (its `Utf8View` substrate goes with the option —
  see #192). Precedes #187's cell moves (29 rows arrive there). Collides on the ten `.plans.txt`
  with `declared-schemas` step 1 and possibly `refcounted-tables`; land before task 7 of
  `ENS-drop-mode-name` or after task 10, never during (review F2). #23's upgrade must keep the
  option off. `refcounted-tables.md:237` says #183 has no owner; this is the owner.
- **Complexity:** M — two production lines plus ~40 lines of scaffolding removal and one test (S),
  lifted to M by eleven golden regenerations (the payload golden reviewed by hand) and a 29-row
  shad-gpu rollout in ~six batches.
- **Unticketed defects found:** none new; the walk's raw export bypassing every type check is the
  gap named under contested point 2 (`test_gpu_recipe_walk.rs:166`).

## #187 — the device widens every decimal to `Decimal128(38, s)`

- **Status after research:** live; ticket framing misdiagnosed — real cause: the export is never
  told a precision, not two engine rules disagreeing.
- **Issue:** same unload site (`gpu_backend/mod.rs:179-183`) refuses "expected Decimal128(15, 2)
  but found Decimal128(38, 2)". Keeps off `tpch/filter_project` gpu × 5 and the tp1-single gpu cell
  of `tpch/hash_join`, `q2`, `tpcds/q16 q33 q61 q77 q90 q94 q95` (registry rows 17, 34, 62, 78, 91,
  95, 96, 102, 126, 127 carry `187`); plus the 29 rows arriving from #183.
- **Root cause:** `export_table_to_ipc` builds `column_metadata{name}` only
  (`cpp/src/gpu_executor.cpp:54-56`); cuDF's `to_arrow_schema` labels a decimal128
  `metadata.precision.value_or(max_precision<__int128_t>())` = 38
  (`third_party/cudf/cpp/src/interop/to_arrow_schema.cpp:117`). On 25.02 — the version shad-gpu
  runs — `column_metadata` has no `precision` member at all
  (`envs/rapids-cuda-12.2/include/cudf/interop.hpp:108-119`; 26.02 adds it at `:110`), and the C++
  never knows the declared precision anyway (`TableResult` is table + names,
  `PlanNode.output_schema` unwritten at `wire/writer.rs:102`). Queries whose sums declare (38,4)
  (q6, q19) pass by coincidence.
- **Fix (as corrected by the review):** move `declared_as` and `widened_decimal`
  (`cpu_backend/mod.rs:236-275`, `:493-506`) into a new `executor/declared.rs`, both `pub(crate)`,
  with two one-line facade items in `executor/mod.rs`; `cpu_backend/mod.rs` imports
  `super::{declared_as, widened_decimal}` — **`check_state_layout` at `cpu_backend/mod.rs:478`
  also calls `widened_decimal`** (review 2.1, verified), so the proposal's private move would not
  compile. At `gpu_backend/mod.rs:176-183` map every decoded batch through `declared_as` before
  `concat_batches`; the cast is `safe: false`, so a value not fitting the declared precision refuses
  rather than nulls. Every other type difference (Int16/Int32, Utf8/Utf8View) still refuses.
  Tests: three unit cases in `declared.rs` (moved CPU case, wider-same-scale relabel, other-scale
  and other-kind refused); one device test in `test_gpu_executors/exec.rs` (project casting `v` to
  `Decimal128(15,2)` through `one_node`, red today). Frozen surfaces: none. Goldens: none move
  (`Decimal128` is 16 bytes at any precision, `common.rs:39`; rows render digits). Registry row
  126 → enabled ×5; rows 17/34/62/78/91/95/96/102/127 re-ticketed after a run. `build-test.md`
  Lib unit +3, device executors +1, grand total 1569 → 1573. Ticket text corrected at close; #190's
  quote of the old CPU message ("DataFusion answered with") moves with it.
- **Rejected approach?** Partly each. Not `wire-schema.md` (no width on the wire, no C++ change —
  and on 25.02 that route cannot work, since `column_metadata` has no precision). It **is** the
  cast-at-the-unload half of `casts.md`, minus the plan-time prediction, the `exports=` rendering
  and the reason enum. What authorises it is the approved `tasks/declared-schemas.md:136-137`:
  "`declared_as` already asks it on the CPU side, and that is where the production fix will
  start." The human should confirm that the rejection of casts.md was about the prediction and not
  about any Rust-side cast at the boundary; if it was about the cast, the only remaining shape is a
  new ABI argument handing the C++ the declared schema (proposal alt. 3, a seventeenth symbol).
- **Contested points:** (1) private `widened_decimal` — review right (above). (2) Proposal says up
  to nine tp1-single cells "may go green"; review 2.2 says none can. **Code supports the review**:
  the CPU join returns DataFusion's output as it comes (`cpu_backend/join.rs:236-251`), chunked at
  `batch_size` 8192 (`tp1-single-mini.cpu.txt:814` hash-join `batch_rows=[[8192 ×732, 4671]]`,
  aggregate `[[1 ×733]]`, merge `in_rows=[[733]]`); the device join emits one batch per probe call
  (`gpu_backend/join.rs:164-180`, `out.extend(prior)`); the device tier compares the whole section
  byte for byte (`tests/common/corpus_gpu.rs:101` → `corpus_golden::assert_section:111-119`). So
  only `filter_project` (filter is 1:1 on both engines, `CpuExec::exec` concatenates at
  `cpu_backend/mod.rs:193`) can go green: 5 cells, not 14. (3) Review 2.3: `declared-schemas.md:7-17`
  and `tasks.md:95` state `declared_as` is CPU-private and the device has no equivalent — both
  become false; flagged for the human, not edited (frozen spec).
- **Minimum corpus query:** `tpch/filter-project` (`testdata/tpch-queries/filter-project.sql`,
  `l_quantity:Decimal128(15,2)`), any mode, gpu — in testdata, off at all five on `187` alone. The
  ad hoc `SELECT s_acctbal FROM supplier` has no device vehicle; the device unit test is the
  minimal reproduction.
- **Cells re-enabled / next wall:** `tpch/filter_project` gpu × 5. The nine tp1-single cells shed
  `187` and land on the unticketed batch-split divergence (q2 on #183 first); their tp4/rowgroup
  cells stay on #152/#175/#180.
- **Overlaps and dependencies:** receives 29 rows from #183. Moves `widened_decimal`, which
  #163's state-layout work (`check_state_layout`) also reads — coordinate. Makes the harness's
  `decimals` fixture cases and `operator-cases-impl.md`'s expected #187 `bug_` tests green or
  deleted. Rebases trivially across `sink-divergence-survey` (rewrites `gpu_backend/mod.rs:176-184`
  on a throw-away branch) and `test-layout` (moves `tests/test_gpu_executors/`).
- **Complexity:** S code (six files, ~150 LOC incl. tests, no golden), M to close — one shad-gpu
  cycle plus re-attributing nine rows to a wall that needs a ticket first.
- **Unticketed defects found:** the CPU/GPU batch-boundary divergence above (`cpu_backend/join.rs:236-251`
  vs `gpu_backend/join.rs:164-180`) — the CPU oracle authors join `batch_rows` the device cannot
  match at any node above a join whose matched output per call exceeds 8192 rows.

## #191 — the device exports `Int16` for `extract(year …)` declared `Int32`

- **Status after research:** live.
- **Issue:** `cpp/src/expr.cpp:659-660` returns `cudf::datetime::extract_datetime_component`
  as is; cuDF answers every component in `int16_t`
  (`envs/rapids-cuda-12.2/include/cudf/datetime.hpp:248-255`), the type rides silently through
  aggregate, sort and export, and the unload refuses "expected Int32 but found Int16". Keeps off
  `tpch/q8` gpu at tp1-single (registry `:108`, `152 191`); q7 and q9 would arrive here after #183.
- **Root cause:** the wire carries the declared type — `ScalarFunctionExprNode.return_type`
  (`gpu_plan.fbs:198-214`, written at `wire/expr_writer.rs:150-179`) — and the only C++ reader is
  `infer_expr_type` (`expr.cpp:395-397`), used for AST routing; the `date_part` arm never reads it.
  `is_ast_able` refuses every scalar function (`:403-406`), so the column path is the only path.
- **Fix (as corrected by the review):** in the `date_part` arm, `cudf::cast(component->view(),
  cudf::data_type{fb_to_type_id(sf->return_type())})` — two lines, unconditional (DataFusion 45
  types every field but `epoch` as `Int32`, and `epoch` is refused at `:658`). Comment cites the
  `CastExprNode` arm (`expr.cpp:912-935`) as the precedent, not the loader (which reads no declared
  type). Regression test in `test_gpu_executors/exec.rs`: a `GpuProject` of
  `date_part('YEAR', Date32(9204))` through `one_node`, asserting `Int32` and 1995 × 6, red today.
  Reword `test_inc2_conformance.rs:175-178`'s doc (it names `extract_year` as why an INT16 key
  exists). Frozen surfaces: none. Goldens: none move. `architecture.md`: two table rows only (`##
  cuDF options`, the flat-buffers table) — **not** the "two stay in C++" paragraph at `:362`,
  which lists casts the plan does not carry (review 2.2, right). `build-test.md:21` 31 → 32 with
  the case named in the prose, totals 1569 → 1570.
- **Contested points:** proposal §6 says q8 tp1-single then lands on #185; review 2.1 says it
  lands on the batch-split wall and #185 is unobservable there. **Code supports the review**:
  `tp1-single-mini.cpu.txt:269-300` shows the CPU's `GpuAggregate` emitting six batches
  (`batch_rows=[[0,0,1,2,1,0]]`) from a join chunked at 8192, and `GpuAggregateBatches
  in_rows=[[4]]`; on the device one probe batch → one join batch → one aggregate batch of two
  groups → merge true input 2 = own output 2, so the line differs with or without #185. The
  registry edit therefore needs a new ticket first — `cost-report/src/main.rs:442-460` exits 1 on
  a registry ticket that resolves nowhere.
- **Minimum corpus query:** `SELECT extract(year FROM o_orderdate) AS o_year FROM orders WHERE
  o_orderkey < 8`, all five modes, gpu; plans as scan → filter → project → unload, refused at the
  unload today. Not in testdata; adding it as `tpch/extract-year.sql` would be the only green
  device cell this fix produces (five cpu + five gpu).
- **Cells re-enabled / next wall:** none from the existing corpus — q8 tp1-single sheds `191` and
  stops on the batch-split wall; the other four modes stay on #152. q7/q9 no longer land here.
- **Overlaps and dependencies:** ordered after #183 only for cell accounting; independent in code.
  `declared-schemas.md:219` and `operator-cases.md` expect this as a `bug_` test — whichever lands
  second deletes/flips it. `test-layout` moves both test files this edits.
- **Complexity:** S for the fix (two lines, one ~30-line device test, comment rewording); M if
  the corpus query is added (five-mode CPU regeneration plus a device run for the five gpu cells).
- **Unticketed defects found:** the batch-split divergence again; the review's reading that #185's
  "every `batch_rows` entry agrees" is not literally true of `tpch/q3` and `q14` at tp1-single
  (`tp1-single-mini.cpu.txt:98`, `:508`, 8192-chunked joins) — though #185 itself is real: q93's
  join emits 36 per-probe batches on both engines and the merge reports 7169 against 7486
  (`tpcds.sf1/tp1-single-mini.cpu.txt:4937-4940`).

## #192 — tpcds/q64 takes 13 GB of host RSS

- **Status after research:** misdiagnosed (real cause: arrow 54's `Utf8View` kernels multiply
  buffer-reference lists along q64's join chain; the `Utf8View` columns exist only because of the
  reader option #183 removes) — wall behind #183.
- **Issue:** the CPU run of q64 grows past 13 GB and is killed; nothing refuses the plan
  (`tests/common/corpus.rs:65` runs unbudgeted). Keeps off q64 cpu × 5 and the gpu cells behind
  them (`corpus_cases.inc:268`, registry row 65). The ticket's "no budget stops it" is a symptom.
- **Root cause:** `as_declared` casts every scan's `Utf8` to `Utf8View` (`source.rs:112-139`), and
  arrow's `From<&GenericByteArray>` reuses the values buffer as one block
  (`arrow-array-54.2.1/src/array/byte_view_array.rs:666-696`). `take_byte_view` copies the whole
  `data_buffers().to_vec()` of the side it gathers from (`arrow-select-54.2.1/src/take.rs:547-557`);
  `concat` has no view arm and `MutableArrayData` collects every input's buffers
  (`arrow-data-54.3.1/src/transform/mod.rs:630-635`). q64 is seventeen Inner joins per branch, each
  build side the `GpuCoalesceAllBatches` over the previous join (`tpcds.sf1/tp1-single.plans.txt:6381-6451`),
  each join output chunked at 8192, so `s_store_name`/`s_zip` carry Π⌈R/8192⌉ buffer references
  — geometric in the chain depth; 24 bytes per `Buffer` reaches GB. cuDF has no view arrays; the
  device side cannot multiply.
- **Fix (as corrected by the review):** no production code. After #183's CPU commit the plan
  declares `Utf8`, `as_declared` never casts, and `take`/`concat` for `Utf8` copy bytes into fresh
  buffers (`take.rs:463-544`, `concat.rs:203-266`) — a constant buffer count, nothing to multiply.
  Then: `corpus_cases.inc:268` q64 → all five cpu modes, `none` gpu; registry row 65 cpu → enabled,
  gpu tickets `152 183 185 187` (a device run decides); eleven fresh golden sections (five
  `<mode>-mini.cpu.txt`, five `.cost.txt`, one `mini.result.txt`) written by `PCK_TEST_FILTER=q64
  UPDATE_CANONICAL=1`, every existing section byte-identical; `build-test.md:7,42` counts (the
  "37 queries" clause is already drift); #192 archived with the mechanism and the versions
  (DataFusion 45 / arrow 54) it was read against. Measure peak RSS and wall time per mode on a
  15 GiB host before flipping the row. Frozen surfaces: none.
- **Contested points:** proposal adds `compact_views` (DataFusion's `gc_string_view_batch` recipe)
  in `cpu_backend/accumulate.rs::one_batch`; review says #183 removes its only input. **Code
  supports the review**: with `schema_force_view_types = false` no `Utf8View` reaches the CPU path
  in DataFusion 45 (no parser option maps strings to views; string functions return the input
  type), so the `as_string_view_opt()` arm would have no reachable input — a guard for an
  impossible case. Also review F2: the proposal's §5 chain query would be swapped by
  `JoinSelection` (`join_selection.rs:61-85`, exact `total_byte_size` from `collect_stat: true`,
  `listing/table.rs:298`) so the chain sits on the probe side and does not multiply; q64 keeps its
  chain on the build side only because an aggregate below the chain makes the estimate `Absent`.
  The review's aggregate-based variant, or q64 itself, is the reproduction. Review F3 (the copied
  heuristic rebuilds every short-string column at every emit) is a correct reading of
  `get_buffer_memory_size` but moot.
- **Minimum corpus query:** q64 at tp1-single, cpu (in testdata). For the mechanism, the review's
  aggregate-rooted six-lookup chain over `store_returns` — verify in plan text that every
  `GpuHashJoin` has `GpuCoalesceAllBatches` as its first child before reading an RSS figure.
- **Cells re-enabled / next wall:** q64 cpu × 5 (if the measured run fits); gpu × 5 stay on
  #152 (36-chunk probe streams even at tp1-single), then #183/#187/#185.
- **Overlaps and dependencies:** dissolved by #183 (its CPU commit; the shad-gpu rollout need not
  finish). If a human wants q64 back before #183, the proposal's `compact_views` is the right
  shape but carries a deletion obligation. #23's upgrade re-checks this class (later arrow gives
  `concat` a compacting view arm; later DataFusion can reintroduce views by parser option).
- **Complexity:** S — no code once sequenced after #183; one measured five-mode run and the
  golden sections it writes.
- **Unticketed defects found:** the CPU accountant's `get_array_memory_size` over-counts shared
  buffers at full capacity (`cpu_backend/backend.rs:131`, `join.rs:218`) — a budget would have
  tripped from the over-count, not from the model; belongs with #182, not filed here.

## #184 — `CudfRepartition{Hash, 1 -> 4}` dies in `spark_hash_partition.cu:179`

- **Status after research:** misdiagnosed (real cause: the shuffle key is a decimal and the
  device hash kernel has no decimal arm — this is #95 reached by a corpus query; the 1→N shape
  is routine).
- **Issue:** `spark_hash_partition.cu:163-185`'s key-type switch lists STRING/INT32/INT64 only
  (after normalising INT8/16, TIMESTAMP_DAYS, DICTIONARY32); `default:` is the `CUDF_FAIL` at
  `:179`. q15's `#11` is `GpuEmitPartitions: hash=[total_revenue@4]` over
  `total_revenue:Decimal128(38,4)` (`tpch.sf1/tp4-rowgroup.plans.txt:795`). Keeps off q15 gpu at
  the three tp4 modes only — the two tp1 plans have no `GpuEmitPartitions`, so those cells are
  #183 alone (00-tickets.md's "× 5" corrected). Latent for seven more decimal-keyed shuffles
  (tpch q10, q18, q2; tpcds q24, q37, q82 at p ≤ 18, q75 at (31,15)) behind their own tickets.
- **Root cause:** comet dispatches by declared precision — `p ≤ 18` hashes the unscaled value as
  8 LE bytes, wider as 16 (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:107-146,
  299-304`); the CPU passes `Decimal128` arrays straight to comet (`spark_partitioning.rs:42`).
  cuDF's `data_type` is `{id, scale}` with no precision, and the repartition arm reads ordinals
  only (`cpp/src/node_session.cpp:388-402`); `PlanNode.output_schema` is never written
  (`wire/writer.rs:102`) though `Field.decimal_precision` exists and `serialize_schema` fills it
  (`wire/serialize.rs:136-172`).
- **Fix (as corrected by the review):** Rust: `Writer::node` delegates to a `node_with_schema`
  (one body, not two); `attach.rs::emit_partitions` writes the emit node's declared schema as
  `PlanNode.output_schema` for that node only; `fb_text.rs` renders it, and `schema_text` prints
  `Decimal128(p,s)` the way `plan_text`'s `type_text` does. C++: `partitioning.hpp` gains
  `struct HashKey { column; decimal_precision }` and both entry points take `vector<HashKey>`;
  a `spark_hash_decimal128_col_kernel` hashing `width` LE bytes of the `__int128_t` (8 if p ≤ 18,
  else 16); the repartition arm reads `output_schema`, refuses its absence, and **also checks the
  column's scale equals `-decimal_scale`** (review F4 — a scale drift would misplace silently);
  `gpu_executor.cpp`'s conformance hook derives precision from the Arrow C-data `format` string
  (`d:P,S`), so the ABI signature is unchanged. Tests, red before: three `*_match_comet_live` gates
  in `test_inc2_conformance.rs` ((15,2), (38,4), composite); **one placement test through the wire**
  in `test_gpu_executors/join.rs` (recipe-built scatter on a cast decimal key, per-lane rows against
  `create_murmur3_hashes` — review F1: the recipe-walk query alone compares a multiset and cannot
  see a wrong width); the walk query on `l_discount` added to `PROVEN`'s cover; one gtest.
  Frozen surfaces: **no** ABI symbol, **no** fbs change; the public C++ header `partitioning.hpp`
  changes signature (three in-repo callers). Goldens: `recipe-payloads.txt` bytes and digests for
  every shuffle query plus a `schema:` line per repartition (both regen variables); no plan,
  execution, cost or result golden. Registry `:115` `183 184` → `183`; `corpus_cases.inc:26-32`,
  `build-test.md:44` and three count rows (murmur3 10 → 13, cuDF smoke 5 → 6, walk 10 → 11,
  executors +1); #184 and #95 both archived; `tickets.md:775-776`'s #195 bullet rewritten;
  `architecture.md:824, 960-972` and the flat-buffers table gain the `output_schema` fact.
- **Rejected approach?** Not asked for this ticket, but note: writing `PlanNode.output_schema` is
  the mechanism `wire-schema.md` was rejected for ("not carried as a per-column width"). Here it is
  one node kind, for a fact the kernel genuinely cannot compute (comet's 8/16 rule needs p), not an
  export label. The no-wire alternatives are a `CudfRepartition` precision field (fbs change) or
  giving up comet-exactness on decimal keys. The human should say whether the rejection reaches it.
- **Contested points:** review F3 — the proposal's 16-byte query `GROUP BY l_extendedprice *
  l_discount` is refused at the Partial aggregate (`aggregate.cpp:162`, ColumnRef keys only)
  before any shuffle; **code supports the review**; use a subquery projecting `x` first. F5 —
  "`aggregate.cpp:217-221` casts the sum to DECIMAL128" is the `avg` input cast; conclusion
  unaffected. F8 — the i64-overflow risk is unreachable because `declared_as` casts `safe: false`
  before the emitter. All three verified.
- **Minimum corpus query:** 8-byte path `SELECT l_discount, sum(l_quantity) FROM lineitem GROUP BY
  l_discount` (`l_discount:Decimal128(15,2)`); 16-byte path `SELECT x, sum(l_quantity) FROM
  (SELECT l_extendedprice * l_discount AS x, l_quantity FROM lineitem) GROUP BY x` — tp4 modes,
  gpu; both plan, run on the CPU, and die at the first `CudfRepartition` call with `type_id=27`.
  Neither in testdata; q15 at tp4 is the committed carrier (16-byte path only).
- **Cells re-enabled / next wall:** none flip — q15's tp4 cells stay on #183 (three `Utf8View`
  sink columns), then #185 or the batch-split wall. Latently unblocks the seven rows above.
- **Overlaps and dependencies:** #95 (same defect, closes together); #189 (`UInt8` grouping id
  refused by the same switch's neighbour on the CPU side) is separate. `test-layout` moves both
  test files; `operator-cases` plans this scatter case as a `bug_` test (`operator-cases-impl.md:833`);
  `declared-schemas.md` §4 defers "schemas on the wire" and the `schema_text` drift — this is that
  later task for one node. Land before task 4 or after it, never during.
- **Complexity:** M — ~10 files, ~200 LOC across two languages, three shad-gpu binaries plus the
  payload regen on the symlinked host.
- **Unticketed defects found:** `fb_text::schema_text` (`:221-233`) prints a bare enum for a
  decimal field, so precision/scale on the wire are invisible to the payload golden (also noted in
  `declared-schemas.md` as drift owed a ticket).

## #168 — fbs `ScalarValue` has no interval variant

- **Status after research:** live; ticket text drift — the writer substitutes nothing any more
  (`wire/writer.rs:50-54`), the whole plan fails and the golden reads `not runnable:`.
- **Issue:** `tpch/mixed-join`'s residual `l_shipdate BETWEEN o_orderdate AND o_orderdate +
  INTERVAL '90' DAY` keeps an `IntervalMonthDayNano` literal; `serialize_scalar_value` falls to
  `unsupported scalar value` (`wire/serialize.rs:100-102`), `expr_writer.rs:32-37` wraps it with
  `(#168)`, `attach_recipes` fails, five plan goldens print `not runnable` (`tpch.sf1/tp1-single.plans.txt:158`
  etc.). Keeps off mixed_join gpu × 5 (registry `:130`, `116 168`); no cpu cell. The only unfolded
  interval in either bench.
- **Root cause:** one missing spelling on the wire. On the device the residual takes the column
  path: `is_ast_able` refuses any binary holding a literal whose `fb_to_type_id` is `EMPTY`
  (`expr.cpp:429-430`), so `build_column_binary`'s rhs-literal path (`:610-617`) would call
  `cudf::binary_operation(timestamp_D, duration_D scalar, ADD, TIMESTAMP_DAYS)` — epoch-day
  addition, the same arithmetic arrow does for a whole-day interval. The gap is `build_scalar`
  (`expr.cpp:453-496`) having no arm.
- **Fix (as corrected by the review):** append `IntervalMonthDayNano` to `enum DataType` and
  three fields (`interval_months/days/nanos`) to `table ScalarValue` in `gpu_plan.fbs` (appended,
  so no ordinal moves and no existing payload byte changes — FlatBuffers omits defaults);
  `serialize.rs` writes whole-day intervals and refuses months or sub-day parts **with the
  `unsupported scalar value:` prefix kept** (review F3 — `wire/tests.rs:630` asserts on it);
  `convert_data_type` maps `Interval(MonthDayNano)`; `expr_writer.rs` drops the `(#168)` wrap;
  `fb_text::scalar_text` renders the triple; `build_scalar` returns a
  `duration_scalar<duration_D>`, refusing months/nanos by name. **Plus** (review F2): refuse an
  interval literal inside a `LeftSemi | LeftAnti | LeftMark` join filter at write time in
  `wire/join.rs` — those residuals go through `build_expr`'s AST ungated (`join.cpp:98`, `:233`)
  and the AST has no `timestamp + duration`. Pinning tests that move: `test_plan_goldens.rs:375-380`
  `assert_eq!(uncrossable, ["tpch mixed-join"])` becomes `is_empty()` (review F1 — the proposal
  missed it; verified), `NOT_RUNNABLE` (`:400`) empties, `PAYLOAD_QUERIES` gains mixed-join **after**
  the digest-verify run (review F4 — with it added first the map-key mismatch reads like a moved
  byte), `wire/tests.rs:578-632` re-pointed at a month interval, two doc examples. Device: a walk
  test `a_join_residual_that_adds_days_to_a_date_answers_on_the_device` (the only end-to-end proof;
  no cell can carry one). Frozen surface: **the fbs moves** (appended member and fields; no ABI
  symbol; no existing wire byte). Goldens: mixed-join's `--- recipes ---` section in five
  `.plans.txt`, a new `recipe-payloads.txt` section. Registry `:130` → `116 152 187`;
  `corpus_cases.inc:80-82` comment; #168 archived; `declared-schemas.md:226-228` becomes false but
  is a frozen spec — note it in the archive entry, do not edit (review F7).
- **Contested points:** F1, F3, F4 as above — all verified against the code and all against the
  proposal. F2 (three shapes move from plan-time to run-time refusal) — verified for the
  semi/anti/mark residual (`join.cpp:98`, `:233`) and the bare literal in a project
  (`project.cpp:45-49` sends an AST-able literal to `build_expr`, whose `default:` throws at
  `expr.cpp:250-253`); the writer-side refusal keeps the first plan-time, the other two are named
  in the archive entry (or one ticket). F6: "paths the corpus already runs" leaned on q17/q3/q10,
  which have no enabled device cell — the walk test is the proof.
- **Minimum corpus query:** `SELECT count(*) FROM orders WHERE o_orderdate + INTERVAL '90' DAY <
  DATE '1993-01-01'` — all five modes, gpu; `not runnable` at plan time today at `#1`; not in
  testdata. `tpch/mixed-join` is the committed carrier (in an Inner residual).
- **Cells re-enabled / next wall:** none directly; mixed_join's five gpu cells move from "cannot
  cross" to `hash_join`'s state — tp1-single on #187 (sums declared (25,2) come back 38; confirm
  with one `PCK_TEST_FILTER=mixed_join` device run rather than infer), the other four on #152.
- **Overlaps and dependencies:** `typed-nulls` (task 11) replaces `build_expr`'s literal arm with a
  dispatch to `build_scalar` — either order is one line. #23's q72 rewrite was chosen to avoid
  intervals because of this ticket; with #168 landed a `Date32 + INTERVAL` form also crosses, but
  the Int32 form stays simpler. `walk-drives-every-plan` counts #168-class refusals among what it
  proves. Not a cell-bearing change; the ticket itself said one query is a thin case for a
  surface change.
- **Complexity:** M — ~25 production lines in five files, ~100 lines of tests, but an fbs append,
  five golden sections, the payload golden under both variables on the symlinked host, and one
  shad-gpu cycle for the walk plus a device corpus filter run.
- **Unticketed defects found:** `interval + date` with the literal on the left declares a
  `DURATION_DAYS` output through `binop_output_type` (`expr.cpp:618-625`) and cuDF would refuse at
  run time; a bare interval literal in a project reaches `build_expr` and throws. Neither is a
  corpus shape; both are refusals and so ticket material.

## #23 — DataFusion 45 cannot physical-plan q27/q70/q72/q86

- **Status after research:** misdiagnosed (real cause: three different defects, and two of the
  four queries can never run here) — live for q27 and q72, with localized fixes that need no upgrade.
- **Issue:** four rows are `plan_status=fail`, all fifteen cells `na` (registry rows 28, 71, 73,
  87), no `corpus_query!` line. q27: `SanityCheckPlan` refuses a `SortPreservingMergeExec` over a
  `UnionExec` with `Child-0 order: []` (`tpcds.sf1/tp1-single.plans.txt:2633`). q72: `Cannot coerce
  arithmetic expression Date32 + Int64` (`:7431`). q70/q86: `Physical plan does not support …
  Grouping` (`:7320`, `:9549`) — **but both are `rank() OVER` window queries**
  (`testdata/tpcds-queries/q70.sql:5`, `q86.sql:5`), which the translator refuses on any DataFusion
  (`planner/translator/nodes.rs:159-163`, #143 archived "not planned"). Their cells can never be
  enabled; the ticket's "unblock q70/q86" and #65's "(q70/q86 after #23)" (`tickets.md:282`) are
  wrong for this engine.
- **Root cause:** q27 — `calculate_union_binary` folds branches with the accumulated constant set;
  `try_add_ordering`'s `constants.is_empty() && ordering_satisfy(..)` guard
  (`datafusion-physical-expr-45.0.0/src/equivalence/properties.rs:2261`, verified) skips the direct
  check once any branch contributed a constant (`g_state`), and a branch with no ordering of its
  own (all its sort keys constant, so `add_sort_above` adds nothing) drops the union's ordering to
  `[]`; the sanity check then refuses. q72 — `TypeCoercion` defers to arrow's `add_wrapping`, and
  `arrow-arith/src/numeric.rs:645-700`'s date arm has no integer case; no DataFusion or arrow
  version read here adds one, so the "46+ fixes it" claim is unsupported for q72.
- **Fix (as corrected by the review):** two engine rules on the one session every planning site
  builds (`lib.rs::build_session_state`). (1) `planner::MergeInputSort`, a `PhysicalOptimizerRule`
  inserted before `SanityCheckPlan` (`with_physical_optimizer_rules` on the list from
  `base.state().physical_optimizers().to_vec()`; `with_physical_optimizer_rule` appends after the
  gate and is wrong): under a `SortPreservingMergeExec` whose input does not satisfy its ordering,
  add a `SortExec` with the merge's fetch and `preserve_partitioning`; inert wherever the sanity
  check passes, so no other golden moves. The translator then emits `GpuMergeSortedPartitions →
  GpuSort → GpuUnion` with no code change. (2) `planner::DateDayArithmetic`, a `FunctionRewrite`
  (runs before `TypeCoercion` with the merged schema): `Date32 ± integer` → `CAST(CAST(d AS Int32) ±
  CAST(n AS Int32) AS Date32)` — days, DuckDB's meaning, every piece already crosses the wire and
  the C++ (`Int32 ↔ Date32` is an arrow reinterpret; `cpp/src/expr.cpp:912-935` routes the cast).
  Both structs `#[derive(Debug)]`. Tests: two `bug_` tests on a plain session (the two-branch union
  of review finding 3; `SELECT DATE '2000-01-01' + 5`) that go red the day DataFusion no longer
  needs the rule; two positive planner tests (assert the **optimized** plan for the cast chain);
  two end-to-end cases (`sum(n_nationkey)` per branch; `sum(l_quantity)` over `l_receiptdate >
  l_shipdate + 5`, avoiding #180's `count(*)` shape). Frozen surfaces: none. Goldens: q27 and q72
  sections in the five tpcds `.plans.txt` (refusal → tree); `recipe-payloads.txt` should not move
  (no new kind or call shape); **plus, for every cpu mode q72 ends up enabled at, its
  `<mode>-mini.cpu.txt`, `.cost.txt` and `mini.result.txt` sections** (review finding 1 — the
  proposal listed the plan files only). Registry rows 28/73 plan cells → enabled with cpu/gpu
  `disabled` on `163 152 183` (q27) and `180 152 183` plus the measured tp1 verdict (q72); rows
  71/73 drop archived `97`; row 87 carries no `97`. `tickets.md#t23` rewritten (two rules are the
  scaffolding; the upgrade removes at most one), `#t65` loses "(q70/q86 after #23)".
  `architecture.md` Planning gains one clause naming the two session rules, same commit.
- **Contested points:** none load-bearing; the review confirms all three mechanisms and adds
  cost. Finding 7 (q72's two LEFT OUTER JOINs arrive as `Right` after `JoinSelection`'s swap, as
  q80's do) changes the reason on the gpu line, not the cells.
- **Minimum corpus query:** q27's defect — `SELECT r, n, g FROM (SELECT n_regionkey r, n_nationkey
  n, 0 g FROM nation UNION ALL SELECT NULL, NULL, 1 FROM nation) t ORDER BY r, n LIMIT 10` (two
  branches suffice; `g` must be selected). q72's — `SELECT count(*) FROM lineitem WHERE
  l_receiptdate > l_shipdate + 5`. Both refused by DataFusion before the engine's planner, every
  mode, both backends; neither in testdata.
- **Cells re-enabled / next wall:** plan cells only — q27 × 5, q72 × 5. q27 cpu stays on #163
  (four `avg`s), gpu on #152/#183. q72 cpu tp4 on #180; cpu tp1 decided by a measured run — its
  FROM-order first join `catalog_sales ⋈ inventory` (~1.44M × ~650 pairs before any filter, #20)
  is paid by the DataFusion oracle too (`corpus.rs:149,168`), so both engine and oracle may not fit
  a 15 GiB runner; fallback is a #192-style disabled line. q70/q86: never, under #143.
- **Overlaps and dependencies:** #183's fix is a DataFusion-45 config fact — the localized path
  here keeps it valid, and any future upgrade must switch the view-type parser option off. #168
  is why the q72 rewrite avoids intervals. #65 becomes moot for q70/q86. #166 names a DataFusion
  fix "in 46.0.0" and is the one concrete reason for the upgrade this ticket does not supply. The
  exec-model corpus is out of scope (q27 runs nowhere under #163; q72's hand lowering is its own
  task).
- **Complexity:** S code (two ~40-line rule files, ~10 lines in `lib.rs`, six tests), M task —
  ten golden sections, a measured q72 run whose oracle may not fit CI, and the execution goldens
  that run authors.
- **Unticketed defects found:** none in the engine. Registry drift: rows 71/73 carry archived
  `97`; #65's text points at a future that cannot happen.

## Cross-ticket observations

- **One unticketed wall behind four of these tickets.** The CPU join returns DataFusion's
  8192-row chunks per probe call (`cpu_backend/join.rs:236-251`); the device join returns one batch
  per call (`gpu_backend/join.rs:164-180`); the device tier compares the CPU-authored section byte
  for byte (`corpus_gpu.rs:101`, `corpus_golden.rs:111-119`). Every node above a join whose matched
  output per call exceeds 8192 rows differs in `batch_rows`, and the merge's `in_rows` differs
  regardless of #185. Named independently by the #187 and #191 reviews; it is where nine of
  #187's ten rows, #191's q8, and #183's candidates with a large join land. It has no ticket, and
  `cost-report` refuses a registry ticket that resolves nowhere, so it must be filed before any of
  those registry edits. A fix candidate is on the CPU side: `CpuJoin::probe_and_fetch` concatenating
  its pieces the way `CpuExec::exec` does (`cpu_backend/mod.rs:193`), at the cost of every join
  `batch_rows` line in the CPU goldens. #185's text ("every `batch_rows` entry agrees") was written
  from cells where the join emits one batch per probe (q93: 36 per-probe batches on both sides).
- **#183 dissolves #192 outright** and moves 31 of its own rows onto #187 and #191 by reading.
  Sequence: #183's CPU commit first; #192 is then a re-enable with a measured run; #187 and #191
  are independent in code but their cell accounting depends on #183's rollout.
- **`schema_digest` hashing types is owned by nobody.** #183's proposal wanted it, #183's review
  defers it to #187's close, #187's proposal defers it to #183 — and the recipe walk's raw export
  (`test_gpu_recipe_walk.rs:166-179`) bypasses the unload where #187's cast lives, so even after
  both land a typed digest reddens five walk tests. It needs the walk's export relabelled by the
  same `declared_as` (reachable once `test-layout` moves the walk into `src/`), then the hash.
- **Two proposals put a per-column decimal precision back on the wire or at the boundary.** #187
  casts at the unload (the half of `casts.md` the archive note names, authorised by
  `declared-schemas.md:136-137`); #184 writes `PlanNode.output_schema` for the repartition node
  (the mechanism of the rejected `wire-schema.md`, for a fact the kernel cannot compute). Both are
  defensible on 25.02, where `column_metadata` has no precision; both should get one explicit
  human decision rather than two implicit ones.
- **Frozen specs falsified by these fixes, none editable by the fixer:** `declared-schemas.md:7-17,
  136-137, 219, 226-228` (by #187, #191, #168), `operator-harness.md`'s `decimals` fixture
  paragraph and `operator-cases-impl.md`'s expected #187/#184 `bug_` tests (by #187, #184),
  `walk-drives-every-plan.md:126` (by #184/#95). Every fix landing after a harness task deletes
  the `bug_` test it makes green; landing before means the test is never written.
- **Chain placement is the shared first-hour hazard.** All seven touch files `test-layout` moves
  (`tests/test_gpu_executors/`, `test_inc2_conformance.rs`, the recipe walk, `tests/common/`) or
  goldens `declared-schemas` regenerates; the reviews converge on "before task 4 or after task 10,
  never during".
- **DataFusion-45 specificity.** #183 (the reader option), #192 (arrow 54 view kernels), #23's
  `MergeInputSort` (the constants guard) are all version facts with `bug_`-style tripwires; #23's
  `DateDayArithmetic` and #191's cast are not — they are dialect and kernel-width rules that survive
  any upgrade. The upgrade #23's title asks for removes at most one of the three and costs every
  golden.
- **Contradictions between proposals:** #187 says the type hash is #183's item; #183's review says
  it is #187's. #191's proposal names #185 as q8's next wall; its review and #187's review name the
  batch-split wall. #184 says q13's 1→4 shuffle runs on a device; q13's device cells are all off on
  #152 (`registry:113`), so the shape is proved only by `a_scatter_answers_with_one_handle_per_lane`
  and the walk's `TWO_LANES` cases.
