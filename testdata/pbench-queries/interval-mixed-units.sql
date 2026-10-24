SELECT f_id,
       f_dt + INTERVAL '1 month 2 days 3 hours' AS dt,
       f_ts_ms + INTERVAL '1 month 2 days 3 hours' AS ms,
       f_ts_us - INTERVAL '1 month 2 days 3 hours' AS us,
       f_ts_ns + INTERVAL '1 month 2 days 3 hours' AS ns
FROM fact
WHERE f_id < 400
ORDER BY f_id
