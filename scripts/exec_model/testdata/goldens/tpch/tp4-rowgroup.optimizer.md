# tpch tp4-rowgroup: join orders and dynamic filters

What the optimizer changed in each query's plan. Every figure behind it is in
[tp4-rowgroup.optimizer.txt](tp4-rowgroup.optimizer.txt), from the same run.

A multi-join (MJ) is a cluster of joins whose order may change. One that changed has a table: a
column for the plan's order, then one for each call that changed it — DPhyp before the run, a
replan — in the order they ran; the calls that left it unchanged are named under the table. A
column headed `inside p2, replan 1` orders only the part of the cluster inside the build `p2`,
whose own run replanned before the main run; the main plan reads `p2` whole. A cluster no call
changed is a line.

A join tree writes the build side in brackets and the probe side bare, and each join's kind as SQL
names it, the side an outer, semi or anti join keeps first. `(f)` marks a filtered relation,
`agg(X)` an aggregate over X, `t#2` the plan's second read of table t, and `MJ2` alone a cluster
read whole, as one relation of another. `p0` is a build a dynamic filter ran ahead, `m0` one a
replan kept; the plan reads both from memory.

C_out is the bytes a cluster's joins are estimated to emit, under the estimates of the call that
made its column: compare within a column, not across columns.

## q1

Nothing fired.

## q2

### q2 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[region#1 (f)] join ([nation#1] join ([supplier#1] join ([p0] join partsupp#1)))] join agg(MJ2)` | `[[[p0] join agg(MJ2)] join partsupp#1] join ([[region#1 (f)] join nation#1] join supplier#1)` |
| changed by | | reordered by DPhyp |
| C_out | 1,615,081 B | 593,363 B (plan's order 1,615,081 B) |

p0 = `part (f)`, 747 rows.

### q2 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[region#2 (f)] join ([nation#2] join ([supplier#2] join partsupp#2))` | `[[[region#2 (f)] join nation#2] join supplier#2] join partsupp#2` |
| changed by | | reordered by DPhyp |
| C_out | 49,280,000 B | 3,896,271 B (plan's order 49,280,000 B) |

### q2 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | partsupp#1 | ps_partkey | p0.p_partkey | 249 to 199723 | 7 → 7 (0-6) |
| after join order | partsupp#1 | ps_partkey | p1.p_partkey | 249 to 199478 | 7 → 7 (0-6) |

p0 = `part (f)`, 747 rows. p1 = `[p0] join agg(MJ2)`, 460 rows, holding MJ1, MJ2.

## q3

MJ1 kept the plan's order: `[[customer (f)] join orders (f)] join lineitem (f)`.

## q4

MJ1 kept the plan's order: `[orders (f)] semi join lineitem (f)`.

## q5

### q5 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[region (f)] join ([nation] join ([supplier] join ([[customer] join orders (f)] join lineitem)))` | `[[[region (f)] join nation] join supplier] join ([[customer] join orders (f)] join lineitem)` |
| changed by | | reordered by DPhyp |
| C_out | 46,678,751 B | 43,668,130 B (plan's order 46,678,751 B) |

## q6

Nothing fired.

## q7

### q7 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation#1 (f)] join ([nation#2 (f)] join ([customer] join ([[supplier] join lineitem (f)] join orders)))` | `[[[nation#1 (f)] join customer] join orders] join ([[nation#2 (f)] join supplier] join lineitem (f))` |
| changed by | | reordered by DPhyp |
| C_out | 266,423,943 B | 10,831,578 B (plan's order 266,423,943 B) |

## q8

### q8 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[region (f)] join ([nation#1] join ([nation#2] join ([customer] join ([orders (f)] join ([supplier] join ([part (f)] join lineitem))))))` | `[nation#1] join ([[[[region (f)] join nation#2] join customer] join ([[part (f)] join lineitem] join orders (f))] join supplier)` |
| changed by | | reordered by DPhyp |
| C_out | 6,144,027 B | 3,147,421 B (plan's order 6,144,027 B) |

## q9

### q9 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation] join ([orders] join ([partsupp] join ([supplier] join ([part (f)] join lineitem))))` | `[[nation] join supplier] join ([[[part (f)] join lineitem] join orders] join partsupp)` |
| changed by | | reordered by DPhyp |
| C_out | 454,838,086 B | 357,961,733 B (plan's order 454,838,086 B) |

Calls that left the order unchanged: inside p0, replan 1 (MJ1's missed build came out 10664 rows, est 40000.0).

p0 = `[[part (f)] join lineitem] join orders`, 319,404 rows.

### q9 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | partsupp | ps_partkey | p0.l_partkey | 3 to 199962 | 7 → 7 (0-6) |

p0 = `[[part (f)] join lineitem] join orders`, 319,404 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's join on p_partkey = l_partkey came out 10664 rows, est 40000.0 (q-error 3.75); m0 = `part (f)`, 10,664 rows.

## q10

### q10 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation] join ([[customer] join orders (f)] join lineitem (f))` | `[nation] join ([[orders (f)] join lineitem (f)] join customer)` |
| changed by | | reordered by DPhyp |
| C_out | 27,072,151 B | 18,123,816 B (plan's order 27,072,151 B) |

### q10 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | customer | c_custkey | p0.o_custkey | 4 to 149995 | 2 → 2 (0-1) |

p0 = `[orders (f)] join lineitem (f)`, 114,705 rows, holding MJ1.

## q11

### q11 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation#1 (f)] join ([supplier#1] join partsupp#1)` | `[[nation#1 (f)] join supplier#1] join partsupp#1` |
| changed by | | reordered by DPhyp |
| C_out | 23,476,000 B | 779,250 B (plan's order 23,476,000 B) |

### q11 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation#2 (f)] join ([supplier#2] join partsupp#2)` | `[[nation#2 (f)] join supplier#2] join partsupp#2` |
| changed by | | reordered by DPhyp |
| C_out | 30,236,000 B | 1,039,250 B (plan's order 30,236,000 B) |

## q12

MJ1 kept the plan's order: `[lineitem (f)] join orders`.

## q13

MJ1 kept the plan's order: `[customer] left join orders (f)`.

## q14

### q14 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[part] join lineitem (f)` | `[lineitem (f)] join part` |
| changed by | | flip: `lineitem (f)` now builds |
| C_out | 4,284,684 B | 4,284,684 B (plan's order 4,284,684 B) |

### q14 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | part | p_partkey | p0.l_partkey | 1 to 199998 | 2 → 2 (0-1) |

p0 = `lineitem (f)`, 75,983 rows.

## q15

### q15 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[supplier] join agg(lineitem#1 (f))] join agg(lineitem#2 (f))` | `[[agg(lineitem#2 (f))] join agg(lineitem#1 (f))] join supplier` |
| changed by | | reordered by DPhyp |
| C_out | 946,115 B | 119 B (plan's order 946,115 B) |

## q16

MJ1 kept the plan's order: `([p0] join partsupp) anti join [supplier (f)]`.

### q16 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | partsupp | ps_partkey | p0.p_partkey | 4 to 199998 | 7 → 7 (0-6) |

p0 = `part (f)`, 29,581 rows.

## q17

### q17 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[part (f)] join lineitem#1] join agg(lineitem#2)` | `[[part (f)] join agg(lineitem#2)] join lineitem#1` |
| changed by | | reordered by DPhyp |
| C_out | 261,653 B | 24,204 B (plan's order 261,653 B) |

## q18

### q18 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer] join orders] join lineitem#2) semi join [agg(lineitem#1) (f)]` | `[[orders semi join [agg(lineitem#1) (f)]] join customer] join lineitem#2` |
| changed by | | reordered by DPhyp |
| C_out | 626,246,486 B | 118,255,664 B (plan's order 626,246,486 B) |

Calls that left the order unchanged: inside p0, replan 1 (MJ1's missed build came out 57 rows, est 300000.0).

p0 = `orders semi join [agg(lineitem#1) (f)]`, 57 rows.

### q18 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | customer | c_custkey | p0.o_custkey | 3566 to 149842 | 2 → 2 (0-1) |

p0 = `orders semi join [agg(lineitem#1) (f)]`, 57 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's semi join on l_orderkey = o_orderkey came out 57 rows, est 300000.0 (q-error 5263.16); m0 = `agg(lineitem#1) (f)`, 57 rows.

## q19

MJ1 kept the plan's order: `[part (f)] join lineitem (f)`.

## q20

MJ1 kept the plan's order: `[[nation (f)] join supplier] semi join ([partsupp semi join [p0]] join agg(lineitem (f)))`.

### q20 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | partsupp | ps_partkey | p0.p_partkey | 5 to 199879 | 7 → 7 (0-6) |

p0 = `part (f)`, 2,127 rows.

## q21

### q21 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[[[nation (f)] join ([orders (f)] join ([supplier] join lineitem#1 (f)))] semi join lineitem#2] anti join lineitem#3 (f)` | `[[[[[nation (f)] join supplier] join lineitem#1 (f)] join orders (f)] semi join lineitem#2] anti join lineitem#3 (f)` | `[[[orders (f)] join m0] semi join lineitem#2] anti join lineitem#3 (f)` | `[[[m0] semi join lineitem#2] join m1] anti join lineitem#3 (f)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on l_orderkey = o_orderkey came out 156739 rows, est 48009.7 (q-error 3.26); flip: `orders (f)` now builds | the build of MJ1's join on o_orderkey = l_orderkey came out 729413 rows, est 286720.0 (q-error 2.54); reordered by DPhyp |
| C_out | 61,219,190 B | 2,312,881 B (plan's order 61,219,190 B) | 1,496,582 B (previous order 1,496,582 B) | 2,085,384 B (previous order 3,807,290 B) |

Calls that left the order unchanged: replan 3 (MJ1's missed build came out 151237 rows, est 31347.8).

m0 = `[[nation (f)] join supplier] join lineitem#1 (f)`, 156,739 rows. m1 = `orders (f)`, 729,413 rows.

## q22

MJ1 kept the plan's order: `[customer#2 (f)] anti join orders`.

## aggregate-groupby

Nothing fired.

## anti-join

MJ1 kept the plan's order: `orders anti join [customer (f)]`.

## anti-join-reorder

### anti-join-reorder MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation (f)] join ([customer] anti join orders)` | `[[nation (f)] join customer] anti join orders` |
| changed by | | reordered by DPhyp |
| C_out | 1,779,392 B | 242,004 B (plan's order 1,779,392 B) |

## cross-join

Nothing fired.

## filter-project

Nothing fired.

## hash-join

MJ1 kept the plan's order: `[orders] join lineitem`.

## join-int

MJ1 kept the plan's order: `[customer] join orders`.

## left-join

MJ1 kept the plan's order: `[orders] left join lineitem`.

## left-join-fixed

MJ1 kept the plan's order: `[lineitem (f)] join ([customer] left join orders)`.

## left-join-reorder

### left-join-reorder MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation (f)] join ([customer] left join orders)` | `[[nation (f)] join customer] left join orders` |
| changed by | | reordered by DPhyp |
| C_out | 55,156,904 B | 2,057,016 B (plan's order 55,156,904 B) |

## mixed-join

MJ1 kept the plan's order: `[orders] join lineitem`.

## nested-limits

Nothing fired.

## nested-loop-join

Nothing fired.

## nested-loop-left-join

Nothing fired.

## rollup-over-join

MJ1 kept the plan's order: `[nation] join customer`.

## scan-limit

Nothing fired.

## semi-join

MJ1 kept the plan's order: `orders semi join [customer (f)]`.

## semi-join-reorder

### semi-join-reorder MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[nation (f)] join ([customer] semi join orders)` | `[[nation (f)] join customer] semi join orders` |
| changed by | | reordered by DPhyp |
| C_out | 3,558,358 B | 302,496 B (plan's order 3,558,358 B) |

## shuffle-additive

Nothing fired.

## shuffle-additive-avg

Nothing fired.

## shuffle-stddev

Nothing fired.

