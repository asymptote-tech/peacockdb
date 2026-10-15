SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM fact WHERE f_k = d_k AND f_qty > d_w)
