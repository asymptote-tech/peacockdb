# peacockdb

A GPU-native SQL engine that runs structured query execution and vector search
in the **same GPU memory** — so the candidate set produced by a `WHERE` clause
never has to cross PCIe to a separate vector store.

Rust + DataFusion front end → GPU-annotated physical plan → FlatBuffers → a
C++/cuDF executor. Status: in development; TPC-H runs end-to-end on GPU.

## One query: structured filter + vector search

```sql
SELECT   p.id, p.title, p.price
FROM     products p
WHERE    p.category = 'outdoor'
   AND   p.price BETWEEN 20 AND 200
ORDER BY p.embedding <-> :query        -- ANN shortlist, then exact rescore
LIMIT    20;
```

What executes, on one GPU:

1. **Structured filter** over ~1B rows (`category`, `price`) → candidate set.
2. **ANN shortlist** over the filtered set → ~100K approximate matches.
3. **Exact rescore** of those candidates against the query vector using the full
   fp16 embeddings (~150 MB) — the quantized index gets the ordering roughly
   right; this fixes it.
4. **Top-20** returned.

Steps 2–4 read the candidate vectors straight out of HBM. The same workload
split across a SQL database + a separate vector service ships those vectors over
PCIe instead.

## Numbers

| | value | notes |
|---|---|---|
| Typical query latency | **~10 ms** | filter + ANN + exact rescore, single GPU; estimate, not yet benchmarked end-to-end |
| Exact-rescore data movement | **~150 MB** → ~0.05 ms over HBM (3 TB/s) vs **~10 ms** over PCIe (16 GB/s) | ~100K candidates × 1.5 KB (fp16) embeddings |
| Throughput | **~100–1,000 queries/sec / GPU** | bandwidth-bound, single-GPU estimate |

These are design targets / back-of-envelope figures for an H100/H200-class GPU,
not measured results. The point of co-location is the data movement: the
rescored candidate set stays resident instead of being copied to and from a
separate service.
