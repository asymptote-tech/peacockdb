# tp4-sized — detail

Branch `ENS-tp4-sized`, forked from `ENS-batching-report` at `325b1825` on 2026-11-06. PR targets
`ENS-batching-report`. Plan: [`tp4-sized-impl.md`](tp4-sized-impl.md).

## Progress

- 2026-11-06: branched, board `building`. Verda unavailable (`VERDA_CLIENT_ID` unset); CPU tests
  local. Developer dispatched at Task 1 Step 1.
- Inherited from query-fuser and batching-report (their detail files): `install-watch` on the
  H100 runs without `--key-file` (editor-eu's key is embedded); the H100 metrics pass needs
  `PCK_BENCH_HBM_FREQ=10000`; dev's disk had ~7 GB free on 2026-11-06 and captures come home;
  `testdata/calibration/hbm-parts/` is an untracked leftover never to be staged.
  batching-report's generator inserts tp4_sized columns when `<ds>.sf40/tp4_sized.benchmark.txt`
  exists; its hand-written Reading in `llm-wiki/reports/query-batching.md` is outside `--check`
  and must be re-read for the tp4_sized sums. CI runs that page's `--check`.

- 2026-11-06, Task 1 (developer). Departures from the plan, the tree winning:
  - `H100_MODES` lives in `peacockdb-core/tests/test_corpus_goldens/benchmark_lists.rs`, not
    `benchmark.rs` (the plan's `:625` is `PREDATES_PROVENANCE`'s use there). Both tests went
    beside it, in `benchmark_lists.rs`.
  - Red/green: `the_h100_list_is_written_at_tp1_single_and_tp4_sized` failed with modes
    `["tp1_single"]` (left) against `["tp1_single", "tp4_sized"]`; `the_h100_tree_is_held_to_both_modes_of_its_list`
    passed at once, containers' rule, as the plan said. After the one-entry change the binary
    ran 66/66, no warnings, `cases.csv` untouched (no `tp4_sized.benchmark.txt` yet: a mode
    with no file is not read, `device_cases::lines`).
  - `build-test.md` brought along: the cpu tier's counts (`test_corpus_goldens` 64 → 66, total
    3729 → 3731) and "sf40 baseline"'s `H100_MODES` sentence, which said tp4-sized "adds its own".
  - The plan's Task 2 Step 6 profiles tpcds in one trace pass. build-test.md "sf40 baseline"
    step 7 says sf40-baseline needed three runs: `nsys_calls.py` refuses the 14 tpcds cases whose
    sf40 join order differs from the sf1 goldens, and `set -e` then skips the rest. Task 2 will
    follow build-test.md's three-run recipe, with `PCK_BENCH_HBM_FREQ=10000` on the metrics pass.
- 2026-11-06, Task 2 (developer), before the H100:
  - **Case set (Step 1).** `device_cases.py` over the H100 `cases.csv` (its `ok` and `wrong`
    rows) against the main list's lines carrying `tp4_sized`: tpch 47 of 47 in the
    intersection (the main list has 48; `anti_join` is `skipped`), tpcds 79 of 79 (82; q17,
    q25, q29 `skipped`). 126 cases; no H100 line lacks `tp4_sized` (Review Focus 1). The lists
    are the H100 `cases.csv`'s `ok`/`wrong` rows, in its order.
  - **Install (Step 2): sf40-baseline's, shipped as it stands.**
    `/home/dmitry/build/peacock/sf40-baseline/install` (the plan's `/build/...` does not exist on
    dev; sf40-baseline's Task 8 recorded the same drift). Built 2026-11-05 11:02 UTC on
    nebius-cpu128 from the worktree at `b415d375` (sf40-baseline's Task 8). Its engine is the
    one that measured the published tp1_single tree: `b415d375..30a6165e` (the results commit)
    changes no engine, FFI, C++ or benchmark-harness source, only DuckDB-oracle test code. So
    both modes are one engine. The engine is not this branch's: dag-driver's executor and wire
    changes came later. The section and trailer formats are unchanged since (`benchmark_section.rs`,
    `benchmark_trailer.rs`: refactors only), and the sf1 `*.plans.txt` goldens that `nsys_calls.py`
    checks against are unchanged too (only `*-mini.cost.txt` moved). No CPU-VM build.
  - **The metrics pass cannot use the two-mode records.** `nsys_hbm.py`'s merge holds the parts
    to every call of the records given, so after Step 5 a tp4_sized-only pass over
    `h100-80gb-hbm3/<ds>.sf40/records.tsv` (tp1-single and tp4-sized rows) would be refused:
    "record calls have no part row". Tooling, not production behaviour, so no ticket. The pass
    runs instead with `--device` pointing at a scratch slug whose `<ds>.sf40/records.tsv` is
    the tp4_sized segments joined and split alone. Those rows are the ones the published record
    carries, byte for byte (`join_records.py` copies a case's rows from its highest attempt).
    The scratch directory is removed after the pass.
  - **tpcds trace.** build-test.md's three runs. `nsys_calls.py` stops at the first refused
    case, and the 14 were found at tp1-single, so the refused set at tp4-sized is read locally
    from run 1's capture before run 2.
  - **Answers.** `bench-each.sh` sets `PCK_WRITE_GPU_RESULT=1`, so the pull brings
    `mode=tp4-sized` sections into `goldens/<ds>.sf40/gpu-result.txt`. The sf40 answer check
    reads tp1_single only. What to keep is decided after the stop.
  - `prepare` is skipped unless `run` refuses the image (exit 2): `docker/` and `run-tests.sh`
    last changed 2026-11-04, before query-fuser's `prepare` at 2026-11-06 05:39.
- 2026-11-06, Task 2, the H100 run (Steps 2-3). 16:23 UTC `start` exit 0. `df -h /` 79 GB free,
  card idle, no `tp4_sized` file and no `calibration/records.tsv` on the VM. `install-watch`
  (no `--key-file`) exit 0. `ship --install /home/dmitry/build/peacock/sf40-baseline/install`
  exit 0; `peacock_gpu_benchmarks` sha256 `c19ed7752dbf55c5…` on dev and the VM. `prepare`
  not run. 16:28 `bench-each.sh … --within …/cases.csv --mode tp4_sized --timeout 1800
  --records /tmp/tp4-sized/records`, detached (`/tmp/tp4-sized/bench.log`).
  - The VM had just booted, so each file's first prefill read the disk: tpch q1's 9.0 GB took
    98 s (tp1_single's took 4.3 s from a warm page cache). The prefill is reported apart, so
    load and execute are unaffected; `prefill_us` is not comparable between the two modes.
  - **tpch q8: `protocol violation: no termination after 1000000 steps`**, after 223 s. That
    is [#238](../tickets/system-hardening.md#t238), the driver's fixed step cap, which it
    predicts for sf40. The cap is unchanged in today's `executor/driver/partitioned.rs`.
  - **tpch q9: timeout at 1800 s.** Card at 0 %, one host thread at 100 % throughout: the
    driver stepping, not the device working (tp1_single's run is 620 ms). The binary then
    segfaulted in exit: this engine leaves a timed-out case with `exit`, the current tree with
    `abort` (dag-driver), for that reason. The section was written first.
  - **tpcds q18 and q24: timeout at 1800 s**, card busy (32-41 %), not a host spin.
  - **My wrapper's cap cut the run at 20:28** (`timeout 14400` around bench-each, exit 124),
    during tpcds q64. The container went on and recorded q64's timeout on the VM. My first
    relaunch (20:33) ran q64 again: under `PEACOCK_BENCH_RESUME=1` the harness re-runs a failed
    section, and bench-each keeps only what is in dev's tree, and I had not pulled. I stopped
    that attempt about 10 min in (`docker stop` on the VM, the local processes by pid), ran
    `$G pull` (q64's failed section came home), and relaunched at 20:47 with a 7 h cap. It kept
    97 sections and went on at q65. Lesson: after a cut, `pull` before rerunning bench-each.
  - **Run done, 21:33.** `bench-each.sh` exit 0: `126 cases at tp4_sized, every one with its
    section`. Step 3's check: tpch 47 sections and 45 `load_us=`, tpcds 79 and 76. 121 record
    segments in `/tmp/tp4-sized/records/`, one per timed case. Wall time per case (container
    start, prefill and eleven runs): tpch 2140 s over 45 timed cases (longest q2, 435 s), tpcds
    6743 s over 76 (longest q14, 747 s).

### Failed at sf40 tp4_sized

| dataset | query | reason |
|---|---|---|
| tpch | q8 | `protocol violation: no termination after 1000000 steps` ([#238](../tickets/system-hardening.md#t238)) |
| tpch | q9 | `timeout: the case ran past 1800 s` (card idle, one host thread busy) |
| tpcds | q18 | `timeout: the case ran past 1800 s` (a `wrong` row at tp1_single, #306) |
| tpcds | q24 | `timeout: the case ran past 1800 s` |
| tpcds | q64 | `timeout: the case ran past 1800 s` |

All five pass tp1_single. None is a wrong answer, so none is a new ticket (plan, Step 4).

### Cache

Every timed section has `pinned_cache=on …` and no `prefill_files=0`: Step 4's two greps print
nothing.

### Profiles

- **Step 5 moved after Step 9.** `split_record.py --cases cases.csv` holds a record to the list's
  modes, and the committed `cases.csv` carries `tp1_single` alone until Step 9 rewrites it
  (cargo, so after the stop). Splitting the joined tp4_sized record against it refuses every
  row ("timed and is not declared"). So join, split and publish run after Step 9, against the
  rewritten list. The metrics pass's scratch record was split against the tp4 case files
  (`/tmp/tp4-sized/{tpch,tpcds}-tp4.csv`, 45 + 76 rows), exit 0.
- **The tp4_sized records are 45x and 130x tp1_single's.** tpch 351,269 rows (33 MB) against
  7,850; tpcds 6,251,249 rows (541 MB) against 48,220. tpcds q88 alone is 817,100 rows, q80
  728,260, q14 544,450. Sized batches make a call per batch.
- **The tpch trace pass**, 21:33-22:08: 45 tests passed in 1271 s under nsys. The capture
  made 32 GB of raw data in the VM's `/tmp/nvidia`, a 2.2 GB `capture.nsys-rep` and a 5.86 GB
  `capture.sqlite`. Dev had 7 GB free, so dev's `testdata/calibration/` was made read-only before
  the pull, and the script was stopped after its first refused rsync (the pull reads the whole
  file, then fails, and retries 100 times). The capture stays on the VM. `nsys_loads.py` and
  `nsys_calls.py` ran on the VM against it (the scripts rsynced there; the shipped goldens),
  into `~/peacock/peacockdb-beta/derived/tpch/`. Before the pass, sf40-baseline's stale ignored
  `testdata/calibration/capture.{sqlite,nsys-rep}` (1.6 GB) were removed from dev.
- **tpcds cannot be traced this way.** Scaled from tpch (32 GB of raw data for 351 k record rows),
  tpcds's 6.25 M rows would need several hundred GB. q88 alone would need more than the VM's
  73 GB free. The metrics pass's per-case captures scale the same way.
- **H100 stopped at 22:53** after tpch's trace. `nsys_calls.py` had run 36 min on the VM with no
  output, so the 5.86 GB sqlite came home to `/tmp/tp4-sized/capture-tpch/` (this worktree's
  `target/debug/incremental`, 6.4 GB, removed for room) and the reader runs on dev, off the
  VM's clock. The VM keeps its copy of the capture.

### Step 9 and Step 5, after the stop

- `UPDATE_CANONICAL=1 … -- the_h100_list_is_the_lists_measured the_benchmark_report_matches_the_trees`:
  both ok. `cases.csv`: only the modes column changed (the other columns compared against
  HEAD). 121 rows `tp1_single|tp4_sized` (120 `ok`, tpcds q73 `wrong` #306); the five
  tp4-failed rows keep `tp1_single` (tpcds q18 stays `wrong` #306); the 4 `skipped` rows are
  unchanged. `device_cases.py --mode tp4_sized` gives the same queries in the same order as the
  profile case files. The page gained the tp4_sized rows. Both tests pass again without the
  variable.
- Records: `join_records.py` over the two published records and the 121 segments: 247 cases
  from 123 segments, exit 0. The free memory was 78.7 GiB in every segment, so no range.
  `split_record.py --cases cases.csv` exit 0: tpch 7,850 + 351,269 rows, tpcds 48,220 +
  6,251,249. `publish_record.sh` exit 0; `records.sha256` keeps 2 `h100-80gb-hbm3/` lines,
  both rewritten (tpch `aeaa83c4…`, tpcds `66991e16…`). `fetch_record.sh` reports `as pinned`
  for each, and each holds `tp1-single` and `tp4-sized` rows (Review Focus 6).
- **Answers.** The pulls added 45 + 76 `mode=tp4-sized` sections to
  `goldens/{tpch,tpcds}.sf40/gpu-result.txt`; the tp1-single sections are byte-identical to
  HEAD. Against tp1-single, from the same engine: 116 identical; tpch `global_stddev`,
  `rollup_stddev`, `shuffle_stddev` and tpcds q39 differ in the last digits of
  stddev/variance (four lanes merge partial sums in another order); tpcds q73 differs in the
  order of tied rows only (#306). No wrong answer. Nothing reads the tp4-sized sections (the sf40
  answer check takes tp1_single, and the coverage check reads sf1 files only), and the spec's
  scope does not list them.
- Full `cargo test --features rust-only -p peacockdb-core` with every new file in place: exit 0,
  no warnings (`--lib` 1209 + 2 ignored, `test_cpu_corpus` 2451, `test_corpus_goldens` 66,
  `test_cost_model` 3).
- tpch `loads.tsv` at tp4-sized (derived on the VM): 116 load nodes over 45 queries, the
  committed file's notes and columns. Summed, load 75.2 s, of which unattributed 23.6 s (31 %),
  decompress + decode 51.4 s, h2d 1.4 s; tp1-single's 130 nodes: 9.0 s, 2.0 s (22 %), 5.5 s,
  1.6 s (Review Focus 5). tpcds at tp4-sized (72 cases): unattributed 70.1 of 174.5 s (40 %),
  against 6.2 of 27.3 s (23 %) at tp1-single.
- **`nsys_calls.py` was quadratic in a thread's calls** (fixed, test first). Each region walked
  its thread's calls through `rows[lo:]`, a copy of the rest of the thread. The tpch tp4_sized
  capture holds 12.2 M libcudf calls and 1.0 M harness ranges, 5.8 M and 350 k of them on one
  thread, so the reader ran 45 min on the VM and on dev without finishing. `calls_in` walks by
  index. `test_a_region_reads_its_own_calls_and_not_the_rest_of_its_thread` counts the
  elements read: 99,998 under the slice, 3 after. All calibration test files pass (`test_plot.py`
  with matplotlib from `/tmp/br-mpl`). The fixed reader takes 4 min on the tpch capture.
  build-test.md's calibration row: 118 → 119.
- **tpch's recipe check refuses 3 of the 45 at tp4-sized:** `global_stddev`, q15, q22 (driven
  seqs and kinds differ from `goldens/tpch.sf1/tp4-sized.plans.txt`, as sf40-baseline's 14 tpcds
  cases at tp1-single). Found from the one capture with `/tmp/tp4-sized/bin/refused.py`, which runs
  `nsys_calls._check_one` per case. tpch's `calls.tsv` therefore needs a capture of the other 42
  (build-test.md's recipe), and `loads.tsv` keeps all 45.
- **Where this stops (2026-11-06 23:40), with the VM stopped.** Done: the run, the failures, the
  cache check, `cases.csv` and the page, the records published, tpch's `loads.tsv` (in
  `/tmp/tp4-sized/derived/tpch/`, not yet moved into `testdata/calibration/h100-80gb-hbm3/tpch.sf40/`).
  Open: tpch's `calls.tsv` (42 cases), tpch's metrics pass, every tpcds profile, Task 3, Task 4
  (`batching_report.py --check` now exits 1: the page is stale with the tp4_sized files in the
  tree, and CI runs that check). Scratch: `/tmp/tp4-sized/` (records, split records, case files,
  the tpch capture, logs). The VM keeps the tpch capture in `~/peacock/peacockdb-beta/testdata/calibration/`.
- 2026-11-06 (coordinator) decisions after the first H100 session:
  - **tpcds profiles: option (b), bounded.** Estimate each tpcds case's capture size from its tp4_sized record rows (tpch's 32 GB raw for 351 k rows is the scale). Capture one case at a time where it fits the H100's free disk with margin. Derive `calls.tsv`, `loads.tsv` and `hbm.tsv` on the VM, and never pull a capture to dev (dev has about 7 GB free). Every case that does not fit is listed as a failed capture, with the estimated size as its reason, as the spec's Profiles paragraph allows. If the estimate for the cases that fit exceeds 6 H100 hours, report before starting.
  - **The stray tp4_sized answers** in `testdata/goldens/{tpch,tpcds}.sf40/gpu-result.txt` were dropped (`git checkout`). They are outside the spec's scope, and nothing reads them.
  - **The benchmark files, `cases.csv`, `records.sha256` and the regenerated page are held** (uncommitted) until Task 4 regenerates `llm-wiki/reports/query-batching.md`. Committed alone they would turn CI's page `--check` red. Task 3's commit takes them together with Task 4's page.
- **Second H100 session (developer), from 23:40.** No capture comes to dev.
  - **The bound for tpcds:** a case fits when its estimated raw nsys data (tpch's 32 GB per
    351,269 record rows, 91 KB a row) is at most 40 GB, about half the VM's 81 GB free (the raw
    data, the report and the export are on disk together). 72 of tpcds's 76 timed cases fit.
    Four do not, and are failed captures: q88 (817,100 rows, about 74 GB), q80 (728,260, 66 GB),
    q14 (544,450, 50 GB), q61 (462,090, 42 GB). The first estimate for the 72 was 5.3 h: 3.3 h
    of trace and 2.0 h of metrics (the benchmark's wall time per case, plus tpch's 13 s/GB of
    report and export and 0.68 ms/row of reading).
  - **The driver**, `/tmp/tp4-sized/bin/trace-cases.sh` (also on the VM in `tp4-run/`), does
    what `create_nsys_profile.sh --trace` does on the host (same env, nsys flags, binary and
    `--exact` names) for one capture per line, then runs `nsys_loads.py` and `nsys_calls.py`
    there and deletes the capture. The script itself was not used for the trace because it
    pulls each capture home. Per-case outputs go to `~/peacock/peacockdb-beta/derived/<ds>/`,
    one summary line per capture in `derived/summary.tsv`. A line whose `loads.tsv` exists is
    skipped, so a rerun resumes.
  - **tpch, 42 cases** (`global_stddev`, q15, q22 left out): one capture, 42 passed, report
    2.17 GB, export 5.82 GB, 2306 s of run and report, 543 s of export and reading. `calls.tsv`
    exit 0 (all 42 match their recipes).
  - **tpcds q1-q6**, one capture each: all passed, all `calls.tsv` match their recipes. q4
    took 1547 s, 1.56x its estimate. At that rate the 72 need about 8 h (about 5.1 h of trace and
    3 h of metrics), past the coordinator's 6 h. So the driver was ended after q6 (its group
    file truncated; the full list kept as `tp4-run/groups-all.txt`), and the rest waits on
    the coordinator.
  - **tpch metrics**, 01:09: `create_nsys_profile.sh --metrics --cases /tmp/tp4-sized/tpch-tp4.csv
    --device h100-tp4-scratch`, with `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12
    PCK_BENCH_HBM_FREQ=10000`. The scratch slug's records are the tp4_sized segments alone,
    joined and split against the profiled cases (tpch 45, tpcds 72), in
    `testdata/calibration/h100-tp4-scratch/` (ignored, removed afterwards).
  - **tpch metrics done, 01:58:** exit 0, `35121 calls from 45 parts`. `hbm.tsv` came home
    (3.5 MB), the flat file was restored from git, and the H100 was stopped (01:58, `STOPPED`).
    The tpcds q1-q6 derived files are home too (`/tmp/tp4-sized/vm-derived/`), and are held,
    not merged, until tpcds is settled.
- **tpch's calibration files**, merged by `/tmp/tp4-sized/bin/merge_mode.py` (the committed
  notes and header, the other modes' rows, then the new tp4-sized rows; the headers and notes
  matched): `loads.tsv` +116 rows (45 queries, from the first capture), `calls.tsv` +7,990 (42),
  `hbm.tsv` +35,121 (45). Additions only.

### Capture failed

| dataset | query | file | reason |
|---|---|---|---|
| tpch | global_stddev, q15, q22 | `calls.tsv` | the recipe check refuses them: the sf40 tp4-sized plan drives other seqs than `goldens/tpch.sf1/tp4-sized.plans.txt` |
| tpcds | q88, q80, q14, q61 | all three | estimated raw capture 74, 66, 50 and 42 GB at tpch's scale, past the 40 GB bound; at tpcds q13's measured 160 KB a row, 131, 117, 87 and 74 GB, against the H100's 80 GB free |
| tpcds | q9, q30, q31, q32, q33, q54, q56, q60, q71, q81, q83, q85, q91, q92 | `calls.tsv` | the recipe check refuses them: the sf40 tp4-sized plan drives other seqs than `goldens/tpcds.sf1/tp4-sized.plans.txt` ("capture does not match qN's recipes -- declared but never driven: …") |

### Tasks 3 and 4 (2026-11-07)

- Task 3 Step 1: `test_corpus_goldens` 66 passed with every file in place.
- Task 4: `batching_report.py --check` exit 1 (stale), the regeneration exit 0, `--check` exit
  0. Both tables carry `tp4_sized load (ms) | tp4_sized execute (ms)`; first3's sums equal the
  three trailers added (tpch 3924.0 and 5363.3 ms, tpcds 2002.4 and 1649.0); the five failures
  show as `(n/N)` with their reasons under the tables. The intro names the tp4_sized side. A new
  Reading bullet compares it with tp1_single, and states that at tp4_sized the summed node times
  track the wall time (TPC-H 93.0 of 97.6 s; tp1_single 21.7 of 25.0 s). The other Reading
  bullets were re-read; their numbers are untouched by tp4_sized and still hold.
- #238's "No corpus or benchmark case reaches it today" now names tpch q8 at tp4_sized.
  build-test.md, "What `calls.tsv` does not cover", gives tp4-sized coverage and capture size.
- Final checks: full `cargo test --features rust-only -p peacockdb-core` exit 0, no warnings
  (1209 + 2 ignored, 2451, 66, 3, …); every `scripts/calibration/tests/test_*.py` passes (119);
  the page's `--check` exit 0.
- 2026-11-07 (coordinator): Tasks 3 and 4 committed together (7b7bc64d, bb89cbc4); tpch profiles complete. **Decided: profile the remaining 66 tpcds cases** (about 7.5 H100 hours): the spec asks for a profile of every benchmarked case, and "failed capture" covers a capture that cannot be taken (q88, q80, q14, q61 by size), not one skipped for time. q1-q6 are captured and derived in `/tmp/tp4-sized/vm-derived/`. A fresh developer resumes at q7.
- **Third H100 session (developer), 2026-11-07 02:16-09:54 UTC: tpcds profiled.** Same install
  (`peacock_gpu_benchmarks` sha256 `c19ed775…`) and the same `scripts/calibration/*.py` on dev and
  the VM, checked by sha256 before the run. One detached driver on the VM, `tp4-run/run-all.sh`:
  `trace-cases.sh` over q7 onward (66 lines), then `metrics-cases.sh` over all 72. Scratch copies
  are in `/tmp/tp4-sized/ship/`.
  - **`metrics-cases.sh`** (new) runs `create_nsys_profile.sh --metrics`'s per-case capture on the
    VM: the same nsys flags, `PEACOCK_BENCHMARK_CAPTURE=metrics`, its own record path, and
    `PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12 PCK_BENCH_HBM_FREQ=10000` (required, no
    defaults). Each case is joined by `nsys_hbm.py --case` against that case's rows of the tp4_sized
    record alone (the notes, the header and the case's rows; the join reads only them anyway). That
    avoids parsing the 541 MB record 72 times. The capture is then deleted, and only the part is kept.
    The script itself was not used because it dies on the first failing case and its merge needs
    every case. Checked on q1 first: 268 calls, 100 µs sampling. The first try read nothing,
    because the driver changed directory before resolving its relative arguments.
    `trace-cases.sh` has the same trait, so `run-all.sh` passes absolute paths.
  - **Trace, q7-q99** (02:19-05:57): 66 runs, all passed, 3.63 h (9129 s run and report, 3931 s
    export and reading). `loads.tsv` for all 66. The recipe check refuses 14 cases' `calls.tsv`:
    q9, q30, q31, q32, q33, q54, q56, q60, q71, q81, q83, q85, q91, q92. That is tp1-single's 14
    without q18 and q61 (neither is profiled here), plus q9 and q54.
  - **The size scale was low.** q13 (318,220 rows, estimated 29 GB) made 51 GB of raw data in
    `/tmp/nvidia`, leaving 27 GB free. That is about 160 KB a row, not tpch's 91 KB. Raw size does
    not follow rows closely: q96 (324,930 rows) made a 0.6 GB report against q13's 3.3 GB. At q13's
    rate the four held out need 74 (q61), 87 (q14), 117 (q80) and 131 GB (q88), against 80 GB free.
    So they stay out.
  - **Metrics, all 72** (05:57-09:53): every case passed and joined, 3.94 h (10,979 s run, 3202 s
    export and join). The largest export was q50's 5.3 GB.
  - Pulled: the per-case `calls.tsv` and `loads.tsv` (q1-q6 byte-identical to the earlier copies in
    `/tmp/tp4-sized/vm-derived/`), the 72 hbm parts, and the logs, into `/tmp/tp4-sized/vm-derived2/`.
    No capture came to dev, and none is left on the VM. The H100 stopped at 09:54 (`STOPPED`).
  - **Merged, with the VM stopped.** `/tmp/tp4-sized/bin/concat_cases.py` joins the per-case files
    in the list's order. It refuses differing notes or headers, and they matched the committed
    files'. `merge_mode.py` then adds them: `calls.tsv` +36,437 rows (58 queries), `loads.tsv` +578
    (72). For hbm, the tp4_sized tpcds record was cut to the 72 cases (3,699,290 rows), and
    `nsys_hbm.py --parts …` (72 parts) merged against it: `369929 calls from 72 parts`, exit 0. On
    dev that takes 10 s and 5 GB. `hbm.tsv` +369,929 rows. Additions only: `git diff --numstat`
    shows no deleted line. `hbm.tsv` is now 35 MB (3.2 MB gzipped), the largest calibration file.
  - Checks: the calibration tests pass (119); `test_corpus_goldens` 66 passed;
    `every_batched_query_has_its_baseline_and_its_loads` passes (it reads `loads.tsv` at tp1 only);
    `batching_report.py --check` exit 0. The page's load split reads `LOADS_MODE = "tp1-single"`
    rows only, so the tpcds tp4 rows leave it unchanged, and it was not regenerated.
- 2026-11-07: tpcds profiles committed. Plan complete. PR #202 against `ENS-batching-report`; board `reviewing`; reviewer round 1 dispatched.

## Review round 1 (2026-11-07)

0 blocking, 2 important, 4 nits.

- **I1 (tickets), coordinator:** #238 now names engine `b415d375` and says the link to tpch q8 is unverified; new #308 (tp4_sized host spin) for tpch q9, tpcds q18, q24, q64 timing out at 1800 s with the host spinning; the Reading cites #308.
- **I2, developer:** after this PR the committed tooling cannot regenerate the H100's profiles in either mode. `create_nsys_profile.sh --cases` makes a case per listed mode (now both); `nsys_hbm.py`'s merge holds parts to every call in the two-mode record; the per-case on-VM drivers live only in `/tmp/tp4-sized/bin`. **Decided:** give `nsys_hbm.py`'s merge a mode scope and `create_nsys_profile.sh --cases` a way to take one mode (test first); rewrite build-test.md "sf40 baseline" step 7 for a two-mode list, with the on-VM per-case recipe and that q14, q61, q80, q88 were left out on an estimate. The `/tmp` drivers are not committed.
- **N3, developer:** `test_calls.py` guards `calls_in` alone; add a `main()`-level or `islice` regression.
- **I2 and N3, developer, done (no VM):**
  - `nsys_hbm.py --mode <mode>` (the record's spelling, `tp4-sized`) holds a merge to that mode's
    calls of the record, both ways. It refuses a mode no record row has, a part of another mode
    ("in no record at mode …"), and `--mode` with `--capture`. Red first:
    `test_a_merge_scoped_to_a_mode_covers_that_mode_s_calls_alone` stopped on `unrecognized
    arguments: --mode`.
  - `create_nsys_profile.sh --cases <csv> --mode <mode>` (the list's spelling, `tp4_sized`)
    keeps that mode's lines for both passes. It passes `--mode tp4-sized` to the merge only, not
    to the per-case joins, whose `--case` already scopes them. It refuses `--mode` without
    `--cases`, `--mode` with `--fused`, a mode `device_cases.py` does not know, and a known mode no
    row carries. Without that last check, the empty list's blank line reached the goldens check as
    dataset `.sf`. Red first: `test_cases_with_a_mode_take_that_mode_alone` failed on
    `unknown argument: --mode`.
  - build-test.md "sf40 baseline" step 7 is rewritten for a two-mode list: `--mode` on every
    scripted pass, the on-VM per-case recipe in prose, the cut record for a pass that leaves cases
    out, and q14, q61, q80 and q88 left out on an estimate. With `--mode`, tpch's tp4_sized metrics
    pass now fits the script. tpcds's still needs the cut record (72 of 76 cases).
  - N3: `CountedCalls` also counts what its iterator yields, and the region lies at call 50,000.
    So `rows[lo:]` and `islice(rows, lo, None)` inside `calls_in` each read about 50,000.
    `test_the_reader_walks_every_region_through_calls_in` wraps `calls_in` and runs `main()`: three
    regions, three walks. Checked by mutating a copy of `nsys_calls.py`, restored byte-identical
    after each: `islice` and the slice turn the first test red, and an inline slice walk in
    `main()` turns the second red.
  - Counts: calibration row 119 → 121; images row 261 → 262; grand total 8187 → 8190 (Python
    1600 → 1603). shellcheck is not installed on dev, so the script was checked with `bash -n` and
    its 50 tests.
- **N4-N6, coordinator:** report text (3.7 s; the 93.0 of 97.6 s set named; "every GPU side"; "exceeds"), build-test.md's sf40 answer check names tp1_single, tpcds unattributed share added here.

## Checkpoints

| step | commit | note |
|---|---|---|
| Task 1 Step 5 | tp4-sized: the h100 list is written at tp1_single and tp4_sized | reported; coordinator commits |
| Task 1 Step 5 | `49a0d619` | tp4-sized: the h100 list is written at tp1_single and tp4_sized; test_corpus_goldens 66, cpu suite green |
| Task 2 (part) | `de71ced0` | tp4-sized: nsys_calls reads a region's own calls, not the rest of its thread; interim: benchmark done (126 cases, 5 failed), files held until the page regenerates; calibration tests 119 |
| tpcds profiles | reported | tp4-sized: tpcds tp4_sized profiled on the H100 (72 of 76; calls.tsv 58); calibration tests 119, test_corpus_goldens 66, page --check 0 |
| review round 1 (I2, N3) | reported | tp4-sized: the hbm merge and the profile script take one mode of a two-mode list; step 7 rewritten; the region walk guarded at main(); calibration tests 121, test_run_gpu 50, test_corpus_goldens 66, page --check 0 |
| Review round 1 fixes | `f4f3fd27` | tp4-sized: the hbm merge and the profile script take one mode of a two-mode list; step 7 rewritten for two modes; I2, N3; calibration 121, test_run_gpu 50, goldens 66, --check 0 |

## VM time

| VM | start (local) | stop (local) | hours |
|---|---|---|---|
| nebius-h100 | 2026-11-06 16:23 UTC | 2026-11-06 22:53 UTC | 6.50 |
| nebius-h100 | 2026-11-06 23:40 UTC | 2026-11-07 01:58 UTC | 2.30 |
| nebius-h100 | 2026-11-07 02:16 UTC | 2026-11-07 09:54 UTC | 7.63 |
| nebius-h100, task total | | | 16.43 |
