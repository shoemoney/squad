//! Negotiate committed-event projection. Draft content is collapsed by default.
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<NegotiateState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::NegotiateState(s)
            | EnvelopePayload::ContestState(ContestState::Negotiate(s)) => {
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
                if a.kind == Some(ContestKind::Negotiate)
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
pub fn lines(s: &NegotiateState) -> Vec<String> {
    vec![
        "NEGOTIATE".into(),
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
pub fn render(v: &View) -> Vec<String> {
    render_width(v, 80)
}
/// Cell callers with a measured width can render the three tables side by side.
pub fn render_width(v: &View, width: usize) -> Vec<String> {
    let mut out = v.state.as_ref().map(lines).unwrap_or_default();
    out.push("PREP".into());
    for a in v
        .artifacts
        .values()
        .filter(|a| a.artifact_kind.as_str() == "negotiate.roles")
    {
        if let Some(b) = &a.body
            && let Ok(setup) = validation::decode::<negotiate::NegotiateSetup>(b)
        {
            for t in setup.tables {
                out.push(format!(
                    "TABLE {}: @{} negotiator / @{} counterpart",
                    t.table, t.negotiator, t.counterpart
                ));
            }
        }
    }
    if !v
        .artifacts
        .values()
        .any(|a| a.artifact_kind.as_str() == "negotiate.report")
    {
        let mut turns = v
            .artifacts
            .values()
            .filter(|a| a.artifact_kind.as_str() == "negotiate.turn")
            .filter_map(|a| a.body.as_ref())
            .filter_map(|b| validation::decode::<negotiate::NegotiateTurn>(b).ok())
            .collect::<Vec<_>>();
        turns.sort_by_key(|t| (t.table, t.round, t.counterpart));
        let mut columns = std::collections::BTreeMap::<u32, Vec<String>>::new();
        for t in turns {
            let lines = columns
                .entry(t.table)
                .or_insert_with(|| vec![format!("TABLE {}", t.table)]);
            lines.push(format!(
                "R{} {}: {}",
                t.round,
                if t.counterpart {
                    "counterpart"
                } else {
                    "negotiator"
                },
                t.body["message"].as_str().unwrap_or_default()
            ));
        }
        out.extend(layout(columns.into_values().collect(), width));
    }
    for a in v
        .artifacts
        .values()
        .filter(|a| a.artifact_kind.as_str() == "negotiate.report")
    {
        if let Some(b) = &a.body
            && let Ok(r) = validation::decode::<negotiate::NegotiateReport>(b)
        {
            out.extend(preparation(&r));
            let columns = r
                .tables
                .iter()
                .map(|t| {
                    let mut lines = vec![format!(
                        "TABLE {} · {}",
                        t.table,
                        squad_core::export::negotiate::outcome_label(t.outcome)
                    )];
                    for turn in &t.turns {
                        lines.push(format!(
                            "R{} {}: {}",
                            turn.round,
                            if turn.counterpart {
                                "counterpart"
                            } else {
                                "negotiator"
                            },
                            turn.body["message"].as_str().unwrap_or_default()
                        ));
                    }
                    lines
                })
                .collect::<Vec<_>>();
            out.extend(layout(columns, width));
            out.push("PLAYBOOK".into());
            if r.no_rehearsal_evidence {
                out.push("No rehearsal evidence".into());
            }
            if let Some(p) = r.playbook {
                out.push(format!("Opening: {}", p.opening));
                for anchor in p.anchors {
                    out.push(format!("Anchor: {anchor}"));
                }
                for c in p.concession_ladder {
                    out.push(format!("Give {} only for {}", c.give, c.only_for));
                }
                for o in p.objections {
                    out.push(format!("Objection: {} → {}", o.objection, o.response));
                }
                out.push(format!("Walk away: {}", p.walk_away_line));
                for z in p.zopa_map {
                    out.push(format!(
                        "ZOPA {}: our limit {}; modelled counterpart {}; overlap {}",
                        z.issue,
                        z.our_limit,
                        z.their_modelled_limit,
                        squad_protocol::human::label(&z.overlap)
                    ));
                }
                for q in p.questions_to_ask {
                    out.push(format!("Ask: {q}"));
                }
                for n in p.table_notes {
                    out.push(format!(
                        "TABLE {} left on table: {}",
                        n.table, n.left_on_table
                    ));
                    for c in n.unrealistic_concessions {
                        out.push(format!(
                            "TABLE {} unrealistic concession / reservation breach: {}",
                            n.table, c
                        ));
                    }
                }
                for a in p.depends_on_assumptions {
                    out.push(format!("Depends on assumption: {a}"));
                }
            }
            for skipped in r.skipped {
                out.push(format!("Skipped: {skipped}"));
            }
        }
    }
    out
}
fn layout(columns: Vec<Vec<String>>, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;
    if width < 120 || columns.len() < 2 {
        return columns.into_iter().flatten().collect();
    }
    let col = width / columns.len();
    let wrapped = columns
        .iter()
        .map(|lines| {
            lines
                .iter()
                .flat_map(|s| wrap(s, col.saturating_sub(2)))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    (0..wrapped.iter().map(Vec::len).max().unwrap_or(0))
        .map(|row| {
            wrapped
                .iter()
                .map(|c| {
                    let line = c.get(row).map(String::as_str).unwrap_or("");
                    format!(
                        "{}{}",
                        line,
                        " ".repeat(col.saturating_sub(1 + line.width()))
                    )
                })
                .collect::<Vec<_>>()
                .join("│")
        })
        .collect()
}
fn wrap(s: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let mut lines = vec![];
    let mut line = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width.max(2) && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push(c);
        used += w;
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn preparation(r: &negotiate::NegotiateReport) -> Vec<String> {
    let mut out = vec![];
    for (n, m) in &r.maps {
        out.push(format!("@{n} interests: {}", m.our_interests.join("; ")));
        out.push(format!("@{n} our BATNA: {}", m.our_batna));
        out.push(format!(
            "@{n} modelled counterpart BATNA: {}",
            m.counterpart_batna
        ));
        for i in &m.counterpart_interests {
            out.push(format!(
                "Counterpart interest ({}): {}",
                squad_protocol::human::label(&i.basis),
                i.text
            ));
        }
        for z in &m.zopa {
            out.push(format!(
                "ZOPA {}: our limit {}; their likely limit {}",
                z.issue, z.our_limit, z.their_likely_limit
            ));
        }
        for t in &m.tactics {
            out.push(format!("Tactic: {t}"));
        }
    }
    if let Some(b) = &r.brief {
        out.push(format!("Strategy: {}", b.strategy_sheet));
        for (i, h) in b.counterpart_briefs.iter().enumerate() {
            out.push(format!(
                "TABLE {} — Hypothetical counterpart, not a fact",
                i + 1
            ));
            out.push(format!(
                "{}; priorities: {}; hidden constraint: {}; walk away: {}",
                h.persona,
                h.priorities.join("; "),
                h.hidden_constraint,
                h.walk_away_rule
            ));
            for limit in &h.reservations {
                out.push(format!("Reservation: {} = {}", limit.issue, limit.limit));
            }
        }
        for a in &b.assumptions_to_confirm {
            out.push(format!("Confirm assumption: {a}"));
        }
    }
    for (n, p) in &r.packages {
        out.push(format!(
            "@{n} package: {}; give {}; get {}; walk away {}",
            p.opener,
            p.gives.join("; "),
            p.gets.join("; "),
            p.walk_away
        ));
    }
    out
}
