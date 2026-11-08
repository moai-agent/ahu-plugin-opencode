# Compatibility and evidence

- Package: `harness-opencode` 0.1.1; runtime protocol 1; host API 1.
- Initial compatible CLI: ahu 0.11.1. Repository bindings currently pin an exact
  CLI version; protocol compatibility is a separate check, not a wildcard promise.
- Native CLI: `opencode`; install and authenticate it independently.
- The descriptor's tested native versions are historical implementation evidence,
  not an allowlist or a claim that every version/mode has passed live checks.
- Direct, interactive and headless modes have different native controls. Preserve
  native trust prompts and account boundaries; do not manufacture compatibility
  by changing native controls. Kiro requires its native defaults/workspace trust.
- Each independently released candidate requires local conformance verification.
  Do not publish until protocol, fixture, real MCP/OTel, eval, resume, cancellation,
  cleanup and selective-update evidence has been reviewed for advertised modes.

Missing usage/model fields remain unknown. A worktree and harness approval mode
are not an OS sandbox. Follow SECURITY.md for executable and capture boundaries.
