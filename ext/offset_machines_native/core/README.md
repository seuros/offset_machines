# offset-machines

> **AMS Kreps**, NCC-4220, *Kreps*-class log tender.

Engine-room flight manual: tally finished offsets and find the first hole in the log.

Initial, unstable. Allocation-free `no_std` per-partition acknowledgement
bitmap: record which offsets finished processing, learn the contiguous commit
candidate, and list the gaps behind the highest acknowledged offset.

This is not a commit log, consumer runtime or offset store. The caller owns
execution, storage and the commit itself. The tracker takes explicit inputs,
has a compile-time window (`Tracker<WORDS>` covers `WORDS * 64` offsets) and
refuses rather than grows.

- `assign(generation, base) -> Result<(), Refusal>` installs a strictly newer
  assignment generation and restarts the window; older or equal generations
  (and zero) are refused as `Stale`.
- `reassign(base) -> Result<u64, Refusal>` bumps the generation by one and
  returns it; `Exhausted` when the generation is already `u64::MAX`.
- `ack(generation, offset) -> Result<Ack, Refusal>` answers `Ack::Recorded`, or
  `Ack::Duplicate` for an idempotent repeat (already acked, or below the
  window). Refused as `Stale` (wrong generation, or unassigned) or
  `OutOfWindow`. Offset `u64::MAX` is never ackable.
- `is_acked(offset) -> bool` is the read-only form of that answer: `true` when
  `offset` is below the base (committed) or recorded in the window, which is
  exactly when `ack` under the current generation answers `Ack::Duplicate`.
  `false` beyond the window, for `u64::MAX` and while unassigned.
- `commit_candidate()` is one past the contiguous acknowledged prefix, or
  `None`. The adapter persists it however it likes.
- `mark_committed(generation, offset) -> Result<Commit, Refusal>` is the
  adapter's confirmation of persistence. Only then does the window slide:
  `Commit::Advanced`. Partial confirmation up to the candidate is allowed. An
  offset at or below the base is an idempotent repeat, `Commit::Duplicate`.
  Refused as `Stale` (wrong generation, or unassigned) or `Unacked` (beyond
  the candidate).
- `status() -> Status` is the five readers as one value. `Status::new::<WORDS>(..)
  -> Option<Status>` is how a `Status` is built from loose numbers: `None` for
  any combination no `Tracker<WORDS>` reports (a frontier past the window, a
  candidate above the frontier, a count that doesn't fit the marks, a base or
  count while unassigned). The fields are private; a `Status` is valid by
  construction.
- `gaps(from)` iterates unacknowledged offsets below the frontier; resume by
  passing the last yielded offset plus one. Output is bounded by the window.

Every call that is not honoured returns `Err(Refusal)` and leaves the tracker
bit-for-bit unchanged. `Refusal` is the crate's only refusal type: `Stale`,
`OutOfWindow`, `Unacked`, `Exhausted`. No enum is `#[non_exhaustive]`: a new
variant is a breaking change every caller matches on. Idempotent repeats are honoured (`Ok`)
and also change nothing.

There is no clock: no operation takes or records an instant.

Crash recovery, persistence, rebalancing protocol and retry policy are adapter
concerns; after a restart, `assign` a newer generation at the stored offset.

```rust
use offset_machines::{Ack, Commit, Refusal, Tracker};
let mut tracker = Tracker::<4>::new(); // 256-offset window
assert_eq!(tracker.ack(1, 100), Err(Refusal::Stale)); // nothing assigned yet
assert_eq!(tracker.assign(1, 100), Ok(()));
assert_eq!(tracker.ack(1, 100), Ok(Ack::Recorded));
assert_eq!(tracker.ack(1, 102), Ok(Ack::Recorded));
assert_eq!(tracker.ack(1, 102), Ok(Ack::Duplicate));
assert!(tracker.is_acked(99) && tracker.is_acked(102) && !tracker.is_acked(101));
assert_eq!(tracker.commit_candidate(), Some(101));
assert_eq!(tracker.gaps(0).next(), Some(101));
assert_eq!(tracker.mark_committed(1, 102), Err(Refusal::Unacked));
assert_eq!(tracker.mark_committed(1, 101), Ok(Commit::Advanced));
assert_eq!(tracker.mark_committed(1, 101), Ok(Commit::Duplicate));
assert_eq!(tracker.base(), 101);
assert_eq!(tracker.reassign(101), Ok(2));
```
