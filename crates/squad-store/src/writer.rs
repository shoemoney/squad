//! One owning SQLite writer thread. Jobs never execute on executor threads.
use crate::{Result, StoreError};
use rusqlite::Connection;
use std::sync::{Arc, mpsc};
type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Store {
    sender: Arc<mpsc::Sender<Job>>,
}
impl Store {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut connection = crate::open(path)?;
        crate::schema::migrate(&mut connection)?;
        connection.execute_batch("INSERT OR IGNORE INTO room_meta VALUES ('main',1,1,1,1,NULL); INSERT OR IGNORE INTO vars(name,value) VALUES ('ROUNDS',10),('GRILL_ROUNDS',2),('CENSUS_PASSES',2);")?;
        let (sender, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("squad-writer".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job(&mut connection);
                }
            })
            .map_err(|_| StoreError::Unavailable)?;
        Ok(Self {
            sender: Arc::new(sender),
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
