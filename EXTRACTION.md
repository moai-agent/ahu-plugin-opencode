# Extraction provenance

Initial source: ahu commit `bffcf8a09cd15c6704f21a40d8c43554d83646a3`, package `plugins/harness-opencode` and its
Rust runtime/native/events libraries. Provider dispatch is specialized for `opencode`.
The frozen wire-contract dependency is vendored with provenance and a content inventory.
No sibling checkout, core executable source, or unpublished remote dependency is needed.
Core and plugin communicate over runtime protocol version 1, not a Rust ABI.

This repository owns its native adapter and releases independently. Changes to the
vendored contract require explicit review and compatibility tests; no automatic sync.
The original cross-provider tests remain hermetic reference fixtures in core.
