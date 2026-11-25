//! Preflight display: authoritative call counts per mode, confirmation bound to
//! the plan identity, post-pick context and status restoration.
use serde_json::json;
use squad_protocol::*;
use squad_tui::{
    actions,
    widgets::preflight::{PostPick, PreflightView, restored_status},
};

fn nicks(n: usize) -> Vec<MemberNick> {
    (0..n)
        .map(|i| Nick::new(&format!("m{i}")).unwrap())
        .collect()
}
fn estimate(calls: u32) -> DebateEstimate {
    DebateEstimate {
        estimated_usd: 0.1234,
        reserved_usd: 0.5,
        remaining_usd: 20.0,
        model_calls: calls,
    }
}
fn debate(rounds: u32, judge: bool, opening: bool, judge_only: bool, calls: u32) -> PreflightView {
    PreflightView {
        pending: false,
        plan: PendingPreflight::Debate(DebatePreflight {
            judge_only,
            debate_id: DebateId::new(),
            plan_id: PlanId::new(),
            expires_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            estimate: estimate(calls),
            judge_model: judge.then(|| "judge/model".into()),
            warning: None,
            confirmation_required: true,
            debaters: [Nick::new("grok").unwrap(), Nick::new("gemini").unwrap()],
            rounds,
            judge_requested: judge,
            opening_round_required: opening,
        }),
    }
}
fn contest(kind: ContestKind, n: usize, calls: u32, settings: serde_json::Value) -> PreflightView {
    PreflightView {
        pending: false,
        plan: PendingPreflight::Contest(ContestPreflight {
            contest_id: ContestId::new(),
            kind,
            plan_id: PlanId::new(),
            expires_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            estimate: estimate(calls),
            participants: nicks(n),
            confirmation_required: true,
            warning: None,
            settings,
        }),
    }
}

// Formulas from the README table, local to the test so the widget never needs them.
fn debate_calls(r: u32, opening: bool, judge: bool) -> u32 {
    2 * (r + u32::from(opening)) + if judge { r + 1 } else { 0 }
}

#[test]
fn debate_shows_core_count_with_round_scoring_per_configured_round() {
    for (r, opening, judge) in [
        (3, false, true),
        (3, true, true),
        (2, true, false),
        (5, false, false),
    ] {
        let calls = debate_calls(r, opening, judge);
        let text = debate(r, judge, opening, false, calls).text();
        assert!(text.contains(&format!("{calls} calls")), "{text}");
        if judge {
            assert!(
                text.contains(&format!("{r} round-scoring judge calls")),
                "{text}"
            );
            assert!(text.contains("1 final judging call"));
        } else {
            assert!(text.contains("no judge calls"));
        }
        assert!(text.contains("grok vs gemini"));
        assert!(text.contains(&format!("{r} rounds")));
        assert!(text.contains(&format!("opening {opening}")));
    }
}

#[test]
fn judge_only_preflight_has_no_round_scoring() {
    let text = debate(3, true, false, true, 1).text();
    assert!(text.contains("judge only"));
    assert!(text.contains("1 calls"));
    assert!(!text.contains("round-scoring judge calls"));
}

#[test]
fn tournament_shows_k_plus_down_to_two_answers_and_k_minus_one_judges() {
    for k in 2..=6u32 {
        let answers: u32 = (2..=k).sum();
        let total = answers + (k - 1);
        assert_eq!(total, (2..=k).map(|n| n + 1).sum::<u32>());
        let text = contest(ContestKind::Tournament, k as usize, total, json!({})).text();
        assert!(text.contains(&format!("{total} calls")));
        assert!(
            text.contains(&format!("{answers} answer calls · {} judge calls", k - 1)),
            "{text}"
        );
    }
}

#[test]
fn grill_shows_rounds_times_two_grillers_plus_one() {
    let (r, g) = (3u32, 3u32);
    let text = contest(
        ContestKind::Grill,
        (g + 1) as usize,
        r * (2 * g + 1),
        json!({"totalRounds": r}),
    )
    .text();
    assert!(text.contains("21 calls"));
    assert!(text.contains("3 rounds × (3 grillers × 2 + 1) calls"));
    assert!(text.contains("leader m0"));
}

#[test]
fn census_shows_passes_and_reserved_attempts() {
    let (p, m) = (2u32, 4u32);
    let text = contest(
        ContestKind::Census,
        m as usize,
        p * (2 * m + 2),
        json!({"passes": p, "reservedCalls": p * (4 * m + 6)}),
    )
    .text();
    assert!(text.contains("20 calls"));
    assert!(text.contains("2 passes"));
    assert!(text.contains("44 reserved attempts"));
    assert!(text.contains("2 passes × (4 members × 2 + 2) calls"));
}

#[test]
fn crosscheck_shows_optional_extraction_call() {
    let m = 3u32;
    for extraction in [false, true] {
        let calls = 2 * m + 4 + u32::from(extraction);
        let text = contest(
            ContestKind::Crosscheck,
            m as usize,
            calls,
            json!({"extractionRequired": extraction}),
        )
        .text();
        assert!(text.contains(&format!("{calls} calls")));
        assert!(
            text.contains(if extraction {
                "extraction call included"
            } else {
                "extraction call not needed"
            }),
            "{text}"
        );
    }
}

#[test]
fn survey_distinguishes_new_and_existing_answers() {
    let m = 3u32;
    let new = contest(
        ContestKind::Survey,
        m as usize,
        3 * m + 2,
        json!({"collect": ["m0","m1","m2"]}),
    )
    .text();
    assert!(
        new.contains("11 calls") && new.contains("collecting 3 new answers"),
        "{new}"
    );
    let existing = contest(
        ContestKind::Survey,
        m as usize,
        2 * m + 2,
        json!({"collect": []}),
    )
    .text();
    assert!(
        existing.contains("8 calls") && existing.contains("using existing answers"),
        "{existing}"
    );
}

#[test]
fn probe_shows_adjusted_participant_count_and_estimate() {
    let m = 4u32;
    let text = contest(ContestKind::Probe, m as usize, 3 * m / 2 + 1, json!({})).text();
    assert!(text.contains("7 calls"));
    assert!(text.contains("4 participants after adjustment"));
    assert!(text.contains("estimate $0.1234") && text.contains("remaining $20.0000"));
}

#[test]
fn controls_bind_to_plan_identity_and_pending_state_blocks_them() {
    let view = debate(3, true, false, false, 10);
    assert!(view.text().ends_with("Y Confirm · N Cancel"));
    let Request::DebateConfirm { plan_id, .. } = actions::preflight(&view.plan, true) else {
        panic!()
    };
    assert_eq!(plan_id, view.plan_id());
    assert!(matches!(
        actions::preflight(&view.plan, false),
        Request::DebateCancel { plan_id: p, .. } if p == view.plan_id()
    ));
    let mut pending = view.clone();
    pending.pending = true;
    assert!(pending.text().ends_with("Waiting for acknowledgement…"));
    let mut auto = contest(ContestKind::Probe, 2, 4, json!({}));
    if let PendingPreflight::Contest(p) = &mut auto.plan {
        p.confirmation_required = false;
    }
    assert!(auto.text().ends_with("Core auto-confirmed"));
}

#[test]
fn post_pick_shows_selected_sources_default_judging_and_excluded_billing() {
    let view = debate(3, true, false, false, 10);
    let debate_pick = PostPick {
        picked: vec!["grok".into(), "gemini".into()],
        grill: false,
        judge_default: true,
        billed_collection_answers: 4,
    };
    let text = view.text_with(Some(&debate_pick));
    assert!(text.contains("Solutions from grok and gemini"));
    assert!(text.contains("Judging requested by default"));
    assert!(text.contains("4 collection answers already billed · excluded from this estimate"));
    let grill = PostPick {
        picked: vec!["claude".into()],
        grill: true,
        judge_default: false,
        billed_collection_answers: 0,
    };
    let text = contest(ContestKind::Grill, 3, 9, json!({"totalRounds": 2})).text_with(Some(&grill));
    assert!(text.contains("Grill leader claude"));
    assert!(!text.contains("excluded from this estimate"));
}

#[test]
fn cancel_and_expiry_restore_prior_status_but_confirmation_does_not() {
    assert_eq!(
        restored_status(&PreflightStatus::Cancelled, "Debate idle"),
        Some("Debate idle")
    );
    assert_eq!(
        restored_status(&PreflightStatus::Expired, "Debate idle"),
        Some("Debate idle")
    );
    assert_eq!(
        restored_status(&PreflightStatus::Confirmed, "Debate idle"),
        None
    );
    assert_eq!(
        restored_status(&PreflightStatus::Rejected, "Debate idle"),
        None
    );
}
