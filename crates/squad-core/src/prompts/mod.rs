//! Embedded, strictly validated prompt templates and selected provider compositions.
//!
//! Call [`PromptPack::load`] at startup. Inputs must already be authorized by the
//! work adapter: this module performs no history lookups or visibility projection.
//! JSON slots accept JSON values, never pre-encoded fragments. A loaded pack owns
//! its effective text, so subsequent filesystem changes cannot alter it.

mod embedded;
pub mod inputs;
pub mod inputs_crosscheck;
mod load;
mod render;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use render::{Inputs, RenderedComposition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptError {
    pub template: String,
    pub placeholder: Option<String>,
    pub message: String,
}

impl PromptError {
    pub(super) fn new(
        template: &str,
        placeholder: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            template: template.into(),
            placeholder: placeholder.map(str::to_owned),
            message: message.into(),
        }
    }
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "prompt {}", self.template)?;
        if let Some(placeholder) = &self.placeholder {
            write!(f, " placeholder {placeholder}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for PromptError {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub version: String,
    pub templates: Vec<TemplateSpec>,
    pub compositions: Vec<CompositionSpec>,
    pub rules: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateSpec {
    pub id: String,
    pub file: String,
    pub placeholders: Vec<String>,
}

/// Manifest metadata remains available to the work/provenance adapter.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompositionSpec {
    pub id: String,
    pub system: Vec<String>,
    pub user: Vec<String>,
    pub provider_user_name: String,
    pub step: String,
    pub repair_suffix: Option<String>,
    pub template_selection: Option<TemplateSelection>,
    pub system_transform: Option<SystemTransform>,
    pub placeholders: Option<Vec<String>>,
    pub system_placeholders: Option<Vec<String>>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateSelection {
    pub template: String,
    pub start_marker: String,
    pub end_marker: String,
    pub include_markers: bool,
    pub required_matches: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemTransform {
    pub operation: String,
    pub template: String,
    pub prefix: String,
    pub replacement_template: String,
    pub required_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPack {
    manifest: Manifest,
    templates: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
}

impl PromptPack {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn template(&self, id: &str) -> Result<&str, PromptError> {
        self.templates
            .get(id)
            .map(String::as_str)
            .ok_or_else(|| PromptError::new(id, None, "unknown template ID"))
    }

    /// Exact override bytes for the owning provenance layer to hash and freeze.
    pub fn overrides(&self) -> &BTreeMap<String, String> {
        &self.overrides
    }

    pub fn composition(&self, id: &str) -> Result<&CompositionSpec, PromptError> {
        self.manifest
            .compositions
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| PromptError::new(id, None, "unknown composition ID"))
    }
}

#[cfg(test)]
mod tests;

/// Protocol adapter: JSON slots contain serialized native JSON values; numeric
/// slots contain decimal integers; other slots contain literal strings. A
/// registered composition returns a JSON object with system/user/step/name,
/// while a template ID (optionally ending in `.md`) returns plain rendered text.
/// The caller freezes and verifies provenance hashes when allocating work.
impl squad_protocol::PromptRenderer for PromptPack {
    fn render(
        &self,
        prompt: &squad_protocol::PromptRef,
        values: &BTreeMap<String, String>,
    ) -> Result<String, squad_protocol::ReaderError> {
        let invalid = || squad_protocol::ReaderError::StoreUnavailable {
            message: "prompt rendering failed".into(),
        };
        if prompt.pack_version.as_str() != self.version() {
            return Err(invalid());
        }
        let id = prompt
            .template_id
            .as_str()
            .strip_suffix(".md")
            .unwrap_or(prompt.template_id.as_str());
        let mut inputs = Inputs::new();
        for (key, value) in values {
            let value = if key.ends_with("_json")
                || key.ends_with("word_limit")
                || matches!(
                    key.as_str(),
                    "round"
                        | "rounds"
                        | "grill_rounds"
                        | "census_pass"
                        | "plan_version"
                        | "next_plan_version"
                        | "entrants_remaining"
                ) {
                serde_json::from_str(value).map_err(|_| invalid())?
            } else {
                Value::String(value.clone())
            };
            inputs.insert(key.clone(), value);
        }
        if self.composition(id).is_ok() {
            let repair = inputs.contains_key("invalid_output_json");
            let rendered = self
                .render_composition(id, &inputs, repair)
                .map_err(|_| invalid())?;
            serde_json::to_string(&rendered).map_err(|_| invalid())
        } else {
            self.render_template(id, &inputs).map_err(|_| invalid())
        }
    }
}
pub mod inputs_tournament;

pub mod inputs_census;
