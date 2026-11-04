# tpcds tp4-single: join orders and dynamic filters

What the optimizer changed in each query's plan. Every figure behind it is in
[tp4-single.optimizer.txt](tp4-single.optimizer.txt), from the same run.

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

MJ2 kept the plan's order: `[p0] join store_returns#1`.

MJ3 kept the plan's order: `[p1] join store_returns#2`.

### q1 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[store (f)] join agg(MJ2)] join customer] join agg(MJ3)` | `[[store (f)] join ([agg(MJ3)] join agg(MJ2))] join customer` |
| changed by | | reordered by DPhyp |
| C_out | 3,627,340 B | 399,670 B (plan's order 3,627,340 B) |

### q1 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_returns#1 | sr_returned_date_sk | p0.d_date_sk | 2451545 to 2451910 | 3 → 2 (0-1) |
| plan as written | store_returns#2 | sr_returned_date_sk | p1.d_date_sk | 2451545 to 2451910 | 3 → 2 (0-1) |

p0 = `date_dim#1 (f)`, 366 rows. p1 = `date_dim#2 (f)`, 366 rows.

## q2

MJ1 kept the plan's order: `[[date_dim#1 (f)] join agg(MJ2)] join MJ3`.

MJ2 kept the plan's order: `[date_dim#2] join union(web_sales#1, catalog_sales#1)`.

MJ3 kept the plan's order: `[date_dim#3 (f)] join agg(MJ4)`.

MJ4 kept the plan's order: `[date_dim#4] join union(web_sales#2, catalog_sales#2)`.

## q3

### q3 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item (f)] join ([p0] join store_sales)` | `[[item (f)] join store_sales] join p0` |
| changed by | | reordered by DPhyp |
| C_out | 93,405,358 B | 304,557 B (plan's order 93,405,358 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 39 rows, est 18.1).

p0 = `date_dim (f)`, 6,000 rows.

### q3 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2415325 to 2488038 | 24 → 24 (0-23) |

p0 = `date_dim (f)`, 6,000 rows.

## q4

### q4 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 | replan 3 | replan 6 | replan 7 |
|---|---|---|---|---|---|---|---|
| join tree | `[[[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ6) (f)] join agg(MJ7)` | `[[[[[agg(MJ6) (f)] join agg(MJ2) (f)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ7)] join agg(MJ3)` | `[[[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)] join agg(MJ7)] join agg(MJ6) (f)` | `[[[[[agg(MJ2) (f)] join agg(MJ5)] join agg(MJ4) (f)] join agg(MJ3)] join agg(MJ7)] join agg(MJ6) (f)` | `[[[[[agg(MJ6) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ7)] join agg(MJ4) (f)] join agg(MJ3)` | `[[[[m10] join agg(MJ7)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ3)` | `[[[[m10] join agg(MJ4) (f)] join agg(MJ7)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_customer_sk = c_customer_sk came out 147010 rows, est 49119.4 (q-error 2.99); reordered by DPhyp | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 285994 rows, est 97635.1 (q-error 2.93); reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); reordered by DPhyp | the build of MJ5's join on cs_bill_customer_sk = c_customer_sk came out 284628 rows, est 64538.9 (q-error 4.41); reordered by DPhyp | the build of MJ7's join on ws_bill_customer_sk = c_customer_sk came out 142487 rows, est 45487.5 (q-error 3.13); reordered by DPhyp |
| C_out | 6,378,822 B | 2,231,138 B (plan's order 6,378,822 B) | 3,438,221 B (previous order 4,793,896 B) | 4,582,101 B (previous order 4,838,465 B) | 10,209,248 B (previous order 15,780,902 B) | 979,839 B (previous order 2,128,765 B) | 1,333,005 B (previous order 1,396,143 B) |

Calls that left the order unchanged: replan 4 (MJ1's missed build came out 11349 rows, est 29402.0); replan 5 (MJ1's missed build came out 4340 rows, est 17399.3); replan 8 (MJ1's missed build came out 1162 rows, est 5262.2); replan 9 (MJ1's missed build came out 125 rows, est 1655.7); replan 10 (MJ1's missed build came out 22 rows, est 71.2); replan 11.

m9 = `agg(MJ6) (f)`, 11,349 rows. m10 = `[m9] join agg(MJ2) (f)`, 4,340 rows.

### q4 MJ2

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[[p0] join store_sales#1] join customer#1` | `[customer#1] join m8` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); flip: `customer#1` now builds |
| C_out | 132,288,333 B | 32,828,989 B (plan's order 132,288,333 B) | 87,965,613 B (previous order 87,965,613 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 4.

m8 = `[p0] join store_sales#1`, 533,427 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q4 MJ3

| | plan | DPhyp before the run | replan 11 |
|---|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[[p1] join store_sales#2] join customer#2` | `[customer#2] join m16` |
| changed by | | reordered by DPhyp | the build of MJ3's join on ss_customer_sk = c_customer_sk came out 536691 rows, est 118806.9 (q-error 4.52); flip: `customer#2` now builds |
| C_out | 156,602,520 B | 38,862,856 B (plan's order 156,602,520 B) | 88,503,868 B (previous order 88,503,868 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6; replan 7; replan 8; replan 9; replan 10.

m16 = `[p1] join store_sales#2`, 536,691 rows. p1 = `date_dim#2 (f)`, 365 rows.

### q4 MJ4

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[p2] join ([customer#3] join catalog_sales#1)` | `[[p2] join catalog_sales#1] join customer#3` | `[customer#3] join m7` |
| changed by | | reordered by DPhyp | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 285994 rows, est 97635.1 (q-error 2.93); flip: `customer#3` now builds |
| C_out | 97,194,876 B | 24,043,297 B (plan's order 97,194,876 B) | 47,162,288 B (previous order 47,162,288 B) |

Calls that left the order unchanged: replan 1; replan 3; replan 4; replan 5; replan 6; replan 7.

m7 = `[p2] join catalog_sales#1`, 285,994 rows. p2 = `date_dim#3 (f)`, 365 rows.

### q4 MJ5

| | plan | DPhyp before the run | replan 6 |
|---|---|---|---|
| join tree | `[p3] join ([customer#4] join catalog_sales#2)` | `[[p3] join catalog_sales#2] join customer#4` | `[customer#4] join m11` |
| changed by | | reordered by DPhyp | the build of MJ5's join on cs_bill_customer_sk = c_customer_sk came out 284628 rows, est 64538.9 (q-error 4.41); flip: `customer#4` now builds |
| C_out | 66,368,249 B | 16,417,651 B (plan's order 66,368,249 B) | 46,937,025 B (previous order 46,937,025 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 7; replan 8; replan 9.

m11 = `[p3] join catalog_sales#2`, 284,628 rows. p3 = `date_dim#4 (f)`, 365 rows.

### q4 MJ6

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p4] join ([customer#5] join web_sales#1)` | `[[p4] join web_sales#1] join customer#5` | `[customer#5] join m6` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_customer_sk = c_customer_sk came out 147010 rows, est 49119.4 (q-error 2.99); flip: `customer#5` now builds |
| C_out | 48,628,375 B | 12,067,734 B (plan's order 48,628,375 B) | 24,242,914 B (previous order 24,242,914 B) |

Calls that left the order unchanged: replan 2; replan 3.

m6 = `[p4] join web_sales#1`, 147,010 rows. p4 = `date_dim#5 (f)`, 365 rows.

### q4 MJ7

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[p5] join ([customer#6] join web_sales#2)` | `[[p5] join web_sales#2] join customer#6` | `[customer#6] join m12` |
| changed by | | reordered by DPhyp | the build of MJ7's join on ws_bill_customer_sk = c_customer_sk came out 142487 rows, est 45487.5 (q-error 3.13); flip: `customer#6` now builds |
| C_out | 45,087,304 B | 11,188,973 B (plan's order 45,087,304 B) | 23,497,042 B (previous order 23,497,042 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6; replan 8.

m12 = `[p5] join web_sales#2`, 142,487 rows. p5 = `date_dim#6 (f)`, 365 rows.

### q4 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2452276 to 2452640 | 24 → 7 (17-23) |
| plan as written | catalog_sales#1 | cs_sold_date_sk | p2.d_date_sk | 2451911 to 2452275 | 12 → 4 (6-9) |
| plan as written | catalog_sales#2 | cs_sold_date_sk | p3.d_date_sk | 2452276 to 2452640 | 12 → 3 (9-11) |
| plan as written | web_sales#1 | ws_sold_date_sk | p4.d_date_sk | 2451911 to 2452275 | 6 → 2 (3-4) |
| plan as written | web_sales#2 | ws_sold_date_sk | p5.d_date_sk | 2452276 to 2452640 | 6 → 2 (4-5) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows. p2 = `date_dim#3 (f)`, 365 rows. p3 = `date_dim#4 (f)`, 365 rows. p4 = `date_dim#5 (f)`, 365 rows. p5 = `date_dim#6 (f)`, 365 rows.

## q5

MJ1 kept the plan's order: `[store] join ([p0] join union(store_sales, store_returns))`.

MJ3 kept the plan's order: `[web_site] join ([p2] join union(web_sales#1, MJ4))`.

MJ4 kept the plan's order: `[web_returns] left join web_sales#2`.

A replan was refused, work outside its builds having started: the build of MJ2's join on page_sk = cp_catalog_page_sk came out 15953 rows, est 38.0 (q-error 420.31).

### q5 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[catalog_page] join ([p1] join union(catalog_sales, catalog_returns))` | `[[p1] join union(catalog_sales, catalog_returns)] join catalog_page` |
| changed by | | flip: `[p1] join union(catalog_sales, catalog_returns)` now builds |
| C_out | 140,620 B | 140,620 B (plan's order 140,620 B) |

p1 = `date_dim#2 (f)`, 15 rows.

### q5 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 15 values, 2451780 to 2451794 | 24 → 3 (10-11,23) |
| plan as written | store_returns | sr_returned_date_sk | p0.d_date_sk | 15 values, 2451780 to 2451794 | 3 → 1 (1) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 15 values, 2451780 to 2451794 | 12 → 1 (5) |
| plan as written | catalog_returns | cr_returned_date_sk | p1.d_date_sk | 15 values, 2451780 to 2451794 | 2 → 1 (0) |
| plan as written | web_sales#1 | ws_sold_date_sk | p2.d_date_sk | 15 values, 2451780 to 2451794 | 6 → 1 (2) |

p0 = `date_dim#1 (f)`, 15 rows. p1 = `date_dim#2 (f)`, 15 rows. p2 = `date_dim#3 (f)`, 15 rows.

## q6

### q6 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[agg(date_dim#1 (f))] join ([item#1] join ([date_dim#2] join ([[customer_address] join customer] join store_sales)))] left join agg(item#2)` | `[[item#1] left join agg(item#2)] join ([[customer_address] join customer] join ([[agg(date_dim#1 (f))] join date_dim#2] join store_sales))` |
| changed by | | reordered by DPhyp |
| C_out | 336,394,359 B | 89,772,204 B (plan's order 336,394,359 B) |

Calls that left the order unchanged: inside p0, replan 1 (MJ1's missed build came out 1 rows, est 30.1).

p0 = `[agg(date_dim#1 (f))] join date_dim#2`, 31 rows.

### q6 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | store_sales | ss_sold_date_sk | p0.d_date_sk | 31 values, 2451911 to 2451941 | 24 → 2 (13,23) |
| after join order | store_sales | ss_item_sk | p1.i_item_sk | 1 to 18000 | 2 → 2 (13,23) |

p0 = `[agg(date_dim#1 (f))] join date_dim#2`, 31 rows, holding MJ1. p1 = `[item#1] left join agg(item#2)`, 18,000 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's join on d_month_seq = d_month_seq came out 1 rows, est 30.1 (q-error 30.10); m0 = `agg(date_dim#1 (f))`, 1 rows.

## q7

### q7 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion (f)] join ([item] join ([p0] join ([customer_demographics (f)] join store_sales)))` | `[[promotion (f)] join ([p0] join ([customer_demographics (f)] join store_sales))] join item` |
| changed by | | reordered by DPhyp |
| C_out | 1,444,947 B | 1,399,307 B (plan's order 1,444,947 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 7761 rows, est 1881.1).

p0 = `date_dim (f)`, 366 rows.

### q7 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim (f)`, 366 rows.

## q8

MJ1 kept the plan's order: `[MJ2] join MJ4`.

MJ2 kept the plan's order: `[agg(customer_address#1 (f))] semi join agg(MJ3) (f)`.

MJ3 kept the plan's order: `[customer (f)] join customer_address#2`.

MJ4 kept the plan's order: `[store] join ([p0] join store_sales)`.

### q8 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2450906 to 2450996 | 24 → 3 (0-1,23) |

p0 = `date_dim (f)`, 91 rows.

## q9

Nothing fired.

## q10

MJ2 kept the plan's order: `[p1] join web_sales`.

MJ3 kept the plan's order: `[p2] join catalog_sales`.

### q10 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[customer_address (f)] join customer] join customer_demographics] semi join ([p0] join store_sales)` | `[[[customer_address (f)] join customer] semi join ([p0] join store_sales)] join customer_demographics` |
| changed by | | reordered by DPhyp |
| C_out | 186,982 B | 168,974 B (plan's order 186,982 B) |

Calls that left the order unchanged: inside p3, replan 1 (MJ1's missed build came out 459 rows, est 131.5).

p0 = `date_dim#1 (f)`, 120 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 92 rows.

### q10 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2452276 to 2452395 | 24 → 3 (17-18,23) |
| plan as written | web_sales | ws_sold_date_sk | p1.d_date_sk | 2452276 to 2452395 | 6 → 1 (4) |
| plan as written | catalog_sales | cs_sold_date_sk | p2.d_date_sk | 2452276 to 2452395 | 12 → 1 (9) |
| after join order | customer_demographics | cd_demo_sk | p3.c_current_cdemo_sk | 21355 to 1909112 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 120 rows. p1 = `date_dim#2 (f)`, 120 rows. p2 = `date_dim#3 (f)`, 120 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 92 rows, holding MJ1.

Probe p3, replan 1: the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 459 rows, est 131.5 (q-error 3.49); m3 = `customer_address (f)`, 459 rows.

## q11

### q11 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` | `[[[agg(MJ2) (f)] join agg(MJ5)] join agg(MJ4) (f)] join agg(MJ3)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ4's join on ws_bill_customer_sk = c_customer_sk came out 147010 rows, est 49119.4 (q-error 2.99); reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); reordered by DPhyp |
| C_out | 4,118,567 B | 1,313,714 B (plan's order 4,118,567 B) | 3,124,352 B (previous order 3,186,264 B) | 5,930,761 B (previous order 7,222,376 B) |

Calls that left the order unchanged: replan 3 (MJ1's missed build came out 11552 rows, est 29402.0); replan 4 (MJ1's missed build came out 4414 rows, est 17710.6); replan 5; replan 6 (MJ1's missed build came out 485 rows, est 6289.4); replan 7.

### q11 MJ2

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[[p0] join store_sales#1] join customer#1` | `[customer#1] join m5` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); flip: `customer#1` now builds |
| C_out | 106,417,326 B | 24,209,354 B (plan's order 106,417,326 B) | 70,762,593 B (previous order 70,762,593 B) |

Calls that left the order unchanged: replan 1; replan 3.

m5 = `[p0] join store_sales#1`, 533,427 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q11 MJ3

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[[p1] join store_sales#2] join customer#2` | `[customer#2] join m10` |
| changed by | | reordered by DPhyp | the build of MJ3's join on ss_customer_sk = c_customer_sk came out 536691 rows, est 118806.9 (q-error 4.52); flip: `customer#2` now builds |
| C_out | 125,976,503 B | 28,658,959 B (plan's order 125,976,503 B) | 71,195,584 B (previous order 71,195,584 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

m10 = `[p1] join store_sales#2`, 536,691 rows. p1 = `date_dim#2 (f)`, 365 rows.

### q11 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p2] join ([customer#3] join web_sales#1)` | `[[p2] join web_sales#1] join customer#3` |
| changed by | | reordered by DPhyp |
| C_out | 39,118,352 B | 8,899,209 B (plan's order 39,118,352 B) |

Calls that left the order unchanged: replan 1 (MJ4's missed build came out 147010 rows, est 49119.4); replan 2.

p2 = `date_dim#3 (f)`, 365 rows.

### q11 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p3] join ([customer#4] join web_sales#2)` | `[[p3] join web_sales#2] join customer#4` |
| changed by | | reordered by DPhyp |
| C_out | 36,269,793 B | 8,251,178 B (plan's order 36,269,793 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5 (MJ5's missed build came out 142487 rows, est 45487.5).

p3 = `date_dim#4 (f)`, 365 rows.

### q11 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2452276 to 2452640 | 24 → 7 (17-23) |
| plan as written | web_sales#1 | ws_sold_date_sk | p2.d_date_sk | 2451911 to 2452275 | 6 → 2 (3-4) |
| plan as written | web_sales#2 | ws_sold_date_sk | p3.d_date_sk | 2452276 to 2452640 | 6 → 2 (4-5) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows. p2 = `date_dim#3 (f)`, 365 rows. p3 = `date_dim#4 (f)`, 365 rows.

## q12

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q13

### q13 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[p0] join ([customer_address (f)] join ([household_demographics (f)] join ([customer_demographics (f)] join ([store] join store_sales (f)))))` | `[[[[[store_sales (f)] join customer_demographics (f)] join customer_address (f)] join household_demographics (f)] join p0] join store` | `[[[[customer_address (f)] join m2] join household_demographics (f)] join p0] join store` | `[store] join ([[m3] join household_demographics (f)] join p0)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 1041 rows, est 64.9 (q-error 16.04); flip: `customer_address (f)` now builds | the build of MJ1's join on ss_hdemo_sk = hd_demo_sk came out 181 rows, est 1.7 (q-error 105.67); flip: `store` now builds |
| C_out | 853,624 B | 16,807 B (plan's order 853,624 B) | 2,756 B (previous order 2,756 B) | 539 B (previous order 539 B) |

Calls that left the order unchanged: replan 3 (MJ1's missed build came out 7 rows, est 1.6).

m2 = `[p1] join customer_demographics (f)`, 1,041 rows. m3 = `[customer_address (f)] join m2`, 181 rows. p0 = `date_dim (f)`, 365 rows. p1 = `store_sales (f)`, 37,096 rows.

### q13 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| after join order | customer_demographics | cd_demo_sk | p1.ss_cdemo_sk | 15 to 1920767 | 16 → 16 (0-15) |

p0 = `date_dim (f)`, 365 rows. p1 = `store_sales (f)`, 37,096 rows.

## q14

MJ1 kept the plan's order: `[p0] join store_sales#1`.

MJ2 kept the plan's order: `[p1] join catalog_sales#1`.

MJ3 kept the plan's order: `[p2] join web_sales#1`.

MJ7 kept the plan's order: `[p7] join store_sales#4`.

MJ8 kept the plan's order: `[p8] join catalog_sales#3`.

MJ9 kept the plan's order: `[p9] join web_sales#3`.

MJ13 kept the plan's order: `[p14] join store_sales#6`.

MJ14 kept the plan's order: `[p15] join catalog_sales#6`.

MJ15 kept the plan's order: `[p16] join web_sales#5`.

### q14 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[p3] join ([item#1] join store_sales#2)] semi join ([item#2] join (agg(MJ5) semi join [[p4] join ([item#3] join web_sales#2)]))` | `[[[p3] join store_sales#2] semi join ([agg(MJ5) semi join [[[p4] join web_sales#2] join item#3]] join item#2)] join item#1` |
| changed by | | reordered by DPhyp; flip: `agg(MJ5) semi join [[[p4] join web_sales#2] join item#3]` now builds |
| C_out | 43,418,281 B | 10,422,733 B (plan's order 43,418,281 B) |

Calls that left the order unchanged: replan 1 (MJ4's missed build came out 86880 rows, est 4037.6); replan 2; replan 3.

p3 = `date_dim#4 (f)`, 30 rows. p4 = `date_dim#5 (f)`, 1,096 rows.

### q14 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ6) semi join [[p5] join ([item#4] join catalog_sales#2)]` | `agg(MJ6) semi join [[[p5] join catalog_sales#2] join item#4]` |
| changed by | | reordered by DPhyp |
| C_out | 46,405,710 B | 19,232,607 B (plan's order 46,405,710 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p5 = `date_dim#6 (f)`, 1,096 rows.

### q14 MJ6

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p6] join ([item#5] join store_sales#3)` | `[item#5] join ([p6] join store_sales#3)` |
| changed by | | reordered by DPhyp |
| C_out | 83,631,678 B | 34,614,597 B (plan's order 83,631,678 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p6 = `date_dim#7 (f)`, 1,096 rows.

### q14 MJ10

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[p10] join ([item#6] join catalog_sales#4)] semi join ([item#7] join (agg(MJ11) semi join [[p11] join ([item#8] join web_sales#4)]))` | `[[[p10] join catalog_sales#4] semi join ([agg(MJ11) semi join [[[p11] join web_sales#4] join item#8]] join item#7)] join item#6` |
| changed by | | reordered by DPhyp; flip: `agg(MJ11) semi join [[[p11] join web_sales#4] join item#8]` now builds |
| C_out | 31,625,237 B | 10,092,088 B (plan's order 31,625,237 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ10's missed build came out 46689 rows, est 2011.1); replan 3.

p10 = `date_dim#11 (f)`, 30 rows. p11 = `date_dim#12 (f)`, 1,096 rows.

### q14 MJ11

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ12) semi join [[p12] join ([item#9] join catalog_sales#5)]` | `agg(MJ12) semi join [[[p12] join catalog_sales#5] join item#9]` |
| changed by | | reordered by DPhyp |
| C_out | 46,405,710 B | 19,232,607 B (plan's order 46,405,710 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p12 = `date_dim#13 (f)`, 1,096 rows.

### q14 MJ12

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p13] join ([item#10] join store_sales#5)` | `[item#10] join ([p13] join store_sales#5)` |
| changed by | | reordered by DPhyp |
| C_out | 83,631,678 B | 34,614,597 B (plan's order 83,631,678 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p13 = `date_dim#14 (f)`, 1,096 rows.

### q14 MJ16

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[p17] join ([item#11] join web_sales#6)] semi join ([item#12] join (agg(MJ17) semi join [[p18] join ([item#13] join web_sales#7)]))` | `[[[p17] join web_sales#6] semi join ([agg(MJ17) semi join [[[p18] join web_sales#7] join item#13]] join item#12)] join item#11` |
| changed by | | reordered by DPhyp; flip: `agg(MJ17) semi join [[[p18] join web_sales#7] join item#13]` now builds |
| C_out | 31,626,050 B | 10,092,962 B (plan's order 31,626,050 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3 (MJ16's missed build came out 23524 rows, est 2018.8).

p17 = `date_dim#18 (f)`, 30 rows. p18 = `date_dim#19 (f)`, 1,096 rows.

### q14 MJ17

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ18) semi join [[p19] join ([item#14] join catalog_sales#7)]` | `agg(MJ18) semi join [[[p19] join catalog_sales#7] join item#14]` |
| changed by | | reordered by DPhyp |
| C_out | 46,405,710 B | 19,232,607 B (plan's order 46,405,710 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p19 = `date_dim#20 (f)`, 1,096 rows.

### q14 MJ18

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p20] join ([item#15] join store_sales#7)` | `[item#15] join ([p20] join store_sales#7)` |
| changed by | | reordered by DPhyp |
| C_out | 83,631,678 B | 34,614,597 B (plan's order 83,631,678 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p20 = `date_dim#21 (f)`, 1,096 rows.

### q14 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |
| plan as written | catalog_sales#1 | cs_sold_date_sk | p1.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | web_sales#1 | ws_sold_date_sk | p2.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | store_sales#2 | ss_sold_date_sk | p3.d_date_sk | 30 values, 2452215 to 2452244 | 24 → 3 (16-17,23) |
| plan as written | web_sales#2 | ws_sold_date_sk | p4.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | catalog_sales#2 | cs_sold_date_sk | p5.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | store_sales#3 | ss_sold_date_sk | p6.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |
| plan as written | store_sales#4 | ss_sold_date_sk | p7.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |
| plan as written | catalog_sales#3 | cs_sold_date_sk | p8.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | web_sales#3 | ws_sold_date_sk | p9.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | catalog_sales#4 | cs_sold_date_sk | p10.d_date_sk | 30 values, 2452215 to 2452244 | 12 → 1 (8) |
| plan as written | web_sales#4 | ws_sold_date_sk | p11.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | catalog_sales#5 | cs_sold_date_sk | p12.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | store_sales#5 | ss_sold_date_sk | p13.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |
| plan as written | store_sales#6 | ss_sold_date_sk | p14.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |
| plan as written | catalog_sales#6 | cs_sold_date_sk | p15.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | web_sales#5 | ws_sold_date_sk | p16.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | web_sales#6 | ws_sold_date_sk | p17.d_date_sk | 30 values, 2452215 to 2452244 | 6 → 1 (4) |
| plan as written | web_sales#7 | ws_sold_date_sk | p18.d_date_sk | 2451180 to 2452275 | 6 → 4 (1-4) |
| plan as written | catalog_sales#7 | cs_sold_date_sk | p19.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | store_sales#7 | ss_sold_date_sk | p20.d_date_sk | 2451180 to 2452275 | 24 → 15 (4-17,23) |

p0 = `date_dim#1 (f)`, 1,096 rows. p1 = `date_dim#2 (f)`, 1,096 rows. p2 = `date_dim#3 (f)`, 1,096 rows. p3 = `date_dim#4 (f)`, 30 rows. p4 = `date_dim#5 (f)`, 1,096 rows. p5 = `date_dim#6 (f)`, 1,096 rows. p6 = `date_dim#7 (f)`, 1,096 rows. p7 = `date_dim#8 (f)`, 1,096 rows. p8 = `date_dim#9 (f)`, 1,096 rows. p9 = `date_dim#10 (f)`, 1,096 rows. p10 = `date_dim#11 (f)`, 30 rows. p11 = `date_dim#12 (f)`, 1,096 rows. p12 = `date_dim#13 (f)`, 1,096 rows. p13 = `date_dim#14 (f)`, 1,096 rows. p14 = `date_dim#15 (f)`, 1,096 rows. p15 = `date_dim#16 (f)`, 1,096 rows. p16 = `date_dim#17 (f)`, 1,096 rows. p17 = `date_dim#18 (f)`, 30 rows. p18 = `date_dim#19 (f)`, 1,096 rows. p19 = `date_dim#20 (f)`, 1,096 rows. p20 = `date_dim#21 (f)`, 1,096 rows.

## q15

### q15 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p0] join ([customer_address] join ([customer] join catalog_sales))` | `[[[p0] join catalog_sales] join customer] join customer_address` |
| changed by | | reordered by DPhyp |
| C_out | 4,830,053 B | 331,238 B (plan's order 4,830,053 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 41224 rows, est 6084.0).

p0 = `date_dim (f)`, 91 rows.

### q15 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2452002 to 2452092 | 12 → 1 (7) |

p0 = `date_dim (f)`, 91 rows.

## q16

### q16 MJ1

| | plan | DPhyp before the run | inside p0, replan 1 |
|---|---|---|---|
| join tree | `[[[call_center (f)] join ([customer_address (f)] join ([date_dim (f)] join catalog_sales#1))] semi join catalog_sales#2] anti join catalog_returns` | `[[call_center (f)] join ([[date_dim (f)] join ([customer_address (f)] join catalog_sales#1)] semi join catalog_sales#2)] anti join catalog_returns` | `[m0] join ([[m2] join ([m1] join catalog_sales#1)] semi join catalog_sales#2)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ca_address_sk = cs_ship_addr_sk came out 2446 rows, est 950.0 (q-error 2.57); reordered by DPhyp |
| C_out | 3,065,289 B | 1,841,055 B (plan's order 3,065,289 B) | 3,130,559 B (previous order 4,732,810 B) |

m0 = `call_center (f)`, 6 rows. m1 = `date_dim (f)`, 61 rows. m2 = `customer_address (f)`, 2,446 rows. p0 = `[call_center (f)] join ([[date_dim (f)] join ([customer_address (f)] join catalog_sales#1)] semi join catalog_sales#2)`, 2,358 rows.

### q16 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | catalog_returns | cr_order_number | p0.cs_order_number | 117666 to 132148 | 2 → 2 (0-1) |

p0 = `[call_center (f)] join ([[date_dim (f)] join ([customer_address (f)] join catalog_sales#1)] semi join catalog_sales#2)`, 2,358 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's join on ca_address_sk = cs_ship_addr_sk came out 2446 rows, est 950.0 (q-error 2.57); m0 = `call_center (f)`, 6 rows; m1 = `date_dim (f)`, 61 rows; m2 = `customer_address (f)`, 2,446 rows.

## q17

### q17 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[[[catalog_sales] join store_returns] join store_sales] join p2] join p1] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[[catalog_sales] join store_returns] join store_sales` now builds; flip: `[[[catalog_sales] join store_returns] join store_sales] join p2` now builds; flip: `[[[[catalog_sales] join store_returns] join store_sales] join p2] join p1` now builds; flip: `[[[[[catalog_sales] join store_returns] join store_sales] join p2] join p1] join p0` now builds; flip: `[[[[[[catalog_sales] join store_returns] join store_sales] join p2] join p1] join p0] join store` now builds |
| C_out | 871,006 B | 1,862 B (plan's order 871,006 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 8 rows, est 2.2).

p0 = `date_dim#1 (f)`, 274 rows. p1 = `date_dim#2 (f)`, 274 rows. p2 = `date_dim#3 (f)`, 91 rows.

### q17 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451911 to 2452184 | 12 → 3 (6-8) |
| plan as written | store_returns | sr_returned_date_sk | p1.d_date_sk | 2451911 to 2452184 | 3 → 1 (1) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 2451911 to 2452001 | 24 → 3 (13-14,23) |

p0 = `date_dim#1 (f)`, 274 rows. p1 = `date_dim#2 (f)`, 274 rows. p2 = `date_dim#3 (f)`, 91 rows.

## q18

### q18 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[p0] join ([customer_address (f)] join ([[customer (f)] join ([customer_demographics#1 (f)] join catalog_sales)] join customer_demographics#2))] join item` | `[[[p0] join ([[customer_address (f)] join customer (f)] join ([customer_demographics#1 (f)] join catalog_sales))] join customer_demographics#2] join item` |
| changed by | | reordered by DPhyp |
| C_out | 6,044,459 B | 3,472,576 B (plan's order 6,044,459 B) |

p0 = `date_dim (f)`, 365 rows.

### q18 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2450815 to 2451179 | 12 → 3 (0-2) |
| after join order | customer_demographics#2 | cd_demo_sk | p1.c_current_cdemo_sk | 6141 to 1919490 | 16 → 16 (0-15) |

p0 = `date_dim (f)`, 365 rows. p1 = `[p0] join ([[customer_address (f)] join customer (f)] join ([customer_demographics#1 (f)] join catalog_sales))`, 1,314 rows, holding MJ1.

## q19

MJ1 kept the plan's order: `[store] join ([[[item (f)] join ([p0] join store_sales)] join customer] join customer_address)`.

### q19 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451119 to 2451148 | 24 → 3 (2-3,23) |

p0 = `date_dim (f)`, 30 rows.

## q20

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q21

### q21 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[p0] join ([item (f)] join ([warehouse] join inventory))` | `[warehouse] join ([p0] join ([item (f)] join inventory))` | `[m1] join ([[p0] join inventory] join m2)` | `[m1] join ([m2] join m3)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on i_item_sk = inv_item_sk came out 1104 rows, est 96.5 (q-error 11.44); reordered by DPhyp | the build of MJ1's join on inv_item_sk = i_item_sk came out 405000 rows, est 16473.8 (q-error 24.58); flip: `m2` now builds |
| C_out | 19,159,564 B | 114,082 B (plan's order 19,159,564 B) | 487,673 B (previous order 1,305,000 B) | 2,066,688 B (previous order 2,066,688 B) |

m1 = `warehouse`, 5 rows. m2 = `item (f)`, 1,104 rows. m3 = `[p0] join inventory`, 405,000 rows. p0 = `date_dim (f)`, 61 rows.

### q21 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory | inv_date_sk | p0.d_date_sk | 2451585 to 2451645 | 96 → 4 (40-43) |

p0 = `date_dim (f)`, 61 rows.

## q22

### q22 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[item] join ([p0] join inventory)` | `[[p0] join inventory] join item` | `[item] join m1` |
| changed by | | flip: `[p0] join inventory` now builds | the build of MJ1's join on inv_item_sk = i_item_sk came out 2340000 rows, est 494213.9 (q-error 4.73); flip: `item` now builds |
| C_out | 39,872,321 B | 39,872,321 B (plan's order 39,872,321 B) | 160,122,147 B (previous order 160,122,147 B) |

m1 = `[p0] join inventory`, 2,340,000 rows. p0 = `date_dim (f)`, 366 rows.

### q22 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory | inv_date_sk | p0.d_date_sk | 2451545 to 2451910 | 96 → 20 (38-57) |

p0 = `date_dim (f)`, 366 rows.

## q23

MJ2 kept the plan's order: `[item#1] join ([p1] join store_sales#1)`.

MJ4 kept the plan's order: `[customer#3] join store_sales#3`.

MJ6 kept the plan's order: `[item#2] join ([p4] join store_sales#4)`.

MJ8 kept the plan's order: `[customer#6] join store_sales#6`.

A replan was refused, work outside its builds having started: the build of MJ5's join on c_customer_sk = ws_bill_customer_sk came out 4703 rows, est 18171.6 (q-error 3.86).

A replan was refused, work outside its builds having started: the build of MJ5's join on ws_bill_customer_sk = c_customer_sk came out 366 rows, est 6724.0 (q-error 18.37).

A replan was refused, work outside its builds having started: the build of MJ5's join on ws_item_sk = i_item_sk came out 366 rows, est 6724.0 (q-error 18.37).

### q23 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[p0] join ([customer#1] join catalog_sales)] join agg(MJ2) (f)] join agg([agg(MJ3)] cross join MJ4) (f)` | `[[[[p0] join catalog_sales] join agg([agg(MJ3)] cross join MJ4) (f)] join customer#1] join agg(MJ2) (f)` | `[[[agg([agg(MJ3)] cross join MJ4) (f)] join m6] join customer#1] join agg(MJ2) (f)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on cs_bill_customer_sk = c_customer_sk came out 13261 rows, est 1939.1 (q-error 6.84); flip: `agg([agg(MJ3)] cross join MJ4) (f)` now builds |
| C_out | 11,463,957 B | 1,604,382 B (plan's order 11,463,957 B) | 10,406,860 B (previous order 10,406,860 B) |

Calls that left the order unchanged: replan 2; replan 3 (MJ1's missed build came out 4703 rows, est 18171.6); replan 4 (MJ1's missed build came out 667 rows, est 13261.0).

m6 = `[p0] join catalog_sales`, 13,261 rows. p0 = `date_dim#1 (f)`, 29 rows.

### q23 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p2] join ([customer#2] join store_sales#2)` | `[customer#2] join ([p2] join store_sales#2)` |
| changed by | | reordered by DPhyp |
| C_out | 125,991,025 B | 98,295,566 B (plan's order 125,991,025 B) |

Calls that left the order unchanged: replan 1; replan 2.

p2 = `date_dim#3 (f)`, 1,461 rows.

### q23 MJ5

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[[[p3] join ([customer#4] join web_sales)] join agg(MJ6) (f)] join agg([agg(MJ7)] cross join MJ8) (f)` | `[[[[p3] join web_sales] join agg([agg(MJ7)] cross join MJ8) (f)] join customer#4] join agg(MJ6) (f)` | `[[[agg([agg(MJ7)] cross join MJ8) (f)] join m7] join customer#4] join agg(MJ6) (f)` |
| changed by | | reordered by DPhyp | the build of MJ5's join on ws_bill_customer_sk = c_customer_sk came out 6724 rows, est 1951.3 (q-error 3.45); flip: `agg([agg(MJ7)] cross join MJ8) (f)` now builds |
| C_out | 11,475,674 B | 1,610,533 B (plan's order 11,475,674 B) | 5,276,806 B (previous order 5,276,806 B) |

Calls that left the order unchanged: replan 1; replan 3; replan 4.

m7 = `[p3] join web_sales`, 6,724 rows. p3 = `date_dim#4 (f)`, 29 rows.

### q23 MJ7

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p5] join ([customer#5] join store_sales#5)` | `[customer#5] join ([p5] join store_sales#5)` |
| changed by | | reordered by DPhyp |
| C_out | 125,991,025 B | 98,295,566 B (plan's order 125,991,025 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4.

p5 = `date_dim#6 (f)`, 1,461 rows.

### q23 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 29 values, 2451576 to 2451604 | 12 → 1 (4) |
| plan as written | store_sales#1 | ss_sold_date_sk | p1.d_date_sk | 2451545 to 2453005 | 24 → 16 (8-23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p2.d_date_sk | 2451545 to 2453005 | 24 → 16 (8-23) |
| plan as written | web_sales | ws_sold_date_sk | p3.d_date_sk | 29 values, 2451576 to 2451604 | 6 → 1 (2) |
| plan as written | store_sales#4 | ss_sold_date_sk | p4.d_date_sk | 2451545 to 2453005 | 24 → 16 (8-23) |
| plan as written | store_sales#5 | ss_sold_date_sk | p5.d_date_sk | 2451545 to 2453005 | 24 → 16 (8-23) |

p0 = `date_dim#1 (f)`, 29 rows. p1 = `date_dim#2 (f)`, 1,461 rows. p2 = `date_dim#3 (f)`, 1,461 rows. p3 = `date_dim#4 (f)`, 29 rows. p4 = `date_dim#5 (f)`, 1,461 rows. p5 = `date_dim#6 (f)`, 1,461 rows.

## q24

### q24 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#1] join ([customer#1] join ([item#1] join ([store#1 (f)] join ([store_returns#1] join store_sales#1))))` | `[[[[[store#1 (f)] join customer_address#1] join customer#1] join store_sales#1] join store_returns#1] join item#1` |
| changed by | | reordered by DPhyp |
| C_out | 25,989,110 B | 4,396 B (plan's order 25,989,110 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 130 rows, est 22.5); replan 2 (MJ1's missed build came out 226 rows, est 52.0).

### q24 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#2] join ([customer#2] join ([item#2 (f)] join ([store#2 (f)] join ([store_returns#2] join store_sales#2))))` | `[[[[[store#2 (f)] join customer_address#2] join customer#2] join store_sales#2] join item#2 (f)] join store_returns#2` |
| changed by | | reordered by DPhyp |
| C_out | 14,598,172 B | 3,809 B (plan's order 14,598,172 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3 (MJ2's missed build came out 130 rows, est 22.5); replan 4 (MJ2's missed build came out 226 rows, est 52.0).

## q25

### q25 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[p2] join ([[catalog_sales] join store_returns] join store_sales)] join p1] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[p2] join ([[catalog_sales] join store_returns] join store_sales)` now builds; flip: `[[p2] join ([[catalog_sales] join store_returns] join store_sales)] join p1` now builds; flip: `[[[p2] join ([[catalog_sales] join store_returns] join store_sales)] join p1] join p0` now builds; flip: `[[[[p2] join ([[catalog_sales] join store_returns] join store_sales)] join p1] join p0] join store` now builds |
| C_out | 640,665 B | 1,518 B (plan's order 640,665 B) |

p0 = `date_dim#1 (f)`, 214 rows. p1 = `date_dim#2 (f)`, 214 rows. p2 = `date_dim#3 (f)`, 30 rows.

### q25 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2452001 to 2452214 | 12 → 2 (7-8) |
| plan as written | store_returns | sr_returned_date_sk | p1.d_date_sk | 2452001 to 2452214 | 3 → 1 (1) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 30 values, 2452001 to 2452030 | 24 → 2 (14,23) |
| after join order | store_sales | ss_item_sk | p3.sr_item_sk | 29 values, 853 to 17477 | 2 → 2 (14,23) |

p0 = `date_dim#1 (f)`, 214 rows. p1 = `date_dim#2 (f)`, 214 rows. p2 = `date_dim#3 (f)`, 30 rows. p3 = `[catalog_sales] join store_returns`, 29 rows, holding MJ1.

## q26

### q26 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[promotion (f)] join ([item] join ([p0] join ([customer_demographics (f)] join catalog_sales)))` | `[[[p0] join ([customer_demographics (f)] join catalog_sales)] join promotion (f)] join item` | `[[promotion (f)] join m1] join item` |
| changed by | | reordered by DPhyp | the build of MJ1's join on cs_promo_sk = p_promo_sk came out 4045 rows, est 1046.5 (q-error 3.87); flip: `promotion (f)` now builds |
| C_out | 672,093 B | 650,916 B (plan's order 672,093 B) | 571,229 B (previous order 571,229 B) |

m1 = `[p0] join ([customer_demographics (f)] join catalog_sales)`, 4,045 rows. p0 = `date_dim (f)`, 366 rows.

### q26 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 12 → 3 (4-6) |

p0 = `date_dim (f)`, 366 rows.

## q27

skipped: refused by datafusion: SanityCheckPlan

## q28

skipped: refused: unsupported: DISTINCT inside count(DISTINCT store_sales.ss_list_price) (#62)

## q29

### q29 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[[[store_returns] join catalog_sales] join p1] join store_sales] join p2] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[[[[store_returns] join catalog_sales] join p1] join store_sales] join p2` now builds; flip: `[[[[[store_returns] join catalog_sales] join p1] join store_sales] join p2] join p0` now builds; flip: `[[[[[[store_returns] join catalog_sales] join p1] join store_sales] join p2] join p0] join store` now builds |
| C_out | 514,352 B | 4,637 B (plan's order 514,352 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 3 rows, est 0.4).

p0 = `date_dim#1 (f)`, 1,096 rows. p1 = `date_dim#2 (f)`, 122 rows. p2 = `date_dim#3 (f)`, 30 rows.

### q29 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | store_returns | sr_returned_date_sk | p1.d_date_sk | 2451423 to 2451544 | 3 → 1 (0) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 30 values, 2451423 to 2451452 | 24 → 2 (6,23) |
| after join order | store_sales | ss_item_sk | p3.sr_item_sk | 1111, 3584, 3694, 4951, 12307, 12830, 13634, 14972, 15673, 15913 | 2 → 2 (6,23) |

p0 = `date_dim#1 (f)`, 1,096 rows. p1 = `date_dim#2 (f)`, 122 rows. p2 = `date_dim#3 (f)`, 30 rows. p3 = `[[store_returns] join catalog_sales] join p1`, 10 rows, holding MJ1.

## q30

MJ2 kept the plan's order: `[[date_dim#1 (f)] join web_returns#1] join customer_address#1`.

MJ3 kept the plan's order: `[[date_dim#2 (f)] join web_returns#2] join customer_address#3`.

### q30 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[agg(MJ2)] join customer] join customer_address#2 (f)] join agg(MJ3)` | `[agg(MJ3)] join ([[customer_address#2 (f)] join customer] join agg(MJ2))` | `[m1] join ([[m0] join agg(MJ2)] join customer)` |
| changed by | | reordered by DPhyp; flip: `agg(MJ3)` now builds | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 2446 rows, est 950.0 (q-error 2.57); reordered by DPhyp |
| C_out | 1,786,399 B | 296,281 B (plan's order 1,786,399 B) | 413,466 B (previous order 1,416,889 B) |

m0 = `agg(MJ3)`, 52 rows. m1 = `customer_address#2 (f)`, 2,446 rows.

## q31

MJ5 kept the plan's order: `[[p3] join web_sales#1] join customer_address#4`.

MJ6 kept the plan's order: `[[p4] join web_sales#2] join customer_address#5`.

### q31 MJ1

| | plan | DPhyp before the run | replan 2 | replan 4 | replan 5 | replan 6 | replan 7 |
|---|---|---|---|---|---|---|---|
| join tree | `[[[[[agg(MJ2)] join agg(MJ3)] join agg(MJ4)] join agg(MJ5)] join agg(MJ6)] join agg(MJ7)` | `[[[[agg(MJ5)] join agg(MJ6)] join ([agg(MJ2)] join agg(MJ3))] join agg(MJ4)] join agg(MJ7)` | `[[[[[agg(MJ6)] join m7] join agg(MJ2)] join agg(MJ3)] join agg(MJ7)] join agg(MJ4)` | `[[[agg(MJ3)] join m11] join agg(MJ7)] join agg(MJ4)` | `[[agg(MJ7)] join m13] join agg(MJ4)` | `[[m13] join agg(MJ4)] join agg(MJ7)` | `[[agg(MJ4)] join m15] join agg(MJ7)` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_addr_sk = ca_address_sk came out 20825 rows, est 6123.4 (q-error 3.40); reordered by DPhyp; flip: `agg(MJ6)` now builds | the build of MJ3's join on ss_addr_sk = ca_address_sk came out 77665 rows, est 10910.2 (q-error 7.12); flip: `agg(MJ3)` now builds | the build of MJ1's join on ca_county = ca_county came out 244 rows, est 97.2 (q-error 2.51); flip: `agg(MJ7)` now builds | the build of MJ7's join on ws_bill_addr_sk = ca_address_sk came out 37604 rows, est 6190.7 (q-error 6.07); reordered by DPhyp | the build of MJ4's join on ss_addr_sk = ca_address_sk came out 143778 rows, est 11026.7 (q-error 13.04); flip: `agg(MJ4)` now builds |
| C_out | 530,917 B | 333,216 B (plan's order 530,917 B) | 186,044 B (previous order 239,015 B) | 26,786 B (previous order 26,786 B) | 36,481 B (previous order 36,481 B) | 36,481 B (previous order 36,481 B) | 36,481 B (previous order 36,481 B) |

Calls that left the order unchanged: replan 1; replan 3.

m7 = `agg(MJ5)`, 942 rows. m9 = `[agg(MJ6)] join m7`, 514 rows. m11 = `[m9] join agg(MJ2)`, 486 rows. m13 = `[agg(MJ3)] join m11`, 244 rows. m15 = `m13`, 244 rows.

### q31 MJ2

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer_address#1] join ([p0] join store_sales#1)` | `[[p0] join store_sales#1] join customer_address#1` | `[customer_address#1] join m10` |
| changed by | | flip: `[p0] join store_sales#1` now builds | the build of MJ2's join on ss_addr_sk = ca_address_sk came out 82426 rows, est 10907.9 (q-error 7.56); flip: `customer_address#1` now builds |
| C_out | 1,353,444 B | 1,353,444 B (plan's order 1,353,444 B) | 4,125,524 B (previous order 4,125,524 B) |

Calls that left the order unchanged: replan 1; replan 2.

m10 = `[p0] join store_sales#1`, 82,426 rows. p0 = `date_dim#1 (f)`, 91 rows.

### q31 MJ3

| | plan | DPhyp before the run | replan 4 |
|---|---|---|---|
| join tree | `[customer_address#2] join ([p1] join store_sales#2)` | `[[p1] join store_sales#2] join customer_address#2` | `[customer_address#2] join m12` |
| changed by | | flip: `[p1] join store_sales#2` now builds | the build of MJ3's join on ss_addr_sk = ca_address_sk came out 77665 rows, est 10910.2 (q-error 7.12); flip: `customer_address#2` now builds |
| C_out | 1,353,444 B | 1,353,444 B (plan's order 1,353,444 B) | 3,887,230 B (previous order 3,887,230 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

m12 = `[p1] join store_sales#2`, 77,665 rows. p1 = `date_dim#2 (f)`, 91 rows.

### q31 MJ4

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[customer_address#3] join ([p2] join store_sales#3)` | `[[p2] join store_sales#3] join customer_address#3` | `[customer_address#3] join m16` |
| changed by | | flip: `[p2] join store_sales#3` now builds | the build of MJ4's join on ss_addr_sk = ca_address_sk came out 143778 rows, est 11026.7 (q-error 13.04); flip: `customer_address#3` now builds |
| C_out | 1,368,317 B | 1,368,317 B (plan's order 1,368,317 B) | 7,196,269 B (previous order 7,196,269 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

m16 = `[p2] join store_sales#3`, 143,778 rows. p2 = `date_dim#3 (f)`, 92 rows.

### q31 MJ7

| | plan | replan 6 |
|---|---|---|
| join tree | `[[p5] join web_sales#3] join customer_address#6` | `[customer_address#6] join m14` |
| changed by | | the build of MJ7's join on ws_bill_addr_sk = ca_address_sk came out 37604 rows, est 6190.7 (q-error 6.07); flip: `customer_address#6` now builds |
| C_out | 560,605 B | 1,882,127 B (previous order 1,882,127 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 2; replan 3; replan 4; replan 5; replan 7.

m14 = `[p5] join web_sales#3`, 37,604 rows. p5 = `date_dim#6 (f)`, 92 rows.

### q31 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451635 | 24 → 3 (8-9,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2451636 to 2451726 | 24 → 3 (9-10,23) |
| plan as written | store_sales#3 | ss_sold_date_sk | p2.d_date_sk | 2451727 to 2451818 | 24 → 3 (10-11,23) |
| plan as written | web_sales#1 | ws_sold_date_sk | p3.d_date_sk | 2451545 to 2451635 | 6 → 1 (2) |
| plan as written | web_sales#2 | ws_sold_date_sk | p4.d_date_sk | 2451636 to 2451726 | 6 → 1 (2) |
| plan as written | web_sales#3 | ws_sold_date_sk | p5.d_date_sk | 2451727 to 2451818 | 6 → 1 (2) |

p0 = `date_dim#1 (f)`, 91 rows. p1 = `date_dim#2 (f)`, 91 rows. p2 = `date_dim#3 (f)`, 92 rows. p3 = `date_dim#4 (f)`, 91 rows. p4 = `date_dim#5 (f)`, 91 rows. p5 = `date_dim#6 (f)`, 92 rows.

## q32

MJ2 kept the plan's order: `[p0] join catalog_sales#2`.

### q32 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[item (f)] join catalog_sales#1] join date_dim#1 (f)] join agg(MJ2)` | `[[[item (f)] join agg(MJ2)] join catalog_sales#1] join date_dim#1 (f)` |
| changed by | | reordered by DPhyp |
| C_out | 48,827 B | 7,689 B (plan's order 48,827 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 4 rows, est 18.1).

### q32 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales#2 | cs_sold_date_sk | p0.d_date_sk | 2451571 to 2451661 | 12 → 2 (4-5) |

p0 = `date_dim#2 (f)`, 91 rows.

## q33

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_bill_addr_sk = ca_address_sk came out 13980 rows, est 2072.9 (q-error 6.74).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 5028 rows, est 334.7 (q-error 15.02).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_bill_addr_sk = ca_address_sk came out 7370 rows, est 2085.8 (q-error 3.53).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 2761 rows, est 336.8 (q-error 8.20).

### q33 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[[p0] join store_sales] join customer_address#1 (f)] join item#2) semi join [item#1 (f)]` | `([[customer_address#1 (f)] join m6] join item#2) semi join [m5]` | `[item#2 semi join [m5]] join ([m7] join m6)` |
| changed by | | flip: `[p0] join store_sales` now builds; flip: `[[p0] join store_sales] join customer_address#1 (f)` now builds | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 26339 rows, est 3716.6 (q-error 7.09); flip: `customer_address#1 (f)` now builds | the build of MJ1's join on ca_address_sk = ss_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 221,688 B | 221,688 B (plan's order 221,688 B) | 280,679 B (previous order 280,679 B) | 612,199 B (previous order 634,293 B) |

m5 = `item#1 (f)`, 1,811 rows. m6 = `[p0] join store_sales`, 26,339 rows. m7 = `customer_address#1 (f)`, 18,246 rows. p0 = `date_dim#1 (f)`, 31 rows.

### q33 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#2 (f)] join ([p1] join catalog_sales)] join item#4) semi join [item#3 (f)]` | `([[[p1] join catalog_sales] join customer_address#2 (f)] join item#4) semi join [item#3 (f)]` |
| changed by | | flip: `[p1] join catalog_sales` now builds |
| C_out | 90,480 B | 90,480 B (plan's order 90,480 B) |

Calls that left the order unchanged: replan 1; replan 2.

p1 = `date_dim#2 (f)`, 31 rows.

### q33 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#3 (f)] join ([p2] join web_sales)] join item#6) semi join [item#5 (f)]` | `([[[p2] join web_sales] join customer_address#3 (f)] join item#6) semi join [item#5 (f)]` |
| changed by | | flip: `[p2] join web_sales` now builds |
| C_out | 90,827 B | 90,827 B (plan's order 90,827 B) |

Calls that left the order unchanged: replan 1; replan 2.

p2 = `date_dim#3 (f)`, 31 rows.

### q33 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 31 values, 2450935 to 2450965 | 24 → 3 (0-1,23) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 31 values, 2450935 to 2450965 | 12 → 1 (0) |
| plan as written | web_sales | ws_sold_date_sk | p2.d_date_sk | 31 values, 2450935 to 2450965 | 6 → 1 (0) |

p0 = `date_dim#1 (f)`, 31 rows. p1 = `date_dim#2 (f)`, 31 rows. p2 = `date_dim#3 (f)`, 31 rows.

## q34

### q34 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer] join agg(MJ2) (f)` | `[agg(MJ2) (f)] join customer` |
| changed by | | flip: `agg(MJ2) (f)` now builds |
| C_out | 94,510 B | 94,510 B (plan's order 94,510 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ1's missed build came out 455 rows, est 6105.7).

### q34 MJ2

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[household_demographics (f)] join ([store (f)] join ([p0] join store_sales))` | `[store (f)] join ([p0] join ([household_demographics (f)] join store_sales))` | `[m1] join ([m2] join ([p0] join store_sales))` |
| changed by | | reordered by DPhyp | the build of MJ2's join on hd_demo_sk = ss_hdemo_sk came out 1040 rows, est 320.0 (q-error 3.25); reordered by DPhyp |
| C_out | 14,104,817 B | 3,005,301 B (plan's order 14,104,817 B) | 9,395,842 B (previous order 9,767,228 B) |

m1 = `store (f)`, 12 rows. m2 = `household_demographics (f)`, 1,040 rows. p0 = `date_dim (f)`, 252 rows.

### q34 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2452272 | 24 → 15 (4-17,23) |

p0 = `date_dim (f)`, 252 rows.

## q35

MJ2 kept the plan's order: `[p1] join web_sales`.

MJ3 kept the plan's order: `[p2] join catalog_sales`.

### q35 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[customer_address] join customer] join customer_demographics] semi join ([p0] join store_sales)` | `[[[customer] semi join ([p0] join store_sales)] join customer_address] join customer_demographics` |
| changed by | | reordered by DPhyp |
| C_out | 11,047,236 B | 8,157,513 B (plan's order 11,047,236 B) |

Calls that left the order unchanged: inside p3, replan 1 (MJ1's missed build came out 23496 rows, est 68515.4).

p0 = `date_dim#1 (f)`, 274 rows. p3 = `[[customer] semi join ([p0] join store_sales)] join customer_address`, 22,704 rows.

### q35 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2452276 to 2452549 | 24 → 5 (17-20,23) |
| plan as written | web_sales | ws_sold_date_sk | p1.d_date_sk | 2452276 to 2452549 | 6 → 2 (4-5) |
| plan as written | catalog_sales | cs_sold_date_sk | p2.d_date_sk | 2452276 to 2452549 | 12 → 2 (9-10) |
| after join order | customer_demographics | cd_demo_sk | p3.c_current_cdemo_sk | 44 to 1920788 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 274 rows. p1 = `date_dim#2 (f)`, 274 rows. p2 = `date_dim#3 (f)`, 274 rows. p3 = `[[customer] semi join ([p0] join store_sales)] join customer_address`, 22,704 rows, holding MJ1.

Probe p3, replan 1: the build of MJ1's join on c_current_addr_sk = ca_address_sk came out 23496 rows, est 68515.4 (q-error 2.92); m3 = `[customer] semi join ([p0] join store_sales)`, 23,496 rows.

## q36

skipped: refused: unsupported: BoundedWindowAggExec: window functions (#143)

## q37

MJ1 kept the plan's order: `[[p0] join ([item (f)] join inventory (f))] join catalog_sales`.

### q37 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory | inv_date_sk | p0.d_date_sk | 2451576 to 2451636 | 96 → 5 (39-43) |

p0 = `date_dim (f)`, 61 rows.

## q38

MJ1 kept the plan's order: `agg(MJ3) semi join [agg(MJ2)]`.

MJ3 kept the plan's order: `agg(MJ5) semi join [agg(MJ4)]`.

### q38 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer#1] join ([p0] join web_sales)` | `[[p0] join web_sales] join customer#1` |
| changed by | | flip: `[p0] join web_sales` now builds |
| C_out | 1,781,743 B | 1,781,743 B (plan's order 1,781,743 B) |

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 142164 rows, est 49253.8).

p0 = `date_dim#1 (f)`, 366 rows.

### q38 MJ4

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer#2] join ([p1] join catalog_sales)` | `[[p1] join catalog_sales] join customer#2` | `[customer#2] join m5` |
| changed by | | flip: `[p1] join catalog_sales` now builds | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 286899 rows, est 73433.4 (q-error 3.91); flip: `customer#2` now builds |
| C_out | 2,662,408 B | 2,662,408 B (plan's order 2,662,408 B) | 6,862,885 B (previous order 6,862,885 B) |

Calls that left the order unchanged: replan 1; replan 2.

m5 = `[p1] join catalog_sales`, 286,899 rows. p1 = `date_dim#2 (f)`, 366 rows.

### q38 MJ5

| | plan | DPhyp before the run | replan 5 |
|---|---|---|---|
| join tree | `[customer#3] join ([p2] join store_sales)` | `[[p2] join store_sales] join customer#3` | `[customer#3] join m7` |
| changed by | | flip: `[p2] join store_sales` now builds | the build of MJ5's join on ss_customer_sk = c_customer_sk came out 541109 rows, est 139514.9 (q-error 3.88); flip: `customer#3` now builds |
| C_out | 5,737,914 B | 5,737,914 B (plan's order 5,737,914 B) | 12,943,820 B (previous order 12,943,820 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4.

m7 = `[p2] join store_sales`, 541,109 rows. p2 = `date_dim#3 (f)`, 366 rows.

### q38 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 6 → 2 (2-3) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 2451545 to 2451910 | 12 → 3 (4-6) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim#1 (f)`, 366 rows. p1 = `date_dim#2 (f)`, 366 rows. p2 = `date_dim#3 (f)`, 366 rows.

## q39

### q39 MJ1

| | plan | replan 1 |
|---|---|---|
| join tree | `[agg(MJ2) (f)] join agg(MJ3) (f)` | `[agg(MJ3) (f)] join agg(MJ2) (f)` |
| changed by | | the build of MJ2's join on inv_item_sk = i_item_sk came out 180000 rows, est 4186.0 (q-error 43.00); flip: `agg(MJ3) (f)` now builds |
| C_out | 68,022 B | 92,159 B (previous order 92,159 B) |

Calls that left the order unchanged: DPhyp before the run; replan 2; replan 3 (MJ1's missed build came out 3170 rows, est 36000.0).

### q39 MJ2

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p0] join ([warehouse#1] join ([item#1] join inventory#1))` | `[warehouse#1] join ([[p0] join inventory#1] join item#1)` | `[m2] join ([item#1] join m3)` |
| changed by | | reordered by DPhyp | the build of MJ2's join on inv_item_sk = i_item_sk came out 180000 rows, est 4186.0 (q-error 43.00); flip: `item#1` now builds |
| C_out | 18,707,880 B | 434,608 B (plan's order 18,707,880 B) | 13,558,500 B (previous order 13,558,500 B) |

Calls that left the order unchanged: replan 2; replan 3.

m2 = `warehouse#1`, 5 rows. m3 = `[p0] join inventory#1`, 180,000 rows. p0 = `date_dim#1 (f)`, 31 rows.

### q39 MJ3

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[p1] join ([warehouse#2] join ([item#2] join inventory#2))` | `[warehouse#2] join ([[p1] join inventory#2] join item#2)` | `[m4] join ([item#2] join m5)` |
| changed by | | reordered by DPhyp | the build of MJ3's join on inv_item_sk = i_item_sk came out 180000 rows, est 5671.3 (q-error 31.74); flip: `item#2` now builds |
| C_out | 28,033,367 B | 588,823 B (plan's order 28,033,367 B) | 13,558,500 B (previous order 13,558,500 B) |

Calls that left the order unchanged: replan 1.

m4 = `warehouse#2`, 5 rows. m5 = `[p1] join inventory#2`, 180,000 rows. p1 = `date_dim#2 (f)`, 28 rows.

### q39 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory#1 | inv_date_sk | p0.d_date_sk | 31 values, 2451911 to 2451941 | 96 → 2 (57-58) |
| plan as written | inventory#2 | inv_date_sk | p1.d_date_sk | 28 values, 2451942 to 2451969 | 96 → 3 (58-60) |

p0 = `date_dim#1 (f)`, 31 rows. p1 = `date_dim#2 (f)`, 28 rows.

## q40

### q40 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p0] join ([item (f)] join ([warehouse] join (catalog_sales left join [catalog_returns])))` | `([warehouse] join ([p0] join ([item (f)] join catalog_sales))) left join [catalog_returns]` | `([m2] join ([m3] join ([p0] join catalog_sales))) left join [m1]` |
| changed by | | reordered by DPhyp | the build of MJ1's join on i_item_sk = cs_item_sk came out 1104 rows, est 96.5 (q-error 11.44); reordered by DPhyp |
| C_out | 27,431,338 B | 98,929 B (plan's order 27,431,338 B) | 460,334 B (previous order 1,131,657 B) |

m1 = `catalog_returns`, 144,067 rows. m2 = `warehouse`, 5 rows. m3 = `item (f)`, 1,104 rows. p0 = `date_dim (f)`, 61 rows.

### q40 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451585 to 2451645 | 12 → 2 (4-5) |

p0 = `date_dim (f)`, 61 rows.

## q41

MJ1 kept the plan's order: `[item#1 (f)] left join agg(item#2 (f))`.

## q42

### q42 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item (f)] join ([p0] join store_sales)` | `[p0] join ([item (f)] join store_sales)` |
| changed by | | reordered by DPhyp |
| C_out | 161,611 B | 128,905 B (plan's order 161,611 B) |

p0 = `date_dim (f)`, 30 rows.

### q42 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451850 to 2451879 | 24 → 3 (11-12,23) |

p0 = `date_dim (f)`, 30 rows.

## q43

MJ1 kept the plan's order: `[store (f)] join ([p0] join store_sales)`.

### q43 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim (f)`, 366 rows.

## q44

skipped: refused: unsupported: BoundedWindowAggExec: window functions (#143)

## q45

### q45 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item#1] join ([p0] join ([customer_address] join ([customer] join web_sales)))` | `[[[[p0] join web_sales] join customer] join item#1] join customer_address` |
| changed by | | reordered by DPhyp |
| C_out | 12,266,740 B | 1,023,535 B (plan's order 12,266,740 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 21369 rows, est 6123.1).

p0 = `date_dim (f)`, 91 rows.

### q45 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2452002 to 2452092 | 6 → 1 (3) |

p0 = `date_dim (f)`, 91 rows.

## q46

### q46 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[customer] join agg(MJ2)] join customer_address#2` | `[[customer_address#2] join customer] join agg(MJ2)` |
| changed by | | reordered by DPhyp |
| C_out | 6,467,832 B | 5,200,949 B (plan's order 6,467,832 B) |

### q46 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#1] join ([household_demographics (f)] join ([store (f)] join ([p0] join store_sales)))` | `[[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))] join customer_address#1` |
| changed by | | reordered by DPhyp; flip: `[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))` now builds |
| C_out | 51,609,845 B | 36,791,625 B (plan's order 51,609,845 B) |

p0 = `date_dim (f)`, 314 rows.

### q46 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451181 to 2452274 | 24 → 15 (4-17,23) |

p0 = `date_dim (f)`, 314 rows.

## q47

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q48

### q48 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[p0] join ([customer_address (f)] join ([customer_demographics (f)] join ([store] join store_sales (f))))` | `[store] join ([[[customer_demographics (f)] join store_sales (f)] join customer_address (f)] join p0)` | `[m1] join ([[customer_address (f)] join m2] join p0)` | `[m1] join ([p0] join m3)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 7644 rows, est 369.8 (q-error 20.67); flip: `customer_address (f)` now builds | the build of MJ1's join on ss_sold_date_sk = d_date_sk came out 1873 rows, est 278.9 (q-error 6.71); flip: `p0` now builds |
| C_out | 16,299,981 B | 205,797 B (plan's order 16,299,981 B) | 7,416 B (previous order 7,416 B) | 9,151 B (previous order 9,151 B) |

m1 = `store`, 12 rows. m2 = `[customer_demographics (f)] join store_sales (f)`, 7,644 rows. m3 = `[customer_address (f)] join m2`, 1,873 rows. p0 = `date_dim (f)`, 366 rows.

### q48 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim (f)`, 366 rows.

## q49

skipped: refused: unsupported: BoundedWindowAggExec: window functions (#143)

## q50

### q50 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p0] join ([date_dim#2] join ([store] join ([store_returns] join store_sales)))` | `[store] join ([[[p0] join store_returns] join store_sales] join date_dim#2)` |
| changed by | | reordered by DPhyp |
| C_out | 33,928,927 B | 395,338 B (plan's order 33,928,927 B) |

p0 = `date_dim#1 (f)`, 31 rows.

### q50 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_returns | sr_returned_date_sk | p0.d_date_sk | 31 values, 2452123 to 2452153 | 3 → 1 (1) |

p0 = `date_dim#1 (f)`, 31 rows.

## q51

skipped: refused: unsupported: BoundedWindowAggExec: window functions (#143)

## q52

### q52 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[item (f)] join ([p0] join store_sales)` | `[p0] join ([item (f)] join store_sales)` |
| changed by | | reordered by DPhyp |
| C_out | 162,116 B | 160,172 B (plan's order 162,116 B) |

p0 = `date_dim (f)`, 30 rows.

### q52 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451850 to 2451879 | 24 → 3 (11-12,23) |

p0 = `date_dim (f)`, 30 rows.

## q53

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q54

### q54 MJ1

| | plan | DPhyp before the run | replan 1 | replan 3 |
|---|---|---|---|---|
| join tree | `[[[[agg(MJ2)] join store_sales] join customer_address] join store] join date_dim#2` | `[[[[store] join customer_address] join agg(MJ2)] join store_sales] join date_dim#2` | `[m1] join ([[agg(MJ2)] join store_sales] join date_dim#2)` | `[[[m1] join agg(MJ2)] join store_sales] join date_dim#2` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 216 rows, est 6.0 (q-error 36.03); reordered by DPhyp | the build of MJ2's join on customer_sk = c_customer_sk came out 1631 rows, est 83.4 (q-error 19.56); reordered by DPhyp |
| C_out | 18,992 B | 53 B (plan's order 18,992 B) | 92,282 B (previous order 153,327 B) | 445,143 B (previous order 4,409,911 B) |

Calls that left the order unchanged: replan 2; replan 4 (MJ1's missed build came out 12 rows, est 216.0).

m1 = `[store] join customer_address`, 216 rows.

### q54 MJ2

| | plan | replan 2 |
|---|---|---|
| join tree | `[[p0] join ([item (f)] join union(catalog_sales, web_sales))] join customer` | `[[m2] join ([p0] join union(catalog_sales, web_sales))] join customer` |
| changed by | | the build of MJ2's join on i_item_sk = item_sk came out 417 rows, est 18.1 (q-error 23.04); reordered by DPhyp |
| C_out | 4,456 B | 77,945 B (previous order 102,683 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 3 (MJ2's missed build came out 1631 rows, est 83.4).

m2 = `item (f)`, 417 rows. p0 = `date_dim#1 (f)`, 31 rows.

### q54 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 31 values, 2451149 to 2451179 | 12 → 2 (1-2) |
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 31 values, 2451149 to 2451179 | 6 → 2 (0-1) |

p0 = `date_dim#1 (f)`, 31 rows.

## q55

MJ1 kept the plan's order: `[item (f)] join ([p0] join store_sales)`.

### q55 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451484 to 2451513 | 24 → 3 (7-8,23) |

p0 = `date_dim (f)`, 30 rows.

## q56

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_bill_addr_sk = ca_address_sk came out 12660 rows, est 1872.2 (q-error 6.76).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 4750 rows, est 302.3 (q-error 15.71).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_bill_addr_sk = ca_address_sk came out 6934 rows, est 1884.0 (q-error 3.68).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 2349 rows, est 304.2 (q-error 7.72).

### q56 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[[p0] join store_sales] join customer_address#1 (f)] join item#2) semi join [item#1 (f)]` | `[[item#2 semi join [m5]] join m6] join customer_address#1 (f)` |
| changed by | | flip: `[p0] join store_sales` now builds; flip: `[[p0] join store_sales] join customer_address#1 (f)` now builds | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 23789 rows, est 1564.9 (q-error 15.20); reordered by DPhyp |
| C_out | 115,430 B | 115,430 B (plan's order 115,430 B) | 92,551 B (previous order 239,974 B) |

m5 = `item#1 (f)`, 500 rows. m6 = `[p0] join store_sales`, 23,789 rows. p0 = `date_dim#1 (f)`, 28 rows.

### q56 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#2 (f)] join ([p1] join catalog_sales)] join item#4) semi join [item#3 (f)]` | `([[[p1] join catalog_sales] join customer_address#2 (f)] join item#4) semi join [item#3 (f)]` |
| changed by | | flip: `[p1] join catalog_sales` now builds |
| C_out | 79,810 B | 79,810 B (plan's order 79,810 B) |

Calls that left the order unchanged: replan 1.

p1 = `date_dim#2 (f)`, 28 rows.

### q56 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#3 (f)] join ([p2] join web_sales)] join item#6) semi join [item#5 (f)]` | `([[[p2] join web_sales] join customer_address#3 (f)] join item#6) semi join [item#5 (f)]` |
| changed by | | flip: `[p2] join web_sales` now builds |
| C_out | 80,116 B | 80,116 B (plan's order 80,116 B) |

Calls that left the order unchanged: replan 1.

p2 = `date_dim#3 (f)`, 28 rows.

### q56 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 28 values, 2451942 to 2451969 | 24 → 2 (13,23) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 28 values, 2451942 to 2451969 | 12 → 1 (7) |
| plan as written | web_sales | ws_sold_date_sk | p2.d_date_sk | 28 values, 2451942 to 2451969 | 6 → 1 (3) |

p0 = `date_dim#1 (f)`, 28 rows. p1 = `date_dim#2 (f)`, 28 rows. p2 = `date_dim#3 (f)`, 28 rows.

## q57

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q58

### q58 MJ1

| | plan | replan 1 | replan 2 |
|---|---|---|---|
| join tree | `[[agg(MJ2)] join agg(MJ3)] join agg(MJ4)` | `[[agg(MJ3)] join agg(MJ2)] join agg(MJ4)` | `[[agg(MJ4)] join agg(MJ3)] join agg(MJ2)` |
| changed by | | the build of MJ2's join on cs_item_sk = i_item_sk came out 7739 rows, est 469.3 (q-error 16.49); flip: `agg(MJ3)` now builds | the build of MJ3's join on ss_item_sk = i_item_sk came out 20063 rows, est 471.1 (q-error 42.59); reordered by DPhyp |
| C_out | 1,816 B | 6,225 B (previous order 6,225 B) | 31,125 B (previous order 87,519 B) |

Calls that left the order unchanged: DPhyp before the run; replan 3.

### q58 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([date_dim#3] join ([item#1] join catalog_sales)) semi join [[date_dim#1 (f)] join date_dim#2]` | `[[date_dim#3 semi join [[date_dim#1 (f)] join date_dim#2]] join catalog_sales] join item#1` |
| changed by | | reordered by DPhyp |
| C_out | 122,370,771 B | 333,126 B (plan's order 122,370,771 B) |

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 7739 rows, est 469.3); replan 2; replan 3.

### q58 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([date_dim#6] join ([item#2] join store_sales)) semi join [[date_dim#4 (f)] join date_dim#5]` | `[[date_dim#6 semi join [[date_dim#4 (f)] join date_dim#5]] join store_sales] join item#2` |
| changed by | | reordered by DPhyp |
| C_out | 244,514,521 B | 668,095 B (plan's order 244,514,521 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ3's missed build came out 20063 rows, est 471.1); replan 3.

### q58 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([date_dim#9] join ([item#3] join web_sales)) semi join [[date_dim#7 (f)] join date_dim#8]` | `[[date_dim#9 semi join [[date_dim#7 (f)] join date_dim#8]] join web_sales] join item#3` |
| changed by | | reordered by DPhyp |
| C_out | 61,067,787 B | 166,922 B (plan's order 61,067,787 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3 (MJ4's missed build came out 5574 rows, est 471.1).

### q58 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451541, 2451542, 2451543, 2451544, 2451545, 2451546, 2451547 | 12 → 1 (4) |
| after join order | store_sales | ss_sold_date_sk | p1.d_date_sk | 2451541, 2451542, 2451543, 2451544, 2451545, 2451546, 2451547 | 24 → 2 (8,23) |
| after join order | web_sales | ws_sold_date_sk | p2.d_date_sk | 2451541, 2451542, 2451543, 2451544, 2451545, 2451546, 2451547 | 6 → 1 (2) |

p0 = `date_dim#3 semi join [[date_dim#1 (f)] join date_dim#2]`, 7 rows, holding MJ2. p1 = `date_dim#6 semi join [[date_dim#4 (f)] join date_dim#5]`, 7 rows, holding MJ3. p2 = `date_dim#9 semi join [[date_dim#7 (f)] join date_dim#8]`, 7 rows, holding MJ4.

## q59

MJ2 kept the plan's order: `[date_dim#1] join store_sales#1`.

MJ4 kept the plan's order: `[date_dim#3] join store_sales#2`.

### q59 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[store#1] join agg(MJ2)] join date_dim#2 (f)] join MJ3` | `[[store#1] join ([date_dim#2 (f)] join agg(MJ2))] join MJ3` |
| changed by | | reordered by DPhyp |
| C_out | 10,245,780 B | 1,167,544 B (plan's order 10,245,780 B) |

### q59 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[store#2] join agg(MJ4)] join date_dim#4 (f)` | `[store#2] join ([date_dim#4 (f)] join agg(MJ4))` |
| changed by | | reordered by DPhyp |
| C_out | 9,145,825 B | 591,998 B (plan's order 9,145,825 B) |

## q60

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_bill_addr_sk = ca_address_sk came out 30788 rows, est 2006.1 (q-error 15.35).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 11552 rows, est 323.9 (q-error 35.66).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_bill_addr_sk = ca_address_sk came out 15629 rows, est 2018.5 (q-error 7.74).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 5932 rows, est 325.9 (q-error 18.20).

### q60 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[[p0] join store_sales] join customer_address#1 (f)] join item#2) semi join [item#1 (f)]` | `[item#2 semi join [m5]] join ([customer_address#1 (f)] join m6)` |
| changed by | | flip: `[p0] join store_sales` now builds; flip: `[[p0] join store_sales] join customer_address#1 (f)` now builds | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 60850 rows, est 3596.9 (q-error 16.92); reordered by DPhyp; flip: `customer_address#1 (f)` now builds |
| C_out | 213,135 B | 213,135 B (plan's order 213,135 B) | 406,513 B (previous order 663,776 B) |

Calls that left the order unchanged: replan 2 (MJ1's missed build came out 18246 rows, est 8074.0).

m5 = `item#1 (f)`, 1,860 rows. m6 = `[p0] join store_sales`, 60,850 rows. p0 = `date_dim#1 (f)`, 30 rows.

### q60 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#2 (f)] join ([p1] join catalog_sales)] join item#4) semi join [item#3 (f)]` | `([[[p1] join catalog_sales] join customer_address#2 (f)] join item#4) semi join [item#3 (f)]` |
| changed by | | flip: `[p1] join catalog_sales` now builds |
| C_out | 86,989 B | 86,989 B (plan's order 86,989 B) |

Calls that left the order unchanged: replan 1; replan 2.

p1 = `date_dim#2 (f)`, 30 rows.

### q60 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#3 (f)] join ([p2] join web_sales)] join item#6) semi join [item#5 (f)]` | `([[[p2] join web_sales] join customer_address#3 (f)] join item#6) semi join [item#5 (f)]` |
| changed by | | flip: `[p2] join web_sales` now builds |
| C_out | 87,322 B | 87,322 B (plan's order 87,322 B) |

Calls that left the order unchanged: replan 1; replan 2.

p2 = `date_dim#3 (f)`, 30 rows.

### q60 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451058 to 2451087 | 24 → 3 (1-2,23) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 30 values, 2451058 to 2451087 | 12 → 1 (1) |
| plan as written | web_sales | ws_sold_date_sk | p2.d_date_sk | 30 values, 2451058 to 2451087 | 6 → 1 (0) |

p0 = `date_dim#1 (f)`, 30 rows. p1 = `date_dim#2 (f)`, 30 rows. p2 = `date_dim#3 (f)`, 30 rows.

## q61

### q61 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[item#1 (f)] join ([[[p0] join ([promotion (f)] join ([store#1 (f)] join store_sales#1))] join customer#1] join customer_address#1 (f))` | `[store#1 (f)] join ([[[[item#1 (f)] join ([p0] join store_sales#1)] join promotion (f)] join customer#1] join customer_address#1 (f))` | `[m2] join ([promotion (f)] join ([[customer_address#1 (f)] join customer#1] join m3))` | `[m2] join ([[[m4] join m3] join customer#1] join m5)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_promo_sk = p_promo_sk came out 8435 rows, est 358.6 (q-error 23.52); reordered by DPhyp | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 20,723,026 B | 276,733 B (plan's order 20,723,026 B) | 202,339 B (previous order 295,571 B) | 353,101 B (previous order 461,604 B) |

m2 = `store#1 (f)`, 12 rows. m3 = `[item#1 (f)] join ([p0] join store_sales#1)`, 8,435 rows. m4 = `promotion (f)`, 158 rows. m5 = `customer_address#1 (f)`, 18,246 rows. p0 = `date_dim#1 (f)`, 30 rows.

### q61 MJ2

| | plan | DPhyp before the run | replan 3 | replan 4 |
|---|---|---|---|---|
| join tree | `[item#2 (f)] join ([customer_address#2 (f)] join ([[p1] join ([store#2 (f)] join store_sales#2)] join customer#2))` | `[store#2 (f)] join ([[[item#2 (f)] join ([p1] join store_sales#2)] join customer#2] join customer_address#2 (f))` | `[m7] join ([[customer_address#2 (f)] join customer#2] join m8)` | `[m7] join ([[m8] join customer#2] join m9)` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 8428 rows, est 358.7 (q-error 23.50); reordered by DPhyp | the build of MJ2's join on ca_address_sk = c_current_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 12,486,552 B | 234,598 B (plan's order 12,486,552 B) | 186,151 B (previous order 327,805 B) | 397,032 B (previous order 420,673 B) |

Calls that left the order unchanged: replan 1; replan 2.

m7 = `store#2 (f)`, 12 rows. m8 = `[item#2 (f)] join ([p1] join store_sales#2)`, 8,428 rows. m9 = `customer_address#2 (f)`, 18,246 rows. p1 = `date_dim#2 (f)`, 30 rows.

### q61 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 30 values, 2451119 to 2451148 | 24 → 3 (2-3,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 30 values, 2451119 to 2451148 | 24 → 3 (2-3,23) |

p0 = `date_dim#1 (f)`, 30 rows. p1 = `date_dim#2 (f)`, 30 rows.

## q62

### q62 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[date_dim (f)] join ([web_site] join ([ship_mode] join ([warehouse] join web_sales)))` | `[ship_mode] join ([web_site] join ([warehouse] join ([date_dim (f)] join web_sales)))` |
| changed by | | reordered by DPhyp |
| C_out | 91,205,099 B | 21,335,441 B (plan's order 91,205,099 B) |

## q63

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q64

MJ3 kept the plan's order: `[catalog_returns#1] join catalog_sales#1`.

MJ5 kept the plan's order: `[catalog_returns#2] join catalog_sales#2`.

### q64 MJ1

| | plan | replan 3 |
|---|---|---|
| join tree | `[agg(MJ2)] join agg(MJ4)` | `[agg(MJ4)] join m12` |
| changed by | | the build of MJ4's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); flip: `agg(MJ4)` now builds |
| C_out | 423 B | 324 B (previous order 324 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 2; replan 4.

m12 = `agg(MJ2)`, 37 rows.

### q64 MJ2

| | plan | DPhyp before the run | inside p2, replan 1 | inside p2, replan 2 | replan 1 | replan 2 |
|---|---|---|---|---|---|---|
| join tree | `[[[[[[[[[[[[[[[[agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)] join date_dim#1 (f)] join store#1] join customer#1] join date_dim#2] join date_dim#3] join customer_demographics#1] join customer_demographics#2] join promotion#1] join household_demographics#1] join household_demographics#2] join customer_address#1] join customer_address#2] join income_band#1] join income_band#2] join item#1 (f)` | `[item#1 (f)] join ([income_band#2] join ([income_band#1] join ([[[[[promotion#1] join ([[[[[[store#1] join ([date_dim#1 (f)] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1] join date_dim#2] join date_dim#3] join customer_demographics#1] join customer_demographics#2)] join household_demographics#1] join household_demographics#2] join customer_address#1] join customer_address#2)))` | `[[[[m2] join ([[p0] join ([store_returns#1] join store_sales#1)] join m3)] join customer#1] join date_dim#2] join date_dim#3` | `[[date_dim#3] join ([date_dim#2] join customer#1)] join ([m2] join ([m3] join m4))` | `[[[income_band#1] join ([[income_band#2] join ([[[[m8] join p4] join customer_demographics#2] join promotion#1] join household_demographics#2)] join household_demographics#1)] join customer_address#1] join customer_address#2` | `[[[m9] join ([[m10] join ([[promotion#1] join m11] join household_demographics#2)] join household_demographics#1)] join customer_address#1] join customer_address#2` |
| changed by | | DPhyp at its budget, the plan's order kept; flip: `date_dim#1 (f)` now builds; flip: `store#1` now builds; flip: `promotion#1` now builds; flip: `income_band#1` now builds; flip: `income_band#2` now builds; flip: `item#1 (f)` now builds | the build of MJ2's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); reordered by DPhyp | the build of MJ2's join on ss_item_sk = cs_item_sk came out 54481 rows, est 12273.7 (q-error 4.44); reordered by DPhyp; flip: `m3` now builds | the build of MJ2's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); reordered by DPhyp | the build of MJ2's join on ss_promo_sk = p_promo_sk came out 37 rows, est 6.4 (q-error 5.82); flip: `promotion#1` now builds |
| C_out | 51,700,850 B | 51,700,850 B (plan's order 51,700,850 B) | 18,024,522 B (previous order 23,730,821 B) | 31,072,501 B (previous order 38,238,852 B) | 13,646 B (previous order 12,085,728 B) | 40,912 B (previous order 40,912 B) |

m2 = `store#1`, 12 rows. m3 = `agg(MJ3) (f)`, 17,157 rows. m4 = `[p0] join ([store_returns#1] join store_sales#1)`, 54,481 rows. m8 = `item#1 (f)`, 12 rows. m9 = `income_band#1`, 20 rows. m10 = `income_band#2`, 20 rows. m11 = `[[m8] join p4] join customer_demographics#2`, 37 rows. p0 = `date_dim#1 (f)`, 365 rows. p2 = `[[[[store#1] join ([p0] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1] join date_dim#2] join date_dim#3`, 48,301 rows. p4 = `[p2] join customer_demographics#1`, 47,848 rows.

### q64 MJ4

| | plan | DPhyp before the run | inside p3, replan 1 | inside p3, replan 2 | replan 1 |
|---|---|---|---|---|---|
| join tree | `[[[[[[[[[[[[[[[[agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)] join date_dim#4 (f)] join store#2] join customer#2] join date_dim#5] join date_dim#6] join customer_demographics#3] join customer_demographics#4] join promotion#2] join household_demographics#3] join household_demographics#4] join customer_address#3] join customer_address#4] join income_band#3] join income_band#4] join item#2 (f)` | `[item#2 (f)] join ([income_band#4] join ([income_band#3] join ([[[[[promotion#2] join ([[[[[[store#2] join ([date_dim#4 (f)] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2] join date_dim#5] join date_dim#6] join customer_demographics#3] join customer_demographics#4)] join household_demographics#3] join household_demographics#4] join customer_address#3] join customer_address#4)))` | `[[[[m5] join ([[p1] join ([store_returns#2] join store_sales#2)] join m6)] join customer#2] join date_dim#5] join date_dim#6` | `[[date_dim#6] join ([date_dim#5] join customer#2)] join ([m5] join ([m6] join m7))` | `[[[income_band#3] join ([[income_band#4] join ([[[[item#2 (f)] join p5] join customer_demographics#4] join promotion#2] join household_demographics#4)] join household_demographics#3)] join customer_address#3] join customer_address#4` |
| changed by | | DPhyp at its budget, the plan's order kept; flip: `date_dim#4 (f)` now builds; flip: `store#2` now builds; flip: `promotion#2` now builds; flip: `income_band#3` now builds; flip: `income_band#4` now builds; flip: `item#2 (f)` now builds | the build of MJ4's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); reordered by DPhyp | the build of MJ4's join on ss_item_sk = cs_item_sk came out 55224 rows, est 14768.8 (q-error 3.74); reordered by DPhyp; flip: `m6` now builds | the build of MJ2's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); reordered by DPhyp |
| C_out | 51,700,850 B | 51,700,850 B (plan's order 51,700,850 B) | 21,371,263 B (previous order 28,121,452 B) | 31,363,293 B (previous order 38,760,345 B) | 122,484 B (previous order 12,337,256 B) |

Calls that left the order unchanged: replan 2; replan 3 (MJ4's missed build came out 12 rows, est 105.6); replan 4 (MJ4's missed build came out 3 rows, est 32.5).

m5 = `store#2`, 12 rows. m6 = `agg(MJ5) (f)`, 17,157 rows. m7 = `[p1] join ([store_returns#2] join store_sales#2)`, 55,224 rows. p1 = `date_dim#4 (f)`, 366 rows. p3 = `[[[[store#2] join ([p1] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2] join date_dim#5] join date_dim#6`, 49,247 rows. p5 = `[p3] join customer_demographics#3`, 48,803 rows.

### q64 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2451544 | 24 → 6 (4-8,23) |
| after join order | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |
| after join order | customer_demographics#1 | cd_demo_sk | p2.ss_cdemo_sk | 37 to 1920795 | 16 → 16 (0-15) |
| after join order | customer_demographics#3 | cd_demo_sk | p3.ss_cdemo_sk | 32 to 1920615 | 16 → 16 (0-15) |
| after join order | customer_demographics#2 | cd_demo_sk | p4.c_current_cdemo_sk | 17 to 1920750 | 16 → 16 (0-15) |
| after join order | customer_demographics#4 | cd_demo_sk | p5.c_current_cdemo_sk | 116 to 1920750 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#4 (f)`, 366 rows. p2 = `[[[[store#1] join ([p0] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1] join date_dim#2] join date_dim#3`, 48,301 rows, holding MJ2, MJ3. p3 = `[[[[store#2] join ([p1] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2] join date_dim#5] join date_dim#6`, 49,247 rows, holding MJ4, MJ5. p4 = `[p2] join customer_demographics#1`, 47,848 rows, holding MJ2. p5 = `[p3] join customer_demographics#3`, 48,803 rows, holding MJ4.

Probe p2, replan 1: the build of MJ2's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); m2 = `store#1`, 12 rows; m3 = `agg(MJ3) (f)`, 17,157 rows.

Probe p2, replan 2: the build of MJ2's join on ss_item_sk = cs_item_sk came out 54481 rows, est 12273.7 (q-error 4.44); m4 = `[p0] join ([store_returns#1] join store_sales#1)`, 54,481 rows.

Probe p3, replan 1: the build of MJ4's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); m5 = `store#2`, 12 rows; m6 = `agg(MJ5) (f)`, 17,157 rows.

Probe p3, replan 2: the build of MJ4's join on ss_item_sk = cs_item_sk came out 55224 rows, est 14768.8 (q-error 3.74); m7 = `[p1] join ([store_returns#2] join store_sales#2)`, 55,224 rows.

## q65

MJ2 kept the plan's order: `[p0] join store_sales#1`.

MJ3 kept the plan's order: `[p1] join store_sales#2`.

### q65 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[store] join agg(MJ2)] join item] join agg(MJ3)` | `[[store] join ([agg(MJ3)] join agg(MJ2))] join item` |
| changed by | | reordered by DPhyp |
| C_out | 28,819,239 B | 5,327,456 B (plan's order 28,819,239 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 121 rows, est 20264.4).

### q65 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2450815 to 2451179 | 24 → 6 (0-4,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2450815 to 2451179 | 24 → 6 (0-4,23) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows.

## q66

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_sold_time_sk = t_time_sk came out 28650 rows, est 9739.2 (q-error 2.94).

### q66 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[ship_mode#1 (f)] join ([time_dim#1 (f)] join ([p0] join ([warehouse#1] join web_sales)))` | `[warehouse#1] join ([[p0] join ([ship_mode#1 (f)] join web_sales)] join time_dim#1 (f))` | `[m3] join ([time_dim#1 (f)] join m4)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ws_sold_time_sk = t_time_sk came out 14670 rows, est 4911.5 (q-error 2.99); flip: `time_dim#1 (f)` now builds |
| C_out | 46,601,960 B | 2,284,031 B (plan's order 46,601,960 B) | 999,635 B (previous order 999,635 B) |

m3 = `warehouse#1`, 5 rows. m4 = `[p0] join ([ship_mode#1 (f)] join web_sales)`, 14,670 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q66 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[ship_mode#2 (f)] join ([time_dim#2 (f)] join ([p1] join ([warehouse#2] join catalog_sales)))` | `[warehouse#2] join ([[p1] join ([ship_mode#2 (f)] join catalog_sales)] join time_dim#2 (f))` |
| changed by | | reordered by DPhyp |
| C_out | 93,123,090 B | 4,562,501 B (plan's order 93,123,090 B) |

Calls that left the order unchanged: replan 1.

p1 = `date_dim#2 (f)`, 365 rows.

### q66 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 6 → 2 (3-4) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 2451911 to 2452275 | 12 → 4 (6-9) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows.

## q67

skipped: refused: unsupported: BoundedWindowAggExec: window functions (#143)

## q68

### q68 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[customer] join agg(MJ2)] join customer_address#2` | `[[agg(MJ2)] join customer] join customer_address#2` |
| changed by | | flip: `agg(MJ2)` now builds |
| C_out | 1,775,170 B | 1,775,170 B (plan's order 1,775,170 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 2286 rows, est 14521.2).

### q68 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#1] join ([household_demographics (f)] join ([store (f)] join ([p0] join store_sales)))` | `[[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))] join customer_address#1` |
| changed by | | reordered by DPhyp; flip: `[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))` now builds |
| C_out | 14,654,679 B | 10,410,698 B (plan's order 14,654,679 B) |

p0 = `date_dim (f)`, 72 rows.

### q68 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2452246 | 24 → 15 (4-17,23) |

p0 = `date_dim (f)`, 72 rows.

## q69

### q69 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[[[customer_address (f)] join customer] join customer_demographics] semi join ([p0] join store_sales)] anti join ([p1] join web_sales)] anti join ([p2] join catalog_sales)` | `[[[[[customer_address (f)] join customer] semi join ([p0] join store_sales)] join customer_demographics] anti join ([p1] join web_sales)] anti join ([p2] join catalog_sales)` |
| changed by | | reordered by DPhyp |
| C_out | 625,351 B | 340,993 B (plan's order 625,351 B) |

p0 = `date_dim#1 (f)`, 91 rows. p1 = `date_dim#2 (f)`, 91 rows. p2 = `date_dim#3 (f)`, 91 rows.

### q69 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2452001 to 2452091 | 24 → 2 (14,23) |
| plan as written | web_sales | ws_sold_date_sk | p1.d_date_sk | 2452001 to 2452091 | 6 → 1 (3) |
| plan as written | catalog_sales | cs_sold_date_sk | p2.d_date_sk | 2452001 to 2452091 | 12 → 1 (7) |
| after join order | customer_demographics | cd_demo_sk | p3.c_current_cdemo_sk | 306 to 1918272 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 91 rows. p1 = `date_dim#2 (f)`, 91 rows. p2 = `date_dim#3 (f)`, 91 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 625 rows, holding MJ1.

## q70

skipped: refused by datafusion: This feature is not implemented: Physical plan does not support logical expression AggregateFunction(AggregateFunction { func: AggregateUDF { inner: Grouping { name: "grouping", signature: Signature { type_signature: VariadicAny, volatility: Immutable } } }, args: [Column(Column { relation: Some(Bare { table: "store" }), name: "s_state" })], distinct: false, filter: None, order_by: None, null_treatment: None })

## q71

MJ1 kept the plan's order: `[[item (f)] join union(MJ2, MJ3, MJ4)] join time_dim (f)`.

MJ2 kept the plan's order: `[p0] join web_sales`.

MJ3 kept the plan's order: `[p1] join catalog_sales`.

MJ4 kept the plan's order: `[p2] join store_sales`.

### q71 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 30 values, 2451484 to 2451513 | 6 → 2 (1-2) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 30 values, 2451484 to 2451513 | 12 → 2 (3-4) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 30 values, 2451484 to 2451513 | 24 → 3 (7-8,23) |

p0 = `date_dim#1 (f)`, 30 rows. p1 = `date_dim#2 (f)`, 30 rows. p2 = `date_dim#3 (f)`, 30 rows.

## q72

skipped: refused by datafusion: type_coercion

## q73

### q73 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer] join agg(MJ2) (f)` | `[agg(MJ2) (f)] join customer` |
| changed by | | flip: `agg(MJ2) (f)` now builds |
| C_out | 27,003 B | 27,003 B (plan's order 27,003 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ1's missed build came out 1 rows, est 1744.5).

### q73 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[household_demographics (f)] join ([store (f)] join ([p0] join store_sales))` | `[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))` |
| changed by | | reordered by DPhyp |
| C_out | 4,029,948 B | 2,400,282 B (plan's order 4,029,948 B) |

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 1040 rows, est 320.0).

p0 = `date_dim (f)`, 72 rows.

### q73 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2452246 | 24 → 15 (4-17,23) |

p0 = `date_dim (f)`, 72 rows.

## q74

### q74 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` | `[[[agg(MJ2) (f)] join agg(MJ5)] join agg(MJ4) (f)] join agg(MJ3)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ4's join on ws_bill_customer_sk = c_customer_sk came out 147010 rows, est 49119.4 (q-error 2.99); reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); reordered by DPhyp |
| C_out | 3,906,779 B | 1,301,636 B (plan's order 3,906,779 B) | 3,095,058 B (previous order 3,156,970 B) | 5,876,235 B (previous order 7,167,850 B) |

Calls that left the order unchanged: replan 3 (MJ1's missed build came out 11552 rows, est 29402.0); replan 4 (MJ1's missed build came out 4414 rows, est 17710.6); replan 5; replan 6 (MJ1's missed build came out 485 rows, est 6289.4); replan 7.

### q74 MJ2

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[[p0] join store_sales#1] join customer#1` | `[customer#1] join m5` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 533427 rows, est 115175.4 (q-error 4.63); flip: `customer#1` now builds |
| C_out | 51,478,017 B | 12,902,178 B (plan's order 51,478,017 B) | 34,230,496 B (previous order 34,230,496 B) |

Calls that left the order unchanged: replan 1; replan 3.

m5 = `[p0] join store_sales#1`, 533,427 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q74 MJ3

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[[p1] join store_sales#2] join customer#2` | `[customer#2] join m10` |
| changed by | | reordered by DPhyp | the build of MJ3's join on ss_customer_sk = c_customer_sk came out 536691 rows, est 118806.9 (q-error 4.52); flip: `customer#2` now builds |
| C_out | 60,939,517 B | 15,273,558 B (plan's order 60,939,517 B) | 34,439,950 B (previous order 34,439,950 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

m10 = `[p1] join store_sales#2`, 536,691 rows. p1 = `date_dim#2 (f)`, 365 rows.

### q74 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p2] join ([customer#3] join web_sales#1)` | `[[p2] join web_sales#1] join customer#3` |
| changed by | | reordered by DPhyp |
| C_out | 18,923,001 B | 4,742,761 B (plan's order 18,923,001 B) |

Calls that left the order unchanged: replan 1 (MJ4's missed build came out 147010 rows, est 49119.4); replan 2.

p2 = `date_dim#3 (f)`, 365 rows.

### q74 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p3] join ([customer#4] join web_sales#2)` | `[[p3] join web_sales#2] join customer#4` |
| changed by | | reordered by DPhyp |
| C_out | 17,545,047 B | 4,397,398 B (plan's order 17,545,047 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5 (MJ5's missed build came out 142487 rows, est 45487.5).

p3 = `date_dim#4 (f)`, 365 rows.

### q74 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2452276 to 2452640 | 24 → 7 (17-23) |
| plan as written | web_sales#1 | ws_sold_date_sk | p2.d_date_sk | 2451911 to 2452275 | 6 → 2 (3-4) |
| plan as written | web_sales#2 | ws_sold_date_sk | p3.d_date_sk | 2452276 to 2452640 | 6 → 2 (4-5) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows. p2 = `date_dim#3 (f)`, 365 rows. p3 = `date_dim#4 (f)`, 365 rows.

## q75

MJ1 kept the plan's order: `[agg(union(MJ2, MJ3, MJ4))] join agg(union(MJ5, MJ6, MJ7))`.

MJ2 kept the plan's order: `[[p0] join ([item#1 (f)] join catalog_sales#1)] left join catalog_returns#1`.

MJ3 kept the plan's order: `[[p1] join ([item#2 (f)] join store_sales#1)] left join store_returns#1`.

MJ4 kept the plan's order: `[[p2] join ([item#3 (f)] join web_sales#1)] left join web_returns#1`.

MJ5 kept the plan's order: `[[p3] join ([item#4 (f)] join catalog_sales#2)] left join catalog_returns#2`.

MJ6 kept the plan's order: `[[p4] join ([item#5 (f)] join store_sales#2)] left join store_returns#2`.

MJ7 kept the plan's order: `[[p5] join ([item#6 (f)] join web_sales#2)] left join web_returns#2`.

A replan was refused, work outside its builds having started: the build of MJ3's left join on ss_ticket_number = sr_ticket_number and ss_item_sk = sr_item_sk came out 52823 rows, est 13192.9 (q-error 4.00).

A replan was refused, work outside its builds having started: the build of MJ4's left join on ws_order_number = wr_order_number and ws_item_sk = wr_item_sk came out 13736 rows, est 4540.2 (q-error 3.03).

A replan was refused, work outside its builds having started: the build of MJ6's left join on ss_ticket_number = sr_ticket_number and ss_item_sk = sr_item_sk came out 53812 rows, est 12251.8 (q-error 4.39).

A replan was refused, work outside its builds having started: the build of MJ7's left join on ws_order_number = wr_order_number and ws_item_sk = wr_item_sk came out 14416 rows, est 4900.7 (q-error 2.94).

### q75 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales#1 | cs_sold_date_sk | p0.d_date_sk | 2452276 to 2452640 | 12 → 3 (9-11) |
| plan as written | store_sales#1 | ss_sold_date_sk | p1.d_date_sk | 2452276 to 2452640 | 24 → 7 (17-23) |
| plan as written | web_sales#1 | ws_sold_date_sk | p2.d_date_sk | 2452276 to 2452640 | 6 → 2 (4-5) |
| plan as written | catalog_sales#2 | cs_sold_date_sk | p3.d_date_sk | 2451911 to 2452275 | 12 → 4 (6-9) |
| plan as written | store_sales#2 | ss_sold_date_sk | p4.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| plan as written | web_sales#2 | ws_sold_date_sk | p5.d_date_sk | 2451911 to 2452275 | 6 → 2 (3-4) |
| after join order | catalog_returns#1 | cr_order_number | p6.cs_order_number | 127737 to 159626 | 2 → 2 (0-1) |
| after join order | catalog_returns#2 | cr_order_number | p7.cs_order_number | 95844 to 127734 | 2 → 1 (0) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows. p2 = `date_dim#3 (f)`, 365 rows. p3 = `date_dim#4 (f)`, 365 rows. p4 = `date_dim#5 (f)`, 365 rows. p5 = `date_dim#6 (f)`, 365 rows. p6 = `[p0] join ([item#1 (f)] join catalog_sales#1)`, 27,388 rows, holding MJ2. p7 = `[p3] join ([item#4 (f)] join catalog_sales#2)`, 28,074 rows, holding MJ5.

## q76

A replan was refused, work outside its builds having started: the build of MJ2's join on ws_sold_date_sk = d_date_sk came out 81 rows, est 170.0 (q-error 2.10).

A replan was refused, work outside its builds having started: the build of MJ3's join on cs_sold_date_sk = d_date_sk came out 3520 rows, est 7086.5 (q-error 2.01).

### q76 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[date_dim#1] join ([item#1] join store_sales (f))` | `[[item#1] join store_sales (f)] join date_dim#1` |
| changed by | | flip: `[item#1] join store_sales (f)` now builds |
| C_out | 12,025,223 B | 12,025,223 B (plan's order 12,025,223 B) |

### q76 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[date_dim#2] join ([item#2] join web_sales (f))` | `[[web_sales (f)] join item#2] join date_dim#2` |
| changed by | | flip: `web_sales (f)` now builds; flip: `[web_sales (f)] join item#2` now builds |
| C_out | 15,791 B | 15,791 B (plan's order 15,791 B) |

### q76 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[date_dim#3] join ([item#3] join catalog_sales (f))` | `[[catalog_sales (f)] join item#3] join date_dim#3` |
| changed by | | flip: `catalog_sales (f)` now builds; flip: `[catalog_sales (f)] join item#3` now builds |
| C_out | 661,540 B | 661,540 B (plan's order 661,540 B) |

## q77

MJ1 kept the plan's order: `agg(MJ3) left join [agg(MJ2)]`.

MJ2 kept the plan's order: `[store#1] join ([p0] join store_returns)`.

MJ3 kept the plan's order: `[store#2] join ([p1] join store_sales)`.

MJ4 kept the plan's order: `[p2] join catalog_returns`.

MJ5 kept the plan's order: `[p3] join catalog_sales`.

MJ6 kept the plan's order: `agg(MJ8) left join [agg(MJ7)]`.

MJ7 kept the plan's order: `[web_page#1] join ([date_dim#5 (f)] join web_returns)`.

MJ8 kept the plan's order: `[web_page#2] join ([p4] join web_sales)`.

### q77 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_returns | sr_returned_date_sk | p0.d_date_sk | 31 values, 2451780 to 2451810 | 3 → 1 (1) |
| plan as written | store_sales | ss_sold_date_sk | p1.d_date_sk | 31 values, 2451780 to 2451810 | 24 → 3 (10-11,23) |
| plan as written | catalog_returns | cr_returned_date_sk | p2.d_date_sk | 31 values, 2451780 to 2451810 | 2 → 1 (0) |
| plan as written | catalog_sales | cs_sold_date_sk | p3.d_date_sk | 31 values, 2451780 to 2451810 | 12 → 1 (5) |
| plan as written | web_sales | ws_sold_date_sk | p4.d_date_sk | 31 values, 2451780 to 2451810 | 6 → 1 (2) |

p0 = `date_dim#1 (f)`, 31 rows. p1 = `date_dim#2 (f)`, 31 rows. p2 = `date_dim#3 (f)`, 31 rows. p3 = `date_dim#4 (f)`, 31 rows. p4 = `date_dim#6 (f)`, 31 rows.

## q78

MJ2 kept the plan's order: `[date_dim#1] join MJ3 (f)`.

MJ3 kept the plan's order: `web_sales left join [web_returns]`.

MJ4 kept the plan's order: `[p0] join MJ5 (f)`.

MJ5 kept the plan's order: `store_sales left join [store_returns]`.

MJ6 kept the plan's order: `[date_dim#3] join MJ7 (f)`.

MJ7 kept the plan's order: `catalog_sales left join [catalog_returns]`.

### q78 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[agg(MJ4) left join [agg(MJ2)]] left join agg(MJ6)` | `([agg(MJ4)] left join agg(MJ6)) left join [agg(MJ2)]` |
| changed by | | reordered by DPhyp |
| C_out | 33,335,177 B | 33,335,177 B (plan's order 33,335,177 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 493526 rows, est 133008.2).

### q78 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim#2 (f)`, 366 rows.

## q79

### q79 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer] join agg(MJ2)` | `[agg(MJ2)] join customer` |
| changed by | | flip: `agg(MJ2)` now builds |
| C_out | 3,876,341 B | 3,876,341 B (plan's order 3,876,341 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 7020 rows, est 52670.1).

### q79 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[household_demographics (f)] join ([store (f)] join ([p0] join store_sales))` | `[store (f)] join ([household_demographics (f)] join ([p0] join store_sales))` |
| changed by | | reordered by DPhyp |
| C_out | 26,722,382 B | 19,174,053 B (plan's order 26,722,382 B) |

p0 = `date_dim (f)`, 157 rows.

### q79 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451183 to 2452275 | 24 → 15 (4-17,23) |

p0 = `date_dim (f)`, 157 rows.

## q80

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 16224 rows, est 2086.1 (q-error 7.78).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 31634 rows, est 2078.1 (q-error 15.22).

### q80 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[promotion#1 (f)] join ([item#1 (f)] join ([store] join ([p0] join (store_sales left join [store_returns]))))` | `([store] join ([promotion#1 (f)] join ([[p0] join store_sales] join item#1 (f)))) left join [store_returns]` | `([m9] join ([m10] join ([item#1 (f)] join m11))) left join [m8]` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_item_sk = i_item_sk came out 63241 rows, est 4172.2 (q-error 15.16); flip: `item#1 (f)` now builds |
| C_out | 30,505,123 B | 1,016,736 B (plan's order 30,505,123 B) | 8,533,330 B (previous order 8,533,330 B) |

Calls that left the order unchanged: replan 2 (MJ1's missed build came out 1055 rows, est 8981.3).

m8 = `store_returns`, 287,867 rows. m9 = `store`, 12 rows. m10 = `promotion#1 (f)`, 294 rows. m11 = `[p0] join store_sales`, 63,241 rows. p0 = `date_dim#1 (f)`, 31 rows.

### q80 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion#2 (f)] join ([item#2 (f)] join ([catalog_page] join ([p1] join (catalog_sales left join [catalog_returns]))))` | `([[promotion#2 (f)] join ([[p1] join catalog_sales] join item#2 (f))] join catalog_page) left join [catalog_returns]` |
| changed by | | reordered by DPhyp |
| C_out | 12,495,855 B | 414,970 B (plan's order 12,495,855 B) |

Calls that left the order unchanged: replan 1; replan 2.

p1 = `date_dim#2 (f)`, 31 rows.

### q80 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion#3 (f)] join ([item#3 (f)] join ([web_site] join ([p2] join (web_sales left join [web_returns]))))` | `([web_site] join ([promotion#3 (f)] join ([[p2] join web_sales] join item#3 (f)))) left join [web_returns]` |
| changed by | | reordered by DPhyp |
| C_out | 12,498,065 B | 416,561 B (plan's order 12,498,065 B) |

Calls that left the order unchanged: replan 1; replan 2.

p2 = `date_dim#3 (f)`, 31 rows.

### q80 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 31 values, 2451780 to 2451810 | 24 → 3 (10-11,23) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 31 values, 2451780 to 2451810 | 12 → 1 (5) |
| plan as written | web_sales | ws_sold_date_sk | p2.d_date_sk | 31 values, 2451780 to 2451810 | 6 → 1 (2) |

p0 = `date_dim#1 (f)`, 31 rows. p1 = `date_dim#2 (f)`, 31 rows. p2 = `date_dim#3 (f)`, 31 rows.

## q81

MJ2 kept the plan's order: `[[p0] join catalog_returns#1] join customer_address#1`.

MJ3 kept the plan's order: `[[p1] join catalog_returns#2] join customer_address#3`.

### q81 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[agg(MJ2)] join customer] join customer_address#2 (f)] join agg(MJ3)` | `[[[agg(MJ3)] join agg(MJ2)] join customer] join customer_address#2 (f)` |
| changed by | | reordered by DPhyp |
| C_out | 1,745,656 B | 422,912 B (plan's order 1,745,656 B) |

### q81 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_returns#1 | cr_returned_date_sk | p0.d_date_sk | 2451545 to 2451910 | 2 → 1 (0) |
| plan as written | catalog_returns#2 | cr_returned_date_sk | p1.d_date_sk | 2451545 to 2451910 | 2 → 1 (0) |

p0 = `date_dim#1 (f)`, 366 rows. p1 = `date_dim#2 (f)`, 366 rows.

## q82

MJ1 kept the plan's order: `[[p0] join ([item (f)] join inventory (f))] join store_sales`.

### q82 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory | inv_date_sk | p0.d_date_sk | 2451690 to 2451750 | 96 → 5 (45-49) |

p0 = `date_dim (f)`, 61 rows.

## q83

### q83 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[agg(MJ2)] join agg(MJ3)] join agg(MJ4)` | `[[agg(MJ4)] join agg(MJ3)] join agg(MJ2)` |
| changed by | | reordered by DPhyp |
| C_out | 2,382 B | 1,591 B (plan's order 2,382 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 196 rows, est 667.6).

### q83 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([date_dim#3] join ([item#1] join catalog_returns)) semi join [date_dim#2 semi join [date_dim#1 (f)]]` | `[[date_dim#3 semi join [date_dim#2 semi join [date_dim#1 (f)]]] join catalog_returns] join item#1` |
| changed by | | reordered by DPhyp |
| C_out | 9,945,954 B | 65,253 B (plan's order 9,945,954 B) |

Calls that left the order unchanged: replan 1.

### q83 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([date_dim#6] join ([item#2] join store_returns)) semi join [date_dim#5 semi join [date_dim#4 (f)]]` | `[[date_dim#6 semi join [date_dim#5 semi join [date_dim#4 (f)]]] join store_returns] join item#2` |
| changed by | | reordered by DPhyp |
| C_out | 19,876,190 B | 134,540 B (plan's order 19,876,190 B) |

### q83 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[item#3] join web_returns] join date_dim#7] semi join (date_dim#9 semi join [date_dim#8 (f)])` | `[[[date_dim#7] semi join (date_dim#9 semi join [date_dim#8 (f)])] join web_returns] join item#3` |
| changed by | | reordered by DPhyp |
| C_out | 4,946,040 B | 31,353 B (plan's order 4,946,040 B) |

### q83 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | store_returns | sr_returned_date_sk | p0.d_date_sk | 21 values, 2451723 to 2451869 | 3 → 2 (0-1) |
| after join order | catalog_returns | cr_returned_date_sk | p1.d_date_sk | 21 values, 2451723 to 2451869 | 2 → 1 (0) |

p0 = `date_dim#6 semi join [date_dim#5 semi join [date_dim#4 (f)]]`, 21 rows, holding MJ3. p1 = `date_dim#3 semi join [date_dim#2 semi join [date_dim#1 (f)]]`, 21 rows, holding MJ2.

## q84

### q84 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[income_band (f)] join ([household_demographics] join ([[customer_address (f)] join customer] join customer_demographics))] join store_returns` | `[income_band (f)] join ([[[[customer_address (f)] join customer] join customer_demographics] join store_returns] join household_demographics)` |
| changed by | | reordered by DPhyp |
| C_out | 26,198 B | 17,980 B (plan's order 26,198 B) |

Calls that left the order unchanged: inside p0, replan 1 (MJ1's missed build came out 464 rows, est 69.8).

p0 = `[customer_address (f)] join customer`, 913 rows.

### q84 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | customer_demographics | cd_demo_sk | p0.c_current_cdemo_sk | 3781 to 1915892 | 16 → 16 (0-15) |

p0 = `[customer_address (f)] join customer`, 913 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 464 rows, est 69.8 (q-error 6.65); m0 = `customer_address (f)`, 464 rows.

## q85

### q85 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[reason] join ([p0] join ([customer_address (f)] join ([[[web_page] join ([web_returns] join web_sales (f))] join customer_demographics#1 (f)] join customer_demographics#2)))` | `[[[[[[[web_sales (f)] join web_returns] join customer_demographics#1 (f)] join customer_address (f)] join p0] join customer_demographics#2] join web_page] join reason` |
| changed by | | reordered by DPhyp; flip: `web_sales (f)` now builds; flip: `[[[[[[web_sales (f)] join web_returns] join customer_demographics#1 (f)] join customer_address (f)] join p0] join customer_demographics#2] join web_page` now builds |
| C_out | 38,730 B | 20,115 B (plan's order 38,730 B) |

Calls that left the order unchanged: inside p1, replan 1 (MJ1's missed build came out 8678 rows, est 1637.0); inside p2, replan 1 (MJ1's missed build came out 21 rows, est 1.0).

p0 = `date_dim (f)`, 366 rows. p1 = `[web_sales (f)] join web_returns`, 803 rows. p2 = `[[[p1] join customer_demographics#1 (f)] join customer_address (f)] join p0`, 1 rows.

### q85 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 6 → 2 (2-3) |
| after join order | customer_demographics#1 | cd_demo_sk | p1.wr_refunded_cdemo_sk | 1019 to 1906870 | 16 → 16 (0-15) |
| after join order | customer_demographics#2 | cd_demo_sk | p2.wr_returning_cdemo_sk | 1037564 | 16 → 1 (8) |

p0 = `date_dim (f)`, 366 rows. p1 = `[web_sales (f)] join web_returns`, 803 rows, holding MJ1. p2 = `[[[p1] join customer_demographics#1 (f)] join customer_address (f)] join p0`, 1 rows, holding MJ1.

Probe p1, replan 1: the build of MJ1's join on ws_item_sk = wr_item_sk and ws_order_number = wr_order_number came out 8678 rows, est 1637.0 (q-error 5.30); m1 = `web_sales (f)`, 8,678 rows.

Probe p2, replan 1: the build of MJ1's join on wr_refunded_addr_sk = ca_address_sk came out 21 rows, est 1.0 (q-error 21.00); m2 = `[p1] join customer_demographics#1 (f)`, 21 rows.

## q86

skipped: refused by datafusion: This feature is not implemented: Physical plan does not support logical expression AggregateFunction(AggregateFunction { func: AggregateUDF { inner: Grouping { name: "grouping", signature: Signature { type_signature: VariadicAny, volatility: Immutable } } }, args: [Column(Column { relation: Some(Bare { table: "item" }), name: "i_category" })], distinct: false, filter: None, order_by: None, null_treatment: None })

## q87

MJ1 kept the plan's order: `agg(MJ3) anti join [agg(MJ2)]`.

MJ3 kept the plan's order: `agg(MJ5) anti join [agg(MJ4)]`.

### q87 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer#1] join ([p0] join web_sales)` | `[[p0] join web_sales] join customer#1` |
| changed by | | flip: `[p0] join web_sales` now builds |
| C_out | 1,781,743 B | 1,781,743 B (plan's order 1,781,743 B) |

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 142164 rows, est 49253.8).

p0 = `date_dim#1 (f)`, 366 rows.

### q87 MJ4

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer#2] join ([p1] join catalog_sales)` | `[[p1] join catalog_sales] join customer#2` | `[customer#2] join m5` |
| changed by | | flip: `[p1] join catalog_sales` now builds | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 286899 rows, est 73433.4 (q-error 3.91); flip: `customer#2` now builds |
| C_out | 2,662,408 B | 2,662,408 B (plan's order 2,662,408 B) | 6,862,885 B (previous order 6,862,885 B) |

Calls that left the order unchanged: replan 1; replan 2.

m5 = `[p1] join catalog_sales`, 286,899 rows. p1 = `date_dim#2 (f)`, 366 rows.

### q87 MJ5

| | plan | DPhyp before the run | replan 5 |
|---|---|---|---|
| join tree | `[customer#3] join ([p2] join store_sales)` | `[[p2] join store_sales] join customer#3` | `[customer#3] join m7` |
| changed by | | flip: `[p2] join store_sales` now builds | the build of MJ5's join on ss_customer_sk = c_customer_sk came out 541109 rows, est 139514.9 (q-error 3.88); flip: `customer#3` now builds |
| C_out | 5,737,914 B | 5,737,914 B (plan's order 5,737,914 B) | 12,943,820 B (previous order 12,943,820 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4.

m7 = `[p2] join store_sales`, 541,109 rows. p2 = `date_dim#3 (f)`, 366 rows.

### q87 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 6 → 2 (2-3) |
| plan as written | catalog_sales | cs_sold_date_sk | p1.d_date_sk | 2451545 to 2451910 | 12 → 3 (4-6) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim#1 (f)`, 366 rows. p1 = `date_dim#2 (f)`, 366 rows. p2 = `date_dim#3 (f)`, 366 rows.

## q88

### q88 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#1 (f)] join ([time_dim#1 (f)] join ([household_demographics#1 (f)] join store_sales#1))` | `[store#1 (f)] join ([household_demographics#1 (f)] join ([time_dim#1 (f)] join store_sales#1))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#2 (f)] join ([time_dim#2 (f)] join ([household_demographics#2 (f)] join store_sales#2))` | `[store#2 (f)] join ([household_demographics#2 (f)] join ([time_dim#2 (f)] join store_sales#2))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#3 (f)] join ([time_dim#3 (f)] join ([household_demographics#3 (f)] join store_sales#3))` | `[store#3 (f)] join ([household_demographics#3 (f)] join ([time_dim#3 (f)] join store_sales#3))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#4 (f)] join ([time_dim#4 (f)] join ([household_demographics#4 (f)] join store_sales#4))` | `[store#4 (f)] join ([household_demographics#4 (f)] join ([time_dim#4 (f)] join store_sales#4))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#5 (f)] join ([time_dim#5 (f)] join ([household_demographics#5 (f)] join store_sales#5))` | `[store#5 (f)] join ([household_demographics#5 (f)] join ([time_dim#5 (f)] join store_sales#5))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ6

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#6 (f)] join ([time_dim#6 (f)] join ([household_demographics#6 (f)] join store_sales#6))` | `[store#6 (f)] join ([household_demographics#6 (f)] join ([time_dim#6 (f)] join store_sales#6))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ7

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#7 (f)] join ([time_dim#7 (f)] join ([household_demographics#7 (f)] join store_sales#7))` | `[store#7 (f)] join ([household_demographics#7 (f)] join ([time_dim#7 (f)] join store_sales#7))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

### q88 MJ8

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store#8 (f)] join ([time_dim#8 (f)] join ([household_demographics#8 (f)] join store_sales#8))` | `[store#8 (f)] join ([household_demographics#8 (f)] join ([time_dim#8 (f)] join store_sales#8))` |
| changed by | | reordered by DPhyp |
| C_out | 11,707,545 B | 2,087,013 B (plan's order 11,707,545 B) |

## q89

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q90

### q90 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[web_page#1 (f)] join ([time_dim#1 (f)] join ([household_demographics#1 (f)] join web_sales#1))` | `[web_page#1 (f)] join ([household_demographics#1 (f)] join ([time_dim#1 (f)] join web_sales#1))` | `[household_demographics#1 (f)] join ([[m0] join web_sales#1] join time_dim#1 (f))` |
| changed by | | reordered by DPhyp | the build of MJ1's join on wp_web_page_sk = ws_web_page_sk came out 1 rows, est 3.2 (q-error 3.22); reordered by DPhyp |
| C_out | 1,222,960 B | 1,028,567 B (plan's order 1,222,960 B) | 415,656 B (previous order 1,024,960 B) |

m0 = `web_page#1 (f)`, 1 rows.

### q90 MJ2

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[web_page#2 (f)] join ([time_dim#2 (f)] join ([household_demographics#2 (f)] join web_sales#2))` | `[web_page#2 (f)] join ([household_demographics#2 (f)] join ([time_dim#2 (f)] join web_sales#2))` | `[household_demographics#2 (f)] join ([[m2] join web_sales#2] join time_dim#2 (f))` |
| changed by | | reordered by DPhyp | the build of MJ2's join on wp_web_page_sk = ws_web_page_sk came out 1 rows, est 3.2 (q-error 3.22); reordered by DPhyp |
| C_out | 1,222,960 B | 1,028,567 B (plan's order 1,222,960 B) | 415,656 B (previous order 1,024,960 B) |

Calls that left the order unchanged: replan 1.

m2 = `web_page#2 (f)`, 1 rows.

## q91

### q91 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[household_demographics (f)] join ([[[[p0] join ([call_center] join catalog_returns)] join customer] join customer_address (f)] join customer_demographics (f))` | `[[[[[[p0] join catalog_returns] join customer] join customer_demographics (f)] join customer_address (f)] join household_demographics (f)] join call_center` |
| changed by | | reordered by DPhyp |
| C_out | 10,784,182 B | 151,379 B (plan's order 10,784,182 B) |

p0 = `date_dim (f)`, 30 rows.

### q91 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_returns | cr_returned_date_sk | p0.d_date_sk | 30 values, 2451119 to 2451148 | 2 → 1 (0) |
| after join order | customer_demographics | cd_demo_sk | p1.c_current_cdemo_sk | 844 to 1919882 | 16 → 16 (0-15) |

p0 = `date_dim (f)`, 30 rows. p1 = `[[p0] join catalog_returns] join customer`, 1,783 rows, holding MJ1.

## q92

MJ2 kept the plan's order: `[p0] join web_sales#2`.

### q92 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[item (f)] join web_sales#1] join date_dim#1 (f)] join agg(MJ2)` | `[[[item (f)] join agg(MJ2)] join web_sales#1] join date_dim#1 (f)` |
| changed by | | reordered by DPhyp |
| C_out | 24,370 B | 4,057 B (plan's order 24,370 B) |

### q92 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales#2 | ws_sold_date_sk | p0.d_date_sk | 2451571 to 2451661 | 6 → 1 (2) |

p0 = `date_dim#2 (f)`, 91 rows.

## q93

MJ1 kept the plan's order: `[reason (f)] join (store_sales left join [store_returns])`.

## q94

### q94 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[web_site (f)] join ([customer_address (f)] join ([date_dim (f)] join web_sales#1))] semi join web_sales#2] anti join web_returns` | `[[[web_site (f)] join ([date_dim (f)] join ([customer_address (f)] join web_sales#1))] semi join web_sales#2] anti join web_returns` |
| changed by | | reordered by DPhyp |
| C_out | 1,489,354 B | 913,508 B (plan's order 1,489,354 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 135 rows, est 14.3).

## q95

### q95 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[web_site (f)] join ([customer_address (f)] join ([date_dim (f)] join web_sales#1))] semi join ([web_sales#2] join web_sales#3)] semi join ([web_returns] join ([web_sales#4] join web_sales#5))` | `[[[web_site (f)] join ([date_dim (f)] join ([customer_address (f)] join web_sales#1))] semi join ([web_returns] join ([web_sales#4] join web_sales#5))] semi join ([web_sales#2] join web_sales#3)` |
| changed by | | reordered by DPhyp |
| C_out | 46,076,475 B | 45,572,035 B (plan's order 46,076,475 B) |

## q96

### q96 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[store (f)] join ([time_dim (f)] join ([household_demographics (f)] join store_sales))` | `[store (f)] join ([household_demographics (f)] join ([time_dim (f)] join store_sales))` |
| changed by | | reordered by DPhyp |
| C_out | 4,798,174 B | 1,917,788 B (plan's order 4,798,174 B) |

## q97

MJ1 kept the plan's order: `[agg(MJ2)] full join agg(MJ3)`.

MJ2 kept the plan's order: `[p0] join catalog_sales`.

MJ3 kept the plan's order: `[p1] join store_sales`.

### q97 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 12 → 3 (4-6) |
| plan as written | store_sales | ss_sold_date_sk | p1.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim#1 (f)`, 366 rows. p1 = `date_dim#2 (f)`, 366 rows.

## q98

skipped: refused: unsupported: WindowAggExec: window functions (#143)

## q99

### q99 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[date_dim (f)] join ([call_center] join ([ship_mode] join ([warehouse] join catalog_sales)))` | `[call_center] join ([ship_mode] join ([warehouse] join ([date_dim (f)] join catalog_sales)))` |
| changed by | | reordered by DPhyp |
| C_out | 192,911,685 B | 45,296,500 B (plan's order 192,911,685 B) |

