#![no_std]
#![doc = include_str!("../README.md")]

/// Outcome of [`Tracker::assign`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Assign {
    /// The assignment was installed; the window restarts at the given base.
    Assigned,
    /// The generation is not newer than the current one (zero is never newer).
    Stale,
}

/// Outcome of [`Tracker::ack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ack {
    /// The offset was recorded.
    Ok,
    /// The offset was already acknowledged or is already below the window.
    Duplicate,
    /// The generation is not the current assignment generation.
    Stale,
    /// The offset is beyond the window (or is `u64::MAX`); nothing was recorded.
    OutOfWindow,
}

/// Outcome of [`Tracker::mark_committed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Commit {
    /// The window slid forward to the confirmed offset.
    Advanced,
    /// The confirmed offset is not beyond the current window base.
    Duplicate,
    /// The generation is not the current assignment generation.
    Stale,
    /// The confirmed offset is beyond the contiguous acknowledged prefix.
    Unacked,
}

/// Per-partition acknowledgement window of `WORDS * 64` offsets.
///
/// The window covers `base .. base + CAPACITY`. Acknowledgements set bits;
/// the window only slides when the adapter confirms persistence through
/// [`Tracker::mark_committed`]. The tracker owns no execution, storage or
/// commit protocol: it never allocates and never decides on its own.
///
/// Offset `u64::MAX` is never acknowledgeable, so every commit candidate is
/// representable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracker<const WORDS: usize> {
    generation: u64,
    base: u64,
    bits: [u64; WORDS],
}

impl<const WORDS: usize> Default for Tracker<WORDS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WORDS: usize> Tracker<WORDS> {
    /// Number of offsets in the window.
    pub const CAPACITY: u64 = WORDS as u64 * 64;

    /// Start unassigned: generation zero, so every acknowledgement is stale.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            generation: 0,
            base: 0,
            bits: [0; WORDS],
        }
    }

    /// Current assignment generation; zero while unassigned.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Lowest offset not yet confirmed committed; the window starts here.
    #[must_use]
    pub const fn base(&self) -> u64 {
        self.base
    }

    /// Install a newer assignment generation and restart the window at
    /// `base_offset`, discarding acknowledgements of the previous generation.
    #[must_use]
    pub const fn assign(&mut self, generation: u64, base_offset: u64) -> Assign {
        if generation <= self.generation {
            return Assign::Stale;
        }
        self.generation = generation;
        self.base = base_offset;
        self.bits = [0; WORDS];
        Assign::Assigned
    }

    /// Assign the next generation at `base_offset`; `None` when the
    /// generation space is exhausted.
    pub fn reassign(&mut self, base_offset: u64) -> Option<u64> {
        let generation = self.generation.checked_add(1)?;
        (self.assign(generation, base_offset) == Assign::Assigned).then_some(generation)
    }

    /// Record that `offset` finished processing under `generation`.
    #[must_use]
    pub fn ack(&mut self, generation: u64, offset: u64) -> Ack {
        if generation != self.generation || generation == 0 {
            return Ack::Stale;
        }
        let Some(index) = offset.checked_sub(self.base) else {
            return Ack::Duplicate;
        };
        if index >= Self::CAPACITY || offset == u64::MAX {
            return Ack::OutOfWindow;
        }
        let Some(word) = usize::try_from(index / 64)
            .ok()
            .and_then(|word| self.bits.get_mut(word))
        else {
            return Ack::OutOfWindow;
        };
        let mask = 1_u64 << (index % 64);
        if *word & mask != 0 {
            return Ack::Duplicate;
        }
        *word |= mask;
        Ack::Ok
    }

    /// Length of the contiguous acknowledged prefix from `base`.
    fn prefix(&self) -> u64 {
        let mut run = 0_u64;
        for word in &self.bits {
            run += u64::from(word.trailing_ones());
            if *word != u64::MAX {
                break;
            }
        }
        run
    }

    /// The next offset to commit: one past the contiguous acknowledged prefix.
    /// `None` when `base` itself is not acknowledged.
    #[must_use]
    pub fn commit_candidate(&self) -> Option<u64> {
        let run = self.prefix();
        (run > 0).then(|| self.base + run)
    }

    /// One past the highest acknowledged offset, or `None` when nothing is
    /// acknowledged. Offsets below it that are not acknowledged are gaps.
    #[must_use]
    pub fn frontier(&self) -> Option<u64> {
        let (index, word) = self
            .bits
            .iter()
            .enumerate()
            .rev()
            .find(|(_, word)| **word != 0)?;
        let top = 64 - u64::from(word.leading_zeros());
        Some(self.base + index as u64 * 64 + top)
    }

    /// Number of acknowledged offsets in the window.
    #[must_use]
    pub fn acked(&self) -> u64 {
        self.bits
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum()
    }

    /// Unacknowledged offsets below the frontier, ascending, starting at
    /// `from` (clamped up to `base`). Resume by passing last yielded + 1.
    #[must_use]
    pub fn gaps(&self, from: u64) -> Gaps<'_, WORDS> {
        Gaps {
            tracker: self,
            next: from.max(self.base),
            end: self.frontier().unwrap_or(self.base),
        }
    }

    /// Adapter confirms offsets below `offset` are durably committed under
    /// `generation`; the window slides to `offset`. Partial confirmation up
    /// to the commit candidate is allowed.
    #[must_use]
    pub fn mark_committed(&mut self, generation: u64, offset: u64) -> Commit {
        if generation != self.generation || generation == 0 {
            return Commit::Stale;
        }
        if offset <= self.base {
            return Commit::Duplicate;
        }
        if self
            .commit_candidate()
            .is_none_or(|candidate| offset > candidate)
        {
            return Commit::Unacked;
        }
        self.slide(offset - self.base);
        self.base = offset;
        Commit::Advanced
    }

    fn slide(&mut self, shift: u64) {
        let old = self.bits;
        let (Ok(words), bits) = (usize::try_from(shift / 64), shift % 64) else {
            self.bits = [0; WORDS];
            return;
        };
        for (i, slot) in self.bits.iter_mut().enumerate() {
            let low = i
                .checked_add(words)
                .and_then(|j| old.get(j))
                .copied()
                .unwrap_or(0);
            let high = i
                .checked_add(words + 1)
                .and_then(|j| old.get(j))
                .copied()
                .unwrap_or(0);
            *slot = if bits == 0 {
                low
            } else {
                (low >> bits) | (high << (64 - bits))
            };
        }
    }

    fn is_acked(&self, offset: u64) -> bool {
        let Some(index) = offset.checked_sub(self.base) else {
            return true;
        };
        usize::try_from(index / 64)
            .ok()
            .and_then(|word| self.bits.get(word))
            .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }
}

/// Resumable iterator over unacknowledged offsets below the frontier.
#[derive(Debug, Clone)]
pub struct Gaps<'a, const WORDS: usize> {
    tracker: &'a Tracker<WORDS>,
    next: u64,
    end: u64,
}

impl<const WORDS: usize> Iterator for Gaps<'_, WORDS> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        while self.next < self.end {
            let offset = self.next;
            self.next += 1;
            if !self.tracker.is_acked(offset) {
                return Some(offset);
            }
        }
        None
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
