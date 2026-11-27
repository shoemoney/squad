//! Human-readable labels for wire enums, so user-facing text never shows Rust `Debug` output.
use serde::Serialize;

/// Shown in place of a value that is absent.
pub const NONE_LABEL: &str = "—";

/// Turns a wire token such as `inProgress`, `IN_PROGRESS` or `in_progress` into `in progress`.
pub fn humanize(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 4);
    let mut prev_lower = false;
    for ch in token.chars() {
        if ch == '_' || ch == '-' {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            prev_lower = false;
            continue;
        }
        if ch.is_uppercase() && prev_lower && !out.ends_with(' ') {
            out.push(' ');
        }
        prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        out.extend(ch.to_lowercase());
    }
    out.trim().to_string()
}

/// Readable label for any serializable value: unit enums become lowercase words, null becomes a dash, lists are comma joined and other shapes fall back to JSON.
pub fn label<T: Serialize + ?Sized>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(v) => label_value(&v),
        Err(_) => NONE_LABEL.to_string(),
    }
}

/// Like [`label`], but an absent value renders as the dash.
pub fn label_opt<T: Serialize>(value: &Option<T>) -> String {
    value.as_ref().map_or_else(|| NONE_LABEL.to_string(), label)
}

/// Comma-joined display forms of a list, or the dash when it is empty.
pub fn list_label<T: std::fmt::Display>(items: &[T]) -> String {
    if items.is_empty() {
        return NONE_LABEL.to_string();
    }
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The nick (or reserved name) of an optional actor, or the dash when unknown.
pub fn actor_label(actor: &Option<crate::Actor>) -> String {
    actor
        .as_ref()
        .map_or_else(|| NONE_LABEL.to_string(), ToString::to_string)
}

fn label_value(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Null => NONE_LABEL.to_string(),
        Value::String(s) => humanize(s),
        Value::Bool(b) => if *b { "yes" } else { "no" }.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) if items.is_empty() => NONE_LABEL.to_string(),
        Value::Array(items) => items.iter().map(label_value).collect::<Vec<_>>().join(", "),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| format!("{}: {}", humanize(k), label_value(v)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn humanizes_wire_tokens() {
        assert_eq!(humanize("inProgress"), "in progress");
        assert_eq!(humanize("DAILY_MAX_REACHED"), "daily max reached");
        assert_eq!(humanize("not_found"), "not found");
        assert_eq!(humanize("Idle"), "idle");
    }
    #[test]
    fn labels_values() {
        assert_eq!(label(&"Idle"), "idle");
        assert_eq!(label(&"inProgress"), "in progress");
        assert_eq!(label(&Option::<String>::None), NONE_LABEL);
        assert_eq!(label(&vec!["a", "b"]), "a, b");
    }
}
