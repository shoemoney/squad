//! Canonical compile-time constants. Output ceilings count visible tokens only.

/// The single supported room.
pub const ROOM: &str = "main";

/// Minimum configured roster size.
pub const MIN_MEMBERS: usize = 1;

/// Maximum configured roster size.
pub const MAX_MEMBERS: usize = 12;

/// Maximum ASCII nick length.
pub const NICK_MAX_BYTES: usize = 32;

/// Initial debate rounds; persisted room values take precedence.
pub const DEFAULT_ROUNDS: u8 = 10;

/// Minimum debate rounds.
pub const MIN_ROUNDS: u8 = 1;

/// Maximum debate rounds.
pub const MAX_ROUNDS: u8 = 20;

/// Initial grill rounds.
pub const DEFAULT_GRILL_ROUNDS: u8 = 2;

/// Minimum grill rounds.
pub const MIN_GRILL_ROUNDS: u8 = 1;

/// Maximum grill rounds.
pub const MAX_GRILL_ROUNDS: u8 = 10;

/// Default room prompt word limit.
pub const ROOM_WORDS: u32 = 500;

/// Default debate turn and grill answer word limit.
pub const TURN_WORDS: u32 = 400;

/// Default final debate turn word limit.
pub const FINAL_TURN_WORDS: u32 = 500;

/// Fixed judge prompt word instruction.
pub const JUDGE_WORDS: u32 = 1_000;

/// Grill question word limit.
pub const GRILL_QUESTION_WORDS: u32 = 200;

/// Default rebuilt plan word limit.
pub const PLAN_WORDS: u32 = 1_200;

/// Ordinary reply visible output ceiling.
pub const REPLY_VISIBLE_TOKENS: u32 = 2_048;

/// Debate turn visible output ceiling.
pub const DEBATE_TURN_VISIBLE_TOKENS: u32 = 2_048;

/// Final judge visible output ceiling.
pub const FINAL_JUDGE_VISIBLE_TOKENS: u32 = 4_096;

/// judge.round visible output ceiling.
pub const ROUND_JUDGE_VISIBLE_TOKENS: u32 = 512;

/// Rebuilt plan visible output ceiling.
pub const PLAN_VISIBLE_TOKENS: u32 = 4_096;

/// Low effort reasoning budget; fixed for judge.round.
pub const REASONING_LOW_TOKENS: u32 = 1_024;

/// Medium effort reasoning budget.
pub const REASONING_MEDIUM_TOKENS: u32 = 4_096;

/// High effort reasoning budget.
pub const REASONING_HIGH_TOKENS: u32 = 8_192;

/// Ordinary reply connection timeout in milliseconds.
pub const CONNECT_TIMEOUT_MS: u64 = 15_000;

/// Ordinary reply first token timeout in milliseconds.
pub const FIRST_TOKEN_TIMEOUT_MS: u64 = 90_000;

/// Ordinary reply idle timeout in milliseconds.
pub const IDLE_TIMEOUT_MS: u64 = 30_000;

/// Ordinary reply total timeout in milliseconds.
pub const TOTAL_TIMEOUT_MS: u64 = 300_000;

/// Debate turn deadline in milliseconds.
pub const DEBATE_TURN_DEADLINE_MS: u64 = 180_000;

/// Final judge deadline in milliseconds.
pub const FINAL_JUDGE_DEADLINE_MS: u64 = 300_000;

/// judge.round deadline in milliseconds.
pub const ROUND_JUDGE_DEADLINE_MS: u64 = 120_000;

/// judge.tournament deadline in milliseconds.
pub const TOURNAMENT_JUDGE_DEADLINE_MS: u64 = 120_000;

/// Ordinary reply attempts including one automatic retry.
pub const REPLY_ATTEMPTS: u8 = 2;

/// Debate turn attempts; orchestrator owns the second.
pub const DEBATE_TURN_ATTEMPTS: u8 = 2;

/// Final judge attempts including schema repairs.
pub const FINAL_JUDGE_ATTEMPTS: u8 = 3;

/// judge.round attempts including schema repairs.
pub const ROUND_JUDGE_ATTEMPTS: u8 = 2;

/// Tournament judge attempts including schema repairs.
pub const TOURNAMENT_JUDGE_ATTEMPTS: u8 = 3;

/// Round replays after exhausted tournament judging.
pub const TOURNAMENT_ROUND_REPLAYS: u8 = 1;

/// Per-agent queue size; overflow is AGENT_QUEUE_FULL.
pub const AGENT_QUEUE_DEPTH: usize = 20;

/// Minimum human message size in UTF-8 bytes.
pub const MIN_MESSAGE_BYTES: usize = 1;

/// Maximum human message size in UTF-8 bytes.
pub const MAX_MESSAGE_BYTES: usize = 32_768;

/// Maximum quotes per human message.
pub const MAX_QUOTES: usize = 12;

/// Maximum solution selections; valid only for debate.
pub const MAX_DEBATE_SOLUTIONS: usize = 2;

/// Default single daily spending cap in USD.
pub const DEFAULT_DAILY_MAX_USD: f64 = 20.0;

/// Default debate confirmation threshold in USD.
pub const DEFAULT_DEBATE_CONFIRM_USD: f64 = 2.0;

/// Maximum storage capacity in bytes.
pub const STORAGE_CAPACITY_BYTES: u64 = 1_073_741_824;

/// Preflight plan lifetime in milliseconds.
pub const PREFLIGHT_EXPIRY_MS: u64 = 600_000;

/// Work lease lifetime in milliseconds.
pub const WORK_LEASE_MS: u64 = 60_000;

/// Work lease renewal interval in milliseconds.
pub const WORK_LEASE_RENEWAL_MS: u64 = 20_000;

/// Ingress channel capacity.
pub const INGRESS_CAPACITY: usize = 256;

/// Mailbox capacity; Full means a wake-up is pending.
pub const MAILBOX_CAPACITY: usize = 1;

/// Feed broadcast capacity; lag recovers from durable events.
pub const FEED_CAPACITY: usize = 4_096;

/// Minimum tournament entrants.
pub const MIN_TOURNAMENT_ENTRANTS: usize = 3;
/// Canonical member nick grammar (ASCII only).
pub const NICK_PATTERN: &str = "^[a-z][a-z0-9_]{0,31}$";

/// Nicks reserved for protocol roles and commands.
pub const RESERVED_NICKS: [&str; 5] = ["me", "all", "judge", "system", "squad"];

/// Default member reasoning effort; unsupported implicit defaults are omitted.
pub const DEFAULT_MEMBER_REASONING_EFFORT: crate::config::ReasoningEffort =
    crate::config::ReasoningEffort::Medium;

/// Fixed judge.round effort.
pub const ROUND_JUDGE_REASONING_EFFORT: crate::config::ReasoningEffort =
    crate::config::ReasoningEffort::Low;

/// Default final judge effort.
pub const DEFAULT_FINAL_JUDGE_REASONING_EFFORT: crate::config::ReasoningEffort =
    crate::config::ReasoningEffort::High;

/// Alias startup probe output allowance.
pub const ALIAS_PROBE_TOKENS: u32 = 16;

/// Alias probe allowance on its single retry after HTTP 400.
pub const ALIAS_PROBE_RETRY_TOKENS: u32 = 2_048;

/// Default event history page size.
pub const DEFAULT_HISTORY_PAGE_SIZE: usize = 100;

/// Minimum event history page size.
pub const MIN_HISTORY_PAGE_SIZE: usize = 1;

/// Maximum event history page size.
pub const MAX_HISTORY_PAGE_SIZE: usize = 500;

/// Maximum presentation delta batching interval in milliseconds.
pub const DELTA_BATCH_INTERVAL_MS: u64 = 50;

/// Flush pending presentation deltas at this UTF-8 byte threshold.
pub const DELTA_BATCH_BYTES: usize = 8 * 1_024;
