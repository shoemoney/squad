//! Shared presentation of scripted contest progress: step labels, stable work
//! cards, retries, skipped members and stopped/failed/incomplete outcomes.
//! Everything shown is committed core state; no engine move runs in the client.
use squad_protocol::*;

pub fn move_label(m: &ContestMove) -> &'static str {
    match m {
        ContestMove::Collect => "collect",
        ContestMove::Pick => "pick",
        ContestMove::Fanout => "fan out",
        ContestMove::Turns => "turns",
        ContestMove::Judge => "judge",
        ContestMove::Tally => "tally",
        ContestMove::Barrier => "barrier",
        ContestMove::Report => "report",
    }
}
/// `Step 2 · fan out · 4 assignments`
pub fn step_label(step: &ContestStep) -> String {
    let n = step.assignments.len();
    let mut label = format!("Step {} · {}", step.ordinal, move_label(&step.move_name));
    if n > 0 {
        label.push_str(&format!(
            " · {n} assignment{}",
            if n == 1 { "" } else { "s" }
        ));
    }
    label
}
/// Visibility scope shown beside a step, so the human knows what members can see.
pub fn visibility_label(kind: ContestKind, step: &ContestStep) -> Option<&'static str> {
    let blind = matches!(step.move_name, ContestMove::Fanout);
    match kind {
        ContestKind::Census if blind && step.ordinal <= 1 => {
            Some("Blind: members cannot see one another's findings")
        }
        ContestKind::Crosscheck if blind && step.ordinal <= 1 => {
            Some("Plan hidden: inventory writers do not receive the plan")
        }
        ContestKind::Survey if blind => {
            Some("Blind: members receive claims without whole answers, authorship, or vote counts")
        }
        ContestKind::Probe if blind && step.ordinal <= 1 => {
            Some("Blind: checkers do not see explanations")
        }
        _ => None,
    }
}

/// One member's work in a step. Keyed by work id so the card never moves when
/// siblings finish, fail or retry.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkCard {
    pub work_id: WorkId,
    pub target: String,
    pub attempt: u32,
    pub status: WorkStatus,
    pub error: Option<String>,
}
impl WorkCard {
    pub fn from_event(w: &WorkStateEvent) -> Self {
        Self {
            work_id: w.work_id,
            target: match &w.target {
                Target::Member(n) => n.to_string(),
                other => format!("{other:?}").to_lowercase(),
            },
            attempt: w.attempt,
            status: w.status.clone(),
            error: w.error.as_ref().map(|e| e.text.to_string()),
        }
    }
    pub fn line(&self) -> String {
        let status = match self.status {
            WorkStatus::Queued => "queued",
            WorkStatus::Running => "running",
            WorkStatus::Completed => "done",
            WorkStatus::Failed => "failed",
            WorkStatus::Cancelled => "stopped",
        };
        let retry = if self.attempt > 1 {
            format!(" · retry {}", self.attempt - 1)
        } else {
            String::new()
        };
        match &self.error {
            Some(e) if self.status == WorkStatus::Failed => {
                format!("{} · {status}{retry} · {e}", self.target)
            }
            _ => format!("{} · {status}{retry}", self.target),
        }
    }
    /// A newer attempt replaces an older one; stale events never regress a card.
    pub fn accepts(&self, next: &WorkCard) -> bool {
        next.attempt >= self.attempt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Running,
    Complete,
    Stopped,
    Failed,
}
pub fn outcome(state: &ContestState) -> RunOutcome {
    let word = match state {
        ContestState::Generic(s) => s.status.as_str().to_ascii_lowercase(),
        ContestState::Selection(s) => format!("{:?}", s.state).to_ascii_lowercase(),
        ContestState::Debate(s) => format!("{:?}", s.state).to_ascii_lowercase(),
        ContestState::Tournament(s) => format!("{:?}", s.state).to_ascii_lowercase(),
        ContestState::Census(s) => format!("{:?}", s.status).to_ascii_lowercase(),
        ContestState::Survey(s) => format!("{:?}", s.state).to_ascii_lowercase(),
        ContestState::Probe(s) => format!("{:?}", s.state).to_ascii_lowercase(),
        ContestState::Grill(s) => format!("{:?}", s.state).to_ascii_lowercase(),
    };
    match word.as_str() {
        "stopped" | "cancelled" | "canceled" => RunOutcome::Stopped,
        "failed" | "rejected" | "expired" => RunOutcome::Failed,
        "complete" | "completed" => RunOutcome::Complete,
        _ => RunOutcome::Running,
    }
}

/// Header line for a contest report. A stopped or failed run is incomplete and
/// never shows a completion or success message.
pub fn report_header(outcome: RunOutcome, skipped: &[String]) -> String {
    let base = match outcome {
        RunOutcome::Running => "Report in progress",
        RunOutcome::Complete if skipped.is_empty() => "Report complete",
        RunOutcome::Complete => "Report complete · some members skipped",
        RunOutcome::Stopped => "Stopped · incomplete report",
        RunOutcome::Failed => "Failed · incomplete report",
    };
    if skipped.is_empty() {
        base.to_owned()
    } else {
        format!("{base} · skipped: {}", skipped.join(", "))
    }
}
/// Lines for one step: label, visibility and the stable work cards.
pub fn step_lines(kind: ContestKind, step: &ContestStep, cards: &[&WorkCard]) -> Vec<String> {
    let mut lines = vec![step_label(step)];
    lines.extend(visibility_label(kind, step).map(str::to_owned));
    lines.extend(cards.iter().map(|c| format!("  {}", c.line())));
    lines
}
/// Tally progress counts only applications the core committed.
pub fn tally_progress(applied: u32, total: u32) -> String {
    format!("tally {applied}/{total} applications committed")
}
