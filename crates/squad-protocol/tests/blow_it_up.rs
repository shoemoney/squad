use serde_json::{Value, json};
use squad_protocol::{blow_it_up::*, validation};
fn vector(id: &str, diversity: bool) -> Value {
    json!({"localId":id,"title":"Access","targetQuote":"No authentication is required.","exploit":"Anyone writes data","severity":"high","likelihood":"likely","blastRadius":"All data","diversity":diversity})
}
#[test]
fn attacks_require_exactly_one_diversity_slot_and_unique_local_ids() {
    for flags in [vec![false], vec![true, true], vec![false, false]] {
        let v = json!({"vectors":flags.iter().enumerate().map(|(i,d)|vector(&i.to_string(),*d)).collect::<Vec<_>>()});
        assert!(validate_artifact("blow_it_up.attack", &v).is_err());
    }
    assert!(
        validate_artifact(
            "blow_it_up.attack",
            &json!({"vectors":[vector("a",false),vector("b",true)]})
        )
        .is_ok()
    );
    assert!(
        validate_artifact(
            "blow_it_up.attack",
            &json!({"vectors":[vector("a",false),vector("a",true)]})
        )
        .is_err()
    );
}
#[test]
fn challenge_verdict_dependencies_and_strict_nullable_fields() {
    for verdict in ["false_positive", "duplicate"] {
        let v = json!({"verdicts":[{"vectorId":"a","verdict":verdict,"reason":"Evidence","duplicateOf":null,"defeatingQuote":null}],"additions":[]});
        assert!(validate_artifact("blow_it_up.challenge", &v).is_err());
    }
    let mut v = json!({"verdicts":[{"vectorId":"a","verdict":"holds","reason":"Evidence","duplicateOf":null,"defeatingQuote":null}],"additions":[]});
    assert!(validate_artifact("blow_it_up.challenge", &v).is_ok());
    v["verdicts"][0]
        .as_object_mut()
        .unwrap()
        .remove("defeatingQuote");
    assert!(validate_artifact("blow_it_up.challenge", &v).is_err());
    assert!(
        validation::decode::<BlowItUpAttack>(
            &json!({"vectors":[vector("a",true)],"finder":"alpha"})
        )
        .is_err()
    );
}
#[test]
fn challenge_schema_has_no_private_diversity_field() {
    let schema = output_schema(&squad_protocol::WorkKind::BlowItUpChallenge)
        .unwrap()
        .to_string();
    assert!(!schema.contains("diversity"));
    assert!(!schema.contains("lens"));
    assert!(!schema.contains("attacker"));
}
fn ruling(sources: Value) -> BlowItUpRuling {
    serde_json::from_value(json!({"findings":[{"findingId":"f1","title":"Access","sourceVectorIds":sources,"severity":"high","likelihood":"likely","exploit":"Anyone writes","blastRadius":"All data","mitigation":"Authenticate","rebuttalStatus":"survived","residual":"Credential abuse","challengeRejection":null}],"killedNotes":[]})).unwrap()
}
#[test]
fn killed_or_absent_sources_cannot_be_reinstated_and_contested_requires_explanation() {
    let pool:Vec<PooledVector>=serde_json::from_value(json!([{"vectorId":"a","title":"Access","targetQuote":"No authentication is required.","exploit":"Anyone writes","severity":"high","likelihood":"likely","blastRadius":"All data","addition":false}])).unwrap();
    let mut tally = BlowItUpTally {
        vectors: vec![VectorTally {
            vector_id: "a".parse().unwrap(),
            accepted: 3,
            false_positive: 2,
            weak: 0,
            status: VectorStatus::Killed,
        }],
    };
    assert!(
        ruling(json!(["a"]))
            .validate_against(&pool, &tally)
            .is_err()
    );
    assert!(ruling(json!([])).validate_against(&pool, &tally).is_err());
    assert!(
        ruling(json!(["missing"]))
            .validate_against(&pool, &tally)
            .is_err()
    );
    tally.vectors[0].status = VectorStatus::Contested;
    let mut r = ruling(json!(["a"]));
    assert!(r.validate_against(&pool, &tally).is_err());
    r.findings[0].rebuttal_status = RebuttalStatus::Weakened;
    assert!(r.validate_against(&pool, &tally).is_ok());
    r.findings[0].rebuttal_status = RebuttalStatus::Survived;
    r.findings[0].challenge_rejection =
        Some("Defenses do not address the exploit".parse().unwrap());
    assert!(r.validate_against(&pool, &tally).is_ok());
}
#[test]
fn parser_examples_enable_and_nick_source_use_whole_roster() {
    use squad_protocol::*;
    let mut ctx = parser::ParseContext {
        roster: vec![
            "alpha".parse().unwrap(),
            "beta".parse().unwrap(),
            "gamma".parse().unwrap(),
        ],
        ..Default::default()
    };
    ctx.records.push(parser::RecordSummary {
        event_id: EventId::new(),
        message_id: Some(MessageId::new()),
        seq: Cursor::new(1).unwrap(),
        kind: "message.final".parse().unwrap(),
        author: Actor::Member("alpha".parse().unwrap()),
        recipients: vec![Recipient::Me],
        content: "No authentication is required.".into(),
        in_reply_to_message_id: Some(MessageId::new()),
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    });
    let parsed = parse(
        "@alpha $blow_it_up",
        &parser::SubmissionMeta::default(),
        &ctx,
    )
    .unwrap();
    let ParsedInput::BlowItUp {
        participants,
        source,
        ..
    } = parsed
    else {
        panic!("wrong AST")
    };
    assert_eq!(participants, ctx.roster);
    assert_eq!(
        source,
        parser::PlanSource::LatestReply {
            nick: "alpha".parse().unwrap()
        }
    );
    assert!(
        parse(
            "@all $blow_it_up ```No authentication is required.```",
            &parser::SubmissionMeta::default(),
            &ctx
        )
        .is_ok()
    );
}
