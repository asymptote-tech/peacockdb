# Accelerated ETL: batched SQL on rented GPUs

*Product proposal, work in progress.*

This proposal describes a self-serve service that runs ETL queries on rented NVIDIA GPUs. It
explains where the cost saving comes from, what the product looks like, and the condition the
case rests on.

## The problem

ETL pipelines cost a lot to run, and users complain about the cost. They also cause friction.
Services such as Snowflake and ClickHouse make the user allocate compute explicitly. Explicit
allocation leaves compute idle, more so than allocation by the system on demand. In Snowflake, a
compute cluster can only grow 2× at each step.

For ETL, CPUs are also less cost-effective than NVIDIA GPUs.

Several startups work in the area of ETL cost-reduction, among them Paraquery and Espresso AI.

## Why GPUs can cost less

We compared a GPU VM and a CPU VM at the same on-demand price on Nebius, about $4.50 per hour
each:

- a CPU VM with 128 vCPUs and 512 GB of RAM, which ran DuckDB 1.5.4 on 128 threads;
- a GPU VM with one H100, 16 vCPUs and 200 GB of RAM, which ran the peacockdb engine.

Both ran TPC-H and TPC-DS queries, with the data already in RAM
([our evals](https://htmlpreview.github.io/?https://github.com/asymptote-tech/peacockdb/blob/ENS-tp4-sized/testdata/benchmark-report.html)).
The H100 VM ran the queries in 10% to 20% less time. At the same hourly price, that is a 10% to
20% cost saving.

## What limits the GPU

On the GPU VM, **data loading** takes 60% to 90% of the query time. Data loading is the work done
before the GPU can compute. The compressed bytes move from RAM to the GPU over the PCIe bus. Then
the GPU decompresses them and decodes them into columns. The move takes about 20% of data loading.
Decompression and decoding take the other 80%.

In a cloud service, the data does not start in RAM. It starts in S3 object storage, and it must
first cross the network card (**NIC**) of the VM. Nebius shows S3 reads as low as 1 GB/s. PCIe
moves 55 GB/s from RAM to the GPU. So the GPU waits for data at two points: the NIC, and data
loading.

## Why batching helps

ETL queries usually do not need an answer at once. So the service can take many queries together,
as a **batch**, and run them as one **fused query**: one plan for the whole batch. A fused query
can load a piece of a table once and use it for every query that reads it. This removes repeated
transfers, decompression and decoding.

A naive
[prototype](https://htmlpreview.github.io/?https://github.com/asymptote-tech/peacockdb/blob/ENS-tp4-sized/testdata/benchmark-report.html#fused-h100-80gb-hbm3-tpcds-sf40)
ran 79 TPC-DS queries as one fused query. It cut data loading time by 30%, compared with one query
at a time. In this prototype, most large tables were still read almost once per mention in the
query text. A fused query that is optimized for the limited GPU memory (VRAM) should do much better.

## What we expect

We expect about 50% lower cost than a CPU VM at the same hourly price. This needs two things
together: batching that uses VRAM well, and NICs fast enough to keep the GPU busy. This figure is an
expectation, not a measurement.

## The product

The user submits a batch of queries against a data lake in S3 that the user owns. The user also
gives a **time window**, the period in which the batch must run. The window lets the service choose
when and where to run it.

- The user registers on the website and enters company and credit card details.
- The user registers the data lake. The service mirrors it into the data center where the GPUs
  run. The mirror avoids an **egress** fee on each query: the charge a cloud makes for data
  that leaves it.
- The user submits a batch with its time window and receives a cost estimate.
- Inside the window, the service runs the batch. The results go into the user's data lake.
- When the batch is done, the user receives the bill.

The first version runs a batch on one VM, which can have more than one GPU. UDFs and non-SQL
steps in a pipeline, such as model inference or calls to external services, are open questions.

## The architecture

- A pool of GPU VMs with 100 Gbit/s NICs, rented from a **neocloud** (a GPU cloud, such
  as Nebius), with long-term discounts.
- An S3 cluster in the same neocloud.
- A mirroring service for the users' Delta Lake tables.
- A scheduling service. Inside a batch's time window, it loads data into **pinned RAM** on a GPU
  VM: host memory that the GPU can read directly. It then starts a container that runs the fused
  query on the GPUs and the CPU cores of the VM.

## The condition

The product depends on one condition. A cloud must rent H100 instances with 100 Gbit/s NICs, or
better 400 Gbit/s, for about $4.50 per hour. If it does, the product can
fill a self-serve niche in the middle of the market.

If it does not, the product needs specialized hardware on premises, with a high fixed cost. That
narrows it to the upper market: customers with enough data for per-byte costs to dominate. The
upper market brings larger data volumes (and more complicated system architecture necessary to support them)
Upper market also has longer sales cycles.

## The first step

The service can start without GPUs, as a self-serve batch ETL system that manages compute on
demand. That MVP is easier to build. It can use the low CPU prices of some neoclouds: $0.011 per
hour for 1 vCPU and 4 GB of RAM, against $0.033 on Nebius. This assumes such compute comes with a
fast enough NIC.
