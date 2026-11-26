# 💸 What a Squad run costs

**Status: partial.** Measured on 2026-11-25 with the default roster and default settings. The study stopped early because OpenRouter's **workspace weekly budget ($250)** ran out partway through the first wave: every call after that returned `403 Workspace weekly budget of $250.00 exceeded`. The workspace was already near $241 for the week before the study began, and the study's own runs spent $13.60. Modes or runs that could not be measured are marked **not measured**. A follow-up will fill them in once the budget is raised or resets (2026-11-30 00:00 UTC).

## Method

- Binary: `cargo build --release -p squad-cli` from branch `cost-study`.
- One run per fresh `SQUAD_DATA_DIR`, sent with `squad send "<message>" --yes --json`. All runs of the first wave were launched at the same time on one machine.
- `SQUAD_DAILY_MAX_USD=100`, so the cap would not block. Tournament and survey still reserve more than $100, so those two were refused even at that cap (see below). Default `ROUNDS` (10), `GRILL_ROUNDS` (2), `CENSUS_PASSES` (2) and default reasoning effort were not changed.
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

## Per-mode summary

| Mode | Typical cost (clean runs) | Range seen | Time | Reserved / actual | Refused at the default $20 cap? | Notes |
|---|---|---|---|---|---|---|
| Ordinary `@all` | $0.16 (one run) | one run | about 23 s | at least 240× | **yes** (≥ $39.6) | n = 1. Needs two more runs for variance. |
| Debate (10 rounds, judged) | $2.85 (one clean run) | $2.44 to $2.85 (the low one lost its final judge call) | about 8 min | 5.9× | no ($16.77) | The only mode under $20 reserved. |
| Tournament | not measured | n/a | n/a | n/a | **yes** ($110.53) | Refused even at a $100 cap. |
| Grill | not measured (failed after 3 calls both times) | $0.17 to $0.19 spent before failing | n/a | n/a | **yes** ($68.67) | Leader step (claude) failed `MODE_INVALID_OUTPUT` twice. |
| Census | not measured (budget hit) | $1.32 to $1.55 spent before failing | n/a | n/a | **yes** ($46.22) | Estimate $20.35, 28 calls planned. |
| Crosscheck | not measured (budget hit) | $1.19 to $1.50 spent before failing | n/a | n/a | **yes** ($20.61) | Over the cap by $0.61. |
| Survey | not measured | n/a | n/a | n/a | **yes** ($108.83) | Refused even at a $100 cap. |
| Probe | $0.96 (one clean complete run) | $0.96 to $1.24 | 165 to 239 s | 27× | **yes** ($26.18) | 10 to 12 calls. The second run failed at the judge with `JUDGE_INVALID_VERDICT`. |

Total spent by the first wave: **$13.60** by Squad's ledger, including the startup probes of 19 data directories. OpenRouter showed $15.22 for the key that day, which also includes about $1.6 spent before the study started.

## What this means for a user

- At the default $20 daily cap, **every mode except a 10-round debate is refused on its first message**, because contest reservations are 2 to 20 times the real cost. This is the known oversized-reservation issue. A plain `@all` question also reserved at least $39.6.
- Real costs are small next to the reservations: about $0.17 for an `@all` question, about $1 for a probe, about $2.50 to $3 for a full judged debate.
- Mixed-vendor failures are real and not about money: `claude` returned `MODE_INVALID_OUTPUT` in grill (2 of 2) and census (2 of 2), `gemini` once in crosscheck, and the probe judge returned `JUDGE_INVALID_VERDICT` once.

## Still to measure

Three ordinary runs, two runs each of tournament, survey, grill, census and crosscheck, a second clean debate, and a second clean probe, with `SQUAD_DAILY_MAX_USD` set high enough for the $110 reservations. A single run of a mode is not enough to quote a range.
