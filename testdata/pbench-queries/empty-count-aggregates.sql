SELECT count(*) AS n, count(t_v) AS nv, sum(t_v) AS s, avg(t_v) AS a FROM tiny WHERE t_id + t_v < 0
