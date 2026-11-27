//! trace contracts; enabled only after the mode implementation lands.
use crate::validation::{self, ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TraceStatus {
    Preflight,
    WaitingForParticipants,
    Extracting,
    Merging,
    Verifying,
    Assembling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl TraceStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: TraceStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl TraceState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid trace state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid trace round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceSource {
    pub source_id: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceSetup {
    pub question: String,
    pub sources: Vec<TraceSource>,
    pub participants: Vec<MemberNick>,
    pub room_words: u32,
    pub plan_words: u32,
    pub models: BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: BTreeMap<String, PromptRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceQuote {
    pub source_id: String,
    pub quote: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceExtractedClaim {
    pub local_id: String,
    pub claim: String,
    pub quotes: Vec<TraceQuote>,
    pub counter_quotes: Vec<TraceQuote>,
    pub scope_note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceExtraction {
    #[schemars(length(max = 10))]
    pub claims: Vec<TraceExtractedClaim>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceQuoteCheck {
    pub member_label: String,
    pub local_id: String,
    pub counter: bool,
    pub quote: TraceQuote,
    pub status: String,
    pub span: Option<TraceSpan>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceSpan {
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceEvidence {
    pub source_id: String,
    pub quote: String,
    pub span: TraceSpan,
    pub counter: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceAssignedClaim {
    pub claim_id: String,
    pub text: String,
    pub quotes: Vec<TraceEvidence>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceClaimRef {
    pub member_label: String,
    pub local_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceCanonicalClaim {
    pub claim_id: String,
    pub text: String,
    pub sources: Vec<TraceClaimRef>,
    pub shared_origin: Vec<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceClaims {
    #[schemars(length(max = 25))]
    pub claims: Vec<TraceCanonicalClaim>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceVerdict {
    pub claim_id: String,
    pub entailment: TraceEntailment,
    pub scope: TraceScope,
    #[schemars(extend("x-required-nullable"=true))]
    pub qualifier_quote: Option<TraceQuote>,
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceVerification {
    pub verdicts: Vec<TraceVerdict>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceLedgerEntry {
    pub claim_id: String,
    pub text: String,
    pub status: TraceClaimStatus,
    pub quotes: Vec<TraceQuoteCheck>,
    pub independent_sources: u32,
    pub contributing_members: u32,
    pub verdicts: BTreeMap<MemberNick, TraceVerdict>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceLedger {
    pub claims: Vec<TraceLedgerEntry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceParagraph {
    pub text: String,
    pub claim_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceSection {
    pub heading: String,
    pub paragraphs: Vec<TraceParagraph>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceDisputed {
    pub claim_id: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceBrief {
    pub sections: Vec<TraceSection>,
    pub disputed: Vec<TraceDisputed>,
    pub gaps: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceReport {
    pub question: String,
    pub sources: Vec<TraceSource>,
    pub extractions: BTreeMap<String, TraceExtraction>,
    pub provenance: BTreeMap<String, MemberNick>,
    pub quote_checks: Vec<TraceQuoteCheck>,
    pub claims: TraceClaims,
    pub assignments: BTreeMap<MemberNick, Vec<String>>,
    pub verifications: BTreeMap<MemberNick, TraceVerification>,
    pub ledger: TraceLedger,
    pub brief: Option<TraceBrief>,
    pub skipped: Vec<String>,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceCaseMaterial {
    pub question: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraceEntailment {
    Supported,
    Partial,
    NotSupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraceScope {
    Ok,
    Overstated,
    MissingQualifier,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraceClaimStatus {
    Supported,
    Disputed,
    Unsupported,
    Unverified,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(tag = "action")]
pub enum TraceTransition {
    Advance,
}

impl TraceSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.question.trim().is_empty()
                && (1..=12).contains(&self.sources.len())
                && (3..=12).contains(&self.participants.len())
                && unique(&self.participants),
            "invalid trace setup",
        )?;
        ensure(
            self.sources
                .iter()
                .enumerate()
                .all(|(i, s)| s.source_id == format!("S{}", i + 1) && !s.text.trim().is_empty()),
            "invalid numbered sources",
        )?;
        ensure(
            self.room_words > 0
                && self.plan_words > 0
                && self.models.contains_key("judge")
                && self
                    .participants
                    .iter()
                    .all(|n| self.models.contains_key(n.as_str())),
            "missing trace binding",
        )
    }
}
impl TraceExtraction {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.claims.len() <= 10
                && unique(&self.claims.iter().map(|c| &c.local_id).collect::<Vec<_>>()),
            "invalid extraction IDs",
        )?;
        for c in &self.claims {
            ensure(
                !c.local_id.trim().is_empty()
                    && !c.claim.trim().is_empty()
                    && (1..=3).contains(&c.quotes.len())
                    && c.counter_quotes.len() <= 2,
                "invalid extracted claim",
            )?;
            for q in c.quotes.iter().chain(&c.counter_quotes) {
                ensure(
                    !q.source_id.is_empty()
                        && !q.quote.trim().is_empty()
                        && q.quote.split_whitespace().count() <= 60,
                    "invalid quote",
                )?;
            }
        }
        Ok(())
    }
}
impl TraceClaims {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.claims.len() <= 25
                && unique(&self.claims.iter().map(|c| &c.claim_id).collect::<Vec<_>>()),
            "invalid canonical IDs",
        )?;
        for c in &self.claims {
            ensure(
                !c.claim_id.is_empty()
                    && !c.text.trim().is_empty()
                    && !c.sources.is_empty()
                    && unique(&c.sources),
                "canonical claim requires source references",
            )?;
            ensure(
                c.shared_origin.iter().all(|g| g.len() >= 2 && unique(g)),
                "invalid shared origin group",
            )?;
        }
        Ok(())
    }
}
impl TraceVerification {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            unique(
                &self
                    .verdicts
                    .iter()
                    .map(|v| &v.claim_id)
                    .collect::<Vec<_>>(),
            ),
            "duplicate verdict",
        )?;
        for v in &self.verdicts {
            ensure(
                !v.claim_id.is_empty()
                    && (v.scope != TraceScope::MissingQualifier || v.qualifier_quote.is_some()),
                "missing qualifier quote",
            )?;
        }
        Ok(())
    }
}
impl TraceBrief {
    pub fn validate(&self) -> ValidationResult {
        for p in self.sections.iter().flat_map(|s| &s.paragraphs) {
            ensure(
                !p.text.trim().is_empty() && !p.claim_ids.is_empty() && unique(&p.claim_ids),
                "paragraph requires claim IDs",
            )?;
        }
        Ok(())
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "trace.extract" => validation::decode::<TraceExtraction>(body)?.validate(),
        "trace.merge" => validation::decode::<TraceClaims>(body)?.validate(),
        "trace.verify" => validation::decode::<TraceVerification>(body)?.validate(),
        "trace.assemble" => validation::decode::<TraceBrief>(body)?.validate(),
        "trace.quote_checks" => {
            validation::decode::<Vec<TraceQuoteCheck>>(body)?;
            Ok(())
        }
        "trace.ledger" => {
            validation::decode::<TraceLedger>(body)?;
            Ok(())
        }
        "trace.report" => {
            validation::decode::<TraceReport>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown trace artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::TraceExtract => validation::schema::<TraceExtraction>(),
        WorkKind::TraceMerge => validation::schema::<TraceClaims>(),
        WorkKind::TraceVerify => validation::schema::<TraceVerification>(),
        WorkKind::TraceAssemble => validation::schema::<TraceBrief>(),
        _ => return None,
    })
}
pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    Some(match kind {
        WorkKind::TraceExtract => "trace.extract",
        WorkKind::TraceMerge => "trace.merge",
        WorkKind::TraceVerify => "trace.verify",
        WorkKind::TraceAssemble => "trace.assemble",
        _ => return None,
    })
}
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => s.split_whitespace().count(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o.values().map(prose_words).sum(),
        _ => 0,
    }
}
