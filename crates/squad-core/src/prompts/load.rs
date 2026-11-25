use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::render::{placeholders, validate_placeholders};
use super::{PromptError, PromptPack, embedded};

impl PromptPack {
    /// Load the embedded pack, with optional local UTF-8 overrides from the environment.
    pub fn load() -> Result<Self, PromptError> {
        let directory = std::env::var_os("SQUAD_PROMPTS_DIR");
        Self::from_override_dir(directory.as_deref().map(Path::new))
    }

    /// Load only the embedded defaults, regardless of process environment.
    pub fn embedded() -> Result<Self, PromptError> {
        Self::from_override_dir(None)
    }

    /// Missing individual files retain defaults; all other read errors fail startup.
    /// Only registered filenames are read. An override manifest cannot change contracts.
    pub fn from_override_dir(directory: Option<&Path>) -> Result<Self, PromptError> {
        let manifest: super::Manifest = serde_json::from_str(embedded::MANIFEST)
            .map_err(|e| PromptError::new("manifest", None, e.to_string()))?;
        if let Some(directory) = directory {
            let metadata = std::fs::metadata(directory).map_err(|e| {
                PromptError::new("pack", None, format!("{}: {e}", directory.display()))
            })?;
            if !metadata.is_dir() {
                return Err(PromptError::new(
                    "pack",
                    None,
                    "override path is not a directory",
                ));
            }
        }
        let mut pack = Self {
            manifest,
            templates: BTreeMap::new(),
            overrides: BTreeMap::new(),
        };
        let mut files = BTreeSet::new();
        for spec in &pack.manifest.templates {
            if pack.templates.contains_key(&spec.id) || !files.insert(&spec.file) {
                return Err(PromptError::new(&spec.id, None, "duplicate registration"));
            }
            let default = embedded::TEMPLATES
                .iter()
                .find(|(id, _)| *id == spec.id)
                .ok_or_else(|| PromptError::new(&spec.id, None, "no embedded default"))?
                .1;
            let mut text = default.to_owned();
            if let Some(directory) = directory {
                let path = directory.join(&spec.file);
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        pack.overrides.insert(spec.id.clone(), content.clone());
                        text = content;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(PromptError::new(
                            &spec.id,
                            None,
                            format!("{}: {e}", path.display()),
                        ));
                    }
                }
            }
            validate_placeholders(&spec.id, &text, &spec.placeholders)?;
            pack.templates.insert(spec.id.clone(), text);
        }
        if pack.templates.len() != embedded::TEMPLATES.len() {
            return Err(PromptError::new(
                "manifest",
                None,
                "embedded inventory differs",
            ));
        }
        let mut ids = BTreeSet::new();
        for composition in &pack.manifest.compositions {
            if !ids.insert(&composition.id) {
                return Err(PromptError::new(
                    &composition.id,
                    None,
                    "duplicate composition",
                ));
            }
            let (system, user) = pack.composition_text(composition, false)?;
            placeholders(&composition.id, &system)?;
            placeholders(&composition.id, &user)?;
            if let Some(expected) = &composition.placeholders {
                validate_placeholders(&composition.id, &user, expected)?;
            }
            if let Some(expected) = &composition.system_placeholders {
                validate_placeholders(&composition.id, &system, expected)?;
            }
            if composition.repair_suffix.is_some() {
                pack.composition_text(composition, true)?;
            }
        }
        Ok(pack)
    }
}
