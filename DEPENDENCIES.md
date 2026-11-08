# Dependency policy

Review manifests and Cargo.lock before each independent release. The pinned
cargo-deny 0.20.2 check covers all features and targets, including development and
build dependencies. Run from this repository root:

```sh
umask 077
mkdir -p .ahu
cargo deny --locked check --deny index-failure
```

The advisory database stays in `.ahu/advisory-db`. Online advisory and registry
checks are required; offline cached checks are diagnostic only. Hosted CI uses
the verified tool archive pinned in `.github/workflows/dependencies.yml`.

The policy comes from ahu's reviewed release policy at core commit
`c1be55431f51e5b75d5e6c5ebcb1fb33c0ca3b81`, restricted to dependencies present in
this package's lockfile. It allows MIT, Apache-2.0 and Unicode-3.0 licenses and
crates.io sources. It rejects advisories, yanked dependencies, unknown sources,
wildcard requirements and unreviewed duplicate versions. Only unpublished
workspace packages are exempt from license checks.

Exact license and duplicate-version exceptions, where needed, are listed in
`deny.toml`; no vulnerability exception is configured. The accountable maintainer
for inherited license exceptions is Orie Steele. Re-review them on each affected
lockfile update and before release. Retain the applicable upstream licenses and
notices with distributed artifacts. New exceptions require maintainer approval,
exact scope, rationale and an expiry or explicit upstream review condition.
