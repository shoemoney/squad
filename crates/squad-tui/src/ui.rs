//! Complete static shell. Region adapters are intentionally thin so the widget
//! workers can replace them without changing application or scheduler state.
use crate::{
    app::App,
    fx::FxCue,
    layout,
    widgets::{
        completion,
        input::InputWidget,
        preflight::{self, PreflightView},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::time::Duration;

pub fn render(app: &mut App, frame: &mut Frame<'_>, now: Duration) {
    let area = frame.area();
    let geometry = layout::with_input_rows(area, app.input.desired_height(area.width));
    let theme = app.theme.clone();
    if geometry.viewport == layout::Viewport::TooSmall {
        crate::screens::terminal_too_small(
            area,
            frame.buffer_mut(),
            &app.session.snapshot.daily_spend.label(),
        );
        app.fx.settle();
        return;
    }
    app.composed
        .prepare(&app.session, geometry.transcript.width, &theme.palette);
    let top = if app.session.transcript.follow {
        app.composed
            .transcript
            .total_rows()
            .saturating_sub(usize::from(geometry.transcript.height.saturating_sub(1)))
    } else {
        app.composed
            .anchor
            .and_then(|anchor| app.composed.transcript.row_for_anchor(anchor))
            .or_else(|| {
                app.session
                    .transcript
                    .anchor
                    .and_then(|(id, source_grapheme)| {
                        app.composed.transcript.row_for_anchor(
                            crate::widgets::transcript::TranscriptAnchor {
                                id,
                                source_grapheme,
                                row_in_header: None,
                            },
                        )
                    })
            })
            .unwrap_or(0)
    };
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(" SQUAD   /   main   ·   six minds, one room")
        .style(theme.quiet_chrome())
        .render(geometry.status, buffer);
    let spend = &app.session.snapshot.daily_spend;
    let budget = crate::widgets::budget::DailySpendView {
        day: spend.day.clone(),
        actual: Some((spend.spent_usd * 1_000_000.) as u64),
        pending: Some((spend.reserved_usd * 1_000_000.) as u64),
        max: Some((spend.limit_usd * 1_000_000.) as u64),
        unknown_calls: spend.unknown_calls,
        refusal: None,
    };
    let spend_area = geometry.spend;
    let budget_rects = crate::widgets::budget::render_effect(
        &budget,
        spend_area,
        buffer,
        &theme.palette,
        app.fx.caps().level,
        now,
    );
    let mut transcript_area = geometry.transcript;
    let participants_area = geometry.participants;
    let active = app.session.debates.values().find(|v| {
        matches!(
            v.state.state,
            squad_protocol::DebateStatus::RoundRunning
                | squad_protocol::DebateStatus::RoundCommitting
                | squad_protocol::DebateStatus::Judging
        )
    });
    if let Some(view) = active {
        Paragraph::new(format!(
            "{} vs {} · {}{}",
            view.state.debaters[0],
            view.state.debaters[1],
            crate::widgets::debate::status_label(&view.state),
            view.tally.as_ref().map_or(String::new(), |t| format!(
                " · total {} : {}",
                t.get(&view.state.debaters[0]).copied().unwrap_or(0),
                t.get(&view.state.debaters[1]).copied().unwrap_or(0)
            ))
        ))
        .style(theme.event_accent())
        .render(
            Rect::new(
                transcript_area.x,
                transcript_area.y,
                transcript_area.width,
                1,
            ),
            buffer,
        );
        transcript_area.y += 1;
        transcript_area.height = transcript_area.height.saturating_sub(1);
    }
    let top = if app.session.transcript.follow {
        app.composed
            .transcript
            .total_rows()
            .saturating_sub(usize::from(transcript_area.height))
    } else {
        top
    };
    let members = participant_models(app);
    let roster = crate::widgets::participants::render(
        &crate::widgets::participants::ParticipantsView {
            members: &members,
            palette: &theme.palette,
            scroll: app.composed.participants.scroll,
        },
        participants_area,
        buffer,
    );
    let transcript = crate::widgets::transcript::render(
        &app.composed.transcript,
        top,
        transcript_area,
        buffer,
        &theme.palette,
    );
    app.composed.top = top;
    app.composed.viewport = transcript_area;
    app.composed.roster_area = participants_area;
    app.composed.roster_rows = roster.total_rows;
    app.composed.logo = Rect::default();
    if app.composed.transcript.is_empty() {
        let art = app.composed.banners.get(
            "SQUAD",
            squad_core::banner::BannerMode::On,
            transcript_area.width,
        );
        let height = (art.height() as u16).min(transcript_area.height);
        let rect = Rect::new(
            transcript_area.x,
            transcript_area.y + transcript_area.height.saturating_sub(height) / 2,
            transcript_area.width,
            height,
        );
        crate::widgets::banner::render_art(&art, rect, buffer, &theme.palette);
        app.composed.logo = rect;
    }
    let mut protected = transcript.protected.clone();
    protected.extend(roster.protected.iter().copied());
    protected.push(geometry.input);
    protected.push(budget_rects.labels);
    protected.push(budget_rects.amounts);
    let mut graphics = roster.names.iter().map(|(_, r)| *r).collect::<Vec<_>>();
    for (id, shapes) in &app.composed.graphics {
        for rect in shapes
            .banners
            .iter()
            .map(|(_, r)| *r)
            .chain([shapes.versus, shapes.tug, shapes.verdict])
            .chain(shapes.scores.iter().map(|(_, _, r)| *r))
        {
            let rect = app.composed.graphic_rect(*id, rect, top, transcript_area);
            if !rect.is_empty() {
                graphics.push(rect);
            }
        }
    }
    // Dedicated graphic cells are available to catalogue painters; prose stays protected.
    protected = protected
        .into_iter()
        .flat_map(|row| subtract_graphics(row, &graphics))
        .collect();
    if !app.composed.started {
        app.composed.started = true;
        app.fx.queue_mut().push(
            crate::fx::FeedOrigin::Live,
            crate::fx::TimedCue {
                event_id: squad_protocol::EventId::new(),
                at: now,
                cue: FxCue::Catalogue {
                    effect: crate::fx::CatalogueEffect::Chrome,
                    target: "startup".into(),
                    duration: Duration::from_millis(1100),
                },
            },
        );
    }
    let mut context = app.fx_context(protected);
    app.fx.pre(&mut context);
    crate::ui_fx::attach(
        app,
        &context,
        &geometry,
        transcript_area,
        &roster,
        top,
        &transcript,
        budget_rects.meter,
    );
    let editor = Rect::new(
        geometry.input.x,
        geometry.input.y + 1,
        geometry.input.width,
        geometry.input.height.saturating_sub(1),
    );
    let recipients = app
        .preview
        .as_ref()
        .map_or("squad".into(), |v| v.recipients.join(" "));
    let chrome = if app.focus_input {
        theme.chrome()
    } else {
        theme.quiet_chrome()
    };
    Paragraph::new(format!(
        "─ Input · To {recipients} · quotes {} {}",
        app.quotes.ids.len(),
        "─".repeat(usize::from(geometry.input.width))
    ))
    .style(chrome)
    .render(
        Rect::new(geometry.input.x, geometry.input.y, geometry.input.width, 1),
        frame.buffer_mut(),
    );
    frame.render_stateful_widget(InputWidget, editor, &mut app.input);
    if app.focus_input
        && let Some((x, y)) = app.input.cursor_in(editor)
    {
        frame.set_cursor_position((x, y));
    }
    app.session.transcript.sync_activity();
    let new_activity = app.session.transcript.new_activity();
    let footer = if app.search_open {
        format!(
            "{} · Enter Next · ↑ Prev · Esc Close",
            app.search.status()
        )
    } else if !app.session.transcript.follow && app.stop_confirmation.is_none() {
        if new_activity > 0 {
            format!(
                "↓ {new_activity} new · back to live · End   |   PgUp/PgDn Scroll · F7 Scores · F6 Focus"
            )
        } else {
            "↓ back to live · End   |   PgUp/PgDn Scroll · F7 Scores · F6 Focus".into()
        }
    } else if app.awaiting_ack {
        format!(
            "Waiting for acknowledgement… {}",
            app.pending_updates
                .iter()
                .map(|v| format!("{}={} pending", v.name.as_str(), v.value))
                .collect::<Vec<_>>()
                .join(" · ")
        )
    } else if app.stop_confirmation.is_some_and(|until| now < until) {
        "Press Esc again within 2 s to stop the active contest.".into()
    } else if let Some(notice) = &app.notice {
        notice.clone()
    } else if let Some(error) = &app.session.last_error {
        error.clone()
    } else if app.input.text().is_empty()
        && let Some(pick) = &app.session.pick
    {
        let pick = &pick.request;
        format!(
            "Pick {}: {} · other messages remain available",
            pick.required_picks,
            pick.eligible
                .iter()
                .map(|n| format!("@{n}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else if area.width < 100 {
        "F6 Focus · Tab Complete · Esc Stop · F7 Scores · F8 Models · Ctrl+C Quit".into()
    } else {
        "F6 Focus · Tab Complete · Ctrl+J Newline · Ctrl+R Recipients · F7 Scores · F8 Models · Esc Stop · Ctrl+C Quit".into()
    };
    let help_style = if !app.session.transcript.follow
        || app.awaiting_ack
        || app.stop_confirmation.is_some_and(|until| now < until)
        || app.session.pick.is_some()
    {
        theme.event_accent()
    } else {
        theme.quiet_chrome()
    };
    Paragraph::new(footer)
        .style(help_style)
        .render(geometry.help, frame.buffer_mut());
    if app.participant_details
        && let Some(member) = members.get(app.composed.participants.focused)
    {
        let details = Rect::new(
            transcript_area.x,
            transcript_area.y,
            transcript_area.width,
            transcript_area.height.min(12),
        );
        Clear.render(details, frame.buffer_mut());
        let mut lines = vec![ratatui::text::Line::raw(
            "Participant · ↑/↓ Choose · F8 Close",
        )];
        lines.extend(member.lines(&theme.palette));
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(ratatui::widgets::Block::bordered().title("Model details"))
            .render(details, frame.buffer_mut());
    }
    if !app.quotes.ids.is_empty() {
        let previews = app.quotes.previews(&app.session.snapshot.context);
        let text = previews
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "Quote {} · {} · {}",
                    i + 1,
                    p.author,
                    crate::markdown::sanitize(&p.text)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let height = (previews.len() as u16)
            .min(3)
            .min(geometry.transcript.height);
        let rect = Rect::new(
            geometry.transcript.x,
            geometry.input.y - height,
            geometry.transcript.width,
            height,
        );
        Clear.render(rect, frame.buffer_mut());
        Paragraph::new(text).render(rect, frame.buffer_mut());
    }
    if app.completion_open {
        let height = (app.completion.items.len().min(6) as u16).min(geometry.transcript.height);
        completion::render(
            &app.completion,
            Rect::new(
                geometry.input.x + 2,
                geometry.input.y - height,
                geometry.input.width.saturating_sub(2),
                height,
            ),
            frame.buffer_mut(),
        );
    }
    if let Some(plan) = app.session.snapshot.preflights.first() {
        let rect = Rect::new(
            geometry.transcript.x,
            geometry.transcript.y,
            geometry.transcript.width,
            9.min(geometry.transcript.height),
        );
        Clear.render(rect, frame.buffer_mut());
        preflight::render(
            &PreflightView {
                plan: plan.clone(),
                pending: app.awaiting_ack,
            },
            rect,
            frame.buffer_mut(),
        );
    }
    if let Some(text) = &app.transient {
        Clear.render(geometry.transcript, frame.buffer_mut());
        Paragraph::new(text.as_str())
            .wrap(Wrap { trim: false })
            .block(Block::default().title("Esc Close").borders(Borders::ALL))
            .render(geometry.transcript, frame.buffer_mut());
    }
    if let Some(path) = &mut app.export_path {
        Clear.render(geometry.transcript, frame.buffer_mut());
        Paragraph::new("Save Markdown export\n\nEnter a destination path below. Existing files are preserved.\nEnter Save · Esc Cancel").block(Block::default().borders(Borders::ALL).title("Export")).render(geometry.transcript, frame.buffer_mut());
        frame.render_stateful_widget(InputWidget, geometry.input, path);
        if let Some((x, y)) = path.cursor_in(geometry.input) {
            frame.set_cursor_position((x, y));
        }
    }
    if app.fx.caps().level != crate::fx::FxLevel::Off {
        for cursor in &transcript.cursors {
            let point = ratatui::layout::Position::new(cursor.x, cursor.y);
            let hue = frame.buffer_mut()[(transcript_area.x, cursor.y)].fg;
            let bubble = crate::fx::bubble::Bubble {
                area: transcript_area,
                rail: Rect::default(),
                header: Rect::default(),
                footer: Rect::default(),
                cursor: Some(point),
                tail: std::array::from_fn(|i| {
                    cursor
                        .x
                        .checked_sub(i as u16 + 1)
                        .filter(|x| *x > transcript_area.x)
                        .map(|x| ratatui::layout::Position::new(x, cursor.y))
                }),
                status: Rect::default(),
                hue,
                text: ratatui::style::Color::Reset,
                error: ratatui::style::Color::Red,
                level: app.fx.caps().level,
                no_color: theme.palette.tier == crate::fx::ColorTier::NoColor,
                moment: crate::fx::bubble::Moment::Streaming,
            };
            bubble.apply(now, frame.buffer_mut(), &context.protected);
        }
    }
    app.fx.post(frame.buffer_mut(), area, now);
    if app.debug_open || app.fx.debug_tag().is_some() {
        let debug_rect = if app.debug_open {
            geometry.transcript
        } else {
            geometry.status
        };
        if app.debug_open {
            Clear.render(debug_rect, frame.buffer_mut());
        }
        crate::widgets::debug::render(
            &crate::widgets::debug::DebugView {
                tag: app.fx.debug_tag(),
                draws: app.draws,
                feed_items: app.feed_items,
            },
            debug_rect,
            frame.buffer_mut(),
        );
        if app.debug_open {
            let mut details = app
                .session
                .debates
                .values()
                .map(|v| {
                    format!(
                        "Debate {} · {:?} · cases {:?}",
                        v.state.debate_id, v.state.state, v.state.case_ids
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            for id in app.session.cases.keys() {
                details.push_str(&format!("\nCase {id}"));
            }
            for id in app.session.contests.keys() {
                details.push_str(&format!("\nContest {id}"));
            }
            let mut detail_rect = debug_rect;
            detail_rect.y += 3;
            detail_rect.height = detail_rect.height.saturating_sub(3);
            Paragraph::new(details)
                .wrap(Wrap { trim: false })
                .render(detail_rect, frame.buffer_mut());
        }
    }
}

fn subtract_graphics(row: Rect, graphics: &[Rect]) -> Vec<Rect> {
    let mut pieces = vec![row];
    for graphic in graphics {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| {
                let overlap = piece.intersection(*graphic);
                if overlap.is_empty() {
                    return vec![piece];
                }
                let mut out = Vec::new();
                if overlap.x > piece.x {
                    out.push(Rect::new(
                        piece.x,
                        piece.y,
                        overlap.x - piece.x,
                        piece.height,
                    ));
                }
                if overlap.right() < piece.right() {
                    out.push(Rect::new(
                        overlap.right(),
                        piece.y,
                        piece.right() - overlap.right(),
                        piece.height,
                    ));
                }
                out
            })
            .collect();
    }
    pieces
}
fn participant_models(app: &App) -> Vec<crate::widgets::participants::ParticipantView> {
    app.session
        .snapshot
        .context
        .roster
        .iter()
        .enumerate()
        .map(|(slot, nick)| {
            let presence = app
                .session
                .snapshot
                .participants
                .iter()
                .find(|p| &p.nick == nick);
            let score = app
                .session
                .snapshot
                .scoreboard
                .iter()
                .find(|s| &s.nick == nick);
            let latest =
                app.session
                    .snapshot
                    .events
                    .iter()
                    .rev()
                    .find_map(|event| match &event.content {
                        squad_protocol::EnvelopePayload::MessageFinal(value)
                        | squad_protocol::EnvelopePayload::AgentReply(value)
                            if event.base.sender == squad_protocol::Actor::Member(nick.clone()) =>
                        {
                            Some((
                                value.completed.model.to_string(),
                                value.completed.usage.as_ref(),
                            ))
                        }
                        squad_protocol::EnvelopePayload::ProbeExplanation(v)
                            if event.base.sender == squad_protocol::Actor::Member(nick.clone()) =>
                        {
                            Some((v.completed.model.to_string(), v.completed.usage.as_ref()))
                        }
                        squad_protocol::EnvelopePayload::ProbeCheck(v)
                            if event.base.sender == squad_protocol::Actor::Member(nick.clone()) =>
                        {
                            Some((v.completed.model.to_string(), v.completed.usage.as_ref()))
                        }
                        squad_protocol::EnvelopePayload::ProbePrediction(v)
                            if event.base.sender == squad_protocol::Actor::Member(nick.clone()) =>
                        {
                            Some((v.completed.model.to_string(), v.completed.usage.as_ref()))
                        }
                        squad_protocol::EnvelopePayload::DebateTurn(turn)
                            if &turn.speaker == nick =>
                        {
                            if let squad_protocol::DebateTurnOutcome::Completed {
                                model,
                                usage,
                                ..
                            } = &turn.outcome
                            {
                                Some((model.to_string(), usage.as_ref()))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    });
            let model = app
                .session
                .snapshot
                .models
                .iter()
                .find(|m| m.nick == squad_protocol::Actor::Member(nick.clone()))
                .map(|m| m.resolved_slug.to_string())
                .unwrap_or_else(|| latest.as_ref().map_or(String::new(), |v| v.0.clone()));
            let version = crate::widgets::participants::short_version(&model);
            crate::widgets::participants::ParticipantView {
                nick: nick.to_string(),
                version,
                model,
                slot,
                presence: presence
                    .map_or(squad_protocol::PresenceStatus::Idle, |p| p.status.clone()),
                queue_depth: presence.map_or(0, |p| p.queue_depth),
                contest: app.session.probe_participant_status(nick),
                tokens: latest
                    .as_ref()
                    .and_then(|v| v.1)
                    .map(|u| u.input_tokens.saturating_add(u.output_tokens)),
                cost_usd: latest.as_ref().and_then(|v| v.1).and_then(|u| u.cost_usd),
                tokens_per_second: app.session.stream_rates.get(nick).copied(),
                wins: score.map_or(0, |s| s.wins as u32),
                losses: score.map_or(0, |s| s.losses as u32),
                trophies: app
                    .session
                    .snapshot
                    .trophies
                    .get(nick)
                    .copied()
                    .unwrap_or(0) as u32,
                eliminated: app.session.tournaments.values().any(|v| {
                    v.state != "COMPLETE"
                        && v.entrants
                            .iter()
                            .any(|e| e.nick == nick.as_str() && e.eliminated)
                }),
            }
        })
        .collect()
}
