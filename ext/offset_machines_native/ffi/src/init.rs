//! Private native entry point.
use crate::{CAPACITY, Tracker};
use magnus::{Error, Ruby, function, method, prelude::*};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("OffsetMachines")?;
    module.const_set("CAPACITY", CAPACITY)?;
    let class = module.define_class("Tracker", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(Tracker::new, 0))?;
    class.define_method("assign", method!(Tracker::assign, 2))?;
    class.define_method("reassign", method!(Tracker::reassign, 1))?;
    class.define_method("ack", method!(Tracker::ack, 2))?;
    class.define_method("mark_committed", method!(Tracker::mark_committed, 2))?;
    class.define_method("acked?", method!(Tracker::is_acked, 1))?;
    class.define_method("commit_candidate", method!(Tracker::commit_candidate, 0))?;
    class.define_method("frontier", method!(Tracker::frontier, 0))?;
    class.define_method("gaps", method!(Tracker::gaps, 2))?;
    class.define_method("generation", method!(Tracker::generation, 0))?;
    class.define_method("base", method!(Tracker::base, 0))?;
    class.define_method("acked", method!(Tracker::acked, 0))?;
    class.define_method("capacity", function!(Tracker::capacity, 0))?;
    Ok(())
}
