SELECT count(*), sum(l.l_quantity), sum(o.o_totalprice)
FROM orders o LEFT JOIN lineitem l ON o.o_orderkey = l.l_orderkey;
