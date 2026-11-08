# Independent releases

This repository releases `harness-opencode` independently of ahu core and other
plugins. The initial candidate is 0.1.1 on `release/v0.1.1`. Nothing is published
automatically; explicit authorization is required before any remote creation,
push, merge, tag, GitHub Release or registry publication.

1. Version changed package contents and commit the candidate. Review the exact
   source, vendored contract provenance, manifest and dependency lockfile.
2. Run formatting, workspace tests and Clippy; build/package from a fresh clone
   or source archive without sibling repositories. Follow INSTALL.md only.
3. Run core conformance against this production executable and the declared core
   versions. Verify selective install and update without changing core or another
   plugin; preserve frozen identity for old tasks. Record unsupported modes.
4. Verify native version/model/mode, actual MCP calls, local OTel, eval outcomes,
   resume, cancellation and owned-resource cleanup. Live skips are not passes.
5. Audit source/runtime packages: license and guides included, no .ahu state,
   private metadata, credentials, captures or compiled source artifacts.
6. After approval, push this candidate, require hosted CI/review, merge to main,
   and tag the reviewed commit as vX.Y.Z. Publish source and only the platform
   binaries actually built and tested. Verify remote tag/release and reinstall
   from the published artifact. Never rewrite an existing tag.

A shared wire-contract update is separately reviewed and pinned; it does not
force other plugins to release. Protocol-breaking changes require an explicit
core/plugin compatibility transition. Keep detailed captures in ignored .ahu/.
