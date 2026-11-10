# Accelerated ETL: the product proposal, rewritten for a wider audience

Kind: prototype. Done by the helper in an interactive session on branch `ENS-accelerated-etl`,
off master (the human's decision, 2026-11-09). There is no chain, no PR and no reviewer. When the
work is done, the branch is squashed to one commit and lands on master. The output is one design
document.

## Why

The product proposal exists only as a working draft,
[`draft-product.md`](../artifacts/accelerated-etl/draft-product.md), a copy of the human's
`~/Documents/product.md`. It is written for the core team. It now goes to a wider audience:
prospective cofounders, design partners and technical reviewers at NVIDIA. These readers know
databases or GPUs in general, but not peacockdb, its benchmarks or its terms.

At the end, the reader must be able to picture the working product, and how it delivers value to
customers, if GPU VMs with high-bandwidth NICs can be rented in the cloud at a low enough price.

## What to write

`llm-wiki/designs/accelerated-etl.md`, at most two pages (about 900 words). It follows the
"Writing reports" section of `llm-wiki/prompts.md`, adapted to two pages: no appendices, and a
glossary only if one is needed. It builds the concepts step by step, in this order:

1. The customer's problem: ETL costs, and the friction of allocating compute yourself.
2. Why NVIDIA GPUs can cost less: the evals, which show 10-20% today with data in RAM.
3. What limits the GPU: data loading takes 60-90% of the GPU time.
4. Why batching helps: the fused-query prototype cut data loading time by 30%.
5. The expectation: about 50% lower cost than CPU, from batching that uses VRAM well, plus NICs
   fast enough to keep the GPU busy. State it as an expectation, not a measurement.
6. The product: self-serve, a batch with a time window, the mirrored data lake, the cost estimate
   and the bill.
7. The architecture: GPU pool with fast NICs from a neocloud, S3 in the same neocloud, the
   mirroring service, the scheduler.
8. The condition everything rests on: GPU VMs with 100-400 Gbit/s NICs at about $4.50/h for an
   H100.
9. The first step, a CPU-only self-serve MVP, and the open questions: UDFs and non-SQL steps.

Prior art: name Paraquery and Espresso AI as companies that work in this area. Say nothing more
about them than that.

## Constraints

- Composition and style only. No new research, and no number that the draft does not give, except
  the 50% expectation above.
- No findings from the comet research.
- Leave implicit: an example customer workload, the freshness of the mirror, and egress for
  results. Leave out: shared VMs and isolation between customers.
- Keep the draft's facts. Fix its slips: "100GB NICs" means 100 Gbit/s NICs.
- Links to the evals stay as the draft gives them.

## Calls made by the helper (the human can change them)

- One causal chain for the bottleneck. Data reaches the GPU in two stages: over the network from
  S3 (about 1 GB/s on Nebius, against 55 GB/s from RAM to GPU over PCIe), then decompression and
  decoding. Batching cuts the second stage; fast NICs cut the first. The 50% needs both. Both
  draft figures stay.
- The 50% is against a 128-core CPU VM at the same hourly price, as in the evals, not against a
  Snowflake or ClickHouse bill. It is an expectation, without a derivation.
- If the condition fails, the draft's alternative stays in one or two sentences: specialized
  on-prem hardware, which narrows the product to the upper market.
- The CPU baseline of the evals is named: DuckDB 1.5.4 on 128 threads, from the cited eval report.

## Scope

New files: `llm-wiki/designs/accelerated-etl.md`, `llm-wiki/artifacts/accelerated-etl/draft-product.md`,
this spec and its plan. No code changes. API changes: none.

## Done when

- The document is at most about 900 words and follows the order above.
- A mechanical check passes: every number traces to the draft (or is the 50% expectation), no
  term appears before its definition, sentences have 25 words or fewer, and the banned words of
  "Writing reports" are absent.
- The branch is one commit, and the human has read the document.
