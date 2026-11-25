//! Survey is a script of frozen fanouts, barriers, tallies and judge moves.
use squad_protocol::*;
use std::collections::BTreeSet;

pub fn transition(
    state: SurveyStatus,
    signal: SurveyTransition,
    collect: bool,
) -> Option<SurveyStatus> {
    use SurveyStatus::*;
    use SurveyTransition::*;
    if state.is_terminal() {
        return None;
    }
    Some(match (state, signal) {
        (_, Stop) => Stopped,
        (_, Fail) => Failed,
        (Preflight, ParticipantsReady) | (WaitingForParticipants, ParticipantsReady) => {
            if collect {
                Collecting
            } else {
                Extracting
            }
        }
        (Collecting, CollectionLocked) => Extracting,
        (Extracting, ClaimsLocked) => Voting,
        (Voting, VotesLocked) => Selecting,
        (Selecting, SelectionLocked) => Revoting,
        (Revoting, RevotesLocked) => Reporting,
        (Reporting, ReportLocked) => Complete,
        _ => return None,
    })
}
fn item<'a>(votes: &'a [SurveyVote], nick: &MemberNick, id: &str) -> Option<&'a SurveyBallotItem> {
    votes
        .iter()
        .find(|v| &v.voter == nick)?
        .ballot
        .votes
        .iter()
        .find(|v| v.claim_id == id)
}
fn tally<'a>(items: impl Iterator<Item = Option<&'a SurveyBallotItem>>) -> SurveyTally {
    let mut t = SurveyTally::default();
    for v in items {
        match v.map(|v| v.vote) {
            Some(SurveyChoice::Agree) => t.agree += 1,
            Some(SurveyChoice::Disagree) => t.disagree += 1,
            Some(SurveyChoice::Unsure) => t.unsure += 1,
            None => t.missing += 1,
        }
    }
    t
}
/// Derive participation from frozen sources and accepted ballots, including
/// first votes later replaced by a second vote. Empty maps need no ballot calls.
pub fn completeness(
    setup: &SurveySetup,
    claims: &SurveyExtraction,
    selected: &[String],
    first: &[SurveyVote],
    second: &[SurveyVote],
) -> SurveyCompleteness {
    SurveyCompleteness::from_report_inputs(
        setup,
        claims,
        first,
        &map(setup, claims, selected, first, second),
    )
}
/// Rational comparisons avoid float ordering and preserve stable claim identities.
pub fn select(
    claims: &SurveyExtraction,
    participants: &[MemberNick],
    first: &[SurveyVote],
) -> Vec<String> {
    let mut ranked = claims
        .claims
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                tally(participants.iter().map(|n| item(first, n, &c.id))),
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, x), (b, y)| {
        let dx = x.agree + x.disagree;
        let dy = y.agree + y.disagree;
        (dx == 0)
            .cmp(&(dy == 0))
            .then_with(|| {
                (x.agree.abs_diff(x.disagree) * dy).cmp(&(y.agree.abs_diff(y.disagree) * dx))
            })
            .then_with(|| dy.cmp(&dx))
            .then_with(|| a.cmp(b))
    });
    ranked.into_iter().take(5).map(|(id, _)| id).collect()
}
/// Remove identifying spans and deduplicate after redaction. Order carries no voter grouping.
pub fn blind_text(text: &str, setup: &SurveySetup) -> String {
    super::blind::redact(
        text,
        &setup
            .participants
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}
pub fn selection(
    setup: &SurveySetup,
    step_id: StepId,
    claims: &SurveyExtraction,
    first: &[SurveyVote],
) -> SurveySelection {
    let tallies = map(setup, claims, &[], first, &[]);
    let selected = select(claims, &setup.participants, first);
    let claims = selected
        .iter()
        .filter_map(|id| claims.claims.iter().find(|c| &c.id == id))
        .map(|c| {
            let reasons = |choice| {
                first
                    .iter()
                    .flat_map(|v| &v.ballot.votes)
                    .filter(|v| v.claim_id == c.id && v.vote == choice)
                    .map(|v| blind_text(&v.reason, setup))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            };
            SurveyAnonymousClaim {
                claim_id: c.id.clone(),
                claim: blind_text(&c.claim, setup),
                agree_reasons: reasons(SurveyChoice::Agree),
                disagree_reasons: reasons(SurveyChoice::Disagree),
                unsure_reasons: reasons(SurveyChoice::Unsure),
            }
        })
        .collect();
    SurveySelection {
        tallies,
        contest_id: setup.contest_id,
        step_id,
        claims,
    }
}
/// Derive classifications once in application code. Judge prose never supplies counts.
pub fn map(
    setup: &SurveySetup,
    claims: &SurveyExtraction,
    selected: &[String],
    first: &[SurveyVote],
    second: &[SurveyVote],
) -> Vec<SurveyClaimMap> {
    claims
        .claims
        .iter()
        .map(|c| {
            let final_item = |n| {
                if selected.contains(&c.id) {
                    item(second, n, &c.id).or_else(|| item(first, n, &c.id))
                } else {
                    item(first, n, &c.id)
                }
            };
            let first_tally = tally(setup.participants.iter().map(|n| item(first, n, &c.id)));
            let final_tally = tally(setup.participants.iter().map(final_item));
            let m = setup.participants.len() as u32;
            let origins = setup
                .answers
                .iter()
                .filter(|a| c.source_answer_ids.contains(&a.message_id))
                .map(|a| &a.nick)
                .collect::<BTreeSet<_>>();
            let others = setup
                .participants
                .iter()
                .filter(|n| !origins.contains(n))
                .collect::<Vec<_>>();
            let mut bands = vec![];
            if (final_tally.agree == m || (m > 1 && final_tally.agree == m - 1))
                && final_tally.unsure == 0
                && final_tally.missing == 0
            {
                bands.push(SurveyBand::Consensus);
            }
            if final_tally.agree > 0 && final_tally.disagree > 0 {
                bands.push(SurveyBand::Contested);
            }
            if origins.len() == 1
                && !others.is_empty()
                && others
                    .iter()
                    .all(|n| final_item(n).is_some_and(|v| v.vote == SurveyChoice::Agree))
            {
                bands.push(SurveyBand::LoneInsights);
            }
            if final_tally.disagree == 1 {
                bands.push(SurveyBand::LoneDissents);
            }
            if (1..=2).contains(&origins.len())
                && !others.is_empty()
                && others
                    .iter()
                    .all(|n| final_item(n).is_some_and(|v| v.vote == SurveyChoice::Disagree))
            {
                bands.push(SurveyBand::RejectedOutliers);
            }
            let switches = if selected.contains(&c.id) {
                setup
                    .participants
                    .iter()
                    .filter_map(|n| {
                        let a = item(first, n, &c.id)?;
                        let b = item(second, n, &c.id)?;
                        (a.vote != b.vote).then(|| SurveySwitch {
                            voter: n.clone(),
                            from: a.vote,
                            to: b.vote,
                        })
                    })
                    .collect()
            } else {
                vec![]
            };
            let not_revoted = if selected.contains(&c.id) {
                setup
                    .participants
                    .iter()
                    .filter(|n| item(second, n, &c.id).is_none())
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            SurveyClaimMap {
                claim_id: c.id.clone(),
                first: first_tally,
                final_tally,
                not_revoted,
                switches,
                bands,
            }
        })
        .collect()
}

use crate::{
    prompts::{PromptPack, inputs_survey},
    router::{Router, envelope},
};
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::BTreeMap;
impl Router {
    pub(crate) fn drive_surveys(&mut self) -> Result<Vec<CommitOutcome>> {
        let ids = self.store.call(|db| {
            let mut q =
                db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='survey'")?;
            Ok(q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = vec![];
        for id in ids {
            let id = id.parse().map_err(|_| StoreError::Unavailable)?;
            if self.engine_scope.as_ref().is_some_and(|s| !s.contains(&id)) {
                continue;
            }
            for _ in 0..32 {
                let state = self
                    .store
                    .call(move |db| squad_store::survey::state(db, id))?;
                if state.state.is_terminal() {
                    break;
                }
                if state.state == SurveyStatus::Preflight {
                    let plan = self.store.call(move |db| {
                        let raw: String = db.query_row(
                            "SELECT plan_json FROM contests WHERE contest_id=?",
                            [id.to_string()],
                            |r| r.get(0),
                        )?;
                        Ok(serde_json::from_str::<ContestPreflight>(&raw)?)
                    })?;
                    if chrono::Utc::now() >= plan.expires_at {
                        let request = EventId::new();
                        let plan_id = plan.plan_id;
                        outcomes.push(self.store.commit(
                            request,
                            "contest-engine".into(),
                            squad_store::commit::digest(&(id, plan_id, "expired"))?,
                            move |tx, group| {
                                crate::router::survey::resolve(
                                    tx, group, request, id, plan_id, false,
                                )
                            },
                        )?);
                    }
                    break;
                }
                if state.state == SurveyStatus::WaitingForParticipants {
                    let waiting=self.store.call(move |db|Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM work w JOIN survey_sources s ON s.contest_id=? WHERE w.kind='reply' AND w.step_id IS NULL AND w.held_by_contest_id IS NULL AND w.status IN ('queued','running') AND json_type(w.scheduling_meta,'$.openModeDormant') IS NULL AND w.target IN (SELECT value FROM json_each(s.setup_json,'$.participants')))",[id.to_string()],|r|r.get::<_,bool>(0))?))?;
                    if waiting {
                        break;
                    }
                    outcomes.push(self.transition(
                        EventId::new(),
                        ContestTransitionSubmission {
                            contest_id: id,
                            kind: ContestKind::Survey,
                            expected_state_version: state.state_version,
                            transition: ContestTransition::Survey(
                                SurveyTransition::ParticipantsReady,
                            ),
                        },
                    )?);
                    continue;
                }
                if let Some(step) = state.step_id {
                    let pending=self.store.call(move |db|Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND status IN ('queued','running'))",[step.to_string()],|r|r.get::<_,bool>(0))?))?;
                    if pending {
                        break;
                    }
                    let request_id = EventId::new();
                    let report = ContestReport {
                        contest_id: id,
                        step_id: step,
                        artifact_event_ids: vec![],
                        skipped_work_ids: vec![],
                        data: serde_json::json!({"surveyStage":state.state}),
                    };
                    outcomes.push(self.barrier(
                        request_id,
                        BarrierCommit {
                            contest_id: id,
                            step_id: step,
                            expected_state_version: state.state_version,
                            barrier_event: envelope(
                                request_id,
                                Actor::System,
                                vec![Recipient::Me],
                                EnvelopePayload::ContestReport(report),
                            ),
                            projections: vec![],
                            next_work: vec![],
                        },
                    )?);
                } else {
                    outcomes.push(self.allocate_survey_stage(&state)?);
                }
            }
        }
        Ok(outcomes)
    }
    fn allocate_survey_stage(&mut self, state: &SurveyState) -> Result<CommitOutcome> {
        let config = self.engine_config.clone().ok_or(StoreError::Unavailable)?;
        let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| StoreError::Unavailable)?;
        let id = state.contest_id;
        let setup = self
            .store
            .call(move |db| squad_store::survey::setup(db, id))?;
        let (kind, composition, move_name) = match state.state {
            SurveyStatus::Collecting => (WorkKind::Reply, "reply", ContestMove::Collect),
            SurveyStatus::Extracting => (
                WorkKind::SurveyExtract,
                "survey.extract",
                ContestMove::Judge,
            ),
            SurveyStatus::Voting => (WorkKind::SurveyVote, "survey.vote", ContestMove::Fanout),
            SurveyStatus::Revoting => {
                (WorkKind::SurveyRevote, "survey.revote", ContestMove::Fanout)
            }
            SurveyStatus::Reporting => {
                (WorkKind::SurveyReport, "survey.report", ContestMove::Report)
            }
            _ => return Err(StoreError::Unavailable),
        };
        let data=match kind {
            WorkKind::Reply=>serde_json::json!({"question":setup.question}),
            WorkKind::SurveyExtract=>serde_json::json!({"question":setup.question,"answers":setup.answers}),
            WorkKind::SurveyVote=>{let claims=self.store.call(move |db|squad_store::survey::claims(db,id))?;inputs_survey::vote_data(&setup,&claims)},
            WorkKind::SurveyRevote=>{let selected=self.store.call(move |db|squad_store::survey::selected(db,id))?;inputs_survey::revote_data(&setup,&selected)},
            WorkKind::SurveyReport=>{
                self.store.call(move |db| {
                    let setup=squad_store::survey::setup(db,id)?;let claims=squad_store::survey::claims(db,id)?;let first=squad_store::survey::ballots(db,id,false)?;let second=squad_store::survey::ballots(db,id,true)?;
                    let selected=squad_store::survey::selected(db,id)?.claims.into_iter().map(|c|c.claim_id).collect::<Vec<_>>();let map=map(&setup,&claims,&selected,&first,&second);
                    Ok(serde_json::json!({"setup":setup,"claims":claims,"first":first,"second":second,"map":map}))
                })?
            },_=>return Err(StoreError::Unavailable)
        };
        let data = if matches!(kind, WorkKind::SurveyVote | WorkKind::SurveyRevote) {
            let tokens=self.store.call(move |db| {let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='survey.redaction'",[id.to_string()],|r|r.get(0))?;let v:serde_json::Value=serde_json::from_str(&raw)?;Ok(serde_json::from_value::<Vec<String>>(v["tokens"].clone())?)})?;
            super::blind::value(data, &tokens)
        } else {
            data
        };
        let step_id = StepId::new();
        let request_id = EventId::new();
        let targets = if matches!(kind, WorkKind::SurveyExtract | WorkKind::SurveyReport) {
            vec![Target::Judge]
        } else {
            setup
                .participants
                .iter()
                .cloned()
                .map(Target::Member)
                .collect()
        };
        let inputs=self.store.call(move |db| {let mut q=db.prepare("SELECT e.id FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind IN ('survey.setup','survey.claims','survey.selection') ORDER BY e.seq")?;let ids=q.query_map([id.to_string()],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;ids.into_iter().map(|s|s.parse().map_err(|_|StoreError::Unavailable)).collect::<Result<Vec<EventId>>>()})?;
        let ordinal = self.store.call(move |db| {
            Ok(db.query_row(
                "SELECT COUNT(*)+1 FROM contest_steps WHERE contest_id=?",
                [id.to_string()],
                |r| r.get::<_, u32>(0),
            )?)
        })?;
        // The trigger event is the accepted setup; the step itself is allocated
        // atomically with its work. Source identity remains stable on restart.
        let trigger = *inputs.first().ok_or(StoreError::Unavailable)?;
        let mut work = vec![];
        let mut assignments = vec![];
        let mut maps = BTreeMap::new();
        for target in targets {
            let case_id = (target == Target::Judge).then(CaseId::new);
            let context_id = ContextId(match (&target, case_id) {
                (Target::Member(n), _) => format!("contest:{id}:{step_id}:{n}"),
                (_, Some(c)) => format!("judge:{c}"),
                _ => return Err(StoreError::Unavailable),
            });
            let wid = WorkId::new();
            let template = if kind == WorkKind::Reply {
                None
            } else {
                Some(PromptRef {
                    template_id: Text::new(format!("{composition}.md")).expect("template"),
                    pack_version: Text::new(pack.version()).map_err(|_| StoreError::Unavailable)?,
                    template_hash: Text::new(squad_store::commit::digest(&(
                        pack.template(composition)
                            .map_err(|_| StoreError::Unavailable)?,
                        pack.overrides(),
                    ))?)
                    .map_err(|_| StoreError::Unavailable)?,
                })
            };
            let w = Work {
                work_id: wid,
                message_id: MessageId::new(),
                in_reply_to_message_id: (kind == WorkKind::Reply)
                    .then_some(setup.question_message_id),
                kind: kind.clone(),
                target: target.clone(),
                context_id: context_id.clone(),
                trigger_event_id: trigger,
                history_through: HistoryPosition::new(1).expect("positive"),
                attempt: 1,
                debate_id: None,
                round: None,
                replay: None,
                case_id,
                prompt_ref: template,
                max_output_tokens: Some(if target == Target::Judge {
                    constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
                } else {
                    2048
                }),
                contest_id: Some(id),
                step_id: Some(step_id),
            };
            let values = inputs_survey::task_values(
                id,
                data.clone(),
                &kind,
                if target == Target::Judge {
                    config.prompts.limits.plan_words
                } else {
                    config.prompts.limits.room_words
                },
            );
            maps.insert(format!("survey.work:{wid}"), serde_json::to_value(values)?);
            assignments.push(ContestAssignment {
                target,
                context_id,
                payload_refs: inputs.clone(),
                work_id: Some(wid),
            });
            work.push(w);
        }
        self.allocate_step(
            request_id,
            ContestStep {
                contest_id: id,
                step_id,
                ordinal,
                script_version: Text::new("survey-v1").expect("literal"),
                move_name,
                inputs,
                assignments,
            },
            vec![],
            work,
            maps,
        )
    }
}
