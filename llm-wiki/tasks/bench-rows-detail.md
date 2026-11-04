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

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|

## Blocked (2026-11-04)

`blocked(building)`: Task 5 (one case on the H100, end to end) needs the H100 path, which waits on
new-hosts' block (the human's GitHub PAT; `new-hosts-detail.md`, "Blocked"); the same obstacle the
analyst diagnosed there. Tasks 1–4 are committed (`d0866f27`, `bcefeab4`). Resume at Task 5 once
new-hosts' Task 10 has proven the H100 path and pinned-loads is through its H100 runs.
