//! Does the instrument change what it measures, and does it measure what it claims?
//!
//! Every record under `testdata/benchmark-results/` assumes a node's reported time is
//! the time it would have taken unobserved. Events record without draining the stream,
//! so they can satisfy that — whether they do is measured here rather than assumed.
//!
//! One query, because this is about the instrument and not the corpus: is `Off` actually
//! off, do the events land where they claim, and did every call open a region. What events
//! mode costs is reported from an sf40 run and written into `build-test.md` — a bound loose
//! enough never to flake on a shared host proves nothing.
#![cfg(not(feature = "rust-only"))]

use peacockdb_core::test_support::{
    TimedRun, device_plan, install_rmm_pool, set_node_timing, timed_run,
};

/// q19 for its shape, not its answer (`test_gpu_corpus` owns that): scan → filter →
/// join → aggregate covers enough operator families that a region left unopened shows up
/// as a too-small Σ device_us. The richest device-enabled query at sf1 — q3 has more node
/// kinds but does not run on the device, and this needs regions, not a plan.
const DATASET: &str = "tpch";
const SF: &str = "1";
const QUERY: &str = "q19";
/// One batch per partition, so the wall-clock bound of check 3 is as tight as it gets.
const MODE: &str = "tp1_single";

/// The pool this binary reserves, as each gtest binary declares its own `kPoolBytes`:
/// bytes, never a share of the device, so two processes fit on one card (#178). Swept on
/// shad-gpu at the sf1 q19 below — 0.5 GiB dies in the lineitem scan with "Maximum pool
/// size exceeded" and 0.75 GiB passes — and rounded up, because the pool cannot grow and
/// a budget at the floor is one fragmentation away from that failure.
const POOL_BYTES: u64 = 2 << 30;

/// Each round runs both modes, so the totals come from interleaved samples. In blocks,
/// host drift would land entirely on whichever mode ran last — which is the difference the
/// two walls are read for.
const ROUNDS: usize = 7;

/// Second-smallest wall clock, matching the benchmark harness: the minimum is the sample
/// most likely to have caught a favourable scheduling accident, the rest are dragged up
/// by whatever else the host was doing.
fn second_smallest(mut runs: Vec<TimedRun>) -> TimedRun {
    runs.sort_by_key(|run| run.total_us);
    runs.swap_remove(1)
}

#[tokio::test]
async fn events_are_free_and_land_where_they_claim() {
    const _: () = assert!(ROUNDS >= 2, "a second minimum needs >= 2 rounds");

    // Before anything allocates: rmm uses whatever resource is current at the time of the
    // call, and without it the two walls would differ by allocator behaviour as much as
    // by instrument.
    let allocator = install_rmm_pool(POOL_BYTES);

    let plan = device_plan(DATASET, SF, QUERY, MODE).await;

    // The switch is process-global. A guard rather than a trailing reset: everything
    // below unwraps, and an unwind would leave every later test in this binary measured.
    struct Loan;
    impl Drop for Loan {
        fn drop(&mut self) {
            set_node_timing("off");
        }
    }
    let _loan = Loan;

    // Discarded: the first execution pays for the page cache, CUDA module load and JIT,
    // and allocator growth — none of which belongs to whichever mode runs first.
    set_node_timing("off");
    timed_run(&plan);

    let modes = ["off", "events"];
    let mut runs: Vec<Vec<TimedRun>> = modes.iter().map(|_| Vec::with_capacity(ROUNDS)).collect();
    for _ in 0..ROUNDS {
        for (slot, mode) in runs.iter_mut().zip(modes) {
            set_node_timing(mode);
            slot.push(timed_run(&plan));
        }
    }
    set_node_timing("off");

    let mut picked = runs.into_iter().map(second_smallest);
    let off = picked.next().unwrap();
    let events = picked.next().unwrap();
    let (off_us, events_us) = (off.total_us, events.total_us);

    let ev_device = events.device_us;
    let over = |us: u64| 100.0 * (us as f64 / off_us as f64 - 1.0);
    // Both walls, every run: what events mode costs is read off this by hand and written
    // into the wiki, since no bound that survives a shared host would say anything.
    eprintln!(
        "node-timing {DATASET}/{QUERY} [{MODE}] alloc=[{allocator}] regions={}\n  \
         off    wall={off_us}us\n  \
         events wall={events_us}us ({:+.1}%)  host={} device={ev_device}",
        events.regions,
        over(events_us),
        events.host_us,
    );

    // 1. Off is off. A region is opened only in a measured mode, so a leak shows up as
    // regions existing at all — and it would put an event pair into every correctness run
    // in the process, where nothing would fail and the suite would just get slower.
    assert_eq!(
        off.regions, 0,
        "NodeTiming::Off recorded {} regions",
        off.regions
    );

    // 2. Zero means no region created its event pair.
    assert!(ev_device > 0, "events mode recorded no device time at all");

    // 3. Placement. Regions record on cuDF's single default stream in host program
    // order, so their intervals are disjoint and must fit inside the wall clock.
    // Exceeding it means a pair spans work that is not its region's — what a mark left
    // at region entry produces, since the pair then swallows the next call's launches.
    assert!(
        ev_device <= events_us,
        "Σ device_us = {ev_device}us exceeds the {events_us}us wall clock: the event \
         pairs are not disjoint, so at least one spans work outside its own region",
    );

    // 4. Nothing went unmeasured. The sums above are over whatever regions came back, so
    // a run that recorded half its calls would satisfy every check so far while describing
    // half a query. At tp1 a call has one output partition, so the two counts are equal
    // rather than merely ordered.
    assert_eq!(
        events.regions, events.journalled,
        "{} regions for {} journalled ABI calls: a call ran without opening one",
        events.regions, events.journalled,
    );
    assert_eq!(
        off.calls, events.calls,
        "off and events ran different plans"
    );
}
