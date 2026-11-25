use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, FeedOrigin, FxQueue, Palette},
    session::Session,
    widgets::tournament::render,
};
use std::time::Duration;
fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
fn committed(payload: EnvelopePayload, seq: i64) -> CommittedEnvelope {
    CommittedEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: EventId::new(),
            causation_id: None,
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: std::time::SystemTime::now().into(),
        },
        content: payload,
        commit: CommitFields {
            seq: Cursor::new(seq).unwrap(),
            accepted_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            commit_index: 0,
            commit_size: 1,
        },
    }
}
fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn committed_states_snapshot_and_replay_cues() {
    let id = ContestId::new();
    let entrants = vec![nick("fable"), nick("grok"), nick("gemini")];
    let mut session = Session::default();
    let mut queue = FxQueue::new();
    let mut seq = 0;
    let mut apply = |s: &mut Session, payload: EnvelopePayload, q: &mut FxQueue| {
        seq += 1;
        let e = committed(payload, seq);
        assert!(s.reduce(
            FeedItem::Committed(e.clone()),
            FeedOrigin::Live,
            Duration::ZERO,
            q
        ));
        let count = q.len();
        s.reduce(FeedItem::Committed(e), FeedOrigin::Live, Duration::ZERO, q);
        assert_eq!(count, q.len());
    };
    apply(
        &mut session,
        EnvelopePayload::TournamentSetup(TournamentSetup {
            contest_id: id,
            prompt_message_id: MessageId::new(),
            prompt: "Design a reliable release process.".into(),
            entrants: entrants.clone(),
            judge_model: "fixture/judge".into(),
            total_rounds: 2,
        }),
        &mut queue,
    );
    let mut state = TournamentState {
        contest_id: id,
        state: TournamentStatus::Answering,
        state_version: 1,
        round: 1,
        replay: 0,
        survivors: entrants.clone(),
        eliminated: vec![],
        terminal_reason: None,
    };
    apply(
        &mut session,
        EnvelopePayload::TournamentState(state.clone()),
        &mut queue,
    );
    for n in &entrants {
        apply(
            &mut session,
            EnvelopePayload::TournamentEntry(TournamentEntry {
                generation: GenerationIdentity {
                    work_id: WorkId::new(),
                    message_id: MessageId::new(),
                    trigger_event_id: EventId::new(),
                    attempt: 1,
                },
                contest_id: id,
                round: 1,
                replay: 0,
                entrant: n.clone(),
                outcome: DebateTurnOutcome::Completed {
                    content: format!("A concrete proposal from {n}."),
                    model: Text::new("fixture/member").unwrap(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
            }),
            &mut queue,
        );
    }
    let ranking = TournamentRanking {
        contest_id: id,
        case_id: CaseId::new(),
        round: 1,
        replay: 0,
        ranking: entrants
            .iter()
            .enumerate()
            .map(|(i, n)| TournamentRank {
                nick: n.clone(),
                rank: i as u32 + 1,
                score: Some(8),
                critique: Some(format!("Critique for {n}.")),
                forfeited: false,
            })
            .collect(),
    };
    apply(
        &mut session,
        EnvelopePayload::TournamentRanking(ranking),
        &mut queue,
    );
    apply(
        &mut session,
        EnvelopePayload::TournamentResult(TournamentResult {
            contest_id: id,
            round: 1,
            entrant: entrants[2].clone(),
            rank: 3,
            remaining_before: 3,
            score: Some(8),
            critique: Some("A weaker proposal".into()),
            survived: false,
        }),
        &mut queue,
    );
    assert_eq!(queue.len(), 2);
    for (label, status, replay) in [
        ("ranking", TournamentStatus::Judging, 0),
        ("replay", TournamentStatus::Answering, 1),
        ("stopped", TournamentStatus::Stopped, 1),
        ("failed", TournamentStatus::Failed, 1),
    ] {
        state.state = status;
        state.replay = replay;
        state.state_version += 1;
        state.terminal_reason = state
            .state
            .is_terminal()
            .then(|| "Judging stopped after retained replay evidence".into());
        apply(
            &mut session,
            EnvelopePayload::TournamentState(state.clone()),
            &mut queue,
        );
        for width in [120, 80, 40] {
            let mut b = Buffer::empty(Rect::new(0, 0, width, 40));
            render(
                &session.tournaments[&id],
                b.area,
                &mut b,
                &Palette::new(ColorTier::NoColor),
            );
            insta::assert_snapshot!(format!("{label}_{width}"), text(&b));
        }
    }
    state.state = TournamentStatus::Complete;
    state.round = 2;
    state.replay = 0;
    state.survivors = vec![entrants[0].clone()];
    state.eliminated = entrants[1..].to_vec();
    state.terminal_reason = None;
    state.state_version += 1;
    apply(
        &mut session,
        EnvelopePayload::TournamentState(state.clone()),
        &mut queue,
    );
    let champion = TournamentChampion {
        contest_id: id,
        champion: entrants[0].clone(),
        placements: entrants
            .iter()
            .enumerate()
            .map(|(i, n)| TournamentPlacement {
                nick: n.clone(),
                placement: i as u32 + 1,
            })
            .collect(),
        trophies: 4,
    };
    apply(
        &mut session,
        EnvelopePayload::TournamentChampion(champion),
        &mut queue,
    );
    assert_eq!(session.snapshot.trophies[&entrants[0]], 4);
    for width in [120, 80, 40] {
        let mut b = Buffer::empty(Rect::new(0, 0, width, 60));
        render(
            &session.tournaments[&id],
            b.area,
            &mut b,
            &Palette::new(ColorTier::NoColor),
        );
        insta::assert_snapshot!(format!("champion_{width}"), text(&b));
    }
    let events = session.snapshot.events.clone();
    let mut restored = Session::default();
    let mut replay = FxQueue::new();
    for e in events {
        restored.reduce(
            FeedItem::Committed(e),
            FeedOrigin::Replay,
            Duration::ZERO,
            &mut replay,
        );
    }
    assert!(replay.is_empty());
    assert_eq!(restored.snapshot.trophies[&entrants[0]], 4);
}
