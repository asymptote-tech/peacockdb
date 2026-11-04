# pinned-loads — detail

## Run log

- 2026-11-04: `ENS-pinned-loads` branched from `ENS-new-hosts` at `add2a081`, while new-hosts is
  `blocked(building)` (the human's GitHub PAT; see `new-hosts-detail.md`, "Blocked"). The H100
  path (the CPU-VM build, `run-gpu.sh`) is not live yet, so only the offline steps can run: Task 1,
  and Task 2 Steps 1–2. Task 2 Step 3 onward needs the H100 and waits.
- 2026-11-04: Task 1 offline: `nsys_loads.py` → `loads.tsv`, its tests, the `--trace` wiring and
  the `build-test.md` edits. Notes below.

## Task 1: the load breakdown

- **Red → green.** `test_loads.py` failed first with `ModuleNotFoundError: No module named
  'nsys_loads'`, then 10/10 passed. Every `scripts/calibration/tests/test_*.py` passes: calls 6,
  fetch 3, hbm 14, loads 10, split 11 on dev; plot 7 in the 25.02 build image (dev has no
  matplotlib).
- **One test more than the plan:** `test_work_reaching_past_the_load_is_clipped_to_it`. No plan
  test pinned the clipping: with clipping removed the other nine still passed.
- **Checked by mutation:** crediting an ambiguous launch to one load fails
  `test_concurrent_…`; counting every launch as off-thread fails the first test; matching every
  OS call as a read fails `test_osrt_…`; removing the clipping fails the new test.
- **No `.gitignore` line.** `testdata/.gitignore` is deny-by-default over `calibration/`, so
  `loads.tsv` is already ignored (`git check-ignore -v` names `testdata/.gitignore:15`). The plan
  asked for a root `.gitignore` line; it would be dead.
- **Small departures from the plan's interface.** `load_windows` keys windows by execution, not
  by raw occurrence: run // call indices. `device_ops` returns a namedtuple that also carries
  `copyKind`, since a non-HtoD copy goes to `other_device_us`. Launches are read from
  `CUPTI_ACTIVITY_KIND_RUNTIME` and, when present, `…_DRIVER`, because `PINNED_ALLOC_RE` names
  `cu*` driver calls. Rates are computed per execution, then the median is taken, then rounded
  to 0.1 GB/s.
- **What the ambiguity rule covers.** Device operations and pinned allocations launched where two
  loads are open go to neither. Only the device operations are printed as `ambiguous`. Host reads
  are intervals, not launches: they are clipped to each load's windows, so two concurrent loads
  both count a read that overlaps them.
- **For Task 2, on the first real capture:**
  - `--trace` records `nvtx,cuda` only, so there is no `OSRT_API`. `host_read_source` is
    `kvikio` only if 25.02's kvikio pushes its `libkvikio` NVTX ranges, else `none`. Check
    which in the before profile. Adding `osrt` to `--trace` would cost overhead in the spans the
    trace pass exists to keep clean; it is not done here.
  - `DECOMPRESS_RE` and `DECODE_RE` have only met synthetic names. Task 2 Step 5 checks them
    against the real kernel names.
  - `nsys_calls.regions` finds a partition's call as the latest call starting before it, on any
    thread, and refuses it if that call is on another thread. So a multi-lane capture whose
    calls interleave across threads is refused before `nsys_loads.py` sees it. tp1_single is
    one lane and is not affected.
- **shellcheck** is on neither dev nor the build image. `koalaman/shellcheck:stable` over HEAD's
  `create_nsys_profile.sh` and over the edited one gives the same findings: SC2034
  (`REMOTE_SSH` unused) and SC1091 (info), both there before. `bash -n` is clean. The image was
  removed after.

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
