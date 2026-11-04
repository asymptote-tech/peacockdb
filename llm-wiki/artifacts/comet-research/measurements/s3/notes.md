# Experiment 1: VM-to-S3 throughput (L40S)

## Summary
- The L40S read eu-north1 S3 at 0.917 GB/s (7.33 Gbit/s) at best: 16 threads, one 9.03 GB object. 64 threads gave 0.862, 4 gave 0.568, and 1 gave 0.129.
- Its boot disk (NETWORK_SSD, 100 GiB) read at 0.061-0.062 GB/s at every thread count, 15 times slower than S3. On this VM, S3 is the fast source and "local disk" is the slow one.
- Nebius states no NIC rate for any VM we use. 7.33 Gbit/s sits inside the assumed L40S range of 5-20 Gbit/s. The H100's row stays an assumption (10-100 Gbit/s).
- Whether 0.9 GB/s is the NIC's limit or the Python client's is not settled: CPU use was 216-283% of 800%, and adding threads past 16 gave nothing (estimate: the cap is outside the client).
- The VM was started for this run (`vm.sh use` found it STOPPED), so no other tenant's load is in these rows. The load columns show only the run's own load.

## Setup
- Host: `l40s`, `computeinstance-e00tnrse7ayntnzcyt`, `gpu-l40s-a` `1gpu-8vcpu-32gb`, eu-north1. The boot disk is `NETWORK_SSD`, 100 GiB, image family `ubuntu24.04-cuda13.0` (`nebius compute instance get`, 2026-11-04). No other disk. The VM's clock is UTC, the same as dev's.
- Object: `s3://tpch-sf40/lineitem.parquet`, 9,030,797,348 B, lineitem's only (so largest) object in the bucket. Default AWS profile, endpoint `https://storage.eu-north1.nebius.cloud:443`, pushed with `vm.sh push-aws l40s`.
- Client: `scripts/s3_throughput.py` @af00cb78 (sha256 matched on the VM), Python 3.12.3, boto3 1.34.46. GB is 1e9 bytes.
- One detached driver ran every row in sequence (03:17:22-03:31:32 UTC), and the developer polled it. Before and after each row it recorded the load average, `nvidia-smi` GPU utilisation and eth0's `rx_bytes`.
- Rows: [`l40s.tsv`](l40s.tsv). Best per source: [`summary.tsv`](summary.tsv).

## Results
| source | threads | GB/s | Gbit/s | client CPU % | local time (UTC) |
|---|--:|--:|--:|--:|---|
| S3 eu-north1 | 1 | 0.129 | 1.03 | 13 | 2026-11-04 03:17:22 |
| S3 eu-north1 | 4 | 0.568 | 4.54 | 65 | 03:18:33 |
| S3 eu-north1 | 16 | **0.917** | 7.33 | 216 | 03:18:49 |
| S3 eu-north1 | 64 | 0.862 | 6.90 | 283 | 03:18:59 |
| S3 to boot disk (boto3 `download_file`, 10 threads) | 10 | 0.152 | 1.22 | - | 03:19:10 |
| boot disk, O_DIRECT | 1 | 0.061 | 0.49 | 1 | 03:20:10 |
| boot disk, O_DIRECT | 4 | 0.061 | 0.49 | 1 | 03:24:09 |
| boot disk, O_DIRECT | 16 | 0.061 | 0.49 | 1 | 03:26:37 |
| boot disk, O_DIRECT | 64 | **0.062** | 0.49 | 1 | 03:29:05 |

- **The bytes crossed eth0.** In each S3 row, eth0's `rx_bytes` rose by 9.06-9.08 GB for 9.03 GB of payload, about 0.3-0.5% overhead. During the disk rows it rose by under 1 MB.
- **The disk rate does not move with threads**, so the limit is the disk, not the reader (inference). Each read took 146-148 s. The copy's 0.152 GB/s is a write into the page cache. The flush that followed fell inside the next row's `sync` (from 03:20:10, about 91 s: the row's 239 s gap less its 148 s read), outside its timed span.
- **Load.** Before the S3 rows the 1-minute load was 0.8-2.0, and the GPU was at 0% throughout. During the disk rows the load rose to 68 at 64 threads, from threads waiting on I/O.
- **ssh stalled while the disk was saturated.** From about 03:24 to 03:28, ssh to the VM timed out in the banner exchange, while the API showed RUNNING. The run finished normally (rc 0). The likely cause is sshd waiting on the same saturated boot disk (inference). Avoid needing ssh while a run saturates the boot disk.

## Stated NIC rates
From `data-path.md` `## Rates for the cost model` @96b2657f. **Nebius states no NIC rate for any of these types** [S8].

| instance type | vCPU | stated NIC | assumed Gbit/s (GB/s) | measured here |
|---|--:|---|---|---|
| gpu-h100-sxm 1gpu-16vcpu-200gb (the cost model's GPU node) | 16 | none; the platform lists a ConnectX-6 100 Gbps host adapter [S8] | 10-100 (1.25-12.5) [S8][S59] | not used (the human's decision) |
| gpu-l40s-a 1gpu-8vcpu-32gb | 8 | none, no adapter listed [S8] | 5-20 (0.63-2.5), from AWS g6e.2xlarge [S14] | 7.33 Gbit/s S3 read, 16 threads |
| cpu-d3 128vcpu-512gb | 128 | none [S8] | 32-100 (4-12.5) | Task 4 |
| cpu-d3 or cpu-e2 16vcpu-64gb | 16 | none | 6.25-32 (0.78-4) | - |
| cpu-d3 or cpu-e2 8vcpu-32gb | 8 | none | 3.125-16 (0.39-2) | - |

- The guest sees no link speed (`/sys/class/net/eth0/speed` is -1, `ethtool` "Unknown!").
- The L40S measurement stands in for the H100's S3 rate only as a lower bound (estimate). The H100 has twice the vCPUs and a listed 100 Gbps adapter. The cost model keeps the H100 at its assumed 10-100 Gbit/s, tagged `modelled`. It may cite 7.33 Gbit/s as a measured single-GPU Nebius VM in the same region.

## For Task 4
- Check the CPU VM's boot disk type and size first. If its "local disk" is also a Nebius network disk, cold reads of sf40 from it may be far slower than S3 at this rate, and a "local" Comet run would be disk-bound unless the page cache (512 GB) holds the data. Say which in each local row.

## Follow-up experiments
- Run the 64-thread S3 read with several processes (or a native client), to tell a NIC cap from a Python/boto3 cap at about 0.9 GB/s.
- Read Nebius's per-disk bandwidth limits for NETWORK_SSD by size, and check them against the measured 0.061 GB/s.
- Repeat the 16-thread S3 row at another time of day; one run per configuration here.

## Sources
- [S8] Nebius VM types: https://docs.nebius.com/compute/virtual-machines/types.md (accessed 2026-11-04), via `data-path.md` @96b2657f
- [S14] AWS accelerated specs: https://docs.aws.amazon.com/ec2/latest/instancetypes/ac.html (accessed 2026-11-04), via `data-path.md` @96b2657f
- [S59] Maknee, network and storage benchmarks on the cloud (third party, unmeasured "10 Gbit/s"): https://maknee.github.io/blog/2025/Network-And-Storage-Training-Skypilot (accessed 2026-11-04), via `data-path.md` @96b2657f
- Measurements: [`l40s.tsv`](l40s.tsv), 2026-11-04 03:17-03:31 UTC on the L40S; the client is `scripts/s3_throughput.py` @af00cb78.
