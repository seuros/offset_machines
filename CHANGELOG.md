# Changelog

## [0.3.0](https://github.com/seuros/offset_machines/compare/offset_machines/v0.2.0...offset_machines/v0.3.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* validate Status through the core, seal it, and consolidate tracker internals

### Features

* validate Status through the core, seal it, and consolidate tracker internals ([3cc7dd3](https://github.com/seuros/offset_machines/commit/3cc7dd3e160c178f1d368464b1dd4029a60be7bd))

## [0.2.0](https://github.com/seuros/offset_machines/compare/offset_machines/v0.1.1...offset_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* see CONVENTIONS: one exhaustive Refusal per crate, Result from every mutator, honoured idempotent repeats, Ruby mirrors the core. No compatibility aliases. Ruby ack answers :recorded; reassign answers :exhausted instead of nil.

### Features

* report refusals through Result&lt;_, Refusal&gt;, keep duplicate acks and commits honoured, add is_acked ([d1ac915](https://github.com/seuros/offset_machines/commit/d1ac9152bfc994abcc5f6bab20220a3961b5043b))

## [0.1.1](https://github.com/seuros/offset_machines/compare/offset_machines-v0.1.0...offset_machines/v0.1.1) (2026-11-12)


### Features

* initial extraction ([e061fd5](https://github.com/seuros/offset_machines/commit/e061fd59c8326783c70701e249d48f55805996e8))
