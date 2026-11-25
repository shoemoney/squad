//! Core frame gate. Real application widget/event-loop integration is the M4 exit gate.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Widget},
};
use squad_tui::fx::{
    Clock, FxContext, FxKey, FxManager, ManualClock, Pass, PassAccess, TerminalCaps,
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use tachyonfx::{Interpolation, fx};

fn setup() -> (FxManager<ManualClock>, ManualClock, Buffer, FxContext) {
    let clock = ManualClock::default();
    let caps = TerminalCaps::resolve(
        |key| match key {
            "SQUAD_FX" => Some("full".into()),
            "COLORTERM" => Some("truecolor".into()),
            _ => None,
        },
        true,
        Size::new(200, 60),
    )
    .unwrap();
    let mut manager = FxManager::new(clock.clone(), caps);
    for stream in 0..12 {
        let key = FxKey::Custom(format!("stream-{stream}"));
        let pass = Pass::new(
            key.clone(),
            fx::fade_from_fg(Color::DarkGray, (280, Interpolation::CubicOut)),
            Rect::new(1, stream * 4 + 1, 198, 1),
            PassAccess::Unprotected,
        );
        manager.add_pass(key, pass, false);
    }
    let key = FxKey::Custom("verdict".into());
    manager.add_pass(
        key.clone(),
        Pass::new(
            key,
            fx::fade_from_fg(Color::DarkGray, (6500, Interpolation::CubicOut)),
            Rect::new(1, 50, 198, 9),
            PassAccess::Unprotected,
        ),
        true,
    );
    let protected = (0..12).map(|s| Rect::new(2, s * 4 + 2, 196, 2)).collect();
    (
        manager,
        clock,
        Buffer::empty(Rect::new(0, 0, 200, 60)),
        FxContext {
            protected,
            streams: true,
            ..Default::default()
        },
    )
}
fn frame(
    manager: &mut FxManager<ManualClock>,
    clock: &ManualClock,
    buffer: &mut Buffer,
    ctx: &mut FxContext,
) {
    buffer.reset();
    manager.pre(ctx);
    Block::default()
        .borders(Borders::ALL)
        .render(buffer.area, buffer);
    for stream in 0..12 {
        Paragraph::new("gemini  ⠹  streaming  ▃▅▇")
            .style(Style::default().fg(Color::Cyan))
            .render(Rect::new(2, stream * 4 + 1, 196, 1), buffer);
        Paragraph::new("A streamed answer with stable body text, evidence and citations. Token batches continue without moving this text.\nA second line of body text remains protected from every glyph-changing buffer pass.").render(Rect::new(2,stream*4+2,196,2),buffer);
    }
    Paragraph::new("VERDICT\nfit               8 : 7\nfeasibility       7 : 9\noriginality       9 : 6\nsurvivedAttacks   8 : 8\nWINNER: gemini\nWhat to steal from claude: a clear and testable plan").style(Style::default().fg(Color::Yellow)).render(Rect::new(2,50,196,9),buffer);
    manager.post(buffer, buffer.area, clock.now());
    black_box(&buffer.content);
}
fn benchmark(c: &mut Criterion) {
    // Warm allocations and check the actual frame workload before Criterion sampling.
    let (mut manager, clock, mut buffer, mut ctx) = setup();
    clock.advance(Duration::from_millis(80));
    for _ in 0..20 {
        frame(&mut manager, &clock, &mut buffer, &mut ctx);
    }
    let start = Instant::now();
    for _ in 0..200 {
        frame(&mut manager, &clock, &mut buffer, &mut ctx);
    }
    let mean = start.elapsed() / 200;
    assert!(
        mean < Duration::from_millis(6),
        "fx_frame core gate exceeded 6 ms: {mean:?}"
    );
    c.bench_function("fx_frame/12_streams_and_verdict_200x60", |b| {
        b.iter_batched(
            setup,
            |(mut manager, clock, mut buffer, mut ctx)| {
                clock.advance(Duration::from_millis(80));
                frame(&mut manager, &clock, &mut buffer, &mut ctx);
            },
            BatchSize::SmallInput,
        )
    });
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
