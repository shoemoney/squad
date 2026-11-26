//! tune script, bounds and pure tally functions belong here.
use squad_protocol::*;
pub fn prepare(_parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    Err(crate::prompts::PromptError::new(
        "tune",
        None,
        "mode is disabled",
    ))
}

pub fn drive(
    _router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    Ok(vec![])
}
