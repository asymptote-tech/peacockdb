SELECT f_id FROM fact WHERE f_qty = 0 OR f_k NOT IN (SELECT s_y FROM sub WHERE s_z = f_qty)
