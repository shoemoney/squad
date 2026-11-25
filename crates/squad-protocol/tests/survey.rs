#![cfg(feature = "mode-survey")]
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
#[test]
fn survey_state_and_anonymous_projection_reject_missing_nullable_and_private_fields() {
    let id = ContestId::new();
    let event = EventId::new();
    let mut state = SurveyState {
        contest_id: id,
        state: SurveyStatus::Voting,
        state_version: 1,
        step_id: None,
        artifact_event_ids: vec![event],
        terminal_reason: None,
    };
    state.validate().unwrap();
    state.artifact_event_ids.push(event);
    assert!(state.validate().is_err());
    state.artifact_event_ids.pop();
    let mut value = serde_json::to_value(&state).unwrap();
    value.as_object_mut().unwrap().remove("terminalReason");
    assert!(validation::decode::<SurveyState>(&value).is_err());
    let mut claim = serde_json::json!({"claimId":"c1","claim":"Claim","agreeReasons":[],"disagreeReasons":[],"unsureReasons":[]});
    validation::decode::<SurveyAnonymousClaim>(&claim).unwrap();
    for field in ["author", "origins", "voteCount", "confidence"] {
        claim[field] = serde_json::json!(1);
        assert!(validation::decode::<SurveyAnonymousClaim>(&claim).is_err());
        claim.as_object_mut().unwrap().remove(field);
    }
    assert!(validation::decode::<SurveyVote>(&serde_json::json!({"contestId":id,"stepId":StepId::new(),"voter":"alpha","ballot":{"votes":[]}})).is_err());
    assert!(validation::decode::<SurveyReportText>(&serde_json::json!({"consensus":[],"contested":[],"loneInsights":[],"loneDissents":[],"rejectedOutliers":[],"limitations":"Caveat","tallies":[],"riskAcceptance":true})).is_err());
}
