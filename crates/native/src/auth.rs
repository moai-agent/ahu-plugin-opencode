use crate::util::{Error, Result, digest_bytes};
use serde_json::{Value, json};
use std::io::Read;
pub struct Identity {
    pub harness: String,
    pub identity_kind: String,
    pub principal: String,
    pub organization: Option<String>,
    pub workspace: Option<String>,
}

impl Identity {
    fn fingerprint(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&json!({
            "harness": self.harness,
            "identity_kind": self.identity_kind,
            "principal": self.principal,
            "organization": self.organization,
            "workspace": self.workspace,
        }))?;
        Ok(digest_bytes(&bytes))
    }
}
fn probe_ollama(configured: &str) -> Result<Identity> {
    let endpoint = if configured.contains("://") {
        configured.to_owned()
    } else {
        format!("http://{configured}")
    };
    let mut base = reqwest::Url::parse(&endpoint)
        .map_err(|_| Error::new("OLLAMA_HOST is not a valid local server address"))?;
    let local = base
        .host_str()
        .and_then(|host| host.parse::<std::net::IpAddr>().ok())
        .is_some_and(|address| address.is_loopback())
        || matches!(base.host_str(), Some("localhost"));
    if !local || !matches!(base.scheme(), "http" | "https") {
        return Err(Error::new(
            "Ollama account binding only supports a loopback Ollama server",
        ));
    }
    base.set_path("/api/me");
    base.set_query(None);
    base.set_fragment(None);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| Error::new("cannot initialize the Ollama identity check"))?;
    let response = client
        .post(base)
        .json(&json!({}))
        .send()
        .map_err(|_| Error::new("cannot reach the local Ollama account identity endpoint"))?;
    if !response.status().is_success() {
        return Err(Error::new(
            "local Ollama did not provide a signed-in account identity",
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > 64 * 1024)
    {
        return Err(Error::new("Ollama account identity response is too large"));
    }
    let mut bytes = Vec::new();
    response
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::new("cannot read the Ollama account identity response"))?;
    if bytes.len() > 64 * 1024 {
        return Err(Error::new("Ollama account identity response is too large"));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::new("Ollama account identity response is invalid"))?;
    let principal = value
        .get("email")
        .and_then(Value::as_str)
        .filter(|email| email.contains('@') && !email.chars().any(char::is_control))
        .ok_or_else(|| Error::new("Ollama is not signed in or returned no account email"))?;
    let account_id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::new("Ollama returned no stable account identifier"))?;
    Ok(Identity {
        harness: "ollama".into(),
        identity_kind: "ollama-account".into(),
        principal: principal.into(),
        organization: None,
        workspace: Some(account_id.into()),
    })
}

// Open agy in the checkout interactively and review its prompt; ahu never
// approves native folder trust automatically.

pub(crate) fn call(
    harness: &str,
    p: &crate::ProbeParams,
    budget: bool,
    model: Option<&str>,
    ollama_host: Option<&str>,
    requested_identity: Option<&str>,
) -> Result<Value> {
    if requested_identity
        .is_some_and(|id| id != harness && !(harness == "opencode" && id == "ollama"))
    {
        return Err(Error::invalid());
    }
    if budget {
        {
            return Ok(json!({"status":"unsupported","windows":[],"signal_type":"unknown"}));
        }
    }

    let identity = match harness {
        "opencode" => {
            let model = model.unwrap_or("").to_ascii_lowercase();
            let cloud = model.split_once('/').is_some_and(|(provider, name)| {
                provider == "ollama" && (name.contains(":cloud") || name.ends_with("-cloud"))
            });
            if !cloud && requested_identity != Some("ollama") {
                return Ok(
                    json!({"status":"unsupported","binding_harness":null,"fingerprint":null}),
                );
            }
            probe_ollama(
                ollama_host
                    .or_else(|| p.environment.get("OLLAMA_HOST").map(String::as_str))
                    .unwrap_or("http://127.0.0.1:11434"),
            )?
        }
        _ => return Err(Error::invalid()),
    };
    Ok(
        json!({"status":"available","binding_harness":identity.harness,"fingerprint":identity.fingerprint()?,"effective_provider_address_verified":harness!="opencode","identity_kind":identity.identity_kind}),
    )
}
