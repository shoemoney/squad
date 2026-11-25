//! Actual app/reducer/Markdown/widgets/catalogue gate. No provider or network.
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::{CatalogueEffect, FeedOrigin, FxCue, TerminalCaps, TimedCue},
    session::{FakeRoom, RoomPort},
    ui,
};
use std::{
    env, fs, io,
    process::Command,
    thread,
    time::{Duration, Instant},
};
// Cumulative user + system CPU, not machine-wide utilization. Linux uses
// proc ticks; macOS falls back to ps time (coarser whole/centisecond resolution).
fn process_cpu_seconds() -> io::Result<f64> {
    if cfg!(target_os = "linux") {
        let stat = fs::read_to_string("/proc/self/stat")?;
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .ok_or_else(|| io::Error::other("invalid proc stat"))?
            .1
            .split_whitespace()
            .collect();
        let user = fields[11].parse::<f64>().map_err(io::Error::other)?;
        let system = fields[12].parse::<f64>().map_err(io::Error::other)?;
        let ticks = Command::new("getconf").arg("CLK_TCK").output()?;
        let ticks = String::from_utf8_lossy(&ticks.stdout)
            .trim()
            .parse::<f64>()
            .map_err(io::Error::other)?;
        return Ok((user + system) / ticks);
    }
    let output = Command::new("ps")
        .args(["-p", &std::process::id().to_string(), "-o", "time="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("ps CPU query failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    let (days, clock) = text.split_once('-').map_or((0.0, text), |(days, clock)| {
        (days.parse::<f64>().unwrap_or(f64::NAN), clock)
    });
    let seconds = clock
        .split(':')
        .try_fold(0.0, |acc, part| {
            part.parse::<f64>().map(|part| acc * 60.0 + part)
        })
        .map_err(io::Error::other)?
        + days * 86400.0;
    if !seconds.is_finite() {
        return Err(io::Error::other("invalid CPU time"));
    }
    Ok(seconds)
}

fn memory() -> String {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        return status
            .lines()
            .filter(|s| s.starts_with("VmRSS:") || s.starts_with("VmHWM:"))
            .collect::<Vec<_>>()
            .join(", ");
    }
    Command::new("ps")
        .args(["-p", &std::process::id().to_string(), "-o", "rss="])
        .output()
        .map(|o| format!("RSS KiB: {}", String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or_else(|e| format!("RSS unavailable: {e}"))
}

fn percentile(samples: &mut [f64], percentile: f64) -> f64 {
    samples.sort_unstable_by(f64::total_cmp);
    samples[((samples.len() as f64 * percentile).ceil() as usize).saturating_sub(1)]
}

fn run(width: u16, height: u16, seconds: u64) -> io::Result<()> {
    let startup = Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()?;
    let snapshot = runtime.block_on(async {
        let room = FakeRoom::new();
        room.start_demo().await.unwrap();
        room.state().await
    });
    let caps = TerminalCaps::resolve(
        |key| match key {
            "SQUAD_FX" => Some("full".into()),
            "COLORTERM" => Some("truecolor".into()),
            _ => None,
        },
        true,
        (width, height).into(),
    )
    .unwrap();
    let mut app = App::new(caps);
    app.session.restore(snapshot);
    app.session.pick = None;
    app.session.snapshot.context.pending_pick = None;
    app.session.snapshot.context.active_contest = None;
    let contest = *app.session.debates.keys().next().unwrap();
    let verdict = app.session.contests[&contest]
        .verdict
        .as_ref()
        .unwrap()
        .completed
        .generation
        .message_id;
    for i in 1..=10_000u128 {
        let id = format!("00000000-0000-7000-8000-{i:012x}").parse().unwrap();
        app.session.transcript.complete(id,Actor::Member(Nick::new("claude").unwrap()),1,"A cached historical answer with **evidence**, 世界 and a grapheme-safe reply.\n\n```rust\nfn answer() -> u32 { 42 }\n```".into());
    }
    let streams: [GenerationIdentity; 12] = std::array::from_fn(|_| GenerationIdentity {
        message_id: MessageId::new(),
        work_id: WorkId::new(),
        trigger_event_id: EventId::new(),
        attempt: 1,
    });
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            let now = app.now();
            ui::render(&mut app, f, now);
        })
        .unwrap();
    eprintln!(
        "{width}x{height} startup_ms={:.3}",
        startup.elapsed().as_secs_f64() * 1000.
    );
    let cpu_start = process_cpu_seconds()?;
    let start = Instant::now();
    let mut next = start;
    let mut token = 0u64;
    let mut key = 1u64;
    let mut scroll = 1u64;
    let mut resize = 1u64;
    let mut celebration = 0u64;
    let mut draws = vec![];
    let mut work = vec![];
    let load_start = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let mut latency = vec![];
    let mut frame_count = 0u64;
    let mut deltas = 0u64;
    while start.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(next.saturating_duration_since(Instant::now()));
        let frame_start = Instant::now();
        let mut pending = vec![];
        let elapsed = start.elapsed();
        while elapsed >= Duration::from_millis(key * 83) {
            if app.input.text().len() >= 40 {
                app.input
                    .apply(squad_tui::widgets::input::EditorAction::SelectAll)
                    .unwrap();
            }
            app.input
                .apply(squad_tui::widgets::input::EditorAction::Insert("a".into()))
                .unwrap();
            pending.push(start + Duration::from_millis(key * 83));
            key += 1;
        }
        while elapsed >= Duration::from_millis(token * 25) {
            for (i, generation) in streams.iter().enumerate() {
                let text = [
                    "Reliable ",
                    "**streaming** ",
                    "with ",
                    "世界 ",
                    "and ",
                    "evidence.\n\n",
                ][token as usize % 6];
                let base = EnvelopeBase {
                    version: 1,
                    id: EventId::new(),
                    room_id: MainRoom::Main,
                    correlation_id: generation.trigger_event_id,
                    causation_id: None,
                    sender: Actor::Member(
                        Nick::new(squad_tui::fx::palette::MEMBERS[i % 6]).unwrap(),
                    ),
                    recipients: vec![Recipient::Me],
                    created_at: std::time::SystemTime::now().into(),
                };
                let now = app.now();
                app.session.reduce(
                    FeedItem::Delta(ProducerEnvelope {
                        base,
                        content: EnvelopePayload::Delta(Delta {
                            generation: generation.clone(),
                            chunk_index: token,
                            text: text.into(),
                            debate_id: None,
                            round: None,
                            case_id: None,
                            contest_id: None,
                            step_id: None,
                        }),
                    }),
                    FeedOrigin::Live,
                    now,
                    app.fx.queue_mut(),
                );
                deltas += 1;
                // Replace the same attempt periodically, bounding the live tail.
                if token > 0 && token.is_multiple_of(240) {
                    let card = app
                        .session
                        .transcript
                        .cards
                        .get_mut(&generation.message_id)
                        .unwrap();
                    card.text.clear();
                    card.pending_text.clear();
                }
            }
            token += 1;
        }
        if elapsed >= Duration::from_secs(scroll * 2) {
            app.session.transcript.follow = scroll.is_multiple_of(2);
            app.session.transcript.anchor = Some((verdict, 0));
            scroll += 1;
        }
        if elapsed >= Duration::from_secs(resize * 5) {
            let current = if resize.is_multiple_of(2) {
                width
            } else if width == squad_tui::layout::MIN_WIDTH {
                // Keep the compact viewport interactive during resize stress.
                width + 1
            } else {
                width - 1
            };
            app.fx.settle();
            terminal.resize(Rect::new(0, 0, current, height)).unwrap();
            resize += 1;
        }
        if elapsed >= Duration::from_secs(celebration * 8) {
            let at = app.now();
            app.fx.queue_mut().push(
                FeedOrigin::Live,
                TimedCue {
                    event_id: EventId::new(),
                    at,
                    cue: FxCue::Catalogue {
                        effect: CatalogueEffect::Verdict,
                        target: contest.to_string(),
                        duration: Duration::from_secs(6),
                    },
                },
            );
            celebration += 1;
        }
        let now = app.now();
        app.expire(now);
        let draw_start = Instant::now();
        let completed = terminal.draw(|f| ui::render(&mut app, f, now)).unwrap();
        let area = completed.buffer.area;
        let geometry =
            squad_tui::layout::with_input_rows(area, app.input.desired_height(area.width));
        for (column, character) in app.input.text().chars().enumerate() {
            assert_eq!(
                completed.buffer[(geometry.input.x + 2 + column as u16, geometry.input.y + 1)]
                    .symbol(),
                character.to_string(),
                "typed text must reach the actual editor"
            );
        }
        let finish = Instant::now();
        draws.push(finish.duration_since(draw_start).as_secs_f64() * 1000.);
        work.push(finish.duration_since(frame_start).as_secs_f64() * 1000.);
        latency.extend(
            pending
                .into_iter()
                .map(|t| finish.duration_since(t).as_secs_f64() * 1000.),
        );
        frame_count += 1;
        next += Duration::from_secs_f64(1. / 60.);
        while next <= finish {
            next += Duration::from_secs_f64(1. / 60.);
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let cpu = (process_cpu_seconds()? - cpu_start) / elapsed * 100.;
    let p95 = percentile(&mut draws, 0.95);
    let p99 = percentile(&mut draws, 0.99);
    let key_p95 = percentile(&mut latency, 0.95);
    let work_p95 = percentile(&mut work, 0.95);
    let work_p99 = percentile(&mut work, 0.99);
    let load_end = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let passed = p95 <= 8. && p99 <= 16.7 && key_p95 <= 50. && cpu <= 25.;
    eprintln!(
        "{width}x{height}: elapsed={elapsed:.3}s frames={frame_count} deltas={deltas} draw_p95_ms={p95:.3} draw_p99_ms={p99:.3} key_p95_ms={key_p95:.3} cpu_one_core_pct={cpu:.3} {} gate={}",
        memory(),
        if passed { "PASS" } else { "FAIL" }
    );
    eprintln!(
        "{width}x{height}: reducer_plus_draw_p95_ms={work_p95:.3} reducer_plus_draw_p99_ms={work_p99:.3} load_start={} load_end={}",
        load_start.trim(),
        load_end.trim()
    );
    if !env::var("COMPOSED_ENFORCE").is_ok_and(|v| v == "0") {
        assert!(passed, "real widget/full effects gate failed");
    }
    Ok(())
}
fn main() -> io::Result<()> {
    let seconds = env::var("COMPOSED_SECS")
        .map_or(Ok(300), |s| s.parse::<u64>().map_err(io::Error::other))?;
    assert!(seconds > 0);
    for (width, height) in [(140, 42), (80, 24), (200, 60)] {
        run(width, height, seconds)?;
    }
    Ok(())
}
