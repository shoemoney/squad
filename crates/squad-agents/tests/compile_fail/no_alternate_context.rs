fn alternate(reader: &dyn squad_protocol::HistoryReader) {
    let _ = reader.for_context(squad_protocol::ContextId("someone_else".into()));
}
fn main() {}
