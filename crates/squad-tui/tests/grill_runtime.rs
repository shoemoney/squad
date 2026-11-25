#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use ratatui::{buffer::Buffer, layout::Rect};
use squad_core::room::RoomHandle;
use squad_protocol::*;
use squad_tui::{
    actions,
    fx::{ColorTier, FeedOrigin, FxQueue, Palette},
    room_adapter::snapshot,
    session::Session,
    widgets::grill::render,
};
use std::{collections::BTreeMap, time::Duration};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};
struct Answers;
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"].as_array().unwrap().last().unwrap()["content"]
            .as_str()
            .unwrap();
        if user.contains("Return exactly PASS") {
            streaming(r#"{"questions":["How is overload handled?"]}"#, "stop")
        } else if user.contains("Return a JSON object with only answers") {
            streaming(
                r#"{"answers":[{"questionIndex":0,"answer":"Use backpressure.","planChanges":"Bound the queue."}]}"#,
                "stop",
            )
        } else if user.contains("Return only the complete standalone revised plan") {
            streaming("Use a bounded queue and backpressure.", "stop")
        } else {
            streaming("Initial plan: one writer.", "stop")
        }
    }
}
async fn events(room: &RoomHandle) -> Vec<CommittedEnvelope> {
    room.page_events(None, 500).await.unwrap()
}
async fn wait_for(room: &RoomHandle, kind: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if events(room)
                .await
                .iter()
                .any(|e| serde_json::to_value(&e.content).unwrap()["kind"] == kind)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
async fn send(room: &RoomHandle, text: &str) -> RequestResult {
    let snap = snapshot(&room.state().await, events(room).await);
    let meta = parser::SubmissionMeta::default();
    let parsed = parse(text, &meta, &snap.context).unwrap();
    room.submit(
        Request::from_parsed_with_context(EventId::new(), parsed, &meta, &snap.context, false)
            .unwrap(),
    )
    .await
}
#[tokio::test]
async fn real_runtime_preflight_actions_committed_snapshots_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.path().join("data")),
        working_dir: dir.path().into(),
        config_dir: None,
        default_data_dir: dir.path().join("data"),
    };
    let mut config = SquadConfig::load_with_env(
        &options,
        &BTreeMap::from([("OPENROUTER_API_KEY".into(), "fake-key".into())]),
    )
    .unwrap();
    config.agents = (0..2)
        .map(|i| config::AgentConfig {
            nick: if i == 0 { "alpha" } else { "beta" }.parse().unwrap(),
            model: format!("fixture/member{i}"),
            persona: format!("PERSONA_{i}"),
            reasoning_effort: None,
        })
        .collect();
    config.judge.model = "fixture/judge".into();
    config.budget.daily_max_usd = 20.;
    config.budget.debate_confirm_usd = 0.;
    let fake = FakeOpenRouter::start().await;
    let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
    fake.catalog(serde_json::json!({"data":models})).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers)
        .mount(&fake.server)
        .await;
    let room = RoomHandle::open_with_client(
        config.clone(),
        "main",
        OpenMode::Interactive,
        squad_agents::openrouter::OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap(),
    )
    .await
    .unwrap();
    let leader = &config.agents[0].nick;
    assert!(matches!(
        send(&room, &format!("@{leader} Design a plan")).await,
        RequestResult::Accepted { .. }
    ));
    wait_for(&room, "message.final").await;
    let pending = send(&room, &format!("@{leader} $grill $GRILL_ROUNDS=1")).await;
    let RequestResult::PendingConfirmation { preflight, .. } = pending else {
        panic!("{pending:?}")
    };
    let mut session = Session::default();
    session.restore(snapshot(&room.state().await, events(&room).await));
    assert!(session.active_contest());
    assert_eq!(session.snapshot.preflights.len(), 1);
    assert!(matches!(
        room.submit(actions::preflight(&preflight, true)).await,
        RequestResult::Accepted { .. }
    ));
    wait_for(&room, "grill.complete").await;
    let all = events(&room).await;
    let mut live = Session::default();
    let mut cues = FxQueue::new();
    for event in &all {
        live.reduce(
            FeedItem::Committed(event.clone()),
            FeedOrigin::Live,
            Duration::ZERO,
            &mut cues,
        );
    }
    let count = cues.len();
    for event in &all {
        live.reduce(
            FeedItem::Committed(event.clone()),
            FeedOrigin::Replay,
            Duration::ZERO,
            &mut cues,
        );
    }
    assert_eq!(count, cues.len());
    assert!(!live.active_contest());
    assert!(live.snapshot.preflights.is_empty());
    let (id, view) = live.grills.iter().next().unwrap();
    assert_eq!(view.state, "COMPLETE");
    assert_eq!(view.plans.len(), 2);
    for width in [120, 80, 40] {
        let area = Rect::new(0, 0, width, 30);
        let mut buf = Buffer::empty(area);
        render(view, area, &mut buf, &Palette::new(ColorTier::TrueColor));
        let text = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(format!("runtime_grill_{width}"), text);
    }
    let mut restored = Session::default();
    restored.restore(snapshot(&room.state().await, all));
    assert_eq!(restored.grills[id], *view);
    let markdown = room.export(ExportScope::Grill, Some(*id)).await.unwrap();
    let mut saved = vec![];
    actions::save_export(&mut saved, &markdown).unwrap();
    assert!(
        String::from_utf8(saved)
            .unwrap()
            .contains("Bound the queue")
    );
    room.shutdown().await.unwrap();
}
