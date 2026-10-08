# distinct-companions implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An aggregate node whose DISTINCT aggregates share one argument (up to DataFusion's
coercion casts) plans beside any companion that merges per column, with or without grouping sets
(#62); the shapes it cannot lower are refused by name; the wire's dead `distinct` field goes; q28,
`tpch/rollup-distinct` and `tpch/distinct-functions` reach the cpu corpus.

**Architecture:** `aggregate_sequence` (`planner/translator/aggregate.rs`) splits into a `Stage`
(everything one sequence needs, filled from a DataFusion pair or built by hand) and
`sequence(stage, shuffle)`. A DISTINCT node is classified before its input is translated, then
lowered to two stages: an inner one grouping on `(__distinct_arg, keys)` running the companions'
inits, and an outer one grouping on `keys` whose init runs each DISTINCT aggregate over the
deduplicated argument and each companion's merge rule over the inner's state. `decompose` learns
where an aggregate's init reads from (`InitFrom`).

**Tech stack:** Rust over DataFusion 45 physical plans; cpu tier (`--features rust-only`);
FlatBuffers schema; C++ build only (no device run).

**Spec:** [`distinct-companions.md`](distinct-companions.md). Reviews: of the original proposal,
[`reports/bugfix-proposals/62-review.md`](../reports/bugfix-proposals/62-review.md); of this spec
and plan, the analyst's at dbf44bcc, folded in.

## Global constraints

- No GPU: no device run and no GPU cycle on this chain (it must not contend with chain J for
  nebius-gpu). The task reaches `done` when every CI job but the GPU tests is green.
- No backend change: both backends run the two-stage plan as they run any aggregate sequence.
- A DISTINCT node's input is translated exactly once. `node()` has no memo and numbers every
  source it reaches; tp4-sized's two-pass planner refuses a plan whose passes reach different
  source counts (`planner/pipeline.rs:52-60`).
- Every existing plan golden unchanged except q28's five sections (refusal → plan);
  `recipe-payloads.txt` unchanged. Task 1 is a pure refactor and moves nothing.
- Refusal messages carry the ticket number in the message (`planner/tests/join_refusals.rs`
  header). A refusal pinned for a known gap is a `bug_` test whose name carries no ticket number;
  the number is in a comment above it (`coding-style.md`).
- Production code uses no `#[cfg(test)]` item: `Schema::state_for` is test-only; read
  `schema.agg_state` directly.
- Wire: `AggregateFuncNode.distinct` marked `(deprecated)`; no field slot moves.
- Builds as `build-test.md` documents them: rust-only with `cargo test --features rust-only`;
  C++ in `cpp/build` with `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`; the FFI rung with `scripts/cargo-cudf.sh`.
- Commit messages at most 10 lines. Build in a workspace; never share a cargo target dir.

## Review focus

1. **A NULL in the DISTINCT argument.** One inner group holds it; the outer `count(x)`/`sum(x)`
   skips it. Expected: DataFusion's answer. Pinned in Task 3 (`ss_customer_sk`, 129,392 NULLs).
2. **An empty keyless input.** Expected: one row, counts `0`. The filter must survive row-group
   pruning (`ss_quantity + ss_item_sk < 0`): every lane then gets zero-row batches, the inner
   merge emits one empty batch, and the outer shortcut gives the identity row through the NULL→0
   wrap. Pinned in Task 3.
3. **One column under two coercions.** `count(DISTINCT c), sum(DISTINCT c)` over an `Int32` `c`
   arrives as `c` and `CAST(c AS Int64)`. Expected: one inner key, not #144's refusal. Pinned in
   Task 3.
4. **A decimal argument with a `sum` or `avg` companion.** The outer init's `sum` over a decimal
   state widens it (`state_type`), and that widened type must be declared, or the cpu refuses
   the init's layout; a `sum` companion's finalize then casts to DataFusion's type. Pinned in
   Task 3 (the outer `avg(c_acctbal)$sum` declared `Decimal128(35, 2)`) and by q28 in Task 6.
5. **The DISTINCT argument is also a group key, under a ROLLUP.** `__distinct_arg` is its own
   column, never masked, so the grand total still counts distinct values. Pinned structurally in
   Tasks 3 and 4; no answer-level case, since no rollup runs at tp4 on the cpu before #189.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/planner/translator/aggregate.rs` | `Stage`, `sequence`, `InitFrom`, `decompose`; routes a DISTINCT node to `distinct` before translating its input |
| `peacockdb-core/src/planner/translator/aggregate/distinct.rs` (new) | `stripped`, `classify`, `lower`: the two stages, the refusals |
| `peacockdb-core/src/planner/translator/aggregate/tests.rs` (new) | `decompose` called directly |
| `peacockdb-core/src/planner/translator/tests.rs` | plan tests of the two stages |
| `peacockdb-core/src/planner/tests/join_refusals.rs` | the #62 pin goes; `bug_` tests for #144, #261 |
| `peacockdb-core/src/tests/end_to_end.rs` | answers at five modes; `sql_answers_match_oracle` |
| `peacockdb-core/src/test_support/corpus.rs` | `CpuOracle::DataFusionDisabled` |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/aggregate_writer.rs`, `cpp/src/operators/aggregate.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `distinct` deprecated |
| `testdata/tpch-queries/rollup-distinct.sql`, `distinct-functions.sql` (new), goldens, `testdata/cost-registry.csv`, `peacockdb-core/tests/common/corpus_cases.inc` | the corpus |
| `peacockdb-core/src/plan/mod.rs`, `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | docs |

---

### Task 1: Split `aggregate_sequence` into a `Stage` and `sequence`

A refactor: the same trees, built through a shape the lowering can fill by hand.

**Files:**
- Modify: `peacockdb-core/src/planner/translator/aggregate.rs`

**Interfaces:**
- Produces (private to `aggregate.rs`; visible to its child modules):

```rust
/// Where one aggregate's init reads from.
#[derive(Clone)]
enum InitFrom {
    /// The values it was written over: every aggregate but the two below.
    Values,
    /// A DISTINCT aggregate in the outer stage: its own argument rebuilt over the inner
    /// stage's `__distinct_arg` (column 0), DataFusion's casts re-applied, and that
    /// argument's type.
    Deduplicated { arg: Expr, arg_type: DataType },
    /// A companion in the outer stage: the state the inner stage left at these positions.
    State(AggStateColumns),
}

/// Everything one aggregate sequence needs, from a DataFusion pair or built by the
/// DISTINCT lowering.
struct Stage {
    input: Box<dyn GpuNode>,
    input_schema: SchemaRef,
    group_by: Vec<Expr>,
    /// The output's key columns: the group keys, then `__grouping_id` under grouping sets.
    key_fields: Vec<Field>,
    grouping_sets: Vec<Vec<bool>>,
    null_exprs: Vec<Expr>,
    aggregates: Vec<(Arc<AggregateFunctionExpr>, InitFrom)>,
    /// The finished output's schema, keys then one column per aggregate; `None` emits state.
    finished: Option<SchemaRef>,
}

fn sequence(stage: Stage, shuffle: Shuffle) -> Result<Box<dyn GpuNode>, PlanError>;
fn decompose(
    aggregates: &[(Arc<AggregateFunctionExpr>, InitFrom)],
    input_schema: &ArrowSchema,
    n_keys: usize,
) -> Result<Decomposed, PlanError>;
```

- [ ] **Step 1: Baseline.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- planner::
```

  Expected: PASS.

- [ ] **Step 2: Move the body.** `aggregate_sequence(t, partial, finisher, shuffle)` keeps its
  signature. Its order becomes: the filter refusal first (today's l.258-262), then
  `let input = node(t, partial.input())?` — so Task 2 can route before the translation. It
  builds a `Stage` from the partial exactly as l.256-283 do today (`input_schema`, `group_by`,
  `key_fields`, `null_exprs`, `grouping_sets`), with
  `aggregates: partial.aggr_expr().iter().map(|a| (a.clone(), InitFrom::Values)).collect()` and
  `finished: finisher.map(|f| f.schema())`, and calls `sequence(stage, shuffle)`. `sequence`
  holds l.285-395, reading the stage's fields; the `finished` closure reads names and the output
  schema from `stage.finished` where it read `finisher.schema()`.
- [ ] **Step 3: `decompose` takes `(aggregate, InitFrom)` pairs.** Only `InitFrom::Values` is
  built in this task; match the other two arms with
  `unreachable!("the DISTINCT lowering is Task 3")` — Task 3 replaces both.
- [ ] **Step 4: Run.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- planner::
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
git status --short testdata/
```

  Expected: PASS; `testdata/` clean.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/planner/translator/aggregate.rs
git commit -m "aggregate_sequence splits into a Stage and sequence(); no plan moves"
```

---

### Task 2: Classify a DISTINCT node; refuse #144, #261 and the unknown by name

**Files:**
- Create: `peacockdb-core/src/planner/translator/aggregate/distinct.rs`
- Create: `peacockdb-core/src/planner/translator/aggregate/tests.rs`
- Modify: `peacockdb-core/src/planner/translator/aggregate.rs` (`mod distinct;`, `#[cfg(test)] mod tests;`, the route, `decompose`'s check)
- Modify: `peacockdb-core/src/planner/tests/join_refusals.rs`

**Interfaces:**
- Produces, in `distinct.rs`:

```rust
pub(super) const DISTINCT_ARG: &str = "__distinct_arg";

pub(super) enum Classified {
    /// Every DISTINCT aggregate's argument strips to `base`, and every companion merges
    /// per column.
    Lower { base: Arc<dyn PhysicalExpr> },
    Refuse(PlanError),
    /// Not a shape the lowering knows; `decompose`'s check refuses it.
    NotOurs,
}

/// The argument under DataFusion's coercion casts that keep distinct values distinct, and
/// those casts' targets, outermost first.
pub(super) fn stripped(
    expr: &Arc<dyn PhysicalExpr>,
    schema: &ArrowSchema,
) -> (Arc<dyn PhysicalExpr>, Vec<DataType>);

pub(super) fn classify(
    aggregates: &[Arc<AggregateFunctionExpr>],
    input_schema: &ArrowSchema,
    finished: bool,
) -> Classified;
```

- [ ] **Step 1: Write the failing tests** in `join_refusals.rs`, beside the #62 pin (which stays
  until Task 3):

```rust
// #144: a second DISTINCT argument needs a gid-multiplying expand.
#[tokio::test]
async fn bug_two_distinct_arguments_are_refused() {
    let fixture = Fixture::new("refuse-two-distinct").await;
    let err = fixture
        .refused("SELECT count(DISTINCT k), count(DISTINCT v) FROM tiny")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#144")),
        "{err}"
    );
}

// #261: a Welford companion's state merges as one MERGE_M2 call, which an init cannot run.
#[tokio::test]
async fn bug_a_stddev_beside_a_distinct_is_refused() {
    let fixture = Fixture::new("refuse-welford-companion").await;
    let err = fixture
        .refused("SELECT stddev(v), count(DISTINCT k) FROM tiny")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#261")),
        "{err}"
    );
}
```

  And in `aggregate/tests.rs`, the net under the classifier:

```rust
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
    let schema = Arc::new(ArrowSchema::new(vec![Field::new("v", DataType::Int64, true)]));
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
```

- [ ] **Step 2: Run them; all three fail** (the two `bug_` tests see `#62`; the direct one sees
  the old wording).

```bash
cargo test --features rust-only -p peacockdb-core --lib -- bug_two_distinct bug_a_stddev_beside a_distinct_aggregate_reaching
```

- [ ] **Step 3: Implement `stripped` and `classify`** in `distinct.rs`.

```rust
/// A cast that keeps distinct values distinct, so deduplicating under it or over it is
/// the same set. DataFusion adds these to `sum`'s and `avg`'s argument (`sum.rs`,
/// `average.rs`) and not to `count`'s.
fn keeps_distinct(from: &DataType, to: &DataType) -> bool {
    use DataType::*;
    match (from, to) {
        (Int8, Int16 | Int32 | Int64) | (Int16, Int32 | Int64) | (Int32, Int64) => true,
        (UInt8, Int16 | Int32 | Int64 | UInt16 | UInt32 | UInt64)
        | (UInt16, Int32 | Int64 | UInt32 | UInt64)
        | (UInt32, Int64 | UInt64) => true,
        // Exact below 2^53, which no key reaches.
        (from, Float64) if from.is_integer() => true,
        // Wider on both sides of the point.
        (Decimal128(p, s), Decimal128(p2, s2)) => {
            s2 >= s && (*p2 as i16 - *s2 as i16) >= (*p as i16 - *s as i16)
        }
        // Fifteen significant digits fit a double's 15.95.
        (Decimal128(p, _), Float64) => *p <= 15,
        _ => false,
    }
}

pub(super) fn stripped(
    expr: &Arc<dyn PhysicalExpr>,
    schema: &ArrowSchema,
) -> (Arc<dyn PhysicalExpr>, Vec<DataType>) {
    let mut casts = Vec::new();
    let mut expr = expr.clone();
    while let Some(cast) = expr.as_any().downcast_ref::<CastExpr>() {
        let Ok(from) = cast.expr().data_type(schema) else { break };
        if !keeps_distinct(&from, cast.cast_type()) {
            break;
        }
        casts.push(cast.cast_type().clone());
        let inner = cast.expr().clone();
        expr = inner;
    }
    (expr, casts)
}

pub(super) fn classify(
    aggregates: &[Arc<AggregateFunctionExpr>],
    input_schema: &ArrowSchema,
    finished: bool,
) -> Classified {
    // A partial with no final above it hands its state on, and a DISTINCT's state is
    // DataFusion's list of values, which nothing here produces.
    if !finished {
        return Classified::NotOurs;
    }
    let mut base: Option<Arc<dyn PhysicalExpr>> = None;
    for aggregate in aggregates.iter().filter(|a| a.is_distinct()) {
        let expressions = aggregate.expressions();
        let [only] = expressions.as_slice() else {
            return Classified::Refuse(PlanError::Unsupported(format!(
                "{}: a DISTINCT over more than one argument (#144)",
                aggregate.name()
            )));
        };
        let (this, _) = stripped(only, input_schema);
        match &base {
            None => base = Some(this),
            Some(first) if first.as_ref() == this.as_ref() => {}
            Some(_) => {
                return Classified::Refuse(PlanError::Unsupported(format!(
                    "{}: a second DISTINCT argument (#144)",
                    aggregate.name()
                )));
            }
        }
    }
    for companion in aggregates.iter().filter(|a| !a.is_distinct()) {
        // An unknown function is left to `decompose`, which refuses it by name.
        if let Ok(spec) = resolve(companion.fun().name())
            && matches!(decomposition(spec.func).merge, Merge::Combined(_))
        {
            return Classified::Refuse(PlanError::Unsupported(format!(
                "{} beside a DISTINCT aggregate: its state merges in one call, which the \
                 outer stage's init cannot run (#261)",
                companion.name()
            )));
        }
    }
    match base {
        Some(base) => Classified::Lower { base },
        None => Classified::NotOurs,
    }
}
```

  Imports: `std::sync::Arc`; `datafusion::arrow::datatypes::{DataType, Schema as ArrowSchema}`;
  `datafusion::physical_expr::{PhysicalExpr, aggregate::AggregateFunctionExpr}`;
  `datafusion::physical_expr::expressions::CastExpr`; `crate::plan::{Merge, PlanError,
  decomposition, resolve}`. The analyst confirmed `dyn PhysicalExpr: PartialEq` in DataFusion 45.

- [ ] **Step 4: Route, before the input is translated, and the net.** In `aggregate_sequence`,
  between the filter refusal and `node(t, partial.input())`:

```rust
    if partial.aggr_expr().iter().any(|a| a.is_distinct()) {
        match distinct::classify(partial.aggr_expr(), &partial.input().schema(), finisher.is_some()) {
            distinct::Classified::Refuse(err) => return Err(err),
            // Task 3 replaces this arm with the lowering. The message is today's, naming
            // the first DISTINCT aggregate, so no golden moves in this task.
            distinct::Classified::Lower { .. } => {
                let first = partial
                    .aggr_expr()
                    .iter()
                    .find(|a| a.is_distinct())
                    .expect("one is distinct");
                return Err(PlanError::Unsupported(format!(
                    "DISTINCT inside {} (#62)",
                    first.name()
                )));
            }
            distinct::Classified::NotOurs => {}
        }
    }
```

  In `decompose`, the `is_distinct()` check, under `InitFrom::Values` only, becomes:

```rust
            return Err(PlanError::Unsupported(format!(
                "DISTINCT inside {} in a shape the lowering does not handle",
                aggregate.name()
            )));
```

- [ ] **Step 5: Run.** The three new tests PASS; the #62 pin still PASSES; the plan goldens
  unchanged (q28's `== q28` sections still read `DISTINCT inside count(DISTINCT
  store_sales.ss_list_price) (#62)`).

```bash
cargo test --features rust-only -p peacockdb-core --lib -- planner:: translator::
git status --short testdata/
```

- [ ] **Step 6: Commit.**

```bash
git add peacockdb-core/src/planner/translator/aggregate.rs peacockdb-core/src/planner/translator/aggregate/ peacockdb-core/src/planner/tests/join_refusals.rs
git commit -m "#62: classify a DISTINCT node; #144, #261 and the unknown refused by name"
```

---

### Task 3: The two-stage lowering, without grouping sets

**Files:**
- Modify: `peacockdb-core/src/planner/translator/aggregate.rs` (`decompose`'s two arms, the outer finalize)
- Modify: `peacockdb-core/src/planner/translator/aggregate/distinct.rs` (`lower`)
- Modify: `peacockdb-core/src/planner/translator/tests.rs`
- Modify: `peacockdb-core/src/planner/tests/join_refusals.rs` (the #62 pin goes)
- Modify: `peacockdb-core/src/tests/end_to_end.rs`
- Modify: `testdata/goldens/tpcds.sf1/*.plans.txt` (q28's five sections), `testdata/cost-registry.csv` (row 29's five plan cells)

**Interfaces:**
- Consumes: Task 1's `Stage`, `sequence`, `InitFrom`; Task 2's `Classified::Lower { base }`,
  `stripped`.
- Produces: `distinct::lower(t, partial, finisher, shuffle, base) -> Result<Box<dyn GpuNode>, PlanError>`.

- [ ] **Step 1: Write the failing plan tests** in `translator/tests.rs` (minimal dataset:
  `customer` has `c_custkey` Int64, `c_nationkey` Int32, `c_mktsegment`, `c_acctbal`
  `Decimal128(15, 2)`). Add `use crate::plan::{AggregateBody, KeyDistribution};` as needed.

```rust
/// Every aggregate node in the tree, parents first.
fn aggregates_in(node: &dyn GpuNode) -> Vec<&dyn GpuNode> {
    let mut found = match as_node_ref(node) {
        NodeRef::Aggregate(_) | NodeRef::AggregateBatches(_) => vec![node],
        _ => Vec::new(),
    };
    for child in node.children() {
        found.extend(aggregates_in(child));
    }
    found
}

fn body_of(node: &dyn GpuNode) -> &AggregateBody {
    match as_node_ref(node) {
        NodeRef::Aggregate(aggregate) => &aggregate.body,
        NodeRef::AggregateBatches(aggregate) => &aggregate.body,
        _ => panic!("{} is not an aggregate", name_of(node)),
    }
}

#[tokio::test]
async fn a_distinct_beside_an_avg_lowers_to_an_inner_and_an_outer_stage() {
    let tree = translated(
        "SELECT c_nationkey, avg(c_acctbal), count(DISTINCT c_mktsegment) \
         FROM customer GROUP BY c_nationkey",
    )
    .await;
    validate_all(tree.as_ref());
    let nodes = aggregates_in(tree.as_ref());
    // The innermost is the inner init: the distinct argument, then the key; the avg's inits.
    let inner_init = body_of(*nodes.last().expect("an inner init"));
    assert_eq!(inner_init.group_by.len(), 2);
    assert_eq!(
        inner_init.aggs.iter().map(|a| a.func).collect::<Vec<_>>(),
        vec![PlanAgg::Sum, PlanAgg::Count]
    );
    // The outer init sums the avg's two state columns and counts the deduplicated argument.
    let outer_init = *nodes
        .iter()
        .rev()
        .find(|node| body_of(**node).group_by.len() == 1)
        .expect("an outer stage grouping on the key alone");
    assert_eq!(
        body_of(outer_init).aggs.iter().map(|a| a.func).collect::<Vec<_>>(),
        vec![PlanAgg::Sum, PlanAgg::Sum, PlanAgg::Count]
    );
    // Its avg sum is the inner Decimal128(25, 2) summed again: declared widened.
    let sum_state = &body_of(outer_init).aggs[0].outputs[0];
    assert_eq!(sum_state.data_type(), &DataType::Decimal128(35, 2), "{sum_state:?}");
}

#[tokio::test]
async fn a_grouped_distinct_at_four_lanes_shuffles_once_on_the_keys_one_column_right() {
    let tree = translated_at_tp4(
        "SELECT c_nationkey, avg(c_acctbal), count(DISTINCT c_mktsegment) \
         FROM customer GROUP BY c_nationkey",
        0,
    )
    .await;
    validate_all(tree.as_ref());
    let emit = find(tree.as_ref(), &|node| matches!(as_node_ref(node), NodeRef::EmitPartitions(_)))
        .expect("the inner stage's shuffle");
    let NodeRef::EmitPartitions(emit) = as_node_ref(emit) else { unreachable!() };
    assert_eq!(emit.hash_keys, vec![1], "the key, after __distinct_arg");
    assert_eq!(shape(tree.as_ref()).matches("EmitPartitions").count(), 1, "{}", shape(tree.as_ref()));
    // The outer stage's finishing merge is co-located with no shuffle of its own.
    let outer = *aggregates_in(tree.as_ref()).first().expect("the outer finish");
    let layout = outer.kind().layout().expect("a layout");
    assert_eq!(layout.n, 4);
    assert_eq!(layout.key_distribution, KeyDistribution::ByHash { hash_keys: vec![0] });
}

#[tokio::test]
async fn a_keyless_distinct_at_four_lanes_collapses_only_the_inner_stage() {
    let tree = translated_at_tp4(
        "SELECT count(c_custkey), count(DISTINCT c_nationkey) FROM customer",
        0,
    )
    .await;
    validate_all(tree.as_ref());
    assert_eq!(shape(tree.as_ref()).matches("MergePartitions").count(), 1, "{}", shape(tree.as_ref()));
}

#[tokio::test]
async fn a_count_and_a_sum_distinct_of_one_int32_column_share_the_inner_key() {
    // DataFusion casts sum's argument to Int64 and leaves count's: one column, two
    // expressions. count(*) keeps DataFusion's own rewrite out of the way.
    let tree = translated(
        "SELECT count(DISTINCT c_nationkey), sum(DISTINCT c_nationkey), count(*) FROM customer",
    )
    .await;
    validate_all(tree.as_ref());
    let inner_init = body_of(*aggregates_in(tree.as_ref()).last().unwrap());
    assert_eq!(inner_init.group_by.len(), 1, "one key: the stripped argument");
}

#[tokio::test]
async fn a_distinct_argument_that_is_also_a_key_is_still_its_own_inner_key() {
    let tree = translated(
        "SELECT c_nationkey, count(DISTINCT c_nationkey), count(*) FROM customer GROUP BY c_nationkey",
    )
    .await;
    validate_all(tree.as_ref());
    assert_eq!(body_of(*aggregates_in(tree.as_ref()).last().unwrap()).group_by.len(), 2);
}
```

  If `PartitionLayout`'s lane count is not a field `n`, or `KeyDistribution` is not
  `PartialEq`, read them the way the neighbouring layout tests do. If the minimal dataset's
  `c_acctbal` is not `Decimal128(15, 2)`, assert the type `PlanAgg::Sum.state_type` gives twice
  over it.

- [ ] **Step 2: Write the failing end-to-end cases** in `tests/end_to_end.rs`, beside
  `a_two_key_group_by_over_many_rows_does_not_emit_a_group_twice`. DataFusion answers `count`
  and `sum` DISTINCT right, so these use the same-SQL oracle:

```rust
#[tokio::test]
async fn a_count_distinct_beside_an_avg_and_a_count_answers_as_datafusion() {
    // q28's shape: DataFusion's own rewrite declines avg and count companions.
    sql_answers_match_datafusion(
        "tpcds",
        "distinct beside avg",
        "SELECT avg(ss_list_price), count(ss_list_price), count(DISTINCT ss_list_price) \
         FROM store_sales WHERE ss_quantity BETWEEN 0 AND 5",
        None,
        Coverage::ModesOnly,
    )
    .await;
}

#[tokio::test]
async fn a_grouped_count_and_sum_distinct_beside_companions_answer_as_datafusion() {
    sql_answers_match_datafusion(
        "tpch",
        "count and sum distinct",
        "SELECT l_returnflag, count(DISTINCT l_suppkey), sum(DISTINCT l_suppkey), count(*), \
         avg(l_quantity) FROM lineitem GROUP BY l_returnflag",
        None,
        Coverage::ModesOnly,
    )
    .await;
}

#[tokio::test]
async fn a_distinct_over_a_column_holding_nulls_does_not_count_the_null() {
    // ss_customer_sk holds 129,392 NULLs at sf1.
    sql_answers_match_datafusion(
        "tpcds",
        "distinct over nulls",
        "SELECT ss_store_sk, count(DISTINCT ss_customer_sk), count(ss_customer_sk), count(*) \
         FROM store_sales GROUP BY ss_store_sk",
        None,
        Coverage::ModesOnly,
    )
    .await;
}

#[tokio::test]
async fn a_keyless_distinct_over_no_rows_answers_zero() {
    // A predicate the row-group statistics cannot prune: `ss_quantity < 0` prunes every row
    // group and the plan is refused before it runs. This one keeps them, and every lane gets
    // zero-row batches.
    sql_answers_match_datafusion(
        "tpcds",
        "distinct over nothing",
        "SELECT count(ss_customer_sk), count(DISTINCT ss_customer_sk) \
         FROM store_sales WHERE ss_quantity + ss_item_sk < 0",
        None,
        Coverage::ModesOnly,
    )
    .await;
}
```

- [ ] **Step 3: Run; all fail** with the temporary `#62` refusal.

```bash
cargo test --features rust-only -p peacockdb-core --lib -- distinct translator::
```

- [ ] **Step 4: `decompose`'s two arms.** Inside the per-aggregate loop, branch on `from`:

  - `InitFrom::Values`: as today.
  - `InitFrom::Deduplicated { arg, arg_type }`: skip DataFusion's `state_fields` arity check (a
    DISTINCT's declared state is its list of values, nothing this engine runs).
    `args = vec![arg.clone()]`; the arg type is `arg_type`; the state fields as for `Values`, but
    nullable.
  - `InitFrom::State(cols)`: `rule.merge` must be `Merge::PerColumn(funcs)` (the classifier
    guarantees it; otherwise `PlanError::Invalid` naming the aggregate), and
    `cols.positions.len() == rule.state.len()`. For each `i`:

```rust
            let at = cols.positions[i] as usize;
            let inner = input_schema.field(at);
            // The outer init runs the merge rule as an ordinary aggregator over the inner's
            // state, so its output is that aggregator's type over the inner's — a decimal sum
            // widens again — paired by position: our state names (`avg(…)$sum`) carry none
            // of DataFusion's `[sum]` tags.
            let field = Field::new(
                format!("{}{}", aggregate.name(), rule.state[i].0),
                funcs[i].state_type(inner.data_type())?,
                true,
            );
            decomposed.init.push(AggCall {
                func: funcs[i],
                args: vec![Expr::column(at as u32, inner.name())],
                outputs: vec![field.clone()],
            });
            state.push(field);
```

    and skip the `Values` init loop for this aggregate. The merge and the annotation are built
    as for `Values`.

  After `finalize(…)`, for any arm but `Values`:

```rust
        let out_type = aggregate.field().data_type().clone();
        let mut finished = finalize(spec, &state, state_at as u32, &out_type);
        if !matches!(from, InitFrom::Values) {
            // A sum or min/max read off state widened by the outer init is cast back to the
            // type DataFusion declares: no executor changes a type the plan did not ask for.
            if matches!(spec.func, AggFunc::Sum | AggFunc::Min | AggFunc::Max)
                && state[0].data_type() != &out_type
            {
                finished = Expr::Cast { expr: Box::new(finished), target: out_type.clone() };
            }
            // A count merged by sum is NULL over an empty keyless input, where SQL says 0.
            if spec.func == AggFunc::Count {
                finished = Expr::Case {
                    comparand: None,
                    when_then: vec![(
                        Expr::unary(UnaryOp::IsNull, finished.clone()),
                        Expr::Literal(ScalarValue::Int64(Some(0))),
                    )],
                    else_expr: Some(Box::new(finished)),
                };
            }
        }
```

  Imports in `aggregate.rs`: `crate::plan::{AggFunc, UnaryOp}`,
  `datafusion::common::ScalarValue`, `datafusion::arrow::datatypes::SchemaRef`.

- [ ] **Step 5: `lower`, grouping sets refused for now** (Task 4 adds them; until then return
  `PlanError::Unsupported("a DISTINCT under grouping sets (#62)".into())` when
  `!group.is_single()`). In `distinct.rs`:

```rust
pub(super) fn lower(
    t: &Translator,
    partial: &AggregateExec,
    finisher: &AggregateExec,
    shuffle: Shuffle,
    base: Arc<dyn PhysicalExpr>,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let input_schema = partial.input().schema();
    let group = partial.group_expr();
    let n = group.expr().len();
    let base_type = base
        .data_type(&input_schema)
        .map_err(|e| PlanError::Invalid(format!("{DISTINCT_ARG}: {e}")))?;

    // The inner stage: the stripped argument first, its own column even where it is also a
    // key, then the keys; the companions' inits; state out. The input is translated here,
    // once: the route ran before `aggregate_sequence` translated anything.
    let mut group_by = vec![translate_expr(&base, &input_schema)?];
    for (expr, _) in group.expr().iter() {
        group_by.push(translate_expr(expr, &input_schema)?);
    }
    let mut key_fields = vec![Field::new(DISTINCT_ARG, base_type.clone(), true)];
    key_fields.extend((0..n).map(|i| partial.schema().field(i).clone()));
    let companions = partial
        .aggr_expr()
        .iter()
        .filter(|a| !a.is_distinct())
        .map(|a| (a.clone(), InitFrom::Values))
        .collect();
    let inner = sequence(
        Stage {
            input: node(t, partial.input())?,
            input_schema: input_schema.clone(),
            group_by,
            key_fields,
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggregates: companions,
            finished: None,
        },
        inner_shuffle(shuffle),
    )?;

    // The outer stage: the keys, over the inner's state; no shuffle — the inner's hash is on
    // a subset of these keys, or the inner collapsed to one lane.
    let inner_schema = inner.kind().schema().expect("an aggregate is not a sink").clone();
    let outer_group: Vec<Expr> = (1..=n)
        .map(|i| Expr::column(i as u32, inner_schema.fields.field(i).name()))
        .collect();
    let finished = finisher.schema();
    let mut aggregates = Vec::with_capacity(partial.aggr_expr().len());
    for a in partial.aggr_expr() {
        if a.is_distinct() {
            let (_, casts) = stripped(&a.expressions()[0], &input_schema);
            let mut arg = Expr::column(0, DISTINCT_ARG);
            let mut arg_type = base_type.clone();
            for target in casts.iter().rev() {
                arg = Expr::Cast { expr: Box::new(arg), target: target.clone() };
                arg_type = target.clone();
            }
            aggregates.push((a.clone(), InitFrom::Deduplicated { arg, arg_type }));
        } else {
            let cols = inner_schema
                .agg_state
                .iter()
                .find(|s| s.output == a.name())
                .cloned()
                .ok_or_else(|| {
                    PlanError::Invalid(format!("{}: the inner stage holds no state for it", a.name()))
                })?;
            aggregates.push((a.clone(), InitFrom::State(cols)));
        }
    }
    sequence(
        Stage {
            input: inner,
            input_schema: inner_schema.fields.clone(),
            group_by: outer_group,
            key_fields: (0..n).map(|i| finished.field(i).clone()).collect(),
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggregates,
            finished: Some(finished),
        },
        Shuffle::None,
    )
}

/// DataFusion's shuffle, moved one column right: its key ordinals are the partial's, and
/// the inner stage puts `__distinct_arg` first.
fn inner_shuffle(shuffle: Shuffle) -> Shuffle {
    match shuffle {
        Shuffle::ByHash { keys, n } => Shuffle::ByHash {
            keys: keys.iter().map(|key| key + 1).collect(),
            n,
        },
        other => other,
    }
}
```

  Replace Task 2's temporary `Lower` arm with
  `return distinct::lower(t, partial, finisher.expect("classify saw a finisher"), shuffle, base)`.
  It sits before `node(t, partial.input())`, so a lowered node translates its input once, inside
  `lower`. More imports in `distinct.rs`: `super::{InitFrom, Shuffle, Stage, sequence}`, and
  `Translator`, `translate_expr`, `node` by the paths `aggregate.rs` uses for them;
  `crate::plan::{Expr, GpuNode}`, `datafusion::arrow::datatypes::Field`,
  `datafusion::physical_plan::aggregates::AggregateExec`.

- [ ] **Step 6: Run the new tests.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- distinct translator::
```

  Expected: PASS at every mode, tp4-sized included (the input is translated once).

- [ ] **Step 7: The #62 pin goes**: delete
  `a_distinct_beside_a_companion_datafusion_cannot_rewrite_is_refused_naming_62` from
  `join_refusals.rs`; its shape is now `a_distinct_beside_an_avg_lowers_…`.

- [ ] **Step 8: q28's plan goldens, and its registry plan cells in the same commit.**

```bash
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git diff --stat testdata/goldens/
```

  Expected: only the five `tpcds.sf1/*.plans.txt` files change, each in its `== q28` section
  alone, from `refused: … (#62)` to a tree. Any other section moving is a regression: stop. Then
  in `testdata/cost-registry.csv` row 29 (q28), the five plan cells `disabled` → `enabled`
  (`the_registry_matches_the_goldens_in_both_directions` maps a non-refused section to
  `enabled`); its cpu cells stay `na` until Task 6.

- [ ] **Step 9: Run the planner and the cpu corpus.**

```bash
cargo test --features rust-only -p peacockdb-core --lib
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
```

  Expected: PASS.

- [ ] **Step 10: Commit.**

```bash
git add peacockdb-core/src testdata/goldens/tpcds.sf1 testdata/cost-registry.csv
git commit -m "#62: a DISTINCT lowers to two stages beside any per-column companion"
```

---

### Task 4: Grouping sets under a DISTINCT

**Files:**
- Modify: `peacockdb-core/src/planner/translator/aggregate/distinct.rs`
- Modify: `peacockdb-core/src/planner/translator/tests.rs`

**Interfaces:**
- Consumes: Task 3's `lower`, `aggregates_in`, `body_of`.

  No end-to-end case: `sql_answers_match_datafusion` runs all five modes, and the tp4 modes of
  any rollup fail on the cpu on #189 (its shuffle cannot hash `__grouping_id`) until chain J's
  repartition-keys lands. The answer against DataFusion comes from Task 6's
  `tpch/rollup-distinct`, whose tp1 cpu cells compare with DataFusion (`data_fusion_exact`).

- [ ] **Step 1: Write the failing tests** in `translator/tests.rs`:

```rust
/// Whether some project casts an expression to `target`.
fn project_casts_to(tree: &dyn GpuNode, target: &DataType) -> bool {
    find(tree, &|node| match as_node_ref(node) {
        NodeRef::Project(project) => project.exprs.iter().any(|named| {
            matches!(&named.expr, Expr::Cast { target: t, .. } if t == target)
        }),
        _ => false,
    })
    .is_some()
}

#[tokio::test]
async fn a_distinct_under_a_rollup_keeps_its_argument_unmasked_in_every_set() {
    let tree = translated(
        "SELECT c_nationkey, c_mktsegment, count(DISTINCT c_custkey), avg(c_acctbal) \
         FROM customer GROUP BY ROLLUP(c_nationkey, c_mktsegment)",
    )
    .await;
    validate_all(tree.as_ref());
    let inner_init = body_of(*aggregates_in(tree.as_ref()).last().unwrap());
    assert!(!inner_init.grouping_sets.is_empty());
    assert!(
        inner_init.grouping_sets.iter().all(|mask| !mask[0]),
        "{:?}",
        inner_init.grouping_sets
    );
    // Two keys and the argument make three: the id stays UInt8, and nothing narrows it.
    assert!(!project_casts_to(tree.as_ref(), &DataType::UInt8), "{}", shape(tree.as_ref()));
}

#[tokio::test]
async fn a_rollup_over_eight_keys_narrows_the_id_above_the_outer_stage() {
    // Eight keys and the argument make nine: the inner id is UInt16 and DataFusion's UInt8.
    let tree = translated(
        "SELECT c_custkey, c_name, c_address, c_nationkey, c_phone, c_acctbal, c_mktsegment, \
         c_comment, count(DISTINCT n_regionkey), count(*) \
         FROM customer JOIN nation ON c_nationkey = n_nationkey \
         GROUP BY ROLLUP(c_custkey, c_name, c_address, c_nationkey, c_phone, c_acctbal, \
         c_mktsegment, c_comment)",
    )
    .await;
    validate_all(tree.as_ref());
    let inner = *aggregates_in(tree.as_ref()).last().unwrap();
    let fields = &inner.kind().schema().unwrap().fields;
    let id = fields.field_with_name(Aggregate::INTERNAL_GROUPING_ID).expect("the inner id");
    assert_eq!(id.data_type(), &DataType::UInt16);
    assert!(project_casts_to(tree.as_ref(), &DataType::UInt8), "{}", shape(tree.as_ref()));
}
```

  `Aggregate` is `datafusion::logical_expr::Aggregate`. If `GpuProject`'s expression list is not
  a field `exprs` of `NamedExpr`s, read it the way `a_projection_becomes_one_node_per_output_column`
  does. The second test's negative control is the first's last assertion: a grouping-set query
  whose id width does not move gets no narrowing cast, so the positive one is not DataFusion's
  own projection over every rollup.

- [ ] **Step 2: Run; both fail** with Task 3's temporary grouping-sets refusal.
- [ ] **Step 3: Implement.** In `lower`, `key_fields` is `let mut`; under `!group.is_single()`:

```rust
    let sets = !group.is_single();
    let inner_id_type = Aggregate::grouping_id_type(n + 1);
    let (grouping_sets, null_exprs) = if sets {
        // The id folds the first key highest; the argument is never masked, so its bit is
        // always 0 and the inner id is DataFusion's over the keys alone. Only its width can
        // differ: n + 1 keys may need the next type.
        key_fields.push(Field::new(Aggregate::INTERNAL_GROUPING_ID, inner_id_type.clone(), false));
        let masks = group
            .groups()
            .iter()
            .map(|mask| std::iter::once(false).chain(mask.iter().copied()).collect())
            .collect();
        let mut nulls = vec![Expr::Literal(
            ScalarValue::try_from(&base_type)
                .map_err(|e| PlanError::Invalid(format!("{DISTINCT_ARG}: {e}")))?,
        )];
        for (expr, _) in group.null_expr().iter() {
            nulls.push(translate_expr(expr, &input_schema)?);
        }
        (masks, nulls)
    } else {
        (Vec::new(), Vec::new())
    };
```

  The outer stage, under `sets`: `outer_group` gains
  `Expr::column((n + 1) as u32, Aggregate::INTERNAL_GROUPING_ID)`; its `key_fields` are the
  finisher's first `n` fields plus the inner's id field; its `finished` schema is the finisher's
  with field `n` replaced by the inner's id field:

```rust
    let finished_fields: Vec<Field> = finisher
        .schema()
        .fields()
        .iter()
        .enumerate()
        .map(|(i, field)| {
            if sets && i == n {
                Field::new(Aggregate::INTERNAL_GROUPING_ID, inner_id_type.clone(), false)
            } else {
                field.as_ref().clone()
            }
        })
        .collect();
    let finished = Arc::new(ArrowSchema::new(finished_fields));
```

  Call the outer `sequence(…)` result `outer`. Then, where the two id types differ:

```rust
    let wanted = finisher.schema();
    if sets && wanted.field(n).data_type() != &inner_id_type {
        let exprs = wanted
            .fields()
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let column = Expr::column(i as u32, field.name());
                let expr = if i == n {
                    Expr::Cast { expr: Box::new(column), target: field.data_type().clone() }
                } else {
                    column
                };
                NamedExpr::new(expr, field.name())
            })
            .collect();
        return Ok(Box::new(GpuProject::new(outer, exprs, Schema::new(wanted))));
    }
    Ok(outer)
```

  Not a cast in the outer `group_by`: `regrouped_key_distribution` and the finalizing merge's
  co-location check read plain column keys only, so a cast key loses the hash distribution.
  Imports: `datafusion::logical_expr::Aggregate`, `datafusion::common::ScalarValue`,
  `crate::plan::{GpuProject, NamedExpr, Schema}`.

- [ ] **Step 4: Run.**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- distinct rollup translator::
cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

  Expected: PASS; no golden moves.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src
git commit -m "#62: a DISTINCT under grouping sets; the id narrowed above the outer stage"
```

---

### Task 5: The wire's `distinct` field goes

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs:153`
- Modify: `peacockdb-core/src/wire/aggregate_writer.rs:177`
- Modify: `cpp/src/operators/aggregate.cpp:~140-155`
- Modify: `cpp/tests/gpu/test_plan_executor.cpp:610,688,909`

- [ ] **Step 1: The schema.** `distinct: bool;` becomes `distinct: bool (deprecated);`, with a
  comment: never set — the planner lowers a DISTINCT before the wire. The slot stays.
- [ ] **Step 2: The writer.** Delete `distinct: false,` in `aggregate_writer.rs`. The Rust side
  is generated at build time; nothing generated is committed.
- [ ] **Step 3: The C++.** Delete the guard block in `aggregate.cpp` (the comment from "make_agg
  would silently compute" through the loop's closing brace) and the `/*distinct=*/false`
  argument at the three `CreateAggregateFuncNode` calls; the generated signature loses it.
- [ ] **Step 4: Build and run**, as `build-test.md`'s workflow table says:

```bash
scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
ctest --test-dir cpp/build -L cpu --output-on-failure
scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git status --short testdata/goldens/recipe-payloads.txt
```

  Expected: PASS; the C++ build compiles `peacock_plan_tests` (not run: device);
  `recipe-payloads.txt` unchanged — FlatBuffers omits a `false` at its default and nothing sets
  `force_defaults`. If it moved, stop: a payload changed. Read `scripts/cargo-cudf.sh`'s header
  for its exact arguments if they differ.

- [ ] **Step 5: Commit.**

```bash
git add flatbuffers/gpu_plan.fbs peacockdb-core/src/wire/aggregate_writer.rs cpp/src/operators/aggregate.cpp cpp/tests/gpu/test_plan_executor.cpp
git commit -m "#62: AggregateFuncNode.distinct deprecated; its writer, guard and test arguments go"
```

---

### Task 6: q28, `tpch/rollup-distinct` and `tpch/distinct-functions` in the corpus

**Files:**
- Create: `testdata/tpch-queries/rollup-distinct.sql`, `testdata/tpch-queries/distinct-functions.sql`
- Modify: `peacockdb-core/src/test_support/corpus.rs` (`CpuOracle::DataFusionDisabled`)
- Modify: `peacockdb-core/src/tests/end_to_end.rs` (`sql_answers_match_oracle`, the distinct-functions case)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv`
- Modify: `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's `*.cpu.txt`, `*.cost.txt`,
  `*.result.txt` sections for the three queries

- [ ] **Step 1: The query files**, as the spec gives them:

```sql
-- A DISTINCT aggregate under a grouping set. DataFusion's SingleDistinctToGroupBy declines
-- grouping sets, so the flag reaches the translator; the avg companion is q28's.
SELECT l_returnflag, l_linestatus,
       count(DISTINCT l_suppkey), avg(l_quantity), count(*)
FROM lineitem
GROUP BY ROLLUP(l_returnflag, l_linestatus);
```

```sql
-- DISTINCT aggregates of the functions the lowering widens to, beside companions DataFusion's
-- SingleDistinctToGroupBy declines (count(*), avg), so the flag reaches the translator.
SELECT l_returnflag,
       count(DISTINCT l_quantity) AS distinct_qty,
       sum(DISTINCT l_quantity) AS sum_distinct_qty,
       avg(DISTINCT l_quantity) AS avg_distinct_qty,
       stddev(DISTINCT l_quantity) AS stddev_distinct_qty,
       count(*) AS n,
       avg(l_extendedprice) AS avg_price
FROM lineitem
GROUP BY l_returnflag;
```

- [ ] **Step 2: The end-to-end oracle variant**, failing first. In `tests/end_to_end.rs`,
  `sql_answers_match_datafusion`'s body moves into:

```rust
/// The same comparison against a different oracle query: for a query DataFusion answers
/// wrong or refuses, an equivalent one it answers right. The engine runs `sql`; DataFusion
/// runs `oracle_sql`, which must produce the same column names and types.
async fn sql_answers_match_oracle(
    dataset: &str,
    query: &str,
    sql: &str,
    oracle_sql: &str,
    tolerance: Option<f64>,
    coverage: Coverage,
)
```

  where only the oracle's `oracle_ctx.sql(…)` reads `oracle_sql`, and
  `sql_answers_match_datafusion(d, q, sql, t, c)` becomes
  `sql_answers_match_oracle(d, q, sql, sql, t, c)`. Then the case:

```rust
#[tokio::test]
async fn distinct_functions_answer_as_their_hand_lowered_form() {
    // DataFusion 45 refuses stddev(DISTINCT) and answers a grouped decimal avg(DISTINCT) as
    // the plain average, so its oracle is this file lowered by hand: the DISTINCT aggregates
    // over the deduplicated values, joined on the key to the companions over every row.
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("distinct-functions.sql"))
        .expect("the corpus query");
    sql_answers_match_oracle(
        "tpch",
        "distinct-functions",
        &sql,
        "SELECT d.l_returnflag, d.distinct_qty, d.sum_distinct_qty, d.avg_distinct_qty, \
                d.stddev_distinct_qty, c.n, c.avg_price \
         FROM (SELECT l_returnflag, count(l_quantity) AS distinct_qty, \
                      sum(l_quantity) AS sum_distinct_qty, avg(l_quantity) AS avg_distinct_qty, \
                      stddev(l_quantity) AS stddev_distinct_qty \
               FROM (SELECT DISTINCT l_returnflag, l_quantity FROM lineitem) \
               GROUP BY l_returnflag) d \
         JOIN (SELECT l_returnflag, count(*) AS n, avg(l_extendedprice) AS avg_price \
               FROM lineitem GROUP BY l_returnflag) c \
         ON d.l_returnflag = c.l_returnflag",
        Some(1e-12),
        Coverage::ModesOnly,
    )
    .await;
}
```

  Run it: FAIL until the file exists, then PASS (the lowering is Task 3's). If the oracle's
  column types differ from the engine's (a nullable flag, a decimal precision), fix the oracle
  with a `CAST`, not the engine; if the tolerance argument is not a relative tolerance, use the
  form `shuffle-stddev`'s comparison uses.

- [ ] **Step 3: The cpu oracle keyword.** In `test_support/corpus.rs`:

```rust
    /// No DataFusion compare: for a query DataFusion 45 answers wrong or refuses. Its
    /// answer is held another way, which the line's comment names.
    DataFusionDisabled,
```

  `cpu_oracle_mode` maps `"data_fusion_disabled"` to it (and its panic message lists it);
  `rel_tol` returns `None` for it; `assert_answer` returns without comparing for it. Grep for
  the other keywords' names (`rg data_fusion_subset peacockdb-core`) and add the new one
  wherever the set is listed.

- [ ] **Step 4: The corpus lines**, in each dataset's place in `corpus_cases.inc`, in the shape
  the file's lines have when this task builds (chain J's duckdb-oracle adds a field; copy a
  neighbour's). First with every cpu mode:

```
corpus_query!(tpcds, 1, q28, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact, schema_validation_enabled);
corpus_query!(tpch, 1, rollup_distinct, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact, schema_validation_enabled);
// DataFusion 45 refuses stddev(DISTINCT) and answers a grouped decimal avg(DISTINCT) as the
// plain average, so DataFusion is no oracle here; end_to_end.rs's
// distinct_functions_answer_as_their_hand_lowered_form holds the answer. DuckDB's answer is to
// be its oracle once chain J's duckdb-oracle has merged.
corpus_query!(tpch, 1, distinct_functions, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_disabled, golden_exact, schema_validation_enabled);
```

  If duckdb-oracle has merged by now: give `rollup_distinct` and `distinct_functions` their
  `duckdb-result.txt` sections from the pinned `testdata/duckdb_result.py` (as
  `duckdb-oracle-impl.md` does for every tpch query) and turn their DuckDB oracle on; the comment
  then says DuckDB is the oracle.

- [ ] **Step 5: Write and run.**

```bash
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- q28 rollup_distinct distinct_functions
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- q28 rollup_distinct distinct_functions
```

  Expected: q28 all five green (a cross join of six keyless aggregates; a mode failing on a
  known ticket's error stays off under it, an unknown error is filed). rollup-distinct: tp1
  green; tp4 red on #189. distinct-functions: all five green. Drop each red mode from its line,
  then run once more without `UPDATE_CANONICAL`.

- [ ] **Step 6: The registry.**
  - Row 29 (q28): the cpu cells as run, gpu `disabled` × 5; tickets: `62` struck, `152` (the
    cross join, join-backend) and any ticket a cpu mode failed on.
  - New tpch rows after `rollup_over_join`, adjusted to what step 5 ran:

```
tpch,1,rollup_distinct,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,disabled,disabled,disabled,ok,rollup count_distinct,65 189
tpch,1,distinct_functions,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,count_distinct,262
```

  `262` is #262 (the DISTINCT lowering's device cells have never run), filed with the spec; the
  row needs a ticket (`registry.rs`: a disabled cell names its ticket).

```bash
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- registry
cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
cargo test --features rust-only -p peacockdb-core --test test_cost_model
cargo test --features rust-only -p peacockdb-core --lib -- distinct_functions
```

  Expected: PASS.

- [ ] **Step 7: Commit.**

```bash
git add testdata peacockdb-core/tests/common/corpus_cases.inc peacockdb-core/src/test_support/corpus.rs peacockdb-core/src/tests/end_to_end.rs
git commit -m "#62: q28, rollup-distinct and distinct-functions on the cpu corpus"
```

---

### Task 7: The docs

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs:~53`, `llm-wiki/architecture.md`, `llm-wiki/build-test.md`,
  `llm-wiki/tickets/corpus-coverage.md` (#65), `llm-wiki/tickets/complete-coverage.md` (#195, #261)

- [ ] **Step 1: `PlanError::Unsupported`'s doc** drops "a mixed distinct (#62)"; name another
  live refusal in its place (#144).
- [ ] **Step 2: `architecture.md`.**
  - "DISTINCT lowers to grouping": replace the paragraph "Any other companion is refused at plan
    time (#62)…" with the lowering as built — two stages; the argument stripped of DataFusion's
    widening casts, first and never masked; the outer init running each companion's per-column
    merge rule as an ordinary aggregator; what stays refused (#144, #261) and the net in
    `decompose`. Keep the null paragraph, which still holds. Fix l.309-310: the wire's `distinct`
    field is deprecated, not "never set".
  - "Grouping sets" (l.283-284): the plan declares the id `UInt8` — except an inner DISTINCT
    stage, which carries one more key and may declare the next width, narrowed by a project.
  - The wire section (l.875): `aggr_funcs` no longer carries `distinct`.
  - One sentence where the corpus oracles are described: `data_fusion_disabled`, and why
    DataFusion 45 is no oracle for `avg`/`stddev(DISTINCT)` beside a companion.
  Short sentences: this page is read for one fact at a time.
- [ ] **Step 3: Tickets.** #65 gains one line: under a DISTINCT the inner id carries one more
  key, so on the device it is doubled, and at 8, 16 or 32 keys the outer project's cast
  overflows it — the fix covers both. #195's bullet "#144 has no refusal of its own" is
  reworded: it has one now, pinned by `bug_two_distinct_arguments_are_refused`. #261: "Pinned,
  once distinct-companions lands, by a `bug_` test" names
  `bug_a_stddev_beside_a_distinct_is_refused` in `planner/tests/join_refusals.rs`. #62 is
  archived at merge by the helper, not here.
- [ ] **Step 4: `build-test.md` counts**: the translator's (+7 plan tests), the refusals' (one
  pin out, two `bug_` in), the aggregate module's new test, the end-to-end's (+5), the corpus
  rows (+3 queries; their cpu cells), the oracle keyword's description. Recount each row from
  the code; set every header the rows sum into and the grand total to the sums, so the page adds
  up (guard-checks fixes its pre-existing 5-test drift first).
- [ ] **Step 5: The full verification bar.**

```bash
cargo test --features rust-only -p peacockdb-core --lib
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
cargo test --features rust-only -p peacockdb-core --test test_cost_model
scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build
ctest --test-dir cpp/build -L cpu
```

  Expected: all green.

- [ ] **Step 6: Commit.**

```bash
git add peacockdb-core/src/plan/mod.rs llm-wiki
git commit -m "#62: the DISTINCT lowering in architecture.md; #65, #195, #261; counts"
```
