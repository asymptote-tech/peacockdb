SELECT d.d_id, t.t_id FROM dim d FULL JOIN tiny t ON d.d_k IS NOT DISTINCT FROM t.t_k
