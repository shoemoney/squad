//! Input is handled by the application even while a catalogue pass is active.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::{CatalogueEffect, FeedOrigin, FxContext, FxCue, TerminalCaps, TimedCue},
};
use std::time::Duration;

const EFFECTS: [CatalogueEffect; 18] = [
    CatalogueEffect::Chrome,
    CatalogueEffect::Status,
    CatalogueEffect::Meter,
    CatalogueEffect::Bubble,
    CatalogueEffect::Dispatch,
    CatalogueEffect::Command,
    CatalogueEffect::Particles,
    CatalogueEffect::Debate,
    CatalogueEffect::Filter,
    CatalogueEffect::Banner,
    CatalogueEffect::Pick,
    CatalogueEffect::Verdict,
    CatalogueEffect::Tournament,
    CatalogueEffect::Grill,
    CatalogueEffect::Census,
    CatalogueEffect::Crosscheck,
    CatalogueEffect::Survey,
    CatalogueEffect::Probe,
];

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn animated(effect: CatalogueEffect, pick: bool) -> App {
    let caps = TerminalCaps::resolve(
        |k| match k {
            "SQUAD_FX" => Some("full".into()),
            "COLORTERM" => Some("truecolor".into()),
            _ => None,
        },
        true,
        (80, 24).into(),
    )
    .unwrap();
    let mut app = App::new(caps);
    let id = ContestId::new();
    app.session.snapshot.context.roster = vec!["alpha".parse().unwrap(), "beta".parse().unwrap()];
    app.session.snapshot.context.active_contest = Some(if pick {
        ContestState::Selection(ContestSelectionState {
            contest_id: id,
            kind: ContestKind::Debate,
            state: ContestSelectionStatus::AwaitingPick,
            state_version: 1,
            question_message_id: MessageId::new(),
            addressed: app.session.snapshot.context.roster.clone(),
            answers: vec![],
            required_picks: 2,
        })
    } else {
        ContestState::Generic(GenericContestState {
            contest_id: id,
            kind: ContestKind::Survey,
            state_version: 1,
            status: Text::new("RUNNING").unwrap(),
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
        })
    });
    app.fx.queue_mut().push(
        FeedOrigin::Live,
        TimedCue {
            event_id: EventId::new(),
            at: Duration::ZERO,
            cue: FxCue::Catalogue {
                effect,
                target: id.to_string(),
                duration: Duration::from_secs(5),
            },
        },
    );
    app.fx.pre(&mut FxContext {
        pending_pick: pick,
        ..Default::default()
    });
    assert_eq!(app.fx.active_count(), 1, "{effect:?}");
    app
}

#[test]
fn every_effect_skips_without_consuming_text_or_submission() {
    for effect in EFFECTS {
        let mut app = animated(effect, false);
        app.handle(key(KeyCode::Char('@')), Duration::ZERO);
        assert_eq!(app.fx.active_count(), 0, "{effect:?}");
        assert_eq!(app.input.text(), "@");
        for c in "alpha ordinary message".chars() {
            app.handle(key(KeyCode::Char(c)), Duration::ZERO);
        }
        app.handle(key(KeyCode::Enter), Duration::ZERO);
        assert!(
            matches!(app.pending, Some(Request::UserMessage { .. })),
            "{effect:?}: {:?}",
            app.pending
        );
        assert!(app.awaiting_ack);
        assert_eq!(app.input.text(), "");
        assert!(app.working.is_some());
    }
}

#[test]
fn escape_priority_and_stop_ack_survive_every_effect_and_pending_pick() {
    for effect in EFFECTS {
        for pick in [false, true] {
            let mut app = animated(effect, pick);
            app.transient = Some("Help".into());
            app.handle(key(KeyCode::Esc), Duration::ZERO);
            assert_eq!(app.fx.active_count(), 0);
            assert!(app.transient.is_none());
            assert!(app.pending.is_none());
            assert!(app.stop_confirmation.is_none());
            app.handle(key(KeyCode::Esc), Duration::from_millis(100));
            assert!(app.pending.is_none());
            assert!(app.stop_confirmation.is_some());
            app.handle(key(KeyCode::Esc), Duration::from_millis(200));
            assert!(
                matches!(app.pending, Some(Request::Stop { .. })),
                "{effect:?} pick={pick}"
            );
            assert!(app.awaiting_ack);
            assert!(
                app.session.active_contest(),
                "no optimistic stop before acknowledgement"
            );
            assert_eq!(app.notice.as_deref(), Some("Stopping…"));
        }
    }
}
