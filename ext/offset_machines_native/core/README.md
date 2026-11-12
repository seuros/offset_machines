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

- `assign(generation, base)` installs a strictly newer assignment generation and
  restarts the window; older or equal generations (and zero) are `Stale`.
  `reassign(base)` bumps the generation by one.
- `ack(generation, offset)` answers `Ok`, `Duplicate` (already acked or below
  the window), `Stale` (wrong generation) or `OutOfWindow`. Offset `u64::MAX`
  is never ackable.
- `commit_candidate()` is one past the contiguous acknowledged prefix, or
  `None`. The adapter persists it however it likes.
- `mark_committed(generation, offset)` is the adapter's confirmation of
  persistence. Only then does the window slide. Partial confirmation up to the
  candidate is allowed; beyond it is `Unacked`.
- `gaps(from)` iterates unacknowledged offsets below the frontier; resume by
  passing the last yielded offset plus one. Output is bounded by the window.

Crash recovery, persistence, rebalancing protocol and retry policy are adapter
concerns; after a restart, `assign` a newer generation at the stored offset.

```rust
use offset_machines::{Ack, Assign, Commit, Tracker};
let mut tracker = Tracker::<4>::new(); // 256-offset window
assert_eq!(tracker.assign(1, 100), Assign::Assigned);
assert_eq!(tracker.ack(1, 100), Ack::Ok);
assert_eq!(tracker.ack(1, 102), Ack::Ok);
assert_eq!(tracker.commit_candidate(), Some(101));
assert_eq!(tracker.gaps(0).next(), Some(101));
assert_eq!(tracker.mark_committed(1, 101), Commit::Advanced);
assert_eq!(tracker.base(), 101);
```
