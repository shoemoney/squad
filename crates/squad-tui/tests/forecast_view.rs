use squad_protocol::*;
use squad_tui::widgets::forecast::*;
#[test]
fn enabled_forecast_is_discoverable_in_completion_help_and_rotating_tips() {
    use squad_tui::typeahead;
    assert!(typeahead::enabled("$forecast"));
    assert!(
        typeahead::MODES
            .iter()
            .any(|(token, _)| *token == "$forecast")
    );
    assert_eq!(typeahead::rank("$forecast", "$fore"), Some(0));
    assert!(typeahead::describe("$forecast", &[]).contains("pooled probability"));
    assert!(
        squad_tui::actions::help_lines()
            .iter()
            .any(|line| line.contains("$forecast"))
    );
    let context = parser::ParseContext {
        roster: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let examples = typeahead::examples(&context);
    let (index, example) = examples
        .iter()
        .enumerate()
        .find(|(_, s)| s.contains("$forecast"))
        .unwrap();
    assert!(matches!(
        parse(example, &parser::SubmissionMeta::default(), &context).unwrap(),
        ParsedInput::Forecast { .. }
    ));
    assert!(typeahead::tip(&context, index as u64 * 8).contains("$forecast"));
}
fn state() -> ForecastState {
    ForecastState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: ForecastStatus::Forecasting,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    }
}
#[test]
fn banner_dividers_and_replay_versions() {
    let mut s = state();
    let mut v = View::default();
    v.apply(EventId::new(), &EnvelopePayload::ForecastState(s.clone()));
    assert!(render(&v).contains(&"FIRST FORECASTS".into()));
    s.state = ForecastStatus::Framing;
    s.state_version = 0;
    v.apply(EventId::new(), &EnvelopePayload::ForecastState(s));
    assert_eq!(v.state.unwrap().state, ForecastStatus::Forecasting);
}
#[test]
fn pooled_bar_keeps_spread_band_and_individual_ticks() {
    let a = forecast::ForecastAggregate {
        probabilities_pct: vec![20, 50, 80],
        sorted_log_odds: vec![-1.3862943611198906, 0.0, 1.3862943611198906],
        k: 0,
        trimmed_log_odds: vec![-1.3862943611198906, 0.0, 1.3862943611198906],
        l: 0.0,
        p: 0.5,
        s: 1.3862943611198906,
        low: 0.2,
        high: 0.8,
        minimum_pct: 20,
        maximum_pct: 80,
        split: None,
    };
    let line = bar(&a);
    assert!(line.contains('░'));
    assert!(line.contains('█'));
    assert!(line.contains('│'));
    assert!(line.starts_with("0%"));
    assert!(line.ends_with("100%"));
    let mut v = View::default();
    let s = state();
    v.apply(EventId::new(), &EnvelopePayload::ForecastState(s.clone()));
    v.apply(
        EventId::new(),
        &EnvelopePayload::ContestArtifact(ContestArtifact {
            contest_id: s.contest_id,
            step_id: StepId::new(),
            artifact_kind: Text::new("forecast.aggregate").unwrap(),
            kind: Some(ContestKind::Forecast),
            round: Some(0),
            actor: Some(Actor::System),
            audience: Some(ArtifactAudience::Human),
            body: Some(serde_json::to_value(a).unwrap()),
            data: serde_json::json!({}),
            work_id: None,
            source_refs: vec![],
        }),
    );
    assert!(render(&v).iter().any(|s| s.contains("Pooled 50.0%")));
    let final_aggregate = squad_core::orchestrator::forecast::aggregate(&[60, 70, 80]).unwrap();
    let mut final_artifact = v.artifacts.values().next().unwrap().clone();
    final_artifact.body = Some(serde_json::to_value(&final_aggregate).unwrap());
    v.apply(
        "ffffffff-ffff-7fff-bfff-ffffffffffff".parse().unwrap(),
        &EnvelopePayload::ContestArtifact(final_artifact),
    );
    let rendered = render(&v);
    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.starts_with("Pooled "))
            .count(),
        1
    );
    assert!(
        rendered
            .iter()
            .any(|line| line.contains(&format!("Pooled {:.1}%", 100.0 * final_aggregate.p)))
    );
}
