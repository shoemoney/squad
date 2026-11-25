//! One case-bound call per durable grant. Repair scheduling remains in the router.
use super::*;
use crate::prompts::inputs::CaseInput;
use squad_agents::{
    accounting::acknowledged,
    agents::{ExecutionError, failed, identity, stream_attempt},
    judge::{JudgeAttemptRunner, JudgeExecutionFuture, JudgeOutput, JudgeValidation},
};
use std::collections::BTreeMap;

pub(super) struct Runner {
    pub client: OpenRouterClient,
    pub config: SquadConfig,
    pub pack: Arc<PromptPack>,
    pub models: squad_agents::resolver::Catalog,
}
pub(super) struct Executor(pub squad_agents::judge::JudgeExecutor);
impl squad_agents::worker::WorkExecutor for Executor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> squad_agents::worker::ExecutionFuture<'a> {
        self.0.execute(context, grant, cancel)
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
            let prepared = self.prepare(context, &mut grant).await;
            let (request, debaters, tally, invocation, rubric) = match prepared {
                Ok(prepared) => prepared,
                Err(e) => {
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
                        let code = match e {
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
                        return failed(context, &grant, code).await;
                    }
                };
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
        let invocation: EventId = serde_json::from_value(state["judgeInvocationId"].clone())
            .map_err(|_| ReaderError::NotRunnable)?;
        let history = context.history.for_work(grant.work.work_id).await?;
        let case = grant.work.case_id.ok_or(ReaderError::NotRunnable)?;
        let mut debaters = None;
        let mut tally = BTreeMap::new();
        let mut rubric = Text::new("v1").expect("version");
        let followup =
            grant.work.prompt_ref.as_ref().is_some_and(|p| {
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
        .map_err(|_| ReaderError::NotRunnable)?;
        if grant.work.prompt_ref.is_none() {
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
        .await?;
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
