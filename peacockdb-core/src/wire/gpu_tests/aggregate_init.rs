//! An aggregate's init on the walk: its per-batch chain per batch, and its done chain — the
//! first call handed nothing — for a lane where no output had a row. The executor's flag,
//! kept per lane here since no executor runs.

use super::*;

impl Walk<'_> {
    /// The per-batch chain per batch, each output going straight out; the done chain where
    /// none of them answered a row, its output going out only if it has rows, as the
    /// executors answer it — so a grouped init over nothing emits nothing here too.
    pub(crate) fn aggregate_init(
        &mut self,
        node: &dyn GpuNode,
        recipe: &Recipe,
        input: &Lanes,
    ) -> Lanes {
        let (per_batch, at_done) = phases(recipe);
        assert!(
            at_done
                .iter()
                .all(|call| call.when == CallPattern::AtDoneIfNothingOut),
            "{}: an init calls at done only if nothing went out",
            node.name()
        );
        let mut lanes = Vec::with_capacity(input.len());
        for lane in input {
            let mut out = Vec::with_capacity(lane.len().max(1));
            for handle in lane {
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                out.push(only(self.chain(&per_batch, &mut at), node.name()));
            }
            if out.iter().all(|handle| self.rows[handle] == 0) {
                let mut at = At::default();
                let done = only(self.chain(&at_done, &mut at), node.name());
                // Dropped where it has no rows: the handle stays in the session's registry
                // until the session closes, which is all a dropped handle costs a walk.
                if self.rows[&done] > 0 {
                    out.push(done);
                }
            }
            lanes.push(out);
        }
        lanes
    }
}

/// nation is one row group, and a walk plans `OneBatchPerLane`, which maps it across
/// `target_partitions` lanes: at two lanes the second receives no batch.
pub(crate) const KEYLESS_OVER_AN_EMPTY_LANE: &str = "SELECT count(*) AS n, \
     sum(n_nationkey) AS s, min(n_regionkey) AS lo FROM nation WHERE n_regionkey >= 0";
pub(crate) const KEYLESS_SUM: &str = "SELECT sum(l_quantity) AS s FROM lineitem";

/// The empty lane's init makes its done call, handed nothing, and answers count 0 and
/// NULLs; the merge folds that row in beside the other lane's, and DataFusion agrees.
#[tokio::test]
async fn a_keyless_aggregate_answers_its_identity_row_on_a_lane_that_saw_no_batch() {
    let walked =
        assert_walk_matches_datafusion_with(KEYLESS_OVER_AN_EMPTY_LANE, TWO_LANES, &mut no_hook)
            .await;
    assert_eq!(
        walked.unfed,
        1,
        "one done call, on the empty lane: {}",
        trail(&walked.calls)
    );
}

#[tokio::test]
async fn a_keyless_aggregate_over_lanes_with_rows_makes_no_done_call() {
    let walked = assert_walk_matches_datafusion_with(KEYLESS_SUM, TWO_LANES, &mut no_hook).await;
    assert_eq!(walked.unfed, 0, "{}", trail(&walked.calls));
    assert_eq!(
        times(&walked.calls, FbKind::Aggregate { merge: false }),
        2,
        "one init call per lane"
    );
}
