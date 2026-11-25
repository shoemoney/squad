//! M4.P1.L1 throwaway spike. No Squad widget, Markdown, or effects dependencies.
//! `cargo bench -p squad-tui --bench twelve_streams`; SPIKE_SECS defaults to 300
//! PER viewport. SPIKE_TERMINAL=1 opts into stdout (requires a 120x36 TTY).
use std::{
    env, fs,
    io::{self, IsTerminal},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::{Backend, CrosstermBackend, TestBackend},
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph},
};

const VIEWPORTS: [(u16, u16); 2] = [(120, 36), (80, 24)];
const DELTAS: [&str; 16] = [
    "Streaming ",
    "prose ",
    "with ",
    "a ",
    "small ",
    "Rust ",
    "example:\n",
    "```rust\n",
    "fn ",
    "answer() ",
    "-> ",
    "u32 ",
    "{ ",
    "42 ",
    "}\n",
    "```\n",
];

struct Workload {
    history: Vec<String>,
    cached: Vec<String>,
    streams: [String; 12],
    input: String,
    anchor: Option<usize>,
    deltas: usize,
    keys: usize,
    scrolls: usize,
    resizes: usize,
}

// ASCII fixture: deterministic, cached wrapping, code fences remain visible.
fn wrap(text: &str, width: usize, rows: &mut Vec<String>) {
    for line in text.lines() {
        if line.is_empty() {
            rows.push(String::new());
        } else {
            rows.extend(
                line.as_bytes()
                    .chunks(width)
                    .map(|part| String::from_utf8(part.to_vec()).expect("ASCII fixture")),
            );
        }
    }
}

impl Workload {
    fn new(width: u16) -> Self {
        let mut state = Self {
            history: (0..10_000).map(|i| format!(
                "member {} / message {i}\nA completed answer with enough prose to wrap at the narrow viewport and exercise history reading.\n```rust\nfn answer() -> u32 {{ 42 }}\n```\n", i % 12
            )).collect(),
            cached: Vec::new(),
            streams: std::array::from_fn(|i| format!("member {i} / live\n")),
            input: String::new(), anchor: None, deltas: 0, keys: 0, scrolls: 0, resizes: 0,
        };
        state.reflow(width);
        state
    }

    fn reflow(&mut self, width: u16) {
        self.cached.clear();
        for message in &self.history {
            wrap(message, usize::from(width), &mut self.cached);
        }
    }

    fn tokens(&mut self, tick: usize, width: u16) {
        for (i, stream) in self.streams.iter_mut().enumerate() {
            stream.push_str(DELTAS[(tick + i) % DELTAS.len()]);
            self.deltas += 1;
            if tick.is_multiple_of(240) {
                let completed = std::mem::replace(stream, format!("member {i} / live\n"));
                wrap(&completed, usize::from(width), &mut self.cached);
                self.history.push(completed);
            }
        }
    }

    fn visible(&self, width: u16, height: u16) -> Vec<Line<'static>> {
        let count = usize::from(height - 4);
        let mut rows = Vec::new();
        if let Some(anchor) = self.anchor {
            rows.extend(self.cached.iter().skip(anchor).take(count).cloned());
        } else {
            for stream in &self.streams {
                wrap(stream, usize::from(width), &mut rows);
            }
            let skip = rows.len().saturating_sub(count);
            rows = rows.into_iter().skip(skip).collect();
        }
        rows.into_iter()
            .map(|row| {
                let color = if row.starts_with("```") || row.starts_with("fn ") {
                    Color::Cyan
                } else {
                    Color::White
                };
                Line::styled(row, Style::default().fg(color))
            })
            .collect()
    }
}

fn render(frame: &mut Frame<'_>, rows: Vec<Line<'static>>, state: &Workload) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new(format!(
            "12 streams x 40 deltas/s | FX off | {} messages | {} deltas",
            state.history.len(),
            state.deltas
        )),
        Rect::new(0, 0, area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(rows),
        Rect::new(0, 1, area.width, area.height - 4),
    );
    frame.render_widget(
        Block::bordered().title("Simulated typing"),
        Rect::new(0, area.height - 3, area.width, 3),
    );
    frame.render_widget(
        Paragraph::new(state.input.as_str()),
        Rect::new(1, area.height - 2, area.width - 2, 1),
    );
}

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

fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    width: u16,
    height: u16,
    seconds: u64,
) -> io::Result<bool>
where
    B::Error: std::fmt::Display,
{
    let startup = Instant::now();
    let mut state = Workload::new(width);
    terminal
        .resize(Rect::new(0, 0, width, height))
        .map_err(|e| io::Error::other(e.to_string()))?;
    let rows = state.visible(width, height);
    terminal
        .draw(|frame| render(frame, rows, &state))
        .map_err(|e| io::Error::other(e.to_string()))?;
    let startup_ms = startup.elapsed().as_secs_f64() * 1000.0;
    let cpu_start = process_cpu_seconds()?;
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    let frame_period = Duration::from_secs_f64(1.0 / 60.0);
    let mut next_frame = start + frame_period;
    let mut token_tick = 1usize;
    let mut key_tick = 1u32;
    let mut scroll_tick = 1u32;
    let mut resize_tick = 1u32;
    let mut current_width = width;
    let mut draws = Vec::new();
    let mut latency = Vec::new();
    while Instant::now() < end {
        thread::sleep(next_frame.saturating_duration_since(Instant::now()));
        let now = Instant::now().min(end);
        let mut pending_keys = Vec::new();
        while start + Duration::from_millis(u64::from(key_tick) * 83) <= now {
            pending_keys.push(start + Duration::from_millis(u64::from(key_tick) * 83));
            if state.input.len() >= 60 {
                state.input.clear();
            }
            state.input.push(char::from(b'a' + (key_tick % 26) as u8));
            state.keys += 1;
            key_tick += 1;
        }
        while start + Duration::from_millis(token_tick as u64 * 25) <= now {
            state.tokens(token_tick, current_width);
            token_tick += 1;
        }
        while start + Duration::from_secs(u64::from(scroll_tick) * 2) <= now {
            state.anchor = match scroll_tick % 4 {
                0 => None,
                1 => Some(state.cached.len().saturating_sub(100)),
                2 => Some(state.cached.len() / 2),
                _ => Some(0),
            };
            state.scrolls += 1;
            scroll_tick += 1;
        }
        while start + Duration::from_secs(u64::from(resize_tick) * 5) <= now {
            current_width = if resize_tick % 2 == 1 {
                width - 1
            } else {
                width
            };
            terminal
                .resize(Rect::new(0, 0, current_width, height))
                .map_err(|e| io::Error::other(e.to_string()))?;
            state.reflow(current_width);
            state.resizes += 1;
            resize_tick += 1;
        }
        let draw_start = Instant::now();
        // Include live-row preparation, layout, buffer diff and backend flush.
        let rows = state.visible(current_width, height);
        let completed = terminal
            .draw(|frame| render(frame, rows, &state))
            .map_err(|e| io::Error::other(e.to_string()))?;
        let finished = Instant::now();
        draws.push(finished.duration_since(draw_start).as_secs_f64() * 1000.0);
        // Verify the mutated input really reached the rendered buffer.
        for (offset, byte) in state.input.bytes().enumerate() {
            assert_eq!(
                completed.buffer[(1 + offset as u16, height - 2)].symbol(),
                char::from(byte).to_string()
            );
        }
        latency.extend(
            pending_keys
                .into_iter()
                .map(|key| finished.duration_since(key).as_secs_f64() * 1000.0),
        );
        next_frame += frame_period;
        // Skip missed frame deadlines, but never skip token/input events.
        while next_frame <= finished {
            next_frame += frame_period;
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let cpu = (process_cpu_seconds()? - cpu_start) / elapsed * 100.0;
    let p95 = percentile(&mut draws, 0.95);
    let p99 = percentile(&mut draws, 0.99);
    let key_p95 = percentile(&mut latency, 0.95);
    let passed = p95 <= 8.0 && p99 <= 16.7 && key_p95 <= 50.0 && cpu <= 25.0;
    eprintln!(
        "{width}x{height}: elapsed={elapsed:.3}s frames={} deltas={} keys={} scrolls={} resizes={} startup_ms={startup_ms:.3} draw_p95_ms={p95:.3} draw_p99_ms={p99:.3} key_p95_ms={key_p95:.3} cpu_one_core_pct={cpu:.3} {} gate={}",
        draws.len(),
        state.deltas,
        state.keys,
        state.scrolls,
        state.resizes,
        memory(),
        if passed { "PASS" } else { "FAIL" }
    );
    Ok(passed)
}

fn main() -> io::Result<()> {
    let seconds =
        env::var("SPIKE_SECS").map_or(Ok(300), |s| s.parse::<u64>().map_err(io::Error::other))?;
    if seconds == 0 {
        return Err(io::Error::other("SPIKE_SECS must be positive"));
    }
    let real = env::var("SPIKE_TERMINAL").is_ok_and(|v| v == "1");
    eprintln!(
        "twelve_streams: mode={} secs_per_viewport={seconds} target={}-{} effects=off",
        if real { "terminal" } else { "TestBackend" },
        env::consts::OS,
        env::consts::ARCH
    );
    let mut passed = true;
    if real {
        if !io::stdout().is_terminal() {
            return Err(io::Error::other("SPIKE_TERMINAL=1 requires a TTY"));
        }
        let (width, height) = crossterm::terminal::size()?;
        if width < 120 || height < 36 {
            return Err(io::Error::other("real terminal must be at least 120x36"));
        }
        execute!(io::stdout(), EnterAlternateScreen)?;
        let result = (|| -> io::Result<()> {
            let backend = CrosstermBackend::new(io::stdout());
            let mut terminal = Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Fixed(Rect::new(0, 0, 120, 36)),
                },
            )?;
            for (width, height) in VIEWPORTS {
                passed &= run(&mut terminal, width, height, seconds)?;
            }
            Ok(())
        })();
        execute!(io::stdout(), LeaveAlternateScreen)?;
        result?;
    } else {
        for (width, height) in VIEWPORTS {
            let mut terminal = Terminal::with_options(
                TestBackend::new(width, height),
                TerminalOptions {
                    viewport: Viewport::Fixed(Rect::new(0, 0, width, height)),
                },
            )
            .expect("infallible TestBackend");
            passed &= run(&mut terminal, width, height, seconds)?;
        }
    }
    if !passed {
        return Err(io::Error::other("performance gate failed"));
    }
    Ok(())
}
