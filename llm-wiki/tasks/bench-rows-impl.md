# bench-rows implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every timed node in a `.benchmark.txt` section carries its `batch_rows` and
`batch_bytes`, and the `--- run ---` trailer carries `load_us`, `execute_us` and `peak_bytes`,
with one parser that reads both the new sections and the committed H200 ones; and a committed
page, `testdata/benchmark-report.html`, that shows every device tree side by side through that
parser.

**Architecture:** the renderer already holds the chosen run's `RunReport`; it gains the
execution golden's per-batch notation by sharing `run_text`'s list renderer through
`node_text`. The trailer's three new numbers come from the same run: `load_us` from the tree's
`GpuLoadParquet` totals under `bench_text`'s own rule, `peak_bytes` from `RunReport::peak_bytes`
(the accountant's peak, no driver change). A new `test_support` reader, modelled on
`golden_text::parse_run_section`, replaces the ad-hoc parsing in `test_corpus_goldens/benchmark.rs`
and is what query-fuser and batching-report read the tree through. The report page is rendered
by `test_support::benchmark_report` from that reader and the device lists' `cases.csv`
(containers-rebase's `parse_device_cases`); a `test_corpus_goldens` test holds the committed
page to the render, as `the_benchmark_cases_follow_the_registry` holds the case list.

**Tech stack:** Rust (`peacockdb-core`: `plan_text`, `test_support`, the `test_corpus_goldens`
binary). No C++, no Python change.

**Spec:** [`bench-rows.md`](bench-rows.md).

**Code base.** Branch from chain L's previous task (`ENS-containers-master` and the tasks above
it). Line numbers below are `origin/ENS-containers`'s for the harness files
(`corpus_benchmark.rs`, `benchmark_trailer.rs`, `benchmark_section.rs`,
`tests/test_corpus_goldens/benchmark.rs`) and master's for the rest; on the branch they shift,
so find each by the function named beside it.

## Global Constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- The node line is "in the same notation as `.cpu.txt`": `batch_rows=[[…],[…]]` and
  `batch_bytes=[[…],[…]]`, by this node's output lane, then by batch — `run_text`'s rendering,
  shared, not copied.
- Taken from the reported run (the 2nd-smallest), the same `Run` the tree's `time_us` comes from.
- `load_us` = Σ `total_us` of the `GpuLoadParquet` nodes; `execute_us` = `device_us − load_us`.
- `peak_bytes` = the most bytes the run's memory accountant held at once.
- The existing H200 files still parse. Re-measuring them is not required.
- The trailer still **ends** with the five provenance keys: `holds_measured`
  (`benchmark_section.rs:61-79`) keeps a resumed section only when its trailer `ends_with` them.
- `build-test.md` documents the format; `architecture.md` "Node display" keeps agreeing with the
  code (Shared rules: every commit keeps code and wiki in agreement).
- The report page: `testdata/benchmark-report.html`; written only by
  `the_benchmark_report_matches_the_trees` under `UPDATE_CANONICAL=1`; no timestamp, no machine
  path, so the same trees always give the same bytes; `peak_bytes` labelled "accountant's peak,
  a lower bound on device memory"; the look of `testdata/report.html`.
- Gates: `cargo test -p peacockdb-core --features rust-only --lib` and
  `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens` green at every
  commit; the cost-report crate unaffected.

## The format this task defines (query-fuser and batching-report parse this)

A timed section, one node shown:

```
== q3
GpuUnload
  time_us=[[149]] total_us=149 batch_rows=[[10]] batch_bytes=[[480]]
  GpuLoadParquet: table=lineitem, projections=[…], partition_groups=[[[0,1,…]]], lanes=1, batches=multiple
    time_us=[[812,790]] total_us=1602 batch_rows=[[6001215,5998785]] batch_bytes=[[96019440,95980560]]
--- run ---
run_us=162690
device_us=162476
load_us=101308
execute_us=61168
peak_bytes=2147483648
runs=[168555,167161,…]
build=release
allocator=rmm-pool initial=69.0GiB …
device=NVIDIA H100 80GB HBM3
driver=…
cuda=…
cudf=…
host=…
```

- **Node line**: unchanged. **The line beneath it**: space-separated `key=value` tokens, in this
  order: `time_us`, `total_us`, `batch_rows`, `batch_bytes`. No value contains a space.
- `time_us` is by **driving lane × call**; `batch_rows`/`batch_bytes` are by **output lane ×
  emitted batch**. They are different axes: an accumulator makes many calls and emits one batch;
  a source makes one call per batch. A lane that emitted nothing is `[]`.
- **Trailer keys, in order**: `run_us`, `device_us`, `load_us`, `execute_us`, `peak_bytes`,
  `runs`, `build`, `allocator`, `device`, `driver`, `cuda`, `cudf`, `host`. Values are integers
  except `runs` (`[a,b,…]`), `build`, `allocator` and the five.
- `peak_bytes` is the accountant's peak of its resident figure: driver-held batches plus every
  live executor's `resident_bytes()`, observed at every hold and every call's end
  (`driver/accounting.rs:116-212`). It is exact for that figure. As **device memory** it is a
  lower bound: it excludes a call's transient scratch (no GPU executor measures it —
  `CallStats::scratch_bytes` is `None` throughout `gpu_backend/`), cuDF and rmm temporaries
  inside a call, pool fragmentation and the parquet reader's staging. It is at least every
  `batch_bytes` entry in its section, since every emitted batch is held first.
- **Legacy sections** (the committed `h200/` tree): the line beneath a node is only
  `time_us=… total_us=…`, and the trailer has no `load_us`, `execute_us` or `peak_bytes`. The
  reader returns `None` for each; it never derives `peak_bytes`. `load_us` can be derived from
  a legacy tree with `BenchSection::tree_load_us()`.

## Review Focus

1. **A trailer whose new keys land after the provenance** — a resumed run then measures every
   case again, silently, because `holds_measured` no longer matches. → Task 2,
   `trailer_names_its_provenance` pins the full key order, and Task 3's
   `a_new_section_with_its_provenance_last_is_read` reads it back.
2. **The committed H200 tree, which has none of the new fields** — must parse and pass every
   arithmetic check; must not be required to carry them. → Task 3,
   `every_total_us_is_the_sum_of_the_time_us_beside_it` over the committed tree, and
   `a_legacy_section_reads_with_no_batches_and_no_load`.
3. **A section written after this task without the fields** (an old binary shipped to the H100)
   — refused, naming the field, outside `h200/`. → Task 3,
   `a_section_without_batches_outside_h200_is_refused`.
4. **A plan with several `GpuLoadParquet` nodes, or none** (a join reads two tables; `GpuEmpty`
   answers from no file) — `load_us` is the sum over all of them, and 0 when there are none,
   never the first one's. → Task 1, `the_load_is_every_loaders_total`; Task 3,
   `a_tree_with_no_loader_loads_nothing`.
5. **A node with an empty output lane, or a routing node whose lanes the driver answered** —
   `batch_rows` keeps the `[]` lane rather than dropping it, so lanes still line up index for
   index with `.cpu.txt`. → Task 1, `a_timing_record_carries_the_execution_goldens_batches`
   compares the tokens against `render_run`'s for the same run, node by node.
6. **A results tree changed and the page not regenerated** — the page silently shows old
   numbers. → Task 4, `the_benchmark_report_matches_the_trees` fails naming the command.
7. **A failure reason holding `<` or `&`** (an error text) breaks the page's markup. → Task 4,
   `text_is_escaped`.

---

### Task 1: the node line carries `batch_rows` and `batch_bytes`; the tree's load total

**Files:**
- Modify: `peacockdb-core/src/plan_text/node_text.rs` — gains the list renderer
- Modify: `peacockdb-core/src/plan_text/run_text.rs:103-116` — `lanes_of`/`numbers` move out
- Modify: `peacockdb-core/src/plan_text/bench_text.rs` (whole file, 104 lines)
- Modify: `peacockdb-core/src/plan_text/mod.rs:45-54` — the two wrappers
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs:248-260` — the caller
- Test: `peacockdb-core/src/executor/driver/tests/render.rs` (the existing render tests, from line ~177)
- Docs: `llm-wiki/architecture.md` "Node display" (line ~1398), `llm-wiki/build-test.md`
  "Corpus benchmarks", **The tree** paragraph (the `time_us=[[22,37,40],[55,1,30]]` example)

**Interfaces:**
- Consumes: `RunReport::emitted: Vec<Vec<Vec<EmittedBatch>>>` (`executor/mod.rs:661`),
  `EmittedBatch { rows: u64, bytes: usize }` (`executor/mod.rs:624`).
- Produces:
  - `pub(super) fn lanes_of<T>(lanes: &[Vec<T>], of: impl Fn(&T) -> u64) -> String` and
    `pub(super) fn numbers(values: &[u64]) -> String` in `node_text.rs` (moved verbatim from
    `run_text.rs`; `run_text` calls them from there).
  - `pub(crate) fn render_timings(root: &dyn GpuNode, times: &Measurements, report: &RunReport) -> String`
    (in `bench_text.rs` and its `plan_text/mod.rs` wrapper). `bench_text`'s own time-list
    helper is renamed `time_lanes` so it does not shadow the shared `lanes_of`.
  - `pub(crate) fn timed_load_us(root: &dyn GpuNode, times: &Measurements) -> u64` (both files):
    Σ `total_of(times, node)` over nodes whose `name()` is `"GpuLoadParquet"`, walked through
    `PlanIndex::build(root)` as `render_timings` does. Named constant
    `pub(crate) const LOAD_NODE: &str = "GpuLoadParquet";` in `bench_text.rs`.

- [ ] **Step 1: Write the failing tests** in `driver/tests/render.rs`

```rust
/// The bench line's batch tokens are the execution golden's, node for node.
#[test]
fn a_timing_record_carries_the_execution_goldens_batches() {
    let script = Script::default().source("part", vec![vec![spec(10, 80), spec(7, 56)]]);
    let plan = unload(filter(source("part", 1)));
    let report = run(plan.as_ref(), &script);
    let times = join_regions(&report, &[]).expect("nothing journalled");
    let bench = render_timings(plan.as_ref(), &times, &report);
    let exec = render_run(plan.as_ref(), &report);
    // the `batch_rows=… batch_bytes=…` pair of each node's second line, in file order
    assert_eq!(batch_tokens(&bench), batch_tokens(&exec));
    assert_eq!(batch_tokens(&bench).len(), 3, "{bench}");
}

#[test]
fn the_load_is_every_loaders_total() {
    // unload(join(source("a", 1), source("b", 1))) from plans.rs: pre-order 0 unload,
    // 1 join, 2 and 3 the loaders. Regions journalled on both loaders and on the join, as
    // `a_region_that_rounds_to_nothing_still_ran` journals the filter's (load_a=40,
    // load_b=25, join_us=7):
    assert_eq!(timed_load_us(plan.as_ref(), &times), 40 + 25);
    assert_eq!(timed_device_us(&times) - timed_load_us(plan.as_ref(), &times), 7);
}
```

`batch_tokens(text) -> Vec<String>` is a local helper: for every line holding `batch_rows=`, the
substring from `batch_rows=` up to the end of the `batch_bytes=…` token (an `.cpu.txt` second
line has `abandoned`/`rows_skipped` after it on early-exit runs only). Also update
`a_timing_record_renders_the_tree_and_what_each_node_cost`'s expected text: each `time_us` line
gains ` batch_rows=… batch_bytes=…` with the values `render_run` prints for the same run (the
source's are `batch_rows=[[10,7]] batch_bytes=[[80,56]]`), and add `&report` to every existing
`render_timings` call in the file.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p peacockdb-core --features rust-only --lib -- driver::tests::render`
Expected: compile errors (`render_timings` takes 2 arguments; no `timed_load_us`).

- [ ] **Step 3: Implement.** Move `lanes_of`/`numbers` to `node_text.rs`; `bench_text::render_node`
  writes `time_us=… total_us=… batch_rows=… batch_bytes=…` on one line using
  `lanes_of(&report.emitted[node], |b| b.rows)` and `|b| b.bytes as u64`; add `timed_load_us`
  and `LOAD_NODE`. Replace `bench_text.rs`'s module doc paragraph "Rows and bytes are
  deliberately absent…" with one saying they are the chosen run's, on the time line, and why
  (query-fuser sizes concurrency from them). In `corpus_benchmark.rs:252` pass `&chosen.report`.

- [ ] **Step 4: Run to see them pass**

Run: `cargo test -p peacockdb-core --features rust-only --lib -- driver::tests::render plan_text`
Expected: PASS. Then the whole `--lib` gate: PASS.

- [ ] **Step 5: Docs.** `architecture.md` "Node display": a benchmark line has
  `time_us=[[…]] total_us=N batch_rows=[[…]] batch_bytes=[[…]]` beneath it, the first by lane
  and call, the last two by lane and batch as in the execution line. `build-test.md` **The
  tree**: the example line and one sentence on the two axes. Update `build-test.md`'s
  `executor::driver::tests` count (line ~362) and the `--lib` total in the cpu heading.

- [ ] **Step 6: Commit**

```bash
git add peacockdb-core/src/plan_text peacockdb-core/src/executor/driver/tests/render.rs \
        peacockdb-core/src/test_support/corpus_benchmark.rs llm-wiki/architecture.md llm-wiki/build-test.md
git commit -m "bench-rows: each benchmark node carries the chosen run's batch rows and bytes"
```

### Task 2: the trailer carries `load_us`, `execute_us` and `peak_bytes`

**Files:**
- Modify: `peacockdb-core/src/test_support/benchmark_trailer.rs:13-40`
- Test: `peacockdb-core/src/test_support/benchmark_trailer/tests.rs`
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs:248-260` (`measure`)
- Docs: `llm-wiki/build-test.md` **The tree** paragraph (the `--- run ---` sentence) and the
  "Benchmark trailer" test row (line ~465)

**Interfaces:**
- Consumes: Task 1's `timed_load_us`, `timed_device_us`; `RunReport::peak_bytes: usize`
  (`executor/mod.rs:639`, `pub(crate)`, so reachable from `test_support`).
- Produces, in `benchmark_trailer.rs`:

```rust
/// What the chosen run cost, as the trailer reports it.
pub(crate) struct RunTotals {
    pub run_us: u64,
    pub device_us: u64,
    pub load_us: u64,
    pub peak_bytes: u64,
}
pub(crate) fn run_section(totals: &RunTotals, spread: &[u64], allocator: &str, provenance: &Provenance) -> String;
```

`run_section` writes `execute_us = device_us − load_us` (plain subtraction: `load_us` is a
subset of the same sum, so it cannot exceed it).

- [ ] **Step 1: Write the failing tests** in `benchmark_trailer/tests.rs`

```rust
#[test]
fn trailer_names_its_provenance() {          // existing; the key list grows
    let totals = RunTotals { run_us: 7, device_us: 5, load_us: 3, peak_bytes: 4096 };
    let trailer = run_section(&totals, &[7, 6, 9], "rmm-pool initial=69.0GiB", &provenance);
    assert_eq!(keys, ["run_us", "device_us", "load_us", "execute_us", "peak_bytes", "runs",
                      "build", "allocator", "device", "driver", "cuda", "cudf", "host"]);
    // the five value lines, and starts_with("--- run ---\n"), as today
}

#[test]
fn execute_is_the_device_time_outside_the_loads() {
    let trailer = run_section(&RunTotals { run_us: 9, device_us: 8, load_us: 5, peak_bytes: 1 }, …);
    for line in ["load_us=5", "execute_us=3", "peak_bytes=1"] {
        assert!(trailer.lines().any(|l| l == line), "no {line:?} in {trailer}");
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_trailer`
Expected: compile error, no `RunTotals`.

- [ ] **Step 3: Implement** `RunTotals` and the new `run_section`. Its doc gains one paragraph on
  `peak_bytes`: the accountant's peak, a lower bound on device memory, and what it leaves out
  (the list in "The format" above, in two lines). In `corpus_benchmark.rs::measure`, build
  `RunTotals { run_us: chosen.total_us, device_us: timed_device_us(&costed),
  load_us: timed_load_us(tree.as_ref(), &costed), peak_bytes: chosen.report.peak_bytes as u64 }`.
  Drop the stale "Printed until there is a file to write it to" `println!` only if it is in the
  way; otherwise leave it.

- [ ] **Step 4: Run to see them pass, and compile the device-only caller**

Run: `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_trailer` → PASS.
Run: `cargo check -p peacockdb-core --features gpu --tests` (needs `cpp/install`; where the host
has none, Task 5's build is this check) → no error in `corpus_benchmark.rs`.

- [ ] **Step 5: Docs.** `build-test.md` **The tree**: the trailer sentence lists the keys in
  order, defines `load_us` and `execute_us`, and says `peak_bytes` is the accountant's peak and
  a lower bound on device memory, naming what it excludes. The "Benchmark trailer" row's
  description and count (2).

- [ ] **Step 6: Commit**

```bash
git add peacockdb-core/src/test_support/benchmark_trailer* peacockdb-core/src/test_support/corpus_benchmark.rs llm-wiki/build-test.md
git commit -m "bench-rows: the trailer splits load from execute and names the accountant's peak"
```

### Task 3: one reader for the tree, and the checks on it

**Files:**
- Create: `peacockdb-core/src/test_support/benchmark_tree.rs` (`mod benchmark_tree;` in
  `test_support/mod.rs`, no feature gate: the reader serves rust-only)
- Create: `peacockdb-core/src/test_support/benchmark_tree/tests.rs`
- Modify: `peacockdb-core/src/test_support/golden_text.rs:238-272` — `nested_numbers` becomes
  `pub(crate)`
- Modify: `peacockdb-core/src/test_support/mod.rs` — the `pub` types and function (beside
  `RunNode`/`parse_run_section`, master lines ~284-318)
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` — `run_trailer`,
  `key_values`, `trailer_field` (50-80) and the time-line loop in
  `every_total_us_is_the_sum_of_the_time_us_beside_it` (98-169) move onto the reader; new checks
- Docs: `llm-wiki/build-test.md` — the benchmark-tree sentence of the `test_corpus_goldens`
  description (line ~87) and its counts; the new `test_support::benchmark_tree::tests` row

**Interfaces:**
- Consumes: `parse_node_line`, `NodeLine<'a>` (`test_support/mod.rs:254-299`), `SKIPPED`,
  `FAILED`.
- Produces (declared in `test_support/mod.rs`, so `peacockdb_core::test_support::…`):

```rust
pub struct BenchNode<'a> {
    pub line: NodeLine<'a>,
    pub time_us: Vec<Vec<u64>>,
    pub total_us: u64,
    /// `None` in a section written before bench-rows.
    pub batch_rows: Option<Vec<Vec<u64>>>,
    pub batch_bytes: Option<Vec<Vec<u64>>>,
    pub children: Vec<usize>,
}
pub struct BenchSection<'a> {
    pub nodes: Vec<BenchNode<'a>>,          // file order, pre-order; nodes[0] is the root
    pub trailer: Vec<(&'a str, &'a str)>,   // `--- run ---` lines in order, split at the first '='
}
impl<'a> BenchSection<'a> {
    pub fn field(&self, key: &str) -> Option<&'a str>;   // every match read: a key twice panics
    pub fn count(&self, key: &str) -> Option<u64>;       // panics on a non-number, like NodeLine::count
    pub fn tree_device_us(&self) -> u64;                  // Σ total_us
    pub fn tree_load_us(&self) -> u64;                    // Σ total_us where line.name == "GpuLoadParquet"
}
/// `None` for a marker (`SKIPPED`) or a failed case's record (`FAILED`); panics on a line the
/// format does not allow, naming it, as `parse_run_section` does.
pub fn parse_benchmark_section(body: &str) -> Option<BenchSection<'_>>;
```

The line beneath a node: `split_whitespace`, each token `key=value`; keys `time_us`,
`total_us`, `batch_rows`, `batch_bytes`; any other key panics naming it.

- [ ] **Step 1: Write the failing unit tests** in `benchmark_tree/tests.rs`, over literal sections

```rust
#[test]
fn a_new_section_with_its_provenance_last_is_read() {
    let s = parse_benchmark_section(NEW).expect("timed");   // NEW: two loaders under a join, full trailer
    assert_eq!(s.nodes.len(), 4);
    assert_eq!(s.nodes[3].batch_rows, Some(vec![vec![6, 4]]));
    assert_eq!(s.nodes[3].batch_bytes, Some(vec![vec![48, 32]]));
    assert_eq!(s.nodes[0].children, [1]);
    assert_eq!(s.count("load_us"), Some(s.tree_load_us()));
    assert_eq!(s.count("execute_us"), Some(s.tree_device_us() - s.tree_load_us()));
    assert_eq!(s.count("peak_bytes"), Some(4096));
    assert_eq!(s.trailer.last(), Some(&("host", "nebius-h100")));
}
#[test]
fn a_legacy_section_reads_with_no_batches_and_no_load() {
    let s = parse_benchmark_section(LEGACY).expect("timed");  // the nested-loop-join section of h200/tpch.sf40/tp1_single, verbatim
    assert!(s.nodes.iter().all(|n| n.batch_rows.is_none() && n.batch_bytes.is_none()));
    assert_eq!((s.count("load_us"), s.count("peak_bytes")), (None, None));
    assert_eq!(s.tree_load_us(), 766 + 849);
}
#[test]
fn a_tree_with_no_loader_loads_nothing() {
    // GpuUnload over a GpuEmpty, new format: tree_load_us() == 0, load_us=0 reads as Some(0)
}
#[test]
fn a_marker_and_a_failure_are_not_timed() { /* SKIPPED…, FAILED… bodies → None */ }
#[test]
#[should_panic(expected = "unknown")]
fn an_unknown_token_beneath_a_node_is_refused() { /* `time_us=[[1]] total_us=1 rows=[[1]]` */ }
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_tree`
Expected: compile error, no `parse_benchmark_section`.

- [ ] **Step 3: Implement** the reader, the stack-of-open-nodes walk of `parse_run_section`
  (`golden_text.rs:195-236`); the trailer is everything after the `--- run ---` line.

- [ ] **Step 4: Run** the same command → PASS.

- [ ] **Step 5: Write the failing checks** in `tests/test_corpus_goldens/benchmark.rs`

  - `every_total_us_is_the_sum_of_the_time_us_beside_it` reads through
    `parse_benchmark_section` (the `is_timed` skip becomes its `None`); keeps every assertion it
    has (each `total_us` = Σ its `time_us`; `device_us` = `tree_device_us()`; `runs` has
    `MEASURED_RUNS` entries; `run_us` its 2nd-smallest). Where a section carries them, it adds:
    `load_us == tree_load_us()`; `load_us + execute_us == device_us`; `batch_rows` and
    `batch_bytes` the same shape on every node (same lane count, same length per lane);
    `peak_bytes >= ` every `batch_bytes` entry in the section.
  - `const PREDATES_ROWS: &[&str] = &["h200"];` beside `PREDATES_PROVENANCE` (line ~625), and
    `fn rows_problem(device: &str, body: &str, at: &str) -> Result<(), String>`: outside
    `PREDATES_ROWS`, a timed section with a node lacking `batch_rows` or `batch_bytes`, or a
    trailer lacking `load_us`, `execute_us` or `peak_bytes`, is refused with
    `"{at}: carries no {field}, and only {PREDATES_ROWS:?} were written before bench-rows"`.
  - `#[test] fn every_section_outside_h200_carries_batches_and_the_load()`: `rows_problem` over
    every timed section of the committed tree (passes today: only `h200/` exists).
  - `#[test] fn a_section_without_batches_outside_h200_is_refused()`: `rows_problem("h100-80gb-hbm3",
    <the committed h200 aggregate-groupby section>, "x")` is `Err` containing `"carries no batch_rows"`;
    the same body under `"h200"` is `Ok`.
  - `run_trailer`/`key_values`/`trailer_field` are replaced by the reader where they read a timed
    section; `device_tree_problem`'s failed-record branch (`key_values(body.lines().skip(1))`)
    keeps its own reading. Remove whatever is left unused.

- [ ] **Step 6: Run** `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens -- benchmark`
  Expected before the last edits: the new tests fail to compile; after: PASS, the H200 tree
  included, and every pre-existing test in the file still PASS.

- [ ] **Step 7: Check the other readers.** `git grep -n "time_us\|benchmark.txt" -- scripts benchmarks`
  — today only `build-test.sh`/`build-test-shadgpu.sh` count `== ` lines and `.benchmark.txt` files,
  and nothing in `scripts/calibration/` reads the tree (it reads the record). Expected: no change
  needed; say so in the detail file. A reader found there gets the new tokens accepted, with a test
  in its `tests/`.

- [ ] **Step 8: Docs.** `build-test.md`: the "benchmark tree gets the same reading" sentence adds
  the load and execute arithmetic, the batch shapes, `peak_bytes` against the batches, and that
  every tree but `h200/` must carry the fields; the new reader's test row; the
  `test_corpus_goldens` and `--lib` counts in the cpu heading.

- [ ] **Step 9: Commit**

```bash
git add peacockdb-core/src/test_support peacockdb-core/tests/test_corpus_goldens/benchmark.rs llm-wiki/build-test.md
git commit -m "bench-rows: one reader for the benchmark tree; new sections must carry batches and the load"
```

### Task 4: the benchmark report page

**Files:**
- Create: `peacockdb-core/src/test_support/benchmark_report.rs`, `…/benchmark_report/tests.rs`
  (`mod benchmark_report;` and the `pub use` in `test_support/mod.rs`, no feature gate)
- Create: `testdata/benchmark-report.html`
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` (the page's test)
- Docs: `llm-wiki/build-test.md` — the generated-files table (the `corpus_benchmark_cases.inc`
  row, containers' ~984) gains the page's row; "Corpus benchmarks", **The tree**: one paragraph
  on the page and the rule **a commit that changes a results tree regenerates the page; the
  test enforces it**; the `test_corpus_goldens` count and the new `test_support::benchmark_report::tests` row

**Interfaces:**
- Consumes: Task 3's `parse_benchmark_section`, `BenchSection::{count, tree_load_us}`;
  containers-rebase's `parse_device_cases`, `CaseStatus`, `DEVICE_CASES_HEADER`; `SKIPPED`,
  `FAILED`, `MODES`.
- Produces (declared in `test_support/mod.rs`; sf40-baseline and query-fuser extend it):

```rust
pub fn benchmark_report_path() -> PathBuf;   // testdata/benchmark-report.html
/// The page for the trees under `root` (a `benchmark-results/` directory). `order`: the main
/// case list's (dataset, query) pairs in file order. `extra`: tables later tasks add below the
/// per-dataset ones, in order (query-fuser's fused batches).
pub fn render_benchmark_report(root: &Path, order: &[(String, String)], extra: &[ReportTable]) -> String;
pub struct ReportTable { pub id: String, pub heading: String, pub columns: Vec<String>, pub rows: Vec<Vec<ReportCell>> }
pub struct ReportCell { pub text: String, pub href: Option<String>, pub class: Option<&'static str> } // "off" | "skip" | "na"
pub const REPORT_REPO: &str = "asymptote-tech/peacockdb";   // ticket_widget.py's DEFAULT_REPO
/// `https://github.com/{REPORT_REPO}/blob/master/testdata/{rel}#L{line}`, as `glyph_url` writes links.
pub fn section_href(rel: &str, line: usize) -> String;
```

**The page's rules** (the module doc says them):
- **Devices:** every directory directly under `root` that holds at least one
  `<ds>.sf<sf>/<mode>.benchmark.txt`, by name. A tree of another engine's files, or a `fused/`
  directory, is no column.
- **Rows:** one per (dataset, sf, query, mode) that any device has a section for; a heading per
  (dataset, sf). Order: datasets and queries as `order` first names them (a query absent from it
  after them, by name), sf numerically, modes in `MODES` order.
- **A timed cell** (`parse_benchmark_section` gives `Some`): total = trailer `device_us`; load =
  `load_us`, or `tree_load_us()` in a section written before this task (the same sum, so exact);
  execute = `execute_us`, or total − load; peak = `peak_bytes`, or `—` (never derived). ms with
  one decimal, peak in GiB with two. The total links to the section (`section_href`, the line of
  its `== ` header). A `wrong` row of the device's `cases.csv` adds `wrong answer #NN`.
- **A failed or skipped cell** spans the device's four columns: `failed: <reason>` (class `off`,
  reason after `reason=`, linked to its section), `skipped: <reason>` for a `SKIPPED` marker
  (class `skip`). Where the device's `cases.csv` has a `skipped` row for the query, its note is
  the reason shown, and it fills that query's rows the device has no section for. Any other
  missing cell is `—` (class `na`).
- **Layout:** `<title>PeacockDB benchmark results</title>`; one intro paragraph (written by
  `the_benchmark_report_matches_the_trees` from `testdata/benchmark-results/`; totals are
  device-side, `load_us + execute_us`; peak is the accountant's peak, a lower bound on device
  memory); per (dataset, sf) an `<h2 id="<ds>-sf<sf>">` and a table, header rows `query | mode |
  <device> (colspan 4)` and `load (ms) | execute (ms) | total (ms) | peak ≥ (GiB)`; then each
  `extra` table under `<h2 id="{id}">`. The `<style>` is `testdata/report.html`'s rules for `body`,
  `table`, `th`, `td`, `h2`, `a`, `.missing` and the `td.g.on/off/skip/na` colours, reused as
  classes `off`/`skip`/`na`. Every text is HTML-escaped. No time, no host, no absolute path; `\n`
  line ends; a final newline.

- [ ] **Step 1: failing unit tests** in `benchmark_report/tests.rs`, over temp trees built from
  literal sections (Task 3's `NEW` and `LEGACY`):
  - `a_timed_section_gives_its_load_execute_total_and_peak`: the row's cells read `1.0`, `0.5`,
    `1.5` and the peak in GiB from a trailer `load_us=1000 execute_us=500 device_us=1500`.
  - `a_legacy_section_derives_load_and_shows_no_peak`: `LEGACY` → load `(766 + 849) / 1000` ms,
    peak `—`.
  - `a_failed_case_shows_its_reason_and_the_list_wins`: a `FAILED` body `reason=timeout: x` →
    `failed: timeout: x`; with a `cases.csv` row `tpch,40,q3,,skipped,oom: y` → `oom: y`.
  - `a_wrong_row_marks_its_cells` (`#7` → `wrong answer #7`).
  - `devices_sit_side_by_side_and_a_case_one_lacks_is_absent` (two devices; the second lacks q3
    → its cell is `class="na"`).
  - `a_directory_without_benchmark_files_is_no_column` (a dir holding only `tpch.sf40/duckdb.tsv`).
  - `rows_follow_the_main_list_then_modes_in_order` (`order` q6, q1; q1 at `tp4_sized` and
    `tp1_single` → q6, q1 tp1_single, q1 tp4_sized).
  - `text_is_escaped` (a reason `a < b & c` → `a &lt; b &amp; c`).
  - `the_page_is_the_same_twice` (two renders equal; neither contains the temp root's path).
  - `section_href_names_the_line`.
- [ ] **Step 2:** `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_report` → compile error.
- [ ] **Step 3: implement**; **Step 4:** the same command → PASS.
- [ ] **Step 5: the page's test** in `benchmark.rs`:
  `the_benchmark_report_matches_the_trees` — `order` = `macro_invocations(&case_list_path(),
  "corpus_query_benchmark!")`'s (dataset, query); text = `render_benchmark_report(&root, &order,
  &[])`. Under `UPDATE_CANONICAL=1` it writes `benchmark_report_path()` (staged and renamed, as
  `the_benchmark_cases_follow_the_registry` does); without, the file must equal the text, else a
  panic naming the first differing line and the command
  `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens -- the_benchmark_report_matches_the_trees`.
  A missing page fails the same way.
- [ ] **Step 6: run** it → FAIL (no page); with `UPDATE_CANONICAL=1` → PASS; without → PASS.
  Open the page in a browser once: the H200's rows, every mode, loads derived, peaks `—`.
- [ ] **Step 7: docs** (Files above) and commit:

```bash
git add peacockdb-core/src/test_support peacockdb-core/tests/test_corpus_goldens/benchmark.rs testdata/benchmark-report.html llm-wiki/build-test.md
git commit -m "bench-rows: a committed page of every device tree, side by side, held to the trees"
```

### Task 5: one case on the H100, end to end

The renderer and the trailer are tested over the mock; this shows the harness writes them from a
real run. The file is not committed: sf40-baseline writes the h100 tree in full.

**Files:** none in git; the section goes into `llm-wiki/tasks/bench-rows-detail.md`.

- [ ] **Step 1: Build first.** Stage the install on nebius-cpu128 (new-hosts Task 10 Step 2) and
  rsync its `cpp/install/` into this tree's `cpp/install/` (`ship`'s default `--install`); stop the
  CPU VM.
- [ ] **Step 2: Start the H100, ship and run one case** on host `dev`, with new-hosts'
  `scripts/env/run-gpu.sh` as `build-test.md` "Benchmarks" documents it after new-hosts:
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu start`
  (exit 0, RUNNING with ssh; exit 75, no capacity: retry later, record it), then
  `G="scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu"; $G prepare && $G ship && PCK_TEST_FILTER=bench_tpch_sf40_q3_tp1_single $G run bench && $G pull`
  (q3: three loaders under joins. If the registry does not enable q3 at `gpu_tp1_single`, use the
  first tpch query that it does and that reads two or more tables.)
  Expected: the case passes; `testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt`
  came home.
- [ ] **Step 3: Read it back.** On `dev`:
  `f=testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt; [ "$(grep -c '^ *time_us=' $f)" = "$(grep -c 'batch_bytes=' $f)" ] && grep -E '^(load_us|execute_us|peak_bytes)=' $f`
  Expected: three lines, `load_us` > 0, `peak_bytes` > 0. (The directory is `device_slug`'s for
  "NVIDIA H100 80GB HBM3"; if the card names itself otherwise, use the directory that came home.)
  Then copy the file to `<scratch>/benchmark-results/h100-80gb-hbm3/tpch.sf40/` and run
  `PEACOCK_TESTDATA_DIR=<scratch> cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens -- every_total_us every_section_outside_h200`
  Expected: PASS (the arithmetic and the presence checks hold on a real section). Render the
  page over `<scratch>` once (the `UPDATE_CANONICAL=1` page test with `PEACOCK_TESTDATA_DIR=<scratch>`)
  and check the H100 column shows the case's load, execute, total and peak.
- [ ] **Step 4: Stop the VM and clean up.**
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu stop`;
  delete the h100 file from the checkout (`git status` shows nothing under `testdata/benchmark-results/`).
  Paste the section, the run command and the q3-or-substitute choice into the detail file and
  commit that file alone: `git commit -m "bench-rows: the format read back from one H100 case"`.

## Self-review

- Spec 1 (node fields, `.cpu.txt` notation, reported run) → Task 1. Spec 2 (`load_us`,
  `execute_us`, `peak_bytes`, the bound named) → Task 2 and "The format". Spec 3 (parsers and
  checks; H200 parses) → Task 3. Spec 4 (`build-test.md`) → Tasks 1-4, Step "Docs". Spec 5
  (the page, one reader, deterministic, regenerated under `UPDATE_CANONICAL=1`) → Task 4.
- `peak_bytes` needs no driver change: `ResidentAccountant::peak` already is the spec's quantity
  and `RunReport` already carries it; `executor/` is untouched.
- Names used across tasks: `render_timings(root, times, report)`, `timed_load_us`, `LOAD_NODE`,
  `RunTotals`, `run_section(&RunTotals, …)`, `parse_benchmark_section`, `BenchSection`,
  `BenchNode`, `PREDATES_ROWS`, `rows_problem`, `render_benchmark_report`, `ReportTable`,
  `ReportCell`, `section_href`, `benchmark_report_path`, `the_benchmark_report_matches_the_trees`.
