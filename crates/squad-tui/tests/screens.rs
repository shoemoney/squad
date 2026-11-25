use ratatui::{buffer::Buffer, layout::Rect};
use squad_tui::screens;
use std::path::Path;

#[test]
fn screen_snapshots() {
    for (width, height) in [(80, 24), (120, 40), (40, 8), (12, 3), (0, 0)] {
        let area = Rect::new(0, 0, width, height);
        for kind in ["missing_key", "room_locked", "too_small"] {
            let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new("x"));
            match kind {
                "missing_key" => {
                    screens::missing_key(area, &mut buffer, Path::new("/tmp/squad/.env"))
                }
                "room_locked" => screens::room_locked(area, &mut buffer, 4242),
                _ => screens::terminal_too_small(
                    area,
                    &mut buffer,
                    "$3.42 / $20.00 | Pending $1.00 | Remaining $15.58",
                ),
            }
            insta::assert_debug_snapshot!(format!("screens_{kind}_{width}x{height}"), buffer);
        }
    }
}

#[test]
fn offset_render_preserves_surroundings_and_one_row_spend() {
    let mut buffer = Buffer::filled(Rect::new(0, 0, 80, 24), ratatui::buffer::Cell::new("x"));
    screens::terminal_too_small(Rect::new(4, 3, 30, 1), &mut buffer, "$3.42 / $20.00");
    assert_eq!(buffer[(4, 3)].symbol(), "$");
    assert_eq!(buffer[(3, 3)].symbol(), "x");
    assert_eq!(buffer[(4, 4)].symbol(), "x");
}
