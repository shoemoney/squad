//! Probe persistence and step-scoped views. No executor can read private role maps.
use crate::{Result, StoreError, error::{decode,encode,invalid}};
use rusqlite::{Connection,params};
use squad_protocol::*;

pub fn state(db:&Connection,id:ContestId)->Result<ProbeState> {
    let raw=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='probe' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    decode(raw)
}
pub fn setup(db:&Connection,id:ContestId)->Result<ProbeSetup> {
    let raw=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='probe.setup'",[id.to_string()],|r|r.get(0))?; decode(raw)
}
pub fn save(db:&Connection,state:&ProbeState,expected:u64,seq:Cursor)->Result<()> {
    if db.execute("UPDATE contests SET state_json=?,state_version=?,active=?,updated_seq=? WHERE contest_id=? AND kind='probe' AND state_version=? AND epoch=(SELECT epoch FROM room_meta)",params![encode(state)?,state.state_version as i64,!state.state.is_terminal(),seq.get(),state.contest_id.to_string(),expected as i64])?!=1 {return Err(invalid("Probe state changed"));}
    if state.state.is_terminal() {release(db,state.contest_id)?; crate::history::release(db,state.contest_id)?;}
    Ok(())
}
pub fn roles(db:&Connection,setup:&ProbeSetup)->Result<()> {
    setup.validate().map_err(|_|invalid("Invalid probe setup"))?;
    for (role,nicks) in [("explainer",&setup.explainers),("checker",&setup.checkers)] {for nick in nicks {db.execute("INSERT INTO probe_roles(contest_id,nick,role) VALUES (?,?,?)",params![setup.contest_id.to_string(),nick.as_str(),role])?;}}
    if let Some(nick)=&setup.sitting_out {db.execute("INSERT INTO probe_roles(contest_id,nick,role) VALUES (?,?,'sitting_out')",params![setup.contest_id.to_string(),nick.as_str()])?;}
    crate::contest_steps::private_map(db,setup.contest_id,"probe.setup",&serde_json::to_value(setup)?)
}
pub fn material(db:&Connection,id:ContestId)->Result<ProbeCaseMaterial> {
    let raw=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='probe.material'",[id.to_string()],|r|r.get(0))?; decode(raw)
}
/// Read only the immutable per-assignment delivery. Never returns artifact envelopes.
pub fn input(db:&Connection,work:&Work)->Result<serde_json::Value> {
    let step=work.step_id.ok_or_else(||invalid("Missing probe step"))?;
    let raw:String=db.query_row("SELECT content FROM history WHERE context_id=? AND projection_key='probe.input'",[&work.context_id.0],|r|r.get(0))?;
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM contest_assignments a JOIN contest_steps s ON s.step_id=a.step_id JOIN contests c ON c.contest_id=s.contest_id WHERE a.step_id=? AND a.work_id=? AND a.context_id=? AND c.kind='probe' AND c.active=1 AND c.epoch=(SELECT epoch FROM room_meta))",params![step.to_string(),work.work_id.to_string(),work.context_id.0],|r|r.get(0))?;
    if !valid {return Err(invalid("Probe assignment not active"));} Ok(serde_json::from_str(&raw)?)
}
fn ids<T>(v:&ProbeOutput<T>,work:&Work)->Result<()> {
    let g=&v.completed.generation;
    if work.contest_id!=Some(v.contest_id) || work.step_id!=Some(v.step_id) || g.work_id!=work.work_id || g.message_id!=work.message_id || g.trigger_event_id!=work.trigger_event_id || g.attempt!=work.attempt {return Err(invalid("Probe result identity mismatch"));} Ok(())
}
pub fn validate_output(db:&Connection,work:&Work,payload:&EnvelopePayload)->Result<()> {
    let id=work.contest_id.ok_or_else(||invalid("Missing probe contest"))?;
    let state=state(db,id)?;
    if state.state.is_terminal() {return Err(invalid("Probe is terminal"));}
    let setup=setup(db,id)?;
    let role=match &work.target {Target::Member(n) if setup.explainers.contains(n)=>"explainer", Target::Member(n) if setup.checkers.contains(n)=>"checker",Target::Judge=>"judge",_=>return Err(invalid("Not a probe participant"))};
    match payload {
        EnvelopePayload::ProbeExplanation(v) if work.kind==WorkKind::ProbeExplain && role=="explainer" && state.state==ProbeStatus::Proposing=>ids(v,work)?,
        EnvelopePayload::ProbeCheck(v) if work.kind==WorkKind::ProbeCheck && role=="checker" && state.state==ProbeStatus::Proposing=> {ids(v,work)?; v.data.validate().map_err(|_|invalid("Invalid check"))?;},
        EnvelopePayload::ProbePrediction(v) if work.kind==WorkKind::ProbePredict && role=="explainer" && state.state==ProbeStatus::Predicting=> {ids(v,work)?;v.data.validate_against(&material(db,id)?.checks).map_err(|_|invalid("Invalid prediction coverage"))?;},
        EnvelopePayload::ProbeReport(v) if work.kind==WorkKind::ProbeReport && role=="judge" && state.state==ProbeStatus::Reporting=> {ids(v,work)?;v.data.validate_against(&material(db,id)?).map_err(|_|invalid("Invalid report references or predictions"))?;},
        _=>return Err(invalid("Probe output not assigned")),
    } Ok(())
}
pub fn accept(db:&Connection,work:&Work,payload:&EnvelopePayload,seq:Cursor)->Result<()> {
    validate_output(db,work,payload)?;
    let id=work.contest_id.ok_or(StoreError::Unavailable)?;
    let step=work.step_id.ok_or(StoreError::Unavailable)?;
    let kind=serde_json::to_value(payload)?["kind"].as_str().ok_or(StoreError::Unavailable)?.to_owned();
    db.execute("INSERT INTO mode_artifacts VALUES (?,?,?,?,?,'[]')",params![seq.get(),id.to_string(),step.to_string(),kind,work.work_id.to_string()])?;
    if let Target::Member(nick)=&work.target {
        let column=if work.kind==WorkKind::ProbePredict {"prediction_event_seq"}else{"initial_event_seq"};
        if db.execute(&format!("UPDATE probe_roles SET {column}=? WHERE contest_id=? AND nick=? AND {column} IS NULL"),params![seq.get(),id.to_string(),nick.as_str()])?!=1 {return Err(invalid("Probe result already accepted"));}
    }
    crate::contest_steps::settle(db,step,&work.target,None)?;
    crate::work::finish(db,work.work_id,WorkStatus::Completed,seq)?;
    crate::contests::associate(db,id,ContestKind::Probe,seq)
}

pub fn release(db:&Connection,id:ContestId)->Result<()> {
    let mut q=db.prepare("SELECT day,SUM(amount_micros) FROM reservations WHERE contest_id=? AND attempt IS NULL AND unresolved=0 GROUP BY day")?;
    let rows=q.query_map([id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (day,amount) in rows {db.execute("UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",params![amount,day])?;}
    db.execute("DELETE FROM reservations WHERE contest_id=? AND attempt IS NULL AND unresolved=0",[id.to_string()])?;
    db.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=?",[id.to_string()])?;
    Ok(())
}
