//! Thin binding to the bounded acknowledgement tracker.
mod init;

use std::cell::RefCell;

use magnus::{
    Error, Integer, RArray, Ruby, Symbol, TryConvert, Value, prelude::*, typed_data::Obj,
};
use offset_machines::{Ack, Commit, Refusal, Status};

/// Words in the compiled window (`WORDS * 64` offsets).
const WORDS: usize = 4;
/// Offsets in the compiled window.
const CAPACITY: u64 = Core::CAPACITY;
/// The window as a gap count: at most this many gaps exist.
const WINDOW: usize = WORDS * 64;

#[derive(Clone, Copy)]
struct Number(u64);

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer (unsigned 64-bit)",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

/// Ruby symbol name of a refusal: the snake case of the core variant.
const fn refusal_name(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Stale => "stale",
        Refusal::OutOfWindow => "out_of_window",
        Refusal::Unacked => "unacked",
        Refusal::Exhausted => "exhausted",
    }
}

/// What `Status.new` checks its five fields with: the core's `Status::new`
/// for the compiled window. `TypeError` for a non-Integer (`nil` is allowed
/// for the two optional marks), `RangeError` for a value outside
/// `0..2**64-1` or a combination no tracker reports.
fn status(
    ruby: &Ruby,
    generation: Number,
    base: Number,
    commit_candidate: Option<Number>,
    frontier: Option<Number>,
    acked: Number,
) -> Result<(), Error> {
    let mark = |mark: Option<Number>| mark.map(|mark| mark.0);
    Status::new::<WORDS>(
        generation.0,
        base.0,
        mark(commit_candidate),
        mark(frontier),
        acked.0,
    )
    .map(|_| ())
    .ok_or_else(|| {
        Error::new(
            ruby.exception_range_error(),
            "no tracker reports this status",
        )
    })
}

/// The symbol for an outcome: `name` of the honoured value, or the refusal's.
fn symbol<T>(ruby: &Ruby, outcome: Result<T, Refusal>, name: fn(T) -> &'static str) -> Symbol {
    ruby.to_symbol(outcome.map_or_else(refusal_name, name))
}

const fn ack_name(ack: Ack) -> &'static str {
    match ack {
        Ack::Recorded => "recorded",
        Ack::Duplicate => "duplicate",
    }
}

const fn commit_name(commit: Commit) -> &'static str {
    match commit {
        Commit::Advanced => "advanced",
        Commit::Duplicate => "duplicate",
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

    /// Run a mutating core call: `FrozenError` before it, nothing after.
    fn apply<T>(this: Obj<Self>, call: impl FnOnce(&mut Core) -> T) -> Result<T, Error> {
        this.check_frozen()?;
        Ok(call(&mut this.core.borrow_mut()))
    }

    fn assign(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Number,
        base: Number,
    ) -> Result<Symbol, Error> {
        let outcome = Self::apply(this, |core| core.assign(generation.0, base.0))?;
        Ok(symbol(ruby, outcome, |()| "assigned"))
    }

    /// The new generation, or a refusal symbol.
    fn reassign(ruby: &Ruby, this: Obj<Self>, base: Number) -> Result<Value, Error> {
        Ok(match Self::apply(this, |core| core.reassign(base.0))? {
            Ok(generation) => ruby.integer_from_u64(generation).as_value(),
            Err(refusal) => ruby.to_symbol(refusal_name(refusal)).as_value(),
        })
    }

    fn ack(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Number,
        offset: Number,
    ) -> Result<Symbol, Error> {
        let outcome = Self::apply(this, |core| core.ack(generation.0, offset.0))?;
        Ok(symbol(ruby, outcome, ack_name))
    }

    fn mark_committed(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Number,
        offset: Number,
    ) -> Result<Symbol, Error> {
        let outcome = Self::apply(this, |core| core.mark_committed(generation.0, offset.0))?;
        Ok(symbol(ruby, outcome, commit_name))
    }

    fn is_acked(&self, offset: Number) -> bool {
        self.core.borrow().is_acked(offset.0)
    }

    fn commit_candidate(&self) -> Option<u64> {
        self.core.borrow().commit_candidate()
    }

    fn frontier(&self) -> Option<u64> {
        self.core.borrow().frontier()
    }

    /// At most `limit` gaps (clamped to the window) at or after `from`.
    fn gaps(ruby: &Ruby, this: &Self, from: Number, limit: Number) -> Result<RArray, Error> {
        // The window bounds the answer whatever the limit.
        let take = usize::try_from(limit.0).map_or(WINDOW, |limit| limit.min(WINDOW));
        let array = ruby.ary_new_capa(take);
        for offset in this.core.borrow().gaps(from.0).take(take) {
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
