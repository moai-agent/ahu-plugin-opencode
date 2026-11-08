use serde::Deserialize;
use serde_json::{Value, json};
const PROMPT: &str = "<prompt: see prompt.txt>";
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Interactive,
    Headless,
    Direct,
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Permissions {
    Prompt,
    AcceptEdits,
    Auto,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    mode: Mode,
    model: String,
    permissions: Permissions,
    work_mode: bool,
    session: Option<String>,
    native_version: String,
    timeout_seconds: u64,
    cwd: String,
    // Retained for the shared wire schema, although this adapter does not consume it.
    #[allow(dead_code)]
    broker_dir: Option<String>,
    // Retained for the shared wire schema, although this adapter does not consume it.
    #[allow(dead_code)]
    native_profile: Option<Value>,
    native_controls: Vec<String>,
}
fn identity(value: &str, limit: usize, alphabet: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || alphabet.contains(&b))
}
pub(super) fn plan(harness: &str, params: Value) -> Result<Value, ()> {
    let r: Request = serde_json::from_value(params).map_err(|_| ())?;
    if !identity(&r.model, 256, b"-_.:/+")
        || !(1..=604800).contains(&r.timeout_seconds)
        || r.session.as_ref().is_some_and(|s| !identity(s, 128, b"-_"))
        || !std::path::Path::new(&r.cwd).is_absolute()
        || r.cwd.contains('\0')
        || r.work_mode && r.permissions != Permissions::Auto
        || r.mode != Mode::Headless && (r.session.is_some() || r.work_mode)
        || r.native_controls.len() > 128
    {
        return Err(());
    }
    // Core enforces explicit project pins. Unpinned native versions float;
    // version-specific controls still require their inspected admission evidence.
    let version = ahu_harness_native::version_token(&r.native_version).ok_or(())?;
    let standalone = version.starts_with("2.");
    if harness == "opencode"
        && !r
            .model
            .split_once('/')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
    {
        return Err(());
    }
    let program = harness;
    let mut args: Vec<String> = Vec::new();
    let mut add = |values: &[&str]| args.extend(values.iter().map(|s| (*s).to_owned()));
    if r.mode == Mode::Direct && r.permissions == Permissions::Auto {
        match harness {
            // The reviewed mini CLI has no unattended permission override.
            "opencode" if standalone => return Err(()),
            "opencode" => add(&["--model", &r.model, "--auto"]),
            _ => return Err(()),
        }
        return Ok(
            json!({"program":program,"args":args,"prompt_arg":null,"native_eval_otlp":harness != "kiro"}),
        );
    }
    if r.mode == Mode::Headless {
        match harness {
            "opencode" => {
                add(&["run"]);
                if standalone {
                    add(&["--standalone"]);
                }
                add(&["--format", "json", "--model", &r.model]);
                if !standalone {
                    add(&["--dir", &r.cwd]);
                }
                match r.permissions {
                    Permissions::Prompt => (),
                    Permissions::Auto => add(&["--auto"]),
                    Permissions::AcceptEdits => return Err(()),
                }
                if let Some(session) = &r.session {
                    add(&["--session", session]);
                }
                add(&["--"]);
            }

            _ => return Err(()),
        }
    } else {
        match harness {
            "opencode" => {
                if r.permissions == Permissions::AcceptEdits
                    || standalone && r.permissions == Permissions::Auto
                {
                    return Err(());
                }
                if standalone {
                    add(&["mini"]);
                }
                add(&["--model", &r.model]);
                if r.permissions == Permissions::Auto {
                    add(&["--auto"]);
                }
                if r.mode == Mode::Interactive {
                    add(&["--prompt"]);
                }
            }

            _ => return Err(()),
        }
    }
    let prompt_arg = if r.mode == Mode::Direct {
        None
    } else {
        let i = args.len();
        args.push(PROMPT.into());
        Some(i)
    };
    Ok(
        json!({"program":program,"args":args,"prompt_arg":prompt_arg,"native_eval_otlp":harness != "kiro"}),
    )
}
