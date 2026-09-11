# peacockdb build & test

Code and tests are authoritative; this page maps them.

## Test categories

**Grand total: 1569 test cases — Rust 1135, C++ 65, Python 369.** The Python figure includes the 93 corpus queries, which only a manual dispatch runs. The header is the sum of the N column, and the rows below it count cases: a target's own `--list` total is larger, because its registry test is counted once in Registry ↔ CSV rather than again in each tier it belongs to. Comparing a row against a target total is how this page gets mistakenly reported as drifting.

**Runs** — `dataset-matrix` = pipeline.yml's job with the generated dataset and the cuDF
matrix, both legs unless a step says one · `cost-report` = the
cost-report job · `shad-gpu` = CI GPU job on the remote host, `--test-threads=1` ·
`manual` = no CI step runs it · `2gpu` = manual, host with ≥2 GPUs (verda-gpu) ·
`validate-large` = validate-large.yml manual dispatch. Everything except the `shad-gpu`
and `2gpu` rows also runs locally; large CPU batches go to verda.

| Category (lang) | Why | Examples | Runs | N |
|---|---|---|---|---|
| GPU↔comet murmur3 (Rust) | the linchpin gate: both sides place every row in the same partition, bit-exact | [gpu_spark_partition_ids_match_comet_live](../peacockdb-core/tests/test_inc2_conformance.rs#L131) | shad-gpu | 10 |
| Recipe walk on a device (Rust) | the recipe plan driven by hand — begin_plan, the calls each recipe names, handles threaded between them, DataFusion on the same SQL as the oracle. One partition and one batch except the aggregates, which take two so a merge happens; `avg` asserts digits, since cuDF takes a divide's scale from its operands where arrow takes it from the declared type. A ROLLUP is here because its masks and NULL placeholders are the one payload the plan line does not imply, and one read re-walks every query to check the kinds a device has run against the kinds the file claims, in both directions | [an_average_finalizes_to_the_digits_the_oracle_computes](../peacockdb-core/tests/test_gpu_recipe_walk.rs) | shad-gpu | 10 |
| Executor contract, both engines (Rust) | one table of input, calls and expected answer ([`executor_cases.inc`](../peacockdb-core/tests/common/executor_cases.inc)) run by the CPU backend here and by the device in `test_gpu_executors`, because a table one side does not read proves that side twice. Eleven rows: filter, project, the lane sorted with and without a fetch, coalesce, a merge with and without its finalize, a merge over state whose keys carry a grouping id, and the scatter at 4 lanes and at 64 — the lane each key lands in is a golden, since co-partitioning is what every partitioned join rests on | [test_cpu_executors](../peacockdb-core/tests/test_cpu_executors.rs) | dataset-matrix | 1 |
| Executors on a device (Rust) | each one handed its node's recipe — the exec nodes one batch at a time (filter, project, per-batch sort, aggregate with and without its finalize, the export with a row range, and an accumulator's recipe refused), the accumulators a stream of them (coalesce, the accumulating sort, the state merge, the Welford merge whose count exports Int64 against a UInt64 declaration ([#163](tickets.md#t163)), the mid-plan limit), and the joins what the matrix says a device runs: Inner at one probe batch, LeftAnti streamed through its finish pass, the scatter's N handles, and the refusals — Left and Full outright, a second probe batch, a zero-input collapse — each naming its ticket; plans hand-built over six rows the test writes itself, since the ABI loads a table only by reading one. `backend.rs` is the caller `executors_for` otherwise has none of: six of the seven categories built from a live session's recipes, and a node asked for at the wrong post-order refused, so the number is an address. The partition accumulator is the seventh and has no caller; its arm reads the child's lane count | [an_aggregate_that_finalizes_runs_both_of_its_calls](../peacockdb-core/tests/test_gpu_executors/exec.rs) | shad-gpu | 31 |
| Per-call ABI (Rust) | the three per-call symbols on a live GPU — a scan's row groups, an export range, a slice — and the release skipped exactly where a call consumed the handle | [test_gpu_abi](../peacockdb-core/tests/test_gpu_abi.rs) | shad-gpu | 4 |
| GpuBatch surface (Rust) | what the batch reports, and that `consume` hands the handle over without releasing it. Needs no device: the release is null-guarded on the executor, so a CPU tier is its home | [executor::ffi_tests](../peacockdb-core/src/executor/ffi_tests/mod.rs) | dataset-matrix | 3 |
| Cost-model goldens (Rust) | `.cost.txt` derivation from `.cpu.txt` × `cost_model.conf` | [cost_goldens_match_and_total_is_byte_identical](../peacockdb-core/tests/test_cost_model.rs#L36) | dataset-matrix | 3 |
| Planner join capability (Rust) | every hash join type crossed with a residual filter, the co-partitioning and lane rules, and the null analysis both ways; writes its own parquet, so no dataset | [test_planner_join_capability](../peacockdb-core/tests/test_planner_join_capability.rs) | dataset-matrix | 13 |
| Null analysis rules (Rust) | every rule in the can-this-column-be-NULL pass, on hand-built nodes — a source declares a not-nullable column here, which no corpus fixture can | [a_scalar_function_can_be_null_even_over_operands_that_cannot](../peacockdb-core/tests/test_null_analysis.rs) | dataset-matrix | 8 |
| Planner join refusals (Rust) | every shape the planner refuses, from the SQL that provokes it; each asserts its ticket is in the message a user sees | [test_planner_join_refusals](../peacockdb-core/tests/test_planner_join_refusals.rs) | dataset-matrix | 10 |
| Plan goldens, tp1-single (Rust) | 1 lane, one batch per chunk — the mode every other is read against; plan tree + `--- recipes ---` + `--- memory ---` per query, one file per bench | [tpch_tp1_single](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 2 |
| Plan goldens, tp1-rowgroup (Rust) | 1 lane, one batch per row group: the finest the mapping expresses, and no budget; plan tree + `--- recipes ---` + `--- memory ---` per query, one file per bench | [tpch_tp1_rowgroup](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 2 |
| Plan goldens, tp4-single (Rust) | 4 lanes, one batch per chunk — the shuffle shapes with batching inert; plan tree + `--- recipes ---` + `--- memory ---` per query, one file per bench | [tpch_tp4_single](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 2 |
| Plan goldens, tp4-rowgroup (Rust) | 4 lanes at row-group granularity: lanes and many batches at once; plan tree + `--- recipes ---` + `--- memory ---` per query, one file per bench | [tpch_tp4_rowgroup](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 2 |
| Plan goldens, tp4-sized (Rust) | 4 lanes, the estimator's target — **the only mode a budget tier moves**, recorded in-band; plan tree + `--- recipes ---` + `--- memory ---` per query, one file per bench | [tpch_tp4_sized](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 2 |
| Plan goldens, meta (Rust) | the registry and the goldens agree both ways; every mode has a golden and every golden a mode; every refusal in a golden names a ticket that exists and carries no host path | [the_registry_matches_the_goldens_in_both_directions](../peacockdb-core/tests/test_plan_goldens.rs#L427) | dataset-matrix | 4 |
| Recipe plan structure (Rust) | the claims the payload golden cannot make: every published seq resolves to the kind its recipe names, over every corpus query rather than only the payload subset; the payload set covers every fb kind and call shape the ten goldens hold; the queries that cannot cross the wire are declared; no plan approaches the verifier's depth cap ([#169](tickets.md#t169)); and the index's post-order agrees with the numbering `attach_recipes` gave, over the corpus, since the two are separate walks in separate files and [#134](tickets.md#t134) is the same pair one boundary over | [every_published_seq_addresses_the_kind_its_recipe_claims](../peacockdb-core/tests/test_plan_goldens.rs) | dataset-matrix | 4 |
| Recipe payloads golden (Rust) | the payload subset at tp4-rowgroup — chosen as a cover over every fb kind and call shape the mode goldens hold, and asserted to be one, so the membership grows when the mapping does — with every payload rendered and a sha256 over the bytes beside it | [the_payload_golden_carries_what_each_call_hands_the_executor](../peacockdb-core/tests/test_plan_goldens.rs#L229) | dataset-matrix | 1 |
| Golden text format (Rust) | the one reader of the format, over strings: the differ must name what moved, what is missing and what is out of order, and a node line must yield its name, depth and fields — every golden assertion in the tree is a comparison through it. The comma traps are cases, since `on=[(a@0, b@1)]` and `Decimal128(38, 15)` each carry one inside brackets | [a_section_that_moved_is_named_with_the_column_that_moved](../peacockdb-core/tests/test_golden_format.rs) | dataset-matrix | 26 |
| Registry ↔ CSV (Rust) | each `cost-registry.csv` execution column matches the cases the corpus expands to, both directions — one binary per engine, since `inventory` collects per linked binary | [the_registry_matches_the_cpu_corpus_in_both_directions](../peacockdb-core/tests/test_cpu_corpus.rs), [the_registry_matches_the_gpu_corpus_in_both_directions](../peacockdb-core/tests/test_gpu_corpus.rs) | dataset-matrix, shad-gpu | 2 |
| CI wiring guard (Rust) | every Rust target must be named by a CI step — CI does not glob, the three lists that decide where a GPU target runs must agree, and both GPU runners must pass `--test-threads=1`, which is the whole of the single-tenant invariant inside a process and is what a device test's `unsafe { set_var }` rests on. Two classes the `--test` sweep cannot see are asserted line by line instead: the crate's `--lib` unit tests, and the CLI, which has no test target at all. The reader is scoped to the rust loop: a file-wide search passes on the comment above the command, and the C++ loop above it shares the loop variable and rightly carries no flag | [every_rust_test_target_is_named_by_ci](../peacockdb-core/tests/test_ci_coverage.rs#L323), [the_three_gpu_target_lists_agree](../peacockdb-core/tests/test_ci_coverage.rs#L419) | cost-report | 6 |
| Module layout rules (Rust) | the component walls, which the compiler mostly cannot check: where a `pub` may appear, that a subcomponent is declared `mod`, that no `super::` chain climbs out of its component, and that no public signature names a type from a private module — `private_interfaces` reads a type's nominal visibility, so an unreachable type spelled `pub` passes it silently. Sibling reach is the case rustc refuses to have an opinion on at all: no visibility level means "my parent but not my siblings". One case compiles a probe crate against the built library to prove `wire::generated` is unreachable from outside, with a positive control so a probe that fails for the wrong reason cannot pass as proof. Five more rules say where test code may live: a test module is named for the build rung it needs and gated for it, in both directions; its body is a file of its own; a `#[cfg(test)]` sits on nothing but a test-module declaration; and a path compiled only under a test gate says `test` in its name. Three registers carry the exceptions the rules allow — the `pub mod` a test crate forces, any cross-component reach into a subcomponent (none today), and the items that keep a `#[cfg(test)]` because no test module can hold them — and all three are checked in the reverse direction too, so an entry outliving its reason goes red. Needs no dataset and no device | [no_public_signature_names_a_type_from_a_private_module](../peacockdb-core/tests/test_module_layout.rs) | cost-report | 16 |
| Lib unit (Rust) | the engine's types, schema annotations, validation rules, both drivers with the scheduler and the resident accountant over a mock backend, the join recipes, the expression writer — every variant and operator, since a wrong scalar is invisible in plan text and wrong on a device — and the CPU backend's executors relaying to DataFusion — the capability matrix among them, run per (type x layout) against oracles written down in the test | [an_outer_join_that_preserves_its_build_side_keeps_the_keys_and_finishes_with_an_anti_join](../peacockdb-core/src/wire/tests.rs#L161) | dataset-matrix | 435 |
| End to end (Rust) | SQL in, rows out: 17 queries planned and run at all five modes against DataFusion on the same SQL, eleven of them also at injected layouts no planner would emit, plus `in_flight_bytes` back to zero and holds equal releases at the end of every run — and seven cases no query list can carry: that DataFusion's partial aggregate does not skip grouping here, the call and pull counts a limit makes, the smallest budget a query fits in completing where the byte below it trips, and that boundary under a drained lane — both `#[ignore]`d against [#182](tasks/active-tickets.md#t182) since pricing from the schema put them out of the search's reach, the model compared against what the calls measured, an answer under the wrong column names not being the same answer, the injected set keeping the shapes only one query has, and a degenerate hash under a right outer refused by name. Two of the 26 are `#[ignore]`d against [#182](tasks/active-tickets.md#t182) — the budget boundary and the rebatcher's peak, both properties that pricing a batch from the plan's schema took away — so 24 run. The first tier where the planner, the recipes, the executors and both drivers run together rather than each against a fixture of the last one's shape — so what it tests is the joins between them, which four tasks of separate proofs cannot reach | [tests::end_to_end](../peacockdb-core/src/tests/end_to_end.rs) | dataset-matrix | 26 |
| Corpus, cpu (Rust) | one `corpus_query!` line per query declaring its cpu and gpu modes and its two oracles, expanded to a case per (query, mode): planned, run on `CpuBackend`, validated, and the answer checked against plain DataFusion at `target_partitions = 1`. 37 queries at the modes each is correct at — `tpcds/q96` carries three disabled by [#180](tasks/active-tickets.md#t180), `tpcds/q77` three by [#175](tickets.md#t175) and `tpcds/q80` three by [#189](tasks/active-tickets.md#t189), `tpcds/q88` three by [#180](tasks/active-tickets.md#t180), and thirteen queries are out entirely on [#163](tickets.md#t163) | [test_cpu_corpus](../peacockdb-core/tests/test_cpu_corpus.rs) | dataset-matrix | 447 |
| Corpus goldens, self-consistency (Rust) | the committed sections against their own arithmetic, with no dataset and no run: `consumed + abandoned == emitted` at every node, lane counts against `lanes=N`, the loader's `batch_rows` a prefix of its own lane's `partition_groups`, the root's `out_rows` against `.result.txt`, and the tree the indentation draws. The golden is written by the run it will later check, so a file that contradicts itself is the only witness to a renderer that is wrong | [test_corpus_goldens](../peacockdb-core/tests/test_corpus_goldens.rs) | dataset-matrix | 20 |
| Corpus, device (Rust) | the same `corpus_query!` lines read from the other side: each enabled (query, mode) runs on a device and asserts, read-only, against the section the cpu authored — plan shape, `in_rows`, the per-batch lists and the bytes — plus the result where `gpu_oracle` names a golden. Six cells today, `q6` at every mode and `q19` at `tp1-single`; the rest are off against [#152](tickets.md#t152), [#183](tasks/active-tickets.md#t183), [#184](tasks/active-tickets.md#t184), [#185](tasks/active-tickets.md#t185) and [#187](tasks/active-tickets.md#t187) | [test_gpu_corpus](../peacockdb-core/tests/test_gpu_corpus.rs) | shad-gpu | 7 |
| Layout injection mechanism (Rust) | the rewrite against itself, with no dataset: a plan rebuilt from its own children is identical in debug — the renderer prints neither a loader's survivors nor `can_be_null`, so the fields a corpus plan never varies are the ones it cannot check — every field name a node's debug prints takes two distinct values across the fixtures, and the selector's output is a cover rather than a prefix | [plan::tests::layout_injection](../peacockdb-core/src/plan/tests/layout_injection.rs) | dataset-matrix | 4 |
| FFI smoke (Rust) | the crate links; executor lifecycle | [test_executor_lifecycle](../peacockdb-ffi/tests/test_ffi.rs#L17) | dataset-matrix | 2 |
| Cost-report renderer (Rust) | glyphs, links and the anchors they resolve to, ratio bucket, regression gate, history | [bucket_threshold_is_1_4](../cost-report/src/main.rs#L1552), [regression_count_drives_exit_decision](../cost-report/src/main.rs#L1623) | cost-report | 37 |
| DuckDB cost extraction (Python) | classifier / pruning / dynamic-filter logic — fails CI before generation | [scan_count_mismatch_fails_loud](../testdata/test_duckdb_cost.py#L280), [compute_pruning_from_rowgroups](../testdata/test_duckdb_cost.py#L226) | cost-report | 41 |
| Exec-model prototype (Python) | the scheduler over mock traits, plus pandas-backed operators checked against a single-shot oracle at five partitioning configs, both limit lowerings, the scalar expressions pinned to what `expr.cpp` does rather than what pandas defaults to, and every join mode run on two backends — one pandas, one emitting FlatBuffers nodes and interpreting them as the C++ does — no project code | [test_a_join_in_its_build_phase_holds_back_its_probe_subtree](../scripts/exec_model/tests/test_scheduling.py), [test_every_join_type_matches_the_oracle_on_both_backends](../scripts/exec_model/tests/test_join_capability.py) | cost-report | 216 |
| Exec-model prototype, TPC-H plan shapes (Python) | the same drivers over real sf1 tables under a live resident budget, each plan re-run at every layout `LayoutInjector` can produce; needs the generated dataset, so it rides dataset-matrix rather than cost-report | [test_the_accumulator_is_what_makes_the_budget_bind](../scripts/exec_model/tests/test_tpch.py), [test_every_layout_gives_the_same_shuffled_join](../scripts/exec_model/tests/test_tpch.py) | dataset-matrix (25.02 leg) | 19 |
| Exec-model corpus (Python) | every TPC-H query and every TPC-DS query the engine runs that needs no window function, lowered by hand and run over whole sf1 tables at three layouts each — TPC-H against a pandas oracle per query, TPC-DS against DuckDB running the query's own text. Minutes, not seconds, so manual dispatch; `PCK_BACKEND=recipe` re-runs the whole set with every join going through the FlatBuffers emulation | [test_corpus_q21_suppliers_who_kept_orders_waiting](../scripts/exec_model/tests/test_tpch_corpus.py), [plans_tpcds.py](../scripts/exec_model/tests/plans_tpcds.py) | manual — exec-model-corpus.yml, 3 shards | 93 |
| C++ CPU/FFI unit | decimal binop typing, AST routability, lifecycle, and the row-range clamp rule; no GPU needed | [DecimalScale.BinopOutputType](../cpp/tests/cpu/test_executor.cpp#L26), [AstRouting.IsAstAble](../cpp/tests/cpu/test_executor.cpp#L81) | dataset-matrix (`ctest -L cpu`) + shad-gpu | 11 |
| cuDF GPU smoke (C++) | the GPU is alive; the Spark-murmur3 kernel matches comet in C++; the timing floor leaves the global switch as it found it | [CudfGpu.SparkPartitionIdsMatchComet2ColWithNulls](../cpp/tests/gpu/test_cudf.cpp#L84), [NodeTiming.FloorRestoresTheSwitch](../cpp/tests/gpu/test_cudf.cpp#L41) | shad-gpu | 5 |
| Plan-executor (C++) | hand-built plan IR through the C++ executor, node by node, plus the per-call entry points at their contract edges (row-group override, export range, slice), the sqrt arm on both evaluators, and a merge that emits state rather than a value | [PlanExecutor.HashJoinNationRegion](../cpp/tests/gpu/test_plan_executor.cpp#L255), [AggregateMerge.WelfordStateComesBackAsStateAndNotAsAValue](../cpp/tests/gpu/test_plan_executor.cpp) | shad-gpu | 27 |
| TPC-H sf40 bare-cuDF (C++) | hand-written cuDF pipelines vs DuckDB sf40; the benchmark vehicle | [TpchSf40.Q1GroupByAggregates](../cpp/tests/gpu/test_tpch.cpp#L216), [Q3JoinsGroupByTopN](../cpp/tests/gpu/test_tpch.cpp#L376) | shad-gpu (sf40 is a hard precondition) | 4 |
| TPC-H+V sf40 bare-cuDF (C++) | the same for the vector-embedding queries | [TpchSf40.Q11VectorBruteForce](../cpp/tests/gpu/test_tpchv.cpp#L326) | shad-gpu | 4 |
| TPC-H sf40 streamed (C++) | the same four queries and the same DuckDB goldens with nothing held resident — a chunked reader under a byte budget, so it answers whether the query fits rather than how fast the operators are | [TpchSf40Streamed.Q1Streamed](../cpp/tests/gpu/test_tpch_streamed.cpp#L337) | manual | 4 |
| Per-operator cuDF timings (C++) | one operator at a time over real sf40 columns, so a query cost reads as a sum of parts; asserts row counts only, so a benchmark cannot time an empty column | [CudfNodes.OperatorTimings](../cpp/tests/gpu/test_cudf_nodes.cpp#L140) | manual | 1 |
| Multi-GPU TPC-H (C++) | WorkerPool, hash_shuffle, per-device RMM pools across GPUs | [TpchSf40.Q3MultiGpu](../cpp/tests/gpu/test_multi_gpu_tpch.cpp#L460) | manual, 2gpu | 4 |
| Multi-GPU TPC-H+V (C++) | the same for the vector queries | [TpchSf40.Q10VectorCustomerTopNMultiGpu](../cpp/tests/gpu/test_multi_gpu_tpchv.cpp#L920) | manual, 2gpu | 4 |
| Multi-GPU basics (C++) | device-local streams + destruction on the owning worker | [BasicMultiGpu.CudfAndCuvsAcrossTwoGpus](../cpp/tests/gpu/test_basic_multi_gpu.cpp#L228) | manual, 2gpu | 1 |
| Dataset validators (Python) | row counts / clustering / embedding stats over whatever dataset they are pointed at; each check tags itself `EXHAUSTIVE` or `SAMPLED` | [validate_tpch.py](../scripts/validate_tpch.py), [check_s3_datasets.py](../scripts/check_s3_datasets.py) | dataset-matrix (sf1) · validate-large (sf40/sf200) | n/a¹ |

¹ Data-driven — the check count depends on the dataset and SF, so these are excluded from
the total above. Everything else in the repo that can be enumerated as a test case is counted.

Notes

- A mode is one of the five `tp<N>-<sizing>` shapes. A `corpus_query!` line declares
  which of them a query is correct at, per engine, and the mode name is the golden's name;
  the tier (`mini`) is the budget those plans are priced at, and it rides in the execution
  goldens' filenames so a budget and a filename cannot name different tiers.
- Execution goldens are per mode, not per query: `<mode>-<tier>.cpu.txt` with a
  `== <query>` section each, the derived `.cost.txt` beside it, and `<tier>.result.txt`
  for the answers. A device run reads the cpu's sections read-only. Result validation is
  `golden_exact` | `golden_approx` | `golden_approx_std` | `live_cpu` | `skip`.
- The crate has no doctest today, so `--doc` runs nothing. The gap #128 names is still open:
  no CI step passes it and the meta guard enumerates only `--test` targets plus `--lib`, so
  the first doctest written would be unrun with nothing saying so.

## Datasets

Host columns are audited, not assumed: they say whether the bytes were actually found
there (`ls` + `sha256sum`, 2026-08-05). Only tpch.minimal is in git; everything else is
generated or fetched per host. S3 is Nebius object storage, endpoint
`https://storage.eu-north1.nebius.cloud:443`, region `eu-north-1`.

| Dataset | Shape | <sub>local</sub> | <sub>verda</sub> | <sub>shad-gpu</sub> | S3 bucket | Used by |
|---|---|:-:|:-:|:-:|---|---|
| tpch.minimal | 5 tables, 19 MB, git-committed | ✓ | ✓ | ✓ | — | C++ plan tests, plan serializer, node executor |
| TPC-H+V sf1, external vectors | 8 tables, 987 MB; GloVe 100-d + DEEP1B 96-d | ✓ | ✓ | ✓ | — | most CPU/GPU rust tests |
| TPC-H+V sf1, synthetic vectors | same tables, hash-generated FLOAT[8] | — | — | — | — | CI only — regenerated every run |
| TPC-DS sf1 | 24 tables, 764 MB | ✓ | ✓ | ✓ | — | plan + CPU + GPU subsets |
| TPC-H+V sf40 | 8 tables, 40 GB | — | — | ✓ | `tpch-sf40` | `peacock_tpch(v)_tests`, multi-GPU (manual) |
| TPC-DS sf200 | 24 tables, 80 GB | — | — | ✓ | `tpcds-sf200` | no tests — S3 check, validate-large |
| TPC-H sf200 | not generated yet | — | — | — | `tpch-sf200` | nothing yet |
| embeddings cache | 1.8 GB local / 129 GB shad-gpu | ✓ | — | ✓ | — | generator input, not a test input |

Paths: `testdata/{tpch.minimal,tpch.sf1,tpcds.sf1,embeddings-cache}`; sf40 and sf200 live
outside the repo on shad-gpu, under `/home/info/peacock-datasets/testdata/`. **+V** means
the TPC-H tables carry vector columns — `part.p_text_embedding` and
`partsupp.ps_image_embedding` + `ps_text_embedding`; TPC-DS does not have embeddings.

The three sf1-class datasets are byte-identical on all three hosts (aggregate parquet
sha256 matches local). Notes on the rows above:

- **The sf1 parquet is generated** — `testdata/generate_testdata.sh` drives DuckDB, and
  `/tpch.sf*/`, `/tpcds.sf*/` are gitignored, so CI regenerates it every run. That is why
  the two sf1 rows differ: `--embeddings synthetic` is the default, so a vector query in
  CI runs against hash-generated FLOAT[8], not the vectors a dev host has.
- **The embeddings cache is a per-host intermediate and is NOT syncable** — no
  `--push`/`--pull` kind covers it; `fetch_embeddings.sh` is local-only and hard-guards
  against running on CI/verda/shad-gpu. Regenerate it where you need it, or ship the
  augmented parquet instead.
- **sf40 lives only on shad-gpu**, and its presence there is a hard precondition of the
  GPU job; the sf40 goldens are committed (see below), so CI compares without touching
  40 GB.
- verda reaches the tree through a `/media/data/peacockdb` symlink to
  `~/peacockdb`. Every test binary honours `PEACOCK_TESTDATA_DIR`, but the plain cpu mode
  of `build-test.sh` does not set it, so those binaries fall back to the build box's path.

## Golden files

All goldens are committed, under `testdata/goldens/`: `tpch.sf1` (38 files), `tpcds.sf1`
(115), `tpch.sf40` (16), plus `recipe-payloads.txt` at the top, the recipe payloads with
a digest each. Most of each sf1 count is the per-query DuckDB cost oracle (22 + 99); the
engine's own are 16 apiece, one plan and one execution set per mode. The committed DuckDB
profile inputs live beside them in `testdata/duckdb-profiles/{tpch,tpcds}` (22 + 99) and
`testdata/duckdb-dynfilters/{tpch,tpcds}` (22 + 99).

The generator scripts live in `testdata/`.

| Golden | Produced by | Depends on | Asserted by |
|---|---|---|---|
| `<mode>.plans.txt` | `test_plan_goldens` with `UPDATE_CANONICAL=1` | sf1 parquet — row-group counts and per-column bytes decide `partition_groups` and the lane rules | the plan golden tiers, section by section |
| `recipe-payloads.txt` | `test_plan_goldens` with `UPDATE_CANONICAL=1`<br>**and** `PEACOCK_REWRITE_RECIPE_BYTES=1` | sf1 parquet, and a fixed `/tmp` symlink for the testdata root — without it the payloads carry this machine's paths and so does the digest | <sub>the_payload_golden_carries_<br>what_each_call_hands_the_executor</sub> |
| `<mode>-<tier>.cpu.txt` | the corpus cpu tier under `UPDATE_CANONICAL=1` (merge and prune) or `PCK_UPDATE_SECTIONS=1` (merge only) — never the GPU | sf1 parquet; one file per mode, a `== <query>` section each | the corpus cpu tier writes and verifies; the device tier verifies read-only |
| `<mode>-<tier>.cost.txt` | derived from the sibling `.cpu.txt` **section**, × `cost_model.conf` | that `.cpu.txt`, `cost_model.conf` | the corpus cpu tier + `test_cost_model`, which re-derives every section independently |
| `<tier>.result.txt` | the last mode a query declares, under either variable; a run without that mode leaves the section alone | sf1 parquet; one section per query, its `mode=` line naming the author | the corpus cpu tier; the device tier where `gpu_oracle` names a golden |
| `<q>.duckdb_cost.txt` | `gen_duckdb_cost.sh --gen`<br>(DuckDB 1.5.4, `threads=1`, pyarrow 19.0.1) | committed pass-1 profiles ∩ pass-2 dynamic-filter bounds ∩ parquet row-group stats | the cost-report widget (directional signal, not a test) |
| `tpch.sf40/duckdb_<q>.csv`, `.count.csv` | `gen_duckdb_goldens.sh --sf 40`<br>on shad-gpu | sf40 parquet, query text from `tpch_query_sql.sh` | `peacock_tpch_tests` / `peacock_tpchv_tests` |

How they hang together — parquet at the top, goldens derived left to right:

```
testdata/{tpch,tpcds}-queries/*.sql
        │
        └── generate_testdata.sh (DuckDB, --embeddings synthetic|external)
                 ▼
        tpch.sf1 / tpcds.sf1   (parquet, gitignored)
          │
          ├── test_plan_goldens, UPDATE_CANONICAL=1
          │     ├──► <mode>.plans.txt
          │     └──► recipe-payloads.txt  [also needs PEACOCK_REWRITE_RECIPE_BYTES=1,
          │                                   and the fixed /tmp symlink]
          │
          ├── the corpus cpu tier, UPDATE_CANONICAL=1   (the author; a device never writes)
          │     ├──► <mode>-<tier>.cpu.txt        (a == <query> section each)
          │     │       └──× cost_model.conf ──► <mode>-<tier>.cost.txt
          │     └──► <tier>.result.txt         (one section per query)
          │
          └── gen_duckdb_cost.sh --gen  (DuckDB 1.5.4, threads=1, pyarrow 19.0.1)
                ├── pass 1, JFP off ──► duckdb-profiles/<bench>/<q>.json     (committed)
                ├── pass 2, JFP on  ──► duckdb-dynfilters/<bench>/<q>.json   (committed)
                └── duckdb_cost.py extract
                      (pass-1 profile ∩ pass-2 bounds ∩ parquet row-group stats)
                        └──► <q>.duckdb_cost.txt

embeddings-cache ──► generate_testdata.sh --embeddings external   (local only, per host)

tpch.sf40 (shad-gpu only) + testdata/tpch_query_sql.sh
    └── gen_duckdb_goldens.sh --sf 40 ──► goldens/tpch.sf40/duckdb_<q>.csv (+ .count.csv)
```

Consequences worth knowing before you regenerate:

- **`.cost.txt` is a pure function of `.cpu.txt` and `cost_model.conf`** — regenerating one
  `.cpu.txt` obliges the sibling `.cost.txt`, and `test_cost_model` re-derives every one of
  them, so a hand-edited `.cost.txt` goes red.
- **A device never authors a golden.** The device tier reads the sections the cpu tier
  wrote, which is what makes a divergence a red test rather than a quietly rewritten
  expectation.
- **The byte-level golden needs a second, deliberate variable**
  (`PEACOCK_REWRITE_RECIPE_BYTES=1`). Under plain `UPDATE_CANONICAL=1` the test verifies
  instead of rewriting, and says so — a bulk regen that moved the wire format goes red
  during the regen, before the goldens are pulled home. It pins what the C++ is handed, and
  it is the file a bulk regen must not quietly rewrite: the diff would come home among the
  others.
- **The `.duckdb_cost.txt` path is re-runnable without DuckDB**: `--extract-only` rebuilds
  the goldens from the committed profiles plus the parquet, so only a genuine oracle change
  needs the 1.5.4 pin.

## CI structure (`.github/workflows/pipeline.yml`)

`pipeline.yml` runs on pushes to master and on every PR, but not on documentation —
`**.md` and `llm-wiki/**`, which nothing builds, tests or reads. It takes two layers:
`paths-ignore` skips a wholly-documentation diff, and the **changes** job skips a
documentation-only push to a PR that carries code, which `paths-ignore` cannot see because
it judges the whole PR diff. Both fail open, and a doc file that ever becomes an input to
something has to come off both lists.

Five independent job chains:

```
changes ──► everything below
dataset-matrix (2 legs)          cpp-build-2502 ──► gpu-tests
cost-report ──► deploy-pages (master push only)          s3-datasets
```

- **changes** — the documentation gate above; every other job carries `needs: changes`
  and runs only on its `code == 'true'`.
- **dataset-matrix** — two matrix legs, each in a RAPIDS container: `cudf: 25.02`
  (`rapidsai/base:25.02-cuda12.0-py3.12`) and `cudf: 26.02`
  (`rapidsai/base:25.10a-cuda12-py3.12` — the leg's label is ahead of its image, #129).
  Both legs do the C++ build through the shared `.github/actions/cpp-build` composite
  (conda gcc + ccache + pinned cmake/ninja), `ctest --test-dir cpp/build -L cpu`,
  generate + validate sf1 (pinned DuckDB — the TPC-DS dsdgen column types drift between
  releases and would move every plan golden), then the CPU rust tiers:
  `test_plan_goldens`, the ffi rung as `--lib -- ffi_tests::` (which links the FFI but
  touches no device), `peacockdb-ffi --test test_ffi`, plus rust-only `--lib`, `test_cpu_executors`,
  and `test_cpu_corpus`. The `peacockdb` CLI is built here
  and not run: it has no test target, so this is the only thing that compiles it.
  The steps needing neither the generated dataset nor a device run on the 25.02 leg alone,
  since a rust-only test cannot see cuDF and a second leg would report the same failure twice:
  the exec-model python, `test_planner_join_capability`, `test_planner_join_refusals`,
  `test_null_analysis`, `test_cost_model`, `test_golden_format` and `test_corpus_goldens`.
  `CMAKE_GENERATOR` and `RUSTFLAGS` are job-level: cargo's fingerprint includes RUSTFLAGS,
  so a step carrying its own recompiles the dependency tree, and the image has ninja and no
  make, which flatc-fork's cmake needs told. Build and run are still separate steps, and
  their remaining env must stay byte-identical or the run step recompiles.
- **cpp-build-2502** — builds the 25.02 C++ side, bundles the Arrow/Parquet runtime libs,
  and stages `test_inc2_conformance`, `test_gpu_abi`, `test_gpu_recipe_walk`,
  `test_gpu_executors` and `test_gpu_corpus` as the `cpp-install-25.02` artifact. Separate from dataset-matrix so the GPU job can start without
  waiting for the CPU tests.
- **gpu-tests** (needs cpp-build-2502) — ssh to **shad-gpu** into a per-run `REMOTE_DIR`:
  rsync artifact + testdata, patch the binaries for glibc 2.35, generate sf1 on the host
  if absent, then run. Three guards, each closing a hole that shipped: sf40 presence is
  asserted by CI (`tpch.sf40/lineitem.parquet`) because the binaries themselves skip and
  exit 0, which would be green having verified nothing; every `peacock_*_tests` binary runs
  by glob with a ran-any assertion (a hand-written list once let `peacock_tpchv_tests` be
  built, shipped and patched but never run); and any binary reporting `PASSED 0 tests` is
  an error. The staged rust binaries then run with `--test-threads=1` (cuDF/RMM share one
  process-wide pool). No `set -e` — statuses are OR'd so one failure cannot skip the rest —
  and `REMOTE_DIR` is removed on `always()`.
- **cost-report** — the report crate and its inputs, and nothing else: python
  `testdata/test_duckdb_cost.py`, the `scripts/exec_model/tests/test_*.py` prototype set,
  and `cargo test -p cost-report`. Then report generation, the PR-comment upsert and the
  cost-regression gate against the base SHA, which fails the job on a regression. On a
  master push it uploads the Pages artifact. The comment has a 65,536-byte body to fit —
  `COMMENT_MAX_BYTES` in `cost-report/src/main.rs`, asserted over the real registry — so
  every anchor but the query cell is dropped from the markdown. The file's FIRST LINE is its
  sentinel and the workflow reads it from there, so the string exists once.
- **deploy-pages** (needs cost-report; master pushes only) — publishes that artifact.
- **s3-datasets** — reads parquet footers of the large S3 datasets over ranged GETs
  (schema, embedding dims, row counts). Skipped on fork PRs, where the secrets do not
  exist, and exits 0 loudly if the credentials are absent anyway.
- **validate-large.yml** — `workflow_dispatch` only, gated on the same repo: validates the
  named datasets in place on shad-gpu with the scale-safe validators, then runs the S3
  metadata check. The `datasets` input defaults to `tpch.sf40 tpcds.sf200`; `tpch.sf200` is
  allow-listed but deliberately not defaulted, because it does not exist yet.

Two traps before adding a step: a container job defaults `run:` to `sh`, not bash, so the
two of them declare `defaults.run.shell: bash` to get `-o pipefail` at all; and the python
steps install the current pandas — 3 on the runner's 3.12, 2.3 on a dev box's 3.10 — so the
prototype has to hold across that boundary, unpinned on purpose.

Which Rust targets run where is not folklore — `test_ci_coverage` fails when a target
exists that no workflow step names. Doctests are the one class outside that guard (#128).
The two python steps need no such guard because neither names files: the extractor test is
one file, and the prototype step globs `test_*.py` and errors when the glob matches
nothing. Inside a prototype file the equivalent hole — a test defined below the
`__main__` footer, which pytest collects and direct execution would not — is closed by
`tests/harness.py`, which reads the source back and fails naming what it missed.

## What `rust-only` means

A cargo feature (`peacockdb-core/rust-only` → `peacockdb-ffi/rust-only`, an empty marker),
and the definition lives in `peacockdb-ffi/build.rs`: under it the build script **skips
cmake entirely**, so nothing links `libpeacock_gpu` and neither cuDF nor a CUDA toolchain
is needed. Everything that reaches the FFI is compiled out behind
`#[cfg(not(feature = "rust-only"))]` — the GPU backend and its batch type, the extern
declarations, and the GPU test files, which are gated at file level because they would have
nothing to call.

So it is the Rust half built against DataFusion alone. That is why it is the fast loop,
and why anything a rust-only binary can do is by definition CPU-only.

**It selects a *build*, not a set of tests.** `cargo test --features rust-only -p
peacockdb-core` runs every target that compiles under it — including the full CPU
execution suite — not just the golden/meta tier. Naming a tier takes `--test`. This has
been mis-transcribed at least once; see the "refactor is verified with a subset" rule
below.

## Local build workflows and caches

One cargo target dir per workflow — this is what prevents cache thrashing (feature flags
and `cudf_ROOT` changes bust fingerprints; sharing a dir means recompiling the DataFusion
stack on every switch):

| Workflow | Command | C++ build dir | Cargo target dir |
|---|---|---|---|
| rust-only (golden regen/verify, fastest loop) | `cargo test --features rust-only -p peacockdb-core --test <target>` (drop `--test` and it runs every target that compiles, not a tier — see above) | — | `target/` |
| cost-report | `cargo test -p cost-report`; preview: `scripts/cost-report-preview.sh` | — | `target/` |
| C++ only, cudf 25.02 | `scripts/build.sh --cudf_ROOT <rapids-cuda-12.2> --gcc-version 12 ...` | `cpp/build` | — |
| C++ + staged Rust bins, cudf 26.02 | `scripts/build-test.sh --build` (drives cmake directly) | `cpp/build26` | `target-cudf-<basename cudf_ROOT>` |
| Rust + cudf (FFI) | via build-test scripts, or manually with `scripts/cargo-cudf.sh` | cargo OUT_DIR | `target-cudf-<basename cudf_ROOT>` |
| Any of the above, containerized | `scripts/docker-build.sh [--no-image] -- <command>` | `<cache-dir>/cpp-build` | `<cache-dir>/cargo-target` |

## Inspecting the tree

Four scripts answer questions about the shape of the tree rather than about a build. They take
no arguments beyond what is shown and need no dataset or device.

| Script | Question | Invocation |
|---|---|---|
| `scripts/case-inventory.sh` | every test case a shape compiles, one per line | `scripts/case-inventory.sh rust-only`, or `CUDF_ROOT=<rapids env> scripts/case-inventory.sh cudf` |
| `scripts/compare-inventory.sh` | did any case stop compiling into its target | `scripts/compare-inventory.sh <shape> <baseline> <current>` — the baseline is an argument, so it outlives whichever task took it |
| `scripts/visibility-dump.py` | every `pub`/`pub(...)` item with its declaring file; `--items` drops the file and visibility, giving the set a move cannot change | `scripts/visibility-dump.py peacockdb-core/src` |
| `scripts/residue-gate.sh` | does a retired name survive anywhere, tracked or not | `scripts/residue-gate.sh` |

`residue-gate.sh` resolves paths against the repo root and checks that the result is non-empty.
It reported a clean tree from the wrong directory once, because `git` rejected the pathspec and
every section came back empty — a gate that finds nothing and a gate that cannot look are the
same output otherwise.

The container and the native path deliberately do **not** share a cargo cache
(`/cache/cargo-target` + `RUSTFLAGS=-C debuginfo=0` vs `$PWD/target-cudf-*`), so
alternating between them costs a cold rebuild each way. That is the price of having
both entry points, not thrash to be diagnosed.

ccache is auto-enabled for both C++ workflows when the binary is present (host
compilers only — ccache + nvcc is unreliable). Rust caching is the per-workflow target
dir below.

Note the C++ side is built *twice* on the cudf path, into two dirs that are both used:
`build-test.sh` drives cmake into `cpp/build26` for the installable libs and the C++
test binaries it ships, while `peacockdb-ffi/build.rs` runs cmake again into cargo's
`OUT_DIR` for the copy the Rust binaries link against. The `.peacock-ffi-cudf-root`
stamp is what keeps the second one from rebuilding on every `--build`.

Rules that keep this healthy:

- **A refactor that must not change behavior is verified with a representative subset**
  — one query per mode/tier per binary, plus the full rust-only tier — not a full
  CPU/GPU suite run: the goldens are the invariant, so unchanged golden bytes and a
  green rust-only tier prove more per minute than re-running everything.
- **Day-to-day iteration is the rust-only loop** — plain cargo into `./target`, no
  wrapper, no C++/CUDA:
  `cargo test --features rust-only -p peacockdb-core --test test_plan_goldens`
- **Never run cudf-feature cargo builds in `./target`** — they would evict the rust-only
  cache and vice versa (the `ffi` feature and `cudf_ROOT` both change fingerprints, and
  the cudf side recompiles the DataFusion stack at opt-3). For one-off cudf/FFI cargo
  commands use `scripts/cargo-cudf.sh`, which requires `CUDF_ROOT` and derives *both*
  `CARGO_TARGET_DIR=target-cudf-$(basename "$CUDF_ROOT")` AND `CC`/`CXX` from that same
  basename (25.02 → gcc-12, 26.02 → gcc-14; an unknown root fails and asks for
  `GCC_VERSION`) — the same dir and the same compiler the build-test scripts use, so it
  shares their warm cache. Both halves matter: cc-rs emits `rerun-if-env-changed` on
  `CC`/`CXX`, so entering a target dir with a different C compiler re-runs every native
  build script (zstd-sys, bzip2-sys, lzma-sys, psm, blake3) and rebuilds the whole
  DataFusion stack above them — before this, alternating between the two entry points
  thrashed the cache each way:
  `CUDF_ROOT=~/data/miniforge3/envs/rapids scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_abi --no-run`
  For anything more than a one-off command, use `build-test.sh` / `build-test-shadgpu.sh`
  instead — they handle build, staging, shipping and running.
- **A cudf-shape binary needs `LD_LIBRARY_PATH` to run at all.** `libpeacock_gpu.so` lives in
  the FFI crate's `OUT_DIR` (`target-cudf-*/debug/build/peacockdb-ffi-*/out/lib`) and nothing
  puts it on the loader path, so the binary exits 127 with a loader error on stderr and prints
  nothing on stdout. It is loud, but a tool that reads stdout and ignores the status sees an
  empty list, which reads exactly like a target the `rust-only` cfg compiled out. Prepend that
  dir and the cuDF root's `lib` before listing or running one by hand.
- The FFI crate caches its cmake `cudf_DIR` in OUT_DIR; both build-test scripts clean it
  **only when the cuDF root changed** (stamp file `.peacock-ffi-cudf-root`);
  `PEACOCK_FFI_CLEAN=1` forces.
- `cpp/build` stays 25.02, `cpp/build26` stays 26.02 — separate dirs also dodge cmake's
  stale `cudf_DIR` cache. Keep both even when stale.
- `-Dcudf_ROOT` is the one prefix the scripts pass. `cpp/CMakeLists.txt` copies it into
  `CMAKE_PREFIX_PATH`, so cuVS and Arrow come from the same env and a fresh build dir needs
  nothing on PATH. Before that, a fresh dir configured only where the env's `bin` was on PATH,
  which CI's image has and a dev host does not.
- cuDF version scope: a functional GPU run on **25.02** (shad-gpu) is sufficient
  verification; **26.02 needs only to compile** (CI builds both legs).

## Memory constraints (15 GiB-class hosts)

- **At most one binary links at a time**: the cmake configures set a Ninja
  `link_pool=1` job pool (parallel links OOM the host; compiles stay parallel). The
  build-test scripts also serialize rust test-binary builds one `--test` per invocation.
- Cold cudf cargo builds are throttled to `CARGO_BUILD_JOBS=3` on <20 GiB hosts
  (override via env).
- Full local CPU suite: run with `-- --test-threads=2` or less; the heavy golden suites
  OOM at default parallelism.

## Remote hosts

| Host | Use | Managed by |
|---|---|---|
| **shad-gpu** (most used) | GPU test suite (cudf 25.02, H200-class; old glibc → patch step) | `scripts/build-test-shadgpu.sh` (`--build --push-binaries --patch --run[-detached]` / `--all`, `--run-status`). Resilient rsync + retries — the link is flaky |
| **verda** (when available) | large CPU runs, golden regen | `scripts/build-test.sh --host verda --all` (add `--rust-only` to skip the C++/FFI half) |
| **verda-gpu** (least used) | same root volume as verda, with a GPU attached | `scripts/build-test.sh --host verda-gpu --gpu --all` |
| **nebius** | large CPU-only VM | manual |

- **Testing the regen *mechanism* is not a full regen.** Scope it with `PCK_TEST_FILTER`
  (forwarded to every staged binary) — two or three queries prove the path as well as
  903 do, and a full `--update-canonical` run rewrites every golden on the remote and
  pulls the whole set back into a git working tree, where an unrelated diff can ride
  home in an unrelated commit. Note that a binary whose tests all filter out runs zero
  tests and passes, so say which binaries actually executed; and a filter naming a query
  matches no test whose name is a property rather than a query, so exercising those takes a
  filter that matches them or a separate run.
- **A GPU binary needs a fixed amount of free VRAM now, not a share of what it finds.** Each
  gtest main reserves a measured byte budget: `peacock_tpch_tests` 69 GiB, `peacock_tpchv_tests`
  30, and 1 GiB each for `peacock_gpu_tests` and `peacock_plan_tests`. Below its budget a binary
  does not shrink — the pool is not built and it runs on rmm's default resource, which for the
  sf40 pair is not slower but red. Read `[rmm] pool of N GiB could not be built` at the top of the
  log first: the `cudaErrorMemoryAllocation` failures under it are its consequence, not separate
  bugs. shad-gpu is a 143.7 GiB H200 shared with work outside this repo, so check `nvidia-smi`
  before debugging a red GPU tier ([#178](tickets.md#t178)). The budgets are H200 numbers;
  verda-gpu has never been sized against them. A run that sweeps a knob past its default sets
  `PEACOCK_RMM_POOL_BYTES=<bytes>` for that run — explicit bytes, no percentage — which is also
  the only way a non-H200 host runs these at all. `peacock_cudf_node_tests` at
  `PEACOCK_NODES_ROWS=100000000` peaks at 17.9 GiB and dies on its declared 10 without it.
- **Prefer verda for large CPU runs** (whole suite or big selections). It is not always
  up (the human starts it manually) — falling back to a local run is completely fine.
- **Comparing files across hosts: use checksums, and `LC_ALL=C sort` for any listing.**
  Two hosts collate `ls`/`sort` differently, so a manifest diff reports differences that
  are pure collation — this produced two false alarms in one session before checksums
  settled it. Compare `sha256sum`/`md5sum` output, not directory listings.
- Rented hosts change SSH host keys on reprovision: `ssh-keygen -R <host>` + re-keyscan
  rather than fighting the mismatch.
- **The shad-gpu patch step uses the build host's glibc version.** shad-gpu runs glibc 2.31,
  so shipped binaries are patched to a prefix under `~/glibc-<version>` there: 2.35 for a
  22.04 build host and CI's container, 2.39 for a 24.04 host such as dev. The version is read
  from `getconf` where the binaries are built, and the prefix is built once on the first patch
  from that host class. A binary patched to a glibc older than its own dies at load with
  `version GLIBC_2.38 not found`.
- **Golden regen**: on verda via `scripts/build-test.sh --host verda --rust-only
  --update-canonical`, then `--pull-goldens` to bring regenerated goldens back; or
  locally with `UPDATE_CANONICAL=1 cargo test --features rust-only ...`. Sync is one
  flag per kind per direction — `--push-<kind>` / `--pull-<kind>` for
  `parquet | goldens | duckdb-profiles | duckdb-dynfilters | queries`.
  **Pushes mirror (`--delete`), pulls are additive**, deliberately: the remote is a
  *partial* mirror (`testdata/goldens/` holds `tpch.sf40/`, which lives only on shad-gpu),
  so mirroring downward would delete fixtures the source host never had, out of a git
  working tree. (The sf40 *dataset* is what lives only there — its 16 CSV goldens are
  committed.) `recipe-payloads.txt` rides along safely because its test refuses to
  regenerate without `PEACOCK_REWRITE_RECIPE_BYTES=1`.
- **A golden that pins a producer against itself proves consistency, not correctness.** Where
  an artifact's only consumer is on the far side of an ABI, the first read by that consumer is
  the first check — until then the text and the digest can agree with each other and with the
  code, and all three be wrong.
  The tpch **embeddings cache is NOT syncable** — a per-host intermediate
  (`fetch_embeddings.sh`, ~1.8 GB, gitignored); regenerate it where you need it.
- Remote CPU runs ship built binaries + goldens + data only — never source. The plain cpu
  mode does not set `PEACOCK_TESTDATA_DIR`, so its binaries fall back to the compile-time
  path and the remote needs a `/media/data/peacockdb` symlink.
- **Large runs (test or regen): arm a monitor that reports progress every 5 minutes** —
  progress may stall (flaky links, OOM kills), and a silent stall looks identical to a
  long run.
- **Large CPU + GPU batches run in parallel** — kick off the shad-gpu run and the
  verda/local CPU run concurrently; neither waits for the other.
- **A run that outlives your ssh session** is `--run-detached`, read back with
  `--run-status`. That flag exits 0 only when the latest run finished with 0 — still
  going, died without writing its code, and a completion belonging to an earlier run are
  all non-zero, because the alternative is a status command that reports someone else's
  success as yours. Run state lives in `$REMOTE_REPO/.run-state/gate.{sh,log,rc,id}`,
  deliberately outside `cpp/install/` — that tree is mirrored with `--delete`, so a marker
  kept there is erased by the next push.

## Benchmarks

Wall-time runs of the C++ suites are manual; the protocol: `PEACOCK_BENCHMARK=1`
(`PEACOCK_BENCHMARK_RUNS=5`), execute-only timing, all-device-synced, report the
**2nd-minimum** of the runs. Current numbers: `llm-wiki/reports/benchmark-minimal.md`.

- **TPC-H / TPC-H+V, single GPU**: build `peacock_tpch_tests` / `peacock_tpchv_tests`
  (25.02 leg) and run on shad-gpu with the sf40 env vars
  (`PEACOCK_TPCH_SF40_DIR`, `PEACOCK_TPCH_GOLDEN_DIR`, `PEACOCK_TPCH_VEC_PARAMS`).
- **Multi-GPU**: build `peacock_multi_gpu_tpch_tests` / `peacock_multi_gpu_tpchv_tests`
  explicitly (EXCLUDE_FROM_ALL) from `cpp/build26` on a multi-GPU host; G=1 baseline via
  `CUDA_VISIBLE_DEVICES=0`. Benchmark **each query in its own process**
  (`--gtest_filter=...`) — multiple benchmark queries in one process are flaky at G≥2
  (process-global cudf stream state across WorkerPool teardowns). Correctness (single
  execute) is fine in one process. Mean-type aggregates must decompose to partial
  SUM+COUNT — never average partial means.

## Antipatterns

- **A wait loop that matches itself.** `timeout 900 bash -c 'until ! pgrep -f "cargo test …";
  do sleep 15; done'` can never exit: the wrapper's own command line contains the pattern, so
  `pgrep -f` finds the waiter and the loop waits for itself. It ends only when the timeout
  fires. Where a pattern is unavoidable, exclude the waiter's own pid.
