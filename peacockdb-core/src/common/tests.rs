use datafusion::arrow::datatypes::{DataType, Field, Schema};

use super::declared_precisions;

#[test]
fn a_decimals_precision_is_declared_and_every_other_column_is_zero() {
    let schema = Schema::new(vec![
        Field::new("narrow", DataType::Decimal128(15, 2), true),
        Field::new("n", DataType::Int64, true),
        Field::new("wide", DataType::Decimal128(38, 6), true),
    ]);
    assert_eq!(declared_precisions(&schema), vec![15, 0, 38]);
}
