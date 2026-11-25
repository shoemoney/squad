//! Embedded, strictly validated prompt templates and selected provider compositions.
//!
//! Call [`PromptPack::load`] at startup. Inputs must already be authorized by the
//! work adapter: this module performs no history lookups or visibility projection.
//! JSON slots accept JSON values, never pre-encoded fragments. A loaded pack owns
//! its effective text, so subsequent filesystem changes cannot alter it.

mod embedded;
mod load;
mod render;

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
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

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub templates: Vec<TemplateSpec>,
    pub compositions: Vec<CompositionSpec>,
    pub rules: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSpec {
    pub id: String,
    pub file: String,
    pub placeholders: Vec<String>,
}

/// Manifest metadata remains available to the work/provenance adapter.
#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSelection {
    pub template: String,
    pub start_marker: String,
    pub end_marker: String,
    pub include_markers: bool,
    pub required_matches: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemTransform {
    pub operation: String,
    pub template: String,
    pub prefix: String,
    pub replacement_template: String,
    pub required_matches: usize,
}

#[derive(Debug, Clone)]
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
