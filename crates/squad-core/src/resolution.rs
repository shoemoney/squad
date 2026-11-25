//! Store-backed startup catalog and probe ledger. Never reads credentials.
use rusqlite::{OptionalExtension, params};
use squad_agents::resolver::{
    AliasCacheEntry, CatalogSnapshot, ProbeAttempt, ResolutionSnapshot, ResolutionStore,
};
use squad_protocol::*;
use squad_store::{Store, StoreError};
use std::collections::BTreeMap;

pub(crate) struct ResolutionPersistence {
    store: Store,
    max: i64,
    probes: BTreeMap<String, WorkId>,
}
impl ResolutionPersistence {
    pub fn new(store: Store, max: i64) -> squad_store::Result<Self> {
        store.call(|db| { db.execute_batch("CREATE TABLE IF NOT EXISTS startup_resolution (id INTEGER PRIMARY KEY CHECK(id=1), snapshot_json TEXT NOT NULL CHECK(json_valid(snapshot_json))) STRICT")?; Ok(()) })?;
        Ok(Self {
            store,
            max,
            probes: BTreeMap::new(),
        })
    }
}
impl ResolutionStore for ResolutionPersistence {
    type Error = StoreError;
    fn cached_alias(
        &self,
        provider: &str,
        alias: &str,
    ) -> squad_store::Result<Option<AliasCacheEntry>> {
        let provider = provider.to_owned();
        let alias = alias.to_owned();
        self.store.call(move |db| {
            let row: Option<(String, i64)> = db
                .query_row(
                    "SELECT model_json,resolved_at FROM alias_cache WHERE provider=? AND alias=?",
                    params![provider, alias],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            row.map(|(json, resolved_at)| {
                Ok(AliasCacheEntry {
                    model: serde_json::from_str(&json)?,
                    resolved_at: u64::try_from(resolved_at).map_err(|_| StoreError::Unavailable)?,
                })
            })
            .transpose()
        })
    }
    fn save_alias(
        &mut self,
        provider: &str,
        alias: &str,
        entry: &AliasCacheEntry,
    ) -> squad_store::Result<()> {
        let provider = provider.to_owned();
        let alias = alias.to_owned();
        let json = serde_json::to_string(&entry.model)?;
        let resolved_at = i64::try_from(entry.resolved_at).map_err(|_| StoreError::Unavailable)?;
        self.store.call(move |db| {
            db.execute("INSERT INTO alias_cache VALUES (?,?,?,?) ON CONFLICT(provider,alias) DO UPDATE SET model_json=excluded.model_json,resolved_at=excluded.resolved_at",params![provider,alias,json,resolved_at])?;
            Ok(())
        })
    }
    fn remove_alias(&mut self, provider: &str, alias: &str) -> squad_store::Result<()> {
        let provider = provider.to_owned();
        let alias = alias.to_owned();
        self.store.call(move |db| {
            db.execute(
                "DELETE FROM alias_cache WHERE provider=? AND alias=?",
                params![provider, alias],
            )?;
            Ok(())
        })
    }
    fn cached_catalog(&self) -> squad_store::Result<Option<CatalogSnapshot>> {
        self.store.call(|db| {
            let raw: Option<String> = db
                .query_row(
                    "SELECT snapshot_json FROM startup_resolution WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            raw.map(|s| Ok(serde_json::from_str::<ResolutionSnapshot>(&s)?.catalog))
                .transpose()
        })
    }
    fn save_resolution(&mut self, snapshot: &ResolutionSnapshot) -> squad_store::Result<()> {
        let snapshot = snapshot.clone();
        let id = EventId::new();
        let digest = squad_store::commit::digest(&snapshot)?;
        self.store.commit(id,"model-resolver".into(),digest,move |tx,group| {
            tx.execute("DELETE FROM catalog",[])?;
            for model in snapshot.catalog.models.values(){
                tx.execute("INSERT INTO catalog VALUES (?,?,?,?,?,?)",params![model.id,model.context_length,model.output_limit,model.pricing.to_string(),serde_json::to_string(&model.supported_parameters)?,chrono::DateTime::from_timestamp(snapshot.catalog.fetched_at as i64,0).ok_or(StoreError::Unavailable)?.to_rfc3339()])?;
            }
            let mut members=Vec::new();
            for (order,pinned) in snapshot.members.iter().enumerate() {
                if let Some(model)=&pinned.model {
                    members.push(Model { nick: Actor::Member(pinned.nick.parse().map_err(|_|StoreError::Unavailable)?), requested_slug: Text::new(pinned.configured.clone()).map_err(|_|StoreError::Unavailable)?, resolved_slug: Text::new(model.id.clone()).map_err(|_|StoreError::Unavailable)?, roster_order: Some(order as u32), context_length: u64::from(model.context_length), pricing:model.pricing.clone(),resolved_at:chrono::Utc::now() });
                }
            }
            // Offline members keep their configured roster position in the
            // snapshot, so available model rows can have gaps in roster_order.
            tx.execute("DELETE FROM models WHERE role='member'",[])?;
            for member in &members { squad_store::models::save_model(tx,member).map_err(|_|StoreError::Unavailable)?; }
            tx.execute("DELETE FROM models WHERE role='judge'",[])?;
            if let Some(model)=&snapshot.judge.model {
                squad_store::models::save_model(tx,&Model {nick:Actor::Judge,requested_slug:Text::new(snapshot.judge.configured.clone()).map_err(|_|StoreError::Unavailable)?,resolved_slug:Text::new(model.id.clone()).map_err(|_|StoreError::Unavailable)?,roster_order:None,context_length:u64::from(model.context_length),pricing:model.pricing.clone(),resolved_at:chrono::Utc::now()}).map_err(|_|StoreError::Unavailable)?;
            }
            tx.execute("INSERT INTO startup_resolution VALUES (1,?) ON CONFLICT(id) DO UPDATE SET snapshot_json=excluded.snapshot_json",[serde_json::to_string(&snapshot)?])?;
            if snapshot.cached {
                group.push(tx,crate::router::envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::SystemNotice(SystemNotice{code:Text::new("MODEL_CATALOG_CACHED").expect("literal"),text:Text::new("WARNING: using cached OpenRouter catalog after three transient failures").expect("literal"),work_id:None,debate_id:None,case_id:None,contest_id:None})))?;
            }
            for pinned in snapshot.members.iter().chain(std::iter::once(&snapshot.judge)).filter(|p|p.model.is_none()) {
                let error=crate::router::error(ErrorCode::MODEL_UNAVAILABLE,pinned.error.clone().unwrap_or_else(||format!("Model resolution failed for {}",pinned.nick)));
                group.push(tx,crate::router::envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::Error(error)))?;
            }
            Ok(group.accepted(id))
        }).map(|_|())
    }
    fn before_probe(
        &mut self,
        nick: &str,
        configured: &str,
        attempt: u32,
        _max_tokens: u32,
    ) -> squad_store::Result<()> {
        if attempt == 1 {
            self.probes.insert(nick.into(), WorkId::new());
        }
        let id = *self.probes.get(nick).ok_or(StoreError::Unavailable)?;
        let nick = nick.to_owned();
        let model = configured.to_owned();
        let max = self.max;
        self.store.call(move |db|{
            let tx=db.transaction()?;let day=chrono::Local::now().format("%Y-%m-%d").to_string();
            squad_store::accounting::admit(&tx,&day,0,max)?;
            tx.execute("INSERT OR IGNORE INTO spend_daily VALUES (?,0,0,0)",[&day])?;
            tx.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,day,started_at,outcome,unresolved) VALUES (?, ?, (SELECT epoch FROM room_meta), ?, ?, ?, ?, ?, 'unknown', 1)",params![id.to_string(),attempt,nick,format!("probe:{nick}"),model,day,chrono::Utc::now().to_rfc3339()])?;
            tx.execute("UPDATE spend_daily SET unknown_calls=unknown_calls+1 WHERE day=?",[day])?;
            tx.commit()?;Ok(())
        })
    }
    fn after_probe(
        &mut self,
        nick: &str,
        _configured: &str,
        attempt: u32,
        probe: &ProbeAttempt,
    ) -> squad_store::Result<()> {
        let id = *self.probes.get(nick).ok_or(StoreError::Unavailable)?;
        let probe = probe.clone();
        self.store.call(move |db| {
            let tx=db.transaction()?;
            let (day,old,unknown):(String,Option<i64>,bool)=tx.query_row("SELECT day,reported_cost_micros,unresolved FROM attempts WHERE work_id=? AND attempt=?",params![id.to_string(),attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let usage=probe.usage.as_ref().and_then(squad_agents::accounting::known_usage);
            let cost=usage.as_ref().and_then(|u|u.cost_usd).map(squad_store::accounting::reported_micros).transpose().map_err(|_|StoreError::Unavailable)?;
            if let Some(cost)=cost {tx.execute("UPDATE spend_daily SET spent_micros=spent_micros+?,unknown_calls=unknown_calls-? WHERE day=?",params![cost-old.unwrap_or(0),u32::from(unknown),day])?;}
            tx.execute("UPDATE attempts SET ended_at=?,outcome=?,provider_request_id=?,response_model=?,usage_json=?,reported_cost_micros=COALESCE(?,reported_cost_micros),unresolved=? WHERE work_id=? AND attempt=?",params![chrono::Utc::now().to_rfc3339(),if probe.error.is_some(){"failed"}else{"completed"},probe.generation_id,probe.response_model,usage.map(|u|serde_json::to_string(&u)).transpose()?,cost,cost.is_none(),id.to_string(),attempt])?;
            tx.commit()?;Ok(())
        })
    }
}

/// One repair per rejected concrete identity for the lifetime of this room.
/// The transport still returns the original failure; no admitted work is replayed.
pub(crate) struct AliasRecovery {
    pub store: Store,
    pub config: SquadConfig,
    pub max: i64,
    pub refreshed: tokio::sync::Mutex<std::collections::BTreeSet<String>>,
}
impl squad_agents::openrouter::ModelRecovery for AliasRecovery {
    fn recover(
        &self,
        client: squad_agents::openrouter::OpenRouterClient,
        model: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let mut refreshed = self.refreshed.lock().await;
            if !refreshed.insert(model.clone()) {
                return;
            }
            let Ok(mut persistence) = ResolutionPersistence::new(self.store.clone(), self.max)
            else {
                return;
            };
            if squad_agents::resolver::refresh_failed_alias(
                &client,
                &self.config,
                &model,
                chrono::Utc::now().timestamp() as u64,
                &mut persistence,
            )
            .await
            .is_err()
            {
                tracing::warn!("Alias cache repair failed; next startup will resolve again");
            }
        })
    }
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod tests;
