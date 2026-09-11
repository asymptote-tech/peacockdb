//! Cost taxonomy + multiplier config (loaded from `cost_model.conf`) and the
//! `.cost.txt` generator.
//!
//! The `.cost.txt` golden is derived purely from the sibling `.cpu.txt` text: parse each
//! node line's type + `output_bytes`, bin it into a category, then sum `multiplier * bytes`
//! over the categories. No executor run, so cost goldens regenerate without the plan run.
//!
//! The taxonomy + multipliers live in `testdata/cost_model.conf`, read at runtime: a
//! multiplier can be retuned and the goldens regenerated without recompiling, and the
//! format is trivial whitespace columns, so no parser crate.

use super::golden_text::{ordered_sections, parse_node_line};
use super::{Category, CostModel, SKIPPED};

/// Load + parse `cost_model.conf`. It lives under the testdata root (and so is
/// relocated by `PEACOCK_TESTDATA_DIR` together with the goldens it drives, e.g.
/// when the suite runs against shipped artifacts on a remote host).
pub(crate) fn load() -> CostModel {
    let path = super::testdata_root().join("cost_model.conf");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read cost config {}: {e}", path.display()));
    parse(&text)
}

/// Parse the config text. Each non-comment, non-blank line is
/// `<category> <multiplier> [comma,separated,nodes]`.
pub(crate) fn parse(text: &str) -> CostModel {
    let mut categories = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let mut cols = line.split_whitespace();
        let name = cols
            .next()
            .expect("cost config: missing category name")
            .to_string();
        let multiplier = cols
            .next()
            .unwrap_or_else(|| panic!("cost config: '{name}' missing multiplier"))
            .parse()
            .unwrap_or_else(|e| panic!("cost config: '{name}' bad multiplier: {e}"));
        let nodes = match cols.next() {
            Some(list) => list.split(',').map(str::to_string).collect(),
            None => Vec::new(),
        };
        categories.push(Category {
            name,
            multiplier,
            nodes,
        });
    }
    CostModel { categories }
}

/// Category index for a node type, or `None` if it is not in the taxonomy.
fn category_of(model: &CostModel, node_type: &str) -> Option<usize> {
    model
        .categories
        .iter()
        .position(|c| c.nodes.iter().any(|n| n == node_type))
}

/// Derive the `.cost.txt` body from a `.cpu.txt` body. One line per category
/// `<category>=<raw bytes> # <node types>`, then a `peacockdb_cost=<total>`
/// footer where `total = Σ(multiplier * bytes)`. Panics (via `ctx` for the
/// message) on a node type absent from the taxonomy — the taxonomy must be total.
pub(crate) fn cost_text_from_cpu(model: &CostModel, cpu_text: &str, ctx: &str) -> String {
    let mut bytes = vec![0u64; model.categories.len()];
    for line in cpu_text.lines() {
        let Some(node) = parse_node_line(line) else {
            continue;
        };
        let Some(ob) = node.count("output_bytes") else {
            continue;
        };
        let cat = category_of(model, node.name).unwrap_or_else(|| {
            panic!(
                "{ctx}: node type '{}' is not in the cost taxonomy",
                node.name
            )
        });
        bytes[cat] += ob;
    }
    let mut total = 0.0f64;
    let mut out = String::new();
    for (i, c) in model.categories.iter().enumerate() {
        total += c.multiplier * bytes[i] as f64;
        let comment = if c.nodes.is_empty() {
            "(placeholder, no node mapping)".to_string()
        } else {
            c.nodes.join(", ")
        };
        out.push_str(&format!("{}={} # {comment}\n", c.name, bytes[i]));
    }
    out.push_str(&format!("peacockdb_cost={}", total.round() as u64));
    out
}

/// The same derivation over a `.cpu.txt` that holds every query in `== <query>`
/// sections: one cost block per section, in the order the source file holds them. A
/// section carrying a marker rather than a run is copied through — a query skipped at
/// this mode has no bytes to price, and dropping its section would make the two files
/// disagree about which queries exist.
pub(crate) fn cost_text_from_sections(model: &CostModel, cpu_text: &str, ctx: &str) -> String {
    let mut out = String::new();
    for (query, body) in ordered_sections(cpu_text) {
        out.push_str(&format!("== {query}\n"));
        match body.starts_with(SKIPPED) {
            true => out.push_str(&body),
            false => {
                out.push_str(&cost_text_from_cpu(model, &body, &format!("{ctx} {query}")));
                out.push('\n');
            }
        }
    }
    out
}
