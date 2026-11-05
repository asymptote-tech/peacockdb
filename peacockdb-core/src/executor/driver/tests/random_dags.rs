//! `validate_dag` tied to the driver over random DAGs: several roots, shared sources, joins,
//! and buffers on random edges, run on the mock under a step bound. A DAG it accepts runs to
//! completion, reconciles, and queues at most one batch per lane anywhere; a DAG the driver
//! cannot finish is one it refused; and once every unbuffered hold has its buffer, no DAG is
//! refused and every one finishes.
//!
//! A reader's cursor never blocks its producer, so an unbuffered hold does not deadlock: the
//! queue in front of it grows. That growth is what the buffer rule forbids, and what tells an
//! accepted DAG from a refused one here.

use std::panic::{AssertUnwindSafe, catch_unwind};

use super::mock::{Mock, Script, spec};
use super::{Driver, refs};
use crate::executor::{FusedRun, RunError};
use crate::plan::{GpuLoadParquet, GpuNode, SharedSource, new_hold_buffer, validate_dag};
use crate::tests::plans::{coalesce_all, filter, project, projected_join, source, unload};
use crate::tests::share::buffer_unbuffered_holds;

/// xorshift64*, seeded and fixed: the determinism rules apply to tests too.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

struct Dag {
    rng: Rng,
    sources: Vec<SharedSource>,
    joins_left: usize,
}

impl Dag {
    /// A subtree at most `depth` deep. A join's build goes through a coalesce, so it hands its
    /// join one batch whatever its source emits.
    fn tree(&mut self, depth: usize) -> Box<dyn GpuNode> {
        let pick = if depth == 0 { 0 } else { self.rng.below(5) };
        match pick {
            1 => filter(self.tree(depth - 1)),
            2 => project(self.tree(depth - 1)),
            3 if self.joins_left > 0 => {
                self.joins_left -= 1;
                let build = coalesce_all(self.tree(depth - 1));
                projected_join(build, self.tree(depth - 1))
            }
            4 => Box::new(new_hold_buffer(self.tree(depth - 1))),
            _ => {
                let at = self.rng.below(self.sources.len());
                Box::new(self.sources[at].clone())
            }
        }
    }
}

/// 2–4 roots over 1–3 shared sources with 0–3 joins among them.
fn random_dag(seed: u64) -> Vec<Box<dyn GpuNode>> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let sources = (0..1 + rng.below(3))
        .map(|at| {
            let load = source(&format!("s{at}"), 1)
                .into_any_box()
                .downcast::<GpuLoadParquet>()
                .expect("plans::source builds a loader");
            SharedSource::new(*load)
        })
        .collect();
    let joins_left = rng.below(4);
    let roots = 2 + rng.below(3);
    let mut dag = Dag {
        rng,
        sources,
        joins_left,
    };
    (0..roots).map(|_| unload(dag.tree(4))).collect()
}

/// Three batches for every source a DAG can draw.
fn script() -> Script {
    (0..3).fold(Script::default(), |script, at| {
        script.source(
            &format!("s{at}"),
            vec![vec![spec(10, 80), spec(20, 160), spec(30, 240)]],
        )
    })
}

/// The mock run under the bound. A driver that stops with work left reports it as a protocol
/// error, the step bound's included; any other failure is the driver's own and a finding.
fn simulate(roots: &[&dyn GpuNode], script: &Script) -> Result<FusedRun, String> {
    match Driver::<Mock>::for_roots(roots, script, None).and_then(|driver| driver.run_fused(10_000))
    {
        Ok(fused) => Ok(fused),
        Err(RunError::Protocol(said)) => Err(said),
        Err(other) => panic!("the driver failed for a reason no schedule explains: {other:?}"),
    }
}

/// The node whose queues held more than one batch per lane at once, if any.
fn grown(fused: &FusedRun) -> Option<usize> {
    let report = &fused.report;
    (0..report.peak_queued.len()).find(|node| report.peak_queued[*node] > report.lanes_of[*node])
}

/// Finished, reconciled, and never more than a batch per lane in any queue.
fn assert_ran_bounded(fused: &FusedRun, seed: u64) {
    assert_eq!(fused.report.holds, fused.report.releases, "seed {seed}");
    assert_eq!(fused.report.in_flight_bytes, 0, "seed {seed}");
    assert_eq!(
        grown(fused),
        None,
        "seed {seed}: a queue grew under a valid plan"
    );
}

#[test]
fn every_dag_validate_dag_accepts_finishes() {
    let script = script();
    let (mut accepted, mut grew) = (0, 0);
    for seed in 0..500 {
        let roots = random_dag(seed);
        let refs = refs(&roots);
        let verdict = validate_dag(&refs, None);
        match simulate(&refs, &script) {
            Ok(fused) if verdict.is_ok() => {
                assert_ran_bounded(&fused, seed);
                accepted += 1;
            }
            Ok(fused) => grew += usize::from(grown(&fused).is_some()),
            Err(said) => {
                assert!(
                    verdict.is_err(),
                    "seed {seed}: accepted, and the driver stopped: {said}\n{roots:?}"
                );
            }
        }
    }
    assert!(
        accepted > 100,
        "only {accepted} of 500 accepted: the generator is too strict"
    );
    assert!(
        grew > 50,
        "only {grew} refused DAGs grew a queue: the refusals are not tested"
    );
}

#[test]
fn no_valid_dag_is_refused() {
    let script = script();
    let mut buffered = 0;
    for seed in 0..500 {
        let roots = random_dag(seed);
        let shape = format!("{roots:?}");
        let outcome = catch_unwind(AssertUnwindSafe(|| buffer_unbuffered_holds(roots, None)));
        let (roots, inserted) = outcome.unwrap_or_else(|why| {
            let why = why
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "a panic with no message".to_string());
            panic!("seed {seed}: {why}\n{shape}")
        });
        let refs = refs(&roots);
        assert_eq!(validate_dag(&refs, None), Ok(()), "seed {seed}");
        let fused = simulate(&refs, &script)
            .unwrap_or_else(|said| panic!("seed {seed}: valid, and the driver stopped: {said}"));
        assert_ran_bounded(&fused, seed);
        buffered += usize::from(inserted > 0);
    }
    assert!(
        buffered > 50,
        "only {buffered} of 500 needed a buffer: the generator is too tame"
    );
}
