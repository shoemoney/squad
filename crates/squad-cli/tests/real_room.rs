//! Bound runtime integration against a loopback provider.
#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::{FakeOpenRouter, MODEL};
use squad_agents::openrouter::OpenRouterClient;
use squad_cli::{output::Output, room_api::RoomApi};
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{collections::BTreeMap, path::Path, time::Duration};

fn config(root: &Path) -> SquadConfig {
    SquadConfig::load_with_env(
        &config::LoadOptions {
            env_file: None,
            data_dir: Some(root.join("data")),
            working_dir: root.to_owned(),
            config_dir: None,
            default_data_dir: root.join("data"),
        },
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "fake-room-key".into()),
            ("SQUAD_MODEL_A".into(), MODEL.into()),
            ("SQUAD_MODEL_B".into(), MODEL.into()),
            ("SQUAD_JUDGE_MODEL".into(), MODEL.into()),
        ]),
    )
    .unwrap()
}
async fn open(config: SquadConfig, mode: OpenMode) -> (RoomHandle, FakeOpenRouter) {
    let server = FakeOpenRouter::start().await;
    server.catalog(fake_openrouter::catalog()).await;
    server
        .completions(vec![
            fake_openrouter::streaming("Fixture answer", "stop"),
            fake_openrouter::streaming("Second answer", "stop"),
        ])
        .await;
    server
        .generation(
            fake_openrouter::GENERATION,
            serde_json::json!({"total_cost":0.000045}),
        )
        .await;
    let client = OpenRouterClient::new(&server.base_url(), "fake-room-key").unwrap();
    let room = RoomHandle::open_with_client(config, "main", mode, client)
        .await
        .unwrap();
    (room, server)
}
fn send(no_wait: bool) -> squad_cli::args::Send {
    squad_cli::args::Send {
        message: "@a Explain the fixture".into(),
        quote: vec![],
        yes: false,
        json: true,
        no_wait,
        verbose: false,
    }
}
#[tokio::test]
async fn send_history_live_and_offline_export() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let (room, server) = open(config.clone(), OpenMode::OneShot).await;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = tokio::time::timeout(
        Duration::from_secs(10),
        squad_cli::send::run_until(
            &room,
            &send(false),
            &mut Output {
                json: true,
                stdout: &mut stdout,
                stderr: &mut stderr,
            },
            || None,
            std::future::pending::<std::io::Result<()>>(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(code, 0);
    assert!(
        String::from_utf8(stdout)
            .unwrap()
            .contains("Fixture answer")
    );
    assert_eq!(server.captured_bodies().await.len(), 1);
    let (room, _server) = open(config.clone(), OpenMode::OneShot).await;
    let events = RoomApi::page_events(&room, None, 500).await.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::UserMessage(_)))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
    );
    let live = room.export(ExportScope::Room, None).await.unwrap();
    let offline =
        squad_core::room::export_offline(&config.data_dir, "main", ExportScope::Room, None)
            .unwrap();
    assert_eq!(live, offline);
    assert!(live.contains("Fixture answer"));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn no_wait_does_not_dispatch_and_interactive_restart_resumes() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let (room, server) = open(config.clone(), OpenMode::OneShot).await;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(
        squad_cli::send::run_until(
            &room,
            &send(true),
            &mut Output {
                json: true,
                stdout: &mut stdout,
                stderr: &mut stderr
            },
            || None,
            std::future::pending::<std::io::Result<()>>()
        )
        .await
        .unwrap(),
        0
    );
    assert!(server.captured_bodies().await.is_empty());
    let (room, server) = open(config, OpenMode::Interactive).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if squad_tui::room_adapter::history(&room)
                .await
                .unwrap()
                .iter()
                .any(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(server.captured_bodies().await.len(), 1);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn real_room_feed_reduces_into_tui_session() {
    use squad_tui::{
        fx::FxQueue,
        session::{RoomPort, Session},
    };
    let root = tempfile::tempdir().unwrap();
    let (room, _server) = open(config(root.path()), OpenMode::OneShot).await;
    let mut feed = RoomPort::subscribe(&room, None).unwrap();
    let mut session = Session::default();
    session.restore(RoomPort::state(&room).await);
    let request_id = EventId::new();
    let meta = parser::SubmissionMeta::default();
    let parsed = parse("@a Explain the fixture", &meta, &session.snapshot.context).unwrap();
    let request = Request::from_parsed(request_id, parsed, &meta, false).unwrap();
    assert!(matches!(
        room.submit(request).await,
        RequestResult::Accepted { .. }
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut queue = FxQueue::new();
        loop {
            let delivery = feed.recv().await.unwrap();
            let done = matches!(&delivery.item, FeedItem::Committed(e) if matches!(e.content, EnvelopePayload::MessageFinal(_)));
            session.reduce(delivery.item, delivery.origin, Duration::ZERO, &mut queue);
            if done { break; }
        }
    }).await.unwrap();
    assert!(
        session
            .transcript
            .cards
            .values()
            .any(|c| c.complete && c.text == "Fixture answer")
    );
    room.shutdown().await.unwrap();
}

// This injected runtime records wakeups without starting provider work. These
// boundary checks isolate commit-only behavior from provider execution.
#[path = "../../squad-core/tests/room_support.rs"]
mod room_support;

#[tokio::test]
async fn commit_only_adapter_persists_without_waking_and_exports_offline() {
    let root = tempfile::tempdir().unwrap();
    let launcher = room_support::Launcher::default();
    let (room, _server) = room_support::open(root.path(), &launcher, OpenMode::OneShot).await;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = squad_cli::send::run_until(
        &room,
        &send(true),
        &mut Output {
            json: true,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
        || None,
        std::future::pending::<std::io::Result<()>>(),
    )
    .await
    .unwrap();
    assert_eq!(code, 0);
    assert!(launcher.wakes.lock().unwrap().is_empty());
    assert!(*launcher.stopped.lock().unwrap());
    let exported = squad_core::room::export_offline(
        &root.path().join("data"),
        "main",
        ExportScope::Room,
        None,
    )
    .unwrap();
    assert!(exported.contains("Explain the fixture"));
    let (room, _server) = room_support::open(root.path(), &launcher, OpenMode::OneShot).await;
    let events = RoomApi::page_events(&room, None, 500).await.unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.content, EnvelopePayload::UserMessage(_)))
            .count(),
        1
    );
    assert_eq!(
        room.export(ExportScope::Room, None).await.unwrap(),
        exported
    );
    let state = RoomApi::state(&room).await;
    let context = <RoomHandle as RoomApi>::parse_context(&state);
    assert_eq!(
        context
            .records
            .iter()
            .filter(|r| r.kind.as_str() == "user.message")
            .count(),
        1
    );
    let request = events
        .iter()
        .find(|e| matches!(e.content, EnvelopePayload::UserMessage(_)))
        .unwrap()
        .base
        .id;
    assert_eq!(
        <RoomHandle as RoomApi>::progress(&state, request),
        squad_cli::room_api::Progress::Running
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn room_handle_feed_replay_and_snapshot_reconcile_in_tui() {
    use squad_tui::{
        fx::{FeedOrigin, FxQueue},
        session::{RoomPort, Session},
    };
    let root = tempfile::tempdir().unwrap();
    let launcher = room_support::Launcher::default();
    let (room, _server) = room_support::open(root.path(), &launcher, OpenMode::OneShot).await;
    let mut feed = RoomPort::subscribe(&room, None).unwrap();
    let mut session = Session::default();
    session.restore(RoomPort::state(&room).await);
    let first = room_support::accepted(room.submit(room_support::set(4)).await);
    let delivery = feed.recv().await.unwrap();
    let mut cues = FxQueue::new();
    session.reduce(delivery.item, delivery.origin, Duration::ZERO, &mut cues);
    assert_eq!(session.snapshot.context.vars[&VariableName::Rounds], 4);
    room_support::accepted(room.submit(room_support::set(5)).await);
    let mut replay = RoomPort::subscribe(&room, Some(first)).unwrap();
    let delivery = replay.recv().await.unwrap();
    assert_eq!(delivery.origin, FeedOrigin::Replay);
    session.reduce(delivery.item, delivery.origin, Duration::ZERO, &mut cues);
    assert_eq!(session.snapshot.context.vars[&VariableName::Rounds], 5);
    let snapshot = RoomPort::state(&room).await;
    assert_eq!(snapshot.context.vars[&VariableName::Rounds], 5);
    assert_eq!(snapshot.context.roster.len(), 2);
    session.restore(snapshot);
    assert_eq!(session.seen.len(), 2);
    room.shutdown().await.unwrap();
}

#[test]
fn doctor_without_a_credential_runs_independent_checks() {
    let root = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_squad"))
        .env_clear()
        .current_dir(root.path())
        .args([
            "--data-dir",
            root.path().join("data").to_str().unwrap(),
            "doctor",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("Credential presence: missing"));
    assert!(text.contains("Data directory:"));
    assert!(text.contains("Accounting:") || text.contains("Daily accounting:"));
    // A missing credential is diagnosed, while the remaining checks still run.
    assert!(!result.status.success());
}

#[tokio::test]
async fn readback_answer_after_restart_waits_for_rewrite_and_fresh_round() {
    let root = tempfile::tempdir().unwrap();
    let mut config = config(root.path());
    let mut third = config.agents[0].clone();
    third.nick = "c".parse().unwrap();
    config.agents.push(third);
    config.budget.daily_max_usd = 200.;
    config.budget.debate_confirm_usd = 100.;
    let server = FakeOpenRouter::start().await;
    let mut catalog = fake_openrouter::catalog();
    catalog["data"][0]["context_length"] = serde_json::json!(1_000_000);
    catalog["data"][0]["top_provider"]["max_completion_tokens"] = serde_json::json!(65_536);
    server.catalog(catalog).await;
    let interpretation = serde_json::json!({"build":"Implement retention","behaviours":[],"edgeCases":[],"assumptions":[]});
    let ledger = serde_json::json!({"entries":[{"entryId":"retention","passage":"The service retains records.","question":"For how long?","readings":[{"readingId":"a","text":"Thirty days","sourceLabels":["Reader 1"]},{"readingId":"b","text":"Forever","sourceLabels":["Reader 2"]}],"behaviourChange":"Records expire or persist"}]});
    let spec = "The service retains records. The operator deletes backups.";
    let rewritten = "The service retains records for thirty days. The operator deletes backups.";
    let rewrite = serde_json::json!({"spec":rewritten,"changes":[{"entryId":"retention","before":"The service retains records.","after":"The service retains records for thirty days."}]});
    let replies = [
        interpretation.clone(),
        interpretation.clone(),
        interpretation.clone(),
        ledger,
        rewrite,
        interpretation.clone(),
        interpretation.clone(),
        interpretation,
        serde_json::json!({"entries":[]}),
    ];
    server
        .completions(
            replies
                .into_iter()
                .map(|v| fake_openrouter::streaming(&v.to_string(), "stop"))
                .collect(),
        )
        .await;
    for (i, message) in [format!("@all $readback {spec}"), "1a".into()]
        .into_iter()
        .enumerate()
    {
        let room = RoomHandle::open_with_client(
            config.clone(),
            "main",
            OpenMode::OneShot,
            OpenRouterClient::new(&server.base_url(), "fake-room-key").unwrap(),
        )
        .await
        .unwrap();
        let mut args = send(false);
        args.message = message;
        args.yes = true;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let exit = tokio::time::timeout(
            Duration::from_secs(15),
            squad_cli::send::run_until(
                &room,
                &args,
                &mut Output {
                    json: true,
                    stdout: &mut stdout,
                    stderr: &mut stderr,
                },
                || None,
                std::future::pending::<std::io::Result<()>>(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            exit,
            0,
            "step {i}: {} {}",
            String::from_utf8_lossy(&stderr),
            String::from_utf8_lossy(&stdout)
        );
        let store = squad_store::Store::open(config.data_dir.join("squad.db")).unwrap();
        if i == 0 {
            assert!(
                store
                    .call(|db| squad_store::contest_steps::pending_human(db))
                    .unwrap()
                    .is_some()
            );
        } else {
            let active = store
                .call(|db| {
                    Ok(
                        db.query_row("SELECT COUNT(*) FROM contests WHERE active=1", [], |r| {
                            r.get::<_, i64>(0)
                        })?,
                    )
                })
                .unwrap();
            assert_eq!(active, 0);
            let contest = store
                .call(|db| {
                    Ok(db.query_row(
                        "SELECT contest_id FROM contests WHERE kind='readback'",
                        [],
                        |r| r.get::<_, String>(0),
                    )?)
                })
                .unwrap()
                .parse()
                .unwrap();
            let report =
                squad_core::export::export(&store, ExportScope::Readback, Some(contest)).unwrap();
            assert!(report.contains(rewritten));
            assert!(report.contains("converged"));
            assert_eq!(server.captured_bodies().await.len(), 9);
        }
        room.shutdown().await.unwrap();
    }
}
