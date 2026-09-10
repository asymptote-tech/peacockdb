# 3 — the pool reserves what a binary needs

Kind: production

Inserted before [`test-layout.md`](test-layout.md), and independent of it: it touches `cpp/` and
one wiki page, and nothing in the layout refactor reads either.

Closes [#178](../tickets.md#t178) tentatively. Every gtest binary that installs a pool reserves 85%
of free VRAM and caps at 95, so two processes on the card cannot both have what they asked for and
the second dies in `pool_memory_resource` with `std::bad_alloc`. Measured once: two jobs
overlapping by under two minutes, three sf40 tests down, at a 14.38 GiB peak on a 139.7 GiB device.
Not a full device — two pools.

**CI no longer collides with itself**: `gpu-tests` carries `concurrency: {group: shad-gpu,
cancel-in-progress: false}`, which is the fix the ticket proposed and it has since landed, so runs
queue rather than overlap. What remains is the card being shared with work outside this repo, which
no group of ours can serialise — and a binary that asks for 85% of a device it does not own is the
part we can fix.

## The pool is not only in sf40 tests

The premise this task started from was that only the sf40 suites take a pool. They do not. Six
binaries call `peacock::install_rmm_pool()` from `main()`:

| Binary | sf40 | Runs |
|---|:-:|---|
| `test_tpch.cpp`, `test_tpchv.cpp` | yes | every gpu-tests job |
| `test_cudf_nodes.cpp`, `test_tpch_streamed.cpp` | yes | manual |
| `test_cudf.cpp` — the GPU smoke and the murmur3 kernel | **no** | every gpu-tests job |
| `test_plan_executor.cpp` — hand-built plans over `tpch.minimal`, 19 MB | **no** | every gpu-tests job |

So an ordinary CI run puts four processes on the host, each asking for 85% of what it finds free,
one of them for a dataset of nineteen megabytes. That is the shape of the collision, and sizing
only the sf40 pair would leave it in place.

`multi_gpu.cpp` is a seventh caller with its own per-device installation. It is manual, needs two
GPUs, and never runs in CI, so it is not part of this and **keeps the percentage constants**, which
stay in the header for it alone with a comment saying so.

## The change

`install_rmm_pool()` takes an explicit byte budget. No percentage, and **no clamp**: a binary asks
for what it needs and a host that cannot supply it fails to build the pool, which is already
`RmmPoolStatus::Unavailable` — the binary carries on with rmm's default resource, correct but
unpooled, and a caller taking timings asserts as it does today. A clamp would silently hand back a
smaller pool than was asked for, and the number a benchmark reports would stop meaning what it says.

Each of the six declares its own budget as a named constant beside its `main()`, with the
measurement that justifies it in the comment. **Take the numbers, do not choose them**: the pool
already carries a `statistics_resource_adaptor`, so run each binary and read its peak. Round up to
something a reader can defend, not to a percentage.

The FFI's `peacock_install_rmm_pool` gains the same argument, because it exists so a Rust caller
gets the allocator the gtest binaries have and that is no longer a fixed thing. What it must not do
is start reading `gpu_memory_limit`, which is stored and ignored — that is [#148](../tickets.md#t148)
and a decision about the product.

## Validation

- Every GPU tier stays byte-identical. This changes how much memory is reserved, never what is
  computed.
- **Two of each in parallel.** On shad-gpu, run `peacock_tpch_tests` twice at once and confirm both
  pass; that is the failure this task exists to make unreachable, and it has never been run
  deliberately.
- Each binary's declared budget is at least its measured peak, and the four that share a CI job sum
  to well under the device.

## The ticket stays open, tentatively closed

`#178` is not deleted and not archived. It is marked **tentatively closed** with the reasoning:
the pool no longer sizes itself against the device, so two runs fit; but the host is shared with
work outside this repo, so a third party can still exhaust it and this cannot be proven closed
from here.

The instruction that goes with it is for whoever meets it next, and it is deliberately narrow: **if
a GPU tier fails with `std::bad_alloc` in `pool_memory_resource`, add a dated line to #178 saying
which run and which binary, re-run the job once, and do not debug it.** The evidence accumulates on
the ticket until there is enough of it to say whether the sizing was wrong or the neighbour was
greedy. A coordinator that stops to diagnose this spends a dispatch on a machine it does not own.

`llm-wiki/prompts.md` carries the same line in the Coordinator section, because a coordinator does
not read `tickets.md` and would otherwise never see it.

## Done when

Six binaries declare explicit byte budgets with their measured peaks recorded; `install_rmm_pool`
takes bytes and does not clamp; the percentage constants remain only for `multi_gpu.cpp` and say
so; two `peacock_tpch_tests` run concurrently on shad-gpu and both pass; every GPU tier is
byte-identical; #178 is marked tentatively closed carrying the retry-don't-debug instruction; and
`prompts.md`'s Coordinator section carries it too.
