SELECT sum(t_v) AS s, min(t_v) AS lo, max(t_v) AS hi FROM tiny WHERE t_id + t_v < 0
