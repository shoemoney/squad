use serde_json::json;
use squad_agents::agents::survey::validate;
use squad_protocol::*;
fn fixture(second: bool) -> (Work, CompletedGeneration) {
    let nick: MemberNick = "alpha".parse().unwrap();
    let contest = ContestId::new();
    let step = StepId::new();
    let id = WorkId::new();
    let message_id = MessageId::new();
    let trigger = EventId::new();
    let work = Work {
        work_id: id,
        message_id,
        in_reply_to_message_id: None,
        kind: if second {
            WorkKind::SurveyRevote
        } else {
            WorkKind::SurveyVote
        },
        target: Target::Member(nick.clone()),
        context_id: ContextId(format!("contest:{contest}:{step}:{nick}")),
        trigger_event_id: trigger,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: None,
        prompt_ref: None,
        max_output_tokens: Some(2048),
        contest_id: Some(contest),
        step_id: Some(step),
    };
    let completed = CompletedGeneration {
        generation: GenerationIdentity {
            message_id,
            work_id: id,
            trigger_event_id: trigger,
            attempt: 1,
        },
        model: Text::new("fixture/member").unwrap(),
        finish_reason: FinishReason::Stop,
        usage: None,
    };
    (work, completed)
}
#[test]
fn canonical_ballot_adapter_rejects_counts_foreign_ids_invalid_confidence_and_missing_evidence() {
    let (work, completed) = fixture(false);
    let data = json!({"question":"Question","claims":[{"claimId":"c1","claim":"Claim"}]});
    let valid = json!({"votes":[{"claimId":"c1","vote":"AGREE","reason":"Supported by evidence","confidence":5}]});
    assert!(matches!(
        validate(&work, &data, &valid.to_string(), completed.clone(), 100).unwrap(),
        EnvelopePayload::SurveyVote(_)
    ));
    for bad in [
        json!({"votes":[],"counts":{"agree":1}}),
        json!({"votes":[{"claimId":"foreign","vote":"AGREE","reason":"Evidence","confidence":3}]}),
        json!({"votes":[{"claimId":"c1","vote":"TRUE","reason":"Evidence","confidence":3}]}),
        json!({"votes":[{"claimId":"c1","vote":"AGREE","reason":"Evidence","confidence":6}]}),
    ] {
        assert!(validate(&work, &data, &bad.to_string(), completed.clone(), 100).is_err());
    }
    assert!(validate(&work, &data, &valid.to_string(), completed.clone(), 1).is_err());
    let (second, completed) = fixture(true);
    assert!(validate(&second, &data, &valid.to_string(), completed.clone(), 100).is_err());
    let mut valid = valid;
    valid["votes"][0]["changeEvidence"] = json!("A reproducible counterexample");
    assert!(matches!(
        validate(&second, &data, &valid.to_string(), completed, 100).unwrap(),
        EnvelopePayload::SurveyRevote(_)
    ));
}
