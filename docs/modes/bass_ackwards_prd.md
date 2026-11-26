# Bass ackwards PRD

The mode is enabled in the parser, typeahead, help, TUI and CLI.

Start with the finished result. Squad agrees on observable acceptance tests, works backward to today's prerequisites, and saves a dependency-ordered PRD.

```text
@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts
@all $bass_ackwards_prd a production-ready status page -- We have a repository and a Linux server, but no deployed application.
```

From the CLI:

```sh
squad send '@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts'
```

1. Members independently propose three to eight acceptance tests. The judge groups equivalent tests without rewording them. Application code freezes tests supported by at least two distinct members, up to twelve.
2. Members independently build backward prerequisite trees, each bounded to twenty nodes and four levels. Every node serves a frozen acceptance test.
3. The judge merges those trees into milestones, phases and tasks. Every task cites a source node and an acceptance test. Dependencies point within the same or an earlier milestone.
4. Each member reviews through one assigned lens: missing steps, ways the plan can fail, or a concrete readback of one milestone. Comments must quote the delivered PRD. The application drops invented quotes without another call.
5. The judge accepts or rejects each surviving comment. The application protects permanent task identities and rejects unrelated edits.
6. Members propose up to three must-have additions each. Final assembly is add-only: existing text and order remain intact.
7. Application code checks task mapping, acceptance-test coverage, missing dependencies and cycles. It computes parallel levels and the longest dependency chain, then saves the Markdown PRD.

The report includes uncovered acceptance tests, skipped members, quote failures, review decisions, must-haves and provenance resolved to member names. A coverage finding does not invent extra work: the file is saved with findings clearly stated.

For an optional final human review, set `/set PRD_HUMAN_REVIEW 1` before starting. At `LAST LOOK`, `/answer <note>` adds a note and `/continue` proceeds to saving. Notes are limited to ten and 500 words together. Human notes are instructions; any impossible application is recorded as a conflict. With no notes there is no additional judge call. The default is off, so one command produces one file. A one-shot send with review enabled returns while waiting, before a file exists.

Six members use at most 28 calls and reserve 60 attempts; three use 16 calls and reserve 36 attempts. Human review reserves one extra judge call (three attempts). Empty review and addition rounds skip their judge calls. Actual dollars depend on selected models and text length; the normal estimate and worst-case reservation are shown in preflight. Unused reservations are released at termination.

The automatic destination is `<data_dir>/exports/prd-<title-slug>-<contestId>.md`, independent of the working directory. Files are written atomically with mode `0600`. The committed report precedes the write, allowing a restart during saving to reproduce the same bytes and destination. A later revision failure still saves the last good PRD. A merge failure saves nothing. If disk writing fails, the stored PRD remains available:

```text
/export bass_ackwards_prd
/export bass_ackwards_prd <contestId>
```

Export includes the final PRD and the complete accepted record, including dropped review comments. A successful CLI run prints `PRD saved to <path>` and emits `export.ready` in JSON output.

## Example run

On 2026-11-26, one release `squad send --yes --json` used the default six-model roster, `SQUAD_DAILY_MAX_USD=150`, `/dev/null` as the environment file and a fresh private data directory. The end result was “a self-hosted status page for a small SaaS, live with alerts”.

- Recorded actual cost, including startup alias probes: **$2.913638**; unknown-cost calls: **0**.
- Wall time, including credential retrieval and startup: **429.081 seconds**.
- Preflight: **28** normal calls, **60** reserved attempts, **$9.531904** normal estimate and **$43.344783** maximum reservation. The remaining reservation was **$0** after termination.
- Actual execution: **21** logical mode calls, **24** mode attempts (one repair each for merge, one review and revision), plus **4** startup alias probes.
- Outcome: **FAILED / judge_failed**, CLI exit **1**. Revision output reached its token limit twice. D98 saved the last good version (version 1); all four application checks passed, with no uncovered DoD items.
- Verified save: **16,568 bytes**, mode **0600**, SHA-256 `99b38c263ab1f915ff467e431cd161e982cb3f2531527e18a73c4c614d60fc72`. The CLI emitted `export.ready` with the saved path.

Saved path:

```text
/tmp/squad-bass-smoke-20261126-hrgkt18a/data/exports/prd-self-hosted-status-page-for-a-small-saas-live-wi-01a21a0e-53d7-701d-9e38-1320a08fa318.md
```

Short excerpt of the saved report:

```markdown
# Self-hosted status page for a small SaaS, live with alerts

Checks passed: 4/4.

**End result:** a self-hosted status page for a small SaaS, live with alerts
**Starting point:** Not stated.

## Overview
Deliver a self-hosted public status page on operator-controlled infrastructure, isolated from the main SaaS application. The plan moves forward from confirming scope, access and tool choice, through an independent HTTPS deployment, to health probes that update component status within five minutes and send outage and recovery alerts to the on-call channel. It then adds authenticated incident management, public history and uptime, and visitor subscriptions with email notifications. The final milestone is a set of acceptance drills against every definition of done, including a full main-application shutdown. No current-state baseline was supplied, so the starting items rest on assumptions that need confirming.

## Definition of done
- D1 — The status page loads over HTTPS at its public domain without login, showing current status for every monitored SaaS component. (verified by: Open the public URL in a logged-out browser.; support: 6)
- D2 — When a monitored component is deliberately taken down, its displayed status changes to degraded or down within five minutes. (verified by: Controlled outage drill with stopwatch; support: 5)
- D3 — A detected outage sends an alert to the on-call channel within five minutes, naming the affected component. (verified by: Trigger a test outage and check the alert channel timestamp.; support: 5)
- D4 — When the failed service recovers, the page returns to operational and a recovery notification reaches the same alert channel. (verified by: Restore the test endpoint and check page and channel.; support: 4)
```

The run exposed unnecessary output overhead from UUID-length document keys. The implementation now allocates compact permanent `K1`, `K2`, … identities from a persisted counter and never reuses a removed identity; M/P/L display numbers remain positional. Assembly prompts request compact JSON and preserve unchanged text. Offline tests cover this correction. There was no second paid smoke.

## Shared integration

An owner-approved one-line fix in `crates/squad-core/src/runtime/judge.rs` routes new-mode judge work directly through its already-registered handler. This avoids the older standalone-judge adapter's work-kind allowlist. The fix is isolated in commit `ce2d07a` so it can be integrated independently for the other modes.

## Implementation evidence

The owned engine/handler harness uses real Store, Router, scoped readers, attempt accounting and a wiremock OpenRouter endpoint. It exercises 28 calls for six members, 29 with a human note, and 26 when both decision rounds are empty. It checks invalid member output with one diagnostic repair, insufficient quorum, last-good-version saving after revision exhaustion, quote dropping, request blindness and one-lens instructions, disk failure, human-note conflicts, stop, and restart checkpoints through the committed report before writing.

The production `RoomHandle` workflow passes as well: all 28 calls, automatic save, export, printed path material and restart without repeated calls. Across the seven mode-specific test targets, 45 executions pass with no failures or ignores (27 distinct test functions; shared contract fixtures run in several targets). The release build and workspace Clippy with warnings denied pass. The live smoke and its limitation are recorded above; final workspace evidence is recorded in the M9.P3 checklist.
