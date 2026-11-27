# 💸 What a Squad run costs

**Status: the original eight workflows are measured; v0.2.0 integration measurements follow below.** Part 1 (2026-11-25) was cut short by OpenRouter's $250 weekly workspace budget. Part 2 (2026-11-26) measured ordinary, debate, survey and tournament on `cost-study-2`. Part 3 (2026-11-26 UTC) measured grill twice, census twice, crosscheck twice and a second successful probe on release build `e4e4bdc` of `release-v0.1.1`, after merging `fix-live-invalid-output` (`18c890d`). All seven part 3 runs completed; three judge outputs needed a paid repair. The default daily cap was $150.

## Method

- Binary: `cargo build --release -p squad-cli` (`CARGO_BUILD_JOBS=4`). Part 1 used branch `cost-study`; part 2 used `cost-study-2`; part 3 used merged `release-v0.1.1` (`e4e4bdc`).
- One run per fresh `SQUAD_DATA_DIR`, sent with `squad send "<message>" --yes --json`. All runs of the first wave were launched at the same time on one machine.
- Part 1 set `SQUAD_DAILY_MAX_USD=100`. **Part 2 left the cap at the new $150 default** (`SQUAD_DAILY_MAX_USD` unset). Tournament and survey still reserve more than $100, so those two were refused even at that cap (see below). Default `ROUNDS` (10), `GRILL_ROUNDS` (2), `CENSUS_PASSES` (2) and default reasoning effort were not changed.
- **Cost** is the ledger total in `squad.db` (`spend_daily`). It was cross-checked against OpenRouter by looking up every call's `providerRequestId` on `/api/v1/generation`. Where all of a run's calls were found, the OpenRouter sum equals the sum of Squad's `usage.record` costs to the millionth of a dollar. The ledger runs about **$0.0015 higher** than the call sum in every fresh data directory: that is Squad's four startup model-identity probes, paid once per data directory.
- **Reserved** is the peak `reserved_micros` in the ledger, sampled twice a second. **Estimate** is the `estimatedUsd` Squad shows in the cost preview.
- Tokens are the sums of the `usage.record` input and output tokens. Output includes reasoning. The reasoning-only count comes from OpenRouter's `native_tokens_reasoning`.
- Wall time is from process start to the `usage.total` line.

### Resolved models (default roster)

| Seat | Configured | Resolved to |
|---|---|---|
| claude | `~anthropic/claude-opus-latest` | `anthropic/claude-opus-5.5` |
| fable | `~anthropic/claude-fable-latest` | `anthropic/claude-fable-5.2` |
| openai | `~openai/gpt-sol-latest` | `openai/gpt-6.2-sol` |
| gemini | `google/gemini-4-neon` | `google/gemini-4-neon` |
| grok | `~x-ai/grok-latest` | `x-ai/grok-4.7` |
| muse | `meta/muse-wonder` | `meta/muse-wonder` |
| judge | `~anthropic/claude-fable-latest` | `anthropic/claude-fable-5.2` |

## Per-run table

`Clean` means the run ended without any call being refused by the OpenRouter budget. Only completed, budget-clean runs count towards the typical cost in the summary. Recovered validation errors still count as paid calls.

| Run | Mode | Outcome | Clean | Cost (ledger) | Calls (failed) | In / out / reasoning tokens | Wall | Estimate | Reserved | Refused at $20? |
|---|---|---|---|---|---|---|---|---|---|---|
| ord1 | ordinary `@all` | COMPLETE | yes | $0.1655 | 6 (0) | 6,755 / 7,326 / 2,675 | 23 s | none shown | ≥ $39.6 (seen after the first reply landed) | yes |
| ord2, ord3 | ordinary `@all` | not run correctly | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| deb1 | debate, `@claude @openai`, judged | COMPLETE | yes | $2.8453 | 33 (0) | 369,977 / 29,727 / 10,844 | 515 s | $7.87 | $16.77 | no |
| deb2 | debate | FAILED: final judge got 403 | no | $2.4422 | 33 (1) | 332,236 / 26,580 / 9,810 | 473 s | $7.87 | $16.77 | no |
| tour1, tour2 | tournament `@all` | REFUSED at the $100 cap; successful measurements in part 2 below | n/a | $0.0015 (startup probes) | 0 | n/a | n/a | not shown | $110.53 | yes |
| grill1 | grill, leader `@claude` | FAILED: `MODE_INVALID_OUTPUT`, `leader_failed` | yes | $0.1691 (plan $0.034 + grill $0.134) | 1 + 3 (0) | 10,334 / 4,969 / 924 | 22 s + 40 s | $34.33 | $68.67 | yes |
| grill2 | grill, leader `@claude` | FAILED: same | yes | $0.1948 (plan $0.038 + grill $0.156) | 1 + 3 (0) | 9,967 / 6,155 / 2,494 | 24 s + 50 s | $34.30 | $68.60 | yes |
| cen1 | census | FAILED: `judge_attempts_exhausted`. 1 call `MODE_INVALID_OUTPUT` (claude), 2 got 403 | no | $1.5542 | 20 (4) | 74,914 / 46,616 / 26,744 | 453 s | $20.35 | $46.22 | yes |
| cen2 | census | FAILED: `no_ratings`. 1 call `MODE_INVALID_OUTPUT` (claude), 6 got 403 | no | $1.3204 | 22 (12) | 40,501 / 35,209 / 20,253 | 239 s | $20.35 | $46.22 | yes |
| cross1 | crosscheck | FAILED. 1 call `MODE_INVALID_OUTPUT` (gemini), 4 got 403 | no | $1.1936 | 22 (9) | 57,149 / 39,170 / 26,847 | 533 s | $9.23 | $20.61 | yes (barely) |
| cross2 | crosscheck | killed by me while 403s were hitting it | no | $1.5049 | n/a | n/a | 649 s | $9.23 | $20.61 | yes (barely) |
| surv1, surv2 | survey `@all` | REFUSED at the $100 cap; successful measurements in part 2 below | n/a | $0.0015 (startup probes) | 0 | n/a | n/a | not shown | $108.83 | yes |
| probe1 | probe `@all` | FAILED: judge `JUDGE_INVALID_VERDICT` | yes | $1.2393 | 12 (0) | 49,711 / 31,881 / 18,718 | 239 s | $6.29 | $26.18 | yes |
| probe2 | probe `@all` | COMPLETE | yes | $0.9607 | 12 (0) | 48,624 / 27,506 / 13,635 | 165 s | $5.86 | $26.18 | yes |

ord2 and ord3 were sent without `@all` by mistake and rejected with `RECIPIENT_REQUIRED` (cost: startup probes only).

## Part 2 runs (2026-11-26, default $150 cap)

Seven runs launched at the same time, each with a fresh data directory, sent with `squad send "<message>" --yes --json`. The OpenRouter per-call lookup found all 157 of 157 calls, and the sum matched Squad's ledger to the millionth of a dollar (ledger is $0.0015 higher per run for the startup probes, as before).

| Run | Mode | Outcome | Cost (ledger) | Calls (failed) | In / out / reasoning tokens | Wall | Estimate | Reserved |
|---|---|---|---|---|---|---|---|---|
| ord4 | ordinary `@all` | COMPLETE | $0.0575 | 6 (0) | 6,360 / 1,677 / 712 | 12 s | none shown | $43.72 |
| ord5 | ordinary `@all` | COMPLETE | $0.0447 | 6 (0) | 6,360 / 1,319 / 503 | 15 s | none shown | $43.72 |
| deb3 | debate, `@claude @openai`, judged | COMPLETE | $2.7778 | 33 (0) | 364,991 / 28,940 / 10,334 | 215 s | $13.14 | $27.87 |
| surv1 | survey `@all` | COMPLETE | $1.7153 | 22 (0) | 109,917 / 46,068 / 19,298 | 223 s | $6.07 | $27.05 |
| surv2 | survey `@all` | COMPLETE | $1.5004 | 21 (0) | 99,036 / 43,616 / 22,089 | 275 s | $6.07 | $27.05 |
| tour1 | tournament `@all` | COMPLETE | $1.7842 | 25 (0) | 99,038 / 48,354 / 24,316 | 279 s | $12.40 | $89.69 |
| tour2 | tournament `@all` | COMPLETE | $1.7071 | 25 (0) | 97,132 / 50,887 / 27,492 | 292 s | $12.40 | $89.69 |

Messages: ordinary `@all What is one practical tip for writing a good commit message?`; debate `@claude @openai $debate Should a new command line tool be written in Rust or Go? @judge`; survey `@all $survey What should a first release of a terminal chat app include, and what should wait?`; tournament `@all $tournament Design a useful CLI`.

Part 2 spent **$9.59** by Squad's ledger ($9.58 of call cost plus $0.01 of startup probes).

## Part 3 runs (2026-11-26 UTC, merged live-output fixes)

The seven runs started in parallel on dev3. Parallel provider and host load can inflate wall time. Configuration cleared every inherited `SQUAD_*` value, read `/dev/null` as the env file, and explicitly set the six default roster models and default judge listed above. The daily cap and all word, round, pass and reasoning settings used code defaults. No local 12-seat roster was loaded.

Grill includes an ordinary Claude seed-plan call followed by `@claude $grill Focus on rollout risks` (22 contest calls). Each census used `squad send` for its initial request and then `squad send "/continue" --yes --json` in the same data directory to run pass 2 with the unchanged plan. The first census send exits successfully at `WAITING_FOR_PLAN`; that is a pause, not the end of a two-pass census. Grill and census wall times below sum their active sends through `usage.total`, excluding time between commands.

| Run | Mode | Outcome | Ledger cost | OpenRouter call cost | Calls (provider failures) | In / out / reasoning tokens | Wall | Estimate | Peak reserved |
|---|---|---|---|---|---|---|---|---|---|
| grill3 | grill, Claude leader | COMPLETE | $0.897465 | $0.8959465 | 23 (0) | 123,603 / 22,812 / 9,736 | 259 s | $6.71 | $31.72 |
| grill4 | grill, Claude leader | COMPLETE | $0.971097 | $0.9695601 | 23 (0) | 121,527 / 28,646 / 15,654 | 300 s | $6.70 | $31.69 |
| cen3 | census, 2 passes | COMPLETE | $2.242693 | $2.2411502 | 28 (0) | 113,935 / 78,108 / 29,284 | 320 s | $6.66 | $30.85 |
| cen4 | census, 2 passes | COMPLETE | $2.418224 | $2.4167742 | 28 (0) | 115,083 / 86,639 / 38,212 | 344 s | $6.66 | $30.85 |
| cross3 | crosscheck | COMPLETE | $2.516225 | $2.5147691 | 17 (0) | 220,152 / 49,423 / 21,525 | 274 s | $5.07 | $19.67 |
| cross4 | crosscheck | COMPLETE | $2.542356 | $2.5408295 | 17 (0) | 217,408 / 48,309 / 25,766 | 246 s | $5.07 | $19.67 |
| probe3 | probe | COMPLETE | $0.853637 | $0.8521505 | 11 (0) | 46,426 / 24,798 / 11,414 | 167 s | $2.60 | $10.61 |

All **147/147** call lookups were found. OpenRouter's **$12.4311801** exactly matched the sum of Squad's `usage.record` call costs. Ledger spend was **$12.441697**, about **$0.010517** higher for startup identity probes and per-attempt rounding across seven fresh data directories. Every final ledger had zero reserved funds and zero unknown calls. No provider calls failed and no 403s occurred.

Validation failures did occur and their repair costs are included:

- `cross3` and `cross4`: one `JUDGE_INVALID_MODE_OUTPUT` each. The first ruling was truncated (`finish_reason=length`); the repair returned a valid ruling. Each therefore made 17 calls instead of the estimated 16.
- `probe3`: one `JUDGE_INVALID_VERDICT`, with diagnostic `group contains distinguishable explanations`. The repair passed; 11 calls instead of the estimated 10.
- Both grills and both two-pass censuses completed without validation repairs or skipped work.

Exact input messages (the seed and contest share a fresh directory for each grill; census continuation reuses its own directory):

```text
@claude Draft a practical release plan for a terminal chat app in at most 300 words.
@claude $grill Focus on rollout risks

@all $census Launch a terminal chat app: ship a signed binary for Linux and macOS, document setup, test the release, announce it, monitor errors, and roll back if needed.
/continue

@all $crosscheck Launch a terminal chat app: ship a signed binary for Linux and macOS, document setup, test the release, announce it, monitor errors, and roll back if needed.
--
Release a reliable terminal chat app with one operator and a small budget.

@all $probe Requests stall intermittently. You may inspect application logs and measure request latency; no production changes are allowed.
```

## Per-mode summary

Cost and wall time cover completed, budget-clean runs across all three parts; historical terminal failures above are excluded. Probe combines the successful part 1 run and part 3 run. Reservations and estimates use the latest measured build for each mode (part 2 for the first four modes, part 3 for the last four). `Reserved ÷ actual` uses the largest current reservation divided by mean ledger cost. Model assignment, prompts and provider behavior vary; these are examples, not matched benchmarks or price guarantees.

| Mode | Runs | Mean cost (min–max) | Mean wall (min–max) | Estimate shown | Peak reserved | Reserved ÷ actual | Fits $150 cap? |
|---|---|---|---|---|---|---|---|
| Ordinary `@all` | 3 | $0.089 ($0.045–$0.166) | 17 s (12–23) | none | $43.72 | 490.0× | yes |
| Debate (10 rounds, judged) | 2 | $2.812 ($2.778–$2.845) | 365 s (215–515) | $13.14 | $27.87 | 9.9× | yes |
| Survey | 2 | $1.608 ($1.500–$1.715) | 249 s (223–275) | $6.07 | $27.05 | 16.8× | yes |
| Tournament | 2 | $1.746 ($1.707–$1.784) | 286 s (279–292) | $12.40 | $89.69 | 51.4× | yes |
| Probe | 2 | $0.907 ($0.854–$0.961) | 166 s (165–167) | $2.60 | $10.61 | 11.7× | yes |
| Grill | 2 | $0.934 ($0.897–$0.971) | 279 s (259–300) | $6.70 ($6.70–$6.71) | $31.69–$31.72 | 34.0× | yes |
| Census (2 passes) | 2 | $2.330 ($2.243–$2.418) | 332 s (320–344) | $6.66 | $30.85 | 13.2× | yes |
| Crosscheck | 2 | $2.529 ($2.516–$2.542) | 260 s (246–274) | $5.07 | $19.67 | 7.8× | yes |

## What this means for a user

- **Every measured mode fits the $150 default cap**, including the largest reservation (tournament, $89.69). Reservations were also reshaped by the v0.1.1 fixes: the $110.53 and $108.83 reservations that blocked tournament and survey in part 1 are now $89.69 and $27.05, and an ordinary `@all` question reserves $43.72 instead of at least $39.6.
- Real cost is still a small fraction of what is held back. Expect about **$0.05 to $0.17** for an `@all` question, about **$1.50 to $1.75** for a survey or a tournament, about **$2.80** for a judged 10-round debate, and about **$0.85 to $0.96** for a probe, **$0.90 to $0.97** for a two-round grill including its seed plan, **$2.24 to $2.42** for a two-pass census, and **$2.52 to $2.54** for a crosscheck. A mode's reservation is released as its calls finish, so it only matters as headroom against the daily cap.
- Cost per ordinary question depends on the question. Part 1's run answered a longer prompt (7,326 output tokens, $0.17); part 2's short question produced 1,300 to 1,700 output tokens ($0.04 to $0.06). Reasoning tokens are billed as output.
- Wall time depends on load. All of part 2 ran 7 runs at once; the debate took 215 s there against 515 s in part 1, when 19 runs shared the machine and OpenRouter's budget limit.
- All seven part 3 runs completed. Crosscheck still produced two truncated first rulings (`JUDGE_INVALID_MODE_OUTPUT`), and probe one invalid first report (`JUDGE_INVALID_VERDICT`); automatic judge repairs recovered all three and are included in the cost. The four grill/census runs had no validation repairs.
- Reservation size depends on plan bounds and model roles. Part 3's probe reserved $10.61 versus $26.18 in part 1; its random role split and brief differ, so the smaller reservation is not a controlled before/after comparison.

The original seven contest modes and ordinary chat are measured in this study. The v0.2.0 additions are recorded separately below.

## v0.2.0 integrated default-roster measurements

These runs use release 0.2.0, the built-in **six-member** roster and judge,
`SQUAD_DAILY_MAX_USD=150`, a fresh private data directory per mode, and
`--env-file /dev/null`. Credentials enter the process environment off-screen.
Each row is one paid mode invocation. Wall time runs from process launch to
observed exit (0.5-second polling); reservation peaks are sampled at that interval.
Actual spend includes startup probes. Source/binary provenance and further runs
are recorded in [integration live evidence](live-smoke-2026-11-26-v0.2.0.md).

| Mode | Outcome | Actual | Wall | Preflight call bound / provider attempts incl. startup | Initial reserved | Sampled peak | Final reserved / unknown |
|---|---|---:|---:|---:|---:|---:|---|
| Rank | COMPLETE, ranked; one member repair | $0.190184 | 38.636 s | 7 / 12 | $18.024576 | $18.024576 | $0 / 0 |
| Forge | COMPLETE, original_won | $0.345117 | 80.682 s | 15 / 19 | $24.736836 | $24.736836 | $0 / 0 |
| Forecast | COMPLETE, complete | $0.470614 | 105.725 s | 14 / 18 | $20.188827 | $20.188827 | $0.000000 / 0 |
| Decide | COMPLETE, complete | $0.400975 | 69.135 s | 14 / 18 | $16.400079 | $16.400080 | $0.000000 / 0 |
| Blow It Up | COMPLETE, reported | $1.469007 | 265.024 s | 13 / 20 | $20.574231 | $20.574231 | $0.000000 / 0 |
| Readback | AWAITING_HUMAN, choices required | $0.428807 | 57.114 s | 8 / 12 | $10.961076 | $10.961076 | $6.724861 / 0 |
| Tune | COMPLETE, tuned | $0.522077 | 134.272 s | 34 / 35 | $24.970323 | $24.970323 | $0.000000 / 0 |
| Bass Ackwards PRD | COMPLETE, saved | $2.552690 | 410.874 s | 28 / 33 | $50.711781 | $50.711781 | $0.000000 / 0 |
| Negotiate | COMPLETE, rehearsed | $1.591324 | 274.577 s | 29 / 39 | $32.021214 | $32.021214 | $0.000000 / 0 |

### Original timeout holds reconciled

The integrated runtime reopened the original rooms with `/who` and reconciled
their delayed receipts through the canonical writer. Forecast and Decide stayed at nineteen attempts each; Debug stayed at twelve.
No new provider generation or direct ledger SQL edit occurred.

| Original smoke | Spent at exit | Held at exit | Late actual charge | Settled spent | Final held / unknown |
|---|---:|---:|---:|---:|---|
| Forecast, dev3 | $0.493022 | $0.510112 | $0.014302 | $0.507324 | $0 / 0 |
| Decide, dev2 | $0.607680 | $0.583480 | $0.014226 | $0.621906 | $0 / 0 |
| Debug, dev2 worker partial | $0.113858 | $1.565995 | $0.037286 | $0.151144 | $0 / 0 |

### Integrated Tune executor cost

The one-generation run dispatches 31 mode calls against a 34-call preflight upper
bound. All accepted work completed; the retained suite has three train and three
held-out cases. The exported task-specific held-out score improved from 0.03 to
1.00; this is a smoke result, not a general prompt-quality benchmark.

| Component | Attempts | Actual |
|---|---:|---:|
| Bare executor (`tune.execute`) | 18 | $0.095800 |
| Member case proposals and mutations | 9 | $0.203165 |
| Judge freeze and score | 4 | $0.221576 |
| Startup probes | 4 | $0.001536 |
| Total | 35 | $0.522077 |

### Integrated Negotiate retries and outcomes

Default six members and three rounds produced twenty-nine completed work items.
Six first member outputs exceeded their word budgets and repaired privately: one
map, two moves and three counterpart replies. Thirty-five mode attempts plus
four startup probes cost $1.591324. Both judge calls passed on the first attempt.
One table ended in a deal; two reached the round ceiling unfinished; none were
abandoned. Final reservation and unknown calls are zero. The preflight bound is
29 calls / 60 reserved attempts; the estimated cost was $12.431808.

Debug's two delayed Grok charges reconciled automatically. Its third old hold
was an undispatched judge attempt: the old work-kind guard proves the provider
runner was never called. That exact record was reconciled at zero through the
accounting writer after a backup dry run, with a system audit notice. This is
source-backed repair, not a policy that treats missing generation IDs as free.
See [audit source and summary](reports/v0.2.0-debug-reconciliation/).
