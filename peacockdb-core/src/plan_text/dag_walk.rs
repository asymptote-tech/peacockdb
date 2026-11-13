//! The one walk every renderer draws its tree with: depth first over the roots in the order
//! given, children left to right, each node visited where a parent reads it.
//!
//! Two rules make a DAG legible as indented text. With several roots each tree is preceded by
//! `root <i>:` and indented one level; with one root there is no header, and the text is the
//! tree's as it always was. A node two or more parents read prints its line prefixed `@<k> `
//! at its first visit, `k` counting such nodes from 1 in walk order; every later visit prints
//! `@<k>` alone and does not descend. The visits are `PlanDag`'s, so a node's index is the one
//! the driver's report and the estimator's model are addressed by.

use std::fmt::Write as _;

use crate::plan::PlanDag;

/// The plan's text: headers and later visits are written here, and each first visit by
/// `first`, handed the node's `PlanDag` index, its indent, its `@k ` prefix (empty for a node
/// one parent reads) and the text to append its lines to.
pub(crate) fn render(
    dag: &PlanDag<'_>,
    first: &mut dyn FnMut(usize, &str, &str, &mut String),
) -> String {
    let mut walk = Walk {
        dag,
        labels: vec![None; dag.nodes.len()],
        shared: 0,
        text: String::new(),
    };
    let several = dag.roots.len() > 1;
    for (ordinal, root) in dag.roots.iter().enumerate() {
        if several {
            let _ = writeln!(walk.text, "root {ordinal}:");
        }
        walk.visit(*root, usize::from(several), first);
    }
    walk.text
}

struct Walk<'d, 'a> {
    dag: &'d PlanDag<'a>,
    labels: Vec<Option<usize>>,
    shared: usize,
    text: String,
}

impl Walk<'_, '_> {
    fn visit(
        &mut self,
        at: usize,
        depth: usize,
        first: &mut dyn FnMut(usize, &str, &str, &mut String),
    ) {
        let indent = "  ".repeat(depth);
        if let Some(label) = self.labels[at] {
            let _ = writeln!(self.text, "{indent}@{label}");
            return;
        }
        let prefix = match self.dag.nodes[at].readers.len() >= 2 {
            true => {
                self.shared += 1;
                self.labels[at] = Some(self.shared);
                format!("@{} ", self.shared)
            }
            false => String::new(),
        };
        first(at, &indent, &prefix, &mut self.text);
        for child in self.dag.nodes[at].children.clone() {
            self.visit(child, depth + 1, first);
        }
    }
}
