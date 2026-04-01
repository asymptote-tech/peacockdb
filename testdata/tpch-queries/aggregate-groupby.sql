SELECT l_returnflag, SUM(l_quantity) FROM lineitem GROUP BY l_returnflag;
