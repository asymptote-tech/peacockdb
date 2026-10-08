use std::sync::{Arc, Barrier};

use super::{
    gpu_result_cudf_matches_path, merge_mode_section, merge_section, recording_cudf_suits_the_path,
    recording_knob,
};
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
        merge_mode_section(&path, "tpch", "1", query, mode, body, "25.02");
    }
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "cudf=25.02\n== q1 mode=tp1-single\nq1's answer\n\
         == q6 mode=tp1-single\nthe single answer\n== q6 mode=tp4-sized\nthe sized answer\n",
        "the registry's row order, then the mode sequence"
    );

    merge_mode_section(
        &path,
        "tpch",
        "1",
        "q6",
        "tp1-single",
        "a moved answer\n",
        "25.02",
    );
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "cudf=25.02\n== q1 mode=tp1-single\nq1's answer\n\
         == q6 mode=tp1-single\na moved answer\n== q6 mode=tp4-sized\nthe sized answer\n",
        "only the cell that reran moved"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A device cell turned off loses its section on the next recording cycle, and a cell that
/// merely did not run this time keeps its own.
///
/// `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` tells the reader to
/// regenerate, so regenerating has to be what clears a stale section. The rule is the
/// registry's enablement and never "what this run wrote": a filtered cycle writes a subset
/// of the enabled cells, and dropping the rest would destroy a correct file.
#[test]
fn a_mode_keyed_merge_drops_a_cell_the_registry_no_longer_enables() {
    let dir = std::env::temp_dir().join(format!("pck-mode-prune-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory");
    let path = dir.join("gpu-result.txt");
    std::fs::write(
        &path,
        "cudf=25.02\n== q1 mode=tp4-sized\nq1 at a cell nothing reran\n\
         == q12 mode=tp1-single\nan answer from before the cell was turned off\n",
    )
    .expect("the file");
    merge_mode_section(
        &path,
        "tpch",
        "1",
        "q6",
        "tp1-single",
        "q6's answer\n",
        "25.02",
    );
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "cudf=25.02\n== q1 mode=tp4-sized\nq1 at a cell nothing reran\n\
         == q6 mode=tp1-single\nq6's answer\n",
        "tpch q12 has no enabled device cell and tpch q1 at tp4-sized has one"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A recorded device answer carries the cuDF it was produced under, as the file's first line.
///
/// A file-level provenance line and not a per-section field, because the rule is per file — one
/// cuDF per `gpu-result.txt` — and because the section bodies are what the DuckDB comparison
/// reads, which a field inside one would have to be stripped back out of. `merged_cells`
/// rewrites the whole file on every merge, so the one line it writes itself cannot go stale.
#[test]
fn a_mode_keyed_merge_stamps_the_cudf_it_recorded_under() {
    let dir = std::env::temp_dir().join(format!("pck-mode-stamp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory");
    let path = dir.join("gpu-result.txt");
    let _ = std::fs::remove_file(&path);
    merge_mode_section(
        &path,
        "tpch",
        "1",
        "q1",
        "tp1-single",
        "q1's answer\n",
        "25.02",
    );
    merge_mode_section(
        &path,
        "tpch",
        "1",
        "q6",
        "tp1-single",
        "q6's answer\n",
        "25.02",
    );
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text,
        "cudf=25.02\n== q1 mode=tp1-single\nq1's answer\n== q6 mode=tp1-single\nq6's answer\n",
        "one provenance line, before the first section, written once"
    );
    assert_eq!(gpu_result_cudf_matches_path(&text, None), Ok(()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Sections another cuDF recorded are dropped rather than carried under this cuDF's stamp.
///
/// The case is the cycle the stamp exists for: a 26.02 host told to record with
/// `PCK_WRITE_GPU_RESULT=1`. The file it leaves says 26.02 and holds 26.02's answers alone, so
/// the stamp it carries is true of every section under it — a mixed file would make the one
/// line a lie about whichever sections it did not write.
#[test]
fn a_mode_keyed_merge_drops_sections_another_cudf_recorded() {
    let dir = std::env::temp_dir().join(format!("pck-mode-restamp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory");
    let path = dir.join("gpu-result.txt");
    std::fs::write(&path, "cudf=25.02\n== q1 mode=tp4-sized\n25.02's answer\n").expect("the file");
    merge_mode_section(
        &path,
        "tpch",
        "1",
        "q6",
        "tp1-single",
        "26.02's answer\n",
        "26.02",
    );
    let text = std::fs::read_to_string(&path).expect("the merged file");
    assert_eq!(
        text, "cudf=26.02\n== q6 mode=tp1-single\n26.02's answer\n",
        "tpch q1 at tp4-sized is an enabled cell, and 25.02 is what recorded it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A recorded file must carry the cuDF its NAME promises, in both directions: `gpu-result.txt`
/// is 25.02's and `gpu-result-<v>.txt` is v's.
///
/// Both halves are an operator's typo. `PCK_WRITE_GPU_RESULT=1` on a 26.02 host writes 26.02
/// answers over the committed file; `=26.02` on a 25.02 host names a file after a cuDF that did
/// not record it, and the comparison then reports 25.02's answers as 26.02's.
#[test]
fn a_recorded_file_must_carry_the_cudf_its_name_promises() {
    let stamped = |cudf: &str| format!("cudf={cudf}\n== q1 mode=tp1-single\nan answer\n");
    assert_eq!(
        gpu_result_cudf_matches_path(&stamped("25.02"), None),
        Ok(())
    );
    assert_eq!(
        gpu_result_cudf_matches_path(&stamped("26.02"), Some("26.02")),
        Ok(())
    );
    let said = gpu_result_cudf_matches_path(&stamped("26.02"), None)
        .expect_err("26.02's answers in the committed file");
    assert!(said.contains("26.02") && said.contains("25.02"), "{said}");
    assert!(
        gpu_result_cudf_matches_path(&stamped("25.02"), Some("26.02")).is_err(),
        "a file named for a cuDF that did not record it"
    );
    let said = gpu_result_cudf_matches_path("== q1 mode=tp1-single\nan answer\n", None)
        .expect_err("no provenance line at all");
    assert!(said.contains("no `cudf=` line"), "{said}");
}

/// Two provenance lines name two cuDFs, and reading the first would let the second contradict
/// it unseen — the file is refused instead.
#[test]
#[should_panic(expected = "carries one")]
fn two_provenance_lines_are_refused_rather_than_read_to_the_first() {
    let _ = gpu_result_cudf_matches_path(
        "cudf=25.02\ncudf=26.02\n== q1 mode=tp1-single\nan answer\n",
        None,
    );
}

/// The same rule at the WRITER, so the cycle that would overwrite 25.02's committed answers
/// fails before it writes rather than after someone commits the result. This is what takes the
/// rule off the operator: the value they type is checked against the cuDF the binary is linked
/// against, and the remedy names the value that writes the file they meant.
#[test]
fn recording_the_committed_file_is_refused_from_another_cudf() {
    assert_eq!(recording_cudf_suits_the_path("25.02", None), Ok(()));
    assert_eq!(
        recording_cudf_suits_the_path("26.02", Some("26.02")),
        Ok(())
    );
    let said = recording_cudf_suits_the_path("26.02", None)
        .expect_err("a 26.02 cycle recording the committed file");
    assert!(said.contains("PCK_WRITE_GPU_RESULT=26.02"), "{said}");
    let said = recording_cudf_suits_the_path("25.02", Some("26.02"))
        .expect_err("a 25.02 cycle naming its file 26.02");
    assert!(said.contains("25.02") && said.contains("26.02"), "{said}");
}

/// The remedy a missing device answer prescribes must name the value that records the file the
/// case just read. `PCK_WRITE_GPU_RESULT=1` records the COMMITTED file, so printing it under
/// `PCK_GPU_RESULT_VERSION=26.02` told the reader to overwrite cuDF 25.02's answers.
#[test]
fn the_remedy_names_the_value_that_records_the_file_the_case_read() {
    assert_eq!(recording_knob(None), "PCK_WRITE_GPU_RESULT=1");
    assert_eq!(recording_knob(Some("26.02")), "PCK_WRITE_GPU_RESULT=26.02");
}
