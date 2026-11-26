//! Manual process-RSS measurement, isolated from other test harnesses.
#![cfg(target_os = "linux")]
use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::MessageId;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::transcript::{TranscriptMessage, TranscriptView, render},
};

#[test]
#[ignore = "manual memory measurement: run this test alone with --ignored --nocapture"]
fn five_hundred_displayed_messages_process_rss() {
    let mut view = TranscriptView::new(140);
    let body = "A realistic **answer** with a code example.\n\n```rust\nlet result = 42;\n```\n\n"
        .repeat(12);
    for n in 1..=500 {
        let id: MessageId = format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap();
        view.upsert(TranscriptMessage::new(id, "claude", &body));
    }
    let area = Rect::new(0, 0, 140, 42);
    let mut buffer = Buffer::empty(area);
    let palette = Palette::new(ColorTier::TrueColor);
    // Visit every viewport so row layout and rendered markdown caches are populated.
    for top in (0..view.total_rows()).step_by(42) {
        render(&view, top, area, &mut buffer, &palette);
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let rss: u64 = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    println!(
        "M6 transcript messages=500 width=140 height=42 body_bytes={} process_rss_kib={rss}",
        body.len()
    );
    assert!(rss <= 256 * 1024, "500-message process RSS exceeds 256 MiB");
}
