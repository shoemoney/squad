//! Assemble one isolated Negotiate invocation from the work-bound history capability.
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &squad_agents::openrouter::OpenRouterClient,
    pack: &crate::prompts::PromptPack,
    config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), squad_agents::agents::ExecutionError> {
    let h = context
        .history
        .for_work(grant.work.work_id)
        .await
        .inspect_err(|e| tracing::error!(error=?e,"Negotiate history read failed"))?;
    let input: serde_json::Value =
        serde_json::from_str(&h.last().ok_or(ReaderError::NotRunnable)?.content).map_err(|e| {
            tracing::error!(error=?e,"Negotiate invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let data = &input["data"];
    let repair = context
        .work
        .repair_for_attempt(grant.work.work_id, grant.work.attempt)
        .await
        .inspect_err(|e| tracing::error!(error=?e,"Negotiate repair read failed"))?;
    let name =
        squad_store::negotiate::composition(&grant.work.kind).ok_or(ReaderError::NotRunnable)?;
    let source = &input["prompt"];
    let system = source[if repair.is_some() {
        "repairSystem"
    } else {
        "system"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let user = source[if repair.is_some() {
        "repairUser"
    } else {
        "user"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let values =
        crate::prompts::inputs_negotiate::frozen(&grant.work, data, repair.as_ref(), system, user)
            .map_err(|e| {
                tracing::error!(error=?e,"Negotiate invocation preparation failed");
                ReaderError::NotRunnable
            })?;
    let rendered = pack
        .render_frozen_composition(name, system, user, &values)
        .map_err(|e| {
            tracing::error!(error=?e,"Negotiate invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let model: squad_agents::resolver::Model = serde_json::from_value(input["model"].clone())
        .map_err(|e| {
            tracing::error!(error=?e,"Negotiate invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let member = match &grant.work.target {
        Target::Member(n) => config.agents.iter().find(|m| &m.nick == n),
        Target::Judge => None,
    };
    let mut request = squad_agents::registry::Registry::new(config)
        .map_err(|_| ReaderError::NotRunnable)?
        .work_request(member, &model, grant)
        .map_err(|e| {
            tracing::error!(error=?e,"Negotiate invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    request.messages = vec![
        squad_agents::openrouter::ChatMessage {
            role: "system".into(),
            name: None,
            content: rendered.system,
        },
        squad_agents::openrouter::ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: rendered.user,
        },
    ];
    request.response_format = Some(
        serde_json::json!({"type":"json_schema","json_schema":{"name":"negotiate_response","strict":true,"schema":negotiate::output_schema(&grant.work.kind)}}),
    );
    let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
    {
        return squad_agents::agents::failed(context, grant, ErrorCode::CONTEXT_TOO_LARGE)
            .await
            .map_err(Into::into);
    }
    squad_agents::agents::negotiate::execute(context, grant, client, request, data, cancel).await
}

/// Construct a fresh projection; no shared case or ordinary history is appended.
#[allow(clippy::too_many_arguments)]
pub fn projection(
    setup: &negotiate::NegotiateSetup,
    state: NegotiateStatus,
    round: u32,
    target: &Target,
    kind: &WorkKind,
    maps: &std::collections::BTreeMap<MemberNick, negotiate::NegotiateMap>,
    brief: Option<&negotiate::NegotiateBrief>,
    packages: &std::collections::BTreeMap<MemberNick, negotiate::NegotiatePackage>,
    tables: &[negotiate::NegotiateTable],
) -> Result<serde_json::Value, ReaderError> {
    use serde_json::json;
    let mut data = json!({"round":round,"wordLimit":match state {NegotiateStatus::Mapping=>setup.room_words,NegotiateStatus::Briefing|NegotiateStatus::WritingPlaybook=>setup.plan_words,_=>200}});
    let blind = |s: &str| crate::orchestrator::negotiate::blind(s, setup);
    match kind {
        WorkKind::NegotiateMap => {
            data["ourSide"] = json!(setup.our_side);
            data["counterpart"] = json!(setup.counterpart);
        }
        WorkKind::NegotiateBrief | WorkKind::NegotiatePlaybook => {
            if *target != Target::Judge {
                return Err(ReaderError::Forbidden);
            }
            data["ourSide"] = json!(blind(&setup.our_side));
            data["counterpart"] = json!(blind(&setup.counterpart));
            // Each anonymous map stands alone; identity keys never reach the judge.
            data["maps"] = json!(
                maps.values()
                    .map(|m| redacted(m, setup))
                    .collect::<Vec<_>>()
            );
            data["tableCount"] = json!(setup.tables.len());
            if *kind == WorkKind::NegotiatePlaybook {
                data["brief"] = json!(redacted(brief.ok_or(ReaderError::NotRunnable)?, setup));
                data["packages"] = json!(
                    packages
                        .values()
                        .map(|p| redacted(p, setup))
                        .collect::<Vec<_>>()
                );
                data["tables"]=json!(tables.iter().map(|t|json!({"table":t.table,"outcome":t.outcome,"roundsUsed":t.rounds_used,"transcript":transcript(t,setup)})).collect::<Vec<_>>());
                data["hasInferredInterests"] = json!(maps.values().any(|m| {
                    m.counterpart_interests
                        .iter()
                        .any(|i| i.basis == negotiate::NegotiateBasis::Inferred)
                }));
                data["noRehearsalEvidence"] = json!(
                    tables
                        .iter()
                        .all(|t| t.outcome == Some(negotiate::NegotiateOutcome::Abandoned))
                );
            }
        }
        WorkKind::NegotiatePackage | WorkKind::NegotiateMove => {
            let Target::Member(n) = target else {
                return Err(ReaderError::Forbidden);
            };
            if setup.tables.iter().any(|t| &t.counterpart == n) {
                return Err(ReaderError::Forbidden);
            }
            let brief = brief.ok_or(ReaderError::NotRunnable)?;
            data["ourSide"] = json!(setup.our_side);
            data["strategySheet"] = json!(brief.strategy_sheet);
            if *kind == WorkKind::NegotiateMove {
                let table = tables
                    .iter()
                    .find(|t| &t.negotiator == n && t.outcome.is_none())
                    .ok_or(ReaderError::Forbidden)?;
                data["table"] = json!(table.table);
                data["transcript"] = json!(transcript(table, setup));
                data["package"] = json!(packages.get(n));
                data["fallback"] = json!(if packages.contains_key(n) {
                    "Use your package"
                } else {
                    "Open from the strategy sheet"
                });
            }
        }
        WorkKind::NegotiateCounter => {
            let Target::Member(n) = target else {
                return Err(ReaderError::Forbidden);
            };
            let table = tables
                .iter()
                .find(|t| &t.counterpart == n && t.outcome.is_none())
                .ok_or(ReaderError::Forbidden)?;
            let brief = brief.ok_or(ReaderError::NotRunnable)?;
            data["table"] = json!(table.table);
            data["publicScenario"] = json!(brief.public_scenario);
            data["hiddenBrief"] = json!(
                brief
                    .counterpart_briefs
                    .get(table.table as usize - 1)
                    .ok_or(ReaderError::NotRunnable)?
            );
            data["transcript"] = json!(transcript(table, setup));
        }
        _ => return Err(ReaderError::Forbidden),
    }
    Ok(data)
}
fn transcript(
    table: &negotiate::NegotiateTable,
    setup: &negotiate::NegotiateSetup,
) -> Vec<serde_json::Value> {
    table.turns.iter().map(|t|serde_json::json!({"round":t.round,"side":if t.counterpart{"counterpart"}else{"negotiator"},"body":redacted(&t.body,setup)})).collect()
}
fn redacted<T: serde::Serialize>(v: &T, setup: &negotiate::NegotiateSetup) -> serde_json::Value {
    fn scrub(v: &mut serde_json::Value, s: &negotiate::NegotiateSetup) {
        match v {
            serde_json::Value::String(t) => *t = crate::orchestrator::negotiate::blind(t, s),
            serde_json::Value::Array(a) => a.iter_mut().for_each(|v| scrub(v, s)),
            serde_json::Value::Object(o) => {
                for (k, v) in o {
                    if !matches!(k.as_str(), "basis" | "response" | "overlap") {
                        scrub(v, s);
                    }
                }
            }
            _ => {}
        }
    }
    let mut v = serde_json::to_value(v).expect("protocol serialization");
    scrub(&mut v, setup);
    v
}
