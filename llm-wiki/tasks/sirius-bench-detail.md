# sirius-bench — detail

## Run log

- 2026-11-09: Task 1 (the report rebase) done by the coordinator before this branch: batching-report
  `20156351`, tp4-sized `20c31566`, containers-only `1eaefe39`, each verified by the report tests
  and pushed, each `done`. `ENS-sirius-bench` branched from `ENS-containers-only` at `1eaefe39`;
  PR target `ENS-containers-only`. Next: developer at Task 2 Step 1.

## Developer notes

### Task 2 (sirius.tsv reader and writer), 2026-11-09

- Files: `peacockdb-core/src/test_support/sirius_results.rs` (+ `sirius_results/tests.rs`, 14
  tests), the types and two `pub fn`s in `test_support/mod.rs` under "Sirius's results",
  `testdata/fixtures/sirius-results.tsv`, and a paragraph in `testdata/fixtures/README.md`.
- Format decisions beyond the plan's list, so Task 7's runner writes to them:
  - The `# ` line must hold exactly the seven keys `sirius duckdb libcudf cuda driver device host`
    in that order (refused at line 1 otherwise). Values are written verbatim; the runner must turn
    spaces into `_` itself (e.g. `device=NVIDIA_H100_80GB_HBM3`), because the Rust writer does not.
  - `skipped` is untimed like `failed` (spec item 8): `ms_2nd_min` and `runs_ms` both `-`.
    `ok`, `wrong`, `unverified` need both present.
  - Note `-` reads as the empty string and the empty string writes as `-`. Notes are not required
    on any status (Task 3 flips statuses without writing notes).
  - Numbers are read leniently (any finite non-negative f64) and written `{:.1}`, so the writer's
    output is the canonical form; a runner that writes `412` rather than `412.0` parses, but a
    rewrite normalizes it. The runner should write one decimal so rewrites are no-ops.
- Fixture preamble uses the short commit `sirius=8d17afe`. If Task 7 writes the full hash, the
  fixture can stay: it is a sample of the shape, not of the pin.
- Mutation check done: removing the key-order filter, the duplicate check, the untimed-with-time
  refusal, or the unknown-status refusal each turns its test red.
- Wiki drift left for Task 10 per the plan: build-test.md's test table has no row for
  `test_support::sirius_results::tests` (14), and the cpu block's `--lib` count (1212) and totals
  are now 14 short.

### Task 3 (Sirius's answers held to DuckDB's, the status test), 2026-11-10

- API added to `test_support/mod.rs`: `AnswerVerdict::NoReference` (unit variant),
  `sf40_sirius_verdict`, `sirius_result_golden`, `sirius_status_for`, and two not in the plan:
  `sirius_results_path(device, dataset, sf)` and `replace_sirius_results(device, dataset, sf, text)`
  (staged and renamed through `testdata::rewrite`, like `write_device_cases`). The plan's
  `write_sirius_results` name was taken by Task 2's renderer, hence `replace_`. Task 4 can use
  `sirius_results_path` rather than building the path again.
- `duckdb_oracle::sf40_verdict` takes a `pub(crate) enum Sf40Answer { Device, Sirius }`. The
  Sirius side (`sirius_verdict`) checks first whether DuckDB's sf40 golden has a section for the
  query; none at all is `NoReference`, before Sirius's file is even looked at. A Sirius section that
  is `failed:`/`skipped:` (duckdb_result.py writes `failed: no answer recorded` for a missing table)
  is `Unanswered`, never compared. The comparison is `Side::Scaled("tp1_single")`, as planned.
- `sirius_status_for`: `failed`/`skipped` keep their status whatever the verdict; a timed row with
  verdict `None` is `unreachable!` (the caller judges every timed row), matching `row_problem`.
- Deviation: `every_sf40_sirius_status_meets_its_answer` judges every timed row (`ok`, `wrong`
  and `unverified`), not just `ok`/`wrong`, so a row stuck at `unverified` after DuckDB gains an
  answer is caught. Datasets are found by listing `<tree>/*.sf40/sirius.tsv`. Under
  `UPDATE_CANONICAL=1` it writes only when a status moved, and still fails on a row whose status
  cannot be set (no Sirius answer, or DuckDB's `failed:`).
- `a_sirius_row_that_meets_duckdb_marked_wrong_is_refused` doctors the shared fixture in memory
  (the plan said a temp copy). The verdicts are passed in, because no committed
  `sirius-result.txt` exists to give a real `Meets`.
- Extra test: `a_query_duckdb_has_no_sf40_answer_for_has_no_reference` (tpcds q17, a `skipped`
  list row with no DuckDB sf40 section). It stays true after Task 9.
- Checked by hand with scratch files, all removed afterwards:
  - a `sirius.tsv` with q17 `ok`: verify fails naming the move to `unverified`; the
    `UPDATE_CANONICAL=1` run rewrites the status only; verify then passes.
  - a q1 row with no `sirius-result.txt`: fails in both modes, naming bench-sirius.sh.
  - a scratch `sirius-result.txt` with DuckDB's own q1 and a doctored q3, against q1 `wrong` and
    q3 `ok`: the test names q1 → `ok` and q3 → `wrong`. So the real `Side::Scaled` comparison runs.
- Messages name `scripts/env/bench-sirius.sh`, which Task 8 creates.
- Wiki drift for Task 10: build-test.md does not yet list the 3 new `sirius_results` unit tests
  (17 now) or the 3 new `test_cpu_corpus` tests.

### Task 4 (the page's Sirius group and ratio line), 2026-11-10

- Code: `test_support/benchmark_report/sirius.rs` (new child module of `benchmark_report`: the
  `Sirius` group, `siriuses`, `sirius_cells`, `sirius_row`, `ratio_line`), wired into
  `dataset_table`. The child exists because the additions took `benchmark_report.rs` to 1008
  lines; it is 879 now. Task 5's CSV adds to the same file and will need the same treatment (a
  `benchmark_report/csv.rs` child, say). Precedent for a production child importing with
  `super::super`: `prototypes/fuser/benchmark/device.rs`.
- Deviation from the plan's Step 4 prediction: a Sirius group appears only for a main-table
  device that has a `sirius.tsv` for some dataset, like the DuckDB column, which appears only with
  a `duckdb.tsv`. With no `sirius.tsv` committed the page did not change at all:
  `UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees` left
  `testdata/benchmark-report.html` byte-identical. A dataset without a file, beside one with,
  shows `—` in the group and no ratio line (Review Focus 3). The test
  `no_sirius_file_anywhere_no_group` pins this choice.
- Ratio line: one per (Sirius group, DuckDB tree) pair, the spec's exact text. Today that is
  exactly one line. With no DuckDB tree at all, the one line counts over nothing. With no query
  `ok` on both sides, `<r>` is `—` (`= — over 0 of m queries`), not a division by zero. `<m>` is
  the rows the main table actually shows (a query no main-table device has a cell for is left
  out, as before). DuckDB rows match by `_`→`-`; Sirius rows match the section name exactly
  (stems).
- Failed and skipped Sirius totals are linked to their line, as peacockdb's failures are.
- Mutation check: counting a non-`ok` Sirius row in the ratio turns
  `the_ratio_counts_rows_both_ok` red.
- Wiki drift for Task 10: build-test.md's "The page" and the report test row (5 new tests, 23
  in `benchmark_report::tests` now) don't mention Sirius yet. The page intro has no sentence on
  what Sirius's time is; the spec doesn't ask for one, so I added none.

### Task 5 (testdata/benchmarks.csv), 2026-11-10

- Code: `benchmark_report/csv.rs` (new child: `render`, the sort key, one line per result);
  `benchmark_report/decided.rs` (new child: `Decided`/`Untimed`/`decided`, the status of a
  device's row, which `shown_of` and the CSV both render from). `benchmark_report.rs` gained
  `Trees` + `trees()` (one loader for the page and the CSV), `render_csv`, `csv_path`. Facade in
  `mod.rs`: `render_benchmarks_csv`, `benchmarks_csv_path`, `write_benchmarks_csv`.
  `sirius_results::status_name` became `pub(crate)` so the CSV spells Sirius statuses through the
  format's own writer. `benchmark_report.rs` is 898 lines after the two splits.
- "Share `shown_of` so a status is decided once": `shown_of` is now `decided` + rendering, with
  no change in behaviour. All 23 page tests that predate this task pass unchanged, and the
  regenerated page is byte-identical to HEAD.
- Subtle, found by a test I first wrote wrong: the list's `skipped` stands over a *failed*
  section only. A timed section with a `skipped` list row stays timed, on the page and so in the
  CSV. That has always been the page's rule; in the committed trees every `skipped` list row's
  section is `status=failed`.
- Rows: every section of every device and mode (h200 included), every `sirius.tsv` row, every
  `duckdb.tsv` row. Order: dataset, sf, query by the page's order (list rank, then name), then
  engine (peacockdb, sirius, duckdb), device name, mode in `MODES` order. Query by stem
  (`_`→`-` for DuckDB's idents). No quoting: no field can hold a comma (stems, slugs, numbers).
- `the_benchmark_report_matches_the_trees` now writes both files under `UPDATE_CANONICAL=1` and
  compares both otherwise, naming the file and the first line that moved.
- Committed CSV: 408 rows + header. 126 duckdb (47 tpch + 79 tpcds); h100 tp1_single 124 ok,
  4 skipped (anti-join, q17, q25, q29), 2 wrong; h100 tp4_sized 120 ok, 5 failed, 1 wrong; h200
  26 rows across its five modes. No Sirius rows until Task 9.
- Wiki drift for Task 10: `benchmarks.csv` is not yet in build-test.md's generated-files table,
  and the 3 new tests (26 in `benchmark_report::tests` now) are not yet counted.

### Task 6 (the peacockdb-sirius image), 2026-11-10 — stopped before Step 1: the mirror

- Obtained and verified. `gh release download stable -R sirius-db/sirius` →
  `sirius-linux_amd64-cuda13.duckdb_extension.gz`, 921,613,272 bytes, sha256
  `01e4797cff315985e66ff829dde3e3bec715482ced2a6142ff1334577638ed1b`. That equals the release's
  `MANIFEST.sha256`, whose `source_sha` is `8d17afec481c56903b61b74a9787423b470298a1` (the
  release is named `stable (8d17afe)`). Kept outside the tree at
  `~/.cache/peacock/sirius-dl/` with the manifest.
- Not mirrored. The plan says to put it in the calibration bucket and pin that URL for
  `docker build`, but:
  - The bucket is private: an anonymous HTTPS GET of a committed record's object returns 403
    (path-style and virtual-host style alike).
  - Every download the existing Dockerfiles make is a public HTTPS URL.
  - `images.yml`'s build job has no storage credentials.
  So a URL pinned in `docker/pins.env` would fail in the build, locally and in CI.
- Upstream offers no fixed URL: the repo has only the moving `stable` and `latest`
  pre-releases. Licence Apache-2.0, so redistributing the binary is allowed (with its licence).
- Options for the coordinator/human (no code written for any):
  (a) upload it to the calibration bucket with a public-read ACL on that one object;
  (b) a separate public bucket for pinned mirrors;
  (c) the bucket stays private, and the build gets storage keys as a BuildKit `--secret`
      (new `images.yml` secrets, and `image.sh` reading peacock.env locally);
  (d) another public host (e.g. a release asset on a public repo, or an OCI artifact on
      ghcr.io, which the workflow already logs into for its push).
- Coordinator's decision, 2026-11-10: (c). The bucket stays private and nothing is published.
  The repo already has the `S3_ACCESS_KEY_ID`/`S3_SECRET_ACCESS_KEY` secrets, so `images.yml`
  passes them to the Sirius build as BuildKit `--secret`s, and `image.sh` reads the same keys
  from `peacock.env` for a local build. The object's bucket path and sha256 (and byte count) are
  the pins; the build verifies the sha256 before installing. The keys never reach a layer, a
  label or the inputs hash.

### Task 6 (the peacockdb-sirius image), 2026-11-10 — built locally, under decision (c)

- Mirror: `s3://calibration/sirius/01e4797cff315985e66ff829dde3e3bec715482ced2a6142ff1334577638ed1b.duckdb_extension.gz`,
  uploaded 2026-11-10 01:01 UTC; head-object reads 921,613,272 bytes; anonymous GET 403. The
  key is named by the content, following the bucket's rule that an object is never replaced. The
  bucket held only per-call records before: these are its first pinned downloads.
- Pins (`docker/pins.env`): `SIRIUS_COMMIT` (full 40 hex), `SIRIUS_EXTENSION_S3`, `_SHA256`,
  `_BYTES`, and `SIRIUS_EXTENSION_UNPACKED_SHA256` (`4e6eff72…ab96`, the 1,510,977,382-byte
  unpacked file). The last is extra to the plan: the self-test checks the installed file against
  a committed hash, not one the build computed. Also `SIRIUS_DUCKDB`, `SIRIUS_LIBCUDF`,
  `DRIVER_MINIMUM_SIRIUS`.
- Keys: `image.sh build sirius` takes `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` from the
  environment (CI: the Build step maps `secrets.S3_*` there for the sirius job only, empty for the
  others). With neither set, it reads them from `~/.config/peacock/peacock.env`, endpoint and
  region included. One without the other is refused. The endpoint and region come from
  `calibration-bucket.sh` (the env's, else Nebius eu-north1). All four go to docker as
  `--secret id=<lowercase>,env=<NAME>`, names only. `docker/sirius-fetch.sh` reads them from
  `/run/secrets` into aws's environment alone. It checks bytes and sha256 before unpacking, then
  the unpacked sha256, and writes nothing on a failure.
- Proof the keys reach nothing (on the built image): `docker history --no-trunc`, `docker image
  inspect`, and every blob of `docker save` decompressed (15 blobs, 1,973,136,653 bytes): 0
  occurrences of the key id or secret. The scan scripts (`scan-keys.sh`, `scan-layers.sh`) are in
  `~/.cache/peacock/sirius-tools/`, outside the tree; they print counts only. Offline: the
  dry-run/fake-docker tests hold the values out of argv, output, label and inputs. A static test
  holds the Dockerfile to: one RUN with secret mounts, in the `fetch` stage; no ARG/ENV naming a
  storage variable; and the final stage taking one file from `fetch`.
- `image-inputs.sh` gained `image_sirius_cudf` and `image_refusal`. `image.sh` and `image-ref.sh`
  now share that one (kind, cuDF) rule; it was duplicated before. The existing images' inputs tags
  are byte-identical to before (checked for all five).
- Extension facts: footer fields (stored reversed) magic `4`, platform `linux_amd64`, DuckDB
  `v1.5.6`, version `dev`, ABI `CPP`; the signature is all zeros (unsigned, so it needs
  `allow_unsigned_extensions`). Its NEEDED list also has `libutil.so.1` and
  `ld-linux-x86-64.so.2` beyond the plan's list; the self-test allows exactly the binary's list.
  The highest symbol version is GLIBC_2.28.
- Sirius's config at 8d17afe: `SIRIUS_CONFIG_FILE`, nested YAML, every key optional, so the
  committed file holds only the three settings. The image sets `SIRIUS_CONFIG_FILE` in its ENV.
- `docker/sirius-bench.py` has only `--self-test` so far; Task 7 adds the runner to it. The image
  has no PyYAML: the self-test reads the config with its own reader for nested scalar mappings.
- Local build: `scripts/env/image.sh build sirius 26.08`, exit 0 (dirty tree, so no commit tag).
  Inputs tag `cudf-26.08-in-1879d6c966ac`, image ID `10793644a3e6`. Size: 1,085,108,272 bytes of
  content (compressed), 1.9 GB unpacked (`du -sx /`), 3.08 GB Docker disk usage (blobs plus
  snapshot). The extension layer is 1.51 GB, the venv 238 MB, Python 154 MB. The plan expected
  2–2.5 GB.
- For the dispatch: Task 7 changes `docker/sirius-bench.py`, an image input, so the inputs tag
  moves after Task 7. An image dispatched after Task 6 is not the one `run-gpu.sh` will look up
  for the run; dispatching after Task 7 avoids a second push.
- Kept outside the tree: `~/.cache/peacock/sirius-dl/` (the verified `.gz` and the release
  manifest).
- Coordinator, 2026-11-10T01:27:39Z: Task 6 committed `a42d8e04`; `images.yml` dispatched
  (`kind=sirius cudf=26.08`), run 42735934061, to prove the secrets path in CI early. It is
  dispatched again after Task 7, whose runner changes the image's inputs.

### Task 7 (the runner), 2026-11-10 — design facts gathered before the code

- Sirius at 8d17afe (README, docs/gpu_execution.md, docs/super-sirius/scan.md,
  src/sirius_extension.cpp, test/tpch_performance/performance_test.py):
  - Python: `duckdb.connect(":memory:", config={"allow_unsigned_extensions": "true"})`, then
    `LOAD '<path>'`. After LOAD every query is intercepted (`gpu_execution` defaults to true).
    Sirius's own benchmark LOADs before it creates the views.
  - `enable_duckdb_fallback` is a connection-scoped boolean, default true ("fallback to duckdb
    execution after an error"). With it false, an unsupported query errors instead of running on
    the CPU.
  - `CALL pin_table('<parquet path>', name = '<table>', tier = 'parquet')`. `tier = 'parquet'`
    caches the raw column-chunk ranges (all columns when `cols` is omitted) and needs
    `scan_manager.cache.mode: sirius`. A query over a view of `read_parquet('<same path>')` is
    matched to the pin automatically.
- Answers-pass settings to copy from `benchmarks/duckdb_minimal.sh` (its `ORACLE`, line ~645):
  `SET default_null_order='nulls_last_on_asc_first_on_desc'` and `SET integer_division=true`. The
  latter is a parser setting, so it must run as its own statement before the query.
- Query files `testdata/<ds>-queries/<stem>.sql` are one statement each; the inner `;` in
  anti-join-reorder, q15, rollup-distinct and semi-join-reorder are inside comments. sf40 data on
  the H100: `/var/lib/peacock/<ds>.sf40/<table>.parquet` (fetch-sf40.sh).
- `testdata/calibration/sirius/` is already ignored by `/calibration/*` in testdata/.gitignore.
- The seccomp profile (`docker/sirius-seccomp.json`) is not in the tree yet. The coordinator
  placed it in Task 7 or 8; it belongs with Task 8, where `run-gpu.sh prepare` ships it and
  `docker run` uses it.

### Task 7 (the runner), 2026-11-10 — done to Step 4

- `docker/sirius-bench.py`: the runner beside `--self-test`. The parent validates everything
  before any query, exit 2: dataset, sf digits, `--runs` ≥ 2 (a second-smallest needs two),
  `--timeout` ≥ 1 s, parquet present, stems matching `[a-z0-9][a-z0-9-]*` with an SQL file each,
  no duplicates, and nvidia-smi answering. The parent never imports duckdb. A worker
  (`multiprocessing` spawn) does LOAD → `SET enable_duckdb_fallback = false` → the two
  answers-pass settings → a view per table → a pin per table → a plain `duckdb.connect(answers)`,
  then reports `pinned`. Per query: one warm-up, `--runs` timed runs of
  `execute(sql).to_arrow_table()`, then the last result is registered as `sirius_answer` on the
  answers connection and `CREATE OR REPLACE TABLE "<stem>" AS SELECT * FROM sirius_answer`.
  The parent writes each row as it arrives (staged and renamed) and sends the next query only
  after that, which is the plan's "acknowledged" rule.
- Deviation: `to_arrow_table()`, not the plan's `fetch_arrow_table()`, which DuckDB 1.5.4 already
  deprecates (checked in the build image). The fake offers only `to_arrow_table`.
- Timeouts count from sending the query (the warm-up's start). On timeout the worker is killed;
  on EOF the row is `crashed (<SIGNAME>)` or `crashed (exit N)`. The next query starts a fresh
  worker, which pins again. A worker that cannot pin is exit 1; the rows written so far stay.
- `--resume` keeps the whole file when its `sirius` and `host` equal this run's, else nothing.
  Whenever nothing is kept the answers database is deleted too, so no stale table renders into
  `sirius-result.txt`.
- **For Task 8 (important):** `host=` comes from `socket.gethostname()`. In a container that is
  the container ID (the smoke run wrote `host=2ec88129d519`), so `--resume` would never keep a
  row. `run-gpu.sh sirius` must pass `docker run --hostname <the VM's hostname>`.
- Tests: `scripts/env/tests/test_sirius_bench.py`, 19 (7 self-test, 12 runner), over a fake
  duckdb (it logs each connect/execute/register as JSON lines and is scripted by
  FAKE_DUCKDB_SCRIPT). Mutations caught: a timed-out worker kept alive; resume ignoring the host;
  an answer-write error written as ok.
- Real-API checks: in the build image (duckdb 1.5.4, pyarrow 25.0.1), register plus the quoted
  hyphenated CTAS into a file database round-trips, and the two settings take effect (7/2 = 3,
  NULLs last). In the rebuilt Sirius image (inputs tag `cudf-26.08-in-564f064de2d2`, self-test
  ok), a GPU-less run with a stand-in nvidia-smi reached LOAD and stopped, exit 1:
  `libnvidia-ml.so.1: cannot open shared object file`. That is the expected stop without a GPU.
- `docker/sirius-seccomp.json` is still not written: Task 8.
- Coordinator, 2026-11-10T01:37:31Z: Task 6's run 42735934061 green. Task 7 committed `d40472d4`;
  `images.yml` dispatched again, run 42737101166 (inputs tag `cudf-26.08-in-564f064de2d2`).
  Dispatch once more before Task 9 if Task 8 moves the inputs tag.

### Task 8 (run-gpu.sh's sirius verb, bench-sirius.sh, the render), 2026-11-10 — in progress

- `docker/sirius-seccomp.json` is written: moby's default, `github.com/moby/profiles`
  `seccomp/default.json` at `245180c51918481c0525424b3ee025d2b435d46c` (tag `seccomp/v0.2.4`, the
  version `docker-v29.9.3`'s go.mod requires; the file is 14,122 bytes, sha256 `785b2429…`), plus
  one last rule allowing the six calls unconditionally. JSON has no comments, so the provenance
  is a top-level `comment` key; moby's `LoadProfile` uses plain `json.Unmarshal`, which ignores
  unknown keys. The canonical JSON of moby's file (sorted keys, no spaces) has sha256
  `20ef2b6bfd980e7dd56a971d9ec0d71bc0f2d1184415f2d968a19bbcff9ee2b2`. The offline test holds the
  profile, less our rule and comment, to that hash.
  - Checked on local Docker 29.2.1 in the Sirius image. Under Docker's default,
    io_uring_setup, mbind and get_mempolicy all get EPERM. Under this profile, io_uring_setup
    gets EFAULT (it passed the filter; the arguments were NULL), and mbind and get_mempolicy
    return 0 without CAP_SYS_NICE.
- Design, as built:
  - `run-gpu.sh prepare --sirius` is the normal prepare plus, at the end, the disk check (only
    when the image is not on the VM yet), the pull and label check, and the driver check (exit 3)
    against the Sirius image's `peacockdb.driver-minimum`. `--no-sf40` beside it is refused:
    Sirius runs sf40.
  - `run-gpu.sh sirius [--detach] -- <runner args>`: the runner args must hold `--dataset` and
    `--sf` and must not hold `--data` or `--out`, which run-gpu.sh supplies (the VM's
    `<data>/<ds>.sf<sf>`, and `/peacock/testdata`). No staged install. It ships
    `testdata/{tpch,tpcds}-queries/` and `testdata/benchmark-results/h100-80gb-hbm3/`. The
    container gets `--hostname "$(hostname)"`, which `--resume` needs.
- Built (Task 8, continued):
  - run-gpu.sh: `KIT` ships `docker/sirius-seccomp.json` on every prepare. `driver_checked`
    takes an image and its prepare (the run image and `prepare` by default). The disk check:
    `SIRIUS_IMAGE_GB=4` (measured 3.08 GB) plus `SIRIUS_SPARE_GB=10`, so 14 GB, under
    `docker info .DockerRootDir`, measured with `df --output=avail -B1` on the VM, in GB of 10^9
    bytes. `vm_pull_ready` now also makes `testdata/goldens/`: a directory that only ever ran
    `sirius` has no `goldens/`, and pull's rsync of it failed (found by
    `test_sirius_detached_is_judged_by_status_and_pulled`).
  - The sirius verb ships `testdata/{tpch,tpcds}-queries/` and
    `testdata/benchmark-results/h100-80gb-hbm3/` with `rsync --update`, so a `sirius.tsv` newer on
    the VM (rows not yet pulled) survives for `--resume`. A pulled but uncommitted `sirius.tsv` on
    dev is shipped too (plain rsync, not `git ls-files`).
  - bench-sirius.sh reads the list with Python's csv (notes hold commas) and keeps every row at
    `--sf`. `--query` takes an ident or a stem; a query that matches no selected dataset is
    refused; with `--dataset all`, a dataset left with no rows is skipped. Single-valued flags
    given twice are refused. `--runs` ≥ 2, matching the runner. The default `--runs` is 10
    (test_support's `MEASURED_RUNS`). Any non-zero run-gpu.sh exit stops the run with that
    code, with no pull and no render, so the caller pulls by hand. The render is
    `$BUILD_TEST_SH`/scripts/build-test.sh (the build image has DuckDB 1.5.4), from the repo root.
  - `testdata/duckdb_result.py --out <file>` needs `--answers`; `generate(..., out=)`.
- The Sirius image's inputs tag did not move in Task 8: still `cudf-26.08-in-564f064de2d2`
  (sirius-seccomp.json is shipped by run-gpu.sh, not read by the Dockerfile).
- New tests: test_run_gpu.py +13 (84), test_bench_sirius.py 11, test_duckdb_result.py +1
  rendering test (needs the duckdb wheel; skipped in CI) and 2 more refusal cases. gpu_host.py's
  fake VM gained the Sirius image (absent until pulled), `docker info .DockerRootDir`, and a
  `df` that answers the world's `free_bytes`.
- Coordinator, 2026-11-10: Task 8 committed `657256f7`; the inputs tag did not move, and run
  42737101166 published `cudf-26.08-in-564f064de2d2` (green). Task 9 (the H100 run) goes to a
  fresh developer: the first one's context was spent on Tasks 2–8.

### Task 9 (the H100 run), 2026-11-10

- VM log: H100 started 2026-11-10T01:57:56Z (was STOPPED; up 01:58:56Z).
- Step 1: `run-gpu.sh … prepare --sirius` 01:59:07Z–02:10:52Z, exit 0. Docker's root had 60 GB free
  before (nothing removed); the Sirius image pulled (`docker images`: 1.97 GB); sf40 kept (tpch 8
  files, tpcds 24). Free after: 62,094,991,360 bytes (`df -h /`: 58G of 174G).
- Step 2: `bench-sirius.sh … --dataset tpch --query q6` 02:10:58Z–02:19:23Z, exit 0. Pinned 8
  tables in 460.3 s; q6 `ok`, 341.0 ms (runs 337.0–378.9). The answer rendered into
  `tpch.sf40/sirius-result.txt`. stdout's only extra lines are the exit-time TLS panic
  (`cannot access a Thread Local Storage value during or after destruction`), after the row.
- Sirius's own log does not reach stdout: its logger is `SIRIUS_LOG_BACKEND` (duckdb, spdlog,
  noop), `SIRIUS_LOG_DIR`, `SIRIUS_LOG_LEVEL`. So the three signatures were checked by a
  diagnostic container on the VM (same image, seccomp, memlock, user; `SIRIUS_LOG_BACKEND=spdlog
  SIRIUS_LOG_LEVEL=info`, tpch sf1, every table pinned, q6 three times; not part of the protocol,
  run 02:21Z). Its log: config loaded from `/opt/sirius/sirius-config.yaml`, q6 as `GPU_SCAN` under
  "Transparent GPU execution"; no fallback line, no `fixed buffers disabled`, no `mbind`, no
  `not permitted`. One warning, Sirius's default: disk spill not configured.
- Step 3, tpch: `run-gpu.sh … sirius --detach -- --dataset tpch --sf 40 --queries <the 48 stems>
  --runs 10 --timeout 900 --resume`, started 02:21:47Z. To resume after a loss: `status`; if the
  container is gone, `pull`, then the same command (it keeps every row this host wrote).

### Human instruction, 2026-11-10: per-query GPU evidence

- **The tpch run of Step 3 was hard-stopped by the human.** The chain was stopped, the container
  `peacock-peacockdb-beta` killed and the H100 soft-stopped at 02:30:37Z. Its rows are discarded,
  q6's included: before the rerun, delete the VM's
  `~/peacock/peacockdb-beta/testdata/benchmark-results/h100-80gb-hbm3/*.sf40/sirius.tsv` and the
  local untracked `tpch.sf40/sirius.tsv` and `tpch.sf40/sirius-result.txt`, and run without
  `--resume`. The "To resume after a loss" line above is superseded.
- **Why.** Nothing in the measured run shows that a query ran on the GPU. Sirius's log is off, and
  `enable_duckdb_fallback = false` is not proof: the analyst saw a `CREATE TABLE … AS` run on
  DuckDB's CPU with fallback off. The sf1 diagnostic proves q6 at sf1 only.
- **New requirement, every Sirius row.** The runner turns Sirius's log on
  (`SIRIUS_LOG_BACKEND=spdlog`, `SIRIUS_LOG_LEVEL=info`, `SIRIUS_LOG_DIR` under
  `testdata/calibration/sirius/<ds>.sf40/`, queries labelled with `sirius_set_query_label`) for each
  query's warm-up, and records whether the warm-up shows the query under "Transparent GPU
  execution" with a GPU operator (`GPU_SCAN`) and no fallback line. The timed runs run at the
  default log level. If the level cannot be switched inside one process, log at info for every
  run, measure the overhead on q6 against the 341.0 ms above, and record it here.
- **A row without that evidence is `failed: not run on the GPU (<what the log showed>)`**, so it
  is out of the ratio. The logs are kept (gitignored, pulled home by `pull`); this file records,
  per dataset, how many rows had the evidence.
- **Work, in order:** Task 7's runner and its offline tests (a fake log with the evidence → `ok`;
  without it, or with a fallback line → `failed: not run on the GPU`); Task 8 only if `run-gpu.sh`
  or `bench-sirius.sh` must pass the log directory; then Task 9 again from Step 2 (q6 first, its
  log showing the evidence), and Step 3 for both datasets without `--resume`. The Sirius image's
  inputs tag moves only if `docker/` changes; republish then, as before.
- Coordinator, 2026-11-10 (restart): resuming at the human instruction's first item, Task 7's
  runner (GPU evidence per query), fresh developer. Then republish the image (`docker/` moves),
  then Task 9 from Step 2.

### Task 7 (GPU evidence), 2026-11-10

- **Sirius's log was never off.** At 8d17afe (`src/config.cpp`) the defaults are backend `spdlog`,
  level `info`, dir `log`, installed at LOAD. The image's WORKDIR is `/tmp`, so the measured q6 run
  (341.0 ms) already logged at info into the container's `/tmp/log/`, discarded with the container.
  The runner now sets `SIRIUS_LOG_BACKEND=spdlog`, `SIRIUS_LOG_LEVEL=info` and
  `SIRIUS_LOG_DIR=<out>/calibration/sirius/<ds>.sf<sf>/log` in the worker before LOAD
  (`src/sirius_context.cpp`, `SiriusContextExtensionCallback`). It is gitignored by
  `/calibration/*`, and `pull` brings it home. The log is kept across runs (appended).
- **The level can be switched in-process:** `SET sirius_log_level = '<level>'` re-targets the
  live sink (`SetLogLevel`, `src/sirius_extension.cpp`). It is not used. The default level is
  info, so the timed runs log at the default, exactly as the 341.0 ms run did. Task 9's q6 should
  land near 341.0 ms; the only difference is that the log now goes to the bind-mounted testdata
  instead of the overlay `/tmp`. Quieter timed runs (`warn`) would depart from Sirius's defaults:
  the spec's call, not made here.
- **The file:** spdlog `daily_file_sink`, so `<dir>/sirius_<YYYY-MM-DD>.log` (local time, UTC in
  the container). It rotates at midnight, and a new worker appends to the same file. Lines are
  `[%Y-%m-%d %T.%e] [%l] [%s:%#] %v`. Sirius flushes them every 3 s (`sirius_log_flush_seconds`)
  and never on demand.
- **The label:** `CALL sirius_set_query_label('<label>')` (a table function over one VARCHAR;
  `src/sirius_extension.cpp:2185`). The label feeds Quent telemetry only, which is off, and it is
  not in any log line. Attribution comes from the CALL statement itself instead. Sirius logs every
  user statement at info as `QueryBegin: instance=… connection=… query=<n> SQL: <sql, whitespace
  collapsed>` (`src/sirius_context.cpp`). Per query the worker records each log file's size,
  runs `CALL sirius_set_query_label('<stem>')`, the warm-up, then
  `CALL sirius_set_query_label('<stem> timed')` (that label goes to the first timed run). It then
  waits up to 10 s for the second CALL's line and judges only the text between the two lines,
  past the recorded sizes, across every `sirius*.log`. This handles late flushes of the previous
  query, an earlier run's log, a rotation and a restarted worker. The wait costs up to about 3 s
  per query, outside the timed runs and inside `--timeout`.
- **Evidence (all three needed):** `Transparent GPU execution: executing query`
  (`src/transparent/physical_sirius_execution.cpp:161`), then `GPU_SCAN` after it. The plan is
  logged as `Query Plan:\n…` by `sirius_engine.cpp:344` for every execution, with operators named
  by `SiriusPhysicalOperatorToString`. And no fallback line.
- **Fallback signatures** (matched by `Transparent execution fallback|falling back to DuckDB
  CPU|fallback to DuckDB`):
  - runtime, `physical_sirius_execution.cpp:357`: `Transparent execution: GPU execution failed at
    runtime; falling back to DuckDB CPU within the same transaction. GPU error: …` (WARN);
  - plan time, `sirius_context.cpp` `OnFinalizePrepare`: `Transparent execution fallback
    (unsupported): …`, `… (replan unsupported): …`, `… (replan failed): …`, `… (runtime
    unavailable): …` and `Transparent execution fallback: …` (INFO).

  With fallback off these throw instead of logging. The banner `Error in Sirius GPU execution,
  fallback to DuckDB` goes to stdout, not the log.
- **The silent CPU path** is why the evidence is positive and not merely "no fallback". A
  non-SELECT statement, a `should_use_duckdb` decline, or an unserializable plan (logged at DEBUG
  only, `sirius_optimizer_extension.cpp:301`) runs on DuckDB's CPU with no line at info and no
  error, even with `enable_duckdb_fallback = false`.
- **A row without evidence** is `failed: not run on the GPU (<shown>)`, where `<shown>` is one of:
  `no Transparent GPU execution line`; `Transparent GPU execution, no GPU_SCAN in its plan`; the
  fallback line's message (the first 100 characters); or `no log of its warm-up in 10 s`. Such a
  query is not timed and has no answer. Every `ok` row had the evidence, so the per-dataset count
  is the number of rows not `failed: not run on the GPU`.
- All eleven strings were found in the shipped extension (`grep -a` in the image). The runner
  could not be run against real Sirius here (no GPU). **Task 9's q6 step is the first real check
  of the markers:** q6 must come back `ok`, and its log must show the two `QueryBegin … SQL: CALL
  sirius_set_query_label(…)` lines around the warm-up.
- Tests: `test_sirius_bench.py` 28 (19 before). The fake DuckDB writes a Sirius-shaped log
  (QueryBegin per statement, buffered and flushed on a timer, daily files). Ten mutations were
  caught, among them: fallback, `executing` and `GPU_SCAN` checks dropped; the start label
  ignored; the sizes ignored; the newest file only; the log environment unset; the end label
  dropped; timed runs despite no evidence; no wait for the flush.
  `test_times_ten_runs_and_reports_the_second_smallest` flaked under disk load (2/15 on HEAD's
  unchanged code), because the fake's call log is written inside the timed run. Its scripted runs
  are now 50 ms apart instead of 10 (0/15 under the same load).
- Task 8 is unchanged: the runner derives the log directory from `--out`, `run-gpu.sh` mounts
  the VM's testdata writable, and `pull` already rsyncs `testdata/calibration/`.
- Image rebuilt locally: inputs tag `cudf-26.08-in-f2f77ce2bf3c` (image f39a2a0f93f5,
  3.08 GB on Docker's disk as before). `--self-test` ok. The image's runner is byte-identical to the
  tree's. **Republish before Task 9:** `gh workflow run images.yml --ref ENS-sirius-bench -f kind=sirius -f cudf=26.08`.
- Coordinator, 2026-11-10T03:02:25Z: GPU-evidence runner committed `fcf0bfb8`; `images.yml`
  dispatched (`kind=sirius cudf=26.08`), run 42746775895, for inputs tag
  `cudf-26.08-in-f2f77ce2bf3c`. Next: Task 9 from Step 2 once it is green.

### Task 9 (the H100 run, with GPU evidence), 2026-11-10

- **VM:** `nebius.sh … start` at 03:09:35Z (was STOPPED; RUNNING 03:10:33Z), soft `stop` at 03:58:59Z,
  exit 0. VM time used: 49.4 min.
- **Before the rerun:** the VM's `tpch.sf40/sirius.tsv` was deleted (no tpcds one existed). The
  local untracked `sirius.tsv` and `sirius-result.txt` were already gone, and the stale local
  `calibration/sirius/tpch.sf40/answers.duckdb` was removed too. No run used `--resume`. The killed
  container from 02:30Z (exit 137) was still there; `vm_not_running` removed it.
- **Step 1:** `run-gpu.sh … prepare --sirius` 03:10:46Z–03:22:26Z, exit 0; sf40 kept, image
  `cudf-26.08-in-f2f77ce2bf3c` pulled (1.97 GB). Free on `/` (Docker's root): 62,020,153,344 bytes
  before, **60,111,265,792 after** (`df -h /`: 56G of 174G). Nothing was removed. The old tag
  `cudf-26.08-in-564f064de2d2` (1.97 GB) is still on the VM and can go when space is short.
- **Step 2:** `bench-sirius.sh … --dataset tpch --query q6` 03:22:49Z–03:30:58Z, exit 0. It pinned 8
  tables in 460.1 s. q6 `ok`, **339.4 ms** (runs 336.5–393.0), against **341.0 ms** before:
  −1.6 ms (−0.5%), inside the spread. Its log (`calibration/sirius/tpch.sf40/log/sirius_2026-11-10.log`)
  has `QueryBegin … SQL: CALL sirius_set_query_label('q6')` (line 104), then `Transparent GPU
  execution: executing query` (108) and `Pipeline #0: GPU_SCAN (id=0) -> PROJECTION (id=1) ->
  UNGROUPED_AGGREGATE (id=2)` (117), then `… label('q6 timed')` (157). That is 11 GPU executions
  in all (the warm-up and 10 timed runs). There is no fallback line, no `fixed buffers`, no
  `mbind`, and no `not permitted`. The one warning is Sirius's default "disk spill not
  configured".
  - **What this q6 time measures:** both q6 runs logged at info (Task 7 (GPU evidence): the 341.0 ms
    run's log went to the container's `/tmp`). So the −1.6 ms compares logging to the bind-mounted
    testdata with logging to the overlay: no measurable cost. Info logging against no logging was
    never measured, because no run had the log off.
- **Step 3:** detached `run-gpu.sh … sirius --detach -- --dataset <ds> --sf 40 --queries <every list
  row> --runs 10 --timeout 900`, with no `--resume`, polled with `status`.
  - tpch: started 03:31:34Z, log ends 03:48:56Z, exit 0. It pinned 8 tables in 464.5 s. 48 rows.
  - tpcds: started 03:49:34Z, log ends 03:57:20Z, exit 0. It pinned 24 tables in 163.2 s. 82 rows.
  - One worker per dataset: no timeout, no crash, no restart. Then `pull` and the render at home
    (`build-test.sh -- python3 testdata/duckdb_result.py … --out …/sirius-result.txt`), both exit 0.
    The pulled `~/.cache/peacock/run-gpu/peacockdb-beta/sirius.log` holds only tpcds's stdout, because
    the tpcds run's log replaced tpch's.
- **GPU evidence:** every timed row has it, 38 of 48 tpch rows and 46 of 82 tpcds rows. No row is
  `failed: not run on the GPU`. The other 10 and 36 rows are `failed` with Sirius's own error,
  because fallback is off. An independent check over the pulled logs agreed: for each timed row,
  the last warm-up segment between its two label lines holds the execution line and `GPU_SCAN`,
  and no fallback line. (The script was a scratch file in `/tmp`, not kept.)
- **Step 4** (`build-test.sh`): `UPDATE_CANONICAL=1 … every_sf40_sirius_status_meets_its_answer`
  moved 7 rows:
  - to `unverified`, because DuckDB has no sf40 answer (peacockdb's `skipped` rows): tpch
    anti-join; tpcds q25 and q29. tpcds q17 is `failed`, so it is untimed.
  - to `wrong`:
    - tpch `nested-loop-left-join` (`region LEFT JOIN nation ON r_regionkey > n_regionkey`).
      Sirius applies the `>` reversed, as `r_regionkey < n_regionkey`: AFRICA (0) pairs with all 20
      nations of regions 1-4, ARGENTINA (1) among them, and MIDDLE EAST (4) with none. A real wrong
      answer. (Corrected after the fix-commit review; first recorded as the inequality not applied.)
    - tpcds q31: rows with a negative last column that DuckDB lacks, and missing rows. A real wrong
      answer.
    - tpch `scan-limit` (`SELECT * FROM lineitem LIMIT 10`, no ORDER BY): another 10 rows. Any 10
      rows are a correct answer, so the oracle is too strict here.
    - tpcds q73 (`ORDER BY cnt DESC, c_last_name ASC`): the same rows, with two "Jones" rows (a
      tie) in the other order. peacockdb's q73 is `wrong` too (#306).
  - Then `UPDATE_CANONICAL=1 … the_benchmark_report_matches_the_trees` rewrote the page and the CSV.
    Both tests then passed without the variable. So did all of `test_corpus_goldens` (66), the
    `sf40`/`sirius` tests of `test_cpu_corpus` (4), and the lib's `sirius_results` and
    `benchmark_report` tests (43).
- **Ratio lines:**
  - tpch: `Σ total Sirius / Σ total DuckDB = 1.29 over 35 of 48 queries`.
  - tpcds: `Σ total Sirius / Σ total DuckDB = 1.32 over 42 of 82 queries`.
- **Final statuses:** tpch 35 ok, 2 wrong, 1 unverified, 10 failed. tpcds 42 ok, 2 wrong, 2
  unverified, 36 failed. `benchmarks.csv` has 130 new Sirius rows (538 rows and the header).
- **`failed` rows, with Sirius's reason** (`Not implemented Error: GPU plan generation failed: …`
  unless marked *runtime*, which is `Sirius GPU execution failed: …`):
  - tpch:
    - Cross product not supported: cross-join, nested-limits.
    - `Unsupported expression in aggregate`: distinct-functions (`stddev(DISTINCT …)`),
      shuffle-stddev and global-stddev (`stddev_samp`).
    - `from_duckdb: Unsupported DuckDB type: INTERVAL` (Invalid Input Error): mixed-join.
    - ROLLUP, CUBE, GROUPING SETS and GROUPING() are not supported in GPU aggregates:
      rollup-distinct, rollup-grouping, rollup-over-join, rollup-stddev.
  - tpcds:
    - Distinct not supported: q6, q8, q14, q38, q41, q54, q75, q87. The spec expected
      INTERSECT/EXCEPT failures; q8, q14, q38 and q87 are among these.
    - Distinct aggregates not supported in GPU path yet (*runtime*): q16, q94, q95.
    - ROLLUP/CUBE/GROUPING SETS: q5, q18, q22, q77, q80.
    - Cross product not supported: q9, q23, q28, q61, q88, q90.
    - `Unsupported expression in projection`: q2 and q78 (`round(… // …, 2)`), q99 (`lower(cc_name)`).
    - `Unsupported expression in aggregate`: q17 and q39 (`stddev_samp`).
    - `Unsupported filter predicate` (`… constant_or_null(…)`): q13, q15, q48.
    - `Unsupported expression on the right side of a join condition` (`upper(ca_country)`): q24.
    - *runtime*, `CUDF failure at: …/cpp/src/io/parquet/bloom_filter_reader.cu:82: Mismatched
      predicate column and literal types`: q33, q43, q56, q60, q91.
- **Files for the Step 5 commit:** the two `sirius.tsv` files, the two `sirius-result.txt` files,
  `testdata/benchmark-report.html`, `testdata/benchmarks.csv` and this file. The logs and answer
  databases under `testdata/calibration/sirius/` are gitignored and stay at home.
- **Wiki drift for Task 10:** build-test.md "Disk" says 81 GB is free on the H100 after `prepare`.
  The measurement is 60.1 GB (10^9 bytes) with both Sirius tags present.
- Coordinator, 2026-11-10: Task 9 committed `8b4d403e`. Next: Task 10 (wiki, test counts), then
  the PR on `ENS-containers-only` and `reviewing`.

### Task 10, 2026-11-10

- Steps 1 and 2 done; `build-test.md` only. Counts, from runs on this tree (all through
  `scripts/build-test.sh`, rust-only):
  - `--lib` 1237 (1235 passed, 2 ignored on #182), `test_cpu_corpus` 2454, `test_corpus_goldens`
    66, `test_cost_model` 3: the cpu block is 3760. Rows: `benchmark_report::tests` 26, the new
    `sirius_results::tests` 17, Corpus cpu 2453 (sf40 tier 7) plus the registry case.
  - `cost-report --test test_source_formatting` 61 passed, before and after the wiki edit.
  - `python3 -m pytest scripts/env/tests -q`: 405 collected, 400 passed. The 5 failures need
    images or installs this laptop lacks: `test_image_any_user.py` ×3 (no local
    `peacockdb-build:cudf-<v>` tag) and `test_run_image.py`'s two library-resolve tests (no
    `PEACOCK_STAGED_INSTALLS`). All are among the 35 run by hand. The base (`1eaefe39`) had 336,
    the "Images (Python)" row's old count, so the row is 405.
  - `testdata/test_duckdb_result.py`: 30 passed (29 at the base).
- Grand total 8368: Rust 6352, C++ 286, Python 1730, the sum of the tables' N columns. The old
  header's Rust 6323 was one short of its own tables (6324) before this branch; summing fixed it.
- Drift fixed beyond the plan: the golden counts (`tpch.sf40` said 16; 18 at the base, 19 now;
  `tpcds.sf40`'s 3 added); the Images table and the `image.sh`/`images.yml` kinds; the CI section's
  images.yml line; the run-gpu.sh verb list (a pointer to the Sirius section); "Disk"'s image sizes.
- Disk: 60 GB free after `prepare --sirius`, from Task 9's 60,111,265,792 bytes, which counts the
  old `cudf-26.08-in-564f064de2d2` tag (2 GB) still on the VM.
- Left: the benchmark data-flow diagram has no Sirius branch (not asked). Step 3 (push, PR on
  `ENS-containers-only`, CI) is the coordinator's.
- Coordinator, 2026-11-10: Task 10 committed; PR #204 on `ENS-containers-only` (18 commits); `reviewing`. Next: reviewer round 1.

### Review round 1, 2026-11-10

0 blocking, 3 important, 4 nits (reviewer's list, pasted by the coordinator).

- **I1.** Sirius's log reports pinned-cache misses on every timed run of 34 of 84 timed rows
  (`[sirius_scan_manager.cpp:1469] cache summary … last_cycle[… miss=N …]`): tpch 9 of 38 (q3,
  q8, q10, q17, q19, q21, anti-join, left-join-fixed, semi-join; q19 2081 per run, q10 972), tpcds
  25 of 46 (q7, q26 46 per run, q85 37). Same count every run, no evictions; range-filter-only
  queries (tpch q1, q6) never miss. Guess: reads the pinned column-chunk ranges don't cover.
  Fix: find what `prefetching_cache` counts as a miss at 8d17afe and what
  `pin_table(tier='parquet')` leaves out; record it and the per-query counts here.
- **I2.** tpcds q2 and q78 fail only because the runner sets `integer_division=true`
  (`docker/sirius-bench.py:61`): DuckDB then names plain division `//`, which Sirius refuses.
  Fix: record the cause here and in build-test.md's protocol; no rerun without the human.
- **I3.** Sirius's `wrong` cells drop the row's note (`benchmark_report/sirius.rs:77-80`), unlike
  peacockdb's. q73's difference is #306 (tied rows by position); tpch scan-limit is `LIMIT 10`
  with no `ORDER BY` checked as `duckdb(sort, exact)`, so any 10 rows are right. Fix: render the
  note after `wrong answer` as `shown_of` does, with a test; notes on both rows; regenerate.
- Nits: N1 `benchmark_report.rs` `//!` block 15 lines (cap 10); N2 `test_cpu_corpus.rs` 1063
  lines (limit 1000), the Sirius block could move; N3 `--resume` keeps rows of an older runner
  with the same pin and host; N4 the committed run used `run-gpu.sh sirius --detach` and a manual
  pull/render, not `bench-sirius.sh` (same runner args).
- Clean: GPU evidence re-checked on all 84 timed rows (10 of 10 runs each, no fallback line);
  ratios recomputed; CSV and `sirius-result.txt` agree with `sirius.tsv`; Review Focus 1–5 tested.

Coordinator's routing: I1 investigation and I3 code to the developer; I2 and the tickets the
coordinator's (markdown). scan-limit goes under #306 as a second case of an oracle that rejects a
right answer, not a new ticket. N1, N2 fixed if cheap; N3, N4 dropped.

### Review round 1: I1, the cache misses

Read at 8d17afe (`/tmp/sirius-src-8d17afe`), libcudf at 25.10 in `third_party/cudf` (26.08 not
local). Logs: `testdata/calibration/sirius/<ds>.sf40/log/`. No rerun.

- **What a miss is.** `prefetching_cache` counts per piece of a read: a request range cut at cache
  chunk boundaries. A piece is a hit when its chunk is cached and its fill covers the bytes; it
  is a miss otherwise (`src/io/cache/prefetching_cache.cpp:922-945`, `++misses`). A miss goes to
  the backend at once (`:993-994`, host reads `:798`), on the query's path. With
  `cache.mode: sirius` the backend is io_uring on an `O_DIRECT` fd (`src/io/cache/config.hpp:130`,
  `src/io/uring/uring_reactor.cpp:576`). So a miss reads the disk, never the page cache. The
  logs show no "O_DIRECT unavailable" warning. A demand read never adds a chunk. Chunks come only
  from a datasource's `fadvise` (`src/io/sirius_datasource.cpp:216-240` →
  `prefetching_cache.cpp:392,476`). So a range nobody pinned misses on every run.
- **What `pin_table(tier='parquet')` pins.** The column-chunk byte ranges of every row group,
  for the `cols` given or all columns (`src/scan_manager/sirius_scan_manager.cpp:2953-2962`,
  `op::scan::column_chunk_ranges`). Its log line says "bytes of column chunks" (`:2993`). It
  leaves out the bloom filters, the page indexes and the footer. The footer is parsed into
  Sirius's metadata store instead, so a query does not reread it.
- **What misses.** Bloom filters. libcudf's predicate pushdown reads one for each row group and
  column that has an equality or IN literal (`cpp/src/io/parquet/predicate_pushdown.cpp:199-226`).
  The read goes through the datasource: `host_read` at `bloom_filter_reader.cu:326`, then
  `device_read_async` or `host_read` for the bitset at `:373/380`. Sirius passes its join-built
  dynamic filters (in-lists) into that same reader filter
  (`src/op/scan/parquet_gpu_ingestible.cpp:1282-1292`, then `read_parquet` at `:1392`). So a join
  probes bloom filters too, with no literal in the SQL.
  - Our parquet has bloom filters, and they sit after the last column chunk, outside every pinned
    range. In sf1, the first bloom filter offset equals the end of the column data in every file,
    tpch and tpcds. The sf40 footers were not checked, because sf40 is not on this laptop. They
    are taken to have the same layout, since they come from the same COPY.
  - Sizes in sf1: tpch averages 1.7 KB (max 16 KB), tpcds 9 KB (max 32 KB).
  - The fit is partial (corrected in review round 2). Range-only queries never miss: tpch q1 and
    q6. Many equality filters, IN lists and semi- and anti-joins miss, but some read nothing:
    tpch q5, q12 (the same lineitem IN as q19), q16, semi-join-reorder, anti-join-reorder; tpcds
    q34, q46, q68, q73, q96. tpcds's `bloom_filter_reader.cu:82` failures
    (q33, q43, q56, q60, q91) show libcudf reading bloom filters inside Sirius's scans.
- **The counts.** Every timed run of a row misses the same count, and its warm-up matches. h2d
  is 0 and there are no evictions anywhere. The line at query N reports query N−1: it is logged
  before `prepare_for_query` takes its snapshot (`sirius_scan_manager.cpp:1469-1470`,
  `prefetching_cache.cpp:1092-1106`). Attributed that way, the result is still 34 of 84 timed
  rows. The other 50 have `miss=0`. The log gives counts, no bytes (`summary()`,
  `prefetching_cache.cpp:1066-1090`). Before the first query, the pin and bind phase missed 17
  (tpch) and 34 (tpcds), all outside the timed runs.

| dataset | query | misses per run | reads | hits |
|---|---|--:|--:|--:|
| tpch | q19 | 2081 | 8014 | 10891 |
| tpch | q10 | 972 | 3019 | 4753 |
| tpch | q21 | 241 | 16155 | 23802 |
| tpch | q17 | 130 | 8014 | 11938 |
| tpch | q8 | 65 | 4448 | 9394 |
| tpch | q3, anti-join, semi-join | 43 | 3974, 102, 102 | 5593, 2531, 2531 |
| tpch | left-join-fixed | 2 | 544 | 1179 |
| tpcds | q7, q26 | 46 | 6588, 2836 | 7996, 3699 |
| tpcds | q85 | 37 | 1422 | 1907 |
| tpcds | q71 | 7 | 4003 | 5607 |
| tpcds | q25, q30, q31, q81 | 4 | | |
| tpcds | q10, q19, q29, q35, q42, q52, q55, q69, q84 | 3 | | |
| tpcds | q3, q45, q50, q66 | 2 | | |
| tpcds | q1, q32, q79, q92 | 1 | | |

- **Judgement.** Yes. On every timed run, these 34 rows include synchronous `O_DIRECT` reads of
  bloom filters from the H100's disk, outside the pin. Each read is small, at most a few 4 KiB
  pages, so its cost is latency, not bandwidth. That cost was never measured. tpcds's 1–7 reads
  are noise beside times of hundreds of ms. tpch q19 (2081 reads) and q10 (972) are where it can
  show; q21 and q17 less so. The other 50 timed rows read only pinned bytes.
- **No setting fixes it.** `pin_table` has no argument for bloom filters (only `cols`). Sirius
  has no setting that turns pushdown or bloom filtering off: `disable_filter_pushdown` is derived
  (`parquet_gpu_ingestible.cpp:857`). Pinning them would be a Sirius change, which the spec
  rules out. Two protocol changes would fix it, each needing the human:
  - Give Sirius the sf40 parquet rewritten without bloom filters (DuckDB's COPY can switch them
    off). libcudf then reads nothing outside the pin. But Sirius loses its row-group pruning by
    bloom filter, and its files differ from peacockdb's.
  - Serve the parquet from tmpfs, so a miss reads memory, not disk. On a kernel that refuses
    `O_DIRECT` there, Sirius falls back to buffered reads (`uring_reactor.cpp:576-578`). Needs host
    RAM for the dataset beside the pinned copy (tpch's pinned column chunks are 42.4 GB). Not
    measured on the H100.
  - Or, cheaper and short of a fix: rerun the rows above 100 misses (tpch q19, q10, q21, q17) both
    ways to measure the cost.

### Review round 1: developer, 2026-11-10

- **I3.** `wrong_mark` in `benchmark_report.rs` is now the one form of the mark, for peacockdb's
  cell and Sirius's: ` <span class="missing">wrong answer <note></span>`, with the note escaped.
  With no note, the mark is just `wrong answer`. peacockdb's notes are plain text (`#306` is not
  a link), so Sirius's are too. Test: `a_wrong_sirius_row_shows_its_note`. It failed first with
  `"300.0 wrong answer"`.
  - `#306` is on tpcds q73 and tpch scan-limit in the two `sirius.tsv` files. The rewrite
    (`UPDATE_CANONICAL=1 … every_sf40_sirius_status_meets_its_answer`) left both files
    byte-identical. It only sets statuses, and it writes nothing when no status moves.
  - New guard test: `a_wrong_sirius_rows_note_survives_the_rewrite`. A mutation that clears the
    note in `sirius_statuses_set` fails it, and fails the older fixture test too.
  - Page: two cells gained `#306`. `benchmarks.csv` has no note column, so it did not change.
- **N1.** The `//!` block of `benchmark_report.rs` is now 10 lines.
- **N2.** The Sirius block moved to `peacockdb-core/tests/test_cpu_corpus/sirius.rs`, a
  `#[path]` module as in `test_corpus_goldens`. `test_cpu_corpus.rs` is 929 lines. The test
  names are unchanged; the libtest path is now `sirius::<name>`, and a name filter still matches.
- **Counts:**
  - `--lib` 1237 → 1238 (`benchmark_report::tests` 26 → 27).
  - `test_cpu_corpus` 2454 → 2455 (Corpus cpu 2453 → 2454, sf40 tier 7 → 8).
  - cpu block 3760 → 3762. Rust 6352 → 6354. Grand total 8368 → 8370.
  - All updated in build-test.md.
- Coordinator, 2026-11-10, on I1: kept as measured. Done when 3 asks for the column-chunk bytes
  pinned, and they are; the bloom-filter reads are Sirius's own design, and pinning them would be a
  Sirius change (out of scope). Recorded in build-test.md's protocol; named in the signoff for the
  human, with the developer's options (no-bloom parquet, tmpfs, or measuring q19/q10/q21/q17 both
  ways). I2 kept as measured, recorded likewise. Next: review round 2.

### Review round 2, 2026-11-10

0 blocking, 3 important, 5 nits. I-1: build-test.md's bloom-filter rule was contradicted by the
logs (tpch q5, q12 … read none); restated as the measured set with a likely cause. I-2: q2/q78 fail
on `round(x // y, 2)`; a bare `//` runs (q59); restated. I-3: scan-limit is not #306's problem;
new ticket #311 (LIMIT without ORDER BY), scan-limit's note to `#311`. Nits: N-a #306 length and
N-b q73's second pair (fixed in the ticket), N-c a 112-column doc line in
`benchmark_report/sirius.rs:59`, N-d the cpu row now links `test_cpu_corpus/sirius.rs`; N-e (a
rerun without `--resume` drops hand notes) dropped. Coordinator wrote the markdown; developer:
scan-limit's note and the regenerated page, N-c.

### Review round 3, 2026-11-10

Round 2's three resolved (re-counted from the logs). 0 blocking, 2 important, 2 nits, all
markdown, fixed by the coordinator: #311's fix no longer offers `duckdb_none` (the oracle refuses
it over two sections; the subset check, as `data_fusion_subset`, is the fix); `tickets.md`'s next
free number 312 and open count 94; #306's closing sentence names Sirius's q73 alone (Sirius's q18
is `failed`); build-test.md "equality or `IN` filters". No finding outstanding: `completing`.
Next: the completeness pass (a fresh reviewer and a fresh analyst).

### Completeness pass, 2026-11-10

Reviewer: 0 blocking, 2 important. Analyst: 1 blocking, 4 important; architecture.md untouched.
- CI (both): run 42758552174 on `6efce89d`, the head's code, finished green after both looked.
- Page intro says every time is device-side, load plus execute; Sirius's is the wall time with an
  Arrow fetch, and its provenance is nowhere on the page (reviewer I-1). → developer: a Sirius
  paragraph beside DuckDB's, with the cache-miss caveat.
- The public Sirius image ships the Apache-2.0 extension without its LICENSE (analyst I2). →
  developer: install it, rebuild; coordinator republishes.
- `--query` without `--resume` replaces the whole `sirius.tsv` and `answers.duckdb`, so pulling a
  one-query run clobbers the dataset (analyst I3). → developer: a narrowed run replaces only its
  own rows and tables.
- The two real `wrong` rows (tpch nested-loop-left-join, tpcds q31) have no note (analyst I4,
  Done when 6). → developer: notes, regenerate.
- The bloom-filter reads (analyst I1, reviewer's shortcut list): Done when 3 is met as written
  (column chunks pinned), so the coordinator keeps the run and does not stop to ask; the page now
  says it, and the signoff names it for the human.
- Shortcuts to fix: the timing test's wall-clock range (keep the fake's write out of the timed
  region). build-test.md's two false sentences (images.yml's token; the data-flow diagram) fixed
  by the coordinator.

### Completeness fixes, 2026-11-10

- **Page intro.** "Times are device-side" now reads "PeacockDB's times are device-side". Where some
  `sirius.tsv` exists, `sirius_intro` (`benchmark_report/sirius.rs`) adds a paragraph before DuckDB's.
  It says what Sirius's total is, that the bytes come from the pin, and that some rows read outside the
  pin (likely bloom filters). It names each file's `sirius`, `duckdb`, `libcudf`, `driver` and `device`,
  with `_` read back as a space. Test: `the_sirius_paragraph_names_its_total_and_provenance`. The page
  is regenerated; `benchmarks.csv` did not move.
- **Licence.** `docker/sirius-LICENSE` is `git show 8d17afe:LICENSE` from the local Sirius checkout
  (sha256 `027e2457…`, Apache-2.0). It is committed rather than fetched. `SIRIUS_LICENSE_SHA256` in
  `pins.env` pins it, and the PIN's `license_sha256` carries it. The build runs `sha256sum -c` on
  it, and the self-test checks it is there and matches. It is also one of `image_inputs`' files.
  Rebuilt locally: the new inputs tag is **`cudf-26.08-in-00259c773420`**, and `--self-test` passes
  in it. The H100's `cudf-26.08-in-f2f77ce2bf3c` is now another inputs tag, so `run-gpu.sh sirius`
  refuses it until the new tag is published and `prepare --sirius` pulls it.
- **`--query` without `--resume`.** The runner takes `--list <every stem>`, and `bench-sirius.sh` passes
  it whenever `--query` narrows a run. With `--list`, the runner keeps the file's other rows as they are
  and does not remove `answers.duckdb`. Rows go in the list's order. The answer worker drops
  `"<stem>"` before each query runs, so a query that now fails leaves no old answer behind. The
  file's `sirius` and `host` must match this run's, or the run is **refused** (exit 2) before
  any query. No file means a fresh start. `--resume` keeps its old meaning; with `--list` it also
  keeps the narrowed queries' rows.
  - Shipping: `pull` copies the VM's `answers.duckdb` home with `--update`, so a VM that lacked the
    full database would have overwritten home's 2.5 GB tpch one with a few tables. The render would
    then have written a few-query `sirius-result.txt`. So for `--list` or `--resume`,
    `run-gpu.sh sirius` now ships the home database first with `--update`, and a newer one on the
    VM stays. A whole run is sent nothing, because it removes the database. `sirius.tsv`'s shipping
    was already right: home's file, unless the VM's is newer.
- **Notes** on the two real `wrong` rows: tpch `nested-loop-left-join` ("Sirius applies the >
  reversed: AFRICA (0) pairs with ARGENTINA (1), MIDDLE EAST (4) with none"; first written as
  ignoring the `>`, corrected after the fix-commit review) and tpcds q31 ("rows DuckDB lacks, with a negative
  last column, and rows missing"). `every_sf40_sirius_status_meets_its_answer` keeps them.
- **Timing test.** The fake DuckDB's `time.perf_counter_ns` is a clock that only moves by each
  run's scripted latency. The test now asserts the exact runs and `10.0`.
- Counts: Images (Python) 405 → 412, `--lib` 1238 → 1239, benchmark report page 27 → 28; grand total
  8370 → 8378.
- Coordinator, 2026-11-10T05:53:04Z: completeness fixes committed `4551f028`; `images.yml` dispatched (sirius 26.08), run 42764886431, for inputs tag `cudf-26.08-in-00259c773420`; a reviewer checks the fix commit.
- Fix-commit review (`4551f028`): licence, Sirius paragraph, timing test closed. 2 important:
  (1) a narrowed or resumed run whose worktree has no `answers.duckdb` (another worktree, or after
  removing the file) keeps timed rows with no answers, and the render writes a one-section
  `sirius-result.txt`; also `--list` with no `sirius.tsv` keeps an old database. Fix: refuse (exit
  2) before any query when a kept timed row's table is missing; with no row kept, remove the
  database. (2) nested-loop-left-join's note is wrong: Sirius applies the `>` reversed
  (`r_regionkey < n_regionkey`: AFRICA (0) with ARGENTINA (1), MIDDLE EAST (4) with none), it does
  not ignore it; reword the note and the Task 9 record. → developer.
- Fix-commit fixes: `without_answers` refuses (exit 2) a kept timed row whose table `answers.duckdb` lacks, before any query; no row kept removes the database. nested-loop-left-join re-read from `sirius-result.txt` (20/15/10/5 pairs for regions 0-3, MIDDLE EAST padded: `r < n`); note, Task 9 record and the fixes note reworded, page regenerated. Images (Python) 412 → 413, total 8379. Rebuilt: inputs tag **`cudf-26.08-in-80a56ae84b1a`**, `--self-test` ok; `00259c773420` is now stale.
- Coordinator: fixes committed `60e43260`; images.yml run 42766876916 for `cudf-26.08-in-80a56ae84b1a`. Signoff written. Next: completeness approved, hard stop, pipeline.yml dispatch.
- Coordinator: images.yml run 42766876916 green on rerun (first attempt hit an Ubuntu mirror sync error). Both completeness passes closed: `completeness approved`.
- Coordinator, 2026-11-10T06:24:00Z: hard stop (all three already STOPPED); `pipeline.yml` dispatched on `01442c36`, run 42768246326. `done` needs it green, GPU Tests (nebius-gpu) included.
- Coordinator, 2026-11-10T07:08:41Z: run 42768246326 green (GPU Tests (nebius-gpu) included), PR run 42766882798 on the last code commit green; hard stop; `done`.
