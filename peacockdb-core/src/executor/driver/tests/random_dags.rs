//! `validate_dag` over random DAGs: several roots, shared sources, joins, and buffers on random
//! edges. Once every unbuffered hold has its buffer, no plan may be refused — the wait-for
//! graph never finds a cycle the buffers left.

use std::panic::{AssertUnwindSafe, catch_unwind};

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

#[test]
fn no_valid_dag_is_refused() {
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
        let refs: Vec<&dyn GpuNode> = roots.iter().map(|root| root.as_ref()).collect();
        assert_eq!(validate_dag(&refs, None), Ok(()), "seed {seed}");
        buffered += usize::from(inserted > 0);
    }
    assert!(
        buffered > 50,
        "only {buffered} of 500 needed a buffer: the generator is too tame"
    );
}
