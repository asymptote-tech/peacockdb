#!/usr/bin/env python3
"""Cost per query of a GPU (H100 node) Comet backend against Comet on CPU, at the human's price parity.

    python3 cost_model.py > cost.md

Reads `inputs.tsv` (every scalar input, tagged measured, published or modelled), the per-query
measurements under `measurements/`, `plans/{eligible,columns}.tsv`, and the tier tables in
`exotic.md` and `data-path.md`. The model and its terms are stated in cost.md's header, which
`header()` writes. A band is the min and max of the ratio over every corner of the inputs' low/high
ranges that the cell reads; the verdict rule is the spec's ("The bar for go").
"""
import csv
import itertools
import math
import re
import statistics
import sys
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
TAGS = ("measured", "published", "modelled")
NIC_TIER_S3_LIKE = ("s3", "caios")
CACHE_TIERS = {
    "vram", "host_ram_pcie5", "peer_ram_ib400_nebius", "peer_ram_ib400_azure_nd",
    "peer_ram_efa_p5_48xlarge", "memserver_rdma_onprem", "kove_sdm", "cxl_pool_switch",
    "cxl_expander_local", "gh200_c2c", "gb200_c2c_per_gpu", "gb200_nvl72_peer_hbm", "gb10_unified",
}
LOCAL_NVME_TIER = "local_nvme_x1_p5_4xlarge"
# A tier swaps the data path and keeps the H100's compute. GB10's GPU reads its memory at 239 GB/s
# against H200's 4,484 (benchmark-minimal.md), so its row would be the H100 at GB10's price.
NOT_H100_CLASS = {"gb10_unified"}
DEPLOYMENTS = {"1x128": (1, "cpu128"), "8x16": (8, "node16"), "16x8": (16, "node8")}


@dataclass(frozen=True)
class Input:
    name: str
    value: float
    low: float
    high: float
    unit: str
    tag: str
    source: str

    def __post_init__(self):
        if self.tag not in TAGS:
            raise ValueError(f"{self.name}: tag {self.tag!r} is not one of {TAGS}")
        if self.tag == "measured" and not self.source.startswith("measurements/"):
            raise ValueError(f"{self.name}: a measured input must point at measurements/")
        if not self.low <= self.value <= self.high:
            raise ValueError(f"{self.name}: value outside [low, high]")


@dataclass(frozen=True)
class Query:
    name: str
    local_s: float      # Comet, 1 x 128, RAM-resident (tmpfs), sequential, fastest run
    s3_s: float         # the same, in-region S3
    scan_bytes: float
    shuffle_bytes: float
    driver_s: float
    sched_s: float
    eligible: float     # split.md's GPU share of the query's operators
    columns: frozenset  # (table, column) pairs it scans
    gpu_s: float | None = None  # measured H100 execute time, where one exists


@dataclass(frozen=True)
class Tier:
    name: str
    rate: float      # GB/s to the GPU
    capacity: float  # GB
    price: float     # $ per GPU-hour


@dataclass
class Inputs:
    scalars: dict
    queries: dict       # dataset -> [Query] in run order
    column_bytes: dict  # (dataset, table, column) -> compressed bytes at sf40
    batch_s: dict       # (dataset, source, mode, batch) -> measured seconds, mean of runs
    tiers: dict
    dataset_bytes: dict  # dataset -> compressed bytes at sf40


@dataclass(frozen=True)
class Band:
    low: float
    mid: float
    high: float
    sharing_factor: float
    device_factor: float


@dataclass(frozen=True)
class Deployment:
    nodes: int
    price: float
    nic_total: object  # bytes/s
    s3_total: object   # bytes/s
    mem_gb: float      # executor memory per node
    ram_gb: float      # RAM per node


class Params:
    """Scalar lookups at their values or at overrides (arrays over corners), recording each
    name read, so a cell's band ranges over exactly the inputs it depends on."""

    def __init__(self, inputs: Inputs, overrides=None):
        self.inputs, self.over, self.used = inputs, overrides or {}, set()

    def __getitem__(self, name):
        self.used.add(name)
        return self.over.get(name, self.inputs.scalars[name].value)

    def fixed(self, name):
        return self.inputs.scalars[name].value


def gpu_seconds(overhead, wire_bytes, rate, load_bytes, decode_rate, compute, shuffle_bytes,
                shuffle_rate, cpu_part):
    """One unit of GPU work: the Spark floor, the slowest of wire, decode and compute, then the
    CPU-side shuffle and the operators the GPU cannot take."""
    return (overhead + np.maximum(np.maximum(wire_bytes / rate, load_bytes / decode_rate), compute)
            + shuffle_bytes / shuffle_rate + cpu_part)


def deployment(src, name) -> Deployment:
    p = src if isinstance(src, Params) else Params(src)
    nodes, kind = DEPLOYMENTS[name]
    nic = nodes * p[f"rate_{kind}_s3"] * 1e9
    s3 = nic if nodes == 1 else np.minimum(nic, p["bucket_cap"] * 1e9)
    return Deployment(nodes, p["price_h100_node"], nic, s3, p.fixed(f"mem_{kind}_gb"),
                      p.fixed(f"ram_{kind}_gb"))


def fits_tier(inputs: Inputs, data_bytes: float) -> str:
    s = inputs.scalars
    if data_bytes <= s["vram_gb"].value * 1e9:
        return "vram"
    if data_bytes <= s["host_ram_gb"].value * 1e9:
        return "host_ram"
    if data_bytes <= inputs.tiers[LOCAL_NVME_TIER].capacity * 1e9:
        return "local_nvme"
    return "none"


def shared_bytes(inputs: Inputs, ds: str, batch) -> float:
    """Bytes a batch moves when every table is read once for all its queries: the union of
    their columns, never more than the queries read alone."""
    alone = sum(q.scan_bytes for q in batch)
    if any(not q.columns for q in batch):
        return alone
    union = set().union(*(q.columns for q in batch))
    return min(alone, sum(inputs.column_bytes[(ds,) + c] for c in union))


# --- the two sides -------------------------------------------------------------------------

def overhead(p, q):
    return np.minimum(q.driver_s + p["scheduler_share"] * q.sched_s, q.local_s)


def tier_spec(p, inputs, tier, eta="s3_efficiency"):
    """(rate bytes/s, capacity bytes, price $/h, decode on the GPU?) for a tier name. `vram` holds
    decoded columns, so its rate is None: no wire term. `eta` names the S3 efficiency: a batch
    reads like Comet's concurrent run, one query alone like its sequential run."""
    if tier == "nic":
        return p["rate_h100_s3"] * 1e9 * p[eta], math.inf, p["price_h100_node"], True
    if tier == "vram":
        return None, p.fixed("vram_gb") * 1e9, p["price_h100_node"], False
    t = inputs.tiers[LOCAL_NVME_TIER if tier == "local-nvme" else tier]
    rate = t.rate * 1e9
    if any(k in t.name for k in NIC_TIER_S3_LIKE):
        rate = rate * p[eta]
    return rate, t.capacity * 1e9, t.price, True


def gpu_device_time(p, inputs, tier, wire, data_bytes, compute, eta="s3_efficiency"):
    """max(wire, decode, compute) for `wire` bytes: a tier smaller than the data serves its share
    and the rest comes over the NIC."""
    rate, cap, _, decode = tier_spec(p, inputs, tier, eta)
    f = 1.0 if cap >= data_bytes else cap / data_bytes
    t_wire = 0.0 if rate is None or f == 0 else wire * f / rate
    if f < 1.0:
        t_wire = t_wire + wire * (1 - f) / (p["rate_h100_s3"] * 1e9 * p[eta])
    load = wire if decode else wire * (1 - f)
    return gpu_seconds(0.0, t_wire, 1.0, load, p["decode_rate"] * 1e9, compute, 0.0, 1.0, 0.0)


def gpu_query_terms(p, q, s):
    """(overhead, compute, cpu_part) of one query at `s` times sf40."""
    ovh = overhead(p, q)
    work = q.local_s - ovh
    if q.gpu_s is not None:
        compute = q.gpu_s * p["gpu_measured_scale"]
    else:
        compute = work * q.eligible / p["gpu_speedup"]
    cpu_part = 0.0
    if q.eligible < 1.0:
        cpu_part = (work * np.minimum(1.0, (1 - q.eligible) * p["ineligible_weight"])
                    * p["cores_cpu"] / p["cores_gpu_node"])
    if s != 1.0:
        grow = s ** p["size_exponent"]
        compute, cpu_part = compute * grow, cpu_part * grow
        if q.shuffle_bytes * s > p.fixed("host_ram_gb") * 1e9:
            compute = compute * p["spill_factor"]
    return ovh, compute, cpu_part


def gpu_batch(p, inputs, ds, batch, tier, data_bytes, s, shared=True):
    terms = [gpu_query_terms(p, q, s) for q in batch]
    ovh = sum(t[0] for t in terms)
    if len(batch) > 1:
        ovh = ovh * p["overhead_overlap"]
    wire = (shared_bytes(inputs, ds, batch) if shared else sum(q.scan_bytes for q in batch)) * s
    device = gpu_device_time(p, inputs, tier, wire, data_bytes, sum(t[1] for t in terms))
    shuffle = sum(q.shuffle_bytes for q in batch) * s / (p["shuffle_rate_gpu_node"] * 1e9)
    return ovh + device + shuffle + sum(t[2] for t in terms)


def cpu_scaled(p, dep, s, fixed, t_local, t_source, shuffle_sum, shuffle_max):
    """CPU seconds on `dep` at `s` times sf40, from the 1 x 128 measurement: the fixed Spark part,
    the work (scaled by size, cluster overhead and spill), the S3 penalty over local (shrinking
    with more aggregate S3 bandwidth by its bandwidth share) and, on a cluster, shuffle over NICs."""
    fixed = np.minimum(fixed, t_local)
    work = t_local - fixed
    if s != 1.0:
        work = work * s ** p["size_exponent"]
    if dep.nodes > 1:
        work = work * p["cluster_overhead"]
    if shuffle_max * s / dep.nodes > dep.mem_gb * 1e9:
        work = work * p["spill_factor"]
    penalty = np.maximum(t_source - t_local, 0.0) * s
    if dep.nodes > 1:
        penalty = penalty * (p["rate_cpu128_s3"] * 1e9 / dep.s3_total) ** p["s3_penalty_bandwidth_share"]
        work = work + shuffle_sum * s * (dep.nodes - 1) / dep.nodes / dep.nic_total
    return fixed + work + penalty


def cpu_batch_measured(p, inputs, ds, source, mode, batch):
    key = (ds, source, mode, batch)
    if key in inputs.batch_s:
        return inputs.batch_s[key] * p["cpu_run_spread"]
    return p[f"cpu128_{source}_{mode}_{batch}_s.{ds}"]


def cpu_source(p, inputs, tier, dep, data_bytes):
    """A cache tier is compared with Comet reading RAM-resident data, while that data fits the
    deployment's RAM; every other tier with Comet reading S3."""
    cache = tier in CACHE_TIERS
    fits = data_bytes <= dep.nodes * dep.ram_gb * p.fixed("cpu_ram_usable") * 1e9
    return "local" if cache and fits else "s3"


def cpu_batch(p, inputs, ds, batch, key, mode, dep, source, s, shares=False):
    t_local = cpu_batch_measured(p, inputs, ds, "local", mode, key)
    t_src = t_local if source == "local" else cpu_batch_measured(p, inputs, ds, "s3", mode, key)
    if shares:
        cut = shared_bytes(inputs, ds, batch) / sum(q.scan_bytes for q in batch)
        t_src = t_local + (t_src - t_local) * cut
    fixed = sum(overhead(p, q) for q in batch)
    return cpu_scaled(p, dep, s, fixed, t_local, t_src, sum(q.shuffle_bytes for q in batch),
                      max(q.shuffle_bytes for q in batch))


def evaluate(p, inputs, ds, workload, batch, baseline, dep_name, data_bytes, tier="nic",
             cpu_shares=False):
    """(ratio, sharing_factor, device_factor) at the values in `p`."""
    s = data_bytes / inputs.dataset_bytes[ds]
    dep = deployment(p, dep_name)
    price = p["price_h100_node"] / tier_spec(p, inputs, tier)[2]
    source = cpu_source(p, inputs, tier, dep, data_bytes)
    qs = inputs.queries[ds]
    if workload == "one-pass":
        ratios = []
        for q in qs:
            ovh, compute, cpu_part = gpu_query_terms(p, q, s)
            t_gpu = (ovh + gpu_device_time(p, inputs, tier, q.scan_bytes * s, data_bytes, compute,
                                           "s3_efficiency_sequential")
                     + q.shuffle_bytes * s / (p["shuffle_rate_gpu_node"] * 1e9) + cpu_part)
            spread = p["cpu_run_spread"]
            t_src = q.local_s if source == "local" else q.s3_s
            t_cpu = cpu_scaled(p, dep, s, ovh, q.local_s * spread, t_src * spread,
                               q.shuffle_bytes, q.shuffle_bytes)
            ratios.append(t_cpu / t_gpu * price)
        stack = np.stack(np.broadcast_arrays(*ratios))
        r = np.median(stack, axis=0) if batch == "median" else stack.min(axis=0)
        return r, 1.0, r
    chosen = qs if batch == "all" else qs[:int(batch)]
    key = "all" if batch == "all" else str(batch)
    if baseline != "sequential" and key != "all":
        raise ValueError("batches 3, 10 and 20 have only the sequential baseline")
    modes = ("sequential", "concurrent") if baseline == "faster" else (baseline,)
    t_cpu = [cpu_batch(p, inputs, ds, chosen, key, m, dep, source, s, cpu_shares) for m in modes]
    t_cpu = t_cpu[0] if len(t_cpu) == 1 else np.minimum(*t_cpu)
    shared = gpu_batch(p, inputs, ds, chosen, tier, data_bytes, s, shared=True)
    alone = gpu_batch(p, inputs, ds, chosen, tier, data_bytes, s, shared=False)
    return t_cpu / shared * price, alone / shared, t_cpu / alone * price


def ranged_used(inputs, used):
    return [n for n in sorted(used) if inputs.scalars[n].low < inputs.scalars[n].high]


def ratio(inputs, ds, workload, batch, baseline, dep_name, data_bytes, tier="nic", **kw) -> Band:
    p = Params(inputs)
    mid, sharing, device = evaluate(p, inputs, ds, workload, batch, baseline, dep_name, data_bytes,
                                    tier, **kw)
    names = ranged_used(inputs, p.used)
    corners = list(itertools.product(*[(inputs.scalars[n].low, inputs.scalars[n].high)
                                       for n in names]))
    if names:
        over = {n: np.array([c[i] for c in corners]) for i, n in enumerate(names)}
        r = evaluate(Params(inputs, over), inputs, ds, workload, batch, baseline, dep_name,
                     data_bytes, tier, **kw)[0]
        low, high = float(np.min(r)), float(np.max(r))
    else:
        low = high = float(mid)
    return Band(low, float(mid), high, float(sharing), float(device))


def sensitivity(inputs, ds, workload, batch, baseline, dep_name, data_bytes, tier="nic", **kw):
    """(input, ratio at its low, ratio at its high), the others at their values, largest swing
    first. The top row is the measurement that would settle a conditional go."""
    p = Params(inputs)
    evaluate(p, inputs, ds, workload, batch, baseline, dep_name, data_bytes, tier, **kw)
    rows = []
    for n in ranged_used(inputs, p.used):
        s = inputs.scalars[n]
        at = [float(evaluate(Params(inputs, {n: v}), inputs, ds, workload, batch, baseline,
                             dep_name, data_bytes, tier, **kw)[0]) for v in (s.low, s.high)]
        rows.append((n, at[0], at[1]))
    return sorted(rows, key=lambda r: -abs(r[2] - r[1]))


def verdict(b: Band) -> str:
    if b.low >= 2.0:
        return "go"
    if b.high >= 2.0:
        return "conditional go"
    return "no-go"


# --- loading -------------------------------------------------------------------------------

def tsv(path):
    with open(path) as f:
        return list(csv.DictReader(f, delimiter="\t"))


def tsv_blocks(md_path, header_start="tier\t"):
    text = Path(md_path).read_text()
    out = []
    for block in re.findall(r"```tsv\n(.*?)```", text, re.S):
        if block.startswith(header_start):
            out += list(csv.DictReader(block.splitlines(), delimiter="\t"))
    return out


def load_inputs(root: Path = HERE) -> Inputs:
    scalars, gpu_ms, tables = {}, {}, []
    for r in tsv(root / "inputs.tsv"):
        if r["unit"] == "table":
            tables.append(r)
            continue
        inp = Input(r["name"], float(r["value"]), float(r["low"]), float(r["high"]), r["unit"],
                    r["tag"], r["source"])
        if r["name"].startswith("gpu_h100_ms."):
            _, ds, q = r["name"].split(".")
            gpu_ms[(ds, q)] = inp.value
        scalars[inp.name] = inp
    summary = tsv(root / "measurements/comet/summary.tsv")
    batch_runs, per_query = {}, {}
    for r in summary:
        cfg = r["config"]
        if cfg.startswith("cal-") or cfg.endswith("-cache"):
            continue
        batch_runs.setdefault((r["dataset"], r["source"], r["mode"], r["batch"]), []).append(
            float(r["batch_ms"]) / 1000)
        if r["mode"] == "sequential" and r["batch"] == "all":
            for item in r["per_query_ms"].split():
                q, ms = item.split(":")
                per_query.setdefault((r["dataset"], r["source"], q), []).append(float(ms) / 1000)
    batch_s = {k: statistics.mean(v) for k, v in batch_runs.items()}
    stats = {}
    for r in tsv(root / "measurements/comet/bytes.tsv"):
        if "-local-sequential-" in r["config"] and not r["config"].endswith("-cache"):
            k = (r["dataset"], r["query"])
            old = stats.get(k)
            row = (int(r["scan_bytes"]), int(r["shuffle_bytes"]), int(r["driver_ms"]) / 1000,
                   int(r["scheduler_ms"]) / 1000)
            stats[k] = row if old is None else (row[0], row[1], min(old[2], row[2]),
                                                 min(old[3], row[3]))
    eligible = {(r["dataset"], r["query"]): float(r["eligible_fraction"])
                for r in tsv(root / "plans/eligible.tsv")}
    columns = {}
    for r in tsv(root / "plans/columns.tsv"):
        columns.setdefault((r["dataset"], r["query"]), set()).add((r["table"], r["column"]))
    column_bytes, dataset_bytes = {}, {}
    for r in tsv(root / "measurements/footers/sf40.tsv"):
        column_bytes[(r["dataset"], r["table"], r["column"])] = int(r["compressed_bytes"])
        dataset_bytes[r["dataset"]] = dataset_bytes.get(r["dataset"], 0) + int(r["compressed_bytes"])
    queries = {}
    for r in summary:
        if r["config"] in ("tpch-local-sequential-ball", "tpcds-local-sequential-ball") \
                and r["batch"] == "all" and r["run"] == "1":
            ds = r["dataset"]
            queries[ds] = []
            for item in r["per_query_ms"].split():
                q = item.split(":")[0]
                scan, shuffle, drv, sch = stats[(ds, q)]
                queries[ds].append(Query(
                    q, min(per_query[(ds, "local", q)]), min(per_query[(ds, "s3", q)]), scan,
                    shuffle, drv, sch, eligible[(ds, q)], frozenset(columns.get((ds, q), ())),
                    gpu_ms[(ds, q)] / 1000 if (ds, q) in gpu_ms else None))
    tiers = {}
    for r in tsv_blocks(root / "exotic.md") + tsv_blocks(root / "data-path.md"):
        tiers[r["tier"]] = Tier(r["tier"], float(r["GB_per_s_to_GPU"]), float(r["capacity_GB"]),
                                float(r["price_per_hour"]))
    inputs = Inputs(scalars, queries, column_bytes, batch_s, tiers, dataset_bytes)
    check_tables(inputs, tables)
    return inputs


TABLE_TOTALS = {
    "cpu_local_s": lambda qs: sum(q.local_s for q in qs),
    "cpu_s3_s": lambda qs: sum(q.s3_s for q in qs),
    "scan_bytes": lambda qs: sum(q.scan_bytes for q in qs),
    "shuffle_bytes": lambda qs: sum(q.shuffle_bytes for q in qs),
    "driver_s": lambda qs: sum(q.driver_s for q in qs),
    "scheduler_s": lambda qs: sum(q.sched_s for q in qs),
    "eligible_fraction": lambda qs: statistics.mean(q.eligible for q in qs),
}


def check_tables(inputs, tables):
    """inputs.tsv's per-query table rows state each table's total; a drift between that row and
    the files the model reads is an error, not a silent change of answer."""
    for r in tables:
        if r["tag"] not in TAGS:
            raise ValueError(f"{r['name']}: tag {r['tag']!r}")
        name, ds = r["name"].split(".")
        if name == "footer_bytes":
            got = inputs.dataset_bytes[ds]
        else:
            got = TABLE_TOTALS[name](inputs.queries[ds])
        if not math.isclose(got, float(r["value"]), rel_tol=2e-3):
            raise ValueError(f"{r['name']}: inputs.tsv says {r['value']}, files give {got}")


# --- cost.md --------------------------------------------------------------------------------

def sizes(inputs, ds):
    b = inputs.dataset_bytes[ds]
    return [("sf40", b), ("sf200", 5 * b), ("1 TB", 1e12), ("10 TB", 10e12)]


def headline_sizes(inputs, ds):
    out = [sizes(inputs, ds)[0]]
    out.append(next(x for x in sizes(inputs, ds) if fits_tier(inputs, x[1]) in ("local_nvme", "none")))
    return out


def fmt(b: Band, factors=True):
    s = f"{b.mid:.2f} [{b.low:.2f}-{b.high:.2f}]"
    return s + (f" s{b.sharing_factor:.2f} d{b.device_factor:.2f}" if factors else "")


ROWS = [("batched", "3", "sequential"), ("batched", "10", "sequential"),
        ("batched", "20", "sequential"), ("batched", "all", "sequential"),
        ("batched", "all", "concurrent"), ("batched", "all", "faster"),
        ("one-pass", "median", "sequential"), ("one-pass", "worst", "sequential")]


def header(inputs):
    return """# Cost model: GPU Comet backend against Comet on CPU

Generated by `python3 cost_model.py > cost.md`; never edited by hand. Inputs: `inputs.tsv`, the
per-query tables under `measurements/` and `plans/`, the tier tables in `exotic.md` and
`data-path.md`. Tests: `python3 -m pytest test_cost_model.py`.

## The model

**Ratio** = CPU cost per query / GPU cost per query. At the human's parity one H100 node, one
128-core node, eight 16-core and sixteen 8-core nodes all cost the H100 node's hourly price, so the
ratio is CPU seconds / GPU seconds; a tier at another price multiplies it by $4.50 / its price.

**GPU side, one H100 node** (16 cores), per query q, or per batch with the sums over its queries:

    T_gpu = overhead + max(wire / rate(tier), wire / decode_rate, compute) + shuffle / shuffle_rate + cpu_part

- `overhead(q) = driver_ms + scheduler_share x scheduler_ms` from `bytes.tsv` (measured; the share is
  modelled because `scheduler_ms` sums gaps over stages that run in parallel), capped at the
  query's own time. A batch pays `overhead_overlap` of the sum: concurrent queries plan and
  schedule under one another's device work.
- `wire(q)` = the query's measured scan bytes. A batch reads each table once for all its queries:
  the union of their columns' compressed bytes from the sf40 footers (`measurements/footers/`),
  never more than the queries read alone. Decode handles the same bytes.
- `compute(q)`: the H100's measured execute time for TPC-H q1, q3, q6, q8 (benchmark-minimal.md,
  8 x H100 table, G=1; published) x `gpu_measured_scale`. Elsewhere `(Comet time - overhead) x
  eligible_fraction / gpu_speedup`, the speedup band spanning those four queries' DuckDB-to-GPU
  ratios (modelled).
- `cpu_part(q) = (Comet time - overhead) x min(1, (1 - eligible_fraction) x ineligible_weight) x 128/16`:
  the operators split.md leaves on the CPU, on 16 cores instead of 128.
- `shuffle / shuffle_rate`: Comet's shuffle stays CPU-side (Arrow IPC + lz4 on 16 cores). One node
  shuffles inside the box, so this term replaces the plan's `shuffle_bytes / NIC`.
- `rate(tier)`: `nic` is the H100 VM's S3 rate x `s3_efficiency` (a batch reads like Comet's
  concurrent run; one-pass takes `s3_efficiency_sequential`). Other tiers come from the tier
  tables; a tier smaller than the data serves its share and the rest comes over the NIC. `vram`
  holds decoded columns, so it skips wire and decode.

**CPU side**, from the 1 x 128 measurements (Comet 1.2.0, one executor on 128 cores). "Local" runs
read RAM-resident data on tmpfs, the strictest local baseline; S3 runs read in-region eu-west1
(measured 1.16 GB/s at 4-16 threads), so no in-region correction is modelled:

    T_cpu = fixed + work x size^a x cluster_overhead x spill + penalty x size x (R_128 / R_dep)^b + shuffle x (k-1)/k / NIC_dep

- `fixed` = the same Spark overhead as the GPU's; `work` = measured local time less `fixed`.
- `penalty` = measured S3 time less local time on 1 x 128 (the S3 runs' cost over RAM-resident
  data); `b` = `s3_penalty_bandwidth_share`, how much of it shrinks with more aggregate S3
  bandwidth. `R_dep` = k x the per-node rate, capped by the Standard bucket's 20 GB/s.
- 8 x 16 and 16 x 8 are modelled from 1 x 128: the same 128 cores, `cluster_overhead`, the S3
  penalty scaled by bandwidth, and shuffle over the nodes' NICs. Their NIC rates are the assumed
  ranges of `data-path.md` "Rates for the cost model" (Nebius states none), tagged `modelled`.
- A cache tier (RAM-class, `vram`) is compared with Comet's RAM-resident runs while the data fits
  half the deployment's RAM; every other tier with Comet's S3 runs.
- TPC-DS ran concurrently from local only; its concurrent S3 time is a modelled input.

**Sizes.** Bytes scale with size; Comet's work and the GPU's compute scale as size^`size_exponent`
(Comet's published runs: core time grows about as SF^0.5-0.9); the Spark overhead does not.
`spill_factor` applies to CPU work where one query's shuffle per node exceeds executor memory,
and to GPU compute where it exceeds the H100 node's host RAM (peacockdb has no spill today).

**Batches** are prefixes of the run order (TPC-H q1-q22, the 82 TPC-DS queries in file order).
**One-pass** is the median, and the worst, of the per-query ratios against Comet's sequential run.
**sharing_factor** (`s`) = GPU time with each query reading alone / with the shared scan;
**device_factor** (`d`) = CPU time / GPU time reading alone; `s x d` = the ratio.

**Band** = min and max of the ratio over every corner of the low/high ranges of the inputs the cell
reads. **Verdict** (the spec's rule): go if the band's low end is at least 2; a band straddling 2 is
a conditional go, settled by the sensitivity table's top row; no-go if its high end is below 2.
Tags: `measured` = this task's files under `measurements/`; `published` = a source document,
external or an earlier in-repo report; `modelled` = an assumption or a derived estimate.
"""


def main():
    inp = load_inputs()
    out = [header(inp)]
    out.append("## Headline cells\n\nBatch \"all\", the faster CPU baseline, parity with eight 16-core "
               "nodes, the NIC tier, per dataset at sf40 and at the first size past the H100 node's "
               "host RAM.\n\n| dataset | size | fits | ratio [band] | sharing | device | verdict |\n"
               "|---|---|---|---|--:|--:|---|")
    heads = []
    for ds in ("tpch", "tpcds"):
        for label, size in headline_sizes(inp, ds):
            b = ratio(inp, ds, "batched", "all", "faster", "8x16", size)
            heads.append((ds, label, size, b))
            out.append(f"| {ds} | {label} ({size / 1e9:.0f} GB) | {fits_tier(inp, size)} | "
                       f"{b.mid:.2f} [{b.low:.2f}-{b.high:.2f}] | {b.sharing_factor:.2f} | "
                       f"{b.device_factor:.2f} | {verdict(b)} |")
    out.append("")
    out.append("## Sensitivity of the headline cells\n\nEach input moved to its low and high end "
               "with the others at their values; the largest swing first. The top row is the "
               "settling measurement.\n")
    for ds, label, size, b in heads:
        out.append(f"**{ds} {label}** (mid {b.mid:.2f})\n\n| input | ratio at low | ratio at high | tag |\n|---|--:|--:|---|")
        for n, lo, hi in sensitivity(inp, ds, "batched", "all", "faster", "8x16", size)[:8]:
            out.append(f"| `{n}` | {lo:.2f} | {hi:.2f} | {inp.scalars[n].tag} |")
        out.append("")
    out.append(inputs_table(inp))
    out += ratio_tables(inp)
    out += shared_cpu_table(inp)
    out += tier_table(inp)
    out += stall_table(inp)
    out += beside_table(inp)
    print("\n".join(out))


def inputs_table(inp):
    rows = ["## Inputs\n\nFrom `inputs.tsv`; per-query tables are summarised by their totals.\n",
            "| input | value | low | high | unit | tag | source |", "|---|--:|--:|--:|---|---|---|"]
    for r in tsv(HERE / "inputs.tsv"):
        rows.append(f"| `{r['name']}` | {r['value']} | {r['low']} | {r['high']} | {r['unit']} | "
                    f"{r['tag']} | {r['source']} |")
    return "\n".join(rows) + "\n"


def ratio_tables(inp):
    out = ["## Ratio tables\n\nCell: mid [low-high] s(haring) d(evice). The NIC tier. Batches 3, 10 "
           "and 20 against the sequential baseline only; \"all\" against both and the faster.\n"]
    for ds in ("tpch", "tpcds"):
        sz = sizes(inp, ds)
        for dep in ("8x16", "16x8", "1x128"):
            out.append(f"**{ds}, {dep}**\n\n| workload | batch | baseline | "
                       + " | ".join(f"{l} ({fits_tier(inp, b)})" for l, b in sz) + " |")
            out.append("|---|---|---|" + "---|" * len(sz))
            for w, batch, base in ROWS:
                cells = [fmt(ratio(inp, ds, w, batch, base, dep, b)) for _, b in sz]
                out.append(f"| {w} | {batch} | {base} | " + " | ".join(cells) + " |")
            out.append("")
    return out


def shared_cpu_table(inp):
    out = ["## CPU with shared scans (modelled)\n\nThe same batches against a CPU baseline that "
           "shares scans as the GPU does: its S3 penalty shrinks by the batch's cut. Faster baseline "
           "for \"all\", sequential otherwise; NIC tier.\n",
           "| dataset | deployment | size | batch | ratio vs plain CPU | ratio vs sharing CPU |",
           "|---|---|---|---|---|---|"]
    for ds in ("tpch", "tpcds"):
        for dep in ("8x16", "1x128"):
            for label, size in headline_sizes(inp, ds):
                for batch, base in (("10", "sequential"), ("all", "faster")):
                    a = ratio(inp, ds, "batched", batch, base, dep, size)
                    b = ratio(inp, ds, "batched", batch, base, dep, size, cpu_shares=True)
                    out.append(f"| {ds} | {dep} | {label} | {batch} | {fmt(a, False)} | {fmt(b, False)} |")
    out.append("\nMeasured beside it: Comet's own in-memory cache, tables cached first, TPC-H local on "
               "1 x 128 ran 53.3 s sequential and 36.1 s concurrent, against 53.2 s and 36.1 s "
               "without it (`measurements/comet/summary.tsv`). On RAM-resident data it bought nothing.\n")
    return out


def tier_table(inp):
    out = ["## Tiers\n\nBatch \"all\" (faster baseline) and batch 10 (sequential), eight 16-core "
           "nodes, at each tier's price. Kind: cache tiers are compared with Comet on RAM-resident "
           "data while it fits half the deployment's RAM, sources with Comet on S3. A tier changes only "
           "the data path: the GPU's compute stays the H100's, so B200 and GB200 rows understate "
           "their GPU and `gb10_unified` is left out (its GPU reads memory 19x slower than an "
           "H200's). `nebius_h100_1gpu_no_local` holds nothing, so it reads as the NIC.\n"]
    names = ["nic", "vram", "local-nvme"] + [t for t in inp.tiers
                                             if t != LOCAL_NVME_TIER and t not in NOT_H100_CLASS]
    for ds in ("tpch", "tpcds"):
        hs = headline_sizes(inp, ds)
        out.append(f"**{ds}**\n\n| tier | GB/s | capacity GB | $/h | kind | "
                   + " | ".join(f"all, {l} | 10, {l}" for l, _ in hs) + " |")
        out.append("|---|--:|--:|--:|---|" + "---|---|" * len(hs))
        for t in names:
            if t == "nic":
                spec = (f"{inp.scalars['rate_h100_s3'].value:g}", "unbounded",
                        f"{inp.scalars['price_h100_node'].value:g}")
            elif t == "vram":
                spec = ("HBM", f"{inp.scalars['vram_gb'].value:g}",
                        f"{inp.scalars['price_h100_node'].value:g}")
            else:
                x = inp.tiers[LOCAL_NVME_TIER if t == "local-nvme" else t]
                spec = (f"{x.rate:g}", f"{x.capacity:g}", f"{x.price:g}")
            cells = []
            for _, size in hs:
                cells.append(fmt(ratio(inp, ds, "batched", "all", "faster", "8x16", size, t), False))
                cells.append(fmt(ratio(inp, ds, "batched", "10", "sequential", "8x16", size, t), False))
            kind = "cache" if t in CACHE_TIERS else "source"
            out.append(f"| {t} | " + " | ".join(spec) + f" | {kind} | " + " | ".join(cells) + " |")
        out.append("")
    return out


def stall_table(inp):
    p = Params(inp)
    d = p["decode_rate"]
    out = ["## Where the GPU stalls on the NIC, per TPC-H query\n\nAt the inputs' values, sf40, one "
           "query alone. Below the stall rate the wire is the slowest term, so the GPU waits; the "
           f"H100 VM's assumed S3 rate is {p['rate_h100_s3']:g} GB/s x efficiency "
           f"{p['s3_efficiency']:g}.\n",
           "| query | scan GB | decode s | compute s | stall rate GB/s | Gbit/s | wire-bound at the NIC tier "
           "| stall rate with decode at its high, GB/s |",
           "|---|--:|--:|--:|--:|--:|---|--:|"]
    d_high = inp.scalars["decode_rate"].high
    nic = p["rate_h100_s3"] * p["s3_efficiency"]
    for q in inp.queries["tpch"]:
        _, compute, _ = gpu_query_terms(p, q, 1.0)
        gb = q.scan_bytes / 1e9
        other = max(gb / d, float(compute))
        stall = gb / other if other > 0 else math.inf
        fast = gb / max(gb / d_high, float(compute))
        out.append(f"| {q.name} | {gb:.2f} | {gb / d:.3f} | {float(compute):.3f} | {stall:.2f} | "
                   f"{stall * 8:.1f} | {'yes' if stall > nic else 'no'} | {fast:.2f} |")
    out.append("")
    return out


def beside_table(inp):
    out = ["## Listed beside the ratio, not folded in\n\nS3 GET requests and bytes over the wire per "
           "pass at sf40. All reads are in-region (eu-west1 for the CPU VM), so egress is zero; idle "
           "time between scheduled windows is not modelled.\n",
           "| dataset | batch | CPU GETs (sequential S3 run) | CPU wire GB | GPU wire GB, shared |",
           "|---|---|--:|--:|--:|"]
    gets = {}
    for r in tsv(HERE / "measurements/comet/bytes.tsv"):
        if r["config"].endswith("-s3-sequential-ball") and r["run"] != "w0":
            gets[(r["dataset"], r["query"])] = int(r["s3_gets"])
    for ds in ("tpch", "tpcds"):
        qs = inp.queries[ds]
        for batch in ("3", "10", "20", "all"):
            b = qs if batch == "all" else qs[:int(batch)]
            g = sum(gets.get((ds, q.name), 0) for q in b)
            out.append(f"| {ds} | {batch} | {g:,} | {sum(q.scan_bytes for q in b) / 1e9:.1f} | "
                       f"{shared_bytes(inp, ds, b) / 1e9:.1f} |")
    out.append("")
    return out


if __name__ == "__main__":
    main()
