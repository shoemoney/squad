use squad_protocol::*;
use squad_store::{Store, forge};
#[test]
fn private_maps_are_immutable_and_clear_cascades_invocations() {
    let store = Store::open(":memory:").unwrap();
    let c = ContestId::new();
    store.call(move|db|{db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'forge',1,1,'{}',0)",[c.to_string()])?;forge::put(db,c,"ballot:a",&serde_json::json!({"D1":"b","D2":"c"}))?;assert!(forge::put(db,c,"ballot:a",&serde_json::json!({"D1":"c","D2":"b"})).is_err());assert_eq!(forge::get(db,c,"ballot:a")?["D1"],"b");assert!(forge::get(db,c,"ballot:b").is_err());let tx=db.transaction()?;squad_store::clear::content(&tx)?;tx.commit()?;assert!(forge::get(db,c,"ballot:a").is_err());Ok(())}).unwrap();
}
#[test]
fn state_version_and_terminal_invariants() {
    let c = ContestId::new();
    let mut state = ForgeState {
        contest_id: c,
        state_version: 1,
        state: ForgeStatus::Drafting,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    assert!(state.validate().is_ok());
    state.state = ForgeStatus::Failed;
    assert!(state.validate().is_err());
    state.terminal_reason = Some("insufficient_drafts".parse().unwrap());
    assert!(state.validate().is_ok());
    state.round = 1;
    assert!(state.validate().is_err());
}
