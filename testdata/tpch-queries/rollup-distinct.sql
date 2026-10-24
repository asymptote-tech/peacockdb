-- A DISTINCT aggregate under a grouping set. DataFusion's SingleDistinctToGroupBy declines
-- grouping sets, so the flag reaches the translator; the avg companion is q28's.
SELECT l_returnflag, l_linestatus,
       count(DISTINCT l_suppkey), avg(l_quantity), count(*)
FROM lineitem
GROUP BY ROLLUP(l_returnflag, l_linestatus);
