SELECT * FROM orders
WHERE o_custkey NOT IN (
    SELECT c_custkey FROM customer WHERE c_mktsegment = 'BUILDING'
);
