//! Cache-only inspection; no runtime lock, network requests or billed probes.
use crate::export::open_readonly;
use squad_protocol::config::SquadConfig;
use std::{fmt::Write, path::Path};

pub fn run(data_dir: &Path, config: Option<&SquadConfig>) -> Result<String, String> {
    if !data_dir.join("squad.db").exists() {
        return Ok("No startup snapshot. Open the room to resolve models.\n".into());
    }
    let db = open_readonly(data_dir).map_err(|_| "Could not read model cache.".to_owned())?;
    let mut query = db.prepare("SELECT nick,requested_slug,resolved_slug,role FROM models ORDER BY CASE role WHEN 'member' THEN 0 ELSE 1 END,roster_order").map_err(|_| "Could not read model cache.".to_owned())?;
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| "Could not read model cache.".to_owned())?;
    let mut text = String::new();
    for row in rows {
        let (nick, configured, resolved, role) =
            row.map_err(|_| "Could not read model cache.".to_owned())?;
        let _ = writeln!(text, "{nick} ({role}): {configured} -> {resolved}");
    }
    if text.is_empty() {
        text.push_str("No startup snapshot. Open the room to resolve models.\n");
    }
    if let Some(config) = config {
        let _ = writeln!(text, "Judge selection: {}", config.judge.model);
        let _ = writeln!(
            text,
            "Judge fallbacks: {}",
            config
                .judge
                .conflict_fallbacks
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        for agent in &config.agents {
            if !text.lines().any(|line| {
                line.starts_with(&format!("{} (member): {} ->", agent.nick, agent.model))
            }) {
                let _ = writeln!(
                    text,
                    "{}: resolution missing or stale; open the room to resolve models.",
                    agent.nick
                );
            }
        }
    }
    Ok(text)
}
