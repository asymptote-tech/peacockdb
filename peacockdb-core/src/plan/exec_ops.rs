//! The 1:1-per-batch nodes: filter, project, and the per-batch sort.

use super::{GpuFilter, GpuProject, GpuSort};
use std::any::Any;

use super::GpuNode;
use super::PlanError;
use super::Schema;
use super::{ColumnOrder, NodeKind, SortOrder};
use super::{Expr, NamedExpr};
use super::{check_column_refs, input_layout, input_schema, rebase_through_projection};

impl GpuNode for GpuFilter {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_column_refs(
            &self.predicate,
            &input_schema(self.input.as_ref()),
            "GpuFilter",
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuProject {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        let against = input_schema(self.input.as_ref());
        for expr in &self.exprs {
            check_column_refs(&expr.expr, &against, "GpuProject")?;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuSort {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        if self.keys.is_empty() {
            return Err(PlanError::Invalid("GpuSort: no sort keys".to_string()));
        }
        let columns = input_schema(self.input.as_ref()).fields.fields().len();
        for key in &self.keys {
            if key.column as usize >= columns {
                return Err(PlanError::Invalid(format!(
                    "GpuSort: sort key @{} is past the {columns} columns its input has",
                    key.column
                )));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn new_filter(
    input: Box<dyn GpuNode>,
    predicate: Expr,
    projection: Option<Vec<u32>>,
    schema: Schema,
) -> GpuFilter {
    // Dropping rows leaves every declared property standing — the lanes, the order
    // within a batch, the key each lane holds — but dropping columns rebases them.
    let layout = match &projection {
        Some(columns) => {
            let kept: Vec<Expr> = columns.iter().map(|c| Expr::column(*c, "")).collect();
            rebase_through_projection(&input_layout(input.as_ref()), &kept)
        }
        None => input_layout(input.as_ref()),
    };
    GpuFilter {
        kind: NodeKind::Intermediate { layout, schema },
        predicate,
        projection,
        input,
    }
}

pub(crate) fn new_project(
    input: Box<dyn GpuNode>,
    exprs: Vec<NamedExpr>,
    schema: Schema,
) -> GpuProject {
    let projected: Vec<Expr> = exprs.iter().map(|e| e.expr.clone()).collect();
    let layout = rebase_through_projection(&input_layout(input.as_ref()), &projected);
    GpuProject {
        kind: NodeKind::Intermediate { layout, schema },
        exprs,
        input,
    }
}

pub(crate) fn new_sort(
    input: Box<dyn GpuNode>,
    keys: Vec<ColumnOrder>,
    fetch: Option<usize>,
) -> GpuSort {
    let mut layout = input_layout(input.as_ref());
    layout.sort_order = SortOrder::batch_sorted(keys.clone());
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuSort {
        kind,
        keys,
        fetch,
        input,
    }
}
