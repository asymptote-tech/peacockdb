SELECT t_id FROM tiny WHERE EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)
