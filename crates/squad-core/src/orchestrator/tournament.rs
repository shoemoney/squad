//! Elimination script over durable fanout, judge, tally, barrier and report moves.
use crate::{
    prompts::PromptPack,
    router::{Router, tournaments as gate},
};
use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{
    Result, StoreError,
    commit::{CommitGroup, CommitOutcome},
};
use std::collections::BTreeMap;

pub fn call_counts(k: u32) -> (u32, u32) {
    (
        (2..=k).map(|n| n + 1).sum(),
        (2..=k).map(|n| 2 * (2 * n + 3)).sum(),
    )
}
/// Tokens are captured from the frozen startup configuration and resolved model
/// bindings. Longer phrases win; Unicode word boundaries retain punctuation.
pub fn redact(text: &str, tokens: &[String]) -> String {
    let mut tokens = tokens
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return text.into();
    }
    let pattern = format!(
        "(?iu)\\b(?:{})\\b",
        tokens
            .iter()
            .map(|s| regex::escape(s))
            .collect::<Vec<_>>()
            .join("|")
    );
    regex::Regex::new(&pattern)
        .expect("escaped identity tokens")
        .replace_all(text, "[redacted]")
        .into_owned()
}
fn step(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &TournamentState,
    m: ContestMove,
) -> Result<()> {
    let ordinal: u32 = tx.query_row(
        "SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",
        [state.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let step = ContestStep {
        contest_id: state.contest_id,
        step_id: StepId::new(),
        ordinal,
        script_version: Text::new("tournament-v1").unwrap(),
        move_name: m,
        inputs: vec![],
        assignments: vec![],
    };
    let e = gate::emit(
        tx,
        group,
        id,
        state.contest_id,
        EnvelopePayload::ContestStep(step.clone()),
    )?;
    squad_store::contest_steps::insert(tx, &step, e.commit.seq)?;
    Ok(())
}
fn work(
    state: &TournamentState,
    setup: &TournamentSetup,
    trigger: EventId,
    target: Target,
    case: Option<CaseId>,
    pack: &PromptPack,
    config: &SquadConfig,
) -> Work {
    let judge = target == Target::Judge;
    let template = if judge {
        "tournament.judge.md"
    } else {
        "tournament.entrant.md"
    };
    Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: (!judge).then_some(setup.prompt_message_id),
        kind: if judge {
            WorkKind::JudgeTournament
        } else {
            WorkKind::Reply
        },
        context_id: ContextId(if let Some(c) = case {
            format!("judge:{c}")
        } else {
            target.to_string()
        }),
        target,
        trigger_event_id: trigger,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: Some(state.round),
        replay: Some(state.replay),
        case_id: case,
        prompt_ref: Some(PromptRef {
            template_id: Text::new(template).unwrap(),
            ..crate::runtime::prompt(
                pack,
                config,
                if judge {
                    "judge.tournament"
                } else {
                    "tournament.entry"
                },
            )
        }),
        max_output_tokens: Some(if judge {
            constants::FINAL_JUDGE_VISIBLE_TOKENS
        } else {
            constants::REPLY_VISIBLE_TOKENS
        }),
        contest_id: Some(state.contest_id),
        step_id: None,
    }
}
impl Router {
    pub(crate) fn drive_tournaments(&mut self) -> Result<Vec<CommitOutcome>> {
        let Some(config) = self.engine_config.clone() else {
            return Ok(vec![]);
        };
        let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| StoreError::Unavailable)?;
        let scope = self.engine_scope.clone();
        let ids = self.store.call(move |db| {
            let mut q =
                db.prepare("SELECT contest_id FROM contests WHERE kind='tournament' AND active=1")?;
            Ok(q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = Vec::new();
        for raw in ids {
            let contest: ContestId = raw.parse().map_err(|_| StoreError::Unavailable)?;
            if scope.as_ref().is_some_and(|s| !s.contains(&contest)) {
                continue;
            }
            outcomes.extend(super::contests::drive_script(|| -> Result<Option<CommitOutcome>> {
                let config=config.clone();let pack=pack.clone();let id=EventId::new();
                // Determine readiness before allocating a receipt. A no-op never
                // schedules a timer or invents new execution identities.
                let ready=self.store.call(move|db|{
                    let s=squad_store::tournaments::state(db,contest)?;
                    Ok(match s.state {
                        TournamentStatus::WaitingForEntrants=>!s.survivors.iter().any(|n|db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND contest_id IS NULL AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get::<_,bool>(0)).unwrap_or(true)),
                        TournamentStatus::Answering|TournamentStatus::Judging=>{
                            let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND round=? AND replay=? AND (status='running' OR (status='queued' AND attempt<CASE kind WHEN 'judge.tournament' THEN 3 ELSE 2 END)) AND (execution_deadline IS NULL OR execution_deadline>?))",params![contest.to_string(),s.round,s.replay,chrono::Utc::now().to_rfc3339()],|r|r.get(0))?;
                            !pending
                        },_=>false
                    })
                })?;
                if !ready{return Ok(None);}
                let out=self.store.commit(id,"contest-engine".into(),squad_store::commit::digest(&(contest,id))?,move|tx,group|{
                    let mut s=squad_store::tournaments::state(tx,contest)?;
                    let capped:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND json_extract(error_json,'$.code')='DAILY_MAX_REACHED')",[contest.to_string()],|r|r.get(0))?;
                    if capped {let old=s.state_version;s.state_version+=1;s.state=TournamentStatus::Failed;s.terminal_reason=Some("DAILY_MAX_REACHED".into());gate::save(tx,group,id,&s,Some(old))?;tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[contest.to_string()])?;return Ok(group.accepted(id));}
                    let setup=squad_store::tournaments::setup(tx,contest)?;
                    let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='tournament.bindings'",[contest.to_string()],|r|r.get(0))?;
                    let bindings:BTreeMap<String,ExecutorBinding>=serde_json::from_str(&raw)?;
                    if s.state==TournamentStatus::WaitingForEntrants {let old=s.state_version;s.state_version+=1;s.state=TournamentStatus::Answering;gate::save(tx,group,id,&s,Some(old))?;}
                    if s.state==TournamentStatus::Answering {
                        let count:u32=tx.query_row("SELECT COUNT(*) FROM work WHERE contest_id=? AND round=? AND replay=? AND kind='reply'",params![contest.to_string(),s.round,s.replay],|r|r.get(0))?;
                        if count==0 {
                            step(tx,group,id,&s,ContestMove::Fanout)?;
                            tx.execute("INSERT INTO tournament_rounds(contest_id,round,replay,state_json) VALUES (?,?,?,'{}')",params![contest.to_string(),s.round,s.replay])?;
                            let e=gate::emit(tx,group,id,contest,EnvelopePayload::TournamentState(s.clone()))?;
                            for nick in &s.survivors {
                                // The setup is delivered only once. Existing member history
                                // remains private and all later ordinary work stays held.
                                if s.round==1 && s.replay==0 { squad_store::history::project(tx,&squad_store::history::attributed(&e,ContextId(nick.to_string()),"tournament.setup",HistoryRole::User,Some("squad".into()),setup.prompt.clone()))?; }
                                let mut w=work(&s,&setup,e.base.id,Target::Member(nick.clone()),None,&pack,&config);
                                let boundary:i64=tx.query_row("SELECT MAX(pos) FROM history WHERE context_id=?",[nick.as_str()],|r|r.get(0))?;w.history_through=HistoryPosition::new(boundary).map_err(|_|gate::invalid())?;
                                squad_store::work::insert(tx,&w,&bindings[nick.as_str()],None)?;
                                crate::router::contests::queued(tx,group,&w)?;
                            }
                        }else{
                            let entries=squad_store::tournaments::entries(tx,contest,s.round,s.replay)?;
                            for nick in &s.survivors {
                                if entries.iter().any(|(_,e)|&e.entrant==nick){continue;}
                                let work_id:String=tx.query_row("SELECT work_id FROM work WHERE contest_id=? AND round=? AND replay=? AND target=?",params![contest.to_string(),s.round,s.replay,nick.as_str()],|r|r.get(0))?;
                                let mut w=squad_store::work::template(tx,work_id.parse().map_err(|_|gate::invalid())?)?.ok_or_else(gate::invalid)?.0;
                                w.attempt=tx.query_row("SELECT MAX(attempt,1) FROM work WHERE work_id=?",[work_id],|r|r.get(0))?;
                                let entry=TournamentEntry{generation:GenerationIdentity{work_id:w.work_id,message_id:w.message_id,trigger_event_id:w.trigger_event_id,attempt:w.attempt},contest_id:contest,round:s.round,replay:s.replay,entrant:nick.clone(),outcome:super::rounds::forfeit(ForfeitReason::Error)};
                                gate::emit(tx,group,id,contest,EnvelopePayload::TournamentEntry(entry))?;
                            }
                            let old=s.state_version;s.state_version+=1;s.state=TournamentStatus::Judging;gate::save(tx,group,id,&s,Some(old))?;
                            step(tx,group,id,&s,ContestMove::Judge)?;
                            let mut shuffled=s.survivors.iter().map(|n|(EventId::new(),n.clone())).collect::<Vec<_>>();shuffled.sort_by_key(|(seed,_)|seed.to_string());
                            let entries=squad_store::tournaments::entries(tx,contest,s.round,s.replay)?;
                            let mut tokens=Vec::new();
                            for a in &config.agents {tokens.extend([a.nick.to_string(),a.model.clone(),a.persona.clone()]);}
                            for binding in bindings.values(){tokens.push(binding.model.to_string());}
                            for token in tokens.clone(){ tokens.extend(token.split(['/', '-', '_', '.', ':']).filter(|p|!p.is_empty()).map(str::to_owned)); }
                            let mut blind=Vec::new();
                            for (i,(_,nick)) in shuffled.iter().enumerate(){let label=((b'A'+i as u8) as char).to_string();tx.execute("INSERT INTO tournament_labels VALUES (?,?,?,?,?)",params![contest.to_string(),s.round,s.replay,label,nick.as_str()])?;
                                let answer=entries.iter().find(|(_,e)|&e.entrant==nick).and_then(|(_,e)|if let DebateTurnOutcome::Completed{content,..}=&e.outcome{Some(redact(content,&tokens))}else{None});blind.push(TournamentBlindEntry{label,answer});}
                            let material=TournamentCaseMaterial{prompt:redact(&setup.prompt,&tokens),round:s.round,entries:blind};
                            let case=CaseId::new();let invocation=EventId::new();
                            let opened=CaseOpened{case_id:case,judge_invocation_id:invocation,work_id:None,judge_model:Text::new(&setup.judge_model).map_err(|_|gate::invalid())?,material:CaseMaterial::Tournament{material:material.clone()},debate_id:None,contest_id:Some(contest)};
                            // The human case carrier also contains only blind material.
                            let e=gate::emit(tx,group,id,contest,EnvelopePayload::CaseOpened(opened.clone()))?;squad_store::cases::open(tx,&opened,e.commit.seq)?;
                            squad_store::history::project(tx,&squad_store::history::attributed(&e,ContextId(format!("judge:{case}")),"tournament.blind",HistoryRole::User,None,serde_json::to_string(&material)?))?;
                            let mut w=work(&s,&setup,e.base.id,Target::Judge,Some(case),&pack,&config);let boundary:i64=tx.query_row("SELECT MAX(pos) FROM history WHERE context_id=?",[&w.context_id.0],|r|r.get(0))?;w.history_through=HistoryPosition::new(boundary).map_err(|_|gate::invalid())?;
                            let mut binding=bindings["judge"].clone();binding.effort=ReasoningEffort::High;
                            squad_store::work::insert(tx,&w,&binding,None)?;crate::router::contests::queued(tx,group,&w)?;
                            tx.execute("UPDATE tournament_rounds SET judge_work_id=? WHERE contest_id=? AND round=? AND replay=?",params![w.work_id.to_string(),contest.to_string(),s.round,s.replay])?;
                        }
                    }else if s.state==TournamentStatus::Judging {
                        let old=s.state_version;s.state_version+=1;
                        if s.replay==0{s.replay=1;s.state=TournamentStatus::Answering;}else{s.state=TournamentStatus::Failed;s.terminal_reason=Some("JUDGE_INVALID_VERDICT".into());let mut error=crate::router::error(ErrorCode::JUDGE_INVALID_VERDICT,"Tournament judging failed after replay");error.contest_id=Some(contest);gate::emit(tx,group,id,contest,EnvelopePayload::Error(error))?;}
                        // Fence all abandoned work before creating the replay.
                        tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[contest.to_string()])?;
                        gate::save(tx,group,id,&s,Some(old))?;
                    }
                    Ok(group.accepted(id))
                })?;Ok(Some(out))
            })?);
        }
        Ok(outcomes)
    }
}
