# bench-rows — detail

## Run log

- 2026-11-04: `ENS-bench-rows` branched from `ENS-pinned-loads` at `bcf1cdef`, while new-hosts and
  pinned-loads are `blocked(building)` on the H100 path (the human's GitHub PAT; see
  `new-hosts-detail.md`, "Blocked"). Tasks 1–4 are offline and run now; Task 5 (one case on the
  H100) waits for the H100 path. pinned-loads' Tasks 4 and 4b (`PinnedCache`, `PrefillStats`,
  `process_prefill`) are committed on the base.

## Task 1: the node line carries `batch_rows` and `batch_bytes`; the tree's load total

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- driver::tests::render`
  failed to compile: `E0432 unresolved import crate::plan_text::timed_load_us` and three `E0061`
  (`render_timings` takes 2 arguments). Then `driver::tests::render plan_text` 35/35.
  Mutation: `timed_load_us` summing only the first loader fails `the_load_is_every_loaders_total`
  (`left: 40, right: 65`).
- **Departures from the plan's text:** none in substance.
  - `the_load_is_every_loaders_total` builds `unload(join(source("a", 1), source("b", 1)))` with a
    script for both tables, and journals one call per node at `abi_calls[node][0][0]` (join 7 µs,
    loaders 40 and 25). The plain join runs under the mock; no coalesce is needed on the build.
  - `batch_tokens` is a local helper in `render.rs`, beside `call_of` and `region`.
  - `LOAD_NODE` is used by `timed_load_us` only; Task 3's reader compares the parsed name with
    the same literal (`tree_load_us`).
- **What is subtle.** `time_us` lanes are the node's driving lanes (`Measurements::lanes`); the
  batch lists are its output lanes (`RunReport::emitted`). They differ on an emitter, and an
  accumulator has many `time_us` entries per lane and one batch. Nothing joins the two lists.
- **Gates:** `--lib` 932 passed, 2 ignored (934; 930 + 2 before); `test_corpus_goldens` 47;
  `cost-report --test test_source_formatting` 32. No warning.

## Task 2: the trailer carries `load_us`, `execute_us`, `peak_bytes`, the cache and the prefill

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_trailer`
  failed to compile (`E0432 unresolved import super::RunTotals`, three `E0061`: `run_section`
  takes 5 arguments), then 3/3. Mutation: writing the five provenance keys before
  `pinned_cache` fails `trailer_names_its_provenance` (Review focus 1).
- **Departures from the plan's text:**
  - `RunTotals`' fields are `pub(crate)`, not `pub`: every other `test_support` implementation
    module's struct fields are, and `pub` is for `test_support/mod.rs`.
  - `run_section`'s doc comment is rewritten to fit the ten-line cap with the new paragraph; it
    points at build-test.md "The tree" for the key definitions.
  - The prefill's stats: `prefill.as_ref().map_or(PrefillStats::default(), |p| p.stats)`. A
    refused prefill (`DoesNotFit`, `Io`) panics at the top of `measure`, so only `Ok` and `Off`
    reach the trailer.
  - The "Printed until there is a file to write it to" `println!` stays: it is not in the way.
- **Plan drift, another task's.** pinned-loads Task 7 (`pinned-loads-impl.md`, its Consumes line
  and Steps reading `[pinned-cache] <case>:`) reads the per-case stderr line this task removes. On
  this chain the case's cache state is the trailer's `pinned_cache=` line instead; the
  `[pinned-cache] prefill:` and `[load-bytes]` lines stay.
- **Gpu shape.** `cargo test --no-run -p peacockdb-core --features gpu --lib --tests` in the
  25.02 build image (`CARGO_TARGET_DIR=target-img-25.02`): exit 0, no warning, after Task 1
  (cold, 13 min 37 s) and after Task 2 (incremental, 1 min 21 s). `target-img-25.02/` and
  `.home-img/` removed afterwards.
- **Unrun (needs a device):** a benchmark section with the new trailer from a real run: Task 5.
- **Gates:** `--lib` 934 passed, 2 ignored (936); `test_corpus_goldens` 47, the H200 tree
  included; `cost-report --test test_source_formatting` 32. No warning.

## Task 3: one reader for the tree, and the checks on it

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_tree`
  failed to compile (`E0432 unresolved import crate::test_support::parse_benchmark_section`),
  then 6/6. `--test test_corpus_goldens -- benchmark` failed to compile (`E0425 rows_problem`, three
  sites), then the binary 49/49, the H200 tree included. Mutation: `rows_problem` skipping the
  node loop fails `a_section_without_batches_outside_h200_is_refused` (it then names `load_us`).
  Over a scratch `PEACOCK_TESTDATA_DIR` holding the H200 tree plus the reader's `NEW` section
  under `h100-80gb-hbm3/`: green as written; red, each naming the section, on `load_us` off by
  one, `execute_us` off by one, `peak_bytes` below a batch, `batch_bytes` one entry short, and
  `prefill_us` deleted (`carries no prefill_us`).
- **A fixture the strict reader caught.** `benchmark_run.rs`'s `timed()` ("a timed section's body
  as the harness writes one") opened with `0.0 CudfScan`, a line no renderer writes;
  `device_tree_problem` used to split at `--- run ---` without reading the tree. Now it reads
  through `parse_benchmark_section`, which panicked "line under no node". The fixture's node line
  is `GpuUnload` now; the reader is right.
- **Departures from the plan's text:**
  - `LOAD_NODE` moved from `plan_text/bench_text.rs` to `plan_text/mod.rs`, so
    `BenchSection::tree_load_us` compares with the renderer's constant, not a second literal.
  - Beyond the plan's five unit tests: `a_trailer_key_read_twice_is_refused` (`field` reads
    every match; coding-style's "reader that stops at the first match"). The reader also
    refuses a node with no time line, a second time line under one node, and a time line
    missing `time_us` or `total_us`.
  - `rows_problem` checks the seven `ROWS_KEYS` (the plan's list) and names the node for a
    missing batch key (`carries no batch_rows under GpuUnload`).
  - `every_total_us_is_the_sum_of_the_time_us_beside_it` names the node, not the file line, in
    its messages: the reader keeps no line numbers.
  - **`benchmark.rs` split.** With the new checks it reached 1050 lines. The calibration record's
    three format tests moved verbatim to `tests/test_corpus_goldens/benchmark_record.rs` (149
    lines), and `benchmark.rs` is 908 lines.
- **Step 7, the other readers:** none needs a change. `git grep -n "time_us\|benchmark.txt" --
  scripts benchmarks`: `build-test.sh` and `build-test-shadgpu.sh` count `== ` headers and
  `.benchmark.txt` files; `scripts/calibration/` reads the record, not the tree (its `device_us`
  hits are nsys columns).
- **Gates:** `--lib` 940 passed, 2 ignored (942); `test_corpus_goldens` 49;
  `cost-report --test test_source_formatting` 32. No warning.

## Task 4: the benchmark report page

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_report`
  failed to compile (`E0432 unresolved imports ReportCell, ReportTable, render_benchmark_report,
  section_href`), then 12/12. Mutations: the section link one line early fails
  `a_timed_section_gives_its_load_execute_total_and_peak`; `<` left unescaped fails
  `text_is_escaped`. `the_benchmark_report_matches_the_trees` with no page: FAIL, "No such file
  … write the page with UPDATE_CANONICAL=1 cargo test …"; with `UPDATE_CANONICAL=1`: PASS, page
  written; without: PASS. A second `UPDATE_CANONICAL=1` run left the page byte-identical (sha256
  `bd348f1a…` both times). A stale page (the intro reworded after writing it) failed naming
  `line 21, column 42` and the command (Review focus 6).
- **The page from the committed trees:** 62 lines, the H200 alone: tpch sf40 22 rows, tpcds sf40
  4 rows (the 26 cases of `h200/cases.csv`), every mode, loads derived (`tree_load_us()`), peaks
  `—`, no `/home` or `/tmp` in it. Rendered once over a scratch root (the H200 tree plus the
  reader's `NEW` section as `h100-80gb-hbm3/tpch.sf40/tp1_single` q6): the two devices side by
  side, the H100's q6 with a peak (`0.00` GiB at 4096 bytes), its missing rowgroup cell `—`.
  Not opened in a browser (offline, no display); read as text.
- **Departures from the plan's text:**
  - The page is written by `write_benchmark_report(text)` (`test_support/mod.rs`), which stages
    and renames through `testdata::rewrite`, now `pub(crate)`; the path stays in one place.
  - The page's test lives in `benchmark.rs` as planned (936 lines after it).
  - Two tests beyond the plan's ten: `a_marker_shows_its_reason` (the `SKIPPED` cell) and
    `an_extra_table_follows_the_datasets` (`ReportTable`, which no caller uses yet).
  - **A timed section always shows its numbers.** The list's `skipped` note replaces a failure,
    a marker or an absence, not a measured section: a skipped row beside a timed section is a
    stale list, and hiding a measurement on its word would lose data silently.
  - `wrong answer #NN` goes after the total, in `.missing` red, on rows whose mode the `wrong`
    row lists. It is not linked to its ticket: the page has no ticket index.
  - Queries match the list with `_` → `-` (`aggregate_groupby` ↔ `aggregate-groupby`); the row
    shows the section's name.
  - A mode file whose name is not one of `MODES` is ignored, so a directory holding only
    such files is no column. `devices` and `files_of` read directories in sorted order.
  - `<style>`: `report.html`'s `body`, `table`, `th, td`, `h2`, `a`, `.missing`, `.source` and
    its `td.g` colours as `tr td.off|skip|na` (the `tr` outranks `tr:nth-child(even) td`, which
    would otherwise paint an even row's failure white), plus `td.n` right-aligned numbers.
    `th`'s `position: sticky` is dropped: two header rows would stack on each other.
- **Gates:** `--lib` 952 passed, 2 ignored (954); `test_corpus_goldens` 50;
  `cost-report --test test_source_formatting` 32.

## Task 5: one case on the H100, end to end (2026-11-05)

- **The case:** `bench_tpch_sf40_q3_tp1_single`. The registry enables q3 at `gpu_tp1_single`, and its
  plan has three loaders (customer, orders, lineitem) under two joins, so no substitute was needed.
- **Build:** the CPU-VM build as `build-test.md` "The CPU-VM build" gives it, from this worktree
  with Tasks 1-4 in it: `check-build-image.sh` passed, `stage-install.sh` exit 0, no warnings.
  **Departure:** the install came home to `~/.cache/peacock/bench-rows-install/`, not this tree's
  `cpp/install/`, and went out with `ship --install` on that path. The local `cpp/install/` serves
  the local gpu builds and was left alone.
- **Run:** `G="scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"; $G prepare && $G ship --install ~/.cache/peacock/bench-rows-install && PCK_TEST_FILTER=bench_tpch_sf40_q3_tp1_single $G run bench && $G pull`.
  Then the VM's `records.tsv` was removed and the H100 stopped. `prepare` exit 0 (11 min; nothing
  to fetch). The case passed in 10 s, and `run-tests.sh bench` was green. stderr:
  `[pinned-cache] prefill: cases=1 files=3 bytes=12051239504 us=6306272 needed=12051239504 limit=158302439424`
  and `[load-bytes] tpch/q3 at tp1-single on a device: compressed=1694871140 decoded=6603217369`.
- **`ship` is slow from dev:** about 300 KB/s up, so the 650 MB install took 20 min. That was most
  of the H100's 35 min. An unchanged install ships in seconds (rsync), so ship once per build.
- **The pulled file held four sections, not one.** The VM's
  `testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt` still had q1, q6
  and q8 from pinned-loads' runs (an older binary: no batches, no `load_us`), and `pull` brought
  them home with the new q3. So the plan's whole-file grep counted 68 `time_us` lines against 16
  `batch_bytes`. Over the q3 section alone: 16 = 16, and all seven lines, as below. Over the whole
  file, `every_section_outside_h200_carries_batches_and_the_load` refuses it, naming
  `q1: carries no batch_rows under GpuUnload` — the check doing its job on an old binary's output.
  **For sf40-baseline's full H100 run:** those three old sections carry this host's own five
  provenance keys, so a resumed run (`PEACOCK_BENCH_RESUME=1`) would keep them as measured. Remove
  the VM's `h100-80gb-hbm3/` tree first, or run without resume.
- **Read back** (q3 only, in a scratch `PEACOCK_TESTDATA_DIR`):
  `every_total_us_is_the_sum_of_the_time_us_beside_it` and
  `every_section_outside_h200_carries_batches_and_the_load` both pass. `load_us` 203448 is the
  three loaders' 35540 + 45060 + 122848. `load_us + execute_us` = `device_us` = 249119.
  `peak_bytes` 5948138877 (5.54 GiB) is at least the largest batch (lineitem's 5759202908). The
  page rendered once over the scratch tree (`UPDATE_CANONICAL=1`) shows the H100 column: q3
  tp1_single, load 203.4, execute 45.7, total 249.1 ms (linked to the section), peak ≥ 5.54 GiB.
- **Clean-up:** both VMs `STOPPED`. The h100 file is removed from the checkout, and `git status`
  shows nothing under `testdata/benchmark-results/`. The pulled file and the run log are kept
  outside git in `~/.cache/peacock/bench-rows-task5/`.
- **The section** (each `partition_groups` list elided to its first and last group):

```
== q3
GpuUnload
  time_us=[[118]] total_us=118 batch_rows=[[10]] batch_bytes=[[328]]
  GpuAccumulateBatchesAndSort: by=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last], fetch=10, lanes=1, batches=single, sorted_on=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last]
    time_us=[[152,20]] total_us=172 batch_rows=[[10]] batch_bytes=[[328]]
    GpuSort: by=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last], fetch=10, lanes=1, batches=single, sorted_on=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last]
      time_us=[[636]] total_us=636 batch_rows=[[10]] batch_bytes=[[328]]
      GpuProject: exprs=[l_orderkey@0, `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3 as revenue, o_orderdate@1, o_shippriority@2], lanes=1, batches=single
        time_us=[[2]] total_us=2 batch_rows=[[453784]] batch_bytes=[[14747980]]
        GpuAggregateBatches: group_by=[l_orderkey@0, o_orderdate@1, o_shippriority@2], aggs=[sum(`sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3) as `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`], final=[`sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3], lanes=1, batches=single
          time_us=[[0,583]] total_us=583 batch_rows=[[453784]] batch_bytes=[[14747980]]
          GpuAggregate: group_by=[l_orderkey@2, o_orderdate@0, o_shippriority@1], aggs=[sum(l_extendedprice@3 * (1 - l_discount@4)) as `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`], lanes=1, batches=multiple
            time_us=[[1646,0]] total_us=1646 batch_rows=[[453784]] batch_bytes=[[14747980]]
            GpuHashJoin: join_type=Inner, on=[(o_orderkey@0, l_orderkey@0)], projection=[o_orderdate@1, o_shippriority@2, l_orderkey@3, l_extendedprice@4, l_discount@5], lanes=1, batches=multiple
              time_us=[[436,24241,0]] total_us=24677 batch_rows=[[1198277]] batch_bytes=[[58266221]]
              GpuCoalesceAllBatches: lanes=1, batches=single
                time_us=[[0,75]] total_us=75 batch_rows=[[5835860]] batch_bytes=[[95562209]]
                GpuHashJoin: join_type=Inner, on=[(c_custkey@0, o_custkey@1)], projection=[o_orderkey@1, o_orderdate@3, o_shippriority@4], lanes=1, batches=multiple
                  time_us=[[96,7084,0]] total_us=7180 batch_rows=[[5835860]] batch_bytes=[[95562209]]
                  GpuCoalesceAllBatches: lanes=1, batches=single
                    time_us=[[0,18]] total_us=18 batch_rows=[[1200419]] batch_bytes=[[9753405]]
                    GpuFilter: predicate=c_mktsegment@1 = BUILDING, projection=[c_custkey@0], lanes=1, batches=multiple
                      time_us=[[519]] total_us=519 batch_rows=[[1200419]] batch_bytes=[[9753405]]
                      GpuLoadParquet: table=customer, projections=[c_custkey@0, c_mktsegment@6], partition_groups=[[[0,…,48]]], lanes=1, batches=multiple
                        time_us=[[35540]] total_us=35540 batch_rows=[[6000000]] batch_bytes=[[127501181]]
                  GpuFilter: predicate=o_orderdate@2 < 1995-03-15, lanes=1, batches=multiple
                    time_us=[[1674]] total_us=1674 batch_rows=[[29150056]] batch_bytes=[[714176372]]
                    GpuLoadParquet: table=orders, projections=[o_orderkey@0, o_custkey@1, o_orderdate@4, o_shippriority@7], partition_groups=[[[0,…,237]]], lanes=1, batches=multiple
                      time_us=[[45060]] total_us=45060 batch_rows=[[29245440]] batch_bytes=[[716513280]]
              GpuFilter: predicate=l_shipdate@3 > 1995-03-15, projection=[l_orderkey@0, l_extendedprice@1, l_discount@2], lanes=1, batches=multiple
                time_us=[[8371]] total_us=8371 batch_rows=[[129398483]] batch_bytes=[[5224463753]]
                GpuLoadParquet: table=lineitem, projections=[l_orderkey@0, l_extendedprice@5, l_discount@6, l_shipdate@10], partition_groups=[[[900,…,1953]]], lanes=1, batches=multiple
                  time_us=[[122848]] total_us=122848 batch_rows=[[129420290]] batch_bytes=[[5759202908]]
--- run ---
run_us=249880
device_us=249119
load_us=203448
execute_us=45671
peak_bytes=5948138877
runs=[265479,251120,251474,251307,250587,249880,250096,249202,250998,250099]
build=release
allocator=rmm-pool initial=69.0GiB max=69.0GiB of 78.7GiB free on a discrete device
pinned_cache=on cap=147.4GiB resident=11.2GiB files=3 fills=3 fill_us=6306227 fallbacks=0 evictions=0 reloads=0 source=default
prefill_files=3
prefill_bytes=12051239504
prefill_us=6306272
device=NVIDIA H100 80GB HBM3
driver=580.173.02
cuda=12.2
cudf=25.02.02
host=computeinstance-e00e6yena4nzjgaym0
```

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
| nebius-cpu128 | 2026-11-05 07:11 | 07:18 | 0.12 | Task 5: the CPU-VM build |
| nebius-h100 | 2026-11-05 07:18 | 07:53 | 0.58 | Task 5: prepare, ship (20 min of upload), one case, pull |

## Blocked (2026-11-04)

`blocked(building)`: Task 5 (one case on the H100, end to end) needs the H100 path, which waits on
new-hosts' block (the human's GitHub PAT; `new-hosts-detail.md`, "Blocked"); the same obstacle the
analyst diagnosed there. Tasks 1–4 are committed (`d0866f27`, `bcefeab4`). Resume at Task 5 once
new-hosts' Task 10 has proven the H100 path and pinned-loads is through its H100 runs.

## Rebase onto master (2026-11-04)

Rebased onto ENS-pinned-loads, which is on master (the ENS-optimizer merge #165 and the
inventory-based case listing). HEAD `1bdc89f3`.

- **`benchmark.rs` conflict (bcefeab4's replay):** the import list. Kept the commit's, without
  `macro_invocations_in`, which master removed. The record tests' imports went with the tests to
  `benchmark_record.rs`. `the_benchmark_report_matches_the_trees` took its row order from
  `macro_invocations_in(CASE_LIST, …)`. It now takes it from `registry_cases()`. The list is
  written from the registry in that order, and `the_benchmark_cases_follow_the_registry` holds
  them byte for byte, so the order is the list's without reading the list's text. The committed
  page renders unchanged.
- **`build-test.md`:** the replay kept HEAD's side of two hunks. Restored by meaning: the cpu
  header is master's plus bench-rows' 25 cases (`--lib` 952, `test_cpu_corpus` 2447,
  `test_corpus_goldens` 50, `test_cost_model` 3 = 3452, each the binary's `--list` total), and the
  grand total moves by the same 25 (7755; Rust 6001). The `benchmark-report.html` golden row is back
  beside HEAD's four new rows; it and **The page** now say the order comes from the registry.
- **Re-proven offline, no VM:** `cargo test --features rust-only -p peacockdb-core` exit 0 —
  `--lib` 950 passed, 2 ignored; `test_cpu_corpus` 2447; `test_corpus_goldens` 50;
  `test_cost_model` 3; `test_golden_format` 41; `test_crate_privacy` 1; no warnings; `testdata/`
  clean after it. `cargo test -p cost-report`: 49 + `test_source_formatting` 32. `--features gpu`
  compiles every target against the local rapids-cuda-12.2 env with no warnings. No C++ changed,
  so no ctest.

- **Second rebase (2026-11-05)** onto pinned-loads' tip `652f521e` (the pinned host cache;
  `[load-bytes] compressed=` now the footer's compressed size; `run-gpu.sh` requires
  `--instance-id`). No code conflict. `build-test.md`: three hunks. The case's stderr is one
  `[load-bytes]` line, with the cache's counters in the trailer (the code: `corpus_benchmark.rs`,
  `benchmark_trailer.rs`), in pinned-loads' "footer's compressed size" wording. Recount: cpu 3453
  (`--lib` 953, `test_cpu_corpus` 2447, `test_corpus_goldens` 50, `test_cost_model` 3), grand
  total 7771 (Rust 6002, C++ 278, Python 1491). `architecture.md` merged cleanly and still agrees.
  Re-proven: `cargo test --features rust-only -p peacockdb-core` exit 0 (`--lib` 951 passed,
  2 ignored; `test_cpu_corpus` 2447; `test_corpus_goldens` 50), with `testdata/` clean afterwards;
  `cargo test -p cost-report` 49 + 32.

## Blocked (2026-11-04, after the rebase)

Still `blocked(building)` at Task 5: the H100 run waits on new-hosts' new block, the two VMs'
disks (`new-hosts-detail.md`, "Blocked (2026-11-04, second)"); the PAT block is cleared.
