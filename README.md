# ahu OpenCode plugin

A Rust runtime adapter that lets [ahu](https://github.com/moai-agent/ahu) launch
and manage tasks through the native `opencode` CLI. This plugin owns native launch,
resume, configuration inspection, and event decoding; ahu owns task state,
worktrees, and process supervision.

Follow the [installation guide](INSTALL.md) to build, stage, activate, update,
and remove the package. You need ahu and an installed, authenticated native CLI.
Source builds require Rust/Cargo and no sibling checkout; installed executables
require neither Cargo nor Python. Native credentials and history stay with OpenCode.

Check [compatibility](COMPATIBILITY.md) for supported versions and modes, and read
[the security notice](SECURITY.md) before granting runtime access. An MCP client
that only connects to `ahu mcp serve` does not need this adapter.

Published versions belong on [GitHub Releases](https://github.com/moai-agent/ahu-plugin-opencode/releases);
initial publication is tracked by [the release checklist](RELEASING.md).
That guide also covers local development checks.
