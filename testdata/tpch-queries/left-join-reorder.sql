-- A left join below a filtered nation join, so it meets 150,000 customers where 6,100 would do:
-- under TES the inner join may move below it.
SELECT c_name, o_orderkey
FROM customer
LEFT JOIN orders ON o_custkey = c_custkey
JOIN nation      ON n_nationkey = c_nationkey
WHERE n_name = 'FRANCE';
