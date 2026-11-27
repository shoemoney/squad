//! Canonical plan and case provenance for frozen contest work.
use crate::{Result, commit, history};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use squad_protocol::*;

fn output_schema(kind: &WorkKind) -> Option<Value> {
    use validation::schema;
    if let Some(mode) = kind.new_mode_kind() {
        return match mode {
            ContestKind::Forge => forge::output_schema(kind),
            ContestKind::Forecast => forecast::output_schema(kind),
            ContestKind::Decide => decide::output_schema(kind),
            ContestKind::Negotiate => negotiate::output_schema(kind),
            ContestKind::BlowItUp => blow_it_up::output_schema(kind),
            ContestKind::Readback => readback::output_schema(kind),
            ContestKind::Debugging => debugging::output_schema(kind),
            ContestKind::Trace => trace::output_schema(kind),
            ContestKind::Rank => rank::output_schema(kind),
            ContestKind::Tune => tune::output_schema(kind),
            ContestKind::BassAckwardsPrd => bass_ackwards_prd::output_schema(kind),
            _ => None,
        };
    }

    Some(match kind {
        WorkKind::Judge => schema::<JudgeVerdict>(),
        WorkKind::JudgeRound => schema::<RoundJudgeResponse>(),
        WorkKind::CensusHunt => schema::<CensusHuntOutput>(),
        WorkKind::CensusMerge => schema::<CensusMergeOutput>(),
        WorkKind::CensusRate => schema::<CensusRatingOutput>(),
        WorkKind::CensusFinal => schema::<CensusFinalOutput>(),
        WorkKind::CrosscheckGoal => schema::<CrosscheckGoal>(),
        WorkKind::CrosscheckInventory => schema::<CrosscheckInventoryResponse>(),
        WorkKind::CrosscheckNormalize => schema::<CrosscheckNormalization>(),
        WorkKind::CrosscheckCell => schema::<CrosscheckReviewResponse>(),
        WorkKind::CrosscheckAudit => schema::<CrosscheckAuditResponse>(),
        WorkKind::CrosscheckRule => schema::<CrosscheckRuling>(),
        WorkKind::SurveyExtract => schema::<SurveyExtraction>(),
        WorkKind::SurveyVote | WorkKind::SurveyRevote => schema::<SurveyBallot>(),
        WorkKind::SurveyReport => schema::<SurveyReportText>(),
        WorkKind::ProbeExplain => schema::<ProbeExplanation>(),
        WorkKind::ProbeCheck => schema::<ProbeCheck>(),
        WorkKind::ProbePredict => schema::<ProbePrediction>(),
        WorkKind::ProbeReport => schema::<ProbeReport>(),
        _ => return None,
    })
}

pub(crate) fn record(db: &Connection, work: &Work, binding: &ExecutorBinding) -> Result<()> {
    let (Some(contest), Some(prompt)) = (work.contest_id, &work.prompt_ref) else {
        return Ok(());
    };
    if work.debate_id.is_none() && output_schema(&work.kind).is_none() {
        return Ok(());
    }
    let (raw_plan, raw_state): (Option<String>, String) = db.query_row(
        "SELECT plan_json,state_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let Some(raw_plan) = raw_plan else {
        return Ok(());
    };
    let mut plan: Value = serde_json::from_str(&raw_plan)?;
    let state: Value = serde_json::from_str(&raw_state)?;
    let projection = history::read(db, &work.context_id, work.history_through)?;
    let schema = output_schema(&work.kind);
    let step: Option<Value> = work.step_id.map(|step| -> Result<Value> {
        let (version, ordinal, movement, pass): (String, u32, String, Option<u32>) = db.query_row(
            "SELECT script_version,ordinal,move,pass FROM contest_steps WHERE step_id=?",
            [step.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        )?;
        Ok(json!({"stepId":step,"scriptVersion":version,"ordinal":ordinal,"move":movement,"pass":pass}))
    }).transpose()?;
    let invocation = json!({
        "workId":work.work_id,"stepId":work.step_id,"kind":work.kind,"step":step,
        "pass":step.as_ref().and_then(|s| s.get("pass")).filter(|p| !p.is_null()).or_else(||state.get("pass")),"planVersion":state.get("planVersion"),
        "round":work.round,"promptRef":prompt,"binding":binding,
        "visibleTokens":work.max_output_tokens,
        "effectiveReasoningEffort":constants::effective_effort(&work.kind,binding.effort),
        "schemaIdentity":schema.as_ref().and_then(|s|s.get("title")),
        "schemaHash":schema.as_ref().map(commit::digest).transpose()?,
        "authorizedInputProjection":{"contextId":work.context_id,"historyThrough":work.history_through,
            "hash":commit::digest(&projection)?}
    });
    let key = work.work_id.to_string();
    let settings = if plan.get("base").is_some() {
        &mut plan["base"]["settings"]
    } else if plan.get("settings").is_some() {
        &mut plan["settings"]
    } else {
        &mut plan
    };
    settings["promptProvenance"]["invocations"][&key] = invocation.clone();
    db.execute(
        "UPDATE contests SET plan_json=? WHERE contest_id=?",
        params![plan.to_string(), contest.to_string()],
    )?;
    if let Some(case) = work.case_id {
        let raw: String = db.query_row(
            "SELECT state_json FROM cases WHERE case_id=?",
            [case.to_string()],
            |r| r.get(0),
        )?;
        let mut state: Value = serde_json::from_str(&raw)?;
        state["promptProvenance"]["invocations"][&key] = invocation;
        if work.debate_id.is_some() {
            state["promptProvenance"]["settings"] = plan["promptSettings"].clone();
            state["promptProvenance"]["overrideHashes"] = plan["promptOverrideHashes"].clone();
        }
        db.execute(
            "UPDATE cases SET state_json=? WHERE case_id=?",
            params![state.to_string(), case.to_string()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_structured_mode_step_records_its_canonical_schema_identity() {
        let kinds = [
            (WorkKind::CensusHunt, "CensusHuntOutput"),
            (WorkKind::CensusMerge, "CensusMergeOutput"),
            (WorkKind::CensusRate, "CensusRatingOutput"),
            (WorkKind::CensusFinal, "CensusFinalOutput"),
            (WorkKind::CrosscheckGoal, "CrosscheckGoal"),
            (WorkKind::CrosscheckInventory, "CrosscheckInventoryResponse"),
            (WorkKind::CrosscheckNormalize, "CrosscheckNormalization"),
            (WorkKind::CrosscheckCell, "CrosscheckReviewResponse"),
            (WorkKind::CrosscheckAudit, "CrosscheckAuditResponse"),
            (WorkKind::CrosscheckRule, "CrosscheckRuling"),
            (WorkKind::SurveyExtract, "SurveyExtraction"),
            (WorkKind::SurveyVote, "SurveyBallot"),
            (WorkKind::SurveyRevote, "SurveyBallot"),
            (WorkKind::SurveyReport, "SurveyReportText"),
            (WorkKind::ProbeExplain, "ProbeExplanation"),
            (WorkKind::ProbeCheck, "ProbeCheck"),
            (WorkKind::ProbePredict, "ProbePrediction"),
            (WorkKind::ProbeReport, "ProbeReport"),
        ];
        for (kind, title) in kinds {
            let schema = output_schema(&kind).unwrap();
            assert_eq!(schema["title"], title, "{kind:?}");
            assert_eq!(schema["additionalProperties"], false, "{kind:?}");
        }
        assert!(output_schema(&WorkKind::DebateTurn).is_none());
    }
    #[test]
    fn new_mode_invocations_keep_step_schema_projection_and_plan_settings_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("provenance.sqlite");
        let store = crate::Store::open(&path).unwrap();
        let mut retained = Vec::new();
        for (mode, kind) in [
            ("census", WorkKind::CensusMerge),
            ("crosscheck", WorkKind::CrosscheckRule),
            ("survey", WorkKind::SurveyExtract),
            ("probe", WorkKind::ProbeReport),
        ] {
            let contest = ContestId::new();
            let case = CaseId::new();
            let id = EventId::new();
            let work_id = WorkId::new();
            let outcome = store.commit(id,"fixture".into(),id.to_string(),move |tx,group| {
                let source = ProducerEnvelope {
                    base: EnvelopeBase {version:1,id,room_id:MainRoom::Main,
                        correlation_id:id,causation_id:None,sender:Actor::System,
                        recipients:vec![Recipient::Me],created_at:chrono::Utc::now()},
                    content:EnvelopePayload::SystemNotice(SystemNotice {code:Text::new("FIXTURE").unwrap(),
                        text:Text::new("[squad notice] authorized source").unwrap(),work_id:None,
                        debate_id:None,case_id:None,contest_id:None}),
                };
                let seq = group.push(tx, source)?;
                let plan = if mode == "survey" {json!({"settings":{"frozen":"KEEP"}})}
                    else {json!({"base":{"settings":{"frozen":"KEEP"}}})};
                tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_json) VALUES (?,?,1,1,?,0,?)",
                    params![contest.to_string(),mode,json!({"pass":2,"planVersion":3}).to_string(),plan.to_string()])?;
                tx.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,1,?,'fixture/judge','{}',?,'{}')",
                    params![case.to_string(),format!("judge:{case}"),contest.to_string(),seq.get()])?;
                let step = StepId::new();
                tx.execute("INSERT INTO contest_steps(step_id,contest_id,ordinal,script_version,move,pass,state_json,inputs_json) VALUES (?,?,1,'audit.v1','judge',2,'{}','[]')",params![step.to_string(),contest.to_string()])?;
                let context = ContextId(format!("judge:{case}"));
                let through = history::project(tx,&HistoryProjection {
                    context_id:context.clone(),origin_event_id:id,projection_key:Text::new("fixture.input").unwrap(),
                    role:HistoryRole::User,name:None,content:"AUTHORIZED_INPUT".into(),meta:HistoryMetadata::Anonymous {anonymous:True},
                })?.unwrap();
                let template_id = serde_json::to_value(&kind)?.as_str().unwrap().to_owned();
                let work = Work {
                    work_id,message_id:MessageId::new(),in_reply_to_message_id:None,kind,
                    target:Target::Judge,context_id:context,trigger_event_id:id,history_through:through,
                    attempt:1,debate_id:None,round:None,replay:None,case_id:Some(case),
                    prompt_ref:Some(PromptRef {template_id:Text::new(template_id).unwrap(),
                        pack_version:Text::new("fixture.v1").unwrap(),template_hash:Text::new("FROZEN_HASH").unwrap()}),
                    max_output_tokens:Some(4096),contest_id:Some(contest),step_id:Some(step),
                };
                work.validate().unwrap();
                let binding = ExecutorBinding {model:Text::new("fixture/judge").unwrap(),effort:ReasoningEffort::Medium};
                record(tx,&work,&binding)?;
                record(tx,&work,&binding)?; // Retry/recovery reuses the same durable identity.
                Ok(group.accepted(id))
            }).unwrap();
            assert!(!outcome.duplicate);
            retained.push((contest, case, work_id));
        }
        drop(store);
        let store = crate::Store::open(path).unwrap();
        store
            .call(move |db| {
                for (contest, case, work) in retained {
                    let raw: String = db.query_row(
                        "SELECT plan_json FROM contests WHERE contest_id=?",
                        [contest.to_string()],
                        |r| r.get(0),
                    )?;
                    let plan: Value = serde_json::from_str(&raw)?;
                    let settings = plan.get("base").unwrap_or(&plan).get("settings").unwrap();
                    assert_eq!(settings["frozen"], "KEEP");
                    let raw: String = db.query_row(
                        "SELECT state_json FROM cases WHERE case_id=?",
                        [case.to_string()],
                        |r| r.get(0),
                    )?;
                    let case: Value = serde_json::from_str(&raw)?;
                    let invocations = case["promptProvenance"]["invocations"].as_object().unwrap();
                    assert_eq!(invocations.len(), 1);
                    let invocation = &invocations[&work.to_string()];
                    assert_eq!(
                        *invocation,
                        settings["promptProvenance"]["invocations"][work.to_string()]
                    );
                    assert_eq!(invocation["pass"], 2);
                    assert_eq!(invocation["planVersion"], 3);
                    assert_eq!(invocation["step"]["scriptVersion"], "audit.v1");
                    assert!(invocation["schemaIdentity"].is_string());
                    assert!(invocation["schemaHash"].is_string());
                    assert!(invocation["authorizedInputProjection"]["hash"].is_string());
                    assert_eq!(invocation["promptRef"]["templateHash"], "FROZEN_HASH");
                }
                Ok(())
            })
            .unwrap();
    }
}
