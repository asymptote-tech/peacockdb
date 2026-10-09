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

## Completeness pass — the reviewer's reading (2026-10-08)

**0 blocking, 3 important**, all of them text rather than code, and all applied. The reviewer
confirmed the branch's substance by simulation: all 24 `PBENCH_JOINS` entries and the
three-kinds collapse test reproduced against `tp4-single.plans.txt` with zero failures; the
section-against-registry guard simulated over all three datasets both ways (59/59, 99/99, 39/39);
`generate_pbench.sh`'s argument rejection **run** (`--chek`, `--check extra`, both exit 1 with the
parquet untouched) and its hardcoded specials counts re-derived arithmetically from `gen.sql`'s
moduli; the two engine changes shown to change no answer; and every number in `build-test.md`
reconciled, 674 cells decomposing as tpch/tpcds's 551 plus pbench's 23×5 + 4×2. It also found no
28th red, by simulating five registry-and-golden guards rather than running them. Two findings it
had confirmed were closed by commit `c83de6be` while it was reading, which is the second time this
has happened — commit the wiki work before dispatching a completeness reviewer.

1. **#189 promised 24 cells its own proposed fix cannot turn on** — and the count was mine, added
   two commits earlier. `uint-key-group` and `uint-key-join` reach #189's refusal by the other
   road: they hash a plain `UInt32` user key, and comet's murmur3 has no unsigned arm at any
   width. The grouping-id fix cannot reach them — `uint-key-group` is a single grouping set so
   `!group.is_single()` excludes it, and `uint-key-join` never enters `aggregate_sequence`. So a
   task implementing #189 from that paragraph would enable 24 cells and turn 6 red, which is the
   "a later task will believe it" failure routed through a ticket instead of a query. Corrected:
   18 from the grouping-id fix, 6 wanting the unsigned arm, which is the second half of #189's own
   mechanism sentence and no part of its fix.
2. **The reason given for #227's open half was false.** It said the device has no count for
   anything to read and then named the accessor that returns it —
   `column_view::null_count()` is a stored cuDF member this repo's own C++ already calls in
   `expr.cpp` and `aggregate.cpp`. What is true is that nothing *exports* it:
   `peacock_handle_schema` carries the schema message alone. And a C++ entry point is not the only
   route — `peacock_result_from_handle` exists and the gpu hook already holds both its arguments,
   so this is a cheap-against-correct trade rather than a missing capability. Corrected in #227,
   in `build-test.md`, and in `schema_validation.rs`'s comment, which carried the same clause.
3. **The branch adds the corpus's first planner refusals that name no ticket**, and
   `every_refusal_names_a_ticket_that_exists` opens with a sentence wider than what it asserts —
   its check is `!cited.is_empty() || !line.starts_with("not runnable")`, so a `refused:` line
   citing nothing passes. Before this branch the only uncited refusals were
   `refused by datafusion:` lines; `cross-empty-build`, `empty-side-left-join` and `in-is-null`
   are the first from our own planner, in all five mode goldens. Their tickets are on the registry
   rows, so the consequence is a reader landing on `plan node EmptyExec` with nowhere to go.
   Citing them would be a third engine change the Restriction does not permit, so the doc sentence
   is narrowed to what the test enforces and names the exemption; the citations belong to the task
   that lifts #155.

Nits it dropped, listed because they are real but not worth a round trip: #257's length, some
capitals in `llm-wiki/`, a second hardcoded dataset list in `collect_cost_goldens` beside
`all_datasets`' "one list" comment, `exec_model/tests/corpus.py`'s inert pbench entry, and two
counts on #220 and #80 that were already stale on the base.

## Rebased onto the new ENS-duckdb-oracle, and dropped to building (2026-10-08)

The second half of the `rebase` the control file ordered. `ENS-duckdb-oracle` moved first (onto
master's `dbf44bcc`, then its device cycle), so this branch's twelve commits were replayed with
`git rebase --onto ENS-duckdb-oracle 410111cf ENS-pbench`. The plain `git rebase ENS-duckdb-oracle`
is wrong here and was aborted after it tried to replay the parent's own rewritten commits — it
reported conflicts in `test_gpu_corpus.rs`, `duckdb_result.py` and three goldens, none of which
were real. Anyone rebasing the next branch wants `--onto` and the old parent tip.

**Every conflict was in the wiki or the board; none in code.** Resolved by union rather than by
side, since both sides were adding:

- `tasks.md` — the state lines, resolved per the note above by the override and not by the
  ownership rule, which is what would have dropped the `rebase needed(building)` marks.
- `build-test.md` — the DuckDB tier count takes this branch's 205 over the parent's 149, and the
  parent's archived-#235 link over this branch's live one. The golden-file section needed both
  halves: `tpch.sf1` 40 and `tpcds.sf1` 117 from the parent's device cycle, `pbench.sf1` 17 and its
  explanation from here. The closing clause was rewritten rather than taken from either side —
  `duckdb-result.txt` is in all three sf1 dirs and `gpu-result.txt` only in the two whose device
  cells are on, which is true of neither side alone.
- `tickets.md` twice, and `tickets/testinfra.md` — next-free is 264, which counts master's #262;
  both #263 and #258 are kept; #235 stays archived while #259 and #255 come across. Re-audited
  mechanically afterwards rather than by eye: every row's declared count equals both its listed
  IDs and its file's `<a id="tNN">` anchors, and the total 122 is their sum.
- One dead link the replay introduced: `df-upgrade.md:177` still pointed `#235` at
  `corpus-coverage.md`, where it no longer lives. Repointed at the archive.

**State is `building`, per the override, and it needs re-proving.** This rebase was not
documentation-only — it carried the parent's two new `gpu-result.txt` goldens and master's
retirement of the docker build, including the edits to `scripts/build-test-shadgpu.sh` and
`scripts/lib/shadgpu-env.sh` that this task's own commands run through. So the state does not come
back on its own: a developer re-runs the proving commands on nebius-gpu first.

Worth knowing for whoever does: a later rebase of this branch onto master's `8806a3c3` will carry
documentation alone, so it will re-verify nothing and this task will keep whatever state it then
holds. The work below is not wasted by the rebase still owed to the human.

## Re-proved after the rebase, on nebius-gpu (2026-10-08)

The re-proof the section above said the state needed. Everything was re-measured rather than
carried over, and the device rung found a real failure that only a device build can see. Local
CPU runs are plain `cargo test --features rust-only -p peacockdb-core` into `./target`,
`--test-threads=2`; the device half is nebius-gpu's L40S under the host override.

**The rebase itself broke nothing.** What it did was make the branch's own defect visible: the
parent's two committed `gpu-result.txt` goldens turned the 27 carried-red cases green, and the
parent's nebius recipe made the device rung runnable for the first time on this branch.

### The 27 red cases are gone, and they were the only thing the rebase was owed

`test_cpu_corpus` was **859 passed / 27 failed** at round 2 and is **886 passed / 0 failed** now.
The 27 were the 26 `duckdb_gpu_<ds>_<q>_<mode>` cases and
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, all waiting on a file no
device had written. `ENS-duckdb-oracle`'s device cycle wrote it, and the rebase carried it. 886 is
what `build-test.md` states, so nothing drifted on that count.

### A real regression, found by the device rung and fixed here

`cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::` came back **534 passed, 2 failed**
where the parent measured 536. Both failures in `src/tests/gpu_tests/nested_cases.rs`:

- `bug_an_inner_nested_loop_join_projection_is_dropped_on_the_cpu`
- `bug_an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_is_dropped_on_the_cpu`

each panicking at `nested_cases.rs:209` — the `assert!` in `cpu_refuses_with` — with
`p_id holds 7 NULL(s) where the node declares it non-nullable (#227)` instead of the
`number of columns(16) must match number of fields(2) in schema` they pin.

**Root cause: this task's own #227 check, and the order it runs in.**
`nulls_where_none_declared` (`cpu_backend/mod.rs`) zips the batch's columns with the
declaration's fields *positionally*, and `declared_as` calls it ahead of everything — which it
must, since the spec requires the check to precede the equal-schema early return. But these two
cases are the shape where the CPU **drops the join's projection** and answers with all 16 crossed
columns against a 2- or 3-field declaration. At a differing count column *i* is not field *i*'s
column, so the rule read a NULL off column 1 and reported it against field 1's name, `p_id` —
whose own column is clean. A refusal that is precise, wrong, and standing in front of the fault
that explains it. The two `bug_` cases pin the dropped projection (#207/#190), so the wrong
message also hid what they exist to watch.

Its own doc comment already stated the intended rule — "a declaration shorter than the batch is a
different fault that `try_new` reports" — and the code order defeated it.

**Fixed where the rule is, not where it fired**: `nulls_where_none_declared` returns `None` when
the counts disagree, so `RecordBatch::try_new` reports the width, which is the one thing that can
name it. The #227 requirement is untouched — at an equal count, which is every case the check
exists for, nothing changed, and the early-return hole it was written to close stays closed.

Red-green, in the rust-only tier where the fault is reproducible without a card:
`a_batch_with_more_columns_than_the_declaration_is_refused_for_the_count`
(`cpu_backend/tests/backend.rs`) — one declared non-nullable `p_id` against two columns, the
batch's own `p_id` clean and the column at its ordinal holding a NULL. Watched fail with the
device rung's exact message, `p_id holds 1 NULL(s) … (#227)`, then pass. The two `bug_` cases are
**unchanged**: they pin what they always pinned, which is the point.

**No ticket.** The defect and its fix land in the same change, so nothing is left behaving
wrongly for anyone to track.

**The validator half does not have this fault**, checked rather than assumed.
`test_support/schema_validation.rs`'s `held_to_declaration` runs `device_divergence` *beside* the
null rule and joins both findings, and `device_divergence` leads with `N columns declared, M held`
— so the width is never hidden there, and its message carries the ordinal. Left alone.

### The bar, measured

Local, `--features rust-only -p peacockdb-core`, after the fix:

| target | measured | `build-test.md` |
|---|--:|--:|
| `test_cpu_corpus` | **886 passed, 0 failed** | 886 |
| `--lib` | **675 passed, 2 ignored** (677 listed) | 677 after this change, 676 before |
| `test_corpus_goldens` | 26 | 26 |
| `test_cost_model` | 3 | 3 |
| `test_golden_format` | 43 | 43 |
| `test_module_layout` | 18 | 18 |
| `test_ci_coverage` | 9 | 9 |
| `cargo test -p cost-report` | 39 (incl. `the_widget_renders_a_pbench_section_beside_the_benchmarks`) | 39 |
| `python3 testdata/test_duckdb_result.py` | Ran 20, OK | 20 |
| `python3 testdata/test_duckdb_cost.py` | Ran 41, OK | 41 |
| `testdata/generate_pbench.sh --check` | `pbench.sf1 matches gen.sql`, exit 0 | — |

`cargo test --features rust-only -p peacockdb-core --no-run`: **zero warnings**.

Device, nebius-gpu's L40S (card idle, 37 GB free), `./scripts/build-test-shadgpu.sh --build`
exit 0 and **0 warnings**, each binary run directly with
`LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib`,
`PEACOCK_TESTDATA_DIR=$PWD/testdata` and `--test-threads=1`:

| binary | measured | `build-test.md` |
|---|--:|--:|
| `cpp/install/bin/peacock_gpu_tests` | 4 passed | 4 |
| `cpp/install/bin/peacock_plan_tests` | 56 passed | 56 |
| `cpp/install/rust-tests/test_gpu_corpus` | 28 passed | 28 |
| `cpp/install/rust-tests/test_node_timing` | 1 passed | 1 |
| `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::` | **536 passed** | 536 |
| `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered | 11 |
| `cpp/install/bin/peacock_cpu_tests` (bonus; needs no card) | 15 passed | 15 |

0 failed everywhere. 536 + 28 + 11 + 1 = 576, the gpu block exactly. The build ran through
`scripts/build-test-shadgpu.sh` and `scripts/lib/shadgpu-env.sh` as master's docker retirement
left them, and through a tree where `scripts/docker-build.sh` and `docker/` are gone — so that
half of the rebase is exercised by this run and nothing misbehaved.

**No golden moved.** `git status --short` is the two code files and nothing else;
`testdata/goldens/{tpch,tpcds}.sf1/gpu-result.txt` are `sha256`-identical on both hosts; no
`gpu-result-<v>.txt` was written; and no `pbench.sf1/gpu-result.txt` exists, which is correct.

### The duckdb-oracle comparison over pbench

56 `duckdb_pbench_*` cases, one per corpus line, all passing inside the 886. By oracle, counted
off `corpus_cases.inc`'s own lines: **16 `duckdb_exact`**, 1 `duckdb_approx`, 10
`duckdb_fingerprint`, 29 `duckdb_none`, 0 `duckdb_divergent`. So 27 pbench rows actually compare
rows against DuckDB and all 27 pass; the 29 `duckdb_none` rows decline because the cpu does not
answer them. Whole corpus: 109 / 16 / 14 / 33 / 4 over 176 lines, exactly the distribution
`build-test.md` states.

### The three-guard check, against the code as it stands

duckdb-oracle's note to a pbench developer is **correct on every point**, verified in source and
then in the running binary:

- **Both guards derive their datasets from `cost-registry.csv`.** `registry_datasets()`
  (`test_cpu_corpus.rs:499`) is `load_csv()` mapped to `(dataset, sf)`, and the coverage guard,
  the cuDF-stamp guard and the `duckdb-result.txt` section guard all iterate it. A new dataset
  needs no edit in `test_cpu_corpus.rs`, as its doc comment claims.
- **Landing with every `gpu_*` cell off is fine, and no `gpu-result.txt` is wanted.** Measured
  off the CSV: pbench's 59 rows hold **280 `disabled` and 15 `na` gpu cells and not one
  `enabled` or `skip`** (123 cpu cells on). `gpu_result_coverage` returns `Ok(())` for
  `text: None` when `enabled.is_empty()`, and the stamp guard `continue`s on an unreadable path.
  Both pass today over a dataset with no file.
- **`duckdb_result.py`'s two dataset lists are this task's and both carry pbench** — `choices`
  and the default list. **Their line numbers in the note are stale**: they are `:274` and `:282`
  now, not `:230` and `:238`; the file grew. And **the empty-glob no-op is gone** rather than
  merely handled: `query_files` exits with
  `duckdb_result.py: <dir> holds no .sql files`, and `--only` matching nothing exits saying
  writing from an empty list would delete every section the golden holds.

### The coordinator's four hand-merged counts: all four right

Checked against source, not against each other.

- **The DuckDB tier at 205.** Correct. `test_cpu_corpus --list` is 886 cases: 674 `cpu_*`, 202
  `duckdb_*` and 10 named checks. The DuckDB tier is those 202 (176 `duckdb_<ds>_<q>` + 26
  `duckdb_gpu_*`) plus the three the page names — every oracle used, the coverage guard, the cuDF
  stamp — so **205**. The cpu tier is the other 674 + 6 checks = 680, and 680 + 205 = 885, the
  `Corpus, cpu` row, + 1 for Registry ↔ CSV = 886.
- **`tpch.sf1` 40, `tpcds.sf1` 117, `pbench.sf1` 17.** All three correct by `ls`. pbench's 17 are
  exactly the five `.plans.txt`, five `-mini.cpu.txt`, five `-mini.cost.txt`, `mini.result.txt`
  and `duckdb-result.txt`, as the page says.
- **The closing clause.** Correct: `duckdb-result.txt` exists and is tracked in all three sf1
  dirs; `gpu-result.txt` exists and is tracked in `tpch.sf1` and `tpcds.sf1` alone — the two with
  enabled device cells (22 tpch, 4 tpcds) — and in neither case is there a stray file.
- **The grand total, the subtotals and `test_cpu_corpus`'s own number.** All were internally
  consistent as merged — 1591 / 7 / 576 each equalled the sum of its block's rows, Rust 2283 =
  2174 + 109, C++ 97, Python 401, grand 2781 — and `test_cpu_corpus` 886 and `--lib` 676 both
  matched measurement.

### The one `build-test.md` edit this run owed, and it is not the coordinator's

This change adds one case, so the page moved by one: `--lib` 676 → **677**, the cpu block
1591 → **1592**, `CPU backend executors` 70 → **71**, Rust 2283 → **2284**, grand 2781 → **2782**.
Re-summed mechanically afterwards: every block header equals its rows and every total its parts.
The `CPU backend executors` row's prose also named none of `declared_as`'s declaration checks —
pbench added two of them in round 2 and the prose never grew — so it now names all three.

### Deferred by the override, not done

- **the sf40 pair**, `peacock_tpch_tests` and `peacock_tpchv_tests`: out by the override, and
  unrunnable here anyway — `testdata/tpch.sf40` does not exist on nebius-gpu and
  `peacock_tpch_tests` reserves 69 GiB against the L40S's 46 GB.
- **`--run-benchmarks`** and the three `bench_` cases inside `peacock_gpu_benchmarks`.
- **Nsight captures** (`create_nsys_profile.sh`) and any H200 timing.
- **The spec's "one shad-gpu cycle"** over the pbench rows, replaced by the override with a build
  and the device binaries. pbench's gpu cells are all off, so there was no recording cycle and no
  `gpu-result.txt` to write. [#259](../archive/archived-tickets.md#t259) still owns turning them on
  and is unchanged by this run.
- **`scripts/exec_model/tests`** not re-run: nothing here touches `scripts/`.
- **A local `ctest -L cpu`**: there is no local C++ build dir, and nothing in this change is C++.
  The same 15 cases ran from the host's freshly built `peacock_cpu_tests` instead, green.

### Housekeeping

No disk cleanup was needed on either host: nebius-gpu had 37 GB free throughout and the local
workspace 66 GB. Nothing was removed anywhere.

`rustfmt --edition 2024 --check` is clean on `cpu_backend/tests/backend.rs`. `cpu_backend/mod.rs`
was edited by hand for the reason round 2 recorded — it is a `mod.rs`, so rustfmt would follow its
`mod` declarations into four unrelated files.

## Review round after the rebase (2026-10-08)

Scoped to the two new commits. **0 blocking, 1 important, 3 nits.** The reviewer re-derived rather
than read: it parsed `corpus_cases.inc` to get `test_cpu_corpus`'s 886 from its parts, re-summed
every tier of `build-test.md`, counted the goldens per directory with `git ls-files`, and checked
all 11 ticket-index rows against their files' anchors. All of it held.

**Important, fixed here: a second dead `#235` link.** `corpus-coverage.md:824`, inside #259's own
text, still pointed at `#t235` in a file the parent branch had archived it out of. The rebase note
claimed one such link and there were two; the reviewer found it by diffing the dead-link set across
the old tip, the new parent and HEAD, which is the check I should have run rather than grepping for
the one path I happened to think of. Repointed at the archive, where the other six live references
go.

**The bug fix stands.** The reviewer reproduced the root cause from source rather than taking the
account here: with `inner`, `joined(false)` keeps `p_id` non-nullable, so declared field 1 is `p_id`
while batch column 1 is `b_key`, which `synthetic` nulls every eleventh row — which is also why two
of the four CPU-refusal cases failed and not four, since `left` makes every `p_*` field nullable and
no violation fires. It confirmed the call order is untouched, so the fix is a guard inside the rule
and not the reorder the spec forbids; that the new case goes red for the stated reason and not
incidentally; and that the fewer-columns direction travels the same comparison and is also better
served by the count. Nothing is let through: at equal widths arrow's own `try_new` still refuses a
NULL in a non-nullable column, so declining at a mismatch only changes which message a reader gets.

### The nits

- **The width hazard survives one line above the fix**, in the decimal relabelling loop, which zips
  positionally with no count guard. A mispaired `Decimal128` column that satisfies `widened_decimal`
  and overflows would name the wrong field, in front of the count — the fault just fixed. Nothing
  worse is reachable: `try_new`'s count check is unconditionally first, and no test today has a
  decimal column in that shape. Left as code, since hoisting the check is logic and this round is
  not the place for it; what is fixed here is the doc comment, which had invited the reader to
  assume the relabelling was guarded the same way. It now says plainly that it is not.
- **#259 was missing from its own file's Contents.** Pre-existing, from when the ticket was
  created; added, first in the Testing section, where its anchor sits. The index listing it last
  while the file holds it 27th is left alone.
- **One corpus case is counted in two rows of the cpu block** — `CPU backend executors` covers the
  whole subtree including `contract.rs`, which also has its own row. Pre-existing and preserved by
  the bump rather than introduced by it, and two independent measurements of `--lib` still agree
  with the page, so something else nets it out. On the record, not acted on.

Not verifiable without a card, and said so: every measured pass count above. The arithmetic between
them is self-consistent and the device list matches the host override item for item.

## Completeness pass, second time round (2026-10-08) — and it reopens the task

Two readings at `9195a39f`, dispatched together, neither seeing the other's list. The reviewer
found the branch sound: **0 blocking, 1 important**. The analyst found **1 blocking** and five
important. The blocking one is right, and it means this task is not finished.

### Blocking — the device cycle was never run, and the override does not defer it

Work item 5, the Registry rules and the Verification bar all ask for one device cycle over the
pbench rows. The entry above files it under "Deferred by the override, not done", and that is
**circular**: the cells are off *because* the cycle is what turns them on. The override defers a
closed list — the sf40 binaries, `--run-benchmarks`, Nsight, H200 timing — and a `test_gpu_corpus`
pass over 580 KB of sf1 parquet on an idle L40S is none of them. The override's own last line says
the opposite: "The GPU halves those tasks deferred for want of a device are now runnable", which is
why its resume list reset this task from `completeness approved` to `building`. I read that reset
as a rebase re-verification and it was not; it was this.

The re-prove did run `test_gpu_corpus`, and it executed 28 cases, none of them pbench's —
`test_gpu_corpus.rs`'s macro expands a `none` gpu-modes line to nothing at all, so a filtered run
over off cells is vacuous rather than empty. What is missing, measured:

- `testdata/goldens/pbench.sf1/gpu-result.txt` does not exist. Work item 4 lists it as a required
  golden; 17 landed and it is the 18th.
- **`int8-key-group`** — one of the spec's 63 queries — has no `.sql`, no registry row and no corpus
  line. It is held back precisely because it is the one row the table expects to pass, so it cannot
  land with an off cell and no ticket. It arrives with the cycle.
- 295 pbench gpu cells (280 `disabled`, 15 `na`) carry tickets that are **predictions read off a
  plan golden, not measurements** — `corpus_cases.inc`'s own section comment says so. The Registry's
  "a cell that fails on a ticket the table does not expect is a finding" rule cannot fire against a
  cell nobody ran.
- About 16 tp1 gpu cells the spec expects to pass are off, plus `int8-key-group`'s five.

**#259 is now a ticket re-deferring work the override made runnable.** Its recipe is right — a
`gpu_pbench_` filter with `PCK_WRITE_GPU_RESULT=1` — but its premise, "shad-gpu was off the network
for the whole task", was superseded the same day and nothing revisited it once the card appeared.
`corpus_cases.inc`'s committed comment still reads "shad-gpu was down" as the live reason and points
at "pbench.md's Task 8", a section that exists only in `-impl.md`, which the archive deletes. Both
want correcting by whoever runs the cycle. One budget note for them, which #259 does not mention:
the PR comment sits at 63,038 bytes against a 65,536 cap, about seven rows of headroom, and
`int8-key-group` plus #255's three queries spend four of them.

### The important findings, and what became of each

Applied here, all four being mine:

1. **#253's new "Decided" block stated a grammar four live corpus lines contradict** (the reviewer's
   one finding). It said the side is a third component; positions are variadic — `divergent` maps all
   of `args[1..]` through `number`, and `corpus_cases.inc:178` writes thirteen components. A literal
   third-component reading turns `21` into a side and reddens a test the block claims it does not
   touch. Rewritten as a trailing non-numeric component, defaulting to `both` when the last
   component parses as a number or is absent, which is implementable without touching any existing
   line — the additivity the paragraph claims.
2. **`architecture.md`'s one falsified sentence.** "the planner always produces a plan" is no longer
   true: `scan_mapping::partition` refuses an empty survivor list, and `empty.parquet` has zero row
   groups, so all five pbench plan goldens now carry that refusal for `cross-empty-build`. No tpch or
   tpcds query reached it, which is why the sentence survived. Corrected to name the refusal and
   #256. The analyst checked the rest of the page and found nothing else falsified.
3. **#173's "Corpus queries: `pbench/finish-without-probe`" was false.** DataFusion plans it
   `CollectLeft` and #140 merges both sides, so it is `lanes=1` at all five modes and one probe lane
   over `tiny`'s 8 rows always accumulates keys — `finish_without_keys` is unreachable. Corrected to
   "none", with the reason, which is the standard the branch already applied to #250 and #208.
4. My own doc comment from the last round ran to 11 lines against the ten-line cap. Trimmed.

Left for the developer who runs the cycle, because they are code or data:

5. **The three `sparse-build-*` rows carry `212` as their only ticket and cannot prove it.** Their
   build side reaches the join through a scatter, and `driver/partitioned.rs:430` keeps a zero-row
   scatter output when the join owes its probe side, so `set_build` runs and `without_build` — which
   is #212 — is never called. The keep/drop decision is in the **shared driver**, so the device will
   behave identically; turning those cells on proves nothing about #212, and a red there will be
   #152, which those rows do not carry. `architecture.md:643-647` already states the rule and needs
   no edit — the tags do.
6. **#137's skew is in the data and recorded nowhere.** No pbench registry row carries `137` and
   joins.md's #137 names no pbench query, while the spec's "not covered by a query" paragraph lists
   five tickets and not this one. Ticket and spec disagree and neither points at the data.
7. **`dim` is missing `d_ts_ms` and `d_ts_ns`**, which the spec's data table says it carries. Nothing
   is red, but the parquet is committed and all 17 goldens derive from it, so a millisecond or
   nanosecond join key cannot be written without regenerating and moving every golden. Cheap now,
   golden churn later, and it is the device half of #240's own subject.
8. **The determinism check sits in no Rust tier**, against a bar that puts it in `rust-only`.
   `generate_pbench.sh --check` has exactly one caller in the tree, `pipeline.yml:183`. Delete that
   line and the only guard that the committed parquet is what `gen.sql` makes vanishes silently.
   Named as a shortcut in the signoff with a defensible reason, but nobody deferred it.

### Also owed, and not mine

**The spec's signoff predates the rebase and the re-prove.** It opens "Solved on the cpu, untouched
on the device", and since it was written the branch was re-proved on nebius-gpu and fixed a real
regression in this task's own #227 check. The write discipline gives the spec one later write and it
is spent, so correcting it is the human's call — but `-impl.md` and `-detail.md` are both deleted at
merge, so as it stands that fix becomes invisible. It wants rewriting when the device cycle lands,
which is the natural moment.

### What both readings confirmed

The bug fix is answer-neutral and correctly pinned: at a differing count the batch is refused either
way and only the message changes, at equal counts nothing changed, and the fire condition is still
reachable and still pinned by three cases. The validator half genuinely does not have the fault.
`build-test.md` has **no drift** — both agents re-derived the corpus distribution from
`corpus_cases.inc` (176 lines, 674 cpu cells, 109/16/14/33/4) and every tier sum and golden count
independently, and all of it reconciles. Dead links: the class is closed, 121 on the parent and 121
at HEAD, identical sets, all pre-existing in archive and report files. The rebase carried pbench's
work intact — diffed outside `llm-wiki`, the only pbench-authored difference against the pre-rebase
tip is the two files of the fix.

CI on `9195a39f` is green on both cuDF legs, the GPU build, the cost report and the S3 check, with
only `GPU Tests (remote)` red on the shad-gpu outage the override exempts.

### State

Back to **`building`**, with the device cycle as the work. Not dispatched by this run, which is out
of window rather than out of options — the next coordinator should dispatch it straight from the
list above. Nothing here is waiting on the human except the signoff rewrite, which can ride along
with the cycle.

## The device cycle, dispatched (2026-10-08)

A fresh coordinator, taking the task the completeness pass reopened. Nothing was re-derived: the
work list is the section above, item for item.

**Probed before the dispatch.** nebius-gpu answers, L40S, **card idle (0 MiB of 46068 used)**, 37 GB
free on `/`. verda could not be probed at all — `scripts/list_verda_instances.sh` exits
`VERDA_CLIENT_ID is not set`, and `verda` resolves nowhere, so there is no address to try; CPU runs
are local, which is what the last two rounds did anyway. Branch `ENS-pbench` at `12d44d81`, clean
tree, PR #168 **MERGEABLE** against `ENS-duckdb-oracle`.

**The order is not free, and it is the one thing this dispatch decides.** Finding 7 changes the
committed parquet, and every one of the 17 goldens derives from it — including the
`gpu-result.txt` the cycle writes. So the data lands first, the goldens are regenerated on it, and
only then does the device cycle run. Running the cycle first means running it twice.

**Why a cell cannot just be "run".** `test_gpu_corpus.rs:27` expands a `none` gpu-modes line to
nothing at all, so there is no filter and no variable that reaches an off cell — the declaration is
the only switch. A cycle therefore edits `corpus_cases.inc` to declare the cells it intends to
measure, runs them, and then settles each line on what it measured. That is also why the last
re-prove's `test_gpu_corpus` ran 28 cases and none of them pbench's.

**The budget the cycle spends.** The cost-report PR comment sits at 63,038 bytes against
`COMMENT_MAX_BYTES` 65,536 — about seven rows of headroom, and `int8-key-group` plus #255's three
queries spend four of them.

Dispatched to one developer with: finding 7 first (the two absent `dim` columns, its own commit),
then the cycle, then findings 5, 6 and 8, which touch neither the data nor the device.

## The device cycle, run (2026-10-08)

The work list was the three sections above, item for item. Everything here is measured; nothing is
read off a plan golden.

**Where it ran.** nebius-gpu, `dmitry@89.169.109.150`, one L40S, card idle, cuDF 25.02 from
`~/data/miniforge3/envs/rapids-cuda-12.2`. Built with `./scripts/build-test-shadgpu.sh --build`
after an uncommitted `rsync -a --delete-after --exclude=.git --filter=':- .gitignore'`, three
times (once per tree state), each staged binary run directly — never `--run`, never
`--pull-results`, both of which ssh to shad-gpu. `gpu-result.txt` came home by plain `scp`.
verda could not be probed: `scripts/list_verda_instances.sh` exits `VERDA_CLIENT_ID is not set`
and the name resolves nowhere, so every CPU run was local. No disk was cleaned; `/` went from
37 GB free to 36 GB.

### The order, and why there were two device runs

Finding 7 first, in its own commit's worth of changes: `dim` gained `d_ts_ms` and `d_ts_ns`, the
parquet was regenerated and every golden that moved was regenerated on it. Only then the cycle,
because `gpu-result.txt` derives from the same parquet and running the cycle first means running
it twice.

The cycle itself is two runs, and that is not avoidable either. Run 1 declares every runnable cell
and measures it; the recorder writes a section *before* the comparisons assert, so the file then
holds a section for every cell that reached an answer — 48 of them, 30 that passed and 18 that
failed only on the cpu-shape comparison. Run 2 is the recording pass with the registry settled to
the 30, and the merge writer drops the other 18 (`merged_cells`). The committed file holds exactly
30 sections, which is what the coverage guard demands in both directions.

### What "running a cell" costs, measured

`test_gpu_corpus.rs:27` expands a `none` gpu-modes line to nothing, so the declaration is the only
switch — but there is a second, harder constraint nobody had named: **`every_device_cell_has_a_cpu_cell_at_the_same_mode`
forbids a gpu cell whose cpu twin is off.** Of pbench's 300 gpu cells (60 rows × 5; it was 295
before `int8-key-group`), 15 are `na` on the three commented-out float rows, and **157 of the
remaining 285 have no cpu cell at their mode and so cannot be declared at all.** 128 were runnable
and all 128 were run. So "295 cells, how many did you run" has the answer 128 of the 285 that a
declaration can reach, and the other 157 are a consequence of a rule rather than of a ticket —
they are the 24 shapes the planner refuses, the 5 narrowed by #190, and the tp4 modes of the four
rows whose cpu cells stop at tp1.

### The per-cell outcome

`on` = enabled; a number = the ticket the cell was measured to fail on; `—` = no cpu cell at that
mode, so not declarable.

| query | tp1-single | tp1-rowgroup | tp4-single | tp4-rowgroup | tp4-sized |
|---|---|---|---|---|---|
| `bool-key-group` | on | on | 206 | 206 | 206 |
| `cross-projection` | 63 | 63 | 63 | 63 | 63 |
| `decimal15-key-group` | on | on | 95 | 95 | 95 |
| `decimal15-key-join` | 220 | 152 | 95 | 95 | 95 |
| `decimal38-key-group` | on | on | 95 | 95 | 95 |
| `exists-null-keys` | 220 | 220 | 220 | 220 | 220 |
| `finish-without-probe` | 220 | 220 | 220 | 220 | 220 |
| `full-join-null-keys` | 152 | 152 | 152 | 152 | 152 |
| `inner-join-hot-keys` | 220 | 152 | 152 | 152 | 152 |
| `int8-key-group` | on | on | on | on | on |
| `left-join-null-keys` | 152 | 152 | 152 | 152 | 152 |
| `not-not-in` | 220 | 152 | 152 | 152 | 152 |
| `not-or-not-in` | 220 | 152 | 152 | 152 | 152 |
| `right-join-null-keys` | 220 | 152 | 152 | 152 | 152 |
| `rollup-small-keys` | 65 | 65 | — | — | — |
| `scalar-subquery-cross` | 63 | 63 | — | — | — |
| `sparse-build-anti` | 220 | 152 | 152 | 152 | 152 |
| `sparse-build-full` | 152 | 152 | 152 | 152 | 152 |
| `sparse-build-right` | 220 | 152 | 152 | 152 | 152 |
| `sparse-probe-left` | on | on | on | on | on |
| `sparse-probe-semi` | on | on | on | on | on |
| `timestamp-ms-key-group` | on | on | 240 | 240 | 240 |
| `timestamp-ns-key-group` | on | on | 240 | 240 | 240 |
| `timestamp-s-key-group` | 240 | 240 | 240 | 240 | 240 |
| `timestamp-us-key-group` | on | on | 240 | 240 | 240 |
| `ts-key-join` | 220 | 152 | 240 | 240 | 240 |
| `uint-key-group` | on | on | — | — | — |
| `uint-key-join` | on | 152 | — | — | — |

Totals: **30 on, 98 off, 128 run.** By ticket, the 98, re-counted off the table above: #152 42,
#220 18, #240 17 (12 at the hash kernel — the three `timestamp-*-key-group` rows and
`ts-key-join`, three tp4 cells each — plus 5 the wire refusal `timestamp-s-key-group` already
declared in `NOT_RUNNABLE`), #95 9, #63 7, #206 3, #65 2.

The failure texts, one per class, so the next reader need not re-run anything:

- **#152** — `GpuHashJoin lane 0: this join's recipe copies its build side per probe batch and the
  ABI has no copy: probe batch 2 has no build side left, since the call for batch 1 erased it`, and
  for Left/Full the probe-batch variant of the same message.
- **#220** — the `.cpu.txt` section comparison, at `corpus_golden.rs:202`. Two shapes.
  `exists-null-keys`'s cpu golden reads `batch_rows=[[0,0,0,0,0,0,0,0,0,0,0,1950]]` against the
  device's `[[1950]]`; `ts-key-join`'s reads `[[8192,8192,…]]` against `[[100000]]`. Where the rows
  happen to land in one cpu batch anyway the only trace is `output_bytes`: `inner-join-hot-keys` at
  `tp1-single` is 553644 on the cpu against 553638 on the device. Both are exactly
  `16 × 34070 + 2 × validity`, two `Int64` columns at a validity byte per eight rows, and a
  validity bitmap rounds up to a whole byte per batch: the cpu's eleven batches
  (`batch_rows=[[3128,3156,3189,3695,3219,3104,3212,3141,3384,3403,1439]]`) need 4262 bytes per
  column where the device's one batch needs 4259, and 2 × 3 is the gap. That is why five of these
  cells look like a six-byte disagreement and are the same defect.
- **#95 / #206 / #240 at the kernel** — `spark_hash_partition.cu:179: peacock spark_partition_ids:
  unsupported key column cuDF type_id=N`, with N 27 (decimal128), 11 (bool), 14/15/16 (timestamp
  ms/us/ns).
- **#63** — two arms, both of them. `scalar-subquery-cross`: `GpuProject lane 0: the output hook
  refused a batch: 0 columns declared, 1 held`, the `__rowcount__` placeholder. `cross-projection`:
  `CudfCrossJoin … cross_join.cu:45: Left table is empty`, a scan projected to no columns — which
  is the second arm #63's own fix proposal names.
- **#65** — `GpuAggregate lane 0: the output hook refused a batch: 2 __grouping_id: UInt8 vs INT32`.

### Six measurements that disagreed with the spec's prediction

The whole return on the round. Each one is now a registry tag.

1. **`sparse-probe-left` and `sparse-probe-semi` pass at all five modes**, where the table expected
   #152 (`gpu tp4` and `gpu` respectively). The `152` tag is gone from both rows; neither carries a
   ticket now.
2. **`exists-null-keys` fails on #220, not #152.** #152's own text says only the semi family
   streams, and this is a LeftSemi: the join completes and only the batching differs. Tag `152` →
   `220`.
3. **`finish-without-probe` fails on #220, not #173.** The completeness pass had already corrected
   #173's claim to "none" from the plan side; the cycle confirms it from the device side. Tag
   `173` → `220`.
4. **The three `sparse-build-*` rows never meet #212** (finding 5, confirmed). `sparse-build-full`
   is #152 at all five; `-right` and `-anti` are #220 at `tp1-single` and #152 at the other four.
   The cause is the one the completeness pass predicted from `driver/partitioned.rs:430`: the
   scatter's zero-row output is kept where the join owes its probe side, so `without_build` is
   never called. Tags `212` → `152` / `152 220`.
5. **`cross-projection` never meets #207, and fails on #63.** The planner narrows this query by
   projecting the *scans* — `GpuLoadParquet: table=tiny, projections=[], schema=[]` — so the
   `GpuCrossJoin` carries no projection for either backend to drop. Tag `207` → `63`, and #207 now
   has no corpus query at all.
6. **`rollup-small-keys` fails on #65, which the row did not carry.** It is the first corpus cell in
   the tree to reach #65 on a device; #65's text said none did. Tag `189` → `65 189`.

Five more rows gained `220` at `tp1-single` on top of the ticket they already carried —
`not-not-in`, `not-or-not-in`, `right-join-null-keys`, `decimal15-key-join` and `ts-key-join`;
`inner-join-hot-keys` carried it already. #220's registry-row count went 82 → 97, which was already
drift (88 before this round) and is now measured.

### Every device answer the cycle recorded agrees with DuckDB

Run 1's file held 48 sections and all 48 `duckdb_gpu_pbench_*` cases pass — the 30 enabled cells
and, as a free measurement, the 18 that fail only the cpu-shape comparison. So nothing in pbench
hits #253's first half (a device answer diverging while the cpu's matches), which that ticket
counted 27 pbench rows as being "in reach" of. The device's answers are right; what #220 moves is
the shape.

### gpu-result.txt, the eighteenth golden

`testdata/goldens/pbench.sf1/gpu-result.txt`, 325 KB, `cudf=25.02` on its first line, 30 sections,
one per enabled cell. Written under `PCK_WRITE_GPU_RESULT=1` ahead of both comparisons, which
`test_module_layout`'s `the_device_case_records_its_answer_before_it_compares_with_the_cpu` pins.
Nothing compares it with a previous version — it had none.

**tpch's and tpcds's copies were deliberately not pulled.** Run 2 was unfiltered, so it rewrote
them on the host too. tpcds's came back byte-identical; tpch's differs in six lines, all of them
float digits in `shuffle-stddev`'s stddev and variance columns, which is the run-to-run spread
build-test.md documents. No cell of either dataset moved and no code under them changed, so pulling
them would have committed noise. Their committed copies are untouched.

### The data change, and the golden churn it cost

`dim` gained `d_ts_ms` (TIMESTAMP_MS) and `d_ts_ns` (TIMESTAMP_NS), mirroring `fact`'s order, so
`d_ts_us` moved from source ordinal 9 to 10 and everything after it by two. Only `dim.parquet`
changed (57,197 → 64,739 bytes); `fact`, `sub`, `tiny` and `empty` came back byte-identical, which
is itself a determinism check. The churn was five lines per `.plans.txt` and two to four per
`-mini.cpu.txt`, every one of them a `projections=[… @N]` renumbering with the `output_bytes`
unchanged — the mapping prices only the projected columns, so two unread columns cost nothing.
`.cost.txt` did not move at all before `int8-key-group` landed, because ordinals are not in it.
`recipe-payloads.txt` did not move: its payload subset is tpch and tpcds only.

Section counts before and after, read for a vanished section (#213): every `-mini.cpu.txt`,
`-mini.cost.txt` and `mini.result.txt` went 56 → 56 on the data change and 56 → 57 with
`int8-key-group`; `duckdb-result.txt` 59 → 60. Nothing disappeared, so `PCK_UPDATE_SECTIONS=1
--exact` was not needed.

Regeneration used `PCK_UPDATE_SECTIONS=1` with a `cpu_pbench_` filter rather than
`UPDATE_CANONICAL=1`, because the latter prunes and a filtered prune would delete the sections the
filter did not run.

### `int8-key-group`

`testdata/pbench-queries/int8-key-group.sql` is `SELECT f_k8, count(*) AS n FROM fact GROUP BY
f_k8`; 200 groups, no NULL (`f_k8` is `(i % 200) - 100`). It lands `all_modes, all_modes,
duckdb_exact, data_fusion_exact, golden_exact, schema_validation_enabled`, a registry row with no
ticket, and it is the only pbench row enabled on the device at every mode — the table said "every
cell on if it passes", and every cell passes. Its plan at tp4 shuffles an `Int8` key, which the
kernel takes (`INT8` is in the supported list), which is why it is the one key-type row that is not
blocked at tp4.

#255's other three queries stay absent: adding them is a planner fix, which the Restriction forbids.

### Findings 5, 6 and 8

- **5** — see outcome 4 above. `architecture.md:643-647` needed no edit; the three tags did, and
  #212 gained a `Corpus queries:` line saying q77 is its only one.
- **6 — #137's skew.** Ticket and spec disagreed and neither pointed at the data; the ticket is the
  one that was wrong, and it is now right. The skew is a *cost* property, so by the spec's own
  criterion ("#136, #145, #154, #197 and #201 change cost or tests, not answers") #137 belongs in
  the spec's "not covered by a query" list and is missing from it — spec drift, reported, not
  fixable here because the spec is frozen. What the data shows, measured off
  `tp4-single-mini.cpu.txt`: `inner-join-hot-keys`'s probe lanes take 2160, 2413, 6538 and 8889 of
  20,000 rows, carrying `fact.f_k`'s 1034 NULL-key rows into one lane for an Inner join that
  discards them — 1034 and not 1000, because `gen.sql`'s `r1 < 0.05` is the intent and 1034 the
  realization. No registry row carries `137`, and none should: nothing fails on it.
- **8 — the determinism check.** `generate_pbench.sh --check` still cannot be a Rust test: it needs
  DuckDB 1.5.4, which no Rust tier has, and a test that skips when the binary is absent is not a
  guard. What was actually wrong was that deleting its one `run:` line was silent, so that is what
  is now red: `the_pbench_determinism_check_is_named_by_a_ci_step` in `test_ci_coverage.rs`, which
  reads the folded workflow lines like every other case there. Red-green proved by deleting
  `pipeline.yml:183` and watching it fail with its own message, then restoring it. The bar asks for
  the check itself in `rust-only`; what landed is the guard that the check runs, in the same tier
  the other CI-wiring assertions live in, and the difference is worth stating rather than papering
  over.

### The two stale records

`corpus_cases.inc`'s pbench section comment no longer says shad-gpu was down or points at
`pbench.md`'s Task 8 (a section that only ever existed in `-impl.md`). It now says what the tree
holds: every runnable cell run once on an L40S under 25.02, 30 on, 157 never runnable, three
queries absent on #255. Ten lines, at the cap.

**#259 is closed** and moved to `archive/archived-tickets.md`'s Done section with a dated closing
paragraph. All three of its pieces are discharged: `int8-key-group` exists and passes, fourteen of
its sixteen orphan tp1 cells are on (the two of `rollup-small-keys` are off on #65, a measured
ticket rather than a prediction), and the rest belong to `join-backend` (#152, #220, #63) and
`repartition-keys` (#95, #206, #240, #189) as it said. `tickets.md`'s index is re-summed: 121 open,
corpus-coverage 34.

### The bar, measured again

Local, `cargo test --features rust-only -p peacockdb-core -- --test-threads=2`, exit 0:

| tier | expected | measured |
|---|--:|--:|
| `--lib` | 677 | **678** (676 pass + 2 `#[ignore]` on #182) |
| `test_cpu_corpus` | 886 | **922** |
| `test_corpus_goldens` | 26 | 26 |
| `test_cost_model` | 3 | 3 |
| `test_golden_format` | 43 | 43 |
| `test_module_layout` | 18 | 18 |
| `test_ci_coverage` | 9 | **10** |
| `cost-report` | 39 | 39 |
| `test_duckdb_result.py` | 20 | 20 |
| `test_duckdb_cost.py` | 41 | 41 |

Device, on nebius-gpu, each binary run directly with `--test-threads=1`, all green:

| binary | expected | measured |
|---|--:|--:|
| `peacock_gpu_tests` | 4 | 4 |
| `peacock_plan_tests` | 56 | 56 |
| `test_gpu_corpus` | 28 | **58** |
| `test_node_timing` | 1 | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 536 | 536 |
| `peacock_gpu_benchmarks --skip bench_` | 8 of 11 | 8 of 11 |

Three numbers moved and all three are intended: `--lib` +1 is the new `dim` schema case,
`test_ci_coverage` +1 the new CI-step case, `test_gpu_corpus` +30 the enabled cells.
`test_cpu_corpus` +36 is 5 cpu cells and 1 DuckDB case for `int8-key-group` plus 30
`duckdb_gpu_pbench_*` cases that did not exist while every pbench device cell was off.

### build-test.md, re-summed rather than eyeballed

Derived from `corpus_cases.inc` and the registry, not from the old figures: 177 lines, 679 cpu
cells, 144 lines running at the modes each is correct at, 33 out entirely, oracles
110/16/14/33/4, the DuckDB tier 177 + 56 + 3 = 236, and so the `Corpus, cpu` row 679 + 6 + 236 =
921, which is the measured 922 less its registry case. Then every header it rolls into: cpu block
1592 → 1629, gpu block 576 → 606, the CI guard 9 → 10, Rust 2284 → 2352, grand total 2782 → 2850.
The golden count for `pbench.sf1` is 17 → 18, `gpu-result.txt` now sits in all three sf1 dirs, and
the dataset row reads 588 KB.

### Deferred by the override, and recorded as deferred

The sf40 pair (`peacock_tpch_tests`, `peacock_tpchv_tests` — 69 GiB against the L40S's 46 GB, and
no `tpch.sf40` on the host), `--run-benchmarks` and the three `bench_` cases, Nsight captures, and
any H200 timing. None of them was run and none of them blocks this task. A `test_gpu_corpus` pass
on sf1 is not on that list, which is what this round existed to settle.

### Drift found and not fixable here

The spec's "Not covered by a query" paragraph lists #136, #145, #154, #197 and #201 and should also
list #137, by its own "change cost or tests, not answers" criterion. The spec is frozen and its one
later write is spent on the signoff, so this is the human's. It rides naturally with the signoff
rewrite the previous completeness pass already asked for.

## Review round on the cycle (2026-10-08)

0 blocking, 7 important, 5 nits. Six were mine; the reviewer confirmed every heavy claim
independently, and found two beyond them: all 30 committed `gpu-result.txt` sections agree with
DuckDB, and the 28 `golden_exact` ones match `mini.result.txt`.

### I5 — the CI-step guard could not go red for the likeliest way the line stops running

`fold_continuations` keeps comment lines verbatim, so a bare `contains` read a commented-out
invocation as coverage: prefixing `pipeline.yml:183` with `# ` left the guard green. That is the
false-coverage mode `build-test.md`'s own entry for this file names, and every other predicate here
is hardened against its near-miss.

The match is now a named predicate, `line_runs_the_pbench_check`, rejecting a line whose trimmed
form starts with `#`, and it is pinned by a unit test of its own —
`the_pbench_check_matcher_rejects_a_commented_out_invocation` — on the pattern
`line_matcher_rejects_both_false_coverage_modes` already set, because a hardening proved once by
hand is one the next refactor is free to undo. That test is why `test_ci_coverage` is 11 and not 10.

Red-green, three ways: the new unit test failed before the predicate was hardened and passes after;
commenting out `pipeline.yml:183` reddens
`the_pbench_determinism_check_is_named_by_a_ci_step` and restoring it greens it; and deleting the
line outright still reddens it, as the previous round proved.

### I6 — the `dim` guard asserted names, not types, and skipped a key pair

Two holes, both real. A `d_ts_ms` regenerated at the wrong *unit* keeps the name `ts_ms`, and a
timestamp unit is the exact class of defect finding 7 was about. And the `k*`/`ts_*` prefix filter
excluded `f_dt`/`d_dt`, which the spec's data table lists among the hashable key types and which
`spark_hash_partition.cu:180` takes as `DATE32`.

The set is now `(name, arrow type)` pairs read through `ParquetRecordBatchReaderBuilder`, so a unit
shows as `Timestamp(Millisecond, None)` against `Timestamp(Microsecond, None)`. **The filter is
gone and is an exclusion instead** — each table's row id and its own measures (`f_qty`, `f_amount`;
`d_w`, `d_name`) — so "must match" is the default and a column added to one table and not the other
goes red without anyone widening a whitelist. That takes the compared set from 13 pairs to **15**:
`dt` and `s` join the thirteen, both agreeing in type, and a count assertion pins the 15 so the
exclusion list cannot quietly swallow a pair. Nothing else needed excluding and no pair is
special-cased.

Red-green by mutating the generator, since the guard reads committed data and that is the only
input it has:

- `d_ts_ms` written `::TIMESTAMP` instead of `::TIMESTAMP_MS` → red, naming
  `("ts_ms", "Timestamp(Millisecond, None)")` against `("ts_ms", "Timestamp(Microsecond, None)")`.
  The old name-only guard was green on this.
- `d_dt` deleted from `gen.sql` → red, naming `("dt", "Date32")`. The old filter never looked at it.

`gen.sql` was restored from a backup taken before the first mutation, regenerated, and
`dim.parquet` checked back to its committed sha256 (`4902c102…`) with `generate_pbench.sh --check`
green after it.

### The four record fixes

- **I1** — the by-ticket distribution of the 98 summed to 95. `ts-key-join`'s three tp4 cells are
  `240` in the table and the parenthetical did not reach them, so #240 is **17**, not 14, and
  12 of those are the hash kernel (the three `timestamp-*-key-group` rows and `ts-key-join`, three
  tp4 cells each) against 5 for the wire refusal. The same undercount made `repartition-keys`
  inherit 26 where it inherits **29**; both files now add to 98.
- **I4** — `fact.f_k` holds **1034** nulls, not 1000: `r1 < 0.05` is the intent, 1034 the
  realization. The lane figures were exact.
- **N1** — five rows gained `220` on top of what they carried, not four; `inner-join-hot-keys`
  carried it already.
- **N4** — the arm that expands `none` to nothing is `test_gpu_corpus.rs:27`.
- **N5** — the six-byte gap is not "four cpu batches". Both figures are exactly
  `16 × 34070 + 2 × validity`: eleven cpu batches round their validity bitmap up to 4262 bytes per
  `Int64` column where the device's one batch needs 4259, and 2 × 3 is the gap. Derived and checked
  against both committed numbers.

## The review round, as the coordinator routed it (2026-10-08)

**0 blocking, 7 important, 5 nits**, on the two commits `d7ecbc00..d64c1f9d`. The reviewer
re-derived rather than read, which is the standard this branch has kept: it recomputed all 128
`.cost.txt` sections from their sibling `.cpu.txt` and `cost_model.conf` with zero mismatches,
re-summed every tier of `build-test.md` from the declarations, checked all eleven ticket-index rows
against their files' anchors, and verified `gpu-result.txt`'s 30 sections against the enabled set in
both directions. Beyond the claim it also found that **all 30 committed sections agree with
DuckDB** and the 28 `golden_exact` ones match `mini.result.txt` exactly, so those cells pass their
cpu comparison too.

Split by whose file it was. Two code findings and the four record fixes went to the developer, whose
section above records them. Six were mine:

- **I2** — `build-test.md`'s enumeration of the 30 device cells described 31, `uint-key-join` being
  at `tp1-single` alone and not both tp1 modes. Re-derived from the registry before fixing.
- **I3** — the `Harness datasets` row moved to 4 and its prose still described three cases. The
  fourth is now named.
- **I4's wiki half** — #137's text said 1000 nulls where `fact.f_k` holds **1034**; confirmed with
  pyarrow over the committed parquet before editing.
- **N2** — #65 ran to 36 non-blank lines against the 30 a `Fix proposed:` block allows. Trimmed to
  exactly 30 by saying the `GROUPING()` consequence once and naming the four `bug_` pins by file
  rather than by test name. No anchor moved, so `ticket_is_open` reads the same set.
- **N3** — `cost-report/src/main.rs`'s comment claimed every pbench row names a ticket. Three no
  longer do; the comment now says which and why the test still exercises the index.
- **The count I5 moved.** The developer's hardening carries its own pinning test, so
  `test_ci_coverage` is **11**, not the 10 this round first measured. `build-test.md` re-summed:
  CI guard row 11, second-table Rust 111, Rust **2353**, grand total **2851**, first table 2242.
  Checked by script — every block header equals its rows and every total its parts.

**I7 is deferred to the signoff, deliberately.** The spec's completeness signoff is false in six
specifics — 580 KB, 56 corpus lines, 59 registry rows, 123 cpu cells, "every gpu cell off and not
one run", and #259 still open. The measured figures are **588 KB, 57 active corpus lines (3
commented out on #243), 60 registry rows, 128 cpu cells on, 30 gpu cells on of 128 run, 18 goldens,
60 of the spec's 63 queries present**. The signoff is the spec's one later write and this is its
slot, so it is replaced wholesale when the completeness pass closes rather than patched now. The
#137 spec drift the developer reported — by the spec's own criterion it belongs in "Not covered by a
query" and is absent — is named there too, the paragraph itself being frozen.

**CI, read on `2907ed10`, the commit that carries the cycle.** Green on `Changed paths`, both cuDF
legs, the GPU build, the cost report and the S3 check; `GPU Tests (remote)` red on
`ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`, which is the shad-gpu
outage the override exempts and not a finding. The cost-report job passing is what settles the
comment-cap risk: the PR comment fits. The board commit `d64c1f9d` has a run of its own and it is
**vacuous** — documentation only, so `changes` answered `code=false` and every job skipped green in
seconds having compiled nothing. Worth knowing before `done`, which asserts CI green on the PR: the
run that counts is the last one over code.

## Completeness pass — the second analyst reading (2026-10-08)

Mechanical verification off the committed artifacts, so the signoff need not re-derive it. Nothing
here was built or run; the parquet was read with pyarrow and duckdb 1.5.4's python module, the
goldens and the registry with text comparison.

### The spec's data table against the parquet

Every promise in `pbench.md`'s table holds except one. `fact` 20,000 rows / 10 row groups; `dim`
2,000 / 1; `sub` 200 / 1; `tiny` 8 / 1. `f_k` 1,034 NULL (5.17%) with 10,058 rows on keys 0–2 and
1,001 distinct values over 0–1000. `f_kf64`: +NaN 199, −NaN 204, −0.0 191, 0.0 563, NULL 182;
`f_kf32` the same NaN and −0.0 counts. `d_kf64`: +NaN 10, −NaN 8, −0.0 8; `d_kf32` +NaN 10, −0.0 8
— so `fact` carries NaN of both signs and `dim` carries −0.0 and NaN, which is what work item 1
asks for. `f_ku32` 9,920 values past `i32::MAX`, 153 NULL. `f_ts_s` and `d_ts_s` hold no
sub-second value. `f_kstruct` ↔ `d_kstruct` and `f_ku32` ↔ `d_ku32` both join non-empty. `sub.s_y`
8 of 200 NULL. `tiny.t_k` two keys, four rows each; seven of its eight `t_pat` patterns match at
least one `d_name`. `empty` carries `tiny`'s four columns.

**The one unmet cell: `empty.parquet` has 0 row groups where the table says 1.** Already recorded
at the head of this file and on #256, and it is what makes `cross-empty-build` a #256 refusal
rather than the #208 demonstration it was written for.

One soft spot, not a defect: `dim`'s "every key twice" is 950 keys twice and 50 once — the 1-in-40
NULL overwrites one copy of 50 keys. The many-to-many property holds.

### The oracle agreement, checked without a device

- All 17 pbench `duckdb_exact` corpus lines: `mini.result.txt`'s rendered rows equal
  `duckdb-result.txt`'s as multisets. No differences.
- All 28 `duckdb_exact` sections of `gpu-result.txt`: the same against `duckdb-result.txt`. No
  differences.
- `uint-key-group`'s fingerprint is byte-identical across `duckdb-result.txt`,
  `mini.result.txt` and both `gpu-result.txt` sections, hash included.
- `duckdb-result.txt` holds 60 sections and 0 `error` sections; `gpu-result.txt` 30, one per
  enabled gpu cell; `mini.result.txt` 57, one per live corpus line.

### The registry arithmetic

60 rows; 57 live corpus lines and 3 commented out on #243. 300 cells per engine, 15 `na`, 285
declarable. cpu: 128 on, 157 off. gpu: 30 on, 98 off with a ticket, 157 undeclarable because the
cpu twin is off. The 98 by ticket — #152 42, #220 18, #240 17, #95 9, #63 7, #206 3, #65 2 —
reproduce the per-cell table above. No row with an `off` or `na` cell lacks a ticket, across the
whole file and not just pbench. Corpus-wide oracle counts match `build-test.md` exactly: 177 lines,
110 `duckdb_exact`, 16 `duckdb_approx`, 14 `duckdb_fingerprint`, 33 `duckdb_none`, 4
`duckdb_divergent`.

### `generate_pbench.sh --check`, the half that needs no regeneration

Its hardcoded `FACT_SPECIALS` (`199 204 191 563 199 204 191`), `DIM_SPECIALS` (`10 8 8 10 8`) and
10-row-group assertion all match the committed parquet, re-derived independently. The regeneration
half was not run: it needs the duckdb CLI and would write outside this reading's permitted path.

### A fourth query cannot go missing silently

Three guards close the loop in different directions, so the absence of the #255 three is asserted
rather than tolerated and a fourth disappearance is loud. Deleting a `.sql` makes
`plan_goldens::pbench_*` re-render without that section and go red against the committed golden.
A registry row with no golden section, or a section with no row, is
`the_registry_matches_the_goldens_in_both_directions`. A `duckdb-result.txt` section with no row,
or a row with no section, is this branch's new
`every_duckdb_result_section_has_a_registry_row_and_every_row_a_section`. And adding one of the
three turns `testdata/test_duckdb_result.py:165` red in the cost-report job.

#255's own citations check out: `common.rs:41-68` panics on any arm it has no deterministic width
for, and `planner/memory_estimation.rs:171` and `:276` call it through `logical_size_from_schema`
at plan time, so a Struct or Interval column in any node's output schema aborts the process.

### The 157-cell bound: where it lives after the merge

Recorded in two places the archive keeps — `corpus_cases.inc:307` (the pbench section header) and
#259's closing note at `archive/archived-tickets.md:64` — plus this file, which the merge deletes.
No later task plans work the bound forbids: `repartition-keys` and `join-backend` each turn on the
cpu side of every row whose gpu cells they claim, in the same task.

## Completeness pass, third time round — and it closes (2026-10-08)

Two readings at `008c777d`, dispatched together, neither seeing the other's list. **The reviewer
found 0 blocking and 5 important; the analyst 0 blocking and 5 important.** The analyst's own
section is above. Both independently found the same thing — **#190** — which is the strongest
signal this pass produced and is why it is the first fix below.

Everything load-bearing was confirmed by one or both, and the agreement is worth recording because
it is what makes the signoff defensible: the Restriction holds (the branch's only non-test
production sources are `cpu_backend/mod.rs` and `wire/expr_writer.rs`, exactly the two the spec
permits); #227's cpu half cannot change an answer, since `declared_as` ends in
`RecordBatch::try_new`, which refuses the same NULL itself, so the check only re-words a refusal
arrow already raises; the 98 off tags are consistent with the capability matrix mode by mode; the
three `sparse-build-*` rows are tagged right, `plan/join.rs:461` making `without_build`
unreachable for Right, Full and RightAnti at every mode; `gpu-result.txt` is held as a record and
never an authority; no golden contradicts itself, Σ `batch_rows` equalling `output_rows` at every
node of all five `.cpu.txt` files; and no coverage regressed — 25 cells went `disabled` → `enabled`
and nothing went the other way.

### The nine fixes, all mine, all prose

**#190 was the one ticket this branch left false, and both readings found it.** Its text said "four
registry rows with `tpch/q11` and `tpcds/q54`". Nine rows carry `190` now and 45 cpu cells are off
on it — pbench's `nl-inner`, `nl-left`, `nl-projection`, `nl-left-decimal` and
`like-column-pattern` joined the four. I re-derived that from the registry before editing. The
ticket now carries the live list and the fact that makes it matter: those rows' device cells have
never run and cannot until the cpu answers, because a gpu cell needs a cpu cell at the same mode.
Trimmed to 14 lines from 20 while doing it, since it was over the cap before this branch touched it.

The rest:

- **#80 and #59** both still said no corpus query exercised a nullable anti/mark key. Seven pbench
  rows do — the same seven on each — refused at all five modes by a message naming both tickets,
  with DuckDB's answers committed. #80 carries the list; #59 points at it and says plainly that
  they are refused rather than enabled, which is why "no enabled query" was true and misleading.
- **#246 claimed "the cpu answers"** its corpus row. It does not: `like-column-pattern`'s nested
  loop carries `projection=[t_id@0, d_id@2]`, read off `tp1-single.plans.txt`, so #190 refuses it
  first and all five cpu cells are off. Its device cells therefore carry #246 unmeasured.
- **#159 and #215** gained the corpus lines their text implied they lacked — #159's "not a shape
  only a constructor produces" is now two measured rows, and #215's row is refused by #190 first.
- **#140 had nothing at all**, despite a whole spec section ("#140's cost, made visible") and three
  delivered readings. It now names them and the test that counts the merges.
- **`build-test.md`'s "End to end" row** said 30 in its N column and "ten cases ... two of the 29
  ... so 27 run" in its prose; the branch bumped the number and not the sentence. Eleven, two of
  30, 28 run, with the nullability case named in the enumeration. The golden-flow diagram's spine
  said "both feed every branch below" where three datasets feed it.
- **`corpus_cases.inc`'s section comment** said a row with no cpu cell is refused by the planner.
  Of the 29 such rows, 24 are, 21 of those citing their ticket on the `refused:` line, and the
  other 5 plan fine and the cpu backend refuses them at run time on #190. Verified by parsing the
  plan golden: 24 refusals, 3 citing no ticket.
- **`repartition-keys-impl.md`'s Task 5c Step 4 was a step that cannot execute.** It told its
  developer to put three pbench queries into `NOT_RUNNABLE`; all three `.sql` files are absent
  until #255 closes, `test_duckdb_result.py:165` asserts their absence, and `NOT_RUNNABLE` is
  checked both ways, so the step goes red on contact. Now deferred in place, citing #255.
- **`join-backend.md` counts 23 cpu cells from an estimate that is now 48**, and that spec is
  frozen. Fixed where the authority is: `reports/join-rewrite-cell-estimate.md` names the move in a
  dated note beside its own row, its original figures left as taken — an estimate rewritten after
  the fact records nothing — and #190 carries the live list a reader will reach first.

Ticket caps re-checked after every edit: the six joins.md tickets I touched are all within theirs,
#140 lands at exactly 15, and the five still over (#173, #220, #243, #250, #256) were over before
this branch and are the house condition both readings scored as a nit.

### What neither reading could verify

The cycle's own measurements — which cell passed, which ticket each of the 98 failed on, and the
device suite counts. Both checked the tags for consistency against the plan goldens, the capability
matrix and each ticket's own text and found no contradiction, but the runs themselves rest on the
record in this file. That is the standing limit of a reading without a card, and it is why the
device half is recorded here in the detail a re-run would need.

## Done (2026-10-08)

CI on `e91e3377`, the commit carrying the completeness pass, is **green on every job but
`GPU Tests (remote)`**: `Changed paths`, both cuDF legs, the GPU build, the cost report and the S3
check. The one failure is `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed
out` — the shad-gpu outage the override exempts, and the same message the chain has had all day.

This run counts, which is the thing worth checking before trusting it: `Changed paths` answered
`code=true` because the commit touches `corpus_cases.inc`, so both legs genuinely built and ran
rather than skipping a documentation diff. The review round's own code commit `af217d73` has the
same shape and the same result. The board-only commit `008c777d` in between has a vacuous run —
every job skipped green in seconds — which is why `done` was read off a commit over code and not
off the branch tip's most recent run.

The cost report job passing is what settles the comment-cap risk the cycle carried: the PR comment
fits under `COMMENT_MAX_BYTES`.

The override's second condition is met too: the GPU tests passed on nebius-gpu, recorded above in
"Re-proved after the rebase" and "The device cycle, run".

**State: `done`.** Terminal for the ensemble; the human merges PR #168.

## Rebased onto the rebased duckdb-oracle (2026-10-09)

`git rebase --onto ENS-duckdb-oracle 0df60133 ENS-pbench`, 25 commits, replayed with a
worktree-local untracked `.gitattributes` giving the six overlapping markdown files `merge=union`
so the replay never stops, and one three-way merge per file afterwards. Nine conflict hunks in all.
The method and why it beats resolving per commit: `duckdb-oracle-detail.md`, "The rebase, executed".

**The intersection was eight files, and two of them are code.** pbench touches 126 files, the new
parent moved 43, and they overlap in `.github/workflows/pipeline.yml`,
`cost-report/src/main.rs`, and six `llm-wiki/*.md`. Both code files **auto-merged with both sides
intact**, which was checked rather than assumed: pbench's own change against the new parent is
exactly the 152/26 and 5/0 it was against the old one, `REGRESSION_FAIL_PCT` appears 11 times in
`cost-report/src/main.rs` on this branch as on its parent, and the 10%-gate error string is in
`pipeline.yml`. Their hunks never touched — pbench works at lines 376–2273 of `main.rs` and
master's `f0a6ecbf` at 36–1941, pbench at lines 177 and 503 of `pipeline.yml` and the other two
sides at 712 and 928. The 118 non-intersecting files came across byte for byte.

**The six markdown files.**

- **`tasks.md`** — three hunks, all states. duckdb-oracle takes its parent branch's `done`, since a
  task's own branch is the authority on its own state. master's resequencing drops stale-cells from
  task 2 and renumbers, as on the parent. The one thing kept from this branch rather than its
  parent: pbench **closes half of #227**, not #227 — this branch's own finding about its own scope,
  which master cannot know yet and which reaches it when PR #168 merges. So the board here differs
  from its parent's in that one line.
- **`tickets.md`** — two hunks, again re-derived from the ticket files' anchors rather than taken
  from a side: **127 open over 11 rows**, every row agreeing with its file in count and order, the
  header equal to the sum. "Next free 264" survives untouched and is right: this chain's #251–#259
  all predate master's reservation of 264–279 for it, and none of them collides with master's
  #261 or #280–#283.
- **`archive/archived-tickets.md`** — one hunk, both sides inserting at the head of **Done**. This
  branch's #259 goes above master's #237 and #236, which keeps the head of the section descending.
- **`tickets/complete-coverage.md`** — one hunk, and the only one needing judgement. Both sides
  rewrote the end of #249 and then appended: master a new #283, this branch a rewritten #249
  paragraph plus #255. Resolution keeps this branch's #249 paragraph (it is strictly more
  informative — it says the two queries cannot reach the wire at all and names #255), then master's
  #283 where master put it, then #255 appended. Eight anchors, matching the index row.
- **`tickets/corpus-coverage.md`** — no conflict at all; the clean three-way merge equalled what
  the union replay had already produced, so the file needed no write.
- **`build-test.md`** — three hunks, the count lines, derived from the page's own tables as before
  and not by adding deltas. **2856 — Rust 2358, C++ 97, Python 401**; cpu 1633 (`--lib` 682,
  `test_cpu_corpus` 922, 26, 3), ffi 7, gpu **606** (536, 58, 11, 1), every tier heading equal to
  its rows and to its own sub-breakdown. The cost-report renderer row is 40: 37 at the fork, +1
  from master's gate tests, +2 from this branch.

**What the re-prove has to look at.** Two of the six commits master brought carry code. `64ced62e`
moves the plan-text renderer, and the duckdb-oracle round established exactly where it bites: only
where a join's pre-projection width differs from build++probe, so `LeftMark`, `RightSemi` and
`RightAnti` and nothing else. duckdb-oracle's goldens were untouched because tpch's three
`projection=` join lines are all `LeftSemi`/`LeftAnti`. **pbench is the branch where that is likely
to be different**: its whole subject is NULL keys and `NOT IN` over NULLs, and a `NOT IN` lowers to
an anti or a mark join. If pbench's plan goldens move, that is the expected regeneration and not a
defect. The second, `f0a6ecbf`, is the cost gate, which the cost-report tests cover.

## Re-proved on the rebased duckdb-oracle (2026-10-09)

The re-prove the rebase above owed. Whole proving set, not a subset: the local `rust-only` tier
in full, the C++ CPU suite, cost-report and its Python half, and the device set on nebius-gpu's
L40S under the host override. **Every tier is green. Ten plan-text goldens moved and all ten moves
are renderer text.** No defect found, no ticket owed.

### What master's `64ced62e` did to this branch, and why it is the expected regeneration

The prediction held. `projection_field` now names each projection position by the node's **own
output schema** instead of by build++probe, so a join whose pre-projection width differs from
build++probe prints different names. pbench has 25 such lines per golden family, and nothing else
in the tree did: `UPDATE_CANONICAL=1` rewrote every `.plans.txt` in all three sf1 dirs and only
pbench's five came back changed, which is independent confirmation that master's own regeneration
of the ten tpcds goldens was right on this base.

Watched red first, in both tiers:

- `--lib`: **675 passed, 5 failed** — the five `planner::tests::plan_goldens::pbench_*`, each
  reporting "5 of 60 queries differ".
- `test_cpu_corpus`: **897 passed, 25 failed** — `cpu_pbench_<q>_<mode>` over five queries × five
  modes, each a single `moved — line N, column C` from the golden differ.

The five queries, and the join each lowers to:

| query | join line | was | is |
|---|---|---|---|
| `finish-without-probe` | `LeftMark` | `[d_id@0, d_w@2, Int64(1)@3]` | `[d_id@0, d_w@2, mark@3]` |
| `not-not-in` | `RightSemi` | `[s_y@0]` | `[f_id@0]` |
| `not-or-not-in` | `RightSemi` | `[s_y@0]` | `[f_id@0]` |
| `sparse-build-anti` | `RightAnti` | `[t_k@0]` | `[f_id@0]` |
| `sparse-probe-semi` | `RightSemi` | `[t_k@0]` | `[d_id@0]` |

Exactly the three join types the duckdb-oracle round predicted, and no fourth. Every other
`projection=` line in these files — `Inner`, `Left`, `Right`, `Full`, `LeftSemi`, and the three
nested-loop and filter variants — is untouched, because for those the output schema and
build++probe agree position for position.

### The proof that it is text and not an answer

Three mechanical checks per file, not a reading:

- **Mask the field and the files are byte-identical.** `sed -E 's/projection=\[[^]]*\]/<X>/g'`
  over `HEAD:<file>` and the regenerated file: `diff -q` clean for all ten. So nothing outside
  `projection=[...]` moved — not a node, not a `lanes=`, not an `on=`, not an `output_rows=`.
- **Every projection ordinal is unchanged.** The `@N` sequence extracted from the projection
  fields is identical before and after: 47 ordinals in each `.plans.txt`, 36 / 36 / 32 / 32 / 32
  in the five `.cpu.txt`. Only the names left of the `@` moved.
- **No section was lost and no row count moved** (#213's hazard): `== ` sections 57 → 57 in every
  `.cpu.txt` and 60 → 60 in every `.plans.txt`; `rows=` lines 348 / 348 / 430 / 430 / 430,
  unchanged.

And the new text is the *correct* text, which the goldens show on their own. In
`finish-without-probe` the `GpuFilter` sitting directly above the mark join already read
`predicate=(d_w@1 = 0) OR mark@2` — the filter has always named the join's output columns
`d_w` and `mark`. The old `Int64(1)@3` on the line below it was naming the probe side's
column 3−build_width and contradicting its own parent. `output_rows=2000, output_bytes=25000` on
that join is unchanged. Same for `sparse-probe-semi`: `output_rows=3, output_bytes=25`, one column
wide, and `d_id` is the column the RightSemi actually emits where `t_k` was the discarded build
key.

`.cost.txt` carries no plan text (checked: zero `projection=` in all five), so the five
`-mini.cost.txt` did not move and `test_cost_model` re-derived all three sections green.
`mini.result.txt`, `duckdb-result.txt` and `gpu-result.txt` did not move either — no answer
changed, which is the whole point.

### Regeneration commands

```
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --lib \
  -- planner::tests::plan_goldens --test-threads=1          # 26 passed; 5 .plans.txt rewritten
PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core \
  --test test_cpu_corpus cpu_pbench_<q>_ -- --test-threads=1   # once per query, 5 passed each
```
`PCK_UPDATE_SECTIONS` (merge only) rather than `UPDATE_CANONICAL` for the `.cpu.txt`, and
`--test-threads=1`, so the two-writer section loss cannot happen. `PEACOCK_REWRITE_RECIPE_BYTES`
was *not* set: `recipe-payloads.txt` verified rather than rewrote, and its two guard cases passed,
so the wire format did not move under the rebase.

### The bar, measured

Local, `cargo test --features rust-only -p peacockdb-core -- --test-threads=2`, after the
regeneration — exit 0, and `--no-run` reports **zero warnings**:

| target | `--list` | result | `build-test.md` |
|---|--:|---|--:|
| `--lib` | 682 | 680 passed, 2 ignored (#182), 0 failed | 682 |
| `test_cpu_corpus` | 922 | **922 passed, 0 failed** | 922 |
| `test_corpus_goldens` | 26 | 26 passed | 26 |
| `test_cost_model` | 3 | 3 passed | 3 |
| `test_golden_format` | 43 | 43 passed | 43 |
| `test_module_layout` | 18 | 18 passed | 18 |
| `test_ci_coverage` | 11 | 11 passed | 11 |
| `cargo test -p cost-report` | **41** | 41 passed | **40 — wrong, see below** |
| `ctest -L cpu` / `peacock_cpu_tests` | 15 | 15 passed | 15 |
| `testdata/test_duckdb_result.py` | — | Ran 20, OK | 20 |
| `testdata/test_duckdb_cost.py` | — | Ran 41, OK | 41 |
| `scripts/exec_model/tests/` (10 files, minus the three run elsewhere) | — | **216 passed** | 216 |
| `scripts/exec_model/tests/test_tpch.py` | — | 19 passed | 19 |
| `scripts/calibration/tests/` (3 files) | — | 12 passed | 12 |
| `testdata/generate_pbench.sh --check` | — | `pbench.sf1 matches gen.sql`, exit 0 | — |

Device, nebius-gpu `dmitry@89.169.109.150` (L40S, card idle, 37 GB free on `/`), cuDF 25.02,
`./scripts/build-test-shadgpu.sh --build` exit 0 and **zero warnings**, each binary run directly
with `LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib`,
`PEACOCK_TESTDATA_DIR=$PWD/testdata`, `--test-threads=1`:

| binary | `--list` | result | `build-test.md` |
|---|--:|---|--:|
| `cpp/install/bin/peacock_gpu_tests` | 4 | 4 passed | 4 |
| `cpp/install/bin/peacock_plan_tests` | 56 | 56 passed | 56 |
| `cpp/install/rust-tests/test_gpu_corpus` | 58 | **58 passed, 0 failed** | 58 |
| `cpp/install/rust-tests/test_node_timing` | 1 | 1 passed | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 536 | 536 passed | 536 |
| `peacock_gpu_benchmarks --skip bench_` | 11 | 8 passed, 3 filtered | 11 |
| `cpp/install/bin/peacock_cpu_tests` (bonus) | 15 | 15 passed | 15 |

`test_gpu_corpus` ran **after** the regenerated goldens were rsynced up, and all 58 cells passed
against them — so the device agrees with the new renderer text as well as the cpu does. The device
authored nothing: all 18 `testdata/goldens/pbench.sf1/*` are md5-identical on both hosts after the
run, and no `gpu-result-<v>.txt` exists anywhere in the tree.

The C++ side of the device build was a no-op (`ninja: no work to do` locally too) — the rebase
moved no file under `cpp/`, so the three C++ binaries are the ones staged on 2026-10-08 and the
four Rust ones were relinked.

### `f0a6ecbf`, the cost gate

Covered and green: `cargo test -p cost-report` 41 passed, including the gate cases master added
(`REGRESSION_FAIL_PCT`) and this branch's three widget cases; `test_ci_coverage` 11 passed, which
is what reads `pipeline.yml`. Nothing about the 10% gate misbehaves against pbench's own 152/26
and 5/0 hunks in `main.rs`.

### build-test.md: one declared figure is wrong, by one

Measured against the page's own tables. Right: cpu **1633** (682 + 922 + 26 + 3), ffi **7**
(`--lib -- ffi_tests::` 4 on the device lib + `peacockdb-ffi --test test_ffi` 3 by source count),
gpu **606** (536 + 58 + 11 + 1), C++ **97** (75 measured — 15 + 4 + 56 — plus the 22 sf40 /
multi-GPU / manual cases the override defers), Python **401** (308 measured, plus the 93 manual
corpus queries).

Wrong: **the cost-report renderer row is 41, not 40.** The row's derivation was
"37 at the fork, +1 from master's gate tests, +2 from this branch"; the truth is **36 at the fork,
+2 from master, +3 from this branch**. Two of the three terms were off and nearly cancelled.
Counted four ways, all agreeing: `cargo test -p cost-report -- --list` says 41; `#[test]` in
`cost-report/src/main.rs` is 41 here, 38 on `ENS-duckdb-oracle`, 36 at `f0a6ecbf^` and 36 at the
rebase's old upstream `0df60133`; `git show f0a6ecbf -- cost-report/src/main.rs` adds 2; and the
three names this branch adds over its parent are
`the_widget_renders_a_pbench_section_beside_the_benchmarks`,
`a_cell_with_all_five_modes_enabled_shows_one_larger_tick` and
`a_cell_with_any_mode_off_keeps_five_glyphs`.

So two sums move with it: **Rust 2358 → 2359** and **grand total 2856 → 2857**. The markdown is
the human's; this round does not edit it.

A trap worth naming: local `master` in this worktree is stale at `2ad302bf` and does **not**
contain `f0a6ecbf`. `origin/master` is `f0a6ecbf`. Counting a master-side figure off the local ref
gives the pre-gate number.

### Deferred by the override, and recorded as deferred

Unchanged from the previous round and none of it blocks the task: the sf40 pair
(`peacock_tpch_tests`, `peacock_tpchv_tests` — 69 GiB against the L40S's 46 GB, and no
`tpch.sf40` on the host), `--run-benchmarks` and the three `bench_` cases, the manual-only C++
rows (streamed sf40, per-operator timings, the three multi-GPU binaries), Nsight captures, any
H200 timing, and the 93-query exec-model corpus. shad-gpu is down, so `--run` and
`--pull-results` were never invoked; verda is unreachable, so every CPU run was local.

### Two local environment facts, still true

`cpp/build` in this worktree is an empty root-owned directory and cmake still dies at configure
there, so the C++ CPU suite was built and run out of `/tmp/dkb-cppbuild`, which is already
configured against this worktree's `cpp/` with `cudf_ROOT` on the 25.02 env. And `matplotlib` is
still absent from the workstation: `pip install --target /tmp/pydeps matplotlib` plus
`PYTHONPATH=/tmp/pydeps MPLBACKEND=Agg` runs `test_plot.py` without touching the system
environment. One new trap beside it — a stray `/tmp/struct.py` from an earlier round shadows the
stdlib `struct` module for anything run with `cwd=/tmp`, which breaks `import matplotlib`; run
from the repo root.

### Files left in the working tree

Ten regenerated goldens under `testdata/goldens/pbench.sf1/` — the five `.plans.txt` and the five
`-mini.cpu.txt`, 25 changed lines each family, all `projection=` names — plus this section of
`pbench-detail.md`. No code change: the rebase's two code files needed none, and nothing in the
suite asked for one.

### The one count the derivation got wrong, and how

The round measured the cost-report renderer row at **41**, not the 40 the rebase derived, so the
page now reads **2857 — Rust 2359**. The derivation said "37 at the fork, +1 from master, +2 from
this branch"; the truth is 36 at the fork, +2 from master, +3 here, confirmed four ways (`cargo
test -p cost-report -- --list`, the `#[test]` count at four revisions, `f0a6ecbf`'s own diff, and
the three test names this branch adds). The cause is worth keeping because it will bite again:
**the local `master` ref in this worktree is stale at `2ad302bf` and does not contain `f0a6ecbf`.**
Any master-side figure read off `master` rather than `origin/master` is a pre-gate number. Read
`origin/master`.
