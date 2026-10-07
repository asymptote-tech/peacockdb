# A scatter's partitions share their table instead of each copying it

Kind: production

**This task closes [#145](../tickets/corpus-coverage.md#t145) (Refcounted handles: stop copying
every partition out of a scatter) and [#197](../tickets/corpus-coverage.md#t197) (the repartition
arm still concatenates a child it can only be handed one of).** Fifth of the join-rewrite chain,
ahead of the join session so the session is written once against the shared-owner handle.
`TableResult` changes here once, to its final shape — one owner per column — which exit-copies
and the join session then use without changing it again. Drawn
from `refcounted-tables.md` §1, §5, §6 (the scatter tests) and §8, which this chain drops; its
join half (§2–§4, the retain symbol) is not needed, since the join session keeps its own build.

## Why it happens

`TableResult` is `{unique_ptr<cudf::table>, column_names}` (`cpp/include/peacock/plan_executor.h:16-19`),
so a handle owns its memory alone. The hash repartition arm (`cpp/src/node_session.cpp:517-578`)
takes its input (`combined`), writes the partitioned copy (`parted`, from `spark_hash_partition`),
then deep-copies each partition out of `parted` into its own table (`:567`). `combined` and `parted`
live until the arm returns, so the peak is about 3× the input (#145's text says 2×; inferred from
scoping, not measured), once per aggregate shuffle and once per join side.

#197: the arm concatenates `child[0]`'s handles, but only one ever arrives. The ticket's reason is
wrong — join-side emits have no coalesce below them; the guarantee is the emitter sending
`&[vec![handle]]` (`executor/gpu_backend/emit.rs:57`) — and so is its cost claim: with one handle
`:537` moves the table, so the dead branch costs nothing. The fix is deletion either way.

## The work

1. `TableResult` takes its final shape, one owner per column:

   ```cpp
   struct TableResult {
     std::vector<std::shared_ptr<cudf::column const>> owners;  // owners[i] keeps columns[i]'s buffers alive;
                                                               // two entries may share one column
     std::vector<cudf::column_view> columns;                   // a whole column of *owners[i], or a row slice of it
     std::vector<std::string> column_names;
     cudf::table_view view() const { return cudf::table_view{columns}; }
     static TableResult owning(std::unique_ptr<cudf::table>, std::vector<std::string> names);
     TableResult slice(cudf::size_type begin, cudf::size_type end) const;   // shares owners
     TableResult select(std::vector<cudf::size_type> const& ordinals) const;  // shares owners
     TableResult with(std::unique_ptr<cudf::column>, std::string name) const;  // appends a computed column
   };
   ```
   Never zero columns (a zero-column view reads 0 rows; the plan's explicit `__rowcount__` keeps
   one). A consumer reads views; one that needs an owning column copies at that site, with a
   reason. A column is freed when the last handle viewing it goes; a slice pins its parent column,
   not the whole table.
   
   The 37 `.table` / `->table` sites across 11 files today (node_session 17, join 6, filter 4,
   project 2, gpu_executor 2, one each in aggregate, dispatch, limit, sort, union, window) read the
   view; a producer wraps its fresh table as owner and whole view. `execute_one`'s signature does
   not change. Erase-on-read stays; erasing drops a reference.
2. The scatter wraps `parted` with `owning` and registers N handles, each its `slice` — every
   partition's columns view row ranges of `parted`'s columns, no copy. `combined` is dropped as soon as `spark_hash_partition` returns, so the peak is
   2× during the partition (an out-of-place `cudf::partition` cannot do better) and 1× after.
3. #197: the arm takes exactly one handle — `owned`/`views` become one, and `child[0].size() != 1`
   is refused by name.
4. **The cost of sharing**, said in the PR and the wiki: one undrained lane keeps all of `parted`
   alive (worse under skew), and the per-partition timers of p1..N−1 drop toward zero, since the
   copy they timed is gone.
5. **String bytes of a slice.** `out_stats.varlen_content_bytes` (`node_session.cpp:229-240`) uses
   `strings_column_view::chars_size`, which reports the unsliced parent's bytes for a sliced view
   (`cudf/strings/strings_column_view.hpp:89-98`). Once partitions are views, every partition would
   report the whole table's string bytes and the string-keyed emits' `batch_bytes` in the cpu
   goldens would stop matching (tpch q1 and shuffle-additive-avg at tp4: six device cells on
   today). The count is taken from the slice's own first and last offsets. The `out_stats` gtest
   scatters a string column.
6. **The accounting ticket**, filed by this task in `memory.md` when it lands: the driver prices
   each lane's batch alone (`driver/accounting.rs`), so a released lane's bytes leave the model
   while the device still holds them behind a sibling — the model under-reports. The fix it names
   is device-reported residency (`GpuBackend::resident_bytes()`).

## Scope

| path | change |
|---|---|
| `cpp/include/peacock/plan_executor.h` | `TableResult` |
| `cpp/src/node_session.cpp` | the scatter; #197; the registry |
| `cpp/src/operators/*.cpp`, `cpp/src/gpu_executor.cpp`, `dispatch.cpp` | the `.table` sites |
| `cpp/tests/gpu/test_plan_executor.cpp` | the scatter tests |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | the handle's shape; counts; #145/#197 archived; the accounting ticket |

Component-level API: `TableResult` (internal to the C++ library). No C ABI change, no wire change.

## Restriction

Ownership and the scatter only. No change to any operator's output, to the driver, or to
accounting beyond the ticket.

## Tests

gtests, an RMM statistics adaptor around each: a scatter's N handles share `parted`'s column owners (release
N−1, read the survivor); the N outputs concatenate back to the input; `out_stats` per partition
unchanged; peak during the call is input + one partitioned table, and the partitioned table after
it; a child of two handles is refused. No golden moves.

## Verification bar

- device: the gtests; the full gpu tier unchanged (every answer the same); one corpus cycle at the
  tp4 modes, benchmark timings recorded before and after.

## Device workflow

`build-test-shadgpu.sh`, two cycles (gtests, then the tiers).
