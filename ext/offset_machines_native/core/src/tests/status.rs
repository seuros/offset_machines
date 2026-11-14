use std::collections::BTreeSet;

use crate::{Refusal, Status, Tracker};

type Parts = (u64, u64, Option<u64>, Option<u64>, u64);

fn parts(status: &Status) -> Parts {
    (
        status.generation(),
        status.base(),
        status.commit_candidate(),
        status.frontier(),
        status.acked(),
    )
}

fn build<const WORDS: usize>(parts: Parts) -> Option<Status> {
    Status::new::<WORDS>(parts.0, parts.1, parts.2, parts.3, parts.4)
}

#[test]
fn a_status_is_exactly_what_some_tracker_reports() {
    const BASE: u64 = 5;
    const SPAN: u64 = 9;
    let mut reported = BTreeSet::new();
    for pattern in 0_u64..1 << SPAN {
        let mut tracker = Tracker::<1>::new();
        assert_eq!(tracker.assign(1, BASE), Ok(()));
        for bit in (0..SPAN).filter(|bit| pattern >> bit & 1 == 1) {
            assert!(tracker.ack(1, BASE + bit).is_ok());
        }
        let status = tracker.status();
        assert_eq!(build::<1>(parts(&status)), Some(status));
        reported.insert(parts(&status));
    }
    let marks = || core::iter::once(None).chain((0..=BASE + SPAN + 1).map(Some));
    for candidate in marks() {
        for frontier in marks().filter(|mark| mark.is_none_or(|mark| mark <= BASE + SPAN)) {
            for acked in 0..=SPAN + 1 {
                let wanted = (1, BASE, candidate, frontier, acked);
                assert_eq!(
                    build::<1>(wanted).is_some(),
                    reported.contains(&wanted),
                    "{wanted:?}"
                );
            }
        }
    }
}

#[test]
fn a_status_respects_the_window_and_the_unassigned_state() {
    let fresh = Tracker::<1>::new().status();
    assert_eq!(parts(&fresh), (0, 0, None, None, 0));
    assert_eq!(build::<1>(parts(&fresh)), Some(fresh));
    assert_eq!(build::<1>((0, 1, None, None, 0)), None);
    assert_eq!(build::<1>((0, 0, Some(1), Some(1), 1)), None);
    assert!(build::<1>((1, 0, Some(64), Some(64), 64)).is_some());
    assert_eq!(build::<1>((1, 0, Some(65), Some(65), 65)), None);
    assert!(build::<2>((1, 0, Some(65), Some(65), 65)).is_some());
    assert_eq!(build::<1>((1, 10, Some(10), Some(11), 1)), None);
    assert_eq!(build::<1>((1, 10, None, Some(9), 1)), None);
    let top = u64::MAX - 1;
    let mut tracker = Tracker::<1>::new();
    assert_eq!(tracker.assign(7, top), Ok(()));
    assert!(tracker.ack(7, top).is_ok());
    assert_eq!(tracker.ack(7, u64::MAX), Err(Refusal::OutOfWindow));
    let status = tracker.status();
    assert_eq!(parts(&status), (7, top, Some(u64::MAX), Some(u64::MAX), 1));
    assert_eq!(build::<1>(parts(&status)), Some(status));
}
