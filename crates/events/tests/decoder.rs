use ahu_harness_events::Decoder;
use serde_json::{Value, json};

fn feed(decoder: &mut Decoder, event: Value) -> Value {
    decoder
        .observe_stdout(&serde_json::to_vec(&event).unwrap())
        .unwrap()
}

#[test]
fn selected_provider_keeps_terminal_summary_and_unknown_usage_separate() {
    let fixtures = [
        (
            "codex",
            vec![
                json!({"type":"item.completed","item":{"type":"agent_message","text":"answer"}}),
                json!({"type":"turn.completed"}),
            ],
        ),
        (
            "claude-code",
            vec![json!({"type":"result","subtype":"success","is_error":false,"result":"answer"})],
        ),
        (
            "antigravity",
            vec![json!({"type":"result","result":{"status":"success","response":"answer"}})],
        ),
        (
            "opencode",
            vec![
                json!({"type":"text","part":{"text":"answer"}}),
                json!({"type":"step_finish","part":{"id":"end","reason":"stop"}}),
            ],
        ),
        (
            "kiro",
            vec![
                json!({"type":"runStarted","data":{"engine":"v3","payloadSchema":"acp","acpProtocolVersion":1}}),
                json!({"type":"runFinished","data":{"sessionId":"sess_synthetic","status":"success","stopReason":"end_turn","finalText":"answer","finalTextTruncated":false}}),
            ],
        ),
    ];
    for (harness, events) in fixtures
        .into_iter()
        .filter(|(harness, _)| *harness == "opencode")
    {
        let mut decoder = Decoder::new(harness).unwrap();
        for event in events {
            feed(&mut decoder, event);
        }
        let result = decoder.finish().unwrap();
        assert_eq!(result["terminal"], true, "{harness}");
        assert_eq!(result["failed"], false, "{harness}: {result}");
        assert_eq!(result["summary"], "answer");
        assert!(result["usage"]["total"].is_null());
        assert!(result["cost"].get("usd").is_none());
        assert_eq!(decoder.finish().unwrap(), result);
    }
    assert!(Decoder::new("invented").is_err());
}

#[test]
fn stream_order_and_opencode_snapshot_dedup_are_preserved() {
    let mut decoder = Decoder::new("opencode").unwrap();
    let step = json!({"type":"step_finish","sessionID":"ses_fixture","part":{"id":"step-a","reason":"stop","cost":0.25,"tokens":{"input_tokens":10}}});
    feed(&mut decoder, step.clone());
    decoder
        .observe_stderr(b"Reading prompt from stdin...")
        .unwrap();
    feed(&mut decoder, step);
    let result = feed(
        &mut decoder,
        json!({"type":"reasoning","part":{"text":"PRIVATE REASONING"}}),
    );
    assert_eq!(result["cost"]["usd"], 0.25);
    assert_eq!(result["terminal"], false);
    assert_eq!(result["usage"]["input"], 10);
    assert!(!result.to_string().contains("PRIVATE"));
    assert_eq!(decoder.finish().unwrap()["terminal"], false);
}

#[test]
fn conflicting_or_redacted_model_evidence_never_claims_one_inference_identity() {
    for (alias, coverage) in [
        (json!("different"), "conflicting"),
        (json!("sk-private"), "redacted"),
    ] {
        let mut decoder = Decoder::new("opencode").unwrap();
        let result = feed(
            &mut decoder,
            json!({"type":"step_finish","model":"fixture","model_name":alias,"part":{"id":"end","reason":"stop"}}),
        );
        assert_eq!(
            result["model_attributes"]["ahu.harness.model.reported.coverage"],
            coverage
        );
        assert!(
            result["model_attributes"]
                .get("ahu.harness.model.reported")
                .is_none()
        );
        assert!(!result.to_string().contains("sk-private"));
    }
}

#[test]
fn late_valid_cost_deduplicates_and_arithmetic_loss_is_visible() {
    let mut decoder = Decoder::new("opencode").unwrap();
    for amount in [
        Value::Null,
        json!(-1),
        json!("invalid"),
        json!(0.25),
        json!(99),
    ] {
        feed(
            &mut decoder,
            json!({"type":"step_finish","part":{"id":"one","cost":amount}}),
        );
    }
    feed(
        &mut decoder,
        json!({"type":"step_finish","part":{"id":"two","cost":0.125}}),
    );
    assert_eq!(decoder.snapshot()["cost"]["usd"], 0.375);
    let mut decoder = Decoder::new("opencode").unwrap();
    for id in ["one", "two"] {
        feed(
            &mut decoder,
            json!({"type":"step_finish","part":{"id":id,"cost":f64::MAX}}),
        );
    }
    assert_eq!(decoder.snapshot()["cost"]["usd"], json!(f64::MAX));
    assert_eq!(decoder.snapshot()["cost_tracking_limited"], true);
}
