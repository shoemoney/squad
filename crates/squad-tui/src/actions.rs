//! Snapshot-bound actions use protocol requests, never a second command grammar.
use squad_protocol::*;
pub fn preflight(plan: &PendingPreflight, confirm: bool) -> Request {
    let request_id = EventId::new();
    match plan {
        PendingPreflight::Debate(p) if confirm => Request::DebateConfirm {
            request_id,
            debate_id: p.debate_id,
            plan_id: p.plan_id,
        },
        PendingPreflight::Debate(p) => Request::DebateCancel {
            request_id,
            debate_id: p.debate_id,
            plan_id: p.plan_id,
        },
        PendingPreflight::Contest(p) if confirm => Request::ContestConfirm {
            request_id,
            contest_id: p.contest_id,
            plan_id: p.plan_id,
        },
        PendingPreflight::Contest(p) => Request::ContestCancel {
            request_id,
            contest_id: p.contest_id,
            plan_id: p.plan_id,
        },
    }
}
pub fn stop() -> Request {
    stop_target(None)
}
/// Global stop when `target` is `None`; otherwise only that member's work.
pub fn stop_target(target: Option<MemberNick>) -> Request {
    Request::Stop {
        request_id: EventId::new(),
        target,
    }
}
/// Replay one failed work item. Successful siblings are never restarted: the
/// core owns the work identity and the request names exactly one.
pub fn retry(work_id: WorkId) -> Request {
    Request::Retry {
        request_id: EventId::new(),
        work_id,
    }
}
/// The retry control a rejected request or work failure offers, if any.
pub fn retry_for(error: &ErrorPayload) -> Option<Request> {
    if !error.retryable {
        return None;
    }
    error
        .retry_action
        .as_ref()
        .map(|a| retry(a.work_id))
        .or_else(|| error.work_id.map(retry))
}
/// Regenerate one reply (`Some`) or the latest reply (`None`).
pub fn regenerate(message_id: Option<MessageId>) -> Request {
    Request::Regen {
        request_id: EventId::new(),
        message_id,
    }
}
pub fn retry_via<'a>(
    room: &'a dyn crate::session::RoomPort,
    work_id: WorkId,
) -> crate::session::RoomFuture<'a, RequestResult> {
    room.submit(retry(work_id))
}
pub fn regenerate_via<'a>(
    room: &'a dyn crate::session::RoomPort,
    message_id: Option<MessageId>,
) -> crate::session::RoomFuture<'a, RequestResult> {
    room.submit(regenerate(message_id))
}
pub fn help() -> Request {
    Request::Help
}
pub fn who() -> Request {
    Request::Who
}
/// Help is the parser-owned example list; the TUI adds no grammar of its own.
pub fn help_lines() -> Vec<String> {
    squad_protocol::parser::help_examples()
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}
/// Every export scope the Foundations contract defines, in presentation order.
pub const EXPORT_SCOPES: [ExportScope; 8] = [
    ExportScope::Room,
    ExportScope::Debate,
    ExportScope::Grill,
    ExportScope::Tournament,
    ExportScope::Census,
    ExportScope::Crosscheck,
    ExportScope::Survey,
    ExportScope::Probe,
];
pub fn scope_name(scope: &ExportScope) -> &'static str {
    match scope {
        ExportScope::Room => "room",
        ExportScope::Debate => "debate",
        ExportScope::Grill => "grill",
        ExportScope::Tournament => "tournament",
        ExportScope::Census => "census",
        ExportScope::Crosscheck => "crosscheck",
        ExportScope::Survey => "survey",
        ExportScope::Probe => "probe",
        ExportScope::Forge => "forge",
        ExportScope::Tune => "tune",
        ExportScope::BassAckwardsPrd => "bass_ackwards_prd",
        ExportScope::Forecast => "forecast",
        ExportScope::Decide => "decide",
        ExportScope::Negotiate => "negotiate",
        ExportScope::BlowItUp => "blow_it_up",
        ExportScope::Readback => "readback",
        ExportScope::Debugging => "debug",
        ExportScope::Trace => "trace",
        ExportScope::Rank => "rank",
    }
}
/// The four report scopes added after the original room/debate/grill/tournament set.
pub fn is_new_report_scope(scope: &ExportScope) -> bool {
    matches!(
        scope,
        ExportScope::Census | ExportScope::Crosscheck | ExportScope::Survey | ExportScope::Probe
    )
}
/// Report scope for a contest kind; the core renders the Markdown.
pub fn scope_for(kind: ContestKind) -> ExportScope {
    match kind {
        ContestKind::Debate => ExportScope::Debate,
        ContestKind::Grill => ExportScope::Grill,
        ContestKind::Tournament => ExportScope::Tournament,
        ContestKind::Census => ExportScope::Census,
        ContestKind::Crosscheck => ExportScope::Crosscheck,
        ContestKind::Survey => ExportScope::Survey,
        ContestKind::Probe => ExportScope::Probe,
        ContestKind::Forge => ExportScope::Forge,
        ContestKind::Tune => ExportScope::Tune,
        ContestKind::BassAckwardsPrd => ExportScope::BassAckwardsPrd,
        ContestKind::Forecast => ExportScope::Forecast,
        ContestKind::Decide => ExportScope::Decide,
        ContestKind::Negotiate => ExportScope::Negotiate,
        ContestKind::BlowItUp => ExportScope::BlowItUp,
        ContestKind::Readback => ExportScope::Readback,
        ContestKind::Debugging => ExportScope::Debugging,
        ContestKind::Trace => ExportScope::Trace,
        ContestKind::Rank => ExportScope::Rank,
    }
}
/// Export a contest report to a destination chosen by the caller. Success is
/// reported only after the whole report was written and flushed.
pub async fn export_report(
    room: &dyn crate::session::RoomPort,
    kind: ContestKind,
    id: ContestId,
    destination: &mut impl std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    export_to(room, scope_for(kind), Some(id), destination).await
}
pub fn export_request(scope: ExportScope, id: Option<ContestId>) -> Request {
    Request::Export { scope, id }
}
/// Default file name for an export destination (never overwrites; see `export_file`).
pub fn export_file_name(scope: &ExportScope, id: Option<ContestId>) -> String {
    match id {
        Some(id) => format!("squad-{}-{id}.md", scope_name(scope)),
        None => format!("squad-{}.md", scope_name(scope)),
    }
}
/// Export is successful only after the destination accepted all bytes.
pub fn save_export(destination: &mut impl std::io::Write, markdown: &str) -> std::io::Result<()> {
    destination.write_all(markdown.as_bytes())?;
    destination.flush()
}

/// Fetch core-owned Markdown and deliver it verbatim. The caller selects the
/// destination; a failed write never returns a success acknowledgement.
pub async fn export_to(
    room: &dyn crate::session::RoomPort,
    scope: ExportScope,
    id: Option<ContestId>,
    destination: &mut impl std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let markdown = room.export(scope, id).await?;
    save_export(destination, &markdown)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub request_id: EventId,
    pub scope: ExportScope,
    pub id: Option<ContestId>,
    pub path: std::path::PathBuf,
}
/// File export runs destination I/O away from the terminal scheduler. Existing
/// files are preserved; choosing another destination is an explicit UI action.
pub async fn export_file(room: &dyn crate::session::RoomPort, job: ExportJob) -> RequestResult {
    let result = match room.export(job.scope, job.id).await {
        Ok(markdown) => tokio::task::spawn_blocking(move || {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&job.path)?;
            save_export(&mut file, &markdown)?;
            Ok::<_, std::io::Error>(format!("Exported to {}", job.path.display()))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|result| result.map_err(|e| e.to_string())),
        Err(e) => Err(e.message),
    };
    match result {
        Ok(text) => RequestResult::Internal(InternalResult::Text { text }),
        Err(message) => RequestResult::Rejected {
            request_id: job.request_id,
            error: ErrorPayload {
                code: ErrorCode::INVALID_COMMAND,
                text: Text::new(message).expect("nonempty export error"),
                retryable: false,
                related_event_id: None,
                work_id: None,
                debate_id: None,
                case_id: None,
                earliest_cursor: None,
                retry_action: None,
                contest_id: None,
                provider_status: None,
                provider_message: None,
            },
        },
    }
}
