SELECT t_k, count(*) AS n FROM tiny WHERE t_id + t_v < 0 GROUP BY t_k
