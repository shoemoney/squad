//! Forge committed-event projection. Draft content is collapsed by default.
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<ForgeState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::ForgeState(s)
            | EnvelopePayload::ContestState(ContestState::Forge(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version <= s.state_version
                }) {
                    if self
                        .state
                        .as_ref()
                        .is_some_and(|old| old.contest_id != s.contest_id)
                    {
                        self.artifacts.clear();
                    }
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Forge)
                    && self
                        .state
                        .as_ref()
                        .is_none_or(|s| s.contest_id == a.contest_id) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn lines(s: &ForgeState) -> Vec<String> {
    vec![
        "FORGE".into(),
        format!(
            "{:?}{}",
            s.state,
            s.terminal_reason
                .as_ref()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default()
        ),
    ]
}
pub fn render(v: &View) -> Vec<String> {
    let mut out = v.state.as_ref().map(lines).unwrap_or_default();
    for divider in ["DRAFTS", "RANKING", "FUSION", "VERDICT"] {
        out.push(divider.into());
        let kind = match divider {
            "DRAFTS" => "forge.draft",
            "RANKING" => "forge.rank",
            "FUSION" => "forge.fuse",
            _ => "forge.report",
        };
        for a in v
            .artifacts
            .values()
            .filter(|a| a.artifact_kind.as_str() == kind)
        {
            if kind == "forge.draft" {
                out.push(format!("{:?} · draft (collapsed)", a.actor));
            } else if kind == "forge.report" {
                if let Some(b) = &a.body
                    && let Ok(r) = validation::decode::<forge::ForgeReport>(b)
                {
                    out.push(squad_core::export::forge::verdict_line(&r));
                    out.extend(r.body.lines().map(str::to_owned));
                }
            } else {
                out.push(format!("{:?} · accepted", a.actor));
            }
        }
    }
    out
}
