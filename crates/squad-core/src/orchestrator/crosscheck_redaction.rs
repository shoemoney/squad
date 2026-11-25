//! Deterministic blind-text projection. Never mutate the human's source records.
use regex::{Regex, RegexBuilder};
use squad_protocol::*;
pub fn tokens(
    config: Option<&SquadConfig>,
    participants: &[MemberNick],
    models: impl Iterator<Item = String>,
) -> Vec<String> {
    let mut tokens = participants
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if let Some(config) = config {
        for a in &config.agents {
            tokens.push(a.persona.clone());
        }
    }
    for slug in models {
        tokens.push(slug.clone());
        tokens.push(slug.trim_start_matches('~').into());
        for part in slug.trim_start_matches('~').split('/') {
            tokens.push(part.into());
            if let Some(family) = part.split('-').next() {
                tokens.push(family.into());
            }
        }
    }
    tokens.retain(|s| !s.trim().is_empty());
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    tokens
}
struct Redactor {
    matcher: Option<Regex>,
    word: Regex,
}
impl Redactor {
    fn new(tokens: &[String]) -> Self {
        let pattern = tokens
            .iter()
            .map(|s| regex::escape(s))
            .collect::<Vec<_>>()
            .join("|");
        Self {
            matcher: (!tokens.is_empty()).then(|| {
                RegexBuilder::new(&pattern)
                    .case_insensitive(true)
                    .build()
                    .expect("escaped tokens")
            }),
            word: Regex::new(r"\w").expect("unicode words"),
        }
    }
    fn replace(&self, text: &str) -> String {
        let Some(matcher) = &self.matcher else {
            return text.into();
        };
        let is_word = |c: char| self.word.is_match(&c.to_string());
        let mut out = String::new();
        let mut through = 0;
        for found in matcher.find_iter(text) {
            if text[..found.start()]
                .chars()
                .next_back()
                .is_some_and(is_word)
                || text[found.end()..].chars().next().is_some_and(is_word)
            {
                continue;
            }
            out.push_str(&text[through..found.start()]);
            out.push_str("[redacted]");
            through = found.end();
        }
        out.push_str(&text[through..]);
        out
    }
}
pub fn redact(text: &str, tokens: &[String]) -> String {
    Redactor::new(tokens).replace(text)
}
pub fn project(material: &mut CrosscheckCaseMaterial, tokens: &[String]) {
    let redactor = Redactor::new(tokens);
    let text =
        |t: &mut Text| *t = Text::new(redactor.replace(t.as_str())).expect("nonempty redaction");
    match material {
        CrosscheckCaseMaterial::Inventory { goal, .. } => {
            text(&mut goal.goal);
            for t in goal
                .constraints
                .iter_mut()
                .chain(goal.uncertainties.iter_mut())
            {
                text(t);
            }
        }
        CrosscheckCaseMaterial::NormalizeInventory { inventories, .. } => {
            for (_, _, i) in inventories {
                for item in &mut i.items {
                    text(&mut item.item);
                    text(&mut item.rationale);
                }
            }
        }
        CrosscheckCaseMaterial::Review { cells, .. } => {
            for cell in cells {
                for item in &mut cell.items {
                    text(item);
                }
            }
        }
        CrosscheckCaseMaterial::Audit { grid, samples, .. } => {
            for cell in grid {
                for item in &mut cell.items {
                    text(item);
                }
            }
            for review in samples {
                text(&mut review.reason);
                if let Some(fix) = &mut review.proposed_fix {
                    text(fix);
                }
                if let Some(cite) = &mut review.citation {
                    text(&mut cite.location);
                    text(&mut cite.quote);
                }
            }
        }
        _ => {}
    }
}
