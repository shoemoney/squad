use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
/// The descriptor owns the advisory lock; dropping it releases the room.
pub struct RoomLock(File);
impl RoomLock {
    pub fn acquire(path: &Path) -> std::io::Result<Self> {
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        if let Err(error) = file.try_lock_exclusive() {
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(error);
            }
            let mut pid = String::new();
            file.read_to_string(&mut pid)?;
            let pid = pid
                .trim()
                .parse::<u32>()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "unknown".into());
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!("Room main is already open in another squad process (pid {pid})."),
            ));
        }
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        write!(file, "{}", std::process::id())?;
        file.sync_all()?;
        Ok(Self(file))
    }
    pub fn file(&self) -> &File {
        &self.0
    }
}
