# frozen_string_literal: true

require 'minitest/autorun'
require 'offset_machines'

# `Status` is final, and sealing stops accidents, not reflection.
class SealTest < Minitest::Test
  FIELDS = { generation: 0, base: 99, commit_candidate: 5, frontier: 1, acked: 7 }.freeze

  # A named subclass raises too, but Ruby binds the constant before
  # `inherited` runs: the class leaks, and can be reopened.
  def leak
    return self.class::Leak if self.class.const_defined?(:Leak, false)

    assert_raises(TypeError) { self.class.class_eval('class Leak < OffsetMachines::Status; end', __FILE__, __LINE__) }
    self.class::Leak
  end

  def test_status_cannot_be_subclassed_or_allocated
    assert_raises(TypeError) { Class.new(OffsetMachines::Status) }
    assert_operator leak, :<, OffsetMachines::Status
    assert_raises(NoMethodError) { OffsetMachines::Status.allocate }
    assert_raises(RangeError) { OffsetMachines::Tracker.new.status.with(base: 1) }
  end

  # The leaked subclass and `Marshal.load` still build values that never ran
  # the check. None is a Status the gem answers, and no method of the gem
  # reads a Status back, so neither can reach the core.
  def test_forged_values_never_pass_for_a_tracker_answer
    leak.class_eval { def initialize(**fields) = Data.instance_method(:initialize).bind_call(self, **fields) }
    forged = leak.new(**FIELDS)
    refute_instance_of OffsetMachines::Status, forged
    refute_equal OffsetMachines::Status.new(0, 0, nil, nil, 0), forged

    real = OffsetMachines::Status.new(1, 10, 11, 11, 1)
    edited = Marshal.load(Marshal.dump(real).sub("i\x06", "i\x00"))
    assert_equal 0, edited.generation
    refute_equal real, edited
    assert_raises(RangeError) { edited.with(acked: 1) }
  end
end
