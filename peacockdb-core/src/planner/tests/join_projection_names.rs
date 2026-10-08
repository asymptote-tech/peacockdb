//! A join's projection in the plan goldens, held against its children rather than against the
//! renderer. The renderer names each ordinal from the node's own schema, so reading it back
//! would agree with any mistake in that schema; the children's schemas and the join type are
//! an independent account of the table the ordinals index.

use std::collections::BTreeSet;

use crate::test_support::{MODES, golden_dir_for};

const JOINS: [&str; 3] = ["GpuHashJoin:", "GpuNestedLoopJoin:", "GpuCrossJoin:"];

/// Every type the corpus projects. A type that stopped appearing would pass vacuously.
const PROJECTED_TYPES: [&str; 8] = [
    "Inner",
    "Left",
    "Right",
    "Full",
    "LeftSemi",
    "LeftAnti",
    "LeftMark",
    "RightSemi",
];

#[test]
fn every_join_projection_in_a_golden_names_the_column_its_ordinal_selects() {
    let mut misnamed = Vec::new();
    let mut seen = BTreeSet::new();
    for (dataset, sf) in [("tpch", "1"), ("tpcds", "1")] {
        for mode in &MODES {
            let path = golden_dir_for(dataset, sf).join(format!("{}.plans.txt", mode.name));
            let text = std::fs::read_to_string(&path).expect("a golden");
            let lines: Vec<&str> = text.lines().collect();
            for (at, line) in lines.iter().enumerate() {
                let body = line.trim_start();
                if !JOINS.iter().any(|join| body.starts_with(join)) {
                    continue;
                }
                let Some(projection) = bracketed(body, "projection=[") else {
                    continue;
                };
                let join_type = join_type_of(body);
                seen.insert(join_type.to_string());
                let [build, probe] = children(&lines, at).map(schema_names);
                let emitted = emitted(join_type, build, probe);
                let own = schema_names(body);
                for (position, item) in top_level(projection).into_iter().enumerate() {
                    let (name, ordinal) = item.rsplit_once('@').expect("name@ordinal");
                    let selected = emitted.get(ordinal.parse::<usize>().expect("an ordinal"));
                    if selected.map(String::as_str) != Some(name)
                        || own.get(position).map(String::as_str) != Some(name)
                    {
                        misnamed.push(format!(
                            "{dataset}/{}: {join_type} prints {item}, selecting {selected:?}",
                            mode.name
                        ));
                    }
                }
            }
        }
    }
    assert!(misnamed.is_empty(), "{}", misnamed.join("\n"));
    for join_type in PROJECTED_TYPES {
        assert!(
            seen.contains(join_type),
            "no {join_type} join projects in the goldens"
        );
    }
}

/// What a join of this type emits before projecting, as names.
fn emitted(join_type: &str, build: Vec<String>, probe: Vec<String>) -> Vec<String> {
    match join_type {
        "Inner" | "Left" | "Right" | "Full" => build.into_iter().chain(probe).collect(),
        "LeftSemi" | "LeftAnti" => build,
        "LeftMark" => build.into_iter().chain(["mark".to_string()]).collect(),
        "RightSemi" | "RightAnti" => probe,
        other => panic!("a join type this test does not know: {other}"),
    }
}

/// A cross join prints no type: it is an inner join without keys.
fn join_type_of(line: &str) -> &str {
    line.split_once("join_type=").map_or("Inner", |(_, rest)| {
        rest.split(',').next().expect("a join type")
    })
}

/// The two lines one indent below the join, build first.
fn children<'a>(lines: &[&'a str], at: usize) -> [&'a str; 2] {
    let depth = indent(lines[at]);
    let below: Vec<&str> = lines[at + 1..]
        .iter()
        .take_while(|line| !line.trim().is_empty() && indent(line) > depth)
        .filter(|line| indent(line) == depth + 2)
        .copied()
        .collect();
    below.try_into().expect("a join has two children")
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The names of a line's `schema=[name:Type, …]`, its last field, quoted as printed.
fn schema_names(line: &str) -> Vec<String> {
    let at = line.rfind("schema=[").expect("a schema");
    top_level(bracketed(&line[at..], "schema=[").expect("a closed schema"))
        .into_iter()
        .map(|field| leading_name(field, ':').to_string())
        .collect()
}

/// The text between `key` (ending in `[`) and its matching `]`.
fn bracketed<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(key)? + key.len();
    let mut depth = 1;
    let mut quoted = false;
    for (offset, c) in line[start..].char_indices() {
        match c {
            '`' => quoted = !quoted,
            '[' | '(' if !quoted => depth += 1,
            ']' | ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(&line[start..start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// A list cut on the commas outside brackets and quotes. A doubled backquote inside a quoted
/// name toggles twice, so it leaves the state as it found it.
fn top_level(list: &str) -> Vec<&str> {
    let (mut items, mut start, mut depth, mut quoted) = (Vec::new(), 0, 0, false);
    for (at, c) in list.char_indices() {
        match c {
            '`' => quoted = !quoted,
            '[' | '(' if !quoted => depth += 1,
            ']' | ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                items.push(list[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    items.push(list[start..].trim());
    items.retain(|item| !item.is_empty());
    items
}

/// The name an item opens with, up to `separator`; a quoted name may hold the separator.
fn leading_name(item: &str, separator: char) -> &str {
    let mut quoted = false;
    for (at, c) in item.char_indices() {
        match c {
            '`' => quoted = !quoted,
            c if c == separator && !quoted => return &item[..at],
            _ => {}
        }
    }
    item
}
