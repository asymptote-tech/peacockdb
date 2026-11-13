//! The benchmark's prefill without a device: which cases a libtest process runs, the file a
//! case plans, and what turns a case's section into a failed one.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::executor::{PinnedCache, PinnedCacheState, PrefillError, PrefillStats};
use crate::test_support::{BenchmarkCase, NOT_TIMED};

use super::{
    ProcessPrefill, case_query_file, prefill_reason, rounds_were_pinned, selected_benchmark_cases,
    unprefilled,
};

const fn case(dataset: &'static str, query: &'static str, mode: &'static str) -> BenchmarkCase {
    BenchmarkCase {
        dataset,
        sf: "40",
        query,
        mode,
    }
}

const CASES: [BenchmarkCase; 6] = [
    case("tpch", "q1", "tp1_single"),
    case("tpch", "q6", "tp1_single"),
    case("tpch", "q6", "tp4_sized"),
    case("tpcds", "q3", "tp1_single"),
    case("tpch", "aggregate_groupby", "tp1_single"),
    case("tpch", "q2", NOT_TIMED),
];

/// `dataset/query/mode` of each case the arguments select, in list order.
fn selected(args: &[&str]) -> Vec<String> {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    selected_benchmark_cases(&args, &CASES)
        .into_iter()
        .map(|case| format!("{}/{}/{}", case.dataset, case.query, case.mode))
        .collect()
}

#[test]
fn selection_follows_libtest_filters() {
    let timed = [
        "tpch/q1/tp1_single",
        "tpch/q6/tp1_single",
        "tpch/q6/tp4_sized",
        "tpcds/q3/tp1_single",
        "tpch/aggregate_groupby/tp1_single",
    ];
    assert_eq!(selected(&[]), timed, "no filter runs every timed case");
    assert_eq!(
        selected(&["bench_tpch_sf40_q6"]),
        ["tpch/q6/tp1_single", "tpch/q6/tp4_sized"]
    );
    assert_eq!(
        selected(&["--exact", "bench_tpch_sf40_q6_tp1_single"]),
        ["tpch/q6/tp1_single"]
    );
    assert_eq!(
        selected(&["bench_tpch", "--skip", "tp4_sized"]),
        [
            "tpch/q1/tp1_single",
            "tpch/q6/tp1_single",
            "tpch/aggregate_groupby/tp1_single"
        ]
    );
    assert_eq!(
        selected(&["--exact", "--skip", "tp4_sized"]),
        timed,
        "under --exact a skip is an equality too, and no name equals `tp4_sized`"
    );
    assert_eq!(
        selected(&["--test-threads", "1", "--nocapture", "q1"]),
        ["tpch/q1/tp1_single"],
        "`1` is --test-threads' value, not a filter"
    );
    assert_eq!(
        selected(&["--test-threads=1", "q1"]),
        ["tpch/q1/tp1_single"]
    );
    assert_eq!(
        selected(&["aggregate_groupby"]),
        ["tpch/aggregate_groupby/tp1_single"]
    );
    assert!(
        selected(&["q2"]).is_empty(),
        "a case timed at no mode never runs"
    );
}

#[test]
fn a_micro_query_plans_by_its_file_name() {
    assert_eq!(case_query_file(&CASES[4]), "aggregate-groupby");
    assert_eq!(case_query_file(&CASES[0]), "q1");
}

#[test]
fn rounds_were_pinned_names_what_moved() {
    let before = PinnedCache {
        state: PinnedCacheState::On,
        fills: 8,
        ..Default::default()
    };
    assert_eq!(rounds_were_pinned(&before, &before), Ok(()));

    let after = PinnedCache {
        fills: 9,
        fallbacks: 1,
        ..before
    };
    assert_eq!(
        rounds_were_pinned(&before, &after),
        Err(
            "pinned cache: 1 fills, 1 fallbacks, 0 reloads, 0 evictions during the rounds"
                .to_string()
        )
    );
}

#[test]
fn a_refused_prefill_fails_every_case_with_its_size() {
    let refused = Err(PrefillError::DoesNotFit {
        needed: 60,
        limit: 49,
    });
    assert_eq!(
        prefill_reason(&refused),
        Some("prefill: needs 60 > limit 49".to_string())
    );
    let unreadable = Err(PrefillError::Io {
        path: PathBuf::from("/data/lineitem.parquet"),
        error: "No such file or directory".to_string(),
    });
    assert_eq!(
        prefill_reason(&unreadable),
        Some("prefill: /data/lineitem.parquet: No such file or directory".to_string())
    );
    assert_eq!(
        prefill_reason(&Err(PrefillError::Off)),
        None,
        "an off cache measures from the path on purpose"
    );
    let done = Ok(ProcessPrefill {
        stats: PrefillStats::default(),
        files: BTreeSet::new(),
    });
    assert_eq!(prefill_reason(&done), None);
}

#[test]
fn a_case_outside_the_prefill_is_refused() {
    let [a, b, c] = ["a.parquet", "b.parquet", "c.parquet"].map(PathBuf::from);
    assert_eq!(
        unprefilled(
            &BTreeSet::from([a.clone(), b.clone()]),
            &BTreeSet::from([a.clone(), c])
        ),
        Some(b.clone())
    );
    assert_eq!(
        unprefilled(&BTreeSet::from([a.clone()]), &BTreeSet::from([a, b])),
        None
    );
}
