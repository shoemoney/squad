//! Tune's human view; the suite remains collapsed until inspected/exported.
use squad_protocol::tune::*;
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<TuneState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::TuneState(s)
            | EnvelopePayload::ContestState(ContestState::Tune(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version < s.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Tune) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn lines(s: &TuneState) -> Vec<String> {
    let divider = match s.state {
        TuneStatus::BuildingCases | TuneStatus::Freezing => "TEST SET".into(),
        TuneStatus::Executing | TuneStatus::Scoring if s.round == 0 => "BASELINE".into(),
        TuneStatus::Executing | TuneStatus::Scoring | TuneStatus::Mutating => {
            format!("GENERATION {}", s.round)
        }
        TuneStatus::HoldoutExecuting | TuneStatus::HoldoutScoring => "HELD OUT".into(),
        TuneStatus::Complete | TuneStatus::Failed | TuneStatus::Reporting | TuneStatus::Stopped => {
            "RESULT".into()
        }
        _ => squad_protocol::human::label(&s.state),
    };
    vec!["TUNE".into(), divider]
}
pub fn render(view: &View) -> Vec<String> {
    let Some(s) = &view.state else {
        return vec![];
    };
    let mut out = lines(s);
    let artifacts: Vec<_> = view
        .artifacts
        .values()
        .filter(|a| a.contest_id == s.contest_id)
        .collect();
    for a in &artifacts {
        let Some(body) = &a.body else {
            continue;
        };
        match a.artifact_kind.as_str() {
            "tune.suite" => {
                if let Ok(suite) = validation::decode::<TuneSuite>(body) {
                    out.push(format!(
                        "Test set: {} train · {} held out (collapsed)",
                        suite.train.len(),
                        suite.holdout.len()
                    ));
                }
            }
            "tune.generation" => {
                if let Ok(g) = validation::decode::<TuneGeneration>(body) {
                    for (id, score) in &g.scores {
                        out.push(format!(
                            "Generation {} · {} · {:.2}{}",
                            g.generation,
                            id,
                            score,
                            if g.promoted && id == &g.champion {
                                " ✓ promoted"
                            } else {
                                ""
                            }
                        ));
                    }
                }
            }
            "tune.report" => {
                if let Ok(r) = validation::decode::<TuneReport>(body) {
                    out.push(r.result_line);
                }
            }
            _ => {}
        }
    }
    let executions = artifacts
        .iter()
        .filter(|a| a.artifact_kind.as_str() == "tune.execute")
        .count();
    if matches!(
        s.state,
        TuneStatus::Executing | TuneStatus::HoldoutExecuting
    ) {
        out.push(format!("Executions complete: {executions}"));
    }
    if s.state == TuneStatus::Stopped {
        out.push("Stopped".into());
    }
    out
}
