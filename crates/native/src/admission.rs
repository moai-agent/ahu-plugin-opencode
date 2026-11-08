//! Bounded, read-only evidence about native hook integrations. Inspection
//! reads at most LIMIT bytes per file and considers at most MAX_ENTRIES entries.
//! No hook bodies are persisted, interpreted, installed, or copied by
//! inspection.
//!
//! Admission is provider-neutral: executable hook and plugin behavior remains
//! unknown unless covered by a harness invocation profile. Inspection never
//! changes native settings or provider environment variables.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::util::{digest_bytes, display_safe};

const LIMIT: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 256;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    // Retained for the shared wire schema, although this adapter does not consume it.
    #[allow(dead_code)]
    Installed,
    Missing,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Disabled,
    Unknown,
    NotApplicable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    Absent,
    VerifiedDisable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Conformance {
    Untested,
    // Retained for the shared wire schema, although this adapter does not consume it.
    #[allow(dead_code)]
    SourceInspected,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub path: PathBuf,
    pub scope: String,
    pub method: String,
    pub digest: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Component {
    pub name: String,
    pub registration: Registration,
    pub activation: Activation,
    pub isolation: Isolation,
    pub conformance: Conformance,
    pub evidence: Vec<Evidence>,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub harness: String,
    pub native_agent: Option<String>,
    pub profile: Option<crate::compatibility::IsolationProfile>,
    pub cli_version: Option<String>,
    pub version_checked: bool,
    pub components: Vec<Component>,
    pub gaps: Vec<String>,
    pub conformance: String,
    pub headless: HeadlessPolicy,
    pub next_action: String,
}
impl Status {
    /// Add a version observation to the same evidence used for interactive
    /// disclosure. This never gates interactive execution or probes a provider.
    pub fn with_version(mut self, version: Option<&str>) -> Self {
        self.version_checked = true;
        self.cli_version = version.map(str::to_owned);

        if let Err(error) =
            crate::compatibility::check_headless_version(&self.harness, version.unwrap_or(""))
        {
            self.headless.allowed = false;
            self.headless.reasons.push(error.to_string());
            if crate::compatibility::supports(
                &self.harness,
                crate::compatibility::Feature::HeadlessLaunch,
            ) {
                self.next_action = format!(
                    "Use a CLI that reports a parseable semantic version. Interactive execution remains available subject to normal prerequisites. {}",
                    self.next_action
                );
            } else {
                self.next_action = "Use interactive execution; this harness has no validated ahu headless adapter.".into();
            }
        }

        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadlessPolicy {
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub evidence_digest: String,
}

/// Result of the native preference presence probe. Values are never decoded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedPreferences {
    #[default]
    NotApplicable,
    Absent,
    Present,
    Unknown,
}

/// Explicit inputs keep fixture tests independent of the host's configuration.
#[derive(Debug, Clone, Default)]
pub struct Locations {
    pub home: Option<PathBuf>,
    pub overrides: BTreeMap<String, String>,
    pub managed_claude: Option<PathBuf>,
    pub managed_opencode: Option<PathBuf>,
    pub managed_preferences: Option<PathBuf>,
    pub system_codex: Option<PathBuf>,
    pub codex_managed_preferences: ManagedPreferences,
}
fn native_agent(harness: &str) -> Option<&'static str> {
    match harness {
        "opencode" => Some("opencode"),

        _ => None,
    }
}
// Check all existing ancestors before opening: dangling links, linked parent
// directories and special files are uncertainty, never evidence of absence.
fn check_path(path: &Path) -> std::result::Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("relative or unresolved path".into());
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("symlink evidence is unresolved".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("cannot inspect path metadata".into()),
        }
    }
    Ok(())
}
fn read(path: &Path) -> std::result::Result<Option<Vec<u8>>, String> {
    check_path(path)?;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cannot inspect file".into()),
    };
    if !meta.is_file() || meta.len() > LIMIT {
        return Err("not a bounded regular file".into());
    }
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open file")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect opened file")?
        .is_file()
    {
        return Err("not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read file")?;
    if bytes.len() as u64 > LIMIT {
        return Err("file exceeds inspection limit".into());
    }
    Ok(Some(bytes))
}
fn component(name: &str, path: &Path, scope: &str) -> Component {
    Component {
        name: name.into(),
        registration: Registration::Unknown,
        activation: Activation::Unknown,
        isolation: Isolation::Unknown,
        conformance: Conformance::Untested,
        evidence: vec![Evidence {
            path: path.into(),
            scope: scope.into(),
            method: "bounded native configuration inspection".into(),
            digest: None,
        }],
        detail: "unverified native component".into(),
    }
}
fn absent(c: &mut Component) {
    c.registration = Registration::Missing;
    c.activation = Activation::NotApplicable;
    c.isolation = Isolation::Absent;
    c.detail = "no registration in inspected scope".into();
}
fn file_bytes(c: &mut Component) -> Option<Vec<u8>> {
    match read(&c.evidence[0].path) {
        Ok(None) => {
            absent(c);
            None
        }
        Ok(Some(bytes)) => {
            c.evidence[0].digest = Some(digest_bytes(&bytes));
            Some(bytes)
        }
        Err(reason) => {
            c.detail = reason;
            None
        }
    }
}

pub fn inspect_in(repo: &Path, harness: &str, locations: &Locations) -> Status {
    let mut components = Vec::new();
    let gaps = vec!["Configuration evidence only; native trust, future surface activation and live delivery are unverified.".into(),
        "Arbitrary hooks, remote plugins and configuration indirection are not interpreted. Same-user concurrent changes are not isolated.".into()];
    let Some(home) = &locations.home else {
        components.push(component("home", Path::new("HOME"), "user"));
        return finish(harness, components, gaps);
    };
    let relevant: &[&str] = match harness {
        "opencode" => &[
            "XDG_CONFIG_HOME",
            "OPENCODE_CONFIG",
            "OPENCODE_CONFIG_DIR",
            "OPENCODE_CONFIG_CONTENT",
            "OPENCODE_AUTH_CONTENT",
            "OPENCODE_TEST_MANAGED_CONFIG_DIR",
            "OPENCODE_TEST_HOME",
            "OPENCODE_DB",
            "XDG_DATA_HOME",
        ],

        _ => &[],
    };
    for key in relevant {
        if locations.overrides.contains_key(*key) {
            let mut c = component(key, Path::new(key), "environment override");
            c.detail = "custom native configuration scope is not verified; inspect it explicitly or use the default scope".into();
            components.push(c);
        }
    }
    match harness {
        "opencode" => {
            scan_opencode_sources(locations, &mut components);
            for (base, scope) in [
                (home.join(".config/opencode"), "user"),
                (home.join(".opencode"), "user legacy"),
                (repo.join(".opencode"), "project"),
            ] {
                scan_directory(&base.join("plugins"), scope, true, &mut components);
                scan_directory(&base.join("plugin"), scope, true, &mut components);
                for name in ["opencode.json", "opencode.jsonc"] {
                    scan_plugin_config(&base.join(name), scope, &mut components);
                }
            }
            for name in ["opencode.json", "opencode.jsonc"] {
                scan_plugin_config(&repo.join(name), "project", &mut components);
            }
        }

        _ => components.push(component("unsupported harness", repo, "unknown")),
    }
    // Native project discovery may walk upward. Bound that search and include
    // parent settings rather than treating a clean leaf as a clean scope.
    for (depth, parent) in repo.ancestors().skip(1).enumerate() {
        if depth >= 32 || components.len() > MAX_ENTRIES {
            let mut c = component("ancestor scope", parent, "project ancestor");
            c.detail = "ancestor inspection limit exceeded".into();
            components.push(c);
            break;
        }
        if harness == "opencode" {
            for name in ["plugin", "plugins"] {
                scan_directory(
                    &parent.join(".opencode").join(name),
                    "project ancestor",
                    true,
                    &mut components,
                );
            }
            for name in ["opencode.json", "opencode.jsonc"] {
                scan_plugin_config(&parent.join(name), "project ancestor", &mut components);
                scan_plugin_config(
                    &parent.join(".opencode").join(name),
                    "project ancestor",
                    &mut components,
                );
            }
        }
    }
    for file in [".mcp.json", ".agents/settings.json"] {
        let path = repo.join(file);
        let mut c = component("additional integration scope", &path, "project");
        if let Some(bytes) = file_bytes(&mut c) {
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(value)
                    if value.is_object()
                        && (file == ".mcp.json"
                            || value.as_object().is_some_and(serde_json::Map::is_empty)) =>
                {
                    absent(&mut c);
                    c.detail = "no executable hook policy in inspected scope; MCP subprocess behavior remains native and outside inspection".into();
                }
                Ok(_) => {
                    c.detail =
                        "unsupported executable configuration scope; behavior is unknown".into()
                }
                Err(_) => c.detail = "malformed integration configuration".into(),
            }
        }
        components.push(c);
    }
    finish(harness, components, gaps)
}

// Reviewed against the installed 2.28.0 and 2.29.0 V3 KAS profile loaders.
// Only the explicitly inspected declarative subset is admitted.
// No arbitrary executable source is certified by its filename or contents.

// Deliberately narrower than the native schema: no executable hooks, knowledge
// indexing, Powers activation, or unknown control fields. Permissions and MCP
// are native policy, not a claim that subprocesses are isolated.

fn finish(harness: &str, mut components: Vec<Component>, gaps: Vec<String>) -> Status {
    // A native user path can also be reached by project ancestor discovery.
    // Retain one observation, without collapsing duplicate hook registrations
    // (their numbered names differ).
    let mut seen = std::collections::BTreeSet::new();
    components.retain(|c| seen.insert((c.name.clone(), c.evidence[0].path.clone())));
    let reasons: Vec<String> = components
        .iter()
        .filter(|c| matches!(c.isolation, Isolation::Unknown))
        .map(|c| {
            format!(
                "{} ({}): {}",
                display_safe(&c.name),
                display_safe(&c.evidence[0].path.to_string_lossy()),
                display_safe(&c.detail)
            )
        })
        .collect();
    let headless = HeadlessPolicy {
        allowed: reasons.is_empty(),
        reasons,
        evidence_digest: digest_bytes(
            &serde_json::to_vec(&components).expect("component serialization"),
        ),
    };
    Status { profile: crate::compatibility::isolation_profile(harness), cli_version: None, version_checked: false, harness: harness.into(), native_agent: native_agent(harness).map(str::to_string),
        components, gaps, conformance: "locally_inspected; live_untested".into(), headless,
        next_action: "Review unresolved native configuration with its owner; use interactive execution or explicit --work-mode when native compatibility is unverified.".into() }
}

/// Presence-only inspection for credentials, opaque native stores and legacy
/// formats. Never open these files, hash their contents, or invoke their loader.
fn opaque_source(path: &Path, name: &str, scope: &str, reason: &str, out: &mut Vec<Component>) {
    let mut c = component(name, path, scope);
    c.evidence[0].method = "metadata only; contents not read".into();
    match check_path(path).and_then(|()| match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("cannot inspect native source metadata".into()),
    }) {
        Ok(false) => absent(&mut c),
        Ok(true) => c.detail = reason.into(),
        Err(reason) => c.detail = reason,
    }
    out.push(c);
}

fn source_directory(path: &Path, name: &str, out: &mut Vec<Component>) -> Vec<PathBuf> {
    let mut c = component(name, path, "native source discovery");
    c.evidence[0].method = "bounded directory names only; file contents not read".into();
    if let Err(reason) = check_path(path) {
        c.detail = reason;
        out.push(c);
        return vec![];
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            absent(&mut c);
            out.push(c);
            return vec![];
        }
        Err(_) => {
            c.detail = "cannot enumerate native source scope".into();
            out.push(c);
            return vec![];
        }
    };
    let mut paths = Vec::new();
    for entry in entries.take(MAX_ENTRIES + 1) {
        let Ok(entry) = entry else {
            c.detail = "cannot inspect native source entry".into();
            out.push(c);
            return vec![];
        };
        paths.push(entry.path());
    }
    if paths.len() > MAX_ENTRIES {
        c.detail = "native source enumeration limit exceeded".into();
        out.push(c);
        return vec![];
    }
    paths.sort();
    paths
}

// OpenCode v1.18.29-v1.18.32 config/config.ts, config/managed.ts,
// auth/index.ts and account/repo.ts. Native authentication can cause remote
// config to add plugins; authentication values and account databases stay native.
fn scan_opencode_sources(locations: &Locations, out: &mut Vec<Component>) {
    let home = locations.home.as_ref().expect("home checked by caller");
    let config = home.join(".config/opencode");
    scan_plugin_config(&config.join("config.json"), "user defaults", out);
    opaque_source(
        &config.join("config"),
        "legacy config",
        "user",
        "legacy native config may declare plugins; its migration/loader is not executed by inspection",
        out,
    );
    let data = home.join(".local/share/opencode");
    opaque_source(
        &data.join("auth.json"),
        "authenticated remote config",
        "user native authentication",
        "authentication store present; remote plugin configuration is unresolved. Credentials are not read and no remote context is fetched; use interactive execution",
        out,
    );
    for path in source_directory(&data, "native account store discovery", out) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with("opencode")
            && [".db", ".db-wal", ".db-shm"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        {
            opaque_source(
                &path,
                "authenticated account config",
                "native account database",
                "account database may select authenticated remote configuration; database and tokens are not read, so headless isolation is unresolved",
                out,
            );
        }
    }
    if let Some(managed) = &locations.managed_opencode {
        for name in ["opencode.json", "opencode.jsonc"] {
            scan_plugin_config(&managed.join(name), "system managed", out);
        }
    }
    if let Some(preferences) = &locations.managed_preferences {
        let domain = "ai.opencode.managed.plist";
        opaque_source(
            &preferences.join(domain),
            "managed preferences",
            "system managed",
            "native managed preferences may declare executable plugins; plist decoding is unsupported",
            out,
        );
        for user in source_directory(preferences, "managed preference domains", out) {
            // The native loader selects a username via the OS, not HOME/USER.
            // Check all bounded candidate domains conservatively, without
            // reading any user's managed preference contents.
            match std::fs::symlink_metadata(&user) {
                Ok(meta) if meta.is_dir() || meta.file_type().is_symlink() => opaque_source(
                    &user.join(domain),
                    "managed user preferences",
                    "system managed",
                    "potential user managed preference source is unresolved; contents are not read",
                    out,
                ),
                Ok(_) => {}
                Err(_) => opaque_source(
                    &user,
                    "managed preference entry",
                    "system managed",
                    "managed source metadata unavailable",
                    out,
                ),
            }
        }
    }
}

// Codex rust-v0.154.0 and rust-v0.155.1 hooks/engine/discovery.rs and
// config/loader: native sources include TOML, plugins, managed and cloud layers.

fn scan_directory(path: &Path, scope: &str, plugins: bool, out: &mut Vec<Component>) {
    let mut c = component(
        if plugins {
            "plugin directory"
        } else {
            "managed fragments"
        },
        path,
        scope,
    );
    if let Err(reason) = check_path(path) {
        c.detail = reason;
        out.push(c);
        return;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            absent(&mut c);
            out.push(c);
            return;
        }
        Err(_) => {
            c.detail = "cannot inspect directory".into();
            out.push(c);
            return;
        }
    };
    let mut paths = Vec::new();
    for entry in entries.take(MAX_ENTRIES + 1) {
        let Ok(entry) = entry else {
            c.detail = "cannot inspect directory entry".into();
            out.push(c);
            return;
        };
        paths.push(entry.path());
    }
    if paths.len() > MAX_ENTRIES {
        c.detail = "directory inspection limit exceeded".into();
        out.push(c);
        return;
    }
    paths.sort();
    if paths.is_empty() {
        absent(&mut c);
        out.push(c);
        return;
    }
    for path in paths {
        let mut item = component("native plugin", &path, scope);
        if file_bytes(&mut item).is_some() {
            item.detail = "unreviewed native code; executable behavior is unknown".into();
        }
        out.push(item);
    }
}
fn scan_plugin_config(path: &Path, scope: &str, out: &mut Vec<Component>) {
    let mut c = component("declared plugins", path, scope);
    if let Some(bytes) = file_bytes(&mut c) {
        // Native substitution runs before JSON parsing. Do not read expansion
        // targets or inspect environment values: even valid literal JSON may
        // expand into a different executable configuration.
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("{env:") || text.contains("{file:") {
            c.detail = "native environment/file substitution is unresolved; expansion targets are not read".into();
            out.push(c);
            return;
        }
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value)
                if value.is_object()
                    && value
                        .get("plugin")
                        .is_none_or(|p| p.as_array().is_some_and(Vec::is_empty))
                    && value.get("plugins").is_none() =>
            {
                absent(&mut c);
            }
            Ok(_) => c.detail =
                "declared plugin or unsupported native configuration; module behavior unresolved"
                    .into(),
            Err(_) => {
                c.detail = "malformed JSON or unsupported JSONC; not evidence of absence".into()
            }
        }
    }
    out.push(c);
}

/// Apply only controls that batch_command also freezes and emits. Inspection
/// without this explicit headless profile retains its interactive evidence.
pub fn headless_status(mut status: Status, version: &str) -> Status {
    if status.harness == "claude-code"
        && let Some(profile) = crate::isolation::profile(&status.harness, version)
    {
        for component in &mut status.components {
            let evidence = &component.evidence[0];
            if status.harness == "claude-code"
                && matches!(
                    evidence.scope.as_str(),
                    "user" | "project" | "project local" | "project ancestor"
                )
                && evidence.digest.is_some()
                && component.name.contains(" registration ")
                && component.isolation == Isolation::Unknown
            {
                component.isolation = Isolation::VerifiedDisable;
                component.activation = Activation::Disabled;
                component.detail = format!(
                    "disabled by invocation profile {}; managed policy is retained",
                    profile.id
                );
            }
        }
        status = finish(&status.harness, status.components, status.gaps);
        status.gaps.push(format!("Headless invocation profile: {}. Managed hooks and unresolved plugins remain admission blockers.", profile.id));
    }
    status.with_version(Some(version))
}

pub(crate) fn trust_only_opencode_account_state(status: &mut Status, home: &Path) -> bool {
    let unresolved = status
        .components
        .iter()
        .filter(|component| matches!(component.isolation, Isolation::Unknown))
        .collect::<Vec<_>>();
    if unresolved.is_empty()
        || unresolved.iter().any(|component| {
            !matches!(
                component.name.as_str(),
                "authenticated account config" | "authenticated remote config"
            ) || component.evidence.len() != 1
                || !matches!(
                    component.evidence[0].scope.as_str(),
                    "native account database" | "user native authentication"
                )
                || component.evidence[0].method != "metadata only; contents not read"
                || !opencode_auth_path(home, &component.evidence[0].path)
        })
    {
        return false;
    }
    status.headless.allowed = true;
    status.headless.reasons.clear();
    status.gaps.push(
        "Project policy explicitly trusts OpenCode native account/authentication state for headless execution. It may select authenticated remote configuration and executable plugins; ahu does not inspect, disable, or constrain that state. Credentials and database contents remain unread.".into(),
    );
    status.next_action =
        "OpenCode native account/authentication state is explicitly trusted by project policy; review it with OpenCode's native controls.".into();
    true
}

fn opencode_auth_path(home: &Path, path: &Path) -> bool {
    let root = home.join(".local/share/opencode");
    if !path.starts_with(&root) {
        return false;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let native_name = name == "auth.json"
        || (name.starts_with("opencode")
            && [".db", ".db-wal", ".db-shm"]
                .iter()
                .any(|suffix| name.ends_with(suffix)));
    native_name
        && std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}
