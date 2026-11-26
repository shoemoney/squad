use rmcp::schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartArgs {
    /// The brief each entrant should answer.
    pub prompt: String,
    /// Member nicknames. Omit this field to use the full roster.
    pub participants: Option<Vec<String>>,
    /// Optional UUID used to retry the same start operation.
    pub request_id: Option<String>,
    /// Bypass the usual cost confirmation threshold; requires server --allow-self-confirm.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmArgs {
    pub run_id: String,
    /// Approve the pending cost estimate with true, or decline with false.
    pub confirm: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunArgs {
    pub run_id: String,
}
