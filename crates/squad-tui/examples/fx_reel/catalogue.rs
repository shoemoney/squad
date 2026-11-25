//! Deterministic canned fixtures shared by the reel, invariant tests and benchmark.
#![allow(dead_code)]
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::{Paragraph, Widget},
};
use squad_tui::fx::{
    verdict::{Contender, Criterion, Verdict},
    *,
};
use std::time::Duration;
pub fn canned() -> Verdict {
    Verdict {
        contenders: [
            Contender {
                nick: "claude".into(),
                hue: Color::Rgb(224, 130, 90),
                art: vec![
                    "  ____ _      _   _   _ ____  _____".into(),
                    " / ___| |    / \\ | | | |  _ \\| ____|".into(),
                    "| |___| |___/ _ \\| |_| | | | |  _|".into(),
                    " \\____|_____/_/ \\_\\___/|____/|_____|".into(),
                ],
                compact_art: vec![
                    "▛▀▘ ▌ ▄▀▄ ▌ ▌ ▛▀▄ ▛▀▘".into(),
                    "▙▄▖ ▙▄ ▌ ▌ ▙▄▌ ▙▄▀ ▙▄▖".into(),
                ],
            },
            Contender {
                nick: "gemini".into(),
                hue: Color::Rgb(91, 155, 255),
                art: vec!["G E M I N I".into()],
                compact_art: vec!["GEMINI".into()],
            },
        ],
        criteria: std::array::from_fn(|i| Criterion {
            name: ["fit", "feasibility", "originality", "survivedAttacks"][i].into(),
            scores: [[8, 7], [8, 9], [9, 7], [9, 8]][i],
        }),
        scale: 10,
        totals: std::array::from_fn(|side| {
            [[8, 7], [8, 9], [9, 7], [9, 8]]
                .iter()
                .map(|scores| scores[side])
                .sum()
        }),
        winner: 0,
        round_tally: [2, 1],
        override_reason: None,
        reasoning: "A focused plan, with a credible path through the hardest objections.".into(),
        steal: "Gemini's staged rollout and clear rollback criteria.".into(),
    }
}

pub const ALL: [CatalogueEffect; 18] = [
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
pub fn duration(effect: CatalogueEffect) -> Duration {
    Duration::from_millis(match effect {
        CatalogueEffect::Verdict => 6500,
        CatalogueEffect::Banner => 1900,
        CatalogueEffect::Chrome => 1100,
        CatalogueEffect::Particles => 2600,
        CatalogueEffect::Debate => 1480,
        CatalogueEffect::Tournament => 900,
        CatalogueEffect::Crosscheck | CatalogueEffect::Probe => 900,
        _ => 400,
    })
}
pub fn paint(effect: CatalogueEffect, b: &mut Buffer, t: Duration, level: FxLevel) {
    let a = b.area;
    if a.width < 20 || a.height < 10 {
        return;
    }
    let rect = Rect::new(a.x + 2, a.y + 2, a.width - 4, a.height - 4);
    let row = Rect::new(rect.x, rect.y, rect.width, 1);
    let colors = [palette::SIGNATURES[0], palette::SIGNATURES[3]];
    let gold = palette::GOLD;
    let p = debate::Presentation {
        level,
        no_color: false,
    };
    let motion = || grill::Motion::new(level, false, false);
    use CatalogueEffect::*;
    match effect {
        Chrome => {
            b.set_string(row.x, row.y, "SQUAD", Style::default().fg(gold));
            chrome::Chrome {
                level,
                no_color: false,
                ansi16: false,
                gold,
                rainbow: &palette::SIGNATURES,
                seed: 42,
            }
            .logo(b, Rect { width: 5, ..row }, t, &[]);
        }
        Status => {
            let indicator = status::StatusStyle {
                level,
                no_color: false,
                ansi16: false,
                hue: colors[0],
            }
            .thinking(t);
            b.set_string(row.x, row.y, indicator.text, indicator.style);
        }
        Meter => meter::Meter {
            level,
            no_color: false,
            palette: meter::Palette {
                gold,
                orange: Color::LightRed,
                red: Color::Red,
                pending: Color::Gray,
            },
        }
        .render(
            b,
            Rect { height: 3, ..rect },
            meter::Spend {
                day: 1,
                actual: 3_420_000,
                pending: 120_000,
                max: 20_000_000,
                unknown_calls: 1,
            },
            t,
        ),
        Bubble => {
            b.set_string(rect.x + 2, rect.y, "claude", Style::default().fg(colors[0]));
            b.set_string(
                rect.x + 2,
                rect.y + 1,
                "A complete answer.",
                Style::default(),
            );
            bubble::Bubble {
                area: rect,
                rail: Rect {
                    width: 1,
                    height: 5,
                    ..rect
                },
                header: Rect {
                    x: rect.x + 2,
                    width: 6,
                    ..row
                },
                footer: Rect {
                    y: rect.y + 4,
                    ..row
                },
                cursor: Some(Position::new(rect.x + 20, rect.y + 1)),
                tail: std::array::from_fn(|i| {
                    Some(Position::new(rect.x + 19 - i as u16, rect.y + 1))
                }),
                status: Rect {
                    x: rect.right() - 5,
                    width: 5,
                    ..row
                },
                hue: colors[0],
                text: Color::White,
                error: Color::Red,
                level,
                no_color: false,
                moment: bubble::Moment::Arrival,
            }
            .apply(t, b, &[]);
        }
        Dispatch => dispatch::DispatchEffect {
            area: a,
            kind: dispatch::Dispatch::Launch {
                gutter: Rect { width: 1, ..rect },
                border: row,
            },
            gold,
            level,
            no_color: false,
            terminal: a,
            ansi16: false,
        }
        .apply(t, b, &[]),
        Command => command::CommandEffect {
            area: a,
            kind: command::Command::Stop {
                cursors: &[],
                rails: &[],
                line: row,
            },
            gold,
            level,
            no_color: false,
        }
        .apply(t, b, &[]),
        Particles => {
            if level == FxLevel::Full && t < duration(effect) {
                let mut pool = particles::ParticlePool::new(42, 30.);
                pool.spawn(particles::Burst {
                    count: 160,
                    origin: particles::Point {
                        x: f32::from(rect.x + rect.width / 2),
                        y: f32::from(rect.y + 3),
                    },
                    velocity: particles::Point { x: 0., y: -5. },
                    spread: particles::Point { x: 20., y: 6. },
                    colors: &colors,
                    glyphs: &['*', '·', '+'],
                    lifetime: (Duration::from_millis(1800), Duration::from_millis(2600)),
                });
                pool.step(t);
                pool.draw(rect, b);
            }
        }
        Debate => debate::VersusCard {
            a: debate::Contender {
                name: "claude",
                wins: 4,
                losses: 2,
                color: colors[0],
            },
            b: debate::Contender {
                name: "gemini",
                wins: 3,
                losses: 1,
                color: colors[1],
            },
            topic: "How should we ship the smallest useful version?",
            seed: 42,
        }
        .draw(rect, b, t, p),
        Filter => {
            if level != FxLevel::Off && t < duration(effect) {
                let filter = filter::Filter {
                    width: rect.width,
                    height: rect.height,
                    tier: ColorTier::TrueColor,
                };
                for x in rect.x..rect.right() {
                    b[(x, rect.y)].set_fg(filter.duotone(x - rect.x, 0, t, colors[0], colors[1]));
                }
            }
        }
        Banner => banner(b, rect, t, level, squad_core::banner::Banner::Round(2)),
        Pick => {
            let pick = pick::Pick::new(pick::Kind::Debate, vec!["claude".into()], false);
            pick.render(
                b,
                &[pick::Candidate {
                    nick: "claude",
                    marker: Rect { width: 1, ..row },
                    accent: row,
                    colour: colors[0],
                }],
                t,
                level,
                ColorTier::TrueColor,
                &[],
            );
        }
        Verdict => verdict::VerdictReveal::new(level, false, true, 42).render(
            b,
            a,
            rect,
            &[],
            t,
            &canned(),
        ),
        Tournament => {
            let rows = [
                tournament::RankingRow {
                    nick: "claude".into(),
                    score: 34,
                    hue: colors[0],
                    critique: "Focused and testable".into(),
                    eliminated: false,
                },
                tournament::RankingRow {
                    nick: "gemini".into(),
                    score: 31,
                    hue: colors[1],
                    critique: "Strong staged rollout".into(),
                    eliminated: true,
                },
            ];
            tournament::RankingReveal::new(level, false, true).render(b, rect, &[], t, &rows);
        }
        Grill => grill::GrillEffect::new(
            motion(),
            1,
            grill::Treatment::PlanVersion { badge: row },
            gold,
        )
        .apply(b, t),
        Census => census::CensusEffect::new(
            motion(),
            1,
            Some(census::Estimate {
                low: 100,
                high: 200,
            }),
            Some(census::Estimate { low: 70, high: 150 }),
        )
        .render_graphic(b, row, t, false, gold),
        Crosscheck => crosscheck::CrosscheckEffect::new(
            motion(),
            1,
            (0..200)
                .map(|i| crosscheck::CellAccent {
                    grid_index: i,
                    rect: Rect::new(rect.x + (i % 20) as u16, rect.y + (i / 20) as u16, 1, 1),
                })
                .collect(),
            gold,
        )
        .apply(b, t, a, None),
        Survey => survey::SurveyEffect::new(
            motion(),
            1,
            Some(survey::Votes {
                agree: 1,
                disagree: 4,
                unsure: 1,
                missing: 0,
            }),
            survey::Votes {
                agree: 4,
                disagree: 1,
                unsure: 1,
                missing: 0,
            },
        )
        .render_graphic(
            b,
            Rect { height: 2, ..rect },
            t,
            false,
            [colors[0], colors[1], gold],
        ),
        Probe => {
            b.set_string(row.x, row.y, "├── accepted branch", Style::default());
            probe::ProbeEffect::new(
                motion(),
                1,
                &[probe::Branch {
                    connectors: (row.x..row.x + 3).map(|x| (x, row.y)).collect(),
                }],
                gold,
            )
            .apply(b, t, a);
        }
    }
}
thread_local! { static BANNER_CACHE: std::cell::RefCell<banner::BannerCache> = std::cell::RefCell::new(banner::BannerCache::default()); }
pub fn banner(
    b: &mut Buffer,
    rect: Rect,
    t: Duration,
    level: FxLevel,
    kind: squad_core::banner::Banner,
) {
    let entrance = BANNER_CACHE.with(|cache| {
        banner::Entrance::new(
            &mut cache.borrow_mut(),
            kind,
            squad_core::banner::BannerMode::On,
            rect.width,
            level,
            banner::Palette::default(),
        )
    });
    entrance.render(b, rect, t, &[]);
}
/// One repeatable timeline; each scene uses the actual integrated catalogue.
pub fn showcase(b: &mut Buffer, rect: Rect, elapsed: Duration, reveal: &verdict::VerdictReveal) {
    if rect.width < 24 || rect.height < 12 {
        Paragraph::new("Resize to at least 28 x 16").render(rect, b);
        return;
    }
    let ms = elapsed.as_millis() as u64;
    let level = FxLevel::Full;
    let colors = [palette::SIGNATURES[0], palette::SIGNATURES[3]];
    let gold = palette::GOLD;
    let chrome = chrome::Chrome {
        level,
        no_color: false,
        ansi16: false,
        gold,
        rainbow: &palette::SIGNATURES,
        seed: 42,
    };
    let row = |dy, h| Rect::new(rect.x, rect.y + dy, rect.width, h).intersection(rect);
    if ms < 2200 {
        // Core owns the FIGlet source; Chrome resolves its supplied glyphs.
        let mut cache = banner::BannerCache::default();
        let logo = cache.get("SQUAD", squad_core::banner::BannerMode::On, rect.width);
        for (i, line) in logo.lines.iter().enumerate() {
            if i < rect.height as usize {
                b.set_stringn(
                    rect.x,
                    rect.y + i as u16,
                    line,
                    rect.width as usize,
                    Style::default().fg(gold),
                );
            }
        }
        chrome.logo(b, row(0, 8), elapsed, &[]);
        if ms >= 1200 {
            for (i, name) in palette::MEMBERS.iter().enumerate() {
                let y = (9 + i as u16).min(rect.height - 1);
                let hue = palette::SIGNATURES[i];
                chrome.roster(
                    b,
                    row(y, 1),
                    name,
                    false,
                    hue,
                    elapsed - Duration::from_millis(1200),
                );
            }
        }
        return;
    }
    if ms < 3000 {
        Paragraph::new("me  @all How should we ship the smallest useful version?\n\nDispatching to claude and gemini").render(row(0,4),b);
        let targets = [
            dispatch::Target {
                row: row(6, 1),
                hue: colors[0],
            },
            dispatch::Target {
                row: row(8, 1),
                hue: colors[1],
            },
        ];
        for (i, name) in ["claude", "gemini"].iter().enumerate() {
            b.set_string(
                rect.x,
                rect.y + 6 + 2 * i as u16,
                name,
                Style::default().fg(colors[i]),
            );
        }
        let t = elapsed - Duration::from_millis(2200);
        dispatch::DispatchEffect {
            area: rect,
            terminal: b.area,
            kind: dispatch::Dispatch::Launch {
                gutter: Rect { width: 1, ..rect },
                border: row(0, 1),
            },
            gold,
            level,
            no_color: false,
            ansi16: false,
        }
        .apply(t, b, &[]);
        dispatch::DispatchEffect {
            area: rect,
            terminal: b.area,
            kind: dispatch::Dispatch::Spokes {
                origin: Position::new(rect.x, rect.y + 2),
                targets: &targets,
            },
            gold,
            level,
            no_color: false,
            ansi16: false,
        }
        .apply(t.saturating_sub(Duration::from_millis(180)), b, &[]);
        return;
    }
    if ms < 5000 {
        paint(
            CatalogueEffect::Debate,
            b,
            elapsed - Duration::from_millis(3000),
            level,
        );
        return;
    }
    if ms < 15800 {
        let n = ((ms - 5000) / 3600) as u32;
        let t = Duration::from_millis((ms - 5000) % 3600);
        let band = if rect.height >= 26 { 8 } else { 5 };
        let stride = if rect.height >= 26 { 5 } else { 4 };
        banner(
            b,
            row(0, band),
            t,
            level,
            match n {
                0 => squad_core::banner::Banner::Opening,
                1 => squad_core::banner::Banner::Round(2),
                _ => squad_core::banner::Banner::FinalRound,
            },
        );
        let t = t.saturating_sub(Duration::from_millis(1800));
        for (i, (name, text)) in [
            ("claude", "Start with one complete workflow and measure."),
            ("gemini", "Stage the rollout with clear rollback criteria."),
        ]
        .iter()
        .enumerate()
        {
            let y = band + 1 + i as u16 * stride;
            if y + 4 >= rect.height {
                continue;
            }
            b.set_stringn(
                rect.x + 2,
                rect.y + y,
                name,
                rect.width.saturating_sub(2) as usize,
                Style::default().fg(colors[i]),
            );
            let shown = (t.as_millis() / 15).min(text.chars().count() as u128) as usize;
            let line: String = text.chars().take(shown).collect();
            b.set_stringn(
                rect.x + 2,
                rect.y + y + 1,
                &line,
                rect.width.saturating_sub(3) as usize,
                Style::default(),
            );
            let end = (rect.x + 2 + shown as u16).min(rect.right() - 1);
            let moment = if t < Duration::from_millis(280) {
                bubble::Moment::Arrival
            } else if t < Duration::from_millis(900) {
                bubble::Moment::Streaming
            } else {
                bubble::Moment::Complete
            };
            bubble::Bubble {
                area: rect,
                rail: Rect::new(rect.x, rect.y + y, 1, 4),
                header: Rect::new(rect.x + 2, rect.y + y, 6, 1),
                footer: Rect::new(rect.x + 2, rect.y + y + 3, rect.width - 2, 1),
                cursor: Some(Position::new(end, rect.y + y + 1)),
                tail: std::array::from_fn(|j| {
                    Some(Position::new(
                        end.saturating_sub(1 + j as u16).max(rect.x + 2),
                        rect.y + y + 1,
                    ))
                }),
                status: Rect::new(rect.right() - 3, rect.y + y, 2, 1),
                hue: colors[i],
                text: Color::White,
                error: Color::Red,
                level,
                no_color: false,
                moment,
            }
            .apply(
                if moment == bubble::Moment::Complete {
                    t.saturating_sub(Duration::from_millis(900))
                } else {
                    t
                },
                b,
                &[],
            );
        }
        let score_y = band + 2 + stride * 2;
        if rect.height > score_y + 3 {
            let p = debate::Presentation {
                level,
                no_color: false,
            };
            let scores = debate::Scores { a: 8 + n, b: 7 + n };
            let score_t = t.saturating_sub(Duration::from_millis(1200));
            debate::ScoreSlam { scores, colors }.draw(row(score_y, 2), b, score_t, p);
            debate::TugBar {
                previous: debate::Scores { a: 0, b: 0 },
                current: scores,
                colors,
            }
            .draw(row(score_y + 3, 1), b, score_t, p);
        }
        return;
    }
    if ms < 17700 {
        banner(
            b,
            row(0, 8),
            elapsed - Duration::from_millis(15800),
            level,
            squad_core::banner::Banner::Verdict,
        );
        return;
    }
    if ms < 24700 {
        reveal.render(
            b,
            b.area,
            rect,
            &[],
            elapsed - Duration::from_millis(17700),
            &canned(),
        );
        return;
    }
    if ms < 27700 {
        banner(
            b,
            row(0, 8),
            Duration::from_secs(2),
            level,
            squad_core::banner::Banner::Tournament,
        );
        let mut slice = Buffer::empty(row(9, rect.height.saturating_sub(9)));
        paint(
            CatalogueEffect::Tournament,
            &mut slice,
            elapsed - Duration::from_millis(24700),
            level,
        );
        for y in slice.area.y..slice.area.bottom() {
            for x in slice.area.x..slice.area.right() {
                b[(x, y)] = slice[(x, y)].clone();
            }
        }
        return;
    }
    banner(
        b,
        row(0, 8),
        Duration::from_secs(2),
        level,
        squad_core::banner::Banner::Survey,
    );
    if rect.height > 12 {
        survey::SurveyEffect::new(
            grill::Motion::new(level, false, false),
            1,
            Some(survey::Votes {
                agree: 1,
                disagree: 4,
                unsure: 1,
                missing: 0,
            }),
            survey::Votes {
                agree: 4,
                disagree: 1,
                unsure: 1,
                missing: 0,
            },
        )
        .render_graphic(
            b,
            row(10, 2),
            elapsed - Duration::from_millis(27700),
            false,
            [colors[0], colors[1], gold],
        );
        Paragraph::new("Ship the smallest useful version.\nAGREE / DISAGREE / UNSURE retain their real counts.\n\nr replay · q quit").render(row(14,rect.height.saturating_sub(14)),b);
    }
}
