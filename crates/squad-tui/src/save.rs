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
    let value = value.get("state").unwrap_or(&value);
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
/// Slug of the first 20 characters of the prompt, once `@mentions` and `$mode`
/// tokens are dropped. Lowercase, runs of non-alphanumerics become one hyphen.
pub fn slug(text: &str) -> String {
    let words = text
        .split_whitespace()
        .filter(|w| !w.starts_with('@') && !w.starts_with('$'))
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::new();
    for c in words.chars().take(20) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}
/// One more than the highest `<stem>-N.<ext>` already in `dir` (1 when none).
pub fn next_number(dir: &Path, stem: &str, ext: &str) -> u32 {
    let prefix = format!("{stem}-");
    let suffix = format!(".{ext}");
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix(&prefix)?
                .strip_suffix(&suffix)?
                .parse::<u32>()
                .ok()
        })
        .max()
        .map_or(1, |n| n + 1)
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

/// Locate exporter-owned headings without interpreting headings inside code fences.
fn heading_offsets(md: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    for line in md.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|c| *c == marker).count();
        if matches!(marker, '`' | '~') && count >= 3 {
            match fence {
                Some((c, n)) if c == marker && count >= n => fence = None,
                None => fence = Some((marker, count)),
                _ => {}
            }
        } else if fence.is_none()
            && let Some(title) = line.strip_prefix("## ")
        {
            out.push((offset, title.trim()));
        }
        offset += line.len();
    }
    out
}
/// Extract standalone artifacts only at known exporter boundaries. Reports with
/// multiple substantive sections stay intact; unknown or ambiguous shapes are lossless.
pub fn headline(md: &str, kind: ContestKind) -> String {
    let headings = heading_offsets(md);
    let unique = |title: &str| {
        let mut matches = headings.iter().filter(|(_, t)| *t == title);
        let offset = matches.next()?.0;
        matches.next().is_none().then_some(offset)
    };
    let section = |title: &str, end: &str| {
        let start = unique(title)?;
        let body = start + md[start..].find('\n')? + 1;
        let stop = unique(end)?;
        (stop >= body).then(|| md[body..stop].trim())
    };
    let picked = match kind {
        ContestKind::Forge => unique("Borda").and_then(|end| {
            // The exporter writes a title, a verdict paragraph, then the full artifact.
            md[..end]
                .strip_prefix("# FORGE\n\n")?
                .split_once("\n\n")
                .map(|(_, body)| body.trim())
        }),
        ContestKind::Tune => section("Winning prompt", "Diff"),
        ContestKind::Readback => section("Final spec", "Convergence"),
        ContestKind::Tournament => {
            let mut champions = headings.iter().filter(|(_, t)| t.starts_with("Champion: "));
            let first = champions.next();
            champions
                .next()
                .is_none()
                .then_some(first)
                .flatten()
                .map(|(start, _)| md[*start..].trim())
        }
        _ => None,
    };
    picked.unwrap_or(md.trim()).to_owned() + "\n"
}

fn create_new(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn write_all(mut file: std::fs::File, markdown: &str) -> std::io::Result<()> {
    use std::io::Write;
    file.write_all(markdown.as_bytes())?;
    file.flush()
}
/// `<dir>/<slug>-N.md` with N one past the highest existing number; never overwrites.
pub fn write_numbered(dir: &Path, slug: &str, markdown: &str) -> std::io::Result<PathBuf> {
    let mut n = next_number(dir, slug, "md");
    loop {
        let path = dir.join(format!("{slug}-{n}.md"));
        match create_new(&path) {
            Ok(file) => {
                write_all(file, markdown)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}
/// An explicit destination is used as given; an existing file is refused with the
/// next free numbered name as a suggestion.
pub fn write_explicit(path: &Path, markdown: &str) -> std::io::Result<PathBuf> {
    match create_new(path) {
        Ok(file) => {
            write_all(file, markdown)?;
            Ok(path.to_path_buf())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "save".into());
            let ext = path
                .extension()
                .map_or("md".into(), |e| e.to_string_lossy().into_owned());
            let n = next_number(dir, &stem, &ext);
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists; try {stem}-{n}.{ext}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            ))
        }
        Err(e) => Err(e),
    }
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
    let cwd = job.cwd.clone();
    let cwd2 = cwd.clone();
    let request_id = job.request_id;
    let explicit = job.explicit.as_ref().map(|p| {
        if p.is_absolute() {
            p.clone()
        } else {
            job.cwd.join(p)
        }
    });
    let name = slug(&job.prompt);
    match tokio::task::spawn_blocking(move || match explicit {
        Some(path) => write_explicit(&path, &markdown),
        None => write_numbered(&cwd2, &name, &markdown),
    })
    .await
    {
        Ok(Ok(path)) => RequestResult::Internal(InternalResult::Text {
            text: format!("Saved {}", shown(&path, &cwd)),
        }),
        Ok(Err(e)) => reject(request_id, format!("Could not save: {e}")),
        Err(e) => reject(request_id, format!("Could not save: {e}")),
    }
}
