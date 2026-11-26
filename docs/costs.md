# 💸 What a Squad run costs

**Status: ordinary, debate, survey and tournament measured; probe from part 1; grill, census and crosscheck pending fix.** Part 1 (2026-11-25) was cut short by OpenRouter's $250 weekly workspace budget. Part 2 (2026-11-26) re-ran what was missing on a release build of branch `cost-study-2`, which carries the $150 default daily cap and all v0.1.1 fixes. All seven part 2 runs were clean: every call completed, no failures, no 403s.

## Method

- Binary: `cargo build --release -p squad-cli` (`CARGO_BUILD_JOBS=4`). Part 1 used branch `cost-study`; part 2 used `cost-study-2`.
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

`Clean` means the run ended without any call being refused by the OpenRouter budget. Only clean runs count towards the typical cost in the summary.

| Run | Mode | Outcome | Clean | Cost (ledger) | Calls (failed) | In / out / reasoning tokens | Wall | Estimate | Reserved | Refused at $20? |
|---|---|---|---|---|---|---|---|---|---|---|
| ord1 | ordinary `@all` | COMPLETE | yes | $0.1655 | 6 (0) | 6,755 / 7,326 / 2,675 | 23 s | none shown | ≥ $39.6 (seen after the first reply landed) | yes |
| ord2, ord3 | ordinary `@all` | not run correctly | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| deb1 | debate, `@claude @openai`, judged | COMPLETE | yes | $2.8453 | 33 (0) | 369,977 / 29,727 / 10,844 | 515 s | $7.87 | $16.77 | no |
| deb2 | debate | FAILED: final judge got 403 | no | $2.4422 | 33 (1) | 332,236 / 26,580 / 9,810 | 473 s | $7.87 | $16.77 | no |
| tour1, tour2 | tournament `@all` | not measured: refused at the $100 cap | n/a | $0.0015 (startup probes) | 0 | n/a | n/a | not shown | $110.53 | yes |
| grill1 | grill, leader `@claude` | FAILED: `MODE_INVALID_OUTPUT`, `leader_failed` | yes | $0.1691 (plan $0.034 + grill $0.134) | 1 + 3 (0) | 10,334 / 4,969 / 924 | 22 s + 40 s | $34.33 | $68.67 | yes |
| grill2 | grill, leader `@claude` | FAILED: same | yes | $0.1948 (plan $0.038 + grill $0.156) | 1 + 3 (0) | 9,967 / 6,155 / 2,494 | 24 s + 50 s | $34.30 | $68.60 | yes |
| cen1 | census | FAILED: `judge_attempts_exhausted`. 1 call `MODE_INVALID_OUTPUT` (claude), 2 got 403 | no | $1.5542 | 20 (4) | 74,914 / 46,616 / 26,744 | 453 s | $20.35 | $46.22 | yes |
| cen2 | census | FAILED: `no_ratings`. 1 call `MODE_INVALID_OUTPUT` (claude), 6 got 403 | no | $1.3204 | 22 (12) | 40,501 / 35,209 / 20,253 | 239 s | $20.35 | $46.22 | yes |
| cross1 | crosscheck | FAILED. 1 call `MODE_INVALID_OUTPUT` (gemini), 4 got 403 | no | $1.1936 | 22 (9) | 57,149 / 39,170 / 26,847 | 533 s | $9.23 | $20.61 | yes (barely) |
| cross2 | crosscheck | killed by me while 403s were hitting it | no | $1.5049 | n/a | n/a | 649 s | $9.23 | $20.61 | yes (barely) |
| surv1, surv2 | survey `@all` | not measured: refused at the $100 cap | n/a | $0.0015 (startup probes) | 0 | n/a | n/a | not shown | $108.83 | yes |
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

## Per-mode summary

Cost and wall time are over the clean runs of both parts. Reserved is the peak reservation seen in the **current build** (part 2). `Reserved ÷ actual` uses the mean cost.

| Mode | Runs | Mean cost | Min | Max | Mean wall time | Estimate shown | Reserved | Reserved ÷ actual | Fits $150 cap? |
|---|---|---|---|---|---|---|---|---|---|
| Ordinary `@all` | 3 | $0.089 | $0.045 | $0.166 | 17 s (12 to 23) | none | $43.72 | 490× | **yes** |
| Debate (10 rounds, judged) | 2 | $2.81 | $2.78 | $2.85 | 6 min (3.6 to 8.6) | $13.14 | $27.87 | 9.9× | **yes** |
| Survey | 2 | $1.61 | $1.50 | $1.72 | 4.1 min (3.7 to 4.6) | $6.07 | $27.05 | 16.8× | **yes** |
| Tournament | 2 | $1.75 | $1.71 | $1.78 | 4.8 min (4.6 to 4.9) | $12.40 | $89.69 | 51× | **yes** |
| Probe | 1 (part 1) | $0.96 | n/a | n/a | 165 s | $5.86 | $26.18 (old build, not re-measured) | 27× | yes (reserved was $26.18) |
| Grill | pending fix | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| Census | pending fix | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| Crosscheck | pending fix | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

## What this means for a user

- **Every measured mode fits the $150 default cap**, including the largest reservation (tournament, $89.69). Reservations were also reshaped by the v0.1.1 fixes: the $110.53 and $108.83 reservations that blocked tournament and survey in part 1 are now $89.69 and $27.05, and an ordinary `@all` question reserves $43.72 instead of at least $39.6.
- Real cost is still a small fraction of what is held back. Expect about **$0.05 to $0.17** for an `@all` question, about **$1.50 to $1.75** for a survey or a tournament, about **$2.80** for a judged 10-round debate, and about **$1** for a probe. A mode's reservation is released as its calls finish, so it only matters as headroom against the daily cap.
- Cost per ordinary question depends on the question. Part 1's run answered a longer prompt (7,326 output tokens, $0.17); part 2's short question produced 1,300 to 1,700 output tokens ($0.04 to $0.06). Reasoning tokens are billed as output.
- Wall time depends on load. All of part 2 ran 7 runs at once; the debate took 215 s there against 515 s in part 1, when 19 runs shared the machine and OpenRouter's budget limit.
- Part 1's `claude` and `gemini` `MODE_INVALID_OUTPUT` failures, the probe judge's `JUDGE_INVALID_VERDICT`, and the debate judge's mid-run 403 did not recur in part 2. Grill, census and crosscheck failed in part 1 and are being fixed, so they are marked pending.

## Still to measure

Grill, census and crosscheck, once the invalid-output fixes land (two runs each), and a second clean probe on the current build.
