//! The `-- declared (rust, pre-serialization) --` section of the payload golden: the schema
//! each call declares its firing produces, one line per call under its node.
//!
//! Renders through `node_text::schema_text`, which prints `Decimal128(15,2)`. Never
//! `wire::fb_text::schema_text`, which formats the fb enum with `{:?}` and prints a bare
//! `Decimal128`, dropping precision and scale. Same name, two modules, and the wrong one
//! loses the digits with nothing going red.

use std::fmt::Write as _;

use super::node_text::schema_text;
use crate::plan::GpuNode;
use crate::wire::RecipePlan;

pub(crate) fn render_declared_schemas(root: &dyn GpuNode, plan: &RecipePlan) -> String {
    let mut text = String::new();
    render_declared_node(root, 0, &mut 0, plan, &mut text);
    text
}

/// Post-order, children first, one position per node — the numbering the recipe plan is
/// indexed by, walked here a second time. `wire::recipes::render_recipe_node` walks the
/// same, and nothing in the types says so; the section test holds the two together.
fn render_declared_node(
    node: &dyn GpuNode,
    depth: usize,
    position: &mut usize,
    plan: &RecipePlan,
    text: &mut String,
) {
    let mut children = String::new();
    for child in node.children() {
        render_declared_node(child, depth + 1, position, plan, &mut children);
    }
    let at = *position;
    *position += 1;
    let indent = "  ".repeat(depth);
    match plan.get(at) {
        // A node that attaches no recipe makes no call, so it has no schema to declare
        // and that is not a gap. Distinguished from `undeclared`, which is one.
        None => {
            let _ = writeln!(text, "{indent}{}: no calls", node.name());
        }
        Some(recipe) => {
            let _ = writeln!(text, "{indent}{}:", node.name());
            for call in &recipe.calls {
                // A bare call has no seq — the exporter's `result_from_handle` is one —
                // so it goes by its symbol, as the recipe section names it.
                let addressed = match call.target {
                    Some((seq, kind)) => format!("#{seq} {kind}"),
                    None => call.symbol.name().to_string(),
                };
                // `undeclared` spelled out: an absent line and a call nothing declared
                // would read the same, and it makes what is not yet declared greppable.
                let declared = match &call.output_schema {
                    Some(schema) => schema_text(schema),
                    None => "undeclared".to_string(),
                };
                let _ = writeln!(text, "{indent}  {addressed}: {declared}");
            }
        }
    }
    text.push_str(&children);
}
