//! `decompose` on its own, for the refusal no SQL reaches.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::functions_aggregate::count::count_udaf;
use datafusion::physical_expr::aggregate::AggregateExprBuilder;
use datafusion::physical_expr::expressions::col;

use super::{InitFrom, decompose};
use crate::plan::PlanError;

/// No SQL reaches it: the classifier lowers or refuses every DISTINCT it is shown. This is
/// the net for a shape it is not shown, which would otherwise run as non-distinct on both
/// engines, where the cpu-vs-device comparison cannot see it.
#[test]
fn a_distinct_aggregate_reaching_decompose_is_refused() {
    let schema = Arc::new(ArrowSchema::new(vec![Field::new(
        "v",
        DataType::Int64,
        true,
    )]));
    let aggregate = AggregateExprBuilder::new(count_udaf(), vec![col("v", &schema).unwrap()])
        .schema(schema.clone())
        .alias("count(DISTINCT v)")
        .distinct()
        .build()
        .expect("a distinct count");
    let err = decompose(&[(Arc::new(aggregate), InitFrom::Values)], &schema, 0)
        .err()
        .expect("refused");
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("in a shape the lowering does not handle")),
        "{err}"
    );
}
