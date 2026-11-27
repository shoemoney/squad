//! User-facing contest panels must never show Rust `Debug` output such as
//! `Some(Member(Nick("claude")))` or raw enum names.
use squad_protocol::*;
use squad_tui::widgets::{
    bass_ackwards_prd, blow_it_up, decide, forecast, forge, negotiate, rank, readback, trace, tune,
};

const FORBIDDEN: [&str; 4] = ["Some(", "Member(", "Nick(", "None"];

fn assert_clean(label: &str, lines: &[String]) {
    let text = lines.join("\n");
    for needle in FORBIDDEN {
        assert!(!text.contains(needle), "{label} leaks `{needle}`:\n{text}");
    }
}

macro_rules! state {
    ($ty:ident, $status:expr) => {
        $ty {
            contest_id: ContestId::new(),
            state_version: 1,
            state: $status,
            step_id: None,
            round: 1,
            artifact_event_ids: vec![],
            terminal_reason: None,
        }
    };
}

fn member_artifact(
    contest: ContestId,
    mode: ContestKind,
    kind: &str,
    body: serde_json::Value,
) -> ContestArtifact {
    ContestArtifact {
        contest_id: contest,
        step_id: StepId::new(),
        artifact_kind: Text::new(kind).unwrap(),
        kind: Some(mode),
        round: Some(1),
        actor: Some(Actor::Member("claude".parse().unwrap())),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
        data: serde_json::json!({}),
        work_id: None,
        source_refs: vec![],
    }
}

#[test]
fn state_headers_show_readable_labels() {
    assert_clean(
        "forecast",
        &forecast::lines(&state!(ForecastState, ForecastStatus::Forecasting)),
    );
    assert_clean(
        "forge",
        &forge::lines(&state!(ForgeState, ForgeStatus::Drafting)),
    );
    assert_clean(
        "negotiate",
        &negotiate::lines(&state!(NegotiateState, NegotiateStatus::Rehearsing)),
    );
    assert_clean(
        "trace",
        &trace::lines(&state!(TraceState, TraceStatus::Verifying)),
    );
    assert_clean(
        "rank",
        &rank::lines(&state!(RankState, RankStatus::Comparing)),
    );
    assert_clean(
        "readback",
        &readback::lines(&state!(ReadbackState, ReadbackStatus::AwaitingHuman)),
    );
    assert_clean(
        "decide",
        &decide::lines(&state!(DecideState, DecideStatus::Scoring)),
    );
    assert_clean(
        "blow_it_up",
        &blow_it_up::lines(&state!(BlowItUpState, BlowItUpStatus::Complete)),
    );
    assert_clean(
        "bass_ackwards_prd",
        &bass_ackwards_prd::lines(&state!(
            BassAckwardsPrdState,
            BassAckwardsPrdStatus::AwaitingHuman
        )),
    );
    let tune_lines = squad_tui::widgets::tune::render(&{
        let mut v = tune::View::default();
        v.apply(
            EventId::new(),
            &EnvelopePayload::TuneState(state!(TuneState, TuneStatus::Mutating)),
        );
        v
    });
    assert_clean("tune", &tune_lines);
    assert!(
        forecast::lines(&state!(ForecastState, ForecastStatus::Forecasting))
            .join(" ")
            .contains("forecasting")
    );
}

#[test]
fn actor_attribution_shows_the_nick_only() {
    let contest = ContestId::new();
    let mut view = forecast::View::default();
    let mut st = state!(ForecastState, ForecastStatus::Forecasting);
    st.contest_id = contest;
    view.apply(EventId::new(), &EnvelopePayload::ForecastState(st));
    view.apply(
        EventId::new(),
        &EnvelopePayload::ContestArtifact(member_artifact(
            contest,
            ContestKind::Forecast,
            "forecast.estimate",
            serde_json::json!({"probabilityPct": 70, "baseRate": {"referenceClass": "r", "ratePct": null}, "factors": [{"direction": "up", "text": "a"}, {"direction": "down", "text": "b"}, {"direction": "up", "text": "c"}], "rationale": "because"}),
        )),
    );
    let lines = forecast::render(&view);
    assert_clean("forecast artifacts", &lines);
    assert!(lines.iter().any(|l| l.contains("claude: 70%")), "{lines:?}");

    let mut forge_view = forge::View::default();
    forge_view.apply(
        EventId::new(),
        &EnvelopePayload::ContestArtifact(member_artifact(
            contest,
            ContestKind::Forge,
            "forge.draft",
            serde_json::json!({}),
        )),
    );
    let lines = forge::render(&forge_view);
    assert_clean("forge artifacts", &lines);
    assert!(
        lines.iter().any(|l| l.starts_with("claude · draft")),
        "{lines:?}"
    );
}
