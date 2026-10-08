//! The harness's own facts: the dataset list every other list reads, and the one knob a
//! dataset moves.

use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};

use super::*;

/// pbench's tables are all under the small-table threshold, so the rule that stops splitting
/// a small source would plan every pbench scan as one lane — and pbench exists to show
/// multi-lane shapes. The override is per dataset and reaches pbench alone: planned with it,
/// tpch and tpcds would move every one of their plan goldens.
#[test]
fn pbench_alone_plans_with_the_small_table_rule_off() {
    for mode in &MODES {
        assert_eq!(
            mode.knobs_for("pbench").small_table_bytes,
            0,
            "{}",
            mode.name
        );
        for other in ["tpch", "tpcds"] {
            assert_eq!(
                mode.knobs_for(other),
                mode.knobs(),
                "{other} at {}",
                mode.name
            );
        }
    }
}

/// The one list. Every dataset loop in the plan goldens, the corpus golden checks and the
/// cost-model test reads this rather than spelling the names again, so a fourth dataset is
/// one edit instead of a dozen a reader has to find.
#[test]
fn the_corpus_datasets_are_the_three() {
    let names: Vec<&str> = CORPUS_DATASETS
        .iter()
        .map(|(dataset, _)| *dataset)
        .collect();
    assert_eq!(names, ["tpch", "tpcds", "pbench"]);
}

/// A dataset in the list has the three directories every tier derives from its name. The
/// loops that read `CORPUS_DATASETS` fail far from here when one is absent — a missing
/// queries directory surfaces as "the query directory" inside a golden renderer.
#[test]
fn every_corpus_dataset_has_its_data_queries_and_goldens() {
    for (dataset, sf) in CORPUS_DATASETS {
        assert!(data_dir_for(dataset, sf).is_dir(), "{dataset}: data");
        assert!(queries_dir_for(dataset).is_dir(), "{dataset}: queries");
        assert!(golden_dir_for(dataset, sf).is_dir(), "{dataset}: goldens");
    }
}

/// `dim` carries a key column of every type `fact` does, which is what lets a join key be
/// written on any of them. Read off the committed parquet: the data is the deliverable, so a
/// regeneration that drops a column has to go red here rather than at the query somebody
/// writes a year later. `d_ts_ms` and `d_ts_ns` were absent for exactly that reason.
#[test]
fn pbenchs_dim_carries_a_key_column_of_every_type_fact_does() {
    let keys = |table: &str, prefix: &str| -> BTreeSet<String> {
        let path = data_dir_for("pbench", "1").join(format!("{table}.parquet"));
        let file = std::fs::File::open(&path).expect("the committed parquet");
        let reader = SerializedFileReader::new(file).expect("a parquet footer");
        reader
            .metadata()
            .file_metadata()
            .schema_descr()
            .columns()
            .iter()
            .filter_map(|column| {
                column
                    .path()
                    .string()
                    .split('.')
                    .next()
                    .expect("a leading component")
                    .strip_prefix(prefix)
                    .filter(|name| name.starts_with('k') || name.starts_with("ts_"))
                    .map(str::to_string)
            })
            .collect()
    };
    let fact = keys("fact", "f_");
    let dim = keys("dim", "d_");
    assert_eq!(
        fact,
        dim,
        "pbench's fact and dim must carry the same key types: fact has {:?} that dim does \
         not, dim has {:?} that fact does not",
        fact.difference(&dim).collect::<Vec<_>>(),
        dim.difference(&fact).collect::<Vec<_>>()
    );
}
