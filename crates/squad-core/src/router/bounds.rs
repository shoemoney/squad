//! Structure-derived reservation bounds for contest plans.
//!
//! A plan step's input is bounded by what the step can actually receive: the
//! frozen prompt templates, the mode's response schema, the frozen user inputs and
//! the accepted artifacts of earlier steps, each capped by its own output ceiling.
//! Sizes are in the same unit as ordinary dispatch (`squad_agents::context`,
//! UTF-8 bytes plus framing), never the model's context window. Each step prices
//! one permitted attempt at its worst case (including a repair resend), and the
//! plan reserves every permitted attempt of every step, so a started attempt can
//! always draw its frozen share.
use crate::prompts::PromptPack;
use rusqlite::Connection;
use squad_protocol::*;
use squad_store::{Result, StoreError};
use std::collections::BTreeMap;

/// Accepted output bytes per output token when it is re-sent in a later prompt.
/// The ordinary estimator counts one token per byte, so this is the byte bound.
pub const OUTPUT_BYTES_PER_TOKEN: u64 = 4;
/// Labels, identifiers and JSON quoting around one embedded artifact.
pub const ARTIFACT_FRAME: u64 = 256;
/// Role/name framing the estimator adds to the system and user messages.
const MESSAGE_FRAME: u64 = 160;
/// Validation diagnostics appended to a repair attempt.
const REPAIR_FRAME: u64 = 2048;

/// One accepted artifact of `visible_tokens` ceiling, as embedded in a later prompt.
pub fn artifact(visible_tokens: u32) -> u64 {
    u64::from(visible_tokens) * OUTPUT_BYTES_PER_TOKEN + ARTIFACT_FRAME
}
/// Exact bytes of a frozen text value embedded as a JSON string.
pub fn json_text(text: &str) -> u64 {
    serde_json::to_string(text).map_or(text.len() as u64 * 6, |s| s.len() as u64)
}
/// Size of a mode response schema as rendered into `mode_schema_json`.
pub fn schema_bytes(schema: serde_json::Value) -> u64 {
    schema.to_string().len() as u64
}
/// Generous bytes per word (about 6 on average), separator and JSON quoting included.
pub const BYTES_PER_WORD: u64 = 8;
/// An accepted artifact whose validator enforces a word limit: the lesser of the
/// word-limit bytes and the output ceiling bytes.
pub fn limited(visible_tokens: u32, words: u32) -> u64 {
    (u64::from(words) * BYTES_PER_WORD).min(u64::from(visible_tokens) * OUTPUT_BYTES_PER_TOKEN)
        + ARTIFACT_FRAME
}
/// An artifact the engine re-projects into a different shape (ids, labels, lenses)
/// before embedding it: a quarter more than its raw bound.
pub fn project(artifact_bytes: u64) -> u64 {
    artifact_bytes * 5 / 4
}
/// A frozen user text after blinding substitutions: at most twice its JSON size.
pub fn blinded(text: &str) -> u64 {
    2 * json_text(text)
}
/// Bytes of several raw templates (steps that are not provider compositions).
pub fn raw_templates(pack: &PromptPack, ids: &[&str]) -> Result<u64> {
    ids.iter().try_fold(0_u64, |sum, id| {
        pack.template(id)
            .map(|t| sum + t.len() as u64)
            .map_err(|_| invalid("Unknown contest template"))
    })
}
/// System and user template bytes of one composition, repair text included.
pub fn template(pack: &PromptPack, composition: &str) -> Result<u64> {
    let (system, user) = pack
        .composition_source(composition, true)
        .or_else(|_| pack.composition_source(composition, false))
        .map_err(|_| invalid("Unknown contest composition"))?;
    Ok(system.len() as u64 + user.len() as u64)
}
/// Reasoning budget of a binding's own effort.
pub fn reasoning_budget(effort: ReasoningEffort) -> u32 {
    match effort {
        ReasoningEffort::Low => constants::REASONING_LOW_TOKENS,
        ReasoningEffort::Medium => constants::REASONING_MEDIUM_TOKENS,
        ReasoningEffort::High => constants::REASONING_HIGH_TOKENS,
    }
}

/// `calls` invocations of one planned step with the same bound.
#[derive(Debug, Clone)]
pub struct Step {
    pub kind: WorkKind,
    /// Member nick or `judge`.
    pub target: String,
    /// Other members that may run this step; the dearest is priced.
    pub alternatives: Vec<String>,
    /// Frozen template bytes (see [`template`]).
    pub template: u64,
    /// `mode_schema_json` bytes (response schema plus any per-claim enums).
    pub schema: u64,
    /// Bounded `mode_data_json` bytes, including every embedded artifact.
    pub data: u64,
    pub visible: u32,
    pub calls: u32,
    pub attempts: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Priced {
    /// One normal pass over every call: first attempts only.
    pub estimated_usd: f64,
    /// Every permitted attempt, each at the worst case its step can send.
    pub reserved_micros: i64,
    /// Frozen per-attempt reservation by work kind (`WorkKind::as_str`).
    pub shares: BTreeMap<String, i64>,
    /// Frozen first-attempt input bound by work kind.
    pub input_bounds: BTreeMap<String, u64>,
    pub calls: u32,
}
impl Priced {
    pub fn reserved_usd(&self) -> f64 {
        self.reserved_micros as f64 / 1e6
    }
    /// Plan settings the store and the dispatch checks read.
    pub fn settings(&self) -> serde_json::Value {
        serde_json::json!({"attemptShares":self.shares,"stepInputBounds":self.input_bounds})
    }
}

/// Key of a work kind in plan settings: its serialized name.
pub fn kind_key(kind: &WorkKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn invalid(message: &'static str) -> StoreError {
    StoreError::Domain(ErrorCode::CONFIG_INVALID, message)
}
/// Price a frozen step list against the resolved catalog. Fails closed with
/// `CONTEXT_TOO_LARGE` when a step cannot fit its model.
pub fn price(
    tx: &Connection,
    bindings: &BTreeMap<String, ExecutorBinding>,
    steps: &[Step],
) -> Result<Priced> {
    let mut out = Priced::default();
    let mut attempts_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for step in steps {
        // A step run by any of several members is priced at the dearest one.
        let mut first = 0.0_f64;
        let mut worst = 0.0_f64;
        let mut input = 0_u64;
        for target in std::iter::once(&step.target).chain(&step.alternatives) {
            let binding = bindings.get(target).ok_or(StoreError::Domain(
                ErrorCode::MODEL_UNAVAILABLE,
                "Missing contest model",
            ))?;
            let (length, pricing): (i64, String) = tx.query_row(
                "SELECT context_length,pricing_json FROM catalog WHERE slug=?",
                [binding.model.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let pricing: serde_json::Value = serde_json::from_str(&pricing)?;
            let price = |key: &str| -> Result<f64> {
                let value = pricing[key]
                    .as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| pricing[key].as_f64())
                    .unwrap_or(0.0);
                if value.is_finite() && value >= 0.0 {
                    Ok(value)
                } else {
                    Err(invalid("Contest pricing is invalid"))
                }
            };
            let reasoning = if target == "judge" {
                reasoning_budget(binding.effort)
                    .max(reasoning_budget(constants::NEW_MODE_JUDGE_REASONING_EFFORT))
            } else {
                reasoning_budget(binding.effort)
            };
            let own_input = step.template + MESSAGE_FRAME + step.schema + step.data;
            let output = u64::from(step.visible) + u64::from(reasoning);
            let repair_input = own_input + artifact(step.visible) + REPAIR_FRAME;
            if repair_input + output + 1024.max((length as u64).div_ceil(20)) > length as u64 {
                return Err(StoreError::Domain(
                    ErrorCode::CONTEXT_TOO_LARGE,
                    "Contest step does not fit the model context",
                ));
            }
            let (prompt, completion) = (price("prompt")?, price("completion")?);
            let cost = |tokens: u64| tokens as f64 * prompt + output as f64 * completion;
            first = first.max(cost(own_input));
            worst = worst.max(cost(repair_input));
            input = input.max(own_input);
        }
        out.estimated_usd += f64::from(step.calls) * first;
        let share = squad_store::accounting::reservation_micros(worst)
            .map_err(|_| invalid("Invalid step reservation"))?;
        let kind = kind_key(&step.kind);
        let entry = out.shares.entry(kind.clone()).or_insert(0);
        *entry = (*entry).max(share);
        let bound = out.input_bounds.entry(kind.clone()).or_insert(0);
        *bound = (*bound).max(input);
        *attempts_by_kind.entry(kind).or_insert(0) +=
            i64::from(step.calls) * i64::from(step.attempts);
        out.calls += step.calls;
    }
    for (kind, share) in &out.shares {
        out.reserved_micros = out
            .reserved_micros
            .checked_add(
                share
                    .checked_mul(attempts_by_kind[kind])
                    .ok_or_else(|| invalid("Reservation overflow"))?,
            )
            .ok_or_else(|| invalid("Reservation overflow"))?;
    }
    Ok(out)
}
