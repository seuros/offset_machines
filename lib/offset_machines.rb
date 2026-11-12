# frozen_string_literal: true

require_relative 'offset_machines/version'
# Native is required: no algorithm fallback.
require 'offset_machines_native/offset_machines_native'

module OffsetMachines
  # Immutable point-in-time view of a tracker.
  Status = Data.define(:generation, :base, :commit_candidate, :frontier, :acked)

  class Tracker
    def status
      Status.new(
        generation: generation,
        base: base,
        commit_candidate: commit_candidate,
        frontier: frontier,
        acked: acked
      )
    end
  end
end
