//! Pick routing, judge validation, help/export presentation, contest progress and
//! diagnostics (M4.P3.L2/L3/L7/L8, M4.P1.L6).
use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::{
    parser::{ParseContext, PendingPick, QuestionFirstMode, SubmissionMeta},
    *,
};
use squad_tui::{
    actions,
    fx::{ColorTier, Palette},
    session::{FakeRoom, judge_request, rejection_text},
    validation::{self, Route},
    widgets::{
        contest_status::*,
        debug::{Diagnostics, render_diagnostics},
        participants::{ParticipantsRects, pick_candidate_rows, pick_marker},
        pick::{Candidate, PickKind, PickStatus, PickView, render},
    },
};

fn ctx_with_pick() -> (ParseContext, ContestId) {
    let id = ContestId::new();
    let ctx = ParseContext {
        roster: ["claude", "grok", "gemini"]
            .map(|n| Nick::new(n).unwrap())
            .to_vec(),
        pending_pick: Some(PendingPick {
            contest_id: id,
            mode: QuestionFirstMode::Debate,
            eligible: vec![Nick::new("grok").unwrap(), Nick::new("gemini").unwrap()],
        }),
        ..Default::default()
    };
    (ctx, id)
}

#[test]
fn mentions_only_pick_routes_to_contest_pick_and_ordinary_text_is_preserved() {
    let (ctx, id) = ctx_with_pick();
    let meta = SubmissionMeta::default();
    let Some(Request::ContestPick {
        contest_id, nicks, ..
    }) = validation::pick_request("@grok @gemini", &meta, &ctx, EventId::new()).unwrap()
    else {
        panic!("expected a pick")
    };
    assert_eq!(contest_id, id);
    assert_eq!(nicks.len(), 2);
    // An unrelated ordinary message is not consumed by the pending pick.
    assert!(
        validation::pick_request("@claude what time is it", &meta, &ctx, EventId::new())
            .unwrap()
            .is_none()
    );
    let v = validation::validate("@claude what time is it", &meta, &ctx).unwrap();
    assert_eq!(validation::route(&v.parsed), Route::Ordinary);
}

#[test]
fn wrong_pick_and_ineligible_pick_use_the_exact_parser_strings() {
    let (ctx, _) = ctx_with_pick();
    let meta = SubmissionMeta::default();
    let one = validation::pick_request("@grok", &meta, &ctx, EventId::new()).unwrap_err();
    assert_eq!(one.message, "Pick exactly two of: @grok, @gemini.");
    let bad = validation::pick_request("@claude @grok", &meta, &ctx, EventId::new()).unwrap_err();
    assert_eq!(
        bad.message,
        "@claude did not answer, so it cannot be picked."
    );
    assert_eq!(
        PickView::wrong_pick_text_for_test(),
        "Pick exactly two of: @grok, @gemini."
    );
}
trait WrongPickFixture {
    fn wrong_pick_text_for_test() -> String;
}
impl WrongPickFixture for PickView {
    fn wrong_pick_text_for_test() -> String {
        let cands = ["grok", "gemini", "claude"]
            .iter()
            .map(|n| Candidate {
                nick: (*n).into(),
                eligible: *n != "claude",
                status: "answered".into(),
                answer: String::new(),
            })
            .collect();
        let mut v = PickView::new(PickKind::Debate, "q", cands);
        v.status = PickStatus::Pending;
        v.wrong_pick_feedback()
    }
}
#[test]
fn insufficient_answers_text_matches_the_prd_string() {
    assert_eq!(
        PickView::insufficient_feedback(1, 3),
        "Not enough answers to start. 1 of 3 members answered."
    );
}

#[test]
fn judge_without_quote_shows_the_parser_string_verbatim() {
    let ctx = ParseContext {
        roster: ["claude", "grok"].map(|n| Nick::new(n).unwrap()).to_vec(),
        ..Default::default()
    };
    let meta = SubmissionMeta::default();
    assert_eq!(
        validation::judge_error("@judge which is best?", &meta, &ctx).as_deref(),
        Some("Quote at least one message for @judge to rule on.")
    );
    assert!(validation::judge_error("hello @claude", &meta, &ctx).is_none());
    assert!(matches!(
        judge_request(None),
        Request::Judge {
            debate_id: None,
            ..
        }
    ));
    let did = DebateId::new();
    assert!(matches!(
        judge_request(Some(did)),
        Request::Judge { debate_id: Some(d), .. } if d == did
    ));
}

#[tokio::test]
async fn rejection_text_is_verbatim_and_session_keeps_pending_pick() {
    let room = FakeRoom::new();
    let result = room.submit_for_test(Request::Who).await;
    assert!(rejection_text(&result).is_none());
    let rejected = squad_tui::session::RoomPort::submit(&room, judge_request(None)).await;
    assert!(rejection_text(&rejected).is_some());
}
trait SubmitHelper {
    async fn submit_for_test(&self, r: Request) -> RequestResult;
}
impl SubmitHelper for FakeRoom {
    async fn submit_for_test(&self, r: Request) -> RequestResult {
        squad_tui::session::RoomPort::submit(self, r).await
    }
}

#[test]
fn help_is_parser_owned_and_covers_every_new_verb_and_export() {
    let help = actions::help_lines().join("\n");
    for needle in [
        "$census",
        "$crosscheck",
        "$survey",
        "$probe",
        "/export census",
        "/export crosscheck",
        "/export survey",
        "/export probe",
        "/continue",
        "/accept",
    ] {
        assert!(help.contains(needle), "help is missing {needle}");
    }
    assert_eq!(
        actions::help_lines().len(),
        squad_protocol::parser::help_examples().len()
    );
    assert!(matches!(actions::help(), Request::Help));
    assert!(matches!(actions::who(), Request::Who));
}

#[test]
fn every_help_export_example_parses_to_an_export_scope() {
    let ctx = ParseContext::default();
    for raw in squad_protocol::parser::HELP_EXAMPLES
        .iter()
        .filter(|r| r.starts_with("/export"))
    {
        let parsed = squad_protocol::parse(raw, &SubmissionMeta::default(), &ctx).unwrap();
        let ParsedInput::Slash {
            action: squad_protocol::parser::SlashAction::Export { scope, .. },
            ..
        } = parsed
        else {
            panic!("{raw}")
        };
        assert!(actions::EXPORT_SCOPES.contains(&scope));
    }
}

#[tokio::test]
async fn export_scopes_name_files_and_new_reports_are_flagged() {
    let mut expected = vec!["census", "crosscheck", "survey", "probe"];
    expected.extend(
        [
            ContestKind::Forge,
            ContestKind::Forecast,
            ContestKind::Decide,
            ContestKind::Negotiate,
            ContestKind::BlowItUp,
            ContestKind::Readback,
            ContestKind::Debugging,
            ContestKind::Trace,
            ContestKind::Rank,
            ContestKind::Tune,
            ContestKind::BassAckwardsPrd,
        ]
        .into_iter()
        .filter(|kind| kind.is_built())
        .map(ContestKind::as_str),
    );
    assert_eq!(actions::EXPORT_SCOPES.len(), 4 + expected.len());
    let new: Vec<_> = actions::EXPORT_SCOPES
        .iter()
        .filter(|s| actions::is_new_report_scope(s))
        .map(actions::scope_name)
        .collect();
    assert_eq!(new, expected);
    let id = ContestId::new();
    assert_eq!(
        actions::export_file_name(&ExportScope::Survey, Some(id)),
        format!("squad-survey-{id}.md")
    );
    assert_eq!(
        actions::export_file_name(&ExportScope::Room, None),
        "squad-room.md"
    );
    for kind in [
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        assert_eq!(
            actions::scope_name(&actions::scope_for(kind)),
            kind.as_str()
        );
    }
    // FakeRoom does not render scoped reports: failure is surfaced, never swallowed.
    let room = FakeRoom::new();
    let mut out = vec![];
    assert!(
        actions::export_report(&room, ContestKind::Census, id, &mut out)
            .await
            .is_err()
    );
    assert!(out.is_empty());
}

fn step(ordinal: u32, mv: ContestMove, n: usize) -> ContestStep {
    ContestStep {
        contest_id: ContestId::new(),
        step_id: StepId::new(),
        ordinal,
        script_version: Text::new("v1").unwrap(),
        move_name: mv,
        inputs: vec![],
        assignments: (0..n)
            .map(|i| ContestAssignment {
                target: Target::Member(Nick::new(&format!("m{i}")).unwrap()),
                context_id: ContextId(format!("m{i}")),
                payload_refs: vec![],
                work_id: None,
            })
            .collect(),
    }
}
fn work(id: WorkId, nick: &str, attempt: u32, status: WorkStatus) -> WorkStateEvent {
    WorkStateEvent {
        work_id: id,
        trigger_event_id: EventId::new(),
        target: Target::Member(Nick::new(nick).unwrap()),
        context_id: ContextId(nick.into()),
        attempt,
        status,
        error: None,
        debate_id: None,
        case_id: None,
        contest_id: None,
    }
}

#[test]
fn step_labels_visibility_and_stable_work_cards() {
    let s = step(1, ContestMove::Fanout, 3);
    assert_eq!(step_label(&s), "Step 1 · fan out · 3 assignments");
    assert_eq!(
        step_label(&step(4, ContestMove::Report, 0)),
        "Step 4 · report"
    );
    assert_eq!(
        step_label(&step(2, ContestMove::Tally, 1)),
        "Step 2 · tally · 1 assignment"
    );
    assert!(
        visibility_label(ContestKind::Census, &s)
            .unwrap()
            .starts_with("Blind")
    );
    assert!(
        visibility_label(ContestKind::Probe, &s)
            .unwrap()
            .contains("checkers")
    );
    assert!(visibility_label(ContestKind::Census, &step(3, ContestMove::Fanout, 1)).is_none());
    assert!(visibility_label(ContestKind::Debate, &s).is_none());

    let w = WorkId::new();
    let queued = WorkCard::from_event(&work(w, "grok", 1, WorkStatus::Queued));
    let retry = WorkCard::from_event(&work(w, "grok", 2, WorkStatus::Running));
    assert!(queued.accepts(&retry));
    assert!(
        !retry.accepts(&queued),
        "a stale attempt never regresses a card"
    );
    assert_eq!(retry.line(), "grok · running · retry 1");
    let lines = step_lines(ContestKind::Census, &s, &[&retry]);
    assert_eq!(lines[0], "Step 1 · fan out · 3 assignments");
    assert!(lines[1].starts_with("Blind"));
    assert_eq!(lines[2], "  grok · running · retry 1");
    assert_eq!(tally_progress(2, 5), "tally 2/5 applications committed");
}

#[test]
fn stopped_and_failed_runs_are_incomplete_and_name_skipped_members() {
    let g = |status: &str| {
        ContestState::Generic(GenericContestState {
            contest_id: ContestId::new(),
            kind: ContestKind::Survey,
            state_version: 1,
            status: Text::new(status).unwrap(),
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
        })
    };
    assert_eq!(outcome(&g("STOPPED")), RunOutcome::Stopped);
    assert_eq!(outcome(&g("FAILED")), RunOutcome::Failed);
    assert_eq!(outcome(&g("COMPLETE")), RunOutcome::Complete);
    assert_eq!(outcome(&g("RUNNING")), RunOutcome::Running);
    let skipped = vec!["grok".to_owned()];
    assert_eq!(
        report_header(RunOutcome::Stopped, &[]),
        "Stopped · incomplete report"
    );
    assert_eq!(
        report_header(RunOutcome::Failed, &skipped),
        "Failed · incomplete report · skipped: grok"
    );
    assert!(!report_header(RunOutcome::Failed, &[]).contains("Report complete"));
    assert_eq!(
        report_header(RunOutcome::Complete, &skipped),
        "Report complete · some members skipped · skipped: grok"
    );
}

#[test]
fn session_tracks_steps_and_cards_per_contest() {
    use squad_tui::fx::{FeedOrigin, FxQueue};
    use std::time::Duration;
    let room = FakeRoom::new();
    let cid = ContestId::new();
    let mut st = step(1, ContestMove::Fanout, 2);
    st.contest_id = cid;
    room.commit(Actor::System, EnvelopePayload::ContestStep(st));
    let w = WorkId::new();
    let mut e = work(w, "grok", 2, WorkStatus::Failed);
    e.contest_id = Some(cid);
    room.commit(Actor::System, EnvelopePayload::WorkState(e.clone()));
    // A late attempt-1 event must not overwrite the retry card.
    e.attempt = 1;
    e.status = WorkStatus::Running;
    room.commit(Actor::System, EnvelopePayload::WorkState(e));
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let snap = rt.block_on(squad_tui::session::RoomPort::state(&room));
    let mut s = squad_tui::session::Session::default();
    let mut q = FxQueue::new();
    let _ = (FeedOrigin::Replay, Duration::ZERO, &mut q);
    s.restore(snap);
    let v = &s.contests[&cid];
    assert_eq!(v.steps[&1].ordinal, 1);
    assert_eq!(v.work[&w].attempt, 2);
    assert_eq!(v.work[&w].status, WorkStatus::Failed);
}

#[test]
fn tournament_never_shows_a_substitution_warning() {
    use squad_tui::widgets::preflight::PreflightView;
    let mk = |kind, warning: &str| PreflightView {
        pending: false,
        plan: PendingPreflight::Contest(ContestPreflight {
            contest_id: ContestId::new(),
            kind,
            plan_id: PlanId::new(),
            expires_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            estimate: DebateEstimate {
                estimated_usd: 0.1,
                reserved_usd: 0.2,
                remaining_usd: 3.0,
                model_calls: 5,
            },
            participants: ["a", "b"].map(|n| Nick::new(n).unwrap()).to_vec(),
            confirmation_required: true,
            warning: Some(Text::new(warning).unwrap()),
            settings: serde_json::json!({}),
        }),
    };
    let sub = "Judge substituted: @gemini shares a model with @grok.";
    assert!(
        !mk(ContestKind::Tournament, sub)
            .text()
            .contains("substituted")
    );
    assert!(
        mk(ContestKind::Tournament, "Large estimate.")
            .text()
            .contains("Large estimate.")
    );
    assert!(
        mk(ContestKind::Crosscheck, sub)
            .text()
            .contains("substituted")
    );
}

#[test]
fn debug_diagnostics_are_redacted_codes_and_counters() {
    let mut d = Diagnostics::default();
    d.counter("draws", 12)
        .counter("feed items\nsk-secret", 3)
        .error(&ErrorCode::INVALID_COMMAND, Some(429));
    assert!(d.lines()[0] == "draws 12");
    assert_eq!(d.lines()[1], "feed_items_sk-secret 3");
    assert!(d.lines()[2].contains("429"));
    let area = Rect::new(0, 0, 40, 4);
    let mut buf = Buffer::empty(area);
    render_diagnostics(&d, area, &mut buf);
    assert!(
        buf.content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
            .contains("draws 12")
    );
}

#[test]
fn pick_hooks_expose_eligible_rows_and_text_is_complete_without_effects() {
    let cands: Vec<Candidate> = ["grok", "gemini", "claude"]
        .iter()
        .map(|n| Candidate {
            nick: (*n).into(),
            eligible: *n != "claude",
            status: "answered".into(),
            answer: String::new(),
        })
        .collect();
    let mut v = PickView::new(PickKind::Debate, "q?", cands);
    assert!(
        v.eligible_nicks().is_empty(),
        "nothing is eligible while collecting"
    );
    v.status = PickStatus::Pending;
    assert_eq!(v.eligible_nicks(), ["grok", "gemini"]);
    let area = Rect::new(0, 0, 60, 12);
    let mut buf = Buffer::empty(area);
    let rects = render(&v, area, &mut buf, &Palette::new(ColorTier::NoColor));
    let (banner, rows) = rects.effect_hooks();
    assert!(!banner.is_empty());
    assert_eq!(
        rows.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["grok", "gemini"]
    );
    let text: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert_eq!(
        text.matches("eligible").count(),
        2,
        "text carries eligibility without effects"
    );

    let p = ParticipantsRects {
        names: vec![
            ("grok".into(), Rect::new(0, 0, 10, 1)),
            ("claude".into(), Rect::new(0, 1, 10, 1)),
        ],
        ..Default::default()
    };
    assert_eq!(pick_candidate_rows(&p, &["grok".into()]).len(), 1);
    assert_eq!(pick_marker("grok", &["grok".into()]), "eligible to pick");
    assert_eq!(pick_marker("claude", &["grok".into()]), "");
}
