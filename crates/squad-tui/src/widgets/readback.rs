//! Idempotent projection of committed readback evidence.
use squad_protocol::readback::*;
use squad_protocol::*;
pub fn lines(s: &ReadbackState) -> Vec<String> {
    vec![
        "READBACK".into(),
        format!(
            "ROUND {} · {}",
            s.round,
            squad_protocol::human::label(&s.state)
        ),
        s.terminal_reason.as_ref().map_or_else(
            || match s.state {
                ReadbackStatus::Interpreting => "READINGS".into(),
                ReadbackStatus::AwaitingHuman => {
                    "READ IT BACK · answer pending · 1b 2a ... or /answer 3: your reading".into()
                }
                ReadbackStatus::Rewriting => "REWRITE".into(),
                _ => String::new(),
            },
            ToString::to_string,
        ),
    ]
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<ReadbackState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::ReadbackState(s)
            | EnvelopePayload::ContestState(ContestState::Readback(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version < s.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Readback)
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    let Some(state) = &view.state else {
        return vec![];
    };
    let mut out = lines(state);
    for a in view
        .artifacts
        .values()
        .filter(|a| a.contest_id == state.contest_id)
    {
        if let Some(body) = &a.body {
            match a.artifact_kind.as_str() {
                "readback.rewrite" => {
                    if let Ok(r) = validation::decode::<ReadbackRewrite>(body) {
                        out.push("REWRITE".into());
                        for c in r.changes {
                            out.push(format!("Before: {}", c.before));
                            out.push(format!("After: {}", c.after));
                        }
                    }
                }
                "readback.report" => {
                    if let Ok(r) = validation::decode::<ReadbackReport>(body) {
                        out.extend(
                            squad_core::export::readback::report_text(&r, false)
                                .lines()
                                .map(str::to_owned),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    out
}
