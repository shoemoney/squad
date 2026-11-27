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
5. The judge accepts or rejects each surviving comment and returns only keyed field edits. The application applies them to the stored PRD, preserving every untouched byte and permanent identity.
6. Members propose up to three must-have additions each. Final assembly is add-only: existing text and order remain intact.
7. Application code checks task mapping, acceptance-test coverage, missing dependencies and cycles. It computes parallel levels and the longest dependency chain, then saves the Markdown PRD.

Revisions, final additions and human-note application use sparse edits (D101), so the judge does not spend output tokens copying the whole PRD. The application checks the reconstructed document before committing a version. Truncated or invalid patches apply nothing and follow the same last-good-version recovery rule. The output ceiling stays at 8,192 tokens; reservations account for the growing document in later inputs.

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

On 2026-11-26, one owner-approved follow-up release `squad send --yes --json` used the default six-model roster, `SQUAD_DAILY_MAX_USD=150`, `/dev/null` as the environment file and a fresh private data directory. The end result was “a self-hosted status page for a small SaaS, live with alerts”. No additional model calls were made after this one smoke.

- Confirmed actual cost, including startup alias probes: **$1.992752**; unknown-cost calls after reconciliation: **0**.
- Wall time, including credential retrieval and startup: **495.946 seconds**.
- Preflight: **28** normal calls, **60** reserved attempts, **$11.357968** normal estimate and **$50.383359** maximum reservation. The remaining reservation after reconciliation was **$0**.
- Actual execution: **28** logical mode calls, **30** mode attempts (one retry each for Grok's tree and must-have calls), plus **4** startup alias probes: **34** provider attempts in total.
- Revision succeeded on its first attempt: **24 decisions and 20 keyed edits**, **10,659 bytes** of structured output. Final assembly also succeeded on its first attempt: **4 decisions and 4 edits**, **3,333 bytes**. Neither response re-emitted the PRD.
- Outcome: **COMPLETE / saved**, CLI exit **0**, saved final PRD **version 3**. All four application checks passed, with no uncovered DoD items. A separate comparison of the stored versions verified D96: unaddressed revision entities stayed identical, and final assembly preserved existing entities and order.
- Verified save: **22,140 bytes**, mode **0600**, SHA-256 `491bdc91a1d586c74cf68b52faa78ebecdc622d0738770b8affe65ecfa4dccdf`. The CLI emitted `export.ready` with the saved path.

Two failed Grok attempts had delayed cost receipts. At CLI shutdown the ledger recorded $1.951478 and conservatively held $1.687617 for two unresolved calls. Two read-only OpenRouter generation lookups confirmed charges of $0.016462 and $0.024812. The existing accounting writer applied those receipts, bringing the settled total to $1.992752, unknown calls to zero and the held reservation to zero. This required no further model execution.

Saved path:

```text
/tmp/squad-bass-fix-smoke-20261126-9rfnfd_s/data/exports/prd-self-hosted-status-page-with-alerts-for-a-small--01a21a4e-f139-7657-be6d-1e3f62558411.md
```

First 40 lines of the saved PRD:

```markdown
# Self-hosted status page with alerts for a small SaaS

Checks passed: 4/4.

**End result:** a self-hosted status page for a small SaaS, live with alerts
**Starting point:** Not stated.

## Overview
Deliver a self-hosted public status page on operator-controlled infrastructure independent of the primary SaaS app, with automated component probes, outage and recovery alerts to the on-call channel within five minutes, operator incident and maintenance publishing with public history, and automatic recovery after host restart. Work proceeds from decisions and inventories, through hosting and deployment, monitoring and alerting, incident publishing, and ends with a production acceptance drill against the frozen DoD. Today's state was not supplied; a greenfield start is assumed.

## Definition of done
- D1 — The status page loads publicly over HTTPS at its production domain from a self-hosted server, showing current status for each listed service component. (verified by: Visiting the URL from an external network browser; support: 6)
- D2 — When a monitored service becomes unreachable, the page shows that component as degraded or down within five minutes without manual intervention. (verified by: Stopping a test service and observing page update; support: 6)
- D3 — A detected outage sends an alert to the configured on-call channel within five minutes, naming the affected component. (verified by: Simulate an outage and check the alert channel for the message; support: 5)
- D4 — The status page remains reachable when the primary SaaS application is down. (verified by: Taking main app offline and loading status page; support: 4)
- D5 — A recovery alert is sent and the component returns to operational status after the failed service is restored. (verified by: Restarting test service and checking alert and page; support: 4)
- D6 — The status page runs on operator-controlled infrastructure, not a third-party status vendor. (verified by: Hosting ownership review; support: 2)
- D7 — Alerts are delivered to the designated channel when a service outage is detected and when it recovers. (verified by: Alert log during downtime drill; support: 2)
- D8 — An authorized operator can post an incident update that appears publicly on the status page with a timestamp. (verified by: Posting a test incident and viewing public page; support: 2)
- D9 — Visitors can view current incidents, maintenance notices, and recent incident history on the page. (verified by: Published test incident appears publicly; support: 2)
- D10 — Status page remains accessible and accurate after a host restart, with monitoring and alerts resuming automatically. (verified by: Restart host and recheck page; support: 2)

## Dependency order
**Critical path:** M1.P1.L1 → M2.P1.L1 → M2.P1.L2 → M2.P2.L1 → M3.P1.L1 → M3.P1.L2 → M3.P2.L1 → M3.P2.L2 → M5.P1.L1 → M5.P1.L2 → M5.P1.L5 → M5.P1.L3
**Can run in parallel:** level 1: M1.P1.L1 → M1.P1.L2 → M1.P2.L1 → M1.P2.L2 → M1.P2.L3 · level 2: M2.P1.L1 · level 3: M2.P1.L2 → M2.P1.L3 → M2.P1.L4 · level 4: M2.P2.L1 · level 5: M2.P2.L2 → M3.P1.L1 → M4.P1.L1 · level 6: M3.P1.L2 → M3.P1.L3 → M3.P1.L4 → M4.P1.L2 · level 7: M3.P2.L1 → M4.P1.L3 · level 8: M3.P2.L2 → M3.P2.L3 → M3.P2.L4 → M3.P2.L5 → M5.P1.L4 · level 9: M5.P1.L1 · level 10: M5.P1.L2 · level 11: M5.P1.L5 · level 12: M5.P1.L3

## Milestones

### M1 — Decisions and inputs
**Goal:** Hosting, software, component inventory, on-call channel and publishing rules are agreed so build work can start.
**Serves:** D1, D10, D2, D3, D4, D5, D6, D7, D8, D9

#### Phase M1.P1 — Hosting and software decisions
- [ ] M1.P1.L1 — Approve hosting budget, choose either a different provider from the main app or the same provider in a different region and account, recording which, and confirm production domain ownership. (owner: Product owner; needs: The frozen DoD is accepted by stakeholders and a decision-maker is named.; after: ; done: D4, D6; risk: Budget approval stalls.)
- [ ] M1.P1.L2 — Record the selection of open-source self-hostable status software that natively provides probes, alerts, incidents and maintenance, listing each approved plugin or integration supplying any missing capability, with a version-controlled deployment manifest including persistent volume configuration. (owner: Technical lead; needs: Requirements for monitoring, alerts and incidents are listed.; after: ; done: D2, D6, D8, D9, D10; risk: The chosen tool may miss a DoD capability, forcing rework or a second tool.)

#### Phase M1.P2 — Inventories and rules
- [ ] M1.P2.L1 — Document the component inventory with, per component, a health-check endpoint, method, interval, how the endpoint is reached from the independent status host (public URL, allowlisted IP or probe credentials), and exact degraded, down and recovery rules in consecutive failures, duration, HTTP status and/or latency, plus an approved stoppable test service for drills. (owner: Service owner; needs: Service owners are available to identify the customer-facing components.; after: ; done: D1, D2, D5; risk: Shallow health checks may report healthy while the service is functionally broken.)
- [ ] M1.P2.L2 — Designate the named on-call channel and its type, store its webhook URL or credentials in a recorded secret store with named access owners, and log a confirmed test message. (owner: Operations lead; needs: The team agrees on which channel is the on-call channel.; after: ; done: D3, D7; risk: Credentials may leak, or nobody may watch the channel.)
- [ ] M1.P2.L3 — Approve operator access rules and incident fields for timestamped updates, maintenance notices and recent history. (owner: Incident lead; needs: An incident owner can confirm who may publish and what visitors should see.; after: ; done: D8, D9; risk: Unclear publishing authority can delay public updates.)
```

The previous full-document revision format exhausted its output ceiling and saved only version 1. D101 replaces that format with sparse keyed edits for revision, final assembly and human-note application. Permanent compact `K1`, `K2`, … identities still come from a persisted counter and are never reused; M/P/L display numbers remain positional. The judge's visible ceiling remains 8,192 tokens, and input reservations include the accumulating document.

## Shared integration

An owner-approved one-line fix in `crates/squad-core/src/runtime/judge.rs` routes new-mode judge work directly through its already-registered handler. This avoids the older standalone-judge adapter's work-kind allowlist. The fix is isolated in commit `ce2d07a` so it can be integrated independently for the other modes.

The full workspace gate has 1,373 passes, seven failures and four intentional ignores. The seven failures are stale enabled-mode expectations in five shared test files, covered by commit `e7fc4b9` on `origin/mode-bass-ackwards-prd-shared-tests`. That commit was inspected and left unmerged because all five files are integrator-owned. No further shared production changes are required for D101.

## Implementation evidence

The owned engine/handler harness uses real Store, Router, scoped readers, attempt accounting and a wiremock OpenRouter endpoint. It exercises 28 calls for six members, 29 with a human note, and 26 when both decision rounds are empty. It checks invalid member output with one diagnostic repair, insufficient quorum, last-good-version saving after revision exhaustion, quote dropping, request blindness and one-lens instructions, disk failure, human-note conflicts, stop, and restart checkpoints through the committed report before writing.

The production `RoomHandle` workflow passes as well: all 28 calls, automatic save, export, printed path material and restart without repeated calls. Across the seven mode-specific test targets, 54 executions pass with no failures or ignores (33 distinct test functions; shared contract fixtures run in several targets). The release build and workspace Clippy with warnings denied pass. D101 regression tests cover a 60-task PRD exceeding the former full-response budget, sparse output under 512 bytes, accepted edits reaching the saved file, invalid targets and identity changes, add-only source and placement checks, preservation of redacted fields, provider-compatible schemas and atomic last-good recovery from truncated patches. Final workspace evidence is recorded in the M9.P3 checklist.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `c7c20ff` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `COMPLETE`, `saved`; 28 planned mode calls and
33 total provider attempts including startup. Spend was $2.552690, observed
wall time 410.874 seconds, and initial/sample-peak reservation
$50.711781/$50.711781. Final reserved funds were $0.000000 and unknown
calls 0. Credential-free offline export passed. Evidence: `/tmp/squad-v020-bass_ackwards_prd-rklsr3wq/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).

Bass Ackwards PRD dispatched twenty-nine mode attempts against the twenty-eight
normal-call plan. The finalizer's first answer exceeded the 2,400-word prose
budget; its one private validation repair passed. All other work completed on
the first attempt. The 25,780-byte PRD was saved under the run's `data/exports/`,
and the emitted `export.ready` path exists. Offline export passed. Final holds
and unknown calls were zero.
