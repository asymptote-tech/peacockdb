//! An interval literal from SQL, planned by DataFusion over pbench: it crosses as the operand
//! of a date or naive timestamp ± and is refused anywhere else, naming #249, before the device
//! sees it.

use super::*;
use crate::planner;
use crate::test_support::{MODES, data_dir_for};

/// `sql` over pbench at tp1-single, through the planner to the recipe writer: `Err` is the
/// writer's refusal.
async fn written(sql: &str) -> Result<(), String> {
    let mode = &MODES[0];
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for("pbench", "1"),
    )
    .await
    .expect("register the tables");
    let physical = ctx
        .sql(sql)
        .await
        .expect("DataFusion plans it")
        .create_physical_plan()
        .await
        .expect("a physical plan");
    let (tree, _memory) =
        planner::plan(&physical, mode.knobs_for("pbench")).expect("the engine plans it");
    attach_recipes(tree.as_ref(), DEFAULT_CHUNK_BYTES)
        .map(|_| ())
        .map_err(|why| why.to_string())
}

#[tokio::test]
async fn a_date_plus_an_interval_literal_crosses() {
    written("SELECT f_dt + INTERVAL '1 month 2 days' AS d FROM fact")
        .await
        .expect("the device's one interval arm");
}

/// The interval is a CASE's answer, and the CASE is the operand: no literal sits beside the
/// date, so only the literal itself can say the device has no arm for it.
#[tokio::test]
async fn an_interval_literal_inside_a_case_operand_is_refused() {
    let said = written(
        "SELECT f_dt + CASE WHEN f_id % 2 = 0 THEN INTERVAL '1' DAY ELSE INTERVAL '2' DAY END \
         AS d FROM fact",
    )
    .await
    .expect_err("the device adds only an interval literal to a date");
    assert!(said.contains("interval"), "{said}");
    assert!(said.contains("(#249)"), "{said}");
}

#[tokio::test]
async fn an_interval_literal_as_a_function_argument_is_refused() {
    let said = written("SELECT date_bin(INTERVAL '1 hour', f_ts_us) AS t FROM fact")
        .await
        .expect_err("no device function takes an interval");
    assert!(said.contains("interval"), "{said}");
    assert!(said.contains("(#249)"), "{said}");
}
