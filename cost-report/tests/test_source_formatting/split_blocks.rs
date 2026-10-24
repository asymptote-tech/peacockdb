//! A doc block a split left behind, over peacockdb-core's `src/` and `tests/`.

/// Two `///` groups separated by a blank line above one item: the lower group is that
/// item's, and the upper one belonged to a declaration that is no longer there. Splitting a
/// function in two and moving only the code leaves this, and nothing reads as missing —
/// every cap still holds and the build is green.
///
/// It covers the SPLIT shape only. The other half of the antipattern is a declaration
/// INSERTED above an existing one, which takes its whole block and leaves no blank line
/// anywhere; that is a property of a diff rather than of a tree, and no check here sees it.
/// The reading habit named in `coding-style.md` is what covers it.
#[test]
fn no_declaration_carries_a_block_left_behind_by_a_split() {
    let mut stranded: Vec<String> = Vec::new();
    for path in rust_sources() {
        let text = std::fs::read_to_string(&path).expect("a source file");
        for (line, at) in stranded_blocks(&text) {
            stranded.push(format!("{}:{}: {line}", path.display(), at + 1));
        }
    }
    assert!(
        stranded.is_empty(),
        "a doc block sits above a blank line and another block — the upper one belonged to \
         a declaration that is gone:\n{}",
        stranded.join("\n")
    );
}

/// The first line of each stranded group, with its line number. A group qualifies when a
/// `///` run is followed by a blank line, then another `///` run, then an item.
fn stranded_blocks(text: &str) -> Vec<(String, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let doc = |at: usize| {
        lines
            .get(at)
            .is_some_and(|l| l.trim_start().starts_with("///"))
    };
    let blank = |at: usize| lines.get(at).is_some_and(|l| l.trim().is_empty());
    let mut found = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if !doc(at) || (at > 0 && (doc(at - 1) || blank_run_above(&lines, at))) {
            at += 1;
            continue;
        }
        let upper = at;
        let mut cursor = at;
        while doc(cursor) {
            cursor += 1;
        }
        if blank(cursor) && doc(cursor + 1) {
            let mut lower = cursor + 1;
            while doc(lower) {
                lower += 1;
            }
            if lines.get(lower).is_some_and(|l| !l.trim().is_empty()) {
                found.push((lines[upper].trim().to_string(), upper));
            }
        }
        at = cursor;
    }
    found
}

/// Whether the line above is blank AND the one above that is a doc line — which means this
/// run is the LOWER half of a pair already reported, not the start of a new one.
fn blank_run_above(lines: &[&str], at: usize) -> bool {
    at >= 2 && lines[at - 1].trim().is_empty() && lines[at - 2].trim_start().starts_with("///")
}

fn rust_sources() -> Vec<std::path::PathBuf> {
    let root = crate::repo_root().join("peacockdb-core");
    let mut found = Vec::new();
    for dir in ["src", "tests"] {
        walk(&root.join(dir), &mut found);
    }
    assert!(found.len() > 50, "only {} sources were read", found.len());
    found
}

fn walk(dir: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => walk(&path, into),
            false if path.extension().is_some_and(|e| e == "rs") => into.push(path),
            false => {}
        }
    }
}

/// The guard's own cases, since the tree is green and a check with nothing to find is one
/// nobody can tell from a check that does not work.
#[test]
fn the_split_guard_finds_the_shape_and_nothing_else() {
    let split = "/// Left behind by a split.\n\n/// The item's own block.\nfn item() {}\n";
    assert_eq!(stranded_blocks(split).len(), 1, "the shape it exists for");

    let contiguous = "/// One block.\n/// Second line.\nfn item() {}\n";
    assert!(
        stranded_blocks(contiguous).is_empty(),
        "an insertion takes the whole block and leaves no blank line — out of scope"
    );

    let two_items = "/// One item.\nfn a() {}\n\n/// Another item.\nfn b() {}\n";
    assert!(
        stranded_blocks(two_items).is_empty(),
        "two documented items in a row are not a stranded block"
    );

    let module_doc = "//! A module.\n\n/// An item.\nfn a() {}\n";
    assert!(
        stranded_blocks(module_doc).is_empty(),
        "a module doc is not one"
    );
}
