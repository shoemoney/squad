//! Single room API for human clients. Subscribe before loading the initial page.
//! On broadcast lag, replay committed events with `events_after(last_seen, limit)`;
//! provisional token text is never reconstructed from SQLite.
use crate::{bus::InProcessBus, router::Router, state::RoomState};
use squad_agents::{
    openrouter::OpenRouterClient,
    resolver::{self, ResolutionSnapshot},
};
use squad_protocol::{config::SquadConfig, *};
use squad_store::{Store, StoreError, room_lock::RoomLock};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomError {
    pub code: ErrorCode,
    pub message: String,
}
impl RoomError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for RoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RoomError {}
impl From<StoreError> for RoomError {
    fn from(value: StoreError) -> Self {
        let code = match &value {
            StoreError::Domain(code, _) => *code,
            _ => ErrorCode::INTERNAL_ERROR,
        };
        Self::new(code, value.to_string())
    }
}
pub type RoomFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, RoomError>> + Send + 'a>>;
/// Integration seam for the owning runtime; doubles need no provider workers.
pub trait RoomRuntime: Send + Sync {
    fn ready(&mut self) -> RoomFuture<'_, ()>;
    fn wake(&self, work_ids: &[WorkId]);
    fn pause(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn reset(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown(self: Box<Self>) -> RoomFuture<'static, ()>;
}
pub trait RuntimeLauncher: Send + Sync {
    /// Run owning recovery before constructing the router or starting workers.
    fn recover_on_open_mode(
        &self,
        store: &Store,
        mode: OpenMode,
        now: DateTime,
    ) -> Result<(), RoomError>;
    #[allow(clippy::too_many_arguments)]
    fn start_with_mode(
        &self,
        config: &SquadConfig,
        store: Store,
        bus: InProcessBus,
        client: OpenRouterClient,
        resolution: ResolutionSnapshot,
        mode: OpenMode,
    ) -> Result<Box<dyn RoomRuntime>, RoomError>;
}
struct DefaultLauncher;
struct LiveRuntime {
    handle: Option<crate::runtime::RuntimeHandle>,
    config: SquadConfig,
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    resolution: ResolutionSnapshot,
    mode: OpenMode,
}
fn runtime_error(error: ReaderError) -> RoomError {
    RoomError::new(
        ErrorCode::UNAVAILABLE,
        format!("Executor runtime: {error:?}"),
    )
}
impl RoomRuntime for LiveRuntime {
    fn ready(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async {
            self.handle
                .as_mut()
                .expect("runtime")
                .ready()
                .await
                .map_err(runtime_error)
        })
    }
    fn wake(&self, ids: &[WorkId]) {
        self.handle.as_ref().expect("runtime").wake(ids);
    }
    fn pause(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async {
            if let Some(handle) = self.handle.take() {
                handle.shutdown().await.map_err(runtime_error)?;
            }
            Ok(())
        })
    }
    fn reset(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async {
            if let Some(handle) = self.handle.take() {
                handle.shutdown().await.map_err(runtime_error)?;
            }
            self.handle = Some(
                crate::runtime::start_with_mode(
                    &self.config,
                    self.store.clone(),
                    self.bus.clone(),
                    self.client.clone(),
                    self.resolution.clone(),
                    self.mode.clone(),
                )
                .map_err(runtime_error)?,
            );
            self.ready().await
        })
    }
    fn shutdown(mut self: Box<Self>) -> RoomFuture<'static, ()> {
        Box::pin(async move {
            if let Some(handle) = self.handle.take() {
                handle.shutdown().await.map_err(runtime_error)?;
            }
            Ok(())
        })
    }
}
impl RuntimeLauncher for DefaultLauncher {
    fn recover_on_open_mode(
        &self,
        store: &Store,
        mode: OpenMode,
        now: DateTime,
    ) -> Result<(), RoomError> {
        crate::router::recovery::recover_on_open_mode(store, mode, now)?;
        Ok(())
    }
    fn start_with_mode(
        &self,
        config: &SquadConfig,
        store: Store,
        bus: InProcessBus,
        client: OpenRouterClient,
        resolution: ResolutionSnapshot,
        mode: OpenMode,
    ) -> Result<Box<dyn RoomRuntime>, RoomError> {
        let handle = crate::runtime::start_with_mode(
            config,
            store.clone(),
            bus.clone(),
            client.clone(),
            resolution.clone(),
            mode.clone(),
        )
        .map_err(runtime_error)?;
        Ok(Box::new(LiveRuntime {
            handle: Some(handle),
            config: config.clone(),
            store,
            bus,
            client,
            resolution,
            mode,
        }))
    }
}
pub fn default_launcher() -> impl RuntimeLauncher {
    DefaultLauncher
}
fn configured_client(config: &SquadConfig) -> Result<OpenRouterClient, RoomError> {
    OpenRouterClient::from_config(config)
        .map_err(|e| RoomError::new(ErrorCode::CONFIG_INVALID, e.to_string()))
}
// Explicit unlock also releases inherited descriptors during concurrent process startup.
struct RoomLease(RoomLock);
impl Drop for RoomLease {
    fn drop(&mut self) {
        let _ = self.0.file().unlock();
    }
}
struct Resources {
    store: Store,
    bus: InProcessBus,
    runtime: Box<dyn RoomRuntime>,
    _lock: RoomLease,
}
pub struct RoomHandle {
    resources: Mutex<Option<Resources>>,
    gate: tokio::sync::RwLock<()>,
    submissions: tokio::sync::Mutex<()>,
    mode: OpenMode,
    roster: Vec<MemberNick>,
    daily_max: i64,
    cached: Mutex<RoomState>,
    observations: Mutex<(FeedReceiver, BTreeMap<MemberNick, Presence>)>,
    offline: BTreeMap<MemberNick, Presence>,
}
impl RoomHandle {
    pub async fn open(config: SquadConfig, room: &str, mode: OpenMode) -> Result<Self, RoomError> {
        if room != "main" || config.room != "main" {
            return Err(RoomError::new(
                ErrorCode::ROOM_NOT_FOUND,
                "Only room main is supported",
            ));
        }
        let client = configured_client(&config)?;
        Self::open_with_client(config, room, mode, client).await
    }
    pub async fn open_with_client(
        config: SquadConfig,
        room: &str,
        mode: OpenMode,
        client: OpenRouterClient,
    ) -> Result<Self, RoomError> {
        Self::open_with_launcher(config, room, mode, client, &default_launcher()).await
    }
    /// Provider and runtime injection for deterministic local fixtures.
    pub async fn open_with_launcher(
        config: SquadConfig,
        room: &str,
        mode: OpenMode,
        client: OpenRouterClient,
        launcher: &dyn RuntimeLauncher,
    ) -> Result<Self, RoomError> {
        if room != "main" || config.room != "main" {
            return Err(RoomError::new(
                ErrorCode::ROOM_NOT_FOUND,
                "Only room main is supported",
            ));
        }
        squad_agents::registry::Registry::new(&config)
            .map_err(|e| RoomError::new(ErrorCode::CONFIG_INVALID, e.to_string()))?;
        let daily_max = squad_store::accounting::reported_micros(config.budget.daily_max_usd)
            .map_err(|_| RoomError::new(ErrorCode::CONFIG_INVALID, "Invalid daily max"))?;
        std::fs::create_dir_all(&config.data_dir).map_err(|_| {
            RoomError::new(ErrorCode::UNAVAILABLE, "Cannot create room data directory")
        })?;
        let lock = RoomLock::acquire(&config.data_dir.join("main.lock"))
            .map_err(|e| RoomError::new(ErrorCode::UNAVAILABLE, e.to_string()))?;
        let lock = RoomLease(lock);
        let path = config.data_dir.join("squad.db");
        let existing_vars = existing_variables(&path)?;
        let store = Store::open(&path)?;
        let rounds = config.defaults.rounds;
        let grill = config.defaults.grill_rounds;
        let census = config.defaults.census_passes;
        store.call(move |db| {
            for (name, value) in [
                ("ROUNDS", rounds),
                ("GRILL_ROUNDS", grill),
                ("CENSUS_PASSES", census),
            ] {
                if !existing_vars.iter().any(|stored| stored == name) {
                    db.execute(
                        "UPDATE vars SET value=? WHERE name=?",
                        rusqlite::params![value, name],
                    )?;
                }
            }
            Ok(())
        })?;
        let mut persistence =
            crate::resolution::ResolutionPersistence::new(store.clone(), daily_max)?;
        let resolution = resolver::resolve_persisted(
            &client,
            &config,
            chrono::Utc::now().timestamp() as u64,
            &mut persistence,
        )
        .await
        .map_err(|e| {
            let code = match &e {
                resolver::ResolutionError::Configuration(_) => ErrorCode::CONFIG_INVALID,
                _ => ErrorCode::MODEL_CATALOG_UNAVAILABLE,
            };
            RoomError::new(code, e.to_string())
        })?;
        launcher.recover_on_open_mode(&store, mode.clone(), chrono::Utc::now())?;
        let mut bindings = BTreeMap::new();
        for (index, pinned) in resolution
            .members
            .iter()
            .chain(std::iter::once(&resolution.judge))
            .enumerate()
        {
            let Some(model) = &pinned.model else {
                continue;
            };
            let judge = index == config.agents.len();
            let effort = if judge {
                config.judge.reasoning_effort
            } else {
                config.agents[index]
                    .reasoning_effort
                    .unwrap_or(config.defaults.reasoning_effort)
            };
            bindings.insert(
                pinned.nick.clone(),
                ExecutorBinding {
                    model: Text::new(model.id.clone()).map_err(|_| {
                        RoomError::new(ErrorCode::CONFIG_INVALID, "Invalid resolved model")
                    })?,
                    effort,
                },
            );
        }
        let roster: Vec<_> = config.agents.iter().map(|a| a.nick.clone()).collect();
        let mut router = Router::new(store.clone(), roster.clone(), bindings, daily_max)?
            .with_confirmation_threshold(config.budget.debate_confirm_usd)?;
        router.variable_defaults = (&config.defaults).into();
        router.engine_config = Some(config.clone());
        if mode == OpenMode::OneShot {
            router.engine_scope = Some(Default::default());
        } else {
            router.drive_contests()?;
        }
        let bus = InProcessBus::start(router);
        let receiver = bus.subscribe();
        let offline: BTreeMap<_, _> = resolution
            .members
            .iter()
            .filter(|p| p.model.is_none())
            .filter_map(|p| {
                let nick: MemberNick = p.nick.parse().ok()?;
                Some((
                    nick.clone(),
                    Presence {
                        nick,
                        status: PresenceStatus::Offline,
                        work_id: None,
                        queue_depth: 0,
                        error_code: Some(ErrorCode::MODEL_UNAVAILABLE),
                        observed_at: chrono::Utc::now(),
                    },
                ))
            })
            .collect();
        let state_presence = offline.clone();
        let state_roster = roster.clone();
        let cached = store.call(move |db| {
            let tx = db.transaction()?;
            crate::state::snapshot(&tx, &state_roster, &state_presence, &local_day(), daily_max)
        })?;
        let mut runtime = launcher.start_with_mode(
            &config,
            store.clone(),
            bus.clone(),
            client,
            resolution,
            mode.clone(),
        )?;
        if let Err(error) = runtime.ready().await {
            let _ = runtime.shutdown().await;
            return Err(error);
        }
        Ok(Self {
            mode,
            submissions: tokio::sync::Mutex::new(()),
            resources: Mutex::new(Some(Resources {
                store,
                bus,
                runtime,
                _lock: lock,
            })),
            gate: tokio::sync::RwLock::new(()),
            roster,
            daily_max,
            cached: Mutex::new(cached),
            offline: offline.clone(),
            observations: Mutex::new((receiver, offline)),
        })
    }
    pub async fn submit(&self, request: Request) -> RequestResult {
        self.submit_scheduled(request, true).await
    }
    /// Durable acceptance without dispatching a provider call (`send --no-wait`).
    /// Requires OneShot mode; Interactive workers may independently resume queued work.
    pub async fn submit_commit_only(&self, request: Request) -> RequestResult {
        self.submit_scheduled(request, false).await
    }
    async fn submit_scheduled(&self, request: Request, dispatch: bool) -> RequestResult {
        let _guard = self.gate.read().await;
        let _serial = self.submissions.lock().await;
        let id = crate::router::request_id(&request);
        if !dispatch && self.mode != OpenMode::OneShot {
            return RequestResult::Rejected {
                request_id: id,
                error: crate::router::error(
                    ErrorCode::INVALID_COMMAND,
                    "Commit-only submission requires OneShot mode",
                ),
            };
        }
        let Some((bus, store)) = self
            .resources
            .lock()
            .expect("room resources")
            .as_ref()
            .map(|r| (r.bus.clone(), r.store.clone()))
        else {
            return RequestResult::Rejected {
                request_id: id,
                error: crate::router::error(ErrorCode::NOT_READY, "Room is shut down"),
            };
        };
        let duplicate = store
            .call(move |db| {
                Ok(db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM receipts WHERE request_id=?)",
                    [id.to_string()],
                    |r| r.get::<_, bool>(0),
                )?)
            })
            .unwrap_or(true);
        let clear = matches!(request, Request::Clear { .. });
        if clear && !duplicate {
            let resources = self.resources.lock().expect("room resources").take();
            if let Some(mut resources) = resources {
                let stopped = resources.runtime.pause().await;
                *self.resources.lock().expect("room resources") = Some(resources);
                if let Err(error) = stopped {
                    return RequestResult::Rejected {
                        request_id: id,
                        error: crate::router::error(error.code, error.message),
                    };
                }
            }
        }
        let result = bus.send(request).await;
        if clear && !duplicate {
            let resources = self.resources.lock().expect("room resources").take();
            if let Some(mut resources) = resources {
                let restarted = resources.runtime.reset().await;
                *self.resources.lock().expect("room resources") = Some(resources);
                if let Err(error) = restarted {
                    return RequestResult::Rejected {
                        request_id: id,
                        error: crate::router::error(error.code, error.message),
                    };
                }
            }
        }
        if dispatch
            && !duplicate
            && let RequestResult::Accepted { event_ids, .. } = &result
        {
            let events = event_ids.clone();
            let work = store.call(move |db| {
                let mut ids = Vec::new();
                let mut q =
                    db.prepare("SELECT work_id FROM work WHERE trigger_event_id=? ORDER BY rowid")?;
                for event in events {
                    // Retry and regeneration retain their original trigger event.
                    // Their committed markers identify the newly admitted work.
                    if let Some(committed) = squad_store::events::by_id(db, event)? {
                        let allocated = match committed.content {
                            EnvelopePayload::WorkState(state)
                                if state.status == WorkStatus::Queued =>
                            {
                                Some(state.work_id)
                            }
                            EnvelopePayload::MessageSuperseded(marker) => {
                                Some(marker.replacement_work_id)
                            }
                            _ => None,
                        };
                        if let Some(id) = allocated
                            && !ids.contains(&id)
                        {
                            ids.push(id);
                        }
                    }
                    for row in q.query_map([event.to_string()], |r| r.get::<_, String>(0))? {
                        let id = row?.parse().map_err(|_| StoreError::Unavailable)?;
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
                Ok(ids)
            });
            if let Ok(ids) = work
                && let Some(resources) = self.resources.lock().expect("room resources").as_ref()
            {
                resources.runtime.wake(&ids);
            }
        }
        result
    }
    pub fn subscribe(&self) -> FeedReceiver {
        if let Some(resources) = self.resources.lock().expect("room resources").as_ref() {
            return resources.bus.subscribe();
        }
        let (sender, receiver) = tokio::sync::broadcast::channel(1);
        drop(sender);
        receiver
    }
    fn store(&self) -> Result<Store, RoomError> {
        self.resources
            .lock()
            .expect("room resources")
            .as_ref()
            .map(|r| r.store.clone())
            .ok_or_else(|| RoomError::new(ErrorCode::NOT_READY, "Room is shut down"))
    }
    pub async fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError> {
        let _guard = self.gate.read().await;
        Ok(self
            .store()?
            .call(move |db| page_before(db, before, limit))?)
    }
    /// Durable broadcast-lag repair. This uses the store's whole-commit-group replay contract.
    pub async fn events_after(
        &self,
        after: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError> {
        let _guard = self.gate.read().await;
        check_limit(limit)?;
        Ok(self
            .store()?
            .call(move |db| squad_store::events::page(db, after, limit))?)
    }
    pub async fn try_state(&self) -> Result<RoomState, RoomError> {
        let _guard = self.gate.read().await;
        let presence = {
            let mut observations = self.observations.lock().expect("room observations");
            loop {
                match observations.0.try_recv() {
                    Ok(FeedItem::Presence(event)) => {
                        if let EnvelopePayload::Presence(p) = event.content {
                            observations.1.insert(p.nick.clone(), p);
                        }
                    }
                    Ok(FeedItem::Committed(event))
                        if matches!(event.content, EnvelopePayload::RoomCleared(_)) =>
                    {
                        observations.1 = self.offline.clone()
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                        observations.1 = self.offline.clone();
                    }
                    Err(_) => break,
                }
            }
            observations.1.clone()
        };
        let roster = self.roster.clone();
        let max = self.daily_max;
        let state = self.store()?.call(move |db| {
            let tx = db.transaction()?;
            crate::state::snapshot(&tx, &roster, &presence, &local_day(), max)
        })?;
        *self.cached.lock().expect("room state") = state.clone();
        Ok(state)
    }
    pub async fn state(&self) -> RoomState {
        match self.try_state().await {
            Ok(state) => state,
            Err(error) => {
                tracing::error!(code=?error.code,"Room state unavailable; retaining last valid snapshot");
                self.cached.lock().expect("room state").clone()
            }
        }
    }
    pub async fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> Result<String, RoomError> {
        let _guard = self.gate.read().await;
        crate::export::export(&self.store()?, scope, id).map_err(export_error)
    }
    pub async fn shutdown(&self) -> Result<(), RoomError> {
        let _guard = self.gate.write().await;
        let resources = self.resources.lock().expect("room resources").take();
        if let Some(resources) = resources {
            let Resources {
                runtime,
                bus,
                store,
                _lock,
            } = resources;
            let result = runtime.shutdown().await;
            drop(bus);
            let persisted = store.call(|db| {
                db.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")?;
                Ok(())
            });
            drop(store);
            drop(_lock);
            result?;
            persisted?;
        }
        Ok(())
    }
}
fn existing_variables(path: &std::path::Path) -> Result<Vec<String>, RoomError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(StoreError::from)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='vars')",
            [],
            |r| r.get(0),
        )
        .map_err(StoreError::from)?;
    if !exists {
        return Ok(Vec::new());
    }
    let mut q = db
        .prepare("SELECT name FROM vars")
        .map_err(StoreError::from)?;
    Ok(q.query_map([], |r| r.get::<_, String>(0))
        .map_err(StoreError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreError::from)?)
}
fn local_day() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
fn check_limit(limit: usize) -> Result<(), RoomError> {
    if !(1..=500).contains(&limit) {
        return Err(RoomError::new(
            ErrorCode::INVALID_COMMAND,
            "Page limit must be between 1 and 500",
        ));
    }
    Ok(())
}
fn page_before(
    db: &mut rusqlite::Connection,
    before: Option<Cursor>,
    limit: usize,
) -> squad_store::Result<Vec<CommittedEnvelope>> {
    check_limit(limit).map_err(|_| {
        StoreError::Domain(
            ErrorCode::INVALID_COMMAND,
            "Page limit must be between 1 and 500",
        )
    })?;
    let tx = db.transaction()?;
    if let Some(cursor) = before {
        // Use the owning validation for future and cleared cursors, even for an empty page.
        squad_store::events::page(&tx, Some(cursor), 0)?;
    }
    // The limit is a target: expand the oldest selected group. If `before`
    // falls inside a group, exclude that group to keep the boundary exclusive.
    let mut q=tx.prepare("WITH selected AS (SELECT commit_start_seq FROM events WHERE (?1 IS NULL OR commit_start_seq+commit_size<=?1) ORDER BY seq DESC LIMIT ?2) SELECT seq FROM events WHERE seq >= (SELECT MIN(commit_start_seq) FROM selected) AND (?1 IS NULL OR commit_start_seq+commit_size<=?1) ORDER BY seq")?;
    let sequences = q
        .query_map(
            rusqlite::params![before.map(Cursor::get), limit as i64],
            |r| r.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    sequences
        .into_iter()
        .map(|seq| squad_store::events::at(&tx, seq))
        .collect()
}
fn export_error(error: crate::export::ExportError) -> RoomError {
    let code = match error {
        crate::export::ExportError::ContestNotFound => ErrorCode::CONTEST_NOT_FOUND,
        crate::export::ExportError::ReadFailed => ErrorCode::UNAVAILABLE,
        _ => ErrorCode::INVALID_COMMAND,
    };
    RoomError::new(code, error.to_string())
}
/// Read-only export never acquires `main.lock` or starts provider workers.
pub fn export_offline(
    data_dir: &std::path::Path,
    room: &str,
    scope: ExportScope,
    id: Option<ContestId>,
) -> Result<String, RoomError> {
    if room != "main" {
        return Err(RoomError::new(
            ErrorCode::ROOM_NOT_FOUND,
            "Only room main is supported",
        ));
    }
    crate::export::export(
        &crate::export::StoreExportSource::new(data_dir.join("squad.db")),
        scope,
        id,
    )
    .map_err(export_error)
}
