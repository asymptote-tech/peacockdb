SELECT f_id FROM fact WHERE NOT (f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub))
