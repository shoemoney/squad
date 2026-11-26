//! One case-bound call per durable grant. Repair scheduling remains in the router.
use super::*;
use crate::prompts::inputs::CaseInput;
use squad_agents::{
    accounting::acknowledged,
    agents::{ExecutionError, failed, failed_with, identity, stream_attempt},
    judge::{JudgeAttemptRunner, JudgeExecutionFuture, JudgeOutput, JudgeValidation},
};
use std::collections::BTreeMap;

pub(super) struct Runner {
    pub client: OpenRouterClient,
    pub config: SquadConfig,
    pub pack: Arc<PromptPack>,
    pub models: squad_agents::resolver::Catalog,
}
pub(super) struct Executor {
    pub ordinary: squad_agents::judge::JudgeExecutor,
    pub crosscheck: Arc<Runner>,
}
impl squad_agents::worker::WorkExecutor for Executor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> squad_agents::worker::ExecutionFuture<'a> {
        if grant.work.kind.new_mode_kind().is_some()
            || grant.work.kind == WorkKind::ProbeReport
            || grant.work.kind.census_judge()
            || matches!(
                grant.work.kind,
                WorkKind::SurveyExtract | WorkKind::SurveyReport
            )
        {
            return self.crosscheck.execute(context, grant, cancel);
        }
        if grant.work.kind.is_crosscheck_judge() {
            Box::pin(super::crosscheck::execute(
                context,
                grant,
                cancel,
                &self.crosscheck.client,
                &self.crosscheck.config,
                &self.crosscheck.pack,
                &self.crosscheck.models,
            ))
        } else {
            self.ordinary.execute(context, grant, cancel)
        }
    }
}
impl JudgeAttemptRunner for Runner {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        mut grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> JudgeExecutionFuture<'a> {
        Box::pin(async move {
            if grant.work.kind.census_composition().is_some() {
                let model = self
                    .models
                    .get(grant.binding.model.as_str())
                    .ok_or(ReaderError::NotRunnable)?;
                let registry = squad_agents::registry::Registry::new(&self.config)
                    .map_err(|_| ReaderError::NotRunnable)?;
                let request = registry
                    .work_request(None, model, &grant)
                    .map_err(|_| ReaderError::NotRunnable)?;
                let executor = squad_agents::agents::census::CensusExecutor {
                    client: self.client.clone(),
                    request,
                    context_length: u64::from(model.context_length),
                };
                return squad_agents::worker::WorkExecutor::execute(
                    &executor, context, grant, cancel,
                )
                .await;
            }
            if matches!(
                grant.work.kind,
                WorkKind::SurveyExtract | WorkKind::SurveyReport
            ) {
                let model = self
                    .models
                    .get(grant.binding.model.as_str())
                    .ok_or(ReaderError::NotRunnable)?;
                let registry = squad_agents::registry::Registry::new(&self.config)
                    .map_err(|_| ReaderError::NotRunnable)?;
                let request = registry
                    .work_request(None, model, &grant)
                    .map_err(|_| ReaderError::NotRunnable)?;
                let system_values = BTreeMap::from([
                    (
                        "member_nicks_json".into(),
                        serde_json::to_string(
                            &self
                                .config
                                .agents
                                .iter()
                                .map(|a| &a.nick)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|_| ReaderError::Forbidden)?,
                    ),
                    (
                        "user_name_json".into(),
                        serde_json::to_string(&self.config.user.name)
                            .map_err(|_| ReaderError::Forbidden)?,
                    ),
                ]);
                let executor = squad_agents::agents::survey::SurveyExecutor {
                    client: self.client.clone(),
                    request,
                    system: super::prompt(&self.pack, &self.config, "judge.system"),
                    system_values,
                    context_length: u64::from(model.context_length),
                };
                return squad_agents::worker::WorkExecutor::execute(
                    &executor, context, grant, cancel,
                )
                .await;
            }
            if let Some(kind) = grant.work.kind.new_mode_kind() {
                return super::execute_new_mode(
                    kind,
                    context,
                    &grant,
                    &self.client,
                    &self.pack,
                    &self.config,
                    cancel,
                )
                .await
                .map_err(|_| ReaderError::NotRunnable);
            }
            if grant.work.kind == WorkKind::ProbeReport {
                return super::probe::execute(
                    context,
                    grant,
                    cancel,
                    &self.config,
                    &self.pack,
                    &self.models,
                    &self.client,
                )
                .await;
            }
            let prepared = self.prepare(context, &mut grant).await;
            let (request, debaters, tally, invocation, rubric) = match prepared {
                Ok(prepared) => prepared,
                Err(e) => {
                    tracing::error!(work_id=%grant.work.work_id,kind=?grant.work.kind,error=?e,"Judge preparation failed");
                    failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await?;
                    return Err(e);
                }
            };
            let output =
                match stream_attempt(context, &grant, &self.client, request, cancel, false).await {
                    Ok(output) => output,
                    Err(ExecutionError::Reader(e)) => return Err(e),
                    // Rate-limit and retry hints are terminal for judges. Only invalid
                    // output may authorize a separate router-owned repair attempt.
                    Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
                        let code = match &e {
                            squad_agents::openrouter::TransportError::Cancelled => {
                                ErrorCode::WORK_CANCELLED
                            }
                            squad_agents::openrouter::TransportError::Deadline
                            | squad_agents::openrouter::TransportError::IdleTimeout
                            | squad_agents::openrouter::TransportError::FirstTokenTimeout => {
                                ErrorCode::PROVIDER_TIMEOUT
                            }
                            _ => ErrorCode::PROVIDER_ERROR,
                        };
                        return failed_with(context, &grant, code, Some(&e)).await;
                    }
                };
            if grant.work.kind == WorkKind::JudgeTournament {
                return self
                    .tournament_output(context, &grant, &output.content)
                    .await;
            }
            let validation = squad_agents::judge::validate_output(
                &grant.work,
                &output.content,
                debaters.as_ref(),
                &tally,
            )?;
            let case = grant.work.case_id.ok_or(ReaderError::NotRunnable)?;
            let completed = CompletedGeneration {
                generation: identity(&grant),
                model: grant.binding.model.clone(),
                finish_reason: output.finish_reason,
                usage: output.usage,
            };
            let payload = match validation {
                JudgeValidation::Accepted(JudgeOutput::Ruling(content)) => {
                    EnvelopePayload::CaseRuling(CaseRuling {
                        completed,
                        case_id: case,
                        judge_invocation_id: invocation,
                        in_reply_to_event_id: grant.work.trigger_event_id,
                        content: Text::new(content).map_err(|_| ReaderError::NotRunnable)?,
                    })
                }
                JudgeValidation::Accepted(JudgeOutput::Verdict(verdict)) => {
                    EnvelopePayload::Verdict(DebateVerdict {
                        completed,
                        debate_id: grant.work.debate_id.ok_or(ReaderError::NotRunnable)?,
                        case_id: case,
                        judge_invocation_id: invocation,
                        rubric_version: rubric,
                        verdict,
                    })
                }
                JudgeValidation::Accepted(JudgeOutput::Round(round)) => {
                    EnvelopePayload::DebateRoundScore(DebateRoundScore {
                        debate_id: grant.work.debate_id.ok_or(ReaderError::NotRunnable)?,
                        case_id: case,
                        round: grant.work.round.ok_or(ReaderError::NotRunnable)?,
                        scores: Some(round.scores),
                        tally,
                        reason: None,
                    })
                }
                JudgeValidation::Repair(repair) => {
                    let mut error = crate::router::error(
                        ErrorCode::JUDGE_INVALID_VERDICT,
                        "Judge output failed validation",
                    );
                    error.work_id = Some(grant.work.work_id);
                    error.case_id = grant.work.case_id;
                    error.debate_id = grant.work.debate_id;
                    error.contest_id = grant.work.contest_id;
                    return acknowledged(
                        context
                            .submit
                            .submit(ExecutorRequest::Failed {
                                work_id: grant.work.work_id,
                                attempt: grant.work.attempt,
                                fence: grant.fence,
                                epoch: grant.epoch,
                                error,
                                repair: Some(repair),
                            })
                            .await,
                    );
                }
                JudgeValidation::Exhausted {
                    unscored_round: true,
                    ..
                } => EnvelopePayload::DebateRoundScore(DebateRoundScore {
                    debate_id: grant.work.debate_id.ok_or(ReaderError::NotRunnable)?,
                    case_id: case,
                    round: grant.work.round.ok_or(ReaderError::NotRunnable)?,
                    scores: None,
                    tally,
                    reason: Some(Text::new("unscored").expect("reason")),
                }),
                JudgeValidation::Exhausted { code, .. } => {
                    return failed(context, &grant, code).await;
                }
            };
            acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: payload,
                    })
                    .await,
            )
        })
    }
}
type Prepared = (
    squad_agents::openrouter::ChatRequest,
    Option<[MemberNick; 2]>,
    BTreeMap<MemberNick, u32>,
    EventId,
    Text,
);
impl Runner {
    async fn prepare(
        &self,
        context: &AgentContext,
        grant: &mut ExecutionGrant,
    ) -> Result<Prepared, ReaderError> {
        let model = self
            .models
            .get(grant.binding.model.as_str())
            .ok_or(ReaderError::NotRunnable)?;
        let values = context
            .work
            .prompt_values_for_work(grant.work.work_id)
            .await?;
        if grant.work.kind == WorkKind::JudgeTournament {
            let frozen: serde_json::Value = serde_json::from_str(
                values
                    .get("tournament_frozen_judge")
                    .ok_or(ReaderError::NotRunnable)?,
            )
            .map_err(|_| ReaderError::NotRunnable)?;
            let rendered: crate::prompts::RenderedComposition =
                serde_json::from_value(frozen["prompt"].clone())
                    .map_err(|_| ReaderError::NotRunnable)?;
            let mut user = rendered.user;
            if let Some(repair) = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
            {
                let suffix = frozen["repair"]
                    .as_str()
                    .ok_or(ReaderError::NotRunnable)?
                    .replace(
                        "{{invalid_output_json}}",
                        &serde_json::to_string(&repair.rejected_output)
                            .map_err(|_| ReaderError::NotRunnable)?,
                    )
                    .replace(
                        "{{validation_errors_json}}",
                        &serde_json::to_string(&repair.validation_errors)
                            .map_err(|_| ReaderError::NotRunnable)?,
                    );
                user.push_str("\n\n");
                user.push_str(&suffix);
            }
            let request = squad_agents::openrouter::ChatRequest {
                model: grant.binding.model.to_string(),
                messages: vec![
                    squad_agents::openrouter::ChatMessage {
                        role: "system".into(),
                        name: None,
                        content: rendered.system,
                    },
                    squad_agents::openrouter::ChatMessage {
                        role: "user".into(),
                        name: Some("squad".into()),
                        content: user,
                    },
                ],
                visible_ceiling: constants::FINAL_JUDGE_VISIBLE_TOKENS,
                published_output_limit: model.output_limit,
                supports_reasoning: model.supports_reasoning(),
                reasoning_effort: ReasoningEffort::High,
                reasoning_explicit: false,
                response_format: None,
                policy: squad_agents::openrouter::ExecutionPolicy::SingleAttempt(
                    squad_agents::openrouter::DeadlineClass::TournamentJudge,
                ),
            };
            let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)?;
            if squad_agents::context::estimate(&request.messages)
                > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
            {
                return Err(ReaderError::NotRunnable);
            }
            let state: serde_json::Value = serde_json::from_str(
                values
                    .get("case_state_json")
                    .ok_or(ReaderError::NotRunnable)?,
            )
            .map_err(|_| ReaderError::NotRunnable)?;
            let invocation = serde_json::from_value(state["judgeInvocationId"].clone())
                .map_err(|_| ReaderError::NotRunnable)?;
            let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)?;
            if squad_agents::context::estimate(&request.messages)
                > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
            {
                return Err(ReaderError::NotRunnable);
            }
            return Ok((
                request,
                None,
                BTreeMap::new(),
                invocation,
                Text::new("v1").unwrap(),
            ));
        }
        let material: CaseMaterial = serde_json::from_str(
            values
                .get("case_material_json")
                .ok_or(ReaderError::NotRunnable)?,
        )
        .map_err(|_| ReaderError::NotRunnable)?;
        let state: serde_json::Value = serde_json::from_str(
            values
                .get("case_state_json")
                .ok_or(ReaderError::NotRunnable)?,
        )
        .map_err(|_| ReaderError::NotRunnable)?;
        let followup_invocation = values.get("case_followup_invocation");
        let invocation: EventId = match followup_invocation {
            Some(raw) => raw.parse().map_err(|_| ReaderError::NotRunnable)?,
            None => serde_json::from_value(state["judgeInvocationId"].clone())
                .map_err(|_| ReaderError::NotRunnable)?,
        };
        let history = context.history.for_work(grant.work.work_id).await?;
        let case = grant.work.case_id.ok_or(ReaderError::NotRunnable)?;
        let mut debaters = None;
        let mut tally = BTreeMap::new();
        let mut rubric = Text::new("v1").expect("version");
        let followup = followup_invocation.is_some()
            || grant.work.prompt_ref.as_ref().is_some_and(|p| {
                p.template_id.as_str().trim_end_matches(".md") == "judge.followup"
            });
        let (id, inputs) = if followup {
            let question = history
                .iter()
                .rev()
                .find(|h| {
                    matches!(
                        h.meta,
                        HistoryMetadata::Attributed {
                            author: Actor::Me,
                            ..
                        }
                    )
                })
                .ok_or(ReaderError::NotRunnable)?;
            self.pack.judge_inputs(
                &self.config,
                &grant.work,
                &grant.binding,
                case,
                CaseInput::Followup {
                    question: &question.content,
                    history: &history,
                },
                None,
            )
        } else {
            match &material {
                CaseMaterial::Tournament { material } => self
                    .pack
                    .tournament_judge_inputs(material)
                    .map(|inputs| ("judge.tournament", inputs)),

                CaseMaterial::Survey { .. } => return Err(ReaderError::Forbidden),
                CaseMaterial::Probe(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Forge(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Tune(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::BassAckwardsPrd(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Forecast(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Decide(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Negotiate(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::BlowItUp(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Readback(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Debugging(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Trace(_) => return Err(ReaderError::NotRunnable),
                CaseMaterial::Rank(_) => return Err(ReaderError::NotRunnable),

                CaseMaterial::Standalone { messages, question } => self.pack.judge_inputs(
                    &self.config,
                    &grant.work,
                    &grant.binding,
                    case,
                    CaseInput::Standalone {
                        question: question.as_str(),
                        quotes: messages,
                    },
                    None,
                ),
                CaseMaterial::Debate { rubric_version, .. } => {
                    // The engine projects its frozen adjudication bundle into this
                    // case's history. Never rebuild it from a room timeline.
                    let bundle = history
                        .iter()
                        .rev()
                        .find_map(|h| {
                            serde_json::from_str::<DebateAdjudicationBundle>(&h.content).ok()
                        })
                        .ok_or(ReaderError::NotRunnable)?;
                    debaters = Some(bundle.solutions.clone().map(|s| s.nick));
                    tally = bundle.round_scoring.round_tally.clone();
                    rubric = rubric_version.clone();
                    self.pack.judge_inputs(
                        &self.config,
                        &grant.work,
                        &grant.binding,
                        case,
                        CaseInput::Debate(&bundle),
                        None,
                    )
                }
            }
        }
        .map_err(|e| {
            tracing::error!(error=%e,"Judge input mapping failed");
            ReaderError::NotRunnable
        })?;
        // The frozen plan records the source template hash; executors select
        // the registered composition that combines system and user templates.
        if let Some(reference) = &mut grant.work.prompt_ref {
            if grant.work.kind != WorkKind::JudgeRound {
                reference.template_id = Text::new(id).map_err(|_| ReaderError::NotRunnable)?;
            }
        } else {
            grant.work.prompt_ref = Some(super::prompt(&self.pack, &self.config, id));
        }
        let values = inputs
            .into_iter()
            .map(|(key, value)| {
                let encoded = if let serde_json::Value::String(s) = &value {
                    if !key.ends_with("_json") {
                        s.clone()
                    } else {
                        value.to_string()
                    }
                } else {
                    value.to_string()
                };
                (key, encoded)
            })
            .collect();
        let request = squad_agents::judge::prepare_attempt(
            context,
            grant,
            values,
            model.supports_reasoning(),
            model.output_limit,
        )
        .await
        .map_err(|e| {
            tracing::error!(composition=%id,error=?e,"Judge composition failed");
            e
        })?;
        let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
            .as_u64()
            .ok_or(ReaderError::NotRunnable)?;
        if squad_agents::context::estimate(&request.messages)
            > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
        {
            return Err(ReaderError::NotRunnable);
        }
        Ok((request, debaters, tally, invocation, rubric))
    }
}

impl Runner {
    async fn tournament_output(
        &self,
        context: &AgentContext,
        grant: &ExecutionGrant,
        raw: &str,
    ) -> Result<(), ReaderError> {
        let values = context
            .work
            .prompt_values_for_work(grant.work.work_id)
            .await?;
        let material: CaseMaterial = serde_json::from_str(
            values
                .get("case_material_json")
                .ok_or(ReaderError::NotRunnable)?,
        )
        .map_err(|_| ReaderError::NotRunnable)?;
        let CaseMaterial::Tournament { material } = material else {
            return Err(ReaderError::Forbidden);
        };
        let labels = material
            .entries
            .iter()
            .map(|e| e.label.clone())
            .collect::<Vec<_>>();
        match TournamentJudgeResponse::from_json(raw, &labels) {
            Ok(response) => acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: EnvelopePayload::TournamentJudgment(TournamentJudgment {
                            generation: identity(grant),
                            contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
                            case_id: grant.work.case_id.ok_or(ReaderError::NotRunnable)?,
                            round: grant.work.round.ok_or(ReaderError::NotRunnable)?,
                            replay: grant.work.replay.ok_or(ReaderError::NotRunnable)?,
                            response,
                        }),
                    })
                    .await,
            ),
            Err(_) => {
                let mut error = crate::router::error(
                    ErrorCode::JUDGE_INVALID_VERDICT,
                    "Tournament ranking failed validation",
                );
                error.work_id = Some(grant.work.work_id);
                error.case_id = grant.work.case_id;
                error.contest_id = grant.work.contest_id;
                let repair=(grant.work.attempt<3).then(||JudgeRepairData{attempt:grant.work.attempt+1,rejected_output:raw.into(),validation_errors:vec!["Return a strict permutation with integer scores 1 through 10 and one-line critiques".into()]});
                acknowledged(
                    context
                        .submit
                        .submit(ExecutorRequest::Failed {
                            work_id: grant.work.work_id,
                            attempt: grant.work.attempt,
                            fence: grant.fence,
                            epoch: grant.epoch,
                            error,
                            repair,
                        })
                        .await,
                )
            }
        }
    }
}
