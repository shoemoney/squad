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
                    let names = view.state.debaters.clone();
                    let entrance = fx::banner::Entrance::new(
                        &mut app.composed.banners,
                        kind,
                        squad_core::banner::BannerMode::On,
                        rect.width,
                        level,
                        fx::banner::Palette {
                            left: colors[0],
                            right: colors[1],
                            gold: palette.chrome(),
                            background: Color::Reset,
                            tier: caps.color,
                        },
                    );
                    let opening = *n <= 1;
                    Pass::catalogue(
                        key,
                        transcript,
                        PassAccess::Unprotected,
                        Duration::from_millis(1480),
                        move |elapsed, buf, _| {
                            entrance.render(buf, rect, elapsed, &[]);
                            if opening {
                                fx::debate::VersusCard {
                                    a: fx::debate::Contender {
                                        name: names[0].as_str(),
                                        wins: 0,
                                        losses: 0,
                                        color: colors[0],
                                    },
                                    b: fx::debate::Contender {
                                        name: names[1].as_str(),
                                        wins: 0,
                                        losses: 0,
                                        color: colors[1],
                                    },
                                    topic: "Durability × responsiveness",
                                    seed,
                                }
                                .draw(
                                    versus,
                                    buf,
                                    elapsed,
                                    presentation,
                                );
                            }
                        },
                    )
                }
            }
            FxCue::VerdictReady { contest_id } => {
                let id = contest_id.to_string().parse().expect("UUID");
                let rect = app
                    .composed
                    .graphics
                    .get(&id)
                    .and_then(|s| {
                        s.banners
                            .iter()
                            .find(|(k, _)| *k == squad_core::banner::Banner::Verdict)
                    })
                    .map(|(_, r)| app.composed.graphic_rect(id, *r, top, transcript))
                    .unwrap_or_default();
                let entrance = fx::banner::Entrance::new(
                    &mut app.composed.banners,
                    squad_core::banner::Banner::Verdict,
                    squad_core::banner::BannerMode::On,
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
                let local = app
                    .composed
                    .graphics
                    .get(&id)
                    .map_or(Rect::default(), |g| g.verdict);
                rect = app.composed.graphic_rect(id, local, top, transcript);
                let names = &state.state.debaters;
                let data = fx::verdict::Verdict {
                    contenders: std::array::from_fn(|i| fx::verdict::Contender {
                        nick: names[i].to_string(),
                        hue: palette.member(names[i].as_str(), i),
                        art: vec![names[i].to_string()],
                        compact_art: vec![names[i].to_string()],
                    }),
                    criteria: std::array::from_fn(|i| fx::verdict::Criterion {
                        name: format!(
                            "{:?}",
                            value.verdict.verdict.scores[0].criteria[i].criterion_id
                        ),
                        scores: std::array::from_fn(|side| {
                            value
                                .verdict
                                .verdict
                                .scores
                                .iter()
                                .find(|s| s.nick == names[side])
                                .map_or(0, |s| u32::from(s.criteria[i].score))
                        }),
                    }),
                    scale: 10,
                    totals: std::array::from_fn(|i| {
                        value
                            .verdict
                            .weighted_totals
                            .iter()
                            .find(|v| v.nick == names[i])
                            .map_or(0, |v| (v.total * 10.).round() as u32)
                    }),
                    winner: usize::from(value.verdict.verdict.winner == names[1]),
                    round_tally: std::array::from_fn(|i| {
                        value
                            .verdict
                            .round_tally
                            .get(&names[i])
                            .copied()
                            .unwrap_or(0)
                    }),
                    override_reason: None,
                    reasoning: value.verdict.verdict.reasoning.clone(),
                    steal: value.verdict.verdict.what_to_steal_from_loser.join(" · "),
                };
                let reveal = fx::verdict::VerdictReveal::new(level, no_color, true, seed);
                Pass::catalogue(
                    key,
                    rect,
                    PassAccess::Unprotected,
                    reveal.duration(),
                    move |elapsed, buf, _| reveal.render(buf, rect, rect, &[], elapsed, &data),
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
