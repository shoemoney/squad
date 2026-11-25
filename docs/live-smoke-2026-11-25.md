# Live smoke — 2026-11-25

The first real OpenRouter smoke run found and fixed a request-construction bug. After the fix, all six default seats replied, a one-round debate reached a scored verdict, and a single-participant survey completed with explicitly cheaper session overrides.

## Setup and budget

- Base: `origin/main` at `42a8711`; branch: `live-smoke`; host: dev2; no subagents.
- Built `squad` with `CARGO_BUILD_JOBS=8 cargo build --release -p squad-cli` (initial build 64 seconds; fixed build 22.45 seconds).
- Fresh private data directory: `/tmp/squad-live-2026-11-25/data` (0700). Every invocation supplied `--env-file /dev/null` to avoid loading any existing environment file.
- Process configuration: `SQUAD_DAILY_MAX_USD=2.50`, `SQUAD_ROUNDS=1`, `SQUAD_ROOM_WORDS=20`, `SQUAD_TURN_WORDS=30`, `SQUAD_FINAL_TURN_WORDS=30`, `NO_COLOR=1`. Other settings used built-in defaults unless noted.
- Credential was fetched only into the invocation environment using `export OPENROUTER_API_KEY="$(aigate-key openrouter)"`. On dev2 the helper was missing from PATH; a shell function ran `/home/shoemoney/bin/aigate-key` on dev1 through SSH, with its output captured directly into that variable. No key was displayed or written to a file.
- Doctor passed before generation and confirmed the $2.50 effective daily cap. The first doctor attempt lacked the helper and failed without making calls.
- Inspected `squad_store::accounting::admit`: admission checks recorded spend plus reservations plus the new reservation against the daily maximum. Debate preflight reserved $2.405020; the default survey was refused with `DAILY_MAX_REACHED`. The cap was never raised. This is admission enforcement, not a provider-side hard billing limit: actual costs reconcile later, and alias probes currently admit with zero additional reservation.

**Final recorded spend: $0.187083.** Sum of stored provider usage costs: **$0.18708245**; the $0.00000055 difference is per-attempt rounding to microdollars. Final reservations: $0. Seven pre-fix failed attempts retain unknown cost rather than an invented zero; they returned no usage. Observed rejected streaming requests returned HTTP 400 before generation. This total is the measured ledger/usage total, not an independent billing invoice for those seven unknown rows. No further live calls were made.

## Commands and results

Every command below has the prefix `target/release/squad --env-file /dev/null --data-dir /tmp/squad-live-2026-11-25/data`, with the environment above. Wall times include credential retrieval, catalog fetch, startup probes, generation, and shutdown. Costs are ledger increments including startup alias probes, rather than only the requested reply. The accidental overlapping Muse invocation was rejected by the room lock; its cost is zero and its partial ledger snapshot is excluded from attribution.

| Run | Command suffix | Exit | Wall seconds | Recorded USD | Result / usage comparison |
|---|---|---:|---:|---:|---|
| doctor | `doctor` | 1 | 0.008 | $0.000000 | Missing helper/credential; no calls. |
| doctor-ready | `doctor` | 0 | 0.264 | $0.000000 | PASS; max $2.50; no calls. |
| first | `send '@claude say hi in five words'` | 1 | 8.283 | $0.001804 | Alias resolved; reply failed PROVIDER_ERROR; no stream/usage for failed call. Probe costs retained. |
| seat-claude | `send '@claude Reply OK.' --json` | 1 | 6.928 | $0.001874 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. Independent provider capture not yet enabled. |
| seat-fable | `send '@fable Reply OK.' --json` | 1 | 7.281 | $0.001804 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. Independent provider capture not yet enabled. |
| seat-openai | `send '@openai Reply OK.' --json` | 1 | 7.333 | $0.001798 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. |
| seat-gemini | `send '@gemini Reply OK.' --json` | 1 | 6.076 | $0.001680 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. |
| seat-muse | `send '@muse Reply OK.' --json` | 1 | 0.264 | $0.000000 | Harness overlap; UNAVAILABLE room lock; no provider call. |
| seat-grok | `send '@grok Reply OK.' --json` | 1 | 6.478 | $0.001692 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. |
| seat-muse-retry | `send '@muse Reply OK.' --json` | 1 | 6.681 | $0.001764 | PROVIDER_ERROR; HTTP 400 request-field conflict; no failed-call usage; probe usage recorded. |
| fixed-first | `send '@claude say hi in five words' --json` | 0 | 9.382 | $0.006494 | Successful SSE response and final reply; usage matches. |
| fixed-claude | `send '@claude Reply OK.' --json` | 0 | 8.236 | $0.006360 | Successful SSE response and final reply; usage matches. |
| fixed-fable | `send '@fable Reply OK.' --json` | 0 | 8.365 | $0.010768 | Successful SSE response and final reply; usage matches. |
| fixed-openai | `send '@openai Reply OK.' --json` | 0 | 7.415 | $0.003314 | Successful SSE response and final reply; usage matches. |
| fixed-gemini | `send '@gemini Reply OK.' --json` | 0 | 12.099 | $0.004697 | Successful SSE response and final reply; usage matches. |
| fixed-grok | `send '@grok Reply OK.' --json` | 0 | 8.813 | $0.004274 | Successful SSE response and final reply; usage matches. |
| fixed-muse | `send '@muse Reply OK.' --json` | 0 | 7.509 | $0.005130 | Successful SSE response and final reply; usage matches. |
| debate | `send '@openai @grok $debate Choose tea or coffee for a short break. Give one brief reason.' --yes --json` | 1 | 7.475 | $0.001798 | DEBATE_TOPIC_MISMATCH: prior replies came from different questions; no contest calls. |
| debate-seed | `send '@openai @grok Choose tea or coffee for a short break. Give one brief reason.' --json` | 0 | 10.179 | $0.006604 | Two successful replies to one shared question; usage matches. |
| debate-verdict | `send '@openai @grok $debate Defend your choice in one short sentence. @judge decide the better short-break recommendation.' --yes --json` | 0 | 45.979 | $0.121362 | COMPLETE; round 7–5 and verdict for openai; four contest calls; usage matches. |
| survey | `send '@openai $survey For a short break, choose tea or coffee and give one brief reason.' --yes --json` | 1 | 7.629 | $0.001798 | DAILY_MAX_REACHED at admission; only startup probes billed. |
| survey-cheap | `send '@openai $survey For a short break, choose tea or coffee and give one brief reason.' --yes --json` | 0 | 17.289 | $0.002068 | COMPLETE with missing revote; one participant; five planned calls, six actual attempts; cheaper overrides below; usage matches. |

For `survey-cheap` only, set `SQUAD_MODEL_OPENAI=openai/gpt-4o-mini` and `SQUAD_JUDGE_MODEL=anthropic/claude-haiku-5.5`. Setting a single member variable selects a one-seat roster. These overrides were reported before the retry; repository defaults were not changed. Preflight reserved $0.265872, estimated $0.11077575, and allowed five calls / twelve attempts. Both revote attempts produced invalid structured output and were charged. The revote work ended failed with `JUDGE_INVALID_VERDICT` / `Survey output failed validation`; the report completed using the first ballot and marked `notRevoted: ["openai"]` for both claims. Exit 0 means the survey script completed, not that every stage succeeded. The default survey reservation used conservative ~100K-token input bounds at the most expensive selected executor's rates and would not fit the cap even for one participant.

## Default seat resolution

| Seat | Requested selection | Resolved concrete slug | Before fix | After fix |
|---|---|---|---|---|
| claude | `~anthropic/claude-opus-latest` | `anthropic/claude-opus-5.5` | PROVIDER_ERROR | Exit 0, reply |
| fable | `~anthropic/claude-fable-latest` | `anthropic/claude-fable-5.2` | PROVIDER_ERROR | Exit 0, reply |
| openai | `~openai/gpt-sol-latest` | `openai/gpt-6.2-sol` | PROVIDER_ERROR | Exit 0, reply |
| gemini | `google/gemini-4-neon` | `google/gemini-4-neon` | PROVIDER_ERROR | Exit 0, reply |
| grok | `~x-ai/grok-latest` | `x-ai/grok-4.7` | PROVIDER_ERROR | Exit 0, reply |
| muse | `meta/muse-wonder` | `meta/muse-wonder` | PROVIDER_ERROR | Exit 0, reply |

The default judge alias resolved to `anthropic/claude-fable-5.2`. None of the six configured selections was unknown to the live catalog.

## Usage reconciliation and streaming

There were 109 priced attempts: 90 startup probes and 19 priced provider streams (seven single-seat replies, two debate seed replies, four debate calls, and six survey attempts including two invalid revotes). All stored usage costs match their attempt accounting after microdollar rounding; their sum matches `spend_daily` exactly. The seven failed attempts have null usage and remain unresolved.

A loopback pass-through recorder was enabled starting with `seat-openai`, forwarding to the real OpenRouter API. It retained only model, reasoning settings, HTTP status, error details, and usage fields, never headers or credentials. All **94 captured provider usage responses** matched stored model/input/output/cost tuples, with no mismatches. The earlier 15 paid probe calls were checked against their stored usage fields but lack independent response capture. Error-response usage is unavailable, not a verified zero.

Successful generation used provider SSE with terminal usage and valid final messages. The CLI consumes ephemeral feed items for wakeups and prints committed final messages rather than provisional token deltas; no visible per-token CLI stream was observed. No runtime crash, panic, hang, or garbled text occurred. The intentionally failing regression test panicked on its assertion before the fix, as expected.

## Findings and fix

1. **Fixed: every reasoning-enabled generation was rejected.** The request sent both `reasoning.effort` and `reasoning.max_tokens`. OpenRouter returned HTTP 400 with the exact message: `Only one of "reasoning.effort" and "reasoning.max_tokens" can be specified`. The CLI reduced this to `PROVIDER_ERROR`, losing the actionable cause. The request builder now sends only `reasoning.max_tokens`, preserving the compile-time low/medium/high budgets and visible-plus-reasoning `max_tokens` ceiling. Alias probes still use effort alone. The PRD request contract and existing assertions were updated accordingly.
2. **Regression proof:** a Wiremock responder reproduces the provider's mutually exclusive field rule. The new test failed against the original builder with `Http(400)` and passed after the fix. It verifies successful streaming, the 4,096-token reasoning budget, and the 6,144-token total ceiling. Tests make no live calls.
3. **Unknown-cost residue:** seven rejected pre-fix attempts remain unresolved and keep `unknown_calls=7` after subsequent successful runs. Rejected-request accounting should distinguish known non-generation failures if provider guarantees permit it; this smoke does not fabricate their costs.
4. **Repeated paid startup probes:** every invocation re-probes all four aliases and the judge. These costs are included in each row, including rejected commands. Offline `doctor` and `models` do not probe.
5. **Verbose accounting events:** the four-call debate printed 416 `usage.record` events in 402,291 bytes of JSON output; the survey (five planned calls, six actual attempts) printed 582 in 563,277 bytes. IDs were distinct and accounting remained idempotent. Metadata handling submits repeated account updates while streaming. This is noisy CLI output, not extra provider calls or duplicated spend.
6. **Doctor has stale status text:** it prints `OpenRouter reachability: skipped until M2` and `Model resolution: skipped until M2`, despite the runtime being integrated. PASS therefore proves local configuration/writability, not online connectivity.
7. **Six-word answer:** the post-fix five-word prompt returned `Hi there, great to meet you!` (six words). This is model instruction noncompliance; no transport corruption was observed.
8. **Debate source requirements:** two separately addressed seat checks have different question identities, so direct reuse was correctly rejected with `DEBATE_TOPIC_MISMATCH`. Sending one common question to both seats allowed the minimum one-round contest to run through round scoring and a final verdict. Direct debate requires explicit `@judge` to request the verdict.
9. **Survey completed with a failed stage:** both cheap-model revote attempts failed validation (`JUDGE_INVALID_VERDICT`, `Survey output failed validation`). The final report retained the first ballot, marked both claims `notRevoted: ["openai"]`, and described the single-voter limitation. The command exited 0 and the contest reached COMPLETE despite that missing revote. Do not read this smoke as a fully successful survey ballot cycle.
10. **Cap refusal wording:** `DAILY_MAX_REACHED` also means a conservative new reservation will not fit, even when actual spend is far below the cap. The successful cheaper survey demonstrates the limit can be respected without raising it.

## Validation

- `CARGO_BUILD_JOBS=8 cargo build --release -p squad-cli`: passed before and after the fix.
- New Wiremock regression: fail before / pass after.
- `CARGO_BUILD_JOBS=8 cargo test --workspace --no-fail-fast`: **912 passed, 0 failed, 2 ignored**.
- `CARGO_BUILD_JOBS=8 cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- No model defaults changed. All work committed to `live-smoke`, never main.

Raw local evidence (not committed): `/tmp/squad-live-2026-11-25/` contains per-invocation stdout/stderr, result snapshots, sanitized provider response metadata, and build/test/check logs. The private SQLite ledger is under its `data/` subdirectory. These are temporary local artifacts and may be removed by normal temporary-directory cleanup.
