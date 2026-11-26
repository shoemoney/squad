//! trace report and export projection belongs here.
use squad_protocol::ContestId;
pub fn render(_snapshot: &super::ExportSnapshot, _id: ContestId) -> Result<String,super::ExportError> {Err(super::ExportError::UnsupportedScope)}
