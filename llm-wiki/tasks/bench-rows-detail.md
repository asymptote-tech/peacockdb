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

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
