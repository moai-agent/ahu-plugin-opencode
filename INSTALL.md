# Install the opencode harness plugin

Use a reviewed source checkout or source archive of this repository. This candidate
is not published yet. Install ahu 0.11.1 and `opencode` separately; sign in through the
native CLI. Existing native trust and permission settings still apply.

## Build and stage

Run from this repository's root, using its pinned Rust toolchain. For an extracted
source archive, first run `git init` in that directory; the included `.gitignore`
lets the packager verify that `.ahu/` is ignored. No sibling checkout is needed:

```sh
umask 077
plugin_source=$(pwd -P)
test ! -L .ahu
mkdir -p .ahu/build .ahu/packages
export CARGO_TARGET_DIR="$plugin_source/.ahu/build/packages"
cargo run --locked --bin package -- "$plugin_source/.ahu/packages/v0.1.1"
package_dir="$plugin_source/.ahu/packages/v0.1.1/harness-opencode"
ahu plugins digest "$package_dir"
```

The destination must be new. Packaging builds a native Rust executable and includes
its manifest, identity metadata, license and guides. Installation never runs Cargo.
Build for each supported OS/architecture separately; do not copy a foreign binary.
The package digest covers the staged package, not just the binary checksum.

## Bind and activate in a project

In the same shell, choose an existing Git repository, or create a disposable one:

```sh
project="$plugin_source/.ahu/install-rehearsal"
mkdir "$project"
git -C "$project" init
cd "$project"
printf '.ahu/\n' > .gitignore
git add .gitignore
git commit -m 'Ignore repository-local ahu data'
ahu setup --harness-plugin "$package_dir"
git diff --no-index /dev/null .agents/ahu/plugins/harness-opencode.md
```

The diff exits 1 for a new file. In an existing project, add `.ahu/` to its ignore
rules rather than replacing them. Review the disabled binding, package path,
version, digest and CLI compatibility. Canonical absolute paths must not traverse
symlinks. Then commit and install:

```sh
git add .agents/ahu/plugins/harness-opencode.md
git commit -m 'Pin reviewed opencode harness plugin'
ahu plugins install harness-opencode
```

Edit that binding to replace `grants: []` with `grants: [harness-runtime]`.
This authorizes executable native integration; discovery alone does not.
Commit the reviewed grant before activation:

```sh
git add .agents/ahu/plugins/harness-opencode.md
git commit -m 'Grant opencode native runtime access'
ahu plugins enable harness-opencode
git diff -- .agents/ahu/plugins/harness-opencode.md
git add .agents/ahu/plugins/harness-opencode.md
git commit -m 'Enable reviewed opencode runtime'
ahu plugins list
ahu setup
```

In setup select this harness and an available exact model. Review generated agent,
MCP and skill files. Stage only those reviewed files, commit them, then run
`ahu lock --update`, review/commit `.agents/ahu/lock.md` if changed, and run `ahu lock`.
A global Git ignore may hide generated skills; use `git check-ignore -v PATH` and
force-add only the specific reviewed generated file. Never force-add native secrets.
For an existing configured project, preserve its other bindings and agent choices.

## Live smoke

Use the agent name created by setup (replace NAME), with an explicitly reviewed
work-mode grant. Require a real MCP call and review the task's result:

```sh
ahu @NAME --work-mode --prompt 'Call ahu_skills_list through the ahu MCP server, then write smoke-ok followed by a newline into smoke.txt. Do not change other files.' --output json
```

Record the returned task ID. Run `ahu result TASK --output json`; verify completion
and the file in the reported worktree. To test native session continuity, replace
TASK below with that ID and run from the project root:

```sh
umask 077
mkdir -p .ahu/validation
printf '%s\n' 'Call ahu_skills_list again and confirm the existing smoke.txt.' > .ahu/validation/resume-prompt.txt
ahu resume TASK --prompt-file .ahu/validation/resume-prompt.txt --output json
ahu wait TASK --output json
```

Check the same native session and actual tool result. Mere launch or tool discovery
is not a passed MCP test.

Review and preserve useful output before cleanup. Remove only the accepted smoke
artifact from the test worktree, verify `git status --porcelain` is empty there,
then use `ahu cleanup TASK` and `ahu remove TASK` from the project. Do not bypass a
refusal or discard unreviewed work. Native session history is retained natively.

## Update this plugin independently

Finish and clean existing tasks using the old package before replacement. Keep
its source/package available until their resume and cleanup requirements are met.
Build a new version into a new staging directory. Update only this binding's
version, source path and digest to the reviewed package, preserving its grants and
CLI compatibility; commit that binding change. Then run:

```sh
ahu plugins update harness-opencode
ahu lock --update
```

Review/commit shared lock changes if any, run `ahu lock`, and repeat the smoke.
The same ahu executable and unrelated plugins remain installed. A compatible
runtime protocol permits independent updates; a plugin requiring a new core
capability must declare that requirement. Updates never silently rebind old tasks.

## Remove

After reviewing and cleaning tasks that depend on this package:

```sh
ahu plugins disable harness-opencode
ahu plugins remove harness-opencode
ahu plugins list
```

Review whether to delete its committed binding/agents or keep the disabled policy.
Removal does not delete native credentials, histories or unrelated plugins.
