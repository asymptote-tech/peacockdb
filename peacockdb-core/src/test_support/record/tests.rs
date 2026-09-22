//! The record's reading of a plan against the `--- recipes ---` section a plan golden
//! prints: the two are what a reader joins on `node_seq`, and nothing else compares them.

use std::collections::BTreeSet;

use crate::executor::nodes_as_recorded;
use crate::planner;
use crate::test_support::{data_dir_for, mode_named, queries_dir_for};
use crate::wire::{Payloads, attach_recipes, render_plan_recipes};

use super::declared_steps;

/// A row's `node_seq` names the steps its `--- recipes ---` line prints.
///
/// Not the numbering — `the_index_and_the_recipes_number_the_same_nodes_the_same_way` in
/// the plan goldens holds that. What this adds is the seq set: the section says node N
/// addresses `#3` and `#4`, the record pairs N with `#3` and `#4`, and a reader's join
/// between the two files rests on those being one statement.
///
/// The line-to-node mapping is a precondition, asserted as one. Here rather than in the
/// benchmark binary because a plan needs no device, and this suite runs in CI.
#[tokio::test]
async fn a_rows_node_seq_names_the_steps_its_recipes_line_prints() {
    let mode = mode_named("tp1_single");
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.knobs().target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tables");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("q6.sql")).expect("q6");
    let plan = ctx
        .sql(&sql)
        .await
        .expect("q6 parses")
        .create_physical_plan()
        .await
        .expect("q6 plans");
    let (tree, _) = planner::plan(&plan, mode.knobs()).expect("this mode runs q6");
    let recipes = attach_recipes(tree.as_ref()).expect("a plan's recipes are structural");

    let declared = declared_steps(&recipes);
    let nodes = nodes_as_recorded(tree.as_ref()).expect("the tree indexes");
    // Payloads omitted: what is in question is which seqs a line names, and the payload
    // lines would put `#` characters under it that belong to no call.
    let text = render_plan_recipes(tree.as_ref(), &recipes, Payloads::Omitted);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        nodes.len(),
        "one line per node; the section rendered {} for {} nodes",
        lines.len(),
        nodes.len()
    );

    // Both walks are pre-order, so an index is a node — and each line then states, for
    // that node, the post-order position the record writes into `node_seq`.
    for (line, (name, post_order)) in lines.iter().zip(&nodes) {
        assert!(
            line.trim_start().starts_with(&format!("{name}:")),
            "the section's line {line:?} is not {name}'s — the two walks disagree about order"
        );
        let named: BTreeSet<u32> = line
            .split('#')
            .skip(1)
            .filter_map(|rest| {
                rest.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect();
        assert_eq!(
            named,
            declared[post_order],
            "{name} at post-order {post_order}: the section names {named:?} and the record \
             would write {:?} — a row joining on `node_seq` would land on another node's line",
            declared[post_order]
        );
    }
    assert!(
        nodes.iter().any(|(_, at)| !declared[at].is_empty()),
        "q6 addresses the device, so this cannot pass by every node declaring nothing"
    );
}
