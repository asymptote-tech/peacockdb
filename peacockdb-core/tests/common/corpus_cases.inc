// The corpus, one line per query, included by both corpus binaries — the cpu's macro expands
// `cpu_modes`, the device's `gpu_modes`, so one declaration covers both engines:
//
//   corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle,
//                 gpu_oracle, schema_validation[(modes)])
//
// Modes are the five planning modes or'd together, `all_modes` for all five, or `none`; every
// argument is written, so a query's whole coverage reads off the line. The three oracles:
//
//   duckdb(sort|no_sort, exact|approx|rel(<tol>, "<column>", …)[, date("<column>", …)])
//     | duckdb_fingerprint | duckdb_none
//   data_fusion(sort, exact|approx) | data_fusion_subset | data_fusion_disabled
//   golden(sort|no_sort, exact|approx_std) | live_cpu
//
// The first argument says whether both answers are sorted before comparing; a query whose plan
// ends in a sort says `no_sort`. `rel` and `date` name columns as our result header prints
// them. llm-wiki/build-test.md has what each compares. The last argument is the device run's
// schema validation; a cell red on schema alone says
// `schema_validation_disabled(modes)` with its ticket on the line, naming the modes it excuses
// so the others keep the hook, or bare where it excuses every mode.
// The first twenty are the smallest plans carrying no ticket; the rest of the corpus is T19.

// --- tpch ---------------------------------------------------------------------
corpus_query!(tpch, 1, q6, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
// q1 and q17 run everywhere on both engines.
corpus_query!(tpch, 1, q1, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q14, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q19, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q12, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
// THE DEVICE COLUMN. Every tpch line runs on the device wherever it runs on the cpu, but one:
// mixed-join (#168). The joins' own blockers, #152's side copies
// and #220's batch splits, went with the join session.
corpus_query!(tpch, 1, q13, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q15, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q17, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q3, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q10, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 1, the five smallest tpch plans the rollout list carries.
// scan-limit's rows are not determined against DataFusion — an unordered LIMIT over lineitem
// — so its cpu oracle is the subset one; filter-project returns most of lineitem, which is
// over the result cap, so its device run takes a live cpu one. scan-limit's does not: #186
// made its cut one lane over the covering prefix in index order, so its five modes answer the
// same rows and the committed section serves them all, which is also what lets its
// duckdb(sort, exact) judge the device's recorded answer — see
// `an_undetermined_lines_rows_are_compared_only_against_committed_files`. Its device cells are
// on and not yet proven on master's rows-only scans: the next device run proves or reverts them.
// filter-project, aggregate-groupby, cross-join and shuffle-additive run everywhere.
corpus_query!(tpch, 1, scan_limit, all_modes, all_modes, duckdb(sort, exact), data_fusion_subset, golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, filter_project, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, aggregate_groupby, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, cross_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_additive, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 2, the five smallest joins. semi-join and anti-join return whole `orders` rows,
// which is over the result cap, so the device compares against a live cpu run.
// All five run everywhere on both engines.
corpus_query!(tpch, 1, hash_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, left_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, semi_join, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, anti_join, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 4, the small end of the tpch list emptied, and two firsts.
// shuffle-additive-avg runs everywhere on both engines.
// shuffle-stddev is the first corpus query to need the tolerance oracle, and the tell is which
// mode is exact: tp1-single matches DataFusion to the digit, and it is the mode with a single
// lane and so nothing to reassociate. The other four differ by ~3e-15 — the association order
// of the Welford merge rather than its arithmetic, since a wrong merge would not come right at
// one lane. shuffle-stddev and nested-loop-left-join run everywhere on both engines, and so
// does nested-limits, whose region scan reads no column and answers its rows as
// `__rowmarker__` (#63) — on and not yet proven on a device, as scan-limit's cells are.
// mixed-join's `none` means "cannot cross to a device" — #168's interval `ScalarValue`.
corpus_query!(tpch, 1, shuffle_additive_avg, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, shuffle_stddev, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, approx), golden(sort, approx_std), schema_validation_enabled);
// rollup-stddev puts a Welford under a grouping set, which neither benchmark has; the grouping-set
// path builds the triple through the one request builder (#280).
corpus_query!(tpch, 1, rollup_stddev, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, approx), golden(sort, approx_std), schema_validation_enabled);
// global-stddev is the keyless Welford, which no other query has: the device runs it through the
// grouped request builder on one constant key (#216).
corpus_query!(tpch, 1, global_stddev, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, approx), golden(sort, approx_std), schema_validation_enabled);
corpus_query!(tpch, 1, mixed_join, all_modes, none, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, nested_loop_left_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, nested_limits, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 5, the tpch list from here being aggregates over joins, and the cpu is where it
// showed. q22's nested-loop join projects, which the cpu applies since its joins became
// DataFusion streams. The rollup runs on the device at every mode.
corpus_query!(tpch, 1, rollup_over_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
// rollup-grouping is the one query reading the grouping id's value: GROUPING() over every key
// plans as the id cast to Int32.
corpus_query!(tpch, 1, rollup_grouping, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q4, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q16, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, q18, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q22, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 6, the rest of tpch bar q7, q8 and q2. q11 is the first corpus query whose
// nested-loop join projects, which the cpu applies; over the result cap, so DuckDB is
// compared by fingerprint and the device against a live cpu run.
corpus_query!(tpch, 1, q20, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q5, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q9, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q11, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(tpch, 1, q21, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 7, the end of tpch: three queries rather than five, because these are its deepest
// plans and the regeneration cost has moved into the queries rather than their number.
// All three run everywhere on both engines.
corpus_query!(tpch, 1, q7, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q8, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, q2, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// distinct-companions (#62): a DISTINCT aggregate beside a companion DataFusion's own rewrite
// declines. rollup-distinct puts one under a grouping set, which no other corpus query does; its
// inner stage shuffles at tp4 on the keys alone, the grouping id dropped as rollup-over-join's is,
// and it runs on the device at every mode. distinct-functions runs on the device at every mode — the
// first plan from the lowering a device has run (#262) — under shuffle-stddev's Welford
// tolerance oracle, its stddev one ULP from the cpu's in two groups.
// DataFusion 45 refuses stddev(DISTINCT) and averages a grouped decimal avg(DISTINCT) plainly,
// so DataFusion is no oracle for distinct-functions: DuckDB is, and
// a_line_asking_datafusion_nothing_compares_its_answer_with_duckdb in test_cpu_corpus.rs holds
// the line to an oracle that compares; end_to_end.rs's
// distinct_functions_answer_as_their_hand_lowered_form holds the answer too. The avg columns
// are decimals ours renders shorter than DuckDB's floats, so both lines' DuckDB compare is the
// approximate one.
corpus_query!(tpch, 1, rollup_distinct, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpch, 1, distinct_functions, all_modes, all_modes, duckdb(sort, approx), data_fusion_disabled, golden(sort, approx_std), schema_validation_enabled);

// --- tpcds --------------------------------------------------------------------
// THE DEVICE COLUMN. Every tpcds line runs on the device wherever it runs on the cpu, but
// one: q2 (#56). The joins' own blockers, #152's side copies and #220's batch splits, went
// with the join session.
corpus_query!(tpcds, 1, q41, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q42, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q3, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q43, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q52, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q55, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q96, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q15, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q37, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q82, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 3, tpcds, chosen for feature spread rather than size: q48 carries no feature code,
// q21 and q84 are top_n, q93 adds an outer join and q34 a correlated subquery; the three
// smaller candidates carry `avg` and run in batches 18 to 20.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q48, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q21, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q93, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q34, all_modes, none, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled); // device: #202, `c_preferred_cust_flag DESC NULLS FIRST` answers its nulls last
corpus_query!(tpcds, 1, q84, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 8, tpcds, chosen by expected executor calls rather than plan size: row groups scanned
// times joins, off the tp4-rowgroup plans; q32 and q26 score as low and run in batch 19.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q45, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q40, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q97, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q50, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q62, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 9, the next five by expected calls. All five run everywhere on both engines.
corpus_query!(tpcds, 1, q73, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q79, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q1, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q7, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q94, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 10, by expected calls, the `avg` queries left to batches 18 to 20.
// All five run everywhere on both engines.
corpus_query!(tpcds, 1, q19, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q8, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q99, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q91, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q46, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 11, by expected calls. All five run everywhere on both engines, q66 among them:
// #55's divisor cast never showed on the device.
corpus_query!(tpcds, 1, q68, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q71, all_modes, none, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled); // device: #202, `ext_price DESC NULLS FIRST` answers its nulls last
corpus_query!(tpcds, 1, q66, all_modes, all_modes, duckdb(no_sort, rel(1e-5, "jan_sales_per_sq_foot", "feb_sales_per_sq_foot", "mar_sales_per_sq_foot", "apr_sales_per_sq_foot", "may_sales_per_sq_foot", "jun_sales_per_sq_foot", "jul_sales_per_sq_foot", "aug_sales_per_sq_foot", "sep_sales_per_sq_foot", "oct_sales_per_sq_foot", "nov_sales_per_sq_foot", "dec_sales_per_sq_foot")), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q76, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q16, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 12, by expected calls. q2 is off on the device on #56. Its device answer matches
// the golden within 1e-11 at every mode, but some cells of its seven `round(…, 2)` columns are
// one ulp off, which widens the rendering past the result cap: the recording is a
// fingerprint, and the DuckDB check on the device's side takes rows under `approx`.
corpus_query!(tpcds, 1, q25, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q29, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q90, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q2, all_modes, none, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q59, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 13, by expected calls. q54's nested-loop join projects, which the cpu applies.
// All five run everywhere on both engines. q78's `ratio`, `round(…, 2)` over Float64, is one
// ulp from DataFusion's and DuckDB's in five cells, so q78 compares within 1e-11 relative (#60).
corpus_query!(tpcds, 1, q78, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, approx_std), schema_validation_enabled);
corpus_query!(tpcds, 1, q5, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q54, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q95, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q38, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 14, by expected calls. All five run everywhere on both engines.
corpus_query!(tpcds, 1, q87, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q11, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q83, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q61, all_modes, all_modes, duckdb(no_sort, rel(1e-5, "promotional_sales.promotions / all_sales.total * Int64(100)")), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q10, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 15, the expensive end of the pool: four semi-joins and a rollup over an outer join.
// q77's Right outer gets no build batch on some tp4 lanes — its build side is a grouped
// aggregate over an Inner join whose empty scatter lane drops — and pads its probe rows over
// an empty build table, as the session does (#212). Its rollup does not meet #189: the shuffle
// hashes the user keys and never the grouping id. All five run everywhere on both engines.
corpus_query!(tpcds, 1, q33, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q56, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q60, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q69, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q77, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 16, the top of the cost ranking. q80 is q77's shape — a rollup over an outer join —
// and it runs everywhere on both engines, as q77 does. So do the other four.
corpus_query!(tpcds, 1, q74, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q58, all_modes, all_modes, duckdb(no_sort, rel(1e-5, "ss_dev", "cs_dev", "ws_dev")), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q80, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q4, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q31, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 17, the four largest plans in the corpus — 324 to 447 nodes against 267 for the
// largest already enabled, and no overlap between the two sets. Their node KINDS are ordinary:
// two of the four sit below the corpus median of nine, so this batch varies plan size and
// nothing else.
// q64 runs at every cpu mode, under 0.9 GB and six seconds each (2026-09-28). The 13 GB it once
// took (#192) was arrow's Utf8View buffer lists multiplying along its join chain, gone with the
// strings declared Utf8 (#183). All four run on the device wherever they run on the cpu.
corpus_query!(tpcds, 1, q75, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q23, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q88, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q64, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 18, the last query outside `avg` and the first four inside it. Each is RUN rather than
// declared: a feature code is a selection filter, not evidence of an outcome. The four `avg`
// queries run everywhere on both engines, q9's scalar subqueries over a declared
// `__rowmarker__` (#63).
// join_int runs everywhere on both engines.
corpus_query!(tpch, 1, join_int, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q6, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q9, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q13, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q14, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);

// T19 batch 19, the first seven of the thirteen `avg` queries, run rather than declared. All
// seven run at every cpu mode; q18 and q22 carry a rollup, which held them off the three tp4
// modes on #189's UInt8 grouping-set id at the shuffle until it stopped being hashed; q24's
// nested-loop join projects, which the cpu applies.
// q17 and the rest run everywhere.
corpus_query!(tpcds, 1, q17, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q18, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q22, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q24, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q26, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q30, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q32, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// T19 batch 20, the last six, run rather than declared, and the rollout's end. All six run
// everywhere on both engines; q39 takes the tolerance oracle, its `cov` differing from
// DataFusion's in the last digits at every mode — the Welford merge over batches, as
// shuffle-stddev found.
corpus_query!(tpcds, 1, q35, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q39, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, approx), golden(no_sort, approx_std), schema_validation_enabled);
corpus_query!(tpcds, 1, q65, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q81, all_modes, all_modes, duckdb(no_sort, exact), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q85, all_modes, all_modes, duckdb(no_sort, approx), data_fusion(sort, exact), golden(no_sort, exact), schema_validation_enabled);
corpus_query!(tpcds, 1, q92, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// distinct-companions (#62): q28 is six global `avg, count, count(DISTINCT ss_list_price)`
// blocks cross-joined, and was refused at every mode until the lowering. It runs everywhere on
// both engines.
corpus_query!(tpcds, 1, q28, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// --- pbench ------------------------------------------------------------------
// What tpch and tpcds lack: one query per demonstrable ticket of chain J, planned with the
// small-table rule off, so `fact` splits across four lanes at tp4 and the one-row-group tables
// leave three empty. llm-wiki/tasks/pbench.md.
//
// BOTH COLUMNS. Every line runs on both engines at every mode it plans at, but these. Refused
// at plan time: `in-is-null` (#250), `not-in-uncorrelated` at tp4 (#267). On the device alone,
// `like-column-pattern` meets #246 and `timestamp-s-key-group` #264. Three are absent, on #255.
corpus_query!(pbench, 1, anti_null_preserved_condition, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, bool_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, cross_empty_build, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, cross_projection, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, decimal15_key_join, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, decimal38_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
// The six empty-input aggregates. Four filter `tiny` with a predicate no row-group statistic
// prunes, so its one row group reaches the init as a zero-row batch: at tp1 on the one lane, at
// the tp4 modes on one lane of four, the other three getting no batch. `empty-pruned-count` and
// `empty-table-count` scan nothing: `t_v < 0` prunes `tiny`'s one row group, and `empty` has
// none. `empty-grouped-count` answers no rows under its columns, so DuckDB compares it exactly.
corpus_query!(pbench, 1, empty_all_null_aggregates, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_count_aggregates, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_dispersion_aggregates, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_grouped_count, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_pruned_count, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_side_left_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, empty_table_count, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, exists_null_keys, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, exists_or_mark, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, finish_without_probe, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, full_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, full_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, in_is_null, none, none, duckdb_none, data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, indf_full_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, inner_join_hot_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, int8_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, left_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, left_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, like_column_pattern, all_modes, none, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, mark_cross_residual, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, nl_full, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_inner, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_anti, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_decimal, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_left_semi, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, nl_mark, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, nl_projection, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_anti, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, nl_right_semi, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_exists_null_keys, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_in_correlated, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_in_uncorrelated, tp1_single | tp1_rowgroup, tp1_single | tp1_rowgroup, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_in_under_or, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_not_in, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, not_or_not_in, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, outer_on_true_empty, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, probe_exists_residual, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, probe_not_exists_residual, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, right_join_null_keys, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, right_join_residual, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, rollup_small_keys, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, scalar_subquery_cross, all_modes, all_modes, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_anti, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_full, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_build_right, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_left, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, sparse_probe_semi, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ms_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_ns_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, timestamp_s_key_group, all_modes, none, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled); // device: #264, a cast group key
corpus_query!(pbench, 1, timestamp_us_key_group, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
corpus_query!(pbench, 1, ts_key_join, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_group, all_modes, all_modes, duckdb_fingerprint, data_fusion(sort, exact), live_cpu, schema_validation_enabled);
corpus_query!(pbench, 1, uint_key_join, all_modes, all_modes, duckdb(sort, exact), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);

// #243 — the cpu keys -0.0 and NaN by their BITS, so the three float rows land with their SQL,
// their plan goldens and their DuckDB answers, and their lines commented out for the task that
// fixes it to enable. MEASURED: the cpu answers float64-key-group in 54 groups against DuckDB's
// 52. The oracle is duckdb(sort, approx) and not exact because arrow renders a NaN `NaN` and DuckDB
// renders it `nan`. Every cpu AND gpu cell is `na` rather than `disabled`: a commented line
// declares nothing, and a `disabled` cpu cell wants a `skipped:` section in the mode golden
// that no declaration writes. llm-wiki/tasks/pbench-detail.md carries the rest.
// corpus_query!(pbench, 1, float32_key_group, all_modes, none, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
// corpus_query!(pbench, 1, float64_key_group, all_modes, none, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
// corpus_query!(pbench, 1, float64_key_join, all_modes, none, duckdb(sort, approx), data_fusion(sort, exact), golden(sort, exact), schema_validation_enabled);
