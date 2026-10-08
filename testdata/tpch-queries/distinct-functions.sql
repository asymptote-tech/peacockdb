-- DISTINCT aggregates of the functions the lowering widens to, beside companions DataFusion's
-- SingleDistinctToGroupBy declines (count(*), avg), so the flag reaches the translator.
SELECT l_returnflag,
       count(DISTINCT l_quantity) AS distinct_qty,
       sum(DISTINCT l_quantity) AS sum_distinct_qty,
       avg(DISTINCT l_quantity) AS avg_distinct_qty,
       stddev(DISTINCT l_quantity) AS stddev_distinct_qty,
       count(*) AS n,
       avg(l_extendedprice) AS avg_price
FROM lineitem
GROUP BY l_returnflag;
