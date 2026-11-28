//! `/save`: write the headline deliverable of the newest finished contest (or the
//! last assistant message) as Markdown next to where squad was launched.
use crate::actions::scope_for;
use crate::session::{RoomPort, Session, contest_active};
use squad_protocol::*;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum SaveSource {
    Export { scope: ExportScope, id: ContestId },
    Text(String),
}
#[derive(Debug, Clone)]
pub struct SaveJob {
    pub request_id: EventId,
    pub source: SaveSource,
    pub mode: String,
    pub kind: Option<ContestKind>,
    pub prompt: String,
    pub explicit: Option<PathBuf>,
    pub cwd: PathBuf,
}

fn kind_of(state: &ContestState) -> Option<ContestKind> {
    Some(match state {
        ContestState::Tune(_) => ContestKind::Tune,
        ContestState::BassAckwardsPrd(_) => ContestKind::BassAckwardsPrd,
        ContestState::Forge(_) => ContestKind::Forge,
        ContestState::Forecast(_) => ContestKind::Forecast,
        ContestState::Decide(_) => ContestKind::Decide,
        ContestState::Negotiate(_) => ContestKind::Negotiate,
        ContestState::BlowItUp(_) => ContestKind::BlowItUp,
        ContestState::Readback(_) => ContestKind::Readback,
        ContestState::Debugging(_) => ContestKind::Debugging,
        ContestState::Trace(_) => ContestKind::Trace,
        ContestState::Rank(_) => ContestKind::Rank,
        ContestState::Generic(s) => s.kind,
        ContestState::Debate(_) => ContestKind::Debate,
        ContestState::Tournament(_) => ContestKind::Tournament,
        ContestState::Census(_) => ContestKind::Census,
        ContestState::Survey(_) => ContestKind::Survey,
        ContestState::Probe(_) => ContestKind::Probe,
        ContestState::Grill(_) => ContestKind::Grill,
        _ => return None,
    })
}
/// A contest that ended on its own terms, not one that failed or was stopped.
fn finished_ok(state: &ContestState) -> bool {
    if contest_active(state) {
        return false;
    }
    let value = serde_json::to_value(state).unwrap_or_default();
    let status = ["state", "status"]
        .iter()
        .find_map(|k| value.get(k).and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_lowercase();
    !["fail", "stop", "cancel", "expire", "reject"]
        .iter()
        .any(|w| status.contains(w))
}
pub fn mode_name(kind: ContestKind) -> String {
    kind.as_str().into()
}
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

/// Decide what to save from the current session. Pure: no I/O.
pub fn plan(
    session: &Session,
    focused: Option<EventId>,
    path: Option<String>,
    request_id: EventId,
    cwd: PathBuf,
) -> Result<SaveJob, String> {
    let records = &session.snapshot.context.records;
    let explicit = path.map(PathBuf::from);
    let latest = session.contests.iter().rev().find_map(|(id, view)| {
        let state = view.state.as_ref()?;
        (finished_ok(state)).then(|| kind_of(state).map(|k| (*id, k)))?
    });
    if let Some((id, kind)) = latest {
        let prompt = records
            .iter()
            .filter(|r| r.author == Actor::Me && r.contest_id == Some(id))
            .max_by_key(|r| r.seq)
            .or_else(|| {
                records
                    .iter()
                    .filter(|r| r.author == Actor::Me)
                    .max_by_key(|r| r.seq)
            })
            .map(|r| r.content.clone())
            .unwrap_or_default();
        return Ok(SaveJob {
            request_id,
            source: SaveSource::Export {
                scope: scope_for(kind),
                id,
            },
            mode: mode_name(kind),
            kind: Some(kind),
            prompt,
            explicit,
            cwd,
        });
    }
    let message = focused
        .and_then(|id| records.iter().find(|r| r.event_id == id))
        .or_else(|| {
            records
                .iter()
                .filter(|r| matches!(r.author, Actor::Member(_)) && !r.content.trim().is_empty())
                .max_by_key(|r| r.seq)
        })
        .ok_or("Nothing to save yet: no finished contest and no reply.")?;
    Ok(SaveJob {
        request_id,
        source: SaveSource::Text(message.content.clone()),
        mode: "message".into(),
        kind: None,
        prompt: message.content.clone(),
        explicit,
        cwd,
    })
}

fn sections(md: &str) -> (String, Vec<(String, String)>) {
    let mut preface = String::new();
    let mut out: Vec<(String, String)> = vec![];
    for line in md.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            out.push((title.trim().to_owned(), String::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
            body.push('\n');
        } else {
            preface.push_str(line);
            preface.push('\n');
        }
    }
    (preface, out)
}
/// The part of a mode's report a person would copy, never the evidence appendix.
pub fn headline(md: &str, kind: ContestKind) -> String {
    if kind == ContestKind::BassAckwardsPrd {
        return md.trim().to_owned() + "\n";
    }
    let (preface, parts) = sections(md);
    let pick = |needle: &str| {
        parts
            .iter()
            .find(|(t, _)| t.to_lowercase().contains(needle))
            .map(|(_, b)| b.trim().to_owned() + "\n")
    };
    let preferred: &[&str] = match kind {
        ContestKind::Tune => &["winning prompt"],
        ContestKind::Readback => &["final spec"],
        ContestKind::Tournament | ContestKind::Debate | ContestKind::Grill => {
            &["champion", "verdict", "final plan"]
        }
        _ => &[],
    };
    for needle in preferred {
        if let Some(body) = pick(needle) {
            return body;
        }
    }
    let mut body: Vec<&str> = preface.lines().collect();
    if body.first().is_some_and(|l| l.starts_with("# ")) {
        body.remove(0);
    }
    let mut text = body.join("\n").trim().to_owned();
    if kind == ContestKind::Forge {
        // Forge: keep the winning artifact only, not the "who won" line.
        text = text
            .split_once("\n\n")
            .map(|(_, rest)| rest.trim().to_owned())
            .unwrap_or(text);
    }
    if text.is_empty()
        && let Some((title, body)) = parts.first()
    {
        text = format!("## {title}\n\n{}", body.trim());
    }
    text + "\n"
}

fn create_unique(base: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::io::ErrorKind;
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "save".into());
    let ext = base.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..10_000 {
        let name = match (n, &ext) {
            (1, Some(e)) => format!("{stem}.{e}"),
            (1, None) => stem.clone(),
            (n, Some(e)) => format!("{stem}-{n}.{e}"),
            (n, None) => format!("{stem}-{n}"),
        };
        let candidate = base.with_file_name(name);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("too many existing files"))
}
pub fn write_unique(base: &Path, markdown: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let (path, mut file) = create_unique(base)?;
    file.write_all(markdown.as_bytes())?;
    file.flush()?;
    Ok(path)
}
fn shown(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}
fn reject(request_id: EventId, message: String) -> RequestResult {
    RequestResult::Rejected {
        request_id,
        error: ErrorPayload {
            code: ErrorCode::INVALID_COMMAND,
            text: Text::new(message).expect("nonempty save error"),
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
    }
}
pub async fn save_file(room: &dyn RoomPort, job: SaveJob) -> RequestResult {
    let markdown = match &job.source {
        SaveSource::Text(text) => text.trim().to_owned() + "\n",
        SaveSource::Export { scope, id } => match room.export(scope.clone(), Some(*id)).await {
            Ok(md) => headline(&md, job.kind.unwrap_or(ContestKind::Debate)),
            Err(e) => return reject(job.request_id, e.message),
        },
    };
    let base = match &job.explicit {
        Some(p) if p.is_absolute() => p.clone(),
        Some(p) => job.cwd.join(p),
        None => job
            .cwd
            .join(format!("{}-{}.md", job.mode, slug(&job.prompt))),
    };
    let cwd = job.cwd.clone();
    let request_id = job.request_id;
    match tokio::task::spawn_blocking(move || write_unique(&base, &markdown)).await {
        Ok(Ok(path)) => RequestResult::Internal(InternalResult::Text {
            text: format!("Saved {}", shown(&path, &cwd)),
        }),
        Ok(Err(e)) => reject(request_id, format!("Could not save: {e}")),
        Err(e) => reject(request_id, format!("Could not save: {e}")),
    }
}
