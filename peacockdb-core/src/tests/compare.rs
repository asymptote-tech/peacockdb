//! What "the same answer" means for one operator on two backends: slot by slot, since output
//! timing is a function of the call sequence on both; within a slot the column names and
//! types, then the rows, exact. A slot is what one call produced — every batch of a `Vec`
//! return, or one lane of an emit — so a scatter that put a row in the wrong lane is a wrong
//! slot and not a passing multiset. A call that produced no batch is an empty slot, which is
//! not a zero-row batch: a limit outside its interval and a one-call join's finish return
//! nothing, and both sides returning nothing is agreement. Nullability is not compared —
//! the device reports it from the data, and `plan/validate.rs` ignores it for that reason.

use std::sync::Arc;

use datafusion::arrow::array::UInt32Array;
use datafusion::arrow::compute::{
    SortColumn, cast, concat_batches, lexsort_to_indices, take_record_batch,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::pretty::pretty_format_batches;

use crate::tests::synthetic::{prefixed, synthetic};

pub(crate) type Slot = Vec<RecordBatch>;

/// `Any`: rows sorted by every column before comparing, for the operators with no order
/// contract. `AsEmitted`: the sorts, whose order is the answer; their synthetic keys have
/// no ties, so neither engine's unstable sort can disagree with the other.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Order {
    Any,
    AsEmitted,
}

pub(crate) fn assert_same(cpu: &[Slot], gpu: &[Slot], order: Order) {
    if let Err(why) = same(cpu, gpu, order) {
        panic!("cpu and gpu differ: {why}");
    }
}

/// The fallible form, so the comparator's own tests can read the reason.
pub(crate) fn same(cpu: &[Slot], gpu: &[Slot], order: Order) -> Result<(), String> {
    if cpu.len() != gpu.len() {
        return Err(format!(
            "cpu produced {} slot{}, gpu {} slot{}",
            cpu.len(),
            plural(cpu.len()),
            gpu.len(),
            plural(gpu.len())
        ));
    }
    for (index, (c, g)) in cpu.iter().zip(gpu).enumerate() {
        same_slot(c, g, order).map_err(|why| format!("slot {index}: {why}"))?;
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn same_slot(cpu: &Slot, gpu: &Slot, order: Order) -> Result<(), String> {
    let (c, g) = match (one_table(cpu), one_table(gpu)) {
        (None, None) => return Ok(()),
        (Some(c), None) => {
            return Err(format!("gpu produced no batch, cpu {} rows", c.num_rows()));
        }
        (None, Some(g)) => {
            return Err(format!("cpu produced no batch, gpu {} rows", g.num_rows()));
        }
        (Some(c), Some(g)) => (c, g),
    };
    if names_and_types(&c) != names_and_types(&g) {
        return Err(format!(
            "schema differs\n  cpu: {}\n  gpu: {}",
            c.schema(),
            g.schema()
        ));
    }
    if c.num_rows() != g.num_rows() {
        return Err(format!(
            "cpu has {} rows, gpu {} rows",
            c.num_rows(),
            g.num_rows()
        ));
    }
    let (c, g) = match order {
        Order::Any => (sorted(&c), sorted(&g)),
        Order::AsEmitted => (c, g),
    };
    // Nullability is out of the comparison, so the rows are compared column by column
    // rather than as batches, whose equality would read the fields' flags too.
    let differ = c
        .columns()
        .iter()
        .zip(g.columns())
        .any(|(a, b)| a.as_ref() != b.as_ref());
    if differ {
        return Err(format!(
            "rows differ\n  cpu:\n{}\n  gpu:\n{}",
            pretty_format_batches(&[c]).unwrap(),
            pretty_format_batches(&[g]).unwrap()
        ));
    }
    Ok(())
}

fn names_and_types(batch: &RecordBatch) -> Vec<(String, DataType)> {
    batch
        .schema()
        .fields()
        .iter()
        .map(|f| (f.name().clone(), f.data_type().clone()))
        .collect()
}

/// A slot's batches concatenated: what one call produced is one table, however it was cut.
/// `None` is a call that produced no batch at all.
fn one_table(slot: &Slot) -> Option<RecordBatch> {
    let first = slot.first()?;
    Some(concat_batches(&first.schema(), slot).expect("one call's batches share a schema"))
}

fn sorted(batch: &RecordBatch) -> RecordBatch {
    if batch.num_rows() == 0 {
        return batch.clone();
    }
    let columns: Vec<SortColumn> = batch
        .columns()
        .iter()
        .map(|values| SortColumn {
            values: values.clone(),
            options: None,
        })
        .collect();
    let indices = lexsort_to_indices(&columns, None).expect("every synthetic type sorts");
    take_record_batch(batch, &indices).expect("a permutation of the batch")
}

#[test]
fn a_slot_whose_value_differs_is_named_with_the_slot() {
    let cpu = vec![vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(8, 2)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the values differ");
    assert!(why.starts_with("slot 0:"), "{why}");
    assert!(why.contains("row"), "{why}");
}

#[test]
fn a_slot_both_sides_left_empty_is_equal_and_one_side_empty_is_named() {
    assert!(same(&[vec![]], &[vec![]], Order::Any).is_ok());
    let why = same(&[vec![synthetic(4, 1)]], &[vec![]], Order::Any).expect_err("one side empty");
    assert!(
        why.contains("gpu produced no batch") && why.contains("4 rows"),
        "{why}"
    );
    let why = same(&[vec![]], &[vec![synthetic(0, 1)]], Order::Any)
        .expect_err("no batch is not zero rows");
    assert!(
        why.contains("cpu produced no batch") && why.contains("0 rows"),
        "{why}"
    );
}

#[test]
fn nullability_alone_is_not_a_difference() {
    let batch = synthetic(8, 1);
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| Field::new(f.name(), f.data_type().clone(), true))
        .collect();
    let all_nullable =
        RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), batch.columns().to_vec()).unwrap();
    assert!(same(&[vec![batch]], &[vec![all_nullable]], Order::AsEmitted).is_ok());
}

#[test]
fn a_slot_whose_type_differs_fails_before_any_row_is_read() {
    let cpu = vec![vec![synthetic(8, 1)]];
    // The same values, then one column retyped.
    let widened = prefixed(&synthetic(8, 1), "");
    let mut fields: Vec<Field> = widened
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect();
    fields[2] = Field::new("i32", DataType::Int64, true);
    let mut columns = widened.columns().to_vec();
    columns[2] = cast(&columns[2], &DataType::Int64).unwrap();
    let gpu = vec![vec![
        RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), columns).unwrap(),
    ]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the schemas differ");
    assert!(why.contains("schema"), "{why}");
    assert!(why.contains("Int32") && why.contains("Int64"), "{why}");
}

#[test]
fn a_differing_row_count_is_named_as_a_count() {
    let cpu = vec![vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(9, 1)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the counts differ");
    assert!(why.contains("8 rows") && why.contains("9 rows"), "{why}");
}

#[test]
fn a_differing_slot_count_is_named_before_any_slot_is_compared() {
    let cpu = vec![vec![synthetic(8, 1)], vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(8, 1)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the slot counts differ");
    assert!(why.contains("2 slots") && why.contains("1 slot"), "{why}");
}

#[test]
fn any_order_sorts_and_as_emitted_does_not() {
    let batch = synthetic(8, 1);
    let reversed = {
        let indices = UInt32Array::from((0..8u32).rev().collect::<Vec<_>>());
        take_record_batch(&batch, &indices).unwrap()
    };
    assert!(
        same(
            &[vec![batch.clone()]],
            &[vec![reversed.clone()]],
            Order::Any
        )
        .is_ok()
    );
    assert!(same(&[vec![batch]], &[vec![reversed]], Order::AsEmitted).is_err());
}

#[test]
fn a_slot_of_several_batches_is_one_table() {
    let whole = synthetic(8, 1);
    let halves = vec![whole.slice(0, 3), whole.slice(3, 5)];
    assert!(same(&[vec![whole]], &[halves], Order::AsEmitted).is_ok());
}

#[test]
#[should_panic(expected = "cpu and gpu differ: slot 0: cpu has 8 rows, gpu 9 rows")]
fn the_asserting_form_panics_with_the_reason() {
    assert_same(
        &[vec![synthetic(8, 1)]],
        &[vec![synthetic(9, 1)]],
        Order::Any,
    );
}
