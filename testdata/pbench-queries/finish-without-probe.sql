SELECT d_id FROM dim WHERE d_w = 0 OR EXISTS (SELECT 1 FROM tiny WHERE t_k = d_k)
