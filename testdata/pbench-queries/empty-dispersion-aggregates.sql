SELECT stddev_samp(t_v) AS sd, var_pop(t_v) AS vp FROM tiny WHERE t_id + t_v < 0
