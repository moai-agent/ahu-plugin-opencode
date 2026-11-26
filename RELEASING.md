# Independent releases

This repository releases `harness-opencode` independently of ahu core and other
plugins. The initial candidate is 0.1.1 on `release/v0.1.1`. Nothing is published
automatically; explicit authorization is required before any remote creation,
push, merge, tag, GitHub Release or registry publication.

1. Version changed package contents and commit the candidate. Review the exact
   source, bundled contract provenance, manifest and dependency lockfile.
2. Run formatting, workspace tests, Clippy and the online dependency check in
   DEPENDENCIES.md; build/package from a fresh clone
   or source archive without sibling repositories. Follow INSTALL.md only.
3. Run core conformance against this production executable and the declared core
   versions. The development command `scripts/check-core-conformance.sh` accepts
   a reviewed core checkout and this staged package; it uses hermetic fixtures
   for other providers and never needs native accounts. CI pins an exact core
   consumer revision. Verify selective install and update without changing core or another
   plugin; preserve frozen identity for old tasks. Record unsupported modes.
4. Verify native version/model/mode, actual MCP calls, local OTel, eval outcomes,
   resume, cancellation and owned-resource cleanup. Live skips are not passes.
5. Audit source/runtime packages: license and guides included, no .ahu state,
   private metadata, credentials, captures or compiled source artifacts.
6. After approval, push this candidate, require hosted CI/review, merge to main,
   and tag the reviewed commit as `vX.Y.Z`. Publish source and only the platform
   binaries actually built and tested. Verify remote tag/release and reinstall
   from the published artifact. Never rewrite an existing tag.

A shared wire-contract update is separately reviewed and pinned; it does not
force other plugins to release. Protocol-breaking changes require an explicit
core/plugin compatibility transition. Keep detailed captures in ignored .ahu/.

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
