use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::fx::particles::{Burst, CAPACITY, ParticlePool, Point};
use std::time::Duration;
fn burst(count: usize) -> Burst<'static> {
    Burst {
        count,
        origin: Point { x: 12.0, y: 3.0 },
        velocity: Point { x: 0.0, y: -4.0 },
        spread: Point { x: 9.0, y: 3.0 },
        colors: &[Color::Yellow, Color::Cyan],
        glyphs: &['*', '·', '+'],
        lifetime: (Duration::from_secs(1), Duration::from_secs(2)),
    }
}
#[test]
fn seeded_pool_is_bounded_and_reusable() {
    let mut a = ParticlePool::new(42, 30.0);
    let mut b = ParticlePool::new(42, 30.0);
    assert_eq!(a.spawn(burst(300)), CAPACITY);
    b.spawn(burst(300));
    assert_eq!(a.spawn(burst(1)), 0);
    for _ in 0..20 {
        a.step(Duration::from_millis(16));
        b.step(Duration::from_millis(16));
        assert_eq!(
            a.active().collect::<Vec<_>>(),
            b.active().collect::<Vec<_>>()
        );
    }
    let mut c = ParticlePool::new(43, 30.0);
    c.spawn(burst(256));
    assert_ne!(a.active().next(), c.active().next());
    a.step(Duration::from_secs(3));
    assert!(a.is_done());
    assert_eq!(a.spawn(burst(160)), 160);
    a.clear();
    assert!(a.is_done());
}
#[test]
fn gravity_is_analytic_and_frame_independent() {
    let mut a = ParticlePool::new(0, 30.0);
    let mut spec = burst(1);
    spec.spread = Point::default();
    a.spawn(spec);
    let mut b = a.clone();
    a.step(Duration::from_millis(200));
    b.step(Duration::from_millis(100));
    b.step(Duration::from_millis(100));
    let p = a.active().next().unwrap();
    let q = b.active().next().unwrap();
    assert!((p.position.y - 2.8).abs() < 1e-5);
    assert!((p.velocity.y - 2.0).abs() < 1e-5);
    assert!((p.position.y - q.position.y).abs() < 1e-5);
}
#[test]
fn clips_protected_blanks_text_and_wide_continuations() {
    let area = Rect::new(10, 10, 12, 4);
    let mut buf = Buffer::empty(area);
    buf.set_string(10, 10, "界 body", ratatui::style::Style::default());
    let original = buf.clone();
    let mut pool = ParticlePool::new(1, 0.0);
    for (x, y) in [(11., 10.), (13., 10.), (12., 11.), (21., 13.), (25., 15.)] {
        let mut spec = burst(1);
        spec.origin = Point { x, y };
        pool.spawn(spec);
    }
    let protected = Rect::new(12, 11, 2, 1);
    pool.draw_protected(area, &mut buf, &[protected], true);
    assert_eq!(buf[(11, 10)], original[(11, 10)]);
    assert_eq!(buf[(13, 10)], original[(13, 10)]);
    assert_eq!(buf[(12, 11)], original[(12, 11)]);
    assert_ne!(buf[(21, 13)].symbol(), " ");
    assert_eq!(buf[(21, 13)].fg, Color::Reset);
}
#[test]
fn spark_storyboard() {
    let area = Rect::new(0, 0, 25, 8);
    let mut frames = Vec::new();
    for ms in [0, 500, 1000, 2000, 2016] {
        let mut pool = ParticlePool::new(42, 8.0);
        pool.spawn(burst(24));
        pool.step(Duration::from_millis(ms));
        let mut buf = Buffer::empty(area);
        buf.set_string(8, 3, "PROTECTED", ratatui::style::Style::default());
        pool.draw(area, &mut buf);
        if ms >= 2000 {
            let mut static_buf = Buffer::empty(area);
            static_buf.set_string(8, 3, "PROTECTED", ratatui::style::Style::default());
            assert_eq!(buf, static_buf);
        }
        frames.push((ms, buf));
    }
    insta::assert_debug_snapshot!(frames);
}
