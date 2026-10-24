SELECT f_dt, f_dt + INTERVAL '1' MONTH AS next_month, f_dt - INTERVAL '1' MONTH AS prev_month
FROM fact
WHERE f_dt IN (DATE '2024-01-31', DATE '2024-02-29', DATE '2024-03-31', DATE '2024-05-31')
GROUP BY f_dt
ORDER BY f_dt
