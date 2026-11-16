# frozen_string_literal: true

require_relative 'offset_machines/version'

module OffsetMachines
  # The core's `Status`: an immutable point-in-time view of a tracker, the
  # answers of its five readers together. Like `Status::new`, it holds only
  # what a tracker of this window could report: anything else raises
  # RangeError (TypeError for a non-Integer; the two marks may be nil).
  Status = Data.define(:generation, :base, :commit_candidate, :frontier, :acked) do
    def initialize(generation:, base:, commit_candidate:, frontier:, acked:)
      OffsetMachines.__send__(:__status, generation, base, commit_candidate, frontier, acked)
      super
    end
  end
end

# Native is required: no algorithm fallback. It validates the Data value above
# and builds `Tracker#status` from the core's `Tracker::status`, the five
# readers taken at once rather than dispatched one by one.
require 'offset_machines_native/offset_machines_native'

module OffsetMachines
  private_class_method :__status

  # The value is final. `initialize` is where it validates, and a subclass
  # could override it, or `allocate` build an instance that never ran it:
  # subclassing raises TypeError and `allocate` is gone. This stops accidents,
  # not reflection (Marshal, `Data#initialize` bound by hand); no method of
  # this gem reads a Status back, so a forged one never reaches the core.
  #
  # `Tracker` wraps native data and is sealed the same way: a subclass could
  # override what callers rely on, and `allocate` would build a shell with no
  # core behind it.
  [Status, Tracker].each do |klass|
    klass.singleton_class.undef_method(:allocate)
    klass.define_singleton_method(:inherited) { |_| raise TypeError, "#{klass} cannot be subclassed" }
  end
end
