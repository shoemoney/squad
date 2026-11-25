use squad_core::orchestrator::{blind, survey};
use squad_protocol::*;
fn setup() -> SurveySetup {
    let participants = (0..4)
        .map(|i| MemberNick::new(&format!("member{i}")).unwrap())
        .collect::<Vec<_>>();
    let answers = participants
        .iter()
        .map(|n| ContestAnswer {
            nick: n.clone(),
            message_id: MessageId::new(),
            text: "Original human-visible source".into(),
        })
        .collect();
    SurveySetup {
        contest_id: ContestId::new(),
        question_message_id: MessageId::new(),
        question: Text::new("Question").unwrap(),
        participants,
        answers,
        collect: false,
    }
}
fn vote(s: &SurveySetup, n: usize, ids: &[&str], choice: SurveyChoice) -> SurveyVote {
    SurveyVote {
        contest_id: s.contest_id,
        step_id: StepId::new(),
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                work_id: WorkId::new(),
                message_id: MessageId::new(),
                trigger_event_id: EventId::new(),
                attempt: 1,
            },
            model: Text::new("fixture/model").unwrap(),
            finish_reason: FinishReason::Stop,
            usage: None,
        },
        voter: s.participants[n].clone(),
        ballot: SurveyBallot {
            votes: ids
                .iter()
                .map(|id| SurveyBallotItem {
                    claim_id: (*id).into(),
                    vote: choice,
                    reason: format!("member{n} supports the claim"),
                    confidence: 3,
                    change_evidence: None,
                })
                .collect(),
        },
    }
}
#[test]
fn application_map_preserves_cross_references_missing_votes_and_observed_switches() {
    let s = setup();
    let claims = SurveyExtraction {
        claims: vec![SurveyClaim {
            id: "c1".into(),
            claim: "A claim".into(),
            source_answer_ids: vec![s.answers[0].message_id],
        }],
    };
    let mut first = (0..4)
        .map(|i| vote(&s, i, &["c1"], SurveyChoice::Agree))
        .collect::<Vec<_>>();
    first[0].ballot.votes[0].vote = SurveyChoice::Disagree;
    let m = survey::map(&s, &claims, &[], &first, &[]);
    assert!(m[0].bands.contains(&SurveyBand::Consensus));
    assert!(m[0].bands.contains(&SurveyBand::LoneInsights));
    assert!(m[0].bands.contains(&SurveyBand::LoneDissents));
    assert!(m[0].bands.contains(&SurveyBand::Contested));
    let second = vec![vote(&s, 0, &["c1"], SurveyChoice::Agree)];
    let m = survey::map(&s, &claims, &["c1".into()], &first, &second);
    assert_eq!(m[0].switches.len(), 1);
    assert_eq!(m[0].final_tally.agree, 4);
    assert_eq!(m[0].not_revoted.len(), 3);
    first.remove(3);
    let m = survey::map(&s, &claims, &[], &first, &[]);
    assert_eq!(m[0].final_tally.missing, 1);
    assert!(!m[0].bands.contains(&SurveyBand::Consensus));
    let first = (0..4)
        .map(|i| {
            vote(
                &s,
                i,
                &["c1"],
                if i == 0 {
                    SurveyChoice::Agree
                } else {
                    SurveyChoice::Disagree
                },
            )
        })
        .collect::<Vec<_>>();
    let m = survey::map(&s, &claims, &[], &first, &[]);
    assert!(m[0].bands.contains(&SurveyBand::RejectedOutliers));
}
#[test]
fn selection_orders_rational_split_decisive_count_then_id_and_deduplicates_reasons() {
    let s = setup();
    let claims = SurveyExtraction {
        claims: ["b", "a", "undecided", "unanimous"]
            .iter()
            .map(|id| SurveyClaim {
                id: (*id).into(),
                claim: format!("claim {id}"),
                source_answer_ids: vec![s.answers[0].message_id],
            })
            .collect(),
    };
    let mut votes = (0..4)
        .map(|i| {
            vote(
                &s,
                i,
                &["a", "b", "undecided", "unanimous"],
                if i < 2 {
                    SurveyChoice::Agree
                } else {
                    SurveyChoice::Disagree
                },
            )
        })
        .collect::<Vec<_>>();
    for v in &mut votes {
        v.ballot.votes[2].vote = SurveyChoice::Unsure;
        v.ballot.votes[3].vote = SurveyChoice::Agree;
        for i in &mut v.ballot.votes {
            i.reason = "Shared reason".into();
        }
    }
    assert_eq!(
        survey::select(&claims, &s.participants, &votes),
        ["a", "b", "unanimous", "undecided"]
    );
    let p = survey::selection(&s, StepId::new(), &claims, &votes);
    assert_eq!(p.claims[0].agree_reasons, vec!["Shared reason"]);
    assert_eq!(p.claims[0].disagree_reasons, vec!["Shared reason"]);
    let raw = serde_json::to_string(&p).unwrap();
    assert!(!raw.contains("voter"));
    assert!(!raw.contains("confidence"));
    assert!(!raw.contains("sourceAnswer"));
}
#[test]
fn redaction_matches_unicode_boundaries_longer_phrases_and_preserves_original() {
    let original = "MEMBER0 uses Anthropic/Claude-Opus. A thoughtful persona agrees. member01 and émember0 stay.";
    let tokens = vec![
        "member0".into(),
        "Anthropic/Claude-Opus".into(),
        "Anthropic".into(),
        "Claude".into(),
        "Opus".into(),
        "A thoughtful persona".into(),
    ];
    assert_eq!(
        blind::redact(original, &tokens),
        "[redacted] uses [redacted]. [redacted] agrees. member01 and émember0 stay."
    );
    assert!(original.contains("MEMBER0"));
    assert_eq!(
        blind::redact("member member", &["member".into()]),
        "[redacted] [redacted]"
    );
}
#[test]
fn report_may_not_invent_band_assignments_or_drop_contested_settling_questions() {
    let s = setup();
    let c = SurveyExtraction {
        claims: vec![SurveyClaim {
            id: "c1".into(),
            claim: "Claim".into(),
            source_answer_ids: vec![s.answers[0].message_id],
        }],
    };
    let votes = (0..4)
        .map(|i| vote(&s, i, &["c1"], SurveyChoice::Unsure))
        .collect::<Vec<_>>();
    let map = survey::map(&s, &c, &[], &votes, &[]);
    assert!(map[0].bands.is_empty());
    assert_eq!(map[0].final_tally.unsure, 4);
    let mut report = SurveyReportText {
        consensus: vec![],
        contested: vec![],
        lone_insights: vec![],
        lone_dissents: vec![],
        rejected_outliers: vec![],
        limitations: "Shared blind spots".into(),
    };
    report.validate(&map).unwrap();
    report.consensus.push(SurveyReportEntry {
        claim_id: "c1".into(),
        explanation: "Made up".into(),
        settling_question: String::new(),
    });
    assert!(report.validate(&map).is_err());
}
#[test]
fn nested_blind_projections_redact_identity_prose_without_changing_claim_ids() {
    let tokens = vec![
        "member0".into(),
        "Anthropic".into(),
        "Claude".into(),
        "Thoughtful persona".into(),
    ];
    let v = serde_json::json!({"question":"MEMBER0 asks", "claims":[{"claimId":"member0-id","claim":"CLAUDE says this.","agreeReasons":["ANTHROPIC and Thoughtful persona agree."]}]});
    let b = blind::value(v.clone(), &tokens);
    assert_eq!(b["claims"][0]["claimId"], "member0-id");
    assert_eq!(b["claims"][0]["claim"], "[redacted] says this.");
    assert!(
        !b["claims"][0]["agreeReasons"]
            .to_string()
            .contains("ANTHROPIC")
    );
    assert_eq!(v["question"], "MEMBER0 asks");
}

#[test]
fn completeness_tracks_each_stage_even_if_later_votes_fill_the_map() {
    let mut s = setup();
    let claims = SurveyExtraction {
        claims: vec![SurveyClaim {
            id: "c1".into(),
            claim: "A claim".into(),
            source_answer_ids: vec![s.answers[0].message_id],
        }],
    };
    let all = (0..4)
        .map(|i| vote(&s, i, &["c1"], SurveyChoice::Agree))
        .collect::<Vec<_>>();
    let complete = survey::completeness(&s, &claims, &["c1".into()], &all, &all);
    assert!(complete.complete && complete.is_complete());
    s.answers.pop();
    let c = survey::completeness(&s, &claims, &["c1".into()], &all[..3], &all);
    assert!(!c.complete);
    assert_eq!(c.missing_answers, vec![s.participants[3].clone()]);
    assert_eq!(c.missing_votes, vec![s.participants[3].clone()]);
    assert!(c.missing_revotes.is_empty());
    // No extracted claims means no required first or second ballot calls.
    let c = survey::completeness(
        &setup(),
        &SurveyExtraction { claims: vec![] },
        &[],
        &[],
        &[],
    );
    assert!(c.complete);
}
