# Changelog

## [0.3.0](https://github.com/seuros/offset_machines/compare/offset-machines-v0.2.0...offset-machines-v0.3.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* see CONVENTIONS: one exhaustive Refusal per crate, Result from every mutator, honoured idempotent repeats, Ruby mirrors the core. No compatibility aliases. Ruby ack answers :recorded; reassign answers :exhausted instead of nil.

### Features

* report refusals through Result&lt;_, Refusal&gt;, keep duplicate acks and commits honoured, add is_acked ([d1ac915](https://github.com/seuros/offset_machines/commit/d1ac9152bfc994abcc5f6bab20220a3961b5043b))

## [0.2.0](https://github.com/seuros/offset_machines/compare/offset-machines-v0.1.0...offset-machines-v0.2.0) (2026-11-12)


### Features

* initial extraction ([e061fd5](https://github.com/seuros/offset_machines/commit/e061fd59c8326783c70701e249d48f55805996e8))
