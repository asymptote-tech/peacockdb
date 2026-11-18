# sirius-timing — detail

## Run log

- 2026-11-17: the run columns (`ee55af2e`), the runner's windows (`2c446b43`) and the page and
  CSV (`ffbe6b1f`) committed; `ENS-sirius-timing` pushed.
- 2026-11-17: the Sirius image of `ffbe6b1f` published by `images.yml` (run 44157901381,
  `kind=sirius`, `cudf=26.08`): self-test ok; tags `cudf-26.08`, `cudf-26.08-in-434990408e0c`,
  `cudf-26.08-ffbe6b1fc663`.
- 2026-11-17, session 1 on the H100: `run-sirius.sh … prepare` (6 min) and the measurements below.
  `run-gpu.sh … prepare` was stopped during its 10 GB build-image pull after 30 min: Sirius needs
  neither the build nor the run image, and the VM had Docker, the toolkit, nsys and sf40 already.

- 2026-11-17, session 2: the rerun, `bench-sirius.sh … --dataset all` with the image of
  `ffbe6b1f`, no `--resume`. tpch whole (pin 460.8 s), then tpcds (pin 163.3 s) to q96, where
  the VM was stopped from outside the run (the ssh dropped; `nebius.sh status` said `STOPPING`
  before any stop of the run's). Started again, `bench-sirius.sh … --dataset tpcds --resume` kept
  the 80 rows on the VM, pinned again (163.9 s), ran q97 and q99, pulled and rendered.

- 2026-11-18, session 3: `executor.scan_manager.num_threads` at its default, 3 and 16, on q6 and
  q9 (finding 3).

## H100 time

| session | start (UTC) | stop (UTC) | minutes | running total |
|---|---|---|---|---|
| 1 | 2026-11-17 19:38 | 20:52 | 74 | 74 |
| 2 | 2026-11-17 20:58 | 21:50, stopped from outside the run | 52 | 126 |
| 2, resumed | 2026-11-17 21:51 | 21:57 | 6 | 132 |
| 3 | 2026-11-18 00:25 | 00:38 | 13 | 145 |

## How the experiments ran

Each experiment runs `sirius-bench.py` in the Sirius image as `vm_sirius` does (seccomp profile,
`--ulimit memlock=-1:-1`, the VM's hostname, all GPUs, the card lock), with its own `--out` and,
when given, its own `--config`, under `testdata/calibration/sirius/experiments/<name>/` on the VM.
Its data directory holds only the tables its queries read, as links. `/mnt/sx` is a tmpfs mounted
for the session (`/dev/shm` is cleared when the ssh session ends, and Docker mounts its own over
it):

```
sudo mkdir -p /mnt/sx && sudo mount -t tmpfs -o size=60g,mode=0755,uid=$(id -u),gid=$(id -g) sx /mnt/sx
mkdir -p /mnt/sx/tpch.sf40
for t in lineitem part customer orders nation region supplier partsupp; do
  cp /var/lib/peacock/tpch.sf40/$t.parquet /mnt/sx/tpch.sf40/; done
mkdir -p /mnt/sx/sub-l && ln -sf /mnt/sx/tpch.sf40/lineitem.parquet /mnt/sx/sub-l/
```

The experiment script, `~/sx.sh` on the VM:

```bash
#!/bin/bash
# sx.sh <name> <ds> <stems> [runs] [config file] [data dir]
set -euo pipefail
name=$1 ds=$2 stems=$3 runs=${4:-10} cfg=${5:-} data=${6:-/var/lib/peacock/$2.sf40}
base=$HOME/peacock/sirius-timing
image=ghcr.io/asymptote-tech/peacockdb-sirius:cudf-26.08-in-434990408e0c
exp=$base/testdata/calibration/sirius/experiments/$name
mkdir -p "$exp/out"
rm -rf "$exp/out/$ds-queries"
cp -r "$base/testdata/$ds-queries" "$exp/out/"
args=(--dataset "$ds" --sf 40 --data "$data" --queries "$stems" --runs "$runs" --timeout 900 --out /exp/out)
if [ -n "$cfg" ]; then cp "$cfg" "$exp/config.yaml"; args+=(--config /exp/config.yaml); fi
[ -e /run/lock/peacock-card.lock ] || (umask 000 && : >> /run/lock/peacock-card.lock)
exec {card}< /run/lock/peacock-card.lock
flock "$card"
echo "== $name $(date -u +%FT%T.%3NZ) start"
docker run --rm --init --gpus all --user "$(id -u):$(id -g)" --hostname "$(hostname)" \
  --security-opt "seccomp=$base/docker/sirius-seccomp.json" --ulimit memlock=-1:-1 \
  -v /var/lib/peacock:/var/lib/peacock:ro -v "$exp:/exp" ${SX_DOCKER_EXTRA:-} \
  "$image" ${SX_PREFIX:-} python3 /opt/sirius/sirius-bench.py "${args[@]}" || echo "== exit $?"
echo "== $name $(date -u +%FT%T.%3NZ) end"
cat "$exp/out/benchmark-results/h100-80gb-hbm3/$ds.sf40/sirius.tsv"
```

The Nsight runs set `SX_DOCKER_EXTRA="-v /mnt/sx:/mnt/sx:ro -v
/opt/nvidia/nsight-systems:/opt/nvidia/nsight-systems:ro --cap-add SYS_ADMIN"` and
`SX_PREFIX="/opt/nvidia/nsight-systems/2025.3.2/target-linux-x64/nsys profile -t cuda,nvtx,osrt
--sample=process-tree --backtrace=fp --cpuctxsw=process-tree -o /exp/<q> --force-overwrite true"`
(the first q6 capture: `-t cuda,nvtx --sample=none --cpuctxsw=none`), with 3 timed runs (the
sampled q6 capture, 2); each report was exported with `nsys export -t sqlite`.

Every Sirius process ends with two Rust panics, `cannot access a Thread Local Storage value during
or after destruction: AccessError`, after its rows are written. They change no row.

In the tables below, **run** is the second-smallest execution window and **wall** the
second-smallest wall, of 10 timed runs unless marked, in ms.

## Findings

### 1. The runner's windows on the H100

`sx.sh s3-q6 tpch q6 10` (image config, all eight tables from disk): `ok`, ten windows, no rebuild
note, every window `outcome=ok`. Wall 339.2 (339 before), run 217.0 (211–276). Each window's plan
is the one validated at prepare (`reusing finalize-validated Sirius plan`). The label `CALL`s open
no window. So planning, outside the window, is about 120 ms of q6's wall.

The pin of all eight tpch tables from the VM's disk took 460 s: about 90 MB/s. A plain `cp` of
`lineitem.parquet` (8.5 GB) reads at 86 MB/s. From tmpfs the pin of `lineitem` takes 4.3 s.

### 2. The copy

Per timed run, from the captures (`lineitem` and the query's other tables on tmpfs):

| query | run | kernels | host-to-device | rate | CPU top frame in the window |
|---|---|---|---|---|---|
| q6 | 222–258 | 13.7 ms | 237 MB in 6.9 ms (976 copies) | 34 GB/s | `vector<RowGroup>` copy |
| q19 | 794–842 | 152.7 ms | 3030 MB in 82 ms (10689) | 37 GB/s | the same |
| q9 | 756–804 | 235.8 ms | 7757 MB in 158 ms (5733) | 49 GB/s | the same |

- **Every timed run copies its column chunks from the pinned host cache.** The bytes are the same
  in every run, the warm-up's included. Nothing stays on the device between runs, so the two
  engines start each run from the same place, and no device-resident way exists to switch off
  (Done when 2's residency case does not arise).
- **`h2d=0`** in the cache summary counts no copy the trace sees. Per Sirius
  (`prefetching_cache.hpp:487`), `h2d` counts chunks the cache (re)loads by host-to-device IO
  (`mark_loading`). So `h2d=0` means the pin held and no chunk was reloaded. The copies the trace
  sees are `cudaMemcpyBatchAsync` from the pinned cache, issued by the parquet reader
  (`fetch_byte_ranges_to_device_async`).
- **The copy is not slow.** The 10 GB/s estimate in #311 guessed the bytes (q6 copies 237 MB) and
  divided them by the whole GPU part. The copies themselves run at 34–49 GB/s: q9's 49 within
  peacockdb's 41–49 GB/s, q6's 34 and q19's 37 near it.
- **The GPU is idle most of the window.** In q6's second run (222 ms) the kernels and copies take
  20.7 ms. For 148 ms the window shows no GPU work and no range but row-group filtering (1 ms at
  74 ms). The scan task then takes 72 ms: 32 ms fetching byte ranges, 8.6 ms decompressing
  (`unsnap_kernel`, snappy, 8.2 ms of kernel), decode and the aggregate.
- **The CPU spends the window copying parquet metadata.** CPU samples inside the window of each
  capture's last timed run alone (q6: 580, q19: 2632, q9: 3185) put
  `std::__do_uninit_copy<cudf::io::parquet::RowGroup…>` on the stack of 288, 586 and 976 of them,
  with `operator new`, `malloc`, `free` and `~vector<RowGroup>` beside it, under
  `hybrid_scan_reader::hybrid_scan_reader(FileMetaData const&)`. Each scan task copies the file's
  whole row-group metadata. Most of these samples are on scan_manager threads: 248 of q6's 288,
  472 of q19's 586, 654 of q9's 976. This is in Sirius's scan path; no setting measured in
  findings 3, 5 and 6 removes it.
- Who builds the readers: over each whole capture, of the samples whose stack unwinds above the
  constructor, `column_chunk_ranges` (`src/op/scan/parquet_materialize.cpp:38`, a reader per
  slice of every split) in 60 of q6's 100, 171 of q9's 232 and 134 of q19's 190, and
  `build_file_scan_info` (`src/op/scan/parquet_gpu_ingestible.cpp:874`) in the rest. The footer
  itself is parsed once per file and shared (`src/scan_manager/sirius_scan_manager.cpp:1404`).
- `cudaHostRegister` (68 calls, 35 ms each) runs at pin time only, none inside a window.

### 3. The settings

| setting | q6 run | q9 run |
|---|---|---|
| defaults (image config) | 204.0 (195–226) | 678.0 (670–812) |
| `executor.pipeline.num_threads: 1` (default 4) | 213.0 | 863.0 |
| `executor.pipeline.num_threads: 8` | 205.0 | 679.0 |
| `operator_params.scan_task_batch_size: 536870912` (default 2.5% of the GPU's memory) | 236.0 | 1683.0 |
| `operator_params.scan_task_batch_size: 8589934592` | 197.0 (196–251) | 640.0 (636–712) |

Tables on tmpfs, one setting at a time, each config the image's plus the one key. One thread or
smaller batches are slower; more threads change nothing; 8 GiB batches are 3–6% faster, inside
both queries' spread.

Session 3, the same way, `executor.scan_manager.num_threads` alone:

| `executor.scan_manager.num_threads` | q6 run | q9 run |
|---|---|---|
| default (all cores left after the other pools, at least 4; 16 cores here) | 188.0 (179–216) | 664.0 (643–732) |
| 3 (the minimum Sirius accepts) | 192.0 (183–231) | 687.0 (677–761) |
| 16 | 214.0 (208–238) | 757.0 (728–858) |

3 threads stay within the default's spread; 16 are slower. `8d17afe`'s scan_manager has no
chunk-size key: `src/sirius_config.cpp` reads `num_threads`, `cpu_affinity`, `backend`,
`reactors`, readahead and the cache. So no chunk size was tried.

### 4. The reads outside the pin

q10 and q19 over the same five tables (lineitem, part, customer, orders, nation), from the VM's
disk and from tmpfs:

| query | misses a run | run, disk | run, tmpfs |
|---|---|---|---|
| q10 | 972 | 472.0 (470–490) | 483.0 (478–555) |
| q19 | 2081 | 659.0 (628–790) | 646.0 (644–689) |

The misses are the same on both, every run (`last_cycle[… miss=972 …]`, `miss=2081`), and their
cost is inside the spread even from a 90 MB/s disk. The bytes they read were not measured; the log
gives counts alone. The parquet without bloom filters was not made: the misses cost nothing
measurable to remove.

### 5. The footer

q6, tables on tmpfs, in one process (the script below), 10 runs per setting in turn, info log:

| `parquet_metadata_cache` | wall | run |
|---|---|---|
| off (DuckDB's default) | 327.7, then 313.5 | 199.0, then 190.0 |
| on | 224.1, then 224.9 | 190.0, then 191.0 |

The cache takes about 90–100 ms off the wall and nothing off the window: it is DuckDB's footer
read at bind, inside planning. The `RowGroup` copy inside the window (finding 2) is Sirius's own
and stays. Registering a table other than as a `read_parquet` view was not tried: `pin_table`
takes the parquet path and a name, and the view is what DuckDB binds the query against.

### 6. The log

The same script at `SIRIUS_LOG_LEVEL=warn`, wall, in the order the four settings ran (cache off,
on, off, on): 313.9, 216.4, 309.2, 239.0 against 327.7, 224.1, 313.5, 224.9 at info. Warn minus
info: −13.8, −7.7, −4.3, +14.1 ms, each inside its spread. The windows need info, so the timed runs
log at info.

The wall script, `/exp/sx-wall.py`, run as `docker run … -e SIRIUS_LOG_LEVEL=<level> <image> python3
/exp/sx-wall.py /mnt/sx/sub-l /exp/q6.sql 10 - "SET parquet_metadata_cache=true" "SET
parquet_metadata_cache=false" "SET parquet_metadata_cache=true"`:

```python
import os, pathlib, sys, time
os.environ.setdefault("SIRIUS_CONFIG_FILE", "/opt/sirius/sirius-config.yaml")
os.environ.setdefault("SIRIUS_LOG_BACKEND", "spdlog")
os.environ.setdefault("SIRIUS_LOG_LEVEL", "info")
os.environ.setdefault("SIRIUS_LOG_DIR", "/exp/log")
import duckdb
data, sql, runs = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]).read_text(), int(sys.argv[3])
con = duckdb.connect(":memory:", config={"allow_unsigned_extensions": "true"})
con.execute("LOAD '/opt/sirius/sirius.duckdb_extension'")
con.execute("SET enable_duckdb_fallback = false")
con.execute("SET default_null_order='nulls_last_on_asc_first_on_desc'")
con.execute("SET integer_division=true")
tables = [(p.stem, p) for p in sorted(data.glob("*.parquet"))]
for t, p in tables:
    con.execute(f"CREATE VIEW \"{t}\" AS SELECT * FROM read_parquet('{p}')")
for t, p in tables:
    con.execute(f"CALL pin_table('{p}', name = '{t}', tier = 'parquet')")
for setting in sys.argv[4:]:
    if setting != "-":
        con.execute(setting)
    con.execute(sql).to_arrow_table()
    walls = []
    for _ in range(runs):
        s = time.perf_counter_ns()
        con.execute(sql).to_arrow_table()
        walls.append((time.perf_counter_ns() - s) / 1e6)
    print(f"{os.environ['SIRIUS_LOG_LEVEL']}\t{setting}\t{sorted(walls)[1]:.1f}", flush=True)
```

The windows of the info process come from its log, paired by `window=`.

## Configuration

The keep rule (spec, Done when 2) over each finding:

- **Kept: none.** Sirius runs as the image runs it today: its config's three departures, the
  `read_parquet` views, the tables pinned from the VM's disk, its log at info.
The rule's "shortens" is read as: a setting shortens a clock when its runs' range lies below the
default's.

- **`parquet_metadata_cache = true`**: shortens Sirius's total by 90–100 ms on q6, its run not at
  all, and is a setting any DuckDB user has. It meets the rule's three conditions. It is not kept,
  a deliberate departure from the rule: DuckDB's timed runs on the page do not set it, so it would
  tilt the one total comparison the page makes (`Σ total Sirius / Σ total DuckDB`), and it leaves
  the run, the comparison with peacockdb, as it is.
- **`operator_params.scan_task_batch_size` 8 GiB**: q9's range (636–712) overlaps the default's
  (670–812), and q6 is unchanged. Not kept.
- **`executor.pipeline.num_threads`**: 1 is slower, 8 the same. Not kept.
- **`executor.scan_manager.num_threads`**: 16 is slower, 3 the same. Not kept.
- **Parquet without bloom filters, or tmpfs**: the reads outside the pin cost nothing measurable.
  Not kept; no layout is made.
- **The log at warn**: inside the spread, and the windows need info.
- **Device residency**: every run copies from the host, so there is nothing to switch.

So no harness change follows, the image of `ffbe6b1f` is the rerun's, and the rerun goes as
`build-test.md` gives it.

## The rerun

Both datasets, every row of the H100's `cases.csv`, Sirius `8d17afe`, driver 580.173.02. Every
timed row has ten windows, each ended `ok`; no row's plan was rebuilt inside a window.

Statuses after `UPDATE_CANONICAL=1 … every_sf40_sirius_status_meets_its_answer`, against the
files before:

| dataset | ok | unverified | wrong | failed |
|---|---|---|---|---|
| tpch | 35 (35) | 1 (1) | 2 (2) | 10 (10) |
| tpcds | 42 (42) | 2 (2) | 2 (2) | 36 (36) |

No row changed status; every `failed` row fails as before. The runner writes each row's note as
it ran (`-` on a timed row), so the four `wrong` rows' notes (tpch nested-loop-left-join and
scan-limit, tpcds q31 and q73) were put back as they were. tpcds's rendered answers are the same
bytes; tpch's differ in scan-limit alone, a `LIMIT 10` without `ORDER BY`.

The ratios on the page: Σ run Sirius / Σ run peacockdb = 0.68 over 35 of 48 tpch queries and
0.61 over 42 of 82 tpcds queries; Σ total Sirius / Σ total DuckDB = 1.28 and 1.29 (1.29 and 1.32
before).
