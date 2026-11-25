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
    Request::Stop {
        request_id: EventId::new(),
        target: None,
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
            },
        },
    }
}
