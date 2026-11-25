//! Mapping from frozen configuration and already authorized work material.
//!
//! Callers obtain material through the work-bound reader; these adapters never
//! fetch a room timeline or widen a reader's visibility boundary.
use serde_json::{Value, json};
use squad_protocol::*;

use super::{Inputs, PromptError, PromptPack};

/// The router-delivered source record for a member's debate context.
pub enum DebateSource<'a> {
    Opening(&'a DebateOpening),
    Setup(&'a DebateSetup),
}
impl DebateSource<'_> {
    fn identity(&self) -> (DebateId, [MemberNick; 2], u32) {
        match self {
            Self::Opening(v) => (v.debate_id, v.debaters.clone(), v.rounds),
            Self::Setup(v) => (v.debate_id, v.solutions.clone().map(|s| s.nick), v.rounds),
        }
    }
}

/// Material from one case, never assembled from ordinary member histories.
pub enum CaseInput<'a> {
    Debate(&'a DebateAdjudicationBundle),
    Standalone {
        question: &'a str,
        quotes: &'a [ResolvedQuote],
    },
    Followup {
        question: &'a str,
        history: &'a [HistoryEntry],
    },
}

fn error(message: &str) -> PromptError {
    PromptError::new("work-inputs", None, message)
}
fn quotes_value(quotes: &[ResolvedQuote]) -> Value {
    Value::Array(
        quotes
            .iter()
            .map(|q| json!({"author": q.author, "text": q.text}))
            .collect(),
    )
}
fn base(config: &SquadConfig) -> Inputs {
    Inputs::from([
        (
            "member_nicks_json".into(),
            json!(
                config
                    .agents
                    .iter()
                    .map(|a| a.nick.as_str())
                    .collect::<Vec<_>>()
            ),
        ),
        ("user_name_json".into(), json!(config.user.name)),
    ])
}

impl PromptPack {
    /// Keep exactly the slots used by the selected composition. This also keeps
    /// member personas out of every judge composition.
    fn selected(
        &self,
        id: &str,
        mut values: Inputs,
        repair: Option<&JudgeRepairData>,
    ) -> Result<Inputs, PromptError> {
        if let Some(repair) = repair {
            values.insert("invalid_output_json".into(), json!(repair.rejected_output));
            values.insert(
                "validation_errors_json".into(),
                json!(repair.validation_errors),
            );
        }
        let (system, user) = self.composition_source(id, repair.is_some())?;
        let required = super::render::placeholders(id, &format!("{system}\n{user}"))?;
        values.retain(|key, _| required.contains(key));
        Ok(values)
    }

    pub fn room_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        question: &str,
        quotes: &[ResolvedQuote],
    ) -> Result<Inputs, PromptError> {
        work.validate()
            .map_err(|_| error("invalid executable work"))?;
        if work.kind != WorkKind::Reply {
            return Err(error("expected reply work"));
        }
        let mut values = member_inputs(config, work, binding)?;
        values.insert("user_text_json".into(), json!(question));
        values.insert("quoted_messages_json".into(), quotes_value(quotes));
        self.selected("reply", values, None)
    }

    pub fn debate_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        source: DebateSource<'_>,
        work_values: &std::collections::BTreeMap<String, String>,
    ) -> Result<(&'static str, Inputs), PromptError> {
        let (debate, debaters, source_rounds) = source.identity();
        let rounds = work_values
            .get("rounds")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| error("missing or invalid frozen rounds"))?;
        if rounds != source_rounds {
            return Err(error("frozen rounds differ from admitted source"));
        }
        match &source {
            DebateSource::Setup(v) => v.validate().map_err(|_| error("invalid debate setup"))?,
            DebateSource::Opening(v) if v.round != 0 => {
                return Err(error("opening source requires round zero"));
            }
            _ => {}
        }
        if work.round == Some(0) && !matches!(source, DebateSource::Opening(_)) {
            return Err(error("round zero requires opening material"));
        }
        work.validate_round_bound(rounds)
            .map_err(|_| error("invalid debate work"))?;
        if work.kind != WorkKind::DebateTurn || work.debate_id != Some(debate) {
            return Err(error("debate identity mismatch"));
        }
        let Target::Member(nick) = &work.target else {
            return Err(error("expected member"));
        };
        let opponent = work_values
            .get("opponent")
            .and_then(|value| MemberNick::new(value).ok())
            .ok_or_else(|| error("missing or invalid frozen opponent"))?;
        if &opponent == nick || !debaters.contains(nick) || !debaters.contains(&opponent) {
            return Err(error("frozen opponent is not the other admitted debater"));
        }
        let round = work.round.ok_or_else(|| error("missing round"))?;
        let id = debate_composition(round, rounds)?;
        let mut values = member_inputs(config, work, binding)?;
        let words = if round > 0 && round == rounds {
            config.prompts.limits.final_turn_words
        } else {
            config.prompts.limits.turn_words
        };
        values.extend(Inputs::from([
            ("debate_id".into(), json!(debate)), ("opponent".into(), json!(opponent)),
            ("round".into(), json!(round)), ("rounds".into(), json!(rounds)),
            ("turn_word_limit".into(), json!(words)),
            ("round_data_json".into(), json!({"debate_id": debate, "round": round, "rounds": rounds, "self": nick, "opponent": opponent, "closing_required": round > 0 && round == rounds})),
        ]));
        Ok((id, self.selected(id, values, None)?))
    }

    /// `case_id` is the identity attached to the authorized material by its
    /// reader. It must match the executor's work, including on follow-ups.
    pub fn judge_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        case_id: CaseId,
        material: CaseInput<'_>,
        repair: Option<&JudgeRepairData>,
    ) -> Result<(&'static str, Inputs), PromptError> {
        work.validate().map_err(|_| error("invalid judge work"))?;
        if work.case_id != Some(case_id)
            || work.target != Target::Judge
            || !matches!(work.kind, WorkKind::Judge | WorkKind::JudgeRound)
        {
            return Err(error("case identity mismatch"));
        }
        if repair.is_some_and(|r| r.attempt != work.attempt) {
            return Err(error("repair attempt mismatch"));
        }
        let mut values = base(config);
        values.insert("judge_model_slug".into(), json!(binding.model));
        values.insert("case_id".into(), json!(case_id));
        let id = match material {
            CaseInput::Debate(bundle) => {
                let debate = work
                    .debate_id
                    .ok_or_else(|| error("missing debate identity"))?;
                let debaters = bundle.solutions.clone().map(|s| s.nick);
                let mut seen_turns = std::collections::BTreeSet::new();
                if bundle
                    .transcript
                    .iter()
                    .any(|t| !seen_turns.insert((t.round, t.speaker.clone())))
                {
                    return Err(error("duplicate transcript outcome"));
                }
                if debaters[0] == debaters[1]
                    || bundle
                        .transcript
                        .iter()
                        .any(|t| t.debate_id != debate || !debaters.contains(&t.speaker))
                    || bundle
                        .round_scoring
                        .round_scores
                        .iter()
                        .any(|s| s.debate_id != debate || s.case_id != case_id)
                {
                    return Err(error("foreign debate material"));
                }
                values.insert("debate_id".into(), json!(debate));
                values.insert("debater_a_json".into(), json!(debaters[0]));
                values.insert("debater_b_json".into(), json!(debaters[1]));
                if work.kind == WorkKind::JudgeRound {
                    let round = work.round.ok_or_else(|| error("missing target round"))?;
                    let transcript: Vec<_> = bundle
                        .transcript
                        .iter()
                        .filter(|t| t.round <= round)
                        .collect();
                    for nick in &debaters {
                        if transcript
                            .iter()
                            .filter(|t| t.round == round && &t.speaker == nick)
                            .count()
                            != 1
                        {
                            return Err(error("target round requires both accepted outcomes once"));
                        }
                    }
                    values.insert("round".into(), json!(round));
                    values.insert("round_judge_data_json".into(), json!({"topic": bundle.topic, "solutions": bundle.solutions, "instruction": bundle.instruction, "transcript": transcript}));
                    values.insert(
                        "round_score_schema_json".into(),
                        round_score_schema(&debaters),
                    );
                    "judge.round"
                } else {
                    verdict::exact_keys(&bundle.round_scoring.round_tally, &debaters)
                        .map_err(|_| error("invalid application tally"))?;
                    let mut bundle = bundle.clone();
                    bundle
                        .round_scoring
                        .round_scores
                        .sort_by_key(|score| score.round);
                    values.insert("judge_data_json".into(), json!(bundle));
                    values.insert("verdict_schema_json".into(), verdict_schema(&debaters));
                    "judge.debate"
                }
            }
            CaseInput::Standalone { question, quotes } => {
                if work.kind != WorkKind::Judge || work.debate_id.is_some() || quotes.is_empty() {
                    return Err(error("expected standalone judge work"));
                }
                values.insert("user_text_json".into(), json!(question));
                values.insert("quoted_messages_json".into(), quotes_value(quotes));
                "judge.standalone"
            }
            CaseInput::Followup { question, history } => {
                if work.kind != WorkKind::Judge {
                    return Err(error("expected follow-up judge work"));
                }
                values.insert("user_text_json".into(), json!(question));
                values.insert("case_data_json".into(), json!(unique_history(history)));
                "judge.followup"
            }
        };
        Ok((id, self.selected(id, values, repair)?))
    }

    /// Render only an accepted verdict checked against the admitted debate and
    /// the application tally. Context projects this as user/squad, never system.
    pub fn render_verdict_announcement(
        &self,
        accepted: &DebateVerdict,
        state: &DebateState,
        tally: &std::collections::BTreeMap<MemberNick, u32>,
        scores: &[DebateRoundScore],
    ) -> Result<String, PromptError> {
        accepted
            .validate_against(state, tally)
            .map_err(|_| error("verdict is not accepted against this debate"))?;
        let mut scores = scores.to_vec();
        scores.sort_by_key(|score| score.round);
        let mut seen = std::collections::BTreeSet::new();
        for score in &scores {
            score
                .validate_against(state)
                .map_err(|_| error("foreign round score"))?;
            if !seen.insert(score.round) {
                return Err(error("duplicate round score"));
            }
        }
        self.render_template(
            "debate.verdict",
            &Inputs::from([
                ("debate_id".into(), json!(accepted.debate_id)),
                ("judge_model_slug".into(), json!(accepted.completed.model)),
                (
                    "verdict_result".into(),
                    json!(format!("{} wins", accepted.verdict.verdict.winner)),
                ),
                ("verdict_json".into(), json!(accepted.verdict)),
                ("round_scores_json".into(), json!(scores)),
            ]),
        )
    }

    pub fn render_debate_source(&self, source: DebateSource<'_>) -> Result<String, PromptError> {
        let (id, values) = match source {
            DebateSource::Opening(v) => (
                "debate.opening",
                Inputs::from([
                    ("debate_id".into(), json!(v.debate_id)),
                    ("topic_json".into(), json!(v.topic_text)),
                    ("debate_instruction_json".into(), json!(v.instruction)),
                ]),
            ),
            DebateSource::Setup(v) => (
                "debate.setup",
                Inputs::from([
                    ("debate_id".into(), json!(v.debate_id)),
                    ("topic_json".into(), json!(v.topic_text)),
                    ("debate_instruction_json".into(), json!(v.instruction)),
                    ("solutions_json".into(), json!(v.solutions)),
                ]),
            ),
        };
        self.render_template(id, &values)
    }
}

fn member_inputs(
    config: &SquadConfig,
    work: &Work,
    binding: &ExecutorBinding,
) -> Result<Inputs, PromptError> {
    let Target::Member(nick) = &work.target else {
        return Err(error("expected member target"));
    };
    let agent = config
        .agents
        .iter()
        .find(|a| &a.nick == nick)
        .ok_or_else(|| error("unconfigured member"))?;
    let mut values = base(config);
    values.extend(Inputs::from([
        ("nick".into(), json!(nick)),
        ("model_slug".into(), json!(binding.model)),
        ("persona_json".into(), json!(agent.persona)),
        (
            "room_word_limit".into(),
            json!(config.prompts.limits.room_words),
        ),
    ]));
    Ok(values)
}

pub fn debate_composition(round: u32, rounds: u32) -> Result<&'static str, PromptError> {
    if rounds == 0 || round > rounds {
        return Err(error("invalid round bound"));
    }
    Ok(match (round, round == rounds) {
        (0, _) => "debate.round0",
        (1, true) => "debate.final.round1",
        (1, false) => "debate.round1",
        (_, true) => "debate.final.rebuttal",
        _ => "debate.rebuttal",
    })
}

/// Deduplicate by durable source identity, never by text (two equal answers
/// from different authors remain separate). Preserve first-delivery order.
pub fn unique_history(history: &[HistoryEntry]) -> Vec<&HistoryEntry> {
    let mut seen = std::collections::HashSet::new();
    history
        .iter()
        .filter(|entry| match &entry.meta {
            HistoryMetadata::Attributed {
                origin_event_id, ..
            } => seen.insert(*origin_event_id),
            HistoryMetadata::Anonymous { .. } => true,
        })
        .collect()
}

pub fn round_score_schema(debaters: &[MemberNick; 2]) -> Value {
    let mut schema = validation::schema::<RoundJudgeResponse>();
    schema["properties"]["scores"] = json!({"type":"object", "additionalProperties":false, "required":debaters, "properties": {
        debaters[0].as_str(): {"$ref":"#/$defs/RoundScore"}, debaters[1].as_str(): {"$ref":"#/$defs/RoundScore"}
    }});
    schema["$defs"]["RoundScore"]["properties"]["reason"]["minLength"] = json!(1);
    schema["$defs"]["RoundScore"]["properties"]["reason"]["maxLength"] = json!(200);
    schema
}
pub fn verdict_schema(debaters: &[MemberNick; 2]) -> Value {
    let mut schema = validation::schema::<JudgeVerdict>();
    schema["properties"]["winner"] = json!({"type":"string", "enum":debaters});
    schema["$defs"]["JudgeScore"]["properties"]["nick"] = json!({"type":"string", "enum":debaters});
    schema
}

/// Map the currently compiled model-bearing moves. Actor-specific Work and
/// material must still pass the owning input adapter above; this selector never
/// turns a fanout assignment into a shared payload. Unbuilt mode kinds have no
/// mapping until their typed contracts are available.
pub fn shared_move_composition(
    move_name: &ContestMove,
    work: &Work,
    total_rounds: u32,
) -> Result<Option<&'static str>, PromptError> {
    work.validate()
        .map_err(|_| error("invalid executable work"))?;
    Ok(match move_name {
        ContestMove::Pick | ContestMove::Tally | ContestMove::Barrier | ContestMove::Report => None,
        ContestMove::Collect if work.kind == WorkKind::Reply => Some("reply"),
        ContestMove::Fanout if work.kind == WorkKind::CrosscheckInventory => {
            Some("crosscheck.inventory")
        }
        ContestMove::Fanout if work.kind == WorkKind::CrosscheckCell => Some("crosscheck.cell"),
        ContestMove::Fanout if work.kind == WorkKind::CrosscheckAudit => Some("crosscheck.audit"),
        ContestMove::Judge if work.kind == WorkKind::CrosscheckGoal => {
            Some("crosscheck.normalize.extract_goal")
        }
        ContestMove::Judge if work.kind == WorkKind::CrosscheckNormalize => {
            Some("crosscheck.normalize.normalize_inventory")
        }
        ContestMove::Judge if work.kind == WorkKind::CrosscheckRule => Some("crosscheck.rule"),
        ContestMove::Fanout | ContestMove::Turns if work.kind == WorkKind::DebateTurn => {
            Some(debate_composition(
                work.round.ok_or_else(|| error("missing debate round"))?,
                total_rounds,
            )?)
        }
        ContestMove::Judge if work.kind == WorkKind::JudgeRound => Some("judge.round"),
        ContestMove::Judge if work.kind == WorkKind::Judge => {
            let followup = work.prompt_ref.as_ref().is_some_and(|p| {
                matches!(
                    p.template_id.as_str(),
                    "judge.followup" | "judge.followup.md"
                )
            });
            Some(if followup {
                "judge.followup"
            } else if work.debate_id.is_some() {
                "judge.debate"
            } else {
                "judge.standalone"
            })
        }
        _ => return Err(error("move has no compiled work composition")),
    })
}
