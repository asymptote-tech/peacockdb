use super::*;

#[test]
fn merge_takes_every_lane_of_its_one_child() {
    assert_eq!(
        Forwarder::MergePartitions { n: 3 }.sources_of(0),
        vec![(0, 0), (0, 1), (0, 2)]
    );
}

#[test]
fn union_lanes_have_exactly_one_source_each() {
    let union = Forwarder::Union {
        lanes: vec![(0, 0), (0, 1), (1, 0)],
    };
    assert_eq!(union.sources_of(0), vec![(0, 0)]);
    assert_eq!(union.sources_of(2), vec![(1, 0)]);
}

#[test]
fn interleave_serves_lane_p_from_lane_p_of_every_child() {
    let interleave = Forwarder::Interleave { children: 3, n: 2 };
    assert_eq!(interleave.sources_of(1), vec![(0, 1), (1, 1), (2, 1)]);
}
