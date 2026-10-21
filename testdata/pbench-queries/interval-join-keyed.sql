SELECT count(*) AS n
FROM fact f JOIN dim d ON f.f_k = d.d_k AND f.f_dt <= d.d_dt + INTERVAL '1' MONTH
