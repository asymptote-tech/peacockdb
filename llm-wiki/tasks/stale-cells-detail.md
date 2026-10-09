# stale-cells — run detail

Working notes. The spec is [`stale-cells.md`](stale-cells.md) (frozen); the plan is
[`stale-cells-impl.md`](stale-cells-impl.md).

## Blocked before it started, and unblocked the same day (2026-10-08)

No branch exists yet. The task was never dispatched, because a fresh analyst's reading of it said
there was nothing a developer could do that would not be guessing at results — every step needs a
card and no card answered. **That is no longer true**: the human's host override moved chain J to
nebius-gpu, the board here says `approved to build`, and `duckdb-oracle` has since run its device
cycle on that card. Dispatch this task; do not re-block it on shad-gpu.

The reasoning below is kept because its first half still holds and is the trap in this task.

- **Step 1 is the device cycle, and steps 2 and 3 are arithmetic over its 16 outcomes.** The one
  device-free edit in the plan is Task 1 Step 1 — flip the four lines' `gpu_modes` to `all_modes`
  and the four rows' five gpu cells to `enabled` — and that edit cannot be left in the tree:
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` fails on an absent
  `gpu-result.txt` wherever a cell is enabled, and committing it would assert that 16 cells are
  proven which have never run. That is the false-coverage shape a reviewer is told to hunt.
- **Hosts, as of the override.** shad-gpu is still down — it timed out on :22 and :443 with DNS
  resolving and the host key intact, so it is the host, and CI's `GPU Tests (remote)` job fails
  the same way. `verda` resolves nowhere. The card to use is **nebius-gpu**, `dmitry@89.169.109.150`,
  an L40S at cuDF 25.02; the override at the top of the chain J section of `tasks.md` carries the
  recipe, and `duckdb-oracle-detail.md` has a worked run of it. This workstation still builds and
  links the device targets but has no card, so the CPU half stays local.

## #253 does not block this task, which is worth recording because it looks as though it should

[#253](../tickets/corpus-coverage.md#t253) says the oracle cannot record a cell whose device answer
diverges from DuckDB while the cpu's matches. That is the outcome step 2 exists to produce, so the
question is live — and the answer is that the spec already prescribes a value that works. A failing
cell is `disabled` in the registry and dropped from the line's `gpu_modes`, and
`duckdb_device_cases!` emits a `duckdb_gpu_*` case only for the modes the line lists, so an off
cell has no device case to go red. The line's `duckdb_oracle` never moves. #253 bites only a task
that wants to keep a diverging cell on, and this spec says the opposite: "a cell that fails … stays
off under it". The cost here is coverage lost — a divergence recorded only as a ticket — not a red
tree.

Narrower still: `authoritative_mode` is the last enabled cpu mode, tp4_sized on all four rows, so
the cpu's own DuckDB comparison already runs at a tp4 mode and a split would have to be
mode-specific within tp4. #253's fingerprint half does reach one of these rows —
`filter_project` is `duckdb_fingerprint`, where no tolerance and no exemption exist at all.

## Two stale facts the plan carried, corrected

`stale-cells-impl.md` cited `corpus_cases.inc:43,44,46,70` for the four lines; they are at
**46, 47, 49, 73** (`filter_project`, `aggregate_groupby`, `shuffle_additive`, `shuffle_stddev`).
The registry rows 123/126/137/139 were right. The `183` comment is at **56**, not 53, and it sits
above the join batch, whose rows do keep `183` — tpch q4, q15, `anti_join`, `cross_join`,
`nested_loop_join`, `nested_loop_left_join` and `semi_join` are all still tagged with it. So the
plan's Task 3 checkbox about that comment is decidable today and the answer is **leave it**. Both
citations are corrected in the plan.

**This correction was itself one line short**, measured during the run below: the four lines are at
**47, 48, 50, 74** and the `183` comment at **57**. The plan now carries those. The decision to
leave the comment stands, and seven rows still keep `183`.

## What it needs now

One `build-test-shadgpu.sh --build` and a filtered device run for the four rows, on nebius-gpu
under the override — never `--run` or `--pull-results`, both of which ssh to shad-gpu.

The earlier note here said `duckdb-oracle`'s cycle could carry these 16 cells too. **It did not**,
and the chance has passed: that cycle recorded exactly the 26 cells enabled at the time, and the
completeness pass verified the recorded set has zero extras. So this task owns its own cycle, and
its first act is still the Task 1 Step 1 edit above — which is why the cycle and the enablement
have to land together rather than in either order.

## Dispatched, after the resequencing (2026-10-09)

Branch `ENS-stale-cells`, forked off `ENS-refcounted-scatter` at `35c0f04b`. Tasks 1–4 are `done`.

**This task moved.** It was task 2 and is now task 5, resequenced by the human in master's
`f0a6ecbf` so that it branches off refcounted-scatter rather than forking the chain off
duckdb-oracle. Nothing about its subject changed — none of its four rows has a join, so the three
tasks that overtook it cannot have touched them — but two things under it did move while it waited:
`tpch/q15` now reads `183 220` rather than `183`, and **#183 and #187 are both archived**, so the
spec's description of them as "closed" is now also true of where they live.

**One decision the coordinator owes it before it builds, and it is a refusal.**
[#253](../tickets/corpus-coverage.md#t253) says the DuckDB oracle cannot record a divergence found
on the device while the cpu agrees — one `duckdb_oracle` per line serves both, so `duckdb_exact`
fails the device case and `duckdb_divergent` fails the cpu case — and it names this task as the
first that can produce one, asking for the decision "before `stale-cells` builds rather than inside
it". The decision: **the harness does not change here.** The spec's Restriction is the 16 cells and
no fix to anything they show, and its step 2 already says what to do with a cell that cannot be
green — it stays off under a ticket. So a device-only divergence from DuckDB is a cell that stays
off under #253, recorded with its query and mode, and #253 gets the concrete instance it so far
lacks. That is cheaper and more honest than widening the harness inside a task whose whole point is
to measure, and it leaves #253 a decision for a task that can design it.

## The measurement, run on nebius-gpu (2026-10-09)

The task's whole content: the 16 cells ran, and **all 16 passed both halves** — the device gate
against the cpu's goldens and the `duckdb_gpu_*` comparison against `duckdb-result.txt`. No cell
stays off, no ticket is owed, and `183` and `187` came off all four rows. **#253 gets no instance
from this run**: not one cell diverged from DuckDB on the device, so the device-only divergence the
decision above was taken for never occurred.

### The sixteen cells

Gate = the case in `test_gpu_corpus` (plan shape, `in_rows`, per-batch lists, bytes, then the
answer per `gpu_oracle`). DuckDB = `duckdb_gpu_tpch_<q>_<mode>` in `test_cpu_corpus`, reading the
recorded `gpu-result.txt` section against `duckdb-result.txt` under the line's `duckdb_oracle`.

| query | mode | gate | DuckDB | decision | ticket |
|---|---|---|---|---|---|
| `tpch/aggregate-groupby` | tp1_rowgroup | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/aggregate-groupby` | tp4_single | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/aggregate-groupby` | tp4_rowgroup | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/aggregate-groupby` | tp4_sized | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/shuffle-additive` | tp1_rowgroup | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/shuffle-additive` | tp4_single | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/shuffle-additive` | tp4_rowgroup | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/shuffle-additive` | tp4_sized | ok | ok (`duckdb_exact`) | **enabled** | — |
| `tpch/shuffle-stddev` | tp1_rowgroup | ok | ok (`duckdb_approx`) | **enabled** | — |
| `tpch/shuffle-stddev` | tp4_single | ok | ok (`duckdb_approx`) | **enabled** | — |
| `tpch/shuffle-stddev` | tp4_rowgroup | ok | ok (`duckdb_approx`) | **enabled** | — |
| `tpch/shuffle-stddev` | tp4_sized | ok | ok (`duckdb_approx`) | **enabled** | — |
| `tpch/filter-project` | tp1_rowgroup | ok | ok (`duckdb_fingerprint`) | **enabled** | — |
| `tpch/filter-project` | tp4_single | ok | ok (`duckdb_fingerprint`) | **enabled** | — |
| `tpch/filter-project` | tp4_rowgroup | ok | ok (`duckdb_fingerprint`) | **enabled** | — |
| `tpch/filter-project` | tp4_sized | ok | ok (`duckdb_fingerprint`) | **enabled** | — |

Four results worth naming individually, because each was a place the estimate could have been
wrong:

- **`filter-project` at the tp4 modes is the one that looked expensive and is not.** Its
  `gpu_oracle` is `live_cpu`, so each of those four cells runs the cpu backend at the same mode
  over 2.4M rows beside the device run. The whole 95-case binary finished in 29 s, so the four
  cost seconds, not minutes. The foreground gate the plan reserved for them was not needed.
- **`filter-project` is also the only one on `duckdb_fingerprint`**, and the fingerprint is
  order-insensitive by construction (`fingerprint.rs`'s `hash_of` sorts the rows), so the device's
  SHA-256 over 2.4M rows is the same at all five modes and equals the one `duckdb_result.py`
  computed. All five sections carry `rows=2402187` and the identical `hash:`.
- **`shuffle-stddev`'s four cells passed with schema validation off**, as its line says
  (`schema_validation_disabled`, #225). Nothing was proved about them *under* validation, so #225
  is untouched and still describes the reason the line reads that way. No cell failed on schema,
  so by spec item 4 the row does not keep `225`.
- **`aggregate-groupby` and `shuffle-additive`** are `golden_exact` on `duckdb_exact` — text for
  text on both sides at four new modes each, nothing tolerated.

### What moved in `gpu-result.txt`, and what did not

`testdata/goldens/tpch.sf1/gpu-result.txt`: 22 sections → **38**, +127/−3 lines. The 16 added
sections are the 16 cells, newly written because the cells turned on — expected, not a finding.

The three removed lines are a **finding, and a small one**: `== shuffle-stddev mode=tp1-single`'s
rows A/F, N/O and R/F moved in their last one or two digits, e.g. `14.426465559178103` →
`14.4264655591781` and `208.12276776323006` → `208.1227677632301`. That is ~1e-15 relative, inside
`golden_approx_std`'s 1e-11 and inside `duckdb_approx`'s tolerance, and both cases are green. It is
run-to-run float nondeterminism in a Welford reduction, which the line's own comment and
`build-test.md` both say to expect; `shuffle-stddev` is the only tpch query declaring Float64
outputs, which is why it is the only section that could move.

Everything else reproduced **byte for byte** against the committed files, which is the stronger half
of this result: `tpcds.sf1/gpu-result.txt` and `pbench.sf1/gpu-result.txt` were both pulled from the
host and `diff`ed clean (4 and 51 sections), and the other 21 tpch sections are unchanged. So the
two files are not committed from this run — only tpch's is, and only because its cell set changed.

### Tags

`183` struck from `aggregate_groupby`, `shuffle_additive`, `shuffle_stddev`; `187` from
`filter_project`. All four rows now have an **empty** `tickets` column, which `registry.rs`'s rule
(`off == 0 || !tickets.is_empty()`, `registry.rs:233-245`) allows because all fifteen cells in each
row are `enabled`. **`187` is now absent from the registry entirely.**

Seven rows still keep `183`, all of them join rows and all `join-backend`'s: `tpch/q4` (`183 220`),
`tpch/q15` (`183 220`), `tpch/anti_join` (`152 183 220`), `tpch/cross_join` (`183 220`),
`tpch/nested_loop_join` (`183`), `tpch/nested_loop_left_join` (`183 220`), `tpch/semi_join`
(`152 183 220`).

So the `183` comment — **`corpus_cases.inc:57`**, not 53 or 56 — is **left as it stands**. It sits
above the T19 batch-2 join block, three of whose rows (`semi_join`, `anti_join`,
`nested_loop_join`) still keep `183`, and `nested_loop_join` keeps it alone with four modes off, so
the sentence is literally true of a row in its own block. Spec item 3 removes the comment only
"when no row keeps `183`", and seven do.

Two comments next to the four lines *were* wrong after the edit and are fixed:

- `corpus_cases.inc:45` said "The other four run at tp1-single, bar cross-join" — now
  "filter-project, aggregate-groupby and shuffle-additive run everywhere on both engines;
  cross-join is off on #220's join batching."
- `corpus_cases.inc:70` said "shuffle-stddev runs at tp1-single on the device" — now "runs
  everywhere on both engines".

### Counts

`build-test.md`: grand total 2991 → **3023**, Rust 2483 → **2515**. The cpu rung 1692 → **1708**
(`test_cpu_corpus` 967 → **983**: +16 `duckdb_gpu_*` cases, row 966 → 982) and the gpu rung 671 →
**687** (`test_gpu_corpus` 79 → **95**, row 78 → **94**). The DuckDB tier's share 257 → **273**. The
device prose: "Seventy-seven cells" → "Ninety-three", twenty-six tpch/tpcds → forty-two with the
four rows moved into the every-mode list, and "the seventy-eighth case" → "the ninety-fourth".

### Commands and evidence

```bash
# local, before the device run — the RED that named exactly the 16 cells
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- every_enabled_device_cell
#   FAILED: missing [("aggregate-groupby","tp1-rowgroup"), … 16 pairs …], not an enabled cell []

rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ dmitry@89.169.109.150:peacockdb-J/
# on the host, in ~/peacockdb-J, after . ~/peacock-env.sh
./scripts/build-test-shadgpu.sh --build                       # 4 binaries staged, ~4 min
PCK_WRITE_GPU_RESULT=1 \
  LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib \
  PEACOCK_TESTDATA_DIR=$PWD/testdata \
  ./cpp/install/rust-tests/test_gpu_corpus --nocapture --test-threads=1
#   ok. 95 passed; 0 failed — 29.06s
rsync dmitry@89.169.109.150:peacockdb-J/testdata/goldens/tpch.sf1/gpu-result.txt …   # by hand
```

Then locally, with the pulled file: `duckdb_gpu_tpch_` **38 passed, 0 failed** (20 of them the four
rows'); the whole rust-only tier `cargo test --features rust-only -p peacockdb-core` **rc=0**, 694 +
11 + 26 + 3 + 983 + 43 + 18 = 1778 cases, 0 failed, 2 ignored (#182's pair); `cargo test -p
cost-report` 41 passed. Then the registry re-synced with the tags struck and the device binary rerun
with recording **off**: **95 passed, 0 failed** in 22.24 s, and `gpu-result.txt`'s md5 unchanged on
the host, which is the proof that the committed file is what a non-recording run is held to.

### Host notes for the next run here

- `pgrep -f "build-test-shadgpu --build"` as a liveness probe **matches its own ssh command line**
  and so reports RUNNING forever. It cost 15 minutes of polling a build that had already finished.
  Poll a `rc` marker file the detached command writes, not a process pattern.
- `ssh host 'cmd &'` without `setsid` and a redirect of ssh's own stdout hangs the ssh client until
  the child closes the channel; the child does survive. `setsid nohup … > /dev/null 2>&1 < /dev/null &`
  returns at once.
- The 95-case device binary is **30 seconds**. Nothing about this tier needs detaching; it was
  detached here only to keep a timeout from owning the result.

### Deferred by the host override

- `--run-benchmarks` and the three `bench_` cases (the sf40 measurement) — not run.
- Nsight (`--trace`, `--metrics`), its captures, `calls.tsv`/`hbm.tsv` and the panels — not run.
- Any H200 timing, and the sf40 pair (`peacock_tpch_tests`, `peacock_tpchv_tests`) — the sf40
  dataset lives only on shad-gpu, which is down.
- `build-test-shadgpu.sh --run` and `--pull-results` — both ssh to shad-gpu; the staged binaries
  were run directly and `gpu-result.txt` came home by plain `rsync`.
- The C++ suites (`PCK_RUN_CPP`) — no C++ changed, and running the binaries directly bypasses them.
- The CPU half stayed local: verda is unreachable and `VERDA_CLIENT_ID` is unset.

### Drift found and not fixed

Two stale `#187` references, both pre-existing and outside this task's Restriction:

- `llm-wiki/tickets/corpus-coverage.md:662`, inside **#186**'s description: "Its device cells then
  meet the decimal export (#187)". #187 is archived Done, so a cell turned on today does not meet
  it — `filter_project`'s four new cells are the evidence.
- `llm-wiki/tasks/stale-cells-impl.md` and this file's earlier sections cite
  `corpus_cases.inc:46,47,49,73` and the comment at 53/56. The real line numbers are **47, 48, 50,
  74** and **57** — one further off than the earlier correction here recorded. Registry rows
  123/126/137/139 were right.

`llm-wiki/architecture.md:1185`'s `(#187)` is **not** drift: it names the failure mode the declared
precision avoids, in a column that describes what would go wrong without it.
