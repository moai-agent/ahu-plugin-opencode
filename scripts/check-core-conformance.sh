#!/bin/sh
# Development/CI validation only. Does not install plugins, use accounts or publish.
set -eu
umask 077
[ "$#" -eq 2 ] || { echo 'usage: scripts/check-core-conformance.sh CORE_CHECKOUT STAGED_PACKAGE' >&2; exit 2; }
core=$(cd "$1" && pwd -P)
selected=$(cd "$2" && pwd -P)
test -f "$core/tests/common/native_runtime.rs"
test -f "$selected/ahu-plugin.md"
test ! -L "$core/.ahu"
mkdir -p "$core/.ahu/validation" "$core/.ahu/build"
run=$(mktemp -d "$core/.ahu/validation/harness-conformance.XXXXXX")
mkdir "$run/packages" "$run/stubs"
export CARGO_TARGET_DIR="$core/.ahu/build/harness-conformance"
for harness in codex claude-code antigravity opencode kiro; do
  source_dir="$core/tests/fixtures/harnesses/harness-$harness"
  package="$run/packages/harness-$harness"
  if [ "$harness" = opencode ]; then
    cp -R "$selected" "$package"
  else
    # Linux embeds debug information in executables; keep fixture packages within
    # the core installer’s unchanged 64 MiB per-file limit.
    CARGO_PROFILE_DEV_DEBUG=none CARGO_PROFILE_DEV_STRIP=debuginfo \
      cargo build --locked --manifest-path "$source_dir/Cargo.toml"
    mkdir -p "$package/bin"
    for file in Cargo.toml ahu-plugin.md README.md LICENSE SECURITY.md; do
      cp "$source_dir/$file" "$package/$file"
    done
    cp "$CARGO_TARGET_DIR/debug/ahu-plugin-$harness" "$package/bin/"
  fi
done
# Discovery stubs are not real native clients and never contact model services.
for command in codex claude agy opencode kiro-cli; do
  printf '#!/bin/sh\nif [ "$1" = --version ]; then echo "1.0.0"; exit 0; fi\nexit 99\n' > "$run/stubs/$command"
  chmod 700 "$run/stubs/$command"
done
export PATH="$run/stubs:$PATH"
export AHU_TEST_HARNESS_PACKAGES_DIR="$run/packages"
cargo test --manifest-path "$core/Cargo.toml" --locked \
  --test harness_runtime --test setup_harness_plugins --test native_admission \
  --test project_plugins --test release_otel -- --test-threads=2
printf 'Conformance completed; local build/test data: %s\n' "$run"
