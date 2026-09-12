//! Two shapes the wire refuses before any device is involved: a type and a scalar the flat
//! buffers cannot carry. Each is planned from the sql that reaches it, over tpch sf1 at
//! tp1-single — the knobs `gpu_tests/declared.rs` drives — so what is asserted here is the
//! refusal a walk there would have met as a panic. At the rust rung because a refusal at
//! `attach_recipes` needs no device; `declared-schemas.md` names both.

use crate::plan::PlanError;
use crate::planner::{self, BatchSizing, PlanKnobs, SMALL_TABLE_BYTES};
use crate::test_support::{GPU_BUDGET, data_dir_for};
use crate::wire::{RecipePlan, attach_recipes};

async fn attached(sql: &str) -> Result<RecipePlan, PlanError> {
    let ctx = crate::register_tables_for(crate::build_session_state(1), &data_dir_for("tpch", "1"))
        .await
        .expect("register the tpch sf1 tables");
    let plan = ctx
        .sql(sql)
        .await
        .expect("datafusion plans it")
        .create_physical_plan()
        .await
        .expect("datafusion lowers it");
    let knobs = PlanKnobs {
        target_partitions: 1,
        sizing: BatchSizing::OneBatchPerLane,
        budget: GPU_BUDGET as u64,
        small_table_bytes: SMALL_TABLE_BYTES,
    };
    let (tree, _) = planner::plan(&plan, knobs).expect("the planner accepts the shape");
    attach_recipes(tree.as_ref())
}

/// `gpu_plan.fbs` has no `Timestamp`, so a cast to one has no payload. The refusal is a
/// `PlanError` naming the type, not a panic — [#200](../../../../llm-wiki/tickets.md#t200)'s
/// other half, which nobody had exercised.
#[tokio::test]
async fn a_cast_to_timestamp_is_refused_naming_the_type() {
    let refused =
        attached("SELECT CAST(l_shipdate AS TIMESTAMP) FROM lineitem WHERE l_orderkey = 1")
            .await
            .expect_err("the wire cannot carry a Timestamp");
    assert!(
        matches!(&refused, PlanError::Unsupported(what) if what.contains("Timestamp(Nanosecond, None)")),
        "{refused}"
    );
}

/// The fbs `ScalarValue` has no interval, so a date plus one cannot be written — the
/// plan-time end of [#168](../../../../llm-wiki/tickets.md#t168).
#[tokio::test]
async fn an_interval_literal_is_refused_naming_168() {
    let refused =
        attached("SELECT l_shipdate + interval '1 day' FROM lineitem WHERE l_orderkey = 1")
            .await
            .expect_err("the wire cannot carry an interval");
    assert!(
        matches!(&refused, PlanError::Unsupported(what)
            if what.contains("IntervalMonthDayNano") && what.contains("#168")),
        "{refused}"
    );
}
