
# Infrastructure tickets

Tests, CI, hosts, testdata etc

## Contents

- [#178 (shared shad-gpu pool) the GPU host is shared, and a pool that cannot be built is a neighbour's fault](#t178)
- [#176 (CI coverage one-way) the CI coverage guard checks one direction only](#t176)
- [#306 (no_sort ties by position) a `no_sort` comparison pairs rows by position, so tied rows in another order fail](#t306)
- [#309 (no en_US in build image) the build image has no en_US.UTF-8 locale, so two script test files cannot run in it](#t309)

<a id="t178"></a>
### #178 (shared shad-gpu pool) the GPU host is shared, and a pool that cannot be built is a neighbour's fault
Each gtest main reserves a fixed byte budget (`kPoolBytes` beside its `main()`, listed in
`build-test.md`). The GPU host is nebius-gpu now (`build-test.md`, "Nebius host"). Our CI runs
queue on the `nebius-gpu` concurrency group. The other chains and the developers share the VM
and do not queue, and neither does work outside this repo. So a neighbour holding part of the
card still fails us. **Tentatively closed**: it cannot
be proven closed from here.

The pool line says whose failure it is. `[rmm] pool of N GiB could not be built with M GiB free`
at the top of the log is a neighbour: date a line below naming the run and the binary, re-run the
job once, and do not debug it. A pool that *was* built and a test that then dies with `Maximum
pool size exceeded` is ours: the budget is too small, and a re-run buys nothing.

- 2026-09-12: CI run `34659896447` on PR #144 (`d41f223a`), `peacock_tpch_tests`: `pool of 69.0
  GiB could not be built with 14.9 GiB free` at 00:08 UTC; `peacock_tpchv_tests` four binaries
  later saw 103.0 GiB free, so a stranger held ~129 GiB for those minutes. Re-run once.
- bp-benchmarks, dispatch 5: not CI — a non-CI process held 62 GiB and 90–98 % of the card
  for six hours, and `peacock_gpu_benchmarks` measured q6 at six times its committed time
  beside it. Nothing measured beside a neighbour is published; the gate ran green meanwhile.

<a id="t176"></a>
### #176 (CI coverage one-way) the CI coverage guard checks one direction only
Priority: low
`every_rust_test_target_is_named_by_ci` fails when a target exists that no workflow runs. Nothing
fails when a workflow names a target that does not exist.

That way round is not silent, but it is expensive and late: cargo errors inside the cuDF leg after
the C++ build and the dataset generation, so a typo or a step added ahead of its test file costs a
full run to discover. The case: a `--test test_cpu_end_to_end` step was added three commits
before the file, and both legs went red on it.

The converse is nearly free — `workspace_test_targets()` and the `--test` line parsing both exist,
so it is one assertion that every target pipeline.yml names is in the workspace set. The exemption
list already gets this treatment; the step lines do not.

<a id="t306"></a>
### #306 (no_sort ties by position) a `no_sort` comparison pairs rows by position, so tied rows in another order fail
`same_sequence` in `test_support/duckdb_oracle.rs` compares a `no_sort` line row by row, with no
tie rule, so two engines that order tied rows differently both fail, though each is right.

Found at sf40, H100, tp1_single (sf40-baseline). Both sides hold the same rows; only rows tied on
every `ORDER BY` key swap (rows counted from 1):

- tpcds q18 (`… LIMIT 100`): the `AAAAAAAAAACJAAAA` pair at 24/25 and the `AAAAAAAAAAHHAAAA`
  pair at 61/62, each a rollup group whose other keys are null.
- tpcds q73 (`ORDER BY cnt DESC, c_last_name`): the Hamilton pair at 16/17 (Nicholas, Harry) and
  the Jones pair at 22/23 (Cynthia, Robert).

Both rows are `wrong #306` in the H100's `cases.csv` until the oracle accepts a permutation of
tied rows. The fix needs the sort keys, which the oracle does not hold today.

<a id="t309"></a>
### #309 (no en_US in build image) the build image has no en_US.UTF-8 locale, so two script test files cannot run in it
Two tests need a locale that sorts unlike C, and fail without one.
`test_fetch_sf40.py::test_fetch_aggregate_is_in_the_c_locales_order` proves `fetch-sf40.sh` checks
an sf40 set in C order under en_US. `test_image.py::test_the_inputs_are_the_same_in_every_locale`
proves an image's label is the same in every locale. In the build image `locale -a` lists `C`,
`C.utf8` and `POSIX`, and there are no locale sources to build one. So the cost-report job runs
both files on the runner, its `on_runner` list.

The fix: the `locales` package and `en_US.UTF-8` generated in `docker/build.Dockerfile`, then the
files join the in-image loop and their `RUNNER_SIDE` entry in `ci_images.rs` goes. It changes the
build image's inputs, so it needs a publication of its own.
