//! Read-only store adapter: never calls Store::open (which configures a writer).
use crate::args::Export;
use rusqlite::{Connection, OpenFlags};
use squad_core::export::{ExportError, ExportSnapshot, ExportSource};
use std::{io::Write, path::Path};

struct OfflineSource<'a>(&'a Path);
impl ExportSource for OfflineSource<'_> {
    fn snapshot(&self) -> Result<ExportSnapshot, ExportError> {
        let mut db = open_readonly(self.0).map_err(|_| ExportError::ReadFailed)?;
        // squad-store owns the transaction and event decoding. Only public events
        // cross this boundary; private artifact maps need a typed store adapter.
        let events = squad_store::export::room(&mut db).map_err(|_| ExportError::ReadFailed)?;
        Ok(ExportSnapshot {
            events,
            ..ExportSnapshot::default()
        })
    }
}
pub fn open_readonly(data_dir: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        data_dir.join("squad.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(db)
}
pub fn run(data_dir: &Path, args: &Export, stdout: &mut dyn Write) -> Result<(), String> {
    let markdown =
        squad_core::export::export_offline(&OfflineSource(data_dir), args.scope.clone(), args.id)
            .map_err(|error| error.to_string())?;
    if let Some(path) = &args.output {
        std::fs::write(path, markdown)
            .map_err(|_| format!("Could not write export: {}.", path.display()))?;
        writeln!(stdout, "{}", path.display())
    } else {
        write!(stdout, "{markdown}")
    }
    .map_err(|_| "Could not write export output.".to_owned())
}
