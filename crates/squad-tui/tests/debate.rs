use squad_core::orchestrator::{preflight, types::*};
use squad_protocol::*;
use squad_tui::{actions, widgets::preflight::PreflightView};

#[path = "../../squad-core/tests/room_support.rs"]
mod support;

fn plan() -> DebatePlan {
    let members = ["alpha", "beta"].map(|n| FrozenModel {
        nick: Some(n.parse().unwrap()),
        resolved_slug: format!("fixture/{n}"),
        context_tokens: 1_000_000,
        input_usd_per_million: 1.,
        output_usd_per_million: 2.,
        reasoning_tokens: 0,
        reasoning_effort: None,
    });
    DebatePlan {
        preflight: DebatePreflight {
            judge_only: false,
            debate_id: DebateId::new(),
            plan_id: PlanId::new(),
            expires_at: "2026-11-26T01:00:00Z".parse().unwrap(),
            estimate: DebateEstimate {
                estimated_usd: 0.,
                reserved_usd: 0.,
                remaining_usd: 20.,
                model_calls: 9,
            },
            judge_model: Some("fixture/judge".into()),
            warning: None,
            confirmation_required: false,
            debaters: ["alpha".parse().unwrap(), "beta".parse().unwrap()],
            rounds: 2,
            judge_requested: true,
            opening_round_required: true,
        },
        sources: FrozenSources {
            topic_message_id: MessageId::new(),
            topic: "Compare plans".into(),
            instruction: String::new(),
            solutions: None,
            cutoff: 0,
        },
        judge: Some(FrozenModel {
            nick: None,
            resolved_slug: "fixture/judge".into(),
            ..members[0].clone()
        }),
        members,
        prompts: FrozenPrompts {
            pack_version: "fixture".into(),
            composition_hashes: Default::default(),
            turn_words: 200,
            final_turn_words: 400,
        },
        script_version: "debate-v1".into(),
        calls: vec![],
        prior_terminal: None,
    }
}

#[test]
fn core_threshold_boundaries_and_ui_confirmation_cancel_and_expiry() {
    let now = "2026-11-26T00:00:00Z".parse().unwrap();
    let freeze = |p: &mut DebatePlan, threshold| {
        preflight::freeze(
            p,
            preflight::ContextBounds {
                members: [100, 100],
                judge: 100,
            },
            20.,
            threshold,
            now,
        )
        .unwrap()
    };
    let mut p = plan();
    freeze(&mut p, 20.);
    let estimate = p.preflight.estimate.estimated_usd;
    for (threshold, automatic) in [
        (estimate - f64::EPSILON, false),
        (estimate, true),
        (estimate + f64::EPSILON, true),
    ] {
        let outcome = freeze(&mut p, threshold);
        assert_eq!(outcome == Some(Confirmation::Automatic), automatic);
        let view = PreflightView {
            pending: false,
            plan: PendingPreflight::Debate(p.preflight.clone()),
        };
        assert_eq!(view.confirmation_required(), !automatic);
        assert!(view.text().contains("9 calls"));
        assert!(view.text().contains("opening true"));
        assert!(view.text().ends_with(if automatic {
            "Core auto-confirmed"
        } else {
            "Y Confirm · N Cancel"
        }));
        let yes = actions::preflight(&view.plan, true);
        assert_eq!(
            preflight::confirmation(&p, Some(&yes), false, now, true).unwrap(),
            Some(Confirmation::Explicit)
        );
        assert_eq!(
            preflight::confirmation(&p, Some(&yes), false, p.preflight.expires_at, true),
            Err(ErrorCode::DEBATE_PLAN_EXPIRED)
        );
        assert_eq!(
            preflight::cancellation(&p, &actions::preflight(&view.plan, false)).unwrap(),
            DebateTransition::Cancel
        );
    }
}

#[test]
fn manual_judging_uses_only_the_named_debate() {
    let id = DebateId::new();
    assert!(
        matches!(squad_tui::session::judge_request(Some(id)), Request::Judge { debate_id: Some(actual), .. } if actual == id)
    );
    assert!(matches!(
        squad_tui::session::judge_request(None),
        Request::Judge {
            debate_id: None,
            ..
        }
    ));
}

#[tokio::test]
async fn equal_and_late_scores_preserve_authoritative_tally_records_and_anchor() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    use squad_tui::{
        app::App,
        fx::{FeedOrigin, FxQueue, TerminalCaps},
        ui,
        widgets::transcript::TranscriptAnchor,
    };
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let launcher = support::Launcher::default();
    let (room, _) = support::open(root.path(), &launcher, OpenMode::OneShot).await;
    let events = support::commit_each(
        &launcher,
        vec![support::debate_state("ROUND_RUNNING", 3, 1)],
    );
    let mut app = App::new(
        TerminalCaps::resolve(
            |k| (k == "SQUAD_FX").then(|| "off".into()),
            false,
            (120, 40).into(),
        )
        .unwrap(),
    );
    let mut queue = FxQueue::new();
    app.session.reduce(
        FeedItem::Committed(events[0].clone()),
        FeedOrigin::Replay,
        Duration::ZERO,
        &mut queue,
    );
    app.session.snapshot.scoreboard = vec![Score {
        nick: "a".parse().unwrap(),
        wins: 4,
        losses: 2,
        trophies: 3,
    }];
    let before = app.session.snapshot.scoreboard.clone();
    let id: DebateId = support::CONTEST.parse().unwrap();
    let score = support::judge(
        "debate.round_score",
        serde_json::json!({
            "debateId":support::CONTEST,"caseId":support::CASE,"round":2,
            "scores":{"a":{"score":8,"reason":"Equal argument; precise rollback evidence"},"b":{"score":8,"reason":"Equal argument; explicit checks"}},
            "tally":{"a":17,"b":19}
        }),
    );
    let events = support::commit_each(&launcher, vec![score]);
    let event = events[0].clone();
    app.session.reduce(
        FeedItem::Committed(event.clone()),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut queue,
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|f| ui::render(&mut app, f, Duration::ZERO))
        .unwrap();
    app.composed.anchor = Some(TranscriptAnchor {
        id: id.to_string().parse().unwrap(),
        source_grapheme: 0,
        row_in_header: Some(0),
    });
    app.session.transcript.follow = false;
    let anchor = app.composed.anchor;
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE)),
        Duration::ZERO,
    );
    terminal
        .draw(|f| ui::render(&mut app, f, Duration::ZERO))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("precise rollback evidence"), "{text}");
    assert_eq!(app.composed.anchor, anchor);
    assert!(!app.session.reduce(
        FeedItem::Committed(event),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut queue
    ));
    let tally = app.session.debates[&id].tally.as_ref().unwrap();
    assert_eq!(tally[&"a".parse().unwrap()], 17);
    assert_eq!(tally[&"b".parse().unwrap()], 19);
    assert_eq!(app.session.snapshot.scoreboard, before);
    room.shutdown().await.unwrap();
}
