SELECT f_k8, f_kb, sum(f_qty) AS q FROM fact GROUP BY ROLLUP (f_k8, f_kb)
