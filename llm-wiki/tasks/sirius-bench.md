# SiriusDB on the H100 beside peacockdb and DuckDB, and a flat CSV of every result

Kind: production

The report page compares peacockdb on the H100 with DuckDB on 128 cores. The other GPU SQL
engine worth measuring against is SiriusDB (`github.com/sirius-db/sirius`), a DuckDB extension
that decodes parquet with libcudf. This task times Sirius on the same H100, over the same queries,
starting from the same place peacockdb starts — the raw parquet bytes already in pinned host
memory — and puts it on the page between peacockdb and DuckDB. It also writes every result the
page holds into one CSV, for ad hoc analysis scripts.

Today's figures, for scale (`ENS-containers-only`, both engines `ok`): Σ total peacockdb / Σ total
DuckDB is 0.88 over 47 tpch sf40 queries and 1.53 over 77 tpcds sf40 queries.

## Done when

1. **The chain is rebased onto the report changes first.** Two report commits landed on lower
   branches after the branches above them were built: `fc4decfb` on `ENS-query-fuser`
   (fused-batches tables with the nonfused totals, reads as `n/m`, multi-line cells) and
   `a647a223` on `ENS-tp4-sized` (the tp1_single main table per dataset, the per-device Modes
   tables under a collapsed section). Each descendant is rebased onto its rebased parent, lowest
   first: `ENS-batching-report` onto `ENS-query-fuser`, `ENS-tp4-sized` onto `ENS-batching-report`,
   `ENS-containers-only` onto `ENS-tp4-sized` (`git rebase --onto <new parent> <old parent tip>`),
   each force-pushed (with lease) to its open PR. This
   task's branch then starts from the rebased `ENS-containers-only`. Conflicts resolve to the
   report commits' intent; `testdata/benchmark-report.html` is regenerated on each rebased branch
   rather than merged by hand.
   - **How these rebases are verified, and when the rebased tasks are `done` again, is the
     chain L note "Report rebase, 2026-11-09" on the board** (`llm-wiki/tasks/tasks.md`), which
     the coordinator reads at startup: report tests only, no build, no review, `done` without
     waiting for CI. The board marks the three tasks `rebase needed(done)`, so the coordinator's
     progress rule runs these rebases before it starts this task.
2. **Sirius runs on the H100 through the existing path.**
   - **Version:** Sirius's `stable` tag (commit `8d17afe`: DuckDB 1.5.6, libcudf 26.08, CUDA 13
     or 12), pinned by commit. Not `latest`, which moves with main and is on DuckDB 2.0.
   - **Image:** a Sirius run image built from committed inputs by `scripts/env/image.sh` and
     published by `images.yml`, under the same input-label rules as the other images. It is
     image.sh's scheme with a third kind, `sirius`, at Sirius's libcudf version:
     `ghcr.io/asymptote-tech/peacockdb-sirius:cudf-26.08`, with the inputs tag
     `:cudf-26.08-in-<hash>` and the commit tag `:cudf-26.08-<commit>`. The Sirius commit is one
     of its pinned inputs, so a new pin is a new inputs tag.
   - **Prebuilt, not compiled.** The image installs the `stable` release's CUDA 13
     `.duckdb_extension` for linux-amd64 (the H100's 580.173 driver meets its ≥580.65.06),
     loaded by Python `duckdb==1.5.6`. The extension links cudf 26.08.01 statically and needs only
     libcuda, libnvidia-ml, libstdc++ and glibc 2.28, so the image carries no libcudf runtime and
     no conda. The `stable` release asset is replaced whenever Sirius promotes a new commit, so
     the `.gz` (921,613,272 bytes, sha256 `01e4797c…` in full in the pin) is mirrored where the
     project keeps its other pinned downloads, and `docker/pins.env` pins the mirror's URL and
     sha256. No Sirius source build, no pixi. If the prebuilt file cannot load or run in the
     image, stop and ask rather than compiling. It is the only new image: there is no Sirius build
     image, and the existing images are unchanged. Sirius ships no Docker image.
   - **The image's CI self-test cannot load the extension:** a GPU-less runner lacks libcuda and
     libnvidia-ml. It checks the sha256, the extension's footer (`v1.5.6 linux_amd64`) and its
     library list instead.
   - **Run:** on the H100 through `scripts/env/run-gpu.sh`, as peacockdb's benchmarks are.
3. **Apples-to-apples, warm from pinned buffers.** Before any timed run, every table of the
   dataset is pinned once with `CALL pin_table(..., tier='parquet')`: its raw parquet column-chunk
   bytes sit in Sirius's pinned host cache, and every timed run copies them to the device and
   decodes them there, as peacockdb's pinned cache does. The pin is not timed and is outside any
   query timeout. Two settings depart from Sirius's defaults, deliberately, because pinning
   requires them: `sirius.executor.scan_manager.cache.mode: sirius` (without it `pin_table`
   refuses) and `sirius.executor.scan_manager.cache.eviction_threshold_fraction: 1.0` (at the
   default 0.8 pinned chunks can be evicted). Also:
   - `SET enable_duckdb_fallback = false`: a query Sirius cannot run on the GPU errors and is a
     `failed` row with its reason, never a silent CPU run. At `stable`, TPC-DS's INTERSECT/EXCEPT
     queries (q8, q14, q38, q87) are expected to fail this way (q51 is not in the H100's list), and
     Sirius refused a DISTINCT with "Distinct not supported" on the H100, so queries using
     DISTINCT may fail too.
   - Quent telemetry off (`sirius.telemetry.enable_quent: false`; it is on by default).
   - Sirius's defaults otherwise, memory included: its GPU pool (95% of device memory) and its
     pinned host cache are its own, not peacockdb's 69 GiB pool. Its scans overlap its compute,
     and that is the engine being measured. A query that runs out of memory is a `failed` row.
4. **The timing protocol is peacockdb's.** The queries are every row of the H100's `cases.csv` at
   sf40, both datasets: `ok`, `wrong` and `skipped` alike. A `skipped` row's note is peacockdb's
   own refusal (tpcds q17/q25/q29's accumulator budget, tpch anti_join's GpuUnload), so Sirius
   runs those queries too, the human's choice. DuckDB's sf40 run covered only the `ok` and `wrong`
   rows, so those queries have no DuckDB answer or time: a timed Sirius row there is
   `unverified` (item 5).
   Per query: one warm-up, then `MEASURED_RUNS` runs, the second-smallest reported. **One Sirius
   process per dataset**, the human's choice: pinning reads around the page cache at the H100's
   ~100 MB/s, so pinning per query would cost about 3.4 h of reads for both datasets against
   about 11 min once per dataset. The time is the query's wall time as the harness measures it
   around the statement and an Arrow fetch of its result (not Python tuples: some sf40 answers
   are 96 M rows). A query that fails or times out does not end the process for the next one;
   a crashed process is restarted, re-pinned, and resumes at the next query.
   - **Only a total.** Sirius exposes no clean load/execute split: its scans overlap its compute,
     and its per-operator times are busy time, not a share of the wall. Load and execute are not
     reported.
   - **No peak.** Sirius's pool peak is a process-lifetime high-water mark that also counts
     reservations; it is not comparable with the accountant's peak and is not reported.
   - **One script runs it, by flags.** A single entry point (my call: `scripts/env/bench-sirius.sh`)
     does the whole run from the laptop: ship, pin, time, collect answers, pull results home. It
     takes flags, not edits or environment variables, in the style of `bench-each.sh` and
     `duckdb_minimal.sh`, with `--help`. At least (names the builder's, meanings fixed):
     `--instance-id`/`--profile` (passed to `run-gpu.sh`), `--dataset tpch|tpcds|all`, `--sf`
     (40 by default), `--cases <cases.csv>` (the H100's by default), `--query <q>` (repeatable,
     narrows the list), `--runs <n>` (`MEASURED_RUNS` by default), `--resume` (keep rows this
     Sirius commit and host already wrote), `--dry-run` (print what would run, run nothing).
     Bad input is refused before any call. Offline tests over a fake `run-gpu.sh` hold the flags
     to what they do, as `test_bench_each.py` does.
5. **Answers are checked.** Each query's Sirius answer is held to DuckDB's stored sf40 answer under
   the same oracle and tolerance rules `every_sf40_device_answer_meets_duckdb` applies to
   peacockdb's. A mismatch makes the row `wrong`. A query with no stored DuckDB sf40 answer
   (peacockdb's `skipped` rows) makes a timed row `unverified`: shown, never `ok`, and out of the
   ratio. The answer is the Arrow result of a Sirius
   `SELECT`: `CREATE TABLE … AS` does not run on Sirius and would store DuckDB's own answer. The
   harness writes that Arrow table into a DuckDB answers database as table `<q>`, as
   `duckdb_minimal.sh --answers` does; after `pull`, the laptop renders it with
   `testdata/duckdb_result.py --answers … --out` (pinned to DuckDB 1.5.4, so not in the image)
   into `testdata/goldens/<ds>.sf40/sirius-result.txt`, `duckdb-result.txt`'s format. Queries are
   named by their stem everywhere (`aggregate-groupby`, not the list's `aggregate_groupby`). The
   harness judges nothing itself; an `UPDATE_CANONICAL`-style Rust
   step sets each row's status in `sirius.tsv` from that comparison, and a test holds the
   committed statuses to it, as peacockdb's device answers are held.
6. **Results live in the device's tree.** `testdata/benchmark-results/h100-80gb-hbm3/<ds>.sf40/sirius.tsv`,
   one per dataset: a `# ` provenance line (Sirius commit, DuckDB, libcudf, CUDA, driver, device,
   host) and one row per query: its time, its runs, and its status (`ok`, `wrong`, `unverified`,
   `failed`, `skipped`) with a reason where it is not `ok`. The engine is in the file name, not the
   directory. A Rust reader parses it, as `parse_duckdb_results` parses `duckdb.tsv`.
7. **The page: new columns in each dataset's main table.** Today's main table
   (`h100-80gb-hbm3 · tp1_single`) has, left to right: `query`; the h100 group `load (ms)`,
   `execute (ms)`, `total (ms)`, `peak ≥ (GiB)`; the DuckDB group `warm (ms)`. This task inserts
   one group between the h100 group and DuckDB, so the table becomes:

   | query | h100-80gb-hbm3 · tp1_single: load, execute, total, peak | **sirius · h100-80gb-hbm3: load (ms), execute (ms), total (ms)** | duckdb-cpu128: warm (ms) |
   |---|---|---|---|

   - First header row: `sirius · h100-80gb-hbm3`, spanning its three columns. Second row:
     `load (ms)`, `execute (ms)`, `total (ms)`. No peak column.
   - **load (ms), execute (ms):** always `—`.
   - **total (ms):** the second-smallest of the timed runs, one decimal, linked to its line in
     `sirius.tsv`. A `wrong` row: the time plus `wrong answer`, as peacockdb's. An `unverified`
     row: the time plus `unverified` (title: no DuckDB answer to check it against). A `failed` row:
     `failed: <reason>` coloured as peacockdb's failures. A `skipped` row: `skipped: <note>`
     coloured as peacockdb's skips. No Sirius row for the query: `—`.
   - **Ratio line**, a paragraph between the dataset's heading and its table:
     `Σ total Sirius / Σ total DuckDB = <r> over <n> of <m> queries`. `<r>` has two decimals,
     `<n>` counts the queries where both Sirius's and DuckDB's rows are `ok`, and `<m>` counts
     the main table's rows. A dataset with no `sirius.tsv` has no line.
   - The Modes tables, the fused tables, and the h200 table do not change.
8. **`testdata/benchmarks.csv`.** Every result the page holds, fused batches excepted, one row
   each. Columns, in order:
   `dataset,sf,query,engine,device,mode,load_ms,exec_ms,total_ms,peak_gib,status`.
   - `engine`: `peacockdb`, `sirius`, `duckdb`. `device`: the result tree's directory name
     (`h100-80gb-hbm3`, `h200`, `duckdb-cpu128`).
   - peacockdb: every device and every mode it has a section for. DuckDB: `mode`, `load_ms`,
     `exec_ms` and `peak_gib` empty, its warm time in `total_ms`. Sirius: `mode` `tp1_single`, `load_ms`,
     `exec_ms` and `peak_gib` empty.
   - `status`: `ok`, `wrong`, `unverified` (Sirius only), `failed`, `skipped`, taken as the page takes it (a device list's
     `skipped` and `wrong` rows over the section). A `failed` or `skipped` row leaves the times
     empty; a `wrong` or `unverified` row keeps them, as the page shows them.
   - Written by the same test that writes the page, from the same trees, regenerated in every
     commit that regenerates the page, and checked byte for byte the same way.
9. **The wiki.** `build-test.md` describes the Sirius image, its run, its protocol and its file;
   "The page" and the report tests' table follow; `benchmarks.csv` joins the generated-files
   table. The H100's free disk after `prepare` is corrected from 81 GB to what the task measures
   (the analyst read 60 GB).

10. **Done is the chain's usual rule for this task itself:** it reaches `done` with its PR's CI
    green.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/prototypes/fuser/report*`, `peacockdb-core/src/test_support/benchmark_report*`, `testdata/benchmark-report.html`, `llm-wiki/build-test.md` | Done when 1: regenerated on each rebased branch |
| `docker/` (a Sirius Dockerfile and its pinned inputs), `scripts/env/image.sh`, `.github/workflows/images.yml` | the Sirius run image |
| `scripts/env/` or `benchmarks/` | the Sirius harness: pin, run, time, collect answers, write `sirius.tsv` |
| `peacockdb-core/src/test_support/` | the `sirius.tsv` reader, the page's Sirius group and ratio line, `benchmarks.csv` |
| `peacockdb-core/tests/test_corpus_goldens/` | the page and CSV test; Sirius's answers against DuckDB's |
| `testdata/goldens/{tpch,tpcds}.sf40/sirius-result.txt` | Sirius's answers |
| `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/sirius.tsv`, `testdata/benchmarks.csv`, `testdata/benchmark-report.html` | results and generated files |
| `scripts/env/tests/` | the harness's and the image's offline tests |
| `llm-wiki/build-test.md` | Done when 9 |

Component-level API: none. No engine change in peacockdb.

## Constraints

- **Publishing the Sirius image needs nothing on master first.** `images.yml` is on master
  already, and a dispatch runs the workflow file of the ref it names (`--ref`), so the branch
  adds its Sirius image to its own `images.yml` and dispatches on itself. A branch dispatch pushes
  the inputs and commit tags only, which is all `run-gpu.sh` reads. If the first push to the new
  package is denied, the human grants the repository Write under the package's "Manage Actions
  access", as the header of `images.yml` says.
- **The H100's disk** holds the new image beside the two it has: `run-gpu.sh prepare` checks free
  space before the pull, and the task records the image's size.
- **Sirius's container needs three things the run image's does not** (each confirmed on the
  H100): a seccomp profile that allows `io_uring_setup`, `io_uring_enter` and `io_uring_register`
  (Docker's default refuses them and every scan fails), `mbind`, `set_mempolicy` and
  `get_mempolicy` in it or `--cap-add SYS_NICE`, and `--ulimit memlock=-1:-1` (at the default
  8 MiB Sirius drops to a slower path). The profile is committed (its source named, since moby's
  default profile JSON moved) and shipped by `run-gpu.sh prepare`. O_DIRECT works on the H100's
  `/var/lib/peacock` (ext4).
- **Its driver needs:** ≥580.65.06 for CUDA 13, ≥570.26 for CUDA 12. The H100 reads 580.173.
- **H100 time** follows the chain's VM rules: `run-gpu.sh` never starts or stops the VM.
- **No Sirius change.** If Sirius cannot do what Done when 3 asks without one, stop and ask.
- No test is weakened, and DuckDB's pinned version and its results stay as they are.

## Not in this task

Fused batches in `benchmarks.csv`; Sirius on any other device or mode; a Sirius load/execute
split or peak; Sirius's TPC-DS fixes past `stable`.
