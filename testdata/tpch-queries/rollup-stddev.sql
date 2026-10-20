-- A stddev and a var under a rollup: the grouping-set path builds the Welford triple (#280).
select l_returnflag, l_linestatus, stddev_samp(l_quantity), var_pop(l_quantity)
from lineitem
group by rollup (l_returnflag, l_linestatus);
