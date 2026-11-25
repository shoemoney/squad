//! Shared, plain banner text. No terminal, environment, colour, or animation state
//! is read here. Exports use [`render_default`]; the TUI supplies width to [`render`].

use std::sync::LazyLock;

use figlet_rs::FIGlet;
pub use squad_protocol::config::BannerMode;
use unicode_width::UnicodeWidthStr;

static DEFAULT: LazyLock<FIGlet> = LazyLock::new(|| {
    FIGlet::from_content(include_str!("fonts/standard.flf")).expect("vendored Standard font")
});
static COMPACT: LazyLock<FIGlet> = LazyLock::new(|| {
    FIGlet::from_content(include_str!("fonts/small.flf")).expect("vendored Small font")
});

/// Presentation boundaries; callers derive these from committed contest state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Banner {
    Opening,
    Round(usize),
    FinalRound,
    Verdict,
    Tournament,
    TournamentRound { round: usize, remaining: usize },
    Final,
    Champion,
    Grill,
    GrillRound(usize),
    FinalPlan,
    Survived,
    ChooseDebate,
    ChooseGrill,
    Census,
    Crosscheck,
    Survey,
    Probe,
}

impl Banner {
    /// The ordinary label is retained even when FIGlet banners are disabled.
    pub fn text(self) -> String {
        match self {
            Self::Opening => "OPENING".into(),
            Self::Round(n) => format!("ROUND {n}"),
            Self::FinalRound => "FINAL ROUND".into(),
            Self::Verdict => "VERDICT".into(),
            Self::Tournament => "TOURNAMENT".into(),
            Self::TournamentRound { remaining: 2, .. } => "FINAL".into(),
            Self::TournamentRound { round, remaining } => {
                format!("ROUND {round} · {remaining} REMAIN")
            }
            Self::Final => "FINAL".into(),
            Self::Champion => "CHAMPION".into(),
            Self::Grill => "GRILL".into(),
            Self::GrillRound(n) => format!("GRILL ROUND {n}"),
            Self::FinalPlan => "FINAL PLAN".into(),
            Self::Survived => "SURVIVED".into(),
            Self::ChooseDebate => "CHOOSE YOUR DEBATE BROS".into(),
            Self::ChooseGrill => "CHOOSE WHO GETS GRILLED".into(),
            Self::Census => "CENSUS".into(),
            Self::Crosscheck => "CROSSCHECK".into(),
            Self::Survey => "SURVEY".into(),
            Self::Probe => "PROBE".into(),
        }
    }

    /// Select the debate boundary, including a one-round debate's final round.
    pub fn debate_round(round: usize, configured_rounds: usize) -> Self {
        if round == 0 {
            Self::Opening
        } else if round == configured_rounds {
            Self::FinalRound
        } else {
            Self::Round(round)
        }
    }
}

/// The selected static presentation, useful for transcript row-height caches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerStyle {
    Default,
    Compact,
    Divider,
    Label,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedBanner {
    /// Complete boundary text, even if an extremely narrow viewport clips it.
    pub label: String,
    /// Centred plain rows, without trailing whitespace or terminal escapes.
    pub lines: Vec<String>,
    pub style: BannerStyle,
}

impl RenderedBanner {
    pub fn height(&self) -> usize {
        self.lines.len()
    }

    pub fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(0)
    }
}

/// Render the embedded default font, without wrapping, centring, or width limits.
/// Output has no trailing spaces or newline, making it suitable for code fences.
/// Unsupported glyphs fall back to the complete plain label rather than vanishing.
pub fn render_default(text: &str) -> String {
    let text = plain_label(text);
    font_lines(&DEFAULT, &text)
        .unwrap_or_else(|| vec![text])
        .join("\n")
}

/// Render for an explicit viewport. `On` tries Standard, then Small, then a divider;
/// `Compact` starts with Small; `Off` retains just the ordinary label.
/// If even the label cannot fit, a single line is ellipsised to the viewport.
/// Width zero yields no rows. The full text remains in [`RenderedBanner::label`].
pub fn render(text: &str, width: usize, mode: BannerMode) -> RenderedBanner {
    let label = plain_label(text);
    let mut rendered = RenderedBanner {
        label,
        lines: Vec::new(),
        style: BannerStyle::Label,
    };
    if width == 0 || rendered.label.is_empty() {
        return rendered;
    }
    if mode != BannerMode::Off {
        let fonts = [
            (BannerStyle::Default, &*DEFAULT),
            (BannerStyle::Compact, &*COMPACT),
        ];
        for (style, font) in fonts {
            if mode == BannerMode::Compact && style == BannerStyle::Default {
                continue;
            }
            if let Some(lines) = font_lines(font, &rendered.label)
                && lines.iter().all(|line| line.width() <= width)
            {
                rendered.style = style;
                rendered.lines = centre(lines, width);
                return rendered;
            }
        }
        rendered.style = BannerStyle::Divider;
    }
    let label_width = rendered.label.width();
    let line = if mode != BannerMode::Off && width >= label_width + 4 {
        let ornament = ((width - label_width - 2) / 2).min(4);
        format!(
            "{} {} {}",
            "─".repeat(ornament),
            rendered.label,
            "─".repeat(ornament)
        )
    } else {
        fit_label(&rendered.label, width)
    };
    rendered.lines = centre(vec![line], width);
    rendered
}

fn font_lines(font: &FIGlet, text: &str) -> Option<Vec<String>> {
    if !text.chars().all(|ch| font.fonts.contains_key(&(ch as u32))) {
        return None;
    }
    let figure = font.convert(text)?;
    let output = figure.to_string();
    let mut lines: Vec<String> = output
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect();
    // Remove the font's blank bottom margin, retaining glyph-relative indentation.
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    Some(lines)
}

fn plain_label(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fit_label(label: &str, width: usize) -> String {
    if label.width() <= width {
        return label.to_owned();
    }
    let mut fitted = String::new();
    for ch in label.chars() {
        if fitted.width() + ch.to_string().width() > width - 1 {
            break;
        }
        fitted.push(ch);
    }
    fitted.push('…');
    fitted
}

fn centre(lines: Vec<String>, width: usize) -> Vec<String> {
    let block_width = lines.iter().map(|line| line.width()).max().unwrap_or(0);
    let padding = " ".repeat((width - block_width) / 2);
    lines
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                line
            } else {
                format!("{padding}{line}")
            }
        })
        .collect()
}
