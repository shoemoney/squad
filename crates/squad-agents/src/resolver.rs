//! Startup-only concrete identity resolution. Persistence belongs to the router.
use crate::openrouter::{OpenRouterClient, StreamEvent, TransportError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use squad_protocol::config::{ConfigError, SquadConfig};
use std::collections::BTreeMap;

pub const CACHE_MAX_AGE_SECONDS: u64 = 24 * 60 * 60;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub context_length: u32,
    pub output_limit: u32,
    pub supported_parameters: Vec<String>,
    pub reasoning_efforts: Vec<String>,
    pub pricing: Value,
}
impl Model {
    pub fn supports_reasoning(&self) -> bool {
        self.supported_parameters.iter().any(|p| p == "reasoning")
    }
    pub fn lowest_reasoning_effort(&self) -> Option<&str> {
        if !self.supports_reasoning() {
            return None;
        }
        ["none", "minimal", "low", "medium", "high", "xhigh"]
            .into_iter()
            .find(|effort| self.reasoning_efforts.iter().any(|e| e == effort))
            .or(Some("low"))
    }
}
pub type Catalog = BTreeMap<String, Model>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub fetched_at: u64,
    pub models: Catalog,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinnedModel {
    pub nick: String,
    pub configured: String,
    pub model: Option<Model>,
    pub error: Option<String>,
    pub probes: Vec<ProbeAttempt>,
}
/// Safe provider metadata retained for router-owned probe accounting. No output
/// or reasoning text is retained in startup snapshots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeAttempt {
    pub max_tokens: u32,
    pub generation_id: Option<String>,
    pub response_model: Option<String>,
    pub usage: Option<crate::openrouter::Usage>,
    pub error: Option<String>,
}
impl ProbeAttempt {
    fn capture(max_tokens: u32, result: &Result<Value, TransportError>) -> Self {
        let value = result.as_ref().ok();
        Self {
            max_tokens,
            generation_id: value
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            response_model: value
                .and_then(|v| v.get("model"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            usage: value
                .and_then(|v| v.get("usage"))
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            error: result.as_ref().err().map(ToString::to_string),
        }
    }
}
impl PinnedModel {
    pub fn response_mismatch(&self, response_model: &str) -> Option<String> {
        let pinned = &self.model.as_ref()?.id;
        (pinned != response_model).then(|| {
            format!(
                "MODEL_IDENTITY_MISMATCH {}: pinned {pinned}, response {response_model}",
                self.nick
            )
        })
    }
    pub fn observe(&self, event: &StreamEvent) -> Option<String> {
        if let StreamEvent::Metadata {
            response_model: Some(model),
            ..
        } = event
        {
            self.response_mismatch(model)
        } else {
            None
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolutionSnapshot {
    pub catalog: CatalogSnapshot,
    pub cached: bool,
    pub members: Vec<PinnedModel>,
    pub judge: PinnedModel,
    pub diagnostics: Vec<String>,
}
/// Adapter must atomically persist catalog and role-aware identities together.
/// Wiring to router-owned persistence is intentionally deferred (M2.P1.L2.T4).
pub trait ResolutionStore {
    type Error;
    fn cached_catalog(&self) -> Result<Option<CatalogSnapshot>, Self::Error>;
    fn save_resolution(&mut self, snapshot: &ResolutionSnapshot) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum ResolutionError {
    Catalog(TransportError),
    InvalidCatalog,
    Configuration(ConfigError),
}
impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(error) => write!(f, "MODEL_RESOLUTION_FAILED catalog: {error}"),
            Self::InvalidCatalog => f.write_str("MODEL_RESOLUTION_FAILED invalid catalog"),
            Self::Configuration(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ResolutionError {}

pub fn normalize_catalog(value: Value) -> Result<Catalog, ResolutionError> {
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(ResolutionError::InvalidCatalog)?;
    let mut models = Catalog::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| s.contains('/') && !s.is_empty())
            .ok_or(ResolutionError::InvalidCatalog)?;
        let number = |value: Option<&Value>| {
            value
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
        };
        let context_length =
            number(entry.get("context_length")).ok_or(ResolutionError::InvalidCatalog)?;
        let output_limit =
            number(entry.pointer("/top_provider/max_completion_tokens")).unwrap_or(context_length);
        let strings = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let model = Model {
            id: id.to_owned(),
            context_length,
            output_limit,
            supported_parameters: strings("supported_parameters"),
            reasoning_efforts: strings("supported_reasoning_efforts"),
            pricing: entry.get("pricing").cloned().unwrap_or(Value::Null),
        };
        if models.insert(id.to_owned(), model).is_some() {
            return Err(ResolutionError::InvalidCatalog);
        }
    }
    Ok(models)
}
fn transient(error: &TransportError) -> bool {
    matches!(
        error,
        TransportError::Network | TransportError::Deadline | TransportError::Http(429 | 500..=599)
    )
}
pub async fn acquire_catalog(
    client: &OpenRouterClient,
    cache: Option<&CatalogSnapshot>,
    now: u64,
) -> Result<(CatalogSnapshot, bool), ResolutionError> {
    for attempt in 1..=3 {
        match client.fetch_catalog().await {
            Ok(value) => {
                return Ok((
                    CatalogSnapshot {
                        fetched_at: now,
                        models: normalize_catalog(value)?,
                    },
                    false,
                ));
            }
            Err(error) if transient(&error) && attempt < 3 => continue,
            Err(error) => {
                if transient(&error)
                    && let Some(cache) = cache.filter(|c| {
                        now.checked_sub(c.fetched_at)
                            .is_some_and(|age| age <= CACHE_MAX_AGE_SECONDS)
                    })
                {
                    return Ok((cache.clone(), true));
                }
                return Err(ResolutionError::Catalog(error));
            }
        }
    }
    unreachable!()
}

async fn pin(
    client: &OpenRouterClient,
    catalog: &Catalog,
    nick: &str,
    configured: &str,
    variable: &str,
) -> Result<PinnedModel, ResolutionError> {
    if !configured.starts_with('~') {
        let model = catalog
            .get(configured)
            .filter(|m| !m.id.starts_with('~'))
            .ok_or_else(|| {
                ResolutionError::Configuration(ConfigError::UnknownModel {
                    variable: variable.to_owned(),
                    slug: configured.to_owned(),
                })
            })?;
        return Ok(PinnedModel {
            nick: nick.to_owned(),
            configured: configured.to_owned(),
            model: Some(model.clone()),
            error: None,
            probes: Vec::new(),
        });
    }
    let effort = catalog
        .get(configured)
        .and_then(Model::lowest_reasoning_effort);
    let mut response = client
        .probe_alias_with_reasoning(configured, 16, effort)
        .await;
    let mut probes = vec![ProbeAttempt::capture(16, &response)];
    if matches!(response, Err(TransportError::Http(400))) {
        response = client
            .probe_alias_with_reasoning(configured, 2048, effort)
            .await;
        probes.push(ProbeAttempt::capture(2048, &response));
    }
    let model = response
        .as_ref()
        .ok()
        .and_then(|v| v.get("model"))
        .and_then(Value::as_str)
        .filter(|id| !id.starts_with('~'))
        .and_then(|id| catalog.get(id))
        .cloned();
    let error = model.is_none().then(|| format!("MODEL_RESOLUTION_FAILED {variable}: alias {configured} has no validated concrete target"));
    Ok(PinnedModel {
        nick: nick.to_owned(),
        configured: configured.to_owned(),
        model,
        error,
        probes,
    })
}
pub async fn resolve(
    client: &OpenRouterClient,
    config: &SquadConfig,
    cache: Option<&CatalogSnapshot>,
    now: u64,
) -> Result<ResolutionSnapshot, ResolutionError> {
    let (catalog, cached) = acquire_catalog(client, cache, now).await?;
    // Validate all concrete selections before spending on any alias probe.
    for (variable, selection) in config
        .agents
        .iter()
        .map(|a| {
            (
                format!("SQUAD_MODEL_{}", a.nick.as_str().to_uppercase()),
                a.model.as_str(),
            )
        })
        .chain(std::iter::once((
            "SQUAD_JUDGE_MODEL".to_owned(),
            config.judge.model.as_str(),
        )))
    {
        if !selection.starts_with('~') && !catalog.models.contains_key(selection) {
            return Err(ResolutionError::Configuration(ConfigError::UnknownModel {
                variable,
                slug: selection.to_owned(),
            }));
        }
    }
    let mut members = Vec::new();
    for member in &config.agents {
        members.push(
            pin(
                client,
                &catalog.models,
                member.nick.as_str(),
                &member.model,
                &format!("SQUAD_MODEL_{}", member.nick.as_str().to_uppercase()),
            )
            .await?,
        );
    }
    let judge = pin(
        client,
        &catalog.models,
        "judge",
        &config.judge.model,
        "SQUAD_JUDGE_MODEL",
    )
    .await?;
    let mut diagnostics = Vec::new();
    if cached {
        diagnostics.push(
            "WARNING: using cached OpenRouter catalog after three transient failures".to_owned(),
        );
    }
    for pinned in members.iter().chain(std::iter::once(&judge)) {
        diagnostics.push(format!(
            "{} selection={} concrete={} catalog_at={} cached={}{}",
            pinned.nick,
            if pinned.configured.starts_with('~') {
                "alias"
            } else {
                "concrete"
            },
            pinned
                .model
                .as_ref()
                .map(|m| m.id.as_str())
                .unwrap_or("offline"),
            catalog.fetched_at,
            cached,
            pinned
                .error
                .as_ref()
                .map(|e| format!(" {e}"))
                .unwrap_or_default()
        ));
    }
    Ok(ResolutionSnapshot {
        catalog,
        cached,
        members,
        judge,
        diagnostics,
    })
}
