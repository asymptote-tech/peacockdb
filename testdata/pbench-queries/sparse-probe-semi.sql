SELECT d_id FROM dim WHERE d_k IN (SELECT t_k FROM tiny)
