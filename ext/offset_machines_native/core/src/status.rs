//! Point-in-time view of a tracker, valid by construction.
use crate::Tracker;

/// What a [`Tracker`] reports about itself at one instant: the answers of its
/// five readers together.
///
/// A `Status` exists only when some tracker could report it. [`Tracker::status`]
/// builds one from a live tracker; [`Status::new`] checks five loose numbers
/// against a window and answers `None` for a combination no tracker reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    generation: u64,
    base: u64,
    commit_candidate: Option<u64>,
    frontier: Option<u64>,
    acked: u64,
}

impl Status {
    /// A view a `Tracker<WORDS>` could report, or `None`.
    ///
    /// The rules, with `run = commit_candidate - base` and
    /// `span = frontier - base` (zero when absent):
    ///
    /// - unassigned (`generation == 0`): `base` is zero and nothing is
    ///   acknowledged;
    /// - `acked == 0` exactly when there is no frontier, and then there is no
    ///   commit candidate either;
    /// - a frontier lies in `base + 1 ..= base + CAPACITY`, a commit candidate
    ///   in `base + 1 ..= frontier`;
    /// - `run == span` means no gaps, so `acked == span`; otherwise the offset
    ///   at `base + run` is a gap and the one below the frontier is
    ///   acknowledged, so `span >= run + 2` and `run < acked < span`.
    #[must_use]
    pub const fn new<const WORDS: usize>(
        generation: u64,
        base: u64,
        commit_candidate: Option<u64>,
        frontier: Option<u64>,
        acked: u64,
    ) -> Option<Self> {
        let status = Self {
            generation,
            base,
            commit_candidate,
            frontier,
            acked,
        };
        let Some(span) = distance(base, frontier) else {
            return None;
        };
        let Some(run) = distance(base, commit_candidate) else {
            return None;
        };
        let assigned = generation != 0 || (base == 0 && acked == 0);
        let counted = if run == span {
            acked == span
        } else {
            run < acked && acked < span
        };
        if assigned && span <= Tracker::<WORDS>::CAPACITY && run <= span && counted {
            Some(status)
        } else {
            None
        }
    }

    /// Assignment generation; zero while unassigned.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Lowest offset not yet confirmed committed.
    #[must_use]
    pub const fn base(&self) -> u64 {
        self.base
    }

    /// One past the contiguous acknowledged prefix, if `base` is acknowledged.
    #[must_use]
    pub const fn commit_candidate(&self) -> Option<u64> {
        self.commit_candidate
    }

    /// One past the highest acknowledged offset, if any is acknowledged.
    #[must_use]
    pub const fn frontier(&self) -> Option<u64> {
        self.frontier
    }

    /// Number of acknowledged offsets in the window.
    #[must_use]
    pub const fn acked(&self) -> u64 {
        self.acked
    }
}

/// How far a mark sits above `base`: zero when absent, `None` when it is
/// present but not above `base` (a mark is always one past an offset).
const fn distance(base: u64, mark: Option<u64>) -> Option<u64> {
    match mark {
        None => Some(0),
        Some(mark) if mark > base => Some(mark - base),
        Some(_) => None,
    }
}

impl<const WORDS: usize> Tracker<WORDS> {
    /// The tracker's five readers as one value.
    #[must_use]
    pub fn status(&self) -> Status {
        Status {
            generation: self.generation(),
            base: self.base(),
            commit_candidate: self.commit_candidate(),
            frontier: self.frontier(),
            acked: self.acked(),
        }
    }
}
