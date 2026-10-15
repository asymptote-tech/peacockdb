SELECT d_id, f_id FROM dim LEFT JOIN fact ON d_k = f_k AND f_qty > d_w
