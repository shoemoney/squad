//! Pack-level prompt provenance: one hash per registered composition, derived
//! from the effective (override-aware) source text including its repair suffix.
//! Contest owners freeze these hashes next to the pack version at admission.
use super::{PromptError, PromptPack};
use squad_protocol::PromptRef;
use std::collections::BTreeMap;

/// Compositions owned by the four structured modes (census, crosscheck,
/// survey, probe). Each must carry frozen provenance before work is issued.
pub const NEW_MODE_COMPOSITIONS: [&str; 18] = [
    "census.hunt",
    "census.merge",
    "census.rate",
    "census.final",
    "crosscheck.inventory",
    "crosscheck.normalize.extract_goal",
    "crosscheck.normalize.normalize_inventory",
    "crosscheck.cell",
    "crosscheck.audit",
    "crosscheck.rule",
    "survey.extract",
    "survey.vote",
    "survey.revote",
    "survey.report",
    "probe.explain",
    "probe.check",
    "probe.predict",
    "probe.report",
];

impl PromptPack {
    /// Hash of the effective system and user text, plus the repair variants
    /// when the composition registers a repair suffix.
    pub fn composition_hash(&self, id: &str) -> Result<String, PromptError> {
        let spec = self.composition(id)?;
        let (system, user) = self.composition_source(id, false)?;
        let repaired = if spec.repair_suffix.is_some() {
            Some(self.composition_source(id, true)?)
        } else {
            None
        };
        squad_store::commit::digest(&(system, user, repaired))
            .map_err(|e| PromptError::new(id, None, e.to_string()))
    }

    /// Frozen reference for one composition, as stored on work.
    pub fn prompt_ref(&self, id: &str) -> Result<PromptRef, PromptError> {
        let bad = |m: &str| PromptError::new(id, None, m.to_owned());
        Ok(PromptRef {
            template_id: squad_protocol::Text::new(id).map_err(|_| bad("invalid template id"))?,
            pack_version: squad_protocol::Text::new(self.version())
                .map_err(|_| bad("invalid pack version"))?,
            template_hash: squad_protocol::Text::new(self.composition_hash(id)?)
                .map_err(|_| bad("invalid hash"))?,
        })
    }

    /// Hashes for every registered composition, keyed by composition ID.
    pub fn composition_hashes(&self) -> Result<BTreeMap<String, String>, PromptError> {
        self.manifest()
            .compositions
            .iter()
            .map(|c| Ok((c.id.clone(), self.composition_hash(&c.id)?)))
            .collect()
    }
}
