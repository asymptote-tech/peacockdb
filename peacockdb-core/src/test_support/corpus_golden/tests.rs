use std::sync::{Arc, Barrier};

use super::merge_section;
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
