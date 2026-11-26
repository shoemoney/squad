//! Human input is durable and has no provider deadline or automatic answer.
use squad_protocol::{ContestKind,parser::PendingHuman};
pub fn complete(pending: &PendingHuman) -> bool {
 pending.kind == ContestKind::Debugging && !pending.answers.is_empty()
 || pending.kind == ContestKind::Readback && (1..=pending.items.len() as u32).all(|n|pending.answers.contains_key(&n))
}
pub fn apply(pending: &mut PendingHuman, text: &str) {
 if pending.kind==ContestKind::Debugging {pending.answers.insert(1,text.into());}
 else if let Some((n,reading))=text.split_once(':') { if let Ok(n)=n.trim().parse() {pending.answers.insert(n,reading.trim().into());} }
 else {for token in text.split_whitespace() {let (n,reading)=token.split_at(token.len()-1);if let Ok(n)=n.parse() {pending.answers.insert(n,reading.into());}}}
}
