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
                // Released uncalled by both executors; here it stays in the session's
                // registry until the session closes.
                if self.rows[handle] == 0 {
                    continue;
                }
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                out.push(only(self.chain(&per_batch, &mut at), node.name()));
            }
            if out.iter().all(|handle| self.rows[handle] == 0) {
                let mut at = At::default();
                let done = only(self.chain(&at_done, &mut at), node.name());
                self.done_rows.push(self.rows[&done]);
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
/// nation's one row group is pruned, so the scan is one lane of no batch (#282): the init's
/// identity row is the whole answer, and a zero or a NULL count or no row each differ from it.
pub(crate) const KEYLESS_OVER_A_SCAN_OF_NOTHING: &str =
    "SELECT count(*) AS n FROM nation WHERE n_nationkey < 0";

/// The empty lane's init makes its done call, handed nothing, and answers one row, count 0
/// and NULLs; the merge folds it in beside the other lane's, and DataFusion agrees. The row
/// count is asserted on its own: a sum of 25 and 0 is a sum of 25 and NULL, and a done call
/// answering no row is dropped, so the answer alone cannot tell them apart.
#[tokio::test]
async fn a_keyless_aggregate_answers_its_identity_row_on_a_lane_that_saw_no_batch() {
    let walked =
        assert_walk_matches_datafusion_with(KEYLESS_OVER_AN_EMPTY_LANE, TWO_LANES, &mut no_hook)
            .await;
    assert_eq!(
        walked.done_rows,
        vec![1],
        "one done call, on the empty lane, answering one row: {}",
        trail(&walked.calls)
    );
}

/// A scan of nothing: the one lane's done call answers the whole answer, `0`.
#[tokio::test]
async fn a_keyless_count_over_a_scan_of_nothing_answers_its_identity_row() {
    let walked = assert_walk_matches_datafusion_with(
        KEYLESS_OVER_A_SCAN_OF_NOTHING,
        TWO_LANES,
        &mut no_hook,
    )
    .await;
    assert_eq!(walked.done_rows, vec![1], "{}", trail(&walked.calls));
}

#[tokio::test]
async fn a_keyless_aggregate_over_lanes_with_rows_makes_no_done_call() {
    let walked = assert_walk_matches_datafusion_with(KEYLESS_SUM, TWO_LANES, &mut no_hook).await;
    assert!(walked.done_rows.is_empty(), "{}", trail(&walked.calls));
    assert_eq!(
        times(&walked.calls, FbKind::Aggregate { merge: false }),
        2,
        "one init call per lane"
    );
}
