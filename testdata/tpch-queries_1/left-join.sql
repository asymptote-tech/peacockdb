SELECT * FROM orders o LEFT JOIN lineitem l ON o.o_orderkey = l.l_orderkey;
