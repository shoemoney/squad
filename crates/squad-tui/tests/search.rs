use squad_protocol::MessageId;
use squad_tui::search::{SearchDocument, SearchState};
#[test]
fn literal_unicode_search_reveals_and_tracks_history_coverage() {
    let id = MessageId::new();
    let documents = vec![SearchDocument {
        id,
        source: "a👩‍💻e\u{301} literal.* literal.*".into(),
        durable: true,
        collapsed: true,
    }];
    let mut search = SearchState {
        query: "literal.*".into(),
        ..Default::default()
    };
    search.refresh(&documents, true, None);
    assert_eq!(search.matches.len(), 2);
    assert!(search.needs_page());
    assert_eq!(search.matches[0].graphemes, 4..13);
    assert_eq!(
        &documents[0].source[search.matches[0].bytes.clone()],
        "literal.*"
    );
    assert!(search.navigate(false).unwrap().reveal);
    let current = search.current;
    search.refresh(&documents, false, None);
    assert_eq!(search.current, current);
    assert!(!search.needs_page());
    search.query.clear();
    search.refresh(&documents, false, None);
    assert!(search.matches.is_empty());
}
