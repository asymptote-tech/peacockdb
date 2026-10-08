# pbench — run detail

Working notes. The spec is [`pbench.md`](pbench.md) (frozen); the plan the developer works in is
[`pbench-impl.md`](pbench-impl.md).

## Branch and PR

- Branch `ENS-pbench`, forked off `ENS-duckdb-oracle` at `410111cf`.
- Task 3 of chain J, so its PR targets **`ENS-duckdb-oracle`**, not master.
- **Task 2 (`stale-cells`) is `blocked(approved to build)` and has no branch**, so pbench forks off
  task 1's branch rather than task 2's. The content dependency is nil — disjoint corpus lines,
  disjoint registry rows, overlapping only in `build-test.md`'s counts and the CSV — so this is a
  resequence for the human at merge time, not a conflict.
- Workspace: `peacockdb-alpha` (`/home/dmitry/workspace/peacockdb-alpha`).

## Hosts and tools, probed 2026-10-08 before the dispatch

- **shad-gpu: down.** Times out on :22 and :443 while DNS resolves and the host key is intact, so
  it is the host. Down all day.
- **verda: unlocatable.** The name resolves nowhere here and there are no `VERDA_*` credentials to
  find the ephemeral instance's IP with. CPU runs are local.
- **duckdb 1.5.4 is local**, both the python module and the CLI — exactly the pin
  `duckdb_result.py` enforces, so the generator and DuckDB's answers run here.
- **Disk: 6.9 GiB free on `/`, 96% full.** `target/` is 19 GB and
  `target-cudf-rapids-cuda-12.2` 7.9 GB.

## Why this task was dispatched while two others wait

An analyst walked chain J's remaining tasks against the device absence. `stale-cells` is blocked —
see [`stale-cells-detail.md`](stale-cells-detail.md). Tasks 4 to 9 each have a device-only
verification bar and each sits behind an earlier task, `join-session-cpp` included: it compiles
here but its whole bar is a gtest matrix whose `main()` installs an RMM pool, and its
`TableResult` and `evaluate_column` surfaces move under tasks 5 and 6, so pulling it forward buys
~2000 lines of C++ reviewed on a compile and rebased over two refactors of its own foundation.

pbench is the one that progresses: the plan isolates the card into **Task 8** and nothing else.
Tasks 1 to 7, 6b, 6c's cpu leg and 9's documentation half all run in the rust-only tier here.
Three mechanisms make that true, each checked in the tree rather than assumed:

- every pbench gpu cell lands `disabled` (plan Task 5 Step 6), and `gpu_result_coverage` returns
  `Ok` for an absent `gpu-result.txt` when no cell is enabled, while `registry_datasets()` derives
  the dataset list from the CSV — both landed by duckdb-oracle's completeness pass for exactly this;
- `int8-key-group`, the one row with no expected ticket, is deliberately not landed until Task 8,
  which satisfies the "a disabled cell names a ticket" rule; nothing requires every
  `pbench-queries/*.sql` or every `duckdb-result.txt` section to have a corpus line, since both are
  read per line;
- duckdb 1.5.4 is local.

**So this task's realistic end state is `completeness approved`, not `done`** — its own bar has a
device cycle, and CI's `gpu-tests` job rsyncs to shad-gpu, so no PR in this chain can go green
while that host is dark. Two tasks will then share one cycle instead of one.

## Carried into the dispatch

- **27 rust-only cases are red on the base and are not pbench's**: the 26 `duckdb_gpu_*` cases
  (22 enabled tpch gpu cells, 4 tpcds) and
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, all waiting on the
  `gpu-result.txt` that no device has written. A pbench run must leave that count at 27 and add
  none.
- **Run the rust-only loop as plain `cargo test --features rust-only` into `./target`**, not
  through `scripts/cargo-cudf.sh`, which every command in the plan spells. That wrapper redirects
  to `target-cudf-rapids-cuda-12.2`, where a rust-only feature set recompiles the DataFusion stack
  from scratch — and `/` has 6.9 GiB left. `build-test.md:1050` states the rule.
- **`testdata/duckdb_result.py` still pins `--dataset choices=["tpch","tpcds"]` at `:230` and
  spells the default list again at `:238`.** duckdb-oracle deliberately left both to this task:
  one word is not enough, and `--dataset pbench` over an empty `pbench-queries/` would write an
  empty `duckdb-result.txt` rather than failing. The plan's `:104`/`:110` citations for this are
  stale from duckdb-oracle's growth; the usage line at `:20` is right.
- **#252**: six cases read the checkout through `CARGO_MANIFEST_DIR`, which a remote CPU run never
  ships, so anything run on verda shows `duckdb_tpcds_q17/q58/q61/q66`,
  `all_modes_expands_to_the_five_in_either_position` and `every_timed_case_is_enabled_on_a_device`
  red for reasons unrelated to the task. Moot while runs are local.
