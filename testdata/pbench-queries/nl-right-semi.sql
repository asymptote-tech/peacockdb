SELECT f_id FROM fact WHERE EXISTS (SELECT 1 FROM tiny WHERE t_v > f_qty)
