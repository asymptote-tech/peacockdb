use std::sync::{Arc, Barrier};

use super::{merge_mode_section, merge_section};
use crate::test_support::Regeneration;

/// Many writers merging one file at once each keep their section. The lock must serialize
/// writers across the rename that publishes, not only writers that opened the same inode
/// (#213): a writer that opened before another's rename otherwise reads the old text and
/// publishes without the other's section.
#[test]
fn concurrent_merges_into_one_file_keep_every_section() {
    const WRITERS: usize = 16;
    let dir = std::env::temp_dir().join(format!("pck-merge-race-{}", std::process::id()));
    for round in 0..20 {
        let path = dir.join(format!("round-{round}.txt"));
        let start = Arc::new(Barrier::new(WRITERS));
        let writers: Vec<_> = (0..WRITERS)
            .map(|i| {
                let (path, start) = (path.clone(), Arc::clone(&start));
                std::thread::spawn(move || {
                    start.wait();
                    let body = format!("body {i}\n");
                    merge_section(&path, &[], &format!("q{i}"), &body, Regeneration::Sections);
                })
            })
            .collect();
        for writer in writers {
            writer.join().expect("a writer");
        }
        let text = std::fs::read_to_string(&path).expect("the merged file");
        let missing: Vec<usize> = (0..WRITERS)
            .filter(|i| !text.contains(&format!("== q{i}\n")))
            .collect();
        assert!(
            missing.is_empty(),
            "round {round} lost sections {missing:?}:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `gpu-result.txt` is keyed by BOTH header fields, so a cycle that reran one cell replaces
/// that cell's section and no other — and the file comes back in one order whatever order
/// the device cases happened to run in, or every pull home is a reordering to read.
#[test]
fn a_mode_keyed_merge_replaces_one_cell_and_orders_the_file() {
    let dir = std::env::temp_dir().join(format!("pck-mode-merge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory");
    let path = dir.join("gpu-result.txt");
    let _ = std::fs::remove_file(&path);
    // Written out of order on purpose. The registry's rows are `cost-registry.csv`'s, where
    // q1 comes before q6 — not `corpus_cases.inc`'s, which opens with q6.
    for (query, mode, body) in [
        ("q6", "tp4-sized", "the sized answer\n"),
        ("q1", "tp1-single", "q1's answer\n"),
        ("q6", "tp1-single", "the single answer\n"),
    ] {
        merge_mode_section(&path, "tpch", "1", query, mode, body);
    }
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "== q1 mode=tp1-single\nq1's answer\n== q6 mode=tp1-single\nthe single answer\n\
         == q6 mode=tp4-sized\nthe sized answer\n",
        "the registry's row order, then the mode sequence"
    );

    merge_mode_section(&path, "tpch", "1", "q6", "tp1-single", "a moved answer\n");
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "== q1 mode=tp1-single\nq1's answer\n== q6 mode=tp1-single\na moved answer\n\
         == q6 mode=tp4-sized\nthe sized answer\n",
        "only the cell that reran moved"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
