use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{CompositionSpec, PromptError, PromptPack};

/// Values from authorized work mappings. Supply native values for JSON slots;
/// `serde_json::to_string` performs their sole encoding during rendering.
pub type Inputs = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedComposition {
    pub system: String,
    pub user: String,
    pub provider_user_name: String,
    pub step: String,
}

/// Tokenize source text only. Inserted data never enters this scanner.
fn tokens<'a>(id: &str, text: &'a str) -> Result<Vec<(&'a str, Option<&'a str>)>, PromptError> {
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let literal = &rest[..start];
        if literal.contains("}}") {
            return Err(PromptError::new(
                id,
                Some("}}"),
                "unmatched closing delimiter",
            ));
        }
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(PromptError::new(id, Some(after), "unclosed placeholder"));
        };
        let name = &after[..end];
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_') {
            return Err(PromptError::new(
                id,
                Some(name),
                "invalid placeholder syntax",
            ));
        }
        result.push((literal, Some(name)));
        rest = &after[end + 2..];
    }
    if rest.contains("}}") {
        return Err(PromptError::new(
            id,
            Some("}}"),
            "unmatched closing delimiter",
        ));
    }
    result.push((rest, None));
    Ok(result)
}

pub(super) fn placeholders(id: &str, text: &str) -> Result<BTreeSet<String>, PromptError> {
    Ok(tokens(id, text)?
        .into_iter()
        .filter_map(|(_, name)| name.map(str::to_owned))
        .collect())
}

pub(super) fn validate_placeholders(
    id: &str,
    text: &str,
    expected: &[String],
) -> Result<(), PromptError> {
    let actual = placeholders(id, text)?;
    let declared: BTreeSet<_> = expected.iter().cloned().collect();
    if declared.len() != expected.len() {
        return Err(PromptError::new(
            id,
            None,
            "duplicate placeholder declaration",
        ));
    }
    if let Some(name) = actual.difference(&declared).next() {
        return Err(PromptError::new(id, Some(name), "unknown placeholder"));
    }
    if let Some(name) = declared.difference(&actual).next() {
        return Err(PromptError::new(
            id,
            Some(name),
            "missing required placeholder",
        ));
    }
    Ok(())
}

fn encode(id: &str, name: &str, value: &Value) -> Result<String, PromptError> {
    let invalid = || PromptError::new(id, Some(name), "invalid placeholder value");
    if name.ends_with("_json") {
        // These source-text slots are always JSON strings, never raw JSON fragments.
        if matches!(
            name,
            "user_text_json"
                | "user_name_json"
                | "persona_json"
                | "topic_json"
                | "debate_instruction_json"
                | "notice_text_json"
                | "invalid_output_json"
                | "debater_a_json"
                | "debater_b_json"
        ) && !value.is_string()
        {
            return Err(invalid());
        }
        return serde_json::to_string(value)
            .map_err(|e| PromptError::new(id, Some(name), e.to_string()));
    }
    if matches!(
        name,
        "round"
            | "rounds"
            | "grill_rounds"
            | "census_pass"
            | "plan_version"
            | "next_plan_version"
            | "entrants_remaining"
    ) || name.ends_with("word_limit")
    {
        let number = value.as_u64().ok_or_else(invalid)?;
        if number == 0 && name != "round" {
            return Err(invalid());
        }
        return Ok(number.to_string());
    }
    let text = value.as_str().ok_or_else(invalid)?;
    let valid = if matches!(
        name,
        "nick" | "opponent" | "failed_nick" | "leader_nick" | "conflicting_nick"
    ) {
        squad_protocol::Nick::new(text).is_ok()
    } else if matches!(name, "contest_id" | "debate_id") {
        text.parse::<squad_protocol::ContestId>().is_ok()
    } else if name == "mode_step" {
        matches!(
            text.split('.').next(),
            Some("census" | "crosscheck" | "survey" | "probe")
        ) && text
            .bytes()
            .all(|c| c.is_ascii_lowercase() || b"._".contains(&c))
    } else if name == "failure_kind" {
        matches!(text, "timeout" | "error")
    } else if matches!(name, "grill_angle" | "verdict_result") {
        // Application-generated phrases only; source prose belongs in JSON slots.
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " _-.,:@()/".contains(c))
    } else {
        !text.is_empty()
            && text
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-.:/@".contains(&c))
    };
    if !valid {
        return Err(invalid());
    }
    Ok(text.to_owned())
}

fn render_text(id: &str, text: &str, inputs: &Inputs) -> Result<String, PromptError> {
    let mut result = String::new();
    for (literal, name) in tokens(id, text)? {
        result.push_str(literal);
        if let Some(name) = name {
            let value = inputs
                .get(name)
                .ok_or_else(|| PromptError::new(id, Some(name), "missing value"))?;
            result.push_str(&encode(id, name, value)?);
        }
    }
    Ok(result)
}

fn validate_inputs(id: &str, text: &str, inputs: &Inputs) -> Result<(), PromptError> {
    let required = placeholders(id, text)?;
    for name in inputs.keys() {
        if !required.contains(name) {
            return Err(PromptError::new(
                id,
                Some(name),
                "unknown input placeholder",
            ));
        }
    }
    for name in &required {
        if !inputs.contains_key(name) {
            return Err(PromptError::new(id, Some(name), "missing value"));
        }
    }
    Ok(())
}

impl PromptPack {
    pub fn render_template(&self, id: &str, inputs: &Inputs) -> Result<String, PromptError> {
        let text = self.template(id)?;
        validate_inputs(id, text, inputs)?;
        render_text(id, text, inputs)
    }

    /// Renders exactly one system and one user message. A repair suffix is
    /// appended inside that same user message. Validate only selected text:
    /// blind judge inputs therefore require no discarded identity placeholders.
    pub fn render_composition(
        &self,
        id: &str,
        inputs: &Inputs,
        repair: bool,
    ) -> Result<RenderedComposition, PromptError> {
        let composition = self.composition(id)?;
        let (system, user) = self.composition_text(composition, repair)?;
        validate_inputs(id, &format!("{system}\n{user}"), inputs)?;
        Ok(RenderedComposition {
            system: render_text(id, &system, inputs)?,
            user: render_text(id, &user, inputs)?,
            provider_user_name: composition.provider_user_name.clone(),
            step: composition.step.clone(),
        })
    }

    /// Effective selected source text for provenance and adapter input discovery.
    pub fn composition_source(
        &self,
        id: &str,
        repair: bool,
    ) -> Result<(String, String), PromptError> {
        self.composition_text(self.composition(id)?, repair)
    }

    pub(super) fn composition_text(
        &self,
        c: &CompositionSpec,
        repair: bool,
    ) -> Result<(String, String), PromptError> {
        let assemble = |ids: &[String], system: bool| -> Result<String, PromptError> {
            let mut sections = Vec::new();
            for id in ids {
                let mut text = self.template(id)?.to_owned();
                if system {
                    if let Some(t) = &c.system_transform
                        && t.template == *id
                    {
                        if t.operation != "replace_exact_prefix"
                            || t.required_matches != 1
                            || !text.starts_with(&t.prefix)
                            || text.matches(&t.prefix).count() != 1
                        {
                            return Err(PromptError::new(
                                id,
                                None,
                                "blind system prefix must occur exactly once at start",
                            ));
                        }
                        text = format!(
                            "{}{}",
                            self.template(&t.replacement_template)?.trim_end(),
                            &text[t.prefix.len()..]
                        );
                    }
                } else if let Some(selection) = &c.template_selection
                    && selection.template == *id
                {
                    if selection.required_matches != 1
                        || text.matches(&selection.start_marker).count() != 1
                        || text.matches(&selection.end_marker).count() != 1
                    {
                        return Err(PromptError::new(
                            id,
                            None,
                            "composition markers must each occur exactly once",
                        ));
                    }
                    let start = text.find(&selection.start_marker).expect("marker checked");
                    let end = text.find(&selection.end_marker).expect("marker checked");
                    let content_start = start + selection.start_marker.len();
                    if end < content_start {
                        return Err(PromptError::new(
                            id,
                            None,
                            "composition markers out of order",
                        ));
                    }
                    text = if selection.include_markers {
                        text[start..end + selection.end_marker.len()].to_owned()
                    } else {
                        text[content_start..end].to_owned()
                    };
                }
                sections.push(text);
            }
            Ok(sections.join("\n"))
        };
        let system = assemble(&c.system, true)?;
        let mut user = assemble(&c.user, false)?;
        if repair {
            let suffix = c
                .repair_suffix
                .as_ref()
                .ok_or_else(|| PromptError::new(&c.id, None, "composition has no repair suffix"))?;
            user.push('\n');
            user.push_str(self.template(suffix)?);
        }
        Ok((system, user))
    }
}
