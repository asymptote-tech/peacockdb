//! The sf40 baseline as the fuser reads it: per query, per node in pre-order, from a
//! `.benchmark.txt` section, and held to the plans it is applied to.

use crate::prototypes::fuser::tests::{filter, lineitem, queries, unload};
use crate::prototypes::fuser::{Baseline, QueryTimes};
use crate::test_support::parse_benchmark_section;

const FILTERED: &str = "\
GpuUnload
  time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
  GpuFilter: predicate=x
    time_us=[[10,20]] total_us=30 batch_rows=[[4,6]] batch_bytes=[[32,48]]
    GpuLoadParquet: table=lineitem
      time_us=[[100]] total_us=100 batch_rows=[[40]] batch_bytes=[[320]]
--- run ---
load_us=100
execute_us=35
peak_bytes=400
";

fn baseline_of(texts: &[&str]) -> Baseline {
    let sections: Vec<_> = texts
        .iter()
        .map(|text| parse_benchmark_section(text).expect("a timed section"))
        .collect();
    Baseline::of(&sections)
}

#[test]
fn a_section_reads_back_by_pre_order() {
    let baseline = baseline_of(&[FILTERED]);
    let QueryTimes {
        names,
        children,
        total_us,
        batch_rows,
        batch_bytes,
        load_us,
        execute_us,
        peak_bytes,
    } = &baseline.per_query[0];
    assert_eq!(names, &["GpuUnload", "GpuFilter", "GpuLoadParquet"]);
    assert_eq!(children, &[vec![1], vec![2], vec![]]);
    assert_eq!(total_us, &[5, 30, 100]);
    assert_eq!(batch_rows, &[1, 10, 40]);
    assert_eq!(batch_bytes, &[17, 80, 320]);
    assert_eq!((*load_us, *execute_us, *peak_bytes), (100, 35, 400));
}

#[test]
#[should_panic(expected = "batch_rows")]
fn a_section_without_batch_rows_panics() {
    let without = FILTERED.replace(" batch_rows=[[4,6]]", "");
    baseline_of(&[&without]);
}

#[test]
#[should_panic(expected = "peak_bytes")]
fn a_section_without_peak_bytes_panics() {
    let without = FILTERED.replace("peak_bytes=400\n", "");
    baseline_of(&[&without]);
}

#[test]
fn a_baseline_of_its_own_plans_passes() {
    let trees = vec![unload(filter(lineitem(&[0], &[0])))];
    assert_eq!(baseline_of(&[FILTERED]).check(&queries(&trees)), Ok(()));
}

#[test]
fn a_baseline_of_another_plan_is_refused() {
    let trees = vec![
        unload(filter(lineitem(&[0], &[0]))),
        unload(lineitem(&[0], &[0])),
    ];
    let refused = baseline_of(&[FILTERED, FILTERED])
        .check(&queries(&trees))
        .expect_err("q1 has no filter");
    assert!(
        refused.contains("q1") && refused.contains("node 1"),
        "{refused}"
    );
}
