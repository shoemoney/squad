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
