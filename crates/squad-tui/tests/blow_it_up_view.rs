use squad_protocol::*;
use squad_tui::widgets::blow_it_up::*;
#[test]
fn banner_dividers_and_terminal_state_replay_are_stable() {
    let id = ContestId::new();
    let state = BlowItUpState {
        contest_id: id,
        state_version: 1,
        state: BlowItUpStatus::Stopped,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: Some("stopped".parse().unwrap()),
    };
    let mut view = View::default();
    let event = EventId::new();
    view.apply(event, &EnvelopePayload::BlowItUpState(state.clone()));
    view.apply(event, &EnvelopePayload::BlowItUpState(state));
    let lines = render(&view).join("\n");
    for expected in [
        "BLOW IT UP",
        "ATTACKS",
        "CHALLENGE",
        "WHAT SURVIVED",
        "Stopped",
    ] {
        assert!(lines.contains(expected));
    }
    assert!(!view.killed_expanded);
    view.toggle_killed();
    assert!(view.killed_expanded);
}
#[test]
fn severity_palette_is_distinct() {
    use squad_protocol::blow_it_up::Severity::*;
    let colors = [Critical, High, Medium, Low].map(severity_color);
    for i in 0..4 {
        for j in 0..i {
            assert_ne!(colors[i], colors[j]);
        }
    }
}

#[test]
fn enabled_mode_appears_in_typeahead_and_help() {
    let ctx = parser::ParseContext {
        roster: ["alpha", "beta", "gamma"]
            .map(|n| n.parse().unwrap())
            .to_vec(),
        ..Default::default()
    };
    assert!(squad_tui::typeahead::enabled("$blow_it_up"));
    assert!(
        squad_tui::typeahead::examples(&ctx)
            .iter()
            .any(|s| s.contains("$blow_it_up"))
    );
    let raw = "@all $bl";
    let c = squad_tui::widgets::completion::CompletionState::at(
        raw,
        raw.len(),
        &ctx,
        &Default::default(),
    );
    assert!(c.items.iter().any(|i| i == "$blow_it_up"));
    let example = squad_protocol::parser::help_examples()
        .into_iter()
        .find(|s| s.contains("$blow_it_up"))
        .unwrap();
    assert!(parse(example, &Default::default(), &ctx).is_ok());
}

#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_tournament_provider;
#[path = "../../squad-core/tests/room_support.rs"]
mod legacy_room_support;
struct LegacyTournamentAnswers;
impl wiremock::Respond for LegacyTournamentAnswers {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let system = body["messages"][0]["content"].as_str().unwrap();
        if system.contains("summoned judge") {
            let user = body["messages"][1]["content"].as_str().unwrap();
            let data = user
                .split("Tournament prompt and anonymized answers, supplied as evidence:\n")
                .nth(1)
                .unwrap()
                .trim_start();
            let data = serde_json::Deserializer::from_str(data)
                .into_iter::<serde_json::Value>()
                .next()
                .unwrap()
                .unwrap();
            fake_tournament_provider::structured_judge(serde_json::json!({
                "ranking":data["entries"].as_array().unwrap().iter().enumerate().map(|(i,e)|serde_json::json!({"label":e["label"],"score":10-i,"critique":"A concrete, supported proposal"})).collect::<Vec<_>>()
            }))
        } else {
            fake_tournament_provider::streaming("A bounded queue with backpressure.", "stop")
        }
    }
}

/// An enabled mode must preserve the legacy tournament. Page after observing the
/// terminal state, so a previously read event page cannot omit the final champion.
#[tokio::test]
async fn enabled_mode_preserves_tournament_champion_with_a_fresh_terminal_snapshot() {
    use std::time::Duration;
    use wiremock::{
        Mock,
        matchers::{method, path},
    };
    let root = tempfile::tempdir().unwrap();
    let mut config = legacy_room_support::config4(root.path());
    config.budget.debate_confirm_usd = 100.;
    let server = legacy_room_support::server().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(LegacyTournamentAnswers)
        .mount(&server)
        .await;
    let room = squad_core::room::RoomHandle::open_with_client(
        config,
        "main",
        OpenMode::Interactive,
        legacy_room_support::client(&server),
    )
    .await
    .unwrap();
    let result = room
        .submit(legacy_room_support::message4(
            "@all $tournament Design a reliable queue.",
            EventId::new(),
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let events = legacy_room_support::page_all(&room, 256).await;
            if events
                .iter()
                .any(|e| matches!(e.content, EnvelopePayload::TournamentResult(_)))
                && room.state().await.active_contest.is_none()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let state = room.state().await;
    let events = legacy_room_support::page_all(&room, 256).await;
    let mut session = squad_tui::session::Session::default();
    session.restore(squad_tui::room_adapter::snapshot(&state, events.clone()));
    let (id, view) = session.tournaments.iter().next().unwrap();
    assert!(
        view.champion.is_some(),
        "Canonical tournament evidence: {:?}",
        events
            .iter()
            .filter(|e| matches!(
                e.content,
                EnvelopePayload::TournamentState(_)
                    | EnvelopePayload::TournamentResult(_)
                    | EnvelopePayload::TournamentChampion(_)
            ))
            .map(|e| &e.content)
            .collect::<Vec<_>>()
    );
    let export = room
        .export(ExportScope::Tournament, Some(*id))
        .await
        .unwrap();
    assert!(export.contains(view.champion.as_ref().unwrap()));
    room.shutdown().await.unwrap();
}

#[test]
fn report_replay_preserves_severity_and_collapses_killed_evidence() {
    use serde_json::json;
    use squad_protocol::blow_it_up::{BlowItUpReport, Severity};
    let report: BlowItUpReport = serde_json::from_value(json!({
        "findings": [{
            "finding": {
                "findingId": "f1", "title": "Unrestricted access",
                "sourceVectorIds": ["v1"], "severity": "critical",
                "likelihood": "likely", "exploit": "Anyone reads stored data.",
                "blastRadius": "All data", "mitigation": "Authorize each read.",
                "rebuttalStatus": "survived", "residual": "Credential theft",
                "challengeRejection": null
            },
            "score": 12, "finderCount": 1, "lone": true, "addition": false
        }],
        "killed": [{
            "vector": {
                "vectorId": "v2", "title": "Unprotected writes",
                "targetQuote": "All writes require authentication.",
                "exploit": "Anyone writes data.", "severity": "high",
                "likelihood": "possible", "blastRadius": "All data", "addition": false
            },
            "tally": {"vectorId": "v2", "accepted": 3, "falsePositive": 2,
                      "weak": 0, "status": "killed"},
            "defeatingQuotes": ["All writes require authentication."],
            "judgeNote": "Check authorization separately."
        }],
        "additions": [], "unchallenged": [], "skipped": []
    }))
    .unwrap();
    let artifact = ContestArtifact {
        kind: Some(ContestKind::BlowItUp),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::to_value(report).unwrap()),
        contest_id: ContestId::new(),
        step_id: StepId::new(),
        artifact_kind: "blow_it_up.report".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: json!({}),
    };
    let contest_id = artifact.contest_id;
    let mut view = View::default();
    let event = EventId::new();
    view.apply(event, &EnvelopePayload::ContestArtifact(artifact.clone()));
    view.apply(event, &EnvelopePayload::ContestArtifact(artifact));
    assert_eq!(view.artifacts.len(), 1);
    let lines = styled_lines(&view);
    let finding = lines
        .iter()
        .find(|l| l.to_string().contains("### Critical"))
        .unwrap();
    assert_eq!(finding.style.fg, Some(severity_color(Severity::Critical)));
    let collapsed = render(&view).join("\n");
    assert!(collapsed.contains("Killed appendix · 1 vectors (collapsed)"));
    assert!(!collapsed.contains("Defeating quote:"));
    view.toggle_killed();
    let expanded = render(&view).join("\n");
    assert!(expanded.contains("Defeating quote: All writes require authentication."));
    assert!(expanded.contains("false_positive 2/3"));
    assert!(expanded.contains("Judge note: Check authorization separately."));
    view.toggle_killed();
    assert_eq!(render(&view).join("\n"), collapsed);

    // Exercise the shared transcript path and its F7 details control too.
    let state = BlowItUpState {
        contest_id,
        state_version: 9,
        state: BlowItUpStatus::Complete,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![event],
        terminal_reason: Some("complete".parse().unwrap()),
    };
    view.state = Some(state.clone());
    let mut session = squad_tui::session::Session::default();
    session.contests.insert(
        contest_id,
        squad_tui::session::ContestView {
            state: Some(ContestState::BlowItUp(state)),
            blow_it_up: view,
            ..Default::default()
        },
    );
    let mut composed = squad_tui::composed::Composed::default();
    let palette = squad_tui::fx::Palette::new(squad_tui::fx::ColorTier::TrueColor);
    composed.prepare(&session, 40, &palette);
    let message_id: MessageId = contest_id.to_string().parse().unwrap();
    let card = composed.transcript.message(message_id).unwrap();
    assert!(
        card.widget_rows
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.style.fg == Some(severity_color(Severity::Critical)))
    );
    assert!(card.widget_rows.iter().all(|l| l.width() <= 40));
    assert!(
        !card
            .widget_rows
            .iter()
            .any(|l| l.to_string().contains("Defeating quote:"))
    );
    composed.toggle_verdict();
    composed.prepare(&session, 40, &palette);
    let card = composed.transcript.message(message_id).unwrap();
    assert!(
        card.widget_rows
            .iter()
            .any(|l| l.to_string().contains("Defeating quote:"))
    );
}
