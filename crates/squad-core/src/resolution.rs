//! Store-backed startup catalog and probe ledger. Never reads credentials.
use rusqlite::{OptionalExtension, params};
use squad_agents::resolver::{CatalogSnapshot, ProbeAttempt, ResolutionSnapshot, ResolutionStore};
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
        self.store.call(move |db| {
            let tx=db.transaction()?;
            tx.execute("DELETE FROM catalog",[])?;
            for model in snapshot.catalog.models.values(){
                tx.execute("INSERT INTO catalog VALUES (?,?,?,?,?,?)",params![model.id,model.context_length,model.output_limit,model.pricing.to_string(),serde_json::to_string(&model.supported_parameters)?,chrono::DateTime::from_timestamp(snapshot.catalog.fetched_at as i64,0).ok_or(StoreError::Unavailable)?.to_rfc3339()])?;
            }
            tx.execute("INSERT INTO startup_resolution VALUES (1,?) ON CONFLICT(id) DO UPDATE SET snapshot_json=excluded.snapshot_json",[serde_json::to_string(&snapshot)?])?;
            tx.commit()?;Ok(())
        })
    }
    fn before_probe(
        &mut self,
        nick: &str,
        configured: &str,
        attempt: u32,
        _max_tokens: u32,
    ) -> squad_store::Result<()> {
        let id = *self.probes.entry(nick.into()).or_default();
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
