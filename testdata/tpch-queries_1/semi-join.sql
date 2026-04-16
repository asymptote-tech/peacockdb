SELECT * FROM orders
WHERE o_custkey IN (
    SELECT c_custkey FROM customer WHERE c_mktsegment = 'BUILDING'
);
