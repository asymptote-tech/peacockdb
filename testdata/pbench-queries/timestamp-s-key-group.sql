SELECT arrow_cast(f_ts_s, 'Timestamp(Second, None)') AS ts, count(*) AS n FROM fact GROUP BY 1
