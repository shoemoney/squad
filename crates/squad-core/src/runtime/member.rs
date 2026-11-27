//! Bind configured member prompt values without exposing configuration or other histories.
use super::*;
use std::collections::BTreeMap;
pub(super) struct Renderer {
    pub pack: Arc<PromptPack>,
    pub config: SquadConfig,
    pub member: squad_protocol::config::AgentConfig,
    pub model: String,
}
impl PromptRenderer for Renderer {
    fn render(
        &self,
        prompt: &PromptRef,
        supplied: &BTreeMap<String, String>,
    ) -> Result<String, ReaderError> {
        let mut values = supplied.clone();
        let model = values
            .get("model_slug")
            .cloned()
            .unwrap_or_else(|| self.model.clone());
        values.extend(BTreeMap::from([
            ("nick".into(), self.member.nick.to_string()),
            ("model_slug".into(), model),
            (
                "member_nicks_json".into(),
                serde_json::to_string(
                    &self
                        .config
                        .agents
                        .iter()
                        .map(|a| a.nick.as_str())
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| unavailable())?,
            ),
            (
                "persona_json".into(),
                serde_json::to_string(&self.member.persona).map_err(|_| unavailable())?,
            ),
            (
                "user_name_json".into(),
                serde_json::to_string(&self.config.user.name).map_err(|_| unavailable())?,
            ),
            (
                "room_word_limit".into(),
                self.config.prompts.limits.room_words.to_string(),
            ),
        ]));
        let final_round = values
            .get("round")
            .is_some_and(|r| r != "0" && Some(r) == values.get("rounds"));
        values.entry("turn_word_limit".into()).or_insert_with(|| {
            if final_round {
                self.config.prompts.limits.final_turn_words
            } else {
                self.config.prompts.limits.turn_words
            }
            .to_string()
        });
        let id = prompt.template_id.as_str().trim_end_matches(".md");
        let composition = self.pack.composition(id).is_ok();
        let source = if composition {
            let (system, user) = self
                .pack
                .composition_source(id, false)
                .map_err(|_| unavailable())?;
            format!("{system}\n{user}")
        } else {
            self.pack
                .template(id)
                .map_err(|_| unavailable())?
                .to_owned()
        };
        // Templates were already validated at startup. Select only their declared
        // slots before handing them to the strict protocol renderer.
        let required: Vec<_> = source
            .split("{{")
            .skip(1)
            .filter_map(|part| part.split_once("}}").map(|(key, _)| key))
            .collect();
        values.retain(|key, _| required.contains(&key.as_str()));
        let rendered = self.pack.render(prompt, &values)?;
        if composition && !id.starts_with("census.") {
            let rendered: crate::prompts::RenderedComposition =
                serde_json::from_str(&rendered).map_err(|_| unavailable())?;
            Ok(rendered.user)
        } else {
            Ok(rendered)
        }
    }
}

pub(super) struct Executor {
    pub pack: Arc<PromptPack>,
    pub client: OpenRouterClient,
    pub config: SquadConfig,
    pub member: squad_protocol::config::AgentConfig,
    pub models: squad_agents::resolver::Catalog,
    pub prompt: squad_agents::context::MemberPrompt,
}
impl squad_agents::worker::WorkExecutor for Executor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> squad_agents::worker::ExecutionFuture<'a> {
        Box::pin(async move {
            if grant.work.kind.is_crosscheck_member() {
                return super::crosscheck::execute(
                    context,
                    grant,
                    cancel,
                    &self.client,
                    &self.config,
                    &self.pack,
                    &self.models,
                )
                .await;
            }
            if let Some(kind) = grant.work.kind.new_mode_kind() {
                return super::execute_new_mode(
                    kind,
                    context,
                    &grant,
                    &self.client,
                    &self.pack,
                    &self.config,
                    cancel,
                )
                .await
                .map_err(super::new_mode_error);
            }
            if grant.work.kind.is_probe() {
                let pack = PromptPack::from_override_dir(self.config.prompts.directory.as_deref())
                    .map_err(|_| ReaderError::NotRunnable)?;
                return super::probe::execute(
                    context,
                    grant,
                    cancel,
                    &self.config,
                    &pack,
                    &self.models,
                    &self.client,
                )
                .await;
            }
            let Some(model) = self.models.get(grant.binding.model.as_str()) else {
                return squad_agents::agents::failed(context, &grant, ErrorCode::MODEL_UNAVAILABLE)
                    .await;
            };
            let registry = squad_agents::registry::Registry::new(&self.config)
                .map_err(|_| ReaderError::NotRunnable)?;
            let request = registry
                .work_request(Some(&self.member), model, &grant)
                .map_err(|_| ReaderError::NotRunnable)?;
            if grant.work.kind.grill_composition().is_some() {
                let executor = squad_agents::agents::grill::GrillExecutor {
                    client: self.client.clone(),
                    request,
                    context_length: u64::from(model.context_length),
                };
                return squad_agents::worker::WorkExecutor::execute(
                    &executor, context, grant, cancel,
                )
                .await;
            }
            if grant.work.kind.census_composition().is_some() {
                let executor = squad_agents::agents::census::CensusExecutor {
                    client: self.client.clone(),
                    request,
                    context_length: u64::from(model.context_length),
                };
                return squad_agents::worker::WorkExecutor::execute(
                    &executor, context, grant, cancel,
                )
                .await;
            }
            if matches!(
                grant.work.kind,
                WorkKind::SurveyVote | WorkKind::SurveyRevote
            ) {
                let executor = squad_agents::agents::survey::SurveyExecutor {
                    client: self.client.clone(),
                    request,
                    system: self.prompt.system.clone(),
                    system_values: self
                        .prompt
                        .values(&grant)
                        .map_err(|_| ReaderError::Forbidden)?,
                    context_length: u64::from(model.context_length),
                };
                return squad_agents::worker::WorkExecutor::execute(
                    &executor, context, grant, cancel,
                )
                .await;
            }
            let executor = squad_agents::agents::member::MemberExecutor {
                client: self.client.clone(),
                request,
                prompt: self.prompt.clone(),
                context_length: u64::from(model.context_length),
            };
            squad_agents::worker::WorkExecutor::execute(&executor, context, grant, cancel).await
        })
    }
}
