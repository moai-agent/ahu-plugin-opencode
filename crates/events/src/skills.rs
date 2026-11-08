//! Bounded synthetic protocol adapters. A tool result is not proof of instruction adherence.
use super::{Events, SkillInvocation};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug)]
pub(super) struct Call {
    id: String,
    harness: String,
    index: usize,
    started: Instant,
    ambiguous: bool,
}

fn call_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_graphic()))
}

fn possible_skill_event(harness: &str, event: &Value) -> bool {
    let is =
        |value: Option<&Value>, expected: &str| value.and_then(Value::as_str) == Some(expected);
    match harness {
        "opencode" => is(event.pointer("/part/tool"), "skill"),

        _ => false,
    }
}

impl Events {
    pub(super) fn observe_skill(&mut self, harness: &str, event: &Value) {
        use crate::SkillEvidence;
        // Exact envelope/tool pairs only. New versions must not turn arbitrary
        // nested payloads, response text, or similarly named MCP tools into use.
        let kind = event.get("type").and_then(Value::as_str);
        let mut inputs = Vec::new();
        match harness {
            "opencode"
                if kind == Some("tool_use")
                    && event.pointer("/part/type").and_then(Value::as_str) == Some("tool")
                    && event.pointer("/part/tool").and_then(Value::as_str) == Some("skill") =>
            {
                let status = self.skill_status(event.pointer("/part/state/status"), false);
                inputs.push((
                    event.pointer("/part/state/input"),
                    event.pointer("/part/callID"),
                    status,
                ));
            }

            _ => (),
        }
        if inputs.is_empty() && possible_skill_event(harness, event) {
            self.skill_unknown_events = self.skill_unknown_events.saturating_add(1);
            if self.skill_observation != SkillEvidence::Observed {
                self.skill_observation = SkillEvidence::Unverified;
            }
        }
        for (input, id, completion) in inputs {
            let name = input
                .and_then(|v| v.get("skill").or_else(|| v.get("name")))
                .and_then(Value::as_str)
                .filter(|name| {
                    !name.is_empty()
                        && name.len() <= 128
                        && name.bytes().all(|b| {
                            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')
                        })
                        && name
                            .bytes()
                            .next()
                            .is_some_and(|b| b.is_ascii_alphanumeric())
                });
            let Some(name) = name else {
                self.skill_unknown_events = self.skill_unknown_events.saturating_add(1);
                if self.skill_observation != SkillEvidence::Observed {
                    self.skill_observation = SkillEvidence::Unverified;
                }
                continue;
            };
            if let Some(id) = call_id(id) {
                let matches: Vec<usize> = self
                    .skill_calls
                    .iter()
                    .enumerate()
                    .filter(|(_, call)| call.harness == harness && call.id == id)
                    .map(|(index, _)| index)
                    .collect();
                if !matches.is_empty() {
                    let call = &self.skill_calls[matches[0]];
                    let index = call.index;
                    // These adapters report snapshots of the same tool call.
                    {
                        let skill = &self.skills[index];
                        if matches.len() == 1 && !call.ambiguous && skill.name == name {
                            if let Some(failed) = completion {
                                let status = if failed { "failed" } else { "completed" };
                                if skill.status == "invoked" {
                                    let elapsed =
                                        call.started.elapsed().as_millis().min(u64::MAX as u128)
                                            as u64;
                                    self.complete_skill(index, failed, Some(elapsed));
                                    continue;
                                }
                                if skill.status == status {
                                    continue;
                                }
                            } else if skill.status == "invoked" {
                                continue;
                            }
                        }
                        self.invalidate_skill_calls(&matches);
                        continue;
                    }
                    // Reused IDs cannot support a unique completion attribution.
                }
            }
            if self.skills.len() >= 128 {
                self.failed = true;
                if !self
                    .blockers
                    .iter()
                    .any(|b| b == "skill invocation limit exceeded")
                {
                    self.blockers.push("skill invocation limit exceeded".into());
                }
                break;
            }
            self.skill_observation = SkillEvidence::Observed;
            let index = self.skills.len();
            self.skills.push(SkillInvocation {
                name: name.into(),
                source: None,
                digest: None,
                status: "invoked".into(),
                completed_at: None,
                elapsed_ms: None,
                harness: Some(harness.into()),
                observed_at: None,
                evidence: SkillEvidence::Observed,
                execution: SkillEvidence::Unverified,
            });
            if let Some(id) = call_id(id) {
                self.skill_calls.push(Call {
                    id: id.into(),
                    harness: harness.into(),
                    index,
                    started: Instant::now(),
                    ambiguous: false,
                });
            }
            if let Some(failed) = completion {
                self.complete_skill(index, failed, None);
            }
        }
        self.observe_skill_results(harness, event);
    }

    fn invalidate_skill_calls(&mut self, matches: &[usize]) {
        self.skill_unknown_events = self.skill_unknown_events.saturating_add(1);
        for index in matches {
            let call = &mut self.skill_calls[*index];
            call.ambiguous = true;
            let skill = &mut self.skills[call.index];
            skill.status = "invoked".into();
            skill.execution = crate::SkillEvidence::Unverified;
            skill.completed_at = None;
            skill.elapsed_ms = None;
        }
    }

    fn skill_status(&mut self, status: Option<&Value>, nested: bool) -> Option<bool> {
        match (nested, status.and_then(Value::as_str)) {
            (false, Some("completed")) | (true, Some("DONE")) => Some(false),
            (false, Some("error")) | (true, Some("ERROR")) => Some(true),
            (_, None) if status.is_none() => None,
            (false, Some("pending" | "running")) | (true, Some("RUNNING")) => None,
            _ => {
                self.skill_unknown_events = self.skill_unknown_events.saturating_add(1);
                None
            }
        }
    }

    fn complete_skill(&mut self, index: usize, failed: bool, elapsed_ms: Option<u64>) {
        let skill = &mut self.skills[index];
        skill.status = if failed { "failed" } else { "completed" }.into();
        skill.execution = crate::SkillEvidence::Observed;
        skill.completed_at = None;
        skill.elapsed_ms = elapsed_ms;
    }

    fn observe_skill_results(&mut self, _harness: &str, _event: &Value) {}
}
