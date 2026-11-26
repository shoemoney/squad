#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use squad_tui::{
    actions,
    fx::{FeedOrigin, FxQueue},
    room_adapter::snapshot,
    session::Session,
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
        if body["model"] == "fixture/judge" {
            if body["messages"].to_string().contains("Score round ") {
                tied_round_score("alpha", "beta")
            } else {
                let scores = ["alpha", "beta"].map(|nick| serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported"})))}));
                structured_judge(
                    serde_json::json!({"winner":"alpha","scores":scores,"reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}),
                )
            }
        } else {
            streaming("Use a bounded queue and backpressure.", "stop")
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
async fn question_first_chat_pick_preflight_debate_verdict_and_restart() {
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
    let mut app = squad_tui::app::App::new(
        squad_tui::fx::TerminalCaps::resolve(
            |key| (key == "SQUAD_FX").then(|| "off".into()),
            true,
            (120, 40).into(),
        )
        .unwrap(),
    );
    let result = send(&room, "@all $debate $ROUNDS=1 Compare queue designs").await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    wait_for(&room, "contest.pick_requested").await;
    app.session
        .restore(snapshot(&room.state().await, events(&room).await));
    assert!(app.session.pick.is_some());
    assert_eq!(
        app.session
            .transcript
            .cards
            .values()
            .filter(|c| c.complete)
            .count(),
        3
    );
    app.input
        .apply(squad_tui::widgets::input::EditorAction::Insert(
            "@alpha @beta".into(),
        ))
        .unwrap();
    app.submit();
    let request = app.pending.take().unwrap();
    assert!(matches!(request, Request::ContestPick { .. }));
    let pending = room.submit(request).await;
    let RequestResult::PendingConfirmation { ref preflight, .. } = pending else {
        panic!("{pending:?}")
    };
    let plan = preflight.clone();
    app.acknowledge(pending);
    assert!(app.input.text().contains("@alpha"));
    let view = squad_tui::widgets::preflight::PreflightView {
        plan: plan.clone(),
        pending: false,
    };
    assert!(view.text().contains("1 round-scoring judge calls"));
    assert_eq!(view.model_calls(), 4);
    assert!(matches!(
        room.submit(actions::preflight(&plan, true)).await,
        RequestResult::Accepted { .. }
    ));
    wait_for(&room, "verdict").await;
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
    assert!(live.pick.is_none());
    assert_eq!(live.cases.len(), 1);
    let debate = live.debates.values().next().unwrap();
    assert_eq!(debate.state.state, DebateStatus::Completed);
    assert_eq!(
        debate
            .rounds
            .iter()
            .filter(|r| matches!(r.scoring, squad_tui::widgets::debate::Scoring::Accepted(_)))
            .count(),
        1
    );
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
    let id = debate_id(&live);
    let markdown = room.export(ExportScope::Debate, Some(id)).await.unwrap();
    assert!(markdown.contains("Tie breaker"));
    let mut restored = Session::default();
    restored.restore(snapshot(&room.state().await, all));
    assert_eq!(restored.debates[&id].state, live.debates[&id].state);
    assert_eq!(restored.cases.len(), 1);
    assert!(restored.pick.is_none());
    room.shutdown().await.unwrap();
}
fn debate_id(session: &Session) -> ContestId {
    *session.debates.keys().next().unwrap()
}
