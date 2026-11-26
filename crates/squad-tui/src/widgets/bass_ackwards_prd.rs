//! Durable PRD view; trees remain collapsed in the transcript summary.
use squad_protocol::*;
pub fn lines(state: &BassAckwardsPrdState) -> Vec<String> {
    vec!["BASS ACKWARDS".into(), format!("{:?}", state.state)]
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<BassAckwardsPrdState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::BassAckwardsPrdState(s)
            | EnvelopePayload::ContestState(ContestState::BassAckwardsPrd(s)) => {
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::BassAckwardsPrd) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    let mut out = view.state.as_ref().map(lines).unwrap_or_default();
    for divider in [
        "DONE FIRST",
        "BACKWARD TREES",
        "MERGED PRD",
        "REVIEW",
        "LAST MUST-HAVES",
        "CHECKS",
        "SAVED",
    ] {
        out.push(divider.into());
        match divider {
            "BACKWARD TREES" => {
                let count = view
                    .artifacts
                    .values()
                    .filter(|a| a.artifact_kind.as_str() == "bass_ackwards_prd.tree")
                    .count();
                out.push(format!("{count} trees (collapsed)"));
            }
            "REVIEW" | "LAST MUST-HAVES" => {
                for a in view.artifacts.values() {
                    if a.artifact_kind.as_str()
                        == if divider == "REVIEW" {
                            "bass_ackwards_prd.revise"
                        } else {
                            "bass_ackwards_prd.finalize"
                        }
                    {
                        if let Some(entries) =
                            a.body.as_ref().and_then(|b| b["decisions"].as_array())
                        {
                            out.push("Decision | Ref | Reason".into());
                            for d in entries {
                                out.push(format!(
                                    "{} | {} | {}",
                                    if d["verdict"] == "accept" {
                                        "✓"
                                    } else {
                                        "✗"
                                    },
                                    d["ref"].as_str().unwrap_or(""),
                                    d["reason"].as_str().unwrap_or("")
                                ));
                            }
                        }
                    }
                }
            }
            "SAVED" => {
                for a in view.artifacts.values() {
                    if a.artifact_kind.as_str() == "bass_ackwards_prd.saved" {
                        if let Some(path) = a.body.as_ref().and_then(|b| b["path"].as_str()) {
                            out.push(format!("PRD saved to {path}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if view
        .state
        .as_ref()
        .is_some_and(|s| s.state == BassAckwardsPrdStatus::AwaitingHuman)
    {
        out.push("LAST LOOK".into());
        out.push("/answer <note>, or /continue to save".into());
    }
    out
}
