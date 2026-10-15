SELECT f_id FROM fact WHERE NOT EXISTS (SELECT 1 FROM dim WHERE d_k = f_k AND d_w < f_qty)
