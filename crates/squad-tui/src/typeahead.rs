//! Shared ranking, completion annotations and parser-checked composer examples.
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::{
    Model,
    parser::{ParseContext, SubmissionMeta},
};

const MODE_INVENTORY: &[(&str, &str)] = &[
    ("$debate", "Head-to-head arguments and a judge"),
    ("$tournament", "Elimination rounds to choose a winner"),
    ("$grill", "Question one model in rounds"),
    ("$census", "Collect and refine independent plans"),
    ("$crosscheck", "Audit a plan for gaps and risks"),
    ("$survey", "Compare independent answers"),
    ("$probe", "Investigate evidence and hypotheses"),
    ("$forge", "Blind drafts fused into one best artifact"),
    (
        "$forecast",
        "A pooled probability with cruxes and signposts",
    ),
    (
        "$decide",
        "Score options on a frozen rubric and see what flips it",
    ),
    ("$negotiate", "Prepare and rehearse a negotiation"),
    (
        "$blow_it_up",
        "Red-team a plan and keep only attacks that survive",
    ),
    ("$readback", "Find where a spec reads two ways and fix it"),
    ("$debug", "Narrow a failure with tests you run"),
    (
        "$trace",
        "Build a brief where every claim quotes its source",
    ),
    ("$rank", "Order a list by pairwise comparison"),
    ("$tune", "Evolve a prompt against a hidden test set"),
    (
        "$bass_ackwards_prd",
        "Plan backward from the end result into a saved PRD",
    ),
];

/// Stable sorting keeps roster/pick priority within each match class.
pub fn rank(candidate: &str, query: &str) -> Option<u8> {
    let candidate = candidate.to_lowercase();
    let query = query.to_lowercase();
    let (candidate, query) =
        if let Some(marker) = query.chars().next().filter(|c| "@$/-".contains(*c)) {
            (candidate.strip_prefix(marker)?, &query[marker.len_utf8()..])
        } else {
            (
                candidate.trim_start_matches(['@', '$', '/', '-']),
                query.as_str(),
            )
        };
    if candidate.starts_with(query) {
        return Some(0);
    }
    if candidate.contains(query) {
        return Some(1);
    }
    let mut chars = candidate.chars();
    query
        .chars()
        .all(|c| chars.by_ref().any(|x| x == c))
        .then_some(2)
}

pub fn describe(token: &str, models: &[Model]) -> String {
    if let Some(nick) = token.strip_prefix('@') {
        if nick == "all" {
            return "Every room member".into();
        }
        return models
            .iter()
            .find(|m| m.nick.to_string() == nick)
            .map(|m| m.resolved_slug.to_string())
            .unwrap_or_default();
    }
    MODES
        .iter()
        .filter(|(token, _)| crate::typeahead::enabled(token))
        .chain(squad_protocol::parser::SLASH_COMMANDS.iter())
        .find(|(name, _)| *name == token)
        .map(|(_, hint)| hint.to_string())
        .unwrap_or_default()
}

/// Only examples accepted in this live context enter the rotation.
pub fn examples(ctx: &ParseContext) -> Vec<String> {
    let mut examples = vec![
        "@all what's the most underrated Unix command?".into(),
        "@all $tournament plan a falling-block puzzle game".into(),
        "/set ROUNDS 3".into(),
        "/who".into(),
        "/help".into(),
    ];
    if ctx.roster.len() >= 2 {
        examples.insert(
            1,
            format!(
                "@{} @{} $debate tabs or spaces @judge",
                ctx.roster[0], ctx.roster[1]
            ),
        );
    }
    examples.extend([
        "@all $forge write the launch post for our CLI".into(),
        "@all $forecast will we ship v1 by June 30?".into(),
        "@all $decide choose a database\n--\nSQLite\nManaged PostgreSQL".into(),
        "@all $blow_it_up ```All requests are accepted without authentication.```".into(),
        "@all $readback The service retains records.".into(),
        "@all $debug Requests stall after deployment\n--\nInspect local logs only".into(),
        "@all $trace When is support available?\n--\nSupport is available Monday through Friday."
            .into(),
        "@all $negotiate Agree a support contract\n--\nThe vendor wants predictable scope.".into(),
        "@all $rank impact for effort\n--\nfeature A\nfeature B\nfeature C".into(),
    ]);
    examples.extend([
        "@all $tune prompt\n--\ntask".into(),
        "@all $bass_ackwards_prd our CLI is installed by 1,000 teams and has a paid tier".into(),
    ]);
    examples.retain(|s| squad_protocol::parse(s, &SubmissionMeta::default(), ctx).is_ok());
    examples
}

const ALL_KEY_TIPS: &[&str] = &[
    "F6 focuses messages; r regenerates the selected reply",
    "Ctrl+F searches the transcript",
    "F6 focuses messages; x stops the selected model",
    "F6 focuses messages; t retries the selected reply",
    "Ctrl+J inserts a newline; Alt+Up/Down recalls history",
    "Tab opens suggestions; ↑/↓ choose; Enter accepts; Esc dismisses",
    "A contest waiting on you: type the answer, or /continue to move on",
];

pub fn tip(ctx: &ParseContext, seconds: u64) -> String {
    let examples = examples(ctx);
    let index = (seconds / 8) as usize % (examples.len() + KEY_TIPS.len());
    let text = examples
        .get(index)
        .map(String::as_str)
        .unwrap_or_else(|| KEY_TIPS[index - examples.len()]);
    format!("TIP: {text}")
}

pub fn render_tip(text: &str, area: Rect, buf: &mut Buffer, style: Style) {
    if area.width >= 60 && area.height > 0 {
        buf.set_stringn(area.x, area.y, text, usize::from(area.width), style);
    }
}

pub fn enabled(token: &str) -> bool {
    match token {
        "$forge" => squad_protocol::forge::ENABLED,
        "$forecast" => squad_protocol::forecast::ENABLED,
        "$decide" => squad_protocol::decide::ENABLED,
        "$negotiate" => squad_protocol::negotiate::ENABLED,
        "$blow_it_up" => squad_protocol::blow_it_up::ENABLED,
        "$readback" => squad_protocol::readback::ENABLED,
        "$debug" => squad_protocol::debugging::ENABLED,
        "$trace" => squad_protocol::trace::ENABLED,
        "$rank" => squad_protocol::rank::ENABLED,
        _ => true,
    }
}

const fn enabled_modes() -> ([(&'static str, &'static str); 18], usize) {
    let flags = [
        true,
        true,
        true,
        true,
        true,
        true,
        true,
        squad_protocol::forge::ENABLED,
        squad_protocol::forecast::ENABLED,
        squad_protocol::decide::ENABLED,
        squad_protocol::negotiate::ENABLED,
        squad_protocol::blow_it_up::ENABLED,
        squad_protocol::readback::ENABLED,
        squad_protocol::debugging::ENABLED,
        squad_protocol::trace::ENABLED,
        squad_protocol::rank::ENABLED,
        squad_protocol::tune::ENABLED,
        squad_protocol::bass_ackwards_prd::ENABLED,
    ];
    let mut values = [("", ""); 18];
    let mut len = 0;
    let mut i = 0;
    while i < 18 {
        if flags[i] {
            values[len] = MODE_INVENTORY[i];
            len += 1;
        }
        i += 1;
    }
    (values, len)
}
const ENABLED_MODES: ([(&str, &str); 18], usize) = enabled_modes();
pub const MODES: &[(&str, &str)] = ENABLED_MODES.0.split_at(ENABLED_MODES.1).0;

pub const KEY_TIPS: &[&str] = if squad_protocol::readback::ENABLED
    || squad_protocol::debugging::ENABLED
    || squad_protocol::bass_ackwards_prd::ENABLED
{
    ALL_KEY_TIPS
} else {
    ALL_KEY_TIPS.split_at(6).0
};
