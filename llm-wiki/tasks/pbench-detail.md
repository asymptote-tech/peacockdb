# pbench — run detail

Working notes. The spec is [`pbench.md`](pbench.md) (frozen); the plan the developer works in is
[`pbench-impl.md`](pbench-impl.md).

## Branch and PR

- Branch `ENS-pbench`, forked off `ENS-duckdb-oracle` at `410111cf`.
- Task 3 of chain J, so its PR targets **`ENS-duckdb-oracle`**, not master. **PR #168**, opened at `87b8feb1`; base verified with `gh pr view 168 --json baseRefName`.
- **Task 2 (`stale-cells`) is `blocked(approved to build)` and has no branch**, so pbench forks off
  task 1's branch rather than task 2's. The content dependency is nil — disjoint corpus lines,
  disjoint registry rows, overlapping only in `build-test.md`'s counts and the CSV — so this is a
  resequence for the human at merge time, not a conflict.
- Workspace: `peacockdb-alpha` (`/home/dmitry/workspace/peacockdb-alpha`).

## Hosts and tools, probed 2026-10-08 before the dispatch

- **shad-gpu: down.** Times out on :22 and :443 while DNS resolves and the host key is intact, so
  it is the host. Down all day.
- **verda: unlocatable.** The name resolves nowhere here and there are no `VERDA_*` credentials to
  find the ephemeral instance's IP with. CPU runs are local.
- **duckdb 1.5.4 is local**, both the python module and the CLI — exactly the pin
  `duckdb_result.py` enforces, so the generator and DuckDB's answers run here.
- **Disk: 6.9 GiB free on `/`, 96% full.** `target/` is 19 GB and
  `target-cudf-rapids-cuda-12.2` 7.9 GB.

## Why this task was dispatched while two others wait

An analyst walked chain J's remaining tasks against the device absence. `stale-cells` is blocked —
see [`stale-cells-detail.md`](stale-cells-detail.md). Tasks 4 to 9 each have a device-only
verification bar and each sits behind an earlier task, `join-session-cpp` included: it compiles
here but its whole bar is a gtest matrix whose `main()` installs an RMM pool, and its
`TableResult` and `evaluate_column` surfaces move under tasks 5 and 6, so pulling it forward buys
~2000 lines of C++ reviewed on a compile and rebased over two refactors of its own foundation.

pbench is the one that progresses: the plan isolates the card into **Task 8** and nothing else.
Tasks 1 to 7, 6b, 6c's cpu leg and 9's documentation half all run in the rust-only tier here.
Three mechanisms make that true, each checked in the tree rather than assumed:

- every pbench gpu cell lands `disabled` (plan Task 5 Step 6), and `gpu_result_coverage` returns
  `Ok` for an absent `gpu-result.txt` when no cell is enabled, while `registry_datasets()` derives
  the dataset list from the CSV — both landed by duckdb-oracle's completeness pass for exactly this;
- `int8-key-group`, the one row with no expected ticket, is deliberately not landed until Task 8,
  which satisfies the "a disabled cell names a ticket" rule; nothing requires every
  `pbench-queries/*.sql` or every `duckdb-result.txt` section to have a corpus line, since both are
  read per line;
- duckdb 1.5.4 is local.

**So this task's realistic end state is `completeness approved`, not `done`** — its own bar has a
device cycle, and CI's `gpu-tests` job rsyncs to shad-gpu, so no PR in this chain can go green
while that host is dark. Two tasks will then share one cycle instead of one.

## Carried into the dispatch

- **27 rust-only cases are red on the base and are not pbench's**: the 26 `duckdb_gpu_*` cases
  (22 enabled tpch gpu cells, 4 tpcds) and
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, all waiting on the
  `gpu-result.txt` that no device has written. A pbench run must leave that count at 27 and add
  none.
- **Run the rust-only loop as plain `cargo test --features rust-only` into `./target`**, not
  through `scripts/cargo-cudf.sh`, which every command in the plan spells. That wrapper redirects
  to `target-cudf-rapids-cuda-12.2`, where a rust-only feature set recompiles the DataFusion stack
  from scratch — and `/` has 6.9 GiB left. `build-test.md:1050` states the rule.
- **`testdata/duckdb_result.py` still pins `--dataset choices=["tpch","tpcds"]` at `:230` and
  spells the default list again at `:238`.** duckdb-oracle deliberately left both to this task:
  one word is not enough, and `--dataset pbench` over an empty `pbench-queries/` would write an
  empty `duckdb-result.txt` rather than failing. The plan's `:104`/`:110` citations for this are
  stale from duckdb-oracle's growth; the usage line at `:20` is right.
- **#252**: six cases read the checkout through `CARGO_MANIFEST_DIR`, which a remote CPU run never
  ships, so anything run on verda shows `duckdb_tpcds_q17/q58/q61/q66`,
  `all_modes_expands_to_the_five_in_either_position` and `every_timed_case_is_enabled_on_a_device`
  red for reasons unrelated to the task. Moot while runs are local.

## Round 1 result (2026-10-08)

Everything but Task 8 and Task 9's two wiki files. **59 of pbench.md's 63 queries landed**; the
four that did not are each recorded below with the ticket that holds them. No device cell is
enabled, no `gpu-result.txt` was written, and the base's 27 red rust-only cases are still exactly
27 and still the same 27.

### What the generator produces

`testdata/pbench/gen.sql` + `testdata/generate_pbench.sh` → `testdata/pbench.sf1/`, **580 KB**
total (fact 519 KB, dim 57 KB, sub 1.3 KB, tiny 603 B, empty 148 B). Byte-identical across two
runs. `fact` 20,000 rows in **10 row groups**; dim 2,000 / 1; sub 200 / 1; tiny 8 / 1.

**`empty.parquet` has 0 row groups, not 1** as pbench.md's table says — a zero-row parquet holds
no row group at all. Nothing depends on the number, and it is why `cross-empty-build` is refused
(see #256 below).

**One deviation from the plan's gen.sql, and it is the task's biggest data finding.** The plan's
`COPY` used `ROW_GROUP_SIZE 1000` alone, and under it **the float specials did not survive the
write**: a dictionary-encoded page keeps whichever of `0.0`/`-0.0` and `NaN`/`-NaN` reached its
dictionary first, because parquet's dictionary dedupes by VALUE and `-0.0 == 0.0`. Measured on the
plan's own SQL: `dim` lost every `-0.0` and every `-NaN` (8 of each), and `fact` lost them
PARTLY — 59 of 191 `-0.0` survived, one row group at a time. Both files were still
`EXCEPT ALL`-identical to the generating query, because DuckDB's set operations compare `-0.0`
equal to `0.0` too. So the plan's Review Focus #3 was real, and the check it proposed
(`specials > 0` on `fact` only) would have passed over it.

The fix is `DICTIONARY_SIZE_LIMIT 0` on `fact` and `dim`, which forces PLAIN encoding. All
specials now survive exactly as generated, and `fact.parquet` got 54 KB **smaller**. Two guards:
`generate_pbench.sh --check` asserts the exact special counts on both tables
(`199 204 191 563 199 204 191` for fact's f_kf64/f_kf32, `10 8 8 10 8` for dim's) and fact's row
group count, and it was proved red-green — rewriting `fact.parquet` without the option fails the
check with `'239 164 59 695 239 164 45'`, while the `EXCEPT ALL` row comparison above it passes.

Every data property pbench.md asks for, measured off the committed parquet:

| property | measured |
|---|---|
| NULL join keys on both sides | `fact.f_k` 1,034 NULL of 20,000 (5.2%); `dim.d_k` 50 of 2,000 |
| skew | 10,058 of 20,000 fact rows on keys 0–2; 1,001 distinct keys, max 1000 |
| many-to-many | 950 of dim's 1,000 non-NULL keys appear exactly twice; `fact⋈dim` is 34,070 rows |
| NULLs under `NOT IN` | `sub.s_y` 8 NULL of 200, so `f_k NOT IN (SELECT s_y FROM sub)` is 0 rows |
| every hashable key type | Int8, Boolean, Float32/64, Decimal128(15,2) and (38,4), Timestamp s/ms/us/ns, Date32, UInt32, Struct, Utf8 — one column each on `fact`, overlapping columns on `dim` |
| float specials | fact: 199 `NaN`, 204 `-NaN`, 191 `-0.0`, 563 `0.0`, 182 NULL. dim: 10/8/8 |
| values past `i32::MAX` | `f_ku32` 9,920 of 20,000, 153 NULL |
| other NULLs | `f_kb` 1,177, `f_kdec38` 177, `f_s` 1,053 |
| empty sides | `empty` 0 rows; `tiny`/`sub` each one row group, so three of four lanes get no batch at tp4 (`partition_groups=[[[0]],[],[],[]]`) |

### The small-table override reached the plan

`tp4-single.plans.txt`: `GpuLoadParquet: table=fact, partition_groups=[[[0,1]],[[2,3,4]],[[5,6]],[[7,8,9]]], lanes=4`.
At tp1 the same scan is one lane over all ten groups. tpch's and tpcds's plan goldens are
byte-unchanged (`git diff` touches no file under `goldens/tpch.sf1` or `goldens/tpcds.sf1`).

### Every query's plan state and cells

123 of 295 pbench cpu cells are enabled; 32 rows have no cpu cell (24 refused by our planner,
5 on #190, 3 commented out on #243). Every gpu cell is off and **none was run** — the ticket on a
row is the blocker read off the plan and off pbench.md's "off on" column.

| query | cpu cells | duckdb oracle | gpu oracle | tickets |
|---|---|---|---|---|
| anti-null-preserved-condition | none | duckdb_none | golden_exact | 59 80 |
| bool-key-group | all five | duckdb_exact | golden_exact | 206 |
| cross-empty-build | none | duckdb_none | golden_exact | 208 256 |
| cross-projection | all five | duckdb_fingerprint | live_cpu | 207 |
| decimal15-key-group | all five | duckdb_exact | golden_exact | 95 |
| decimal15-key-join | all five | duckdb_fingerprint | live_cpu | 95 152 |
| decimal38-key-group | all five | duckdb_exact | golden_exact | 95 |
| empty-side-left-join | none | duckdb_none | golden_exact | 155 |
| exists-null-keys | all five | duckdb_exact | golden_exact | 152 |
| exists-or-mark | none | duckdb_none | golden_exact | 59 80 |
| finish-without-probe | all five | duckdb_exact | golden_exact | 173 |
| float32-key-group | commented out, cells `na` | duckdb_approx | golden_exact | 243 |
| float64-key-group | commented out, cells `na` | duckdb_approx | golden_exact | 243 |
| float64-key-join | commented out, cells `na` | duckdb_approx | golden_exact | 243 |
| full-join-null-keys | all five | duckdb_fingerprint | live_cpu | 152 |
| full-join-residual | none | duckdb_none | golden_exact | 153 |
| in-is-null | none | duckdb_none | golden_exact | 155 250 257 |
| indf-full-join | none | duckdb_none | golden_exact | 155 160 |
| inner-join-hot-keys | all five | duckdb_fingerprint | live_cpu | 152 220 |
| left-join-null-keys | all five | duckdb_fingerprint | live_cpu | 152 |
| left-join-residual | none | duckdb_none | golden_exact | 153 |
| like-column-pattern | none | duckdb_none | golden_exact | 190 246 |
| mark-cross-residual | none | duckdb_none | golden_exact | 59 80 |
| nl-full | none | duckdb_none | golden_exact | 160 |
| nl-inner | none | duckdb_none | golden_exact | 152 190 |
| nl-left | none | duckdb_none | golden_exact | 152 190 |
| nl-left-anti | none | duckdb_none | golden_exact | 160 |
| nl-left-decimal | none | duckdb_none | golden_exact | 190 215 |
| nl-left-semi | none | duckdb_none | golden_exact | 160 |
| nl-mark | none | duckdb_none | golden_exact | 160 |
| nl-projection | none | duckdb_none | golden_exact | 152 190 |
| nl-right | none | duckdb_none | golden_exact | 160 |
| nl-right-anti | none | duckdb_none | golden_exact | 160 |
| nl-right-semi | none | duckdb_none | golden_exact | 160 |
| not-exists-null-keys | none | duckdb_none | golden_exact | 59 80 |
| not-in-correlated | none | duckdb_none | golden_exact | 59 80 |
| not-in-uncorrelated | none | duckdb_none | golden_exact | 59 80 |
| not-in-under-or | none | duckdb_none | golden_exact | 59 80 |
| not-not-in | all five | duckdb_exact | golden_exact | 152 |
| not-or-not-in | all five | duckdb_exact | golden_exact | 152 |
| outer-on-true-empty | none | duckdb_none | golden_exact | 160 |
| probe-exists-residual | none | duckdb_none | golden_exact | 159 |
| probe-not-exists-residual | none | duckdb_none | golden_exact | 159 |
| right-join-null-keys | all five | duckdb_fingerprint | live_cpu | 152 |
| right-join-residual | none | duckdb_none | golden_exact | 153 |
| rollup-small-keys | the two tp1 | duckdb_exact | golden_exact | 189 |
| scalar-subquery-cross | the two tp1 | duckdb_approx | golden_exact | 63 199 |
| sparse-build-anti | all five | duckdb_exact | golden_exact | 212 |
| sparse-build-full | all five | duckdb_fingerprint | live_cpu | 212 |
| sparse-build-right | all five | duckdb_fingerprint | live_cpu | 212 |
| sparse-probe-left | all five | duckdb_exact | golden_exact | 152 |
| sparse-probe-semi | all five | duckdb_exact | golden_exact | 152 |
| timestamp-ms-key-group | all five | duckdb_exact | golden_exact | 240 |
| timestamp-ns-key-group | all five | duckdb_exact | golden_exact | 240 |
| timestamp-s-key-group | all five | duckdb_exact | golden_exact | 240 |
| timestamp-us-key-group | all five | duckdb_exact | golden_exact | 240 |
| ts-key-join | all five | duckdb_fingerprint | live_cpu | 152 240 |
| uint-key-group | the two tp1 | duckdb_fingerprint | live_cpu | 189 |
| uint-key-join | the two tp1 | duckdb_exact | golden_exact | 152 189 |

Every refusal is read off `testdata/goldens/pbench.sf1/*.plans.txt` and is the same at all five
modes. No query is refused by DataFusion itself, so no row has an `na` plan cell.

### Where the measurements disagreed with pbench.md

Eight, each a measurement rather than a choice.

1. **`not-not-in` and `not-or-not-in` PLAN.** pbench.md expects both off on #80. DataFusion 45
   already folds `NOT (x NOT IN …)` to `f_k IN (…)` and plans a `GpuHashJoin RightSemi`, which is
   exactly the shape pbench.md's "plans as" column predicted for after the rewrite — and a semi
   join over NULL keys is sound, so nothing refuses it. Both run at all five cpu modes and agree
   with DuckDB (5,002 and 4,915 rows). Their gpu cells carry `152`.
2. **Every pbench nested-loop join that plans is #190 on the CPU, not just `nl-projection`.**
   `nl-inner`, `nl-left`, `nl-left-decimal`, `nl-projection` and `like-column-pattern` all narrow,
   and `cpu_backend/join.rs:140` passes `None` where DataFusion takes the projection — the node
   declares 2 fields and DataFusion answers 4. So none of the five runs on either engine. The
   plan anticipated this for three of them ("possibly nl-inner, nl-left, nl-left-decimal");
   `like-column-pattern` is the fourth and was not foreseen.
3. **`scalar-subquery-cross` is #199 on the cpu at the three tp4 modes**, not only #63 on the
   device: `Column 'count(*)' is declared as non-nullable but contains null values` — the merged
   `count(*)` over an empty lane, the same cell tpcds q96, q90 and q88 carry, and only the tp4
   modes leave a lane empty. It runs at the two tp1 modes.
4. **`in-is-null` does not demonstrate #250, and DataFusion answers it wrongly** — see **#257**
   below. Its plan cells are disabled on `155` (the refusal it meets, `plan node EmptyExec`).
5. **`cross-empty-build` does not demonstrate #208** — it never reaches run time. See **#256**.
6. **Three queries cannot be in the tree at all** — see **#255**.
7. **#212 did not fire on the cpu.** pbench.md expects the three `sparse-build-*` off at the tp4
   cpu modes on #212; all three run at all five modes and agree with DuckDB. `212` stays on the
   rows as the device-side expectation Task 8 tests.
8. **Nine answers are over the 256 KiB result cap**, so both sides hold a fingerprint and the
   rows take `duckdb_fingerprint` with `gpu_oracle = live_cpu`
   (`each_declarations_two_oracles_suit_each_other` requires the pair):
   cross-projection, decimal15-key-join, full-join-null-keys, inner-join-hot-keys,
   left-join-null-keys, right-join-null-keys, sparse-build-full, sparse-build-right, ts-key-join,
   plus uint-key-group once its tp1 authority writes the section.

### Tickets filed

- **#255 — the planner PANICS on a Struct or Interval column** (`complete-coverage.md`).
  `common::type_structural_size` has an arm per flat type and `panic!`s otherwise, and the
  planner's memory estimation calls it for every field of every node's output schema. So
  `struct-key-join`, `struct-through-join` and `interval-through-join` abort the process at plan
  time rather than being refused — and a query file for any one of them in
  `testdata/pbench-queries/` takes all five of pbench's plan goldens down with it. The three are
  therefore NOT in the tree; their SQL is in the ticket. This is why 59 queries landed and not 62.
  Not #249: that is the wire writing an unnamed type as `Null`, and this fires before a plan
  exists, so #249's own "corpus queries" paragraph was corrected to say so.
- **#256 — a scan whose row groups all prune is refused as an invalid plan** (`joins.md`).
  `scan_mapping::partition` returns `PlanError::Invalid` on an empty survivor list, with a comment
  saying the meaning of an empty scan is the caller's decision; nobody made it. `empty.parquet` has
  zero row groups, so `cross-empty-build` is refused at plan time. The wider reach is a filter
  that prunes every row group — `WHERE f_id > 1000000` over any table — which is an ordinary
  selective query. DataFusion plans both and answers zero rows.
- **#257 — DataFusion 45 answers `(x IN (subquery)) IS NULL` as nothing at all** (`df-upgrade.md`).
  Measured: `in-is-null` plans to a bare `EmptyExec` — both scans gone — and DataFusion answers
  0 rows where DuckDB answers 14,998, which is the right answer. It is ours because DataFusion at
  one partition is the corpus' cpu oracle, so the query would be checked against the wrong answer
  and agree with it. Today our planner refuses the `EmptyExec` (#155), so nothing serves the wrong
  answer; the moment that arm lands it will. **The DuckDB oracle is the only thing that could have
  caught this**, which is #235's whole argument, demonstrated.

No ticket was attached to a row that the spec did not already expect, except the three above and
`190` on `like-column-pattern`.

### The engine changes, both refusals

- **A wire-type refusal names its ticket** (`wire/expr_writer.rs`): `#240` for a `Timestamp`
  target, `#249` otherwise. Pinned by
  `wire::expr_writer::tests::a_type_the_wire_cannot_name_is_refused_with_its_ticket`, which was
  run red first. `timestamp-s-key-group`'s golden line now reads
  `not runnable: unsupported: unsupported Arrow data type: Timestamp(Second, None) (#240) at #1`,
  which is what lets `every_refusal_names_a_ticket_that_exists` accept it.
- **#227's cpu half** (`cpu_backend/mod.rs`): `nulls_where_none_declared` names every column
  holding a NULL where its field declares none, with the count, and `declared_as` calls it AHEAD
  of its early return. Two tests, both red first.
  **Note for the task that closes #227:** the equal-schema hole the spec describes is not
  reachable through a `RecordBatch` — arrow validates nullability in `try_new` and
  `try_new_with_options` alike, so a batch whose own schema says non-nullable cannot hold a NULL,
  and a batch whose schema says nullable is not equal to a non-nullable declaration. That is why
  the rule is a function over `(columns, declared)` tested directly, and why the end-to-end test
  asserts the new message rather than the hole. The device half (the two validators in
  `test_support/schema_validation.rs`, which compare `DeviceSchema` — name, type id and scale,
  with no null counts at all) is untouched and is where the real gap is: #227 closes when it lands.

### The collapse readings (pbench.md's "collapse to one lane")

Read off `tp4-single.plans.txt`, and pinned by
`pbench_shows_three_kinds_of_join_collapsing_four_lanes_to_one`:

- **collapse-nested-loop** — `nl-inner`: `GpuNestedLoopJoin lanes=1` with a `GpuMergePartitions`
  on each side, over a `fact` loader at `lanes=4`. Reached.
- **collapse-cross** — `cross-projection`: the same with `GpuCrossJoin`. Reached.
- **collapse-collect-left** — **REACHED, and pbench.md left it open.** `sparse-probe-left` is a
  KEYED join (`on=[(t_k@1, d_k@1)]`) at `lanes=1` with a `GpuMergePartitions` on both sides:
  DataFusion planned it `CollectLeft` and #140 merges rather than broadcasts. `sparse-probe-semi`
  and `finish-without-probe` are the same shape.
- **collapse-not-in** — not reached: `not-in-uncorrelated` is refused on #59/#80 today. The task
  that lifts it reads the merge then, as pbench.md says.

### Deviations from pbench-impl.md

1. **`int8-key-group`'s QUERY FILE is held back too, not just its row.** The plan has Task 2 write
   63 `.sql` files and Task 5 write 62 rows. Those cannot both hold:
   `plan_goldens.rs::the_registry_matches_the_goldens_in_both_directions` requires a registry row
   for every section of a plan golden, and a section exists for every `.sql` in the queries
   directory. So `int8-key-group.sql` lands in Task 8 with its line and row. Two alternatives the
   human may prefer, both legal under `registry.rs` (whose rule counts `disabled` cells only, so
   `na` does not need a ticket): land the file now with an `enabled/na/na` row and no corpus line;
   or land it fully with cpu `all_modes` and gpu cells `na`. The second gives the cpu coverage now
   and leaves Task 8 only the gpu cells, but writes `na` where every other pbench row writes
   `disabled`, which is the shape the "a disabled cell names a ticket" rule exists to stop people
   reaching for. Held as the plan says instead.
2. **`DICTIONARY_SIZE_LIMIT 0` on `fact` and `dim`** (above).
3. **`f_kdec38`'s expression needed an explicit cast.** The plan's
   `((i % 300) * 1234567890123.0001)::DECIMAL(38,4)` fails to bind — DuckDB types the literal
   `DECIMAL(17,4)` and overflows the product at `DECIMAL(18)`. Written
   `((i % 300) * 1234567890123.0001::DECIMAL(27,4))::DECIMAL(38,4)`.
4. **`duckdb_result.py` grew a `query_files` guard and a `mkdir`,** not just the dataset name.
   `generate` wrote the golden from whatever the glob returned, so `--dataset pbench` over an empty
   `pbench-queries/` would have written an EMPTY `duckdb-result.txt` — a golden every
   `duckdb_<ds>_<q>` case reads as "DuckDB does not answer this query". Three ways in are now
   refused by name: no query directory, a directory with no `.sql`, and an `--only` that matches
   nothing (which would otherwise replace the whole golden with the sections it did match — a
   hazard that predates this task and still applies to a NON-empty `--only`). Five tests in
   `test_duckdb_result.py`, run red first. The `mkdir` is because a dataset's first golden arrives
   before its directory does.
5. **The plan's `#[cfg(test)] mod dataset_knobs { … }` inline in `test_support/mod.rs` would fail
   `test_module_layout`** (`a_test_module_lives_in_its_own_file`). The tests are in
   `peacockdb-core/src/test_support/tests.rs` behind `#[cfg(test)] mod tests;`, the convention the
   rest of the component already uses.
6. **`every_published_seq_addresses_the_kind_its_recipe_claims` now reads `NOT_RUNNABLE`** instead
   of asserting `["tpch mixed-join"]`. pbench's `timestamp-s-key-group` is a second uncrossable
   query, and that table already declares exactly this set — so the alternative was a second list
   to keep in step.
7. **The payload golden keeps its own two-dataset loop.** `the_payload_golden_carries_what_each_call_hands_the_executor`
   reads `["tpch", "tpcds"]` explicitly, as the plan says. The OTHER payload test —
   `the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce` — now reads
   `CORPUS_DATASETS`, and **pbench introduced no fb kind and no call shape the payload golden does
   not already cover**, so it is green with no payload query added and no `sha256=` line moved.
8. **The PR comment went over GitHub's cap and needed two more byte cuts** — see below.
9. **No commits.** The human commits. The plan's commit order still matters: the goldens under
   `testdata/goldens/pbench.sf1/` have to be in the same commit as, or an earlier one than, the
   `CORPUS_DATASETS` change in `test_support/mod.rs`, or the commit in between points every
   dataset loop at goldens that do not exist.

### The cost widget, and the comment cap

`all_datasets` is one constructor `main` and the tests both read, so the page and the cap guard
cannot disagree about which datasets exist. `collect_cost_goldens` gains `goldens/pbench.sf1`.
pbench has no `qN.duckdb_cost.txt`, so its rows render with no DuckDB cost, as tpch's named
queries do.

**pbench's 59 rows put the PR comment at 79,352 bytes against the 65,536-byte cap.** Three cuts,
in the order they were measured:

| change | bytes | left |
|---|--:|--:|
| (start, with pbench) | 79,352 | — |
| 6b's one larger tick (`✔` for a fully enabled cell) | −3,444 | 75,908 |
| the comment's query links take a 7-character sha | −6,501 | 69,407 |
| no `<sub>` on the three mode cells | −6,369 | **63,038** |

That leaves **2,498 bytes spare, about 7 more rows**. Neither cut loses anything: GitHub resolves
an abbreviated sha in a blob path exactly as it resolves the full one (and the HTML page keeps the
full one, being archived per sha), and the `<sub>` was shrinking the glyphs that are the thing the
comment is scanned for. **But 7 rows is the whole remaining budget**, and this chain spends 4 of
them: `int8-key-group` in Task 8, and #255's three queries when it closes. The next task to add a
corpus query will go red on `the_pr_comment_fits_under_the_body_cap`, and the lever left is a real
decision rather than a byte cut — the Features column out of the comment (≈9 KB, information the
page still carries), or pbench collapsed in the comment to its summary line with the rows on the
page only.

### Suite numbers, before and after

Measured on this branch, `cargo test --features rust-only` into `./target` (NOT through
`scripts/cargo-cudf.sh`, per `build-test.md:1050` — the wrapper's target dir would recompile the
DataFusion stack with 6.9 GiB free).

| suite | base | now | delta |
|---|--:|--:|---|
| `--lib` | 657 + 2 ignored | 670 + 2 ignored | +13: 5 pbench plan cases, 3 in `test_support::tests`, 2 plan-shape/collapse, 1 wire refusal, 2 #227 |
| `test_cpu_corpus` | 679 pass / **27 fail** | 858 pass / **27 fail** | +179 cases: 123 cpu cells + 56 `duckdb_pbench_*` |
| `test_corpus_goldens` | 26 | 26 | — (now over three datasets) |
| `test_cost_model` | 3 | 3 | — (now over three golden dirs) |
| `test_golden_format` | 43 | 43 | — |
| `test_module_layout` | 18 | 18 | — |
| `test_ci_coverage` | 9 | 9 | — |
| `cost-report` | 36 | 39 | +3 |
| `test_duckdb_result.py` | 14 | 20 | +6 |

Not in the table, and run because Task 7 touched them: `scripts/exec_model/tests -k corpus`
**94/94 pass** (the documented "minutes, not seconds" manual suite — 79 in a 30-minute window and
the remaining 15 named explicitly, 15:22); `bash -n` clean on `build-test.sh`,
`build-test-shadgpu.sh` and `generate_pbench.sh`; `cargo test --features rust-only
-p peacockdb-core --no-run` emits **zero warnings**. `cost-report` has one pre-existing
`sha_links is never used` warning, present on the base and untouched.

The workflow edit was verified mechanically, both ways the developer section asks for:
`yaml.safe_load` parses `pipeline.yml` (7 jobs), and each of the two edited `run:` blocks was
rendered to a file and `bash -n`'d — dataset-matrix / "Generate SF-1 test data" and gpu-tests /
"Rsync artifacts and testdata to remote", both exit 0. CI itself was not looked at.

**The 27 red are the base's 27, unchanged**: the 26 `duckdb_gpu_<ds>_<q>_<mode>` cases (22 tpch,
4 tpcds) and `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`. All 27 carry
`does not exist, so no device answer is recorded` — grepped, 27 occurrences. pbench added no 28th:
no pbench gpu cell is enabled, so `gpu_result_coverage` returns `Ok` for its absent file.

**`build-test.md:25`'s count was right and mine was briefly wrong**: `test_cpu_corpus` is 706 at
the base (679 + 27), not 707.

### What the next person needs

**Task 8's device cycle, exactly.** Every pbench gpu cell is `disabled` and not one has been run,
so Task 8 is a first measurement rather than a confirmation.

1. **Enable every pbench line's `gpu_modes` in a scratch edit** and run
   `scripts/build-test-shadgpu.sh` with `PCK_TEST_FILTER='gpu_pbench_'` and
   `PCK_WRITE_GPU_RESULT=1`, then `--pull-results`. The host scripts already ship
   `testdata/pbench-queries` and `testdata/pbench.sf1` (`build-test-shadgpu.sh`, and
   `pipeline.yml`'s gpu-tests rsync), and nothing generates pbench on the host.
2. **Only the 27 rows with a cpu cell can have a gpu cell at all** —
   `every_device_cell_has_a_cpu_cell_at_the_same_mode`. The 32 rows with no cpu cell are closed to
   Task 8: their device cells stay off until the chain lifts their cpu refusal.
   The rows to look at, and what each is expected to show:
   - **expected to PASS at tp1** (no shuffle): bool-key-group, decimal15/38-key-group,
     timestamp-ms/us/ns-key-group, uint-key-group, rollup-small-keys. Each carries one ticket
     (#206, #95, #240, #189) that pbench.md says is a tp4-only blocker, so a tp1 cell that fails
     is a finding and wants its own ticket.
   - **expected to FAIL at tp4** on the shuffle's hasher: the same rows (#206 bool, #95 decimal,
     #240 timestamp, #189 unsigned and the grouping id).
   - **`timestamp-s-key-group` is not runnable on any device**: `NOT_RUNNABLE` declares it on #240
     and the plan golden carries the line in all five modes. Its gpu cells cannot be enabled until
     repartition-keys adds the fbs timestamp types.
   - **expected to FAIL on #152** (the build handle not surviving a streamed probe): every join
     that plans — exists-null-keys, full/left/right-join-null-keys, inner-join-hot-keys,
     decimal15-key-join, uint-key-join, ts-key-join, not-not-in, not-or-not-in, sparse-probe-left,
     sparse-probe-semi. `tp1-single` is the mode that usually reaches past it.
   - **the three `sparse-build-*`** carry `212` and the cpu did NOT meet it (all five modes run),
     so the device is where #212 is tested at all.
   - **`finish-without-probe`** carries `173` (a LeftMark whose probe lanes get no batch);
     **`scalar-subquery-cross`** `63` (`copy_if_else` over two branch types) and runs on the cpu
     at the two tp1 modes only; **`cross-projection`** `207`.
   - **nine rows say `gpu_oracle = live_cpu`** because their answers are over the result cap:
     cross-projection, decimal15-key-join, full-join-null-keys, inner-join-hot-keys,
     left-join-null-keys, right-join-null-keys, sparse-build-full, sparse-build-right, ts-key-join,
     uint-key-group. Those compare against a device-side cpu run, not a committed section.
3. **`int8-key-group` lands in Task 8 whole** — `testdata/pbench-queries/int8-key-group.sql` with
   `SELECT f_k8, count(*) AS n FROM fact GROUP BY f_k8`, its corpus line, and its registry row.
   Adding the file rewrites all five plan goldens (one more section each), needs
   `python3 testdata/duckdb_result.py --dataset pbench` re-run, and needs its cpu/cost/`mini.result`
   sections merged (`PCK_UPDATE_SECTIONS=1 … --test test_cpu_corpus -- int8_key_group`). Expected:
   cpu at all five modes, gpu at all five (Int8 narrows to INT32 in the kernel) — and if every cell
   is on it is the one row that legally carries no ticket.
4. **Do NOT turn a cell on without the cycle**:
   `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` fails for a cell with no
   `gpu-result.txt` section, and that is the guard holding the base's 27 red today.
5. **Watch the comment cap** (above): Task 8 adds one row and leaves ~6 of 7.

**For the task that closes #255**: the three queries' SQL is in the ticket. Adding them needs the
estimator to REFUSE a type it has no deterministic width for, through `PlanError` — the panic's own
comment says a nested type's width cannot be derived from the parent row count, so the arm wanted
is a refusal and not a number. Then `struct-key-join` carries `245` and `249`,
`struct-through-join` and `interval-through-join` carry `249`, and all three need a
`duckdb_result.py` `cell()` that renders a struct and an interval as arrow-rs does: DuckDB's
section holds `{'a': 1, 'b': 'v1'}` and `1 day, 0:00:00` where arrow prints `{a: 1, b: v1}` and an
interval's own form, so `duckdb_exact` over them compares two spellings and not two answers.

**For the task that closes #243**: the three commented lines are in `corpus_cases.inc` with the
measurement above them, and their rows carry `243` with every cpu and gpu cell `na` — a commented
line declares nothing, so a `disabled` cpu cell would want a `skipped:` golden section that no
declaration writes.

### The wiki lines this round wants, for the human

`build-test.md` and `architecture.md` are the human's, so the round did not touch them. Every
number below is measured on this branch.

**`build-test.md`**

1. **`:25`**, the cpu tier header:
   `1394 cases: --lib 659, test_cpu_corpus 706, test_corpus_goldens 26, test_cost_model 3`
   → **`1573 cases: --lib 672, test_cpu_corpus 885, test_corpus_goldens 26, test_cost_model 3`**
   (`--lib` is 670 passing + 2 ignored, as the 659 was 657 + 2.)
2. **`:29`**, the `Corpus, cpu` row: **705 → 884** (the `Registry ↔ CSV, cpu` row stays 1, and
   884 + 1 = 885).
3. **`:30`–`:45`**, the prose under those two rows. The numbers that moved:
   - `116 queries at the modes each is correct at` → **175** (116 + 59 pbench rows with a line);
     of those, **143** have at least one enabled cpu cell.
   - `551 cells` → **674**.
   - The four-queries-out list gains pbench's: **24 refused by our planner**, **5 on #190**
     (nl-inner, nl-left, nl-left-decimal, nl-projection, like-column-pattern), **3 commented out
     on #243**, and the mode-scoped ones — rollup-small-keys, uint-key-group, uint-key-join and
     scalar-subquery-cross each run at the two tp1 modes only (#189, and #199 for the last).
   - `Then the DuckDB tier, 149 of the count` → **205** (176 `duckdb_<ds>_<q>` cases, one per
     corpus line, + 26 `duckdb_gpu_*` + the 3 meta cases).
   - `Today 93 lines are duckdb_exact, 15 duckdb_approx, 4 duckdb_fingerprint, 4 duckdb_none and
     4 duckdb_divergent` → **109 duckdb_exact, 16 duckdb_approx, 14 duckdb_fingerprint, 33
     duckdb_none, 4 duckdb_divergent** (176 lines).
4. **The Datasets table, after the `tpch.minimal` row at `:662`**, a new row:
   `| pbench.sf1 | 5 tables, 580 KB, git-committed | ✓ | ✓ | ✓ | — | plan + CPU corpus subsets, the join-rewrite chain's shapes |`
   — all three hosts ✓ because it is committed and rsynced, not generated.
5. **`:671`, the `Paths:` line**: `testdata/{tpch.minimal,tpch.sf1,tpcds.sf1,embeddings-cache}`
   → add `pbench.sf1`.
6. **`:679`, the "The sf1 parquet is generated" note**: say that pbench is the exception —
   committed like `tpch.minimal`, written by `testdata/generate_pbench.sh` from
   `testdata/pbench/gen.sql`, and checked rather than regenerated in CI
   (`generate_pbench.sh --check` in dataset-matrix). It is in no S3 bucket and
   `check_s3_datasets.py` / `dataset_checks.py` key on tpch/tpcds table specs, so neither
   changes.
7. **`:697`, the Golden files paragraph**: `tpch.sf1` (39 files), `tpcds.sf1` (116), `tpch.sf40`
   (16) → add **`pbench.sf1` (17)**: the five `.plans.txt`, the five `-mini.cpu.txt`, the five
   `-mini.cost.txt`, `mini.result.txt` and `duckdb-result.txt`. pbench has no `duckdb_cost`
   goldens — its queries are named, not `qN` — which is why 17 and not 17 + 59.
8. **The goldens diagram at `:720`**: a second top-level branch beside
   `generate_testdata.sh`, since pbench's parquet is not generated per host:
   `testdata/pbench/gen.sql → generate_pbench.sh → pbench.sf1 (parquet, COMMITTED)`, feeding the
   same `plan_goldens` / corpus-cpu-tier branches below it. The `duckdb_result.py` box already
   covers it.
9. **`:398`, the gpu tier line: NOTHING to change.** No pbench device cell is enabled, so the
   device tier's count is unmoved. That line is Task 8's.

**`architecture.md`** — one paragraph after the small-table rule's at `:96`–`:102`:

> The threshold is per dataset where the corpus plans. `test_support::small_table_bytes_for` gives
> pbench `0` and every other dataset `planner::SMALL_TABLE_BYTES`: pbench's five tables are all
> under 5 MB, so the rule as tpch and tpcds see it would plan every pbench scan as one lane — and
> pbench exists to show the multi-lane shapes those two cannot reach. With it off, `fact` splits
> across four lanes at tp4 (`partition_groups=[[[0,1]],[[2,3,4]],[[5,6]],[[7,8,9]]]`) and the
> one-row-group tables leave three of four lanes with no batch at all.

Nothing in `architecture.md`'s capability matrix moves: this task changed no join arm, and its two
engine changes are refusals that change no answer.

### The wiki counts, as applied — four of the round's figures were wrong

A researcher recomputed the page from the tree rather than applying the round's deltas, and four
numbers did not survive.

- **The cpu header is 1586, not the 1573 the round proposed** — 1573 is what you get summing the
  stale `--lib` 659 against the new `test_cpu_corpus` 885. 672 + 885 + 26 + 3 = 1586.
- **The corpus has 176 lines, not 175**, and the round's "116 + 59" was two errors cancelling
  badly. 116 was never a line count: it is the number of lines with at least one enabled cpu cell,
  read together with the page's "four queries are out entirely". And `corpus_cases.inc` gained
  **56** lines, not 59 — 59 is the number of rows added to `cost-registry.csv`, three of which are
  the #243 rows whose `corpus_query!` line is commented out. So: 176 lines, **143** with an
  enabled cpu cell, **33** out entirely, and those 33 are exactly the 33 `duckdb_none` lines.
- **Five of the thirteen new `--lib` cases had no row on the page**, which is why the block's rows
  would not have summed to its header: `test_support/tests.rs` is a module unit nobody had a row
  for, and `plan_goldens.rs`'s two pbench assertions
  (`every_pbench_join_plans_as_its_spec_says`, `pbench_shows_three_kinds_of_join_collapsing_four_lanes_to_one`)
  belong to none of the three plan-golden rows that exist — they say what the planner was supposed
  to do, where a plan golden records what it did. Two rows added, `Harness datasets` (3) and
  `pbench's plan shapes` (2). The other eight land on existing rows: the five mode rows 2 → 3,
  `CPU backend executors` 68 → 70, `Expression writer` 16 → 17.
- **The cost-report row was one high before this task** (36 at HEAD against a stated 37), so the
  old grand total inherited that. Both the page's +2 and the crate's +3 arrive at 39, which is
  measured, so the new value is unambiguous and the drift is gone.

Applied, with the arithmetic closing: cpu header 1586 equal to its rows, grand total **2776**
(Rust 2278, C++ 97, Python 401). Also taken: the Datasets table gains `pbench.sf1` and the
sentence above it stops saying only tpch.minimal is in git; the `Paths:` line; a note that pbench
is the committed exception to "the sf1 parquet is generated", with why a dataset whose point is
its values is committed rather than regenerated; the Golden files paragraph at 17 files with the
reason it has no per-query DuckDB cost oracle; the goldens diagram gaining pbench's own top-level
branch; "the ten goldens" now fifteen; and `architecture.md`'s per-dataset small-table paragraph,
as drafted here.

**The comment cap went to a ticket rather than into this task.** [#258](../tickets/testinfra.md#t258)
carries the measurement, the three cuts and the two remaining levers, with the argument for taking
the Features column out of the comment. It fits today with about seven rows spare, and the choice
changes what every reviewer sees in every comment from then on, so it is the human's rather than
this task's — which is also why it is filed where somebody will read it before the guard goes red.

## Review round 1 (2026-10-08)

A fresh reviewer over `git diff ENS-duckdb-oracle...HEAD`. **1 blocking, 6 important, 6 nits.** It
ran duckdb 1.5.4 over the committed parquet, python over the goldens and the CSV, and read
DataFusion 45's vendored source; it ran no cargo.

**The positive half, which on a data task is worth as much as the findings.** The data bites on
every property it was built for, measured: `sub.s_y` holds 8 NULLs, so `not-in-uncorrelated`'s
right answer is 0 rows where a two-valued engine answers 13,964 — the sharpest discriminator in
the set; `anti-null-preserved-condition` keeps 1,091 rows of which 87 are kept only because `d_kb`
is NULL, which is the spec's stated point; `f_ku32` has 9,920 values past `i32::MAX` and
`uint-key-join` still finds 1,984 matches; `full-join-null-keys` has 1,088 padded rows; `t_pat`'s
eight patterns match 3,722 rows; every answer is non-degenerate except the two meant to be. All 24
refused rows cite their row's ticket at all five modes, checked mechanically over 59 rows × 5
modes. The small-table override is visible in the golden — `fact` at `lanes=4` with
`partition_groups=[[[0,1]],[[2,3,4]],[[5,6]],[[7,8,9]]]`, `dim`/`sub`/`tiny` at one group over
four declared lanes — so the #173 and #212 shapes come for free. 55 of 56 pbench result sections
are byte-identical to DuckDB's; the 56th is `48.550000` against `48.55` and is correctly
`duckdb_approx`. `build-test.md`'s recomputed numbers all check out, 884 = 674 + 5 + 205 included.

**#257 is confirmed, and its explanation was wrong.** The reviewer reproduced DuckDB's 14,998 and
decomposed it (1,034 NULL `f_k` plus 13,964 non-matching, with 8 NULLs in `sub.s_y`), then traced
the mechanism in DataFusion 45: an `InSubquery` inside a larger expression becomes a LeftMark join
plus a `mark` reference; the mark is declared non-nullable; `IsNull` over a non-nullable expression
folds to `false`; `where false` becomes an `EmptyRelation` that propagates and takes both scans
with it. So the ticket's guess at the cause was right but its claim that the logical plan still
carried the filter was read off the *initial* plan. Corrected. #255 and #256 also hold, the first
by reading and the second with the second way in confirmed.

### Blocking

1. **#227's device half never landed and the cpu half is a message, not a check.**
   `test_support/schema_validation.rs` is not in the diff at all, and `device_schema::device_divergence`
   compares column count, names and types with no nullability and no null counts — while the spec
   names that file and both validators and says #227 closes when both checks land. The cpu half is
   correct but inert as coverage: `declared_as` ends at `RecordBatch::try_new`, where arrow already
   refuses the violation, which the round's own docstring says. **The card being down does not
   explain it** — `cpu_schema_validator` is reachable in the rust-only tier by its own doc comment,
   so the null-count check can be added and proven here. Handed to the developer; whatever is left
   over goes in the signoff and the board stops claiming #227.

### Important

2. **`generate_pbench.sh --check` cannot go red for the thing CI says it proves.** It reads the
   sign counts off the committed `pbench.sf1/`, not off the regeneration in `$OUT`, and `EXCEPT ALL`
   is blind to `-0.0` against `0.0` — so a `gen.sql` that stopped producing the float specials
   passes, which is the exact regression the round's own `DICTIONARY_SIZE_LIMIT 0` finding was
   about. `pipeline.yml` claims the step proves them.
3. **The script writes on any unrecognized argument.** `MODE=${1:-write}` then a single
   `--check` test, so `--chek` or `--dry-run` overwrites the committed data, against
   `coding-style.md`'s rule to validate arguments before the first side effect.
4. **Eight ticket sentences the branch falsified** — the one kind of drift a dataset written against
   open tickets guarantees. Taken by the coordinator: #206, #153, #160 and #173 now have queries;
   #189's cell count 15 → 24 and #199's 9 → 12; #245's named query could not land behind #255 and
   #250's lands on #155 instead, so both tickets now say so rather than naming a query that does
   not do what they claim.
5. **`duckdb-result.txt` holds three sections for queries that do not exist** —
   `interval-through-join`, `struct-key-join`, `struct-through-join`, the three held back on #255.
   The reviewer regenerated the golden and it is byte-identical except for exactly those three,
   which a regeneration deletes; nothing enumerates this golden's sections against the registry,
   where the plan goldens have that guard both ways. And the orphan is not usable: the interval
   renders as Python's `str(timedelta)`, `1 day, 0:00:00`, which is not arrow-rs's form.
6. **An insertion stole a doc comment** — `all_enabled` went in under `mode_cell_html`'s 13-line
   block with no blank line, so the block documents a two-line predicate and the function it was
   written for has none.
7. **#258's heading said three rows of headroom where its body measured seven** — mine, and the
   figure decides the conclusion, since chain J's four owed rows fit under seven and not under
   three. Corrected.

### Nits

Taken by the coordinator: the capitals-for-emphasis this branch added under `llm-wiki/`
(`PANICS`, `WIRE`, `RUN`, and the diagram's `COMMITTED`) — the ones left are in this file, which is
a run record rather than a page read under pressure. Handed to the developer: `na` should count as
off in the registry's ticket rule, where it filters on `disabled` alone and so lets a row of `na`
cells carry no ticket; three comment-cap overruns; rustfmt on two touched files, which nothing in
CI gates; and `plan_goldens.rs:787` saying `dim` is four lanes at tp4 when it is one row group over
four declared lanes, three empty.

## A chain fact worth not rediscovering: a documentation-only push makes CI skip

PR #167's latest run reports every job `skipping`, because the last commits on
`ENS-duckdb-oracle` touch only `llm-wiki/`. `pipeline.yml`'s `changes` job is doing what it should
— building nothing for a wholly-documentation diff — but the effect on a chain branch is that
`gh pr checks` shows no failure and the PR reads as though it passed. It did not run. So a `done`
decision, which is the one transition that asserts CI green, has to look at the last run that
actually built something, not at the newest one. #167's real state is the earlier red: 27 cases
on the missing `gpu-result.txt`.

## Round 2 result (2026-10-08)

The reviewer's one blocking finding, five importants and four nits, each with what was done and
what was measured. No device, same as round 1: shad-gpu stayed down, every run below is plain
`cargo test --features rust-only` into `./target`.

### 1 (blocking) — #227's validator half landed; the device half did not, and why

`nulls_where_none_declared(&ArrowSchema, &[usize])` is now in
`test_support/schema_validation.rs` beside `device_divergence`, and `held_to_declaration` runs
both comparisons and joins their findings. Which half runs is a stated argument rather than an
inference: `NullsHeld::{Unread, PerColumn(&[usize])}`. `cpu_schema_validator` passes
`PerColumn`, read off the arrow batch; `gpu_schema_validator` passes `Unread`.

Proved red twice, in the shape that shipped:
- the rule itself — three cases in `test_support/schema_validation/tests.rs`, written against a
  function that did not exist;
- through the hook — `a_declaration_with_one_field_non_nullable_is_refused_naming_the_nulls`
  (`src/tests/end_to_end/schema_validation.rs`). pbench `bool-key-group` at tp1-single, whose
  project emits the NULL `f_kb` group; the validator's index is built over a copy of the tree
  with that field declared non-nullable, so the lie is told to the validator alone. With the
  `NullsHeld::PerColumn` arm removed the case fails and the other two stay green.

**What is left of #227, and why no card could be begged for it.** The device holds no null
counts anywhere this side can read. `peacock_handle_schema` hands back an Arrow IPC *schema
message* — no counts — and `cudf::to_arrow_schema` carries no usable nullability either, which
is why `device_schema.rs`'s header says nullability is not in `DeviceSchema`. There is no
`peacock_handle_null_counts`: closing the device half needs a new C++ entry point over
`table_for(handle)`'s `column_view::null_count()`, and `peacock-ffi`'s `build.rs` runs cmake
over cudf for any non-`rust-only` build, so that arm cannot even be type-checked here. **#227
does not close with this task.** What closes it is one FFI read plus flipping `Unread` to
`PerColumn` in `gpu_schema_validator` — the comparison is already written and already proved.

The one line of `gpu_schema_validator` that changed is unbuildable locally for the same reason
as every other device line in this branch.

The cpu backend's own `nulls_where_none_declared` was left where it is rather than shared. The
two take different inputs — arrays against counts, because a count is what a device read gives
— and report into different error types; the shared part is the one-line predicate, which is
not worth a `TEST_ONLY_ITEMS` entry point across a component wall.

### 2 (important) — `--check` now reads the regeneration, and goes red when gen.sql drifts

`generate_pbench.sh`'s specials and row-group queries read `$HERE/pbench.sf1/` — the committed
fixture — in both modes, so only the `EXCEPT ALL` loop ever saw `$OUT`. They are now three
functions over a directory argument, run over `$OUT` and the committed file in turn, each
against the same literal; equal-to-the-same-literal is stricter than equal-to-each-other.

Red-green, measured:
- `DICTIONARY_SIZE_LIMIT 0` dropped from gen.sql's `fact` and `dim` COPYs, committed parquet
  untouched. **The committed script at HEAD prints `pbench.sf1 matches gen.sql` and exits 0.**
  The new one exits 1 with
  `error: /tmp/tmp.XXXX/fact.parquet's float specials are '239 164 59 695 239 164 45', not
  '199 204 191 563 199 204 191'` — naming the temp directory, so it is the regeneration that
  failed. The `EXCEPT ALL` loop above it passed in both runs, which is the reviewer's point
  about `-0.0` against `0.0` made twice.
- gen.sql restored, `--check` green, parquet md5s unchanged.

`pipeline.yml:180-182`'s claim ("including the float specials") is now true and needed no edit.

### 3 (important) — every argument validated before the first side effect

`MODE=${1:-write}` is replaced by an argument-count check and a `case` over `""` and `--check`.
`write` is not an accepted literal: a flag whose only effect is the default is what
`coding-style.md:32-38` forbids. Measured — `--chek`, `-c`, `--dry-run` and `--check extra` all
exit 1 with a message and a usage line, and `md5sum -c` over `testdata/pbench.sf1/*.parquet`
after all four shows every file unchanged. Write mode still rewrites byte-identical files.

### 4 (important) — the three orphan sections dropped, and a guard both ways

`every_duckdb_result_section_has_a_registry_row_and_every_row_a_section`, in
`tests/test_cpu_corpus.rs` beside the `gpu-result.txt` guard, over `registry_datasets()`. The
registry's rows and the golden's sections, both directions, the registry's `_` mapped through
`stem`.

Watched red before the drop: `pbench: duckdb-result.txt has sections with no registry row
["interval-through-join", "struct-key-join", "struct-through-join"] and rows with no section []`.
Then `python3 testdata/duckdb_result.py --dataset pbench`, and the regenerated file is the
committed one **minus exactly those three sections and nothing else** — 2,036 lines and 53,044
bytes removed, 136,635 → 134,599 lines, zero added lines in the diff. Green after.

They were not kept for #255: their SQL is in the ticket, and `interval-through-join`'s section
rendered `iv` as Python's `str(timedelta)` (`1 day, 0:00:00`), which is not how arrow-rs prints
an interval, so the section would have had to be rewritten anyway. Measured on the other two
datasets for the guard's cost: tpch 39 sections / 39 rows / 39 `.sql`, tpcds 99 / 99 / 99,
pbench now 59 / 59 / 59.

### 5 (important) — the stolen doc comment, and a second one the review missed

`all_enabled` moved below `mode_cell_html`, with its own two lines. `mode_cell_html`'s block
came back to it and was cut from 15 lines to 9: it had been a merge of two blocks and its counts
contradicted each other and the code — "four execution-mode `<td>`s", "ONE `colspan=4`", "three
mode cells", "Three cells rather than fifteen columns". The code emits three mode cells and
`colspan="3"` (`main.rs:1024`, `:1135`), which is what it now says.

**The same antipattern a second time, in the test module.** `the_pr_comment_fits_under_the_body_cap`
had lost its four-line block to `the_widget_renders_a_pbench_section_beside_the_benchmarks`,
inserted above it with no blank line — so the byte-cap guard, the one test in that file a later
reader goes looking for, documented nothing. The three inserted tests moved below it.

`test_golden_format`'s `no_declaration_carries_a_block_left_behind_by_a_split` cannot see either
of these and says so in its own doc: it covers the SPLIT shape, where a blank line is left
behind. Both of these were contiguous insertions. What would have caught both mechanically is a
doc-block line-cap check (17 lines and 11 lines against the cap of 10) — not built here, because
it goes red on pre-existing blocks across the tree and that is a task of its own.

### Nits

- **`na` counts as off in the registry's ticket rule** (`test_support/registry.rs`). Red-green:
  with `pbench/float64_key_group`'s `tickets` blanked, `the_registry_matches_the_cpu_corpus_in_both_directions`
  passed before and fails after with `cost-registry.csv:152: 10 cells off and no ticket`. Safe
  over the whole CSV — measured: 21 rows carry an `na` cell and all 21 name a ticket; the only
  three ticketless rows (tpch q1, q6, shuffle_additive_avg) are enabled at every cell.
- **Comment caps.** `corpus_cases.inc`'s pbench header 22 lines → 10, and its #243 block 13 → 10
  counting the three commented-out `corpus_query!` lines as part of the run. The detail those
  two shed is in this file. `cpu_backend/mod.rs`'s `declared_as` comment 5 → 4 inside the body,
  `plan_goldens.rs`'s NOT_RUNNABLE comment 5 → 4. Measured after: the only comment runs over 10
  lines left in `corpus_cases.inc` are the file header (13) and the tpcds block at 241 (13),
  both pre-existing.
- **rustfmt.** The two files the review named are clean, and three more were not. `plan_goldens.rs`
  (the 110-character use block), `tests/test_corpus_goldens.rs` (107), `test_support/tests.rs`
  and `cpu_backend/tests/backend.rs` all run through `rustfmt --edition 2024`; every hunk in the
  last two is this task's own code. `cpu_backend/mod.rs` was fixed **by hand** — it is a `mod.rs`,
  so rustfmt follows its `mod` declarations and would have reformatted `expr_physical/tests.rs`,
  `gpu_tests/murmur_conformance.rs`, `source.rs` and `tests/backend.rs` with it. For the same
  reason `wire/expr_writer/tests.rs` was left alone: its seven divergences all sit outside the
  lines this task added. `cost-report/src/main.rs` is not rustfmt-formatted at all and was not
  touched by rustfmt; its new lines match the file's own idiom. Formatting
  `tests/test_corpus_goldens.rs` carried four pre-existing hunks with it, which is the cost of
  the rule as written.
- **`plan_goldens.rs`'s `dim` sentence.** It said `fact` and `dim` are four lanes at tp4. They
  both declare four; `fact`'s ten row groups spread over all four and `dim`'s one fills lane 0
  and leaves three empty (`partition_groups=[[[0]],[],[],[]]`). Reworded; the test is unchanged.

### One thing beyond the list

The two e2e rebuilders — the retype and the nullability tightening — were one copy of 25 lines
each, so they are one `with_a_project_lying_about_its_first_field` taking the edit as a closure,
each call site asserting inside its own closure that the declaration it writes is not the true
one.

### Suite numbers, measured after the last edit

| suite | round 1 | round 2 |
|---|--:|--:|
| `--lib` | 670 + 2 ignored | **674** + 2 ignored |
| `test_cpu_corpus` | 858 pass / 27 fail | **859 pass / 27 fail** |
| `test_corpus_goldens` | 26 | 26 |
| `test_cost_model` | 3 | 3 |
| `test_golden_format` | 43 | 43 |
| `test_module_layout` | 18 | 18 |
| `test_ci_coverage` | 9 | 9 |
| `cargo test -p cost-report` | 39 | 39 |
| `python3 testdata/test_duckdb_result.py` | 20 | 20 |

`--lib` +4: three in `test_support::schema_validation::tests`, one in
`tests::end_to_end::schema_validation`. `test_cpu_corpus` +1: the section-against-registry
guard. `generate_pbench.sh --check` green and `generate_pbench.sh` (write) green, both after the
red proofs above.

**The red count is still exactly 27 and still the same 27 by name** — diffed against the list
taken before this round's edits, identical. 26 `duckdb_gpu_<ds>_<q>_<mode>` (22 tpch, 4 tpcds)
and `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`; all 27 carry `does not
exist, so no device answer is recorded`, grepped, 27 occurrences. No 28th.

`cargo test --features rust-only -p peacockdb-core --no-run` emits zero warnings. `cost-report`
still has its one pre-existing `sha_links is never used`. No workflow was edited this round.
`scripts/exec_model/tests` was not re-run: nothing this round touches `scripts/`.

### What the next person needs, on top of round 1's list

- **#227 is half closed.** The signoff should say so. The rule and its two red-green cases are
  in the tree; the device read is not, and it is a C++ entry point plus one word.
- **The duckdb-result guard means a query file and a registry row now have to land together.**
  `int8-key-group` in Task 8 and #255's three queries each need their `.sql`, their corpus line,
  their registry row and a `duckdb_result.py` re-run in one change — a `.sql` alone now fails
  `every_duckdb_result_section_has_a_registry_row_and_every_row_a_section` rather than passing
  silently.
- **`--check` is now a real guard over gen.sql**, so a change to the generator's COPY options
  will go red on the regeneration's specials. The expected counts are literals at the bottom of
  the script; a deliberate data change edits them, and should say why in the same commit.

### The board's "closes #227" is now "closes half of"

Round 2 landed the validator-side null-count check and proved it red two ways, and then established
that the other half cannot land on this workstation or in this task: the device holds no null count
for anything to read — `peacock_handle_schema` returns an IPC *schema* message and cuDF stores no
nullability — so the remaining work is a new C++ entry point over `table_for(handle)`'s
`column_view::null_count()`, and `peacockdb-ffi`'s `build.rs` runs cmake over cuDF for any build
that is not `rust-only`, so that arm cannot even be type-checked here. The comparison is already
shaped for it: `NullsHeld::{Unread, PerColumn(&[usize])}` is an explicit argument, and the device
flavour passes `Unread` today.

So the board heading says "closes half of #227" rather than "closes #227", and the signoff names
the remaining half. #227's own text needs no change — it already reads "#227 closes when both
checks land", which is exactly right.

## Completeness pass — the analyst's reading (2026-10-08)

A fresh analyst over `8431f317`, asking what is missing. Two blocking, three important, and a
short `architecture.md` list. All of it is applied or filed; none of it reopened the task.

1. **Blocking: the device half had no owner, and its recipe lived only in files the merge
   deletes.** The spec's work item 5 and its device bar are carried by `pbench-impl.md`'s Task 8
   and this file's "What the next person needs", and the archive rule deletes both. Three things
   were therefore about to vanish: `int8-key-group`, which has no query, no registry row, no corpus
   line and no ticket and would simply never land; 16 tp1 gpu cells that `repartition-keys` does
   not claim (its text says the **tp4** cells turn on), recreating behind us the exact condition
   `stale-cells` exists to clear; and the cycle itself. **Filed as #259**, which also names the
   single card visit that settles it alongside #235's last step and `stale-cells`' 16 cells.
2. **Blocking: #227 would have read as satisfied.** Its text asks only for the cpu half, which
   shipped — and the claim that "#227's own text needs no change" was wrong, because the sentence
   relied on ("#227 closes when both checks land") is in `pbench.md`, which the board retires, not
   in the ticket. #227 now carries the device read, why cuDF has no count to give it, and the
   `NullsHeld` shape already waiting for it.
3. **Important: three more falsified ticket sentences**, all on later tasks' paths, which the
   round that fixed nine of them missed: #240 said no corpus query exists when five pbench rows
   carry it and it is `repartition-keys`' own ticket; #95's "eight hash a decimal key" is eleven;
   #63 named `tpcds/q9` alone. Applied. The analyst also found **#208 had no corpus-queries line at
   all** and no reference to #256, so a `join-backend` developer would look for its evidence and
   find nothing — #208 now says its pbench query is refused earlier, on #256.
4. **Important: the determinism check sits in no Rust tier.** `generate_pbench.sh --check` is
   invoked from one line of `pipeline.yml` in dataset-matrix; no Rust target runs it, it is in none
   of `build-test.md`'s case tables, and `test_ci_coverage` guards Rust targets only. Drop that
   line and the only guard that the committed parquet is what `gen.sql` makes disappears silently.
   Named in the signoff rather than fixed: wiring it to a Rust target would mean a Rust test
   shelling out to duckdb, which is a bigger decision than this task should take.
5. **Nothing in the tree asserts a count the four absent queries falsify** — checked: the Python
   count is a floor for pbench, `PBENCH_JOINS` is a hand list, and `build-test.md`'s figures
   reconcile exactly. One guard asserts the *absence*, which is deliberate and good, and #255 now
   names it as the case to delete when it closes.

**`architecture.md`:** two sentences, both applied. `:519` said "in the corpus every cross join
pairs a one-row aggregate result with another", which pbench's `cross-projection` — a 20,000 × 8
cartesian of two base tables — falsifies; and `:1278` said DuckDB's cost oracle "runs each query
twice", which is false for 59 of 176 lines, pbench having no `qN.duckdb_cost.txt` at all. The
capability matrix needed nothing: the analyst diffed pbench's plan goldens against tpch's and
tpcds' and found no node kind, no join type and no refusal reason those two did not already carry.

**What chain J will trip over**, the part only this reading produces:

- **`repartition-keys` has a step it cannot execute.** Its Scope requires adding
  `struct-key-join`, `struct-through-join` and `interval-through-join` to `NOT_RUNNABLE`, and
  `every_query_that_cannot_cross_the_wire_is_declared_and_every_declaration_is_true` is
  bidirectional — a declaration with no `not runnable:` line in all five goldens goes red. Those
  three queries cannot exist until #255 lands. That task's spec is frozen, so this goes in its
  detail file at dispatch time. (Its own text says "two" in one place and "three" in another; that
  predates this branch.)
- **`repartition-keys` owns the first over-cap fingerprint on pbench**, `uint-key-group` over a
  19,848-row answer, and turns on both its cpu tp4 cells and its gpu cells — all-or-nothing under
  #253.
- **#253 got worse and sooner.** Fingerprint lines went 4 → 14, ten of them pbench rows whose
  device comparison is `live_cpu`; and the device-diverges-while-the-cpu-matches half now has 27
  pbench rows in reach, every one already DuckDB-green on the cpu, in a dataset built to make the
  device differ. #253 updated: the decision is owed before `repartition-keys`, not before
  `stale-cells`.
- **What `join-backend` already owns is fine** — `collapse-not-in`, the `PBENCH_JOINS` hand-off,
  `empty-side-left-join`, `indf-full-join` and the `sparse-build-*` lanes are all in its plan, and
  #257 records the trap that `in-is-null` starts agreeing with DataFusion's wrong answer the moment
  that task's `EmptyExec` arm lands. pbench does reach shad-gpu: all three ship paths carry it.
