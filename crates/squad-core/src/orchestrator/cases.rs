//! Case references are selected from authorized material, never member histories.
use squad_protocol::*;

pub fn standalone(
    case_id: CaseId,
    invocation: EventId,
    work_id: WorkId,
    model: Text,
    question: &str,
    quotes: Vec<ResolvedQuote>,
) -> Result<CaseOpened, ErrorCode> {
    if quotes.is_empty() {
        return Err(ErrorCode::JUDGE_QUOTE_REQUIRED);
    }
    Ok(CaseOpened {
        case_id,
        judge_invocation_id: invocation,
        work_id: Some(work_id),
        judge_model: model,
        material: CaseMaterial::Standalone {
            messages: quotes,
            question: Text::new(question).map_err(|_| ErrorCode::INVALID_COMMAND)?,
        },
        debate_id: None,
        contest_id: None,
    })
}
pub fn debate(
    case_id: CaseId,
    invocation: EventId,
    model: Text,
    setup: DebateSetup,
    rubric: Text,
) -> Result<CaseOpened, ErrorCode> {
    setup.validate().map_err(|_| ErrorCode::INVALID_REFERENCE)?;
    Ok(CaseOpened {
        case_id,
        judge_invocation_id: invocation,
        work_id: None,
        judge_model: model,
        debate_id: Some(setup.debate_id),
        contest_id: Some(setup.debate_id),
        material: CaseMaterial::Debate {
            setup,
            rubric_version: rubric,
        },
    })
}
/// A verdict quote is resolved by the router to its existing case. A follow-up
/// uses that same case, a fresh invocation/work, and never reopens a score update.
pub fn followup(
    case_id: CaseId,
    user_event_id: EventId,
    invocation: EventId,
    work_id: WorkId,
) -> CaseFollowup {
    CaseFollowup {
        case_id,
        user_event_id,
        judge_invocation_id: invocation,
        work_id,
    }
}
pub fn context(case_id: CaseId) -> ContextId {
    ContextId(format!("judge:{case_id}"))
}

/// Freeze only canonical committed material supplied by the router adapter.
/// `committed_rounds` can be partial for a stopped debate; no retroactive scores.
pub fn bundle(
    plan: &super::types::DebatePlan,
    state: &DebateState,
    setup: &DebateSetup,
    transcript: &[DebateTurn],
    scores: &[DebateRoundScore],
    committed_rounds: &[u32],
    rubric: FrozenDebateRubric,
) -> Result<DebateAdjudicationBundle, ErrorCode> {
    setup.validate().map_err(|_| ErrorCode::INVALID_REFERENCE)?;
    if setup.debate_id != state.debate_id
        || setup.topic_message_id != plan.sources.topic_message_id
        || setup.topic_text != plan.sources.topic
        || setup.instruction != plan.sources.instruction
        || setup.solutions.clone().map(|s| s.nick) != state.debaters
    {
        return Err(ErrorCode::INVALID_REFERENCE);
    }
    let expected = canonical_rubric(&rubric.version)?;
    if rubric.criteria != expected.criteria || rubric.verdict_schema != expected.verdict_schema {
        return Err(ErrorCode::JUDGE_INVALID_VERDICT);
    }
    let mut seen = std::collections::BTreeSet::new();
    for turn in transcript {
        if turn.debate_id != state.debate_id
            || turn.round == 0
            || !committed_rounds.contains(&turn.round)
            || !state.debaters.contains(&turn.speaker)
            || !seen.insert((turn.round, turn.speaker.clone()))
        {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
    }
    if committed_rounds.iter().any(|r| {
        !state
            .debaters
            .iter()
            .all(|n| seen.contains(&(*r, n.clone())))
    }) {
        return Err(ErrorCode::INVALID_REFERENCE);
    }
    let scoring = super::judge::scoring_evidence(state, scores, committed_rounds)?;
    Ok(DebateAdjudicationBundle {
        topic: AdjudicationTopic {
            message_id: setup.topic_message_id,
            text: setup.topic_text.clone(),
        },
        solutions: setup.solutions.clone(),
        transcript: transcript.to_vec(),
        instruction: setup.instruction.clone(),
        rubric,
        judge: super::provenance::judge(plan)?,
        round_scoring: scoring,
    })
}
pub fn open_debate<R: super::types::ContestRouter>(
    router: &mut R,
    event: ProducerEnvelope,
    bundle: DebateAdjudicationBundle,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    let EnvelopePayload::CaseOpened(opened) = &event.content else {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_REFERENCE,
            "Case opening event required",
        ));
    };
    let CaseMaterial::Debate { setup, .. } = &opened.material else {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_REFERENCE,
            "Debate case required",
        ));
    };
    if bundle.topic.message_id != setup.topic_message_id
        || bundle.topic.text != setup.topic_text
        || bundle.solutions != setup.solutions
        || bundle.instruction != setup.instruction
        || bundle.judge.model != opened.judge_model.as_str()
    {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_REFERENCE,
            "Case bundle differs from admitted setup",
        ));
    }
    router.open_debate_case(event, bundle)
}

/// Canonical competitive rubric and model-output schema. Totals/tally are not
/// model fields; assemble_verdict computes them from accepted evidence.
pub fn canonical_rubric(version: &str) -> Result<FrozenDebateRubric, ErrorCode> {
    if version.trim().is_empty() {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    Ok(FrozenDebateRubric {
        version: version.into(),
        criteria: vec![
            RubricCriterion {
                criterion_id: CriterionId::Fit,
                weight_percent: 30,
            },
            RubricCriterion {
                criterion_id: CriterionId::Feasibility,
                weight_percent: 30,
            },
            RubricCriterion {
                criterion_id: CriterionId::Originality,
                weight_percent: 15,
            },
            RubricCriterion {
                criterion_id: CriterionId::SurvivedAttacks,
                weight_percent: 25,
            },
        ],
        verdict_schema: validation::schema::<JudgeVerdict>(),
    })
}
