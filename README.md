# ahu opencode plugin

An independently released Rust adapter for the `opencode` native CLI. Install only
this package when ahu needs to manage opencode. Other harness plugins are optional.
A harness using `ahu mcp serve` solely as an MCP client does not need this adapter.

This repository owns native launch arguments, setup proposals, admission, account
observations, resume integration and event normalization. ahu core owns task state,
process supervision, worktrees, grants and acceptance of results. Native credentials,
conversation history and trust settings remain with the native CLI.

Read [INSTALL.md](INSTALL.md) for the complete standalone build, package, install,
smoke, update and removal sequence. Read [COMPATIBILITY.md](COMPATIBILITY.md) for
version boundaries and [SECURITY.md](SECURITY.md) before granting runtime access.
Customer builds and execution require Rust/Cargo and the native CLI, never Python.

No sibling checkout or ahu source tree is needed to build. The small wire-contract
crate is a frozen vendored dependency with [provenance](vendor/PROVENANCE.json).
Native implementation crates belong to this repository. See
[EXTRACTION.md](EXTRACTION.md) for migration provenance.

This is an unpublished local release candidate. Remote clone/tag commands become
available only after the release is approved and published. See
[RELEASING.md](RELEASING.md) for independent release gates.

## Development

```sh
umask 077
mkdir -p .ahu/build
export CARGO_TARGET_DIR="$PWD/.ahu/build/development"
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Protocol tests are hermetic. Live conformance additionally needs an installed,
signed-in native CLI. Record unavailable modes as unverified. Keep builds,
packages, captures and evaluation records under this repository's ignored `.ahu/`.
