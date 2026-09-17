// The corpus, one line per query — read and diffed as a line rather than as up to ten
// macro invocations that can disagree with each other. Included by both corpus binaries,
// which define `corpus_query!` differently: the cpu one expands `cpu_modes`, the device one
// `gpu_modes`, so a query enabled at four modes on the cpu and one on a device is still one
// declaration.
//
//   corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, cpu_oracle, gpu_oracle)
//
// The mode arguments are the five planning modes the plan goldens carry, or'd together, or
// `none`. Every argument is written rather than deduced from the one beside it: a query's
// whole coverage reads off the line.
//
// The twenty here are chosen by plan size off the committed goldens rather than by taste —
// the smallest carrying no ticket — because the constraint is not what can plan but what is
// small enough that a failure is legible. The rest of the corpus is T19.

// --- tpch ---------------------------------------------------------------------
corpus_query!(tpch, 1, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, data_fusion_exact, golden_exact);
// q1 runs everywhere on both engines; q17 at every cpu mode and at tp1-single on the
// device, where its other four modes are #152's build-side copy.
corpus_query!(tpch, 1, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q14, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q19, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q12, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
// THE DEVICE COLUMN. q6 runs at all five modes and q19 at tp1-single; every other device
// cell is off against a ticket. The causes: #152 refuses a join whose recipe copies a side
// the ABI cannot copy (by name rather than by crash), #184 is a cuDF failure in the hash
// scatter, #185 is the device reporting its own output as in_rows where the cpu reports what
// it consumed, #220 is the cpu splitting a join's output into more batches than the device.
// T19 is where the rest are settled query by query.
corpus_query!(tpch, 1, q13, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q15, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q17, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q3, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q10, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 1, the five smallest tpch plans the rollout list carries.
// scan-limit's rows are not determined — an unordered LIMIT over lineitem — so its modes
// need not agree and the oracle is the subset one; filter-project returns most of lineitem,
// which is over the result cap. Both take a live cpu run on the device for that reason.
// scan-limit runs at the tp4 modes only: at the two tp1 modes the planner puts the limit
// in the scan alone, and the cpu backend's reader does not apply it — six million rows
// where ten were asked for, #186.
// aggregate-groupby, shuffle-additive and filter-project run at tp1-single; cross-join is off
// on #220's join batching, scan-limit on #188, where cuDF refuses row groups and a row count in
// one read — #186's shape, erroring rather than ignored.
corpus_query!(tpch, 1, scan_limit, tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_subset, live_cpu);
corpus_query!(tpch, 1, filter_project, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, live_cpu);
corpus_query!(tpch, 1, aggregate_groupby, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, cross_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, shuffle_additive, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);

// T19 batch 2, the five smallest joins. semi-join and anti-join return whole `orders` rows,
// which is over the result cap, so the device compares against a live cpu run.
// nested-loop-join runs at tp1-single. Off: #152's copy shapes for left-join (its probe
// batch) and hash-join (its build side) at four modes; at tp1-single hash-join is #185's
// in_rows at the merge, and semi-join and anti-join are #220's join batching at the unload.
// A row keeping `183` has modes never run past the string class.
corpus_query!(tpch, 1, hash_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, left_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, semi_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, live_cpu);
corpus_query!(tpch, 1, anti_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, live_cpu);
corpus_query!(tpch, 1, nested_loop_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);

// T19 batch 4, the small end of the tpch list emptied, and two firsts.
// shuffle-additive-avg runs everywhere on both engines.
// shuffle-stddev is the first corpus query to need the tolerance oracle, and the tell is which
// mode is exact: tp1-single matches DataFusion to the digit, and it is the mode with a single
// lane and so nothing to reassociate. The other four differ by ~3e-15 — the association order
// of the Welford merge rather than its arithmetic, since a wrong merge would not come right at
// one lane. shuffle-stddev runs at tp1-single on the device; nested-loop-left-join is off on
// #220's join batching, nested-limits on #188 (two intervals on one path, scan-limit's refusal),
// and mixed-join's `none` means "cannot cross to a device" — #168's interval `ScalarValue`.
corpus_query!(tpch, 1, shuffle_additive_avg, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, shuffle_stddev, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_approximate, golden_approx_std);
corpus_query!(tpch, 1, mixed_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, nested_loop_left_join, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, nested_limits, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 5, the tpch list from here being aggregates over joins, and the cpu is where it
// showed. Two of the five are cut before a device ever sees them: q22's nested-loop join
// projects and the cpu backend drops the projection — #190, at every mode; rollup-over-join
// runs at the two tp1 modes and not at the three tp4 ones, which is the shuffle rather than
// the query — #189, whose grouping-set id is UInt8 and whose hasher has no arm for it.
// On the device every query here aggregates over a join: #152's build-side copy where the join
// sees several probe batches, and at tp1-single, which reaches past it, the golden's `in_rows`
// at the merge (#185) for q16, the rollup and q18, and #220's batching for q4.
corpus_query!(tpch, 1, rollup_over_join, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q4, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q16, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, live_cpu);
corpus_query!(tpch, 1, q18, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q22, none, none, data_fusion_exact, golden_exact);

// T19 batch 6, the rest of tpch bar q7, q8 and q2. q11 is out at every mode on both engines:
// its nested-loop join carries a projection and the cpu backend passes `None` where DataFusion
// takes one, so the node declares two columns and the join answers three — #190. The first
// corpus query whose nested-loop join projects, which is why four batches went past it.
// tp1-single is the only mode whose join gets a single probe batch, so it is the only one past
// #152's build-side copy, and what it meets there is per query: #220's join batching for q20
// and q21, #185's `in_rows` at the merge for q5, #191's year for q9.
corpus_query!(tpch, 1, q20, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q5, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q9, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q11, none, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q21, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 7, the end of tpch: three queries rather than five, because these are its deepest
// plans and the regeneration cost has moved into the queries rather than their number.
// #152 at four modes each, and at tp1-single, the mode that always reaches the unload: q7
// and q8 are #191's Int16 for the year extract where the plan declared Int32, a type
// disagreement the export cannot relabel; q2 is #220's batching at its min aggregate.
corpus_query!(tpch, 1, q7, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q8, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpch, 1, q2, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// --- tpcds --------------------------------------------------------------------
corpus_query!(tpcds, 1, q41, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q42, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q3, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q43, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q52, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q55, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
// q96's `count(*)` is declared non-nullable and the state merge answers nullable, at the
// three tp4 modes only — the shuffle is what puts a merge under it. The two tp1 modes
// run, so this is a mode-scoped disablement rather than a query-scoped one.
corpus_query!(tpcds, 1, q96, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q15, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q37, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q82, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);

// T19 batch 3, tpcds, chosen for feature spread rather than size: q48 carries no feature code,
// q21 and q84 are top_n, q93 adds an outer join and q34 a correlated subquery; the three
// smaller candidates carry `avg` and run in batches 18 to 20.
// q84 runs at tp1-single. The rest: #152's build-side copy at the other modes, and at tp1-single
// the device RUNS and the golden catches it — #185 for all four. q93 reports in_rows 7169
// against the cpu's 7486, so the rule is the merge counting its own output rather than its
// input, and `[[1]]` is that rule with no group keys.
corpus_query!(tpcds, 1, q48, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q21, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q93, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q34, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q84, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);

// T19 batch 8, tpcds, chosen by expected executor calls rather than plan size: row groups scanned
// times joins, off the tp4-rowgroup plans; q32 and q26 score as low and run in batch 19.
// #152 at four modes, and at tp1-single #220's batching for q45, #185's `in_rows` for q40,
// q50 and q62 — with q97 as the exception: its outer join copies the PROBE batch, which #152
// refuses at any batch count, so the one mode that usually gets far enough does not.
corpus_query!(tpcds, 1, q45, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q40, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q97, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q50, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q62, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 9, the next five by expected calls. q1 and q7 run at every cpu mode; on the device
// both are #185's `in_rows` at tp1-single, their other modes never run.
// The other three's fifteen device cells: #152's build copy at four modes, and at tp1-single
// #185's `in_rows` for q73 and q79, #220's batching for q94.
corpus_query!(tpcds, 1, q73, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q79, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q1, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q7, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q94, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 10, by expected calls, the `avg` queries left to batches 18 to 20.
// Twenty-five device cells: #152's build copy at four modes, and at tp1-single #220's batching
// for q19 and q91, #185's `in_rows` for q8, q46 and q99.
corpus_query!(tpcds, 1, q19, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q8, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q99, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q91, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q46, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 11, by expected calls. q71 is #152 at tp1-single too, where every other query so
// far has reached the unload: its probe side is three fact tables unioned, and a fact table
// streams in 8192-row batches whatever the mode. `single` is a scan's partitioning, not a
// promise of one batch — what decides #152's build half is the probe side's SIZE.
corpus_query!(tpcds, 1, q68, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q71, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q66, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q76, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q16, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 12, by expected calls. q90 runs at the two tp1 modes and not the three tp4 ones:
// #180's shuffled `count(*)` merging to nullable against a non-nullable declaration, the second
// query to reach it after q96 and mode-scoped for the same reason — a shuffle is what puts a
// state merge under the aggregate.
// q2 is #152 at tp1-single as well, and it is the first query the mechanism rule predicted
// rather than explained: its probe side unions web_sales and catalog_sales, so the join sees
// several batches at every mode.
corpus_query!(tpcds, 1, q25, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q29, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q90, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q2, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q59, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 13, by expected calls, and both of the rollout's cpu-side tickets get a second query.
// q5 carries a rollup, so #189's UInt8 grouping-set id meets the hasher at the three tp4 modes and
// the two tp1 ones are clean — mode-scoped exactly as rollup-over-join was. q54's nested-loop join
// projects, so #190 drops it and the node declares three columns against DataFusion's four.
// On the device, q38 is the third cell in the rollout where the device RAN and the golden caught
// it rather than the run refusing — #185 again, in_rows 11788 against the cpu's 12446, which is
// the merge counting its own output.
corpus_query!(tpcds, 1, q78, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q5, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q54, none, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q95, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q38, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 14, by expected calls, and the batch-count rule LOST to the query written to test
// it. q11's eleven joins all copy their build side and four take 336 or 88 probe batches at
// tp1-single, where the rule says #152 must fire; the device reached the unload instead, and is
// off there on #220's join batching. What separates it from q71 and q2, which do refuse there,
// is not in these goldens — see #152. q87's RightAnti-against-#175 race resolved as neither:
// it ran, and the golden caught it on #185.
corpus_query!(tpcds, 1, q87, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q11, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q83, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q61, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q10, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 15, the expensive end of the pool: four semi-joins and a rollup over an outer join.
// q77 runs at the two tp1 modes only: its Right outer's build side is a grouped aggregate over
// an Inner join whose empty scatter lane drops as it should, so the outer gets no build batch at
// all — #212, the route #175's fix does not reach. Whether its rollup would also meet #189 at
// those modes is unknown — #212 refuses first, and a disabled cell cannot show the ordering.
corpus_query!(tpcds, 1, q33, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q56, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q60, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q69, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q77, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);

// T19 batch 16, the top of the cost ranking. q80 is q77's shape — a rollup over an outer join —
// and it refuses at the three tp4 modes on #189's UInt8 grouping-set id rather than on #212's
// missing build batch, which is the ticket that took q77 there. Same two candidates, opposite winner,
// so which one a query meets is a property of the query and not of the shape.
corpus_query!(tpcds, 1, q74, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q58, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q80, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q4, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q31, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 17, the four largest plans in the corpus — 324 to 447 nodes against 267 for the
// largest already enabled, and no overlap between the two sets. Their node KINDS are ordinary:
// two of the four sit below the corpus median of nine, so this batch varies plan size and
// nothing else. q88 runs at the two tp1 modes only — #180's shuffled `count(*)` against a
// non-nullable declaration, the third query to reach it after tpcds q96 and q90.
// q64 is out at every mode on both engines and is not re-run: it takes 13 GB on a 15 GiB host,
// which is #192 — the corpus tier passes no budget and its accumulators are estimated at 35.9 GB.
// Twelve device cells: #152 at every mode for q75 and at four for q23, whose tp1-single is off
// on #220's join batching; q88 at tp1-single is #185 again — the device ran and the golden
// caught it — and its tp1-rowgroup was never run past the string class, so the row's 152 stands.
corpus_query!(tpcds, 1, q75, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q23, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q88, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q64, none, none, data_fusion_exact, golden_exact);

// T19 batch 18, the last query outside `avg` and the first four inside it. Each is RUN rather than
// declared: a feature code is a selection filter, not evidence of an outcome. The four `avg`
// queries run at every cpu mode. On the device at tp1-single, their other modes never run: q6 is
// #152's probe-batch copy; q9 fails in the CASE over its scalar subqueries, `copy_if_else`
// refusing two branch types (#63); q13 and q14 are #185's `in_rows`.
// join_int's five device cells are #152 at four modes and #185 at tp1-single, the sixth cell in
// the rollout the device ran and the golden caught.
corpus_query!(tpch, 1, join_int, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q6, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q9, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q13, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q14, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 19, the first seven of the thirteen `avg` queries, run rather than declared. Four run
// at every cpu mode; q18 and q22 carry a rollup and run at the two tp1 modes, the three tp4 ones
// being #189's UInt8 grouping-set id at the shuffle; q24's nested-loop join projects and the cpu
// backend drops it (#190), at every mode. On the device at tp1-single, the other modes never run:
// q17 answers zero rows, and the cpu's accumulating sort emits nothing where the device emits the
// zero-row batch (#205); q18 and q30 are #220's join batching; q22, q26 and q32 are #185's
// `in_rows`.
corpus_query!(tpcds, 1, q17, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q18, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q22, tp1_single | tp1_rowgroup, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q24, none, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q26, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q30, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q32, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);

// T19 batch 20, the last six, run rather than declared, and the rollout's end. All six run at
// every cpu mode; q39 takes the tolerance oracle, its `cov` differing from DataFusion's in the
// last digits at every mode — the Welford merge over batches, as shuffle-stddev found. On the
// device at tp1-single: q85 runs, its other four modes #152's build-side copy; q39 is #57's
// value-form CASE in the filter; q35 and q81 are #220's join batching; q65 and q92 are #185's
// `in_rows`.
corpus_query!(tpcds, 1, q35, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q39, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_approximate, golden_approx_std);
corpus_query!(tpcds, 1, q65, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q81, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q85, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, tp1_single, data_fusion_exact, golden_exact);
corpus_query!(tpcds, 1, q92, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);
