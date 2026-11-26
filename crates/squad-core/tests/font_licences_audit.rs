#[test]
fn vendored_font_licence_and_notice_ship_with_both_embedded_fonts() {
    let licence = include_str!("../src/banner/fonts/LICENSE-FIGlet.txt");
    let notice = include_str!("../../../NOTICE");
    assert!(licence.contains("Redistribution and use in source and binary forms"));
    assert!(licence.contains("Glenn Chappell and Ian Chai"));
    assert!(notice.contains("Standard") && notice.contains("Small"));
    for font in [
        include_str!("../src/banner/fonts/standard.flf"),
        include_str!("../src/banner/fonts/small.flf"),
    ] {
        assert!(font.starts_with("flf2a"));
        assert!(font.contains("figlet") || font.contains("FIGlet"));
    }
}
