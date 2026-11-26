//! TUI destinations preserve the core export byte for byte in every scope.
#[path = "../../squad-core/tests/room_support.rs"]
mod support;
use squad_protocol::*;
use squad_tui::actions;
use support::*;

struct FlushFailure(Vec<u8>);
impl std::io::Write for FlushFailure {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("fixture flush failure"))
    }
}

#[tokio::test]
async fn all_scopes_preserve_core_markdown_and_report_success_only_after_flush() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, server) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let mut items = debate_fixture();
    items.extend(tournament_grill_pick_fixture());
    items.extend(new_modes_fixture());
    commit_each(&launcher, items);
    let calls = server.received_requests().await.unwrap().len();
    for (scope, id) in [
        (ExportScope::Room, None),
        (ExportScope::Debate, Some(CONTEST.parse().unwrap())),
        (ExportScope::Tournament, Some(uuid(500).parse().unwrap())),
        (ExportScope::Grill, Some(uuid(600).parse().unwrap())),
        (ExportScope::Census, Some(uuid(800).parse().unwrap())),
        (ExportScope::Crosscheck, Some(uuid(801).parse().unwrap())),
        (ExportScope::Survey, Some(uuid(802).parse().unwrap())),
        (ExportScope::Probe, Some(uuid(803).parse().unwrap())),
    ] {
        let expected = room.export(scope.clone(), id).await.unwrap();
        assert!(!expected.contains("fake-room-key"));
        let mut saved = Vec::new();
        actions::export_to(&room, scope.clone(), id, &mut saved)
            .await
            .unwrap();
        assert_eq!(saved, expected.as_bytes(), "{scope:?}");
        let mut failed = FlushFailure(vec![]);
        assert!(
            actions::export_to(&room, scope.clone(), id, &mut failed)
                .await
                .is_err()
        );
        assert_eq!(
            failed.0,
            expected.as_bytes(),
            "failure must come from flush"
        );
        let destination = root.path().join(format!("{scope:?}.md"));
        let result = actions::export_file(
            &room,
            actions::ExportJob {
                request_id: EventId::new(),
                scope: scope.clone(),
                id,
                path: destination.clone(),
            },
        )
        .await;
        assert!(
            matches!(result, RequestResult::Internal(InternalResult::Text { .. })),
            "{result:?}"
        );
        assert_eq!(std::fs::read(destination).unwrap(), saved);
    }
    assert_eq!(server.received_requests().await.unwrap().len(), calls);
    room.shutdown().await.unwrap();
}
