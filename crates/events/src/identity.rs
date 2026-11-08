//! Bounded native model evidence. A first value cannot establish session identity.
use serde_json::Value;

#[derive(Debug, Default)]
pub(super) struct ModelEvidence {
    first: Option<String>,
    conflicting: bool,
    redacted: bool,
    missing: bool,
}

impl ModelEvidence {
    pub(super) fn observe<'a>(&mut self, values: impl Iterator<Item = &'a Value>) {
        let mut found = false;
        for value in values {
            found = true;
            let Some(value) = value.as_str().filter(|v| crate::safe_identity(v)) else {
                self.redacted = true;
                continue;
            };
            if let Some(first) = &self.first {
                self.conflicting |= first != value;
            } else {
                self.first = Some(value.to_owned());
            }
        }
        self.missing |= !found;
    }

    fn attributes(&self, complete_stream: bool) -> Vec<(&'static str, String)> {
        let coverage = if self.conflicting {
            "conflicting"
        } else if self.redacted {
            "redacted"
        } else if self.missing || !complete_stream {
            "partial"
        } else {
            "complete"
        };
        let mut attributes = vec![("ahu.harness.model.reported.coverage", coverage.into())];
        if !self.conflicting
            && !self.redacted
            && let Some(model) = &self.first
        {
            attributes.push(("ahu.harness.model.reported", model.clone()));
        }
        attributes
    }
}

impl super::Events {
    pub(crate) fn reported_model_attributes(&self) -> Vec<(&'static str, String)> {
        self.model_evidence.attributes(
            self.terminal && self.unknown_events == 0 && self.trajectory.observation.complete(),
        )
    }
}
