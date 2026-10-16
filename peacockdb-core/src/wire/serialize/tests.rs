//! The conversions' own properties, over strings and a built buffer: which fb member each
//! Arrow type maps to, and what a type the enum cannot name does at a schema.

use std::sync::Arc;

use datafusion::arrow::datatypes::{
    DataType, Field, IntervalUnit, Schema as ArrowSchema, TimeUnit,
};

use super::fb;
use super::{convert_data_type, serialize_schema};
use crate::plan::PlanError;

#[test]
fn every_timestamp_unit_crosses_the_wire_with_its_unit() {
    // The zone is not on the wire: cuDF has no zone, and the values are UTC int64s either
    // way, so a zoned timestamp maps to the same member as a naive one of its unit.
    for (unit, fb_ty) in [
        (TimeUnit::Second, fb::DataType::TimestampSecond),
        (TimeUnit::Millisecond, fb::DataType::TimestampMillisecond),
        (TimeUnit::Microsecond, fb::DataType::TimestampMicrosecond),
        (TimeUnit::Nanosecond, fb::DataType::TimestampNanosecond),
    ] {
        assert_eq!(
            convert_data_type(&DataType::Timestamp(unit.clone(), None)),
            Ok(fb_ty)
        );
        assert_eq!(
            convert_data_type(&DataType::Timestamp(unit, Some("UTC".into()))),
            Ok(fb_ty)
        );
    }
}

// #249: the wire enum has no Interval member, so a schema holding one is refused at plan
// time naming the column — the refusal is what the engine does wrong to a user, and the
// member that would carry it is what #249 still owes. The `Null` this used to write
// instead is gone.
#[test]
fn bug_a_schema_holding_an_interval_is_refused_at_plan_time() {
    let schema: Arc<ArrowSchema> = Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Int32, true),
        Field::new("iv", DataType::Interval(IntervalUnit::MonthDayNano), true),
    ]));
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let err = serialize_schema(&mut b, &schema).expect_err("an interval has no wire type (#249)");
    assert!(
        matches!(&err, PlanError::Unsupported(why) if why.contains("iv") && why.contains("Interval")),
        "{err}"
    );
}

#[test]
fn a_serialized_schema_carries_each_timestamp_unit_under_its_own_name() {
    // `fb_text` renders a field's type through the generated enum's `Debug`, so the name
    // read back here is the one a payload golden would print.
    for (unit, name) in [
        (TimeUnit::Second, "TimestampSecond"),
        (TimeUnit::Millisecond, "TimestampMillisecond"),
        (TimeUnit::Microsecond, "TimestampMicrosecond"),
        (TimeUnit::Nanosecond, "TimestampNanosecond"),
    ] {
        let schema: Arc<ArrowSchema> = Arc::new(ArrowSchema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(unit, None),
            true,
        )]));
        let mut b = flatbuffers::FlatBufferBuilder::new();
        let offset = serialize_schema(&mut b, &schema).expect("it serializes");
        b.finish(offset, None);
        let read = flatbuffers::root::<fb::Schema>(b.finished_data()).expect("it verifies");
        let field = read.fields().expect("one field").get(0);
        assert_eq!(format!("{:?}", field.data_type()), name);
    }
}
