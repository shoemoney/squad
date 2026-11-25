//! Crosscheck's frozen artifacts. Provider responses never carry human acceptance.
use crate::{
    validation::{ValidationResult, ensure},
    *,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrosscheckStatus {
    Preflight,
    WaitingForParticipants,
    Extracting,
    Inventorying,
    BuildingGrid,
    Reviewing,
    Auditing,
    Ruling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl CrosscheckStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckGoal {
    pub goal: Text,
    pub constraints: Vec<Text>,
    pub uncertainties: Vec<Text>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckSetup {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub plan_version: u64,
    pub plan: Text,
    pub participants: Vec<MemberNick>,
    #[schemars(extend("x-required-nullable" = true))]
    pub goal: Option<CrosscheckGoal>,
}
impl CrosscheckSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.plan_version > 0
                && self.participants.len() >= 3
                && self.participants.iter().collect::<BTreeSet<_>>().len()
                    == self.participants.len(),
            "invalid crosscheck setup",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckState {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub plan_version: u64,
    pub grid_version: u64,
    pub state_version: u64,
    pub status: CrosscheckStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl CrosscheckState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && self.plan_version > 0,
            "invalid crosscheck state",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckPreflight {
    pub base: ContestPreflight,
    pub extraction_required: bool,
    pub reserved_attempts: u32,
}
impl CrosscheckPreflight {
    pub fn validate(&self) -> ValidationResult {
        let m = self.base.participants.len() as u32;
        let e = u32::from(self.extraction_required);
        ensure(
            self.base.kind == ContestKind::Crosscheck
                && m >= 3
                && self.base.estimate.model_calls == 2 * m + 4 + e
                && self.reserved_attempts == 4 * m + 10 + 3 * e,
            "invalid crosscheck call estimate",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrosscheckTransition {
    Confirm,
    ParticipantsReady,
    ExtractionCommitted,
    InventoriesCommitted,
    GridCommitted,
    ReviewsCommitted,
    AuditsCommitted,
    RulingCommitted,
    ReportCommitted,
    Stop,
    Fail,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CrosscheckLens {
    Stakeholders,
    Phases,
    Dependencies,
    FailureStates,
    OperatingConditions,
    DecisionRights,
}
impl CrosscheckLens {
    pub const ALL: [Self; 6] = [
        Self::Stakeholders,
        Self::Phases,
        Self::Dependencies,
        Self::FailureStates,
        Self::OperatingConditions,
        Self::DecisionRights,
    ];
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckInventoryItem {
    pub local_id: Text,
    pub item: Text,
    pub rationale: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckInventoryResponse {
    pub items: Vec<CrosscheckInventoryItem>,
}
impl CrosscheckInventoryResponse {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.items.len() == 3
                && self
                    .items
                    .iter()
                    .map(|i| i.local_id.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 3,
            "inventory requires three unique items",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckInventory {
    pub contest_id: ContestId,
    pub work_id: WorkId,
    pub member: MemberNick,
    pub lens: CrosscheckLens,
    pub response: CrosscheckInventoryResponse,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckSourceItem {
    pub inventory_event_id: EventId,
    pub local_id: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckNormalizedItem {
    pub id: Text,
    pub lens: CrosscheckLens,
    pub item: Text,
    pub source_item_ids: Vec<CrosscheckSourceItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckNormalization {
    pub items: Vec<CrosscheckNormalizedItem>,
    pub priority_order: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckCell {
    pub cell_id: CellId,
    pub key: String,
    pub items: Vec<Text>,
    pub originators: Vec<MemberNick>,
    #[schemars(extend("x-required-nullable" = true))]
    pub reviewer: Option<MemberNick>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckGrid {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub plan_version: u64,
    pub grid_version: u64,
    pub items: Vec<CrosscheckNormalizedItem>,
    pub cells: Vec<CrosscheckCell>,
    pub omitted: Vec<CrosscheckCell>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckCitation {
    pub location: Text,
    pub quote: Text,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CrosscheckDisposition {
    Covered,
    Inapplicable,
    Gap,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckCellReview {
    pub cell_id: CellId,
    pub status: CrosscheckDisposition,
    #[schemars(extend("x-required-nullable" = true))]
    pub citation: Option<CrosscheckCitation>,
    pub reason: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub proposed_fix: Option<Text>,
}
impl CrosscheckCellReview {
    pub fn validate(&self, plan: &str) -> ValidationResult {
        ensure(
            self.status != CrosscheckDisposition::Covered
                || self
                    .citation
                    .as_ref()
                    .is_some_and(|c| plan.contains(c.quote.as_str())),
            "covered cell requires a plan citation",
        )?;
        ensure(
            self.status != CrosscheckDisposition::Gap || self.proposed_fix.is_some(),
            "gap requires proposed fix",
        )?;
        ensure(
            self.status != CrosscheckDisposition::Covered
                || !self
                    .reason
                    .as_str()
                    .to_ascii_lowercase()
                    .contains("probably"),
            "probable coverage is a gap",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckReviewResponse {
    pub cells: Vec<CrosscheckCellReview>,
}
impl CrosscheckReviewResponse {
    pub fn validate(&self, assigned: &[CellId], plan: &str) -> ValidationResult {
        exact_cells(self.cells.iter().map(|c| c.cell_id), assigned)?;
        for cell in &self.cells {
            cell.validate(plan)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckReview {
    pub contest_id: ContestId,
    pub work_id: WorkId,
    pub reviewer: MemberNick,
    pub response: CrosscheckReviewResponse,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CrosscheckAuditVerdict {
    Upheld,
    Challenged,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckAuditFinding {
    pub cell_id: CellId,
    pub verdict: CrosscheckAuditVerdict,
    pub reason: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub citation: Option<CrosscheckCitation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckMissedCase {
    pub case: Text,
    pub reason: Text,
    pub proposed_fix: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckAuditResponse {
    pub audits: Vec<CrosscheckAuditFinding>,
    pub missed_case: CrosscheckMissedCase,
}
impl CrosscheckAuditResponse {
    pub fn validate(&self, sampled: &[CellId]) -> ValidationResult {
        exact_cells(self.audits.iter().map(|c| c.cell_id), sampled)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckAudit {
    pub contest_id: ContestId,
    pub work_id: WorkId,
    pub auditor: MemberNick,
    pub sample: Vec<CellId>,
    pub response: CrosscheckAuditResponse,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckAddition {
    pub audit_work_id: WorkId,
    pub decision: CrosscheckAdditionDecision,
    pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CrosscheckAdditionDecision {
    Added,
    Merged { cell_id: CellId },
    Rejected,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckRuling {
    pub cells: Vec<CrosscheckCellReview>,
    pub additions: Vec<CrosscheckAddition>,
    pub residual_risks: Vec<Text>,
}
impl CrosscheckRuling {
    pub fn validate(
        &self,
        grid: &CrosscheckGrid,
        audits: &[CrosscheckAudit],
        plan: &str,
    ) -> ValidationResult {
        exact_cells(
            self.cells.iter().map(|c| c.cell_id),
            &grid.cells.iter().map(|c| c.cell_id).collect::<Vec<_>>(),
        )?;
        for cell in &self.cells {
            cell.validate(plan)?;
        }
        ensure(
            self.additions.len() == audits.len()
                && self
                    .additions
                    .iter()
                    .map(|a| a.audit_work_id)
                    .collect::<BTreeSet<_>>()
                    == audits.iter().map(|a| a.work_id).collect(),
            "ruling must resolve every proposed addition once",
        )?;
        for a in &self.additions {
            if let CrosscheckAdditionDecision::Merged { cell_id } = a.decision {
                ensure(
                    grid.cells.iter().any(|c| c.cell_id == cell_id),
                    "addition merge references unknown cell",
                )?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckAcceptance {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub plan_version: u64,
    pub grid_version: u64,
    pub expected_state_version: u64,
    pub cell_ids: Vec<CellId>,
    pub residual_risk_ids: Vec<Text>,
    pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckLedgerCell {
    pub cell: CrosscheckCell,
    #[schemars(extend("x-required-nullable" = true))]
    pub review: Option<CrosscheckCellReview>,
    pub challenges: Vec<CrosscheckAuditFinding>,
    #[schemars(extend("x-required-nullable" = true))]
    pub ruling: Option<CrosscheckCellReview>,
    #[schemars(extend("x-required-nullable" = true))]
    pub acceptance_event_id: Option<EventId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckResidualRisk {
    pub id: Text,
    pub case: Text,
    pub proposed_fix: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub acceptance_event_id: Option<EventId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckReport {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub plan_version: u64,
    pub grid_version: u64,
    pub ledger: Vec<CrosscheckLedgerCell>,
    pub omitted: Vec<CrosscheckCell>,
    pub residual_risks: Vec<CrosscheckResidualRisk>,
    pub failed_inventories: Vec<MemberNick>,
    pub failed_reviewers: Vec<MemberNick>,
    pub failed_auditors: Vec<MemberNick>,
    pub audit_clean: bool,
    pub bounded_stop: bool,
}
/// These variants are also the complete visibility allowlist. Fresh member
/// contexts never deserialize a human transcript or an attributed grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "task",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CrosscheckCaseMaterial {
    ExtractGoal {
        plan: Text,
    },
    Inventory {
        goal: CrosscheckGoal,
        lens: CrosscheckLens,
    },
    NormalizeInventory {
        goal: CrosscheckGoal,
        inventories: Vec<(EventId, CrosscheckLens, CrosscheckInventoryResponse)>,
    },
    Review {
        plan: Text,
        cells: Vec<CrosscheckVisibleCell>,
    },
    Audit {
        plan: Text,
        grid: Vec<CrosscheckVisibleCell>,
        samples: Vec<CrosscheckCellReview>,
    },
    Rule {
        plan: Text,
        grid: CrosscheckGrid,
        reviews: Vec<CrosscheckReview>,
        audits: Vec<CrosscheckAudit>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckVisibleCell {
    pub cell_id: CellId,
    pub items: Vec<Text>,
}
impl CrosscheckCaseMaterial {
    pub fn composition(&self) -> &'static str {
        match self {
            Self::ExtractGoal { .. } => "crosscheck.normalize.extract_goal",
            Self::Inventory { .. } => "crosscheck.inventory",
            Self::NormalizeInventory { .. } => "crosscheck.normalize.normalize_inventory",
            Self::Review { .. } => "crosscheck.cell",
            Self::Audit { .. } => "crosscheck.audit",
            Self::Rule { .. } => "crosscheck.rule",
        }
    }
    pub fn response_schema(&self) -> serde_json::Value {
        match self {
            Self::ExtractGoal { .. } => validation::schema::<CrosscheckGoal>(),
            Self::Inventory { .. } => validation::schema::<CrosscheckInventoryResponse>(),
            Self::NormalizeInventory { .. } => validation::schema::<CrosscheckNormalization>(),
            Self::Review { .. } => validation::schema::<CrosscheckReviewResponse>(),
            Self::Audit { .. } => validation::schema::<CrosscheckAuditResponse>(),
            Self::Rule { .. } => validation::schema::<CrosscheckRuling>(),
        }
    }
}
pub fn exact_cells(actual: impl Iterator<Item = CellId>, expected: &[CellId]) -> ValidationResult {
    let ids = actual.collect::<Vec<_>>();
    ensure(
        ids.len() == expected.len()
            && ids.iter().copied().collect::<BTreeSet<_>>().len() == ids.len()
            && ids.iter().copied().collect::<BTreeSet<_>>() == expected.iter().copied().collect(),
        "cell coverage differs from frozen assignment",
    )
}

impl CrosscheckState {
    pub fn generic(&self) -> GenericContestState {
        GenericContestState {
            contest_id: self.contest_id,
            kind: ContestKind::Crosscheck,
            state_version: self.state_version,
            status: Text::new(
                serde_json::to_value(self.status)
                    .expect("status")
                    .as_str()
                    .expect("string"),
            )
            .expect("status"),
            step_id: self.step_id,
            artifact_event_ids: vec![],
            terminal_reason: self.terminal_reason.clone(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckGoalRecord {
    pub contest_id: ContestId,
    pub work_id: WorkId,
    pub goal: CrosscheckGoal,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrosscheckRulingRecord {
    pub contest_id: ContestId,
    pub work_id: WorkId,
    pub ruling: CrosscheckRuling,
}
