# #152 — GpuHashJoin: the build handle does not survive a streamed probe

Read at master c18e063a. Paths relative to `/media/data/peacockdb`. Nothing was built or run.

Short answer to the question asked: **no localized fix exists.** The sixteen-symbol C ABI has no
operation that yields two handles from one without consuming the one, so nothing confined to the
Rust executor or to the C++ session can give a second probe batch a build side. The smallest
non-local fix is **one additive C ABI symbol** — `peacock_handle_retain`, the symbol
`llm-wiki/tasks/refcounted-tables.md` §2 already specifies — implemented today as a deep copy
inside `NodeSession`, with no FlatBuffers change, no wire change, no plan golden moving, and
without the 39-site `shared_ptr` refactor that spec bundles it with. That refactor (#145) then
becomes a pure C++ cost change behind the same symbol.

## 1. Issue

The device refuses every join whose probe side arrives in more than one batch, and refuses a Left
or Full outer join at its first probe batch whatever the batch count. The CPU answers both.

Where the refusal is raised:

- `peacockdb-core/src/executor/gpu_backend/join.rs:304-314` — `build_copy`: `Input::BuildSideCopy`
  is resolved by handing the first probe call the original build handle (`self.build.take()`,
  `:257`) and refusing the second by name (`"probe batch {N} has no build side left, since the call
  for batch 1 erased it (#152)"`).
- `join.rs:293-299` — `copy_of`: `Input::BatchCopy` is refused unconditionally (`"this join's recipe
  copies its probe batch … and the ABI has no copy … (#152)"`). It takes a `&mut Option<GpuBatch>`
  it never reads.

Which recipes name a copy (`peacockdb-core/src/wire/join.rs`, `wire/attach.rs`):

| recipe | copy named | where |
|---|---|---|
| Inner, Right, RightSemi, RightAnti (one call per probe batch) | `BuildSideCopy` | `wire/join.rs:57` |
| Left, Full — the key project | `BatchCopy` | `wire/join.rs:75-79` |
| Left, Full — the per-call Inner/Right join | `BuildSideCopy` | `wire/join.rs:89` |
| nested-loop Inner | `BuildSideCopy` | `wire/join.rs:349` |
| cross join | `BuildSideCopy` | `wire/attach.rs:333` |

Only the build-side semi family (LeftSemi, LeftAnti, LeftMark: key project alone per batch,
`wire/join.rs:66-81`) and the single-batch shapes (nested-loop Left, filtered semi/anti) name no
copy, which is why those are the only joins the device streams today.

What it disables (from `SCRATCH/00-tickets.md`, checked against `testdata/cost-registry.csv`): 79
registry rows carry `152` in `tickets`; every one has all five `gpu_*` cells `disabled` except
`tpch/q19`, whose `gpu_tp1_single` is enabled. `corpus_cases.inc` attributes the four non-tp1-single
cells of most join queries to #152 and, for queries whose probe is a union of fact tables (`tpcds/q71`,
`tpcds/q2`) or whose join is a Left/Full (`tpch/left_join`, `tpcds/q97`, `tpch/q13`), the tp1-single
cell too (`corpus_cases.inc:60-61, 107-110, 159-161, 187-190, 201-203`). Correction to 00-tickets: the
registry's `tickets` column records the *first* cause observed per row, and the causes are ordered by
where the run dies — a join refusing before the unload hides #183/#185/#187/#191. So #152 is the
recorded blocker on 79 rows but the *sole* blocker on far fewer; §6 works that out.

## 2. Root cause

Two facts, one on each side of the ABI, and the recipe vocabulary sitting between them.

**The C++ session consumes every input it reads.** `NodeSession::Impl` keeps intermediates in
`std::unordered_map<uint64_t, TableResult> registry` (`cpp/src/node_session.cpp:179`), and
`TableResult` owns its table exclusively — `std::unique_ptr<cudf::table> table`
(`cpp/src/plan_executor.h:15-18`). Every arm of `execute_node` does
`inputs.push_back(std::move(it->second)); impl_->registry.erase(it);` before calling the operator:
the map arm at `node_session.cpp:453-457`, the collapse arm at `:267-272`, the repartition arm at
`:364-369`; `slice_handle` does the same at `:525-530`. The operator then takes the moved
`TableResult` by value through `take_input` (`cpp/src/operators/dispatch.cpp:102-110`, `return
std::move((*in->items)[in->idx++])`) and `execute_one` owns the vector for the call
(`cpp/src/peacock/operators.h:56-58`). `execute_hash_join` (`cpp/src/operators/join.cpp:42-46`) only
ever reads `left.table->view()` — it never needs to own the build side — but the ownership model
gives it no other way to receive one.

**No symbol mints a handle from a handle without consuming it.** `cpp/include/peacock_gpu.h:141-201`:
`execute_node` consumes its inputs; `execute_scan_rowgroups` reads parquet; `slice_handle` "CONSUMING
`handle` as every operation on a resident table does" (`:175-182`, and `node_session.cpp:525-542`
erases before copying); `result_from_handle` does not consume but produces an IPC buffer on the host,
not a handle; `handle_release` and `end_plan` only destroy. Nominal workarounds fail on the same fact:
`slice_handle(h, 0, UINT64_MAX)` is one handle in, one out; a `CudfCoalescePartitions` or
`CudfRepartition` arm handed one handle consumes it and emits one (or N deep-copied slices of it, but
the input is gone). The frozen fbs vocabulary also has no node returning its input beside its output
(architecture.md, "What the frozen surface costs").

**The recipe declares the copy and the Rust executor cannot make it.** The planner's capability
matrix (`peacockdb-core/src/plan/join.rs:405-444`) says the probe streams for Inner, Right, Left,
Full, RightSemi, RightAnti, so the recipe writer names `Input::BuildSideCopy` per probe batch
(`wire/join.rs:41-63`) and, for the two build-preserving outers whose key project and per-call join
both read the batch, `Input::BatchCopy` (`wire/join.rs:66-92`). `Input`'s own docs say what happens
next (`peacockdb-core/src/wire/mod.rs:135-144`): "The surface has no copy symbol, so a recipe naming
this is one an executor refuses until #145". `GpuProbingJoin::make` (`join.rs:241-272`) resolves the
inputs to raw `u64`s and hands them to `execute_node` (`gpu_backend/mod.rs:217-234`), which consumes
them; `GpuBatch::consume` (`executor/gpu_batch.rs:13-16`) skips the `Drop` release for exactly that
reason. So after the first probe call the build handle is erased on the device and `self.build` is
`None` on the host.

**The CPU twin has the copy for free.** `cpu_backend/join.rs:238-249`: `run_node(keys,
vec![vec![batch.clone()]])` and `run_node(join, vec![vec![self.build.clone()], vec![batch]])` — an
Arrow `RecordBatch` clone is an `Arc` bump. The two backends run the same decomposition; the device
lacks one primitive. The Python model (`scripts/exec_model/operators/recipe_join.py:125, 272`) makes
the same two copies through `NodeSession.copy_handle` (`operators/recipe.py:392-405`, "**Not on the
frozen surface.**") and has been run over the whole corpus at three layouts against the pandas
oracle, so per-probe-batch copies are a proven semantics, not a guess.

Why the corpus sees it as "four modes out of five": a probe side batched per row group or per
estimator target reaches the join as several batches; at tp1-single a scan is one batch per chunk,
so most joins get one probe batch and the run goes on to die at the unload instead
(`corpus_cases.inc:107-110`). A probe side that is a union of fact scans is several batches even at
tp1-single (`:187-190`, `:201-203`).

## 3. Localized fix — as localized as the surface allows

**One additive ABI symbol, a deep copy today, nothing above the ABI moves when #145 makes it O(1).**

### 3.1 C++ — `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp`

Declare on `NodeSession`, beside `slice_handle` (`plan_executor.h:118-121`):

```cpp
/// A second handle on the rows behind `handle`. The input is NOT consumed — the one
/// operation on a resident table that leaves its input standing — and the two handles are
/// independent names, each disposed of once, by a call that consumes it or by `release`.
/// Today the second handle owns a copy of the rows; #145 makes the two share one
/// allocation, which changes what this costs and nothing about what it means.
uint64_t retain(uint64_t handle);
```

Define in `node_session.cpp` after `slice_handle` (`:542`):

```cpp
uint64_t NodeSession::retain(uint64_t handle) {
  auto it = impl_->registry.find(handle);
  if (it == impl_->registry.end())
    throw std::runtime_error("NodeSession::retain: unknown input handle");
  TableResult copy;
  copy.column_names = it->second.column_names;
  copy.table = std::make_unique<cudf::table>(it->second.table->view());  // deep copy until #145
  uint64_t out = impl_->next_handle++;
  impl_->registry.emplace(out, std::move(copy));
  return out;
}
```

`std::make_unique<cudf::table>(view)` is the copy `slice_handle` already makes (`:537-538`); nothing
new is being trusted. No `NodeStats` out-param: rows and bytes are known from the original batch.

### 3.2 C ABI — `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`

Add, in the per-call group after `peacock_executor_slice_handle` (`peacock_gpu.h:175-182`):

```c
/// Mint a second handle on the same resident table. `handle` is NOT consumed: the two are
/// independent names, consuming or releasing either leaves the other valid, and the table
/// lives while any handle on it does. A failure leaves the session standing — nothing was
/// consumed — and an unknown handle is a failure, not a new handle to nothing.
/// @return 0 on success, non-zero on failure.
int peacock_handle_retain(peacock_executor_t* executor, uint64_t handle, uint64_t* out_handle);
```

Body in `gpu_executor.cpp` next to `peacock_executor_slice_handle` (`:271-290`), shaped like
`peacock_result_from_handle` (`:292-321`) on failure — set `last_error`, return 1, **do not**
`session.reset()`: the registry is unchanged when the copy throws (the `emplace` is the last
statement), so this door belongs with the one that "reads a handle and touches nothing". Extend the
FAILURE POLICY comment at `peacock_gpu.h:133-139` by one clause naming it there.

Return a **new id** rather than a count on the old one, for the reason `refcounted-tables.md` §2
gives: `peacock_handle_release` is documented idempotent (`peacock_gpu.h:197`), and a decrementing
release would break that. Same signature and contract as that spec's symbol minus its "O(1) — no rows
are copied" sentence, which becomes true when #145 lands and is the only line that then changes.

### 3.3 FFI — `peacockdb-ffi/src/lib.rs`

One `extern` after `peacock_executor_slice_handle` (`:131-137`):

```rust
/// Mint a second handle on the same resident table without consuming `handle`; each of
/// the two is then disposed of once, by a consuming call or by `peacock_handle_release`.
pub fn peacock_handle_retain(executor: *mut PeacockExecutor, handle: u64, out_handle: *mut u64) -> i32;
```

### 3.4 Rust executor — `peacockdb-core/src/executor/gpu_backend/join.rs`

- Delete `copy_of` (`:289-299`) and `build_copy` (`:301-314`), the `probes` field (`:139`) and its
  increment (`:165`) — it existed only for the refusal message.
- Replace the two arms in `make` (`:251`, `:256-259`) with a retain that leaves the original where it
  is:

```rust
Input::BatchCopy => vec![self.retain(batch.as_ref(), "the probe batch")?],
Input::BuildSideCopy => vec![self.retain(self.build.as_ref(), "the build side")?],
```

```rust
/// A second handle on a batch a later call still needs — the build side for the next probe
/// batch, or this batch for the join after its key project. The call consumes the retained
/// handle; the original stays here until the call that consumes it or the drop that releases
/// it.
fn retain(&self, batch: Option<&GpuBatch>, what: &str) -> Result<u64, BackendError> {
    let batch = batch.ok_or_else(|| BackendError::new(format!("{what} was already consumed")))?;
    let mut out = 0u64;
    let rc = unsafe { peacock_handle_retain(self.join.executor, batch.handle(), &mut out) };
    if rc != 0 {
        return Err(BackendError::new(format!(
            "handle_retain({}) for {what}: {}", batch.handle(), last_error(self.join.executor)
        )));
    }
    Ok(out)
}
```

  The retained handle is a bare `u64` handed straight into `execute_node`, which consumes it — no
  `GpuBatch` wraps it, so there is nothing to release and no accountant hold to balance. If a later
  input in the same call fails to resolve, the retained handle sits in the registry until
  `end_plan`, exactly as an already-`consume()`d `Input::Batch` does today; a failure ends the query.
- `Input::BuildSide` (`:252-255`) is unchanged: the finish is the last reader and takes the original.
  For a join with no finish, `finish_and_fetch` returns at `:186-188` and dropping `self` releases
  the build through `GpuBatch::Drop` — once, at the end of the stream, which is the release count
  the CPU has.
- Rewrite the module doc (`:1-9`) and the `build` field doc (`:134-136`): `None` after the finish
  consumed it; a join with no finish keeps it until dropped. `build_bytes`' doc (`:143-144`) follows.

### 3.5 Accounting — `gpu_backend/backend.rs:278-294`

`resident_bytes` is unchanged and becomes truthful: the build side now really is resident for the
whole probe stream, as `cpu_backend/join.rs:216-218` already reports on the CPU.
`scratch_bytes` today is `build_bytes + n_bytes` when the probe reads the build, which is exactly
one build-side copy plus one batch-sized output; add `n_bytes` once more where the recipe names
`BatchCopy`, mirroring `probe_reads_build` (`join.rs:157-162`) with a `probe_copies_batch()` that
asks `per_probe` for `Input::BatchCopy`. That is the deep copy's whole footprint, and it is a
transient inside the call, gone when `execute_one` returns.

Nothing in `driver/` changes. Nothing in the estimator changes — it already holds a join's build
side as a constant (architecture.md, "Batch sizing").

### 3.6 Deliberately not touched

- `TableResult`'s ownership (`plan_executor.h:15`), `take_input`, `execute_one`, every operator: the
  39-site `shared_ptr` change is #145's and is independent of this symbol — after it, `retain`'s
  body is `copy.owner = it->second.owner; copy.view = it->second.view;` and nothing else moves.
- `Input::BuildSideCopy` / `BatchCopy` names and docs' *meaning*: with a deep copy the name is
  accurate. The rename to `…Again` that `refcounted-tables.md` §4 proposes belongs with #145, when
  the copy stops being one; keeping the names here is what keeps all ten `*.plans.txt` byte-identical
  (`build copy` appears 69 times in `tpch.sf1/tp1-rowgroup.plans.txt` alone). Only the sentence "The
  surface has no copy symbol, so a recipe naming this is one an executor refuses until #145" in
  `wire/mod.rs:135-138, 141-144` is rewritten to name `peacock_handle_retain`.
- `AbiSymbol` (`wire/mod.rs:45-50`): a retain is how an executor resolves an input, not a call the
  recipe schedules, so it gets no variant and no `Call`. Adding one would move every join line in
  the goldens for no information the `Input` does not already carry.
- The planner, `capability`, `per_call_join_type`, `finish_join_type`, the fbs schema,
  `recipe-payloads.txt`, the `.cpu.txt`/`.cost.txt`/`.result.txt` goldens (CPU-authored).
- `finish_without_keys` (`join.rs:209-238`) and `without_build` (`:103-114`): #173 and #175, not this.

### 3.7 CPU and GPU stay one engine

Both backends already run the identical decomposition from the identical recipe facts (`plan/join.rs`
is the single reader for both: `wire/join.rs:34`, `cpu_backend/join.rs:70-84`, `gpu_backend/backend.rs:125-136`).
The CPU makes its two copies at `cpu_backend/join.rs:240, 248` by `Arc` clone; after this change the
device makes them at the same two points by `retain`. No CPU code changes. The device corpus tier
then compares each re-enabled cell against the cpu-authored section read-only (plan shape, `in_rows`,
per-batch lists, bytes — `tests/common/corpus_gpu.rs:87-113`), which is the agreement check.

### 3.8 Tests that pin today's behaviour and must move

Red-before-fix is available cheaply, which the spec notes; the loop in the recipe walk fails today
with `NodeSession::execute_node: unknown input handle` from `node_session.cpp:455`.

- `cpp/tests/gpu/test_plan_executor.cpp`, a `HandleRetain` suite beside `SliceHandle` (`~1300-1360`,
  same `customer_scan_plan`/`CApiPlan` idiom): retain then consume the retained id through
  `slice_handle`, read the original — rows intact; consume the original, read the retained — both
  orders; retain of an unknown handle returns non-zero and mints nothing; and the session is still
  loaded after that failure (a scan still runs), unlike `SliceHandle.AnUnknownHandleFails`.
- `peacockdb-core/tests/test_gpu_abi.rs` (header `:1-7` says "three per-call symbols" → four): one
  test — retain, wrap the retained id in a `GpuBatch` and drop it, export the original still works;
  and a retained id consumed by a slice leaves the original exportable.
- `peacockdb-core/tests/test_gpu_executors/join.rs:53-88`
  `a_left_joins_probe_batch_cannot_be_read_twice_and_says_which_ticket` becomes the positive test the
  fixture was built for: Left over `(a,6),(b,5)` ⋈ `(a,2)` answers `(a,6,a,2)` per probe and
  `(b,5,NULL,NULL)` at done through the pad project. `:249-312`
  `a_second_probe_batch_of_a_copying_join_is_refused_by_name` becomes "a second probe batch sees
  the same build side": both row groups' matches asserted, and one batch out per probe batch.
- `peacockdb-core/tests/test_gpu_recipe_walk.rs`: `resolve` (`:206-224`) gets distinct arms —
  `Input::BatchCopy` / `Input::BuildSideCopy` call `peacock_handle_retain` on `at.batch` / `at.build`
  (the `Session` there already wraps the raw ABI); the join arm's one-probe-batch assert (`:437-446`)
  becomes a loop over `probe_lane` holding `at.build` across batches and handing the original to the
  finish or releasing it; a third knob set `MANY_BATCHES { target_partitions: 1, sizing:
  BatchSizing::OneBatchPerRowGroup, .. }` beside `ONE_LANE`/`TWO_LANES` (`:43-56`); one query using
  it — §5's Inner query, whose probe (`customer`) is two row groups; and `driven` (`:780-792`) drops
  the `CrossJoin | NestedLoopJoin` refusal text if a shape here plans them (the corpus's
  `cross_join`/`nested_loop_join` SQL does), else re-words it to "no shape here plans one" — the
  `PROVEN` array is checked both ways, so the two move together.
- `peacockdb-core/src/wire/tests.rs:150-168, 172-214`: unchanged — the recipe was always right.
- `peacockdb-core/tests/test_cpu_end_to_end.rs:333` comment ("the only cell with no device path at
  all (#152)") and `tests/test_cpu_corpus.rs:84-88` doc ("the moment #152 clears somebody enables
  device modes in bulk") are rewritten as the bulk enable happens.
- `scripts/exec_model/operators/recipe.py:29, 392-405` and `recipe_join.py:16`: the docstrings stop
  saying `copy_handle` is off-surface and name `peacock_handle_retain` as what it models; the
  `copies`/`copied_bytes` tallies and `tests/test_join_capability.py:439` stay — they are #145's
  evidence now.

### 3.9 Registry, corpus declarations, wiki

- `testdata/cost-registry.csv`: `152` leaves the `tickets` column of all 79 rows; each cell is then
  run on shad-gpu in batches of about five (`build-test-shadgpu.sh`, as T19 did) and either enabled
  or re-attributed to the next cause. `corpus_cases.inc` `gpu_modes` and the batch comments at the
  lines in §1 follow the runs. This is the slow part; the code is not.
- `llm-wiki/architecture.md`: "What a streamed probe costs" (`:496-502`) — "None of those copies
  can be taken — the surface has no symbol for one" becomes "each is one `peacock_handle_retain`,
  a device copy until #145"; "From node to seqs" `GpuHashJoin` row (`:762`) — "the build handle
  would need copying before each, since the call consumes it (#152)" → "the build handle is
  retained before each, since the call consumes what it is given"; "Three additive ABI symbols"
  (`:791-800`) → four, with a bullet: a streamed join would otherwise have its build side erased by
  its first probe call; "What the frozen surface costs" rows one and two (`:838-846`) → the cost is
  the copy's bytes, the unfreeze #145; "The ABI is sixteen symbols in five groups … the three
  per-call entry points" (`:885-890`) → seventeen and four (eighteen if `operator-harness.md`'s
  `peacock_handle_from_arrow` lands first — reconcile the count against the header, not this note).
- `llm-wiki/build-test.md:21, 22, 44`: the "Executors on a device" row drops "Left and Full
  outright, a second probe batch" from its refusal list; "Per-call ABI" says four symbols; the
  device-corpus row's cell count and ticket list move with the runs.
- `llm-wiki/tickets.md`: #152 archived when its cells are gone from the registry (the spec's own
  rule); #145 (`~:696-706`) loses "T16 refuses a second until this lands" and gains "makes
  `peacock_handle_retain` O(1)", and its "No ABI change" is corrected as the spec says; #155's two
  #152 rows (`:396-397`) → "removed by `peacock_handle_retain`; its copy by #145".
- `llm-wiki/tasks/refcounted-tables.md`: §2-§4 and the recipe-walk half of §6 are this proposal and
  come out of it; what stays is §1, §5, the scatter tests, §7's rename decision and §8. Its
  "closes #152" line moves here.
- `llm-wiki/reports/hacks-audit.md` scaffolding this removes: item 9 (`copy_of`, `build_copy`, the
  "first batch gets the original" trick — deleted, not adapted), item 10 (the walk's conflated
  `resolve`, the one-probe-batch assert, two refusal arms), item 13's first bullet (decided: names
  stay, goldens stay). Respects: item 11 (`finish_without_keys`'s LeftAnti arm is #173's), the six
  empty-lane arms (#173/#175), item 12 (`schema_digest` hashes names only — more cells now reach the
  comparator, and #183 owns that sentence).

## 4. Alternatives rejected

- **Keep the handle in the registry when the seq is a join (C++ rule keyed on node kind).** An
  implicit behaviour switch (`coding-style.md`), breaks "input handles are CONSUMED" for one kind,
  and the Rust side then has to know which of its handles survived; `refcounted-tables.md` §3 gives
  the same reason — the wire carries a menu of kernels, not the driver's chain.
- **`slice_handle(h, 0, UINT64_MAX)` as the copy.** It consumes its input (`node_session.cpp:530`);
  one handle in, one out. Changing it to not consume rewrites a frozen symbol's documented contract
  and reddens `SliceHandle.*`, `slicing_keeps_the_rows_named_and_consumes_the_handle` and the limit
  executor's release logic (`gpu_backend/accumulate.rs:433`).
- **Borrow semantics on `execute_node` (never erase; Rust releases).** Changes a frozen symbol's
  contract at every call site (five executors, three test crates, 27 gtests) and still needs shared
  ownership because `take_input` moves by value — it is #145 plus an interface change, the option
  the spec's §1 rejects for the same reason.
- **Planner: coalesce the probe under every copying join.** Ends streaming ("one call means the
  whole join result materializes as one table"), is engine-specific plan shape the device tier
  cannot have (it asserts the cpu's plan), and moves every plan golden.
- **Re-read the build side from parquet per probe batch through `execute_scan_rowgroups`.** Only
  a bare scan is a build side; anything computed is not re-readable.
- **An fbs node that returns its input beside its output.** architecture.md's other unfreeze:
  an fbs semantics change touching `read.rs`, `node_children`, every operator's output count and the
  payload golden — far larger than a symbol, for the same effect.
- **The bundled `refcounted-tables.md` as written.** Correct, and this proposal is its strict
  prefix; rejected as the *smallest* step because 39 sites in 11 files, the input rename (ten plan
  goldens) and an accountant that under-reports (its §8) are #145's costs, none of which #152 needs.
- **A plan-time refusal instead of the run-time one.** Hides the cells further; not a fix.

## 5. Minimum corpus query

Two halves, two queries, both against `testdata/tpch.sf1`. Neither exists in `testdata/`; both are
smaller than any corpus join (`hash-join.sql` builds over 1.5M `orders` rows).

**Half one — the build side across probe batches (Inner).**

```sql
SELECT n.n_nationkey, c.c_custkey
FROM nation n JOIN customer c ON c.c_nationkey = n.n_nationkey
WHERE c.c_acctbal > 9990
```

`nation` is one row group and the smaller side, so DataFusion makes it the build; `customer` is two
row groups (122880 + 27120 — `test_gpu_abi.rs:27-28`) and under `SMALL_TABLE_BYTES` on its three
projected columns, so it stays one lane at tp4. Result: two integer columns, about 150 rows — no
string (#183), no decimal (#187), no aggregate (#185, #180). Plans at all five modes on both
backends (Inner streams: `plan/join.rs:416`); the CPU runs it everywhere. On the device: **tp1-single
and tp4-single pass** (one probe batch, the original handle is handed over); **tp1-rowgroup and
tp4-rowgroup refuse** at the second probe call in `gpu_backend/join.rs:304-314` with `"probe batch 2
has no build side left, since the call for batch 1 erased it (#152)"`; tp4-sized refuses iff the
estimator's target leaves the two row groups in two batches (read `partition_groups` off the plan).
Backend: GpuBackend only. The recipe walk at `MANY_BATCHES` (§3.8) is the same query with no driver.

**Half two — the probe batch read twice (Left).**

```sql
SELECT c.c_custkey, o.o_orderkey
FROM customer c LEFT JOIN orders o ON o.o_custkey = c.c_custkey
WHERE c.c_custkey < 20
```

The filtered `customer` is the build (Left preserved side), `orders` the probe. Plans at every mode
(Left with no residual filter streams with a finish: `plan/join.rs:426`); the CPU runs it. The device
refuses **at every mode at the first probe call** in `join.rs:293-299` — the key project names
`BatchCopy` (`wire/join.rs:75-79`) — so this half is independent of batch count. Result ≈ 190
integer rows.

Where the corpus already carries each half: `tpch/q19` (Inner; its tp1-single device cell is the
one join cell that passes today) and `tpch/left_join` (Left; refused at every mode).

## 6. Cells re-enabled

All 79 rows move off #152. Which then go green only a device run tells, because the registry
recorded the first refusal, not the next. Sorting the 79 rows by what their `tickets` column names
besides `152` (closed #97/#115/#116 dropped):

- **No other open ticket named — 7 rows, 34 cells** (`tpch/q19` ×4, `tpch/q13`, `tpch/left_join`,
  `tpcds/q41`, `q71`, `q75`, `q97` ×5). Inferred next cause from the result schema, since none was
  observed: `q41` (`i_product_name`) and `q71` (`i_brand`, a decimal sum) → #183/#187; `left_join`
  → #187 (same `sum(l_quantity)` as `hash_join`, which the registry records at `(25,2)` vs 38;
  `corpus_cases.inc:60-62`); `q75` → #187 (decimal `sales_amt_diff`); `q13` → #185 risk (two grouped
  merges; q93 shows grouped merges misreport); `q97` → #185 risk (keyless merge over a multi-batch
  Full join; q14 shows the `[[3,3,3,3]]` vs `[[1,1,1,1]]` shape); `q19` ×4 → the best candidate to
  go green — its tp1-single cell already passes with the same result type, and its risk is #185 on
  the merge that tp1-single skips. Honest count of cells this fix alone is *likely* to turn on: the
  four `tpch/q19` cells, possibly `q13` and `q97`.
- **Legacy device tickets only — 2 rows, 10 cells**: `tpcds/q2` (#56), `tpcds/q78` (#60);
  unverified since T19 never got past the join.
- **Stay off behind a named next cause — 70 rows**: #183 on 49 (44 alone, plus `q69`/`q21`
  with #59/#80, `q80`, `q66`, `tpch/q16`, `rollup_over_join`), #185 on 9 (`q38 q48 q93 tpch/q3
  q14 join_int`, `q87`, `q88`, `q96`), #187 on 8 (`q33 tpch/q2 hash_join`, `q16 q94 q95`, `q61`,
  `q77`, `q90`), #191 on `tpch/q8`, and the cpu-side #175/#180/#189 rows whose tp4 cells have no
  cpu twin to compare against (`every_device_cell_has_a_cpu_cell_at_the_same_mode`).

The `corpus_query!` lines to touch first are therefore `tpch/q19` (`corpus_cases.inc:25`), then
`tpch/q13` (`:34`) and `tpcds/q97` (`:164`); every other line changes only its comment's attribution
until #183/#185/#187 land.

## 7. Risks and unknowns

- **The copy's cost is unmeasured.** B × build_bytes of device memcpy per lane, which #152's text
  asked to weigh off the goldens ("tpch q3 is 24:1 by rows and 73:1 by bytes") and nobody did;
  `tpcds/q11` takes 336 and 88 probe batches against fact-sized builds (`corpus_cases.inc:223-227`).
  Correct and slow rather than wrong; no test asserts time; #145 removes it. The extra transient is
  priced by the existing `scratch_bytes`, and the device corpus tier passes no budget (#192).
- **Which cells go green** — §6 is inference from result schemas, not observation.
- **`std::make_unique<cudf::table>(view)` over string and decimal columns** — the same call
  `slice_handle` makes, so already exercised, but on whole build sides rather than slices.
- **Two new symbols in flight.** `operator-harness.md` adds `peacock_handle_from_arrow`; the
  architecture.md count and the header's grouping must be reconciled by whichever lands second.
- **The recipe walk's `assert_results_match` over several exported batches** — the inner-join test
  compares as a multiset (`:659`), so batch order should not matter; unverified for a 150-row result.
- **Whether `customer` really drops to one lane at tp4** under the byte rule — read the plan; if it
  keeps four lanes the two row groups land in two lanes of one batch each and only the tp1-rowgroup
  cell of §5's first query exposes the defect.
- **`finish_without_keys`'s LeftAnti arm** (`join.rs:211-216`) now sees a build that was never
  consumed by a probe call, which is the case it was written for; the `None`-returns-empty hazard
  hacks-audit item 11 names is unchanged and still #173's.
- Not verifiable by reading: that cuDF's `hash_join`/`gather` never mutate their input views (they
  take `table_view` by value; every operator here reads `.table->view()` only), which is what makes
  a per-call copy semantically identical to sharing.

## 8. Complexity

**M.** Code: 7 files — `peacock_gpu.h`, `gpu_executor.cpp`, `plan_executor.h`, `node_session.cpp`,
`peacockdb-ffi/src/lib.rs`, `gpu_backend/join.rs`, `gpu_backend/backend.rs` — about 60 lines added
and 40 deleted, plus doc rewrites in `wire/mod.rs` and `recipe.py`. Tests: a gtest suite, two
`test_gpu_abi` cases, two `test_gpu_executors/join.rs` rewrites, and the recipe walk's loop, knob set,
`resolve` arms and one query. A frozen surface changes **additively** (the C ABI gains its
seventeenth symbol); the FlatBuffers schema, the wire format and the declared-schema contract do not
change. **No golden regenerates**: `*.plans.txt` keep `build copy`, `recipe-payloads.txt` is
untouched, the execution goldens are cpu-authored. What makes it M rather than S is not the code
but the 79-row registry re-attribution, which is one device run per cell on shad-gpu and lands
most rows on #183/#185/#187 rather than green.
