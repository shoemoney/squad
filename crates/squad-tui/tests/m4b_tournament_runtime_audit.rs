#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
#[path = "../../squad-core/tests/room_support.rs"]
mod support;
use ratatui::{Terminal, backend::TestBackend};
use squad_core::room::RoomHandle;
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps, room_adapter, session::Session, ui};
use std::time::Duration;
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

struct Answers;
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
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
            fake_openrouter::structured_judge(
                serde_json::json!({"ranking":data["entries"].as_array().unwrap().iter().enumerate().map(|(i,e)|serde_json::json!({"label":e["label"],"score":10-i,"critique":"A concrete, supported proposal"})).collect::<Vec<_>>()}),
            )
        } else {
            fake_openrouter::streaming("A bounded queue with backpressure.", "stop")
        }
    }
}

#[tokio::test]
async fn fake_provider_tournament_reaches_the_static_tui_and_exports_the_same_champion() {
    let root = tempfile::tempdir().unwrap();
    let mut config = support::config4(root.path());
    config.budget.debate_confirm_usd = 100.;
    let server = support::server().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(Answers)
        .mount(&server)
        .await;
    let room = RoomHandle::open_with_client(
        config,
        "main",
        OpenMode::Interactive,
        support::client(&server),
    )
    .await
    .unwrap();
    let result = room
        .submit(support::message4(
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
            let events = support::page_all(&room, 256).await;
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
    let mut session = Session::default();
    let state = room.state().await;
    let events = support::page_all(&room, 256).await;
    session.restore(room_adapter::snapshot(&state, events));
    assert!(!session.active_contest());
    let (id, view) = session.tournaments.iter().next().unwrap();
    assert_eq!(view.rounds.len(), 3);
    let champion = view.champion.as_ref().unwrap();
    let markdown = room
        .export(ExportScope::Tournament, Some(*id))
        .await
        .unwrap();
    assert!(markdown.contains(champion));
    assert!(markdown.contains("A concrete, supported proposal"));
    for size in [(120, 40), (80, 24)] {
        let mut app = App::new(
            TerminalCaps::resolve(
                |k| (k == "SQUAD_FX").then(|| "off".into()),
                false,
                size.into(),
            )
            .unwrap(),
        );
        app.session.restore(session.snapshot.clone());
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal
            .draw(|f| ui::render(&mut app, f, Duration::ZERO))
            .unwrap();
        assert_eq!(app.fx.active_count(), 0);
        assert!(
            app.session
                .tournaments
                .values()
                .any(|v| v.champion.as_ref() == Some(champion))
        );
        assert!(app.composed.transcript.total_rows() > 0);
    }
    room.shutdown().await.unwrap();
}
