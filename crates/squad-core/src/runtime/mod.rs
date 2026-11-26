//! Private member workers and dynamically discovered case executors.
pub mod contexts;
mod crosscheck;
mod judge;
mod member;
mod probe;
use crate::{bus::InProcessBus, prompts::PromptPack};
use squad_agents::{openrouter::OpenRouterClient, resolver::ResolutionSnapshot};
use squad_protocol::*;
use squad_store::Store;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct RuntimeHandle {
    cancel: CancellationToken,
    task: JoinHandle<Result<(), ReaderError>>,
    ready: tokio::sync::watch::Receiver<bool>,
    scope: contexts::Scope,
    wakeup: Arc<tokio::sync::Notify>,
    bus: InProcessBus,
    roster: Vec<MemberNick>,
}
impl RuntimeHandle {
    pub fn is_ready(&self) -> bool {
        *self.ready.borrow()
    }
    pub async fn ready(&mut self) -> Result<(), ReaderError> {
        while !*self.ready.borrow_and_update() {
            self.ready
                .changed()
                .await
                .map_err(|_| ReaderError::NotRunnable)?;
        }
        Ok(())
    }
    pub async fn shutdown(mut self) -> Result<(), ReaderError> {
        self.cancel.cancel();
        (&mut self.task)
            .await
            .map_err(|_| ReaderError::StoreUnavailable {
                message: "Executor runtime stopped".into(),
            })?
    }
}
impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub fn start(
    config: &SquadConfig,
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    resolution: ResolutionSnapshot,
) -> Result<RuntimeHandle, ReaderError> {
    launch(config, store, bus, client, resolution)
}

pub fn start_with_mode(
    config: &SquadConfig,
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    resolution: ResolutionSnapshot,
    mode: OpenMode,
) -> Result<RuntimeHandle, ReaderError> {
    launch_mode(config, store, bus, client, resolution, mode)
}
fn launch(
    config: &SquadConfig,
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    resolution: ResolutionSnapshot,
) -> Result<RuntimeHandle, ReaderError> {
    launch_mode(
        config,
        store,
        bus,
        client,
        resolution,
        OpenMode::Interactive,
    )
}
fn unavailable() -> ReaderError {
    ReaderError::StoreUnavailable {
        message: "Executor runtime unavailable".into(),
    }
}
pub(crate) fn prompt(pack: &PromptPack, config: &SquadConfig, id: &str) -> PromptRef {
    let material = serde_json::json!({"template": id, "overrides": pack.overrides(), "user": config.user.name, "members": config.agents.iter().map(|a| (&a.nick, &a.model, &a.persona, a.reasoning_effort)).collect::<Vec<_>>(), "roomWords": config.prompts.limits.room_words, "turnWords": config.prompts.limits.turn_words, "finalWords": config.prompts.limits.final_turn_words, "effort": config.defaults.reasoning_effort, "judgeEffort": config.judge.reasoning_effort});
    PromptRef {
        pack_version: Text::new(pack.version()).expect("pack version"),
        template_id: Text::new(id).expect("template id"),
        template_hash: Text::new(squad_store::commit::digest(&material).expect("prompt digest"))
            .expect("hash"),
    }
}
impl RuntimeHandle {
    pub fn wake(&self, work_ids: &[WorkId]) {
        wake_scope(&self.scope, &self.wakeup, &self.bus, &self.roster, work_ids);
    }
}
fn wake_scope(
    scope: &contexts::Scope,
    wakeup: &tokio::sync::Notify,
    bus: &InProcessBus,
    roster: &[MemberNick],
    work_ids: &[WorkId],
) {
    let mut ids = scope.ids.lock().expect("dispatch scope");
    for id in work_ids {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    drop(ids);
    for nick in roster {
        bus.wake_mailbox(&ExecutorKey::Member(nick.clone()));
    }
    wakeup.notify_one();
}
fn launch_mode(
    config: &SquadConfig,
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    resolution: ResolutionSnapshot,
    mode: OpenMode,
) -> Result<RuntimeHandle, ReaderError> {
    let pack = Arc::new(
        PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| unavailable())?,
    );
    let _registry =
        squad_agents::registry::Registry::new(config).map_err(|_| ReaderError::NotRunnable)?;
    let roster: Vec<_> = config.agents.iter().map(|a| a.nick.clone()).collect();
    let mut members = Vec::new();
    for member in &config.agents {
        let Some(pinned) = resolution
            .members
            .iter()
            .find(|p| p.nick == member.nick.as_str())
            .and_then(|p| p.model.as_ref())
        else {
            continue;
        };
        let renderer = Arc::new(member::Renderer {
            pack: pack.clone(),
            config: config.clone(),
            member: member.clone(),
            model: pinned.id.clone(),
        });
        members.push((
            member.nick.clone(),
            renderer,
            Arc::new(member::Executor {
                pack: pack.clone(),
                client: client.clone(),
                config: config.clone(),
                member: member.clone(),
                models: {
                    let mut models = resolution.catalog.models.clone();
                    models.insert(pinned.id.clone(), pinned.clone());
                    models
                },
                prompt: squad_agents::context::MemberPrompt {
                    system: prompt(&pack, config, "room.system"),
                    roster: roster.clone(),
                    user_name: config.user.name.clone(),
                    persona: member.persona.clone(),
                },
            }),
        ));
    }
    let judge_model = resolution.judge.model;
    let judge = Arc::new(judge::Runner {
        client,
        config: config.clone(),
        pack: pack.clone(),
        models: {
            let mut models = resolution.catalog.models.clone();
            if let Some(judge_model) = judge_model {
                models.insert(judge_model.id.clone(), judge_model);
            }
            models
        },
    });
    let epoch = store
        .call(|db| Ok(db.query_row("SELECT epoch FROM room_meta", [], |r| r.get::<_, i64>(0))?))
        .map_err(|_| unavailable())? as u64;
    let cancel = CancellationToken::new();
    let scope = contexts::Scope {
        ids: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let wakeup = Arc::new(tokio::sync::Notify::new());
    let (ready_tx, ready) = tokio::sync::watch::channel(false);
    let child_cancel = cancel.clone();
    let task_scope = scope.clone();
    let task_wakeup = wakeup.clone();
    let handle_bus = bus.clone();
    let handle_roster = roster.clone();
    let task = tokio::spawn(async move {
        let _cancel_on_exit = child_cancel.clone().drop_guard();
        let mut tasks = Vec::new();
        for (nick, renderer, executor) in members {
            let context = contexts::agent_context(
                store.clone(),
                &bus,
                ExecutorKey::Member(nick),
                epoch,
                renderer,
                child_cancel.child_token(),
            );
            let context = if mode == OpenMode::OneShot {
                contexts::scoped(context, store.clone(), task_scope.clone())
            } else {
                context
            };
            tasks.push(squad_agents::worker::spawn(context, executor));
        }
        let mut cases: Vec<(CaseId, JoinHandle<Result<(), ReaderError>>)> = Vec::new();
        let mut feed = bus.subscribe();
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        let mut result = Ok(());
        loop {
            let case_ids = store.call(|db| {
                let mut query = db.prepare("SELECT DISTINCT case_id FROM work WHERE target='judge' AND status IN ('queued','running') AND epoch=(SELECT epoch FROM room_meta)")?;
                Ok(query.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?)
            }).map_err(|_| unavailable());
            let case_ids = match case_ids {
                Ok(ids) => ids,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            };
            let mut i = 0;
            while i < cases.len() {
                if cases[i].1.is_finished() {
                    let (_, task) = cases.swap_remove(i);
                    if let Err(e) = task.await.unwrap_or_else(|_| Err(unavailable())) {
                        result = Err(e);
                    }
                } else {
                    i += 1;
                }
            }
            for id in case_ids {
                let case: CaseId = id.parse().map_err(|_| unavailable())?;
                if cases.iter().any(|(id, _)| *id == case) {
                    continue;
                }
                let context = contexts::agent_context(
                    store.clone(),
                    &bus,
                    ExecutorKey::Case(case),
                    epoch,
                    pack.clone(),
                    child_cancel.child_token(),
                );
                let context = if mode == OpenMode::OneShot {
                    contexts::scoped(context, store.clone(), task_scope.clone())
                } else {
                    context
                };
                if context.work.unfinished().await?.is_empty() {
                    continue;
                }
                cases.push((
                    case,
                    squad_agents::worker::spawn(
                        context,
                        Arc::new(judge::Executor {
                            ordinary: squad_agents::judge::JudgeExecutor {
                                runner: judge.clone(),
                            },
                            crosscheck: judge.clone(),
                        }),
                    ),
                ));
            }
            let _ = ready_tx.send(true);
            // Cancellation can finish idle workers before this supervisor observes
            // its cancellation branch. Their normal exit is not a runtime failure.
            if !child_cancel.is_cancelled() && tasks.iter().any(|task| task.is_finished()) {
                result = Err(unavailable());
                break;
            }
            if result.is_err() {
                break;
            }
            tokio::select! {
                biased;
                _ = child_cancel.cancelled() => break,
                _ = task_wakeup.notified() => { for nick in &roster { bus.wake_mailbox(&ExecutorKey::Member(nick.clone())); } },
                _ = interval.tick() => {},
                event = feed.recv() => {
                    if mode == OpenMode::OneShot && let Ok(FeedItem::Committed(event)) = event
                        && let EnvelopePayload::WorkState(work) = event.content
                        && work.status == WorkStatus::Queued && let Some(contest) = work.contest_id {
                        let ids=task_scope.ids.lock().expect("dispatch scope").clone();
                        let admitted=store.call(move |db| {
                            Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND work_id IN (SELECT value FROM json_each(?)))",rusqlite::params![contest.to_string(),serde_json::to_string(&ids)?],|r|r.get::<_,bool>(0))?)
                        }).map_err(|_|unavailable())?;
                        if admitted {
                            wake_scope(&task_scope,&task_wakeup,&bus,&roster,&[work.work_id]);
                        }
                    }
                },
            }
        }
        let _ = ready_tx.send(false);
        child_cancel.cancel();
        for task in tasks
            .into_iter()
            .chain(cases.into_iter().map(|(_, task)| task))
        {
            if let Err(e) = task.await.unwrap_or_else(|_| Err(unavailable()))
                && result.is_ok()
            {
                result = Err(e);
            }
        }
        result
    });
    Ok(RuntimeHandle {
        cancel,
        task,
        ready,
        scope,
        wakeup,
        bus: handle_bus,
        roster: handle_roster,
    })
}

pub mod forge;

pub mod forecast;

pub mod decide;

pub mod negotiate;

pub mod blow_it_up;

pub mod readback;

pub mod debugging;

pub mod trace;

pub mod rank;

#[allow(clippy::too_many_arguments)]
async fn execute_new_mode(kind: ContestKind, context: &AgentContext, grant: &ExecutionGrant, client: &squad_agents::openrouter::OpenRouterClient, pack: &PromptPack, config: &SquadConfig, cancel: tokio_util::sync::CancellationToken) -> Result<(),squad_agents::agents::ExecutionError> { match kind {
ContestKind::Forge => forge::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Forecast => forecast::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Decide => decide::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Negotiate => negotiate::execute(context,grant,client,pack,config,cancel).await,
ContestKind::BlowItUp => blow_it_up::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Readback => readback::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Debugging => debugging::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Trace => trace::execute(context,grant,client,pack,config,cancel).await,
ContestKind::Rank => rank::execute(context,grant,client,pack,config,cancel).await,
_ => Err(ReaderError::NotRunnable.into()) } }
