"""Tests for cost_model.py, on small synthetic inputs whose answers can be checked by hand."""
import itertools
import statistics
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import cost_model as cm  # noqa: E402

# Every scalar the model reads, at a value that keeps each term easy to follow.
NEUTRAL = {
    "price_h100_node": 4.5, "s3_efficiency_sequential": 1.0, "rate_h100_s3": 5.0, "rate_cpu128_s3": 5.0, "rate_node16_s3": 1.0,
    "rate_node8_s3": 0.5, "bucket_cap": 20.0, "s3_efficiency": 1.0, "decode_rate": 1000.0,
    "gpu_speedup": 10.0, "gpu_measured_scale": 1.0, "shuffle_rate_gpu_node": 1000.0,
    "ineligible_weight": 1.0, "scheduler_share": 0.0, "overhead_overlap": 1.0,
    "cpu_run_spread": 1.0, "cluster_overhead": 1.0, "s3_penalty_bandwidth_share": 1.0,
    "size_exponent": 1.0, "spill_factor": 1.0, "vram_gb": 80.0, "host_ram_gb": 200.0,
    "cores_gpu_node": 16.0, "cores_cpu": 128.0, "mem_cpu128_gb": 320.0, "mem_node16_gb": 40.0,
    "mem_node8_gb": 20.0, "ram_cpu128_gb": 512.0, "ram_node16_gb": 64.0, "ram_node8_gb": 32.0,
    "cpu_ram_usable": 0.5,
}
TIERS = {"local_nvme_x1_p5_4xlarge": cm.Tier("local_nvme_x1_p5_4xlarge", 3.9, 3840.0, 6.88)}


def scalars(**ranged):
    out = {k: cm.Input(k, v, v, v, "x", "modelled", "test") for k, v in NEUTRAL.items()}
    for k, (v, lo, hi) in ranged.items():
        out[k] = cm.Input(k, v, lo, hi, "x", "modelled", "test")
    return out


def query(name, local_s, s3_s, scan_gb, cols, gpu_s=None, eligible=1.0, driver_s=0.0):
    return cm.Query(name, local_s=local_s, s3_s=s3_s, scan_bytes=scan_gb * 1e9, shuffle_bytes=0.0,
                    driver_s=driver_s, sched_s=0.0, eligible=eligible, columns=frozenset(cols),
                    gpu_s=gpu_s)


def inputs(queries, batch_s, column_gb, ranged=None, dataset_gb=40.0):
    return cm.Inputs(
        scalars=scalars(**(ranged or {})),
        queries={"tpch": queries},
        column_bytes={("tpch",) + k: v * 1e9 for k, v in column_gb.items()},
        batch_s={("tpch",) + k: v for k, v in batch_s.items()},
        tiers=TIERS,
        dataset_bytes={"tpch": dataset_gb * 1e9},
    )


def two_queries(ranged=None):
    qs = [query("q1", 10.0, 12.0, 10, [("t", "a"), ("t", "b")], gpu_s=1.0),
          query("q2", 6.0, 8.0, 6, [("t", "a"), ("t", "c")], gpu_s=0.5)]
    batch = {("local", "sequential", "all"): 16.0, ("s3", "sequential", "all"): 20.0,
             ("local", "concurrent", "all"): 12.0, ("s3", "concurrent", "all"): 14.0}
    return inputs(qs, batch, {("t", "a"): 4, ("t", "b"): 6, ("t", "c"): 2}, ranged)


def test_nic_bound_scan():
    assert cm.gpu_seconds(overhead=0.0, wire_bytes=40e9, rate=5e9, load_bytes=0.0, decode_rate=1e9,
                          compute=1.0, shuffle_bytes=0.0, shuffle_rate=1e9, cpu_part=0.0) == 8.0


def test_spark_overhead_floor():
    assert cm.gpu_seconds(overhead=0.3, wire_bytes=0.0, rate=5e9, load_bytes=0.0, decode_rate=1e9,
                          compute=0.0, shuffle_bytes=0.0, shuffle_rate=1e9, cpu_part=0.0) == 0.3


def test_sharing_uses_computed_overlap():
    inp = two_queries()
    assert cm.shared_bytes(inp, "tpch", inp.queries["tpch"]) == pytest.approx(12e9)


def test_ratio_factors():
    b = cm.ratio(two_queries(), "tpch", "batched", "all", "concurrent", "1x128", 40e9)
    assert b.sharing_factor * b.device_factor == pytest.approx(b.mid)
    assert b.sharing_factor > 1.0


def test_parity_pricing():
    inp = two_queries()
    h100 = inp.scalars["price_h100_node"].value
    d8, d16 = cm.deployment(inp, "8x16"), cm.deployment(inp, "16x8")
    assert cm.deployment(inp, "1x128").price == d8.price == d16.price == h100
    assert d8.nic_total == pytest.approx(8 * NEUTRAL["rate_node16_s3"] * 1e9)
    assert d16.nic_total == pytest.approx(16 * NEUTRAL["rate_node8_s3"] * 1e9)


def test_untagged_input_rejected():
    with pytest.raises(ValueError):
        cm.Input("x", 1.0, 1.0, 1.0, "GB/s", "guess", "somewhere")


def test_measured_input_points_at_measurements():
    with pytest.raises(ValueError):
        cm.Input("x", 1.0, 1.0, 1.0, "GB/s", "measured", "llm-wiki/reports/benchmark-minimal.md")
    cm.Input("x", 1.0, 1.0, 1.0, "GB/s", "measured", "measurements/s3/summary.tsv")


def test_band_from_corners():
    # gpu_measured_scale raises GPU time, cpu_run_spread raises CPU time: the band's ends are the
    # mixed corners, and neither "all low" nor "all high" reaches them.
    ranged = {"gpu_measured_scale": (1.0, 0.5, 4.0), "cpu_run_spread": (1.0, 0.5, 2.0)}
    inp = two_queries(ranged)
    cell = ("tpch", "batched", "all", "concurrent", "1x128", 40e9)
    corners = {}
    for g, c in itertools.product((0.5, 4.0), (0.5, 2.0)):
        fixed = two_queries({"gpu_measured_scale": (g, g, g), "cpu_run_spread": (c, c, c)})
        corners[(g, c)] = cm.ratio(fixed, *cell).mid
    b = cm.ratio(inp, *cell)
    assert b.low == pytest.approx(min(corners.values())) == pytest.approx(corners[(4.0, 0.5)])
    assert b.high == pytest.approx(max(corners.values())) == pytest.approx(corners[(0.5, 2.0)])
    assert b.low < min(corners[(0.5, 0.5)], corners[(4.0, 2.0)])


def test_sensitivity_order():
    ranged = {"gpu_measured_scale": (1.0, 0.9, 1.1), "cpu_run_spread": (1.0, 0.5, 2.0)}
    rows = cm.sensitivity(two_queries(ranged), "tpch", "batched", "all", "concurrent", "1x128", 40e9)
    assert [r[0] for r in rows[:2]] == ["cpu_run_spread", "gpu_measured_scale"]
    swings = [abs(hi - lo) for _, lo, hi in rows]
    assert swings == sorted(swings, reverse=True)


def test_fits_tier():
    inp = two_queries()
    assert cm.fits_tier(inp, 60e9) == "vram"
    assert cm.fits_tier(inp, 150e9) == "host_ram"
    assert cm.fits_tier(inp, 1e12) == "local_nvme"
    assert cm.fits_tier(inp, 10e12) == "none"


def test_one_pass():
    qs = [query("q1", 4.0, 4.0, 0, [], gpu_s=1.0),
          query("q2", 3.0, 3.0, 0, [], gpu_s=2.0),
          query("q3", 9.0, 9.0, 0, [], gpu_s=1.0)]
    batch = {("local", "sequential", "all"): 16.0, ("s3", "sequential", "all"): 16.0}
    inp = inputs(qs, batch, {})
    per_query = [4.0 / 1.0, 3.0 / 2.0, 9.0 / 1.0]
    med = cm.ratio(inp, "tpch", "one-pass", "median", "sequential", "1x128", 40e9)
    worst = cm.ratio(inp, "tpch", "one-pass", "worst", "sequential", "1x128", 40e9)
    assert med.mid == pytest.approx(statistics.median(per_query))
    assert worst.mid == pytest.approx(min(per_query))
    assert med.sharing_factor == 1.0


def test_nic_rate_band_over_corners():
    # The NIC tier's rate is rate_h100_s3 x s3_efficiency; both ranged make it an array of corners.
    ranged = {"rate_h100_s3": (5.0, 2.5, 10.0), "s3_efficiency": (1.0, 0.5, 1.0)}
    b = cm.ratio(two_queries(ranged), "tpch", "batched", "all", "concurrent", "1x128", 40e9)
    # 12 GB shared: wire 1.2-9.6 s against 1.5 s of compute; CPU 14 s.
    assert b.low == pytest.approx(14.0 / 9.6)
    assert b.high == pytest.approx(14.0 / 1.5)


def test_an_empty_tier_sends_everything_over_the_nic():
    inp = two_queries()
    inp.tiers["no_local"] = cm.Tier("no_local", 0.0, 0.0, 4.5)
    cell = ("tpch", "batched", "all", "concurrent", "1x128", 40e9)
    assert cm.ratio(inp, *cell, tier="no_local").mid == pytest.approx(cm.ratio(inp, *cell).mid)


def test_one_pass_reads_at_the_sequential_efficiency():
    # One query alone reads S3 the way Comet's sequential run did; a batch reads like the
    # concurrent run. 10 GB at 5 GB/s x 0.5 is 4 s against a CPU's 8 s.
    qs = [query("q1", 8.0, 8.0, 10, [], gpu_s=0.1)]
    batch = {("local", "sequential", "all"): 8.0, ("s3", "sequential", "all"): 8.0}
    inp = inputs(qs, batch, {})
    inp.scalars["s3_efficiency_sequential"] = cm.Input("s3_efficiency_sequential", 0.5, 0.5, 0.5,
                                                       "x", "modelled", "test")
    b = cm.ratio(inp, "tpch", "one-pass", "median", "sequential", "1x128", 40e9)
    assert b.mid == pytest.approx(8.0 / 4.0)
