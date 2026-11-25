//! Catalogue painters attached to actual widget geometry after the static scene.
use crate::{
    app::App,
    fx::{self, CatalogueEffect, ColorTier, FxContext, FxCue, Pass, PassAccess},
    layout::ViewportLayout,
    widgets::transcript::TranscriptRects,
};
use ratatui::{
    layout::{Position, Rect},
    style::{Color, Style},
};
use std::time::Duration;

#[allow(clippy::too_many_arguments)]
pub fn attach(
    app: &mut App,
    context: &FxContext,
    layout: &ViewportLayout,
    transcript: Rect,
    roster: &crate::widgets::participants::ParticipantsRects,
    top: usize,
    messages: &TranscriptRects,
    meter: Rect,
) {
    let caps = app.fx.caps().clone();
    let level = caps.level;
    let no_color = caps.color == ColorTier::NoColor;
    let presentation = fx::debate::Presentation { level, no_color };
    app.composed
        .active_cues
        .extend(context.cues.iter().cloned());
    let now = app.now();
    app.composed
        .active_cues
        .retain(|cue| now.saturating_sub(cue.at) <= cue.cue.duration());
    let cues = app.composed.active_cues.clone();
    for cue in &cues {
        let palette = app.theme.palette.clone();
        let mut rect = layout.status;
        let key = cue.cue.key();
        let seed = u64::from(cue.rng_seed());
        let pass = match &cue.cue {
            FxCue::Catalogue {
                effect: CatalogueEffect::Chrome,
                ..
            } => {
                let chrome = fx::chrome::Chrome {
                    level,
                    no_color,
                    ansi16: caps.color == ColorTier::Ansi16,
                    gold: palette.chrome(),
                    rainbow: &fx::palette::SIGNATURES,
                    seed,
                };
                let roster_names = roster.names.clone();
                let logo = app.composed.logo;
                let area = Rect::new(0, 0, caps.size.width, caps.size.height);
                Pass::catalogue(
                    key,
                    area,
                    PassAccess::Unprotected,
                    cue.cue.duration(),
                    move |elapsed, buf, _| {
                        chrome.logo(buf, rect, elapsed, &[]);
                        chrome.logo(buf, logo, elapsed, &[]);
                        for (i, (name, row)) in roster_names.iter().enumerate() {
                            chrome.roster(buf, *row, name, false, palette.member(name, i), elapsed);
                        }
                    },
                )
            }
            FxCue::Catalogue {
                effect: CatalogueEffect::Dispatch,
                ..
            } => {
                let gutter = Rect::new(transcript.x, transcript.y, 1, transcript.height);
                let border = Rect::new(layout.input.x, layout.input.y, layout.input.width, 1);
                let terminal = Rect::new(0, 0, caps.size.width, caps.size.height);
                let targets = roster
                    .names
                    .iter()
                    .enumerate()
                    .map(|(i, (name, row))| fx::dispatch::Target {
                        row: *row,
                        hue: palette.member(name, i),
                    })
                    .collect::<Vec<_>>();
                let gold = palette.chrome();
                Pass::catalogue(
                    key,
                    terminal,
                    PassAccess::Unprotected,
                    Duration::from_millis(600),
                    move |elapsed, buf, _| {
                        fx::dispatch::DispatchEffect {
                            area: terminal,
                            kind: fx::dispatch::Dispatch::Launch { gutter, border },
                            gold,
                            level,
                            no_color,
                            terminal,
                            ansi16: caps.color == ColorTier::Ansi16,
                        }
                        .apply(elapsed, buf, &[]);
                        fx::dispatch::DispatchEffect {
                            area: terminal,
                            kind: fx::dispatch::Dispatch::Spokes {
                                origin: Position::new(gutter.x, gutter.bottom().saturating_sub(1)),
                                targets: &targets,
                            },
                            gold,
                            level,
                            no_color,
                            terminal,
                            ansi16: caps.color == ColorTier::Ansi16,
                        }
                        .apply(elapsed, buf, &[]);
                    },
                )
            }
            FxCue::RoundStarted { contest_id, n, .. }
            | FxCue::ScorePosted {
                contest_id,
                round: n,
            } => {
                let id: squad_protocol::MessageId = contest_id.to_string().parse().expect("UUID");
                let contest: squad_protocol::ContestId =
                    contest_id.to_string().parse().expect("UUID");
                let Some(view) = app.session.debates.get(&contest) else {
                    continue;
                };
                let Some(shapes) = app.composed.graphics.get(&id) else {
                    continue;
                };
                let colors = [
                    palette.member(view.state.debaters[0].as_str(), 0),
                    palette.member(view.state.debaters[1].as_str(), 1),
                ];
                if matches!(cue.cue, FxCue::ScorePosted { .. }) {
                    let Some(score) = app.session.contests[&contest].scores.get(n) else {
                        continue;
                    };
                    let scores = fx::debate::Scores {
                        a: score
                            .tally
                            .get(&view.state.debaters[0])
                            .copied()
                            .unwrap_or(0),
                        b: score
                            .tally
                            .get(&view.state.debaters[1])
                            .copied()
                            .unwrap_or(0),
                    };
                    let round_scores = fx::debate::Scores {
                        a: score
                            .scores
                            .as_ref()
                            .and_then(|s| s.get(&view.state.debaters[0]))
                            .map_or(0, |s| u32::from(s.score)),
                        b: score
                            .scores
                            .as_ref()
                            .and_then(|s| s.get(&view.state.debaters[1]))
                            .map_or(0, |s| u32::from(s.score)),
                    };
                    let previous = app.session.contests[&contest]
                        .scores
                        .range(..*n)
                        .next_back()
                        .map(|(_, s)| fx::debate::Scores {
                            a: s.tally.get(&view.state.debaters[0]).copied().unwrap_or(0),
                            b: s.tally.get(&view.state.debaters[1]).copied().unwrap_or(0),
                        })
                        .unwrap_or(fx::debate::Scores { a: 0, b: 0 });
                    let cells = shapes
                        .scores
                        .iter()
                        .filter(|(round, _, _)| round == n)
                        .map(|(_, nick, rect)| {
                            (
                                usize::from(nick == &view.state.debaters[1]),
                                app.composed.graphic_rect(id, *rect, top, transcript),
                            )
                        })
                        .filter(|(_, r)| !r.is_empty())
                        .collect::<Vec<_>>();
                    let tug = app.composed.graphic_rect(id, shapes.tug, top, transcript);
                    Pass::catalogue(
                        key,
                        transcript,
                        PassAccess::Unprotected,
                        Duration::from_millis(520),
                        move |elapsed, buf, _| {
                            let sampled = fx::debate::ScoreSlam {
                                scores: round_scores,
                                colors,
                            }
                            .sample(elapsed, presentation.level);
                            for (side, cell) in &cells {
                                let value = if *side == 0 { sampled.a } else { sampled.b };
                                buf.set_stringn(
                                    cell.x,
                                    cell.y,
                                    value.to_string(),
                                    usize::from(cell.width),
                                    Style::default().fg(colors[*side]),
                                );
                            }
                            fx::debate::TugBar {
                                previous,
                                current: scores,
                                colors,
                            }
                            .draw(tug, buf, elapsed, presentation);
                        },
                    )
                } else {
                    let kind = squad_core::banner::Banner::debate_round(
                        *n as usize,
                        view.state.total_rounds as usize,
                    );
                    let banner = shapes
                        .banners
                        .iter()
                        .find(|(k, _)| *k == kind)
                        .map(|(_, r)| *r)
                        .unwrap_or_default();
                    rect = app.composed.graphic_rect(id, banner, top, transcript);
                    let versus = app
                        .composed
                        .graphic_rect(id, shapes.versus, top, transcript);
                    let opening = *n == 0 || (*n == 1 && !view.rounds.iter().any(|r| r.round == 0));
                    Pass::catalogue(
                        key,
                        transcript,
                        PassAccess::Unprotected,
                        Duration::from_millis(1480),
                        move |elapsed, buf, _| {
                            tint_prepared(buf, rect, elapsed, colors[0], level);
                            if opening {
                                versus_entrance(buf, versus, elapsed, colors, level);
                            }
                        },
                    )
                }
            }
            FxCue::ModeBannerReady {
                contest_id,
                mode: fx::ReportMode::Survey,
            } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let local = app
                    .composed
                    .graphics
                    .get(&id)
                    .and_then(|g| g.banners.first())
                    .map(|(_, r)| *r)
                    .unwrap_or_default();
                rect = app.composed.graphic_rect(id, local, top, transcript);
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    Duration::from_millis(1900),
                    move |elapsed, buf, _| {
                        tint_prepared(buf, rect, elapsed, palette.chrome(), level)
                    },
                )
            }
            FxCue::SurveyTallyReady { contest_id, .. } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let contest = contest_id.to_string().parse().expect("UUID");
                let Some(view) = app
                    .session
                    .contests
                    .get(&contest)
                    .and_then(|c| c.survey.as_ref())
                else {
                    continue;
                };
                let Some(shapes) = app.composed.graphics.get(&id) else {
                    continue;
                };
                let graphics = shapes
                    .survey
                    .iter()
                    .filter_map(|(claim, r)| {
                        let m = view.tallies().iter().find(|m| &m.claim_id == claim)?;
                        let rect = app.composed.graphic_rect(id, *r, top, transcript);
                        let effect = fx::survey::SurveyEffect::new(
                            fx::survey::Motion::new(level, no_color, false),
                            seed,
                            view.report()
                                .map(|_| crate::widgets::survey::votes(&m.first)),
                            crate::widgets::survey::votes(&m.final_tally),
                        );
                        Some((rect, effect))
                    })
                    .collect::<Vec<_>>();
                Pass::catalogue(
                    key,
                    transcript,
                    PassAccess::Unprotected,
                    Duration::from_millis(400),
                    move |elapsed, buf, _| {
                        for (rect, effect) in &graphics {
                            effect.render_graphic(
                                buf,
                                *rect,
                                elapsed,
                                rect.width < 48,
                                [Color::Green, Color::Red, Color::Yellow],
                            );
                        }
                    },
                )
            }
            FxCue::TournamentRankingReady { contest_id, round } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let Some(shapes) = app
                    .composed
                    .graphics
                    .get(&id)
                    .and_then(|s| s.tournament.as_ref())
                else {
                    continue;
                };
                let rows = shapes
                    .rankings
                    .iter()
                    .filter(|(r, _, _)| r == round)
                    .map(|(_, _, r)| app.composed.graphic_rect(id, *r, top, transcript))
                    .collect::<Vec<_>>();
                let effect = fx::tournament::RankingReveal::new(level, no_color, true);
                let duration = effect.duration(rows.len());
                Pass::catalogue(
                    key,
                    transcript,
                    PassAccess::Unprotected,
                    duration,
                    move |elapsed, buf, _| effect.render_prepared(buf, &rows, elapsed),
                )
            }
            FxCue::TournamentEliminated { contest_id, member } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let marker = app
                    .composed
                    .graphics
                    .get(&id)
                    .and_then(|s| s.tournament.as_ref())
                    .and_then(|s| s.eliminations.iter().find(|(n, _)| n == member.as_str()))
                    .map(|(_, r)| app.composed.graphic_rect(id, *r, top, transcript))
                    .unwrap_or_default();
                let mut fx = fx::tournament::TournamentFx::default();
                let Some(effect) =
                    fx.elimination(&cue.event_id.to_string(), true, true, level, no_color)
                else {
                    continue;
                };
                Pass::catalogue(
                    key,
                    marker,
                    PassAccess::Unprotected,
                    effect.duration(),
                    move |elapsed, buf, _| effect.render(buf, marker, elapsed),
                )
            }
            FxCue::TournamentChampion { contest_id, member } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let marker = app
                    .composed
                    .graphics
                    .get(&id)
                    .and_then(|s| s.tournament.as_ref())
                    .map(|s| app.composed.graphic_rect(id, s.champion, top, transcript))
                    .unwrap_or_default();
                let effect = fx::tournament::ChampionReveal::new(
                    level,
                    no_color,
                    true,
                    u64::from(cue.rng_seed()),
                );
                let winner = fx::verdict::Contender {
                    nick: member.to_string(),
                    hue: palette.member(member.as_str(), 0),
                    art: vec![],
                    compact_art: vec![],
                };
                Pass::catalogue(
                    key,
                    marker,
                    PassAccess::Unprotected,
                    Duration::from_millis(6500),
                    move |elapsed, buf, _| effect.render(buf, marker, &[], elapsed, &winner),
                )
            }
            FxCue::VerdictReady { contest_id } => {
                let contest: squad_protocol::ContestId =
                    contest_id.to_string().parse().expect("UUID");
                let winner = app
                    .session
                    .contests
                    .get(&contest)
                    .and_then(|c| c.verdict.as_ref())
                    .and_then(|v| {
                        let id = v.completed.generation.message_id;
                        app.composed.graphics.get(&id).map(|s| {
                            (
                                app.composed.graphic_rect(id, s.winner, top, transcript),
                                palette.member(v.verdict.verdict.winner.as_str(), 0),
                            )
                        })
                    });
                let Some((rect, hue)) = winner else {
                    continue;
                };
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    Duration::from_millis(850),
                    move |elapsed, buf, _| tint_prepared(buf, rect, elapsed, hue, level),
                )
            }
            FxCue::Catalogue {
                effect: CatalogueEffect::Verdict,
                target,
                ..
            } => {
                let Ok(contest) = target.parse::<squad_protocol::ContestId>() else {
                    continue;
                };
                let Some(value) = app
                    .session
                    .contests
                    .get(&contest)
                    .and_then(|c| c.verdict.as_ref())
                else {
                    continue;
                };
                let Some(state) = app.session.debates.get(&contest) else {
                    continue;
                };
                let id = value.completed.generation.message_id;
                let Some(shapes) = app.composed.graphics.get(&id) else {
                    continue;
                };
                rect = app
                    .composed
                    .graphic_rect(id, shapes.verdict, top, transcript);
                let winner_rect = app
                    .composed
                    .graphic_rect(id, shapes.winner, top, transcript);
                let cells = shapes
                    .totals
                    .iter()
                    .map(|r| app.composed.graphic_rect(id, *r, top, transcript))
                    .collect::<Vec<_>>();
                let totals = state.state.debaters.each_ref().map(|nick| {
                    value
                        .verdict
                        .weighted_totals
                        .iter()
                        .find(|t| &t.nick == nick)
                        .map_or(0., |t| t.total)
                });
                let colors = state.state.debaters.each_ref().map(|n| {
                    palette.member(n.as_str(), usize::from(n == &state.state.debaters[1]))
                });
                let winner = &value.verdict.verdict.winner;
                let slot = usize::from(winner == &state.state.debaters[1]);
                let hue = palette.member(winner.as_str(), slot);
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    Duration::from_millis(1800),
                    move |elapsed, buf, _| {
                        reveal_decision(
                            buf,
                            &cells,
                            totals,
                            colors,
                            winner_rect,
                            hue,
                            elapsed,
                            level,
                        );
                    },
                )
            }

            FxCue::BubbleOpened { id, .. }
            | FxCue::BubbleCompleted { id }
            | FxCue::BubbleFailed { id } => {
                let Some((_, visible)) = messages.cards.iter().find(|(message, _)| message == id)
                else {
                    continue;
                };
                let card = &app.session.transcript.cards[id];
                let hue = match &card.author {
                    squad_protocol::Actor::Member(n) => palette.member(n.as_str(), 0),
                    _ => palette.chrome(),
                };
                let rail = Rect::new(visible.x, visible.y, 1, visible.height);
                let footer = Rect::new(rail.x, rail.bottom().saturating_sub(1), 1, 1);
                let moment = if matches!(cue.cue, FxCue::BubbleOpened { .. }) {
                    fx::bubble::Moment::Arrival
                } else {
                    fx::bubble::Moment::Complete
                };
                let bubble = fx::bubble::Bubble {
                    area: rail,
                    rail,
                    header: rail,
                    footer,
                    cursor: Some(Position::new(footer.x, footer.y)),
                    tail: [None; 4],
                    status: Rect::default(),
                    hue,
                    text: Color::Reset,
                    error: Color::Red,
                    level,
                    no_color,
                    moment,
                };
                Pass::catalogue(
                    key,
                    rail,
                    PassAccess::Unprotected,
                    Duration::from_millis(280),
                    move |elapsed, buf, _| bubble.apply(elapsed, buf, &[]),
                )
            }
            FxCue::ModeBannerReady {
                contest_id,
                mode: fx::ReportMode::Census,
            } => {
                let id: squad_protocol::MessageId = contest_id.to_string().parse().expect("UUID");
                let Some(shapes) = app.composed.graphics.get(&id) else {
                    continue;
                };
                let Some((_, banner)) = shapes.banners.first() else {
                    continue;
                };
                let rect = app.composed.graphic_rect(id, *banner, top, transcript);
                let entrance = fx::banner::Entrance::new(
                    &mut fx::banner::BannerCache::default(),
                    squad_core::banner::Banner::Census,
                    squad_core::banner::BannerMode::Off,
                    rect.width,
                    level,
                    fx::banner::Palette {
                        gold: palette.chrome(),
                        tier: caps.color,
                        ..Default::default()
                    },
                );
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    entrance.duration(),
                    move |elapsed, buf, _| entrance.render(buf, rect, elapsed, &[]),
                )
            }
            FxCue::CensusEstimateReady { contest_id, .. } => {
                let id: squad_protocol::MessageId = contest_id.to_string().parse().expect("UUID");
                let Some(shapes) = app.composed.graphics.get(&id) else {
                    continue;
                };
                let rect = app
                    .composed
                    .graphic_rect(id, shapes.census_estimate, top, transcript);
                let effect = fx::census::CensusEffect::new(
                    fx::census::Motion::new(level, no_color, false),
                    seed,
                    None,
                    None,
                );
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    effect.duration(),
                    move |elapsed, buf, _| {
                        effect.render_graphic(buf, rect, elapsed, rect.width < 60, palette.chrome())
                    },
                )
            }
            FxCue::CrosscheckCellsReady {
                contest_id,
                artifact_id,
            } => {
                let id: squad_protocol::MessageId = contest_id.to_string().parse().expect("UUID");
                let committed = app
                    .session
                    .snapshot
                    .events
                    .iter()
                    .find(|e| e.base.id == *artifact_id);
                let selected = committed
                    .map(|e| match &e.content {
                        squad_protocol::EnvelopePayload::CrosscheckReview(r) => r
                            .response
                            .cells
                            .iter()
                            .map(|c| c.cell_id)
                            .collect::<Vec<_>>(),
                        squad_protocol::EnvelopePayload::ContestArtifact(a)
                            if a.artifact_kind.as_str() == "crosscheck.cell" =>
                        {
                            squad_protocol::validation::decode::<
                                squad_protocol::CrosscheckReviewResponse,
                            >(&a.data)
                            .map(|r| r.cells.into_iter().map(|c| c.cell_id).collect())
                            .unwrap_or_default()
                        }
                        squad_protocol::EnvelopePayload::CrosscheckReport(r) => {
                            r.ledger.iter().map(|c| c.cell.cell_id).collect()
                        }
                        _ => vec![],
                    })
                    .unwrap_or_default();
                let cells = app
                    .composed
                    .graphics
                    .get(&id)
                    .map(|g| {
                        g.crosscheck_cells
                            .iter()
                            .filter(|(cell, _, _)| selected.contains(cell))
                            .map(|(_, index, rect)| fx::crosscheck::CellAccent {
                                grid_index: *index,
                                rect: app.composed.graphic_rect(id, *rect, top, transcript),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let summary = messages
                    .cards
                    .iter()
                    .find(|(message, _)| message == &id)
                    .map(|(_, r)| Rect::new(r.x, r.y, r.width, 1));
                let narrow = (transcript.width < 40).then_some(summary).flatten();
                let effect = fx::crosscheck::CrosscheckEffect::new(
                    fx::crosscheck::Motion::new(level, no_color, false),
                    seed,
                    cells,
                    palette.chrome(),
                );
                Pass::catalogue(
                    key,
                    transcript,
                    PassAccess::Unprotected,
                    cue.cue.duration(),
                    move |elapsed, buf, _| effect.apply(buf, elapsed, transcript, narrow),
                )
            }
            FxCue::ModeBannerReady { contest_id, .. } => {
                let id: squad_protocol::MessageId = contest_id.to_string().parse().expect("UUID");
                let Some((_, visible)) = messages.cards.iter().find(|(message, _)| message == &id)
                else {
                    continue;
                };
                let area = Rect::new(visible.x, visible.y, visible.width, 1);
                let gold = palette.chrome();
                Pass::catalogue(
                    key,
                    area,
                    PassAccess::Unprotected,
                    cue.cue.duration(),
                    move |elapsed, buf, _| tint_prepared(buf, area, elapsed, gold, level),
                )
            }
            FxCue::DailySpendChanged => {
                let gold = palette.chrome();
                Pass::catalogue(
                    key,
                    meter,
                    PassAccess::Unprotected,
                    Duration::from_millis(400),
                    move |elapsed, buf, _| {
                        if elapsed.as_millis() < 200 {
                            buf.set_style(meter, Style::default().fg(gold));
                        }
                    },
                )
            }
            _ => continue,
        };
        app.fx.reflow_pass(cue, pass);
    }
}

/// Style the prepared scene: glyphs and logical row offsets remain exactly those
/// used by static composition, including partially visible banners/cards.
fn tint_prepared(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    elapsed: Duration,
    color: Color,
    level: fx::FxLevel,
) {
    if level == fx::FxLevel::Off || elapsed >= Duration::from_millis(850) {
        return;
    }
    let clip = area.intersection(buf.area);
    for y in clip.y..clip.bottom() {
        for x in clip.x..clip.right() {
            if elapsed < Duration::from_millis(150) {
                buf[(x, y)].modifier.insert(ratatui::style::Modifier::DIM);
            } else if elapsed < Duration::from_millis(400) {
                buf[(x, y)].set_fg(color);
            }
        }
    }
}

/// Count the accepted totals into their own cells, then land the winner accent.
#[allow(clippy::too_many_arguments)]
fn reveal_decision(
    buf: &mut ratatui::buffer::Buffer,
    cells: &[Rect],
    totals: [f64; 2],
    colors: [Color; 2],
    winner: Rect,
    hue: Color,
    elapsed: Duration,
    level: fx::FxLevel,
) {
    if level == fx::FxLevel::Off {
        return;
    }
    let ms = elapsed.as_millis() as f64;
    let duration = if level == fx::FxLevel::Lite {
        400.
    } else {
        900.
    };
    let progress = (ms / duration).clamp(0., 1.);
    let eased = 1. - (1. - progress).powi(3);
    for (i, cell) in cells.iter().enumerate().take(2) {
        if cell.is_empty() {
            continue;
        }
        buf.set_stringn(
            cell.x,
            cell.y,
            format!("{:.2}", totals[i] * eased),
            cell.width as usize,
            Style::default()
                .fg(colors[i])
                .add_modifier(ratatui::style::Modifier::BOLD),
        );
    }
    let style = if progress < 1. {
        Style::default().add_modifier(ratatui::style::Modifier::DIM)
    } else if ms < duration + 350. {
        Style::default()
            .fg(hue)
            .add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED)
    } else {
        Style::default()
            .fg(hue)
            .add_modifier(ratatui::style::Modifier::BOLD)
    };
    buf.set_style(winner.intersection(buf.area), style);
}

/// A single opposing accent sweep over the compact matchup. It settles without
/// changing transcript geometry or moving the topic under the reader.
fn versus_entrance(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    elapsed: Duration,
    colors: [Color; 2],
    level: fx::FxLevel,
) {
    if level == fx::FxLevel::Off || elapsed >= Duration::from_millis(700) {
        return;
    }
    let area = area.intersection(buf.area);
    let progress = (elapsed.as_secs_f32() / 0.28).min(1.);
    let reach = (f32::from(area.width) * 0.5 * progress) as u16;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let side = usize::from(x >= area.x + area.width / 2);
            let distance = if side == 0 {
                x - area.x
            } else {
                area.right() - 1 - x
            };
            if distance <= reach {
                buf[(x, y)]
                    .set_fg(colors[side])
                    .modifier
                    .insert(ratatui::style::Modifier::BOLD);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verdict_counts_only_score_cells_then_accents_winner() {
        let mut original = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 40, 8));
        original.set_string(2, 1, "Winner: @claude", Style::default().fg(Color::Yellow));
        original.set_string(2, 2, "claude 8.00 : 7.00 openai", Style::default());
        original.set_string(2, 4, "Reasoning stays still", Style::default());
        let cells = [Rect::new(9, 2, 4, 1), Rect::new(16, 2, 4, 1)];
        let winner = Rect::new(2, 1, 14, 1);
        let sample = |ms, level| {
            let mut buf = original.clone();
            reveal_decision(
                &mut buf,
                &cells,
                [8., 7.],
                [Color::Yellow, Color::Green],
                winner,
                Color::Yellow,
                Duration::from_millis(ms),
                level,
            );
            buf
        };
        let early = sample(0, fx::FxLevel::Full);
        assert_eq!(early[(9, 2)].symbol(), "0");
        let landed = sample(1000, fx::FxLevel::Full);
        assert_eq!(landed[(9, 2)].symbol(), "8");
        assert!(
            landed[(2, 1)]
                .modifier
                .contains(ratatui::style::Modifier::REVERSED)
        );
        assert_eq!(landed[(2, 4)], original[(2, 4)]);
        assert_eq!(sample(0, fx::FxLevel::Off), original);
    }
    #[test]
    fn clipped_entrances_preserve_prepared_glyphs_and_neighbors() {
        let mut original = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 80, 24));
        original.set_string(18, 3, "ROUND 2 OF 3", Style::default());
        original.set_string(18, 4, "claude W4 L2  VS  openai W3 L1", Style::default());
        original.set_string(18, 5, "A topic from committed setup", Style::default());
        for ms in [0, 100, 200, 399, 850] {
            let mut frame = original.clone();
            tint_prepared(
                &mut frame,
                Rect::new(18, 3, 62, 3),
                Duration::from_millis(ms),
                Color::Yellow,
                fx::FxLevel::Full,
            );
            for (before, after) in original.content.iter().zip(&frame.content) {
                assert_eq!(before.symbol(), after.symbol());
            }
            assert_eq!(original[(0, 0)], frame[(0, 0)]);
        }
    }
}
