-- A semi join below a filtered nation join, so it meets 150,000 customers where 6,100 would do.
-- EXISTS in the outer WHERE would be decorrelated over the whole FROM and plan the semi join
-- above the nation join, the cheap order already; the derived table keeps it below.
SELECT c_custkey, c_name
FROM (SELECT * FROM customer
      WHERE EXISTS (SELECT 1 FROM orders WHERE o_custkey = c_custkey)) c
JOIN nation ON n_nationkey = c_nationkey
WHERE n_name = 'FRANCE';
