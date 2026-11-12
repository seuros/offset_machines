use std::{collections::BTreeSet, vec::Vec};

use super::*;

type Small = Tracker<2>;

fn assigned(generation: u64, base: u64) -> Small {
    let mut tracker = Small::new();
    assert_eq!(tracker.assign(generation, base), Assign::Assigned);
    tracker
}

#[test]
fn unassigned_tracker_refuses_everything() {
    let mut tracker = Small::new();
    assert_eq!(tracker.ack(0, 0), Ack::Stale);
    assert_eq!(tracker.ack(1, 0), Ack::Stale);
    assert_eq!(tracker.mark_committed(0, 1), Commit::Stale);
    assert_eq!(tracker.assign(0, 5), Assign::Stale);
    assert_eq!(tracker.commit_candidate(), None);
    assert_eq!(Small::CAPACITY, 128);
}

#[test]
fn contiguous_prefix_and_gaps() {
    let mut tracker = assigned(1, 100);
    assert_eq!(tracker.commit_candidate(), None);
    for offset in [102, 100, 105] {
        assert_eq!(tracker.ack(1, offset), Ack::Ok);
    }
    assert_eq!(tracker.commit_candidate(), Some(101));
    assert_eq!(tracker.frontier(), Some(106));
    assert_eq!(tracker.acked(), 3);
    let gaps: Vec<u64> = tracker.gaps(0).collect();
    assert_eq!(gaps, [101, 103, 104]);
    let resumed: Vec<u64> = tracker.gaps(102).collect();
    assert_eq!(resumed, [103, 104]);
    assert_eq!(tracker.ack(1, 101), Ack::Ok);
    assert_eq!(tracker.commit_candidate(), Some(103));
}

#[test]
fn ack_refusals() {
    let mut tracker = assigned(2, 10);
    assert_eq!(tracker.ack(1, 10), Ack::Stale);
    assert_eq!(tracker.ack(3, 10), Ack::Stale);
    assert_eq!(tracker.ack(2, 9), Ack::Duplicate);
    assert_eq!(tracker.ack(2, 10), Ack::Ok);
    assert_eq!(tracker.ack(2, 10), Ack::Duplicate);
    assert_eq!(tracker.ack(2, 10 + 127), Ack::Ok);
    assert_eq!(tracker.ack(2, 10 + 128), Ack::OutOfWindow);
    assert_eq!(tracker.ack(2, u64::MAX), Ack::OutOfWindow);
    assert_eq!(tracker.acked(), 2);
}

#[test]
fn window_slides_only_on_confirmation() {
    let mut tracker = assigned(1, 0);
    for offset in 0..70 {
        assert_eq!(tracker.ack(1, offset), Ack::Ok);
    }
    assert_eq!(tracker.ack(1, 128), Ack::OutOfWindow);
    assert_eq!(tracker.commit_candidate(), Some(70));
    assert_eq!(tracker.base(), 0);
    assert_eq!(tracker.mark_committed(1, 71), Commit::Unacked);
    assert_eq!(tracker.mark_committed(2, 10), Commit::Stale);
    assert_eq!(tracker.mark_committed(1, 0), Commit::Duplicate);
    assert_eq!(tracker.mark_committed(1, 65), Commit::Advanced);
    assert_eq!(tracker.base(), 65);
    assert_eq!(tracker.commit_candidate(), Some(70));
    assert_eq!(tracker.ack(1, 128), Ack::Ok);
    assert_eq!(tracker.ack(1, 64), Ack::Duplicate);
    assert_eq!(tracker.mark_committed(1, 70), Commit::Advanced);
    assert_eq!(tracker.commit_candidate(), None);
    assert_eq!(tracker.frontier(), Some(129));
}

#[test]
fn full_window_commits_and_slides_clean() {
    let mut tracker = assigned(1, u64::MAX - 1000);
    let base = tracker.base();
    for offset in base..base + 128 {
        assert_eq!(tracker.ack(1, offset), Ack::Ok);
    }
    assert_eq!(tracker.commit_candidate(), Some(base + 128));
    assert_eq!(tracker.mark_committed(1, base + 128), Commit::Advanced);
    assert_eq!(tracker.acked(), 0);
    assert_eq!(tracker.frontier(), None);
}

#[test]
fn top_offset_is_never_ackable() {
    let mut tracker = assigned(1, u64::MAX - 1);
    assert_eq!(tracker.ack(1, u64::MAX - 1), Ack::Ok);
    assert_eq!(tracker.ack(1, u64::MAX), Ack::OutOfWindow);
    assert_eq!(tracker.commit_candidate(), Some(u64::MAX));
}

#[test]
fn reassign_discards_old_generation() {
    let mut tracker = assigned(1, 0);
    assert_eq!(tracker.ack(1, 0), Ack::Ok);
    assert_eq!(tracker.assign(1, 50), Assign::Stale);
    assert_eq!(tracker.reassign(40), Some(2));
    assert_eq!(tracker.base(), 40);
    assert_eq!(tracker.acked(), 0);
    assert_eq!(tracker.ack(1, 40), Ack::Stale);
    assert_eq!(tracker.ack(2, 40), Ack::Ok);
    let mut exhausted = assigned(u64::MAX, 0);
    assert_eq!(exhausted.reassign(0), None);
    assert_eq!(exhausted.generation(), u64::MAX);
}

#[test]
fn gap_iteration_is_resumable_in_bounded_chunks() {
    let mut tracker = assigned(1, 0);
    assert_eq!(tracker.ack(1, 127), Ack::Ok);
    let mut seen = Vec::new();
    let mut from = 0;
    loop {
        let chunk: Vec<u64> = tracker.gaps(from).take(10).collect();
        let Some(last) = chunk.last().copied() else {
            break;
        };
        from = last + 1;
        seen.extend(chunk);
    }
    assert_eq!(seen, (0..127).collect::<Vec<u64>>());
}

#[test]
fn refusals_leave_the_tracker_untouched() {
    let mut tracker = assigned(5, 1000);
    for offset in [1000, 1001, 1003] {
        assert_eq!(tracker.ack(5, offset), Ack::Ok);
    }
    let before = tracker.clone();
    assert_eq!(tracker.assign(5, 0), Assign::Stale);
    assert_eq!(tracker.assign(4, 0), Assign::Stale);
    assert_eq!(tracker.assign(0, 0), Assign::Stale);
    assert_eq!(tracker.ack(4, 1002), Ack::Stale);
    assert_eq!(tracker.ack(6, 1002), Ack::Stale);
    assert_eq!(tracker.ack(0, 1002), Ack::Stale);
    assert_eq!(tracker.ack(5, 1001), Ack::Duplicate);
    assert_eq!(tracker.ack(5, 0), Ack::Duplicate);
    assert_eq!(tracker.ack(5, 1000 + Small::CAPACITY), Ack::OutOfWindow);
    assert_eq!(tracker.ack(5, u64::MAX), Ack::OutOfWindow);
    assert_eq!(tracker.mark_committed(4, 1001), Commit::Stale);
    assert_eq!(tracker.mark_committed(6, 1001), Commit::Stale);
    assert_eq!(tracker.mark_committed(5, 1000), Commit::Duplicate);
    assert_eq!(tracker.mark_committed(5, 0), Commit::Duplicate);
    assert_eq!(tracker.mark_committed(5, 1003), Commit::Unacked);
    assert_eq!(tracker.mark_committed(5, u64::MAX), Commit::Unacked);
    assert_eq!(tracker, before);
    let mut spent = assigned(u64::MAX, 7);
    assert_eq!(spent.ack(u64::MAX, 7), Ack::Ok);
    let before = spent.clone();
    assert_eq!(spent.reassign(0), None);
    assert_eq!(spent, before);
}

struct Model {
    generation: u64,
    base: u64,
    acked: BTreeSet<u64>,
}

impl Model {
    fn candidate(&self) -> Option<u64> {
        let mut next = self.base;
        while self.acked.contains(&next) {
            next += 1;
        }
        (next > self.base).then_some(next)
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn matches_reference_model() {
    for seed in 1..40_u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut tracker = Small::new();
        let mut model = Model {
            generation: 0,
            base: 0,
            acked: BTreeSet::new(),
        };
        for _ in 0..3000 {
            match rng.next() % 10 {
                0 => {
                    let generation = rng.next() % 4 + model.generation.saturating_sub(1);
                    let base = rng.next() % 300;
                    let result = tracker.assign(generation, base);
                    if generation > model.generation {
                        assert_eq!(result, Assign::Assigned);
                        model = Model {
                            generation,
                            base,
                            acked: BTreeSet::new(),
                        };
                    } else {
                        assert_eq!(result, Assign::Stale);
                    }
                }
                1..=6 => {
                    let generation = model.generation + u64::from(rng.next().is_multiple_of(8));
                    let offset = model.base.saturating_sub(2) + rng.next() % 140;
                    let result = tracker.ack(generation, offset);
                    let expected = if generation != model.generation || generation == 0 {
                        Ack::Stale
                    } else if offset < model.base || model.acked.contains(&offset) {
                        Ack::Duplicate
                    } else if offset >= model.base + Small::CAPACITY {
                        Ack::OutOfWindow
                    } else {
                        model.acked.insert(offset);
                        Ack::Ok
                    };
                    assert_eq!(result, expected);
                }
                _ => {
                    let candidate = model.candidate();
                    let target = model.base + rng.next() % 40;
                    let result = tracker.mark_committed(model.generation, target);
                    let expected = if model.generation == 0 {
                        Commit::Stale
                    } else if target <= model.base {
                        Commit::Duplicate
                    } else if candidate.is_none_or(|c| target > c) {
                        Commit::Unacked
                    } else {
                        model.base = target;
                        model.acked.retain(|offset| *offset >= target);
                        Commit::Advanced
                    };
                    assert_eq!(result, expected);
                }
            }
            assert_eq!(tracker.base(), model.base);
            assert_eq!(tracker.commit_candidate(), model.candidate());
            assert_eq!(tracker.acked(), model.acked.len() as u64);
            let frontier = model.acked.last().map(|last| last + 1);
            assert_eq!(tracker.frontier(), frontier);
            let want: Vec<u64> = (model.base..frontier.unwrap_or(model.base))
                .filter(|offset| !model.acked.contains(offset))
                .collect();
            assert_eq!(tracker.gaps(0).collect::<Vec<u64>>(), want);
        }
    }
}
