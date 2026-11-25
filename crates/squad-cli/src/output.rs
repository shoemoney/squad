use serde_json::{Value, json};
use squad_protocol::*;
use std::io::{self, Write};

pub struct Output<'a> {
    pub json: bool,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}
impl Output<'_> {
    pub fn record(&mut self, value: Value) -> io::Result<()> {
        serde_json::to_writer(&mut self.stdout, &value)?;
        writeln!(self.stdout)
    }
    pub fn error(
        &mut self,
        id: Option<EventId>,
        code: ErrorCode,
        message: &str,
        retryable: bool,
    ) -> io::Result<()> {
        if self.json {
            let mut value = json!({"type":"error","error":{"code":code,"message":message,"retryable":retryable}});
            if let Some(id) = id {
                value["requestId"] = json!(id);
            }
            self.record(value)
        } else {
            writeln!(self.stderr, "{message}")
        }
    }
    pub fn ack(
        &mut self,
        request_id: EventId,
        event_ids: Vec<EventId>,
        cursor: Option<Cursor>,
    ) -> io::Result<()> {
        if self.json {
            let mut value = json!({"type":"ack","requestId":request_id,"eventIds":event_ids});
            if let Some(cursor) = cursor {
                value["cursor"] = json!(cursor);
            }
            self.record(value)
        } else if let Some(cursor) = cursor {
            writeln!(self.stdout, "Accepted (cursor {}).", cursor.get())
        } else {
            writeln!(self.stdout, "Accepted.")
        }
    }
    pub fn preflight(
        &mut self,
        request_id: EventId,
        preflight: &PendingPreflight,
    ) -> io::Result<()> {
        let (kind, mut value) = match preflight {
            PendingPreflight::Debate(p) => ("debate".to_owned(), json!(p)),
            PendingPreflight::Contest(p) => (
                serde_json::to_value(p.kind)?
                    .as_str()
                    .unwrap_or("contest")
                    .to_owned(),
                json!(p),
            ),
        };
        if self.json {
            value["type"] = json!(format!("{kind}.preflight"));
            value["requestId"] = json!(request_id);
            self.record(value)
        } else {
            let estimate = match preflight {
                PendingPreflight::Debate(p) => &p.estimate,
                PendingPreflight::Contest(p) => &p.estimate,
            };
            writeln!(
                self.stdout,
                "{kind} preflight: {} model calls; estimate ${:.2}; reserved ${:.2}; remaining ${:.2}",
                estimate.model_calls,
                estimate.estimated_usd,
                estimate.reserved_usd,
                estimate.remaining_usd
            )?;
            match preflight {
                PendingPreflight::Debate(p) => {
                    writeln!(
                        self.stdout,
                        "Rounds: {}; debaters: {}, {}; judge: {}",
                        p.rounds,
                        p.debaters[0],
                        p.debaters[1],
                        p.judge_model.as_deref().unwrap_or("none")
                    )?;
                    if let Some(warning) = &p.warning {
                        writeln!(self.stdout, "Warning: {warning}")?;
                    }
                }
                PendingPreflight::Contest(p) => {
                    writeln!(
                        self.stdout,
                        "Participants: {}",
                        p.participants
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                    writeln!(
                        self.stdout,
                        "Settings: {}",
                        serde_json::to_string_pretty(&p.settings)?
                    )?;
                    if let Some(warning) = &p.warning {
                        writeln!(self.stdout, "Warning: {warning}")?;
                    }
                }
            }
            Ok(())
        }
    }
    /// Serialize the shared event intact, including scores, unscored markers,
    /// tallies and mode-specific reports. No ephemeral deltas reach this method.
    pub fn event(&mut self, event: &CommittedEnvelope) -> io::Result<()> {
        self.event_with_round(event, None)
    }
    pub fn event_with_round(
        &mut self,
        event: &CommittedEnvelope,
        latest_round: Option<u32>,
    ) -> io::Result<()> {
        if self.json {
            self.record(json!({"type":"event","cursor":event.commit.seq,"event":event}))
        } else {
            let value = serde_json::to_value(event)?;
            let author = value["sender"].as_str().unwrap_or("system");
            let kind = value["kind"].as_str().unwrap_or("event");
            let payload = &value["payload"];
            if let EnvelopePayload::DebateRoundScore(score) = &event.content {
                let late = if latest_round.is_some_and(|round| round > score.round) {
                    " (late)"
                } else {
                    ""
                };
                let status = if score.scores.is_none() {
                    "unscored"
                } else {
                    "score"
                };
                return writeln!(
                    self.stdout,
                    "Round {} {status}{late}: {}",
                    score.round,
                    serde_json::to_string_pretty(payload)?
                );
            }
            if kind == "message.final" {
                writeln!(
                    self.stdout,
                    "{author}: {}",
                    payload["content"]
                        .as_str()
                        .or_else(|| payload["body"].as_str())
                        .unwrap_or("")
                )
            } else {
                // Field labels retain the owning round/step and attribution even
                // for late scores; committed values are never recomputed here.
                writeln!(
                    self.stdout,
                    "{author} [{kind}] {}",
                    serde_json::to_string_pretty(payload)?
                )
            }
        }
    }
}
