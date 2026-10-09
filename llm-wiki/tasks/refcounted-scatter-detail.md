# refcounted-scatter — run detail

## Dispatched (2026-10-09)

Branch `ENS-refcounted-scatter`, forked off `ENS-repartition-keys` at `ec1e6d79`. Tasks 1–3 of the
chain are all `done` on master `f0a6ecbf` after this run's rebase; this is the first task the chain
has built since.

**Its place in the chain, which decides what may not change.** `TableResult` takes its final shape
here, once, and `exit-copies` (task 6) and `join-session-cpp` (task 7) are written against it
without changing it again. So the shape is the deliverable as much as the scatter is.

**What the host override takes out of the spec.** The spec's verification bar asks for benchmark
timings before and after, and its device workflow asks for `build-test-shadgpu.sh` two cycles.
shad-gpu is down, so: `--build` only and the staged binaries run directly on nebius-gpu
(`dmitry@89.169.109.150`, L40S, cuDF 25.02), and **every benchmark measurement is deferred** —
`--run-benchmarks`, Nsight, any H200 timing. The spec's own §4 and its Tests section ask for two
things that are *not* benchmark runs and are therefore still in: the per-partition timers dropping
toward zero, and the hand-recorded peak-memory drop from the RMM statistics adaptor. Those are
gtest-scale measurements on the card and they stay.

**The two measurements the spec wants by hand**, because no assertion can hold them: the peak
during the scatter call (the partitioned table stays resident afterwards either way, so a
before-minus-after bound cannot work), and the per-partition timer collapse. Both go in this file.
