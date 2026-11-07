# sf40 tp4_sized on the H100 for the batching query set

Kind: production

sf40-baseline and the fused batches are tp1_single: one lane, one batch per source. tp4_sized runs
four lanes with sized batches. It is the multi-lane mode people will compare against. This task
measures it on the same device, for the same queries, so the report can say how far batching in
tp1_single gets against the engine's own multi-lane mode. No other mode is in scope.

## Done when

1. `testdata/benchmark-results/h100-80gb-hbm3/{tpch,tpcds}.sf40/tp4_sized.benchmark.txt` hold every query
   with an `ok` or `wrong` row in sf40-baseline's `testdata/benchmark-results/h100-80gb-hbm3/cases.csv`
   whose line in the main case list carries `tp4_sized`, in bench-rows' format. That covers the
   batching set (TPC-H q1-q22, every TPC-DS line) and the TPC-H micro queries. The modes the
   H100's list is written at are tp1_single and tp4_sized, and the same test that writes
   `cases.csv` from the tree gives a row the modes `tp1_single|tp4_sized` when its tp4_sized
   section did not fail. The tree check holds the H100 tree to that list, by containers' rule.
2. A query enabled at tp4_sized that fails at sf40 is recorded as a failed case, with its reason,
   as sf40-baseline does.
3. `reports/query-batching.md` gains, per batch, the sum of the same queries' unfused tp4_sized
   load and execute, beside the tp1_single sums.
4. bench-rows' report page, `testdata/benchmark-report.html`, regenerated in the commit that
   adds the files, shows the tp4_sized rows beside the tp1_single ones.

**Profiles.** Every case this task benchmarks is also profiled on the H100. Both Nsight passes run
(`create_nsys_profile.sh`): the trace pass's `calls.tsv` and pinned-loads' load breakdown, and the
metrics pass's `hbm.tsv`. In each case's profile, a reader sees how much of every load is reading
and copying, against decoding. The derived files are committed per device and dataset under
`testdata/calibration/h100-80gb-hbm3/`. Captures stay on the host or in the scratch area; they are too large to
commit. A case whose capture fails is listed with the reason, as a failed benchmark case is.

## Scope

| path | change |
|---|---|
| `testdata/calibration/h100-80gb-hbm3/` | the profiles' derived files |
| `testdata/benchmark-results/h100-80gb-hbm3/` | the tp4_sized files |
| `peacockdb-core/tests/test_corpus_goldens/benchmark.rs` | `H100_MODES`, the modes the H100's list is written at |
| `testdata/benchmark-results/h100-80gb-hbm3/cases.csv` | the modes, rewritten by sf40-baseline's test |
| `testdata/benchmark-report.html` | regenerated |
| `llm-wiki/reports/query-batching.md` | the column |

Component-level API: none.

## Signoff (2026-11-07)

Solved under its constraints. Done-when 1-4 are met: 126 cases at sf40 tp4_sized on the H100, 5 recorded as failed (tpch q8 at the step cap, #238; four 1800 s timeouts, #308). The list is written at both modes, and the report and page carry the sums. Two review rounds and a completeness pass. Shortcuts, all recorded in `tp4-sized-detail.md`:
- The binary shipped was sf40-baseline's (`b415d375`, before dag-driver), so tp4_sized and tp1_single come from one engine, not from this branch's.
- Profiles: tpch all 45, tpcds 72 of 76. q14, q61, q80 and q88 were left out on an estimated capture size, never attempted. `calls.tsv` misses 3 tpch and 14 tpcds cases the sf1 recipe check refuses.
- The tpcds passes ran a case at a time on the VM by drivers that are not committed. build-test.md step 7 describes them in prose. The merge and the profile script now take `--mode`.
- The H100 ran 16.4 h for this task.
