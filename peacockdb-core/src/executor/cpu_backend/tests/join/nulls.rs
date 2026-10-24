//! `null_equals_null` on the cpu backend, for every join type under both values: one NULL build
//! key and one NULL probe key, matched under `true` and matching nothing under `false`.

use super::*;

fn keyed(join_type: JoinType, null_equals_null: bool) -> GpuHashJoin {
    GpuHashJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        join_type,
        vec![(0, 0)],
        None,
        Vec::new(),
        null_equals_null,
        None,
        columns(&output_of(join_type)),
    )
}

fn one_row(fields: &[(&str, DataType)], second: ArrayRef) -> CpuBatch {
    let key: ArrayRef = Arc::new(Int64Array::from(vec![None::<i64>]));
    CpuBatch::new(
        RecordBatch::try_new(
            Arc::new(columns(fields).fields.as_ref().clone()),
            vec![key, second],
        )
        .expect("one row fits its schema"),
    )
}

/// Every row the join answers — its probe call and its finish — for a NULL key on each side.
fn answer(join_type: JoinType, null_equals_null: bool) -> Vec<String> {
    let node = keyed(join_type, null_equals_null);
    let join = CpuJoin::hash(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the join builds");
    let build = one_row(&dim_columns(), Arc::new(StringArray::from(vec!["n"])));
    let probe = one_row(&fact_columns(), Arc::new(Int64Array::from(vec![99i64])));
    let (mut probing, _) = join.set_build(Some(build)).expect("the build side is set");
    let (produced, _) = probing.probe_and_fetch(probe).expect("the probe runs");
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    rows_of(produced.into_iter().chain(finished))
}

/// The audit's cpu column: each of the nine types honours `null_equals_null` both ways.
#[test]
fn cpu_null_keys_follow_null_equals_null_for_every_type() {
    for join_type in ALL_TYPES {
        let (matched, unmatched): (&[&str], &[&str]) = match join_type {
            JoinType::Inner => (&["-|n|-|99"], &[]),
            JoinType::Left => (&["-|n|-|99"], &["-|n|-|-"]),
            JoinType::Right => (&["-|n|-|99"], &["-|-|-|99"]),
            JoinType::Full => (&["-|n|-|99"], &["-|n|-|-", "-|-|-|99"]),
            JoinType::LeftSemi => (&["-|n"], &[]),
            JoinType::LeftAnti => (&[], &["-|n"]),
            JoinType::LeftMark => (&["-|n|true"], &["-|n|false"]),
            JoinType::RightSemi => (&["-|99"], &[]),
            JoinType::RightAnti => (&[], &["-|99"]),
        };
        assert_eq!(
            answer(join_type, true),
            rows(matched),
            "{join_type:?} under true"
        );
        assert_eq!(
            answer(join_type, false),
            rows(unmatched),
            "{join_type:?} under false"
        );
    }
}
