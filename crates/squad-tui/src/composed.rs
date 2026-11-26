//! Prepared inline widgets share the transcript's height index and source anchors.
use crate::{
    fx::Palette,
    session::Session,
    widgets::{
        self,
        transcript::{TranscriptMessage, TranscriptView},
    },
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
};
use squad_protocol::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Composed {
    pub transcript: TranscriptView,
    pub participants: widgets::participants::ParticipantsState,
    pub banners: widgets::banner::BannerCache,
    pub graphics: BTreeMap<MessageId, Graphics>,
    signature: Option<(usize, usize, usize, u16)>,
    pub started: bool,
    pub verdict_expanded: bool,
    pub active_cues: Vec<crate::fx::TimedCue>,
    pub anchor: Option<widgets::transcript::TranscriptAnchor>,
    pub top: usize,
    pub viewport: Rect,
    pub roster_area: Rect,
    pub roster_rows: usize,
    pub logo: Rect,
    pick_id: Option<MessageId>,
    epoch: u64,
    artifact_signature: Option<(usize, usize, u16)>,
}
#[derive(Clone, Default)]
pub struct Graphics {
    pub tournament: Option<widgets::tournament::TournamentRects>,
    pub banners: Vec<(squad_core::banner::Banner, Rect)>,
    pub probe_connectors: Vec<(u16, u16)>,
    pub versus: Rect,
    pub scores: Vec<(u32, MemberNick, Rect)>,
    pub tug: Rect,
    pub verdict: Rect,
    pub winner: Rect,
    pub totals: Vec<Rect>,
    pub census_estimate: Rect,
    pub crosscheck_cells: Vec<(CellId, usize, Rect)>,
    pub survey: Vec<(String, Rect)>,
}
fn identity(id: impl ToString) -> MessageId {
    id.to_string().parse().expect("protocol UUID")
}
fn rows(buffer: &Buffer, height: usize) -> Vec<Line<'static>> {
    (0..height.min(usize::from(buffer.area.height)))
        .map(|y| {
            let mut spans = Vec::new();
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y as u16)];
                // Wide glyph continuation cells are represented by the glyph itself.
                if x > 0
                    && unicode_width::UnicodeWidthStr::width(buffer[(x - 1, y as u16)].symbol()) > 1
                {
                    continue;
                }
                spans.push(Span::styled(cell.symbol().to_owned(), cell.style()));
            }
            Line::from(spans)
        })
        .collect()
}
impl Composed {
    pub fn invalidate_artifacts(&mut self) {
        self.signature = None;
        self.artifact_signature = None;
    }
    pub fn toggle_verdict(&mut self) {
        self.verdict_expanded = !self.verdict_expanded;
        if self.anchor.is_none() {
            self.anchor = self.transcript.anchor_at(self.top);
        }
        if let Some((&id, _)) = self
            .graphics
            .iter()
            .rev()
            .find(|(_, g)| !g.verdict.is_empty())
        {
            self.anchor = Some(widgets::transcript::TranscriptAnchor {
                id,
                source_grapheme: 0,
                row_in_header: Some(0),
            });
        }
        self.signature = None;
        self.artifact_signature = None;
    }
    /// Moves the viewport by `delta` rows and returns true when it reached the live bottom.
    pub fn scroll(&mut self, delta: isize) -> bool {
        let bottom = self
            .transcript
            .total_rows()
            .saturating_sub(usize::from(self.viewport.height));
        let top = self.top.saturating_add_signed(delta).min(bottom);
        self.anchor = self.transcript.anchor_at(top);
        top >= bottom
    }
    pub fn prepare(&mut self, session: &Session, width: u16, palette: &Palette) {
        if self.epoch != session.presentation_epoch {
            self.transcript = TranscriptView::new(width);
            self.graphics.clear();
            self.signature = None;
            self.artifact_signature = None;
            self.pick_id = None;
            self.epoch = session.presentation_epoch;
        }
        let signature = (
            session.snapshot.events.len(),
            session.transcript.cards.len(),
            session.transcript.revision as usize,
            width,
        );
        if self.signature == Some(signature) {
            return;
        }
        if self.signature.is_none_or(|old| old.1 > signature.1) {
            let mut retained = std::collections::BTreeSet::new();
            retained.extend(
                session
                    .transcript
                    .cards
                    .keys()
                    .filter(|id| !session.debate_messages.contains(id))
                    .copied(),
            );
            retained.extend(session.contests.keys().map(identity));
            retained.extend(session.contests.values().filter_map(|v| {
                v.verdict
                    .as_ref()
                    .map(|v| v.completed.generation.message_id)
            }));
            retained.extend(session.cases.keys().map(identity));
            self.transcript.retain(|id| retained.contains(&id));
        }
        let pick_id = session
            .pick
            .as_ref()
            .map(|pick| identity(pick.request.contest_id));
        if self.pick_id != pick_id
            && let Some(previous) = self.pick_id
        {
            self.transcript.retain(|id| id != previous);
        }
        self.pick_id = pick_id;
        self.signature = Some(signature);
        self.transcript.resize(width);
        let body_width = width.saturating_sub(1);
        let mut models = Vec::new();
        let mut superseded = session.transcript.superseded.clone();
        for event in &session.snapshot.events {
            if let EnvelopePayload::MessageSuperseded(s) = &event.content {
                superseded.insert(s.superseded_message_id, s.replacement_message_id);
            }
        }
        for card in session.transcript.cards.values() {
            if session.debate_messages.contains(&card.id) {
                continue;
            }
            let replacement = superseded.get(&card.id).copied();
            let state = if replacement.is_some() {
                crate::transcript::CardState::Superseded
            } else {
                session
                    .transcript
                    .state_of(card.id)
                    .unwrap_or(crate::transcript::CardState::Complete)
            };
            if self.transcript.message(card.id).is_some_and(|m| {
                m.source == card.text
                    && m.complete == card.complete
                    && m.attempt == card.attempt
                    && m.status == state.label()
            }) {
                continue;
            }
            let author = match &card.author {
                Actor::Member(n) => n.to_string(),
                Actor::Me => "me".into(),
                Actor::Judge => "judge".into(),
                Actor::System => "system".into(),
            };
            let mut model = TranscriptMessage::new(card.id, author, &card.text);
            model.slot = session
                .snapshot
                .context
                .roster
                .iter()
                .position(|n| n.as_str() == model.author)
                .unwrap_or(0);
            model.attempt = card.attempt;
            model.complete = card.complete;
            model.status = state.label().into();
            match state {
                crate::transcript::CardState::Superseded => {
                    let by = replacement
                        .and_then(|id| session.transcript.cards.get(&id))
                        .map_or_else(
                            || "a replacement reply".to_owned(),
                            |c| match &c.author {
                                Actor::Member(n) => format!("@{n}"),
                                _ => "a replacement reply".to_owned(),
                            },
                        );
                    model.before_body.push(Line::raw(format!(
                        "⟳ Replaced by {by}. This earlier attempt is kept for reference."
                    )));
                }
                crate::transcript::CardState::Incomplete(cancelled) => {
                    model.after_body.push(Line::raw(if cancelled {
                        "— Output stopped: cancelled before it finished."
                    } else {
                        "— Output stopped: this reply did not finish."
                    }));
                }
                _ => {}
            }
            models.push(model);
        }
        let artifact_signature = (
            session.snapshot.events.len(),
            session
                .debate_messages
                .iter()
                .filter_map(|id| session.transcript.cards.get(id))
                .map(|c| c.text.len())
                .sum(),
            width,
        );
        if self.artifact_signature != Some(artifact_signature) {
            self.artifact_signature = Some(artifact_signature);
            self.graphics.clear();
            let expanded = self.verdict_expanded;
            let revision = session.snapshot.events.len() as u64;
            for (index, (id, source)) in session.debates.iter().enumerate() {
                let mut view = source.clone();
                view.number = index + 1;
                view.rounds
                    .iter_mut()
                    .for_each(|round| round.reasons_expanded = expanded);
                view.topic = session.snapshot.events.iter().rev().find_map(|event| {
                    if let EnvelopePayload::DebateOpening(opening) = &event.content {
                        (opening.debate_id == *id).then(|| opening.topic_text.to_string())
                    } else {
                        None
                    }
                });
                view.records = view.state.debaters.each_ref().map(|nick| {
                    session
                        .snapshot
                        .scoreboard
                        .iter()
                        .find(|s| &s.nick == nick)
                        .map_or((0, 0), |s| (s.wins as u32, s.losses as u32))
                });
                for (message, delta) in &session.debate_streams {
                    if let Some(card) = session.transcript.cards.get(message)
                        && let Actor::Member(nick) = &card.author
                    {
                        view.update_stream(nick, delta, &card.text);
                    }
                }
                view.prepare_banners(body_width, &mut self.banners);
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 512));
                let rect = scratch.area;
                let mut geometry = widgets::debate::render(&view, rect, &mut scratch, palette);
                if geometry.content_height > usize::from(scratch.area.height) {
                    scratch = Buffer::empty(Rect::new(
                        0,
                        0,
                        body_width,
                        u16::try_from(geometry.content_height).unwrap_or(u16::MAX),
                    ));
                    geometry = widgets::debate::render(&view, scratch.area, &mut scratch, palette);
                }
                let mut model = TranscriptMessage::new(identity(id), "DEBATE", "");
                model.widget_rows = rows(&scratch, geometry.content_height);
                model.presentation.revision = session.snapshot.events.len() as u64;
                self.graphics.insert(
                    model.id,
                    Graphics {
                        banners: geometry.banners,
                        versus: geometry.versus_card,
                        scores: geometry.score_cells,
                        tug: geometry.tug_bar,
                        ..Default::default()
                    },
                );
                models.push(model);
                if let Some(value) = session.contests.get(id).and_then(|c| c.verdict.as_ref())
                    && let Ok(mut verdict) =
                        widgets::verdict::VerdictView::from_debate(value, &view.state.debaters)
                {
                    verdict.show_criterion_reasons = self.verdict_expanded;
                    verdict.prepare_banner(body_width.saturating_sub(2), &mut self.banners);
                    verdict.round_scores = session.contests[id].scores.values().cloned().collect();
                    let rect = scratch.area;
                    let geometry = widgets::verdict::render(&verdict, rect, &mut scratch, palette);
                    let mut model =
                        TranscriptMessage::new(value.completed.generation.message_id, "JUDGE", "");
                    model.widget_rows = rows(&scratch, geometry.content_height);
                    self.graphics.insert(
                        model.id,
                        Graphics {
                            verdict: geometry.card,
                            winner: geometry.winner,
                            totals: geometry.totals,
                            ..Default::default()
                        },
                    );
                    models.push(model);
                }
            }
            for (id, source) in &session.census {
                let mut view = source.clone();
                view.details_expanded = self.verdict_expanded;
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 2048));
                let shapes = widgets::census::render(&view, scratch.area, &mut scratch, palette);
                self.graphics.insert(
                    identity(id),
                    Graphics {
                        banners: vec![(squad_core::banner::Banner::Census, shapes.banner)],
                        census_estimate: shapes.estimate_graphic,
                        ..Default::default()
                    },
                );
                let height = (0..2048)
                    .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                    .map_or(0, |y| usize::from(y) + 1);
                let mut model = TranscriptMessage::new(identity(id), "CENSUS", "");
                model.widget_rows = rows(&scratch, height);
                models.push(model);
            }
            for (id, view) in &session.cases {
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 512));
                let rect = scratch.area;
                let mut geometry = widgets::case::render(view, rect, &mut scratch, palette);
                if geometry.content_height > usize::from(scratch.area.height) {
                    scratch = Buffer::empty(Rect::new(
                        0,
                        0,
                        body_width,
                        u16::try_from(geometry.content_height).unwrap_or(u16::MAX),
                    ));
                    geometry = widgets::case::render(view, scratch.area, &mut scratch, palette);
                }
                let mut model = TranscriptMessage::new(identity(id), "CASE", "");
                model.widget_rows = rows(&scratch, geometry.content_height);
                model.presentation.expanded = !view.collapsed;
                model.presentation.report_revision = revision;
                models.push(model);
            }
            for (id, source) in &session.tournaments {
                let mut view = source.clone();
                if expanded {
                    view.rounds
                        .iter_mut()
                        .for_each(|round| round.expanded = true);
                }
                let view = &view;
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 4096));
                let geometry =
                    widgets::tournament::render(view, scratch.area, &mut scratch, palette);
                let height = (0..scratch.area.height)
                    .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                    .map_or(0, |y| usize::from(y) + 1);
                let mut model = TranscriptMessage::new(identity(id), "TOURNAMENT", "");
                model.widget_rows = rows(&scratch, height);
                model.presentation.revision = revision;
                model.presentation.expanded = view.expanded;
                model.presentation.rankings_expanded =
                    view.rounds.iter().all(|round| round.expanded);
                self.graphics.insert(
                    model.id,
                    Graphics {
                        tournament: Some(geometry),
                        ..Default::default()
                    },
                );
                models.push(model);
            }
            for (id, source) in &session.grills {
                let mut view = source.clone();
                if expanded {
                    view.appendix_expanded = true;
                    view.plans
                        .iter_mut()
                        .for_each(|plan| plan.expanded = Some(true));
                }
                let view = &view;
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 4096));
                let area = scratch.area;
                widgets::grill::render(view, area, &mut scratch, palette);
                let height = (0..area.height)
                    .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                    .map_or(0, |y| usize::from(y) + 1);
                let mut model = TranscriptMessage::new(identity(id), "GRILL", "");
                model.widget_rows = rows(&scratch, height);
                model.presentation.revision = revision;
                model.presentation.expanded = view.expanded;
                model.presentation.appendix_expanded = view.appendix_expanded;
                model.presentation.rankings_expanded =
                    view.plans.iter().all(|p| p.expanded == Some(true));
                models.push(model);
            }
            for (id, source) in &session.probes {
                let mut view = source.clone();
                view.banner_art = Some(
                    self.banners.get(
                        "PROBE",
                        view.banner_mode
                            .unwrap_or(squad_core::banner::BannerMode::On),
                        body_width,
                    ),
                );
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 4096));
                let geometry = widgets::probe::render(&view, scratch.area, &mut scratch, palette);
                let mut model = TranscriptMessage::new(identity(id), "PROBE", "");
                model.widget_rows = rows(&scratch, geometry.content_height);
                model.presentation.expanded = !view.matrix_collapsed;
                model.presentation.report_revision = revision;
                self.graphics.insert(
                    model.id,
                    Graphics {
                        banners: vec![(squad_core::banner::Banner::Probe, geometry.banner)],
                        probe_connectors: geometry.connectors,
                        ..Default::default()
                    },
                );
                models.push(model);
            }
            for (id, contest) in &session.contests {
                if contest.crosscheck.state.is_some() {
                    let height = contest
                        .crosscheck
                        .lines()
                        .iter()
                        .map(|line| line.width().div_ceil(usize::from(body_width.max(1))) * 2 + 1)
                        .sum::<usize>()
                        .min(usize::from(u16::MAX)) as u16;
                    let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, height));
                    widgets::crosscheck::render(
                        &contest.crosscheck,
                        scratch.area,
                        &mut scratch,
                        palette,
                    );
                    let height = (0..scratch.area.height)
                        .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                        .map_or(0, |y| usize::from(y) + 1);
                    let mut cells = vec![];
                    if body_width >= 36
                        && let Some(grid) = &contest.crosscheck.grid
                    {
                        let headers = (0..scratch.area.height)
                            .filter_map(|y| {
                                let key = (0..36)
                                    .map(|x| scratch[(x, y)].symbol())
                                    .collect::<String>();
                                key.parse::<CellId>().ok().map(|id| (id, y))
                            })
                            .collect::<BTreeMap<_, _>>();
                        for (index, cell) in grid.cells.iter().enumerate() {
                            if let Some(y) = headers.get(&cell.cell_id) {
                                cells.push((cell.cell_id, index, Rect::new(0, *y, body_width, 1)));
                            }
                        }
                    }
                    self.graphics.insert(
                        identity(id),
                        Graphics {
                            crosscheck_cells: cells,
                            ..Default::default()
                        },
                    );
                    let mut model = TranscriptMessage::new(identity(id), "CROSSCHECK", "");
                    model.widget_rows = rows(&scratch, height);
                    model.presentation.expanded = contest.crosscheck.collapsed_cells.is_empty();
                    model.presentation.report_revision = revision;
                    models.push(model);
                    continue;
                }
                if let Some(view) = &contest.survey {
                    let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 4096));
                    let area = scratch.area;
                    let (height, banner, survey) =
                        widgets::survey::render_geometry(view, area, &mut scratch);
                    self.graphics.insert(
                        identity(id),
                        Graphics {
                            banners: vec![(squad_core::banner::Banner::Survey, banner)],
                            survey,
                            ..Default::default()
                        },
                    );
                    let mut model = TranscriptMessage::new(identity(id), "SURVEY", "");
                    model.widget_rows = rows(&scratch, height);
                    model.presentation.expanded = !view.collapsed;
                    model.presentation.report_revision = revision;
                    models.push(model);
                    continue;
                }
                if let Some(ContestState::Generic(state)) = &contest.state {
                    let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 128));
                    let rect = scratch.area;
                    match state.kind {
                        ContestKind::Grill => {
                            let mut view = widgets::grill::GrillView::new("", vec![]);
                            view.apply_state(state);
                            widgets::grill::render(&view, rect, &mut scratch, palette);
                        }
                        ContestKind::Tournament => {
                            let mut view = widgets::tournament::TournamentView::new("", vec![]);
                            view.apply_state(state);
                            widgets::tournament::render(&view, rect, &mut scratch, palette);
                        }
                        _ => continue,
                    }
                    let height = (0..128)
                        .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                        .map_or(0, |y| usize::from(y) + 1);
                    let mut model =
                        TranscriptMessage::new(identity(id), format!("{:?}", state.kind), "");
                    model.widget_rows = rows(&scratch, height);
                    models.push(model);
                }
            }
            if let Some(pick) = &session.pick {
                let mut view = widgets::pick::PickView::new(
                    if pick.request.kind == ContestKind::Grill {
                        widgets::pick::PickKind::Grill
                    } else {
                        widgets::pick::PickKind::Debate
                    },
                    &pick.request.banner,
                    pick.request
                        .eligible
                        .iter()
                        .map(|nick| widgets::pick::Candidate {
                            nick: nick.to_string(),
                            eligible: true,
                            status: "answered".into(),
                            answer: String::new(),
                        })
                        .collect(),
                );
                view.id = Some(pick.request.contest_id);
                view.status = widgets::pick::PickStatus::Pending;
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 64));
                let rect = scratch.area;
                widgets::pick::render(&view, rect, &mut scratch, palette);
                let height = (0..64)
                    .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                    .map_or(0, |y| usize::from(y) + 1);
                let mut model =
                    TranscriptMessage::new(identity(pick.request.contest_id), "PICK", "");
                model.widget_rows = rows(&scratch, height);
                model.presentation.pick_revision = revision + pick.request.eligible.len() as u64;
                models.push(model);
            }
        }
        // Shared script progress belongs inside the same anchored, cached card.
        for model in &mut models {
            let id: ContestId = model.id.to_string().parse().expect("protocol UUID");
            if let Some(contest) = session.contests.get(&id)
                && (!contest.steps.is_empty() || !contest.work.is_empty())
                && matches!(
                    model.author.as_str(),
                    "CENSUS" | "CROSSCHECK" | "SURVEY" | "PROBE"
                )
            {
                let mut progress = vec![Line::from(crate::markdown::sanitize(
                    &contest.report_header(),
                ))];
                if let Some(step) = contest.steps.values().next_back() {
                    progress.push(Line::from(widgets::contest_status::step_label(step)));
                    let kind = match model.author.as_str() {
                        "CENSUS" => ContestKind::Census,
                        "CROSSCHECK" => ContestKind::Crosscheck,
                        "SURVEY" => ContestKind::Survey,
                        _ => ContestKind::Probe,
                    };
                    if let Some(label) = widgets::contest_status::visibility_label(kind, step) {
                        progress.push(Line::from(label));
                    }
                }
                progress.extend(
                    contest
                        .work
                        .values()
                        .map(|work| Line::from(crate::markdown::sanitize(&work.line()))),
                );
                // Append below the report so effect geometry remains relative to its static rows.
                model.widget_rows.extend(progress);
            }
        }
        models.sort_by_key(|m| (m.author == "PICK", m.id));
        if let Some(first) = self.transcript.first_id() {
            let older = models
                .iter()
                .filter(|m| m.id < first && self.transcript.message(m.id).is_none())
                .cloned()
                .collect::<Vec<_>>();
            if !older.is_empty() {
                self.transcript.prepend(older);
            }
        }
        for model in models {
            self.transcript.upsert(model);
        }
    }
    pub fn graphic_rect(&self, id: MessageId, local: Rect, top: usize, area: Rect) -> Rect {
        if local.is_empty() {
            return Rect::default();
        }
        let Some(start) = self.transcript.start_row(id) else {
            return Rect::default();
        };
        let y = start + usize::from(local.y);
        let bottom = y + usize::from(local.height);
        let clipped_top = y.max(top);
        let clipped_bottom = bottom.min(top + usize::from(area.height));
        if clipped_bottom <= clipped_top {
            return Rect::default();
        }
        Rect::new(
            area.x + 1 + local.x,
            area.y + (clipped_top - top) as u16,
            local.width,
            (clipped_bottom - clipped_top) as u16,
        )
        .intersection(area)
    }
}
