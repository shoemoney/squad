use squad_core::export::{ExportSnapshot, render};
use squad_protocol::*;
fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
#[test]
fn typed_tournament_export_has_public_rankings_failures_and_placements_only() {
    let id: ContestId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let case: CaseId = id.to_string().parse().unwrap();
    let entrants = vec![nick("fable"), nick("grok"), nick("gemini")];
    let setup = TournamentSetup {
        contest_id: id,
        prompt_message_id: id.to_string().parse().unwrap(),
        prompt: "Design a bridge.".into(),
        entrants: entrants.clone(),
        judge_model: "fixture/judge".into(),
        total_rounds: 2,
    };
    let ranking = TournamentRanking {
        contest_id: id,
        case_id: case,
        round: 1,
        replay: 1,
        ranking: entrants
            .iter()
            .enumerate()
            .map(|(i, n)| TournamentRank {
                nick: n.clone(),
                rank: i as u32 + 1,
                score: (i < 2).then_some(8),
                critique: (i < 2).then(|| "Supported design".into()),
                forfeited: i == 2,
            })
            .collect(),
    };
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
        trophies: 1,
    };
    let payloads = vec![
        EnvelopePayload::TournamentSetup(setup),
        EnvelopePayload::TournamentState(TournamentState {
            contest_id: id,
            state: TournamentStatus::Failed,
            state_version: 1,
            round: 1,
            replay: 0,
            survivors: entrants,
            eliminated: vec![],
            terminal_reason: Some("Judging exhausted; retained failed evidence".into()),
        }),
        EnvelopePayload::TournamentRanking(ranking),
        EnvelopePayload::TournamentResult(TournamentResult {
            contest_id: id,
            round: 1,
            entrant: nick("gemini"),
            rank: 3,
            remaining_before: 3,
            score: None,
            critique: None,
            survived: false,
        }),
        EnvelopePayload::TournamentChampion(champion),
    ];
    let events = payloads
        .into_iter()
        .enumerate()
        .map(|(i, content)| CommittedEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: format!("01900000-0000-7000-8000-{:012}", i + 10)
                    .parse()
                    .unwrap(),
                room_id: MainRoom::Main,
                correlation_id: id.to_string().parse().unwrap(),
                causation_id: None,
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            },
            content,
            commit: CommitFields {
                seq: Cursor::new(i as i64 + 1).unwrap(),
                accepted_at: "2026-11-25T00:00:00Z".parse().unwrap(),
                commit_index: 0,
                commit_size: 1,
            },
        })
        .collect();
    let output = render(
        &ExportSnapshot {
            events,
            ..Default::default()
        },
        ExportScope::Tournament,
        Some(id),
    )
    .unwrap();
    assert!(!output.contains("Entrant A") && !output.contains("labelMap"));
    insta::assert_snapshot!(output);
}
