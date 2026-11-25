use ratatui::layout::Rect;
use squad_tui::layout::{self, Viewport};

#[test]
fn geometries() {
    let cases = [
        (80, 24),
        (120, 36),
        (120, 40),
        (119, 24),
        (79, 24),
        (80, 23),
        (0, 0),
    ];
    let snapshot: Vec<_> = cases
        .into_iter()
        .map(|(w, h)| format!("{w}x{h}: {:#?}", layout::layout(Rect::new(0, 0, w, h))))
        .collect();
    insta::assert_snapshot!("layout_geometries", snapshot.join("\n\n"));
}

#[test]
fn growth_offsets_and_footer() {
    for width in [80, 119, 120, 200] {
        for requested in 0..12 {
            let area = Rect::new(7, 9, width, 40);
            let l = layout::with_input_rows(area, requested);
            assert_ne!(l.viewport, Viewport::TooSmall);
            assert_eq!(l.input.height, requested.clamp(4, 8));
            assert_eq!(l.transcript.bottom(), l.input.y);
            assert_eq!(l.participants.right(), l.divider.x);
            assert_eq!(l.divider.right(), l.transcript.x);
            assert_eq!(l.transcript.right(), area.right());
            assert_eq!(
                l.status.height
                    + l.spend.height
                    + l.transcript.height
                    + l.input.height
                    + l.help.height,
                area.height
            );
            assert_eq!(layout::spend_destination(&l, 40), l.spend);
            let footer = layout::spend_destination(&l, 1);
            assert_eq!(footer.bottom(), l.participants.bottom());
            assert_eq!(footer.height, 1);
        }
    }
}

#[test]
fn tiny_sizes_are_bounded() {
    for width in 0..81 {
        for height in 0..25 {
            let area = Rect::new(3, 5, width, height);
            let l = layout::layout(area);
            for r in [
                l.status,
                l.spend,
                l.participants,
                l.divider,
                l.transcript,
                l.input,
                l.help,
            ] {
                assert!(r.right() <= area.right());
                assert!(r.bottom() <= area.bottom());
            }
        }
    }
}
