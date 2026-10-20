//! Which nodes call at done only if nothing went out: an aggregate's init, every one of them,
//! and nothing else. A node whose executor does not expect the pattern would make the call at
//! the wrong moment or refuse it, so the plan goldens are read for the claim in both
//! directions.

use crate::test_support::{CORPUS_DATASETS, MODES, golden_dir_for};

#[test]
fn only_an_aggregate_init_calls_at_done_if_nothing_went_out_and_every_init_does() {
    let mut inits = 0;
    for &(dataset, sf) in CORPUS_DATASETS {
        for mode in &MODES {
            let path = golden_dir_for(dataset, sf).join(format!("{}.plans.txt", mode.name));
            let text = std::fs::read_to_string(&path).expect("a mode golden");
            // Every recipe line, not the first: a golden holds one section per query.
            for line in text.lines().map(str::trim_start) {
                let calls_if_nothing_out = line.contains("at done, if nothing went out:");
                let is_init = line.starts_with("GpuAggregate: calling_lanes=");
                assert_eq!(calls_if_nothing_out, is_init, "{}: {line}", path.display());
                inits += usize::from(is_init);
            }
        }
    }
    assert!(
        inits > 100,
        "only {inits} init lines read — the goldens moved"
    );
}
