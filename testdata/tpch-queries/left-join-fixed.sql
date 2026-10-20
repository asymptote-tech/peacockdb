-- A left join that must stay below lineitem's join: lineitem joins on o_orderkey, a column of its
-- null-supplying side, so customer LEFT JOIN (orders JOIN lineitem) would keep every customer.
-- It stays Left because DataFusion simplifies an outer join only under a Filter directly above
-- it, and an inner join's equi-key is not one.
SELECT c_name, o_orderkey, l_linenumber
FROM customer
LEFT JOIN orders ON o_custkey = c_custkey
JOIN lineitem    ON l_orderkey = o_orderkey
WHERE l_shipdate = DATE '1995-03-15';
