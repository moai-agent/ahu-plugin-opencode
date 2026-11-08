use ahu_harness_native::call;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::PathBuf};

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    exe: PathBuf,
}
impl Fixture {
    fn new(body: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let exe = root.join("native");
        fs::write(&exe, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            _tmp: tmp,
            root,
            home,
            exe,
        }
    }
    fn probe(&self) -> Value {
        json!({"project":self.root,"home":self.home,"native_executable":self.exe,"environment":{}})
    }
    fn setup(&self) -> Value {
        json!({"project":self.root,"home":self.home,"ahu_executable":self.exe,"skills":[]})
    }
}

#[test]
fn setup_proposes_all_provider_formats_and_never_writes() {
    let f = Fixture::new("exit 99");
    for h in ["opencode"] {
        let mut p = f.setup();
        p["skills"] = json!([{"name":"test-skill","text":"# synthetic\n"}]);
        let result = call(h, "setup", &p).unwrap();
        let changes = result["changes"].as_array().unwrap();
        assert!(changes.len() >= 2);
        for change in changes {
            assert!(change["expected_before_sha256"].is_null());
            assert!(!PathBuf::from(change["path"].as_str().unwrap()).exists());
        }
        let text = changes[0]["text"].as_str().unwrap();
        if h == "codex" {
            let v: toml::Value = toml::from_str(text).unwrap();
            assert_eq!(
                v["mcp_servers"]["ahu"]["tools"]["ahu_agents_list"]["approval_mode"].as_str(),
                Some("approve")
            );
        } else {
            let v: Value = serde_json::from_str(text).unwrap();
            let key = if h == "opencode" { "mcp" } else { "mcpServers" };
            assert!(v[key]["ahu"].is_object());
            if h == "kiro" {
                assert_eq!(v[key]["ahu"]["env"]["AHU_TASK_ID"], "${AHU_TASK_ID}");
            }
        }
    }
}

#[test]
fn opencode_identity_is_explicitly_routed_or_unsupported() {
    let f = Fixture::new("exit 99");
    let mut p = f.probe();
    p["model"] = json!("other/model");
    assert_eq!(
        call("opencode", "auth", &p).unwrap()["status"],
        "unsupported"
    );
    p["model"] = json!("ollama/glm-5.3:cloud");
    p["ollama_host"] = json!("https://example.invalid");
    assert!(call("opencode", "auth", &p).is_err());
}

#[test]
fn ollama_cloud_route_uses_explicit_loopback_identity_with_no_raw_output() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let start = std::time::Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("synthetic server failed: {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
            assert!(request.len() < 8192);
        }
        assert!(request.starts_with(b"POST /api/me HTTP/1.1"));
        let body = r#"{"email":"fixture@example.invalid","id":"synthetic-account","token":"private-sentinel"}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let f = Fixture::new("exit 99");
    let mut params = f.probe();
    params["model"] = json!("ollama/glm-5.3:cloud");
    params["ollama_host"] = json!(endpoint);
    let result = call("opencode", "auth", &params).unwrap();
    server.join().unwrap();
    assert_eq!(result["binding_harness"], "ollama");
    assert_eq!(result["effective_provider_address_verified"], false);
    assert!(!result.to_string().contains("private-sentinel"));
    assert!(!result.to_string().contains("fixture@"));
}

#[test]
fn masked_native_overrides_never_probe_the_default_account() {
    let f = Fixture::new("touch should-not-run; exit 99");
    for (harness, variable) in [
        ("codex", "CODEX_HOME"),
        ("claude-code", "ANTHROPIC_API_KEY"),
        ("opencode", "OLLAMA_HOST"),
        ("kiro", "KIRO_HOME"),
        ("antigravity", "GEMINI_API_KEY"),
    ] {
        let mut p = f.probe();
        p["environment_names"] = json!([variable]);
        assert!(call(harness, "auth", &p).is_err());
        assert!(call(harness, "budget", &p).is_err());
    }
    assert!(!f.root.join("should-not-run").exists());
}

#[test]
fn bounded_recovery_validates_export_and_never_replays() {
    use sha2::{Digest, Sha256};
    let export = json!({"info":{"id":"ses_synthetic123","outcome":"succeeded","time":{"created":1010,"updated":1900,"idle":1900}},"messages":[{"id":"prompt","type":"user","time":{"created":1011}},{"id":"answer","type":"assistant","finish":"stop","content":[],"time":{"created":1012,"completed":1800}},{"id":"idle","type":"idle","outcome":"succeeded","time":{"created":1900}}]});
    let f = Fixture::new(&format!(
        "test \"$1 $2 $3 $4 $5\" = 'session export --sanitize --standalone ses_synthetic123' || exit 98\nprintf '%s' '{}'",
        export
    ));
    let digest = format!("{:x}", Sha256::digest(fs::read(&f.exe).unwrap()));
    let mut p = json!({"harness":"opencode","native_version":"2.0.32","native_executable":f.exe,"expected_executable_digest":digest,"worktree":f.root,"session":"ses_synthetic123","attempt_started_ms":1000});
    assert_eq!(
        call("opencode", "recover", &p).unwrap(),
        json!({"recovered":true})
    );
    p["attempt_started_ms"] = json!(2000);
    assert_eq!(
        call("opencode", "recover", &p).unwrap(),
        json!({"recovered":false})
    );
    p["expected_executable_digest"] = json!("a".repeat(64));
    assert!(call("opencode", "recover", &p).is_err());
    p["native_version"] = json!("2.0.33");
    assert_eq!(
        call("opencode", "recover", &p).unwrap(),
        json!({"recovered":false})
    );
}
