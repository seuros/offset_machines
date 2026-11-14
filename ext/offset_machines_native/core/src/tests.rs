use std::{collections::BTreeSet, vec::Vec};

use super::*;
use matryona_machines::testing::{XorShift64, refuses, unchanged};

mod status;

type Small = Tracker<2>;

fn assigned(generation: u64, base: u64) -> Small {
    let mut tracker = Small::new();
    assert_eq!(tracker.assign(generation, base), Ok(()));
    tracker
}

/// Record every offset in `offsets` under `generation`, each for the first time.
fn ack_all(tracker: &mut Small, generation: u64, offsets: impl IntoIterator<Item = u64>) {
    for offset in offsets {
        assert_eq!(
            tracker.ack(generation, offset),
            Ok(Ack::Recorded),
            "{offset}"
        );
    }
}

/// Generation 5 at base 1000 with 1000, 1001 and 1003 acknowledged: a
/// candidate of 1002 and one gap.
fn with_gap() -> Small {
    let mut tracker = assigned(5, 1000);
    ack_all(&mut tracker, 5, [1000, 1001, 1003]);
    tracker
}

#[test]
fn is_acked_reads_the_base_and_the_window() {
    let unassigned = Small::new();
    assert!(!unassigned.is_acked(0) && !unassigned.is_acked(u64::MAX));
    let mut tracker = assigned(3, 100);
    assert!(tracker.is_acked(0) && tracker.is_acked(99));
    assert!(!tracker.is_acked(100) && !tracker.is_acked(102));
    assert_eq!(tracker.ack(3, 102), Ok(Ack::Recorded));
    assert!(tracker.is_acked(102) && !tracker.is_acked(101));
    // Beyond the window nothing is recorded, and the top offset never is.
    assert!(!tracker.is_acked(100 + Small::CAPACITY) && !tracker.is_acked(u64::MAX));
    assert_eq!(tracker.ack(3, 100), Ok(Ack::Recorded));
    assert_eq!(tracker.mark_committed(3, 101), Ok(Commit::Advanced));
    assert!(tracker.is_acked(100) && tracker.is_acked(102) && !tracker.is_acked(101));
    // A newer generation forgets the window and answers from its own base.
    assert_eq!(tracker.reassign(50), Ok(4));
    assert!(tracker.is_acked(49) && !tracker.is_acked(50) && !tracker.is_acked(102));
}

#[test]
fn contiguous_prefix_and_gaps() {
    let mut tracker = assigned(1, 100);
    assert_eq!(tracker.commit_candidate(), None);
    ack_all(&mut tracker, 1, [102, 100, 105]);
    assert_eq!(tracker.commit_candidate(), Some(101));
    assert_eq!(tracker.frontier(), Some(106));
    assert_eq!(tracker.acked(), 3);
    let gaps: Vec<u64> = tracker.gaps(0).collect();
    assert_eq!(gaps, [101, 103, 104]);
    let resumed: Vec<u64> = tracker.gaps(102).collect();
    assert_eq!(resumed, [103, 104]);
    assert_eq!(tracker.ack(1, 101), Ok(Ack::Recorded));
    assert_eq!(tracker.commit_candidate(), Some(103));
}

#[test]
fn ack_refusals() {
    let mut tracker = assigned(2, 10);
    assert_eq!(tracker.ack(1, 10), Err(Refusal::Stale));
    assert_eq!(tracker.ack(3, 10), Err(Refusal::Stale));
    assert_eq!(tracker.ack(2, 9), Ok(Ack::Duplicate));
    assert_eq!(tracker.ack(2, 10), Ok(Ack::Recorded));
    assert_eq!(tracker.ack(2, 10), Ok(Ack::Duplicate));
    assert_eq!(tracker.ack(2, 10 + 127), Ok(Ack::Recorded));
    assert_eq!(tracker.ack(2, 10 + 128), Err(Refusal::OutOfWindow));
    assert_eq!(tracker.ack(2, u64::MAX), Err(Refusal::OutOfWindow));
    assert_eq!(tracker.acked(), 2);
}

#[test]
fn window_slides_only_on_confirmation() {
    let mut tracker = assigned(1, 0);
    ack_all(&mut tracker, 1, 0..70);
    assert_eq!(tracker.ack(1, 128), Err(Refusal::OutOfWindow));
    assert_eq!(tracker.commit_candidate(), Some(70));
    assert_eq!(tracker.base(), 0);
    assert_eq!(tracker.mark_committed(1, 71), Err(Refusal::Unacked));
    assert_eq!(tracker.mark_committed(2, 10), Err(Refusal::Stale));
    assert_eq!(tracker.mark_committed(1, 0), Ok(Commit::Duplicate));
    assert_eq!(tracker.mark_committed(1, 65), Ok(Commit::Advanced));
    assert_eq!(tracker.base(), 65);
    assert_eq!(tracker.commit_candidate(), Some(70));
    assert_eq!(tracker.ack(1, 128), Ok(Ack::Recorded));
    assert_eq!(tracker.ack(1, 64), Ok(Ack::Duplicate));
    assert_eq!(tracker.mark_committed(1, 70), Ok(Commit::Advanced));
    assert_eq!(tracker.commit_candidate(), None);
    assert_eq!(tracker.frontier(), Some(129));
}

#[test]
fn full_window_commits_and_slides_clean() {
    let mut tracker = assigned(1, u64::MAX - 1000);
    let base = tracker.base();
    ack_all(&mut tracker, 1, base..base + Small::CAPACITY);
    assert_eq!(tracker.commit_candidate(), Some(base + 128));
    assert_eq!(tracker.mark_committed(1, base + 128), Ok(Commit::Advanced));
    assert_eq!(tracker.acked(), 0);
    assert_eq!(tracker.frontier(), None);
}

#[test]
fn reassign_discards_old_generation() {
    let mut tracker = assigned(1, 0);
    assert_eq!(tracker.ack(1, 0), Ok(Ack::Recorded));
    assert_eq!(tracker.assign(1, 50), Err(Refusal::Stale));
    assert_eq!(tracker.reassign(40), Ok(2));
    assert_eq!(tracker.base(), 40);
    assert_eq!(tracker.acked(), 0);
    assert_eq!(tracker.ack(1, 40), Err(Refusal::Stale));
    assert_eq!(tracker.ack(2, 40), Ok(Ack::Recorded));
    let mut exhausted = assigned(u64::MAX, 0);
    assert_eq!(exhausted.reassign(0), Err(Refusal::Exhausted));
    assert_eq!(exhausted.generation(), u64::MAX);
}

#[test]
fn gap_iteration_is_resumable_in_bounded_chunks() {
    let mut tracker = assigned(1, 0);
    assert_eq!(tracker.ack(1, 127), Ok(Ack::Recorded));
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
    let mut tracker = with_gap();
    let t = &mut tracker;
    for generation in [5, 4, 0] {
        refuses(t, Refusal::Stale, |t| t.assign(generation, 0));
    }
    for generation in [4, 6, 0] {
        refuses(t, Refusal::Stale, |t| t.ack(generation, 1002));
        refuses(t, Refusal::Stale, |t| t.mark_committed(generation, 1001));
    }
    for offset in [1000 + Small::CAPACITY, u64::MAX] {
        refuses(t, Refusal::OutOfWindow, |t| t.ack(5, offset));
        refuses(t, Refusal::Unacked, |t| t.mark_committed(5, offset));
    }
    refuses(t, Refusal::Unacked, |t| t.mark_committed(5, 1003));
    let mut spent = assigned(u64::MAX, 7);
    assert_eq!(spent.ack(u64::MAX, 7), Ok(Ack::Recorded));
    refuses(&mut spent, Refusal::Exhausted, |t| t.reassign(0));
    refuses(&mut spent, Refusal::Stale, |t| t.assign(u64::MAX, 0));
    // Unassigned, every generation is stale, zero included.
    let mut fresh = Small::new();
    for generation in [0, 1] {
        refuses(&mut fresh, Refusal::Stale, |t| t.ack(generation, 0));
        refuses(&mut fresh, Refusal::Stale, |t| {
            t.mark_committed(generation, 1)
        });
    }
    refuses(&mut fresh, Refusal::Stale, |t| t.assign(0, 5));
}

#[test]
fn idempotent_repeats_are_honoured_and_change_nothing() {
    let mut tracker = with_gap();
    let t = &mut tracker;
    for offset in [1001, 1003, 999, 0] {
        assert_eq!(unchanged(t, |t| t.ack(5, offset)), Ok(Ack::Duplicate));
    }
    for offset in [1000, 999, 0] {
        assert_eq!(
            unchanged(t, |t| t.mark_committed(5, offset)),
            Ok(Commit::Duplicate)
        );
    }
    assert_eq!(t.mark_committed(5, 1002), Ok(Commit::Advanced));
    assert_eq!(
        unchanged(t, |t| t.mark_committed(5, 1002)),
        Ok(Commit::Duplicate)
    );
    assert_eq!(unchanged(t, |t| t.ack(5, 1001)), Ok(Ack::Duplicate));
}

#[test]
fn refusal_is_an_error_with_a_message() {
    use std::string::ToString;
    let all = [
        Refusal::Stale,
        Refusal::OutOfWindow,
        Refusal::Unacked,
        Refusal::Exhausted,
    ];
    let messages: BTreeSet<_> = all.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), all.len());
    let error: &dyn core::error::Error = &Refusal::Stale;
    assert!(error.source().is_none());
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

impl Model {
    fn new(generation: u64, base: u64) -> Self {
        Self {
            generation,
            base,
            acked: BTreeSet::new(),
        }
    }

    fn assign(&mut self, generation: u64, base: u64) -> Result<(), Refusal> {
        if generation <= self.generation {
            return Err(Refusal::Stale);
        }
        *self = Self::new(generation, base);
        Ok(())
    }

    fn ack(&mut self, generation: u64, offset: u64) -> Result<Ack, Refusal> {
        if generation != self.generation || generation == 0 {
            Err(Refusal::Stale)
        } else if offset < self.base || self.acked.contains(&offset) {
            Ok(Ack::Duplicate)
        } else if offset - self.base >= Small::CAPACITY {
            Err(Refusal::OutOfWindow)
        } else {
            self.acked.insert(offset);
            Ok(Ack::Recorded)
        }
    }

    fn mark_committed(&mut self, generation: u64, target: u64) -> Result<Commit, Refusal> {
        if generation != self.generation || generation == 0 {
            Err(Refusal::Stale)
        } else if target <= self.base {
            Ok(Commit::Duplicate)
        } else if self.candidate().is_none_or(|c| target > c) {
            Err(Refusal::Unacked)
        } else {
            self.base = target;
            self.acked.retain(|offset| *offset >= target);
            Ok(Commit::Advanced)
        }
    }
}

/// One random operation applied to both; returns whether it was honoured
/// with an effect (anything else must leave the tracker untouched).
fn step(rng: &mut XorShift64, tracker: &mut Small, model: &mut Model) -> bool {
    let off_generation = model.generation.saturating_add(rng.below(2));
    let generation = if rng.next_u64().is_multiple_of(8) {
        off_generation
    } else {
        model.generation
    };
    match rng.below(12) {
        0 => {
            let generation = rng
                .below(4)
                .saturating_add(model.generation.saturating_sub(1));
            let base = rng.below(300);
            let result = tracker.assign(generation, base);
            assert_eq!(result, model.assign(generation, base));
            result.is_ok()
        }
        1 => {
            let base = rng.below(300);
            let result = tracker.reassign(base);
            let expected = model
                .generation
                .checked_add(1)
                .ok_or(Refusal::Exhausted)
                .and_then(|next| model.assign(next, base).map(|()| next));
            assert_eq!(result, expected);
            result.is_ok()
        }
        2..=7 => {
            let offset = model.base.saturating_sub(2) + rng.below(140);
            let result = tracker.ack(generation, offset);
            assert_eq!(result, model.ack(generation, offset));
            result == Ok(Ack::Recorded)
        }
        _ => {
            let target = model.base.saturating_sub(1) + rng.below(40);
            let result = tracker.mark_committed(generation, target);
            assert_eq!(result, model.mark_committed(generation, target));
            result == Ok(Commit::Advanced)
        }
    }
}

#[test]
fn matches_reference_model() {
    for seed in 1..40_u64 {
        let mut rng = XorShift64::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut tracker = Small::new();
        let mut model = Model::new(0, 0);
        if seed.is_multiple_of(8) {
            // Start near the top so the generation space runs out.
            assert_eq!(tracker.assign(u64::MAX - 2, 0), Ok(()));
            assert_eq!(model.assign(u64::MAX - 2, 0), Ok(()));
        }
        let (mut refused, mut repeated) = (0_u32, 0_u32);
        for _ in 0..3000 {
            let before = tracker.clone();
            let generation = model.generation;
            if step(&mut rng, &mut tracker, &mut model) {
                assert_ne!(tracker, before);
            } else {
                assert_eq!(tracker, before);
                refused += 1;
            }
            repeated += u32::from(model.generation == generation);
            assert_eq!(tracker.generation(), model.generation);
            assert_eq!(tracker.base(), model.base);
            assert_eq!(tracker.commit_candidate(), model.candidate());
            assert_eq!(tracker.acked(), model.acked.len() as u64);
            let frontier = model.acked.last().map(|last| last + 1);
            assert_eq!(tracker.frontier(), frontier);
            let want: Vec<u64> = (model.base..frontier.unwrap_or(model.base))
                .filter(|offset| !model.acked.contains(offset))
                .collect();
            assert_eq!(tracker.gaps(0).collect::<Vec<u64>>(), want);
            let probe = model.base.saturating_sub(2) + rng.below(140);
            let repeat = tracker.clone().ack(model.generation, probe) == Ok(Ack::Duplicate);
            assert_eq!(tracker.is_acked(probe), repeat);
        }
        assert!(refused > 100 && repeated > 100);
    }
}
