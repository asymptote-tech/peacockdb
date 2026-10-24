SELECT f_id, f_kb, f_k FROM fact WHERE f_id % 20 = 0 ORDER BY f_kb NULLS LAST, f_k DESC NULLS FIRST, f_id DESC
