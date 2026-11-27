//! Idempotent Trace projection with claim evidence inspection.
use squad_protocol::trace::*;
use squad_protocol::*;
pub fn lines(s: &TraceState) -> Vec<String> {
    vec![
        "TRACE".into(),
        match s.state {
            TraceStatus::Extracting => "EXTRACT",
            TraceStatus::Merging => "QUOTE CHECK",
            TraceStatus::Verifying => "VERIFY",
            TraceStatus::Assembling | TraceStatus::Reporting | TraceStatus::Complete => "BRIEF",
            _ => "TRACE",
        }
        .into(),
        format!(
            "{}{}",
            squad_protocol::human::label(&s.state),
            s.terminal_reason
                .as_ref()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default()
        ),
    ]
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<TraceState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
    pub selected_claim: Option<String>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::TraceState(s)
            | EnvelopePayload::ContestState(ContestState::Trace(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version < s.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Trace)
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
    pub fn claim_ids(&self) -> Vec<String> {
        let Some(state) = &self.state else {
            return vec![];
        };
        let mut ids = std::collections::BTreeSet::new();
        for artifact in self
            .artifacts
            .values()
            .filter(|a| a.contest_id == state.contest_id)
        {
            if let Some(body) = &artifact.body {
                let ledger = match artifact.artifact_kind.as_str() {
                    "trace.report" => validation::decode::<TraceReport>(body)
                        .ok()
                        .map(|r| r.ledger),
                    "trace.ledger" => validation::decode::<TraceLedger>(body).ok(),
                    _ => None,
                };
                if let Some(ledger) = ledger {
                    ids.extend(ledger.claims.into_iter().map(|claim| claim.claim_id));
                }
            }
        }
        ids.into_iter().collect()
    }
    pub fn select_claim(&mut self, id: &str) {
        self.selected_claim = Some(id.trim_matches(['[', ']']).into());
    }
    pub fn inspect(&self) -> Vec<String> {
        let Some(s) = &self.state else { return vec![] };
        let Some(id) = &self.selected_claim else {
            return vec![];
        };
        for a in self
            .artifacts
            .values()
            .filter(|a| a.contest_id == s.contest_id)
        {
            if let Some(body) = &a.body {
                let ledger = match a.artifact_kind.as_str() {
                    "trace.report" => validation::decode::<TraceReport>(body)
                        .ok()
                        .map(|r| r.ledger),
                    "trace.ledger" => validation::decode::<TraceLedger>(body).ok(),
                    _ => None,
                };
                if let Some(l) = ledger
                    && let Some(c) = l.claims.iter().find(|c| c.claim_id == *id)
                {
                    return std::iter::once(format!(
                        "[{}] {}: {}",
                        c.claim_id,
                        squad_protocol::human::label(&c.status),
                        c.text
                    ))
                    .chain(c.quotes.iter().filter_map(|q| {
                        q.span.as_ref().map(|s| {
                            format!(
                                "{} bytes {}..{}: {}",
                                q.quote.source_id, s.start, s.end, q.quote.quote
                            )
                        })
                    }))
                    .collect();
                }
            }
        }
        vec![]
    }
}
pub fn render(v: &View) -> Vec<String> {
    let Some(s) = &v.state else { return vec![] };
    let mut out = lines(s);
    for a in v
        .artifacts
        .values()
        .filter(|a| a.contest_id == s.contest_id && a.artifact_kind.as_str() == "trace.report")
    {
        if let Some(body) = &a.body
            && let Ok(r) = validation::decode::<TraceReport>(body)
        {
            out.extend(
                squad_core::export::trace::report_text(&r, false)
                    .lines()
                    .map(str::to_owned),
            );
        }
    }
    out.extend(v.inspect());
    out
}
