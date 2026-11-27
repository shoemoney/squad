use squad_core::export::decide::matrix_text;
use squad_protocol::decide::*;
#[test]
fn close_line_has_no_winner_and_keeps_failed_explanation_arithmetic() {
    let m = DecideMatrix {
        cells: vec![
            DecideMedian {
                option_id: "a".into(),
                criterion_id: "cost".into(),
                median: 6.0,
                contested: true,
            },
            DecideMedian {
                option_id: "b".into(),
                criterion_id: "cost".into(),
                median: 5.8,
                contested: false,
            },
        ],
        ranking: vec![
            DecideTotal {
                option_id: "a".into(),
                total: 6.0,
            },
            DecideTotal {
                option_id: "b".into(),
                total: 5.8,
            },
        ],
        margin: 0.2,
        winner: None,
        leave_one_out: vec![],
        flips: vec![],
        robust: false,
        member_support: 3,
    };
    let text = matrix_text(&m);
    assert!(text.contains("Too close to call between a and b."));
    assert!(!text.contains("Winner:"));
    assert!(text.contains("6.00*"));
    assert!(text.contains("WHAT FLIPS IT"));
}
