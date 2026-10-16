SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL
