use squad_core::orchestrator::verbatim::verbatim;
#[test]
fn normalized_quotes_match_original_byte_spans() {
    for (quote, source) in [
        (
            "A 'quoted' phrase - yes",
            "prefix A ‘quoted’ phrase — yes suffix",
        ),
        (
            "one two three four",
            "before one\n\t two   three four after",
        ),
        ("full width ABC words", "full width ＡＢＣ words"),
        ("café is here now", "cafe\u{301} is here now"),
        ("office ligature words", "oﬃce ligature words"),
    ] {
        let range = verbatim(quote, source).unwrap_or_else(|| panic!("{quote}"));
        assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        assert_eq!(
            verbatim(quote, &source[range.clone()]),
            Some(0..range.len())
        );
    }
}
#[test]
fn first_match_case_contiguity_and_minimum_are_enforced() {
    assert_eq!(
        verbatim("twelve chars", "twelve chars twelve chars"),
        Some(0..12)
    );
    for (quote, source) in [
        ("too short", "too short"),
        ("Twelve chars", "twelve chars"),
        ("one passage ... another", "one passage and another"),
        ("one passage … another", "one passage and another"),
        ("missing quote text", "actual source text"),
    ] {
        assert!(verbatim(quote, source).is_none());
    }
}
