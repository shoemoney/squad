//! Model-cache persistence; startup still owns configuration and resolution.
use rusqlite::{Connection, params};
use squad_protocol::{Actor, Model};
pub fn save_model(connection: &Connection, model: &Model) -> Result<(), ModelError> {
    let (nick, role) = match &model.nick {
        Actor::Member(nick) if model.roster_order.is_some() => (nick.as_str(), "member"),
        Actor::Judge if model.roster_order.is_none() => ("judge", "judge"),
        _ => return Err(ModelError::InvalidModel),
    };
    let context_length =
        i64::try_from(model.context_length).map_err(|_| ModelError::InvalidModel)?;
    if model.context_length == 0 {
        return Err(ModelError::InvalidModel);
    }
    connection.execute("INSERT INTO models(nick,role,roster_order,requested_slug,resolved_slug,context_length,pricing_json,resolved_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(nick) DO UPDATE SET role=excluded.role,roster_order=excluded.roster_order,requested_slug=excluded.requested_slug,resolved_slug=excluded.resolved_slug,context_length=excluded.context_length,pricing_json=excluded.pricing_json,resolved_at=excluded.resolved_at",
        params![nick,role,model.roster_order,model.requested_slug.as_str(),model.resolved_slug.as_str(),context_length,model.pricing.to_string(),model.resolved_at.to_rfc3339()])?;
    Ok(())
}

#[derive(Debug)]
pub enum ModelError {
    InvalidModel,
    Database(rusqlite::Error),
}
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModel => {
                f.write_str("invalid model cache role, roster order or context length")
            }
            Self::Database(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ModelError {}
impl From<rusqlite::Error> for ModelError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
/// Replace the member cache as one atomic roster operation; retain the case judge.
/// Delete/reinsert is required because member rows cannot have NULL roster_order.
pub fn save_roster(tx: &rusqlite::Transaction<'_>, models: &[Model]) -> Result<(), ModelError> {
    let mut nicks = std::collections::HashSet::new();
    for (order, model) in models.iter().enumerate() {
        if !matches!(&model.nick, Actor::Member(nick) if nicks.insert(nick.clone()))
            || model.roster_order != u32::try_from(order).ok()
            || model.context_length == 0
            || i64::try_from(model.context_length).is_err()
        {
            return Err(ModelError::InvalidModel);
        }
    }
    // A savepoint preserves the previous roster even if the caller catches an error
    // and commits its surrounding startup transaction.
    tx.execute_batch("SAVEPOINT save_roster")?;
    let result = (|| {
        tx.execute("DELETE FROM models WHERE role='member'", [])?;
        for model in models {
            save_model(tx, model)?;
        }
        Ok(())
    })();
    if result.is_err() {
        tx.execute_batch("ROLLBACK TO save_roster")?;
    }
    tx.execute_batch("RELEASE save_roster")?;
    result
}
