//! Independent wire-boundary checks against the shipped executable.
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn exchange(input: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ahu-plugin-opencode"));
    command.env_clear();
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let mut child = command
        .arg("--ahu-harness-runtime-protocol=1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
fn request(id: u64, operation: &str, params: Value) -> String {
    format!(
        "{}\n",
        json!({"protocol_version":1,"id":id,"operation":operation,"params":params})
    )
}
#[test]
fn malformed_unterminated_and_unknown_requests_are_refused_without_echo() {
    for input in [
        b"private-invalid-json\n".as_slice(),
        b"{\"protocol_version\":1}",
        b"{\"protocol_version\":2,\"id\":1,\"operation\":\"probe\",\"params\":{}}\n",
        b"{\"protocol_version\":1,\"id\":1,\"operation\":\"unknown\",\"params\":{}}\n",
        b"{\"protocol_version\":1,\"id\":1,\"id\":2,\"operation\":\"probe\",\"params\":{}}\n",
    ] {
        let output = exchange(input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}
#[test]
fn a_foreign_decoder_is_rejected_and_does_not_claim_the_session() {
    let input = request(1, "decode_start", json!({"harness":"foreign-provider"}))
        + &request(2, "decode_start", json!({"harness":"opencode"}));
    let output = exchange(input.as_bytes());
    assert!(output.status.success());
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].get("error").is_some());
    assert!(rows[1].get("result").is_some());
}
#[test]
fn duplicate_ids_are_refused_after_exactly_one_response() {
    let line = request(1, "decode_start", json!({"harness":"opencode"}));
    let output = exchange((line.clone() + &line).as_bytes());
    assert!(!output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().lines().count(), 1);
    assert!(output.stderr.is_empty());
}
#[test]
fn decoder_state_cannot_be_started_twice() {
    let input = request(1, "decode_start", json!({"harness":"opencode"}))
        + &request(2, "decode_start", json!({"harness":"opencode"}));
    let output = exchange(input.as_bytes());
    assert!(output.status.success());
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(rows[0].get("result").is_some());
    assert!(rows[1].get("error").is_some());
}
#[test]
fn finishing_without_a_decoder_is_an_explicit_error() {
    let output = exchange(request(1, "decode_finish", json!({})).as_bytes());
    assert!(output.status.success());
    let row: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(row.get("error").is_some());
    assert!(output.stderr.is_empty());
}
