//! One authorized judge attempt. Scheduling, persistence of repair records and
//! round-score exhaustion belong to the router; this module never retries a
//! transport call or reads another case's context.
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use squad_protocol::{constants as limits, *};

use crate::openrouter::{ChatMessage, ChatRequest, DeadlineClass, ExecutionPolicy};

#[derive(Deserialize)]
struct RenderedPrompt {
    system: String,
    user: String,
    provider_user_name: String,
    step: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JudgeOutput {
    Round(RoundJudgeResponse),
    Verdict(Verdict),
    Ruling(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JudgeValidation {
    Accepted(JudgeOutput),
    /// Persist this against the next authorized attempt, never in history.
    Repair(JudgeRepairData),
    /// The router records scores:null/reason:unscored for round work, or the
    /// JUDGE_INVALID_VERDICT terminal failure for final verdict work.
    Exhausted {
        code: ErrorCode,
        unscored_round: bool,
    },
}

fn forbidden<T>() -> Result<T, ReaderError> {
    Err(ReaderError::Forbidden)
}

/// Render through the opaque protocol capability, adding only the repair record
/// authorized for this exact work attempt. Values are the owning core adapter's
/// slots (JSON values serialized once at this trait boundary).
pub async fn prepare_attempt(
    context: &AgentContext,
    grant: &ExecutionGrant,
    mut values: BTreeMap<String, String>,
    supports_reasoning: bool,
    published_output_limit: u32,
) -> Result<ChatRequest, ReaderError> {
    let work = &grant.work;
    if context.executor != grant.owner
        || !grant.owner.owns(work)
        || work.validate().is_err()
        || !matches!(work.kind, WorkKind::Judge | WorkKind::JudgeRound)
        || work.attempt > attempt_limit(work)
        || values.contains_key("invalid_output_json")
        || values.contains_key("validation_errors_json")
    {
        return forbidden();
    }
    let repair = context
        .work
        .repair_for_attempt(work.work_id, work.attempt)
        .await?;
    if let Some(repair) = repair {
        if repair.attempt != work.attempt || work.attempt == 1 {
            return forbidden();
        }
        values.insert(
            "invalid_output_json".into(),
            serde_json::to_string(&repair.rejected_output).expect("string JSON"),
        );
        values.insert(
            "validation_errors_json".into(),
            serde_json::to_string(&repair.validation_errors).expect("string vector JSON"),
        );
    }
    let prompt = work.prompt_ref.as_ref().ok_or(ReaderError::NotRunnable)?;
    let rendered: RenderedPrompt = serde_json::from_str(&context.render.render(prompt, &values)?)
        .map_err(|_| ReaderError::NotRunnable)?;
    let expected = if work.kind == WorkKind::JudgeRound {
        "judge.round"
    } else if rendered.step == "judge.followup" {
        "judge.followup"
    } else if work.debate_id.is_some() {
        "judge.debate"
    } else {
        "judge.standalone"
    };
    if rendered.step != expected || rendered.provider_user_name != "squad" {
        return forbidden();
    }
    let round = work.kind == WorkKind::JudgeRound;
    Ok(ChatRequest {
        model: grant.binding.model.as_str().to_owned(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                name: None,
                content: rendered.system,
            },
            ChatMessage {
                role: "user".into(),
                name: Some("squad".into()),
                content: rendered.user,
            },
        ],
        visible_ceiling: work.max_output_tokens.unwrap_or(if round {
            limits::ROUND_JUDGE_VISIBLE_TOKENS
        } else {
            limits::FINAL_JUDGE_VISIBLE_TOKENS
        }),
        published_output_limit,
        supports_reasoning,
        reasoning_effort: if round {
            ReasoningEffort::Low
        } else {
            grant.binding.effort
        },
        reasoning_explicit: false,
        response_format: None,
        policy: ExecutionPolicy::SingleAttempt(if round {
            DeadlineClass::RoundJudge
        } else {
            DeadlineClass::FinalJudge
        }),
    })
}

fn attempt_limit(work: &Work) -> u32 {
    u32::from(if work.kind == WorkKind::JudgeRound {
        limits::ROUND_JUDGE_ATTEMPTS
    } else {
        limits::FINAL_JUDGE_ATTEMPTS
    })
}

/// Validate only with M1's canonical validators. The supplied tally is the
/// router's committed value; this worker never accepts totals from the model.
/// Standalone and follow-up cases return free-form rulings.
pub fn validate_output(
    work: &Work,
    raw: &str,
    debaters: Option<&[MemberNick; 2]>,
    tally: &BTreeMap<MemberNick, u32>,
) -> Result<JudgeValidation, ReaderError> {
    if work.validate().is_err()
        || !matches!(work.kind, WorkKind::Judge | WorkKind::JudgeRound)
        || work.attempt > attempt_limit(work)
    {
        return forbidden();
    }
    // A retained debate follow-up is free-form as well: the selected prompt,
    // rather than debateId alone, establishes whether a verdict is required.
    let followup = work.prompt_ref.as_ref().is_some_and(|p| {
        matches!(
            p.template_id.as_str(),
            "judge.followup" | "judge.followup.md"
        )
    });
    if work.kind == WorkKind::Judge && (work.debate_id.is_none() || followup) {
        return Ok(JudgeValidation::Accepted(JudgeOutput::Ruling(
            raw.to_owned(),
        )));
    }
    let debaters = debaters.ok_or(ReaderError::NotRunnable)?;
    let value = serde_json::from_str::<Value>(raw).map_err(|_| "invalid JSON".to_owned());
    let validated = value.and_then(|value| {
        if work.kind == WorkKind::JudgeRound {
            verdict::validate_round_response(&value, debaters).map(JudgeOutput::Round)
        } else {
            verdict::validate_judge_verdict(&value, debaters, tally).map(JudgeOutput::Verdict)
        }
        .map_err(|error| error.message)
    });
    Ok(match validated {
        Ok(output) => JudgeValidation::Accepted(output),
        Err(message) if work.attempt < attempt_limit(work) => {
            JudgeValidation::Repair(JudgeRepairData {
                attempt: work.attempt + 1,
                rejected_output: raw.to_owned(),
                validation_errors: vec![message],
            })
        }
        Err(_) => JudgeValidation::Exhausted {
            code: ErrorCode::JUDGE_INVALID_VERDICT,
            unscored_round: work.kind == WorkKind::JudgeRound,
        },
    })
}

/// Thin integration seam for the shared worker's WorkExecutor. The runner owns
/// the transport/accounting/result adapter; it must make one authorized call.
/// Switch the implemented trait to worker::WorkExecutor when integrating.
pub type JudgeExecutionFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ReaderError>> + Send + 'a>>;
pub trait JudgeAttemptRunner: Send + Sync {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: tokio_util::sync::CancellationToken,
    ) -> JudgeExecutionFuture<'a>;
}
pub struct JudgeExecutor {
    pub runner: std::sync::Arc<dyn JudgeAttemptRunner>,
}
impl JudgeExecutor {
    pub fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: tokio_util::sync::CancellationToken,
    ) -> JudgeExecutionFuture<'a> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Ok(());
            }
            if context.executor != grant.owner
                || !grant.owner.owns(&grant.work)
                || !matches!(
                    grant.work.kind,
                    WorkKind::Judge
                        | WorkKind::JudgeRound
                        | WorkKind::SurveyExtract
                        | WorkKind::SurveyReport
                )
            {
                return forbidden();
            }
            self.runner.execute(context, grant, cancel).await
        })
    }
}
