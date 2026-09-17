//! The schema validator as a driver hook, on the CPU backend: every batch of a planned
//! query held to its node's declaration, and a declaration that lies refused by name.
//!
//! The CPU backend's `declared_as` already holds every stage to its declaration, so a
//! retyped node would be refused by the executor before the hook saw its batch. The lie is
//! therefore told to the validator alone: the planned tree runs as it is, and the index the
//! validator reads is built over the same tree with one project's field retyped.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};

use crate::executor::{CpuBackend, PlanIndex, RunError, run_with_hook};
use crate::plan::{GpuNode, GpuProject, NodeRef, Schema, as_node_ref};
use crate::planner;
use crate::test_support::{MODES, cpu_schema_validator, data_dir_for, queries_dir_for};
use crate::tests::rebuild::{rebuild, schema_of};

/// A field no planned column is declared as, so the retype always diverges.
const RETYPED_TO: DataType = DataType::Int16;

async fn planned(
    query: &str,
    mode: &crate::test_support::Mode,
) -> (datafusion::prelude::SessionContext, Box<dyn GpuNode>) {
    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join(format!("{query}.sql")))
        .expect("the query text");
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir,
    )
    .await
    .expect("register the tables");
    let plan = ctx
        .sql(&sql)
        .await
        .expect("the query plans")
        .create_physical_plan()
        .await
        .expect("the query has a physical plan");
    let (tree, _memory) = planner::plan(&plan, mode.knobs())
        .unwrap_or_else(|error| panic!("{query} at {}: {error}", mode.name));
    (ctx, tree)
}

#[tokio::test]
async fn every_batch_of_a_small_query_matches_its_nodes_declaration() {
    for mode in &MODES {
        let (ctx, tree) = planned("q6", mode).await;
        let index = PlanIndex::build(tree.as_ref()).expect("the plan indexes");
        let report = run_with_hook::<CpuBackend>(
            tree.as_ref(),
            &ctx.task_ctx(),
            None,
            Some(cpu_schema_validator(&index)),
        )
        .unwrap_or_else(|error| panic!("q6 at {}: {error}", mode.name));
        assert_eq!(
            report.in_flight_bytes, 0,
            "q6 at {} ended holding batches",
            mode.name
        );
    }
}

#[tokio::test]
async fn a_declaration_with_one_field_retyped_is_refused_naming_the_field() {
    let mode = &MODES[0];
    let (ctx, tree) = planned("q6", mode).await;
    let mut retyped = None;
    let lying = with_a_project_retyped(tree.as_ref(), &mut retyped);
    let field = retyped.expect("q6 carries a project");
    let index = PlanIndex::build(lying.as_ref()).expect("the retyped plan indexes");
    let said = match run_with_hook::<CpuBackend>(
        tree.as_ref(),
        &ctx.task_ctx(),
        None,
        Some(cpu_schema_validator(&index)),
    ) {
        Err(RunError::CallFailed(said)) => said,
        other => panic!("expected the validator's refusal, got {other:?}"),
    };
    assert!(
        said.contains("GpuProject"),
        "the refusal names no node: {said}"
    );
    assert!(
        said.contains(&format!("{field}: Int16 vs")),
        "the refusal does not name `{field}` and the type it was declared as: {said}"
    );
}

/// The tree rebuilt with the first project met bottom-up declaring its first field as
/// [`RETYPED_TO`]; `retyped` takes that field's name.
fn with_a_project_retyped(node: &dyn GpuNode, retyped: &mut Option<String>) -> Box<dyn GpuNode> {
    let children: Vec<Box<dyn GpuNode>> = node
        .children()
        .into_iter()
        .map(|child| with_a_project_retyped(child, retyped))
        .collect();
    match as_node_ref(node) {
        NodeRef::Project(project) if retyped.is_none() => {
            let declared = schema_of(node);
            let mut fields: Vec<Field> = declared
                .fields
                .fields()
                .iter()
                .map(|field| field.as_ref().clone())
                .collect();
            assert_ne!(
                fields[0].data_type(),
                &RETYPED_TO,
                "the retype must change the type"
            );
            *retyped = Some(fields[0].name().clone());
            fields[0] = fields[0].clone().with_data_type(RETYPED_TO);
            let child = children.into_iter().next().expect("a project has a child");
            Box::new(GpuProject::new(
                child,
                project.exprs.clone(),
                Schema::new(Arc::new(ArrowSchema::new(fields))),
            ))
        }
        _ => rebuild(node, children),
    }
}
