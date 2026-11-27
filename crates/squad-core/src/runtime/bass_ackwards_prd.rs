//! Frozen scoped calls and crash-safe publication of a committed PRD report.
use squad_agents::{agents::ExecutionError, worker::WorkExecutor};
use squad_protocol::bass_ackwards_prd::*;
use squad_protocol::*;
use squad_store::{Result, bass_ackwards_prd as store, commit::CommitOutcome};
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &squad_agents::openrouter::OpenRouterClient,
    _pack: &crate::prompts::PromptPack,
    config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> std::result::Result<(), ExecutionError> {
    let entries = context.history.for_work(grant.work.work_id).await?;
    let material: serde_json::Value =
        serde_json::from_str(&entries.last().ok_or(ReaderError::NotRunnable)?.content)
            .map_err(|_| ReaderError::NotRunnable)?;
    let model: squad_agents::resolver::Model =
        serde_json::from_value(material["model"].clone()).map_err(|_| ReaderError::NotRunnable)?;
    let registry =
        squad_agents::registry::Registry::new(config).map_err(|_| ReaderError::NotRunnable)?;
    let member = match &grant.work.target {
        Target::Member(n) => config.agents.iter().find(|a| &a.nick == n),
        Target::Judge => None,
    };
    let request = registry
        .work_request(member, &model, grant)
        .map_err(ExecutionError::Transport)?;
    squad_agents::agents::bass_ackwards_prd::BassAckwardsPrdExecutor {
        client: client.clone(),
        request,
        context_length: u64::from(model.context_length),
    }
    .execute(context, grant.clone(), cancel)
    .await?;
    Ok(())
}
pub fn save_file(router: &mut crate::router::Router, contest: ContestId) -> Result<CommitOutcome> {
    use sha2::{Digest, Sha256};
    let (setup, report, state) = router.store.call(move |db| {
        Ok((
            store::setup(db, contest)?,
            store::get::<BassAckwardsPrdReport>(db, contest, "report")?,
            store::state(db, contest)?,
        ))
    })?;
    let doc = router
        .store
        .call(move |db| store::get::<BassAckwardsPrdDocument>(db, contest, "document"))?;
    let filename = format!(
        "prd-{}-{contest}.md",
        crate::orchestrator::bass_ackwards_prd::slug(&doc.title)
    );
    let write = crate::export::write_export_file(
        std::path::Path::new(&setup.data_dir),
        &filename,
        report.markdown.as_bytes(),
    );
    let id = EventId::new();
    router.store.commit(
        id,
        "contest-engine".into(),
        squad_store::commit::digest(&state)?,
        move |tx, g| {
            let mut s = store::state(tx, contest)?;
            if s != state || s.state != BassAckwardsPrdStatus::Saving {
                return Err(store::invalid());
            }
            // Saving owns its own durable step; it can be reconstructed after a crash.
            let step = ContestStep {
                contest_id: contest,
                step_id: StepId::new(),
                ordinal: 12,
                script_version: Text::new("bass-ackwards-prd-v1").expect("script"),
                move_name: ContestMove::Report,
                inputs: vec![],
                assignments: vec![],
            };
            let seq = g.push(
                tx,
                crate::router::envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::ContestStep(step.clone()),
                ),
            )?;
            squad_store::contests::associate(tx, contest, ContestKind::BassAckwardsPrd, seq)?;
            squad_store::contest_steps::insert(tx, &step, seq)?;
            s.step_id = Some(step.step_id);
            let mut report = report;
            let reason = match write {
                Ok(path) => {
                    let saved = BassAckwardsPrdSaved {
                        filename,
                        path: path.to_string_lossy().into_owned(),
                        bytes: report.markdown.len() as u64,
                        sha256: format!("{:x}", Sha256::digest(report.markdown.as_bytes())),
                    };
                    crate::router::bass_ackwards_prd::artifact(
                        tx,
                        g,
                        id,
                        &s,
                        "saved",
                        serde_json::to_value(&saved)?,
                    )?;
                    store::put(tx, contest, "saved", &saved)?;
                    if store::optional::<bool>(tx, contest, "judge_failed")?.unwrap_or(false) {
                        "judge_failed"
                    } else if report.checks.covered {
                        "saved"
                    } else {
                        "saved_with_findings"
                    }
                }
                Err(e) => {
                    report.path = None;
                    report.save_error = Some(e.to_string());
                    crate::router::bass_ackwards_prd::artifact(
                        tx,
                        g,
                        id,
                        &s,
                        "report",
                        serde_json::to_value(&report)?,
                    )?;
                    store::put(tx, contest, "report", &report)?;
                    "save_failed"
                }
            };
            s.state = if reason == "judge_failed" {
                BassAckwardsPrdStatus::Failed
            } else {
                BassAckwardsPrdStatus::Complete
            };
            s.terminal_reason = Some(Text::new(reason).expect("reason"));
            let version = s.state_version;
            s.state_version += 1;
            crate::router::bass_ackwards_prd::save(tx, g, id, &s, Some(version))?;
            Ok(g.accepted(id))
        },
    )
}
