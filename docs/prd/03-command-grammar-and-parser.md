# Command grammar and parser

`crates/squad-protocol/src/parser/` owns tokenization, grammar, the parser AST, variable expansion, and exact user-facing validation strings. The TUI and one-shot commands parse with the same Rust crate within the single `squad` binary. Parsing is deterministic and has no side effects. The router revalidates the supplied AST and references against authoritative configuration, room variables, and referenced records before acceptance.

Shared identifiers, envelope fields, and the complete `ErrorCode` enum belong to **Message protocol and bus**. `SubmissionMeta` and parser context types are declared in Protocol; `RoomHandle` operations belong to **Room API and CLI**. `SquadConfig` and environment loading belong to **Foundations**. Source resolution, cases, admission, and contest execution belong to **Contests**. The parser consumes those contracts without defining alternate versions.

Live requests use the in-process bus, and durable room state uses SQLite. The parser receives supplied snapshots and record summaries rather than reading or changing persistent state.

Only human input is parsed for routing or commands. Model output, quoted text, expanded variables, and resolved source text are never scanned for executable syntax.

All seven modes use the shared contest engine under D71. The parser identifies the requested mode, resolves selectors, extracts arguments, and stages variable updates. The engine owns scripts over `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier`, and `report`, including their visibility, failure rules, and preflight call counts. Move names are not additional executable commands. The parser does not assign lenses, divide work, choose auditors, split probe roles, compute tallies, or assemble reports.

Canonical values are defined in [Protocol’s canonical numbers and configuration](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration).

Configuration comes from the single `.env` file through the environment-loading contract in **Foundations**. Process environment variables override file values. Word limits use `SQUAD_ROOM_WORDS`, `SQUAD_TURN_WORDS`, `SQUAD_FINAL_TURN_WORDS`, and optional `SQUAD_PLAN_WORDS`; spending settings use `SQUAD_DAILY_MAX_USD` and `SQUAD_DEBATE_CONFIRM_USD`. `SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS`. Optional `SQUAD_CENSUS_PASSES` seeds `CENSUS_PASSES` and is not included in `.env.example`. Optional `SQUAD_REASONING_EFFORT_<NICK>` overrides a member’s default effort. Timeouts, attempt counts, queue depth, visible output token ceilings, reasoning budgets, message size limits, preflight expiry, leases, and channel capacities are compile-time constants in `squad-protocol`.

For models whose catalog entry lists reasoning support, the request sends `max_tokens = visible ceiling + reasoning budget` and an explicit `reasoning.max_tokens` equal to that budget. Models without reasoning support receive the visible ceiling without reasoning parameters. Default effort does not reject an unsupported model; an explicitly configured unsupported effort follows the validation contract in **Agents and OpenRouter**. Provider request construction, alias probes, and cost accounting belong to that section.

The daily max uses actual reported call costs under **Message protocol and bus**. In-flight calls finish when the max is reached. New generation and contest estimates that exceed the remaining daily amount are rejected under the admission contract in **Contests**.

### Grammar

The following EBNF operates outside protected code and escaped text. `WS` means one or more spaces, tabs, or newlines. Leading and trailing whitespace is permitted.

```ebnf
input            = slash | content ;

slash            = "/", slash-name, { WS, argument } ;
slash-name       = letter, { letter | digit | "-" } ;
argument         = non-whitespace, { non-whitespace } ;

content          = segment, { segment } ;
segment          = assignment
                 | all-selector
                 | mention
                 | command
                 | variable
                 | protected
                 | text ;

all-selector     = "@all", { WS, exclusion } ;
exclusion        = "-", identifier ;

mention          = "@", identifier ;
command          = "$", verb ;
variable         = "$", variable-name ;
assignment       = variable, "=", [ assignment-value ] ;
assignment-value = non-whitespace, { non-whitespace } ;

identifier       = letter, { letter | digit | "_" | "-" } ;
verb             = lower, { lower | digit | "_" } ;
variable-name    = upper, { upper | digit | "_" } ;

protected        = escape | inline-code | fenced-code ;
escape           = "\", ( "@" | "$" | "\" | "`" | "/" ) ;
inline-code      = "`", { inline-character }, "`" ;
fenced-code      = fence-open, { fenced-character }, fence-close ;

lower            = "a" … "z" ;
upper            = "A" … "Z" ;
letter           = lower | upper ;
digit            = "0" … "9" ;
```

`identifier` recognizes candidates for diagnostics. Configured member names must satisfy the canonical nick pattern. Incorrect casing or an unconfigured candidate produces the unknown-recipient error; the parser never changes case or invents a member.

`text` consumes characters not recognized by a higher-priority rule. Active sigil tokens begin at input start, after whitespace, or after `(`, `[`, or `{`. Names use maximal matching. Consequently, `person@example.com` is text, while `(@claude)` contains a mention. Terminal punctuation remains text unless consumed as part of an assignment value.

A bare `@` or `$`, and `$` followed by a digit, are literal text. A token-boundary sigil followed by a name-shaped sequence must satisfy its grammar. `$Rounds` is invalid. Names are case-sensitive.

Slash syntax applies only when the first non-whitespace character is an unescaped `/`. Slash arguments receive no mention, variable, or assignment expansion. The command-specific argument rules below validate them.

An assignment is recognized wherever an active uppercase variable is immediately followed by `=`. Its value continues to whitespace or input end. Empty values are invalid. Assignments can appear before, between, or after message content.

After removing assignment tokens, a submission containing only whitespace is an assignment control operation. Otherwise it is parsed as a message, standalone judge request, contest submission, or pending-pick response. Slash commands and assignment-only submissions require no recipients and never become model messages.

The supported executable `$verb` commands are `$debate`, `$tournament`, `$grill`, `$census`, `$crosscheck`, `$survey`, and `$probe`. A submission may contain only one executable `$verb`. Variables and assignments do not count as commands.

A pick candidate has the contextual grammar below. It is recognized only while the supplied room snapshot contains a pending pick.

```ebnf
pick-candidate   = mention, { WS, mention } ;
```

A pick candidate contains only explicit member mentions and whitespace, with no `@all`, command, assignment, body text, protected text, quote attachment, or solution selection. Its required cardinality and eligibility depend on the pending contest.

For pasted-plan `$crosscheck`, a line containing only `--`, with optional surrounding spaces or tabs, separates the plan from an optional goal-and-constraints brief. Only the first such line outside protected text is a separator. It is recognized before variable expansion. The separator has no special meaning in other commands, quoted sources, or the brief after the first separator.

### Protected text and escaping

Inline code uses paired single backticks on one line. Fenced code opens with three backticks at line start, optionally preceded by up to three spaces and followed by a language tag. It closes with a separate line containing three backticks and optional surrounding whitespace.

Code contents cannot contain active mentions, exclusions, commands, variables, or assignments. Code delimiters remain in message bodies and contest instructions.

Supported escapes remove the protecting backslash in resulting text. `\@claude`, `\$ROUNDS`, and `\/help` are literal. Unknown escapes remain unchanged. Escape processing occurs outside code only.

Email addresses need no escaping because their internal `@` is not at a token boundary. Use `\@` or code formatting to guarantee literal treatment in any position.

The entry point is `parse(raw: &str, meta: &SubmissionMeta, ctx: &ParseContext) -> ParseResult`; `SubmissionMeta`, `ParseContext`, `RecordSummary`, `VariableName`, `VariableUpdate` and `ExportScope` are declared in [Protocol](02-message-protocol-store-and-bus.md#shared-api-value-types).

### Parsed AST

`MemberNick`, `EventId`, `MessageId`, `DebateId`, `ContestId`, and `ErrorCode` below use the shared protocol types. Arrays and values receive runtime validation. The helper enums represent the existing literal values.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
} // UTF-8 byte offsets into raw; end exclusive.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationalInput {
    pub raw: String,
    #[serde(default)]
    pub quote: Vec<EventId>,
    #[serde(default)]
    pub variable_updates: Vec<VariableUpdate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JudgeRecipient {
    #[serde(rename = "judge")]
    Judge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionFirstMode {
    Debate,
    Grill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlanSource {
    Pasted {
        text: String,
    },
    Quoted {
        event_id: EventId,
    },
    LatestReply {
        nick: MemberNick,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SurveySource {
    Question {
        text: String,
    },
    LatestParallelAnswers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingPick {
    pub contest_id: ContestId,
    pub mode: QuestionFirstMode,
    pub eligible: Vec<MemberNick>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "name",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum SlashAction {
    Set {
        variable: VariableName,
        value: u8,
    },
    Stop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nick: Option<MemberNick>,
    },
    Judge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debate_id: Option<DebateId>,
    },
    Regen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<MessageId>,
    },
    Export {
        scope: ExportScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<ContestId>,
    },
    Continue {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<String>,
    },
    Accept {
        cell_ids: Vec<CellId>,
        reason: String,
    },
    Clear,
    Who,
    Help,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ParsedInput {
    Message {
        #[serde(flatten)]
        input: ConversationalInput,
        recipients: Vec<MemberNick>,
        body: String,
    },
    Debate {
        #[serde(flatten)]
        input: ConversationalInput,
        debaters: [MemberNick; 2],
        rounds: u8,
        instruction: String,
        judge_requested: bool,
        #[serde(default)]
        reply_to: Vec<EventId>,
    },
    Judge {
        #[serde(flatten)]
        input: ConversationalInput,
        recipients: [JudgeRecipient; 1],
        question: String,
    },
    Assignment {
        raw: String,
        updates: Vec<VariableUpdate>,
    },
    Slash {
        raw: String,
        action: SlashAction,
    },
    Tournament {
        #[serde(flatten)]
        input: ConversationalInput,
        entrants: Vec<MemberNick>,
        prompt: String,
    },
    Grill {
        #[serde(flatten)]
        input: ConversationalInput,
        leader: MemberNick,
        grillers: Vec<MemberNick>,
        rounds: u8,
        focus: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_selection: Option<EventId>,
    },
    QuestionFirst {
        #[serde(flatten)]
        input: ConversationalInput,
        mode: QuestionFirstMode,
        candidates: Vec<MemberNick>,
        question: String,
        rounds: u8,
        judge_requested: bool,
    },
    ContestPick {
        raw: String,
        contest_id: ContestId,
        nicks: Vec<MemberNick>,
    },
    Census {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<MemberNick>,
        plan: PlanSource,
        passes: u8,
    },
    Crosscheck {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<MemberNick>,
        plan: PlanSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal_constraints: Option<String>,
    },
    Survey {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<MemberNick>,
        source: SurveySource,
    },
    Probe {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<MemberNick>,
        brief: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Span,
}

pub type ParseResult = Result<ParsedInput, ParseError>;
```

`ParseResult` is a plain Rust `Result`; it has no custom boolean discriminator. The conversational fields remain flattened, `variableUpdates` is an array, and omitted optional slash IDs remain absent. Wire validation follows the strict schema contract in **Message protocol and bus**, including validation before deserialization where flattened or required nullable fields prevent serde derive alone from enforcing the contract.

`raw` preserves the submitted text exactly. Spans use UTF-8 byte offsets into that text, including when an error concerns a removed token. Span endpoints must lie on UTF-8 character boundaries. An attachment error without a corresponding text token uses the whole-input span.

`quote` and `replyTo` are supplied through the submission metadata contract in **Room API and CLI**. They are not typed syntax. Missing arrays normalize to empty arrays. The parser does not introduce a quote command or interpret pasted IDs as attachments.

Bodies remove active recipient and assignment tokens, expand variables, decode escapes, and trim surrounding whitespace. Removing tokens joins adjacent whitespace with one space; other whitespace remains unchanged. Protected code remains intact.

Omitted optional slash IDs stay absent in the AST. Their target resolution belongs to the relevant execution contract.

`PendingPick` is a supplied parser snapshot, not a second persisted contest record. The router revalidates its contest ID, state, and eligible answers before accepting a `ContestPick`. The AST maps to the canonical `Request::ContestPick { contestId, nicks }` contract in **Message protocol and bus**.

`variableUpdates` and assignment-only `updates` contain at most one update per variable, ordered by the textual position of each variable’s final assignment. Empty conversational update arrays mean no staged changes.

`PlanSource` and `SurveySource` describe human source choices. They do not contain a second copy of a persisted contest or answer batch. The router resolves and freezes authoritative source records at admission. `LatestReply` and `LatestParallelAnswers` never mean “look again before each model call.”

The four new AST variants enter their respective scripts directly. They do not use `QuestionFirstMode`, create a pending pick, or reinterpret mentions-only input as a control response.

### Recipient resolution

Ordinary recipients are configured room members. Recipient arrays preserve first-occurrence order and remove duplicates. `@all` expands to the current configured roster in configuration order, excluding reserved roles.

The roster comes from `SQUAD_MODEL_<NICK>` variables under the configuration contract in **Foundations**. Roster changes take effect on restart.

`@all -nick [-nick...]` removes the named members from that expansion. Exclusions form one uninterrupted whitespace-separated group directly after `@all`. Repeated exclusions have the same effect as one exclusion. Every exclusion must name a configured member.

The first non-exclusion segment ends the group. A later `-nick`, or a `-nick` without preceding `@all`, is ordinary body text. Exclusions never add recipients.

An `@all` selector cannot be combined with explicit member mentions. Excluding every configured member is an error. The committed message stores the resolved explicit recipient list, following **Message protocol and bus**.

`@me` cannot receive a submitted conversational message. `@judge` follows its separate rules below and never expands from `@all`.

Contest candidate and entrant lists use the same expansion, exclusion, and deduplication rules. `@judge` never counts toward a member count. Direct grill leader selection uses the first distinct explicit member mention.

Census, crosscheck, survey, and probe participant lists use the same expansion, exclusions, first-occurrence order, and deduplication. Census and crosscheck additionally support the sole-member source shortcut specified below. A repeated mention does not turn that shortcut into a restricted participant list.

### Room variables

`ROUNDS`, `GRILL_ROUNDS`, and `CENSUS_PASSES` are the v1 variables. Bare `$ROUNDS`, `$GRILL_ROUNDS`, and `$CENSUS_PASSES` expand to their effective values for this submission.

`SQUAD_ROUNDS` seeds `ROUNDS` when the room has no stored value. Its default is 10, with a minimum of 1 and maximum of 20. A stored room value takes precedence over that seed.

`SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS` when the room has no stored value. Its default is 2, with a minimum of 1 and maximum of 10. A stored room value takes precedence over that seed.

`SQUAD_CENSUS_PASSES` seeds `CENSUS_PASSES` when the room has no stored value. Its default is 2, with a minimum of 1 and maximum of 5. A stored room value takes precedence over that seed. The environment variable is optional and is not included in `.env.example`.

`$ROUNDS=value`, `$GRILL_ROUNDS=value`, and `$CENSUS_PASSES=value` work anywhere outside protected text in a conversational submission. `/set ROUNDS value`, `/set GRILL_ROUNDS value`, and `/set CENSUS_PASSES value` perform the equivalent standalone updates. Values accept decimal digits only; signs, fractions, and exponent notation are invalid.

The parser validates all assignments before producing a successful result. For each variable, the last assignment in textual order determines its staged update. The effective values supply every bare variable expansion and the applicable contest’s `rounds` or `passes`, including references preceding the assignments. `ROUNDS` controls debates; `GRILL_ROUNDS` controls grills; `CENSUS_PASSES` controls censuses. None controls tournament length, crosscheck steps, survey steps, or probe steps.

For example:

```text
@grok @gemini $debate until $ROUNDS $ROUNDS=5
```

produces `rounds: 5` and instruction `until 5`.

```text
@grok $grill test it for $GRILL_ROUNDS rounds $GRILL_ROUNDS=3
```

produces a direct grill with `rounds: 3` and focus `test it for 3 rounds`, provided `grok` has an eligible plan.

```text
@all $census $CENSUS_PASSES=3
```

with one eligible quoted plan produces `passes: 3` and a staged sticky value of 3.

Assignments are staged data in the AST. Parsing never changes room state. Rejected submissions leave all sticky values unchanged. The router commits updates through the variable-change contract in **Message protocol and bus**. Acceptance and confirmation timing for a contest follow **Contests**.

A contest freezes its applicable resolved round or pass count at admission. A question-first submission carries its resolved round count through answer collection and the pick step. Later assignments affect subsequent contests. Census plan changes between passes follow its script; they do not change the admitted pass count.

### Quote attachments and solution selection

A human conversational submission may carry `quote` metadata under the canonical limit. The router resolves each referenced event and delivers its text with the original author label as part of that user message, following the quote contract in **Message protocol and bus**.

Quoted text is evidence. Mentions, commands, variables, and assignments inside it remain literal. Quotes do not add recipients or independently select debate solutions.

`replyTo` is valid only on direct `$debate`, with at most two IDs. It selects original solutions rather than quoting arbitrary content. Explicit selections must satisfy the source-selection contract in **Contests**. A supplied invalid selection is rejected rather than silently replaced. Question-first contests use their collected answers and reject supplied solution selections.

For a direct grill, a selected plan is supplied through `quote`, not `replyTo`. Exactly one eligible quoted reply from the leader selects that plan and becomes `planSelection`; other quoted material remains evidence under the quote contract. Supplied quotes without an eligible leader reply, or with multiple eligible leader replies, are rejected as an ambiguous or invalid plan selection. Without quotes, `planSelection` is absent and admission resolves the leader’s most recent eligible reply to `me`. Plan eligibility and authoritative source freezing belong to **Contests**.

Census and crosscheck accept a selected member reply or a selected grill plan version through `quote`. Exactly one eligible plan source must be present among supplied quotes. Supplied quotes with no eligible plan source or multiple eligible plan sources are rejected; the parser does not fall back to a pasted plan or latest reply. Other quoted material remains evidence under the quote contract and does not become another plan. The engine’s per-step delivery rules control which evidence reaches each actor.

A quoted grill source identifies a stored plan version, not the grill’s report wrapper or question-and-answer appendix. Selecting the latest plan in the TUI supplies that version’s event ID. Source eligibility and freezing belong to **Contests**.

Quotes do not select survey answer batches or replace the required probe brief. Quoted, expanded, and resolved source text remains literal throughout all four modes.

Outside a debate, quoting a verdict is the supported judge-follow-up path. A TUI reply action on a verdict therefore supplies its ID through `quote`, not `replyTo`.

Slash commands, assignment-only operations, and pick messages accept neither quote attachments nor solution selections.

### Standalone judge requests

`@judge` is legal in exactly these contexts:

1. Inside `$debate`, where it requests judging after the debate; question-first debates request judging by default.
2. As the sole recipient of a message carrying at least one quote, opening a new case.
3. As the sole recipient of a message quoting a verdict, reopening that verdict’s case.
4. Inside `$tournament`, where judging is already implied and the token does not add an entrant.
5. Inside `$census`, `$crosscheck`, `$survey`, or `$probe`, where the script’s judge calls are already implied and the token does not add a participant or request an additional ruling.

`@judge` with `$grill` returns:

```text
@judge cannot be used with $grill.
```

Every other use returns:

```text
Quote at least one message for @judge to rule on.
```

A new standalone case supplies exactly the quoted messages and the user’s question. Its ruling is free-form text, not the scored debate verdict schema.

A verdict quote invokes the sealed-case follow-up contract in **Contests**. That contract supplies the case material, its verdict, and its prior follow-ups. The parser does not read room-member history, allocate a case ID, or assemble judge context.

### Debate extraction

The direct `$debate` form requires exactly two distinct, explicit configured member mentions and no `@all`. `@all`, including an exclusion form, or three or more distinct explicit member mentions selects the question-first form below. Repeated mentions do not create additional debaters or candidates.

In the direct form, an active `@judge` token sets `judgeRequested: true`. Without it, `judgeRequested` is false. Question-first debates set it to true by default. Escaped mentions, code contents, and quoted text cannot request judging.

To extract the instruction:

1. Remove `$debate`, active debater mentions, and active assignment tokens.
2. Retain active `@judge` tokens at their original positions as instruction text.
3. Expand variables using the submission’s effective room-variable snapshot.
4. Decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace.

The canonical direct command, with the default room value:

```text
@grok @gemini $debate your solutions until $ROUNDS at which time @judge decides which is better
```

produces:

```text
your solutions until 10 at which time @judge decides which is better
```

Numeric prose such as `until 7` does not set the round count. An empty direct debate instruction is legal.

The parser passes the instruction and optional solution selections to **Contests**:

- Existing solutions use that section’s latest-eligible-reply and shared-topic rules.
- Explicit selections override recency and retain authorship, eligibility, and shared-topic validation.
- A fresh topic is valid. If either debater lacks an eligible prior solution on the topic, the orchestrator runs opening round 0 for both. Missing inferred solutions are not a parser rejection.
- Round 0 does not count toward `ROUNDS`. Its positions become the solutions through the `debate.setup` contract.

A direct debate submission creates no ordinary reply work. Admission, `debate.preflight`, confirmation, waiting for busy debaters, and source freezing belong to **Contests**. Every preflight states the resolved round count, number of model calls, and estimated cost.

Only one contest may be active in the room. A second contest submission receives the exact occupied-room error below. A pending pick counts as an active contest. A busy member alone does not cause that rejection; the admitted debate may wait in `WAITING_FOR_DEBATERS`.

A direct debate without `@judge` ends in `COMPLETE_UNJUDGED`. `/judge` can request judging afterward. Judge-model conflict handling belongs to **Contests** and does not change the AST.

### Tournament extraction

`@all $tournament <prompt>` and `@a @b @c $tournament <prompt>` start a tournament. `@all -nick` exclusions work. At least three distinct configured entrants and a non-whitespace prompt are required.

Remove `$tournament`, active entrant selectors and exclusions, active assignments, and any active `@judge` token to extract the prompt. Expand variables, decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace.

Judging is implied. There is no pick step and no `ROUNDS` setting for tournament length. With `k` entrants, the tournament runs `k-1` elimination rounds under **Contests**. Blind judging keeps the configured judge even when its model is an entrant.

The parser emits the resolved entrants and prompt. The contest engine owns entry generation, rankings, eliminations, retries, visibility, and trophies. Its preflight shows `k + (k-1) + ... + 2` entry calls plus `k-1` judge calls and the estimated cost.

### Grill extraction

`@leader $grill [focus]` starts a direct grill of the leader’s most recent eligible reply to `me`, or the eligible plan selected through quote metadata. Every other configured room member becomes a griller. At least one griller is required.

`@leader @griller... $grill` with no free text is also direct. The first distinct member mention is the leader and the remaining distinct mentions restrict the grillers. This form preserves explicit griller selection.

Two or more distinct member mentions with free text select the question-first form. `@all`, including exclusions, always selects the question-first form. The complete decision table below controls classification.

A single-member direct form without an eligible prior or selected plan is rejected with:

```text
@{nick} has no plan to grill yet. Ask it something first.
```

For a direct grill, remove `$grill`, active participant mentions, and active assignments to extract the focus. Expand variables, decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace. An empty direct focus is legal. Active `@judge` is prohibited.

The parser emits the leader, grillers, focus, selected plan if supplied, and effective `GRILL_ROUNDS`. Admission freezes the source under **Contests**. The grill has no judge and no winner. Its shared-floor visibility, sequential turns, rotation, `PASS` handling, plan rebuilding, and final appendix belong to that section.

The preflight states the resolved round count, `rounds × (grillers × 2 + 1)` model calls, and estimated cost.

### Census and crosscheck plan arguments

Both modes accept `@all`, `@all -nick...`, or an explicit participant list. A sole distinct explicit member mention is a source shortcut: all configured room members participate, and that member’s latest eligible reply to `me` supplies the plan when no explicit plan source is supplied. The named member is not a leader. At least three participants must remain.

For an explicit list of two or more members, the list supplies only the participants. It never chooses the first member’s latest reply as the plan. An `@all` selector also has no implicit latest-reply source.

Remove the mode command, active participant selectors and exclusions, active assignments, and any active `@judge` token before extracting arguments. Expand variables from the effective snapshot, decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace.

For census, source selection is:

1. With supplied quotes, select exactly one eligible quoted plan source. Non-whitespace extracted body text is rejected because it would also supply a pasted plan.
2. Without quotes, non-whitespace extracted body text is the pasted plan.
3. Without quotes or body text, the sole-member shortcut emits `LatestReply`.
4. Otherwise reject the missing plan.

For crosscheck, source selection is:

1. With supplied quotes, select exactly one eligible quoted plan source. The extracted body is the optional goal-and-constraints brief.
2. Without quotes, a sole-member shortcut emits `LatestReply`. The extracted body is the optional goal-and-constraints brief.
3. Otherwise the extracted body supplies a pasted plan. The first unprotected `--` separator line divides the pasted plan from an optional goal-and-constraints brief. Without a separator, the entire extracted body is the plan.
4. Reject an empty pasted plan. Normalize an empty brief to absent.

A sole-member crosscheck with a pasted plan uses a quoted source or an `@all` participant selector to avoid confusing the plan with the brief. No model call guesses which part of an unseparated pasted body is the plan.

A supplied invalid quoted source is rejected rather than replaced by another source. Latest-reply eligibility and authoritative plan freezing use supplied record summaries and the source contract in **Contests**.

### Census extraction

`@all $census` with an eligible quoted plan, `@all $census <pasted plan>`, and `@nick $census` are the census entry forms. Exclusions and explicit participant lists follow the plan-argument rules above.

The parser emits `Census` with the resolved participants, `PlanSource`, and effective `CENSUS_PASSES`. A bare number in the plan does not set the pass count. Neither `ROUNDS` nor `GRILL_ROUNDS` changes it.

The engine owns pass scheduling, lens assignment and rotation, blind gap hunts, anonymised ledgers, ratings, additions, estimates, plan replacement between passes, and the quiet stop rule. Its preflight is derived from the script and the admitted pass count.

### Crosscheck extraction

`@all $crosscheck` with an eligible quoted plan and optional free text for goal and constraints starts a crosscheck. Pasted-plan and latest-reply forms follow the plan-argument rules above.

The parser emits `Crosscheck` with the resolved participants, `PlanSource`, and an optional `goalConstraints` string. The brief is free text; the parser does not require labels or judge whether its goal and constraints are complete. If it is absent, the engine adds the goal-and-constraints extraction call to preflight.

The engine owns blind inventory generation, grid construction and trimming, cell assignment, fresh-context auditors, closure challenges, risk acceptance, and the bounded stop rule. The parser does not distribute the plan to inventory writers or add source text to their payloads.

### Survey extraction

`@all $survey <question>` starts a survey with new parallel answers. `@all $survey` reuses the room’s most recent eligible set of parallel answers to one question. Explicit participant lists and `@all` exclusions work. At least one configured participant is required.

Remove `$survey`, active participant selectors and exclusions, active assignments, and any active `@judge` token. Expand variables, decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace.

A non-whitespace body emits `SurveySource::Question { text }`. An empty body emits `LatestParallelAnswers`. Quoted text does not supply the question or select a different answer batch.

For reuse, contextual validation requires a stored parallel-answer set with at least one eligible answer from the selected participants. It never combines unrelated latest replies from different user questions. The engine freezes the source batch, its question, and eligible selected answers at admission; missing or failed members follow its report rules.

The parser does not extract claims, count votes, identify contested claims, or classify the five report bands. Survey has no human pick step and uses no round or pass variable.

### Probe extraction

`@all $probe <brief>` starts a probe. The brief describes the observation, what is ruled out, and what can be inspected. Explicit participant lists and `@all` exclusions work.

Remove `$probe`, active participant selectors and exclusions, active assignments, and any active `@judge` token. Expand variables, decode escapes, preserve protected code, normalize whitespace across removed tokens, and trim surrounding whitespace. The resulting brief must contain non-whitespace text. The parser does not require labelled fields or infer whether the supplied observations are true.

After selector expansion and deduplication, at least four members must remain. An even count is accepted unchanged. An odd count of at least five is accepted with all selected members preserved in the AST; the engine leaves the last selected member sitting out, records that fact in the transcript, and randomly splits the remaining even list into explainers and checkers. “Last” means last in the resolved first-occurrence order, or roster order for `@all`, before the random split.

The parser does not choose roles or checks, make predictions, execute checks, or decide an explanation is true. Probe has no pick step and uses no round or pass variable.

### Question-first contests and picks

A question-first submission emits `QuestionFirst` with the resolved candidate list, extracted question, applicable round count, and `judgeRequested: true` for debate or false for grill.

The following table uses distinct configured member counts after deduplication. Free text means non-whitespace content after removing the command, assignments, selectors, exclusions, and control mentions. An active `@judge` token alone does not supply a question.

| Command | Selector and free text | Result |
|---|---|---|
| `$debate` | Exactly two explicit members, no `@all`, with or without free text | Direct debate; optional `@judge`. |
| `$debate` | Three or more explicit members, with free text | Question-first debate; judging requested by default. |
| `$debate` | `@all` or `@all -nick...`, with free text and at least two candidates | Question-first debate; judging requested by default. |
| `$debate` | Question-first selector without free text | `Add a question after $debate.` |
| `$debate` | `@all` leaves fewer than two candidates | `$debate needs at least two candidates.` |
| `$debate` | Fewer than two explicit members without `@all` | `$debate requires exactly two distinct debaters.` |
| `$grill` | Exactly one explicit member, with or without free text, and an eligible prior or selected plan | Direct grill; all other configured members grill. |
| `$grill` | Exactly one explicit member without an eligible prior or selected plan | `@{nick} has no plan to grill yet. Ask it something first.` |
| `$grill` | Two or more explicit members, no free text | Direct grill; first member leads, remaining members grill. |
| `$grill` | Two or more explicit members, with free text | Question-first grill. |
| `$grill` | `@all` or `@all -nick...`, with free text and at least two candidates | Question-first grill. |
| `$grill` | `@all` or `@all -nick...`, without free text | `Add a question after $grill.` |
| `$grill` | Question-first selector leaves fewer than two candidates | `$grill needs at least two candidates.` |
| `$grill` | No member selector | `$grill needs a leader or at least two candidates.` |
| `$tournament` | At least three explicit entrants or an `@all` expansion, with free text | Tournament; judging implied; no pick. |
| `$tournament` | Fewer than three entrants | `$tournament needs at least three entrants.` |
| `$tournament` | At least three entrants without free text | `Add a prompt after $tournament.` |

Question-first extraction removes the command, active candidate selectors and exclusions, and active assignments, then applies the same variable expansion, escape handling, protected-text preservation, whitespace normalization, and trimming as direct extraction. A debate’s active `@judge` tokens remain instruction text once a real question is present.

The candidates answer as ordinary, mutually invisible replies during `COLLECTING_ANSWERS`. These calls count toward daily spend. When every candidate has answered, errored, or been stopped, the engine determines eligible answers. Fewer than two eligible answers ends either kind as `FAILED` with:

```text
Not enough answers to start. {n} of {m} members answered.
```

Otherwise the contest enters `AWAITING_PICK`. The debate banner is `CHOOSE YOUR DEBATE BROS`; the grill banner is `CHOOSE WHO GETS GRILLED`. The TUI owns their rendering and offers eligible nicks first in autocomplete.

While a pick is pending, mentions-only input is checked before ordinary-message body validation. Deduplicate mentions in first-occurrence order. A debate pick requires exactly two distinct eligible nicks; a grill pick requires exactly one. Check the required count before eligibility, then report the leftmost ineligible mention.

The parser emits `ContestPick` bound to the pending contest. It is a control request, creates no ordinary model message, and stages no variable updates. The router selects the stored collected answer for each chosen nick, even if a newer ordinary reply appeared while the pick was pending. A debate uses the original question as its topic and the two collected answers as its solutions. A grill uses the chosen answer as the plan and every other eligible answering member as a griller.

Wrong picks leave the pending pick intact. The exact errors are:

```text
Pick exactly two of: {eligible}.
Pick exactly one of: {eligible}.
@{nick} did not answer, so it cannot be picked.
```

`{eligible}` is the pending eligible list in roster order, formatted as comma-separated `@nick` mentions, for example `@grok, @gemini, @claude`.

Other input is handled normally while the pick remains pending. For example, `@claude explain your third point` is an ordinary message. Without a pending pick, mentions-only input follows ordinary-message validation and has no special meaning.

Preflight and cost confirmation happen after a valid pick. The later estimate counts only the ensuing contest calls; it excludes the ordinary answers already collected and billed. A pending pick does not expire and is restored after restart under **Contests**.

`/stop` during a pending pick cancels the contest as `STOPPED` with reason `pick_cancelled`; the collected answers remain ordinary replies in the transcript and private histories. Escape follows the TUI contract: close the topmost transient element first; with none open, request global stop and require a confirming second Escape within 2 s during a contest.

### Slash commands

This is the complete v1 slash command set. Commands are case-sensitive and consume the entire submission.

| Form | Parser result and behavior |
|---|---|
| `/set NAME value` | Accept `NAME=ROUNDS`, `NAME=GRILL_ROUNDS`, or `NAME=CENSUS_PASSES`; emit the equivalent variable update. |
| `/stop` | Emit a room-wide stop action under the cancellation contracts in **Agents and OpenRouter** and **Contests**. |
| `/stop @nick` | Emit a stop action for one configured member. |
| `/judge [debateId]` | Request judging of an eligible stopped or finished debate’s existing transcript under **Contests**. |
| `/regen [messageId]` | Request reply regeneration under **Message protocol and bus** and **Room API and CLI**. |
| `/export room\|debate\|grill\|tournament\|census\|crosscheck\|survey\|probe [id]` | Request a markdown export under **Room API and CLI**. The scope is required. |
| `/continue ["<plan>"]` | Continue the active census at `WAITING_FOR_PLAN`; absent plan keeps the current plan, while a quoted string supplies replacement text. A single eligible quote attachment may supply the replacement instead. |
| `/accept <cellId…> -- <reason>` | Explicitly accept the named residual risks on the current `COMPLETE` crosscheck, bound to its plan, grid and state version. |
| `/clear` | Wipe the persistent room under **Message protocol and bus**. This is not a display-only action. |
| `/who` | Show the configured roster, resolved model identities, presence, debate win/loss records, and tournament trophy counts through the shared room-state contract. |
| `/help` | Show this grammar, supported commands, and examples without invoking a model. |

The exact usage strings are:

```text
/set ROUNDS <1-20>
/set GRILL_ROUNDS <1-10>
/set CENSUS_PASSES <1-5>
/stop [@nick]
/judge [debateId]
/regen [messageId]
/continue ["<plan>"]
/accept <cellId…> -- <reason>
/export room|debate|grill|tournament|census|crosscheck|survey|probe [id]
/clear
/who
/help
```

`/continue` accepts zero arguments or one double-quoted JSON string, decoded once without mention, variable or assignment expansion. The replacement must contain non-whitespace text. At most one eligible plan quote attachment may be supplied instead of text; both sources are rejected. It resolves the active census ID and expected state version from `ParseContext`. Continuing reacquires released holds and drains earlier ordinary work before the next pass.

`/accept` requires one or more unique UUIDv4 `CellId` values, a literal `--` separator, and a non-whitespace reason consuming the remainder unchanged. It targets the latest retained `COMPLETE` crosscheck in the supplied summaries; the front end freezes its contest ID, plan/grid event IDs and expected state version in `Request::CrosscheckAccept`. The router rejects stale or unavailable targets. These controls are parsed in M1, with execution enabled only when the corresponding mode lands in M5.

Argument counts are checked before argument values. For an incorrect `/set` argument count, the usage is `/set ROUNDS <1-20> | /set GRILL_ROUNDS <1-10> | /set CENSUS_PASSES <1-5>`. Optional IDs use the identifier validation defined by their owning contracts. The parser does not resolve optional targets from prose.

Stopping does not require an active contest. Stop scope, queued-work cancellation, and terminal-transition arbitration follow the owning cancellation contracts. A stopped debate preserves its transcript and skips automatic judging.

`/judge` targets debates only; it does not judge grills, restart tournament ranking, or start an additional ruling for census, crosscheck, survey, or probe. Judge-only admission, confirmation, cancellation, and restoration of a debate’s prior status belong to **Contests**.

`/export` uses `RoomHandle::export(scope, id: Option<ContestId>) -> Result<String, RoomError>` under **Room API and CLI**. Markdown export and banner text live in `squad-core`; the parser does not write files.

Models resolve at startup as defined by **Agents and OpenRouter**. `SQUAD_MODEL_<NICK>` values supply model slugs; a concrete slug selects a fixed version, and a `~...-latest` alias tracks releases under the configuration contract in **Foundations**. Every completion uses the concrete slug resolved at startup.

`$review`, `$reveal`, `$vote`, and `$summarize` remain reserved for later versions.

### Validation and exact errors

Return one error. Validate in this order:

1. Input emptiness, byte size, and lexical validity.
2. Submission form and command support.
3. Command arguments, assignment values, and attachment shape.
4. Recipient legality and required content, including contextual pick cardinality and eligibility.
5. Authoritative reference, case, source, and admission checks.

Within a category, report the leftmost text violation. For metadata checks, validate quotes before solution selections and preserve attachment order. Within a pick candidate, count precedes eligibility. Outside a contest command, the standalone `@judge` legality rule takes precedence over ordinary recipient-combination errors. Tournament judging and the prohibition of `@judge` in grills follow their command-specific rules.

For the four new modes, selector legality and participant count precede missing body or source checks. Census and crosscheck source eligibility, latest-reply availability, and survey answer-batch availability are contextual checks in category 5. The census quoted-and-pasted conflict is an argument check in category 3. An accepted odd probe count is not an error.

Placeholders are substituted without additional quoting. This table owns each exact string and its mapping to Protocol’s existing `ErrorCode` enum. `@me`, question-first minimums and census both-source conflicts use `INVALID_COMMAND`; ambiguous selected grill/census sources use their named source-missing code. Busy codes depend on the requested mode, not the currently active mode.

| Rule | Code | Exact user-facing error |
|---|---|---|
| Empty or whitespace-only input | `INVALID_COMMAND` | `Enter a message or command.` |
| Text exceeds the canonical byte limit | `MESSAGE_TOO_LARGE` | `Keep the submission within 32,768 UTF-8 bytes.` |
| Unterminated inline code | `INVALID_COMMAND` | `Close the inline code with a backtick.` |
| Unterminated fence | `INVALID_COMMAND` | `Close the code block with three backticks.` |
| Invalid active sigil token | `INVALID_COMMAND` | `Invalid token: {token}. Escape the sigil to use it literally.` |
| Unknown `$verb` | `INVALID_COMMAND` | `Unknown command: ${verb}.` |
| Reserved future command | `COMMAND_NOT_IMPLEMENTED` | `Command ${verb} is reserved and unavailable in v1.` |
| More than one `$verb` | `INVALID_COMMAND` | `Use only one $ command per submission.` |
| Unknown slash command | `INVALID_COMMAND` | `Unknown slash command: /{name}.` |
| Wrong slash argument count | `INVALID_COMMAND` | `Usage: {usage}` |
| Unknown variable, including `/set` | `INVALID_COMMAND` | `Unknown variable: {name}.` |
| Invalid `ROUNDS` value | `INVALID_COMMAND` | `ROUNDS must be an integer from 1 to 20.` |
| Invalid slash identifier | `INVALID_COMMAND` | `Invalid {argument}: {value}.` |
| Too many quotes | `INVALID_REFERENCE` | `Attach no more than 12 quoted messages.` |
| Quote reference cannot be resolved | `INVALID_REFERENCE` | `Quoted message {id} is unavailable.` |
| Quotes attached to a control operation other than `/continue` | `INVALID_REFERENCE` | `Quotes require a conversational message.` |
| `replyTo` outside `$debate` | `INVALID_REFERENCE` | `Solution selection is only allowed with $debate.` |
| Too many solution selections | `INVALID_REFERENCE` | `Select no more than two solutions for $debate.` |
| Unknown or incorrectly cased member | `UNKNOWN_RECIPIENT` | `Unknown recipient: @{nick}.` |
| `@me` as recipient | `INVALID_COMMAND` | `@me cannot be a recipient.` |
| Illegal standalone `@judge` use | `JUDGE_QUOTE_REQUIRED` | `Quote at least one message for @judge to rule on.` |
| `@all` mixed with member mentions | `INVALID_COMMAND` | `Use @all by itself or name individual recipients.` |
| Exclusion names an unconfigured member | `UNKNOWN_RECIPIENT` | `Unknown recipient: @{nick}.` |
| Exclusions remove every member | `INVALID_COMMAND` | `@all exclusions must leave at least one recipient.` |
| Direct debate lacks exactly two distinct members | `INVALID_COMMAND` | `$debate requires exactly two distinct debaters.` |
| Ordinary message lacks recipients | `RECIPIENT_REQUIRED` | `Add at least one recipient: @nick or @all.` |
| Ordinary message has no non-whitespace body | `INVALID_COMMAND` | `Enter message text after the recipients.` |
| Standalone judge request has no question text | `INVALID_COMMAND` | `Enter message text after the recipients.` |
| Selected solution is missing, belongs to another author, or is not an eligible reply to `me` | `DEBATE_SOURCE_MISSING` | `Selected solution is not a reply to me from @{nick}.` |
| Selected solutions answer different messages | `DEBATE_TOPIC_MISMATCH` | `Debaters' solutions must answer the same user message.` |
| Another contest is active | `DEBATE_BUSY for a debate request; otherwise CONTEST_BUSY` | `A {kind} is already running. Stop it before starting another.` |
| Invalid `GRILL_ROUNDS` value | `INVALID_COMMAND` | `GRILL_ROUNDS must be an integer from 1 to 10.` |
| Tournament has fewer than three entrants | `TOURNAMENT_ENTRANTS_REQUIRED` | `$tournament needs at least three entrants.` |
| Tournament has no prompt | `INVALID_COMMAND` | `Add a prompt after $tournament.` |
| Direct grill has no eligible prior or selected leader plan | `GRILL_SOURCE_MISSING` | `@{nick} has no plan to grill yet. Ask it something first.` |
| Direct grill has no griller | `GRILL_PARTICIPANTS_REQUIRED` | `$grill needs at least one griller.` |
| Grill has no member selector | `GRILL_PARTICIPANTS_REQUIRED` | `$grill needs a leader or at least two candidates.` |
| Quoted direct-grill selection lacks exactly one eligible leader reply | `GRILL_SOURCE_MISSING` | `Select exactly one reply from @{nick} as the plan to grill.` |
| Active `@judge` in a grill | `INVALID_COMMAND` | `@judge cannot be used with $grill.` |
| Question-first debate has fewer than two candidates | `INVALID_COMMAND` | `$debate needs at least two candidates.` |
| Question-first grill has fewer than two candidates | `INVALID_COMMAND` | `$grill needs at least two candidates.` |
| Question-first debate has no question | `INVALID_COMMAND` | `Add a question after $debate.` |
| Question-first grill has no question | `INVALID_COMMAND` | `Add a question after $grill.` |
| Question-first contest has supplied solution selections | `INVALID_REFERENCE` | `Question-first contests use their collected answers; do not attach solution selections.` |
| Pending debate pick has the wrong distinct-member count | `CONTEST_PICK_INVALID` | `Pick exactly two of: {eligible}.` |
| Pending grill pick has the wrong distinct-member count | `CONTEST_PICK_INVALID` | `Pick exactly one of: {eligible}.` |
| Pick names a member without an eligible collected answer | `CONTEST_PICK_INVALID` | `@{nick} did not answer, so it cannot be picked.` |
| Too few eligible collected answers to proceed | `CONTEST_NOT_ENOUGH_ANSWERS` | `Not enough answers to start. {n} of {m} members answered.` |
| Daily max reached or contest estimate exceeds the remaining daily amount | `DAILY_MAX_REACHED` | `Daily max of ${max} reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.` |
| Invalid `CENSUS_PASSES` value | `INVALID_COMMAND` | `CENSUS_PASSES must be an integer from 1 to 5.` |
| Census has fewer than three resolved participants | `CENSUS_PARTICIPANTS_REQUIRED` | `$census needs at least three participants.` |
| Crosscheck has fewer than three resolved participants | `CROSSCHECK_PARTICIPANTS_REQUIRED` | `$crosscheck needs at least three participants.` |
| Survey has no member selector | `SURVEY_PARTICIPANTS_REQUIRED` | `Add at least one recipient: @nick or @all.` |
| Probe has fewer than four selected members | `PROBE_PARTICIPANTS_REQUIRED` | `$probe needs at least four members; an odd selection leaves the last member out.` |
| Census has no quoted, pasted, or latest-reply plan source | `CENSUS_SOURCE_MISSING` | `Add a plan for $census: quote one plan, paste it, or use @nick for that member's latest reply.` |
| Crosscheck has no quoted, pasted, or latest-reply plan source | `CROSSCHECK_SOURCE_MISSING` | `Add a plan for $crosscheck: quote one plan, paste it, or use @nick for that member's latest reply.` |
| Census supplies both quotes and non-whitespace pasted-plan text | `INVALID_COMMAND` | `Supply the $census plan by quote or pasted text, not both.` |
| Census quotes lack exactly one eligible plan source | `CENSUS_SOURCE_MISSING` | `Select exactly one member reply or grill plan version for $census.` |
| Crosscheck quotes lack exactly one eligible plan source | `CROSSCHECK_SOURCE_MISSING` | `Select exactly one member reply or grill plan version for $crosscheck.` |
| Census latest-reply shortcut has no eligible reply | `CENSUS_SOURCE_MISSING` | `@{nick} has no plan for $census yet. Ask it something first.` |
| Crosscheck latest-reply shortcut has no eligible reply | `CROSSCHECK_SOURCE_MISSING` | `@{nick} has no plan for $crosscheck yet. Ask it something first.` |
| Survey reuse has no eligible parallel-answer set for the selected members | `SURVEY_SOURCE_MISSING` | `No parallel answers are available for $survey. Add a question after $survey.` |
| Probe has no non-whitespace brief | `PROBE_INPUT_REQUIRED` | `Add an observation, what is ruled out, and what can be inspected after $probe.` |
| Invalid `/continue` quoted text, empty replacement, or both attachment and text | `INVALID_COMMAND` | `Usage: /continue ["<plan>"]` |
| `/continue` has no waiting census or no remaining pass | `INVALID_STATE_TRANSITION` | `No census is waiting for a plan.` |
| `/continue` quote cannot select one eligible plan | `CENSUS_SOURCE_MISSING` | `Select exactly one member reply or grill plan version for /continue.` |
| `/accept` has missing cells, duplicate cells, no separator, or empty reason | `INVALID_COMMAND` | `Usage: /accept <cellId…> -- <reason>` |
| `/accept` has no retained COMPLETE crosscheck | `CONTEST_NOT_FOUND` | `No complete crosscheck is available for acceptance.` |
| `/accept` references stale plan, grid, state or cell | `CROSSCHECK_ACCEPTANCE_STALE` | `Crosscheck acceptance is stale. Refresh the coverage ledger.` |

Invalid `/accept` cell syntax uses `INVALID_COMMAND` with `Invalid cellId: {value}.`; an unknown but well-formed cell uses `CROSSCHECK_ACCEPTANCE_STALE`.

For invalid slash identifiers, `{argument}` is exactly `debateId`, `messageId`, `cellId`, or `id`, as applicable.

For the occupied-room error, `{kind}` is the active contest’s lowercase kind: `debate`, `tournament`, `grill`, `census`, `crosscheck`, `survey`, or `probe`. For the daily-max error, `{max}` is formatted with two decimal places; the default produces `Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.`

Missing recipients use `RECIPIENT_REQUIRED`; unknown recipients use `UNKNOWN_RECIPIENT`. Reserved future commands use `COMMAND_NOT_IMPLEMENTED`. Shared-topic failures use `DEBATE_TOPIC_MISMATCH`; daily-max failures use `DAILY_MAX_REACHED`. Occupied-room, contest-admission, and pick failures use the canonical code assignments in **Message protocol and bus**.

The four modes’ participant-count, plan-argument, plan-source, survey-source, and probe-brief failures use the canonical code assignments in **Message protocol and bus**. This section owns their exact strings and precedence without adding a second error-code enum.

Absent inferred solutions trigger the opening-round path rather than `DEBATE_SOURCE_MISSING`. Invalid explicit solution selections remain errors. **Message protocol and bus** owns all code assignments and the complete enum.

Runtime failures, including provider availability, context capacity, daily spend limits, persistence, and execution-time target eligibility, use the owning execution contracts. They are not lexer failures. The insufficient-answers string belongs to the contest engine’s terminal failure path, not initial parsing.

### Input examples

Assume the default roster, `ROUNDS=10`, `GRILL_ROUNDS=2`, and `CENSUS_PASSES=2` unless a condition states otherwise. Quote and solution IDs in the metadata column are symbolic references to stored records, not text the human types. Examples without listed metadata carry no attachments.

| Typed input | Submission metadata or condition | Expected result |
|---|---|---|
| `@claude Hello` | None | Message to `claude`; body `Hello`. |
| `@grok @gemini Compare this` | None | Message to both members. |
| `@all Suggest a use case` | Default roster | Message to all configured members. |
| `@all -grok -muse Suggest a use case` | Default roster | Explicit recipients `claude`, `fable`, `openai`, `gemini`. |
| `@all -grok -grok Hello` | Default roster | Exclude `grok` once. |
| `@all -unknown Hello` | None | `Unknown recipient: @unknown.` |
| `@all -grok Hello` | Roster contains only `grok` | `@all exclusions must leave at least one recipient.` |
| `@all Explain -grok` | None | All members receive body `Explain -grok`. |
| `@claude Explain -grok` | None | Only `claude` receives body `Explain -grok`. |
| `Hello` | None | `Add at least one recipient: @nick or @all.` |
| `@claude` | None | `Enter message text after the recipients.` |
| `@Claude Hello` | None | `Unknown recipient: @Claude.` |
| `@unknown Hello` | None | `Unknown recipient: @unknown.` |
| `@me Hello` | None | `@me cannot be a recipient.` |
| `@all @grok Hello` | None | `Use @all by itself or name individual recipients.` |
| `@grok @grok Hello` | None | One recipient: `grok`. |
| `@claude Email person@example.com` | None | Email retained literally. |
| `@claude Explain \@grok` | None | Body `Explain @grok`; only `claude` receives it. |
| `@claude Explain \$ROUNDS` | None | Body `Explain $ROUNDS`. |
| `@claude Budget $20` | None | Body `Budget $20`. |
| ``@claude Explain `@grok $ROUNDS` `` | None | Code retained; no additional recipient or expansion. |
| `@claude Use $ROUNDS rounds` | None | Body `Use 10 rounds`. |
| `$ROUNDS=5` | None | Assignment-only control operation. |
| `/set ROUNDS 5` | None | Equivalent variable update. |
| `$ROUNDS=5 @grok Hello` | None | Message to `grok`, body `Hello`, staged sticky value 5. |
| `@grok Hello $ROUNDS=5` | None | Same message and staged update. |
| `@grok Use $ROUNDS rounds $ROUNDS=5` | None | Body `Use 5 rounds`; staged sticky value 5. |
| `$ROUNDS=4 $ROUNDS=5` | None | Assignment-only operation setting 5. |
| `$ROUNDS=0` | None | `ROUNDS must be an integer from 1 to 20.` |
| `$ROUNDS=3.5` | None | Same integer error. |
| `$ROUNDS=` | None | Same integer error. |
| `@grok Hello $ROUNDS=0` | Current value 3 | Reject; current value remains 3. |
| `$OTHER=2` | None | `Unknown variable: OTHER.` |
| `@grok Use $OTHER` | None | `Unknown variable: OTHER.` |
| `@grok $Rounds` | None | `Invalid token: $Rounds. Escape the sigil to use it literally.` |
| `@grok $vote Yes` | None | `Command $vote is reserved and unavailable in v1.` |
| `@grok $review This` | None | `Command $review is reserved and unavailable in v1.` |
| `@grok $dance` | None | `Unknown command: $dance.` |
| `@grok Critique this` | `quote: [q1]`; q1 authored by `gemini` | `grok` receives the question and author-labelled quote; no other member receives it. |
| `@grok Critique this` | `replyTo: [q1]` | `Solution selection is only allowed with $debate.` |
| `@judge Decide` | No quotes | `Quote at least one message for @judge to rule on.` |
| `@judge Which is best?` | `quote: [q1, q2]`; neither is a verdict | Standalone judge request opening a fresh case; free-form ruling. |
| `@judge Explain your decision` | `quote: [v1]`; v1 is a verdict | Judge follow-up on v1’s sealed case. |
| `@judge @grok Decide` | `quote: [q1]` | `Quote at least one message for @judge to rule on.` |
| `@grok @gemini $debate Defend your solutions` | Compatible prior solutions | Ten-round debate; `judgeRequested: false`. |
| `@grok @gemini $debate until $ROUNDS @judge decides` | Compatible prior solutions | Instruction `until 10 @judge decides`; `judgeRequested: true`. |
| `@grok @claude $debate tabs vs spaces` | No prior solutions | Opening round 0 for both, then the configured rounds; no automatic judge. |
| `@grok @claude $debate tabs vs spaces @judge` | Only one has a prior solution on the topic | Opening round 0 for both, followed by the configured rounds and judging. |
| `@grok @gemini $debate $ROUNDS=5 until $ROUNDS` | Compatible prior solutions | Five-round debate; instruction `until 5`; staged sticky value 5. |
| `@grok @gemini $debate until 7` | Compatible prior solutions | Ten rounds; numeric prose does not assign `ROUNDS`. |
| `@grok @gemini $debate Compare these` | `replyTo: [g1, g2]`; valid shared-topic replies from the two debaters | Explicit solution selections override recency. |
| `@grok @gemini $debate Compare these` | Explicit selections answer different messages | `Debaters' solutions must answer the same user message.` |
| `@grok $debate` | None | `$debate requires exactly two distinct debaters.` |
| `@grok @gemini @muse $debate` | None | `Add a question after $debate.` |
| `@all $debate` | None | `Add a question after $debate.` |
| `@all -muse $debate` | None | `Add a question after $debate.` |
| `@grok @gemini $debate $debate` | None | `Use only one $ command per submission.` |
| `@grok @gemini $debate Continue` | Debate d1 is active | `A debate is already running. Stop it before starting another.` |
| `/help` | None | Slash action `help`. |
| `/who` | None | Slash action `who`. |
| `/clear` | None | Persistent-room clear action. |
| `/stop` | None | Room-wide stop action; no recipient or active contest is required for parsing. |
| `/stop @grok` | None | Targeted stop action for `grok`. |
| `/stop now` | None | `Usage: /stop [@nick]` |
| `/judge` | None | Judge action with target omitted. |
| `/judge d1` | d1 represents a valid debate identifier | Judge action targeting the existing debate transcript. |
| `/regen` | None | Regeneration action with target omitted. |
| `/regen m1` | m1 represents a valid message identifier | Regeneration action targeting m1. |
| `/export room` | None | Markdown room-export action. |
| `/export debate d1` | d1 represents a valid debate identifier | Markdown debate-export action. |
| `/export` | None | `Usage: /export room\|debate\|grill\|tournament\|census\|crosscheck\|survey\|probe [id]` |
| `/unknown` | None | `Unknown slash command: /unknown.` |
| `$GRILL_ROUNDS=3` | None | Assignment-only operation setting `GRILL_ROUNDS` to 3. |
| `/set GRILL_ROUNDS 3` | None | Equivalent variable update. |
| `$GRILL_ROUNDS=0` | None | `GRILL_ROUNDS must be an integer from 1 to 10.` |
| `$GRILL_ROUNDS=11` | None | Same integer error. |
| `$ROUNDS=5 $GRILL_ROUNDS=3` | None | One assignment control operation staging both independent updates. |
| `$ROUNDS=5 $GRILL_ROUNDS=0` | Stored values 10 and 2 | Reject; both stored values remain unchanged. |
| `@claude Use $ROUNDS and $GRILL_ROUNDS $GRILL_ROUNDS=3 $ROUNDS=5` | None | Body `Use 5 and 3`; stages both updates. |
| `@all $tournament Design a useful CLI` | Default roster | Tournament with all six entrants; judging implied; no pick. |
| `@all -muse $tournament Design a useful CLI` | Default roster | Tournament with the five remaining entrants. |
| `@grok @gemini @claude $tournament Design a useful CLI @judge` | None | Three entrants; prompt `Design a useful CLI`; judging implied. |
| `@grok @gemini $tournament Design a useful CLI` | None | `$tournament needs at least three entrants.` |
| `@grok @grok @gemini $tournament Design a useful CLI` | None | Same entrant-count error. |
| `@all $tournament` | Default roster | `Add a prompt after $tournament.` |
| `@grok $grill` | `grok` has an eligible prior plan | Direct two-round grill; every other member grills. |
| `@grok $grill Focus on operating cost` | `grok` has an eligible prior plan | Direct grill; focus `Focus on operating cost`. |
| `@grok $grill` | No eligible prior plan or quoted selection | `@grok has no plan to grill yet. Ask it something first.` |
| `@grok $grill` | `quote: [p1]`; p1 is an eligible reply from `grok` | Direct grill using p1 as the selected plan. |
| `@grok $grill` | `quote: [p1, p2]`; both are eligible replies from `grok` | `Select exactly one reply from @grok as the plan to grill.` |
| `@grok @claude @gemini $grill` | `grok` has an eligible prior plan | Direct grill; `grok` leads; only `claude` and `gemini` grill. |
| `@grok @claude $grill Design a backup plan` | None | Question-first grill for two candidates; no leader selected yet. |
| `@grok $grill $GRILL_ROUNDS=3` | `grok` has an eligible prior plan | Direct three-round grill; staged sticky value 3. |
| `@grok $grill @judge` | `grok` has an eligible prior plan | `@judge cannot be used with $grill.` |
| `@all $debate Design a backup plan` | Default roster | Question-first debate; ordinary answers, then a two-member pick; judging requested by default. |
| `@all -muse $debate Design a backup plan` | Default roster | Same flow with five candidates. |
| `@grok @gemini @claude $debate Design a backup plan` | None | Question-first debate with three candidates. |
| `@grok @gemini $debate Design a backup plan` | None | Direct debate; fresh-topic opening round if required; no pick. |
| `@all $grill Design a backup plan` | Default roster | Question-first grill; ordinary answers, then a one-member leader pick. |
| `@all $grill` | None | `Add a question after $grill.` |
| `@all $debate Design a backup plan` | `replyTo: [q1]` | `Question-first contests use their collected answers; do not attach solution selections.` |
| `@grok @gemini` | Debate pick pending; eligible `grok`, `gemini`, `claude` | `ContestPick` for the pending contest, selecting those two collected answers. |
| `@grok` | Same pending debate pick | `Pick exactly two of: @grok, @gemini, @claude.` |
| `@grok @grok` | Same pending debate pick | Same distinct-count error. |
| `@grok @muse` | Same pending debate pick; `muse` did not answer | `@muse did not answer, so it cannot be picked.` |
| `@grok` | Grill pick pending; eligible `grok`, `gemini`, `claude` | `ContestPick` selecting `grok` as leader; the other two become grillers. |
| `@grok @gemini` | Same pending grill pick | `Pick exactly one of: @grok, @gemini, @claude.` |
| `@claude explain your third point` | Either pick pending | Ordinary message; pick stays pending. |
| `@grok @gemini` | No pending pick | `Enter message text after the recipients.` |
| `/stop` | Pick pending | Cancel contest with reason `pick_cancelled`; collected answers remain. |
| `@grok @gemini $debate Continue` | Grill active | `A grill is already running. Stop it before starting another.` |
| `/export grill` | None | Markdown grill-export action with target omitted. |
| `/export tournament t1` | t1 represents a valid tournament identifier | Markdown tournament-export action. |
| `@all $census` | `quote: [p1]`; p1 is an eligible plan reply | Census of p1 with six participants and two passes. |
| `@all $census Launch one region, then expand after review.` | None | Census with the extracted text as its pasted plan. |
| `@grok $census` | `grok` has an eligible latest reply | Census of that reply; all six configured members participate. |
| `@grok @grok $census` | Same condition | Same sole-member source shortcut after deduplication. |
| `@grok $census` | No eligible latest reply | `@grok has no plan for $census yet. Ask it something first.` |
| `@all -muse $census` | `quote: [p1]` | Five census participants; two passes. |
| `@grok @gemini $census Plan text` | None | `$census needs at least three participants.` |
| `@all $census` | No quotes | `Add a plan for $census: quote one plan, paste it, or use @nick for that member's latest reply.` |
| `@all $census Another plan` | `quote: [p1]` | `Supply the $census plan by quote or pasted text, not both.` |
| `@all $census` | `quote: [p1, p2]`; both are eligible plans | `Select exactly one member reply or grill plan version for $census.` |
| `@all $census` | `quote: [gp3]`; gp3 is a stored grill plan version | Census of that frozen plan version. |
| `@all $census $CENSUS_PASSES=5` | `quote: [p1]` | Five passes; stages `CENSUS_PASSES=5`. |
| `$CENSUS_PASSES=1` | None | Assignment-only operation setting the pass count to 1. |
| `/set CENSUS_PASSES 3` | None | Equivalent variable update. |
| `$CENSUS_PASSES=0` | None | `CENSUS_PASSES must be an integer from 1 to 5.` |
| `$CENSUS_PASSES=6` | None | Same integer error. |
| `$CENSUS_PASSES=2.5` | None | Same integer error. |
| `$CENSUS_PASSES=` | None | Same integer error. |
| `@claude Use $CENSUS_PASSES passes $CENSUS_PASSES=3` | None | Body `Use 3 passes`; stages the sticky value 3. |
| `$ROUNDS=5 $GRILL_ROUNDS=3 $CENSUS_PASSES=0` | Stored values 10, 2, and 2 | Reject; all stored values remain unchanged. |
| `@all $crosscheck Keep service available; no extra staff.` | `quote: [p1]` | Crosscheck of p1 with the supplied goal-and-constraints brief. |
| `@all $crosscheck` | `quote: [p1]` | Crosscheck of p1; goal and constraints absent, so preflight includes extraction. |
| `@grok $crosscheck No extra staff.` | `grok` has an eligible latest reply | All members crosscheck that reply; brief `No extra staff.` |
| `@all $crosscheck Launch one region, then expand after review.` | None | Pasted plan; goal and constraints absent. |
| `@all $crosscheck` | No quotes | `Add a plan for $crosscheck: quote one plan, paste it, or use @nick for that member's latest reply.` |
| `@grok @gemini $crosscheck` | `quote: [p1]` | `$crosscheck needs at least three participants.` |
| `@all $crosscheck` | `quote: [p1, p2]`; both are eligible plans | `Select exactly one member reply or grill plan version for $crosscheck.` |
| `@all $survey Which rollout should we use?` | None | Survey with new parallel answers; no human pick. |
| `@all $survey` | Most recent eligible parallel-answer set exists | Survey reusing that set’s selected-member answers and original question. |
| `@all -muse $survey` | Same condition | Reuse restricted to the five selected members. |
| `@all $survey` | No eligible parallel-answer set | `No parallel answers are available for $survey. Add a question after $survey.` |
| `@all $probe Requests stall after deploy; database ruled out; inspect logs and metrics.` | Default roster | Probe with six selected members and the extracted brief. |
| `@all -muse $probe Requests stall; inspect logs.` | Default roster | Five selected members accepted; the last in resolved roster order sits out before the engine splits roles. |
| `@grok @gemini @claude @openai $probe Requests stall; inspect logs.` | None | Four-member probe. |
| `@grok @grok @gemini @claude $probe Requests stall.` | None | `$probe needs at least four members; an odd selection leaves the last member out.` |
| `@all $probe` | Default roster | `Add an observation, what is ruled out, and what can be inspected after $probe.` |
| `@all $census @judge` | `quote: [p1]` | Census; script judging implied; no extra ruling. |
| `@all $survey Which rollout? @judge` | None | Survey question `Which rollout?`; script judging implied. |
| `@all $probe Investigate the outage. $survey` | None | `Use only one $ command per submission.` |
| `@all $census` | `quote: [p1]`; `replyTo: [p1]` | `Solution selection is only allowed with $debate.` |
| `@all $survey New question` | Census active | `A census is already running. Stop it before starting another.` |
| `@all $probe Investigate the outage.` | Crosscheck active | `A crosscheck is already running. Stop it before starting another.` |
| `@all $census` | `quote: [p1]`; survey active | `A survey is already running. Stop it before starting another.` |
| `@all $crosscheck` | `quote: [p1]`; probe active | `A probe is already running. Stop it before starting another.` |
| `@claude Explain \$census` | None | Ordinary message with literal body `Explain $census`. |
| `/export census` | None | Markdown census-export action with target omitted. |
| `/export crosscheck c1` | c1 represents a valid contest identifier | Markdown crosscheck-export action. |
| `/export survey` | None | Markdown survey-export action with target omitted. |
| `/export probe p1` | p1 represents a valid contest identifier | Markdown probe-export action. |

This pasted crosscheck supplies both arguments:

```text
@all $crosscheck
Launch one region, measure support demand, then expand after review.
--
Goal: keep service available.
Constraints: no extra staff and no weekend launch.
```

It produces a pasted plan of `Launch one region, measure support demand, then expand after review.` and a `goalConstraints` brief containing the two labelled lines. The labels are ordinary text. A `--` line inside protected code remains part of the plan.

### Parser verification

Tests use table-driven cases for every input example, validation rule, and precedence rule. `insta` snapshots record complete successful AST values and errors, including exact strings, attachment order, and UTF-8 byte spans. Cases include multibyte UTF-8 characters and verify that span endpoints remain on character boundaries.

A `proptest` parse/format/parse round-trip uses a test-only formatter for generated valid submissions. Both parses receive the same roster, room-variable snapshot, pending-pick snapshot, record summaries, and submission metadata. The property compares semantic AST fields after canonical formatting; separate assertions verify that each parse preserves its own submitted `raw` text exactly.

The parser and contextual validation tests use supplied snapshots and record summaries. They require no model calls or running contest engine.

Contest cases cover every row of the decision table, including two-member direct debates, three-member question-first debates, single-leader direct grills, multiple-member grills with and without free text, `@all` exclusions, required questions, tournament minimums, implicit judging, and prohibited grill judging.

Variable cases cover all three defaults and ranges, independent last-assignment precedence, expansion before assignments, mixed-variable assignment-only operations, and rejection of all staged changes when any assignment is invalid.

Pick cases cover both modes, distinct-member counts, ineligible answers, duplicate mentions, exact eligible-list formatting, pending contest IDs, ordinary messages during a pick, mentions-only input without a pick, and rejection against stale authoritative pick state. Protected, quoted, or expanded text never becomes a pick or an executable command.

The contest engine owns insufficient-answer transitions, restart restoration, cancellation effects, preflight estimates, and daily-spend admission tests. Parser fixtures verify the shared strings and request boundaries without implementing those execution paths.

New-mode cases cover selector expansion and deduplication, sole-member plan shortcuts, census and crosscheck minimums, quoted and pasted plans, grill plan versions, invalid and ambiguous sources, census source conflicts, crosscheck brief separation, missing briefs, survey new-question and answer-reuse forms, and even and odd probe selections.

Crosscheck separator cases verify recognition before variable expansion, preservation inside protected code, first-separator behavior, empty-plan rejection, empty-brief normalization, and no separator interpretation for quoted or latest-reply plans.

Source fixtures verify that survey reuse never combines unrelated questions and that the router revalidates stale plan and answer-batch summaries. Probe fixtures preserve the full selected order so the engine can identify the sit-out member before random role assignment.

Every new mode is tested against each occupied-room kind, pending picks, the single-command rule, direct-debate-only solution selections, protected syntax, implied judging, and all four new export scopes. Shared engine move names remain unknown commands unless separately reserved by the grammar.


Continuation and acceptance fixtures cover kept and replaced plans, quoted attachments, escaped double quotes, both-source rejection, drained holds, COMPLETE-only acceptance, unique cells, required reasons, stale versions and exact codes. Execution fixtures land in M5.P7 and M5.P8.
