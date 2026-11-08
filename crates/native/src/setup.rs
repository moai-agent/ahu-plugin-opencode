//! Proposed edits only. The host owns conflict-checked publication.
use crate::{Error, Result, files, util::digest_bytes};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: PathBuf,
    home: PathBuf,
    ahu_executable: PathBuf,
    skills: Vec<Skill>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Skill {
    name: String,
    text: String,
}

fn proposal(path: &Path, before: Option<&[u8]>, text: String) -> Result<Value> {
    if text.len() > files::LIMIT {
        return Err(Error::invalid());
    }
    Ok(json!({"path":path,"expected_before_sha256":before.map(digest_bytes),"text":text}))
}
pub(crate) fn call(harness: &str, params: &Value) -> Result<Value> {
    let p: Params = crate::decode(params)?;
    crate::directory(&p.project)?;
    crate::directory(&p.home)?;
    if !p.ahu_executable.is_absolute() || !p.ahu_executable.is_file() || p.skills.len() > 128 {
        return Err(Error::invalid());
    }
    let mut changes = Vec::new();
    let (path, key) = match harness {
        "opencode" => {
            if std::fs::symlink_metadata(p.project.join("opencode.jsonc")).is_ok() {
                return Err(Error::invalid());
            }
            (p.project.join("opencode.json"), "mcp")
        }

        _ => return Err(Error::invalid()),
    };
    files::private_scope(&p.project, &path)?;
    let before = files::read(&path)?;
    let after = {
        let mut doc: Value = match before.as_deref() {
            None => json!({}),
            Some(b) if harness == "antigravity" && b.iter().all(u8::is_ascii_whitespace) => {
                json!({})
            }
            Some(b) => serde_json::from_slice(b)?,
        };
        let servers = doc
            .as_object_mut()
            .ok_or_else(Error::invalid)?
            .entry(key)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(Error::invalid)?;
        let desired =
            { json!({"type":"local","command":[p.ahu_executable,"mcp","serve"],"enabled":true}) };
        let server = servers.entry("ahu").or_insert(desired.clone());
        {
            if *server != desired {
                return Err(Error::invalid());
            }
        }
        serde_json::to_string_pretty(&doc)? + "\n"
    };
    if before.as_deref() != Some(after.as_bytes()) {
        changes.push(proposal(&path, before.as_deref(), after)?);
    }
    let mut names = std::collections::BTreeSet::new();
    for skill in p.skills {
        if skill.name.is_empty()
            || skill.name.len() > 128
            || !skill
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            || !names.insert(skill.name.clone())
            || skill.text.len() > files::LIMIT
        {
            return Err(Error::invalid());
        }
        let roots: &[&str] = match harness {
            "opencode" => &[".agents/skills", ".opencode/skills"],
            _ => &[".agents/skills"],
        };
        for root in roots {
            let path = p.project.join(root).join(&skill.name).join("SKILL.md");
            files::private_scope(&p.project, &path)?;
            let before = files::read(&path)?;
            if let Some(bytes) = before.as_deref() {
                if bytes != skill.text.as_bytes() {
                    return Err(Error::invalid());
                }
            } else {
                changes.push(proposal(&path, None, skill.text.clone())?);
            }
        }
    }
    if serde_json::to_vec(&changes)?.len() > 4 * files::LIMIT {
        return Err(Error::invalid());
    }
    Ok(json!({"changes":changes,"publication":"host_transaction_required"}))
}
