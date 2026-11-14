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

# Native is required: no algorithm fallback. It validates the Data value above.
require 'offset_machines_native/offset_machines_native'

module OffsetMachines
  private_class_method :__status

  # The value is final. `initialize` is where it validates, and a subclass
  # could override it, or `allocate` build an instance that never ran it:
  # subclassing raises TypeError and `allocate` is gone. This stops accidents,
  # not reflection (Marshal, `Data#initialize` bound by hand); no method of
  # this gem reads a Status back, so a forged one never reaches the core.
  Status.singleton_class.undef_method(:allocate)
  Status.define_singleton_method(:inherited) { |_| raise TypeError, "#{Status} cannot be subclassed" }

  class Tracker
    # The core's `Tracker::status`.
    def status = Status.new(generation, base, commit_candidate, frontier, acked)
  end
end
