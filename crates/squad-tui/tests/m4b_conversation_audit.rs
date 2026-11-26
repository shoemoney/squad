//! The M5 room fixture uses SQLite and a loopback catalogue, never OpenRouter.
#[path = "../../squad-core/tests/room_support.rs"]
mod support;
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    actions, app::App, fx::TerminalCaps, room_adapter, ui, widgets::preflight::PreflightView,
};
use std::time::Duration;
use support::*;

#[tokio::test]
async fn new_mode_preflights_display_the_actual_m5_plan_and_enforce_one_claim() {
    for (kind, command) in [
        (
            ContestKind::Census,
            "@all $census Design a database service.",
        ),
        (
            ContestKind::Crosscheck,
            "@all $crosscheck Maintain service.\n--\nKeep service available.",
        ),
        (
            ContestKind::Crosscheck,
            "@all $crosscheck Keep service available.",
        ),
        (ContestKind::Survey, "@all $survey Which checks help?"),
        (
            ContestKind::Probe,
            "@all $probe Noise during charging\nRuled out: Fan\nCan inspect: Exterior indicator",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let launcher = Launcher::default();
        let (room, server) = open4(root.path(), &launcher, OpenMode::OneShot).await;
        let result = room.submit(message4(command, EventId::new())).await;
        let RequestResult::PendingConfirmation { preflight, .. } = result else {
            panic!("{kind:?}: {result:?}")
        };
        let PendingPreflight::Contest(plan) = &preflight else {
            panic!()
        };
        assert_eq!(plan.kind, kind);
        let view = PreflightView {
            plan: preflight.clone(),
            pending: false,
        };
        let text = view.text();
        assert!(
            text.contains(&format!("{} calls", plan.estimate.model_calls)),
            "{text}"
        );
        assert!(
            text.contains(&format!("estimate ${:.4}", plan.estimate.estimated_usd)),
            "{text}"
        );
        assert!(
            text.contains(&format!("remaining ${:.4}", plan.estimate.remaining_usd)),
            "{text}"
        );
        match kind {
            ContestKind::Census => assert!(text.contains("passes")),
            ContestKind::Crosscheck => assert!(
                text.contains(
                    if plan.settings["extractionRequired"]
                        .as_bool()
                        .unwrap_or(false)
                    {
                        "extraction call included"
                    } else {
                        "extraction call not needed"
                    }
                ),
                "{text}"
            ),
            ContestKind::Survey => assert!(text.contains("collecting 4 new answers"), "{text}"),
            ContestKind::Probe => {
                assert!(text.contains("4 participants after adjustment"), "{text}")
            }
            _ => unreachable!(),
        }
        let events = page_all(&room, 256).await;
        for size in [(120, 40), (80, 24)] {
            let caps = TerminalCaps::resolve(
                |k| (k == "SQUAD_FX").then(|| "off".into()),
                false,
                size.into(),
            )
            .unwrap();
            let mut app = App::new(caps);
            app.session
                .restore(room_adapter::snapshot(&room.state().await, events.clone()));
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
            terminal
                .draw(|frame| ui::render(&mut app, frame, Duration::ZERO))
                .unwrap();
            let output: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(output.contains("Confirm"), "{kind:?} {output}");
            assert_eq!(app.fx.active_count(), 0);
        }
        let competing = room
            .submit(message4(
                "@all $survey A competing question?",
                EventId::new(),
            ))
            .await;
        assert!(
            matches!(competing, RequestResult::Rejected { .. }),
            "{competing:?}"
        );
        assert!(matches!(
            room.submit(actions::preflight(&preflight, false)).await,
            RequestResult::Accepted { .. }
        ));
        assert!(room.state().await.active_contest.is_none());
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method.as_str() == "GET")
        );
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn survey_existing_answer_preflight_uses_m5_boolean_and_omits_collection_calls() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
    let request = message4("@all Which checks help?", EventId::new());
    let Request::UserMessage { ref input, .. } = request else {
        panic!()
    };
    let question = input.message_id;
    accepted(room.submit(request).await);
    commit_each(
        &launcher,
        (0..4)
            .map(|i| {
                let mut body = generation();
                body["messageId"] = uuid(200 + i).into();
                body["inReplyToMessageId"] = question.to_string().into();
                body["content"] = "Use a repeatable load test.".into();
                (
                    Actor::Member(nick(&format!("m{i}"))),
                    vec![Recipient::Me],
                    payload("message.final", body),
                )
            })
            .collect(),
    );
    let snapshot = room_adapter::snapshot(&room.state().await, page_all(&room, 256).await);
    let meta = parser::SubmissionMeta::default();
    let parsed = parse("@all $survey", &meta, &snapshot.context).unwrap();
    let request =
        Request::from_parsed_with_context(EventId::new(), parsed, &meta, &snapshot.context, false)
            .unwrap();
    let result = room.submit(request).await;
    let RequestResult::PendingConfirmation { preflight, .. } = result else {
        panic!("{result:?}")
    };
    let PendingPreflight::Contest(plan) = &preflight else {
        panic!()
    };
    assert_eq!(plan.settings["collect"], false);
    assert_eq!(plan.estimate.model_calls, 10);
    let text = PreflightView {
        plan: preflight,
        pending: false,
    }
    .text();
    assert!(
        text.contains("10 calls") && text.contains("using existing answers"),
        "{text}"
    );
    room.shutdown().await.unwrap();
}
