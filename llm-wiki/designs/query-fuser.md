# The query fuser: its hold rule, cap rule and fused format

The rules `src/prototypes/fuser/` implements and the format of its results, moved verbatim from
query-fuser's detail file when the task was archived. The spec is query-fuser's entry in
`llm-wiki/archive/archived-tasks.md`; "spec 1a" and "spec 4" are its items, and the review
rounds cited are in `git show d290698f:llm-wiki/tasks/query-fuser-detail.md`.

## The hold rule (spec 1a)

All numbers are the sf40-baseline tree's, per origin node: Σ `batch_bytes` and Σ `batch_rows`
over lanes and batches. The rule is `decide`, `hold_inputs` and `HoldLedger::choose` in
`prototypes/fuser/mod.rs`.
- An edge's bytes are its child's. A project inserted to select from the shared load stands for
  the reader's own load. The edge right above the shared load carries the union, which nothing
  recorded, so it is not a candidate.
- A buffer above a join runs that join early, so it also keeps the join's build child's bytes,
  for every join below it on the path. A build child that is a buffer an earlier decision
  inserted is priced as the node it buffers (review round 2, nit 7).
- An edge is allowed where every join below it on the path recorded no more rows than its child
  on the path. In a root that waits for the cap, also only where nothing at or under it
  keeps state, for every hold there, a join's as well as the cap's (`HeldReader.waits`). A
  join's buffer placed above state before the cap was known, in a root that then waits, is
  taken out with its record, and its reader decided again under that rule.
- Placement: the allowed edge with the fewest bytes; a tie goes to the lower edge.
- A re-read costs the reader's own load's `loads.tsv` `load_us`, keyed by the query and the seq
  of its `CudfScan` recipe. It is recorded either way. It also adds that load's recorded
  `batch_bytes` back to its root (`reread_bytes`): the load is no longer shared.
- Headroom: `budget − (S + B + max root_bytes)`; over budget when `S + B + max root_bytes >
  budget`. All three come from `cap_inputs` over the fused plan at each decision (review
  round 2, I1): a group a re-read dissolved, and rebuilt, frees its bytes for the decisions
  after, and a buffer taken out is no longer charged. Within one resolve the shared loads are
  as the last rebuild left them, since groups are rebuilt when a resolve ends. Readers are
  decided in the order validation names them.
- The choice, where a buffer is placed. Within the budget: the buffer where its bytes fit the
  headroom, else a re-read. Over budget (review round 1, I2, the coordinator's decision): the
  buffer where its bytes are below `reread_bytes`, else a re-read; a buffer then costs less
  than the re-read it replaces. No allowed edge: a re-read.
- A re-read takes its reader out of its group. After every resolve, in `fuse` and in each
  `settle` round, each group's shared load is rebuilt over the members left: the union of
  their own loads, every inserted project re-pointed at it. A member left alone reads its own
  load, and its group is gone from `shared` (review round 1, B1). The cap then reads the
  rebuilt loads.
- A chooser that buffers a waiting reader above state is refused, `PlanError::Invalid` naming
  the chooser, rather than looping between it and validation (review round 1, I1).

## The cap rule (spec 4)

`budget = pool − pool × 10 / 100`, the pool being the rmm pool the run installed (69 GiB
unless `PEACOCK_RMM_POOL_BYTES` says otherwise). The margin covers what the accountant's peak,
a lower bound, leaves out. `S = Σ shared_bytes` and `B = Σ buffer_bytes` are charged whatever
the cap. Roots are taken largest first; the cap is the largest k with `S + B + Σ top k ≤
budget`, at least 1. `over_budget` is `S + B + max root > budget`, and the run goes ahead.
Inputs: per shared load, its member with the largest recorded load bytes, scaled from that
member's planned bytes to the union's; per buffer, its decision's bytes; per root, its
`peak_bytes` less the recorded bytes of its loads now shared. `settle` loops: the cap, then
`resolve_cap_holds` under it with the ledger, until the cap stops falling.

## The fused format

`testdata/benchmark-results/h100-80gb-hbm3/fused/<ds>.sf40/tp1_single.fused.txt`, one
`== <batch>` section per batch in `BATCH_SIZES` order (`first3`, `first10`, `first20`, `all`).
Written by `render_fused_section` and read back by `parse_fused_section`
(`prototypes/fuser/section.rs`), the page's table and `every_fused_section_adds_up` alike. A
batch the list does not exceed is `skipped: same set as all: …`; a batch that panicked is
`status=failed reason=…` and the five provenance keys, as a tree section is.

```
--- batch ---
queries=[q1,q2,q3]
outcome=measured                  # or out-of-memory, then reason=<one line>
shared=[lineitem:q1+q3,orders:q3]
unshared=[q2/lineitem:limit-cut]
reads=[lineitem:2,orders:1]
buffers=1
buffer_bytes=123456789
rereads=1
hold query=q3 table=lineitem hold=GpuHashJoin choice=buffer above=7 bytes=123456789 reread_us=48120 why=fits: 123456789 <= 900000000
cap=2
pool_bytes=74088185856
budget_bytes=66679367270
estimate_bytes=51234567890
over_budget=false
answers=match                     # measured only
--- merge ---
saved_us=812
entry saved_us=640 members=q1:3,q3:5 node=GpuFilter: …
--- pipeline ---
model=<the model, one line>
load_order=[lineitem,orders]
serial_us=…
makespan_us=…
saved_us=…
fused_measured_us=…               # measured only
pipelined_estimate_us=…           # measured only
--- tree ---                      # measured only
<render_timings_of_roots of the chosen run>
--- run ---
<run_section of the chosen run; out of memory: the five provenance keys alone>
```

Keys:
- `queries`: the roots in order, the batch's prefix of the H100 `cases.csv` list.
- `shared`: per shared load, its table and the queries reading it, a query once per scan.
  `unshared`: `<query>/<table>:<why>` per scan left alone beside others of its file, `why`
  `limit-cut`, `nulls:<column>` or `rows-only`.
- `reads`: per table, the distinct loads of it in the fused plan: one per shared load, plus each
  re-read and unshared scan. `Σ reads` is the tree's distinct `GpuLoadParquet` nodes.
- `buffers`, `buffer_bytes`, `rereads`: the count of `choice=buffer` lines, the Σ of their
  `bytes`, and the count of `choice=re-read` lines.
- `hold`: one per held reader, in the order they were resolved. `hold` is the node whose hold it
  enters (`GpuHashJoin`, or `GpuUnload` for a root waiting on the cap). `above` is the pre-order
  index, in that query's own tree, of the node the buffer sits on (`-` for a re-read). `bytes`
  is what a buffer would keep, `reread_us` what a re-read costs (`loads.tsv`), recorded either
  way. `why` is last and keeps its spaces: `fits: …`, `too large: …`, or `no edge: …`.
- `cap`, `pool_bytes`, `budget_bytes`, `estimate_bytes`, `over_budget`: the cap rule below, the
  pool being what the run installed. `estimate_bytes <= budget_bytes` unless `over_budget`.
- `answers=match`: every root's answer equalled its query run alone on the same device; a
  mismatch fails the batch, so no other value is written.
- `--- merge ---`: the merge hint. `saved_us` the batch's total, then one `entry` per group,
  largest saving first. A group's members come from two or more queries (the completeness
  pass): a query repeating a subtree with itself over a shared load is no entry; `members` are `<query>:<pre-order index in its own tree>`; `node` is last
  and keeps its spaces.
- `--- pipeline ---`: the pipelining hint, the model stated on one line. `load_order` the tables
  in the order the model loads them; `serial_us`, `makespan_us`, `saved_us = serial − makespan`.
  `fused_measured_us` is the chosen run's `load_us + execute_us` (`device_us`), and
  `pipelined_estimate_us = fused_measured_us − saved_us`, saturating.
- `--- tree ---`: `root <i>:` before each root; a node several roots read is written once as
  `@<k> <line>` and named after as `@<k>`; each node's time line beneath it, bench-rows' format.
- `--- run ---`: bench-rows' trailer, its arithmetic over the tree with each shared node once.
  `runs` holds `MEASURED_RUNS` times, `FUSED_ALL_RUNS` for `all`; `run_us` is its 2nd-smallest.
  `pinned_cache` is the cache at the batch's end, `prefill_*` this batch's own prefill call: a
  later batch whose files an earlier one loaded reports `prefill_files=0`.
- An out-of-memory batch keeps `--- batch ---` (its holds, cap and estimate), `--- merge ---`
  and the pipeline's model lines; no tree, and its `--- run ---` is the five provenance keys,
  which is what a resumed run keeps it by.
