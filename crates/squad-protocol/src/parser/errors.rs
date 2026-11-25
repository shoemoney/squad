//! Exact diagnostics owned by the command grammar.
use super::{ParseError, Span};
use crate::ErrorCode;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Empty,
    TooLarge,
    Inline,
    Fence,
    InvalidToken,
    UnknownCommand,
    Reserved,
    MultipleCommands,
    UnknownSlash,
    Usage,
    UnknownVariable,
    Rounds,
    InvalidIdentifier,
    TooManyQuotes,
    QuoteUnavailable,
    ControlQuotes,
    SelectionOutside,
    TooManySelections,
    UnknownRecipient,
    Me,
    JudgeQuote,
    AllMixed,
    UnknownExclusion,
    AllEmpty,
    DebatersRequired,
    RecipientRequired,
    BodyRequired,
    JudgeBodyRequired,
    SolutionInvalid,
    TopicMismatch,
    Busy,
    GrillRounds,
    TournamentCount,
    TournamentPrompt,
    GrillMissing,
    GrillCount,
    GrillSelector,
    GrillSelection,
    GrillJudge,
    DebateCandidates,
    GrillCandidates,
    DebateQuestion,
    GrillQuestion,
    QuestionSelection,
    DebatePick,
    GrillPick,
    PickIneligible,
    StalePick,
    InsufficientAnswers,
    DailyMax,
    CensusPasses,
    CensusCount,
    CrosscheckCount,
    SurveyCount,
    ProbeCount,
    CensusMissing,
    CrosscheckMissing,
    CensusConflict,
    CensusSelection,
    CrosscheckSelection,
    CensusLatest,
    CrosscheckLatest,
    SurveyMissing,
    ProbeBrief,
    CensusNotWaiting,
    ContinueSelection,
    NoCompleteCrosscheck,
    AcceptanceStale,
}
impl Rule {
    pub fn template(self) -> &'static str {
        match self {
            Self::CensusNotWaiting => "No census is waiting for a plan.",
            Self::ContinueSelection => {
                "Select exactly one member reply or grill plan version for /continue."
            }
            Self::NoCompleteCrosscheck => "No complete crosscheck is available for acceptance.",
            Self::AcceptanceStale => "Crosscheck acceptance is stale. Refresh the coverage ledger.",
            Self::Empty => "Enter a message or command.",
            Self::TooLarge => "Keep the submission within 32,768 UTF-8 bytes.",
            Self::Inline => "Close the inline code with a backtick.",
            Self::Fence => "Close the code block with three backticks.",
            Self::InvalidToken => "Invalid token: {token}. Escape the sigil to use it literally.",
            Self::UnknownCommand => "Unknown command: ${verb}.",
            Self::Reserved => "Command ${verb} is reserved and unavailable in v1.",
            Self::MultipleCommands => "Use only one $ command per submission.",
            Self::UnknownSlash => "Unknown slash command: /{name}.",
            Self::Usage => "Usage: {usage}",
            Self::UnknownVariable => "Unknown variable: {name}.",
            Self::Rounds => "ROUNDS must be an integer from 1 to 20.",
            Self::InvalidIdentifier => "Invalid {argument}: {value}.",
            Self::TooManyQuotes => "Attach no more than 12 quoted messages.",
            Self::QuoteUnavailable => "Quoted message {id} is unavailable.",
            Self::ControlQuotes => "Quotes require a conversational message.",
            Self::SelectionOutside => "Solution selection is only allowed with $debate.",
            Self::TooManySelections => "Select no more than two solutions for $debate.",
            Self::UnknownRecipient => "Unknown recipient: @{nick}.",
            Self::Me => "@me cannot be a recipient.",
            Self::JudgeQuote => "Quote at least one message for @judge to rule on.",
            Self::AllMixed => "Use @all by itself or name individual recipients.",
            Self::UnknownExclusion => "Unknown recipient: @{nick}.",
            Self::AllEmpty => "@all exclusions must leave at least one recipient.",
            Self::DebatersRequired => "$debate requires exactly two distinct debaters.",
            Self::RecipientRequired => "Add at least one recipient: @nick or @all.",
            Self::BodyRequired => "Enter message text after the recipients.",
            Self::JudgeBodyRequired => "Enter message text after the recipients.",
            Self::SolutionInvalid => "Selected solution is not a reply to me from @{nick}.",
            Self::TopicMismatch => "Debaters' solutions must answer the same user message.",
            Self::Busy => "A {kind} is already running. Stop it before starting another.",
            Self::GrillRounds => "GRILL_ROUNDS must be an integer from 1 to 10.",
            Self::TournamentCount => "$tournament needs at least three entrants.",
            Self::TournamentPrompt => "Add a prompt after $tournament.",
            Self::GrillMissing => "@{nick} has no plan to grill yet. Ask it something first.",
            Self::GrillCount => "$grill needs at least one griller.",
            Self::GrillSelector => "$grill needs a leader or at least two candidates.",
            Self::GrillSelection => "Select exactly one reply from @{nick} as the plan to grill.",
            Self::GrillJudge => "@judge cannot be used with $grill.",
            Self::DebateCandidates => "$debate needs at least two candidates.",
            Self::GrillCandidates => "$grill needs at least two candidates.",
            Self::DebateQuestion => "Add a question after $debate.",
            Self::GrillQuestion => "Add a question after $grill.",
            Self::QuestionSelection => {
                "Question-first contests use their collected answers; do not attach solution selections."
            }
            Self::DebatePick => "Pick exactly two of: {eligible}.",
            Self::GrillPick => "Pick exactly one of: {eligible}.",
            Self::StalePick => "The pending pick is no longer available.",
            Self::PickIneligible => "@{nick} did not answer, so it cannot be picked.",
            Self::InsufficientAnswers => {
                "Not enough answers to start. {n} of {m} members answered."
            }
            Self::DailyMax => {
                "Daily max of ${max} reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow."
            }
            Self::CensusPasses => "CENSUS_PASSES must be an integer from 1 to 5.",
            Self::CensusCount => "$census needs at least three participants.",
            Self::CrosscheckCount => "$crosscheck needs at least three participants.",
            Self::SurveyCount => "Add at least one recipient: @nick or @all.",
            Self::ProbeCount => {
                "$probe needs at least four members; an odd selection leaves the last member out."
            }
            Self::CensusMissing => {
                "Add a plan for $census: quote one plan, paste it, or use @nick for that member's latest reply."
            }
            Self::CrosscheckMissing => {
                "Add a plan for $crosscheck: quote one plan, paste it, or use @nick for that member's latest reply."
            }
            Self::CensusConflict => "Supply the $census plan by quote or pasted text, not both.",
            Self::CensusSelection => {
                "Select exactly one member reply or grill plan version for $census."
            }
            Self::CrosscheckSelection => {
                "Select exactly one member reply or grill plan version for $crosscheck."
            }
            Self::CensusLatest => "@{nick} has no plan for $census yet. Ask it something first.",
            Self::CrosscheckLatest => {
                "@{nick} has no plan for $crosscheck yet. Ask it something first."
            }
            Self::SurveyMissing => {
                "No parallel answers are available for $survey. Add a question after $survey."
            }
            Self::ProbeBrief => {
                "Add an observation, what is ruled out, and what can be inspected after $probe."
            }
        }
    }
    pub fn code(self) -> ErrorCode {
        match self {
            Self::CensusNotWaiting => ErrorCode::INVALID_STATE_TRANSITION,
            Self::ContinueSelection => ErrorCode::CENSUS_SOURCE_MISSING,
            Self::NoCompleteCrosscheck => ErrorCode::CONTEST_NOT_FOUND,
            Self::AcceptanceStale => ErrorCode::CROSSCHECK_ACCEPTANCE_STALE,
            Self::Empty => ErrorCode::INVALID_COMMAND,
            Self::TooLarge => ErrorCode::MESSAGE_TOO_LARGE,
            Self::Inline => ErrorCode::INVALID_COMMAND,
            Self::Fence => ErrorCode::INVALID_COMMAND,
            Self::InvalidToken => ErrorCode::INVALID_COMMAND,
            Self::UnknownCommand => ErrorCode::INVALID_COMMAND,
            Self::Reserved => ErrorCode::COMMAND_NOT_IMPLEMENTED,
            Self::MultipleCommands => ErrorCode::INVALID_COMMAND,
            Self::UnknownSlash => ErrorCode::INVALID_COMMAND,
            Self::Usage => ErrorCode::INVALID_COMMAND,
            Self::UnknownVariable => ErrorCode::INVALID_COMMAND,
            Self::Rounds => ErrorCode::INVALID_COMMAND,
            Self::InvalidIdentifier => ErrorCode::INVALID_COMMAND,
            Self::TooManyQuotes => ErrorCode::INVALID_REFERENCE,
            Self::QuoteUnavailable => ErrorCode::INVALID_REFERENCE,
            Self::ControlQuotes => ErrorCode::INVALID_REFERENCE,
            Self::SelectionOutside => ErrorCode::INVALID_REFERENCE,
            Self::TooManySelections => ErrorCode::INVALID_REFERENCE,
            Self::UnknownRecipient => ErrorCode::UNKNOWN_RECIPIENT,
            Self::Me => ErrorCode::INVALID_COMMAND,
            Self::JudgeQuote => ErrorCode::JUDGE_QUOTE_REQUIRED,
            Self::AllMixed => ErrorCode::INVALID_COMMAND,
            Self::UnknownExclusion => ErrorCode::UNKNOWN_RECIPIENT,
            Self::AllEmpty => ErrorCode::INVALID_COMMAND,
            Self::DebatersRequired => ErrorCode::INVALID_COMMAND,
            Self::RecipientRequired => ErrorCode::RECIPIENT_REQUIRED,
            Self::BodyRequired => ErrorCode::INVALID_COMMAND,
            Self::JudgeBodyRequired => ErrorCode::INVALID_COMMAND,
            Self::SolutionInvalid => ErrorCode::DEBATE_SOURCE_MISSING,
            Self::TopicMismatch => ErrorCode::DEBATE_TOPIC_MISMATCH,
            Self::Busy => ErrorCode::CONTEST_BUSY,
            Self::GrillRounds => ErrorCode::INVALID_COMMAND,
            Self::TournamentCount => ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED,
            Self::TournamentPrompt => ErrorCode::INVALID_COMMAND,
            Self::GrillMissing => ErrorCode::GRILL_SOURCE_MISSING,
            Self::GrillCount => ErrorCode::GRILL_PARTICIPANTS_REQUIRED,
            Self::GrillSelector => ErrorCode::GRILL_PARTICIPANTS_REQUIRED,
            Self::GrillSelection => ErrorCode::GRILL_SOURCE_MISSING,
            Self::GrillJudge => ErrorCode::INVALID_COMMAND,
            Self::DebateCandidates => ErrorCode::INVALID_COMMAND,
            Self::GrillCandidates => ErrorCode::INVALID_COMMAND,
            Self::DebateQuestion => ErrorCode::INVALID_COMMAND,
            Self::GrillQuestion => ErrorCode::INVALID_COMMAND,
            Self::QuestionSelection => ErrorCode::INVALID_REFERENCE,
            Self::DebatePick => ErrorCode::CONTEST_PICK_INVALID,
            Self::GrillPick => ErrorCode::CONTEST_PICK_INVALID,
            Self::StalePick => ErrorCode::CONTEST_PICK_INVALID,
            Self::PickIneligible => ErrorCode::CONTEST_PICK_INVALID,
            Self::InsufficientAnswers => ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS,
            Self::DailyMax => ErrorCode::DAILY_MAX_REACHED,
            Self::CensusPasses => ErrorCode::INVALID_COMMAND,
            Self::CensusCount => ErrorCode::CENSUS_PARTICIPANTS_REQUIRED,
            Self::CrosscheckCount => ErrorCode::CROSSCHECK_PARTICIPANTS_REQUIRED,
            Self::SurveyCount => ErrorCode::SURVEY_PARTICIPANTS_REQUIRED,
            Self::ProbeCount => ErrorCode::PROBE_PARTICIPANTS_REQUIRED,
            Self::CensusMissing => ErrorCode::CENSUS_SOURCE_MISSING,
            Self::CrosscheckMissing => ErrorCode::CROSSCHECK_SOURCE_MISSING,
            Self::CensusConflict => ErrorCode::INVALID_COMMAND,
            Self::CensusSelection => ErrorCode::CENSUS_SOURCE_MISSING,
            Self::CrosscheckSelection => ErrorCode::CROSSCHECK_SOURCE_MISSING,
            Self::CensusLatest => ErrorCode::CENSUS_SOURCE_MISSING,
            Self::CrosscheckLatest => ErrorCode::CROSSCHECK_SOURCE_MISSING,
            Self::SurveyMissing => ErrorCode::SURVEY_SOURCE_MISSING,
            Self::ProbeBrief => ErrorCode::PROBE_INPUT_REQUIRED,
        }
    }
    pub fn error(self, span: Span, values: &[(&str, &str)]) -> ParseError {
        let mut message = self.template().to_owned();
        for (key, value) in values {
            message = message.replace(&format!("{{{key}}}"), value);
        }
        ParseError {
            code: self.code(),
            message,
            span,
        }
    }
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ParseError {}

/// The execution layer shares parser-owned exact diagnostics without formatting its own strings.
pub fn daily_max(max: f64, span: Span) -> ParseError {
    Rule::DailyMax.error(span, &[("max", &format!("{max:.2}"))])
}
pub fn insufficient_answers(answered: usize, candidates: usize, span: Span) -> ParseError {
    Rule::InsufficientAnswers.error(
        span,
        &[("n", &answered.to_string()), ("m", &candidates.to_string())],
    )
}
pub fn contest_busy(
    requested: crate::ContestKind,
    active: crate::ContestKind,
    span: Span,
) -> ParseError {
    let mut error = Rule::Busy.error(span, &[("kind", active.as_str())]);
    if requested == crate::ContestKind::Debate {
        error.code = ErrorCode::DEBATE_BUSY;
    }
    error
}

/// All parser-owned diagnostics in specification order.
pub const ALL_RULES: &[Rule] = &[
    Rule::Empty,
    Rule::TooLarge,
    Rule::Inline,
    Rule::Fence,
    Rule::InvalidToken,
    Rule::UnknownCommand,
    Rule::Reserved,
    Rule::MultipleCommands,
    Rule::UnknownSlash,
    Rule::Usage,
    Rule::UnknownVariable,
    Rule::Rounds,
    Rule::InvalidIdentifier,
    Rule::TooManyQuotes,
    Rule::QuoteUnavailable,
    Rule::ControlQuotes,
    Rule::SelectionOutside,
    Rule::TooManySelections,
    Rule::UnknownRecipient,
    Rule::Me,
    Rule::JudgeQuote,
    Rule::AllMixed,
    Rule::UnknownExclusion,
    Rule::AllEmpty,
    Rule::DebatersRequired,
    Rule::RecipientRequired,
    Rule::BodyRequired,
    Rule::JudgeBodyRequired,
    Rule::SolutionInvalid,
    Rule::TopicMismatch,
    Rule::Busy,
    Rule::GrillRounds,
    Rule::TournamentCount,
    Rule::TournamentPrompt,
    Rule::GrillMissing,
    Rule::GrillCount,
    Rule::GrillSelector,
    Rule::GrillSelection,
    Rule::GrillJudge,
    Rule::DebateCandidates,
    Rule::GrillCandidates,
    Rule::DebateQuestion,
    Rule::GrillQuestion,
    Rule::QuestionSelection,
    Rule::DebatePick,
    Rule::GrillPick,
    Rule::PickIneligible,
    Rule::StalePick,
    Rule::InsufficientAnswers,
    Rule::DailyMax,
    Rule::CensusPasses,
    Rule::CensusCount,
    Rule::CrosscheckCount,
    Rule::SurveyCount,
    Rule::ProbeCount,
    Rule::CensusMissing,
    Rule::CrosscheckMissing,
    Rule::CensusConflict,
    Rule::CensusSelection,
    Rule::CrosscheckSelection,
    Rule::CensusLatest,
    Rule::CrosscheckLatest,
    Rule::SurveyMissing,
    Rule::ProbeBrief,
    Rule::CensusNotWaiting,
    Rule::ContinueSelection,
    Rule::NoCompleteCrosscheck,
    Rule::AcceptanceStale,
];
