SELECT f_id FROM fact WHERE NOT (f_k NOT IN (SELECT s_y FROM sub))
