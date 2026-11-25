use squad_protocol::ErrorCode;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    Domain(ErrorCode, &'static str),
    Unavailable,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain(_, message) => f.write_str(message),
            Self::Sql(_) => f.write_str("Storage operation failed"),
            Self::Json(_) => f.write_str("Invalid stored contract"),
            Self::Unavailable => f.write_str("Store unavailable"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, StoreError>;
pub(crate) fn invalid(message: &'static str) -> StoreError {
    StoreError::Domain(ErrorCode::INVALID_COMMAND, message)
}
pub(crate) fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    Ok(serde_json::from_str(&value)?)
}
