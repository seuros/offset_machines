# frozen_string_literal: true

require 'minitest/autorun'
require 'offset_machines'
require 'open3'
require 'rbconfig'

# The class of the tracker the extension wraps is fixed when it loads. Nothing
# a caller does later, redefining the constant or `Object.const_get`, reaches
# a tracker being built. Each case runs in a fresh process: the class is fixed
# once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  PRELUDE = <<~RUBY
    require 'offset_machines'
    O = OffsetMachines
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Tracker constant redefined before the first tracker made it an
  # instance of the new class, without any of the gem's methods.
  def test_a_redefined_tracker_constant_does_not_change_the_tracker_class
    assert_equal 'true true assigned recorded', isolated(<<~RUBY)
      original = O::Tracker
      O.send(:remove_const, :Tracker)
      O.const_set(:Tracker, Class.new)
      fresh = original.new
      assigned = original.assigned(1, 10)
      puts [fresh.instance_of?(original), assigned.instance_of?(original), fresh.assign(1, 0),
            assigned.ack(1, 10)].join(' ')
    RUBY
  end

  # Bug: building the first tracker ran `Object.const_get`, a caller's code in
  # the middle of a native call.
  def test_building_a_tracker_runs_no_const_get
    assert_equal '0 false Tracker Tracker', isolated(<<~RUBY)
      hits = 0
      log = []
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        log.freeze
        super(*args)
      end
      trackers = [O::Tracker.new, O::Tracker.assigned(1, 10)]
      puts [hits, log.frozen?, *trackers.map { |t| t.class.name.split('::').last }].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal 'assigned recorded', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran mid-call' }
      puts [O::Tracker.new.assign(1, 0), O::Tracker.assigned(1, 10).ack(1, 10)].join(' ')
    RUBY
  end
end
