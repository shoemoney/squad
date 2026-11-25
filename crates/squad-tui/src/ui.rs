//! Complete static shell. Region adapters are intentionally thin so the widget
//! workers can replace them without changing application or scheduler state.
use crate::{
    app::App,
    fx::{FxCue, Pass, PassAccess},
    layout,
    session::{ContestView, DailySpendView, PickView, RoomSnapshot},
    transcript::Transcript,
    widgets::{
        completion,
        input::InputWidget,
        preflight::{self, PreflightView},
    },
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
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
    let mut context = app.fx_context(vec![geometry.transcript, geometry.input, geometry.spend]);
    app.fx.pre(&mut context);
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(" SQUAD   /   main")
        .style(theme.chrome())
        .render(geometry.status, buffer);
    render_budget(&app.session.snapshot.daily_spend, geometry.spend, buffer);
    render_participants(&app.session.snapshot, geometry.participants, buffer);
    render_transcript(&app.session.transcript, geometry.transcript, buffer);
    // Contest state is kept independently from message cards. This temporary
    // adapter shows the latest contest at the bottom of the transcript region.
    if let Some(view) = app.session.contests.values().next_back() {
        let h = geometry
            .transcript
            .height
            .min(if view.verdict.is_some() { 6 } else { 3 });
        render_contest(
            view,
            Rect::new(
                geometry.transcript.x,
                geometry.transcript.bottom() - h,
                geometry.transcript.width,
                h,
            ),
            buffer,
        );
    }
    if let Some(pick) = &app.session.pick {
        render_pick(
            pick,
            Rect::new(
                geometry.transcript.x,
                geometry.transcript.y,
                geometry.transcript.width,
                3,
            ),
            buffer,
        );
    }
    frame.render_stateful_widget(InputWidget, geometry.input, &mut app.input);
    if app.focus_input
        && let Some((x, y)) = app.input.cursor_in(geometry.input)
    {
        frame.set_cursor_position((x, y));
    }
    let footer = if app.awaiting_ack {
        format!(
            "Waiting for acknowledgement… {}",
            app.pending_updates
                .iter()
                .map(|v| format!("{}={} pending", v.name.as_str(), v.value))
                .collect::<Vec<_>>()
                .join(" · ")
        )
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
    } else if let Ok(valid) = &app.preview {
        format!(
            "To {} · quotes {} · F6 Focus · Tab Complete · Esc Stop",
            valid.recipients.join(" "),
            app.quotes.ids.len()
        )
    } else {
        "F6 Focus · Tab Complete · Ctrl+J Newline · Ctrl+R Recipients · Esc Stop · Ctrl+C Quit"
            .into()
    };
    Paragraph::new(footer)
        .style(Style::default().add_modifier(Modifier::DIM))
        .render(geometry.help, frame.buffer_mut());
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
    // Attach bounded catalogue painters only after pre admits a fresh live cue.
    // Geometry and the complete static state already exist at this point.
    for cue in &context.cues {
        let rect = match cue.cue {
            FxCue::DailySpendChanged => geometry.spend,
            _ => geometry.status,
        };
        let gold = theme.palette.chrome();
        let pass = Pass::catalogue(
            cue.cue.key(),
            rect,
            PassAccess::Unprotected,
            cue.cue.duration(),
            move |elapsed, buf, _| {
                if elapsed < Duration::from_millis(180) {
                    buf.set_style(rect, Style::default().fg(gold).add_modifier(Modifier::BOLD));
                }
            },
        );
        app.fx.attach_pass(cue, pass);
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
    }
}

pub fn render_budget(view: &DailySpendView, rect: Rect, buffer: &mut Buffer) {
    Paragraph::new(view.label()).render(rect, buffer);
}
pub fn render_participants(view: &RoomSnapshot, rect: Rect, buffer: &mut Buffer) {
    let lines = view
        .context
        .roster
        .iter()
        .map(|nick| {
            let status = view
                .participants
                .iter()
                .find(|p| &p.nick == nick)
                .map(|p| format!("{:?}", p.status))
                .unwrap_or_else(|| "Idle".into());
            Line::from(format!(" {nick} · {status}"))
        })
        .collect::<Vec<_>>();
    Paragraph::new(lines)
        .block(Block::default().title("ROOM").borders(Borders::RIGHT))
        .render(rect, buffer);
}
pub fn render_transcript(view: &Transcript, rect: Rect, buffer: &mut Buffer) {
    let mut lines = vec![];
    for card in view.cards.values() {
        let author = match &card.author {
            squad_protocol::Actor::Member(n) => n.to_string(),
            squad_protocol::Actor::Me => "me".into(),
            squad_protocol::Actor::Judge => "judge".into(),
            squad_protocol::Actor::System => "system".into(),
        };
        lines.push(Line::from(Span::styled(
            format!(" {author}{}", if card.complete { "" } else { " •" }),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for row in crate::text::wrap(&card.text, usize::from(rect.width.saturating_sub(2)).max(1)) {
            lines.push(Line::from(format!(
                " {}",
                row.cells
                    .iter()
                    .map(|c| c.display.as_str())
                    .collect::<String>()
            )));
        }
        lines.push(Line::default());
    }
    let skip = lines.len().saturating_sub(usize::from(rect.height));
    Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()).render(rect, buffer);
}
pub fn render_contest(view: &ContestView, rect: Rect, buffer: &mut Buffer) {
    let text = if let Some(v) = &view.verdict {
        format!(
            "VERDICT · {} wins\n{}",
            v.verdict.verdict.winner, v.verdict.verdict.reasoning
        )
    } else if let Some(d) = &view.debate {
        format!(
            "{} vs {} · Round {}/{} · {:?}",
            d.debaters[0], d.debaters[1], d.round, d.total_rounds, d.state
        )
    } else {
        format!("{:?}", view.state)
    };
    Clear.render(rect, buffer);
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::TOP))
        .render(rect, buffer);
}
pub fn render_pick(view: &PickView, rect: Rect, buffer: &mut Buffer) {
    Clear.render(rect, buffer);
    Paragraph::new(format!(
        "{}\nPick {}: {}",
        view.request.banner,
        view.request.required_picks,
        view.request
            .eligible
            .iter()
            .map(|n| format!("@{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    ))
    .render(rect, buffer);
}
