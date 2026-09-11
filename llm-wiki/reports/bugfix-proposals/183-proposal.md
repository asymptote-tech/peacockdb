# #183 — proposal: the plan declares the string type both readers produce

Read at master c18e063a. Paths are relative to `/media/data/peacockdb`. DataFusion 45.0.0,
datafusion-common 45.0.0 and parquet 54.2.1 sources cited are the ones in `Cargo.lock`, read from
the cargo registry. Nothing was built or run; every claim below is by reading, and section 7 names
the ones only a run can settle.

## 1. Issue

On a device, every query whose sink carries a string column is refused at the unload, after the
whole plan has run: `executor/gpu_backend/mod.rs:179-183` concatenates the decoded IPC batches
against the sink's declared schema, `concat_batches` ends in `RecordBatch::try_new`, and the
message is

    GpuUnload lane 0: the exported stream is not the sink's rows: column types must match schema
    types, expected Utf8View but found Utf8 at column index N

The declaration says `Utf8View`; the device's export is `Utf8` (`cpp/src/gpu_executor.cpp:74`,
`cudf::to_arrow_schema`, which maps cuDF's one string type `STRING` to `arrow::utf8()` and can map
it to nothing else). Same bytes, same values, two Arrow spellings of "string".

What it disables — `testdata/cost-registry.csv`, 60 rows carrying `183` in the `tickets` column, and
the T19 comments at `peacockdb-core/tests/common/corpus_cases.inc` 29-30, 47-48, 59-60, 78, 95-96,
107, 120, 146, 170-171, 180, 226, 263 — is the gpu column only, and the 00-tickets.md row is right
about the shape: the tp1-single cell of every query whose join sees one probe batch (the other four
modes refuse earlier on #152), and all five modes for the ten rows that carry no `152`: tpch
aggregate_groupby anti_join cross_join nested_loop_join nested_loop_left_join semi_join
shuffle_additive shuffle_stddev q4, and q15 (which stays off on #184). Every one of the 60 has a
`Utf8View` in its tp1-single sink schema, checked over `testdata/goldens/*/tp1-single.plans.txt`
(the node under `GpuUnload`); no `183` row lacks one.

One correction to the 00-tickets.md row, not to the ticket: the cells do not all come back when the
string agrees. 29 of the 60 rows also declare a `Decimal128(p, s)` with p < 38 at the sink, and the
device exports every decimal at precision 38 (#187), so the unload's message moves to the decimal
column rather than going away; two more (tpch q7, q9) carry an `extract(year …)` declared `Int32`
that the device exports `Int16` (#191). Section 6 has the lists.

## 2. Root cause

The declaration is not DataFusion's *type* for the column. It is DataFusion's *parquet reader
option*, and this engine never runs that reader.

- `peacockdb-core/src/lib.rs:52`: `read_table` registers every table through
  `ParquetFormat::default()`. That format's own options are `TableParquetOptions::default()`,
  whose `global.schema_force_view_types` defaults to `true`
  (`datafusion-common-45.0.0/src/config.rs:428-430`).
- `ParquetFormat::infer_schema` (`datafusion-45.0.0/src/datasource/file_format/parquet.rs:370-374`)
  therefore runs `transform_schema_to_view` over the merged file schema, turning every `Utf8`
  field the parquet file declares into `Utf8View`. The option's own doc (`:240-248`) states the
  reason: DataFusion's parquet reader "is optimized for reading `StringView` … such queries are
  significantly faster". It is an execution optimization of a reader this engine does not use.
- The `ListingTable` built at `lib.rs:57-61` carries that schema; DataFusion plans every expression,
  literal coercion and aggregate against it; the translator copies it into the source node
  (`planner/translator/nodes.rs:408`, `Schema::new(parquet.schema())`) and every node above derives
  from it. So `Utf8View` is in all ten `.plans.txt` goldens (425 occurrences in `tpch.sf1/tp1-single`,
  3565 in `tpcds.sf1/tp1-single`), and on the wire as fb `DataType::Utf8View` in scan schemas,
  aggregate input schemas, casts and literals (`wire/serialize.rs:72-78`, `:130`).
- Neither engine can honour it natively. The CPU backend reads with arrow-rs's own parquet reader,
  which answers `Utf8` (`executor/cpu_backend/source.rs:86-104`), and then **casts every scan batch
  to `Utf8View`** to match the declaration (`source.rs:112-139`, `as_declared`; its doc says exactly
  this: "a string column reaches the rest of the plan as a view type"). The device reads with cuDF,
  whose `STRING` is one type (`cpp/src/expr.cpp:87-89` maps `Utf8`, `LargeUtf8` and `Utf8View` all to
  it) and whose export is `Utf8`. The CPU pays a cast per batch to agree with a declaration; the
  device cannot pay it and is refused.
- Two more places on the CPU exist only because of this: `cpu_backend/spark_partitioning.rs:53-71`
  casts `Utf8View` hash keys back to `Utf8` because comet's murmur3 rejects view layouts, and the
  corpus comparator `tests/common/result_text.rs:85-100` hashes column names and not types, its doc
  naming this ticket as the reason.

The unload's check is correct and stays. `architecture.md` "Every cast is explicit": no executor may
change a type the plan did not ask for, and the plan golden prints the declared schema so a reader
can see it. The defect is that the declaration was inherited from a reader we do not run, and the
one executor that could conform did so by a hidden cast.

In DataFusion 45 this option is the only source of `Utf8View`: no SQL-side `map_varchar_to_utf8view`
exists yet (grep over `datafusion-common-45.0.0/src/config.rs`), and every string function returns
`Utf8` for `Utf8` input. Turning it off removes the type from every plan.

## 3. Localized fix

### 3a. One owner for the option — `peacockdb-core/src/lib.rs`

`build_session_state` (`:25-36`) already clones the config to set `target_partitions`. Add one
setting beside it:

```rust
config.options_mut().execution.target_partitions = target_partitions;
// DataFusion reads parquet strings as `Utf8View` for its own reader's sake, and this engine
// never runs that reader. Both of its own — arrow-rs on the CPU, cuDF on a device — answer
// `Utf8`, so the plan declares what they produce and neither side casts a string to match.
config.options_mut().execution.parquet.schema_force_view_types = false;
```

`read_table` (`:52`) then takes the session's parquet options instead of the format's defaults, so
the rule is stated once and the `register_parquet` fixtures (`tests/common/join_fixture.rs:92`,
which go through `copied_table_options()` → `default_table_options()` →
`combine_with_session_config`, `datafusion-45.0.0/src/execution/context/parquet.rs:50-51`,
`session_state.rs:821-824`, `datafusion-common-45.0.0/src/config.rs:1398-1402`) agree with it:

```rust
let format = Arc::new(
    ParquetFormat::default()
        .with_options(ctx.state().default_table_options().parquet)
        .with_enable_pruning(true),
);
```

(`ParquetFormat::with_options` is `parquet.rs:228-231`; `with_enable_pruning(true)` is the value it
already had and stays only so the diff reads as no other change.) Two lines of production code.
The alternative — `.with_force_view_types(false)` at `:52` alone — leaves the `register_parquet`
fixtures on view types and states the rule in a place the config does not see; not recommended.

### 3b. The scaffolding that goes with it

- `executor/cpu_backend/source.rs:112-139`, `as_declared`. Its cast arm has no reachable input once
  the declaration is `Utf8`: the raw reader and the declaration agree on every corpus type
  (`Utf8`, `Int32`, `Int64`, `Date32`, `Decimal128(p,s)` from the file's own annotation), and the
  only difference left is field metadata, which the fast path at `:118` misses and the relabel
  through `RecordBatch::try_new` at `:137` absorbs. Replace the `cast(...)` at `:127-134` with a
  refusal carrying the same three facts (column, read type, declared type) — the shape
  `declared_as` already has at `cpu_backend/mod.rs:239-275`, "relabel, refuse the rest" — and
  rewrite the doc at `:112-116` to say what the function now is: the reader's batch under the
  declared field list, refused where a type differs, because a scan that reads a type the plan did
  not declare is a planner drift and not something to convert. This is the one deliberate CPU
  behaviour change beyond the declaration. The minimal alternative is to keep the cast and reword
  the doc; it leaves a cast with no consumer, which is the shape `coding-style.md` "Building around
  a bug" warns outlives its reason.
- `executor/cpu_backend/spark_partitioning.rs:53-71`, `hash_keys`. Delete the `Utf8View` and
  `BinaryView` arms and the doc's second sentence; the function becomes "evaluate each key
  expression into an array". Drop the now-unused `cast` and `DataType` imports. comet then sees the
  same `Utf8` bytes it saw after the cast, so lane assignment does not move — the executor contract
  table already scatters `Utf8` keys at 4 and 64 lanes on both engines
  (`tests/common/executor_cases.inc:12`, the `k: Utf8` input; `tests/test_cpu_executors.rs:85`, `:290`; `tests/test_gpu_executors/contract.rs:208`).
- `tests/common/result_text.rs:85-100`, `schema_digest`. Hash `field.data_type()` beside the name
  and rewrite the doc: names and types, because the engine relabels every stage to the declaration
  and the oracle plans the same SQL in the same session, so a type that differs here is a
  declaration DataFusion did not make. Do this as its own commit so a red is attributable (risk
  in section 7). This is hacks-audit finding 12's "at close, the digest should hash types again".
- Leave alone, deliberately: `wire/serialize.rs:72-78` and `:130` (the codec is total over the fb
  enum, which keeps its `Utf8View` member — a frozen enum does not lose a value), `cpp/src/expr.cpp`
  string arms (same reason), `common.rs:35-37` and `:115-119` (a total pricing function).
  Correcting the comment at `serialize.rs:73-75` — it calls `Utf8View` "an optimizer rewrite of
  string literals"; it was coercion to the column's type — costs nothing if the file is open.

### 3c. Tests

- Regression test, red before 3a and green after, in
  `peacockdb-core/src/planner/translator/schema_tests.rs` beside `translated`/`types_of`:
  `a_source_declares_the_types_its_readers_produce`. Translate `SELECT n_name, n_nationkey FROM
  nation` at one lane over `tpch.minimal`, find the `GpuLoadParquet`, open its `file` with
  `ArrowReaderMetadata::load` (the reader `CpuSource` uses, `source.rs:61`), and assert each
  declared field's `data_type()` equals the file schema's at the projected ordinal. Today
  `Utf8View ≠ Utf8` on `n_name`. The comment names the ticket and the two readers.
- Three assertions flip `Utf8View` → `Utf8`: `plan_text/tests.rs:151`,
  `planner/translator/schema_tests.rs:157`, `:286`, `:306`.
- `as_declared`'s refusal (3b) wants one unit case: a `CpuSource` over a hand-written parquet whose
  declared schema names a different type refuses naming the column. `cpu_backend/tests/` has the
  fixture idiom.

### 3d. Goldens, registry, corpus lines, wiki

| artifact | moves | why |
|---|---|---|
| `testdata/goldens/{tpch,tpcds}.sf1/<mode>.plans.txt`, all ten | yes | every `schema=[… :Utf8View …]` becomes `Utf8`; one expression changes, tpcds q24's join filter `CAST(upper(ca_country) AS Utf8View)` loses its cast (the only `AS Utf8View` in any golden). No node appears or disappears, no `partition_groups`, `lanes` or `--- memory ---` line moves — the estimator prices `Utf8` and `Utf8View` identically (`common.rs:35`, `memory_estimation.rs:175-183`). Anything else in the diff is a finding, not a regeneration |
| `testdata/goldens/recipe-payloads.txt` | yes, bytes and text | fb `Utf8View` → `Utf8` in `CudfScan.file_schema`, `CudfAggregate.aggr_input_schema`, `CudfUnion.output_schema`, cast targets, function return types, literal tags (the `null::Utf8View` at `:3354` included). Needs `UPDATE_CANONICAL=1` **and** `PEACOCK_REWRITE_RECIPE_BYTES=1`; review the section text for tag changes only, then accept the digests |
| `<mode>-mini.cpu.txt`, `.cost.txt` | expected no | structural bytes are `(rows+1)·4` for both types; content is `offsets[rows]−offsets[0]` for `Utf8` against Σ valid lengths for `Utf8View`, equal unless a `Utf8` array carries bytes under null slots. No corpus query uses `nullif`; every other kernel on this path (reader, `take`, `filter`, `concat`, `zip`, builders) writes zero-length nulls. An unchanged file is the check |
| `mini.result.txt` | no | rendered rows, no types |
| `testdata/cost-registry.csv`, `corpus_cases.inc` | per the device run | section 6 |
| `llm-wiki/tasks/active-tickets.md` #183 | closes to `archive/archived-tickets.md` when its cells are gone, with the corrected cause: a reader option, not a DataFusion type |
| `llm-wiki/build-test.md:44` | reword the device-corpus row's blocker list after the run |
| `llm-wiki/architecture.md` | no sentence falsified; nothing to add unless asked |
| `llm-wiki/tasks/declared-schemas.md` row 1 | the human's: `SELECT n_name FROM nation` "Utf8View declared" stops being a divergence; the spec's own rule ("where the survey shows a class does not occur, drop its query") covers it |

### 3e. How CPU and GPU stay one engine

Both declare `Utf8`. The CPU reads `Utf8` and no longer converts; the device reads `STRING` and
exports `Utf8`; the unload's exact-type check passes for strings and keeps refusing everything
else. The C++ needs no change: `fb_to_type_id`, the literal builders and `is_ast_able`
(`expr.cpp:87-89`, `:226-236`, `:331-333`, `:476-478`) already treat the three string tags as one.
Placement is unchanged on both sides (same bytes hashed). The wire format does not change — the
same fields carry a different enum value.

## 4. Alternatives rejected

- Cast at the unload (the archived `casts.md`): rejected by the human already — builds the
  divergence into the plan and disarms the one type check on the device path.
- Export `Utf8View` from C++: cuDF 25.02/26.02 has no string-view column and `to_arrow_schema`
  cannot emit `vu`; it would be an Arrow-side cast in the exporter, the same cast one layer down.
- Rewrite `Utf8View` → `Utf8` in the translator: DataFusion's physical expressions, which the CPU
  backend runs, were planned against `Utf8View` — a `Utf8` column against a `Utf8View` literal is an
  arrow comparison error, so every expression type would need rewriting too.
- Wait for the operator harness / declared-schemas catalog: they would record this as a `bug_`
  test; the cause is already known and is a config line.
- `.with_force_view_types(false)` on the format alone: two statements of one rule, and the
  `register_parquet` fixtures stay on view types.

## 5. Minimum corpus query

    SELECT r_name FROM region

tpch sf1 (or `tpch.minimal`), tp1-single, GPU backend. Plans today: `GpuUnload` over a
`GpuLoadParquet` declaring `r_name:Utf8View`. Runs to the unload and fails there with "expected
Utf8View but found Utf8 at column index 0". The CPU backend answers it (after casting the scan's
`Utf8` to `Utf8View`). After the fix the same plan declares `r_name:Utf8` and the device's export
matches. The smallest committed query on the same path is `tpch/cross-join`
(`testdata/tpch-queries/cross-join.sql`, `SELECT * FROM region, nation`), off at all five device
modes on `183` alone.

## 6. Cells re-enabled

Derived from each row's tp1-single sink schema and its other tickets; a shad-gpu run decides.

**Move to #187, not green (29 rows)** — a `Decimal128(p<38, s)` sits at the sink, so the unload's
message moves to that column: tpcds q3 q8 q15 q19 q25 q37 q40 q42 q43 q45 q46 q52 q55 q56 q58 q59 q60
q68 q76 q79 q80 q82 q91; tpch aggregate_groupby anti_join semi_join shuffle_additive q10 q18. Their
registry `tickets` cells swap `183` for `187`; no `corpus_query!` line changes.

**Move to #191 (2 rows)** — `extract(year …)` declared `Int32` at the sink: tpch q7, q9.

**Candidates (29 rows)**, each a run to take:

- No `152`, so all five modes are in play: tpch cross_join, nested_loop_join, nested_loop_left_join
  (their inputs are region and nation, one row group each, so every mode is a single-batch probe),
  q4 (a build-side semi join, whose probe calls never touch the build side), shuffle_stddev. q15
  stays off on #184. Realistic next walls: #185 wherever a `GpuAggregateBatches` merges more than one
  batch (q4 and shuffle_stddev at the tp4 modes, at least), and the Left nested-loop finish for
  nested_loop_left_join, which the capability matrix says a device runs.
- With `152`, the tp1-single cell only: tpcds q4 q10 q11 q21 q23 q29 q31 q34 q50 q62 q66 q69 q73 q74
  q83 q84 q99; tpch q5 q12 q16 q20 q21 rollup_over_join. Next walls by ticket: #185 (any multi-batch
  merge), #55 for q66, the latent #59/#80 for q69 and q16/q21. #97 and #116 on some rows are closed
  tickets and come off the cell whatever the run says.

A cell that goes green is enabled in both `corpus_cases.inc` and the registry; a cell that moves is
re-ticketed in the registry and its batch comment. #183 closes when no cell carries it.

## 7. Risks and unknowns

- **Plan shape under `Utf8`.** Predicted diff is `Utf8View` → `Utf8` in schema lists plus q24's
  cast. DataFusion's coercion could differ elsewhere in a way the goldens do not show me; the
  regenerated `.plans.txt` diff is where it shows, and a moved node line is to be understood before
  the golden is accepted.
- **Row-group pruning survivors.** `scan_mapping/rowgroup_prune.rs:113-121` builds the pruning
  predicate over `config.file_schema`, now `Utf8`. parquet 54 converts statistics for both types
  (`parquet-54.2.1/src/arrow/arrow_reader/statistics.rs:494`, the `Utf8View` arm, beside the
  `Utf8` one), so survivors should not move; if any `partition_groups` line moves, pruning behaved
  differently under one of the two types and that is a separate finding.
- **`.cpu.txt` byte figures.** Argued equal in 3d; a `Utf8` array with bytes under null slots would
  change `output_bytes` for that node. Not seen by reading; an unchanged file proves it.
- **`schema_digest` hashing types** may redden a CPU cell whose declared sink type differs from
  DataFusion's oracle output type. `test_cpu_end_to_end.rs:243-244` already asserts `(name, type)`
  equality for 17 queries at five modes, so the class is unlikely to be wide; a red is a real
  declaration divergence and gets a ticket rather than a revert.
- **Which candidates go green** — a device question; the two deterministic moves (#187, #191) are
  by reading the sink schemas and the two tickets' mechanisms.
- **CPU tier timing.** DataFusion's `Utf8` kernels against `Utf8View` ones; unmeasured either way,
  and the per-batch scan cast disappears.
- **Nothing in the C++ changes**, so the 26.02 compile-only leg carries no risk; the device tiers
  should still run once (`test_gpu_recipe_walk`, `test_gpu_executors`, `test_gpu_corpus`) because
  every plan they drive now carries a different type tag on the wire.

## 8. Complexity

**M.** The production change is two lines in `lib.rs` plus three small scaffolding edits
(`source.rs`, `spark_partitioning.rs`, `result_text.rs`, ~40 lines net) and one regression test —
S on its own. What makes it M: all ten plan goldens and the payload digest golden regenerate
(mechanical, but the payload one is the "must not quietly rewrite" file and is reviewed by hand),
and closing the ticket is a shad-gpu rollout over 60 registry rows in batches of about five, each
re-ticketed or enabled from what the run says. No frozen surface changes: no ABI symbol, no
`gpu_plan.fbs` change, no wire-format change, no declared-schema contract change — the wire carries
a different value of an existing enum.
