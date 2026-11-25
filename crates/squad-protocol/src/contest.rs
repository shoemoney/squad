//! Shared contest kind names.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContestKind {
    Debate,
    Tournament,
    Grill,
    Census,
    Crosscheck,
    Survey,
    Probe,
}
impl ContestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debate => "debate",
            Self::Tournament => "tournament",
            Self::Grill => "grill",
            Self::Census => "census",
            Self::Crosscheck => "crosscheck",
            Self::Survey => "survey",
            Self::Probe => "probe",
        }
    }
}
