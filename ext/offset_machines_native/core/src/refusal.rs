//! The one answer for every call that changed nothing.
use core::fmt;

/// Why a call was not honoured. Every refusal leaves the tracker exactly as
/// it was; no operation reports "nothing happened" any other way.
///
/// An idempotent repeat is not a refusal: acknowledging an offset twice or
/// confirming a commit twice is honoured as [`Ack::Duplicate`](crate::Ack) or
/// [`Commit::Duplicate`](crate::Commit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The generation is not acceptable: for [`Tracker::assign`] it is not
    /// newer than the current one (zero is never newer); for [`Tracker::ack`]
    /// and [`Tracker::mark_committed`] it is not the current assignment
    /// generation, or the tracker is unassigned.
    ///
    /// [`Tracker::assign`]: crate::Tracker::assign
    /// [`Tracker::ack`]: crate::Tracker::ack
    /// [`Tracker::mark_committed`]: crate::Tracker::mark_committed
    Stale,
    /// The offset is beyond the window, or is `u64::MAX`.
    OutOfWindow,
    /// The confirmed offset is beyond the contiguous acknowledged prefix.
    Unacked,
    /// The generation space is spent; [`Tracker::reassign`] has no next
    /// generation to issue.
    ///
    /// [`Tracker::reassign`]: crate::Tracker::reassign
    Exhausted,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stale => "the generation is not current",
            Self::OutOfWindow => "the offset is beyond the window",
            Self::Unacked => "the offset is beyond the acknowledged prefix",
            Self::Exhausted => "generation space exhausted",
        })
    }
}

impl core::error::Error for Refusal {}
