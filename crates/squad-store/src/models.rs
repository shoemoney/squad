//! Model-cache persistence; startup still owns configuration and resolution.
use rusqlite::{Connection, params};
use squad_protocol::{Actor, Model};
pub fn save_model(connection: &Connection, model: &Model) -> rusqlite::Result<()> {
    let (nick, role) = match &model.nick {
        Actor::Member(nick) if model.roster_order.is_some() => (nick.as_str(), "member"),
        Actor::Judge if model.roster_order.is_none() => ("judge", "judge"),
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let context_length =
        i64::try_from(model.context_length).map_err(|_| rusqlite::Error::InvalidQuery)?;
    if model.context_length == 0 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute("INSERT INTO models(nick,role,roster_order,requested_slug,resolved_slug,context_length,pricing_json,resolved_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(nick) DO UPDATE SET role=excluded.role,roster_order=excluded.roster_order,requested_slug=excluded.requested_slug,resolved_slug=excluded.resolved_slug,context_length=excluded.context_length,pricing_json=excluded.pricing_json,resolved_at=excluded.resolved_at",
        params![nick,role,model.roster_order,model.requested_slug.as_str(),model.resolved_slug.as_str(),context_length,model.pricing.to_string(),model.resolved_at.to_rfc3339()])?;
    Ok(())
}
