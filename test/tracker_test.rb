# frozen_string_literal: true

require 'minitest/autorun'
require 'offset_machines'

class TrackerTest < Minitest::Test
  def test_capacity_is_compiled_constant
    assert_equal 256, OffsetMachines::CAPACITY
    assert_equal OffsetMachines::CAPACITY, OffsetMachines::Tracker.new.capacity
  end

  def test_unassigned_refuses
    t = OffsetMachines::Tracker.new
    assert_equal :stale, t.ack(0, 0)
    assert_equal :stale, t.ack(1, 0)
    assert_equal :stale, t.mark_committed(0, 1)
    assert_equal :stale, t.mark_committed(1, 1)
    assert_equal :stale, t.assign(0, 5)
    assert_nil t.commit_candidate
    assert_equal OffsetMachines::Tracker.new.status, t.status
  end

  def test_prefix_gaps_and_commit
    t = OffsetMachines::Tracker.new
    assert_equal :assigned, t.assign(1, 100)
    assert_equal :recorded, t.ack(1, 102)
    assert_equal :recorded, t.ack(1, 100)
    assert_equal :duplicate, t.ack(1, 100)
    assert_equal :duplicate, t.ack(1, 99)
    assert_equal 101, t.commit_candidate
    assert_equal 103, t.frontier
    assert_equal [101], t.gaps(0, 10)
    assert_equal :unacked, t.mark_committed(1, 102)
    assert_equal :advanced, t.mark_committed(1, 101)
    assert_equal :duplicate, t.mark_committed(1, 101)
    assert_equal 101, t.base
    assert_equal [101], t.gaps(0, 10)
    assert_equal 1, t.acked
  end

  def test_acked_reads_the_base_and_the_window
    t = OffsetMachines::Tracker.new
    refute t.acked?(0)
    t.assign(3, 100)
    assert t.acked?(99)
    refute t.acked?(100)
    assert_equal :recorded, t.ack(3, 102)
    assert t.acked?(102)
    refute t.acked?(101)
    refute t.acked?(100 + OffsetMachines::CAPACITY)
    refute t.acked?(2**64 - 1)
    [99, 102].each { |offset| assert_equal :duplicate, t.ack(3, offset) }
    assert_equal 4, t.reassign(50)
    assert t.acked?(49)
    refute t.acked?(102)
    assert_raises(TypeError) { t.acked?(1.0) }
    assert_raises(RangeError) { t.acked?(-1) }
    assert t.freeze.acked?(49)
  end

  def test_window_bound_and_gap_limit
    t = OffsetMachines::Tracker.new
    t.assign(1, 0)
    assert_equal :recorded, t.ack(1, 255)
    assert_equal :out_of_window, t.ack(1, 256)
    assert_equal :out_of_window, t.ack(1, 2**64 - 1)
    assert_equal (0..9).to_a, t.gaps(0, 10)
    assert_equal 255, t.gaps(0, 10**6).size
    assert_equal [], t.gaps(0, 0)
    assert_equal [10, 11], t.gaps(10, 2)
  end

  def test_reassign_rejects_stale_generation
    t = OffsetMachines::Tracker.new
    t.assign(1, 0)
    t.ack(1, 0)
    assert_equal 2, t.reassign(50)
    assert_equal 2, t.generation
    assert_equal 0, t.acked
    assert_equal :stale, t.ack(1, 50)
    assert_equal :stale, t.mark_committed(1, 51)
    assert_equal :recorded, t.ack(2, 50)
    assert_equal :stale, t.assign(2, 0)
    assert_equal :assigned, t.assign(2**64 - 1, 0)
    assert_equal :exhausted, t.reassign(0)
    assert_equal 2**64 - 1, t.generation
  end

  def test_refusals_change_nothing
    t = OffsetMachines::Tracker.new
    t.assign(5, 1000)
    [1000, 1001, 1003].each { |offset| assert_equal :recorded, t.ack(5, offset) }
    before = t.status
    gaps = t.gaps(0, 256)
    refused = [
      [:stale, -> { t.assign(5, 0) }], [:stale, -> { t.assign(4, 0) }], [:stale, -> { t.assign(0, 0) }],
      [:stale, -> { t.ack(4, 1002) }], [:stale, -> { t.ack(6, 1002) }], [:stale, -> { t.ack(0, 1002) }],
      [:out_of_window, -> { t.ack(5, 1256) }], [:out_of_window, -> { t.ack(5, 2**64 - 1) }],
      [:stale, -> { t.mark_committed(4, 1001) }], [:stale, -> { t.mark_committed(6, 1001) }],
      [:unacked, -> { t.mark_committed(5, 1003) }], [:unacked, -> { t.mark_committed(5, 2**64 - 1) }]
    ]
    refused.each do |symbol, call|
      assert_equal symbol, call.call
      assert_equal before, t.status
      assert_equal gaps, t.gaps(0, 256)
    end
    spent = OffsetMachines::Tracker.new
    spent.assign(2**64 - 1, 7)
    spent.ack(2**64 - 1, 7)
    before = spent.status
    assert_equal :exhausted, spent.reassign(0)
    assert_equal before, spent.status
  end

  def test_idempotent_repeats_are_honoured_and_change_nothing
    t = OffsetMachines::Tracker.new
    t.assign(5, 1000)
    [1000, 1001, 1003].each { |offset| assert_equal :recorded, t.ack(5, offset) }
    assert_equal :advanced, t.mark_committed(5, 1001)
    before = t.status
    [1001, 1003, 1000, 0].each { |offset| assert_equal :duplicate, t.ack(5, offset) }
    [1001, 1000, 0].each { |offset| assert_equal :duplicate, t.mark_committed(5, offset) }
    assert_equal before, t.status
    assert_equal [1002], t.gaps(0, 256)
  end

  def test_status_is_data
    t = OffsetMachines::Tracker.new
    t.assign(3, 10)
    t.ack(3, 10)
    s = t.status
    assert_instance_of OffsetMachines::Status, s
    keys = %i[generation base commit_candidate frontier acked]
    assert_equal [3, 10, 11, 11, 1], s.deconstruct_keys(nil).values_at(*keys)
    assert_predicate s, :frozen?
  end

  def test_strict_inputs_and_freeze
    t = OffsetMachines::Tracker.new
    [1.0, Float::NAN, nil, '1', Object.new].each do |v|
      assert_raises(TypeError) { t.assign(v, 0) }
      assert_raises(TypeError) { t.ack(1, v) }
      assert_raises(TypeError) { t.gaps(v, 1) }
    end
    [-1, 2**64].each { |n| assert_raises(RangeError) { t.assign(n, 0) } }
    assert_raises(TypeError) { OffsetMachines::Tracker.allocate }
    t.freeze
    assert_raises(FrozenError) { t.assign(1, 0) }
    assert_raises(FrozenError) { t.reassign(0) }
    assert_raises(FrozenError) { t.ack(1, 0) }
    assert_raises(FrozenError) { t.mark_committed(1, 1) }
    assert_equal 256, t.capacity
  end

  def test_gc_stress
    previous = GC.stress
    GC.stress = true
    t = OffsetMachines::Tracker.new
    t.assign(1, 2**64 - 300)
    assert_equal :recorded, t.ack(1, 2**64 - 300)
    assert_equal 2**64 - 299, t.commit_candidate
    GC.compact
    assert_equal 1, t.acked
  ensure
    GC.stress = previous
  end
end
