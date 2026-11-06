//! The source rewrite's publication: the new text replaces the file whole, and the staged copy
//! does not outlive the rename.

use super::rewrite;

#[test]
fn a_rewrite_replaces_the_file_and_leaves_no_staged_copy() {
    let dir = std::env::temp_dir().join(format!("pck-rewrite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory");
    let file = dir.join("cases.inc");
    std::fs::write(&file, "the old list, longer than the new one\n").expect("the old list");
    rewrite(&file, "the new list\n");
    let text = std::fs::read_to_string(&file).expect("the rewritten list");
    let mut left: Vec<_> = std::fs::read_dir(&dir)
        .expect("the scratch directory")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    left.sort();
    std::fs::remove_dir_all(&dir).expect("the scratch directory");
    assert_eq!(text, "the new list\n");
    assert_eq!(left, ["cases.inc"]);
}
