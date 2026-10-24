SELECT f_id, f_s FROM fact WHERE f_id % 20 = 0 ORDER BY f_s DESC NULLS LAST, f_id
