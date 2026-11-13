//! Thin binding to the bounded acknowledgement tracker.
mod init;

use std::cell::RefCell;

use magnus::{
    Error, Integer, RArray, Ruby, Symbol, TryConvert, Value, prelude::*, typed_data::Obj,
};
use offset_machines::{Ack, Commit, Refusal};

const WORDS: usize = 4;
const CAPACITY: u64 = WORDS as u64 * 64;

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

#[magnus::wrap(class = "OffsetMachines::Tracker", free_immediately, size)]
struct Tracker {
    core: RefCell<offset_machines::Tracker<WORDS>>,
}

impl Tracker {
    const fn new() -> Self {
        Self {
            core: RefCell::new(offset_machines::Tracker::new()),
        }
    }

    fn assign(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Number,
        base: Number,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().assign(generation.0, base.0);
        Ok(ruby.to_symbol(match outcome {
            Ok(()) => "assigned",
            Err(refusal) => refusal_name(refusal),
        }))
    }

    /// The new generation, or a refusal symbol.
    fn reassign(ruby: &Ruby, this: Obj<Self>, base: Number) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().reassign(base.0);
        Ok(match outcome {
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
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().ack(generation.0, offset.0);
        Ok(ruby.to_symbol(match outcome {
            Ok(Ack::Duplicate) => "duplicate",
            Ok(Ack::Recorded) => "recorded",
            Err(refusal) => refusal_name(refusal),
        }))
    }

    fn mark_committed(
        ruby: &Ruby,
        this: Obj<Self>,
        generation: Number,
        offset: Number,
    ) -> Result<Symbol, Error> {
        this.check_frozen()?;
        let outcome = this
            .core
            .borrow_mut()
            .mark_committed(generation.0, offset.0);
        Ok(ruby.to_symbol(match outcome {
            Ok(Commit::Duplicate) => "duplicate",
            Ok(Commit::Advanced) => "advanced",
            Err(refusal) => refusal_name(refusal),
        }))
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
        let take = usize::try_from(limit.0.min(CAPACITY)).unwrap_or(0);
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
