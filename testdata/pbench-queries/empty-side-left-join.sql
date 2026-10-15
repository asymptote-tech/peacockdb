SELECT t.t_id, d.d_id FROM tiny t LEFT JOIN (SELECT * FROM dim WHERE false) d ON t.t_k = d.d_k
