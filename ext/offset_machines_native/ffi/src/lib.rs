//! Thin binding to the bounded acknowledgement tracker.
mod init;

use std::cell::RefCell;

use magnus::{Error, RArray, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use matryona_machines::ruby::{Symbols, integer, optional, symbol};
use offset_machines::{Ack, Commit, Refusal, Status};

/// Words in the compiled window (`WORDS * 64` offsets).
const WORDS: usize = 4;
/// Offsets in the compiled window.
const CAPACITY: u64 = Core::CAPACITY;
/// The window as a gap count: at most this many gaps exist.
const WINDOW: usize = WORDS * 64;

matryona_machines::symbols! {
    /// The core's refusals as Symbols: the snake case of each variant.
    Refusals for Refusal as "a refusal" {
        Stale => "stale",
        OutOfWindow => "out_of_window",
        Unacked => "unacked",
        Exhausted => "exhausted",
    }
}

matryona_machines::symbols! {
    /// The honoured answers of `ack` as Symbols.
    Acks for Ack as "an ack" {
        Recorded => "recorded",
        Duplicate => "duplicate",
    }
}

matryona_machines::symbols! {
    /// The honoured answers of `mark_committed` as Symbols.
    Commits for Commit as "a commit" {
        Advanced => "advanced",
        Duplicate => "duplicate",
    }
}

/// What `Status.new` checks its five fields with: the core's `Status::new`
/// for the compiled window. `TypeError` for a non-Integer (`nil` is allowed
/// for the two optional marks), `RangeError` for a value outside
/// `0..2**64-1` or a combination no tracker reports.
fn status(
    ruby: &Ruby,
    generation: Value,
    base: Value,
    commit_candidate: Value,
    frontier: Value,
    acked: Value,
) -> Result<(), Error> {
    Status::new::<WORDS>(
        integer(generation, "generation")?,
        integer(base, "base")?,
        optional(commit_candidate, "commit_candidate")?,
        optional(frontier, "frontier")?,
        integer(acked, "acked")?,
    )
    .map(|_| ())
    .ok_or_else(|| {
        Error::new(
            ruby.exception_range_error(),
            "no tracker reports this status",
        )
    })
}

/// The symbol for an outcome: the honoured value's from table `S`, or the
/// refusal's.
fn answer<S: Symbols>(ruby: &Ruby, outcome: Result<S::Variant, Refusal>) -> Symbol {
    match outcome {
        Ok(honoured) => S::symbol(ruby, honoured),
        Err(refusal) => Refusals::symbol(ruby, refusal),
    }
}

type Core = offset_machines::Tracker<WORDS>;

#[magnus::wrap(class = "OffsetMachines::Tracker", free_immediately, size)]
struct Tracker {
    core: RefCell<Core>,
}

impl Tracker {
    const fn new() -> Self {
        Self {
            core: RefCell::new(Core::new()),
        }
    }

    fn assign(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Value,
        base: Value,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let (generation, base) = (integer(generation, "generation")?, integer(base, "base")?);
        let outcome = this.core.borrow_mut().assign(generation, base);
        Ok(match outcome {
            Ok(()) => symbol(ruby, "assigned"),
            Err(refusal) => Refusals::symbol(ruby, refusal),
        })
    }

    /// The new generation, or a refusal symbol.
    fn reassign(ruby: &Ruby, this: Obj<Self>, base: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let base = integer(base, "base")?;
        let outcome = this.core.borrow_mut().reassign(base);
        Ok(match outcome {
            Ok(generation) => ruby.integer_from_u64(generation).as_value(),
            Err(refusal) => Refusals::symbol(ruby, refusal).as_value(),
        })
    }

    fn ack(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Value,
        offset: Value,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let (generation, offset) = (
            integer(generation, "generation")?,
            integer(offset, "offset")?,
        );
        let outcome = this.core.borrow_mut().ack(generation, offset);
        Ok(answer::<Acks>(ruby, outcome))
    }

    fn mark_committed(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Value,
        offset: Value,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let (generation, offset) = (
            integer(generation, "generation")?,
            integer(offset, "offset")?,
        );
        let outcome = this.core.borrow_mut().mark_committed(generation, offset);
        Ok(answer::<Commits>(ruby, outcome))
    }

    fn is_acked(&self, offset: Value) -> Result<bool, Error> {
        Ok(self.core.borrow().is_acked(integer(offset, "offset")?))
    }

    fn commit_candidate(&self) -> Option<u64> {
        self.core.borrow().commit_candidate()
    }

    fn frontier(&self) -> Option<u64> {
        self.core.borrow().frontier()
    }

    /// At most `limit` gaps (clamped to the window) at or after `from`.
    fn gaps(ruby: &Ruby, this: &Self, from: Value, limit: Value) -> Result<RArray, Error> {
        let (from, limit): (u64, u64) = (integer(from, "from")?, integer(limit, "limit")?);
        // The window bounds the answer whatever the limit.
        let take = usize::try_from(limit).map_or(WINDOW, |limit| limit.min(WINDOW));
        let array = ruby.ary_new_capa(take);
        for offset in this.core.borrow().gaps(from).take(take) {
            array.push(offset)?;
        }
        Ok(array)
    }

    fn generation(&self) -> u64 {
        self.core.borrow().generation()
    }

    fn base(&self) -> u64 {
        self.core.borrow().base()
    }

    fn acked(&self) -> u64 {
        self.core.borrow().acked()
    }

    const fn capacity() -> u64 {
        CAPACITY
    }
}
