-- GROUPING() over the full key list reads the grouping-set id itself, so its value and width
-- are the answer, not just a tag that keeps the sets apart (#65).
select n_regionkey, n_nationkey, grouping(n_regionkey, n_nationkey) as g, count(*)
from nation
group by rollup (n_regionkey, n_nationkey);
