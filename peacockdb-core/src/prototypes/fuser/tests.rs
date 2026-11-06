//! What the fuser's tests share: the two choosers they pass in place of the sf40 hold rule, and
//! hand-built plans over real sf1 files, since the fuser re-reads the union's row-group
//! metadata. Below them, the sf1 answer checks.

mod end_to_end;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::{JoinType, ScalarValue};
use datafusion::parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};

use crate::plan::{AggCall, AggregateBody, Batching, BinaryOp, Expr, PlanAgg, Schema};
use crate::plan::{GpuAggregate, GpuCoalesceAllBatches, GpuFilter, GpuHashJoin, GpuLoadParquet};
use crate::plan::{GpuNode, GpuUnload};
use crate::planner::{partition, row_group_metadata};
use crate::test_support::data_dir_for;

use super::{Baseline, Choice, Fused, HeldReader, HoldDecision, Query, QueryTimes, fuse};

/// Every held reader reads its table again.
pub(crate) fn re_read_every_hold(_: &Fused, _: &HeldReader) -> HoldDecision {
    HoldDecision {
        choice: Choice::ReRead,
        bytes: 0,
        reread_us: 0,
        why: "test: re-read".to_string(),
    }
}

/// Every held reader is buffered on the lowest edge of its path.
pub(crate) fn buffer_every_hold_lowest(_: &Fused, held: &HeldReader) -> HoldDecision {
    HoldDecision {
        choice: Choice::Buffer { edge: held.path[0] },
        bytes: 0,
        reread_us: 0,
        why: "test: lowest edge".to_string(),
    }
}

// ── hand-built plans over real sf1 files ─────────────────────────────────────

/// The filters below keep the rows whose first column, an Int64 key, is above this.
const KEY_FLOOR: i64 = 100_000;

pub(crate) fn tpch(table: &str) -> PathBuf {
    data_dir_for("tpch", "1").join(format!("{table}.parquet"))
}

/// A one-lane, one-batch load of `path` over the file ordinals `projection`, keeping the row
/// groups `keep`, as the translator builds one at tp1_single.
pub(crate) fn load(path: &Path, projection: &[u32], keep: &[u32]) -> Box<dyn GpuNode> {
    let file = path.display().to_string();
    let ordinals: Vec<usize> = projection.iter().map(|o| *o as usize).collect();
    let scan = row_group_metadata(&file, &ordinals, Some(keep)).expect("the metadata reads");
    let reader = std::fs::File::open(path).expect("the file opens");
    let arrow = ArrowReaderMetadata::load(&reader, ArrowReaderOptions::new())
        .expect("the footer reads")
        .schema()
        .project(&ordinals)
        .expect("the projection is the file's");
    let schema = match projection.is_empty() {
        true => Schema::rowmarker(),
        false => Schema::new(Arc::new(arrow)),
    };
    let groups = match scan.groups.is_empty() {
        true => vec![Vec::new()],
        false => partition(&scan.groups, 1, Batching::Off).expect("one lane partitions"),
    };
    let table = path
        .file_stem()
        .expect("a stem")
        .to_string_lossy()
        .to_string();
    Box::new(GpuLoadParquet::new(
        table,
        projection.to_vec(),
        groups,
        &scan,
        schema,
    ))
}

pub(crate) fn lineitem(projection: &[u32], keep: &[u32]) -> Box<dyn GpuNode> {
    load(&tpch("lineitem"), projection, keep)
}

pub(crate) fn orders(projection: &[u32], keep: &[u32]) -> Box<dyn GpuNode> {
    load(&tpch("orders"), projection, keep)
}

pub(crate) fn schema_of(node: &dyn GpuNode) -> Schema {
    node.kind().schema().expect("a schema").clone()
}

pub(crate) fn unload(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuUnload::new(input, None))
}

pub(crate) fn filter(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    filter_above(input, KEY_FLOOR)
}

/// Keeps the rows whose first column, an Int64 key, is above `floor`.
pub(crate) fn filter_above(input: Box<dyn GpuNode>, floor: i64) -> Box<dyn GpuNode> {
    let schema = schema_of(input.as_ref());
    let key = schema.fields.field(0).name().clone();
    let predicate = Expr::binary(
        Expr::column(0, &key),
        BinaryOp::Gt,
        Expr::Literal(ScalarValue::Int64(Some(floor))),
        DataType::Boolean,
    );
    Box::new(GpuFilter::new(input, predicate, None, schema))
}

pub(crate) fn coalesce(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuCoalesceAllBatches::new(input))
}

/// `count(<first column>)` over the whole input, keyless, emitting its state.
pub(crate) fn count(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    let key = schema_of(input.as_ref()).fields.field(0).name().clone();
    let output = Field::new("n", DataType::Int64, true);
    let state = Schema::new(Arc::new(ArrowSchema::new(vec![output.clone()])));
    let body = AggregateBody {
        group_by: Vec::new(),
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Count,
            args: vec![Expr::column(0, &key)],
            outputs: vec![output],
        }],
        finalize: None,
    };
    Box::new(GpuAggregate::new(input, body, state.clone(), state))
}

/// An inner join on the first column of each side, emitting both sides' columns.
pub(crate) fn join(build: Box<dyn GpuNode>, probe: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    let fields: Vec<Arc<Field>> = [build.as_ref(), probe.as_ref()]
        .iter()
        .flat_map(|side| {
            schema_of(*side)
                .fields
                .fields()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    let schema = Schema::new(Arc::new(ArrowSchema::new(fields)));
    Box::new(GpuHashJoin::new(
        build,
        probe,
        JoinType::Inner,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        None,
        schema,
    ))
}

pub(crate) fn queries(trees: &[Box<dyn GpuNode>]) -> Vec<Query<'_>> {
    trees
        .iter()
        .enumerate()
        .map(|(at, tree)| Query {
            name: format!("q{at}"),
            tree: tree.as_ref(),
        })
        .collect()
}

pub(crate) fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

pub(crate) fn fused_with(
    trees: &[Box<dyn GpuNode>],
    choose: &mut dyn FnMut(&Fused, &HeldReader) -> HoldDecision,
) -> Fused {
    fuse(&queries(trees), Batching::Off, choose).expect("the plans fuse")
}

pub(crate) fn child(node: &dyn GpuNode, slot: usize) -> &dyn GpuNode {
    node.children()[slot]
}

/// `unload(join(coalesce(orders), filter(lineitem)))` beside `unload(filter(lineitem))`: the
/// first query's lineitem reader sits in the join's hold, the second's outside any.
pub(crate) fn held_pair(first: &[u32], second: &[u32]) -> Vec<Box<dyn GpuNode>> {
    vec![
        unload(join(
            coalesce(orders(&[0], &[0])),
            filter(lineitem(first, &[0, 1, 2])),
        )),
        unload(filter(lineitem(second, &[0, 1, 2]))),
    ]
}

// ── baselines for hand-built plans ───────────────────────────────────────────

/// Each tree's nodes in pre-order: names, and children as indices.
fn pre_order(tree: &dyn GpuNode) -> (Vec<String>, Vec<Vec<usize>>) {
    fn walk(node: &dyn GpuNode, names: &mut Vec<String>, children: &mut Vec<Vec<usize>>) {
        let at = names.len();
        names.push(node.name().to_string());
        children.push(Vec::new());
        for child in node.children() {
            children[at].push(names.len());
            walk(child, names, children);
        }
    }
    let (mut names, mut children) = (Vec::new(), Vec::new());
    walk(tree, &mut names, &mut children);
    (names, children)
}

/// A baseline for `trees` in which query `q`'s node of name `n` took `us(q, n)`, every other
/// figure 0.
pub(crate) fn baseline(trees: &[Box<dyn GpuNode>], us: impl Fn(usize, &str) -> u64) -> Baseline {
    let per_query = trees
        .iter()
        .enumerate()
        .map(|(query, tree)| {
            let (names, children) = pre_order(tree.as_ref());
            let zeros = vec![0; names.len()];
            QueryTimes {
                total_us: names.iter().map(|name| us(query, name)).collect(),
                names,
                children,
                batch_rows: zeros.clone(),
                batch_bytes: zeros,
                load_us: 0,
                execute_us: 0,
                peak_bytes: 0,
            }
        })
        .collect();
    Baseline { per_query }
}
