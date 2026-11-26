//! Deterministic cost/context planning. No provider calls occur during planning.
use super::types::{CallBound, ContestRouter, DebatePlan, FrozenModel};
use squad_protocol::*;

#[derive(Debug, Clone, Copy)]
pub struct ContextBounds {
    /// Includes each participant's admitted private history and setup exactly once.
    pub members: [u64; 2],
    /// Topic, solutions, instruction, rubric and case system prompt.
    pub judge: u64,
}
fn cost(model: &FrozenModel, input: u64, output: u64) -> Result<f64, ErrorCode> {
    if model.resolved_slug.trim().is_empty() || model.context_tokens == 0 {
        return Err(ErrorCode::MODEL_UNAVAILABLE);
    }
    if ![model.input_usd_per_million, model.output_usd_per_million]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0)
    {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    let value = (input as f64 * model.input_usd_per_million
        + output as f64 * model.output_usd_per_million)
        / 1_000_000.0;
    if !value.is_finite() {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    Ok(value)
}
fn call(
    model: &FrozenModel,
    kind: WorkKind,
    round: Option<u32>,
    actor: Target,
    input: u64,
) -> Result<CallBound, ErrorCode> {
    let configured_budget = match model.reasoning_effort {
        None => 0,
        Some(ReasoningEffort::Low) => constants::REASONING_LOW_TOKENS,
        Some(ReasoningEffort::Medium) => constants::REASONING_MEDIUM_TOKENS,
        Some(ReasoningEffort::High) => constants::REASONING_HIGH_TOKENS,
    };
    if model.reasoning_tokens != configured_budget {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    let (visible, reasoning, attempts, deadline) = match kind {
        WorkKind::DebateTurn => (
            constants::DEBATE_TURN_VISIBLE_TOKENS,
            model.reasoning_tokens,
            constants::DEBATE_TURN_ATTEMPTS,
            constants::DEBATE_TURN_DEADLINE_MS,
        ),
        WorkKind::JudgeRound => (
            constants::ROUND_JUDGE_VISIBLE_TOKENS,
            if let Some(effort) = model.reasoning_effort {
                constants::reasoning_budget(constants::effective_effort(&kind, effort))
            } else {
                0
            },
            constants::ROUND_JUDGE_ATTEMPTS,
            constants::ROUND_JUDGE_DEADLINE_MS,
        ),
        WorkKind::Judge => (
            constants::FINAL_JUDGE_VISIBLE_TOKENS,
            model.reasoning_tokens,
            constants::FINAL_JUDGE_ATTEMPTS,
            constants::FINAL_JUDGE_DEADLINE_MS,
        ),
        _ => return Err(ErrorCode::INVALID_COMMAND),
    };
    let output = u64::from(visible) + u64::from(reasoning);
    // Structured repair includes rejected visible output and validation diagnostics.
    // The adapter must include its diagnostic framing in the base input bound.
    let repair = if kind == WorkKind::DebateTurn {
        0
    } else {
        u64::from(visible)
    };
    let estimated_usd = cost(model, input, output)?;
    if input
        .checked_add(output)
        .and_then(|n| n.checked_add(repair))
        .is_none_or(|n| n > model.context_tokens)
    {
        return Err(ErrorCode::CONTEXT_TOO_LARGE);
    }
    let reasoning_effort = if model.reasoning_effort.is_some() && kind == WorkKind::JudgeRound {
        Some(constants::ROUND_JUDGE_REASONING_EFFORT)
    } else {
        model.reasoning_effort
    };
    Ok(CallBound {
        kind,
        round,
        actor,
        input_tokens: input,
        visible_tokens: visible,
        reasoning_tokens: reasoning,
        reasoning_effort,
        attempts,
        deadline_ms: deadline,
        estimated_usd,
    })
}
/// Freeze all invocation bounds. `remaining_usd` is a display snapshot only;
/// the router must recheck/reserve the live daily ledger atomically on admission.
pub fn freeze(
    plan: &mut DebatePlan,
    bounds: ContextBounds,
    remaining_usd: f64,
    threshold: f64,
    now: DateTime,
) -> Result<Option<Confirmation>, ErrorCode> {
    if ![remaining_usd, threshold]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0)
    {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    if plan.sources.topic.trim().is_empty()
        || plan.prompts.pack_version.is_empty()
        || plan.prompts.turn_words == 0
        || plan.prompts.final_turn_words == 0
    {
        return Err(ErrorCode::DEBATE_PLAN_STALE);
    }
    if let Some(solutions) = &plan.sources.solutions
        && (solutions.clone().map(|s| s.nick) != plan.preflight.debaters
            || solutions.iter().any(|s| s.text.trim().is_empty())
            || solutions[0].message_id == solutions[1].message_id)
    {
        return Err(ErrorCode::DEBATE_SOURCE_MISSING);
    }
    if plan.preflight.judge_requested
        && plan.judge.as_ref().is_some_and(|j| {
            plan.members
                .iter()
                .any(|m| m.resolved_slug == j.resolved_slug)
        })
    {
        return Err(ErrorCode::MODEL_UNAVAILABLE);
    }
    // Validate identities/rounds before arithmetic or expanding a bounded loop.
    if !(u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
        .contains(&plan.preflight.rounds)
    {
        return Err(ErrorCode::INVALID_COMMAND);
    }
    let mut check = plan.preflight.clone();
    check.estimate.model_calls = check.planned_calls();
    check.validate().map_err(|_| ErrorCode::INVALID_COMMAND)?;
    if plan.preflight.opening_round_required != plan.sources.solutions.is_none()
        && !plan.preflight.judge_only
    {
        return Err(ErrorCode::DEBATE_PLAN_STALE);
    }
    if plan.preflight.judge_only {
        let prior = plan
            .prior_terminal
            .as_ref()
            .ok_or(ErrorCode::DEBATE_PLAN_STALE)?;
        super::judge::manual_prior(prior)?;
        if prior.debate_id != plan.preflight.debate_id
            || prior.debaters != plan.preflight.debaters
            || prior.total_rounds != plan.preflight.rounds
        {
            return Err(ErrorCode::DEBATE_PLAN_STALE);
        }
    }
    let mut calls = Vec::new();
    // Prior turns are re-sent in the same unit as ordinary dispatch (UTF-8 bytes
    // plus framing), so each is bounded by its word limit or four bytes per
    // visible token, whichever is lower, exactly like the other modes' artifacts.
    let turn = crate::router::bounds::limited(
        constants::DEBATE_TURN_VISIBLE_TOKENS,
        plan.prompts.turn_words.max(plan.prompts.final_turn_words),
    );
    let score = crate::router::bounds::artifact(constants::ROUND_JUDGE_VISIBLE_TOKENS);
    let solutions: u64 = plan.sources.solutions.as_ref().map_or(0, |s| {
        s.iter()
            .map(|s| {
                crate::router::bounds::json_text(s.text.as_str())
                    + crate::router::bounds::ARTIFACT_FRAME
            })
            .sum()
    });
    let first = u32::from(!plan.preflight.opening_round_required);
    let judge_base = bounds.judge.saturating_add(solutions);
    let mut scored = 0_u64;
    if !plan.preflight.judge_only {
        for round in first..=plan.preflight.rounds {
            let prior = u64::from(round - first);
            for (index, member) in plan.members.iter().enumerate() {
                if member.nick.as_ref() != Some(&plan.preflight.debaters[index]) {
                    return Err(ErrorCode::DEBATE_PLAN_STALE);
                }
                let input = bounds.members[index]
                    .saturating_add(solutions)
                    .saturating_add(prior * 2 * turn);
                calls.push(call(
                    member,
                    WorkKind::DebateTurn,
                    Some(round),
                    Target::Member(plan.preflight.debaters[index].clone()),
                    input,
                )?);
            }
            if round > 0 && plan.preflight.judge_requested {
                let judge = plan.judge.as_ref().ok_or(ErrorCode::MODEL_UNAVAILABLE)?;
                let input = judge_base
                    .saturating_add((prior + 1) * 2 * turn)
                    .saturating_add(scored * score);
                calls.push(call(
                    judge,
                    WorkKind::JudgeRound,
                    Some(round),
                    Target::Judge,
                    input,
                )?);
                scored += 1;
            }
        }
    }
    let judge_input = judge_base
        .saturating_add(
            u64::from(
                plan.preflight.rounds + 1 - if plan.preflight.judge_only { 0 } else { first },
            ) * 2
                * turn,
        )
        .saturating_add(scored * score);
    if plan.preflight.judge_requested {
        let judge = plan.judge.as_ref().ok_or(ErrorCode::MODEL_UNAVAILABLE)?;
        if plan.preflight.judge_model.as_deref() != Some(judge.resolved_slug.as_str()) {
            return Err(ErrorCode::DEBATE_PLAN_STALE);
        }
        calls.push(call(
            judge,
            WorkKind::Judge,
            None,
            Target::Judge,
            judge_input,
        )?);
    }
    let estimated_usd = calls.iter().map(|c| c.estimated_usd).sum::<f64>();
    let mut reserved_usd = 0.0;
    for bound in &calls {
        let model = match &bound.actor {
            Target::Judge => plan.judge.as_ref().ok_or(ErrorCode::MODEL_UNAVAILABLE)?,
            Target::Member(nick) => plan
                .members
                .iter()
                .find(|m| m.nick.as_ref() == Some(nick))
                .ok_or(ErrorCode::MODEL_UNAVAILABLE)?,
        };
        let repair_input = if bound.kind == WorkKind::DebateTurn {
            0
        } else {
            u64::from(bound.visible_tokens)
        };
        reserved_usd += bound.estimated_usd
            + f64::from(bound.attempts - 1)
                * cost(
                    model,
                    bound.input_tokens.saturating_add(repair_input),
                    u64::from(bound.visible_tokens) + u64::from(bound.reasoning_tokens),
                )?;
    }
    if !estimated_usd.is_finite() || !reserved_usd.is_finite() {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    if reserved_usd > remaining_usd {
        return Err(ErrorCode::DAILY_MAX_REACHED);
    }
    plan.preflight.estimate = DebateEstimate {
        estimated_usd,
        reserved_usd,
        remaining_usd,
        model_calls: calls.len() as u32,
    };
    plan.preflight.expires_at =
        now + chrono::Duration::milliseconds(constants::PREFLIGHT_EXPIRY_MS as i64);
    plan.preflight.confirmation_required = estimated_usd > threshold;
    plan.preflight
        .validate()
        .map_err(|_| ErrorCode::INVALID_COMMAND)?;
    plan.calls = calls;
    Ok((!plan.preflight.confirmation_required).then_some(Confirmation::Automatic))
}
/// Emit even when auto-confirmed or CLI preconfirmed. Admission is a subsequent
/// versioned transition and must never suppress the preflight feed record.
pub fn emit<R: ContestRouter>(
    router: &mut R,
    envelope: ProducerEnvelope,
    plan: &DebatePlan,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    if envelope.content != EnvelopePayload::DebatePreflight(plan.preflight.clone()) {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::DEBATE_PLAN_STALE,
            "Preflight envelope differs from frozen plan",
        ));
    }
    router.produce(envelope)
}
pub fn confirmation(
    plan: &DebatePlan,
    request: Option<&Request>,
    preconfirmed: bool,
    now: DateTime,
    unchanged: bool,
) -> Result<Option<Confirmation>, ErrorCode> {
    let p = &plan.preflight;
    let explicit = match request {
        Some(Request::DebateConfirm {
            debate_id, plan_id, ..
        }) => Some((*debate_id, *plan_id)),
        Some(Request::ContestConfirm {
            contest_id,
            plan_id,
            ..
        }) => Some((*contest_id, *plan_id)),
        Some(_) => return Err(ErrorCode::INVALID_COMMAND),
        None => None,
    };
    let (debate, id) = explicit.unwrap_or((p.debate_id, p.plan_id));
    p.validate_confirmation(debate, id, now, unchanged)?;
    Ok(if explicit.is_some() {
        Some(Confirmation::Explicit)
    } else if preconfirmed {
        Some(Confirmation::Preconfirmed)
    } else if !p.confirmation_required {
        Some(Confirmation::Automatic)
    } else {
        None
    })
}
pub fn cancellation(plan: &DebatePlan, request: &Request) -> Result<DebateTransition, ErrorCode> {
    let ids = match request {
        Request::DebateCancel {
            debate_id, plan_id, ..
        } => (*debate_id, *plan_id),
        Request::ContestCancel {
            contest_id,
            plan_id,
            ..
        } => (*contest_id, *plan_id),
        _ => return Err(ErrorCode::INVALID_COMMAND),
    };
    if ids != (plan.preflight.debate_id, plan.preflight.plan_id) {
        return Err(ErrorCode::DEBATE_PLAN_STALE);
    }
    Ok(if plan.preflight.judge_only {
        DebateTransition::JudgeOnlyCancelled
    } else {
        DebateTransition::Cancel
    })
}

pub fn prepare_new_mode(kind: ContestKind, parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> { match kind {
ContestKind::Forge => super::forge::prepare(parsed),
ContestKind::Forecast => super::forecast::prepare(parsed),
ContestKind::Decide => super::decide::prepare(parsed),
ContestKind::Negotiate => super::negotiate::prepare(parsed),
ContestKind::BlowItUp => super::blow_it_up::prepare(parsed),
ContestKind::Readback => super::readback::prepare(parsed),
ContestKind::Debugging => super::debugging::prepare(parsed),
ContestKind::Trace => super::trace::prepare(parsed),
ContestKind::Rank => super::rank::prepare(parsed),
_ => Err(crate::prompts::PromptError::new("mode",None,"not a new mode")) } }
