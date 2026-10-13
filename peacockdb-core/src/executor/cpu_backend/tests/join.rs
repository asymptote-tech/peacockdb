//! The join capability matrix, run: one row per mode, each against rows written down here.
//!
//! `dim(k, label)` is the build side and `fact(fk, v)` the probe, the pair the matrix works
//! its examples on. The probe arrives in two batches on purpose — a build row unmatched in
//! the first and matched in the second is the whole reason the finishing types answer at
//! done rather than per call.

use super::*;
use crate::executor::cpu_backend::CpuJoin;
use crate::plan::{GpuCrossJoin, GpuHashJoin, GpuNestedLoopJoin};
use crate::plan::{JoinFilterColumn, JoinSide};
use datafusion::common::JoinType;

const DIM: [(i64, &str); 3] = [(1, "a"), (2, "b"), (3, "c")];
/// Split as `[(2,20), (2,21)]` then `[(4,40)]`: `k=2` matches only the first batch and
/// `k=1` and `k=3` match neither, so a per-batch answer and a finished one differ.
const FACT: [&[(i64, i64)]; 2] = [&[(2, 20), (2, 21)], &[(4, 40)]];

fn dim_columns() -> Vec<(&'static str, DataType)> {
    vec![("k", DataType::Int64), ("label", DataType::Utf8)]
}

fn fact_columns() -> Vec<(&'static str, DataType)> {
    vec![("fk", DataType::Int64), ("v", DataType::Int64)]
}

fn dim() -> CpuBatch {
    let keys: ArrayRef = Arc::new(Int64Array::from(
        DIM.iter().map(|(k, _)| Some(*k)).collect::<Vec<_>>(),
    ));
    let labels: ArrayRef = Arc::new(StringArray::from(
        DIM.iter().map(|(_, l)| Some(*l)).collect::<Vec<_>>(),
    ));
    CpuBatch::new(
        RecordBatch::try_new(
            Arc::new(columns(&dim_columns()).fields.as_ref().clone()),
            vec![keys, labels],
        )
        .expect("the build side fits its schema"),
    )
}

fn fact(rows: &[(i64, i64)]) -> CpuBatch {
    let fks: ArrayRef = Arc::new(Int64Array::from(
        rows.iter().map(|(fk, _)| Some(*fk)).collect::<Vec<_>>(),
    ));
    let vs: ArrayRef = Arc::new(Int64Array::from(
        rows.iter().map(|(_, v)| Some(*v)).collect::<Vec<_>>(),
    ));
    CpuBatch::new(
        RecordBatch::try_new(
            Arc::new(columns(&fact_columns()).fields.as_ref().clone()),
            vec![fks, vs],
        )
        .expect("a probe batch fits its schema"),
    )
}

/// A stub input with the layout a join requires of that side: the build is one batch per
/// lane, the probe streams.
fn side(fields: &[(&str, DataType)], batches: BatchLayout) -> Box<dyn GpuNode> {
    Given::of(columns(fields), batches)
}

fn hash_join(join_type: JoinType, output: &[(&str, DataType)]) -> GpuHashJoin {
    GpuHashJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        join_type,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        None,
        columns(output),
    )
}

/// Every row the join emitted, rendered and sorted: a join's output order is its probe
/// batches' order and its finish pass comes last, so what a mode owes is a multiset.
fn rows_of(batches: impl IntoIterator<Item = CpuBatch>) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for batch in batches {
        let batch = batch.record_batch();
        for row in 0..batch.num_rows() {
            let cells: Vec<String> = (0..batch.num_columns())
                .map(|column| {
                    match ScalarValue::try_from_array(batch.column(column), row)
                        .expect("a value at every position")
                    {
                        value if value.is_null() => "-".to_string(),
                        ScalarValue::Utf8(Some(text)) => text,
                        other => other.to_string(),
                    }
                })
                .collect();
            rows.push(cells.join("|"));
        }
    }
    rows.sort();
    rows
}

/// The build side set, both probe batches through, then the finish — the call sequence a
/// driver makes, in the order the typestate allows.
fn drive(join: CpuJoin) -> Vec<String> {
    let (mut probing, _) = join.set_build(Some(dim())).expect("the build side is set");
    let mut out = Vec::new();
    for batch in FACT {
        let (produced, _) = probing
            .probe_and_fetch(fact(batch))
            .expect("the probe runs");
        out.extend(produced);
    }
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    out.extend(finished);
    rows_of(out)
}

fn both_sides() -> Vec<(&'static str, DataType)> {
    [dim_columns(), fact_columns()].concat()
}

fn rows(rows: &[&str]) -> Vec<String> {
    let mut rows: Vec<String> = rows.iter().map(|row| row.to_string()).collect();
    rows.sort();
    rows
}

/// The four types that emit both sides. Each owes the same two matched rows plus whatever
/// its preserved side adds — and for Left and Full those extra rows can only be known once
/// every probe batch has gone past, which is what the finish pass is.
#[test]
fn the_types_that_emit_both_sides_answer_what_the_matrix_says() {
    let matched = ["2|b|2|20", "2|b|2|21"];
    let build_unmatched = ["1|a|-|-", "3|c|-|-"];
    let probe_unmatched = ["-|-|4|40"];
    let cases: [(JoinType, Vec<&str>); 4] = [
        (JoinType::Inner, matched.to_vec()),
        (
            JoinType::Left,
            [matched.as_slice(), build_unmatched.as_slice()].concat(),
        ),
        (
            JoinType::Right,
            [matched.as_slice(), probe_unmatched.as_slice()].concat(),
        ),
        (
            JoinType::Full,
            [
                matched.as_slice(),
                build_unmatched.as_slice(),
                probe_unmatched.as_slice(),
            ]
            .concat(),
        ),
    ];
    for (join_type, expected) in cases {
        let node = hash_join(join_type, &both_sides());
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        assert_eq!(drive(join), rows(&expected), "{join_type:?}");
    }
}

/// The build-side semi family: no per-call join at all, so the whole answer is the finish
/// pass over the keys the probe batches contributed.
#[test]
fn the_build_side_semi_family_answers_out_of_its_finish_pass_alone() {
    let mark = [dim_columns(), vec![("mark", DataType::Boolean)]].concat();
    let cases: [(JoinType, Vec<(&str, DataType)>, Vec<&str>); 3] = [
        (JoinType::LeftSemi, dim_columns(), vec!["2|b"]),
        (JoinType::LeftAnti, dim_columns(), vec!["1|a", "3|c"]),
        (
            JoinType::LeftMark,
            mark,
            vec!["1|a|false", "2|b|true", "3|c|false"],
        ),
    ];
    for (join_type, output, expected) in cases {
        let node = hash_join(join_type, &output);
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        assert_eq!(drive(join), rows(&expected), "{join_type:?}");
    }
}

/// The probe-side semi family: membership in a complete build side is a per-row question,
/// so every row leaves with its own batch and there is no finish at all.
#[test]
fn the_probe_side_semi_family_answers_per_batch_and_finishes_with_nothing() {
    let cases: [(JoinType, Vec<&str>); 2] = [
        (JoinType::RightSemi, vec!["2|20", "2|21"]),
        (JoinType::RightAnti, vec!["4|40"]),
    ];
    for (join_type, expected) in cases {
        let node = hash_join(join_type, &fact_columns());
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let (mut probing, _) = join.set_build(Some(dim())).expect("the build side is set");
        let mut out = Vec::new();
        for batch in FACT {
            let (produced, _) = probing
                .probe_and_fetch(fact(batch))
                .expect("the probe runs");
            out.extend(produced);
        }
        let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
        assert!(finished.is_none(), "{join_type:?} owes nothing at done");
        assert_eq!(rows_of(out), rows(&expected), "{join_type:?}");
    }
}

/// A projection that keeps two of the four columns, one from each side.
fn projecting_join(
    join_type: JoinType,
    projection: Vec<u32>,
    output: &[(&str, DataType)],
) -> GpuHashJoin {
    GpuHashJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        join_type,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        Some(projection),
        columns(output),
    )
}

/// The finish pass emits the columns the node's projection keeps, in its order — not every
/// build column with a pad after it.
///
/// It is the pad that makes this worth a case rather than the join: the anti join at done
/// emits every build column whatever the projection says, so the project above it is what
/// cuts the row down, and it walks the projection rather than the build side. Every other
/// join case in this file passes `projection: None`, so nothing here reached that walk —
/// which is the CPU twin of a defect the device tests already pin.
#[test]
fn a_projecting_outer_join_pads_the_columns_its_projection_keeps() {
    // [label, v]: one column from each side, and the build key dropped — so a pad that
    // emitted the build side whole would produce three columns, not two.
    let output = [("label", DataType::Utf8), ("v", DataType::Int64)];
    let node = projecting_join(JoinType::Left, vec![1, 3], &output);
    let join = CpuJoin::hash(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the join builds");
    assert_eq!(
        drive(join),
        rows(&["a|-", "b|20", "b|21", "c|-"]),
        "two columns per row, and the unmatched build rows carry a typed NULL for v"
    );
}

/// The one thing a per-batch answer cannot get right on its own: `k=2` matches the first
/// probe batch and nothing in the second. A Left join that answered per batch would pad it
/// as unmatched against the second and emit the row twice.
#[test]
fn a_build_row_matched_in_one_batch_is_not_padded_against_another() {
    let node = hash_join(JoinType::Left, &both_sides());
    let join = CpuJoin::hash(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the join builds");
    let (mut probing, _) = join.set_build(Some(dim())).expect("the build side is set");
    let mut per_batch = Vec::new();
    for batch in FACT {
        let (produced, _) = probing
            .probe_and_fetch(fact(batch))
            .expect("the probe runs");
        per_batch.extend(produced);
    }
    assert_eq!(
        rows_of(per_batch),
        rows(&["2|b|2|20", "2|b|2|21"]),
        "the probe calls emit matches only — a padded row before done would be a guess"
    );
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    assert_eq!(
        rows_of(finished),
        rows(&["1|a|-|-", "3|c|-|-"]),
        "and the finish emits exactly the build rows nothing ever matched"
    );
}

/// No key to co-locate on, so both sides are one lane and every build row meets every row
/// of the batch. Nothing at done: a cross join's answer is decided by (build, this batch).
#[test]
fn a_cross_join_pairs_every_build_row_with_every_probe_row() {
    let node = GpuCrossJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        None,
        columns(&both_sides()),
    );
    let join = CpuJoin::cross(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the cross join builds");
    let answered = drive(join);
    assert_eq!(answered.len(), 9, "three build rows by three probe rows");
    assert!(answered.contains(&"1|a|4|40".to_string()));
    assert!(answered.contains(&"3|c|2|20".to_string()));
}

fn greater(build: u32, probe: u32) -> (Expr, Vec<JoinFilterColumn>) {
    (
        Expr::binary(
            Expr::column(0, "k"),
            BinaryOp::Gt,
            Expr::column(1, "fk"),
            DataType::Boolean,
        ),
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: build,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: probe,
            },
        ],
    )
}

/// A predicate rather than a key: `k > fk` pairs only `k=3` with the two `fk=2` rows.
#[test]
fn a_nested_loop_inner_join_emits_the_pairs_its_predicate_keeps() {
    let (filter, filter_columns) = greater(0, 0);
    let node = GpuNestedLoopJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        JoinType::Inner,
        filter,
        filter_columns,
        None,
        columns(&both_sides()),
    );
    let join = CpuJoin::nested_loop(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the nested loop join builds");
    assert_eq!(drive(join), rows(&["3|c|2|20", "3|c|2|21"]));
}

/// The left form pads the build rows no pair kept, at the finish: the probe call answers
/// the pairs, and only once the last probe batch has gone past is a build row unmatched.
#[test]
fn a_nested_loop_left_join_pads_the_build_rows_no_pair_kept() {
    let (filter, filter_columns) = greater(0, 0);
    let node = GpuNestedLoopJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::SingleBatch),
        JoinType::Left,
        filter,
        filter_columns,
        None,
        columns(&both_sides()),
    );
    let join = CpuJoin::nested_loop(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the nested loop join builds");
    let (mut probing, _) = join.set_build(Some(dim())).expect("the build side is set");
    let whole_probe: Vec<(i64, i64)> = FACT.concat();
    let (produced, _) = probing
        .probe_and_fetch(fact(&whole_probe))
        .expect("the probe runs");
    assert_eq!(rows_of(produced), rows(&["3|c|2|20", "3|c|2|21"]));
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    assert_eq!(rows_of(finished), rows(&["1|a|-|-", "2|b|-|-"]));
}

/// `null_equals_null` decides what the finish owes, not only what a probe call matches: a
/// NULL build key a NULL probe key matched under `true` is not unmatched at done.
#[test]
fn a_null_key_matches_a_null_key_in_the_finish_pass_when_the_node_says_so() {
    let with_nulls = |null_equals_null: bool| {
        GpuHashJoin::new(
            side(&dim_columns(), BatchLayout::SingleBatch),
            side(&fact_columns(), BatchLayout::MultipleBatches),
            JoinType::LeftAnti,
            vec![(0, 0)],
            None,
            Vec::new(),
            null_equals_null,
            None,
            columns(&dim_columns()),
        )
    };
    let keyed_null = |node: &GpuHashJoin| {
        let join = CpuJoin::hash(
            node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let build: ArrayRef = Arc::new(Int64Array::from(vec![None, Some(1i64)]));
        let labels: ArrayRef = Arc::new(StringArray::from(vec![Some("null key"), Some("one")]));
        let build = CpuBatch::new(
            RecordBatch::try_new(
                Arc::new(columns(&dim_columns()).fields.as_ref().clone()),
                vec![build, labels],
            )
            .expect("the build side fits"),
        );
        let (mut probing, _) = join.set_build(Some(build)).expect("the build side is set");
        let probe_keys: ArrayRef = Arc::new(Int64Array::from(vec![None, Some(9i64)]));
        let probe_values: ArrayRef = Arc::new(Int64Array::from(vec![Some(0i64), Some(9)]));
        let probe = CpuBatch::new(
            RecordBatch::try_new(
                Arc::new(columns(&fact_columns()).fields.as_ref().clone()),
                vec![probe_keys, probe_values],
            )
            .expect("the probe fits"),
        );
        probing.probe_and_fetch(probe).expect("the probe runs");
        let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
        rows_of(finished)
    };
    assert_eq!(
        keyed_null(&with_nulls(true)),
        rows(&["1|one"]),
        "the NULL build key found the NULL probe key, so only `1` is unmatched"
    );
    assert_eq!(
        keyed_null(&with_nulls(false)),
        rows(&["-|null key", "1|one"]),
        "and with SQL's own semantics a NULL key matches nothing, so both are unmatched"
    );
}

/// The matrix's second dimension. A residual filter is not decoration: the matrix refuses
/// five of its cells (#153, #159), and the four it admits answer where their unfiltered
/// twins do — so every cell above has a twin here.
///
/// `k * 10 = v` reads both sides, which is what makes it a residual rather than something
/// DataFusion would have pushed below the join. It keeps the `(2, 20)` pair and drops
/// `(2, 21)`.
fn residual() -> (Expr, Vec<JoinFilterColumn>) {
    (
        Expr::binary(
            Expr::binary(
                Expr::column(0, "k"),
                BinaryOp::Multiply,
                Expr::Literal(ScalarValue::Int64(Some(10))),
                DataType::Int64,
            ),
            BinaryOp::Eq,
            Expr::column(1, "v"),
            DataType::Boolean,
        ),
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: 0,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: 1,
            },
        ],
    )
}

fn filtered(join_type: JoinType, output: &[(&str, DataType)]) -> GpuHashJoin {
    let (filter, filter_columns) = residual();
    GpuHashJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        // The planner coalesces a filtered semi join's probe side, and this is that batch.
        side(&fact_columns(), BatchLayout::SingleBatch),
        join_type,
        vec![(0, 0)],
        Some(filter),
        filter_columns,
        false,
        None,
        columns(output),
    )
}

/// Where a filtered cell's rows come out: where the unfiltered cell's do. A residual changes
/// which pairs match, not which call answers — Inner in its probe call, the build-side semi
/// family at the finish, as the session answers both (#220).
#[test]
fn the_filtered_column_answers_where_the_unfiltered_one_does() {
    let cases: [(JoinType, Vec<(&str, DataType)>, Vec<&str>); 4] = [
        (JoinType::Inner, both_sides(), vec!["2|b|2|20"]),
        (JoinType::LeftSemi, dim_columns(), vec!["2|b"]),
        (JoinType::LeftAnti, dim_columns(), vec!["1|a", "3|c"]),
        (
            JoinType::LeftMark,
            [dim_columns(), vec![("mark", DataType::Boolean)]].concat(),
            vec!["1|a|false", "2|b|true", "3|c|false"],
        ),
    ];
    for (join_type, output, expected) in cases {
        let node = filtered(join_type, &output);
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let (mut probing, _) = join.set_build(Some(dim())).expect("the build side is set");
        let whole_probe: Vec<(i64, i64)> = FACT.concat();
        let (per_call, _) = probing
            .probe_and_fetch(fact(&whole_probe))
            .expect("the probe runs");
        let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
        let (answering, silent) = match join_type {
            JoinType::Inner => (per_call, finished),
            _ => (finished, per_call),
        };
        assert!(
            silent.is_none(),
            "{join_type:?} with a residual answers in one call only"
        );
        assert_eq!(
            rows_of(answering),
            rows(&expected),
            "{join_type:?} with a residual"
        );
    }
}

/// The five cells the matrix refuses — the three outer forms and the two right-handed semi
/// ones — each for a defect or a missing cuDF variant rather than for anything the engine
/// decided, so each names the ticket a reader would follow.
#[test]
fn the_filtered_column_refuses_five_cells_by_ticket() {
    let cases: [(JoinType, &str); 5] = [
        (JoinType::Left, "#153"),
        (JoinType::Right, "#153"),
        (JoinType::Full, "#153"),
        (JoinType::RightSemi, "#159"),
        (JoinType::RightAnti, "#159"),
    ];
    for (join_type, ticket) in cases {
        let node = filtered(join_type, &both_sides());
        let refused = match CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        ) {
            Err(refused) => refused,
            Ok(_) => panic!("{join_type:?} with a residual is refused by the matrix"),
        };
        let message = format!("{refused}");
        assert!(
            message.contains(ticket),
            "{join_type:?} has to name {ticket}: {message}"
        );
    }
}

/// A lane whose probe was empty — routine above a shuffle, and the shape where the two
/// backends had no answer between them. An anti join owes every build row: nothing
/// matched, because nothing arrived to match.
#[test]
fn a_finish_over_no_probe_keys_at_all_owes_every_build_row() {
    let node = hash_join(JoinType::LeftAnti, &dim_columns());
    let join = CpuJoin::hash(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the join builds");
    let (probing, _) = join.set_build(Some(dim())).expect("the build side is set");
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    assert_eq!(
        rows_of(finished),
        rows(&["1|a", "2|b", "3|c"]),
        "no probe batch ever arrived, so no build row was ever matched"
    );
}

/// #212 — a build side that produced no batch at all, for a type whose answer is its probe
/// side: the join runs over an empty build table, so Right and Full pad every probe row and
/// RightAnti keeps every one. The six types every row of which is built from a build row say
/// they owe nothing.
#[test]
fn set_build_none_runs_over_an_empty_build() {
    let padded = ["-|-|2|20", "-|-|2|21", "-|-|4|40"];
    let owing: [(JoinType, Vec<(&str, DataType)>, Vec<&str>); 3] = [
        (JoinType::Right, both_sides(), padded.to_vec()),
        (JoinType::Full, both_sides(), padded.to_vec()),
        (
            JoinType::RightAnti,
            fact_columns(),
            vec!["2|20", "2|21", "4|40"],
        ),
    ];
    for (join_type, output, expected) in owing {
        let node = hash_join(join_type, &output);
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let (mut probing, _) = match join.set_build(None) {
            Ok(probing) => probing,
            Err(error) => panic!("{join_type:?} answers an absent build side: {error:?}"),
        };
        assert!(!probing.owes_nothing(), "{join_type:?} owes its probe side");
        let mut out = Vec::new();
        for batch in FACT {
            let (produced, _) = probing
                .probe_and_fetch(fact(batch))
                .expect("the probe runs");
            out.extend(produced);
        }
        let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
        out.extend(finished);
        assert_eq!(rows_of(out), rows(&expected), "{join_type:?}");
    }
    for join_type in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::LeftSemi,
        JoinType::LeftAnti,
        JoinType::LeftMark,
        JoinType::RightSemi,
    ] {
        let node = hash_join(join_type, &both_sides());
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let probing = match join.set_build(None) {
            Ok((probing, _)) => probing,
            Err(error) => panic!("{join_type:?} owes nothing: {error:?}"),
        };
        assert!(probing.owes_nothing(), "{join_type:?}");
    }
}

/// A build batch of no rows owes what no build batch owes: the device reads the row count,
/// and a lane the device drains is one the cpu drains, or the two engines' per-node lists
/// part over a zero-row build.
#[test]
fn a_zero_row_build_batch_owes_what_no_build_batch_owes() {
    for join_type in ALL_TYPES {
        let node = hash_join(join_type, &output_of(join_type));
        let join = CpuJoin::hash(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the join builds");
        let (probing, _) = join
            .set_build(Some(dim_of(&[])))
            .expect("the build side is set");
        assert_eq!(
            probing.owes_nothing(),
            crate::plan::empty_build_owes_nothing(join_type),
            "{join_type:?} over a zero-row build"
        );
    }
}

const ALL_TYPES: [JoinType; 9] = [
    JoinType::Inner,
    JoinType::Left,
    JoinType::Right,
    JoinType::Full,
    JoinType::LeftSemi,
    JoinType::LeftAnti,
    JoinType::LeftMark,
    JoinType::RightSemi,
    JoinType::RightAnti,
];

/// The columns each type emits over `dim` and `fact`, with no projection.
fn output_of(join_type: JoinType) -> Vec<(&'static str, DataType)> {
    match join_type {
        JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => both_sides(),
        JoinType::LeftSemi | JoinType::LeftAnti => dim_columns(),
        JoinType::LeftMark => [dim_columns(), vec![("mark", DataType::Boolean)]].concat(),
        JoinType::RightSemi | JoinType::RightAnti => fact_columns(),
    }
}

/// `dim` over the rows given, so a case can hand the join a build side of its own size.
fn dim_of(rows: &[(i64, &str)]) -> CpuBatch {
    let keys: ArrayRef = Arc::new(Int64Array::from(
        rows.iter().map(|(k, _)| Some(*k)).collect::<Vec<_>>(),
    ));
    let labels: ArrayRef = Arc::new(StringArray::from(
        rows.iter().map(|(_, l)| Some(*l)).collect::<Vec<_>>(),
    ));
    CpuBatch::new(
        RecordBatch::try_new(
            Arc::new(columns(&dim_columns()).fields.as_ref().clone()),
            vec![keys, labels],
        )
        .expect("the build side fits its schema"),
    )
}

/// A context whose operators emit one row per batch, so every answer longer than a row is
/// one DataFusion splits into chunks.
fn chunking_ctx() -> Arc<TaskContext> {
    use datafusion::execution::context::SessionConfig;
    SessionContext::new_with_config(SessionConfig::new().with_batch_size(1)).task_ctx()
}

/// Every probe call and the finish, in order, each as the batches it answered.
fn calls(probing_join: CpuJoin, build: CpuBatch, probe: &[&[(i64, i64)]]) -> Vec<Vec<CpuBatch>> {
    let (mut probing, _) = probing_join
        .set_build(Some(build))
        .expect("the build side is set");
    let mut answered = Vec::new();
    for batch in probe {
        let (produced, _) = probing
            .probe_and_fetch(fact(batch))
            .expect("the probe runs");
        answered.push(produced.into_iter().collect());
    }
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    answered.push(finished.into_iter().collect());
    answered
}

fn hash_join_executor(node: &GpuHashJoin, ctx: Arc<TaskContext>) -> CpuJoin {
    CpuJoin::hash(
        node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx,
    )
    .expect("the join builds")
}

fn nested_loop_executor(join_type: JoinType, ctx: Arc<TaskContext>) -> CpuJoin {
    let (filter, filter_columns) = greater(0, 0);
    let node = GpuNestedLoopJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        join_type,
        filter,
        filter_columns,
        None,
        columns(&both_sides()),
    );
    CpuJoin::nested_loop(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx,
    )
    .expect("the nested loop join builds")
}

/// One batch a call however DataFusion chunks it (#220): at a batch size of one row every
/// answer of two rows or more arrives in pieces, and the call still answers them as one —
/// the same rows a context that does not chunk answers.
#[test]
fn every_type_answers_at_most_one_batch_per_probe_call() {
    let mut joins: Vec<(String, Box<dyn Fn(Arc<TaskContext>) -> CpuJoin>)> = Vec::new();
    for join_type in ALL_TYPES {
        let node = hash_join(join_type, &output_of(join_type));
        joins.push((
            format!("hash {join_type:?}"),
            Box::new(move |ctx| hash_join_executor(&node, ctx)),
        ));
    }
    for join_type in [JoinType::Inner, JoinType::Left] {
        joins.push((
            format!("nested loop {join_type:?}"),
            Box::new(move |ctx| nested_loop_executor(join_type, ctx)),
        ));
    }
    // Two probe rows a build row, so every matching probe call answers more than one row.
    let build = || dim_of(&[(1, "a"), (2, "b"), (2, "bb"), (3, "c")]);
    for (what, join) in &joins {
        let chunked = calls(join(chunking_ctx()), build(), &FACT);
        for (call, answered) in chunked.iter().enumerate() {
            assert!(
                answered.len() <= 1,
                "{what}: call {call} answered {} batches",
                answered.len()
            );
        }
        let whole = calls(join(ctx()), build(), &FACT);
        assert_eq!(
            rows_of(chunked.into_iter().flatten()),
            rows_of(whole.into_iter().flatten()),
            "{what}: the one batch holds every chunk"
        );
    }
}

/// Review Focus 2: LeftSemi, LeftAnti and LeftMark answer nothing per probe call, filtered
/// or not — DataFusion yields a zero-row chunk per probe batch for these, and the device
/// answers handle 0 — and their whole answer at the finish.
#[test]
fn the_build_side_semi_family_answers_nothing_per_probe() {
    let cases: [(JoinType, Vec<&str>, Vec<&str>); 3] = [
        (JoinType::LeftSemi, vec!["2|b"], vec!["2|b"]),
        (JoinType::LeftAnti, vec!["1|a", "3|c"], vec!["1|a", "3|c"]),
        (
            JoinType::LeftMark,
            vec!["1|a|false", "2|b|true", "3|c|false"],
            vec!["1|a|false", "2|b|true", "3|c|false"],
        ),
    ];
    for (join_type, unfiltered, with_residual) in cases {
        for (node, expected) in [
            (hash_join(join_type, &output_of(join_type)), unfiltered),
            (filtered(join_type, &output_of(join_type)), with_residual),
        ] {
            let filtered = node.filter.is_some();
            let mut answered = calls(hash_join_executor(&node, ctx()), dim(), &FACT);
            let finish = answered.pop().expect("a finish slot");
            for (call, probe) in answered.iter().enumerate() {
                assert!(
                    probe.is_empty(),
                    "{join_type:?} filtered={filtered}: probe call {call} answered a batch"
                );
            }
            assert_eq!(
                finish.len(),
                1,
                "{join_type:?} filtered={filtered}: one batch at the finish"
            );
            assert_eq!(
                rows_of(finish),
                rows(&expected),
                "{join_type:?} filtered={filtered}"
            );
        }
    }
}

/// Review Focus 2, the other half: every type that answers per probe answers a zero-row
/// probe batch with one zero-row batch, as the device's session does — not with nothing.
#[test]
fn a_zero_row_probe_between_two_with_rows_is_answered_by_a_zero_row_batch() {
    let probe: [&[(i64, i64)]; 3] = [FACT[0], &[], FACT[1]];
    let mut joins: Vec<(String, CpuJoin)> = Vec::new();
    for join_type in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
        JoinType::RightSemi,
        JoinType::RightAnti,
    ] {
        let node = hash_join(join_type, &output_of(join_type));
        joins.push((
            format!("hash {join_type:?}"),
            hash_join_executor(&node, ctx()),
        ));
    }
    for join_type in [JoinType::Inner, JoinType::Left] {
        joins.push((
            format!("nested loop {join_type:?}"),
            nested_loop_executor(join_type, ctx()),
        ));
    }
    for (what, join) in joins {
        let answered = calls(join, dim(), &probe);
        let empty = &answered[1];
        assert_eq!(
            empty.len(),
            1,
            "{what}: the zero-row probe call answers one batch"
        );
        assert_eq!(empty[0].record_batch().num_rows(), 0, "{what}");
    }
}

/// #190 — a nested loop applies its own projection, Inner and Left alike: two columns of
/// the four, one from each side, and a Left's padded rows carry a typed NULL for `v`.
#[test]
fn a_nested_loop_join_applies_its_projection() {
    let output = [("label", DataType::Utf8), ("v", DataType::Int64)];
    let cases: [(JoinType, Vec<&str>); 2] = [
        (JoinType::Inner, vec!["c|20", "c|21"]),
        (JoinType::Left, vec!["a|-", "b|-", "c|20", "c|21"]),
    ];
    for (join_type, expected) in cases {
        let (filter, filter_columns) = greater(0, 0);
        let node = GpuNestedLoopJoin::new(
            side(&dim_columns(), BatchLayout::SingleBatch),
            side(&fact_columns(), BatchLayout::MultipleBatches),
            join_type,
            filter,
            filter_columns,
            Some(vec![1, 3]),
            columns(&output),
        );
        let join = CpuJoin::nested_loop(
            &node,
            &columns(&dim_columns()).fields,
            &columns(&fact_columns()).fields,
            ctx(),
        )
        .expect("the nested loop join builds");
        assert_eq!(drive(join), rows(&expected), "{join_type:?}");
    }
}

/// #207 — a cross join applies its projection: the label and `v` of every pair.
#[test]
fn a_cross_join_applies_its_projection() {
    let output = [("label", DataType::Utf8), ("v", DataType::Int64)];
    let node = GpuCrossJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        Some(vec![1, 3]),
        columns(&output),
    );
    let join = CpuJoin::cross(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the cross join builds");
    assert_eq!(
        drive(join),
        rows(&[
            "a|20", "a|21", "a|40", "b|20", "b|21", "b|40", "c|20", "c|21", "c|40"
        ])
    );
}

/// #208 — over a build batch of no rows a cross join's probe call answers one zero-row
/// batch, as the session does, where DataFusion's own stream ends without one.
#[test]
fn a_cross_join_over_a_zero_row_build_answers_one_zero_row_batch() {
    let node = GpuCrossJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        None,
        columns(&both_sides()),
    );
    let join = CpuJoin::cross(
        &node,
        &columns(&dim_columns()).fields,
        &columns(&fact_columns()).fields,
        ctx(),
    )
    .expect("the cross join builds");
    let answered = calls(join, dim_of(&[]), &[FACT[0]]);
    assert_eq!(answered[0].len(), 1, "the probe call answers one batch");
    assert_eq!(answered[0][0].record_batch().num_rows(), 0);
    assert!(
        answered[1].is_empty(),
        "a cross join answers nothing at done"
    );
}
