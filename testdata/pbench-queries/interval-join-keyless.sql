SELECT count(*) AS n
FROM fact f JOIN dim d ON f.f_dt + INTERVAL '7' DAY < d.d_dt
WHERE f.f_id < 50 AND d.d_id < 50
