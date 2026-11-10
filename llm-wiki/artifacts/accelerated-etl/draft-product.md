
*Proposal is work in progress*

# Problem

Running ETL pipelines has friction for users (allocating compute in services like Snowflake/ClickHouse), and has high costs. CPUs are not as cost-effective as NVidia GPUs for batch workflows. Additionally, allocating compute explicitly is less efficient compared to when the system allocates it on demand. Some compute would be idle (especially in Snowflake where compute clusters are only allowed to grow 2x on each increase).

**Solution**: self-serve service that allowes a user to submit a batch of multiple ETL queries over a data lake in S3. The system will allocate compute (with GPU acceleration) and execute these queries in the given time window. Results go into user's data lake. The system will use neoclouds (e.g Nebius) to manage GPU compute. Pipeline will be bottlenecked on ability to read from S3 fast enough for the GPU not to be idle. Nebius shows S3 reads as low as 1GB/second, compared to 55GB/s RAM-GPU over PCIe.

The product idea hinges on cloud's ability to provide GPU instances with 100Gb/s (better 400Gb/S) NICs. If such instances are available, at costs around 4.5$/hour for H100 instance, the product can fill a mid-marked self-serve niche.
Additionally, at a start, the product can proceed as self-serve batch ETL system that manages on-demand compute, but without GPU acceleration. Techinically, that's easier to build in an MVP. It can utilize very low CPU instance costs that some neoclouds provide: 0.011$/hour for 1vCPU+4GB RAM (vs 0.033$ in Nebius) - assuming such compute can be rented with sufficiently high-capacity NIC.

# Background

NVidia GPU acceleration can achieve lower latency and lower cost for analytical queries. High cost of running ETL is a common complaint from users. A number of startups already work in this area.

This product proposal focuses on providing users with a convenient lower-cost product.
The main open question is whether the product can be be built using rented GPUs offered by clouds/neoclouds, or it requires specialized hardware that has to be installed on-prem.
The high fixed cost of installing specialized on-prem hardware will significantly narrow the application of the product to only up-marked niches. Up-marked customers have sufficient amounts of data for the linear costs to start dominating. Targeting up-market complicates both software arhictecture (higher data volumes) and business operations (longer sales cycles).

In (our evals)[https://htmlpreview.github.io/?https://github.com/asymptote-tech/peacockdb/blob/ENS-tp4-sized/testdata/benchmark-report.html], we compared time required to run queries from popular tpc-h and tpc-ds benchmarks on 128-core x86_64 CPU machine, and Nvidia H100 machine. These two configurations have very similar on-demand hourly price in Nebius, about 4.50$/hour each for 128vCPU 512GB RAM AMD host, and 16 vCPU, 200GB RAM, 1 H100 host.

Our evals show that running on H100 achieves generally 10%-20% lower query time compared to the 128-core CPU machine. Given the same hourly cost of this hardware, that would constitute 10-20% cost saving. In these evals both engines read data cached in RAM. There is once nuance: data loading takes 60%-90% of time for the GPU eval. It further breaks down into crossing compressed bytes over PCIe bus (~20% of data loading), decompressing and decoding Arrow buffers (remaining 80% of data loading).

This picture suggests a batching optimization: plan, optimize and run many large ETL queries together, not individually. This will allow reusing loaded, decompressed and decoded segments across many queries.
A (naive prototype)[https://htmlpreview.github.io/?https://github.com/asymptote-tech/peacockdb/blob/ENS-tp4-sized/testdata/benchmark-report.html#fused-h100-80gb-hbm3-tpcds-sf40] reduced data loading time by 30% when 79 TPC-DS queries ran as a fused query, compared to running them one at a time. In this prototype, most large tables we still re-read almost as many times as they occured in the query text. Optimizing such a fused query to use limited GPU VRAM efficiently is expected to result in much bigger improvement.

# Product proposal

Batched ETL with GPU acceleration. The user submits a batch of queries against a data lake in S3 he owns, along with the time window to execute.
The engine includes a scheduler that loads data into pinned RAM buffers in a GPU VMs inside that time window, and spins up a container that executes compute CPU/GPU computation. VMs can be shared between users. The batch of queries will utilize GPUs, as wells present CPU cores on the VM. The batch is optimized and executed as a fused query, with the focus to reduce the number of bytes that need to cross the PCIe link to the GPU (and number of times the same segments need to be decompressed and decoded).

Product is self-serve. The user registers on the website, enters his company and credit card information, registers the data lake which will be mirrored in the DC where GPUs run to avoid per-query egress fee.
When the user submits a batch of queries, he receives cost estimate. Upon completion, the user receives the bill.

Initial version will support 1 host, with potentially multiple GPUs.

To be worked out: UDFs and non-SQL steps in the ETL pipeline (e.g. running inference or calling external services).

System architecture: a pool of GPU-compute with 100GB NICs rented from a neocloud (with long-term discounts). S3 cluster in the same neocloud. Delta lake mirroring service. Scheduling service.
