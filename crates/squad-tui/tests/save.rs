use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use squad_protocol::{parser::RecordSummary, *};
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    save::{self, SaveJob},
    session::{RoomError, RoomFeed, RoomFuture, RoomPort, RoomSnapshot},
};
use std::{path::PathBuf, time::Duration};

const FORGE_EXPORT: &str = "# FORGE\n\nFusion by @alpha beat the best draft.\n\nThe winning artifact body.\nSecond line.\n\n## Borda\n\n| @alpha | 9 |\n\n## Provenance\n\nDraft A by @hidden-provider\n";

struct Stub(String);
impl RoomPort for Stub {
    fn submit(&self, _: Request) -> RoomFuture<'_, RequestResult> {
        unimplemented!()
    }
    fn subscribe(&self, _: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        unimplemented!()
    }
    fn page_events(
        &self,
        _: Option<Cursor>,
        _: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        unimplemented!()
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        unimplemented!()
    }
    fn export(
        &self,
        _: ExportScope,
        _: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        let text = self.0.clone();
        Box::pin(async move { Ok(text) })
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        unimplemented!()
    }
}
fn app() -> App {
    App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap())
}
fn record(seq: i64, author: Actor, text: &str, contest: Option<ContestId>) -> RecordSummary {
    RecordSummary {
        event_id: EventId::new(),
        message_id: Some(MessageId::new()),
        seq: Cursor::new(seq).unwrap(),
        kind: Text::new("message.final").unwrap(),
        author,
        recipients: vec![],
        content: text.into(),
        in_reply_to_message_id: None,
        contest_id: contest,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    }
}
fn finished_forge(app: &mut App) -> ContestId {
    let id = ContestId::new();
    app.session.contests.entry(id).or_default().state =
        Some(ContestState::Generic(GenericContestState {
            contest_id: id,
            kind: ContestKind::Forge,
            state_version: 3,
            status: Text::new("COMPLETE").unwrap(),
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
        }));
    app.session.snapshot.context.records = vec![
        record(1, Actor::Me, "Write a launch plan for Squad!", Some(id)),
        record(
            2,
            Actor::Member(Nick::new("alpha").unwrap()),
            "a draft",
            None,
        ),
    ];
    id
}
fn job(app: &App, path: Option<&str>, cwd: PathBuf) -> SaveJob {
    save::plan(
        &app.session,
        None,
        path.map(str::to_owned),
        EventId::new(),
        cwd,
    )
    .unwrap()
}
fn saved(result: RequestResult) -> String {
    match result {
        RequestResult::Internal(InternalResult::Text { text }) => text,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn forge_winner_is_saved_alone_with_default_name_and_0600() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let job = job(&app, None, dir.path().into());
    let text = saved(save::save_file(&Stub(FORGE_EXPORT.into()), job).await);
    assert_eq!(text, "Saved forge-write-a-launch-plan-for-squad.md");
    let path = dir.path().join("forge-write-a-launch-plan-for-squad.md");
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body, "The winning artifact body.\nSecond line.\n");
    assert!(!body.contains("@alpha") && !body.contains("hidden-provider"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[tokio::test]
async fn saving_twice_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let room = Stub(FORGE_EXPORT.into());
    for expected in [
        "Saved forge-write-a-launch-plan-for-squad.md",
        "Saved forge-write-a-launch-plan-for-squad-2.md",
        "Saved forge-write-a-launch-plan-for-squad-3.md",
    ] {
        let text = saved(save::save_file(&room, job(&app, None, dir.path().into())).await);
        assert_eq!(text, expected);
    }
}

#[tokio::test]
async fn explicit_path_is_relative_to_cwd_and_still_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let room = Stub(FORGE_EXPORT.into());
    let first =
        saved(save::save_file(&room, job(&app, Some("out/plan.md"), dir.path().into())).await);
    assert_eq!(first, "Saved out/plan.md");
    let second =
        saved(save::save_file(&room, job(&app, Some("out/plan.md"), dir.path().into())).await);
    assert_eq!(second, "Saved out/plan-2.md");
    let missing = save::save_file(&room, job(&app, Some("nope/x.md"), dir.path().into())).await;
    assert!(matches!(missing, RequestResult::Rejected { .. }));
}

#[tokio::test]
async fn no_contest_falls_back_to_last_assistant_message_or_the_focused_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let first = record(
        2,
        Actor::Member(Nick::new("alpha").unwrap()),
        "older reply",
        None,
    );
    let focus = first.event_id;
    app.session.snapshot.context.records = vec![
        record(1, Actor::Me, "hello there", None),
        first,
        record(
            3,
            Actor::Member(Nick::new("beta").unwrap()),
            "the newest reply",
            None,
        ),
    ];
    let room = Stub(String::new());
    let latest = job(&app, None, dir.path().into());
    saved(save::save_file(&room, latest).await);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("message-the-newest-reply.md")).unwrap(),
        "the newest reply\n"
    );
    let focused = save::plan(
        &app.session,
        Some(focus),
        None,
        EventId::new(),
        dir.path().into(),
    )
    .unwrap();
    saved(save::save_file(&room, focused).await);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("message-older-reply.md")).unwrap(),
        "older reply\n"
    );
    app.session.snapshot.context.records.clear();
    assert!(save::plan(&app.session, None, None, EventId::new(), dir.path().into()).is_err());
}

#[test]
fn headline_picks_the_deliverable_per_mode() {
    let tune =
        "# TUNE\n\nresult\n\nReason: `x`\n\n## Winning prompt\n\nBe brief.\n\n## Diff\n\n+x\n";
    assert_eq!(save::headline(tune, ContestKind::Tune), "Be brief.\n");
    let decide = "# Decide 1\n\nThe answer is B.\n\n## Decision tree\n\nnode\n";
    assert_eq!(
        save::headline(decide, ContestKind::Decide),
        "The answer is B.\n"
    );
    assert_eq!(save::slug("  Hello, World -- ok?! "), "hello-world-ok");
    assert!(save::slug(&"x".repeat(100)).len() <= 40);
}

#[test]
fn slash_save_runs_through_submit_without_reaching_the_room() {
    let mut app = app();
    finished_forge(&mut app);
    for c in "/save".chars() {
        app.handle(
            Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
            Duration::ZERO,
        );
    }
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Duration::ZERO,
    );
    assert!(app.pending.is_none());
    assert!(app.pending_save.is_some());
    assert_eq!(app.input.text(), "");
}
