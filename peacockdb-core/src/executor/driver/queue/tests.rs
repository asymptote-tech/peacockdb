use super::*;
use crate::executor::driver::accounting::{Held, ResidentAccountant};
use crate::executor::driver::tests::mock::MockBatch;

fn batch(rows: usize) -> MockBatch {
    MockBatch {
        rows,
        bytes: rows * 8,
    }
}

/// `rows` pushed, each held once, as a producer's step pushes its output.
fn queue_of(readers: usize, rows: &[usize], acct: &mut ResidentAccountant) -> OutQueue<MockBatch> {
    let mut queue = OutQueue::new(readers);
    for rows in rows {
        let held = Held::of(batch(*rows));
        acct.hold(&held.charge);
        queue.push(held);
    }
    queue
}

#[test]
fn one_reader_is_a_fifo_with_no_share() {
    let mut acct = ResidentAccountant::new(0, None);
    let mut queue = queue_of(1, &[1, 2, 3], &mut acct);
    assert_eq!(queue.high_water(), 3);
    for rows in [1, 2, 3] {
        assert_eq!(queue.front_rows(0), Some(rows as u64));
        let taken = queue.take(0).expect("no share").expect("a batch");
        assert_eq!(taken.batch.rows, rows);
        assert_eq!(queue.len(), 3 - rows, "the only reader takes the original");
        acct.release(taken.charge).expect("held");
    }
    assert!(!queue.has(0));
    assert_eq!(acct.hops(), (3, 3));
    assert_eq!(acct.in_flight(), 0);
}

#[test]
fn a_non_last_reader_gets_a_share_and_the_last_the_original() {
    let mut acct = ResidentAccountant::new(0, None);
    let mut queue = queue_of(2, &[5], &mut acct);
    let share = queue.take(1).expect("a share").expect("a batch");
    assert_eq!(queue.len(), 1, "reader 0 has yet to take it");
    assert!(!queue.has(1) && queue.has(0));
    acct.release(share.charge).expect("a holder");
    assert_eq!(
        (acct.hops(), acct.in_flight()),
        ((1, 0), 40),
        "the original still holds it"
    );
    let original = queue.take(0).expect("no share").expect("a batch");
    assert_eq!(queue.len(), 0);
    acct.release(original.charge).expect("the last holder");
    assert_eq!((acct.hops(), acct.in_flight()), ((1, 1), 0));
}

#[test]
fn retiring_the_last_live_reader_hands_back_the_originals() {
    let mut acct = ResidentAccountant::new(0, None);
    let mut queue = queue_of(2, &[1, 2], &mut acct);
    let share = queue.take(0).expect("a share").expect("a batch");
    assert!(queue.retire(0).is_empty(), "reader 1 still wants both");
    let original = queue.take(1).expect("no share").expect("a batch");
    assert_eq!(original.batch.rows, 1);
    let left = queue.retire(1);
    assert_eq!(
        left.iter().map(|held| held.batch.rows).collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(queue.len(), 0);
    for held in [share, original].into_iter().chain(left) {
        acct.release(held.charge).expect("a holder");
    }
    assert_eq!(acct.hops(), (2, 2));
    assert_eq!(acct.in_flight(), 0);
}

#[test]
fn a_drain_gives_each_entry_once_with_its_readers_left() {
    let mut acct = ResidentAccountant::new(0, None);
    let mut queue = queue_of(2, &[1, 2], &mut acct);
    let share = queue.take(0).expect("a share").expect("a batch");
    let drained = queue.drain();
    assert_eq!(
        drained
            .iter()
            .map(|(held, left)| (held.batch.rows, *left))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 2)]
    );
    for held in std::iter::once(share).chain(drained.into_iter().map(|(held, _)| held)) {
        acct.release(held.charge).expect("a holder");
    }
    assert_eq!(acct.hops(), (2, 2));
}
