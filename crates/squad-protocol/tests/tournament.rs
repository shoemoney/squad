use squad_protocol::*;
#[test]
fn strict_rankings_are_permutations_with_integer_scores_and_one_line_critiques() {
    let labels = vec!["A".into(), "B".into()];
    let valid = r#"{"ranking":[{"label":"B","score":8,"critique":"Strong"},{"label":"A","score":8,"critique":"Weaker"}]}"#;
    assert!(TournamentJudgeResponse::from_json(valid, &labels).is_ok());
    for invalid in [
        valid.replace("\"B\"", "\"A\""),
        valid.replace("8", "0"),
        valid.replace("8", "8.5"),
        valid.replace("Strong", ""),
        valid.replace("Strong", "two\\nlines"),
        valid.replace("\"score\":8", "\"extra\":1,\"score\":8"),
    ] {
        assert!(TournamentJudgeResponse::from_json(&invalid, &labels).is_err());
    }
    assert!(TournamentJudgeResponse::from_json(valid, &["A".into()]).is_err());
}
#[test]
fn tournament_work_requires_replay_and_judge_allowances() {
    let id = "01900000-0000-7000-8000-000000000001";
    let mut w:Work=validation::decode(&serde_json::json!({"workId":id,"messageId":id,"kind":"judge.tournament","target":"judge","contextId":format!("judge:{id}"),"triggerEventId":id,"historyThrough":"1","attempt":1,"contestId":id,"caseId":id,"round":1,"replay":0,"maxOutputTokens":4096,"promptRef":{"templateId":"tournament.judge.md","packVersion":"v1","templateHash":"hash"}})).unwrap();
    assert!(w.validate().is_ok());
    w.replay = None;
    assert!(w.validate().is_err());
    w.replay = Some(0);
    w.attempt = 4;
    assert!(w.validate().is_err());
    w.attempt = 3;
    w.max_output_tokens = Some(8192);
    assert!(w.validate().is_err());
}
