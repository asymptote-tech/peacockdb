SELECT * FROM orders o
JOIN lineitem l
  ON o.o_orderkey = l.l_orderkey
 AND l.l_shipdate BETWEEN o.o_orderdate AND o.o_orderdate + INTERVAL '90' DAY;
