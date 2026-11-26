use squad_protocol::*;
use squad_tui::widgets::blow_it_up::*;
#[test]
fn banner_dividers_and_terminal_state_replay_are_stable() {
    let id = ContestId::new();
    let state = BlowItUpState {
        contest_id: id,
        state_version: 1,
        state: BlowItUpStatus::Stopped,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: Some("stopped".parse().unwrap()),
    };
    let mut view = View::default();
    let event = EventId::new();
    view.apply(event, &EnvelopePayload::BlowItUpState(state.clone()));
    view.apply(event, &EnvelopePayload::BlowItUpState(state));
    let lines = render(&view).join("\n");
    for expected in [
        "BLOW IT UP",
        "ATTACKS",
        "CHALLENGE",
        "WHAT SURVIVED",
        "Stopped",
    ] {
        assert!(lines.contains(expected));
    }
    assert!(!view.killed_expanded);
    view.toggle_killed();
    assert!(view.killed_expanded);
}
#[test]
fn severity_palette_is_distinct() {
    use squad_protocol::blow_it_up::Severity::*;
    let colors = [Critical, High, Medium, Low].map(severity_color);
    for i in 0..4 {
        for j in 0..i {
            assert_ne!(colors[i], colors[j]);
        }
    }
}

#[test]
fn enabled_mode_appears_in_typeahead_and_help() {
    let ctx = parser::ParseContext {
        roster: ["alpha", "beta", "gamma"]
            .map(|n| n.parse().unwrap())
            .to_vec(),
        ..Default::default()
    };
    assert!(squad_tui::typeahead::enabled("$blow_it_up"));
    let raw = "@all $bl";
    let c = squad_tui::widgets::completion::CompletionState::at(
        raw,
        raw.len(),
        &ctx,
        &Default::default(),
    );
    assert!(c.items.iter().any(|i| i == "$blow_it_up"));
    let example = squad_protocol::parser::help_examples()
        .into_iter()
        .find(|s| s.contains("$blow_it_up"))
        .unwrap();
    assert!(parse(example, &Default::default(), &ctx).is_ok());
}
