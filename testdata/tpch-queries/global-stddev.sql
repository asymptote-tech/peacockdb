-- A keyless stddev and var: the device's Welford over no keys (#216).
SELECT stddev_samp(l_quantity) AS sd, var_samp(l_quantity) AS v FROM lineitem;
