//! Shared committed human-step presentation.
use squad_protocol::parser::PendingHuman;
pub fn lines(pending: &PendingHuman) -> Vec<String> {
    let mut out = vec![
        pending.kind.as_str().to_uppercase(),
        pending.prompt.clone(),
        "answer pending".into(),
    ];
    out.extend(
        pending
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| format!("{}. {}", i + 1, item)),
    );
    if pending.kind == squad_protocol::ContestKind::BassAckwardsPrd {
        out.push("/answer <your note>, then /continue to save".into());
        return out;
    }
    out.push(
        if pending.kind == squad_protocol::ContestKind::Readback {
            "1b 2a or /answer 3: your own reading"
        } else {
            "/answer O1 <what you saw>"
        }
        .into(),
    );
    out
}
