#![no_std]
#![doc = include_str!("../README.md")]

mod refusal;
mod status;

pub use refusal::Refusal;
pub use status::Status;

/// Offsets per bitmap word: one bit each in a `u64`.
const WORD_BITS: u64 = 64;

/// Honoured outcome of [`Tracker::ack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ack {
    /// The offset was recorded.
    Recorded,
    /// Idempotent repeat: the offset was already acknowledged or is already
    /// below the window (committed). Nothing changed.
    Duplicate,
}

/// Honoured outcome of [`Tracker::mark_committed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commit {
    /// The window slid forward to the confirmed offset.
    Advanced,
    /// Idempotent repeat: the confirmed offset is not beyond the current
    /// window base, so it is already committed. Nothing changed.
    Duplicate,
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
    pub const CAPACITY: u64 = WORDS as u64 * WORD_BITS;

    /// Start unassigned: generation zero, so every acknowledgement and
    /// confirmation is refused as [`Refusal::Stale`].
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
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] when `generation` is not newer than the current one
    /// (zero is never newer).
    pub const fn assign(&mut self, generation: u64, base_offset: u64) -> Result<(), Refusal> {
        if generation <= self.generation {
            return Err(Refusal::Stale);
        }
        self.install(generation, base_offset);
        Ok(())
    }

    /// Start the window of a generation already known to be newer.
    const fn install(&mut self, generation: u64, base_offset: u64) {
        self.generation = generation;
        self.base = base_offset;
        self.bits = [0; WORDS];
    }

    /// Assign the next generation at `base_offset` and return it.
    ///
    /// # Errors
    ///
    /// [`Refusal::Exhausted`] when the current generation is `u64::MAX`.
    pub fn reassign(&mut self, base_offset: u64) -> Result<u64, Refusal> {
        let generation = self.generation.checked_add(1).ok_or(Refusal::Exhausted)?;
        // One past the current generation is newer by construction: no
        // `Stale` arm to carry.
        self.install(generation, base_offset);
        Ok(generation)
    }

    /// Record that `offset` finished processing under `generation`.
    ///
    /// An offset already acknowledged, or already below the window, is an
    /// idempotent repeat: [`Ack::Duplicate`].
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] when `generation` is not the current assignment
    /// generation (always, while unassigned); [`Refusal::OutOfWindow`] when
    /// `offset` is beyond the window or is `u64::MAX`.
    pub fn ack(&mut self, generation: u64, offset: u64) -> Result<Ack, Refusal> {
        self.check_current(generation)?;
        let Some(index) = offset.checked_sub(self.base) else {
            return Ok(Ack::Duplicate);
        };
        if offset == u64::MAX {
            return Err(Refusal::OutOfWindow);
        }
        // The window is exactly the words there are: no word, no window.
        let (word, mask) = Self::slot(index);
        let word = word
            .and_then(|word| self.bits.get_mut(word))
            .ok_or(Refusal::OutOfWindow)?;
        if *word & mask != 0 {
            return Ok(Ack::Duplicate);
        }
        *word |= mask;
        Ok(Ack::Recorded)
    }

    /// [`Refusal::Stale`] unless `generation` is the current, assigned one.
    const fn check_current(&self, generation: u64) -> Result<(), Refusal> {
        if generation != self.generation || generation == 0 {
            Err(Refusal::Stale)
        } else {
            Ok(())
        }
    }

    /// Word index (if it fits a `usize`) and bit mask of the slot `index`
    /// offsets above `base`; the caller checks the word exists.
    fn slot(index: u64) -> (Option<usize>, u64) {
        (
            usize::try_from(index / WORD_BITS).ok(),
            1 << (index % WORD_BITS),
        )
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
        let top = WORD_BITS - u64::from(word.leading_zeros());
        Some(self.base + index as u64 * WORD_BITS + top)
    }

    /// Number of acknowledged offsets in the window.
    #[must_use]
    pub fn acked(&self) -> u64 {
        self.bits
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum()
    }

    /// Whether `offset` is already acknowledged under the current generation:
    /// it is below `base` (covered by the committed base) or recorded in the
    /// window. Exactly the offsets for which [`Tracker::ack`] with the current
    /// generation answers [`Ack::Duplicate`].
    ///
    /// `false` for an offset in the window that is not recorded, for any
    /// offset beyond the window, for `u64::MAX`, and for every offset while
    /// unassigned (the base is zero and the window empty). A pure query: it
    /// takes no generation because it reads only the current one, and a newer
    /// [`Tracker::assign`] forgets what the previous generation acknowledged.
    #[must_use]
    pub fn is_acked(&self, offset: u64) -> bool {
        let Some(index) = offset.checked_sub(self.base) else {
            return true;
        };
        let (word, mask) = Self::slot(index);
        word.and_then(|word| self.bits.get(word))
            .is_some_and(|word| word & mask != 0)
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
    ///
    /// An offset not beyond the window base is already committed: an
    /// idempotent repeat, [`Commit::Duplicate`].
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] when `generation` is not the current assignment
    /// generation (always, while unassigned); [`Refusal::Unacked`] when
    /// `offset` is beyond the contiguous acknowledged prefix.
    pub fn mark_committed(&mut self, generation: u64, offset: u64) -> Result<Commit, Refusal> {
        self.check_current(generation)?;
        if offset <= self.base {
            return Ok(Commit::Duplicate);
        }
        if self
            .commit_candidate()
            .is_none_or(|candidate| offset > candidate)
        {
            return Err(Refusal::Unacked);
        }
        self.slide(offset - self.base);
        self.base = offset;
        Ok(Commit::Advanced)
    }

    fn slide(&mut self, shift: u64) {
        let old = self.bits;
        let (Ok(words), bits) = (usize::try_from(shift / WORD_BITS), shift % WORD_BITS) else {
            self.bits = [0; WORDS];
            return;
        };
        // A word past the old window reads as nothing acknowledged.
        let old_word = |i: usize, ahead: usize| {
            i.checked_add(ahead)
                .and_then(|j| old.get(j))
                .copied()
                .unwrap_or(0)
        };
        for (i, slot) in self.bits.iter_mut().enumerate() {
            let low = old_word(i, words);
            *slot = if bits == 0 {
                low
            } else {
                (low >> bits) | (old_word(i, words.saturating_add(1)) << (WORD_BITS - bits))
            };
        }
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
