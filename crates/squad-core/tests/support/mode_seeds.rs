//! Run rows a mode needs before `/stop` or a human answer can load its state.
#![allow(dead_code)]
use squad_protocol::*;
// Mode callbacks persist resolved choices and stop reports in the owned run row.
pub fn seed_readback(db: &rusqlite::Connection, contest: ContestId) -> squad_store::Result<()> {
    use squad_protocol::readback::*;
    let setup = ReadbackSetup {
        spec: "The service retains records. The operator deletes backups.".into(),
        participants: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        rounds: 2,
        room_words: 120,
        plan_words: 500,
        models: ["a", "b", "c", "judge"]
            .map(|n| {
                (
                    n.into(),
                    ExecutorBinding {
                        model: "test/model".parse().unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .into(),
        prompt_refs: ["readback.interpret", "readback.diff", "readback.rewrite"]
            .map(|name| {
                (
                    name.into(),
                    PromptRef {
                        pack_version: "fixture-v1".parse().unwrap(),
                        template_id: name.parse().unwrap(),
                        template_hash: "fixture-hash".parse().unwrap(),
                    },
                )
            })
            .into(),
    };
    squad_store::readback::insert(db, contest, &setup)?;
    let mut run = squad_store::readback::run(db, contest)?;
    let entries = [
        "The service retains records.",
        "The operator deletes backups.",
    ]
    .into_iter()
    .enumerate()
    .map(|(i, passage)| ReadbackEntry {
        entry_id: format!("entry{i}"),
        passage: passage.into(),
        question: "Which behaviour?".into(),
        behaviour_change: "Observable behaviour differs".into(),
        readings: ["a", "b"]
            .map(|id| ReadbackReading {
                reading_id: id.into(),
                text: format!("Meaning {id}"),
                source_labels: vec![format!("Reader {}", if id == "a" { 1 } else { 2 })],
            })
            .to_vec(),
    })
    .collect();
    run.rounds.push(squad_store::readback::Round {
        spec: setup.spec,
        ledger: Some(ReadbackLedger { entries }),
        ..Default::default()
    });
    squad_store::readback::save(db, contest, &run)
}
pub fn seed_debugging(db: &rusqlite::Connection, contest: ContestId) -> squad_store::Result<()> {
    use squad_protocol::debugging::*;
    let models = ["a", "b", "c", "judge"]
        .map(|name| {
            (
                name.into(),
                ExecutorBinding {
                    model: "test/model".parse().unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .into();
    let prompt_refs = [
        "debug.hypothesize",
        "debug.merge",
        "debug.test",
        "debug.select",
        "debug.report",
    ]
    .map(|name| {
        (
            name.into(),
            PromptRef {
                pack_version: "fixture-v1".parse().unwrap(),
                template_id: name.parse().unwrap(),
                template_hash: "fixture-hash".parse().unwrap(),
            },
        )
    })
    .into();
    let setup = DebuggingSetup {
        brief: "Requests fail".into(),
        limits: "Read only".into(),
        participants: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        rounds: 3,
        room_words: 120,
        plan_words: 500,
        models,
        prompt_refs,
    };
    squad_store::debugging::insert(db, contest, &setup)?;
    let mut run = squad_store::debugging::run(db, contest)?;
    run.differential = serde_json::from_value(serde_json::json!([
        {"hypothesis":{"hypothesisId":"H1","cause":"DNS","sources":["A:a"],"supporting":[],"contradicting":[],"priorSpread":["high"]},"status":"live","consistentResults":0,"failedPrediction":null},
        {"hypothesis":{"hypothesisId":"H2","cause":"Server","sources":["A:b"],"supporting":[],"contradicting":[],"priorSpread":["low"]},"status":"live","consistentResults":0,"failedPrediction":null}
    ]))?;
    run.rounds.push(DebuggingRound {round:1,proposals:Default::default(),selection:Some(serde_json::from_value(serde_json::json!({"testRef":"A:t","instruction":"Read the resolver output","outcomes":[{"label":"O1","description":"DNS fails"},{"label":"O2","description":"DNS works"}],"predictions":[{"hypothesisId":"H1","outcome":"O1"},{"hypothesisId":"H2","outcome":"O2"}],"addedHypotheses":[],"notSelected":[]}))?),result:None,elimination:None});
    run.resume = DebuggingStatus::AwaitingHuman;
    squad_store::debugging::save(db, contest, &run)
}
