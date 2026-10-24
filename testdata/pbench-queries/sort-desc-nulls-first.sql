SELECT f_id, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_k DESC NULLS FIRST, f_id
