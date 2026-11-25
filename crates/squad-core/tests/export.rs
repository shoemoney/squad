use serde_json::{Value, json};
use squad_core::export::GrillQuestion;
use squad_core::{banner::Banner, export::*};
use squad_protocol::*;

const CONTEST: &str = "01900000-0000-7000-8000-000000000100";
const OTHER: &str = "01900000-0000-7000-8000-000000000200";
const CASE: &str = "01900000-0000-7000-8000-000000000300";
fn uuid(n: u32) -> String {
    format!("01900000-0000-7000-8000-{n:012x}")
}
fn event(n: u32, kind: &str, payload: Value) -> CommittedEnvelope {
    let sender = match kind {
        "user.message" => "me",
        "message.final" => "alpha",
        "verdict" | "debate.round_score" | "case.ruling" => "judge",
        _ => "system",
    };
    serde_json::from_value(json!({"version":1,"id":uuid(n),"roomId":"main","correlationId":uuid(999),"sender":sender,"recipients":["me"],"createdAt":"2026-11-24T00:00:00Z","seq":n.to_string(),"acceptedAt":"2026-11-24T00:00:00Z","commitIndex":0,"commitSize":1,"kind":kind,"payload":payload})).unwrap()
}
fn state(n: u32, status: &str, round: u32) -> CommittedEnvelope {
    event(
        n,
        "debate.state",
        json!({"debateId":CONTEST,"state":status,"debaters":["alpha","beta"],"round":round,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":true,"stateVersion":n,"terminalReason":null,"caseId":CASE,"caseIds":[CASE]}),
    )
}
fn generation() -> Value {
    json!({"messageId":uuid(90),"workId":uuid(91),"triggerEventId":uuid(92),"attempt":1,"model":"public/model","finishReason":"stop","usage":null})
}
fn user(n: u32, text: &str) -> CommittedEnvelope {
    event(
        n,
        "user.message",
        json!({"messageId":uuid(n),"text":text,"body":text,"resolvedQuotes":[]}),
    )
}
fn debate() -> ExportSnapshot {
    let mut verdict = generation();
    verdict.as_object_mut().unwrap().extend(json!({"debateId":CONTEST,"caseId":CASE,"judgeInvocationId":uuid(93),"rubricVersion":"v1","winner":"alpha","scores":[{"nick":"alpha","criteria":[{"criterionId":"fit","score":9,"reasoning":"Matches the question."}]}],"reasoning":"The design fits the brief.","whatToStealFromLoser":["Borrow the index."],"tallyOverrideReason":"Stronger overall design.","weightedTotals":[{"nick":"alpha","total":8.5},{"nick":"beta","total":8}],"roundTally":{"alpha":1,"beta":2}}).as_object().unwrap().clone());
    ExportSnapshot {
        events: vec![
            event(
                1,
                "debate.setup",
                json!({"debateId":CONTEST,"topicMessageId":uuid(70),"topicText":"Build a search engine","solutions":[{"nick":"alpha","messageId":uuid(71),"text":"Use an index"},{"nick":"beta","messageId":uuid(72),"text":"Scan everything"}],"instruction":"PRIVATE-INSTRUCTION","rounds":3,"judgeModel":"judge/model","judgeSwapped":false}),
            ),
            state(2, "ROUND_RUNNING", 0),
            state(3, "ROUND_COMMITTING", 0),
            state(4, "ROUND_RUNNING", 1),
            event(
                5,
                "debate.turn",
                json!({"messageId":uuid(50),"workId":uuid(51),"triggerEventId":uuid(52),"attempt":1,"debateId":CONTEST,"round":1,"speaker":"alpha","outcome":{"status":"completed","content":"Index the pages.","model":"public/model","finishReason":"stop","usage":null}}),
            ),
            state(6, "ROUND_RUNNING", 3),
            event(
                7,
                "debate.round_score",
                json!({"debateId":CONTEST,"caseId":CASE,"round":1,"scores":{"alpha":{"score":9,"reason":"Clear"},"beta":{"score":7,"reason":"Slow"}},"tally":{"alpha":1,"beta":0}}),
            ),
            event(
                8,
                "debate.round_score",
                json!({"debateId":CONTEST,"caseId":CASE,"round":3,"scores":null,"tally":{"alpha":1,"beta":0},"reason":"unscored"}),
            ),
            event(
                9,
                "debate.turn",
                json!({"messageId":uuid(53),"workId":uuid(54),"triggerEventId":uuid(55),"attempt":1,"debateId":CONTEST,"round":3,"speaker":"beta","outcome":{"status":"forfeit","reasonCode":"timeout","text":"Timed out"}}),
            ),
            state(10, "JUDGING", 3),
            event(11, "verdict", verdict),
            state(12, "COMPLETED", 3),
        ],
        ..Default::default()
    }
}
#[test]
fn debate_snapshot_and_late_round_identity() {
    let snapshot = debate();
    let output = render(
        &snapshot,
        ExportScope::Debate,
        Some(CONTEST.parse().unwrap()),
    )
    .unwrap();
    insta::assert_snapshot!("debate", output);
    assert!(output.contains("round 1 score"));
    assert!(!output.contains("PRIVATE-INSTRUCTION"));
    for b in [
        Banner::Opening,
        Banner::Round(1),
        Banner::FinalRound,
        Banner::Verdict,
    ] {
        assert_eq!(
            output
                .matches(&squad_core::banner::render_default(&b.text()))
                .count(),
            1
        );
    }
    let mut reversed = snapshot.clone();
    reversed.events.reverse();
    assert_eq!(
        output,
        render(
            &reversed,
            ExportScope::Debate,
            Some(CONTEST.parse().unwrap())
        )
        .unwrap()
    );
}
#[test]
fn room_supersession_and_private_records() {
    let mut completion = generation();
    completion.as_object_mut().unwrap().extend(
        json!({"inReplyToMessageId":uuid(1),"content":"Original reply"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let s = ExportSnapshot {
        events: vec![
            user(1, "Hello"),
            event(2, "message.final", completion),
            event(
                3,
                "message.superseded",
                json!({"supersededEventId":uuid(2),"supersededMessageId":uuid(90),"member":"alpha","inReplyToMessageId":uuid(1),"replacementWorkId":uuid(101),"replacementMessageId":uuid(102)}),
            ),
            event(
                4,
                "system.notice",
                json!({"code":"notice","text":"Public notice"}),
            ),
            event(
                5,
                "contest.artifact",
                json!({"contestId":CONTEST,"stepId":uuid(80),"artifactKind":"private","sourceRefs":[],"data":{"labelMap":"SECRET-LABEL-MAP","reasoning":"SECRET-REASONING","prompt":"SECRET-PROMPT"}}),
            ),
        ],
        ..Default::default()
    };
    let output = render(&s, ExportScope::Room, None).unwrap();
    assert!(!output.contains("SECRET"));
    assert!(output.contains("superseded"));
    insta::assert_snapshot!("room", output);
}
#[test]
fn question_first_and_scope_isolation() {
    let mut s = ExportSnapshot {
        events: vec![
            user(1, "Question"),
            user(2, "Unrelated"),
            event(
                3,
                "contest.collecting",
                json!({"contestId":CONTEST,"kind":"debate","questionMessageId":uuid(1),"question":"Question","addressed":["alpha","beta"]}),
            ),
            event(
                4,
                "contest.pick_requested",
                json!({"contestId":CONTEST,"kind":"debate","questionMessageId":uuid(1),"eligible":["alpha","beta"],"requiredPicks":2,"banner":"Choose"}),
            ),
            event(
                5,
                "contest.state",
                json!({"kind":"selection","state":{"contestId":CONTEST,"kind":"debate","state":"STOPPED","stateVersion":1,"questionMessageId":uuid(1),"addressed":["alpha","beta"],"answers":[{"nick":"alpha","messageId":uuid(6),"text":"A solution"}],"requiredPicks":2}}),
            ),
        ],
        ..Default::default()
    };
    let output = render(&s, ExportScope::Debate, Some(CONTEST.parse().unwrap())).unwrap();
    assert!(!output.contains("Unrelated"));
    insta::assert_snapshot!("question_first", output);
    s.events.push(event(
        7,
        "room.cleared",
        json!({"epoch":2,"previousThrough":"6"}),
    ));
    assert_eq!(
        render(&s, ExportScope::Room, None).unwrap(),
        "# Squad transcript\n\n"
    );
    assert_eq!(
        render(&s, ExportScope::Debate, Some(CONTEST.parse().unwrap())),
        Err(ExportError::ContestNotFound)
    );
}
#[test]
fn tournament_and_grill_public_projections() {
    let mut s = ExportSnapshot::default();
    for (n, id, kind, status) in [
        (1, CONTEST, "tournament", "RUNNING"),
        (3, OTHER, "grill", "COMPLETED"),
    ] {
        s.events.push(event(n,"contest.state",json!({"kind":"generic","state":{"contestId":id,"kind":kind,"stateVersion":1,"status":status,"artifactEventIds":[],"terminalReason":null}})));
        s.events.push(event(n+1,"contest.artifact",json!({"contestId":id,"stepId":uuid(80),"artifactKind":"public","sourceRefs":[],"data":{"labelMap":"NEVER-EXPORT"}})));
    }
    s.boundaries
        .insert(uuid(1).parse().unwrap(), Banner::Tournament);
    s.boundaries
        .insert(uuid(3).parse().unwrap(), Banner::FinalPlan);
    s.artifacts.insert(
        uuid(2).parse().unwrap(),
        PublicArtifact::Tournament {
            round: 1,
            entries: vec![("alpha".parse().unwrap(), "An entry".into())],
            rankings: vec![Ranking {
                nick: "alpha".parse().unwrap(),
                placement: 1,
                score: 9.0,
                critique: "Strong".into(),
                eliminated: false,
                failed_replay: true,
            }],
            champion: Some("alpha".parse().unwrap()),
        },
    );
    s.artifacts.insert(
        uuid(4).parse().unwrap(),
        PublicArtifact::Grill {
            plans: vec![
                PlanVersion {
                    version: 1,
                    author: "alpha".parse().unwrap(),
                    content: "Draft".into(),
                    final_plan: false,
                },
                PlanVersion {
                    version: 2,
                    author: "alpha".parse().unwrap(),
                    content: "Revised plan".into(),
                    final_plan: true,
                },
            ],
            questions: vec![GrillQuestion {
                round: 1,
                griller: "beta".parse().unwrap(),
                question: "What about cost?".into(),
                leader: "alpha".parse().unwrap(),
                answer: "Cache it".into(),
                plan_changes: "Added caching".into(),
            }],
        },
    );
    let tournament = render(&s, ExportScope::Tournament, Some(CONTEST.parse().unwrap())).unwrap();
    let grill = render(&s, ExportScope::Grill, Some(OTHER.parse().unwrap())).unwrap();
    assert!(!tournament.contains("Revised plan"));
    assert!(!grill.contains("An entry"));
    assert!(!grill.contains("NEVER-EXPORT"));
    insta::assert_snapshot!("tournament", tournament);
    insta::assert_snapshot!("grill", grill);
}
#[test]
fn rejects_invalid_scopes_and_snapshots() {
    let mut s = debate();
    let id = Some(CONTEST.parse().unwrap());
    assert_eq!(
        render(&s, ExportScope::Room, id),
        Err(ExportError::RoomIdForbidden)
    );
    assert_eq!(
        render(&s, ExportScope::Debate, None),
        Err(ExportError::ContestIdRequired)
    );
    assert_eq!(
        render(&s, ExportScope::Grill, id),
        Err(ExportError::ScopeMismatch)
    );
    assert_eq!(
        render(&s, ExportScope::Census, id),
        Err(ExportError::ScopeMismatch)
    );
    s.events.push(s.events[0].clone());
    assert_eq!(
        render(&s, ExportScope::Room, None),
        Err(ExportError::InvalidSnapshot)
    );
}
#[test]
fn both_entry_points_take_exactly_one_snapshot() {
    struct Source(std::cell::Cell<u32>);
    impl ExportSource for Source {
        fn snapshot(&self) -> Result<ExportSnapshot, ExportError> {
            self.0.set(self.0.get() + 1);
            Ok(debate())
        }
    }
    let source = Source(std::cell::Cell::new(0));
    let live = export(&source, ExportScope::Room, None).unwrap();
    assert_eq!(source.0.get(), 1);
    assert_eq!(
        live,
        export_offline(&source, ExportScope::Room, None).unwrap()
    );
    assert_eq!(source.0.get(), 2);
    assert!(export(&source, ExportScope::Debate, None).is_err());
    assert_eq!(source.0.get(), 2);
}

#[test]
fn excludes_diagnostics_and_rejects_uncommitted_events() {
    let mut s = ExportSnapshot {
        events: vec![event(
            2,
            "error",
            json!({"code":"INTERNAL_ERROR","text":"PRIVATE-DIAGNOSTIC","retryable":false}),
        )],
        ..Default::default()
    };
    let mut private = event(
        1,
        "system.notice",
        json!({"code":"notice","text":"placeholder"}),
    );
    private.content = EnvelopePayload::VarChange(VariableChange {
        name: VariableName::Rounds,
        value: 3,
        previous_value: 2,
    });
    s.events.push(private);
    assert_eq!(
        render(&s, ExportScope::Room, None).unwrap(),
        "# Squad transcript\n\n"
    );
    s.events[0].content = EnvelopePayload::AgentReply(
        serde_json::from_value({
            let mut value = generation();
            value.as_object_mut().unwrap().extend(
                json!({"inReplyToMessageId":uuid(3),"content":"UNCOMMITTED"})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            value
        })
        .unwrap(),
    );
    assert_eq!(
        render(&s, ExportScope::Room, None),
        Err(ExportError::InvalidSnapshot)
    );
}

#[test]
fn standalone_case_exports_only_public_question_sources_and_ruling() {
    let mut ruling = generation();
    ruling.as_object_mut().unwrap().extend(json!({"caseId":CASE,"judgeInvocationId":uuid(93),"inReplyToEventId":uuid(1),"content":"Use the index"}).as_object().unwrap().clone());
    let s = ExportSnapshot {
        events: vec![
            event(
                1,
                "case.opened",
                json!({"caseId":CASE,"judgeInvocationId":uuid(93),"judgeModel":"judge/model","material":{"kind":"standalone","messages":[{"eventId":uuid(20),"author":"alpha","text":"An indexed design"}],"question":"Which design?"}}),
            ),
            event(2, "case.ruling", ruling),
        ],
        ..Default::default()
    };
    let output = render(&s, ExportScope::Room, None).unwrap();
    assert!(output.contains("Which design?"));
    assert!(output.contains("Source · alpha"));
    assert!(output.contains("Use the index"));
    assert!(!output.contains(&uuid(93)));
}

#[test]
fn survey_exports_committed_sources_both_ballots_all_bands_and_observed_switches() {
    let claim = json!({"id":"c1","claim":"The proposal is feasible.","sourceAnswerIds":[uuid(71)]});
    let map = json!([{"claimId":"c1","first":{"agree":1,"disagree":1,"unsure":0,"missing":0},"finalTally":{"agree":0,"disagree":2,"unsure":0,"missing":0},"notRevoted":["beta"],"switches":[{"voter":"alpha","from":"AGREE","to":"DISAGREE"}],"bands":["rejectedOutliers"]}]);
    let ballot = |voter: &str, choice: &str, second: bool| {
        let mut item = json!({"claimId":"c1","vote":choice,"reason":"The capacity measurement matters.","confidence":4});
        if second {
            item["changeEvidence"] = json!("A successful load test would change my mind.");
        }
        json!({"contestId":CONTEST,"stepId":uuid(80),"completed":generation(),"voter":voter,"ballot":{"votes":[item]}})
    };
    let snapshot = ExportSnapshot {
        events: vec![
            event(
                1,
                "survey.setup",
                json!({"contestId":CONTEST,"questionMessageId":uuid(70),"question":"Is the proposal feasible?","participants":["alpha","beta"],"answers":[{"nick":"alpha","messageId":uuid(71),"text":"It fits the capacity budget."},{"nick":"beta","messageId":uuid(72),"text":"A load test is needed."}],"collect":false}),
            ),
            event(
                2,
                "survey.claims",
                json!({"contestId":CONTEST,"stepId":uuid(80),"completed":generation(),"extraction":{"claims":[claim]}}),
            ),
            event(3, "survey.vote", ballot("alpha", "AGREE", false)),
            event(4, "survey.vote", ballot("beta", "DISAGREE", false)),
            event(
                5,
                "survey.selection",
                json!({"contestId":CONTEST,"stepId":uuid(80),"tallies":map,"claims":[{"claimId":"c1","claim":"The proposal is feasible.","agreeReasons":["Fits the budget"],"disagreeReasons":["Requires a load test"],"unsureReasons":[]}]}),
            ),
            event(6, "survey.revote", ballot("alpha", "DISAGREE", true)),
            event(
                7,
                "survey.report",
                json!({"contestId":CONTEST,"stepId":uuid(80),"completed":generation(),"map":map,"report":{"consensus":[],"contested":[],"loneInsights":[],"loneDissents":[],"rejectedOutliers":[{"claimId":"c1","explanation":"Likely error: verify with an independent load test.","settlingQuestion":"Which load profile matches production?"}],"limitations":"Models can share blind spots. Beta did not re-vote; its first ballot remains recorded."}}),
            ),
            event(
                8,
                "survey.state",
                json!({"contestId":CONTEST,"state":"COMPLETE","stateVersion":9,"stepId":null,"artifactEventIds":[uuid(2),uuid(3),uuid(4),uuid(5),uuid(6),uuid(7)],"terminalReason":null}),
            ),
        ],
        ..Default::default()
    };
    let output = render(
        &snapshot,
        ExportScope::Survey,
        Some(CONTEST.parse().unwrap()),
    )
    .unwrap();
    insta::assert_snapshot!("survey_committed_export", output);
    assert_eq!(
        render(
            &snapshot,
            ExportScope::Debate,
            Some(CONTEST.parse().unwrap())
        )
        .unwrap_err(),
        ExportError::ScopeMismatch
    );
    assert_eq!(
        render(&snapshot, ExportScope::Survey, None).unwrap_err(),
        ExportError::ContestIdRequired
    );
}

#[test]
fn typed_grill_export_retains_versions_and_attributed_appendix() {
    let snapshot = ExportSnapshot {
        events: vec![
            event(
                1,
                "grill.setup",
                json!({"contestId":CONTEST,"leader":"alpha","grillers":["beta"],"sourceMessageId":uuid(70),"initialPlan":"Original plan","focus":"Costs","totalRounds":2}),
            ),
            event(
                2,
                "grill.plan",
                json!({"contestId":CONTEST,"leader":"alpha","round":0,"version":1,"text":"Original plan"}),
            ),
            event(
                3,
                "grill.question",
                json!({"contestId":CONTEST,"round":1,"turn":0,"griller":"beta","leader":"alpha","planVersion":1,"angle":"cost","questions":["Who pays?"]}),
            ),
            event(
                4,
                "grill.answer",
                json!({"contestId":CONTEST,"round":1,"turn":0,"leader":"alpha","questionEventId":uuid(3),"answers":[{"questionIndex":0,"answer":"Users pay","planChanges":"Add pricing"}]}),
            ),
            event(
                5,
                "grill.plan",
                json!({"contestId":CONTEST,"leader":"alpha","round":1,"version":2,"text":"Updated plan with pricing"}),
            ),
            event(
                6,
                "grill.complete",
                json!({"contestId":CONTEST,"finalPlanEventId":uuid(5),"finalPlanVersion":2,"appendix":[{"questionEventId":uuid(3),"questionIndex":0,"griller":"beta","question":"Who pays?","answer":"Users pay","planChanges":"Add pricing"}]}),
            ),
        ],
        ..Default::default()
    };
    let text = render(
        &snapshot,
        ExportScope::Grill,
        Some(CONTEST.parse().unwrap()),
    )
    .unwrap();
    for expected in [
        "Plan v1",
        "Plan v2",
        "Original plan",
        "Updated plan with pricing",
        "@beta",
        "Who pays?",
        "Users pay",
        "Add pricing",
        "FINAL PLAN",
    ] {
        assert!(text.contains(expected), "{expected}");
    }
    assert!(!text.contains("winner"));
    assert!(
        render(
            &snapshot,
            ExportScope::Debate,
            Some(CONTEST.parse().unwrap())
        )
        .is_err()
    );
}
