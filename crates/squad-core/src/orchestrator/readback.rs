//! readback script, bounds and pure tally functions belong here.
use squad_protocol::*;
pub fn prepare(_parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
 Err(crate::prompts::PromptError::new("readback",None,"mode is disabled"))
}
