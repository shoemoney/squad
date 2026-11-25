//! Read-only store adapter: never calls Store::open (which configures a writer).
use crate::args::Export;
use rusqlite::{Connection, OpenFlags};
use std::{io::Write, path::Path};

pub fn open_readonly(data_dir: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        data_dir.join("squad.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(db)
}
pub fn run(data_dir: &Path, args: &Export, stdout: &mut dyn Write) -> Result<(), String> {
    let markdown = squad_core::room::export_offline(data_dir, "main", args.scope.clone(), args.id)
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.output {
        std::fs::write(path, markdown)
            .map_err(|_| format!("Could not write export: {}.", path.display()))?;
        writeln!(stdout, "{}", path.display())
    } else {
        write!(stdout, "{}", squad_tui::markdown::sanitize(&markdown))
    }
    .map_err(|_| "Could not write export output.".to_owned())
}
