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
    pub versus: Rect,
    pub scores: Vec<(u32, MemberNick, Rect)>,
    pub tug: Rect,
    pub verdict: Rect,
    pub winner: Rect,
    pub totals: Vec<Rect>,
    pub census_estimate: Rect,
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
    pub fn toggle_verdict(&mut self) {
        self.verdict_expanded = !self.verdict_expanded;
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
    pub fn scroll(&mut self, delta: isize) {
        let top = self.top.saturating_add_signed(delta).min(
            self.transcript
                .total_rows()
                .saturating_sub(usize::from(self.viewport.height)),
        );
        self.anchor = self.transcript.anchor_at(top);
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
        for card in session.transcript.cards.values() {
            if session.debate_messages.contains(&card.id) {
                continue;
            }
            if self.transcript.message(card.id).is_some_and(|m| {
                m.source == card.text && m.complete == card.complete && m.attempt == card.attempt
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
            model.status = if card.complete {
                "complete"
            } else {
                "streaming"
            }
            .into();
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
            for (index, (id, source)) in session.debates.iter().enumerate() {
                let mut view = source.clone();
                view.number = index + 1;
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
                models.push(model);
            }
            for (id, view) in &session.tournaments {
                let mut scratch = Buffer::empty(Rect::new(0, 0, body_width, 4096));
                let geometry =
                    widgets::tournament::render(view, scratch.area, &mut scratch, palette);
                let height = (0..scratch.area.height)
                    .rfind(|y| (0..body_width).any(|x| scratch[(x, *y)].symbol() != " "))
                    .map_or(0, |y| usize::from(y) + 1);
                let mut model = TranscriptMessage::new(identity(id), "TOURNAMENT", "");
                model.widget_rows = rows(&scratch, height);
                self.graphics.insert(
                    model.id,
                    Graphics {
                        tournament: Some(geometry),
                        ..Default::default()
                    },
                );
                models.push(model);
            }
            for (id, contest) in &session.contests {
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
                models.push(model);
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
