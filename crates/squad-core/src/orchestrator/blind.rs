//! Deterministic identity redaction for blind contest projections.
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn tokens(config: &SquadConfig, bindings: &BTreeMap<String, ExecutorBinding>) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for member in &config.agents {
        tokens.insert(member.nick.to_string());
        if !member.persona.trim().is_empty() {
            tokens.insert(member.persona.clone());
        }
    }
    for slug in config
        .agents
        .iter()
        .map(|m| m.model.as_str())
        .chain(std::iter::once(config.judge.model.as_str()))
        .chain(bindings.values().map(|b| b.model.as_str()))
    {
        tokens.insert(slug.into());
        tokens.extend(
            slug.split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .map(String::from),
        );
    }
    tokens.into_iter().collect()
}
pub fn redact(text: &str, tokens: &[String]) -> String {
    let mut tokens = tokens.iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    tokens.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    if tokens.is_empty() {
        return text.into();
    }
    let pattern = format!(
        r"(?i)\b(?:{})\b",
        tokens
            .iter()
            .map(|s| regex::escape(s))
            .collect::<Vec<_>>()
            .join("|")
    );
    regex::Regex::new(&pattern)
        .expect("escaped redaction tokens")
        .replace_all(text, "[redacted]")
        .into_owned()
}
pub fn value(mut value: serde_json::Value, tokens: &[String]) -> serde_json::Value {
    fn visit(v: &mut serde_json::Value, tokens: &[String], prose: bool) {
        match v {
            serde_json::Value::String(s) if prose => *s = redact(s, tokens),
            serde_json::Value::Array(a) => {
                for v in a {
                    visit(v, tokens, prose)
                }
            }
            serde_json::Value::Object(o) => {
                for (k, v) in o {
                    visit(
                        v,
                        tokens,
                        matches!(
                            k.as_str(),
                            "claim"
                                | "question"
                                | "reason"
                                | "agreeReasons"
                                | "disagreeReasons"
                                | "unsureReasons"
                        ),
                    )
                }
            }
            _ => {}
        }
    }
    visit(&mut value, tokens, false);
    value
}
