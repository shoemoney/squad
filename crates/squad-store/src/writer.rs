//! One owning SQLite writer thread. Jobs never execute on executor threads.
use crate::{Result, StoreError};
use rusqlite::Connection;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
#[derive(Debug)]
pub enum RoomOpenError {
    Lock(std::io::Error),
    Store(StoreError),
}
impl std::fmt::Display for RoomOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lock(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for RoomOpenError {}
impl From<StoreError> for RoomOpenError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Store {
    sender: Arc<mpsc::Sender<Job>>,
    vacuum_pending: Arc<AtomicBool>,
}
impl Store {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::with_lock(path.as_ref(), None)
    }
    /// The writer owns the lock until all queued jobs finish and SQLite closes.
    pub fn open_room(data_dir: &std::path::Path) -> std::result::Result<Self, RoomOpenError> {
        crate::open::private_dir(data_dir).map_err(RoomOpenError::Lock)?;
        let lock = crate::room_lock::RoomLock::acquire(&data_dir.join("main.lock"))
            .map_err(RoomOpenError::Lock)?;
        Self::with_lock(&data_dir.join("squad.db"), Some(lock)).map_err(RoomOpenError::Store)
    }
    fn with_lock(path: &std::path::Path, lock: Option<crate::room_lock::RoomLock>) -> Result<Self> {
        let mut connection = crate::open(path)?;
        crate::schema::migrate(&mut connection)?;
        connection.execute_batch("INSERT OR IGNORE INTO room_meta VALUES ('main',2,1,1,1,NULL); INSERT OR IGNORE INTO vars(name,value) VALUES ('ROUNDS',10),('GRILL_ROUNDS',2),('CENSUS_PASSES',2);")?;
        let (sender, receiver) = mpsc::channel::<Job>();
        let vacuum_pending = Arc::new(AtomicBool::new(false));
        let retry_vacuum = vacuum_pending.clone();
        std::thread::Builder::new()
            .name("squad-writer".into())
            .spawn(move || {
                let room_lock = lock;
                while let Ok(job) = receiver.recv() {
                    if retry_vacuum.load(Ordering::Acquire)
                        && connection.execute_batch("VACUUM").is_ok()
                    {
                        retry_vacuum.store(false, Ordering::Release);
                    }
                    job(&mut connection);
                }
                drop(connection);
                drop(room_lock);
            })
            .map_err(|_| StoreError::Unavailable)?;
        Ok(Self {
            sender: Arc::new(sender),
            vacuum_pending,
        })
    }
    /// A failed cleanup remains queued for the next writer operation.
    pub fn vacuum_after_clear(&self) -> Result<()> {
        self.vacuum_pending.store(true, Ordering::Release);
        let pending = self.vacuum_pending.clone();
        self.call(move |db| {
            if pending.load(Ordering::Acquire) {
                db.execute_batch("VACUUM")?;
            }
            pending.store(false, Ordering::Release);
            Ok(())
        })
    }
    pub fn call<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Box::new(move |connection| {
                let _ = sender.send(job(connection));
            }))
            .map_err(|_| StoreError::Unavailable)?;
        receiver.recv().map_err(|_| StoreError::Unavailable)?
    }
}
