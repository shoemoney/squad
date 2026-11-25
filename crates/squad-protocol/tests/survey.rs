use squad_protocol::*;
#[test]
fn ballot_coverage_and_change_evidence_are_strict() {
    let mut b = SurveyBallot {
        votes: vec![SurveyBallotItem {
            claim_id: "c1".into(),
            vote: SurveyChoice::Agree,
            reason: "Observed evidence".into(),
            confidence: 5,
            change_evidence: None,
        }],
    };
    let ids = vec!["c1".into()];
    b.validate(&ids, false).unwrap();
    assert!(b.validate(&ids, true).is_err());
    b.votes[0].change_evidence = Some("A contrary measurement".into());
    b.validate(&ids, true).unwrap();
    assert!(b.validate(&ids, false).is_err());
    b.votes[0].confidence = 6;
    assert!(b.validate(&ids, true).is_err());
    b.votes[0].confidence = 1;
    b.votes.push(b.votes[0].clone());
    assert!(b.validate(&ids, true).is_err());
    assert!(
        validation::decode::<SurveyBallot>(&serde_json::json!({"votes":[],"tally":{}})).is_err()
    );
}
#[test]
fn script_counts_include_optional_collection_and_shared_attempts() {
    assert_eq!(SurveyPreflight::counts(6, true), (20, 42));
    assert_eq!(SurveyPreflight::counts(6, false), (14, 30));
    assert_eq!(SurveyPreflight::counts(4, true), (14, 30));
}
#[test]
fn extraction_rejects_foreign_origins_duplicate_text_and_overflow() {
    let a = ContestAnswer {
        nick: MemberNick::new("alpha").unwrap(),
        message_id: MessageId::new(),
        text: "source".into(),
    };
    let c = SurveyClaim {
        id: "c1".into(),
        claim: "Atomic proposition".into(),
        source_answer_ids: vec![a.message_id],
    };
    let mut e = SurveyExtraction {
        claims: vec![c.clone()],
    };
    e.validate(std::slice::from_ref(&a)).unwrap();
    e.claims[0].source_answer_ids = vec![MessageId::new()];
    assert!(e.validate(std::slice::from_ref(&a)).is_err());
    e.claims[0] = c.clone();
    let mut duplicate = c.clone();
    duplicate.id = "c2".into();
    duplicate.claim = " atomic   PROPOSITION ".into();
    e.claims.push(duplicate);
    assert!(e.validate(std::slice::from_ref(&a)).is_err());
    e.claims = (0..26)
        .map(|i| SurveyClaim {
            id: format!("c{i}"),
            claim: format!("Proposition {i}"),
            ..c.clone()
        })
        .collect();
    assert!(e.validate(&[a]).is_err());
}
