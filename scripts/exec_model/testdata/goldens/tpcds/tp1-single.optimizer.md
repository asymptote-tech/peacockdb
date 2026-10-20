# tpcds tp1-single: join orders and dynamic filters

What the optimizer changed in each query's plan. Every figure behind it is in
[tp1-single.optimizer.txt](tp1-single.optimizer.txt), from the same run.

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
| join tree | `[item (f)] join ([p0] join store_sales)` | `[p0] join ([item (f)] join store_sales)` |
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

| | plan | DPhyp before the run | replan 1 | replan 2 | replan 3 | replan 5 | replan 6 |
|---|---|---|---|---|---|---|---|
| join tree | `[[[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ6) (f)] join agg(MJ7)` | `[[[[[agg(MJ6) (f)] join agg(MJ2) (f)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ7)] join agg(MJ3)` | `[[[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)] join agg(MJ7)] join agg(MJ6) (f)` | `[agg(MJ6) (f)] join ([[[[agg(MJ2) (f)] join agg(MJ5)] join agg(MJ4) (f)] join agg(MJ3)] join agg(MJ7))` | `[[[[[m8] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ7)] join agg(MJ4) (f)] join agg(MJ3)` | `[[[[m9] join agg(MJ7)] join agg(MJ4) (f)] join agg(MJ5)] join agg(MJ3)` | `[[[[m9] join agg(MJ4) (f)] join agg(MJ7)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_customer_sk = c_customer_sk came out 147024 rows, est 49124.4 (q-error 2.99); reordered by DPhyp | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 286690 rows, est 97873.6 (q-error 2.93); reordered by DPhyp; flip: `agg(MJ6) (f)` now builds | the build of MJ1's join on c_customer_id = c_customer_id came out 11349 rows, est 29404.8 (q-error 2.59); reordered by DPhyp | the build of MJ5's join on cs_bill_customer_sk = c_customer_sk came out 285317 rows, est 66831.7 (q-error 4.27); reordered by DPhyp | the build of MJ7's join on ws_bill_customer_sk = c_customer_sk came out 142500 rows, est 45547.2 (q-error 3.13); reordered by DPhyp |
| C_out | 9,173,152 B | 2,290,130 B (plan's order 9,173,152 B) | 3,867,277 B (previous order 5,091,153 B) | 5,882,346 B (previous order 6,180,984 B) | 2,653,674 B (previous order 5,825,890 B) | 991,544 B (previous order 2,144,739 B) | 1,350,024 B (previous order 1,412,950 B) |

Calls that left the order unchanged: replan 4 (MJ1's missed build came out 4340 rows, est 11763.8); replan 7 (MJ1's missed build came out 1162 rows, est 5265.9); replan 8 (MJ1's missed build came out 125 rows, est 1655.8); replan 9 (MJ1's missed build came out 22 rows, est 71.3).

m8 = `agg(MJ6) (f)`, 11,349 rows. m9 = `[m8] join agg(MJ2) (f)`, 4,340 rows.

### q4 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[customer#1] join ([p0] join store_sales#1)` |
| changed by | | reordered by DPhyp |
| C_out | 132,288,333 B | 32,828,989 B (plan's order 132,288,333 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

p0 = `date_dim#1 (f)`, 365 rows.

### q4 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[customer#2] join ([p1] join store_sales#2)` |
| changed by | | reordered by DPhyp |
| C_out | 156,602,520 B | 38,862,856 B (plan's order 156,602,520 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6; replan 7; replan 8; replan 9.

p1 = `date_dim#2 (f)`, 365 rows.

### q4 MJ4

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[p2] join ([customer#3] join catalog_sales#1)` | `[[p2] join catalog_sales#1] join customer#3` | `[customer#3] join m7` |
| changed by | | reordered by DPhyp | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 286690 rows, est 97873.6 (q-error 2.93); flip: `customer#3` now builds |
| C_out | 97,194,876 B | 24,043,297 B (plan's order 97,194,876 B) | 47,277,063 B (previous order 47,277,063 B) |

Calls that left the order unchanged: replan 1; replan 3; replan 4; replan 5; replan 6.

m7 = `[p2] join catalog_sales#1`, 286,690 rows. p2 = `date_dim#3 (f)`, 365 rows.

### q4 MJ5

| | plan | DPhyp before the run | replan 5 |
|---|---|---|---|
| join tree | `[p3] join ([customer#4] join catalog_sales#2)` | `[[p3] join catalog_sales#2] join customer#4` | `[customer#4] join m10` |
| changed by | | reordered by DPhyp | the build of MJ5's join on cs_bill_customer_sk = c_customer_sk came out 285317 rows, est 66831.7 (q-error 4.27); flip: `customer#4` now builds |
| C_out | 66,368,249 B | 16,417,651 B (plan's order 66,368,249 B) | 47,050,646 B (previous order 47,050,646 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 6; replan 7; replan 8.

m10 = `[p3] join catalog_sales#2`, 285,317 rows. p3 = `date_dim#4 (f)`, 365 rows.

### q4 MJ6

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p4] join ([customer#5] join web_sales#1)` | `[[p4] join web_sales#1] join customer#5` | `[customer#5] join m6` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_customer_sk = c_customer_sk came out 147024 rows, est 49124.4 (q-error 2.99); flip: `customer#5` now builds |
| C_out | 48,628,375 B | 12,067,734 B (plan's order 48,628,375 B) | 24,245,223 B (previous order 24,245,223 B) |

Calls that left the order unchanged: replan 2.

m6 = `[p4] join web_sales#1`, 147,024 rows. p4 = `date_dim#5 (f)`, 365 rows.

### q4 MJ7

| | plan | DPhyp before the run | replan 6 |
|---|---|---|---|
| join tree | `[p5] join ([customer#6] join web_sales#2)` | `[[p5] join web_sales#2] join customer#6` | `[customer#6] join m11` |
| changed by | | reordered by DPhyp | the build of MJ7's join on ws_bill_customer_sk = c_customer_sk came out 142500 rows, est 45547.2 (q-error 3.13); flip: `customer#6` now builds |
| C_out | 45,087,304 B | 11,188,973 B (plan's order 45,087,304 B) | 23,499,185 B (previous order 23,499,185 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 7.

m11 = `[p5] join web_sales#2`, 142,500 rows. p5 = `date_dim#6 (f)`, 365 rows.

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

A replan was refused, work outside its builds having started: the build of MJ2's join on page_sk = cp_catalog_page_sk came out 16000 rows, est 948.9 (q-error 16.86).

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

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 7761 rows, est 2265.7).

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

p0 = `date_dim#1 (f)`, 120 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 97 rows.

### q10 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2452276 to 2452395 | 24 → 3 (17-18,23) |
| plan as written | web_sales | ws_sold_date_sk | p1.d_date_sk | 2452276 to 2452395 | 6 → 1 (4) |
| plan as written | catalog_sales | cs_sold_date_sk | p2.d_date_sk | 2452276 to 2452395 | 12 → 1 (9) |
| after join order | customer_demographics | cd_demo_sk | p3.c_current_cdemo_sk | 21355 to 1909112 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 120 rows. p1 = `date_dim#2 (f)`, 120 rows. p2 = `date_dim#3 (f)`, 120 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 97 rows, holding MJ1.

Probe p3, replan 1: the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 459 rows, est 131.5 (q-error 3.49); m3 = `customer_address (f)`, 459 rows.

## q11

### q11 MJ1

| | plan | DPhyp before the run | replan 1 | replan 2 |
|---|---|---|---|---|
| join tree | `[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` | `[[agg(MJ4) (f)] join ([agg(MJ2) (f)] join agg(MJ5))] join agg(MJ3)` | `[[[m5] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ4's join on ws_bill_customer_sk = c_customer_sk came out 147024 rows, est 49124.4 (q-error 2.99); reordered by DPhyp | the build of MJ1's join on c_customer_id = c_customer_id came out 11552 rows, est 29404.8 (q-error 2.55); reordered by DPhyp |
| C_out | 6,091,633 B | 1,376,031 B (plan's order 6,091,633 B) | 3,724,859 B (previous order 3,796,718 B) | 1,617,924 B (previous order 2,390,627 B) |

Calls that left the order unchanged: replan 3; replan 4 (MJ1's missed build came out 4414 rows, est 17916.5); replan 5; replan 6 (MJ1's missed build came out 485 rows, est 6289.9); replan 7.

m5 = `agg(MJ4) (f)`, 11,552 rows.

### q11 MJ2

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[[p0] join store_sales#1] join customer#1` | `[customer#1] join m6` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 546314 rows, est 133637.7 (q-error 4.09); flip: `customer#1` now builds |
| C_out | 106,417,326 B | 24,209,354 B (plan's order 106,417,326 B) | 72,472,138 B (previous order 72,472,138 B) |

Calls that left the order unchanged: replan 1; replan 2.

m6 = `[p0] join store_sales#1`, 546,314 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q11 MJ3

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[[p1] join store_sales#2] join customer#2` | `[customer#2] join m10` |
| changed by | | reordered by DPhyp | the build of MJ3's join on ss_customer_sk = c_customer_sk came out 549739 rows, est 158199.9 (q-error 3.47); flip: `customer#2` now builds |
| C_out | 125,976,503 B | 28,658,959 B (plan's order 125,976,503 B) | 72,926,487 B (previous order 72,926,487 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

m10 = `[p1] join store_sales#2`, 549,739 rows. p1 = `date_dim#2 (f)`, 365 rows.

### q11 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p2] join ([customer#3] join web_sales#1)` | `[[p2] join web_sales#1] join customer#3` |
| changed by | | reordered by DPhyp |
| C_out | 39,118,352 B | 8,899,209 B (plan's order 39,118,352 B) |

Calls that left the order unchanged: replan 1 (MJ4's missed build came out 147024 rows, est 49124.4).

p2 = `date_dim#3 (f)`, 365 rows.

### q11 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p3] join ([customer#4] join web_sales#2)` | `[[p3] join web_sales#2] join customer#4` |
| changed by | | reordered by DPhyp |
| C_out | 36,269,793 B | 8,251,178 B (plan's order 36,269,793 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5 (MJ5's missed build came out 142500 rows, est 45547.2).

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

| | plan | DPhyp before the run | replan 1 | replan 4 |
|---|---|---|---|---|
| join tree | `[p0] join ([customer_address (f)] join ([household_demographics (f)] join ([customer_demographics (f)] join ([store] join store_sales (f)))))` | `[[[[[store_sales (f)] join customer_demographics (f)] join customer_address (f)] join household_demographics (f)] join p0] join store` | `[[[[customer_address (f)] join m2] join household_demographics (f)] join p0] join store` | `[store] join m5` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_addr_sk = ca_address_sk came out 1061 rows, est 66.9 (q-error 15.87); flip: `customer_address (f)` now builds | the build of MJ1's join on ss_store_sk = s_store_sk came out 4 rows, est 1.4 (q-error 2.86); flip: `store` now builds |
| C_out | 853,624 B | 16,807 B (plan's order 853,624 B) | 2,809 B (previous order 2,809 B) | 162 B (previous order 162 B) |

Calls that left the order unchanged: replan 2 (MJ1's missed build came out 181 rows, est 1.4); replan 3 (MJ1's missed build came out 7 rows, est 1.5).

m2 = `[p1] join customer_demographics (f)`, 1,061 rows. m3 = `[customer_address (f)] join m2`, 181 rows. m4 = `[m3] join household_demographics (f)`, 7 rows. m5 = `[m4] join p0`, 4 rows. p0 = `date_dim (f)`, 365 rows. p1 = `store_sales (f)`, 37,869 rows.

### q13 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451911 to 2452275 | 24 → 6 (13-17,23) |
| after join order | customer_demographics | cd_demo_sk | p1.ss_cdemo_sk | 15 to 1920767 | 16 → 16 (0-15) |

p0 = `date_dim (f)`, 365 rows. p1 = `store_sales (f)`, 37,869 rows.

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[p3] join ([item#1] join store_sales#2)] semi join ([item#2] join (agg(MJ5) semi join [[p4] join ([item#3] join web_sales#2)]))` | `[[[p3] join store_sales#2] semi join ([agg(MJ5) semi join [[item#3] join ([p4] join web_sales#2)]] join item#2)] join item#1` | `[item#1] join ([m22] semi join ([agg(MJ5) semi join [[item#3] join ([p4] join web_sales#2)]] join item#2))` |
| changed by | | reordered by DPhyp; flip: `agg(MJ5) semi join [[item#3] join ([p4] join web_sales#2)]` now builds | the build of MJ4's semi join on ss_item_sk = i_item_sk came out 86880 rows, est 4927.5 (q-error 17.63); flip: `item#1` now builds |
| C_out | 43,418,281 B | 10,422,733 B (plan's order 43,418,281 B) | 16,901,359 B (previous order 16,901,359 B) |

Calls that left the order unchanged: replan 2; replan 3.

m22 = `[p3] join store_sales#2`, 86,880 rows. p3 = `date_dim#4 (f)`, 30 rows. p4 = `date_dim#5 (f)`, 1,096 rows.

### q14 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ6) semi join [[p5] join ([item#4] join catalog_sales#2)]` | `agg(MJ6) semi join [[item#4] join ([p5] join catalog_sales#2)]` |
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

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[[p10] join ([item#6] join catalog_sales#4)] semi join ([item#7] join (agg(MJ11) semi join [[p11] join ([item#8] join web_sales#4)]))` | `[[[p10] join catalog_sales#4] semi join ([agg(MJ11) semi join [[item#8] join ([p11] join web_sales#4)]] join item#7)] join item#6` | `[item#6] join ([m24] semi join ([agg(MJ11) semi join [[item#8] join ([p11] join web_sales#4)]] join item#7))` |
| changed by | | reordered by DPhyp; flip: `agg(MJ11) semi join [[item#8] join ([p11] join web_sales#4)]` now builds | the build of MJ10's semi join on cs_item_sk = i_item_sk came out 46689 rows, est 2011.1 (q-error 23.22); flip: `item#6` now builds |
| C_out | 31,625,237 B | 10,092,088 B (plan's order 31,625,237 B) | 13,645,888 B (previous order 13,645,888 B) |

Calls that left the order unchanged: replan 1; replan 3.

m24 = `[p10] join catalog_sales#4`, 46,689 rows. p10 = `date_dim#11 (f)`, 30 rows. p11 = `date_dim#12 (f)`, 1,096 rows.

### q14 MJ11

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ12) semi join [[p12] join ([item#9] join catalog_sales#5)]` | `agg(MJ12) semi join [[item#9] join ([p12] join catalog_sales#5)]` |
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

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[[p17] join ([item#11] join web_sales#6)] semi join ([item#12] join (agg(MJ17) semi join [[p18] join ([item#13] join web_sales#7)]))` | `[[[p17] join web_sales#6] semi join ([agg(MJ17) semi join [[item#13] join ([p18] join web_sales#7)]] join item#12)] join item#11` | `[item#11] join ([m27] semi join ([agg(MJ17) semi join [[item#13] join ([p18] join web_sales#7)]] join item#12))` |
| changed by | | reordered by DPhyp; flip: `agg(MJ17) semi join [[item#13] join ([p18] join web_sales#7)]` now builds | the build of MJ16's semi join on ws_item_sk = i_item_sk came out 23524 rows, est 2018.8 (q-error 11.65); flip: `item#11` now builds |
| C_out | 31,626,050 B | 10,092,962 B (plan's order 31,626,050 B) | 11,769,523 B (previous order 11,769,523 B) |

Calls that left the order unchanged: replan 1; replan 2.

m27 = `[p17] join web_sales#6`, 23,524 rows. p17 = `date_dim#18 (f)`, 30 rows. p18 = `date_dim#19 (f)`, 1,096 rows.

### q14 MJ17

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `agg(MJ18) semi join [[p19] join ([item#14] join catalog_sales#7)]` | `agg(MJ18) semi join [[item#14] join ([p19] join catalog_sales#7)]` |
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

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 41341 rows, est 6100.3).

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0] join store] join item` | `[[[[[p2] join m3] join p1] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[[store_returns] join catalog_sales] join store_sales` now builds; flip: `[[[store_returns] join catalog_sales] join store_sales] join p2` now builds; flip: `[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1` now builds; flip: `[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0` now builds; flip: `[[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0] join store` now builds | the build of MJ1's join on ss_sold_date_sk = d_date_sk came out 22 rows, est 3.2 (q-error 6.97); flip: `p2` now builds |
| C_out | 871,006 B | 1,862 B (plan's order 871,006 B) | 75 B (previous order 75 B) |

m3 = `[[store_returns] join catalog_sales] join store_sales`, 22 rows. p0 = `date_dim#1 (f)`, 274 rows. p1 = `date_dim#2 (f)`, 274 rows. p2 = `date_dim#3 (f)`, 91 rows.

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

p0 = `date_dim (f)`, 365 rows. p1 = `[p0] join ([[customer_address (f)] join customer (f)] join ([customer_demographics#1 (f)] join catalog_sales))`, 1,326 rows, holding MJ1.

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p0] join ([item (f)] join ([warehouse] join inventory))` | `[warehouse] join ([p0] join ([item (f)] join inventory))` | `[m1] join ([m2] join ([p0] join inventory))` |
| changed by | | reordered by DPhyp | the build of MJ1's join on i_item_sk = inv_item_sk came out 1104 rows, est 96.5 (q-error 11.44); reordered by DPhyp |
| C_out | 19,159,564 B | 114,082 B (plan's order 19,159,564 B) | 487,673 B (previous order 1,305,000 B) |

m1 = `warehouse`, 5 rows. m2 = `item (f)`, 1,104 rows. p0 = `date_dim (f)`, 61 rows.

### q21 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | inventory | inv_date_sk | p0.d_date_sk | 2451585 to 2451645 | 96 → 4 (40-43) |

p0 = `date_dim (f)`, 61 rows.

## q22

MJ1 kept the plan's order: `[item] join ([p0] join inventory)`.

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

A replan was refused, work outside its builds having started: the build of MJ5's join on ws_bill_customer_sk = c_customer_sk came out 366 rows, est 6725.0 (q-error 18.37).

A replan was refused, work outside its builds having started: the build of MJ5's join on ws_item_sk = i_item_sk came out 366 rows, est 6725.0 (q-error 18.37).

### q23 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[p0] join ([customer#1] join catalog_sales)] join agg(MJ2) (f)] join agg([agg(MJ3)] cross join MJ4) (f)` | `[[[[p0] join catalog_sales] join agg([agg(MJ3)] cross join MJ4) (f)] join customer#1] join agg(MJ2) (f)` | `[[[agg([agg(MJ3)] cross join MJ4) (f)] join m6] join customer#1] join agg(MJ2) (f)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on cs_bill_customer_sk = c_customer_sk came out 13302 rows, est 1944.1 (q-error 6.84); flip: `agg([agg(MJ3)] cross join MJ4) (f)` now builds |
| C_out | 11,679,883 B | 1,703,228 B (plan's order 11,679,883 B) | 11,115,376 B (previous order 11,115,376 B) |

Calls that left the order unchanged: replan 2; replan 3 (MJ1's missed build came out 4703 rows, est 18171.6); replan 4 (MJ1's missed build came out 667 rows, est 13302.0).

m6 = `[p0] join catalog_sales`, 13,302 rows. p0 = `date_dim#1 (f)`, 29 rows.

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
| changed by | | reordered by DPhyp | the build of MJ5's join on ws_bill_customer_sk = c_customer_sk came out 6725 rows, est 1951.5 (q-error 3.45); flip: `agg([agg(MJ7)] cross join MJ8) (f)` now builds |
| C_out | 11,692,427 B | 1,709,758 B (plan's order 11,692,427 B) | 5,619,524 B (previous order 5,619,524 B) |

Calls that left the order unchanged: replan 1; replan 3; replan 4.

m7 = `[p3] join web_sales`, 6,725 rows. p3 = `date_dim#4 (f)`, 29 rows.

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

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 130 rows, est 23.2); replan 2 (MJ1's missed build came out 226 rows, est 52.0).

### q24 MJ2

| | plan | DPhyp before the run | replan 4 |
|---|---|---|---|
| join tree | `[customer_address#2] join ([customer#2] join ([item#2 (f)] join ([store#2 (f)] join ([store_returns#2] join store_sales#2))))` | `[[[[[store#2 (f)] join customer_address#2] join customer#2] join store_sales#2] join item#2 (f)] join store_returns#2` | `[[item#2 (f)] join ([m4] join store_sales#2)] join store_returns#2` |
| changed by | | reordered by DPhyp | the build of MJ2's join on s_store_sk = ss_store_sk and c_customer_sk = ss_customer_sk came out 226 rows, est 52.0 (q-error 4.35); flip: `item#2 (f)` now builds |
| C_out | 14,598,172 B | 3,809 B (plan's order 14,598,172 B) | 49,331 B (previous order 49,331 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3 (MJ2's missed build came out 130 rows, est 23.2); replan 5 (MJ2's missed build came out 409 rows, est 195.2).

m3 = `[store#2 (f)] join customer_address#2`, 130 rows. m4 = `[m3] join customer#2`, 226 rows.

## q25

### q25 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0] join store] join item` | `[[[[[p2] join m4] join p1] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[[store_returns] join catalog_sales] join store_sales` now builds; flip: `[[[store_returns] join catalog_sales] join store_sales] join p2` now builds; flip: `[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1` now builds; flip: `[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0` now builds; flip: `[[[[[[store_returns] join catalog_sales] join store_sales] join p2] join p1] join p0] join store` now builds | the build of MJ1's join on ss_sold_date_sk = d_date_sk came out 11 rows, est 1.9 (q-error 5.89); flip: `p2` now builds |
| C_out | 640,665 B | 1,518 B (plan's order 640,665 B) | 17 B (previous order 17 B) |

m4 = `[p3] join store_sales`, 11 rows. p0 = `date_dim#1 (f)`, 214 rows. p1 = `date_dim#2 (f)`, 214 rows. p2 = `date_dim#3 (f)`, 30 rows. p3 = `[store_returns] join catalog_sales`, 29 rows.

### q25 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2452001 to 2452214 | 12 → 2 (7-8) |
| plan as written | store_returns | sr_returned_date_sk | p1.d_date_sk | 2452001 to 2452214 | 3 → 1 (1) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 30 values, 2452001 to 2452030 | 24 → 2 (14,23) |
| after join order | store_sales | ss_item_sk | p3.sr_item_sk | 29 values, 853 to 17477 | 2 → 2 (14,23) |

p0 = `date_dim#1 (f)`, 214 rows. p1 = `date_dim#2 (f)`, 214 rows. p2 = `date_dim#3 (f)`, 30 rows. p3 = `[store_returns] join catalog_sales`, 29 rows, holding MJ1.

## q26

### q26 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion (f)] join ([item] join ([p0] join ([customer_demographics (f)] join catalog_sales)))` | `[[promotion (f)] join ([p0] join ([customer_demographics (f)] join catalog_sales))] join item` |
| changed by | | reordered by DPhyp |
| C_out | 672,093 B | 650,916 B (plan's order 672,093 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 4032 rows, est 1051.3).

p0 = `date_dim (f)`, 366 rows.

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
| join tree | `[item] join ([store] join ([p0] join ([p1] join ([p2] join ([catalog_sales] join ([store_returns] join store_sales))))))` | `[[[[[[p1] join ([store_returns] join catalog_sales)] join store_sales] join p2] join p0] join store] join item` |
| changed by | | reordered by DPhyp; flip: `[[[p1] join ([store_returns] join catalog_sales)] join store_sales] join p2` now builds; flip: `[[[[p1] join ([store_returns] join catalog_sales)] join store_sales] join p2] join p0` now builds; flip: `[[[[[p1] join ([store_returns] join catalog_sales)] join store_sales] join p2] join p0] join store` now builds |
| C_out | 514,352 B | 4,637 B (plan's order 514,352 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 4 rows, est 0.6).

p0 = `date_dim#1 (f)`, 1,096 rows. p1 = `date_dim#2 (f)`, 122 rows. p2 = `date_dim#3 (f)`, 30 rows.

### q29 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales | cs_sold_date_sk | p0.d_date_sk | 2451180 to 2452275 | 12 → 8 (2-9) |
| plan as written | store_returns | sr_returned_date_sk | p1.d_date_sk | 2451423 to 2451544 | 3 → 1 (0) |
| plan as written | store_sales | ss_sold_date_sk | p2.d_date_sk | 30 values, 2451423 to 2451452 | 24 → 2 (6,23) |
| after join order | store_sales | ss_item_sk | p3.sr_item_sk | 1111, 3584, 3694, 4951, 12307, 12830, 13634, 14972, 15673, 15913 | 2 → 2 (6,23) |

p0 = `date_dim#1 (f)`, 1,096 rows. p1 = `date_dim#2 (f)`, 122 rows. p2 = `date_dim#3 (f)`, 30 rows. p3 = `[p1] join ([store_returns] join catalog_sales)`, 10 rows, holding MJ1.

## q30

MJ2 kept the plan's order: `[[date_dim#1 (f)] join web_returns#1] join customer_address#1`.

MJ3 kept the plan's order: `[[date_dim#2 (f)] join web_returns#2] join customer_address#3`.

### q30 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[agg(MJ2)] join customer] join customer_address#2 (f)] join agg(MJ3)` | `[agg(MJ3)] join ([[customer_address#2 (f)] join customer] join agg(MJ2))` | `[m1] join ([[m0] join agg(MJ2)] join customer)` |
| changed by | | reordered by DPhyp; flip: `agg(MJ3)` now builds | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 2446 rows, est 950.0 (q-error 2.57); reordered by DPhyp |
| C_out | 1,956,405 B | 299,856 B (plan's order 1,956,405 B) | 452,814 B (previous order 1,418,400 B) |

m0 = `agg(MJ3)`, 52 rows. m1 = `customer_address#2 (f)`, 2,446 rows.

## q31

MJ5 kept the plan's order: `[[p3] join web_sales#1] join customer_address#4`.

MJ6 kept the plan's order: `[[p4] join web_sales#2] join customer_address#5`.

### q31 MJ1

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[[[[[agg(MJ2)] join agg(MJ3)] join agg(MJ4)] join agg(MJ5)] join agg(MJ6)] join agg(MJ7)` | `[[[[agg(MJ5)] join agg(MJ6)] join ([agg(MJ2)] join agg(MJ3))] join agg(MJ4)] join agg(MJ7)` | `[[[[[m7] join agg(MJ6)] join agg(MJ2)] join agg(MJ3)] join agg(MJ7)] join agg(MJ4)` |
| changed by | | reordered by DPhyp | the build of MJ6's join on ws_bill_addr_sk = ca_address_sk came out 20827 rows, est 6123.7 (q-error 3.40); reordered by DPhyp |
| C_out | 530,917 B | 333,216 B (plan's order 530,917 B) | 186,044 B (previous order 239,015 B) |

Calls that left the order unchanged: replan 1; replan 3; replan 4; replan 5 (MJ1's missed build came out 244 rows, est 97.2); replan 6; replan 7.

m7 = `agg(MJ5)`, 942 rows.

### q31 MJ2

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer_address#1] join ([p0] join store_sales#1)` | `[[p0] join store_sales#1] join customer_address#1` | `[customer_address#1] join m10` |
| changed by | | flip: `[p0] join store_sales#1` now builds | the build of MJ2's join on ss_addr_sk = ca_address_sk came out 84293 rows, est 14946.7 (q-error 5.64); flip: `customer_address#1` now builds |
| C_out | 1,353,444 B | 1,353,444 B (plan's order 1,353,444 B) | 4,218,970 B (previous order 4,218,970 B) |

Calls that left the order unchanged: replan 1; replan 2.

m10 = `[p0] join store_sales#1`, 84,293 rows. p0 = `date_dim#1 (f)`, 91 rows.

### q31 MJ3

| | plan | DPhyp before the run | replan 4 |
|---|---|---|---|
| join tree | `[customer_address#2] join ([p1] join store_sales#2)` | `[[p1] join store_sales#2] join customer_address#2` | `[customer_address#2] join m12` |
| changed by | | flip: `[p1] join store_sales#2` now builds | the build of MJ3's join on ss_addr_sk = ca_address_sk came out 79565 rows, est 14946.7 (q-error 5.32); flip: `customer_address#2` now builds |
| C_out | 1,353,444 B | 1,353,444 B (plan's order 1,353,444 B) | 3,982,328 B (previous order 3,982,328 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3.

m12 = `[p1] join store_sales#2`, 79,565 rows. p1 = `date_dim#2 (f)`, 91 rows.

### q31 MJ4

| | plan | DPhyp before the run | replan 7 |
|---|---|---|---|
| join tree | `[customer_address#3] join ([p2] join store_sales#3)` | `[[p2] join store_sales#3] join customer_address#3` | `[customer_address#3] join m16` |
| changed by | | flip: `[p2] join store_sales#3` now builds | the build of MJ4's join on ss_addr_sk = ca_address_sk came out 147221 rows, est 15111.0 (q-error 9.74); flip: `customer_address#3` now builds |
| C_out | 1,368,317 B | 1,368,317 B (plan's order 1,368,317 B) | 7,368,595 B (previous order 7,368,595 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

m16 = `[p2] join store_sales#3`, 147,221 rows. p2 = `date_dim#3 (f)`, 92 rows.

### q31 MJ7

| | plan | replan 6 |
|---|---|---|
| join tree | `[[p5] join web_sales#3] join customer_address#6` | `[customer_address#6] join m14` |
| changed by | | the build of MJ7's join on ws_bill_addr_sk = ca_address_sk came out 37605 rows, est 6191.0 (q-error 6.07); flip: `customer_address#6` now builds |
| C_out | 560,605 B | 1,882,177 B (previous order 1,882,177 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 2; replan 3; replan 4; replan 5.

m14 = `[p5] join web_sales#3`, 37,605 rows. p5 = `date_dim#6 (f)`, 92 rows.

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[item (f)] join catalog_sales#1] join date_dim#1 (f)] join agg(MJ2)` | `[date_dim#1 (f)] join ([[item (f)] join agg(MJ2)] join catalog_sales#1)` | `[[[m2] join agg(MJ2)] join catalog_sales#1] join p1` |
| changed by | | reordered by DPhyp | the build of MJ1's join on i_item_sk = cs_item_sk came out 4 rows, est 18.1 (q-error 4.52); flip: `[[m2] join agg(MJ2)] join catalog_sales#1` now builds |
| C_out | 48,827 B | 7,689 B (plan's order 48,827 B) | 371 B (previous order 371 B) |

Calls that left the order unchanged: replan 2 (MJ1's missed build came out 22 rows, est 10.9).

m2 = `item (f)`, 4 rows. p1 = `date_dim#1 (f)`, 91 rows.

### q32 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | catalog_sales#2 | cs_sold_date_sk | p0.d_date_sk | 2451571 to 2451661 | 12 → 2 (4-5) |
| after join order | catalog_sales#1 | cs_sold_date_sk | p1.d_date_sk | 2451571 to 2451661 | 12 → 2 (4-5) |

p0 = `date_dim#2 (f)`, 91 rows. p1 = `date_dim#1 (f)`, 91 rows.

## q33

MJ2 kept the plan's order: `([[customer_address#2 (f)] join ([p1] join catalog_sales)] join item#4) semi join [item#3 (f)]`.

MJ3 kept the plan's order: `([[customer_address#3 (f)] join ([p2] join web_sales)] join item#6) semi join [item#5 (f)]`.

A replan was refused, work outside its builds having started: the build of MJ2's join on ca_address_sk = cs_bill_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 5028 rows, est 335.6 (q-error 14.98).

A replan was refused, work outside its builds having started: the build of MJ3's join on ca_address_sk = ws_bill_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 2761 rows, est 336.9 (q-error 8.20).

### q33 MJ1

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[customer_address#1 (f)] join ([p0] join store_sales)] join item#2) semi join [item#1 (f)]` | `[item#2 semi join [m5]] join m7` |
| changed by | | flip: `[customer_address#1 (f)] join ([p0] join store_sales)` now builds | the build of MJ1's join on ss_item_sk = i_item_sk came out 10034 rows, est 1858.1 (q-error 5.40); reordered by DPhyp |
| C_out | 221,688 B | 221,688 B (plan's order 221,688 B) | 386,505 B (previous order 418,841 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 18246 rows, est 8074.0).

m5 = `item#1 (f)`, 1,812 rows. m6 = `customer_address#1 (f)`, 18,246 rows. m7 = `[m6] join ([p0] join store_sales)`, 10,034 rows. p0 = `date_dim#1 (f)`, 31 rows.

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
| C_out | 105,339 B | 105,339 B (plan's order 105,339 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ1's missed build came out 455 rows, est 7074.5).

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
| join tree | `[[[customer_address] join customer] join customer_demographics] semi join ([p0] join store_sales)` | `[[customer_address] join ([customer] semi join ([p0] join store_sales))] join customer_demographics` |
| changed by | | reordered by DPhyp |
| C_out | 11,047,236 B | 8,157,513 B (plan's order 11,047,236 B) |

p0 = `date_dim#1 (f)`, 274 rows.

### q35 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2452276 to 2452549 | 24 → 5 (17-20,23) |
| plan as written | web_sales | ws_sold_date_sk | p1.d_date_sk | 2452276 to 2452549 | 6 → 2 (4-5) |
| plan as written | catalog_sales | cs_sold_date_sk | p2.d_date_sk | 2452276 to 2452549 | 12 → 2 (9-10) |
| after join order | customer_demographics | cd_demo_sk | p3.c_current_cdemo_sk | 44 to 1920788 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 274 rows. p1 = `date_dim#2 (f)`, 274 rows. p2 = `date_dim#3 (f)`, 274 rows. p3 = `[customer_address] join ([customer] semi join ([p0] join store_sales))`, 23,496 rows, holding MJ1.

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

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 142180 rows, est 49259.0).

p0 = `date_dim#1 (f)`, 366 rows.

### q38 MJ4

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer#2] join ([p1] join catalog_sales)` | `[[p1] join catalog_sales] join customer#2` | `[customer#2] join m5` |
| changed by | | flip: `[p1] join catalog_sales` now builds | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 287563 rows, est 73606.3 (q-error 3.91); flip: `customer#2` now builds |
| C_out | 2,662,408 B | 2,662,408 B (plan's order 2,662,408 B) | 6,878,769 B (previous order 6,878,769 B) |

Calls that left the order unchanged: replan 1; replan 2.

m5 = `[p1] join catalog_sales`, 287,563 rows. p1 = `date_dim#2 (f)`, 366 rows.

### q38 MJ5

| | plan | DPhyp before the run | replan 5 |
|---|---|---|---|
| join tree | `[customer#3] join ([p2] join store_sales)` | `[[p2] join store_sales] join customer#3` | `[customer#3] join m7` |
| changed by | | flip: `[p2] join store_sales` now builds | the build of MJ5's join on ss_customer_sk = c_customer_sk came out 553970 rows, est 158633.4 (q-error 3.49); flip: `customer#3` now builds |
| C_out | 5,737,914 B | 5,737,914 B (plan's order 5,737,914 B) | 13,251,467 B (previous order 13,251,467 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4.

m7 = `[p2] join store_sales`, 553,970 rows. p2 = `date_dim#3 (f)`, 366 rows.

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

Calls that left the order unchanged: DPhyp before the run; replan 2 (MJ1's missed build came out 3170 rows, est 1134.3).

### q39 MJ2

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[p0] join ([warehouse#1] join ([item#1] join inventory#1))` | `[warehouse#1] join ([[p0] join inventory#1] join item#1)` | `[m2] join ([item#1] join m3)` |
| changed by | | reordered by DPhyp | the build of MJ2's join on inv_item_sk = i_item_sk came out 180000 rows, est 4186.0 (q-error 43.00); flip: `item#1` now builds |
| C_out | 18,707,880 B | 434,608 B (plan's order 18,707,880 B) | 13,558,500 B (previous order 13,558,500 B) |

Calls that left the order unchanged: replan 2.

m2 = `warehouse#1`, 5 rows. m3 = `[p0] join inventory#1`, 180,000 rows. p0 = `date_dim#1 (f)`, 31 rows.

### q39 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p1] join ([warehouse#2] join ([item#2] join inventory#2))` | `[warehouse#2] join ([item#2] join ([p1] join inventory#2))` |
| changed by | | reordered by DPhyp |
| C_out | 28,033,367 B | 588,823 B (plan's order 28,033,367 B) |

Calls that left the order unchanged: replan 1.

p1 = `date_dim#2 (f)`, 28 rows.

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[item#1] join ([p0] join ([customer_address] join ([customer] join web_sales)))` | `[[[[p0] join web_sales] join customer] join item#1] join customer_address` | `[[item#1] join ([m1] join customer)] join customer_address` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ws_bill_customer_sk = c_customer_sk came out 21370 rows, est 6123.7 (q-error 3.49); flip: `item#1` now builds |
| C_out | 12,266,740 B | 1,023,535 B (plan's order 12,266,740 B) | 2,879,980 B (previous order 2,879,980 B) |

m1 = `[p0] join web_sales`, 21,370 rows. p0 = `date_dim (f)`, 91 rows.

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
| C_out | 7,494,543 B | 5,380,204 B (plan's order 7,494,543 B) |

### q46 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#1] join ([household_demographics (f)] join ([store (f)] join ([p0] join store_sales)))` | `[customer_address#1] join ([store (f)] join ([household_demographics (f)] join ([p0] join store_sales)))` |
| changed by | | reordered by DPhyp |
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

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p0] join ([customer_address (f)] join ([customer_demographics (f)] join ([store] join store_sales (f))))` | `[store] join ([p0] join ([customer_address (f)] join ([customer_demographics (f)] join store_sales (f))))` |
| changed by | | reordered by DPhyp |
| C_out | 16,299,981 B | 205,797 B (plan's order 16,299,981 B) |

p0 = `date_dim (f)`, 366 rows.

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
| changed by | | reordered by DPhyp | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 216 rows, est 6.4 (q-error 33.89); reordered by DPhyp | the build of MJ2's join on customer_sk = c_customer_sk came out 1637 rows, est 417.0 (q-error 3.93); reordered by DPhyp |
| C_out | 94,958 B | 60 B (plan's order 94,958 B) | 301,298 B (previous order 445,143 B) | 445,143 B (previous order 4,425,319 B) |

Calls that left the order unchanged: replan 2; replan 4 (MJ1's missed build came out 12 rows, est 216.0).

m1 = `[store] join customer_address`, 216 rows.

### q54 MJ2

| | plan | replan 2 |
|---|---|---|
| join tree | `[[p0] join ([item (f)] join union(catalog_sales, web_sales))] join customer` | `[[m2] join ([p0] join union(catalog_sales, web_sales))] join customer` |
| changed by | | the build of MJ2's join on i_item_sk = item_sk came out 417 rows, est 18.1 (q-error 23.04); reordered by DPhyp |
| C_out | 4,456 B | 77,945 B (previous order 102,683 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 3 (MJ2's missed build came out 1637 rows, est 417.0).

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

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_bill_addr_sk = ca_address_sk came out 12692 rows, est 1877.0 (q-error 6.76).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 4750 rows, est 303.1 (q-error 15.67).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_bill_addr_sk = ca_address_sk came out 6934 rows, est 1884.2 (q-error 3.68).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 2349 rows, est 304.3 (q-error 7.72).

### q56 MJ1

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[customer_address#1 (f)] join ([p0] join store_sales)] join item#2) semi join [item#1 (f)]` | `[[item#2 semi join [m5]] join ([p0] join store_sales)] join m6` |
| changed by | | flip: `[customer_address#1 (f)] join ([p0] join store_sales)` now builds | the build of MJ1's join on ca_address_sk = ss_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 115,430 B | 115,430 B (plan's order 115,430 B) | 124,016 B (previous order 149,777 B) |

Calls that left the order unchanged: replan 2 (MJ1's missed build came out 1339 rows, est 147.5).

m5 = `item#1 (f)`, 500 rows. m6 = `customer_address#1 (f)`, 18,246 rows. p0 = `date_dim#1 (f)`, 28 rows.

### q56 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#2 (f)] join ([p1] join catalog_sales)] join item#4) semi join [item#3 (f)]` | `([[[p1] join catalog_sales] join customer_address#2 (f)] join item#4) semi join [item#3 (f)]` |
| changed by | | flip: `[p1] join catalog_sales` now builds |
| C_out | 79,810 B | 79,810 B (plan's order 79,810 B) |

Calls that left the order unchanged: replan 1; replan 2.

p1 = `date_dim#2 (f)`, 28 rows.

### q56 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `([[customer_address#3 (f)] join ([p2] join web_sales)] join item#6) semi join [item#5 (f)]` | `([[[p2] join web_sales] join customer_address#3 (f)] join item#6) semi join [item#5 (f)]` |
| changed by | | flip: `[p2] join web_sales` now builds |
| C_out | 80,116 B | 80,116 B (plan's order 80,116 B) |

Calls that left the order unchanged: replan 1; replan 2.

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

| | plan | replan 1 | replan 2 | replan 3 |
|---|---|---|---|---|
| join tree | `[[agg(MJ2)] join agg(MJ3)] join agg(MJ4)` | `[[agg(MJ3)] join agg(MJ2)] join agg(MJ4)` | `[[agg(MJ4)] join agg(MJ3)] join agg(MJ2)` | `[agg(MJ2)] join ([agg(MJ4)] join agg(MJ3))` |
| changed by | | the build of MJ2's join on cs_item_sk = i_item_sk came out 7739 rows, est 469.3 (q-error 16.49); flip: `agg(MJ3)` now builds | the build of MJ3's join on ss_item_sk = i_item_sk came out 20063 rows, est 678.7 (q-error 29.56); reordered by DPhyp | the build of MJ4's join on ws_item_sk = i_item_sk came out 5574 rows, est 471.1 (q-error 11.83); flip: `agg(MJ2)` now builds |
| C_out | 1,825 B | 8,969 B (previous order 8,969 B) | 31,125 B (previous order 87,519 B) | 368,302 B (previous order 368,302 B) |

Calls that left the order unchanged: DPhyp before the run.

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

Calls that left the order unchanged: replan 1; replan 2 (MJ3's missed build came out 20063 rows, est 678.7); replan 3.

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
| join tree | `[[[store#1] join agg(MJ2)] join date_dim#2 (f)] join MJ3` | `[MJ3] join ([store#1] join ([date_dim#2 (f)] join agg(MJ2)))` |
| changed by | | reordered by DPhyp; flip: `MJ3` now builds |
| C_out | 10,263,328 B | 1,185,091 B (plan's order 10,263,328 B) |

### q59 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[store#2] join agg(MJ4)] join date_dim#4 (f)` | `[store#2] join ([date_dim#4 (f)] join agg(MJ4))` |
| changed by | | reordered by DPhyp |
| C_out | 9,145,825 B | 591,998 B (plan's order 9,145,825 B) |

## q60

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_bill_addr_sk = ca_address_sk came out 30857 rows, est 2011.1 (q-error 15.34).

A replan was refused, work outside its builds having started: the build of MJ2's join on cs_item_sk = i_item_sk came out 11552 rows, est 324.8 (q-error 35.57).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_bill_addr_sk = ca_address_sk came out 15632 rows, est 2018.8 (q-error 7.74).

A replan was refused, work outside its builds having started: the build of MJ3's join on ws_item_sk = i_item_sk came out 5932 rows, est 326.0 (q-error 18.20).

### q60 MJ1

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `([item#2] join ([customer_address#1 (f)] join ([p0] join store_sales))) semi join [item#1 (f)]` | `([[customer_address#1 (f)] join ([p0] join store_sales)] join item#2) semi join [item#1 (f)]` | `[item#2 semi join [m5]] join m7` |
| changed by | | flip: `[customer_address#1 (f)] join ([p0] join store_sales)` now builds | the build of MJ1's join on ss_item_sk = i_item_sk came out 22109 rows, est 1798.1 (q-error 12.30); reordered by DPhyp |
| C_out | 213,135 B | 213,135 B (plan's order 213,135 B) | 254,858 B (previous order 957,378 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 18246 rows, est 8074.0).

m5 = `item#1 (f)`, 1,860 rows. m6 = `customer_address#1 (f)`, 18,246 rows. m7 = `[m6] join ([p0] join store_sales)`, 22,109 rows. p0 = `date_dim#1 (f)`, 30 rows.

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
| join tree | `[item#1 (f)] join ([[[p0] join ([promotion (f)] join ([store#1 (f)] join store_sales#1))] join customer#1] join customer_address#1 (f))` | `[store#1 (f)] join ([[[promotion (f)] join ([item#1 (f)] join ([p0] join store_sales#1))] join customer#1] join customer_address#1 (f))` | `[m2] join ([[customer_address#1 (f)] join customer#1] join m3)` | `[m2] join ([[m3] join customer#1] join m4)` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ss_customer_sk = c_customer_sk came out 4387 rows, est 241.7 (q-error 18.15); reordered by DPhyp | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 20,723,026 B | 276,733 B (plan's order 20,723,026 B) | 159,805 B (previous order 170,631 B) | 206,666 B (previous order 361,134 B) |

m2 = `store#1 (f)`, 12 rows. m3 = `[promotion (f)] join ([item#1 (f)] join ([p0] join store_sales#1))`, 4,387 rows. m4 = `customer_address#1 (f)`, 18,246 rows. p0 = `date_dim#1 (f)`, 30 rows.

### q61 MJ2

| | plan | DPhyp before the run | replan 3 | replan 4 |
|---|---|---|---|---|
| join tree | `[item#2 (f)] join ([customer_address#2 (f)] join ([[p1] join ([store#2 (f)] join store_sales#2)] join customer#2))` | `[store#2 (f)] join ([[[item#2 (f)] join ([p1] join store_sales#2)] join customer#2] join customer_address#2 (f))` | `[m6] join ([[customer_address#2 (f)] join customer#2] join m7)` | `[m6] join ([m8] join ([m7] join customer#2))` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 8621 rows, est 491.6 (q-error 17.54); reordered by DPhyp | the build of MJ2's join on ca_address_sk = c_current_addr_sk came out 18246 rows, est 8074.0 (q-error 2.26); reordered by DPhyp |
| C_out | 12,486,552 B | 234,598 B (plan's order 12,486,552 B) | 187,409 B (previous order 335,312 B) | 406,124 B (previous order 423,516 B) |

Calls that left the order unchanged: replan 1; replan 2.

m6 = `store#2 (f)`, 12 rows. m7 = `[item#2 (f)] join ([p1] join store_sales#2)`, 8,621 rows. m8 = `customer_address#2 (f)`, 18,246 rows. p1 = `date_dim#2 (f)`, 30 rows.

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
| join tree | `[agg(MJ2)] join agg(MJ4)` | `[agg(MJ4)] join m16` |
| changed by | | the build of MJ4's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); flip: `agg(MJ4)` now builds |
| C_out | 671 B | 331 B (previous order 331 B) |

Calls that left the order unchanged: DPhyp before the run; replan 1; replan 2; replan 4.

m16 = `agg(MJ2)`, 37 rows.

### q64 MJ2

| | plan | DPhyp before the run | inside p2, replan 1 | inside p2, replan 2 | replan 1 | replan 2 |
|---|---|---|---|---|---|---|
| join tree | `[[[[[[[[[[[[[[[[agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)] join date_dim#1 (f)] join store#1] join customer#1] join date_dim#2] join date_dim#3] join customer_demographics#1] join customer_demographics#2] join promotion#1] join household_demographics#1] join household_demographics#2] join customer_address#1] join customer_address#2] join income_band#1] join income_band#2] join item#1 (f)` | `[item#1 (f)] join ([income_band#2] join ([income_band#1] join ([[[household_demographics#2] join ([household_demographics#1] join ([promotion#1] join ([[[date_dim#3] join ([date_dim#2] join ([[store#1] join ([date_dim#1 (f)] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1))] join customer_demographics#1] join customer_demographics#2)))] join customer_address#1] join customer_address#2)))` | `[m2] join ([m3] join ([[m4] join ([m5] join ([p0] join ([store_returns#1] join store_sales#1)))] join customer#1))` | `[[m2] join ([m3] join customer#1)] join m6` | `[[[income_band#1] join ([[income_band#2] join ([[[[m12] join p4] join customer_demographics#2] join promotion#1] join household_demographics#2)] join household_demographics#1)] join customer_address#1] join customer_address#2` | `[[[m13] join ([[m14] join ([[promotion#1] join m15] join household_demographics#2)] join household_demographics#1)] join customer_address#1] join customer_address#2` |
| changed by | | DPhyp at its budget, the plan's order kept; flip: `date_dim#1 (f)` now builds; flip: `store#1` now builds; flip: `date_dim#2` now builds; flip: `date_dim#3` now builds; flip: `promotion#1` now builds; flip: `household_demographics#1` now builds; flip: `household_demographics#2` now builds; flip: `income_band#1` now builds; flip: `income_band#2` now builds; flip: `item#1 (f)` now builds | the build of MJ2's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 52034 rows, est 13355.7 (q-error 3.90); reordered by DPhyp | the build of MJ2's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); reordered by DPhyp | the build of MJ2's join on ss_promo_sk = p_promo_sk came out 37 rows, est 6.4 (q-error 5.75); flip: `promotion#1` now builds |
| C_out | 51,700,850 B | 51,700,850 B (plan's order 51,700,850 B) | 18,024,522 B (previous order 23,730,821 B) | 17,828,278 B (previous order 24,234,836 B) | 13,775 B (previous order 12,200,149 B) | 40,912 B (previous order 40,912 B) |

m2 = `date_dim#3`, 73,049 rows. m3 = `date_dim#2`, 73,049 rows. m4 = `store#1`, 12 rows. m5 = `agg(MJ3) (f)`, 17,157 rows. m6 = `[m4] join ([m5] join ([p0] join ([store_returns#1] join store_sales#1)))`, 52,034 rows. m12 = `item#1 (f)`, 12 rows. m13 = `income_band#1`, 20 rows. m14 = `income_band#2`, 20 rows. m15 = `[[m12] join p4] join customer_demographics#2`, 37 rows. p0 = `date_dim#1 (f)`, 365 rows. p2 = `[date_dim#3] join ([date_dim#2] join ([[store#1] join ([p0] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1))`, 48,625 rows. p4 = `[p2] join customer_demographics#1`, 48,301 rows.

### q64 MJ4

| | plan | DPhyp before the run | inside p3, replan 1 | inside p3, replan 2 | replan 1 | replan 3 | replan 4 |
|---|---|---|---|---|---|---|---|
| join tree | `[[[[[[[[[[[[[[[[agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)] join date_dim#4 (f)] join store#2] join customer#2] join date_dim#5] join date_dim#6] join customer_demographics#3] join customer_demographics#4] join promotion#2] join household_demographics#3] join household_demographics#4] join customer_address#3] join customer_address#4] join income_band#3] join income_band#4] join item#2 (f)` | `[item#2 (f)] join ([income_band#4] join ([income_band#3] join ([[[household_demographics#4] join ([household_demographics#3] join ([promotion#2] join ([[[date_dim#6] join ([date_dim#5] join ([[store#2] join ([date_dim#4 (f)] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2))] join customer_demographics#3] join customer_demographics#4)))] join customer_address#3] join customer_address#4)))` | `[m7] join ([m8] join ([[m9] join ([m10] join ([p1] join ([store_returns#2] join store_sales#2)))] join customer#2))` | `[[m7] join ([m8] join customer#2)] join m11` | `[[[income_band#3] join ([[income_band#4] join ([[promotion#2] join ([[item#2 (f)] join p5] join customer_demographics#4)] join household_demographics#4)] join household_demographics#3)] join customer_address#3] join customer_address#4` | `[[[m17] join ([[m18] join ([[[[m20] join p5] join customer_demographics#4] join m19] join household_demographics#4)] join household_demographics#3)] join customer_address#3] join customer_address#4` | `[[[[[[[[m21] join customer_demographics#4] join m19] join household_demographics#4] join m18] join household_demographics#3] join m17] join customer_address#3] join customer_address#4` |
| changed by | | DPhyp at its budget, the plan's order kept; flip: `date_dim#4 (f)` now builds; flip: `store#2` now builds; flip: `date_dim#5` now builds; flip: `date_dim#6` now builds; flip: `promotion#2` now builds; flip: `household_demographics#3` now builds; flip: `household_demographics#4` now builds; flip: `income_band#3` now builds; flip: `income_band#4` now builds; flip: `item#2 (f)` now builds | the build of MJ4's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); reordered by DPhyp | the build of MJ4's join on ss_customer_sk = c_customer_sk came out 52717 rows, est 15853.8 (q-error 3.33); reordered by DPhyp | the build of MJ2's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); reordered by DPhyp | the build of MJ4's join on i_item_sk = ss_item_sk came out 12 rows, est 105.6 (q-error 8.80); flip: `[[m20] join p5] join customer_demographics#4` now builds | the build of MJ4's join on c_current_cdemo_sk = cd_demo_sk came out 3 rows, est 32.8 (q-error 10.94); flip: `[[[m21] join customer_demographics#4] join m19] join household_demographics#4` now builds; flip: `[[[[[m21] join customer_demographics#4] join m19] join household_demographics#4] join m18] join household_demographics#3` now builds |
| C_out | 51,700,850 B | 51,700,850 B (plan's order 51,700,850 B) | 21,371,263 B (previous order 28,121,452 B) | 17,934,314 B (previous order 24,552,943 B) | 123,598 B (previous order 12,449,498 B) | 14,045 B (previous order 14,045 B) | 760 B (previous order 760 B) |

Calls that left the order unchanged: replan 2.

m7 = `date_dim#6`, 73,049 rows. m8 = `date_dim#5`, 73,049 rows. m9 = `store#2`, 12 rows. m10 = `agg(MJ5) (f)`, 17,157 rows. m11 = `[m9] join ([m10] join ([p1] join ([store_returns#2] join store_sales#2)))`, 52,717 rows. m17 = `income_band#3`, 20 rows. m18 = `income_band#4`, 20 rows. m19 = `promotion#2`, 300 rows. m20 = `item#2 (f)`, 12 rows. m21 = `[m20] join p5`, 3 rows. p1 = `date_dim#4 (f)`, 366 rows. p3 = `[date_dim#6] join ([date_dim#5] join ([[store#2] join ([p1] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2))`, 49,549 rows. p5 = `[p3] join customer_demographics#3`, 49,247 rows.

### q64 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2451180 to 2451544 | 24 → 6 (4-8,23) |
| after join order | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |
| after join order | customer_demographics#1 | cd_demo_sk | p2.ss_cdemo_sk | 37 to 1920795 | 16 → 16 (0-15) |
| after join order | customer_demographics#3 | cd_demo_sk | p3.ss_cdemo_sk | 32 to 1920615 | 16 → 16 (0-15) |
| after join order | customer_demographics#2 | cd_demo_sk | p4.c_current_cdemo_sk | 17 to 1920750 | 16 → 16 (0-15) |
| after join order | customer_demographics#4 | cd_demo_sk | p5.c_current_cdemo_sk | 116 to 1920750 | 16 → 16 (0-15) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#4 (f)`, 366 rows. p2 = `[date_dim#3] join ([date_dim#2] join ([[store#1] join ([p0] join ([agg(MJ3) (f)] join ([store_returns#1] join store_sales#1)))] join customer#1))`, 48,625 rows, holding MJ2, MJ3. p3 = `[date_dim#6] join ([date_dim#5] join ([[store#2] join ([p1] join ([agg(MJ5) (f)] join ([store_returns#2] join store_sales#2)))] join customer#2))`, 49,549 rows, holding MJ4, MJ5. p4 = `[p2] join customer_demographics#1`, 48,301 rows, holding MJ2. p5 = `[p3] join customer_demographics#3`, 49,247 rows, holding MJ4.

Probe p2, replan 1: the build of MJ2's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); m2 = `date_dim#3`, 73,049 rows; m3 = `date_dim#2`, 73,049 rows; m4 = `store#1`, 12 rows; m5 = `agg(MJ3) (f)`, 17,157 rows.

Probe p2, replan 2: the build of MJ2's join on ss_customer_sk = c_customer_sk came out 52034 rows, est 13355.7 (q-error 3.90); m6 = `[m4] join ([m5] join ([p0] join ([store_returns#1] join store_sales#1)))`, 52,034 rows.

Probe p3, replan 1: the build of MJ4's join on cs_item_sk = ss_item_sk came out 17157 rows, est 3572.4 (q-error 4.80); m7 = `date_dim#6`, 73,049 rows; m8 = `date_dim#5`, 73,049 rows; m9 = `store#2`, 12 rows; m10 = `agg(MJ5) (f)`, 17,157 rows.

Probe p3, replan 2: the build of MJ4's join on ss_customer_sk = c_customer_sk came out 52717 rows, est 15853.8 (q-error 3.33); m11 = `[m9] join ([m10] join ([p1] join ([store_returns#2] join store_sales#2)))`, 52,717 rows.

## q65

MJ2 kept the plan's order: `[p0] join store_sales#1`.

MJ3 kept the plan's order: `[p1] join store_sales#2`.

### q65 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[store] join agg(MJ2)] join item] join agg(MJ3)` | `[[store] join ([agg(MJ3)] join agg(MJ2))] join item` |
| changed by | | reordered by DPhyp |
| C_out | 28,819,239 B | 5,327,456 B (plan's order 28,819,239 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 121 rows, est 21600.0).

### q65 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales#1 | ss_sold_date_sk | p0.d_date_sk | 2450815 to 2451179 | 24 → 6 (0-4,23) |
| plan as written | store_sales#2 | ss_sold_date_sk | p1.d_date_sk | 2450815 to 2451179 | 24 → 6 (0-4,23) |

p0 = `date_dim#1 (f)`, 365 rows. p1 = `date_dim#2 (f)`, 365 rows.

## q66

### q66 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[ship_mode#1 (f)] join ([time_dim#1 (f)] join ([p0] join ([warehouse#1] join web_sales)))` | `[warehouse#1] join ([time_dim#1 (f)] join ([p0] join ([ship_mode#1 (f)] join web_sales)))` |
| changed by | | reordered by DPhyp |
| C_out | 46,601,960 B | 2,284,031 B (plan's order 46,601,960 B) |

p0 = `date_dim#1 (f)`, 365 rows.

### q66 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[ship_mode#2 (f)] join ([time_dim#2 (f)] join ([p1] join ([warehouse#2] join catalog_sales)))` | `[warehouse#2] join ([time_dim#2 (f)] join ([p1] join ([ship_mode#2 (f)] join catalog_sales)))` |
| changed by | | reordered by DPhyp |
| C_out | 93,123,090 B | 4,562,501 B (plan's order 93,123,090 B) |

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[customer] join agg(MJ2)] join customer_address#2` | `[customer_address#2] join ([agg(MJ2)] join customer)` | `[[m2] join customer] join m1` |
| changed by | | flip: `agg(MJ2)` now builds; flip: `customer_address#2` now builds | the build of MJ1's join on ss_customer_sk = c_customer_sk came out 2348 rows, est 17491.9 (q-error 7.45); flip: `[m2] join customer` now builds |
| C_out | 2,056,963 B | 2,056,963 B (plan's order 2,056,963 B) | 10,419,198 B (previous order 10,419,198 B) |

m1 = `customer_address#2`, 50,000 rows. m2 = `agg(MJ2)`, 2,348 rows.

### q68 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[customer_address#1] join ([household_demographics (f)] join ([store (f)] join ([p0] join store_sales)))` | `[customer_address#1] join ([store (f)] join ([household_demographics (f)] join ([p0] join store_sales)))` |
| changed by | | reordered by DPhyp |
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

p0 = `date_dim#1 (f)`, 91 rows. p1 = `date_dim#2 (f)`, 91 rows. p2 = `date_dim#3 (f)`, 91 rows. p3 = `[[customer_address (f)] join customer] semi join ([p0] join store_sales)`, 652 rows, holding MJ1.

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
| C_out | 30,097 B | 30,097 B (plan's order 30,097 B) |

Calls that left the order unchanged: replan 1; replan 2 (MJ1's missed build came out 82 rows, est 2021.3).

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
| join tree | `[[[agg(MJ2) (f)] join agg(MJ3)] join agg(MJ4) (f)] join agg(MJ5)` | `[[[agg(MJ4) (f)] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` | `[[agg(MJ4) (f)] join ([agg(MJ2) (f)] join agg(MJ5))] join agg(MJ3)` | `[[[m5] join agg(MJ2) (f)] join agg(MJ5)] join agg(MJ3)` |
| changed by | | reordered by DPhyp | the build of MJ4's join on ws_bill_customer_sk = c_customer_sk came out 147024 rows, est 49124.4 (q-error 2.99); reordered by DPhyp | the build of MJ1's join on c_customer_id = c_customer_id came out 11552 rows, est 29404.8 (q-error 2.55); reordered by DPhyp |
| C_out | 5,777,961 B | 1,359,627 B (plan's order 5,777,961 B) | 3,679,599 B (previous order 3,751,457 B) | 1,598,637 B (previous order 2,371,340 B) |

Calls that left the order unchanged: replan 3; replan 4 (MJ1's missed build came out 4414 rows, est 17916.5); replan 5; replan 6 (MJ1's missed build came out 485 rows, est 6289.9).

m5 = `agg(MJ4) (f)`, 11,552 rows.

### q74 MJ2

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[p0] join ([customer#1] join store_sales#1)` | `[[p0] join store_sales#1] join customer#1` | `[customer#1] join m6` |
| changed by | | reordered by DPhyp | the build of MJ2's join on ss_customer_sk = c_customer_sk came out 546314 rows, est 133637.7 (q-error 4.09); flip: `customer#1` now builds |
| C_out | 51,478,017 B | 12,902,178 B (plan's order 51,478,017 B) | 35,057,467 B (previous order 35,057,467 B) |

Calls that left the order unchanged: replan 1; replan 2.

m6 = `[p0] join store_sales#1`, 546,314 rows. p0 = `date_dim#1 (f)`, 365 rows.

### q74 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p1] join ([customer#2] join store_sales#2)` | `[customer#2] join ([p1] join store_sales#2)` |
| changed by | | reordered by DPhyp |
| C_out | 60,939,517 B | 15,273,558 B (plan's order 60,939,517 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5; replan 6.

p1 = `date_dim#2 (f)`, 365 rows.

### q74 MJ4

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p2] join ([customer#3] join web_sales#1)` | `[[p2] join web_sales#1] join customer#3` |
| changed by | | reordered by DPhyp |
| C_out | 18,923,001 B | 4,742,761 B (plan's order 18,923,001 B) |

Calls that left the order unchanged: replan 1 (MJ4's missed build came out 147024 rows, est 49124.4).

p2 = `date_dim#3 (f)`, 365 rows.

### q74 MJ5

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[p3] join ([customer#4] join web_sales#2)` | `[[p3] join web_sales#2] join customer#4` |
| changed by | | reordered by DPhyp |
| C_out | 17,545,047 B | 4,397,398 B (plan's order 17,545,047 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4; replan 5 (MJ5's missed build came out 142500 rows, est 45547.2).

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

A replan was refused, work outside its builds having started: the build of MJ3's left join on ss_ticket_number = sr_ticket_number and ss_item_sk = sr_item_sk came out 52823 rows, est 15782.2 (q-error 3.35).

A replan was refused, work outside its builds having started: the build of MJ4's left join on ws_order_number = wr_order_number and ws_item_sk = wr_item_sk came out 13736 rows, est 4543.8 (q-error 3.02).

A replan was refused, work outside its builds having started: the build of MJ6's left join on ss_ticket_number = sr_ticket_number and ss_item_sk = sr_item_sk came out 53812 rows, est 13331.8 (q-error 4.04).

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

MJ1 kept the plan's order: `[date_dim#1] join ([item#1] join store_sales (f))`.

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
| C_out | 35,784,134 B | 35,784,134 B (plan's order 35,784,134 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 493526 rows, est 142779.6).

### q78 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | store_sales | ss_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 24 → 7 (8-13,23) |

p0 = `date_dim#2 (f)`, 366 rows.

## q79

MJ1 kept the plan's order: `[customer] join agg(MJ2)`.

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

A replan was refused, work outside its builds having started: the build of MJ3's join on i_item_sk = ws_item_sk came out 1055 rows, est 8981.3 (q-error 8.51).

A replan was refused, work outside its builds having started: the build of MJ2's join on i_item_sk = cs_item_sk came out 1055 rows, est 8981.3 (q-error 8.51).

### q80 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion#1 (f)] join ([item#1 (f)] join ([store] join ([p0] join (store_sales left join [store_returns]))))` | `([store] join ([promotion#1 (f)] join ([item#1 (f)] join ([p0] join store_sales)))) left join [store_returns]` |
| changed by | | reordered by DPhyp |
| C_out | 30,505,123 B | 1,016,736 B (plan's order 30,505,123 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 1055 rows, est 8981.3).

p0 = `date_dim#1 (f)`, 31 rows.

### q80 MJ2

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion#2 (f)] join ([item#2 (f)] join ([catalog_page] join ([p1] join (catalog_sales left join [catalog_returns]))))` | `([[promotion#2 (f)] join ([item#2 (f)] join ([p1] join catalog_sales))] join catalog_page) left join [catalog_returns]` |
| changed by | | reordered by DPhyp |
| C_out | 12,495,855 B | 414,970 B (plan's order 12,495,855 B) |

Calls that left the order unchanged: replan 1.

p1 = `date_dim#2 (f)`, 31 rows.

### q80 MJ3

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[promotion#3 (f)] join ([item#3 (f)] join ([web_site] join ([p2] join (web_sales left join [web_returns]))))` | `([web_site] join ([promotion#3 (f)] join ([item#3 (f)] join ([p2] join web_sales)))) left join [web_returns]` |
| changed by | | reordered by DPhyp |
| C_out | 12,498,065 B | 416,561 B (plan's order 12,498,065 B) |

Calls that left the order unchanged: replan 1.

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

| | plan | DPhyp before the run | replan 1 |
|---|---|---|---|
| join tree | `[[[agg(MJ2)] join customer] join customer_address#2 (f)] join agg(MJ3)` | `[customer_address#2 (f)] join ([[agg(MJ3)] join agg(MJ2)] join customer)` | `[[[agg(MJ3)] join agg(MJ2)] join customer] join m2` |
| changed by | | reordered by DPhyp | the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 2446 rows, est 950.0 (q-error 2.57); flip: `[[agg(MJ3)] join agg(MJ2)] join customer` now builds |
| C_out | 1,781,302 B | 431,547 B (plan's order 1,781,302 B) | 606,556 B (previous order 606,556 B) |

m2 = `customer_address#2 (f)`, 2,446 rows.

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
| C_out | 2,423 B | 1,666 B (plan's order 2,423 B) |

Calls that left the order unchanged: replan 1 (MJ1's missed build came out 196 rows, est 698.8).

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

p0 = `[customer_address (f)] join customer`, 947 rows.

### q84 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| after join order | customer_demographics | cd_demo_sk | p0.c_current_cdemo_sk | 3781 to 1915892 | 16 → 16 (0-15) |

p0 = `[customer_address (f)] join customer`, 947 rows, holding MJ1.

Probe p0, replan 1: the build of MJ1's join on ca_address_sk = c_current_addr_sk came out 464 rows, est 69.8 (q-error 6.65); m0 = `customer_address (f)`, 464 rows.

## q85

### q85 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[reason] join ([p0] join ([customer_address (f)] join ([[[web_page] join ([web_returns] join web_sales (f))] join customer_demographics#1 (f)] join customer_demographics#2)))` | `[[[[[[[web_sales (f)] join web_returns] join customer_demographics#1 (f)] join customer_address (f)] join p0] join customer_demographics#2] join web_page] join reason` |
| changed by | | reordered by DPhyp; flip: `web_sales (f)` now builds; flip: `[[[[[[web_sales (f)] join web_returns] join customer_demographics#1 (f)] join customer_address (f)] join p0] join customer_demographics#2] join web_page` now builds |
| C_out | 38,730 B | 20,115 B (plan's order 38,730 B) |

Calls that left the order unchanged: inside p1, replan 1 (MJ1's missed build came out 8678 rows, est 1637.0); inside p2, replan 1 (MJ1's missed build came out 22 rows, est 1.0).

p0 = `date_dim (f)`, 366 rows. p1 = `[web_sales (f)] join web_returns`, 835 rows. p2 = `[[[p1] join customer_demographics#1 (f)] join customer_address (f)] join p0`, 1 rows.

### q85 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales | ws_sold_date_sk | p0.d_date_sk | 2451545 to 2451910 | 6 → 2 (2-3) |
| after join order | customer_demographics#1 | cd_demo_sk | p1.wr_refunded_cdemo_sk | 1019 to 1906870 | 16 → 16 (0-15) |
| after join order | customer_demographics#2 | cd_demo_sk | p2.wr_returning_cdemo_sk | 1037564 | 16 → 1 (8) |

p0 = `date_dim (f)`, 366 rows. p1 = `[web_sales (f)] join web_returns`, 835 rows, holding MJ1. p2 = `[[[p1] join customer_demographics#1 (f)] join customer_address (f)] join p0`, 1 rows, holding MJ1.

Probe p1, replan 1: the build of MJ1's join on ws_item_sk = wr_item_sk and ws_order_number = wr_order_number came out 8678 rows, est 1637.0 (q-error 5.30); m1 = `web_sales (f)`, 8,678 rows.

Probe p2, replan 1: the build of MJ1's join on wr_refunded_addr_sk = ca_address_sk came out 22 rows, est 1.0 (q-error 22.00); m2 = `[p1] join customer_demographics#1 (f)`, 22 rows.

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

Calls that left the order unchanged: replan 1 (MJ2's missed build came out 142180 rows, est 49259.0).

p0 = `date_dim#1 (f)`, 366 rows.

### q87 MJ4

| | plan | DPhyp before the run | replan 3 |
|---|---|---|---|
| join tree | `[customer#2] join ([p1] join catalog_sales)` | `[[p1] join catalog_sales] join customer#2` | `[customer#2] join m5` |
| changed by | | flip: `[p1] join catalog_sales` now builds | the build of MJ4's join on cs_bill_customer_sk = c_customer_sk came out 287563 rows, est 73606.3 (q-error 3.91); flip: `customer#2` now builds |
| C_out | 2,662,408 B | 2,662,408 B (plan's order 2,662,408 B) | 6,878,769 B (previous order 6,878,769 B) |

Calls that left the order unchanged: replan 1; replan 2.

m5 = `[p1] join catalog_sales`, 287,563 rows. p1 = `date_dim#2 (f)`, 366 rows.

### q87 MJ5

| | plan | DPhyp before the run | replan 5 |
|---|---|---|---|
| join tree | `[customer#3] join ([p2] join store_sales)` | `[[p2] join store_sales] join customer#3` | `[customer#3] join m7` |
| changed by | | flip: `[p2] join store_sales` now builds | the build of MJ5's join on ss_customer_sk = c_customer_sk came out 553970 rows, est 158633.4 (q-error 3.49); flip: `customer#3` now builds |
| C_out | 5,737,914 B | 5,737,914 B (plan's order 5,737,914 B) | 13,251,467 B (previous order 13,251,467 B) |

Calls that left the order unchanged: replan 1; replan 2; replan 3; replan 4.

m7 = `[p2] join store_sales`, 553,970 rows. p2 = `date_dim#3 (f)`, 366 rows.

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
| join tree | `[web_page#1 (f)] join ([time_dim#1 (f)] join ([household_demographics#1 (f)] join web_sales#1))` | `[web_page#1 (f)] join ([household_demographics#1 (f)] join ([time_dim#1 (f)] join web_sales#1))` | `[household_demographics#1 (f)] join ([time_dim#1 (f)] join ([m0] join web_sales#1))` |
| changed by | | reordered by DPhyp | the build of MJ1's join on wp_web_page_sk = ws_web_page_sk came out 1 rows, est 3.2 (q-error 3.22); reordered by DPhyp |
| C_out | 1,222,960 B | 1,028,567 B (plan's order 1,222,960 B) | 415,656 B (previous order 1,024,960 B) |

m0 = `web_page#1 (f)`, 1 rows.

### q90 MJ2

| | plan | DPhyp before the run | replan 2 |
|---|---|---|---|
| join tree | `[web_page#2 (f)] join ([time_dim#2 (f)] join ([household_demographics#2 (f)] join web_sales#2))` | `[web_page#2 (f)] join ([household_demographics#2 (f)] join ([time_dim#2 (f)] join web_sales#2))` | `[household_demographics#2 (f)] join ([time_dim#2 (f)] join ([m2] join web_sales#2))` |
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

p0 = `date_dim (f)`, 30 rows. p1 = `[[p0] join catalog_returns] join customer`, 1,857 rows, holding MJ1.

## q92

MJ2 kept the plan's order: `[p0] join web_sales#2`.

### q92 MJ1

| | plan | DPhyp before the run |
|---|---|---|
| join tree | `[[[item (f)] join web_sales#1] join date_dim#1 (f)] join agg(MJ2)` | `[date_dim#1 (f)] join ([[item (f)] join agg(MJ2)] join web_sales#1)` |
| changed by | | reordered by DPhyp |
| C_out | 24,370 B | 4,057 B (plan's order 24,370 B) |

### q92 dynamic filters

| pass | scan | column | build | keys | row groups |
|---|---|---|---|---|---|
| plan as written | web_sales#2 | ws_sold_date_sk | p0.d_date_sk | 2451571 to 2451661 | 6 → 1 (2) |
| after join order | web_sales#1 | ws_sold_date_sk | p1.d_date_sk | 2451571 to 2451661 | 6 → 1 (2) |

p0 = `date_dim#2 (f)`, 91 rows. p1 = `date_dim#1 (f)`, 91 rows.

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

