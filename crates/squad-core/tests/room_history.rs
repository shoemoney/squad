#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn backward_pages_are_ascending_exclusive_and_validate_limits() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    assert!(room.page_events(None, 100).await.unwrap().is_empty());
    for value in [3, 4, 5] {
        accepted(room.submit(set(value)).await);
    }
    let all = room.page_events(None, 100).await.unwrap();
    assert_eq!(all.len(), 3);
    let newest = room.page_events(None, 2).await.unwrap();
    assert_eq!(newest, all[1..]);
    assert_eq!(
        room.page_events(Some(newest[0].commit.seq), 2)
            .await
            .unwrap(),
        all[..1]
    );
    assert!(
        room.page_events(Some(all[0].commit.seq), 1)
            .await
            .unwrap()
            .is_empty()
    );
    for limit in [0, 501, usize::MAX] {
        assert_eq!(
            room.page_events(None, limit).await.unwrap_err().code,
            ErrorCode::INVALID_COMMAND
        );
    }
    assert_eq!(
        room.page_events(Some(Cursor::new(999).unwrap()), 1)
            .await
            .unwrap_err()
            .code,
        ErrorCode::CURSOR_INVALID
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn subscribe_before_history_allows_overlap_and_clear_expires_old_cursor() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    let cursor = accepted(room.submit(set(3)).await);
    let FeedItem::Committed(event) = feed.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(room.page_events(None, 100).await.unwrap()[0], event);
    let clear = accepted(
        room.submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await,
    );
    assert!(clear > cursor);
    assert_eq!(
        room.page_events(Some(cursor), 100).await.unwrap_err().code,
        ErrorCode::CURSOR_EXPIRED
    );
    let events = room.page_events(None, 100).await.unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].content, EnvelopePayload::RoomCleared(_)));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn finalized_events_survive_restart_without_provisional_text() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    accepted(room.submit(message("@a durable", EventId::new())).await);
    let before = room.page_events(None, 100).await.unwrap();
    room.shutdown().await.unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    assert_eq!(room.page_events(None, 100).await.unwrap(), before);
    assert!(matches!(
        room.subscribe().try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn backward_pagination_keeps_whole_commit_groups_even_above_target_limit() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let oldest = accepted(room.submit(set(3)).await);
    accepted(
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![
                VariableUpdate {
                    name: VariableName::Rounds,
                    value: 4,
                },
                VariableUpdate {
                    name: VariableName::GrillRounds,
                    value: 3,
                },
            ],
        })
        .await,
    );
    let page = room.page_events(None, 1).await.unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].commit.commit_index, 0);
    assert_eq!(page[1].commit.commit_index, 1);
    let older = room.page_events(Some(page[0].commit.seq), 1).await.unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].commit.seq, oldest);
    let interior = room.page_events(Some(page[1].commit.seq), 1).await.unwrap();
    assert_eq!(interior, older);
    room.shutdown().await.unwrap();
}

// ---- M3.P2.L1.T5-T7: durable history for scores, contests and the four new modes ----

/// Page the full history at several limits, across a restart, and require byte-equal committed events.
async fn assert_pages_and_recovery_preserve(
    root: &std::path::Path,
    launcher: &Launcher,
    room: squad_core::room::RoomHandle,
    committed: &[CommittedEnvelope],
) {
    for limit in [1, 2, 3, 500] {
        let paged = page_all(&room, limit).await;
        assert_eq!(paged, committed, "limit {limit}");
        assert!(paged.windows(2).all(|w| w[0].commit.seq < w[1].commit.seq));
    }
    room.shutdown().await.unwrap();
    let _ = launcher;
    let (room, _) = open(root, &Launcher::default(), OpenMode::OneShot).await;
    assert_eq!(page_all(&room, 2).await, committed, "after recovery");
    assert!(matches!(
        room.subscribe().try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn round_scores_late_markers_and_final_tallies_survive_paging_and_recovery() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let committed = commit_each(&launcher, debate_fixture());
    // The late round-3 score is an unscored marker that still names its round and tally.
    let late = committed
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::DebateRoundScore(s) => Some(s),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(late.len(), 2);
    assert_eq!(late[0].round, 1);
    assert!(late[0].scores.is_some());
    assert_eq!((late[1].round, late[1].scores.is_none()), (3, true));
    let verdict = committed
        .iter()
        .find_map(|e| match &e.content {
            EnvelopePayload::Verdict(v) => Some(v),
            _ => None,
        })
        .unwrap();
    assert_eq!(verdict.verdict.round_tally[&nick("a")], 2);
    // Banners are derived on export; rendering never appends events or changes the cursor.
    let before = room.page_events(None, 500).await.unwrap().len();
    let contest: ContestId = CONTEST.parse().unwrap();
    let export = room
        .export(ExportScope::Debate, Some(contest))
        .await
        .unwrap();
    assert!(export.contains("round 1 score"));
    assert_eq!(room.page_events(None, 500).await.unwrap().len(), before);
    assert_eq!(
        room.export(ExportScope::Debate, Some(contest))
            .await
            .unwrap(),
        export
    );
    assert_pages_and_recovery_preserve(root.path(), &launcher, room, &committed).await;
}

#[tokio::test]
async fn tournament_grill_collection_and_pick_events_keep_order_attribution_and_payloads() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let committed = commit_each(&launcher, tournament_grill_pick_fixture());
    let kinds: Vec<String> = committed
        .iter()
        .map(|e| {
            serde_json::to_value(&e.content).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "contest.state",
            "contest.artifact",
            "contest.state",
            "contest.state",
            "contest.artifact",
            "contest.collecting",
            "contest.pick_requested",
            "contest.picked"
        ]
    );
    assert!(committed.iter().all(|e| e.base.sender == Actor::System));
    let paged = page_all(&room, 3).await;
    // The pick events keep their owning payload fields rather than a derived summary.
    let EnvelopePayload::ContestPickRequested(requested) = &paged[6].content else {
        panic!("{:?}", paged[6])
    };
    assert_eq!(requested.eligible, vec![nick("a"), nick("b")]);
    assert_eq!(requested.required_picks, 1);
    let EnvelopePayload::ContestPicked(picked) = &paged[7].content else {
        panic!()
    };
    assert_eq!(picked.nicks, vec![nick("a")]);
    assert_pages_and_recovery_preserve(root.path(), &launcher, room, &committed).await;
}

#[tokio::test]
async fn census_crosscheck_survey_and_probe_events_keep_metadata_skipped_members_and_partials() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let committed = commit_each(&launcher, new_modes_fixture());
    assert_eq!(committed.len(), 16);
    let text = serde_json::to_string(&page_all(&room, 4).await).unwrap();
    assert_eq!(
        text.matches("skippedMembers").count(),
        8,
        "artifact and report for each mode"
    );
    for mode in ["census", "crosscheck", "survey", "probe"] {
        assert!(text.contains(&format!("{mode} setup")), "{mode}");
    }
    assert!(text.contains("\"partial\":true"));
    assert!(text.contains("\"humanVisible\""));
    // The latest state of every mode is the terminal partial run, not the in-flight one.
    let states: Vec<_> = committed
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::ContestState(ContestState::Generic(g)) => Some(g.status.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(states.iter().filter(|s| **s == "STOPPED").count(), 4);
    assert_pages_and_recovery_preserve(root.path(), &launcher, room, &committed).await;
}
