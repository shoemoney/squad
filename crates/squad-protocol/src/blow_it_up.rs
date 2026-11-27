//! blow_it_up contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = true;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlowItUpStatus {
    Preflight,
    WaitingForParticipants,
    Attacking,
    Challenging,
    Ruling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl BlowItUpStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: BlowItUpStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl BlowItUpState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid blow_it_up state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid blow_it_up round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlowItUpTransition {
    #[serde(rename = "blow_it_up.participants_ready")]
    ParticipantsReady,
}

pub const LENSES: [&str; 6] = [
    "outside attacker",
    "malicious insider or abusive user",
    "operator error and operations failure",
    "dependency and supply-chain failure",
    "incentives, cost and fraud",
    "legal, compliance and reputation",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpSetup {
    pub contest_id: ContestId,
    pub target: String,
    pub scope: String,
    pub participants: Vec<MemberNick>,
    pub lenses: std::collections::BTreeMap<MemberNick, String>,
    pub source_event_ids: Vec<EventId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpVector {
    pub local_id: Text,
    pub title: Text,
    pub target_quote: Text,
    pub exploit: Text,
    pub severity: Severity,
    pub likelihood: Likelihood,
    pub blast_radius: Text,
    pub diversity: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpAttack {
    #[schemars(length(min = 1, max = 6))]
    pub vectors: Vec<BlowItUpVector>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdditionalVector {
    pub local_id: Text,
    pub title: Text,
    pub target_quote: Text,
    pub exploit: Text,
    pub severity: Severity,
    pub likelihood: Likelihood,
    pub blast_radius: Text,
}
impl From<AdditionalVector> for BlowItUpVector {
    fn from(v: AdditionalVector) -> Self {
        Self {
            local_id: v.local_id,
            title: v.title,
            target_quote: v.target_quote,
            exploit: v.exploit,
            severity: v.severity,
            likelihood: v.likelihood,
            blast_radius: v.blast_radius,
            diversity: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PooledVector {
    pub vector_id: Text,
    pub title: Text,
    pub target_quote: Text,
    pub exploit: Text,
    pub severity: Severity,
    pub likelihood: Likelihood,
    pub blast_radius: Text,
    pub addition: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VectorProvenance {
    pub vector_id: Text,
    pub attacker: MemberNick,
    pub local_id: Text,
    pub addition: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpPool {
    pub vectors: Vec<PooledVector>,
    pub provenance: Vec<VectorProvenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChallengeVerdict {
    pub vector_id: Text,
    pub verdict: Verdict,
    pub reason: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub duplicate_of: Option<Text>,
    #[schemars(extend("x-required-nullable" = true))]
    pub defeating_quote: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpChallenge {
    #[schemars(length(max = 72))]
    pub verdicts: Vec<ChallengeVerdict>,
    #[schemars(length(max = 2))]
    pub additions: Vec<AdditionalVector>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VectorTally {
    pub vector_id: Text,
    pub accepted: u32,
    pub false_positive: u32,
    pub weak: u32,
    pub status: VectorStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpTally {
    pub vectors: Vec<VectorTally>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub finding_id: Text,
    pub title: Text,
    #[schemars(length(min = 1, max = 96))]
    pub source_vector_ids: Vec<Text>,
    pub severity: Severity,
    pub likelihood: Likelihood,
    pub exploit: Text,
    pub blast_radius: Text,
    pub mitigation: Text,
    pub rebuttal_status: RebuttalStatus,
    pub residual: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub challenge_rejection: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KilledNote {
    pub vector_id: Text,
    pub note: Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpRuling {
    #[schemars(length(max = 96))]
    pub findings: Vec<Finding>,
    #[schemars(length(max = 72))]
    pub killed_notes: Vec<KilledNote>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedFinding {
    pub finding: Finding,
    pub score: u32,
    pub finder_count: u32,
    pub lone: bool,
    pub addition: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KilledVector {
    pub vector: PooledVector,
    pub tally: VectorTally,
    pub defeating_quotes: Vec<Text>,
    pub judge_note: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkippedMember {
    pub member: MemberNick,
    pub step: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpReport {
    pub findings: Vec<RankedFinding>,
    pub killed: Vec<KilledVector>,
    pub additions: Vec<PooledVector>,
    pub unchallenged: Vec<Text>,
    pub skipped: Vec<SkippedMember>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpCaseMaterial {
    pub target: String,
    pub scope: String,
    pub vectors: Vec<PooledVector>,
    pub tallies: BlowItUpTally,
    pub challenges: Vec<ChallengeVerdict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Likelihood {
    Likely,
    Possible,
    Unlikely,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Holds,
    Weak,
    FalsePositive,
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorStatus {
    Killed,
    Contested,
    Standing,
    Unchallenged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RebuttalStatus {
    Survived,
    Weakened,
}

impl Severity {
    pub fn weight(self) -> u32 {
        match self {
            Self::Critical => 4,
            Self::High => 3,
            Self::Medium => 2,
            Self::Low => 1,
        }
    }
}
impl Likelihood {
    pub fn weight(self) -> u32 {
        match self {
            Self::Likely => 3,
            Self::Possible => 2,
            Self::Unlikely => 1,
        }
    }
}
impl BlowItUpSetup {
    pub fn calls(&self) -> u32 {
        self.participants.len() as u32 * 2 + 1
    }
    pub fn attempts(&self) -> u32 {
        self.participants.len() as u32 * 4 + 3
    }
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (2..=12).contains(&self.participants.len())
                && unique(&self.participants)
                && !self.target.trim().is_empty(),
            "invalid target or participants",
        )?;
        ensure(
            self.lenses.len() == self.participants.len()
                && self.participants.iter().all(|n| {
                    self.lenses
                        .get(n)
                        .is_some_and(|l| LENSES.contains(&l.as_str()))
                }),
            "invalid lens draw",
        )
    }
}
impl BlowItUpAttack {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=6).contains(&self.vectors.len())
                && self.vectors.iter().filter(|v| v.diversity).count() == 1,
            "exactly one diversity vector required",
        )?;
        ensure(
            unique(&self.vectors.iter().map(|v| &v.local_id).collect::<Vec<_>>()),
            "duplicate localId",
        )
    }
}
impl BlowItUpChallenge {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.additions.len() <= 2
                && self.verdicts.len() <= 72
                && unique(
                    &self
                        .verdicts
                        .iter()
                        .map(|v| &v.vector_id)
                        .collect::<Vec<_>>(),
                ),
            "invalid challenge cardinality",
        )?;
        ensure(
            unique(
                &self
                    .additions
                    .iter()
                    .map(|v| &v.local_id)
                    .collect::<Vec<_>>(),
            ),
            "duplicate addition localId",
        )?;
        for v in &self.verdicts {
            ensure(
                v.reason.as_str().split_whitespace().count() <= 20,
                "challenge reason exceeds 20 words",
            )?;
            ensure(
                (v.verdict == Verdict::FalsePositive) == v.defeating_quote.is_some(),
                "false_positive requires defeatingQuote only",
            )?;
            ensure(
                (v.verdict == Verdict::Duplicate) == v.duplicate_of.is_some(),
                "duplicate requires duplicateOf only",
            )?;
            ensure(
                v.duplicate_of.as_ref() != Some(&v.vector_id),
                "self duplicate",
            )?;
        }
        Ok(())
    }
}
impl BlowItUpRuling {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.findings.len() <= 96
                && self.killed_notes.len() <= 72
                && unique(
                    &self
                        .findings
                        .iter()
                        .map(|v| &v.finding_id)
                        .collect::<Vec<_>>(),
                )
                && unique(
                    &self
                        .killed_notes
                        .iter()
                        .map(|v| &v.vector_id)
                        .collect::<Vec<_>>(),
                ),
            "invalid ruling cardinality",
        )?;
        for f in &self.findings {
            ensure(
                !f.source_vector_ids.is_empty() && unique(&f.source_vector_ids),
                "finding requires unique sources",
            )?;
        }
        Ok(())
    }
    pub fn validate_against(
        &self,
        pool: &[PooledVector],
        tally: &BlowItUpTally,
    ) -> ValidationResult {
        self.validate()?;
        for f in &self.findings {
            for id in &f.source_vector_ids {
                ensure(
                    pool.iter().any(|v| &v.vector_id == id),
                    "unknown source vector",
                )?;
                let t = tally.vectors.iter().find(|t| &t.vector_id == id);
                ensure(
                    t.is_some_and(|t| t.status != VectorStatus::Killed),
                    "killed source cannot be reinstated",
                )?;
                if t.is_some_and(|t| t.status == VectorStatus::Contested) {
                    ensure(
                        f.rebuttal_status == RebuttalStatus::Weakened
                            || f.challenge_rejection.is_some(),
                        "contested sources require weakened or explanation",
                    )?;
                }
            }
        }
        for n in &self.killed_notes {
            ensure(
                tally
                    .vectors
                    .iter()
                    .any(|t| t.vector_id == n.vector_id && t.status == VectorStatus::Killed),
                "note requires killed vector",
            )?;
        }
        Ok(())
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    use validation::decode;
    match kind {
        "blow_it_up.attack" => decode::<BlowItUpAttack>(body)?.validate(),
        "blow_it_up.challenge" => decode::<BlowItUpChallenge>(body)?.validate(),
        "blow_it_up.rule" => decode::<BlowItUpRuling>(body)?.validate(),
        "blow_it_up.report" => {
            decode::<BlowItUpReport>(body)?;
            Ok(())
        }
        "blow_it_up.pool" => {
            decode::<BlowItUpPool>(body)?;
            Ok(())
        }
        "blow_it_up.tally" => {
            decode::<BlowItUpTally>(body)?;
            Ok(())
        }
        "blow_it_up.case" => {
            decode::<BlowItUpCaseMaterial>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown blow_it_up artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::BlowItUpAttack => validation::schema::<BlowItUpAttack>(),
        WorkKind::BlowItUpChallenge => validation::schema::<BlowItUpChallenge>(),
        WorkKind::BlowItUpRule => validation::schema::<BlowItUpRuling>(),
        _ => return None,
    })
}
pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    match kind {
        WorkKind::BlowItUpAttack => Some("blow_it_up.attack"),
        WorkKind::BlowItUpChallenge => Some("blow_it_up.challenge"),
        WorkKind::BlowItUpRule => Some("blow_it_up.rule"),
        _ => None,
    }
}

impl BlowItUpCaseMaterial {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.target.trim().is_empty() && self.vectors.len() <= 96,
            "invalid case target or pool",
        )?;
        ensure(
            unique(
                &self
                    .vectors
                    .iter()
                    .map(|v| &v.vector_id)
                    .collect::<Vec<_>>(),
            ),
            "duplicate pooled vector",
        )?;
        ensure(
            self.tallies.vectors.len() == self.vectors.len()
                && unique(
                    &self
                        .tallies
                        .vectors
                        .iter()
                        .map(|v| &v.vector_id)
                        .collect::<Vec<_>>(),
                ),
            "invalid case tally coverage",
        )?;
        for t in &self.tallies.vectors {
            ensure(
                self.vectors.iter().any(|v| v.vector_id == t.vector_id)
                    && t.accepted <= 11
                    && t.false_positive <= t.accepted
                    && t.weak <= t.accepted - t.false_positive,
                "invalid case tally",
            )?;
            let expected = if t.accepted < 2 {
                VectorStatus::Unchallenged
            } else if t.false_positive * 2 > t.accepted {
                VectorStatus::Killed
            } else if (t.false_positive + t.weak) * 2 > t.accepted {
                VectorStatus::Contested
            } else {
                VectorStatus::Standing
            };
            ensure(t.status == expected, "case tally status mismatch")?;
        }
        for c in &self.challenges {
            ensure(
                self.vectors.iter().any(|v| v.vector_id == c.vector_id),
                "unknown challenge vector",
            )?;
        }
        Ok(())
    }
}
