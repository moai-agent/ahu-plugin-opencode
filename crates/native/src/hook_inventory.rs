//! Harness hooks: shell commands the harness runs on its own lifecycle events.
//!
//! ahu reports visible hooks with their scope, event, and digest. Hooks outside
//! shared project policy raise a consistency warning because they can differ
//! between teammates. Local repository settings travel with the snapshot; home
//! and managed settings remain at their native locations.
//!
//! ahu never adds, edits, removes, or disables hooks. Hook and approval-settings
//! inspection covers Claude Code; other harnesses report unknown coverage.
//! Hook commands are not serialized because they can contain credentials.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::util::Result;

/// Where a hook is configured, which decides whether it is project policy and
/// whether it travels into a task worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// `<repo>/.claude/settings.json` — shared project policy.
    Project,
    /// `<repo>/.claude/settings.local.json` — in the repository, but typically
    /// ignored by Git, so not necessarily shared or reviewed.
    ProjectLocal,
    /// `~/.claude/settings.json` — this machine's user only.
    User,
    /// Machine policy. Required, and not removable by ahu.
    Managed,
}

/// One configured hook command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hook {
    /// Harness lifecycle event, e.g. `PreToolUse`.
    pub event: String,
    /// Matcher the event is filtered by, when the event supports one.
    pub matcher: Option<String>,
    /// `command` for a shell hook; anything else is recorded as declared.
    pub kind: String,
    /// The shell command, when this is a command hook.
    ///
    /// Never serialized: hook commands routinely embed credentials, and task
    /// records are ordinary files. `command_digest` identifies the command for
    /// drift detection without storing it.
    #[serde(skip)]
    pub command: Option<String>,
    /// Digest of the command, which is what gets persisted and compared.
    #[serde(default)]
    pub command_digest: String,
    pub scope: Scope,
    /// Settings file the hook is declared in, repository-relative when inside
    /// the repository and absolute otherwise.
    pub source: String,
}

/// What a settings file says about the session's approval boundary.
///
/// Native settings can affect approvals independently of manifest flags, so
/// the inventory reports their relevant declarations without inferring an
/// effective boundary.
///
/// Values are only ever repeated for keys whose values are policy. `env` is the
/// exception: its values are routinely credentials, so only the names are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals_widened: Option<bool>,
    /// Settings file this came from, labelled as the hook scan labels it.
    pub source: String,
    pub scope: Scope,
    /// `permissions.defaultMode`, e.g. `bypassPermissions`.
    pub default_mode: Option<String>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub ask: Vec<String>,
    pub additional_directories: Vec<String>,
    pub enabled_plugins: Vec<String>,
    pub enable_all_project_mcp_servers: Option<bool>,
    pub enabled_mcpjson_servers: Vec<String>,
    /// Names only. A settings `env` block routinely holds API tokens.
    pub env_names: Vec<String>,
    /// Top-level keys ahu does not interpret, named rather than silently
    /// dropped — the same treatment the hook scan already gives a shape it
    /// cannot walk.
    pub uninterpreted_keys: Vec<String>,
}

impl SettingsFacts {
    fn new(source: &str, scope: Scope) -> Self {
        SettingsFacts {
            approvals_widened: None,
            source: source.to_string(),
            scope,
            default_mode: None,
            allow: Vec::new(),
            deny: Vec::new(),
            ask: Vec::new(),
            additional_directories: Vec::new(),
            enabled_plugins: Vec::new(),
            enable_all_project_mcp_servers: None,
            enabled_mcpjson_servers: Vec::new(),
            env_names: Vec::new(),
            uninterpreted_keys: Vec::new(),
        }
    }

    /// Whether this file says anything about the approval boundary at all.
    pub fn is_empty(&self) -> bool {
        self.default_mode.is_none()
            && self.allow.is_empty()
            && self.deny.is_empty()
            && self.ask.is_empty()
            && self.additional_directories.is_empty()
            && self.enabled_plugins.is_empty()
            && self.enable_all_project_mcp_servers.is_none()
            && self.enabled_mcpjson_servers.is_empty()
            && self.env_names.is_empty()
            && self.uninterpreted_keys.is_empty()
    }

    /// Whether this file widens what the harness would otherwise ask about.
    ///
    /// Deliberately generous: anything that pre-approves tools, adds a directory
    /// the session may reach, or removes a trust prompt counts.
    pub fn widens_approvals(&self) -> bool {
        matches!(
            self.default_mode.as_deref(),
            Some("bypassPermissions") | Some("acceptEdits") | Some("auto") | Some("dontAsk")
        ) || !self.allow.is_empty()
            || !self.additional_directories.is_empty()
            || self.enable_all_project_mcp_servers == Some(true)
            || !self.enabled_mcpjson_servers.is_empty()
            || !self.enabled_plugins.is_empty()
    }
}

/// An MCP server a repository asks the harness to start.
///
/// A repository MCP declaration is copied with configuration and reported as a
/// potential subprocess source. Whether it runs depends on the selected harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    pub source: String,
    pub name: String,
    /// The command as declared, including its arguments. This is a process the
    /// session will start, so it is shown whole rather than reduced to a program
    /// name — unlike a hook label, an MCP `command` is not a place credentials
    /// conventionally live, and the reader needs to see `sh -c curl … | sh`.
    pub command: String,
}

/// A plugin module an `opencode.json` declares.
///
/// OpenCode installs and runs these at startup, so a name here is executable
/// configuration the reader is trusting, not an inert setting. ahu records the
/// module string exactly as declared — it may be an npm specifier that resolves
/// to code ahu never sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredPlugin {
    pub source: String,
    pub module: String,
}

/// Every hook ahu could find, plus an honest account of what it could not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookInventory {
    pub hooks: Vec<Hook>,
    /// What the settings files say beyond their `hooks` key.
    #[serde(default)]
    pub settings: Vec<SettingsFacts>,
    /// MCP servers declared by the repository's `.mcp.json`.
    #[serde(default)]
    pub mcp_servers: Vec<McpServer>,
    /// Plugin modules declared by the repository's `opencode.json(c)`.
    ///
    /// Skipped when empty so a record written by a build without this field and
    /// a record written by one with it are byte-identical. Task records are
    /// frozen and digested; a new always-present `[]` would change the digest of
    /// every existing record that declares no plugin, which would read as
    /// configuration drift where nothing about the configuration moved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared_plugins: Vec<DeclaredPlugin>,
    /// Settings files that exist but could not be parsed. Their hooks are
    /// unknown, which is reported rather than treated as "none".
    pub unreadable: Vec<String>,
    /// Settings files ahu looked for and did not find. Used to say which scopes
    /// were actually checked.
    pub checked: Vec<String>,
    /// Set when ahu has no implementation of this launch's harness hook surface,
    /// so "none found" would be a false negative rather than a result.
    #[serde(default)]
    pub unscanned_harness: Option<String>,
}

/// Where the non-repository settings files live on this machine.
///
/// Taken as a parameter rather than read from the environment inside the scan so
/// the whole hook inventory is testable without mutating process-wide state.
#[derive(Debug, Clone, Default)]
pub struct Locations {
    /// The user's home directory, holding `~/.claude/settings.json`.
    pub home: Option<PathBuf>,
}

/// Whether ahu has an implementation of a harness's hook configuration.
///
/// Harnesses without an implemented hook inventory (e.g. Codex, Antigravity)
/// produce an explicit unknown-coverage report rather than a misleading empty
/// inventory.
pub fn hook_surface_is_implemented(harness_id: &str) -> bool {
    matches!(harness_id, "claude-code" | "opencode")
}

/// Read hooks and settings using explicit machine locations.
pub fn collect_for(
    repo_root: &Path,
    harness_id: &str,
    locations: &Locations,
) -> Result<HookInventory> {
    let mut inventory = HookInventory::default();
    // `.mcp.json` is read for every harness: it is repository configuration that
    // travels into the task worktree either way. Whether a declared server
    // starts depends on the selected harness.
    collect_mcp_servers(repo_root, &mut inventory);
    collect_mcp_file(repo_root, ".kiro/settings/mcp.json", &mut inventory);
    // Repository OpenCode plugins and configuration travel into the task worktree whatever
    // the harness is, and its plugin entries are modules OpenCode installs and
    // executes at startup.
    collect_opencode_plugins_in_repo(repo_root, &mut inventory);
    if !hook_surface_is_implemented(harness_id) {
        inventory.unscanned_harness = Some(harness_id.to_string());
        return Ok(inventory);
    }

    {
        collect_opencode_user_config(locations, &mut inventory);
    }

    inventory
        .hooks
        .sort_by(|a, b| (a.scope, &a.event, &a.matcher).cmp(&(b.scope, &b.event, &b.matcher)));
    inventory
        .declared_plugins
        .sort_by(|a, b| (&a.source, &a.module).cmp(&(&b.source, &b.module)));
    inventory.declared_plugins.dedup();
    inventory.checked.sort();
    inventory.checked.dedup();
    inventory.unreadable.sort();
    inventory.unreadable.dedup();
    Ok(inventory)
}

/// Record the plugin modules an `opencode.json` or `opencode.jsonc` declares,
/// as well as local `.opencode/plugin/*` modules in the repository.
///
/// These are named, not resolved: OpenCode installs them itself, from wherever
/// the specifier points. Reporting the names is the whole point — the snapshot
/// already carries the file and digests it, but a digest of a file naming a
/// remote module tells the reader nothing about what will run, and an npm
/// specifier is not a file the executable-bit scan can see.
///
/// A `.jsonc` with comments will not parse as JSON. That is recorded as
/// unreadable rather than as "no plugins", because the difference matters.
fn collect_opencode_plugins_in_repo(repo_root: &Path, inventory: &mut HookInventory) {
    let project_files = [
        ("opencode.json", Scope::Project),
        ("opencode.jsonc", Scope::Project),
        (".opencode/opencode.json", Scope::Project),
        (".opencode/opencode.jsonc", Scope::Project),
    ];
    for (source, scope) in project_files {
        let path = match crate::util::resolve_existing_within(repo_root, source) {
            Ok(Some(path)) => path,
            Ok(None) => continue,
            Err(_) => {
                inventory.unreadable.push(format!(
                    "{source} (not read: it or one of its parent directories is a symlink, so \
                     its plugin declarations are not this repository's)"
                ));
                continue;
            }
        };
        let Ok(bytes) = read_inventory_file(&path) else {
            inventory.unreadable.push(source.to_string());
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            inventory.unreadable.push(format!(
                "{source} (not valid JSON to ahu, so its plugin declarations are unknown rather \
                 than absent; JSONC comments are not parsed)"
            ));
            continue;
        };
        inventory.checked.push(source.to_string());
        let facts = parse_opencode_settings(&value, scope, source);
        if !facts.is_empty() {
            inventory.settings.push(facts);
        }
        collect_mcp_from_value(&value, source, inventory);
        if let Some(plugins) = value.get("plugin").or_else(|| value.get("plugins")) {
            if let Some(plugins) = plugins.as_array() {
                for module in plugins {
                    inventory.declared_plugins.push(DeclaredPlugin {
                        source: source.to_string(),
                        module: module
                            .as_str()
                            .unwrap_or("(non-string plugin entry)")
                            .to_string(),
                    });
                }
            } else {
                inventory.unreadable.push(format!(
                    "{source} (its plugin key is not a shape ahu understands, so its plugins are \
                     unknown rather than absent)"
                ));
            }
        }
    }

    // Repository local plugin directories and files:
    for dir in [".opencode/plugin", ".opencode/plugins"] {
        let resolved = match crate::util::resolve_existing_within(repo_root, dir) {
            Ok(Some(path)) => path,
            Ok(None) => continue,
            Err(_) => {
                inventory.unreadable.push(format!(
                    "{dir} (not read: it or one of its parent directories is a symlink, so \
                     its plugins are not this repository's)"
                ));
                continue;
            }
        };
        if resolved.is_dir() {
            match std::fs::read_dir(&resolved) {
                Ok(entries) => {
                    let mut files = Vec::new();
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let rel = path
                                .strip_prefix(repo_root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();
                            files.push(rel);
                        }
                    }
                    files.sort();
                    for file in files {
                        inventory.checked.push(file.clone());
                        inventory.declared_plugins.push(DeclaredPlugin {
                            source: file.clone(),
                            module: file,
                        });
                    }
                }
                Err(_) => {
                    inventory.unreadable.push(dir.to_string());
                }
            }
        } else if resolved.is_file() {
            inventory.checked.push(dir.to_string());
            inventory.declared_plugins.push(DeclaredPlugin {
                source: dir.to_string(),
                module: dir.to_string(),
            });
        }
    }

    for single in [
        ".opencode/plugin.js",
        ".opencode/plugin.ts",
        ".opencode/plugins.js",
        ".opencode/plugins.ts",
    ] {
        if let Ok(Some(path)) = crate::util::resolve_existing_within(repo_root, single) {
            if !path.is_file() {
                continue;
            }
            inventory.checked.push(single.to_string());
            inventory.declared_plugins.push(DeclaredPlugin {
                source: single.to_string(),
                module: single.to_string(),
            });
        }
    }
}

/// User-level OpenCode configuration and plugins.
fn collect_opencode_user_config(locations: &Locations, inventory: &mut HookInventory) {
    let Some(home) = &locations.home else {
        return;
    };
    let user_configs = [
        home.join(".config/opencode/opencode.json"),
        home.join(".config/opencode/opencode.jsonc"),
        home.join(".opencode/opencode.json"),
        home.join(".opencode/opencode.jsonc"),
    ];
    for path in user_configs {
        if !path.exists() {
            continue;
        }
        let display = path.to_string_lossy().to_string();
        let Ok(bytes) = read_inventory_file(&path) else {
            inventory.unreadable.push(display);
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            inventory.unreadable.push(format!(
                "{display} (not valid JSON to ahu, so its plugin declarations are unknown rather \
                 than absent; JSONC comments are not parsed)"
            ));
            continue;
        };
        inventory.checked.push(display.clone());
        let facts = parse_opencode_settings(&value, Scope::User, &display);
        if !facts.is_empty() {
            inventory.settings.push(facts);
        }
        collect_mcp_from_value(&value, &display, inventory);
        if let Some(plugins) = value.get("plugin").or_else(|| value.get("plugins")) {
            if let Some(plugins) = plugins.as_array() {
                for module in plugins {
                    inventory.declared_plugins.push(DeclaredPlugin {
                        source: display.clone(),
                        module: module
                            .as_str()
                            .unwrap_or("(non-string plugin entry)")
                            .to_string(),
                    });
                }
            } else {
                inventory.unreadable.push(format!(
                    "{display} (its plugin key is not a shape ahu understands, so its plugins are \
                     unknown rather than absent)"
                ));
            }
        }
    }

    let user_plugin_dirs = [
        home.join(".config/opencode/plugin"),
        home.join(".config/opencode/plugins"),
        home.join(".opencode/plugin"),
        home.join(".opencode/plugins"),
    ];
    for dir in user_plugin_dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut files = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    files.push(path.to_string_lossy().to_string());
                }
            }
            files.sort();
            for file in files {
                inventory.checked.push(file.clone());
                inventory.declared_plugins.push(DeclaredPlugin {
                    source: file.clone(),
                    module: file,
                });
            }
        }
    }
}

fn collect_mcp_servers(repo_root: &Path, inventory: &mut HookInventory) {
    collect_mcp_file(repo_root, ".mcp.json", inventory);
}

fn collect_mcp_file(repo_root: &Path, source: &str, inventory: &mut HookInventory) {
    let path = match crate::util::resolve_existing_within(repo_root, source) {
        Ok(Some(path)) => path,
        Ok(None) => return,
        Err(_) => {
            inventory.unreadable.push(format!(
                "{source} (not read: it or one of its parent directories is a symlink, so its \
                 MCP servers are not this repository's)"
            ));
            return;
        }
    };
    let Ok(bytes) = read_inventory_file(&path) else {
        inventory.unreadable.push(source.to_string());
        return;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        inventory.unreadable.push(source.to_string());
        return;
    };
    if value
        .get("mcpServers")
        .or_else(|| value.get("mcp"))
        .is_some_and(|servers| !servers.is_object())
    {
        inventory.unreadable.push(format!(
            "{source} (its mcpServers key is not a shape ahu understands, so its servers are \
             unknown rather than absent)"
        ));
        return;
    }
    collect_mcp_from_value(&value, source, inventory);
}

fn collect_mcp_from_value(value: &serde_json::Value, source: &str, inventory: &mut HookInventory) {
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("mcp"))
        .and_then(|s| s.as_object());
    let Some(servers) = servers else {
        return;
    };
    for (name, server) in servers {
        let command = server
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("(no command declared)");
        let args = server
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().unwrap_or("(non-string)").to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        inventory.mcp_servers.push(McpServer {
            source: source.to_string(),
            name: name.clone(),
            command: if args.is_empty() {
                command.to_string()
            } else {
                format!("{command} {args}")
            },
        });
    }
}

/// Top-level OpenCode settings keys ahu interprets. Anything else is named as unknown.
const OPENCODE_INTERPRETED_KEYS: &[&str] = &[
    "plugin",
    "plugins",
    "permission",
    "permissions",
    "mcp",
    "mcpServers",
    "env",
    "model",
    "provider",
    "agent",
    "mode",
    "command",
    "$schema",
    "username",
];

/// Pull the approval-relevant keys out of one OpenCode settings document.
fn parse_opencode_settings(value: &serde_json::Value, scope: Scope, source: &str) -> SettingsFacts {
    let mut facts = SettingsFacts::new(source, scope);
    let Some(object) = value.as_object() else {
        return facts;
    };
    for key in object.keys() {
        if !OPENCODE_INTERPRETED_KEYS.contains(&key.as_str()) {
            facts.uninterpreted_keys.push(key.clone());
        }
    }
    if let Some(permissions) = object
        .get("permissions")
        .or_else(|| object.get("permission"))
    {
        facts.default_mode = permissions
            .get("defaultMode")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        facts.allow = string_list(permissions.get("allow"));
        facts.deny = string_list(permissions.get("deny"));
        facts.ask = string_list(permissions.get("ask"));
        facts.additional_directories = string_list(permissions.get("additionalDirectories"));
        if let Some(map) = permissions.as_object() {
            for (action, val) in map {
                match val.as_str() {
                    Some("allow") => facts.allow.push(action.clone()),
                    Some("deny") => facts.deny.push(action.clone()),
                    Some("ask") => facts.ask.push(action.clone()),
                    _ => {}
                }
            }
        }
    }
    facts.enabled_plugins = string_list(object.get("enabledPlugins"));
    if let Some(env) = object.get("env").and_then(|e| e.as_object()) {
        facts.env_names = env.keys().cloned().collect();
    }
    facts
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| match item.as_str() {
                    Some(text) => text.to_string(),
                    None => item.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// Inventory must not block on native FIFOs or read an unbounded declaration.
fn read_inventory_file(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > crate::files::LIMIT as u64 {
        return Err(std::io::Error::other("native inventory file is invalid"));
    }
    let mut bytes = Vec::new();
    file.take(crate::files::LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > crate::files::LIMIT {
        return Err(std::io::Error::other("native inventory file exceeds limit"));
    }
    Ok(bytes)
}
