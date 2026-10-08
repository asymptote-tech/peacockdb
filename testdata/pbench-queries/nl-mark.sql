SELECT t_id FROM tiny WHERE t_k = 0 OR EXISTS (SELECT 1 FROM fact WHERE t_v > f_qty)
